//! `tapboxd sim`: an interactive console on real time. Type the same commands
//! as in scenario files and watch what the box would do.

use std::io::Write as _;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use anyhow::Result;
use tapbox_core::{Button, ButtonState, Command, Event, Millis, Playback, Uid};
use tokio::io::{AsyncBufReadExt as _, BufReader};
use tokio::sync::{mpsc, oneshot, watch};

use super::Bench;
use super::runner::{CLICK, DEFAULT_TRACK_LENGTH, TICK, apply_instant};
use super::script::{self, Step};
use crate::app::App;
use crate::outputs::describe_led;
use crate::player::{Player, SimPlayer};
use crate::store::Store;
use crate::web::{self, Action, Request, SimAction, build_snapshot, snapshot::SimCardView};

/// How `tapboxd sim` runs.
#[derive(Debug, Clone, Copy)]
pub struct Options {
    /// Serve the web app here.
    pub web: Option<SocketAddr>,
    /// Start in Wi-Fi setup mode, as a box with no known network would.
    pub wifi_setup: bool,
}

enum Input {
    Step(Step),
    /// A timed step finished; just show the prompt again.
    Done,
    Status,
    Cards,
    Problems,
    Help,
    Quit,
}

const HOLD_SLACK: std::time::Duration = std::time::Duration::from_millis(50);

const HELP: &str = "\
cards:    tap <card>   lift
buttons:  click|press|release <play|vol-up|vol-down>   hold <button> <3s>
sources:  bluetooth start|stop   airplay start|stop
other:    wifi done   power off|cut   player end|fail   track-length <time>   wait <time>
checks:   expect ... (as in scenario files)   status   problems   cards   help   quit";

/// Run the console until `quit`, Ctrl-D or Ctrl-C.
///
/// # Errors
/// If the app can't be built.
#[allow(clippy::too_many_lines, reason = "one flat event loop reads best in one place")]
pub async fn run(bench: &Bench, player: Player, store: Option<Store>, options: Options) -> Result<()> {
    let (mut app, mut msgs) = bench.build_app_with(player, store.clone(), true, options.wifi_setup)?;
    println!("tapbox testbench. Simulated hardware; type `help`.\n{HELP}\n");
    print_cards(bench);

    let (tx, mut inputs) = mpsc::channel::<Input>(64);
    tokio::spawn(read_stdin(tx, options.web.is_none()));

    let sim_cards: Vec<SimCardView> = bench
        .cards
        .values()
        .map(|card| SimCardView { name: card.name.clone(), description: card.description.clone().unwrap_or_default() })
        .collect();
    let (web_tx, mut web_rx) = mpsc::channel::<Request>(64);
    let (snapshot_tx, snapshot_rx) = watch::channel(build_snapshot(&mut app, true, sim_cards.clone()));
    if let Some(addr) = options.web {
        let requests = web_tx.clone();
        let library = app.library_dir().to_path_buf();
        tokio::spawn(async move {
            if let Err(err) = web::serve(addr, requests, snapshot_rx, library).await {
                eprintln!("  ! web app: {err:#}");
            }
        });
        println!("web app: http://{addr}/\n");
    }

    let start = Instant::now();
    let now = || Millis(u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX));
    let watchdog = sd_notify::watchdog_enabled();
    let _ = sd_notify::notify(&[sd_notify::NotifyState::Ready]);

    let mut ticker = tokio::time::interval(TICK);
    let mut last: Vec<Command> = Vec::new();
    app.tick(now()).await; // boot: volume and LED
    if options.wifi_setup {
        app.handle(now(), Event::Control(tapbox_card::ControlCommand::WifiSetup)).await;
    }
    prompt();
    loop {
        tokio::select! {
            _ = ticker.tick() => {
                app.tick(now()).await;
                last.extend(app.take_log());
                if watchdog.is_some() {
                    let _ = sd_notify::notify(&[sd_notify::NotifyState::Watchdog]);
                }
            }
            Some(msg) = msgs.recv() => {
                app.on_msg(now(), msg).await;
                last.extend(app.take_log());
            }
            Some(request) = web_rx.recv() => {
                let result = match request.action {
                    Action::App(action) => app.web(now(), action).await,
                    Action::Sim(action) => sim_action(&mut app, bench, now(), action, &web_tx).await,
                };
                let _ = request.reply.send(result);
                last.extend(app.take_log());
            }
            input = inputs.recv() => {
                match input {
                    None | Some(Input::Quit) => break,
                    Some(Input::Done) => {}
                    Some(Input::Help) => println!("{HELP}"),
                    Some(Input::Cards) => print_cards(bench),
                    Some(Input::Problems) => print_problems(&app, bench),
                    Some(Input::Status) => print_status(&app, bench),
                    Some(Input::Step(Step::Expect { expectation, negated })) => {
                        match script::check(&app, bench, &last, &expectation, negated) {
                            Ok(()) => println!("  ✓"),
                            Err(err) => println!("  ✗ {err}"),
                        }
                    }
                    Some(Input::Step(Step::PowerCut)) => {
                        if matches!(app.player(), Player::Mpv(_)) {
                            println!("  ! `power cut` needs the simulated player");
                        } else {
                            println!("  ⚡ power cut: everything not saved is gone; booting again");
                            (app, msgs) = bench.build_app(
                                Player::Sim(SimPlayer::new(DEFAULT_TRACK_LENGTH)),
                                store.clone(),
                                true,
                            )?;
                            app.tick(now()).await;
                            last.clear();
                        }
                    }
                    Some(Input::Step(step)) => {
                        if step.is_action() {
                            last.clear();
                        }
                        if let Err(err) = apply_instant(&mut app, bench, now(), &step).await {
                            println!("  ! {err:#}");
                        }
                        last.extend(app.take_log());
                    }
                }
                prompt();
            }
            _ = tokio::signal::ctrl_c() => break,
        }
        snapshot_tx.send_if_modified(|current| {
            let fresh = build_snapshot(&mut app, true, sim_cards.clone());
            let changed = *current != fresh;
            if changed {
                *current = fresh;
            }
            changed
        });
    }
    println!();
    Ok(())
}

/// Reads stdin and expands timed steps (`click`, `hold`, `wait`) into
/// primitives sent at the right moments.
async fn read_stdin(tx: mpsc::Sender<Input>, quit_on_eof: bool) {
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let input = match line.trim() {
            "help" | "?" => Input::Help,
            "cards" => Input::Cards,
            "problems" => Input::Problems,
            "status" | "s" => Input::Status,
            "quit" | "exit" | "q" => Input::Quit,
            _ => match script::parse_line(&line) {
                Ok(Some(step)) => {
                    if send_timed(&tx, step).await.is_err() {
                        return;
                    }
                    continue;
                }
                Ok(None) => Input::Help,
                Err(err) => {
                    println!("  ! {err:#}");
                    prompt();
                    continue;
                }
            },
        };
        if tx.send(input).await.is_err() {
            return;
        }
    }
    // Without a terminal (e.g. run in the background for the web app), keep going.
    if quit_on_eof {
        let _ = tx.send(Input::Quit).await;
    }
}

async fn send_timed(tx: &mpsc::Sender<Input>, step: Step) -> Result<(), mpsc::error::SendError<Input>> {
    let (button, held) = match step {
        Step::Click(button) => (button, CLICK),
        Step::Hold(button, held) => (button, held),
        Step::Wait(duration) => {
            tokio::time::sleep(duration).await;
            return tx.send(Input::Done).await;
        }
        other => return tx.send(Input::Step(other)).await,
    };
    tx.send(Input::Step(Step::Press(button))).await?;
    // Channel and scheduling latency would otherwise land a typed `hold play 3s`
    // a few milliseconds short of the 3 s threshold.
    tokio::time::sleep(held + HOLD_SLACK).await;
    tx.send(Input::Step(Step::Release(button))).await
}

fn prompt() {
    print!("tapbox> ");
    let _ = std::io::stdout().flush();
}

fn print_cards(bench: &Bench) {
    println!("cards in {}:", bench.dir.join("cards").display());
    for card in bench.cards.values() {
        println!("  {:<18} uid {}  {}", card.name, card.uid, card.description.as_deref().unwrap_or(""));
    }
}

fn print_problems(app: &App, bench: &Bench) {
    let mut any = false;
    for record in app.problems().rev() {
        any = true;
        let card = card_name(bench, &record.problem.uid);
        let ago = record.at.elapsed().map_or(0, |elapsed| elapsed.as_secs());
        println!("  {ago:>4}s ago  {card:<16} {}", record.problem.kind.describe());
    }
    if !any {
        println!("  no problems");
    }
}

fn card_name(bench: &Bench, uid: &tapbox_core::Uid) -> String {
    bench.cards.values().find(|card| card.uid == *uid).map_or_else(|| uid.to_string(), |card| card.name.clone())
}

fn print_status(app: &App, bench: &Bench) {
    let core = app.core();
    let name = |uid: &tapbox_core::Uid| {
        bench.cards.values().find(|card| card.uid == *uid).map_or_else(|| uid.to_string(), |card| card.name.clone())
    };
    let playback = match core.playback() {
        Playback::Idle => "idle".to_owned(),
        Playback::Playing(uid) => format!("playing {}", name(&uid)),
        Playback::Paused(uid) => format!("paused {}", name(&uid)),
    };
    println!("  playback  {playback}");
    println!("  volume    {} (max {})", core.volume(), core.max_volume());
    println!("  LED       {}", describe_led(app.outputs().led()));
    match app.player().loaded() {
        Some(playable) => println!("  loaded    {playable}"),
        None => println!("  loaded    nothing"),
    }
    if app.outputs().airplay_muted() {
        println!("  AirPlay   muted");
    }
}

/// Carry out a simulator-panel action from the web page.
async fn sim_action(
    app: &mut App,
    bench: &Bench,
    now: Millis,
    action: SimAction,
    requests: &mpsc::Sender<Request>,
) -> Result<(), String> {
    let press_then_release = |button: Button, after: Duration| {
        let requests = requests.clone();
        tokio::spawn(async move {
            tokio::time::sleep(after).await;
            let (reply, _) = oneshot::channel();
            let _ = requests.send(Request { action: Action::Sim(SimAction::Release(button)), reply }).await;
        });
    };
    match action {
        SimAction::Tap(name) => match bench.card(&name) {
            Ok(card) => app.tap(now, card.uid.clone(), &card.bytes).await,
            // A blank card made with "tap a new blank card" is known only by its UID.
            Err(err) => {
                let uid = Uid::from_hex(&name).ok_or_else(|| err.to_string())?;
                app.tap(now, uid, BLANK_TAG).await;
            }
        },
        SimAction::TapNewBlank => {
            if let Some(uid) = new_blank_uid() {
                app.tap(now, uid, BLANK_TAG).await;
            }
        }
        SimAction::Lift => app.handle(now, Event::CardRemoved).await,
        SimAction::Click(button) => {
            app.handle(now, Event::Button { button, state: ButtonState::Pressed }).await;
            press_then_release(button, CLICK);
        }
        SimAction::Hold(button, duration) => {
            app.handle(now, Event::Button { button, state: ButtonState::Pressed }).await;
            press_then_release(button, duration + HOLD_SLACK);
        }
        SimAction::Release(button) => {
            app.handle(now, Event::Button { button, state: ButtonState::Released }).await;
        }
        SimAction::External(source, true) => app.handle(now, Event::ExternalStarted(source)).await,
        SimAction::External(source, false) => app.handle(now, Event::ExternalStopped(source)).await,
    }
    Ok(())
}

/// An empty NDEF message: what a new sticker from the shop holds.
const BLANK_TAG: &[u8] = &[0x03, 0x00, 0xFE];

/// A fresh 7-byte NTAG-style UID (starts with the NXP manufacturer byte).
/// Always `Some`: seven bytes is a valid UID length.
fn new_blank_uid() -> Option<Uid> {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos());
    let count = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut bytes = vec![0x04];
    bytes.extend_from_slice(&(nanos ^ count.rotate_left(16)).to_be_bytes());
    bytes.extend_from_slice(&count.to_be_bytes()[2..]);
    Uid::new(bytes)
}

//! `tapboxd sim`: an interactive console on real time. Type the same commands
//! as in scenario files and watch what the box would do.

use std::io::Write as _;
use std::time::Instant;

use anyhow::Result;
use tapbox_core::{Command, Millis, Playback};
use tokio::io::{AsyncBufReadExt as _, BufReader};
use tokio::sync::mpsc;

use super::Bench;
use super::runner::{CLICK, DEFAULT_TRACK_LENGTH, TICK, apply_instant};
use super::script::{self, Step};
use crate::app::App;
use crate::outputs::describe_led;
use crate::player::{Player, SimPlayer};
use crate::store::Store;

enum Input {
    Step(Step),
    /// A timed step finished; just show the prompt again.
    Done,
    Status,
    Cards,
    Help,
    Quit,
}

const HOLD_SLACK: std::time::Duration = std::time::Duration::from_millis(50);

const HELP: &str = "\
cards:    tap <card>   lift
buttons:  click|press|release <play|vol-up|vol-down>   hold <button> <3s>
sources:  bluetooth start|stop   airplay start|stop
other:    battery low|ok   power off|cut   player end|fail   track-length <time>   wait <time>
checks:   expect ... (as in scenario files)   status   cards   help   quit";

/// Run the console until `quit`, Ctrl-D or Ctrl-C.
///
/// # Errors
/// If the app can't be built.
pub async fn run(bench: &Bench, player: Player, store: Option<Store>) -> Result<()> {
    let (mut app, mut msgs) = bench.build_app(player, store.clone(), true)?;
    println!("tapbox testbench. Simulated hardware; type `help`.\n{HELP}\n");
    print_cards(bench);

    let (tx, mut inputs) = mpsc::channel::<Input>(64);
    tokio::spawn(read_stdin(tx));

    let start = Instant::now();
    let now = || Millis(u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX));
    let watchdog = sd_notify::watchdog_enabled();
    let _ = sd_notify::notify(&[sd_notify::NotifyState::Ready]);

    let mut ticker = tokio::time::interval(TICK);
    let mut last: Vec<Command> = Vec::new();
    app.tick(now()).await; // boot: volume and LED
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
            input = inputs.recv() => {
                match input {
                    None | Some(Input::Quit) => break,
                    Some(Input::Done) => {}
                    Some(Input::Help) => println!("{HELP}"),
                    Some(Input::Cards) => print_cards(bench),
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
    }
    println!();
    Ok(())
}

/// Reads stdin and expands timed steps (`click`, `hold`, `wait`) into
/// primitives sent at the right moments.
async fn read_stdin(tx: mpsc::Sender<Input>) {
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let input = match line.trim() {
            "help" | "?" => Input::Help,
            "cards" => Input::Cards,
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
    let _ = tx.send(Input::Quit).await;
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

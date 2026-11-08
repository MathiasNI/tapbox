//! The imperative shell around [`Core`]: feeds it events, carries out its
//! commands. The same `App` runs on real time (daemon, interactive
//! testbench) and on virtual time (scripted scenarios); whoever drives it
//! passes the clock in.

use std::collections::VecDeque;
use std::time::{Duration, SystemTime};

use tapbox_core::{Command, Core, Event, Failure, Millis, PlayerEvent, Problem, Uid};
use tokio::sync::mpsc;

use crate::outputs::{SimOutputs, describe_flash, describe_led};
use crate::player::Player;
use crate::resolver::{self, Playable, Resolution, Resolver};
use crate::store::Store;

/// Results of background work, fed back into the app's loop.
#[derive(Debug)]
pub enum AppMsg {
    /// A verified download finished (or failed).
    Downloaded {
        /// Card it was for.
        uid: Uid,
        /// Where playback should start.
        start: Duration,
        /// The cached file, or why it failed.
        result: Result<(tapbox_card::ContentHash, std::path::PathBuf), String>,
    },
}

/// Problems kept for the web app; oldest dropped first.
pub const PROBLEM_LOG_LEN: usize = 50;

/// A logged problem.
#[derive(Debug, Clone)]
pub struct ProblemRecord {
    /// Wall-clock time, for display.
    pub at: SystemTime,
    /// What happened, and with which card.
    pub problem: Problem,
}

/// Daemon state: core plus drivers.
#[derive(Debug)]
pub struct App {
    core: Core,
    player: Player,
    resolver: Resolver,
    outputs: SimOutputs,
    store: Option<Store>,
    http: reqwest::Client,
    max_download_bytes: u64,
    msgs: mpsc::UnboundedSender<AppMsg>,
    /// Events produced while executing commands (e.g. a failed resolve).
    pending: VecDeque<Event>,
    /// Commands executed since [`App::take_log`] was last called.
    log: Vec<Command>,
    /// Recent problems, newest last.
    problems: VecDeque<ProblemRecord>,
    echo: bool,
}

/// Everything `App::new` needs.
#[derive(Debug)]
pub struct Parts {
    /// The state machine, already restored from saved state.
    pub core: Core,
    /// Player backend.
    pub player: Player,
    /// Library index.
    pub resolver: Resolver,
    /// Where to save state; `None` keeps it in memory (scenarios).
    pub store: Option<Store>,
    /// Download size limit.
    pub max_download_bytes: u64,
    /// Print every command to stdout (interactive testbench).
    pub echo: bool,
}

impl App {
    /// Build the app. Returns the receiver for background results, which the
    /// driving loop must feed to [`App::on_msg`].
    ///
    /// # Errors
    /// If the HTTP client can't be built.
    pub fn new(parts: Parts) -> anyhow::Result<(Self, mpsc::UnboundedReceiver<AppMsg>)> {
        let (msgs, rx) = mpsc::unbounded_channel();
        // reqwest is built without a bundled crypto provider (keeps the build
        // pure Rust); install ring's. Errors only mean one is already installed.
        let _ = rustls::crypto::ring::default_provider().install_default();
        let http = reqwest::Client::builder().connect_timeout(Duration::from_secs(10)).build()?;
        let app = Self {
            core: parts.core,
            player: parts.player,
            resolver: parts.resolver,
            outputs: SimOutputs::default(),
            store: parts.store,
            http,
            max_download_bytes: parts.max_download_bytes,
            msgs,
            pending: VecDeque::new(),
            log: Vec::new(),
            problems: VecDeque::new(),
            echo: parts.echo,
        };
        Ok((app, rx))
    }

    /// The state machine, read-only.
    #[must_use]
    pub fn core(&self) -> &Core {
        &self.core
    }

    /// The player, read-only.
    #[must_use]
    pub fn player(&self) -> &Player {
        &self.player
    }

    /// The player, for testbench overrides (force end / failure).
    pub fn player_mut(&mut self) -> &mut Player {
        &mut self.player
    }

    /// Simulated outputs, read-only.
    #[must_use]
    pub fn outputs(&self) -> &SimOutputs {
        &self.outputs
    }

    /// Recent problems, newest last (at most [`PROBLEM_LOG_LEN`]).
    pub fn problems(&self) -> impl DoubleEndedIterator<Item = &ProblemRecord> {
        self.problems.iter()
    }

    /// Commands executed since the last call.
    pub fn take_log(&mut self) -> Vec<Command> {
        std::mem::take(&mut self.log)
    }

    /// Feed an event (and anything it causes) through the core.
    pub async fn handle(&mut self, now: Millis, event: Event) {
        self.pending.push_back(event);
        while let Some(event) = self.pending.pop_front() {
            for command in self.core.handle(now, event) {
                self.execute(now, command).await;
            }
        }
    }

    /// Advance time: collect player feedback, then tick the core.
    pub async fn tick(&mut self, now: Millis) {
        for event in self.player.poll(now) {
            self.handle(now, Event::Player(event)).await;
        }
        self.handle(now, Event::Tick).await;
    }

    /// Handle a background result.
    pub async fn on_msg(&mut self, now: Millis, msg: AppMsg) {
        match msg {
            AppMsg::Downloaded { uid, start, result } => {
                let still_wanted =
                    matches!(self.core.playback(), tapbox_core::Playback::Playing(current) if current == uid);
                match result {
                    Ok((hash, path)) => {
                        self.resolver.add(hash, path.clone());
                        if still_wanted {
                            self.load(now, &Playable::File(path), start).await;
                        }
                    }
                    Err(err) => {
                        tracing::warn!(%err, "download failed");
                        if still_wanted {
                            self.handle(now, Event::Player(PlayerEvent::Failed(Failure::Download(err)))).await;
                        }
                    }
                }
            }
        }
    }

    async fn execute(&mut self, now: Millis, command: Command) {
        if self.echo {
            println!("  → {}", describe(&command));
        }
        tracing::debug!(?command, "execute");
        self.log.push(command.clone());
        let result = match command {
            Command::Play { uid, sources, start } => {
                match self.resolver.resolve(&sources) {
                    Resolution::Ready(playable) => self.load(now, &playable, start).await,
                    Resolution::Download { url, hash } => self.spawn_download(uid, url, hash, start),
                    Resolution::Unresolvable => {
                        let detail = sources.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ");
                        tracing::warn!(%uid, sources = %detail, "no playable source on this card");
                        self.pending.push_back(Event::Player(PlayerEvent::Failed(Failure::NothingPlayable(detail))));
                    }
                }
                Ok(())
            }
            Command::Pause => self.player.set_paused(now, true).await,
            Command::Resume => self.player.set_paused(now, false).await,
            Command::Stop => self.player.stop().await,
            Command::SetVolume(volume) => self.player.set_volume(volume).await,
            Command::Led(state) => {
                self.outputs.set_led(state);
                Ok(())
            }
            Command::Problem(problem) => {
                tracing::warn!(uid = %problem.uid, problem = problem.kind.name(), "{}", problem.kind.describe());
                if self.problems.len() == PROBLEM_LOG_LEN {
                    self.problems.pop_front();
                }
                self.problems.push_back(ProblemRecord { at: SystemTime::now(), problem });
                Ok(())
            }
            Command::Flash(flash) => {
                self.outputs.flash(flash);
                Ok(())
            }
            Command::PauseBluetooth => {
                self.outputs.pause_bluetooth();
                Ok(())
            }
            Command::MuteAirPlay(muted) => {
                self.outputs.mute_airplay(muted);
                Ok(())
            }
            Command::OpenBluetoothPairing(window) => {
                self.outputs.open_bluetooth_pairing(window);
                Ok(())
            }
            Command::EnterWifiSetup => {
                self.outputs.enter_wifi_setup();
                Ok(())
            }
            Command::SayName => {
                self.outputs.say_name();
                Ok(())
            }
            Command::Persist => self.persist(),
        };
        if let Err(err) = result {
            tracing::error!(%err, "command failed");
        }
    }

    async fn load(&mut self, now: Millis, playable: &Playable, start: Duration) {
        if self.echo {
            println!("  → playing {playable} from {}s", start.as_secs());
        }
        if let Err(err) = self.player.load(now, playable, start).await {
            tracing::error!(%err, "player failed to load");
            self.pending.push_back(Event::Player(PlayerEvent::Failed(Failure::Playback(format!("{err:#}")))));
        }
    }

    fn spawn_download(&self, uid: Uid, url: String, hash: tapbox_card::ContentHash, start: Duration) {
        if self.echo {
            println!("  → downloading {url} (will verify {hash})");
        }
        let client = self.http.clone();
        let cache = self.resolver.cache_dir().to_path_buf();
        let max = self.max_download_bytes;
        let msgs = self.msgs.clone();
        tokio::spawn(async move {
            let result = resolver::download(&client, &url, hash, &cache, max)
                .await
                .map(|path| (hash, path))
                .map_err(|err| format!("{err:#}"));
            let _ = msgs.send(AppMsg::Downloaded { uid, start, result });
        });
    }

    fn persist(&self) -> anyhow::Result<()> {
        match &self.store {
            Some(store) => store.save(self.core.persistent_state()),
            None => Ok(()),
        }
    }
}

/// One-line description of a command, for the interactive testbench.
#[must_use]
pub fn describe(command: &Command) -> String {
    match command {
        Command::Play { uid, start, .. } => format!("play card {uid} from {}s", start.as_secs()),
        Command::Pause => "pause".into(),
        Command::Resume => "resume".into(),
        Command::Stop => "stop".into(),
        Command::SetVolume(volume) => format!("volume {volume}"),
        Command::PauseBluetooth => "pause the Bluetooth sender (AVRCP)".into(),
        Command::MuteAirPlay(true) => "mute AirPlay".into(),
        Command::MuteAirPlay(false) => "unmute AirPlay".into(),
        Command::OpenBluetoothPairing(window) => format!("Bluetooth pairing open for {}s", window.as_secs()),
        Command::EnterWifiSetup => "Wi-Fi setup hotspot".into(),
        Command::SayName => "say the box name and address".into(),
        Command::Led(state) => format!("LED {}", describe_led(*state)),
        Command::Flash(flash) => format!("LED {}", describe_flash(*flash)),
        Command::Problem(problem) => format!("problem logged for card {}: {}", problem.uid, problem.kind.describe()),
        Command::Persist => "save state".into(),
    }
}

/// Stable kebab-case name of a command, used by `expect cmd` in scenarios.
#[must_use]
pub fn command_name(command: &Command) -> &'static str {
    match command {
        Command::Play { .. } => "play",
        Command::Pause => "pause",
        Command::Resume => "resume",
        Command::Stop => "stop",
        Command::SetVolume(_) => "set-volume",
        Command::PauseBluetooth => "pause-bluetooth",
        Command::MuteAirPlay(true) => "mute-airplay",
        Command::MuteAirPlay(false) => "unmute-airplay",
        Command::OpenBluetoothPairing(_) => "open-bluetooth-pairing",
        Command::EnterWifiSetup => "enter-wifi-setup",
        Command::SayName => "say-name",
        Command::Led(_) => "led",
        Command::Flash(_) => "flash",
        Command::Problem(_) => "problem",
        Command::Persist => "persist",
    }
}

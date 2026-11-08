//! The imperative shell around [`Core`]: feeds it events, carries out its
//! commands. The same `App` runs on real time (daemon, interactive
//! testbench) and on virtual time (scripted scenarios); whoever drives it
//! passes the clock in.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use tapbox_card::{CardKind, ContentHash, Percent, Source, Sources};
use tapbox_core::{CardRead, Command, Core, Event, Failure, Millis, PlayerEvent, Problem, Uid};
use tokio::sync::mpsc;

use crate::boxdata::{BoxData, BoxDataFile, Link};
use crate::outputs::{SimOutputs, SimWifi, describe_flash, describe_led};
use crate::player::Player;
use crate::reader::read_tag;
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
    /// A Wi-Fi join attempt has had time to finish.
    WifiAttempt {
        /// Network.
        ssid: String,
        /// Password typed.
        password: String,
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
    box_file: BoxDataFile,
    box_data: BoxData,
    library_dir: PathBuf,
    /// The web app is waiting for a card to link: the next tap is captured, not played.
    linking: bool,
    last_card: Option<LastCard>,
    /// Titles seen on cards (written on them or from links), for display.
    titles: HashMap<Uid, String>,
    wifi: SimWifi,
    file_sizes: HashMap<ContentHash, u64>,
}

/// The most recently tapped card, for the web app's "link a card" flow.
#[derive(Debug, Clone)]
pub struct LastCard {
    /// Its UID.
    pub uid: Uid,
    /// When it was tapped.
    pub at: SystemTime,
    /// What is written on it, in short.
    pub content: String,
    /// Whether it was captured by link mode.
    pub captured: bool,
}

/// Changes requested by the web app.
#[derive(Debug, Clone)]
pub enum WebAction {
    /// Capture the next tapped card instead of playing it.
    StartLinking,
    /// Stop waiting for a card.
    CancelLinking,
    /// Link a card to a library file.
    Link {
        /// The card.
        uid: Uid,
        /// Shown in the app.
        title: String,
        /// Library file to play.
        hash: ContentHash,
    },
    /// Remove a card's link.
    Unlink {
        /// The card.
        uid: Uid,
    },
    /// An upload finished; the file is in the library directory.
    FileAdded {
        /// Its hash.
        hash: ContentHash,
        /// Where it was written.
        path: PathBuf,
    },
    /// Delete a library file.
    DeleteFile {
        /// Its hash.
        hash: ContentHash,
    },
    /// Rename the box.
    SetName(String),
    /// Volume now.
    SetVolume(u8),
    /// Hearing-protection limit.
    SetMaxVolume(u8),
    /// What lifting a card does.
    SetOnCardRemoved(tapbox_core::OnCardRemoved),
    /// Open Bluetooth pairing.
    PairBluetooth,
    /// Play/pause toggle.
    TogglePlay,
    /// Join a Wi-Fi network (setup page or settings).
    WifiConnect {
        /// Network name.
        ssid: String,
        /// Its password ("" for open networks).
        password: String,
    },
    /// Go back to Wi-Fi setup (hotspot) mode.
    WifiSetup,
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
    /// Where card links and the box name are kept.
    pub box_file: BoxDataFile,
    /// The library directory (uploads go here).
    pub library_dir: PathBuf,
    /// Start in Wi-Fi setup mode (no known network).
    pub wifi_setup: bool,
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
        let box_data = parts.box_file.load();
        let mut core = parts.core;
        let mut titles = HashMap::new();
        for (uid, link) in &box_data.links {
            if let Some(sources) = link.typed_sources() {
                core.set_link(uid.clone(), Some(sources));
            }
            titles.insert(uid.clone(), link.title.clone());
        }
        let app = Self {
            core,
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
            box_file: parts.box_file,
            box_data,
            library_dir: parts.library_dir,
            linking: false,
            last_card: None,
            titles,
            wifi: SimWifi::new(parts.wifi_setup),
            file_sizes: HashMap::new(),
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

    /// A card was placed on the reader: `user_memory` is what the reader read.
    /// In link mode the card is captured for the web app instead of played.
    pub async fn tap(&mut self, now: Millis, uid: Uid, user_memory: &[u8]) {
        let read = read_tag(user_memory);
        let content = match &read {
            CardRead::Card(card) => {
                if let Some(title) = card.title() {
                    self.titles.entry(uid.clone()).or_insert_with(|| title.as_str().to_owned());
                }
                match card.kind() {
                    CardKind::Content(sources) => format!("tapbox card: {}", describe_sources(sources)),
                    CardKind::Control(command) => format!("control card: {command}"),
                }
            }
            CardRead::Blank => "blank card".to_owned(),
            CardRead::Invalid { reason } => format!("unreadable: {reason}"),
        };
        let captured = self.linking;
        self.last_card = Some(LastCard { uid: uid.clone(), at: SystemTime::now(), content, captured });
        if captured {
            self.linking = false;
            if self.echo {
                println!("  → card {uid} captured for linking");
            }
            self.execute(now, Command::Flash(tapbox_core::Flash::Acknowledge)).await;
            return;
        }
        self.handle(now, Event::CardPresent { uid, read }).await;
    }

    /// Carry out a web app request.
    ///
    /// # Errors
    /// A message for the user when the request can't be done.
    pub async fn web(&mut self, now: Millis, action: WebAction) -> Result<(), String> {
        match action {
            WebAction::StartLinking => self.linking = true,
            WebAction::CancelLinking => self.linking = false,
            WebAction::Link { uid, title, hash } => {
                if !self.resolver.has(&hash) {
                    return Err("that file is not in the library".into());
                }
                let link = Link { title: title.clone(), sources: vec![hash.to_ni()] };
                self.core.set_link(uid.clone(), link.typed_sources());
                self.titles.insert(uid.clone(), title);
                self.box_data.links.insert(uid, link);
                self.save_box_data()?;
            }
            WebAction::Unlink { uid } => {
                self.core.set_link(uid.clone(), None);
                self.box_data.links.remove(&uid);
                self.titles.remove(&uid);
                self.save_box_data()?;
            }
            WebAction::FileAdded { hash, path } => self.resolver.add_library(hash, path),
            WebAction::DeleteFile { hash } => {
                let path = self.resolver.remove_library(&hash).ok_or("no such file")?;
                std::fs::remove_file(&path).map_err(|err| format!("could not delete {}: {err}", path.display()))?;
            }
            WebAction::SetName(name) => {
                let name = name.trim();
                if name.is_empty() || name.len() > 40 {
                    return Err("the name must be 1 to 40 characters".into());
                }
                self.box_data.name = Some(name.to_owned());
                self.save_box_data()?;
            }
            WebAction::SetVolume(volume) => self.handle(now, Event::SetVolume(Percent::saturating(volume))).await,
            WebAction::SetMaxVolume(max) => {
                let command = tapbox_card::ControlCommand::MaxVolume(Percent::saturating(max));
                self.handle(now, Event::Control(command)).await;
            }
            WebAction::SetOnCardRemoved(behaviour) => self.handle(now, Event::SetOnCardRemoved(behaviour)).await,
            WebAction::PairBluetooth => {
                self.handle(now, Event::Control(tapbox_card::ControlCommand::BluetoothPair)).await;
            }
            WebAction::WifiConnect { ssid, password } => {
                if !self.wifi.networks().iter().any(|network| network.ssid == ssid) {
                    return Err("that network isn't in range".into());
                }
                self.wifi.begin_connect(&ssid);
                let msgs = self.msgs.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    let _ = msgs.send(AppMsg::WifiAttempt { ssid, password });
                });
            }
            WebAction::WifiSetup => self.handle(now, Event::Control(tapbox_card::ControlCommand::WifiSetup)).await,
            WebAction::TogglePlay => {
                use tapbox_core::{Button, ButtonState};
                self.handle(now, Event::Button { button: Button::Play, state: ButtonState::Pressed }).await;
                self.handle(now, Event::Button { button: Button::Play, state: ButtonState::Released }).await;
            }
        }
        Ok(())
    }

    /// Wi-Fi state and visible networks.
    #[must_use]
    pub fn wifi(&self) -> &SimWifi {
        &self.wifi
    }

    /// Size of a library file in bytes (cached).
    pub fn file_size(&mut self, hash: &ContentHash, path: &std::path::Path) -> u64 {
        *self.file_sizes.entry(*hash).or_insert_with(|| std::fs::metadata(path).map_or(0, |m| m.len()))
    }

    fn save_box_data(&self) -> Result<(), String> {
        self.box_file.save(&self.box_data).map_err(|err| format!("could not save: {err:#}"))
    }

    /// Box name (default "tapbox").
    #[must_use]
    pub fn name(&self) -> &str {
        self.box_data.name.as_deref().unwrap_or("tapbox")
    }

    /// Cards linked on this box.
    #[must_use]
    pub fn links(&self) -> &std::collections::BTreeMap<Uid, Link> {
        &self.box_data.links
    }

    /// Library files, by hash.
    pub fn library_files(&self) -> impl Iterator<Item = (&ContentHash, &PathBuf)> {
        self.resolver.library_files()
    }

    /// Where uploads go.
    #[must_use]
    pub fn library_dir(&self) -> &std::path::Path {
        &self.library_dir
    }

    /// Whether the next tap will be captured for linking.
    #[must_use]
    pub fn is_linking(&self) -> bool {
        self.linking
    }

    /// The last tapped card.
    #[must_use]
    pub fn last_card(&self) -> Option<&LastCard> {
        self.last_card.as_ref()
    }

    /// Title for a card, if known.
    #[must_use]
    pub fn title(&self, uid: &Uid) -> Option<&str> {
        self.titles.get(uid).map(String::as_str)
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
            AppMsg::WifiAttempt { ssid, password } => {
                if self.wifi.finish(&ssid, &password) {
                    self.handle(now, Event::WifiSetupEnded).await;
                }
            }
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
                self.wifi.enter_setup();
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

fn describe_sources(sources: &Sources) -> String {
    sources
        .iter()
        .map(|source| match source {
            Source::Hash(_) => "a library file".to_owned(),
            other => other.to_uri(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

use std::collections::BTreeMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tapbox_card::{CardKind, ControlCommand, Percent, Sources};

use crate::buttons::{ButtonTracker, Gesture};
use crate::types::{
    Button, ButtonState, CardRead, Command, Event, ExternalSource, Failure, Flash, LedState, Millis, PlayerEvent,
    Problem, ProblemKind, Uid,
};

/// What happens when the card is lifted while it plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OnCardRemoved {
    /// Pause, like a Toniebox (default).
    #[default]
    Pause,
    /// Keep playing.
    KeepPlaying,
}

/// Settings the core needs. Loaded from the daemon's config file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Config {
    /// Volume at first boot (later boots use the saved volume).
    pub initial_volume: Percent,
    /// Hearing protection: the volume can never go above this.
    pub max_volume: Percent,
    /// Percentage points per volume button press.
    pub volume_step: u8,
    /// Behaviour when the card is lifted.
    pub on_card_removed: OnCardRemoved,
    /// Whether control cards (`c` field) are obeyed.
    pub control_cards: bool,
    /// How long Bluetooth pairing stays open.
    #[serde(with = "seconds")]
    pub pairing_window: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            initial_volume: Percent::saturating(40),
            max_volume: Percent::saturating(70),
            volume_step: 5,
            on_card_removed: OnCardRemoved::Pause,
            control_cards: true,
            pairing_window: Duration::from_secs(120),
        }
    }
}

mod seconds {
    use std::time::Duration;

    use serde::{Deserialize, Deserializer, Serializer};

    pub(super) fn serialize<S: Serializer>(value: &Duration, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u64(value.as_secs())
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Duration, D::Error> {
        u64::deserialize(deserializer).map(Duration::from_secs)
    }
}

/// State that survives a reboot: only the parents' maximum volume. Playback
/// state (resume positions, current volume) deliberately does not: switching
/// the box off is a fresh start. Wi-Fi and Bluetooth pairings are kept by
/// NetworkManager and BlueZ, not here.
///
/// The daemon writes it when the core emits [`Command::Persist`], which only
/// happens when a vol-max control card changes the limit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PersistentState {
    /// Maximum volume set by a control card (overrides the config).
    pub max_volume: Option<Percent>,
}

/// What the player is doing, as far as the core knows. For status displays and tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Playback {
    /// Nothing loaded.
    Idle,
    /// Playing this card.
    Playing(Uid),
    /// This card is loaded and paused.
    Paused(Uid),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum State {
    Idle,
    Playing { uid: Uid },
    Paused { uid: Uid },
}

/// The tapbox state machine. See the crate docs.
#[derive(Debug)]
#[allow(clippy::struct_excessive_bools, reason = "independent flags, each read in one place")]
pub struct Core {
    config: Config,
    uid_map: BTreeMap<Uid, Sources>,
    volume: Percent,
    max_volume: Percent,
    positions: BTreeMap<Uid, Duration>,
    state: State,
    /// Content card currently on the reader.
    card_on_reader: Option<Uid>,
    /// External source that currently owns the speaker.
    external: Option<ExternalSource>,
    airplay_muted: bool,
    buttons: ButtonTracker,
    pairing_until: Option<Millis>,
    wifi_setup: bool,
    battery_low: bool,
    powering_off: bool,
    sleep_at: Option<Millis>,
    led: LedState,
    started: bool,
    persist_now: bool,
}

impl Core {
    /// A core with the given config and UID → sources mapping for blank cards.
    #[must_use]
    pub fn new(config: Config, uid_map: BTreeMap<Uid, Sources>) -> Self {
        Self {
            volume: config.initial_volume.min(config.max_volume),
            max_volume: config.max_volume,
            config,
            uid_map,
            positions: BTreeMap::new(),
            state: State::Idle,
            card_on_reader: None,
            external: None,
            airplay_muted: false,
            buttons: ButtonTracker::default(),
            pairing_until: None,
            wifi_setup: false,
            battery_low: false,
            powering_off: false,
            sleep_at: None,
            led: LedState::Booting,
            started: false,
            persist_now: false,
        }
    }

    /// Restore state saved by an earlier run.
    #[must_use]
    pub fn with_state(mut self, state: PersistentState) -> Self {
        if let Some(max) = state.max_volume {
            self.max_volume = max;
            self.volume = self.volume.min(max);
        }
        self
    }

    /// Snapshot of what should survive a reboot.
    #[must_use]
    pub fn persistent_state(&self) -> PersistentState {
        PersistentState { max_volume: (self.max_volume != self.config.max_volume).then_some(self.max_volume) }
    }

    /// Current playback state.
    #[must_use]
    pub fn playback(&self) -> Playback {
        match &self.state {
            State::Idle => Playback::Idle,
            State::Playing { uid } => Playback::Playing(uid.clone()),
            State::Paused { uid } => Playback::Paused(uid.clone()),
        }
    }

    /// Current volume.
    #[must_use]
    pub fn volume(&self) -> Percent {
        self.volume
    }

    /// Current maximum volume.
    #[must_use]
    pub fn max_volume(&self) -> Percent {
        self.max_volume
    }

    /// Saved resume position for a card.
    #[must_use]
    pub fn position(&self, uid: &Uid) -> Option<Duration> {
        self.positions.get(uid).copied()
    }

    /// The LED state last emitted.
    #[must_use]
    pub fn led(&self) -> LedState {
        self.led
    }

    /// Feed one event; returns what to do, in order.
    pub fn handle(&mut self, now: Millis, event: Event) -> Vec<Command> {
        let mut out = Vec::new();
        if !self.started {
            self.started = true;
            out.push(Command::SetVolume(self.volume));
        }
        match event {
            Event::Tick => self.on_tick(now, &mut out),
            Event::CardPresent { uid, read } => self.on_card(now, uid, read, &mut out),
            Event::CardRemoved => self.on_card_removed(&mut out),
            Event::Button { button, state } => self.on_button(now, button, state, &mut out),
            Event::Player(event) => self.on_player(event, &mut out),
            Event::ExternalStarted(source) => self.on_external_started(source, &mut out),
            Event::ExternalStopped(source) => self.on_external_stopped(source, &mut out),
            Event::BatteryLow => self.battery_low = true,
            Event::BatteryOk => self.battery_low = false,
            Event::WifiSetupEnded => self.wifi_setup = false,
            Event::PowerOff => self.power_off(&mut out),
        }
        self.finish(&mut out);
        out
    }

    fn on_tick(&mut self, now: Millis, out: &mut Vec<Command>) {
        if let Some(gesture) = self.buttons.on_tick(now) {
            self.on_gesture(now, gesture, out);
        }
        if self.pairing_until.is_some_and(|until| now >= until) {
            self.pairing_until = None;
        }
        if self.sleep_at.is_some_and(|at| now >= at) {
            self.sleep_at = None;
            self.pause(out);
        }
    }

    fn on_card(&mut self, now: Millis, uid: Uid, read: CardRead, out: &mut Vec<Command>) {
        match read {
            CardRead::Invalid { reason } => problem(out, uid, ProblemKind::InvalidCard(reason)),
            CardRead::Blank => match self.uid_map.get(&uid).cloned() {
                Some(sources) => self.play_card(uid, sources, out),
                None => problem(out, uid, ProblemKind::UnknownCard),
            },
            CardRead::Card(card) => match card.kind() {
                CardKind::Content(sources) => self.play_card(uid, sources.clone(), out),
                CardKind::Control(command) if self.config.control_cards => {
                    out.push(Command::Flash(Flash::Acknowledge));
                    self.on_control(now, *command, out);
                }
                CardKind::Control(_) => problem(out, uid, ProblemKind::ControlCardsDisabled),
            },
        }
    }

    fn play_card(&mut self, uid: Uid, sources: Sources, out: &mut Vec<Command>) {
        out.push(Command::Flash(Flash::CardRecognised));
        self.take_speaker(out);
        match &self.state {
            State::Playing { uid: current } if *current == uid => {}
            State::Paused { uid: current } if *current == uid => out.push(Command::Resume),
            _ => out.push(Command::Play {
                start: self.positions.get(&uid).copied().unwrap_or_default(),
                uid: uid.clone(),
                sources,
            }),
        }
        self.state = State::Playing { uid: uid.clone() };
        self.card_on_reader = Some(uid);
    }

    fn on_card_removed(&mut self, out: &mut Vec<Command>) {
        let Some(lifted) = self.card_on_reader.take() else {
            return;
        };
        let playing_lifted = matches!(&self.state, State::Playing { uid } if *uid == lifted);
        if playing_lifted && self.config.on_card_removed == OnCardRemoved::Pause {
            self.pause(out);
        }
    }

    fn on_button(&mut self, now: Millis, button: Button, state: ButtonState, out: &mut Vec<Command>) {
        if let Some(gesture) = self.buttons.on_edge(now, button, state) {
            self.on_gesture(now, gesture, out);
        }
    }

    fn on_gesture(&mut self, now: Millis, gesture: Gesture, out: &mut Vec<Command>) {
        match gesture {
            Gesture::PlayClick => match &self.state {
                State::Playing { .. } => self.pause(out),
                State::Paused { uid } => {
                    let uid = uid.clone();
                    self.take_speaker(out);
                    out.push(Command::Resume);
                    self.state = State::Playing { uid };
                }
                State::Idle => {}
            },
            Gesture::VolumeUp => {
                let step = self.config.volume_step;
                self.set_volume(Percent::saturating(self.volume.get().saturating_add(step)), out);
            }
            Gesture::VolumeDown => {
                let step = self.config.volume_step;
                self.set_volume(Percent::saturating(self.volume.get().saturating_sub(step)), out);
            }
            Gesture::BluetoothPairing => {
                out.push(Command::Flash(Flash::Acknowledge));
                self.open_pairing(now, out);
            }
            Gesture::WifiSetup => {
                out.push(Command::Flash(Flash::Acknowledge));
                self.enter_wifi_setup(out);
            }
        }
    }

    fn on_control(&mut self, now: Millis, command: ControlCommand, out: &mut Vec<Command>) {
        match command {
            ControlCommand::BluetoothPair => self.open_pairing(now, out),
            ControlCommand::WifiSetup => self.enter_wifi_setup(out),
            ControlCommand::SayName => out.push(Command::SayName),
            ControlCommand::MaxVolume(max) => {
                self.max_volume = max;
                self.persist_now = true;
                if self.volume > max {
                    self.set_volume(max, out);
                }
            }
            ControlCommand::Sleep { minutes } => {
                self.sleep_at = Some(now.plus(Duration::from_secs(u64::from(minutes) * 60)));
            }
        }
    }

    fn on_player(&mut self, event: PlayerEvent, out: &mut Vec<Command>) {
        let loaded = match &self.state {
            State::Playing { uid } | State::Paused { uid } => uid.clone(),
            State::Idle => return,
        };
        match event {
            PlayerEvent::Position(position) => {
                self.positions.insert(loaded, position);
            }
            PlayerEvent::Ended => {
                self.positions.remove(&loaded);
                self.state = State::Idle;
            }
            PlayerEvent::Failed(failure) => {
                let kind = match failure {
                    Failure::NothingPlayable(detail) => ProblemKind::NothingPlayable(detail),
                    Failure::Download(detail) => ProblemKind::DownloadFailed(detail),
                    Failure::Playback(detail) => ProblemKind::PlaybackFailed(detail),
                };
                problem(out, loaded, kind);
                self.state = State::Idle;
            }
        }
    }

    fn on_external_started(&mut self, source: ExternalSource, out: &mut Vec<Command>) {
        // Last source to start wins.
        self.pause(out);
        if source == ExternalSource::AirPlay && self.airplay_muted {
            self.airplay_muted = false;
            out.push(Command::MuteAirPlay(false));
        }
        self.external = Some(source);
    }

    fn on_external_stopped(&mut self, source: ExternalSource, out: &mut Vec<Command>) {
        if self.external == Some(source) {
            self.external = None;
        }
        if source == ExternalSource::AirPlay && self.airplay_muted {
            self.airplay_muted = false;
            out.push(Command::MuteAirPlay(false));
        }
    }

    /// The card takes the speaker from whichever external source has it.
    fn take_speaker(&mut self, out: &mut Vec<Command>) {
        match self.external.take() {
            Some(ExternalSource::Bluetooth) => out.push(Command::PauseBluetooth),
            Some(ExternalSource::AirPlay) => {
                self.airplay_muted = true;
                out.push(Command::MuteAirPlay(true));
            }
            None => {}
        }
    }

    fn pause(&mut self, out: &mut Vec<Command>) {
        if let State::Playing { uid } = &self.state {
            self.state = State::Paused { uid: uid.clone() };
            out.push(Command::Pause);
        }
    }

    /// Clean shutdown in progress: stop playing and show it on the LED.
    fn power_off(&mut self, out: &mut Vec<Command>) {
        self.powering_off = true;
        self.pause(out);
    }

    fn set_volume(&mut self, volume: Percent, out: &mut Vec<Command>) {
        let volume = volume.min(self.max_volume);
        if volume != self.volume {
            self.volume = volume;
            out.push(Command::SetVolume(volume));
        }
    }

    fn open_pairing(&mut self, now: Millis, out: &mut Vec<Command>) {
        self.pairing_until = Some(now.plus(self.config.pairing_window));
        out.push(Command::OpenBluetoothPairing(self.config.pairing_window));
    }

    fn enter_wifi_setup(&mut self, out: &mut Vec<Command>) {
        self.wifi_setup = true;
        out.push(Command::EnterWifiSetup);
    }

    fn led_state(&self) -> LedState {
        if self.powering_off {
            LedState::ShuttingDown
        } else if self.wifi_setup {
            LedState::WifiSetup
        } else if self.pairing_until.is_some() {
            LedState::BluetoothPairing
        } else if self.battery_low {
            LedState::BatteryLow
        } else if self.external == Some(ExternalSource::AirPlay) {
            LedState::AirPlay
        } else if matches!(self.state, State::Playing { .. }) {
            LedState::Playing
        } else {
            LedState::Idle
        }
    }

    fn finish(&mut self, out: &mut Vec<Command>) {
        let led = self.led_state();
        if led != self.led {
            self.led = led;
            out.push(Command::Led(led));
        }
        if self.persist_now {
            out.push(Command::Persist);
            self.persist_now = false;
        }
    }
}

/// Red flash plus a log entry: every problem is both seen on the box and
/// explained in the web app.
fn problem(out: &mut Vec<Command>, uid: Uid, kind: ProblemKind) {
    out.push(Command::Flash(Flash::Error));
    out.push(Command::Problem(Problem { uid, kind }));
}

#[cfg(test)]
#[path = "core_tests.rs"]
mod tests;

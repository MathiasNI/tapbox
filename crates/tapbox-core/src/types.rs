use std::fmt;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tapbox_card::{Card, Percent, Sources};

/// A monotonic timestamp in milliseconds, supplied by whoever drives the core.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Millis(pub u64);

impl Millis {
    /// `self + duration`, saturating.
    #[must_use]
    pub fn plus(self, duration: Duration) -> Self {
        Self(self.0.saturating_add(u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)))
    }

    /// Time elapsed since `earlier` (zero if `earlier` is in the future).
    #[must_use]
    pub fn since(self, earlier: Self) -> Duration {
        Duration::from_millis(self.0.saturating_sub(earlier.0))
    }
}

/// An NFC tag UID: 4, 7 or 10 bytes.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Uid(Vec<u8>);

impl Uid {
    /// `None` unless `bytes` is 4, 7 or 10 bytes long.
    #[must_use]
    pub fn new(bytes: Vec<u8>) -> Option<Self> {
        matches!(bytes.len(), 4 | 7 | 10).then_some(Self(bytes))
    }

    /// Parse lower- or upper-case hex, optionally with `:` separators.
    #[must_use]
    pub fn from_hex(hex: &str) -> Option<Self> {
        let digits: Vec<u8> = hex.bytes().filter(|b| *b != b':').collect();
        if !digits.len().is_multiple_of(2) {
            return None;
        }
        let bytes = digits
            .chunks(2)
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok())
            .collect::<Option<Vec<u8>>>()?;
        Self::new(bytes)
    }

    /// The raw bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Display for Uid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.iter().try_for_each(|byte| write!(f, "{byte:02x}"))
    }
}

impl fmt::Debug for Uid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Uid({self})")
    }
}

impl TryFrom<String> for Uid {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::from_hex(&value).ok_or_else(|| format!("`{value}` is not a 4, 7 or 10 byte hex UID"))
    }
}

impl From<Uid> for String {
    fn from(value: Uid) -> Self {
        value.to_string()
    }
}

/// What was read from a tag, already decoded by the reader task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CardRead {
    /// A valid tapbox (or compatible URI) card.
    Card(Card),
    /// No usable NDEF data; look the UID up in the local mapping.
    Blank,
    /// The card could not be decoded (malformed, newer spec version, …).
    Invalid {
        /// Why, for the problem log.
        reason: String,
    },
}

/// The three front buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Button {
    /// Volume down.
    VolumeDown,
    /// Play / pause.
    Play,
    /// Volume up.
    VolumeUp,
}

/// Button edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonState {
    /// The button went down.
    Pressed,
    /// The button came up.
    Released,
}

/// Audio sources other than cards that can take the speaker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExternalSource {
    /// A phone streaming over Bluetooth A2DP.
    Bluetooth,
    /// An AirPlay 2 session via shairport-sync.
    AirPlay,
}

/// Feedback from the player.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlayerEvent {
    /// Current playback position.
    Position(Duration),
    /// The content played to the end.
    Ended,
    /// The card's content could not be played.
    Failed(Failure),
}

/// Why a card's content could not be played.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// None of the card's sources can be played on this box (file not in the
    /// library and no URL, `file:` paths, Spotify without the plugin, …).
    NothingPlayable(String),
    /// A download failed or did not match the card's hash.
    Download(String),
    /// The player could not open or decode it.
    Playback(String),
}

/// Something went wrong with a card. Shown as a red flash on the box and
/// kept in the problem log for the web app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    /// The card it happened with.
    pub uid: Uid,
    /// What happened.
    pub kind: ProblemKind,
}

/// What went wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProblemKind {
    /// The card's data is broken or from a newer spec version.
    InvalidCard(String),
    /// A blank card whose UID is not linked to anything.
    UnknownCard,
    /// A control card, but control cards are turned off.
    ControlCardsDisabled,
    /// None of the card's sources can be played here.
    NothingPlayable(String),
    /// Download failed or the file didn't match the card's hash.
    DownloadFailed(String),
    /// The player couldn't play the file or stream.
    PlaybackFailed(String),
}

impl ProblemKind {
    /// Stable kebab-case name (testbench `expect problem`, logs, web app).
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::InvalidCard(_) => "invalid-card",
            Self::UnknownCard => "unknown-card",
            Self::ControlCardsDisabled => "control-cards-disabled",
            Self::NothingPlayable(_) => "nothing-playable",
            Self::DownloadFailed(_) => "download-failed",
            Self::PlaybackFailed(_) => "playback-failed",
        }
    }

    /// One-line explanation for people.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::InvalidCard(reason) => format!("card data is invalid: {reason}"),
            Self::UnknownCard => "blank card that isn't linked to anything yet".to_owned(),
            Self::ControlCardsDisabled => "control card, but control cards are turned off".to_owned(),
            Self::NothingPlayable(detail) => format!("nothing on this card can be played here: {detail}"),
            Self::DownloadFailed(detail) => format!("download failed: {detail}"),
            Self::PlaybackFailed(detail) => format!("could not play it: {detail}"),
        }
    }
}

/// Input to the core.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// Time passed. Drives button gestures, timers and periodic saves.
    Tick,
    /// A card was placed on the reader.
    CardPresent {
        /// The tag's UID.
        uid: Uid,
        /// The decoded contents.
        read: CardRead,
    },
    /// The card was lifted off the reader.
    CardRemoved,
    /// A button changed state.
    Button {
        /// Which button.
        button: Button,
        /// Pressed or released.
        state: ButtonState,
    },
    /// Feedback from the player.
    Player(PlayerEvent),
    /// Bluetooth or AirPlay started playing.
    ExternalStarted(ExternalSource),
    /// Bluetooth or AirPlay stopped playing.
    ExternalStopped(ExternalSource),
    /// The battery board reports a low battery.
    BatteryLow,
    /// The battery is charging or no longer low.
    BatteryOk,
    /// Wi-Fi setup finished: connected to a network, or the hotspot timed out
    /// (reported by comitup).
    WifiSetupEnded,
    /// A control command from the web app (same effect as the control card).
    Control(tapbox_card::ControlCommand),
    /// Set the volume from the web app (still capped by the max volume).
    SetVolume(Percent),
    /// Change what happens when a card is lifted (web app setting).
    SetOnCardRemoved(crate::OnCardRemoved),
    /// The system is shutting down cleanly: the battery board reached its
    /// low-battery threshold, or someone ran `poweroff`. The daemon sees this
    /// as SIGTERM from systemd. (The master switch cuts power without
    /// warning; nothing is sent for that, and nothing needs to be.)
    PowerOff,
}

/// A one-shot LED flash on top of the current [`LedState`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flash {
    /// Green: card recognised.
    CardRecognised,
    /// Red: unknown card or error.
    Error,
    /// White: a button gesture or control card was accepted.
    Acknowledge,
}

/// The LED's steady state, from `docs/setup-ux.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedState {
    /// White pulse.
    Booting,
    /// Dim green.
    Idle,
    /// Green.
    Playing,
    /// Purple.
    AirPlay,
    /// Blue blinking.
    BluetoothPairing,
    /// Orange blinking.
    WifiSetup,
    /// Red slow blink.
    BatteryLow,
    /// White fading out.
    ShuttingDown,
}

/// Output of the core: something for the I/O layer to carry out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Resolve `sources` and start playing from `start`.
    Play {
        /// Card being played; position reports are attributed to it.
        uid: Uid,
        /// Sources from the card, in priority order.
        sources: Sources,
        /// Where to start.
        start: Duration,
    },
    /// Pause the current content, keeping it loaded.
    Pause,
    /// Resume the paused content.
    Resume,
    /// Stop and unload the current content.
    Stop,
    /// Set output volume (already capped by the maximum volume).
    SetVolume(Percent),
    /// Ask a Bluetooth sender to pause (AVRCP).
    PauseBluetooth,
    /// Mute or unmute the AirPlay stream locally (AirPlay 2 can't be paused remotely).
    MuteAirPlay(bool),
    /// Make the box discoverable for Bluetooth pairing.
    OpenBluetoothPairing(Duration),
    /// Start the Wi-Fi setup hotspot.
    EnterWifiSetup,
    /// Speak the box's name and address.
    SayName,
    /// Set the LED's steady state.
    Led(LedState),
    /// Flash the LED once.
    Flash(Flash),
    /// Record a problem (always comes with a red flash).
    Problem(Problem),
    /// Save [`crate::PersistentState`] now (only after a max-volume change).
    Persist,
}

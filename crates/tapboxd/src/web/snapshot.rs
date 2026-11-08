//! Everything the page shows, in one JSON object. Rebuilt by the app loop
//! and pushed to the page whenever it changes.

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tapbox_card::Source;
use tapbox_core::{Flash, LedState, OnCardRemoved, Playback};

use crate::app::App;
use crate::outputs::{WifiNetwork, WifiState, describe_led};

/// The page's view of the box.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Snapshot {
    /// Box name.
    pub name: String,
    /// Running in the testbench (shows the simulator panel).
    pub simulated: bool,
    /// What's playing.
    pub playback: PlaybackView,
    /// Volume now, 0 to 100.
    pub volume: u8,
    /// Hearing-protection limit, 0 to 100.
    pub max_volume: u8,
    /// `pause` or `keep-playing`.
    pub on_card_removed: OnCardRemoved,
    /// Status light.
    pub led: LedView,
    /// Waiting for a card to link.
    pub linking: bool,
    /// The last card put on the reader.
    pub last_card: Option<LastCardView>,
    /// Cards linked on this box.
    pub cards: Vec<CardView>,
    /// Audio files on the box.
    pub library: Vec<FileView>,
    /// Recent problems, newest first.
    pub problems: Vec<ProblemView>,
    /// Wi-Fi.
    pub wifi: WifiState,
    /// Networks in range (for the setup page).
    pub networks: Vec<WifiNetwork>,
    /// Virtual cards (testbench only).
    pub sim_cards: Vec<SimCardView>,
}

impl Snapshot {
    /// Whether the box is in Wi-Fi setup mode (or joining a network from it).
    #[must_use]
    pub fn wifi_in_setup(&self) -> bool {
        !matches!(self.wifi, WifiState::Connected { .. })
    }
}

/// What's playing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlaybackView {
    /// `idle`, `playing` or `paused`.
    pub state: &'static str,
    /// Card UID.
    pub uid: Option<String>,
    /// Card title, if known.
    pub title: Option<String>,
}

/// Status light.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LedView {
    /// Machine name, e.g. `playing`.
    pub state: &'static str,
    /// For people, e.g. "green (playing)".
    pub description: &'static str,
    /// Last flash: `card`, `error` or `ack`.
    pub flash: Option<&'static str>,
    /// Increments on every flash.
    pub flash_count: u64,
}

/// The last card put on the reader.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LastCardView {
    /// UID in hex.
    pub uid: String,
    /// What is on it.
    pub content: String,
    /// Captured by link mode.
    pub captured: bool,
    /// Already linked on this box.
    pub linked: bool,
    /// Seconds since the epoch.
    pub at: u64,
}

/// A linked card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CardView {
    /// UID in hex.
    pub uid: String,
    /// Title.
    pub title: String,
    /// Library file it plays (hex hash), if it's a library link.
    pub hash: Option<String>,
    /// File name, if the file is in the library.
    pub file_name: Option<String>,
    /// The linked file is gone.
    pub missing: bool,
}

/// A library file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FileView {
    /// Hex hash.
    pub hash: String,
    /// File name.
    pub name: String,
    /// Bytes.
    pub size: u64,
    /// Number of linked cards that play it.
    pub used_by: usize,
}

/// A logged problem.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProblemView {
    /// Seconds since the epoch.
    pub at: u64,
    /// Card UID in hex.
    pub uid: String,
    /// Card title, if known.
    pub card: Option<String>,
    /// Machine name, e.g. `unknown-card`.
    pub kind: &'static str,
    /// For people.
    pub message: String,
}

/// A virtual card (testbench).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SimCardView {
    /// Name to tap it by.
    pub name: String,
    /// What it is.
    pub description: String,
}

fn unix(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

fn led_name(state: LedState) -> &'static str {
    match state {
        LedState::Booting => "booting",
        LedState::Idle => "idle",
        LedState::Playing => "playing",
        LedState::AirPlay => "airplay",
        LedState::BluetoothPairing => "bt-pairing",
        LedState::WifiSetup => "wifi-setup",
        LedState::BatteryLow => "battery-low",
        LedState::ShuttingDown => "shutting-down",
    }
}

/// Build the page's view of `app`.
pub fn build_snapshot(app: &mut App, simulated: bool, sim_cards: Vec<SimCardView>) -> Snapshot {
    let core = app.core();
    let (state, uid) = match core.playback() {
        Playback::Idle => ("idle", None),
        Playback::Playing(uid) => ("playing", Some(uid)),
        Playback::Paused(uid) => ("paused", Some(uid)),
    };
    let playback = PlaybackView {
        state,
        title: uid.as_ref().and_then(|uid| app.title(uid)).map(str::to_owned),
        uid: uid.map(|uid| uid.to_string()),
    };
    let volume = core.volume().get();
    let max_volume = core.max_volume().get();
    let on_card_removed = core.card_removed_behaviour();
    let outputs = app.outputs();
    let led = LedView {
        state: led_name(outputs.led()),
        description: describe_led(outputs.led()),
        flash: outputs.last_flash().map(|flash| match flash {
            Flash::CardRecognised => "card",
            Flash::Error => "error",
            Flash::Acknowledge => "ack",
        }),
        flash_count: outputs.flash_count(),
    };

    let files: Vec<(String, String, std::path::PathBuf, tapbox_card::ContentHash)> = app
        .library_files()
        .map(|(hash, path)| {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("?").to_owned();
            (hash.to_hex(), name, path.clone(), *hash)
        })
        .collect();
    let names: BTreeMap<String, String> = files.iter().map(|(hex, name, ..)| (hex.clone(), name.clone())).collect();

    let cards: Vec<CardView> = app
        .links()
        .iter()
        .map(|(uid, link)| {
            let hash = link.typed_sources().and_then(|sources| {
                sources.iter().find_map(|source| match source {
                    Source::Hash(hash) => Some(hash.to_hex()),
                    _ => None,
                })
            });
            let file_name = hash.as_ref().and_then(|hash| names.get(hash).cloned());
            CardView {
                uid: uid.to_string(),
                title: link.title.clone(),
                missing: hash.is_some() && file_name.is_none(),
                hash,
                file_name,
            }
        })
        .collect();

    let mut library: Vec<FileView> = files
        .into_iter()
        .map(|(hex, name, path, hash)| {
            let used_by = cards.iter().filter(|card| card.hash.as_deref() == Some(hex.as_str())).count();
            FileView { size: app.file_size(&hash, &path), hash: hex, name, used_by }
        })
        .collect();
    library.sort_by_key(|file| file.name.to_lowercase());

    let problems = app
        .problems()
        .rev()
        .map(|record| ProblemView {
            at: unix(record.at),
            uid: record.problem.uid.to_string(),
            card: app.title(&record.problem.uid).map(str::to_owned),
            kind: record.problem.kind.name(),
            message: record.problem.kind.describe(),
        })
        .collect();

    let last_card = app.last_card().map(|card| LastCardView {
        uid: card.uid.to_string(),
        content: card.content.clone(),
        captured: card.captured,
        linked: app.core().is_linked(&card.uid),
        at: unix(card.at),
    });

    Snapshot {
        name: app.name().to_owned(),
        simulated,
        playback,
        volume,
        max_volume,
        on_card_removed,
        led,
        linking: app.is_linking(),
        last_card,
        cards,
        library,
        problems,
        wifi: app.wifi().state().clone(),
        networks: app.wifi().networks().to_vec(),
        sim_cards,
    }
}

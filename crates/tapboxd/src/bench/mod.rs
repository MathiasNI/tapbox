//! The testbench: run the real daemon logic against simulated hardware.
//!
//! A bench directory (the repo's `testbench/`) holds:
//!
//! - `tapbox.toml`: a normal daemon config (library, UID map, core settings),
//! - `cards/*.json`: virtual cards, encoded to real NDEF bytes and decoded by
//!   the same code path as a real reader,
//! - `library/`: audio files, found by content hash,
//! - `scenarios/*.tbx`: scripted tests (see [`script`]).
//!
//! Two ways to use it: `tapboxd sim` is an interactive console on real time;
//! `tapboxd test` runs scenarios on a virtual clock and checks expectations.

pub mod repl;
pub mod runner;
pub mod script;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use serde::Deserialize;
use tapbox_card::ndef::{self, Record};
use tapbox_card::{Card, MEDIA_TYPE, encode_tag, encode_uri_tag};
use tapbox_core::{Core, Uid};
use tokio::sync::mpsc;

use crate::app::{App, AppMsg, Parts};
use crate::boxdata::BoxDataFile;
use crate::config::Config;
use crate::player::Player;
use crate::resolver::Resolver;
use crate::store::Store;

/// A virtual card.
#[derive(Debug, Clone)]
pub struct BenchCard {
    /// File stem, used in commands: `tap story`.
    pub name: String,
    /// The tag UID.
    pub uid: Uid,
    /// What a reader would read from the tag's user memory.
    pub bytes: Vec<u8>,
    /// Free-text note from the card file.
    pub description: Option<String>,
}

/// On-disk shape of `cards/<name>.json`. Exactly one of `card`, `uri`,
/// `raw-hex` may be given; none means a blank tag.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct CardFile {
    uid: Uid,
    description: Option<String>,
    /// A tapbox JSON payload, written as-is (it may be deliberately invalid).
    card: Option<serde_json::Value>,
    /// URI record written before the tapbox record.
    phone_uri: Option<String>,
    /// A tag holding only a URI record, as a phone app would write it.
    uri: Option<String>,
    /// Raw user memory in hex, for malformed-tag tests.
    raw_hex: Option<String>,
}

/// A loaded bench directory.
#[derive(Debug)]
pub struct Bench {
    /// Bench directory.
    pub dir: PathBuf,
    /// Its config.
    pub config: Config,
    /// Virtual cards by name.
    pub cards: BTreeMap<String, BenchCard>,
}

impl Bench {
    /// Load `dir/tapbox.toml` and `dir/cards/*.json`.
    ///
    /// # Errors
    /// If the config or a card file is missing or invalid.
    pub fn load(dir: &Path) -> Result<Self> {
        let config = Config::load(&dir.join("tapbox.toml"))?;
        let mut cards = BTreeMap::new();
        let cards_dir = dir.join("cards");
        for entry in fs::read_dir(&cards_dir).with_context(|| format!("reading {}", cards_dir.display()))? {
            let path = entry?.path();
            if path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }
            let card = load_card(&path).with_context(|| format!("loading {}", path.display()))?;
            cards.insert(card.name.clone(), card);
        }
        Ok(Self { dir: dir.to_path_buf(), config, cards })
    }

    /// Look a card up by name.
    ///
    /// # Errors
    /// If there is no such card.
    pub fn card(&self, name: &str) -> Result<&BenchCard> {
        self.cards.get(name).with_context(|| {
            let known: Vec<&str> = self.cards.keys().map(String::as_str).collect();
            format!("no card named `{name}` (known: {})", known.join(", "))
        })
    }

    /// Build an app on this bench's config.
    ///
    /// # Errors
    /// If the library can't be indexed.
    pub fn build_app(
        &self,
        player: Player,
        store: Option<Store>,
        echo: bool,
    ) -> Result<(App, mpsc::UnboundedReceiver<AppMsg>)> {
        self.build_app_with(player, store, echo, false)
    }

    /// As [`Bench::build_app`], optionally starting in Wi-Fi setup mode.
    ///
    /// # Errors
    /// If the library can't be indexed.
    pub fn build_app_with(
        &self,
        player: Player,
        store: Option<Store>,
        echo: bool,
        wifi_setup: bool,
    ) -> Result<(App, mpsc::UnboundedReceiver<AppMsg>)> {
        let saved = store.as_ref().map(Store::load).unwrap_or_default();
        let core = Core::new(self.config.core.clone(), self.config.uid_sources()?).with_state(saved);
        // Scenarios (memory store) hash fresh and write nothing; `sim` keeps an index on disk.
        let index = store.as_ref().filter(|store| store.is_on_disk()).map(|_| self.config.library_index_file());
        let resolver =
            Resolver::new(&self.config.library_dir, self.config.cache_dir(), index.as_deref(), self.config.allow_http)?;
        let box_file = match &store {
            Some(store) if store.is_on_disk() => BoxDataFile::new(self.config.box_file()),
            _ => BoxDataFile::memory(),
        };
        App::new(Parts {
            core,
            player,
            resolver,
            store,
            max_download_bytes: self.config.max_download_mb * 1024 * 1024,
            echo,
            box_file,
            library_dir: self.config.library_dir.clone(),
            wifi_setup,
        })
    }
}

fn load_card(path: &Path) -> Result<BenchCard> {
    let file: CardFile = serde_json::from_str(&fs::read_to_string(path)?)?;
    let name = path.file_stem().and_then(|stem| stem.to_str()).context("card file name is not UTF-8")?.to_owned();
    let bytes = match (&file.card, &file.uri, &file.raw_hex) {
        (Some(payload), None, None) => {
            let json = serde_json::to_vec(payload)?;
            match Card::from_json(&json) {
                Ok(card) => encode_tag(&card, file.phone_uri.as_deref())?,
                // Deliberately invalid card: write the payload as-is so the reader rejects it.
                Err(_) => ndef::encode_tlv(&ndef::encode_message(&[Record::media(MEDIA_TYPE, json)])?)?,
            }
        }
        (None, Some(uri), None) => encode_uri_tag(uri)?,
        (None, None, Some(hex)) => parse_hex(hex)?,
        (None, None, None) => vec![0x03, 0x00, 0xFE],
        _ => bail!("give at most one of `card`, `uri`, `raw-hex`"),
    };
    Ok(BenchCard { name, uid: file.uid, bytes, description: file.description })
}

/// Parse hex, ignoring whitespace.
///
/// # Errors
/// On an odd number of digits or a non-hex character.
pub fn parse_hex(hex: &str) -> Result<Vec<u8>> {
    let digits: Vec<u8> = hex.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    if !digits.len().is_multiple_of(2) {
        bail!("odd number of hex digits");
    }
    digits
        .chunks(2)
        .map(|pair| {
            let pair = std::str::from_utf8(pair)?;
            u8::from_str_radix(pair, 16).with_context(|| format!("`{pair}` is not hex"))
        })
        .collect()
}

/// Format bytes as spaced hex, 32 per line.
#[must_use]
pub fn format_hex(bytes: &[u8]) -> String {
    bytes
        .chunks(32)
        .map(|line| line.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" "))
        .collect::<Vec<_>>()
        .join("\n")
}

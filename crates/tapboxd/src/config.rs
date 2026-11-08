//! The daemon's TOML config file. Relative paths are resolved against the
//! directory the config file is in.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use serde::Deserialize;
use tapbox_card::{Source, Sources};
use tapbox_core::Uid;

/// Top-level config.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Config {
    /// Writable data: state file and download cache.
    pub data_dir: PathBuf,
    /// Audio files, found by content hash.
    pub library_dir: PathBuf,
    /// Allow `http:` sources (local servers).
    #[serde(default = "yes")]
    pub allow_http: bool,
    /// Largest download accepted, in megabytes.
    #[serde(default = "default_max_download_mb")]
    pub max_download_mb: u64,
    /// Behaviour settings passed to the core.
    #[serde(default)]
    pub core: tapbox_core::Config,
    /// Blank cards: UID → sources.
    #[serde(default)]
    pub uid_map: BTreeMap<Uid, Vec<String>>,
    /// Pin assignments, used by the hardware drivers.
    #[serde(default)]
    pub hardware: Hardware,
}

/// BCM GPIO numbers, defaults from `docs/hardware.md`.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Hardware {
    /// Volume down button.
    pub button_volume_down: u32,
    /// Play button.
    pub button_play: u32,
    /// Volume up button.
    pub button_volume_up: u32,
    /// RGB LED red, green, blue.
    pub led_rgb: [u32; 3],
    /// PN532 IRQ line, if wired.
    pub nfc_irq: Option<u32>,
    /// I²C bus device.
    pub i2c_bus: PathBuf,
}

impl Default for Hardware {
    fn default() -> Self {
        Self {
            button_volume_down: 5,
            button_play: 27,
            button_volume_up: 13,
            led_rgb: [12, 16, 26],
            nfc_irq: Some(4),
            i2c_bus: PathBuf::from("/dev/i2c-1"),
        }
    }
}

fn yes() -> bool {
    true
}

fn default_max_download_mb() -> u64 {
    512
}

impl Config {
    /// Read and validate a config file.
    ///
    /// # Errors
    /// If the file can't be read or parsed, or a `uid-map` entry is not a valid source list.
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let mut config: Self = toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
        let base = path.parent().unwrap_or_else(|| Path::new("."));
        config.data_dir = base.join(&config.data_dir);
        config.library_dir = base.join(&config.library_dir);
        config.uid_sources()?;
        Ok(config)
    }

    /// The `uid-map` as typed source lists.
    ///
    /// # Errors
    /// If an entry has no sources or more than four.
    pub fn uid_sources(&self) -> Result<BTreeMap<Uid, Sources>> {
        self.uid_map
            .iter()
            .map(|(uid, uris)| {
                let sources = Sources::new(uris.iter().map(|uri| Source::parse(uri)).collect())
                    .with_context(|| format!("uid-map entry {uid}"))?;
                Ok((uid.clone(), sources))
            })
            .collect()
    }

    /// Where the persistent state is stored.
    #[must_use]
    pub fn state_file(&self) -> PathBuf {
        self.data_dir.join("state.json")
    }

    /// Saved library hashes, so boots only hash new or changed files.
    #[must_use]
    pub fn library_index_file(&self) -> PathBuf {
        self.data_dir.join("library-index.json")
    }

    /// Where verified downloads are cached.
    #[must_use]
    pub fn cache_dir(&self) -> PathBuf {
        self.data_dir.join("cache")
    }
}

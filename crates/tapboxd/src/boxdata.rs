//! What parents set up in the web app: the box's name and which card plays
//! what. Saved atomically in `<data-dir>/box.json` whenever it changes.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tapbox_card::{Source, Sources};
use tapbox_core::Uid;

use crate::store::write_atomic;

/// A card linked on this box.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Link {
    /// Shown in the web app.
    pub title: String,
    /// What it plays, as card-spec source URIs (usually one `ni:` hash).
    pub sources: Vec<String>,
}

impl Link {
    /// The sources as the core wants them; `None` if the stored list is unusable.
    #[must_use]
    pub fn typed_sources(&self) -> Option<Sources> {
        Sources::new(self.sources.iter().map(|uri| Source::parse(uri)).collect()).ok()
    }
}

/// Everything in `box.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BoxData {
    /// Box name, shown in the web app (and later as the network name).
    pub name: Option<String>,
    /// Card links, by UID.
    pub links: BTreeMap<Uid, Link>,
}

/// Where `box.json` lives. `None` path keeps it in memory (scenarios).
#[derive(Debug, Clone)]
pub struct BoxDataFile {
    path: Option<PathBuf>,
}

impl BoxDataFile {
    /// A file at `path`.
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self { path: Some(path) }
    }

    /// Nothing on disk.
    #[must_use]
    pub fn memory() -> Self {
        Self { path: None }
    }

    /// Load; missing or corrupt files give empty data (logged).
    #[must_use]
    pub fn load(&self) -> BoxData {
        let Some(path) = &self.path else { return BoxData::default() };
        match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|err| {
                tracing::warn!(path = %path.display(), %err, "ignoring corrupt box.json");
                BoxData::default()
            }),
            Err(_) => BoxData::default(),
        }
    }

    /// Save atomically.
    ///
    /// # Errors
    /// On filesystem errors.
    pub fn save(&self, data: &BoxData) -> Result<()> {
        match &self.path {
            Some(path) => write_atomic(path, &serde_json::to_vec_pretty(data)?),
            None => Ok(()),
        }
    }

    /// The path, if on disk.
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let file = BoxDataFile::new(dir.path().join("box.json"));
        assert_eq!(file.load(), BoxData::default());
        let mut data = BoxData { name: Some("Kids' room".into()), ..BoxData::default() };
        data.links.insert(
            Uid::from_hex("04a1b2c3d4e5f6").unwrap(),
            Link { title: "Bedtime".into(), sources: vec!["https://example.org/a.mp3".into()] },
        );
        file.save(&data).unwrap();
        assert_eq!(file.load(), data);
        assert!(data.links.values().next().unwrap().typed_sources().is_some());
    }
}

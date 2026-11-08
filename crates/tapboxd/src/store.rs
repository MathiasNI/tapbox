//! Power-loss-safe storage of [`PersistentState`]: write to a temp file,
//! fsync, rename over the old file, fsync the directory.

use std::fs::{self, File};
use std::io::Write as _;
use std::path::PathBuf;

use anyhow::{Context as _, Result};
use tapbox_core::PersistentState;

/// The state file.
#[derive(Debug, Clone)]
pub struct Store {
    path: PathBuf,
}

impl Store {
    /// A store at `path`. The parent directory is created on first save.
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Load saved state. A missing file is a first boot; a corrupt file is
    /// logged and ignored, so a bad write can never keep the box from starting.
    #[must_use]
    pub fn load(&self) -> PersistentState {
        match fs::read(&self.path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|err| {
                tracing::warn!(path = %self.path.display(), %err, "ignoring corrupt state file");
                PersistentState::default()
            }),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => PersistentState::default(),
            Err(err) => {
                tracing::warn!(path = %self.path.display(), %err, "cannot read state file");
                PersistentState::default()
            }
        }
    }

    /// Save atomically.
    ///
    /// # Errors
    /// On any filesystem error; the previous state file is left intact.
    pub fn save(&self, state: &PersistentState) -> Result<()> {
        let dir = self.path.parent().context("state file has no parent directory")?;
        fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        let tmp = self.path.with_extension("json.tmp");
        let json = serde_json::to_vec_pretty(state)?;
        {
            let mut file = File::create(&tmp).with_context(|| format!("creating {}", tmp.display()))?;
            file.write_all(&json)?;
            file.sync_all()?;
        }
        fs::rename(&tmp, &self.path).with_context(|| format!("replacing {}", self.path.display()))?;
        File::open(dir)?.sync_all()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tapbox_card::Percent;

    #[test]
    fn round_trip_and_corruption() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().join("sub/state.json"));
        assert_eq!(store.load(), PersistentState::default());

        let state = PersistentState { volume: Some(Percent::saturating(33)), ..PersistentState::default() };
        store.save(&state).unwrap();
        assert_eq!(store.load(), state);

        fs::write(dir.path().join("sub/state.json"), b"{not json").unwrap();
        assert_eq!(store.load(), PersistentState::default());
    }
}

//! Power-loss-safe storage of [`PersistentState`] (today: the parents' max
//! volume): write to a temp file, fsync, rename over the old file, fsync the
//! directory. The master switch cuts power without warning, so a write must be
//! safe to interrupt at any instant. Writes are rare: only when a vol-max card
//! changes the limit.
//!
//! A memory-backed store exists for the testbench, so scenarios can cut
//! power and boot again from whatever was last saved.

use std::fs::{self, File};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context as _, Result, anyhow};
use tapbox_core::PersistentState;

/// Where state is saved. Cloning a memory store shares it.
#[derive(Debug, Clone)]
pub struct Store {
    backend: Backend,
}

#[derive(Debug, Clone)]
enum Backend {
    File(PathBuf),
    Memory(Arc<Mutex<Option<PersistentState>>>),
}

impl Store {
    /// A store at `path`. The parent directory is created on first save.
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self { backend: Backend::File(path) }
    }

    /// An in-memory store (testbench).
    #[must_use]
    pub fn memory() -> Self {
        Self { backend: Backend::Memory(Arc::default()) }
    }

    /// Whether this store writes to disk (the daemon and `sim`), as opposed to
    /// memory (scenarios).
    #[must_use]
    pub fn is_on_disk(&self) -> bool {
        matches!(self.backend, Backend::File(_))
    }

    /// Load saved state. A missing file is a first boot; a corrupt file is
    /// logged and ignored, so a bad write can never keep the box from starting.
    #[must_use]
    pub fn load(&self) -> PersistentState {
        match &self.backend {
            Backend::File(path) => load_file(path),
            Backend::Memory(slot) => slot.lock().ok().and_then(|slot| *slot).unwrap_or_default(),
        }
    }

    /// Save atomically.
    ///
    /// # Errors
    /// On any filesystem error; the previous state file is left intact.
    pub fn save(&self, state: PersistentState) -> Result<()> {
        match &self.backend {
            Backend::File(path) => save_file(path, state),
            Backend::Memory(slot) => {
                *slot.lock().map_err(|_| anyhow!("state lock poisoned"))? = Some(state);
                Ok(())
            }
        }
    }
}

fn load_file(path: &Path) -> PersistentState {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|err| {
            tracing::warn!(path = %path.display(), %err, "ignoring corrupt state file");
            PersistentState::default()
        }),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => PersistentState::default(),
        Err(err) => {
            tracing::warn!(path = %path.display(), %err, "cannot read state file");
            PersistentState::default()
        }
    }
}

fn save_file(path: &Path, state: PersistentState) -> Result<()> {
    write_atomic(path, &serde_json::to_vec_pretty(&state)?)
}

/// Write `bytes` to `path` so that, whenever power goes, the file holds either
/// the old or the new content: temp file, fsync, rename, fsync the directory.
///
/// # Errors
/// On any filesystem error; the previous file is left intact.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let dir = path.parent().context("file has no parent directory")?;
    fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let tmp = path.with_extension("tmp");
    {
        let mut file = File::create(&tmp).with_context(|| format!("creating {}", tmp.display()))?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    fs::rename(&tmp, path).with_context(|| format!("replacing {}", path.display()))?;
    File::open(dir)?.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tapbox_card::Percent;

    #[test]
    fn file_round_trip_and_corruption() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().join("sub/state.json"));
        assert_eq!(store.load(), PersistentState::default());

        let state = PersistentState { max_volume: Some(Percent::saturating(33)), ..PersistentState::default() };
        store.save(state).unwrap();
        assert_eq!(store.load(), state);

        fs::write(dir.path().join("sub/state.json"), b"{not json").unwrap();
        assert_eq!(store.load(), PersistentState::default());
    }

    #[test]
    fn a_leftover_temp_file_does_not_matter() {
        // Power cut between writing the temp file and the rename.
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().join("state.json"));
        let state = PersistentState { max_volume: Some(Percent::saturating(20)), ..PersistentState::default() };
        store.save(state).unwrap();
        fs::write(dir.path().join("state.tmp"), b"{half wri").unwrap();
        assert_eq!(store.load(), state);
    }

    #[test]
    fn memory_store_is_shared_between_clones() {
        let store = Store::memory();
        let state = PersistentState { max_volume: Some(Percent::saturating(50)), ..PersistentState::default() };
        store.clone().save(state).unwrap();
        assert_eq!(store.load(), state);
    }
}

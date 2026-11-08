//! Turns a card's sources into something the player can open (spec section 4).
//!
//! Order: content hash in the library or cache → URL (downloaded and verified
//! when the card also carries a hash, streamed otherwise) → plugins. Spotify
//! and manifests are not handled yet and are skipped.

use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File};
use std::io::{BufReader, Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

use anyhow::{Context as _, Result, bail};
use sha2::{Digest as _, Sha256};
use tapbox_card::{ContentHash, Source, Sources};
use tokio::io::AsyncWriteExt as _;

/// Something the player can open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Playable {
    /// A local, hash-verified file.
    File(PathBuf),
    /// A URL to stream (only for cards without a hash).
    Url(String),
}

impl std::fmt::Display for Playable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::File(path) => write!(f, "{}", path.display()),
            Self::Url(url) => f.write_str(url),
        }
    }
}

/// Outcome of a local resolution attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    /// Play this now.
    Ready(Playable),
    /// Download `url`, verify it against `hash`, then play it.
    Download {
        /// Where to fetch it.
        url: String,
        /// What it must hash to.
        hash: ContentHash,
    },
    /// No source can be played on this box.
    Unresolvable,
}

/// Library and cache index.
#[derive(Debug)]
pub struct Resolver {
    files: HashMap<ContentHash, PathBuf>,
    cache_dir: PathBuf,
    allow_http: bool,
}

impl Resolver {
    /// Index `library_dir` by content hash, and `cache_dir` by file name.
    ///
    /// Library files are hashed in chunks (memory use stays flat for any file
    /// size). With `index_file`, hashes are remembered by file name, size and
    /// modification time, so a boot only hashes new or changed files.
    ///
    /// # Errors
    /// If a directory exists but can't be read.
    pub fn new(library_dir: &Path, cache_dir: PathBuf, index_file: Option<&Path>, allow_http: bool) -> Result<Self> {
        let mut files = HashMap::new();
        if library_dir.is_dir() {
            let previous = index_file.map(LibraryIndex::load).unwrap_or_default();
            let mut current = LibraryIndex::default();
            for entry in fs::read_dir(library_dir).with_context(|| format!("reading {}", library_dir.display()))? {
                let path = entry?.path();
                let Some(name) = path.file_name().and_then(|name| name.to_str()).map(str::to_owned) else {
                    continue;
                };
                let metadata = fs::metadata(&path)?;
                if !metadata.is_file() || name.starts_with('.') {
                    continue;
                }
                let stamp = FileStamp::of(&metadata);
                let hash = match previous.files.get(&name) {
                    Some(known) if known.stamp == stamp => known.hash,
                    _ => hash_file(&path).with_context(|| format!("hashing {}", path.display()))?,
                };
                current.files.insert(name, IndexedFile { stamp, hash });
                files.insert(hash, path);
            }
            if let Some(index_file) = index_file
                && current != previous
                && let Err(err) = current.save(index_file)
            {
                tracing::warn!(%err, "cannot save the library index; it will be rebuilt next boot");
            }
        }
        if cache_dir.is_dir() {
            for entry in fs::read_dir(&cache_dir)? {
                let path = entry?.path();
                if let Some(hash) = path.file_name().and_then(|name| name.to_str()).and_then(hash_from_hex) {
                    files.entry(hash).or_insert(path);
                }
            }
        }
        tracing::info!(files = files.len(), library = %library_dir.display(), "library indexed");
        Ok(Self { files, cache_dir, allow_http })
    }

    /// Resolve without touching the network.
    #[must_use]
    pub fn resolve(&self, sources: &Sources) -> Resolution {
        let card_hash = sources.iter().find_map(|source| match source {
            Source::Hash(hash) => Some(*hash),
            _ => None,
        });
        for source in sources {
            match source {
                Source::Hash(hash) => {
                    if let Some(path) = self.files.get(hash) {
                        return Resolution::Ready(Playable::File(path.clone()));
                    }
                }
                Source::Https(url) => return self.url(url, card_hash),
                Source::Http(url) if self.allow_http => return self.url(url, card_hash),
                Source::Http(_) | Source::Spotify(_) | Source::Unsupported(_) => {}
            }
        }
        Resolution::Unresolvable
    }

    fn url(&self, url: &str, hash: Option<ContentHash>) -> Resolution {
        match hash {
            None => Resolution::Ready(Playable::Url(url.to_owned())),
            Some(hash) => match self.files.get(&hash) {
                Some(path) => Resolution::Ready(Playable::File(path.clone())),
                None => Resolution::Download { url: url.to_owned(), hash },
            },
        }
    }

    /// Record a verified download.
    pub fn add(&mut self, hash: ContentHash, path: PathBuf) {
        self.files.insert(hash, path);
    }

    /// Where downloads go.
    #[must_use]
    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }
}

/// Hash a file in 64 KiB chunks.
fn hash_file(path: &Path) -> std::io::Result<ContentHash> {
    let mut reader = BufReader::new(File::open(path)?);
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(ContentHash::from_digest(hasher.finalize().into()))
}

/// What a library file looked like when it was hashed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct FileStamp {
    size: u64,
    modified_secs: u64,
    modified_nanos: u32,
}

impl FileStamp {
    fn of(metadata: &fs::Metadata) -> Self {
        let modified =
            metadata.modified().ok().and_then(|time| time.duration_since(UNIX_EPOCH).ok()).unwrap_or_default();
        Self { size: metadata.len(), modified_secs: modified.as_secs(), modified_nanos: modified.subsec_nanos() }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct IndexedFile {
    #[serde(flatten)]
    stamp: FileStamp,
    #[serde(with = "hex_hash")]
    hash: ContentHash,
}

/// Saved hashes of library files, keyed by file name.
#[derive(Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct LibraryIndex {
    files: BTreeMap<String, IndexedFile>,
}

impl LibraryIndex {
    /// A missing or unreadable index just means everything gets hashed again.
    fn load(path: &Path) -> Self {
        fs::read(path).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
    }

    /// Atomic write, like the state store: power can go at any moment.
    fn save(&self, path: &Path) -> Result<()> {
        let dir = path.parent().context("index file has no parent directory")?;
        fs::create_dir_all(dir)?;
        let tmp = path.with_extension("json.tmp");
        {
            let mut file = File::create(&tmp)?;
            file.write_all(&serde_json::to_vec(self)?)?;
            file.sync_all()?;
        }
        fs::rename(&tmp, path)?;
        File::open(dir)?.sync_all()?;
        Ok(())
    }
}

mod hex_hash {
    use serde::{Deserialize, Deserializer, Serializer, de::Error as _};
    use tapbox_card::ContentHash;

    pub(super) fn serialize<S: Serializer>(hash: &ContentHash, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&hash.to_hex())
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<ContentHash, D::Error> {
        let text = String::deserialize(deserializer)?;
        super::hash_from_hex(&text).ok_or_else(|| D::Error::custom("bad hash"))
    }
}

fn hash_from_hex(name: &str) -> Option<ContentHash> {
    if name.len() != 64 {
        return None;
    }
    let mut digest = [0u8; 32];
    for (index, byte) in digest.iter_mut().enumerate() {
        *byte = u8::from_str_radix(name.get(index * 2..index * 2 + 2)?, 16).ok()?;
    }
    Some(ContentHash::from_digest(digest))
}

/// Download `url` into `cache_dir`, verifying it against `hash`. A mismatch is
/// an error and the file is deleted (spec section 4, rule 2).
///
/// # Errors
/// On network errors, a body larger than `max_bytes`, or a hash mismatch.
pub async fn download(
    client: &reqwest::Client,
    url: &str,
    hash: ContentHash,
    cache_dir: &Path,
    max_bytes: u64,
) -> Result<PathBuf> {
    tokio::fs::create_dir_all(cache_dir).await?;
    let final_path = cache_dir.join(hash.to_hex());
    let tmp_path = cache_dir.join(format!("{}.part", hash.to_hex()));

    let result = async {
        let mut response = client.get(url).send().await?.error_for_status()?;
        let mut file = tokio::fs::File::create(&tmp_path).await?;
        let mut hasher = Sha256::new();
        let mut total: u64 = 0;
        while let Some(chunk) = response.chunk().await? {
            total += chunk.len() as u64;
            if total > max_bytes {
                bail!("download exceeds {max_bytes} bytes");
            }
            hasher.update(&chunk);
            file.write_all(&chunk).await?;
        }
        file.sync_all().await?;
        let actual = ContentHash::from_digest(hasher.finalize().into());
        if actual != hash {
            bail!("hash mismatch: card says {hash}, download is {actual}");
        }
        tokio::fs::rename(&tmp_path, &final_path).await?;
        Ok(final_path.clone())
    }
    .await;

    if result.is_err() {
        let _ = tokio::fs::remove_file(&tmp_path).await;
    }
    result.with_context(|| format!("downloading {url}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolver_with(content: &[u8]) -> (tempfile::TempDir, Resolver, ContentHash) {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("lib")).unwrap();
        fs::write(dir.path().join("lib/a.mp3"), content).unwrap();
        let resolver = Resolver::new(&dir.path().join("lib"), dir.path().join("cache"), None, false).unwrap();
        let hash = ContentHash::from_digest(Sha256::digest(content).into());
        (dir, resolver, hash)
    }

    fn sources(list: &[&str]) -> Sources {
        Sources::new(list.iter().map(|uri| Source::parse(uri)).collect()).unwrap()
    }

    #[test]
    fn hash_in_library_wins() {
        let (_dir, resolver, hash) = resolver_with(b"audio");
        let resolution = resolver.resolve(&sources(&[&hash.to_ni(), "https://x.org/a.mp3"]));
        assert!(matches!(resolution, Resolution::Ready(Playable::File(_))));
    }

    #[test]
    fn missing_hash_with_url_means_verified_download() {
        let (_dir, resolver, _) = resolver_with(b"audio");
        let other = ContentHash::from_digest([1; 32]);
        assert_eq!(
            resolver.resolve(&sources(&[&other.to_ni(), "https://x.org/a.mp3"])),
            Resolution::Download { url: "https://x.org/a.mp3".into(), hash: other }
        );
    }

    #[test]
    fn url_without_hash_streams() {
        let (_dir, resolver, _) = resolver_with(b"audio");
        assert_eq!(
            resolver.resolve(&sources(&["https://x.org/a.mp3"])),
            Resolution::Ready(Playable::Url("https://x.org/a.mp3".into()))
        );
    }

    #[test]
    fn skips_http_when_disabled_and_unknown_schemes() {
        let (_dir, resolver, _) = resolver_with(b"audio");
        assert_eq!(
            resolver.resolve(&sources(&["http://nas/a.mp3", "file:///etc/passwd", "spotify:album:x"])),
            Resolution::Unresolvable
        );
    }

    #[test]
    fn chunked_hash_matches_one_shot_hash() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.bin");
        let content: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        fs::write(&path, &content).unwrap();
        assert_eq!(hash_file(&path).unwrap(), ContentHash::from_digest(Sha256::digest(&content).into()));
    }

    #[test]
    fn index_skips_unchanged_files_and_rehashes_changed_ones() {
        let dir = tempfile::tempdir().unwrap();
        let lib = dir.path().join("lib");
        let index = dir.path().join("state/library-index.json");
        fs::create_dir(&lib).unwrap();
        fs::write(lib.join("a.mp3"), b"one").unwrap();
        let first = Resolver::new(&lib, dir.path().join("cache"), Some(&index), false).unwrap();
        let hash_one = ContentHash::from_digest(Sha256::digest(b"one").into());
        assert!(first.files.contains_key(&hash_one));

        // A wrong hash planted in the index for an unchanged file is trusted:
        // proof that the file was not hashed again.
        let mut saved = LibraryIndex::load(&index);
        let planted = ContentHash::from_digest([9; 32]);
        saved.files.get_mut("a.mp3").unwrap().hash = planted;
        saved.save(&index).unwrap();
        let second = Resolver::new(&lib, dir.path().join("cache"), Some(&index), false).unwrap();
        assert!(second.files.contains_key(&planted));

        // Changing the file (size differs) forces a re-hash.
        fs::write(lib.join("a.mp3"), b"two!").unwrap();
        let third = Resolver::new(&lib, dir.path().join("cache"), Some(&index), false).unwrap();
        assert!(third.files.contains_key(&ContentHash::from_digest(Sha256::digest(b"two!").into())));
        assert!(!third.files.contains_key(&planted));
    }

    #[test]
    fn hex_names() {
        let hash = ContentHash::from_digest([0xAB; 32]);
        assert_eq!(hash_from_hex(&hash.to_hex()), Some(hash));
        assert_eq!(hash_from_hex("abc"), None);
    }
}

//! Turns a card's sources into something the player can open (spec section 4).
//!
//! Order: content hash in the library or cache → URL (downloaded and verified
//! when the card also carries a hash, streamed otherwise) → plugins. Spotify
//! and manifests are not handled yet and are skipped.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

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
    /// Index `library_dir` (by hashing every file) and `cache_dir` (by file name).
    ///
    /// # Errors
    /// If a directory exists but can't be read.
    pub fn new(library_dir: &Path, cache_dir: PathBuf, allow_http: bool) -> Result<Self> {
        let mut files = HashMap::new();
        if library_dir.is_dir() {
            for entry in fs::read_dir(library_dir).with_context(|| format!("reading {}", library_dir.display()))? {
                let path = entry?.path();
                if path.is_file() {
                    let digest = Sha256::digest(fs::read(&path)?);
                    files.insert(ContentHash::from_digest(digest.into()), path);
                }
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
        let resolver = Resolver::new(&dir.path().join("lib"), dir.path().join("cache"), false).unwrap();
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
    fn hex_names() {
        let hash = ContentHash::from_digest([0xAB; 32]);
        assert_eq!(hash_from_hex(&hash.to_hex()), Some(hash));
        assert_eq!(hash_from_hex("abc"), None);
    }
}

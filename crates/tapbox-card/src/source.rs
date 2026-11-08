//! Card sources: where a box can find the audio.

use alloc::borrow::ToOwned as _;
use alloc::format;
use alloc::string::String;
use core::fmt;

use thiserror::Error;

use crate::hash::{ContentHash, HashError, strip_prefix_ignore_case};

/// Longest URL a card may carry.
const MAX_URL_LEN: usize = 512;

/// One entry in a card's source list (spec section 4).
///
/// A box tries the sources in order and plays the first one it can resolve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// Content identified by its SHA-256 hash (`ni:`).
    Hash(ContentHash),
    /// An audio file, playlist, podcast feed or manifest (`https:` / `http:`).
    Web(WebUrl),
    /// A Spotify item, played through the Spotify plugin.
    Spotify(SpotifyUri),
    /// A well-formed URI with a scheme this version doesn't know.
    ///
    /// Kept so cards round-trip unchanged; boxes skip it when resolving.
    Unknown(String),
}

/// Why a source string was rejected.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SourceError {
    /// The string has no URI scheme.
    #[error("source is not a URI")]
    NotAUri,
    /// Local paths are never accepted from a card.
    #[error("file: and local paths are not allowed")]
    ForbiddenScheme,
    /// The `ni:` hash is invalid.
    #[error("invalid content hash: {0}")]
    Hash(#[from] HashError),
    /// The `http(s):` URL is invalid.
    #[error("invalid web URL")]
    InvalidUrl,
    /// The `spotify:` URI is invalid.
    #[error("invalid Spotify URI")]
    InvalidSpotifyUri,
}

impl Source {
    /// Parses and validates one source URI.
    ///
    /// # Errors
    ///
    /// Returns a [`SourceError`] for strings that are not URIs, for `file:`
    /// URIs and for malformed `ni:`, `http(s):` or `spotify:` URIs.
    pub fn parse(uri: &str) -> Result<Self, SourceError> {
        let scheme = scheme_of(uri).ok_or(SourceError::NotAUri)?;
        match scheme.to_ascii_lowercase().as_str() {
            "ni" => Ok(Self::Hash(ContentHash::parse(uri)?)),
            "http" | "https" => Ok(Self::Web(WebUrl::parse(uri)?)),
            "spotify" => Ok(Self::Spotify(SpotifyUri::parse(uri)?)),
            "file" => Err(SourceError::ForbiddenScheme),
            _ => Ok(Self::Unknown(uri.to_owned())),
        }
    }
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Hash(hash) => hash.fmt(f),
            Self::Web(url) => url.fmt(f),
            Self::Spotify(uri) => uri.fmt(f),
            Self::Unknown(uri) => f.write_str(uri),
        }
    }
}

/// Returns the RFC 3986 scheme of `uri`, if it has one.
fn scheme_of(uri: &str) -> Option<&str> {
    let (scheme, _) = uri.split_once(':')?;
    let mut chars = scheme.chars();
    let first_is_alpha = chars.next().is_some_and(|c| c.is_ascii_alphabetic());
    let rest_is_valid = chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    (first_is_alpha && rest_is_valid).then_some(scheme)
}

/// A validated `http://` or `https://` URL.
///
/// Validation is deliberately light (no full URL parser in `no_std`): a known
/// scheme, a non-empty host, no whitespace or control characters and a sane
/// length.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WebUrl(String);

impl WebUrl {
    /// Parses and validates a web URL.
    ///
    /// # Errors
    ///
    /// Returns [`SourceError::InvalidUrl`] when the URL fails validation.
    pub fn parse(url: &str) -> Result<Self, SourceError> {
        let after_scheme = strip_prefix_ignore_case(url, "https://")
            .or_else(|| strip_prefix_ignore_case(url, "http://"))
            .ok_or(SourceError::InvalidUrl)?;
        let host = after_scheme
            .split(['/', '?', '#'])
            .next()
            .unwrap_or_default();
        let is_clean = url.chars().all(|c| !c.is_whitespace() && !c.is_control());
        if host.is_empty() || !is_clean || url.len() > MAX_URL_LEN {
            return Err(SourceError::InvalidUrl);
        }
        Ok(Self(url.to_owned()))
    }

    /// Whether the URL uses TLS.
    #[must_use]
    pub fn is_https(&self) -> bool {
        strip_prefix_ignore_case(&self.0, "https://").is_some()
    }

    /// The URL as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for WebUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The kinds of Spotify item a card can point to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpotifyKind {
    /// A single track.
    Track,
    /// An album.
    Album,
    /// A playlist.
    Playlist,
    /// An artist (plays their top tracks).
    Artist,
    /// A podcast or audiobook show.
    Show,
    /// One podcast episode.
    Episode,
}

impl SpotifyKind {
    const ALL: [Self; 6] = [
        Self::Track,
        Self::Album,
        Self::Playlist,
        Self::Artist,
        Self::Show,
        Self::Episode,
    ];

    /// The kind as it appears in URIs, e.g. `album`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Track => "track",
            Self::Album => "album",
            Self::Playlist => "playlist",
            Self::Artist => "artist",
            Self::Show => "show",
            Self::Episode => "episode",
        }
    }

    fn parse(kind: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.as_str() == kind)
    }
}

/// A validated `spotify:<kind>:<id>` URI.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SpotifyUri {
    kind: SpotifyKind,
    id: String,
}

impl SpotifyUri {
    /// Parses `spotify:<kind>:<id>`, where `id` is 22 base62 characters.
    ///
    /// # Errors
    ///
    /// Returns [`SourceError::InvalidSpotifyUri`] for any other shape.
    pub fn parse(uri: &str) -> Result<Self, SourceError> {
        let rest =
            strip_prefix_ignore_case(uri, "spotify:").ok_or(SourceError::InvalidSpotifyUri)?;
        let (kind, id) = rest.split_once(':').ok_or(SourceError::InvalidSpotifyUri)?;
        Self::from_parts(kind, id)
    }

    /// Converts a share link such as `https://open.spotify.com/album/<id>`
    /// (optionally with an `intl-xx/` segment and a query) into a URI.
    #[must_use]
    pub fn from_open_url(url: &WebUrl) -> Option<Self> {
        let path = strip_prefix_ignore_case(url.as_str(), "https://open.spotify.com/")?;
        let path = path.split(['?', '#']).next().unwrap_or_default();
        let mut segments = path.split('/').filter(|s| !s.is_empty());
        let mut kind = segments.next()?;
        if kind.starts_with("intl-") {
            kind = segments.next()?;
        }
        let id = segments.next()?;
        Self::from_parts(kind, id).ok()
    }

    fn from_parts(kind: &str, id: &str) -> Result<Self, SourceError> {
        let kind = SpotifyKind::parse(kind).ok_or(SourceError::InvalidSpotifyUri)?;
        let id_is_valid = id.len() == 22 && id.chars().all(|c| c.is_ascii_alphanumeric());
        if !id_is_valid {
            return Err(SourceError::InvalidSpotifyUri);
        }
        Ok(Self {
            kind,
            id: id.to_owned(),
        })
    }

    /// What the URI points to.
    #[must_use]
    pub const fn kind(&self) -> SpotifyKind {
        self.kind
    }

    /// The 22-character base62 Spotify ID.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The canonical `spotify:<kind>:<id>` form.
    #[must_use]
    pub fn to_uri(&self) -> String {
        format!("spotify:{}:{}", self.kind.as_str(), self.id)
    }
}

impl fmt::Display for SpotifyUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_uri())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString as _;

    const ALBUM_ID: &str = "4aawyAB9vmqN3uQ7FjRGTy";

    #[test]
    fn parses_each_scheme() {
        assert!(matches!(
            Source::parse("ni:///sha-256;47DEQpj8HBSa-_TImW-5JCeuQeRkm5NMpJWZG3hSuFU"),
            Ok(Source::Hash(_))
        ));
        assert!(matches!(
            Source::parse("https://example.org/a.mp3"),
            Ok(Source::Web(url)) if url.is_https()
        ));
        assert!(matches!(
            Source::parse("http://nas.local/a.mp3"),
            Ok(Source::Web(url)) if !url.is_https()
        ));
        assert!(matches!(
            Source::parse(&format!("spotify:album:{ALBUM_ID}")),
            Ok(Source::Spotify(uri)) if uri.kind() == SpotifyKind::Album
        ));
        assert_eq!(
            Source::parse("tidal:track:1"),
            Ok(Source::Unknown("tidal:track:1".into()))
        );
    }

    #[test]
    fn rejects_local_paths_and_non_uris() {
        assert_eq!(
            Source::parse("file:///etc/passwd"),
            Err(SourceError::ForbiddenScheme)
        );
        assert_eq!(
            Source::parse("FILE:///x"),
            Err(SourceError::ForbiddenScheme)
        );
        assert_eq!(Source::parse("/etc/passwd"), Err(SourceError::NotAUri));
        assert_eq!(Source::parse("1abc:x"), Err(SourceError::NotAUri));
    }

    #[test]
    fn validates_web_urls() {
        assert_eq!(WebUrl::parse("https:///a"), Err(SourceError::InvalidUrl));
        assert_eq!(WebUrl::parse("https://a b"), Err(SourceError::InvalidUrl));
        assert_eq!(WebUrl::parse("ftp://a"), Err(SourceError::InvalidUrl));
        let long = format!("https://a/{}", "x".repeat(MAX_URL_LEN));
        assert_eq!(WebUrl::parse(&long), Err(SourceError::InvalidUrl));
    }

    #[test]
    fn validates_spotify_uris() {
        assert!(SpotifyUri::parse("spotify:album:short").is_err());
        assert!(SpotifyUri::parse(&format!("spotify:song:{ALBUM_ID}")).is_err());
        assert!(SpotifyUri::parse(&format!("spotify:album:{ALBUM_ID}")).is_ok());
    }

    #[test]
    fn converts_open_spotify_links() {
        for link in [
            format!("https://open.spotify.com/album/{ALBUM_ID}"),
            format!("https://open.spotify.com/intl-no/album/{ALBUM_ID}?si=abc"),
        ] {
            let url = WebUrl::parse(&link).unwrap();
            let uri = SpotifyUri::from_open_url(&url).unwrap();
            assert_eq!(uri.to_uri(), format!("spotify:album:{ALBUM_ID}"));
        }
        let other = WebUrl::parse("https://example.org/album/x").unwrap();
        assert_eq!(SpotifyUri::from_open_url(&other), None);
    }

    #[test]
    fn display_round_trips() {
        for uri in [
            "ni:///sha-256;47DEQpj8HBSa-_TImW-5JCeuQeRkm5NMpJWZG3hSuFU",
            "https://example.org/a.mp3",
            "spotify:album:4aawyAB9vmqN3uQ7FjRGTy",
            "tidal:track:1",
        ] {
            assert_eq!(Source::parse(uri).unwrap().to_string(), uri);
        }
    }
}

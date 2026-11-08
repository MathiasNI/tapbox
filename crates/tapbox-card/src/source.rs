use std::fmt;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;

/// A SHA-256 content hash, written on cards as an RFC 6920 `ni:` URI.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ContentHash([u8; 32]);

impl ContentHash {
    const NI_ALGORITHM: &'static str = "sha-256";

    /// Wrap a raw 32-byte SHA-256 digest.
    #[must_use]
    pub const fn from_digest(digest: [u8; 32]) -> Self {
        Self(digest)
    }

    /// The raw digest.
    #[must_use]
    pub const fn digest(&self) -> &[u8; 32] {
        &self.0
    }

    /// Parse `ni:///sha-256;<base64url>` (an authority, as in `ni://host/…`, is allowed and ignored).
    #[must_use]
    pub fn from_ni(uri: &str) -> Option<Self> {
        let rest = strip_prefix_ignore_case(uri, "ni://")?;
        let (_authority, path) = rest.split_once('/')?;
        let path = path.split_once('?').map_or(path, |(path, _query)| path);
        let (algorithm, value) = path.split_once(';')?;
        if !algorithm.eq_ignore_ascii_case(Self::NI_ALGORITHM) {
            return None;
        }
        let bytes = URL_SAFE_NO_PAD.decode(value).ok()?;
        Some(Self(bytes.try_into().ok()?))
    }

    /// The canonical `ni:///sha-256;…` form.
    #[must_use]
    pub fn to_ni(&self) -> String {
        format!("ni:///{};{}", Self::NI_ALGORITHM, URL_SAFE_NO_PAD.encode(self.0))
    }

    /// Lower-case hex, used for file names in the library and cache.
    #[must_use]
    pub fn to_hex(&self) -> String {
        use fmt::Write as _;
        self.0.iter().fold(String::with_capacity(64), |mut out, byte| {
            let _ = write!(out, "{byte:02x}");
            out
        })
    }
}

impl fmt::Debug for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ContentHash({})", self.to_ni())
    }
}

impl fmt::Display for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_ni())
    }
}

/// One entry in a card's source list. Sources are tried in order.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Source {
    /// Content by hash, looked up in the box's library and cache.
    Hash(ContentHash),
    /// Audio file, playlist, podcast feed or manifest over HTTPS.
    Https(String),
    /// As [`Source::Https`], for local servers. A box may disable it.
    Http(String),
    /// A `spotify:` URI, handled by the Spotify plugin.
    Spotify(String),
    /// Anything else, including `file:` and malformed `ni:` URIs. Always skipped.
    Unsupported(String),
}

impl Source {
    /// Classify a source URI from a card. Never fails: unknown schemes become
    /// [`Source::Unsupported`], which the resolver skips (spec section 4).
    #[must_use]
    pub fn parse(uri: &str) -> Self {
        if strip_prefix_ignore_case(uri, "ni:").is_some() {
            return ContentHash::from_ni(uri).map_or_else(|| Self::Unsupported(uri.to_owned()), Self::Hash);
        }
        if strip_prefix_ignore_case(uri, "https://").is_some() {
            Self::Https(uri.to_owned())
        } else if strip_prefix_ignore_case(uri, "http://").is_some() {
            Self::Http(uri.to_owned())
        } else if strip_prefix_ignore_case(uri, "spotify:").is_some() {
            Self::Spotify(uri.to_owned())
        } else {
            Self::Unsupported(uri.to_owned())
        }
    }

    /// The URI as written on the card.
    #[must_use]
    pub fn to_uri(&self) -> String {
        match self {
            Self::Hash(hash) => hash.to_ni(),
            Self::Https(uri) | Self::Http(uri) | Self::Spotify(uri) | Self::Unsupported(uri) => uri.clone(),
        }
    }
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_uri())
    }
}

pub(crate) fn strip_prefix_ignore_case<'a>(value: &'a str, prefix: &str) -> Option<&'a str> {
    let head = value.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix).then(|| &value[prefix.len()..])
}

#[cfg(test)]
mod tests {
    use super::*;

    const EMPTY_FILE_NI: &str = "ni:///sha-256;47DEQpj8HBSa-_TImW-5JCeuQeRkm5NMpJWZG3hSuFU";

    #[test]
    fn ni_round_trip() {
        let hash = ContentHash::from_ni(EMPTY_FILE_NI).unwrap();
        assert_eq!(hash.to_ni(), EMPTY_FILE_NI);
        assert_eq!(hash.to_hex(), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    }

    #[test]
    fn ni_variants() {
        assert!(
            ContentHash::from_ni("NI://example.org/SHA-256;47DEQpj8HBSa-_TImW-5JCeuQeRkm5NMpJWZG3hSuFU?ct=audio/mpeg")
                .is_some()
        );
        assert!(ContentHash::from_ni("ni:///sha-512;47DEQpj8HBSa").is_none());
        assert!(ContentHash::from_ni("ni:///sha-256;tooshort").is_none());
    }

    #[test]
    fn classifies_schemes() {
        assert!(matches!(Source::parse(EMPTY_FILE_NI), Source::Hash(_)));
        assert!(matches!(Source::parse("HTTPS://a.b/c.mp3"), Source::Https(_)));
        assert!(matches!(Source::parse("http://nas.local/x.mp3"), Source::Http(_)));
        assert!(matches!(Source::parse("spotify:album:1"), Source::Spotify(_)));
        assert!(matches!(Source::parse("file:///etc/passwd"), Source::Unsupported(_)));
        assert!(matches!(Source::parse("/etc/passwd"), Source::Unsupported(_)));
        assert!(matches!(Source::parse("ni:///sha-256;bad"), Source::Unsupported(_)));
        assert!(matches!(Source::parse("ñ"), Source::Unsupported(_)));
    }
}

//! Content hashes as RFC 6920 `ni:` URIs.

use alloc::string::String;
use core::fmt;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use thiserror::Error;

const SCHEME: &str = "ni:";
const ALGORITHM: &str = "sha-256";
const DIGEST_LEN: usize = 32;

/// A SHA-256 content hash, written on cards as `ni:///sha-256;<base64url>`.
///
/// The hash identifies audio by its bytes, so a box can find the file in its
/// own library no matter where the file came from.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ContentHash([u8; DIGEST_LEN]);

/// Why a string is not a valid content hash.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum HashError {
    /// The string does not start with `ni:`.
    #[error("not an ni: URI")]
    NotNiUri,
    /// The `ni:` URI has no `/` before the algorithm.
    #[error("malformed ni: URI")]
    Malformed,
    /// Only `sha-256` is supported.
    #[error("unsupported hash algorithm")]
    UnsupportedAlgorithm,
    /// The digest is not valid unpadded base64url.
    #[error("digest is not valid base64url")]
    InvalidBase64,
    /// The digest does not decode to 32 bytes.
    #[error("digest must be 32 bytes, got {0}")]
    WrongLength(usize),
}

impl ContentHash {
    /// Wraps a SHA-256 digest computed elsewhere.
    #[must_use]
    pub const fn from_digest(digest: [u8; DIGEST_LEN]) -> Self {
        Self(digest)
    }

    /// The raw 32-byte digest.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; DIGEST_LEN] {
        &self.0
    }

    /// Parses `ni://[authority]/sha-256;<base64url>[?query]`.
    ///
    /// The authority and query are allowed by RFC 6920 and ignored here.
    ///
    /// # Errors
    ///
    /// Returns a [`HashError`] when the string is not a SHA-256 `ni:` URI.
    pub fn parse(uri: &str) -> Result<Self, HashError> {
        let rest = strip_prefix_ignore_case(uri, SCHEME).ok_or(HashError::NotNiUri)?;
        let rest = rest.strip_prefix("//").ok_or(HashError::Malformed)?;
        let (_authority, path) = rest.split_once('/').ok_or(HashError::Malformed)?;
        let path = path.split_once('?').map_or(path, |(path, _query)| path);
        let (algorithm, digest) = path.split_once(';').ok_or(HashError::Malformed)?;
        if !algorithm.eq_ignore_ascii_case(ALGORITHM) {
            return Err(HashError::UnsupportedAlgorithm);
        }
        let bytes = URL_SAFE_NO_PAD
            .decode(digest)
            .map_err(|_| HashError::InvalidBase64)?;
        let digest: [u8; DIGEST_LEN] = bytes
            .as_slice()
            .try_into()
            .map_err(|_| HashError::WrongLength(bytes.len()))?;
        Ok(Self(digest))
    }

    /// The canonical form, `ni:///sha-256;<base64url>`.
    #[must_use]
    pub fn to_uri(&self) -> String {
        let mut uri = String::from("ni:///sha-256;");
        URL_SAFE_NO_PAD.encode_string(self.0, &mut uri);
        uri
    }
}

impl fmt::Display for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_uri())
    }
}

impl fmt::Debug for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ContentHash({self})")
    }
}

/// Case-insensitive ASCII `strip_prefix`.
pub(crate) fn strip_prefix_ignore_case<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    let head = s.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix)
        .then(|| s.get(prefix.len()..))
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// SHA-256 of the empty input.
    const EMPTY: &str = "ni:///sha-256;47DEQpj8HBSa-_TImW-5JCeuQeRkm5NMpJWZG3hSuFU";

    #[test]
    fn parses_and_formats_canonical_form() {
        let hash = ContentHash::parse(EMPTY).unwrap();
        assert_eq!(hash.as_bytes()[..4], [0xe3, 0xb0, 0xc4, 0x42]);
        assert_eq!(hash.to_uri(), EMPTY);
    }

    #[test]
    fn ignores_authority_query_and_case() {
        let uri =
            "NI://example.org/SHA-256;47DEQpj8HBSa-_TImW-5JCeuQeRkm5NMpJWZG3hSuFU?ct=audio/mpeg";
        assert_eq!(ContentHash::parse(uri).unwrap().to_uri(), EMPTY);
    }

    #[test]
    fn rejects_bad_input() {
        assert_eq!(ContentHash::parse("https://x"), Err(HashError::NotNiUri));
        assert_eq!(
            ContentHash::parse("ni:sha-256;abc"),
            Err(HashError::Malformed)
        );
        assert_eq!(
            ContentHash::parse("ni:///md5;abc"),
            Err(HashError::UnsupportedAlgorithm)
        );
        assert_eq!(
            ContentHash::parse("ni:///sha-256;!!"),
            Err(HashError::InvalidBase64)
        );
        assert_eq!(
            ContentHash::parse("ni:///sha-256;AAAA"),
            Err(HashError::WrongLength(3))
        );
    }
}

//! Codec for the open tapbox NFC card format.
//!
//! A tapbox card is a *pointer*, not storage: it says where the audio can be
//! found (by content hash, by URL, or through a plugin such as Spotify), or it
//! carries a control command for the box. The format is specified in
//! `docs/card-spec.md`; this crate is the reference implementation.
//!
//! The crate does no I/O. It turns the raw bytes read from a tag's user
//! memory into a typed [`Card`] and back:
//!
//! ```
//! use tapbox_card::{decode_tag, encode_tag, Card, ContentHash, Source, Sources, TagContent};
//!
//! let card = Card::content(
//!     Some("Example story"),
//!     Sources::new(vec![Source::parse("https://example.org/story.mp3")])?,
//! )?;
//! let bytes = encode_tag(&card, None)?;
//! assert_eq!(decode_tag(&bytes)?, TagContent::Card(card));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! Everything read from a card is untrusted. Decoding never panics, enforces
//! the size limits from the spec and rejects local paths.

mod card;
mod error;
pub mod ndef;
mod source;
mod tag;
mod uri;

pub use card::{
    Card, CardKind, ControlCommand, MAX_JSON_BYTES, MAX_SOURCES, MAX_TITLE_BYTES, MEDIA_TYPE, Percent,
    SUPPORTED_VERSION, Sources, Title,
};
pub use error::{CardError, DecodeError, EncodeError, NdefError};
pub use source::{ContentHash, Source};
pub use tag::{TagContent, decode_tag, encode_tag, encode_uri_tag};

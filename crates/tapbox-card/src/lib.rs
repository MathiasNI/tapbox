//! Encoder and decoder for the open **tapbox card format**.
//!
//! A tapbox card is an NFC Forum Type 2 tag (reference: NTAG215) holding a
//! standard NDEF message. The message carries one media-type record,
//! [`MEDIA_TYPE`], whose payload is a small JSON object that points to audio:
//! by content hash, by URL or by a plugin URI such as `spotify:`.
//! The full specification lives in `docs/card-spec.md`.
//!
//! The crate is `no_std` (it needs `alloc`), so the same code runs on the
//! Raspberry Pi daemon, in the browser through WebAssembly and on
//! microcontrollers.
//!
//! # Layers
//!
//! | Module | Responsibility |
//! |---|---|
//! | [`tlv`] | Find and wrap the NDEF message inside a tag's data area |
//! | [`ndef`] | Encode and decode NDEF records, including URI records |
//! | [`Card`], [`Source`], [`Command`] | The tapbox payload and its validation |
//! | [`decode_tag`], [`encode_tag`] | Everything above in one call |
//!
//! # Example
//!
//! ```
//! use tapbox_card::{decode_tag, encode_tag, Card, Content, Decoded, Origin, Source};
//!
//! let content = Content::new(
//!     Some("Example story".into()),
//!     vec![Source::parse("https://example.org/story.mp3")?],
//! )?;
//! let card = Card::Content(content);
//!
//! let bytes = encode_tag(&card, None)?;
//! let decoded = decode_tag(&bytes)?;
//! assert_eq!(decoded, Decoded::Card { card, origin: Origin::Tapbox });
//! # Ok::<(), Box<dyn core::error::Error>>(())
//! ```

#![no_std]

extern crate alloc;

mod card;
mod command;
mod hash;
pub mod ndef;
mod source;
mod tag;
pub mod tlv;

pub use card::{Card, CardError, Content};
pub use command::{Command, CommandError};
pub use hash::{ContentHash, HashError};
pub use source::{Source, SourceError, SpotifyKind, SpotifyUri, WebUrl};
pub use tag::{DecodeError, Decoded, EncodeError, Origin, decode_tag, encode_tag};

/// NDEF media type of the tapbox record.
pub const MEDIA_TYPE: &str = "application/vnd.tapbox+json";

/// Highest spec major version (`v`) this crate understands.
pub const SPEC_VERSION: u64 = 1;

/// Largest JSON payload a box accepts. Cards are untrusted input.
pub const MAX_PAYLOAD_LEN: usize = 1024;

/// Longest allowed title, in bytes of UTF-8.
pub const MAX_TITLE_LEN: usize = 64;

/// Most sources a single card may list.
pub const MAX_SOURCES: usize = 4;

/// Usable NDEF data area of an NTAG215, in bytes.
pub const NTAG215_DATA_AREA: usize = 496;

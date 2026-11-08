//! Whole-tag encoding and decoding: TLV + NDEF + card in one call.

use alloc::vec;
use alloc::vec::Vec;

use thiserror::Error;

use crate::MEDIA_TYPE;
use crate::card::{Card, CardError, Content};
use crate::ndef::{self, NdefError, Record, Tnf};
use crate::source::{Source, SpotifyUri, WebUrl};
use crate::tlv::{self, Scan, TooLong};

/// Where a decoded card came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// A tapbox record (spec section 3).
    Tapbox,
    /// A plain URI record written by an ordinary NFC app (spec section 7).
    UriRecord,
}

/// What a tag's data area holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decoded {
    /// A card the box can act on.
    Card {
        /// The card.
        card: Card,
        /// How it was stored.
        origin: Origin,
    },
    /// No NDEF message, or an empty one. Fall back to the UID mapping.
    Empty,
    /// NDEF records, but nothing tapbox understands. Fall back to the UID
    /// mapping.
    Unrecognized,
}

/// Why a tag could not be decoded.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DecodeError {
    /// The NDEF message is malformed.
    #[error(transparent)]
    Ndef(#[from] NdefError),
    /// The tapbox record is invalid.
    #[error(transparent)]
    Card(#[from] CardError),
}

/// Why a card could not be encoded.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EncodeError {
    /// The NDEF message could not be built.
    #[error(transparent)]
    Ndef(#[from] NdefError),
    /// The message is too long for a TLV.
    #[error(transparent)]
    TooLong(#[from] TooLong),
}

/// Decodes a tag's data area (the bytes from NTAG page 4 onwards).
///
/// A tapbox record wins over other records. Without one, the first URI record
/// is used if it holds a web URL; `open.spotify.com` links become `spotify:`
/// sources.
///
/// # Errors
///
/// Returns a [`DecodeError`] for a malformed NDEF message or an invalid
/// tapbox record. An invalid card must not fall back to the UID mapping: the
/// box should show the error state instead.
pub fn decode_tag(area: &[u8]) -> Result<Decoded, DecodeError> {
    let message = match tlv::find_ndef(area) {
        Scan::Found(message) if !message.is_empty() => message,
        Scan::Found(_) | Scan::NotFound | Scan::Incomplete => return Ok(Decoded::Empty),
    };
    let records = ndef::decode_message(message)?;
    if records.iter().all(|record| record.tnf == Tnf::Empty) {
        return Ok(Decoded::Empty);
    }

    if let Some(record) = records.iter().find(|record| record.is_media(MEDIA_TYPE)) {
        let card = Card::from_json(&record.payload)?;
        return Ok(Decoded::Card {
            card,
            origin: Origin::Tapbox,
        });
    }

    for record in &records {
        if let Some(uri) = record.as_uri()? {
            if let Some(source) = source_from_plain_uri(&uri) {
                let content = Content::new(None, vec![source])?;
                return Ok(Decoded::Card {
                    card: Card::Content(content),
                    origin: Origin::UriRecord,
                });
            }
        }
    }
    Ok(Decoded::Unrecognized)
}

/// Encodes a card as a tag data area: optional landing-page URI record,
/// the tapbox record, then the TLV terminator.
///
/// The landing page is what a phone opens when someone taps the card with it.
/// Pad the result with zeros to a page boundary when writing.
///
/// # Errors
///
/// Returns an [`EncodeError`] if the message can't be built.
pub fn encode_tag(card: &Card, landing_page: Option<&WebUrl>) -> Result<Vec<u8>, EncodeError> {
    let mut records = Vec::with_capacity(2);
    if let Some(url) = landing_page {
        records.push(Record::uri(url.as_str()));
    }
    records.push(Record::media(MEDIA_TYPE, card.to_json()));
    let message = ndef::encode_message(&records)?;
    Ok(tlv::wrap_ndef(&message)?)
}

/// A plain URI record is only usable if it's a web URL; open.spotify.com
/// links are converted to `spotify:` sources.
fn source_from_plain_uri(uri: &str) -> Option<Source> {
    let url = WebUrl::parse(uri).ok()?;
    Some(SpotifyUri::from_open_url(&url).map_or(Source::Web(url), Source::Spotify))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Command;

    fn area(records: &[Record]) -> Vec<u8> {
        tlv::wrap_ndef(&ndef::encode_message(records).unwrap()).unwrap()
    }

    #[test]
    fn round_trips_with_landing_page() {
        let card = Card::Control(Command::BluetoothPair);
        let landing = WebUrl::parse("https://example.org/card").unwrap();
        let bytes = encode_tag(&card, Some(&landing)).unwrap();
        assert_eq!(
            decode_tag(&bytes),
            Ok(Decoded::Card {
                card,
                origin: Origin::Tapbox
            })
        );
    }

    #[test]
    fn blank_and_empty_tags() {
        assert_eq!(decode_tag(&[0x00; 16]), Ok(Decoded::Empty));
        assert_eq!(decode_tag(&[0x03, 0x00, 0xFE]), Ok(Decoded::Empty));
        assert_eq!(
            decode_tag(&[0x03, 0x03, 0xD0, 0x00, 0x00, 0xFE]),
            Ok(Decoded::Empty)
        );
    }

    #[test]
    fn falls_back_to_plain_uri_records() {
        let decoded = decode_tag(&area(&[Record::uri("https://example.org/a.mp3")])).unwrap();
        let Decoded::Card {
            card: Card::Content(content),
            origin: Origin::UriRecord,
        } = decoded
        else {
            panic!("expected a URI-record card, got {decoded:?}");
        };
        assert!(matches!(content.sources(), [Source::Web(_)]));

        let spotify = Record::uri("https://open.spotify.com/album/4aawyAB9vmqN3uQ7FjRGTy");
        let decoded = decode_tag(&area(&[spotify])).unwrap();
        assert!(matches!(
            decoded,
            Decoded::Card { card: Card::Content(c), .. } if matches!(c.sources(), [Source::Spotify(_)])
        ));
    }

    #[test]
    fn unrelated_records_are_unrecognized() {
        let vcard = Record::media("text/vcard", b"BEGIN:VCARD".to_vec());
        assert_eq!(decode_tag(&area(&[vcard])), Ok(Decoded::Unrecognized));
        assert_eq!(
            decode_tag(&area(&[Record::uri("tel:+4712345678")])),
            Ok(Decoded::Unrecognized)
        );
    }

    #[test]
    fn invalid_tapbox_record_is_an_error() {
        let bad = Record::media(MEDIA_TYPE, br#"{"v":9,"s":["https://a.org"]}"#.to_vec());
        assert_eq!(
            decode_tag(&area(&[bad])),
            Err(DecodeError::Card(CardError::UnsupportedVersion(9)))
        );
    }
}

use crate::card::{Card, MEDIA_TYPE, Sources};
use crate::error::{DecodeError, EncodeError};
use crate::ndef::{self, Record, Tnf};
use crate::source::Source;
use crate::uri;

/// What a tag's user memory turned out to contain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TagContent {
    /// A tapbox record, or a plain URI record the box can play (spec section 7).
    Card(Card),
    /// No usable NDEF data. The box falls back to its local UID mapping.
    Blank,
}

/// Decode the user memory of an NFC Forum Type 2 tag (bytes from page 4 on).
///
/// Order of preference (spec sections 3 and 7):
/// 1. the first `application/vnd.tapbox+json` record,
/// 2. the first URI record with an `https:`/`http:` URL (Spotify web links become `spotify:` URIs),
/// 3. otherwise [`TagContent::Blank`].
///
/// # Errors
/// [`DecodeError`] if the container is malformed or the tapbox record is invalid.
/// A malformed *URI* record is not an error; it is simply not used.
pub fn decode_tag(user_memory: &[u8]) -> Result<TagContent, DecodeError> {
    let Some(message) = ndef::find_message(user_memory)? else {
        return Ok(TagContent::Blank);
    };
    let records = ndef::parse_message(message)?;

    if let Some(record) = records.iter().find(|record| record.is_media(MEDIA_TYPE)) {
        return Ok(TagContent::Card(Card::from_json(&record.payload)?));
    }

    let playable_uri = records
        .iter()
        .filter(|record| record.tnf == Tnf::WellKnown && record.record_type == b"U")
        .filter_map(|record| uri::decode(&record.payload))
        .find_map(|uri| {
            if let Some(spotify) = uri::spotify_link_to_uri(&uri) {
                return Some(Source::Spotify(spotify));
            }
            match Source::parse(&uri) {
                source @ (Source::Https(_) | Source::Http(_)) => Some(source),
                _ => None,
            }
        });

    match playable_uri {
        Some(source) => Ok(TagContent::Card(Card::content(None, Sources::one(source))?)),
        None => Ok(TagContent::Blank),
    }
}

/// Encode a card as Type 2 tag user memory: an optional URI record first (so a
/// phone tapping the card opens something useful), then the tapbox record.
///
/// # Errors
/// [`EncodeError`] if the JSON payload exceeds the spec limit or the message
/// is too large for a TLV block.
pub fn encode_tag(card: &Card, phone_uri: Option<&str>) -> Result<Vec<u8>, EncodeError> {
    let json = card.to_json();
    if json.len() > crate::MAX_JSON_BYTES {
        return Err(crate::CardError::PayloadTooLarge { len: json.len(), max: crate::MAX_JSON_BYTES }.into());
    }
    let mut records = Vec::with_capacity(2);
    if let Some(uri) = phone_uri {
        records.push(Record {
            tnf: Tnf::WellKnown,
            record_type: b"U".to_vec(),
            id: Vec::new(),
            payload: uri::encode(uri),
        });
    }
    records.push(Record::media(MEDIA_TYPE, json.into_bytes()));
    let message = ndef::encode_message(&records)?;
    Ok(ndef::encode_tlv(&message)?)
}

/// Encode a tag holding only a URI record, as a phone app such as NFC Tools
/// would write it (spec section 7).
///
/// # Errors
/// [`EncodeError`] if the URI is too long for a TLV block.
pub fn encode_uri_tag(uri: &str) -> Result<Vec<u8>, EncodeError> {
    let record = Record { tnf: Tnf::WellKnown, record_type: b"U".to_vec(), id: Vec::new(), payload: uri::encode(uri) };
    Ok(ndef::encode_tlv(&ndef::encode_message(&[record])?)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ContentHash, ControlCommand, Percent};

    fn uri_tag(uri: &str) -> Vec<u8> {
        encode_uri_tag(uri).unwrap()
    }

    #[test]
    fn plain_https_uri_becomes_content_card() {
        let TagContent::Card(card) = decode_tag(&uri_tag("https://example.org/feed.xml")).unwrap() else {
            panic!("expected a card");
        };
        assert_eq!(
            card.kind(),
            &crate::CardKind::Content(Sources::one(Source::Https("https://example.org/feed.xml".into())))
        );
    }

    #[test]
    fn spotify_web_link_becomes_spotify_uri() {
        let TagContent::Card(card) = decode_tag(&uri_tag("https://open.spotify.com/album/abc123")).unwrap() else {
            panic!("expected a card");
        };
        assert_eq!(
            card.kind(),
            &crate::CardKind::Content(Sources::one(Source::Spotify("spotify:album:abc123".into())))
        );
    }

    #[test]
    fn non_playable_uri_is_blank() {
        assert_eq!(decode_tag(&uri_tag("tel:+4712345678")).unwrap(), TagContent::Blank);
        assert_eq!(decode_tag(&uri_tag("file:///etc/passwd")).unwrap(), TagContent::Blank);
    }

    #[test]
    fn tapbox_record_wins_over_leading_uri_record() {
        let card = Card::control(Some("Volume 60"), ControlCommand::MaxVolume(Percent::saturating(60))).unwrap();
        let bytes = encode_tag(&card, Some("https://example.org/about")).unwrap();
        assert_eq!(decode_tag(&bytes).unwrap(), TagContent::Card(card));
    }

    #[test]
    fn hash_card_round_trip() {
        let hash = ContentHash::from_digest([7; 32]);
        let card = Card::content(None, Sources::one(Source::Hash(hash))).unwrap();
        assert_eq!(decode_tag(&encode_tag(&card, None).unwrap()).unwrap(), TagContent::Card(card));
    }
}

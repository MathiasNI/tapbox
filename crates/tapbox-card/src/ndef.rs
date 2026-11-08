//! NFC Forum Type 2 TLV container and NDEF records.
//!
//! Only what tapbox needs: find the NDEF message TLV in a tag's user memory,
//! split it into records, and the reverse. Lock/memory control TLVs and NULL
//! padding are skipped. Chunked records are rejected.

use crate::error::NdefError;

const TLV_NULL: u8 = 0x00;
const TLV_NDEF: u8 = 0x03;
const TLV_TERMINATOR: u8 = 0xFE;
/// A TLV length byte of `0xFF` means a two-byte length follows.
const TLV_LONG_LENGTH: u8 = 0xFF;

const FLAG_MB: u8 = 0x80;
const FLAG_ME: u8 = 0x40;
const FLAG_CF: u8 = 0x20;
const FLAG_SR: u8 = 0x10;
const FLAG_IL: u8 = 0x08;
const TNF_MASK: u8 = 0x07;

/// Upper bound on records per message. Real cards have one or two.
pub const MAX_RECORDS: usize = 16;

/// Type Name Format of an NDEF record (NFC Forum NDEF 1.0, section 3.2.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tnf {
    /// `0x00`: empty record.
    Empty,
    /// `0x01`: NFC Forum well-known type, e.g. `U` for URI.
    WellKnown,
    /// `0x02`: media type (RFC 2046), e.g. `application/vnd.tapbox+json`.
    MediaType,
    /// `0x03`: absolute URI.
    AbsoluteUri,
    /// `0x04`: NFC Forum external type.
    External,
    /// `0x05`: unknown.
    Unknown,
    /// `0x06`: unchanged (chunk continuation).
    Unchanged,
    /// `0x07`: reserved.
    Reserved,
}

impl Tnf {
    fn from_bits(bits: u8) -> Self {
        match bits & TNF_MASK {
            0x00 => Self::Empty,
            0x01 => Self::WellKnown,
            0x02 => Self::MediaType,
            0x03 => Self::AbsoluteUri,
            0x04 => Self::External,
            0x05 => Self::Unknown,
            0x06 => Self::Unchanged,
            _ => Self::Reserved,
        }
    }

    fn bits(self) -> u8 {
        match self {
            Self::Empty => 0x00,
            Self::WellKnown => 0x01,
            Self::MediaType => 0x02,
            Self::AbsoluteUri => 0x03,
            Self::External => 0x04,
            Self::Unknown => 0x05,
            Self::Unchanged => 0x06,
            Self::Reserved => 0x07,
        }
    }
}

/// One NDEF record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    /// How to interpret `record_type`.
    pub tnf: Tnf,
    /// Record type, e.g. `b"U"` or `b"application/vnd.tapbox+json"`.
    pub record_type: Vec<u8>,
    /// Optional record id; empty if absent.
    pub id: Vec<u8>,
    /// Record payload.
    pub payload: Vec<u8>,
}

impl Record {
    /// A media-type record (TNF `0x02`).
    #[must_use]
    pub fn media(media_type: &str, payload: Vec<u8>) -> Self {
        Self { tnf: Tnf::MediaType, record_type: media_type.as_bytes().to_vec(), id: Vec::new(), payload }
    }

    /// Whether this is a media-type record of the given type (case-insensitive, per RFC 2045).
    #[must_use]
    pub fn is_media(&self, media_type: &str) -> bool {
        self.tnf == Tnf::MediaType && self.record_type.eq_ignore_ascii_case(media_type.as_bytes())
    }
}

/// Small bounds-checked reader over a byte slice.
struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn is_empty(&self) -> bool {
        self.pos >= self.data.len()
    }

    fn u8(&mut self) -> Result<u8, NdefError> {
        let byte = *self.data.get(self.pos).ok_or(NdefError::Truncated)?;
        self.pos += 1;
        Ok(byte)
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8], NdefError> {
        let end = self.pos.checked_add(len).ok_or(NdefError::Truncated)?;
        let slice = self.data.get(self.pos..end).ok_or(NdefError::Truncated)?;
        self.pos = end;
        Ok(slice)
    }

    fn u16_be(&mut self) -> Result<u16, NdefError> {
        let bytes = self.take(2)?;
        Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
    }

    fn u32_be(&mut self) -> Result<u32, NdefError> {
        let bytes = self.take(4)?;
        Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }
}

/// Find the first NDEF message TLV in a Type 2 tag's user memory (starting at page 4).
///
/// Returns `Ok(None)` when the tag holds no NDEF message (blank or formatted
/// for something else).
///
/// # Errors
/// [`NdefError::Truncated`] if a TLV length runs past the end of `user_memory`.
pub fn find_message(user_memory: &[u8]) -> Result<Option<&[u8]>, NdefError> {
    let mut cursor = Cursor::new(user_memory);
    while !cursor.is_empty() {
        let tag = cursor.u8()?;
        match tag {
            TLV_NULL => {}
            TLV_TERMINATOR => return Ok(None),
            _ => {
                let short = cursor.u8()?;
                let len = if short == TLV_LONG_LENGTH { usize::from(cursor.u16_be()?) } else { usize::from(short) };
                let value = cursor.take(len)?;
                if tag == TLV_NDEF {
                    return Ok(Some(value));
                }
            }
        }
    }
    Ok(None)
}

/// Split an NDEF message into records.
///
/// # Errors
/// [`NdefError`] if the message is truncated, chunked or has too many records.
pub fn parse_message(message: &[u8]) -> Result<Vec<Record>, NdefError> {
    let mut cursor = Cursor::new(message);
    let mut records = Vec::new();
    while !cursor.is_empty() {
        if records.len() == MAX_RECORDS {
            return Err(NdefError::TooManyRecords { max: MAX_RECORDS });
        }
        let header = cursor.u8()?;
        if header & FLAG_CF != 0 {
            return Err(NdefError::Chunked);
        }
        let type_len = usize::from(cursor.u8()?);
        let payload_len = if header & FLAG_SR != 0 {
            usize::from(cursor.u8()?)
        } else {
            usize::try_from(cursor.u32_be()?).map_err(|_| NdefError::Truncated)?
        };
        let id_len = if header & FLAG_IL != 0 { usize::from(cursor.u8()?) } else { 0 };
        let record_type = cursor.take(type_len)?.to_vec();
        let id = cursor.take(id_len)?.to_vec();
        let payload = cursor.take(payload_len)?.to_vec();
        records.push(Record { tnf: Tnf::from_bits(header), record_type, id, payload });
        if header & FLAG_ME != 0 {
            break;
        }
    }
    Ok(records)
}

/// Encode records as one NDEF message (short records where the payload allows).
///
/// # Errors
/// [`NdefError::TooLarge`] if a type, id or payload exceeds its length field.
pub fn encode_message(records: &[Record]) -> Result<Vec<u8>, NdefError> {
    let mut out = Vec::new();
    let last = records.len().saturating_sub(1);
    for (index, record) in records.iter().enumerate() {
        let too_large = |len: usize| NdefError::TooLarge { len };
        let type_len = u8::try_from(record.record_type.len()).map_err(|_| too_large(record.record_type.len()))?;
        let id_len = u8::try_from(record.id.len()).map_err(|_| too_large(record.id.len()))?;
        let short_payload = u8::try_from(record.payload.len()).ok();

        let mut header = record.tnf.bits();
        if index == 0 {
            header |= FLAG_MB;
        }
        if index == last {
            header |= FLAG_ME;
        }
        if short_payload.is_some() {
            header |= FLAG_SR;
        }
        if id_len > 0 {
            header |= FLAG_IL;
        }

        out.push(header);
        out.push(type_len);
        if let Some(len) = short_payload {
            out.push(len);
        } else {
            let len = u32::try_from(record.payload.len()).map_err(|_| too_large(record.payload.len()))?;
            out.extend_from_slice(&len.to_be_bytes());
        }
        if id_len > 0 {
            out.push(id_len);
        }
        out.extend_from_slice(&record.record_type);
        out.extend_from_slice(&record.id);
        out.extend_from_slice(&record.payload);
    }
    Ok(out)
}

/// Wrap an NDEF message in an NDEF TLV followed by a terminator TLV.
///
/// # Errors
/// [`NdefError::TooLarge`] if the message exceeds the 3-byte TLV length (65 534 bytes).
pub fn encode_tlv(message: &[u8]) -> Result<Vec<u8>, NdefError> {
    let mut out = Vec::with_capacity(message.len() + 5);
    out.push(TLV_NDEF);
    match u8::try_from(message.len()) {
        Ok(len) if len < TLV_LONG_LENGTH => out.push(len),
        _ => {
            let len = u16::try_from(message.len())
                .ok()
                .filter(|len| *len < u16::MAX)
                .ok_or(NdefError::TooLarge { len: message.len() })?;
            out.push(TLV_LONG_LENGTH);
            out.extend_from_slice(&len.to_be_bytes());
        }
    }
    out.extend_from_slice(message);
    out.push(TLV_TERMINATOR);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_null_and_other_tlvs() {
        // NULL, lock control TLV (3 bytes), then NDEF with an empty record.
        let mem = [0x00, 0x01, 0x03, 0xA0, 0x10, 0x44, 0x03, 0x03, 0xD0, 0x00, 0x00, 0xFE];
        assert_eq!(find_message(&mem), Ok(Some(&[0xD0, 0x00, 0x00][..])));
    }

    #[test]
    fn blank_tag_has_no_message() {
        assert_eq!(find_message(&[0x03, 0x00, 0xFE]), Ok(Some(&[][..])));
        assert_eq!(find_message(&[0xFE, 0x03]), Ok(None));
        assert_eq!(find_message(&[0x00; 16]), Ok(None));
        assert_eq!(find_message(&[]), Ok(None));
    }

    #[test]
    fn truncated_tlv_is_an_error() {
        assert_eq!(find_message(&[0x03, 0x10, 0xD0]), Err(NdefError::Truncated));
        assert_eq!(find_message(&[0x03, 0xFF, 0x01]), Err(NdefError::Truncated));
    }

    #[test]
    fn long_tlv_and_long_record_round_trip() {
        let record = Record::media("application/x-test", vec![0xAB; 300]);
        let message = encode_message(std::slice::from_ref(&record)).unwrap();
        let tlv = encode_tlv(&message).unwrap();
        assert_eq!(tlv[1], 0xFF, "3-byte TLV length expected");
        let found = find_message(&tlv).unwrap().unwrap();
        assert_eq!(parse_message(found).unwrap(), vec![record]);
    }

    #[test]
    fn chunked_records_are_rejected() {
        assert_eq!(parse_message(&[0xB2, 0x00, 0x00]), Err(NdefError::Chunked));
    }
}

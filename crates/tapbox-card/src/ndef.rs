//! NFC Forum NDEF records and messages.
//!
//! Only what tapbox needs: short and long records, IDs, media-type and
//! well-known URI records. Chunked records are rejected.

use alloc::string::String;
use alloc::vec::Vec;

use thiserror::Error;

const MB: u8 = 0x80;
const ME: u8 = 0x40;
const CF: u8 = 0x20;
const SR: u8 = 0x10;
const IL: u8 = 0x08;
const TNF_MASK: u8 = 0x07;

/// Well-known record type of URI records.
const URI_TYPE: &[u8] = b"U";

/// URI identifier codes from the NFC Forum URI Record Type Definition.
/// The index is the code.
const URI_PREFIXES: [&str; 36] = [
    "",
    "http://www.",
    "https://www.",
    "http://",
    "https://",
    "tel:",
    "mailto:",
    "ftp://anonymous:anonymous@",
    "ftp://ftp.",
    "ftps://",
    "sftp://",
    "smb://",
    "nfs://",
    "ftp://",
    "dav://",
    "news:",
    "telnet://",
    "imap:",
    "rtsp://",
    "urn:",
    "pop:",
    "sip:",
    "sips:",
    "tftp:",
    "btspp://",
    "btl2cap://",
    "btgoep://",
    "tcpobex://",
    "irdaobex://",
    "file://",
    "urn:epc:id:",
    "urn:epc:tag:",
    "urn:epc:pat:",
    "urn:epc:raw:",
    "urn:epc:",
    "urn:nfc:",
];

/// Type Name Format: how to read a record's type field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Tnf {
    /// No type, no payload.
    Empty = 0,
    /// NFC Forum well-known type, e.g. `U` for URI.
    WellKnown = 1,
    /// RFC 2046 media type, e.g. `application/vnd.tapbox+json`.
    Media = 2,
    /// Absolute URI type.
    AbsoluteUri = 3,
    /// NFC Forum external type.
    External = 4,
    /// Unknown type.
    Unknown = 5,
    /// Continuation of a chunked record.
    Unchanged = 6,
    /// Reserved.
    Reserved = 7,
}

impl Tnf {
    const fn from_bits(bits: u8) -> Self {
        match bits & TNF_MASK {
            0 => Self::Empty,
            1 => Self::WellKnown,
            2 => Self::Media,
            3 => Self::AbsoluteUri,
            4 => Self::External,
            5 => Self::Unknown,
            6 => Self::Unchanged,
            _ => Self::Reserved,
        }
    }
}

/// One NDEF record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    /// How to interpret [`Record::record_type`].
    pub tnf: Tnf,
    /// The record type, at most 255 bytes.
    pub record_type: Vec<u8>,
    /// Optional record ID, at most 255 bytes.
    pub id: Vec<u8>,
    /// The payload.
    pub payload: Vec<u8>,
}

/// Why bytes are not a valid NDEF message, or a message can't be encoded.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum NdefError {
    /// The message ends in the middle of a record.
    #[error("NDEF message is truncated")]
    Truncated,
    /// The first record lacks the Message Begin flag.
    #[error("first record lacks the Message Begin flag")]
    MissingMessageBegin,
    /// Chunked records are not supported.
    #[error("chunked NDEF records are not supported")]
    ChunkedRecord,
    /// A message must have at least one record.
    #[error("an NDEF message needs at least one record")]
    NoRecords,
    /// A type or ID is longer than 255 bytes.
    #[error("record type or ID is longer than 255 bytes")]
    FieldTooLong,
    /// A payload is longer than `u32::MAX` bytes.
    #[error("record payload is too long")]
    PayloadTooLong,
    /// A URI record uses a reserved identifier code.
    #[error("URI record uses reserved prefix code {0:#04x}")]
    ReservedUriPrefix(u8),
    /// A URI record is not valid UTF-8.
    #[error("URI record is not valid UTF-8")]
    InvalidUtf8,
}

impl Record {
    /// A media-type record, e.g. the tapbox record.
    #[must_use]
    pub fn media(media_type: &str, payload: Vec<u8>) -> Self {
        Self {
            tnf: Tnf::Media,
            record_type: media_type.as_bytes().to_vec(),
            id: Vec::new(),
            payload,
        }
    }

    /// A well-known URI record, using the shortest identifier code.
    #[must_use]
    pub fn uri(uri: &str) -> Self {
        let (code, prefix) = URI_PREFIXES
            .iter()
            .enumerate()
            .skip(1)
            .filter(|(_, prefix)| uri.starts_with(*prefix))
            .max_by_key(|(_, prefix)| prefix.len())
            .unwrap_or((0, &""));
        let mut payload = Vec::with_capacity(1 + uri.len() - prefix.len());
        // There are 36 codes, so the index always fits in a byte.
        payload.push(u8::try_from(code).unwrap_or(0));
        payload.extend_from_slice(uri.get(prefix.len()..).unwrap_or(uri).as_bytes());
        Self {
            tnf: Tnf::WellKnown,
            record_type: URI_TYPE.to_vec(),
            id: Vec::new(),
            payload,
        }
    }

    /// Whether this is a media record of `media_type` (case-insensitive, as
    /// media types are).
    #[must_use]
    pub fn is_media(&self, media_type: &str) -> bool {
        self.tnf == Tnf::Media && self.record_type.eq_ignore_ascii_case(media_type.as_bytes())
    }

    /// The full URI, if this is a well-known URI record.
    ///
    /// # Errors
    ///
    /// Returns an [`NdefError`] for a reserved prefix code or invalid UTF-8.
    pub fn as_uri(&self) -> Result<Option<String>, NdefError> {
        if self.tnf != Tnf::WellKnown || self.record_type != URI_TYPE {
            return Ok(None);
        }
        let Some((&code, rest)) = self.payload.split_first() else {
            return Ok(Some(String::new()));
        };
        let prefix = URI_PREFIXES
            .get(usize::from(code))
            .ok_or(NdefError::ReservedUriPrefix(code))?;
        let rest = core::str::from_utf8(rest).map_err(|_| NdefError::InvalidUtf8)?;
        let mut uri = String::with_capacity(prefix.len() + rest.len());
        uri.push_str(prefix);
        uri.push_str(rest);
        Ok(Some(uri))
    }
}

/// Decodes an NDEF message into its records.
///
/// Bytes after the record with the Message End flag are ignored.
///
/// # Errors
///
/// Returns an [`NdefError`] for truncated or malformed messages.
pub fn decode_message(bytes: &[u8]) -> Result<Vec<Record>, NdefError> {
    let mut reader = Reader(bytes);
    let mut records = Vec::new();
    loop {
        let header = reader.byte()?;
        if records.is_empty() && header & MB == 0 {
            return Err(NdefError::MissingMessageBegin);
        }
        if header & CF != 0 {
            return Err(NdefError::ChunkedRecord);
        }
        let type_len = usize::from(reader.byte()?);
        let payload_len = if header & SR != 0 {
            usize::from(reader.byte()?)
        } else {
            let len = u32::from_be_bytes(reader.array()?);
            usize::try_from(len).map_err(|_| NdefError::Truncated)?
        };
        let id_len = if header & IL != 0 {
            usize::from(reader.byte()?)
        } else {
            0
        };
        records.push(Record {
            tnf: Tnf::from_bits(header),
            record_type: reader.take(type_len)?.to_vec(),
            id: reader.take(id_len)?.to_vec(),
            payload: reader.take(payload_len)?.to_vec(),
        });
        if header & ME != 0 {
            return Ok(records);
        }
    }
}

/// Encodes records as one NDEF message, setting the MB, ME, SR and IL flags.
///
/// # Errors
///
/// Returns an [`NdefError`] for an empty list or oversized fields.
pub fn encode_message(records: &[Record]) -> Result<Vec<u8>, NdefError> {
    if records.is_empty() {
        return Err(NdefError::NoRecords);
    }
    let last = records.len() - 1;
    let mut out = Vec::new();
    for (index, record) in records.iter().enumerate() {
        let type_len =
            u8::try_from(record.record_type.len()).map_err(|_| NdefError::FieldTooLong)?;
        let id_len = u8::try_from(record.id.len()).map_err(|_| NdefError::FieldTooLong)?;
        let short_payload = u8::try_from(record.payload.len()).ok();

        let mut header = record.tnf as u8;
        if index == 0 {
            header |= MB;
        }
        if index == last {
            header |= ME;
        }
        if short_payload.is_some() {
            header |= SR;
        }
        if id_len > 0 {
            header |= IL;
        }

        out.push(header);
        out.push(type_len);
        if let Some(len) = short_payload {
            out.push(len);
        } else {
            let len = u32::try_from(record.payload.len()).map_err(|_| NdefError::PayloadTooLong)?;
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

/// A bounds-checked cursor over a byte slice.
struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], NdefError> {
        let (head, tail) = self.0.split_at_checked(n).ok_or(NdefError::Truncated)?;
        self.0 = tail;
        Ok(head)
    }

    fn byte(&mut self) -> Result<u8, NdefError> {
        Ok(self.array::<1>()?[0])
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], NdefError> {
        self.take(N)?.try_into().map_err(|_| NdefError::Truncated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn short_record_round_trips() {
        let record = Record::media("text/plain", b"hi".to_vec());
        let bytes = encode_message(core::slice::from_ref(&record)).unwrap();
        assert_eq!(
            bytes,
            [
                0xD2, 10, 2, b't', b'e', b'x', b't', b'/', b'p', b'l', b'a', b'i', b'n', b'h', b'i'
            ]
        );
        assert_eq!(decode_message(&bytes).unwrap(), vec![record]);
    }

    #[test]
    fn long_record_with_id_round_trips() {
        let mut record = Record::media("a/b", vec![7; 300]);
        record.id = b"x".to_vec();
        let bytes = encode_message(core::slice::from_ref(&record)).unwrap();
        assert_eq!(bytes[..7], [0xCA, 3, 0, 0, 0x01, 0x2C, 1]);
        assert_eq!(decode_message(&bytes).unwrap(), vec![record]);
    }

    #[test]
    fn multi_record_flags() {
        let records = [Record::uri("https://a.org"), Record::media("a/b", vec![1])];
        let bytes = encode_message(&records).unwrap();
        assert_eq!(bytes[0] & (MB | ME), MB);
        assert_eq!(decode_message(&bytes).unwrap(), records);
    }

    #[test]
    fn uri_records_use_the_longest_prefix() {
        let record = Record::uri("https://www.example.org/x");
        assert_eq!(record.payload[0], 0x02);
        assert_eq!(&record.payload[1..], b"example.org/x");
        assert_eq!(
            record.as_uri().unwrap().as_deref(),
            Some("https://www.example.org/x")
        );

        let plain = Record::uri("spotify:album:x");
        assert_eq!(plain.payload[0], 0x00);
        assert_eq!(plain.as_uri().unwrap().as_deref(), Some("spotify:album:x"));
    }

    #[test]
    fn rejects_malformed_messages() {
        assert_eq!(decode_message(&[]), Err(NdefError::Truncated));
        assert_eq!(
            decode_message(&[0x51, 0, 0]),
            Err(NdefError::MissingMessageBegin)
        );
        assert_eq!(
            decode_message(&[0xB2, 1, 1, b'a', 1]),
            Err(NdefError::ChunkedRecord)
        );
        assert_eq!(
            decode_message(&[0xD2, 1, 5, b'a', 1]),
            Err(NdefError::Truncated)
        );
        assert_eq!(
            decode_message(&[0x92, 1, 1, b'a', 1]),
            Err(NdefError::Truncated)
        );
        assert_eq!(encode_message(&[]), Err(NdefError::NoRecords));
    }

    #[test]
    fn rejects_reserved_uri_prefix() {
        let record = Record {
            tnf: Tnf::WellKnown,
            record_type: b"U".to_vec(),
            id: Vec::new(),
            payload: vec![0x99, b'x'],
        };
        assert_eq!(record.as_uri(), Err(NdefError::ReservedUriPrefix(0x99)));
    }
}

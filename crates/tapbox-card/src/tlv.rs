//! NFC Forum Type 2 Tag TLV blocks around the NDEF message.
//!
//! A tag's data area (from page 4 on an NTAG) is a sequence of TLVs:
//! `type, length, value`. The NDEF message lives in the first NDEF TLV
//! (`0x03`); a terminator TLV (`0xFE`) ends the area.

use alloc::vec::Vec;

use thiserror::Error;

/// Padding byte; has no length or value.
pub const NULL_TLV: u8 = 0x00;
/// Lock control TLV.
pub const LOCK_CONTROL_TLV: u8 = 0x01;
/// Memory control TLV.
pub const MEMORY_CONTROL_TLV: u8 = 0x02;
/// The TLV holding the NDEF message.
pub const NDEF_TLV: u8 = 0x03;
/// Proprietary TLV.
pub const PROPRIETARY_TLV: u8 = 0xFD;
/// Marks the end of the data area.
pub const TERMINATOR_TLV: u8 = 0xFE;

/// Largest value a TLV can describe (three-byte length form).
pub const MAX_TLV_LEN: usize = 0xFFFE;

/// Result of looking for the NDEF message in a data area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scan<'a> {
    /// The NDEF TLV was found; this is its value, the NDEF message.
    Found(&'a [u8]),
    /// A terminator came before any NDEF TLV.
    NotFound,
    /// The bytes end before the scan could finish.
    ///
    /// When reading a tag in chunks, read more and scan again. When the whole
    /// data area was given, treat this like [`Scan::NotFound`].
    Incomplete,
}

/// Why a message can't be wrapped in a TLV.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("NDEF message is {0} bytes, the TLV limit is {MAX_TLV_LEN}")]
pub struct TooLong(pub usize);

/// Finds the NDEF message in a tag's data area, skipping other TLVs.
#[must_use]
pub fn find_ndef(area: &[u8]) -> Scan<'_> {
    let mut rest = area;
    loop {
        let Some((&tlv_type, after_type)) = rest.split_first() else {
            return Scan::Incomplete;
        };
        match tlv_type {
            NULL_TLV => {
                rest = after_type;
                continue;
            }
            TERMINATOR_TLV => return Scan::NotFound,
            _ => {}
        }
        let Some((len, after_len)) = read_length(after_type) else {
            return Scan::Incomplete;
        };
        let Some((value, after_value)) = after_len.split_at_checked(len) else {
            return Scan::Incomplete;
        };
        if tlv_type == NDEF_TLV {
            return Scan::Found(value);
        }
        rest = after_value;
    }
}

/// Wraps an NDEF message in an NDEF TLV followed by a terminator.
///
/// # Errors
///
/// Returns [`TooLong`] for messages over [`MAX_TLV_LEN`] bytes.
pub fn wrap_ndef(message: &[u8]) -> Result<Vec<u8>, TooLong> {
    let len = message.len();
    let mut out = Vec::with_capacity(len + 5);
    out.push(NDEF_TLV);
    match u8::try_from(len) {
        Ok(short) if short < 0xFF => out.push(short),
        _ => {
            let long = u16::try_from(len)
                .ok()
                .filter(|&l| usize::from(l) <= MAX_TLV_LEN)
                .ok_or(TooLong(len))?;
            out.push(0xFF);
            out.extend_from_slice(&long.to_be_bytes());
        }
    }
    out.extend_from_slice(message);
    out.push(TERMINATOR_TLV);
    Ok(out)
}

/// Reads a one- or three-byte TLV length.
fn read_length(bytes: &[u8]) -> Option<(usize, &[u8])> {
    let (&first, rest) = bytes.split_first()?;
    if first != 0xFF {
        return Some((usize::from(first), rest));
    }
    let (long, rest) = rest.split_first_chunk::<2>()?;
    Some((usize::from(u16::from_be_bytes(*long)), rest))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn finds_ndef_after_padding_and_other_tlvs() {
        let area = [
            0x00, 0x01, 0x03, 0xA0, 0x0C, 0x34, 0x03, 0x02, 0xAB, 0xCD, 0xFE,
        ];
        assert_eq!(find_ndef(&area), Scan::Found(&[0xAB, 0xCD]));
    }

    #[test]
    fn reports_missing_and_incomplete() {
        assert_eq!(find_ndef(&[0xFE, 0x03, 0x00]), Scan::NotFound);
        assert_eq!(find_ndef(&[0x00; 16]), Scan::Incomplete);
        assert_eq!(find_ndef(&[0x03, 0x05, 0x01]), Scan::Incomplete);
        assert_eq!(find_ndef(&[0x03, 0xFF, 0x01]), Scan::Incomplete);
    }

    #[test]
    fn wraps_short_and_long_messages() {
        assert_eq!(wrap_ndef(&[1, 2]).unwrap(), [0x03, 0x02, 1, 2, 0xFE]);

        let long = vec![7; 300];
        let wrapped = wrap_ndef(&long).unwrap();
        assert_eq!(wrapped[..4], [0x03, 0xFF, 0x01, 0x2C]);
        assert_eq!(find_ndef(&wrapped), Scan::Found(&long[..]));

        // 255 bytes needs the long form: 0xFF in the short form means "long".
        let edge = vec![1; 255];
        assert_eq!(wrap_ndef(&edge).unwrap()[..4], [0x03, 0xFF, 0x00, 0xFF]);

        assert_eq!(wrap_ndef(&vec![0; 0x1_0000]), Err(TooLong(0x1_0000)));
    }
}

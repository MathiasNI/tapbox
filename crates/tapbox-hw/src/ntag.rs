//! Reading and writing NFC Forum Type 2 tags (NTAG213/215/216).
//!
//! Memory is organised in 4-byte pages. Page 3 is the capability container
//! (CC), which says whether the tag is NDEF-formatted, how large the data
//! area is and whether it may be written. The data area starts at page 4.

use alloc::vec::Vec;

use tapbox_card::tlv::{self, Scan};
use tapbox_card::{Card, DecodeError, Decoded, EncodeError, WebUrl, decode_tag, encode_tag};
use thiserror::Error;

/// Bytes per page.
pub const PAGE_SIZE: usize = 4;

/// First page of the NDEF data area.
pub const DATA_AREA_START_PAGE: u8 = 4;

/// Page holding the capability container.
const CC_PAGE: u8 = 3;

/// Bytes returned by one READ command (four pages).
const READ_LEN: usize = 16;

/// CC magic number meaning "NDEF data is present / may be present".
const NDEF_MAGIC: u8 = 0xE1;

/// An empty NDEF TLV followed by a terminator, padded to one page.
///
/// Written to page 4 first, so an interrupted write leaves a valid empty tag
/// instead of a half-written card.
const EMPTY_NDEF_PAGE: [u8; PAGE_SIZE] = [0x03, 0x00, 0xFE, 0x00];

/// Raw access to an NFC Forum Type 2 tag, implemented by reader drivers.
pub trait Type2Tag {
    /// Transport error, e.g. an I²C failure or the tag leaving the field.
    type Error: core::error::Error + 'static;

    /// The READ command: 16 bytes (four pages) starting at `page`.
    ///
    /// # Errors
    ///
    /// Returns the transport error if the command fails.
    fn read_pages(&mut self, page: u8) -> Result<[u8; READ_LEN], Self::Error>;

    /// The WRITE command: four bytes to `page`.
    ///
    /// # Errors
    ///
    /// Returns the transport error if the command fails.
    fn write_page(&mut self, page: u8, data: [u8; PAGE_SIZE]) -> Result<(), Self::Error>;
}

/// Everything that can go wrong reading or writing a card.
#[derive(Debug, Error)]
pub enum NtagError<E: core::error::Error + 'static> {
    /// Talking to the tag failed.
    #[error("tag communication failed")]
    Transport(#[source] E),
    /// The capability container doesn't describe an NDEF tag.
    ///
    /// The CC is one-time programmable, so tapbox never formats tags itself.
    #[error("tag is not NDEF-formatted")]
    NotNdefFormatted,
    /// The CC marks the data area read-only (the tag is locked).
    #[error("tag is read-only")]
    ReadOnly,
    /// The card doesn't fit in the tag's data area.
    #[error("card needs {needed} bytes but the tag holds {capacity}")]
    TooLarge {
        /// Encoded size of the card.
        needed: usize,
        /// Size of the tag's data area.
        capacity: usize,
    },
    /// Reading back after a write gave different bytes.
    #[error("written data did not read back correctly")]
    VerifyFailed,
    /// The card could not be encoded.
    #[error(transparent)]
    Encode(#[from] EncodeError),
    /// The tag holds an invalid card.
    #[error(transparent)]
    Decode(#[from] DecodeError),
}

/// The capability container in page 3.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapabilityContainer {
    /// Mapping version; major version in the high nibble.
    pub version: u8,
    /// Size of the data area in bytes (496 on an NTAG215).
    pub data_area_len: usize,
    /// Whether the data area may be written.
    pub writable: bool,
}

impl CapabilityContainer {
    /// Parses page 3. Returns `None` unless it holds the NDEF magic number
    /// and a supported major version (1).
    #[must_use]
    #[expect(clippy::verbose_bit_mask, reason = "reads like the NFC Forum spec")]
    pub fn parse(page: [u8; PAGE_SIZE]) -> Option<Self> {
        let [magic, version, size, access] = page;
        // High nibble: read access, low nibble: write access; 0 means granted.
        let readable = access >> 4 == 0;
        if magic != NDEF_MAGIC || version >> 4 != 1 || !readable {
            return None;
        }
        Some(Self {
            version,
            data_area_len: usize::from(size) * 8,
            writable: access & 0x0F == 0,
        })
    }
}

/// Reads the capability container and as much of the data area as needed to
/// cover the NDEF message.
///
/// # Errors
///
/// Returns [`NtagError::NotNdefFormatted`] for tags without a valid CC, or a
/// transport error.
pub fn read_data_area<T: Type2Tag>(
    tag: &mut T,
) -> Result<(CapabilityContainer, Vec<u8>), NtagError<T::Error>> {
    let cc = read_capability_container(tag)?;
    let mut area = Vec::with_capacity(cc.data_area_len);
    let mut page = DATA_AREA_START_PAGE;
    while area.len() < cc.data_area_len {
        let chunk = tag.read_pages(page).map_err(NtagError::Transport)?;
        let wanted = (cc.data_area_len - area.len()).min(READ_LEN);
        area.extend_from_slice(chunk.get(..wanted).unwrap_or(&chunk));
        if !matches!(tlv::find_ndef(&area), Scan::Incomplete) {
            break;
        }
        page = page.saturating_add(4);
    }
    Ok((cc, area))
}

/// Reads and decodes the card on a tag.
///
/// Tags that aren't NDEF-formatted decode as [`Decoded::Empty`], so the box
/// can fall back to the UID mapping.
///
/// # Errors
///
/// Returns a transport error, or [`NtagError::Decode`] for an invalid card.
pub fn read_card<T: Type2Tag>(tag: &mut T) -> Result<Decoded, NtagError<T::Error>> {
    match read_data_area(tag) {
        Ok((_, area)) => Ok(decode_tag(&area)?),
        Err(NtagError::NotNdefFormatted) => Ok(Decoded::Empty),
        Err(error) => Err(error),
    }
}

/// Writes a card and verifies it by reading it back.
///
/// The write order keeps the tag valid at every step: page 4 first becomes an
/// empty NDEF message, then the rest of the card is written, and page 4 gets
/// its real content last. If the tag leaves the field halfway, it reads as
/// empty instead of as a corrupt card.
///
/// Tags are never locked; locking is permanent and must be a separate,
/// explicit action.
///
/// # Errors
///
/// Returns [`NtagError::ReadOnly`], [`NtagError::TooLarge`],
/// [`NtagError::VerifyFailed`] or a transport error.
pub fn write_card<T: Type2Tag>(
    tag: &mut T,
    card: &Card,
    landing_page: Option<&WebUrl>,
) -> Result<(), NtagError<T::Error>> {
    let cc = read_capability_container(tag)?;
    if !cc.writable {
        return Err(NtagError::ReadOnly);
    }
    let mut bytes = encode_tag(card, landing_page)?;
    if bytes.len() > cc.data_area_len {
        return Err(NtagError::TooLarge {
            needed: bytes.len(),
            capacity: cc.data_area_len,
        });
    }
    bytes.resize(bytes.len().next_multiple_of(PAGE_SIZE), 0x00);
    let pages: Vec<[u8; PAGE_SIZE]> = bytes
        .chunks_exact(PAGE_SIZE)
        .filter_map(|chunk| chunk.try_into().ok())
        .collect();
    let Some((first, rest)) = pages.split_first() else {
        return Ok(());
    };

    tag.write_page(DATA_AREA_START_PAGE, EMPTY_NDEF_PAGE)
        .map_err(NtagError::Transport)?;
    for (page, data) in (DATA_AREA_START_PAGE + 1..).zip(rest) {
        tag.write_page(page, *data).map_err(NtagError::Transport)?;
    }
    tag.write_page(DATA_AREA_START_PAGE, *first)
        .map_err(NtagError::Transport)?;

    let (_, written) = read_data_area(tag)?;
    if written.get(..bytes.len()) != Some(bytes.as_slice()) {
        return Err(NtagError::VerifyFailed);
    }
    Ok(())
}

fn read_capability_container<T: Type2Tag>(
    tag: &mut T,
) -> Result<CapabilityContainer, NtagError<T::Error>> {
    let first_pages = tag.read_pages(0).map_err(NtagError::Transport)?;
    let cc_offset = usize::from(CC_PAGE) * PAGE_SIZE;
    let cc_page: [u8; PAGE_SIZE] = first_pages
        .get(cc_offset..cc_offset + PAGE_SIZE)
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or(NtagError::NotNdefFormatted)?;
    CapabilityContainer::parse(cc_page).ok_or(NtagError::NotNdefFormatted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{MemoryTag, MemoryTagError};
    use alloc::string::String;
    use alloc::vec;
    use tapbox_card::{Command, Content, Origin, Source};

    fn story() -> Card {
        let source = Source::parse("https://example.org/story.mp3").unwrap();
        Card::Content(Content::new(Some("Story".into()), vec![source]).unwrap())
    }

    #[test]
    fn parses_capability_containers() {
        let ntag215 = CapabilityContainer::parse([0xE1, 0x10, 0x3E, 0x00]).unwrap();
        assert_eq!(ntag215.data_area_len, 496);
        assert!(ntag215.writable);
        assert!(
            !CapabilityContainer::parse([0xE1, 0x10, 0x3E, 0x0F])
                .unwrap()
                .writable
        );
        assert_eq!(CapabilityContainer::parse([0x00, 0x10, 0x3E, 0x00]), None);
        assert_eq!(CapabilityContainer::parse([0xE1, 0x20, 0x3E, 0x00]), None);
    }

    #[test]
    fn writes_and_reads_back() {
        let mut tag = MemoryTag::ntag215();
        assert_eq!(read_card(&mut tag).unwrap(), Decoded::Empty);

        let landing = WebUrl::parse("https://example.org/card").unwrap();
        write_card(&mut tag, &story(), Some(&landing)).unwrap();
        assert_eq!(
            read_card(&mut tag).unwrap(),
            Decoded::Card {
                card: story(),
                origin: Origin::Tapbox
            }
        );

        // Overwriting with a shorter card works too.
        let control = Card::Control(Command::BluetoothPair);
        write_card(&mut tag, &control, None).unwrap();
        assert_eq!(
            read_card(&mut tag).unwrap(),
            Decoded::Card {
                card: control,
                origin: Origin::Tapbox
            }
        );
    }

    #[test]
    fn interrupted_write_leaves_an_empty_tag() {
        let mut tag = MemoryTag::ntag215();
        write_card(&mut tag, &story(), None).unwrap();

        // The new card takes 15 page writes: the empty marker, 13 body pages,
        // then page 4. Interrupt after every possible number of writes.
        let new_card = Card::Control(Command::SayName);
        let total_writes = encode_tag(&new_card, None)
            .unwrap()
            .len()
            .div_ceil(PAGE_SIZE)
            + 1;
        for writes_before_failure in 1..total_writes {
            let mut tag = tag.clone();
            tag.fail_after_writes(writes_before_failure);
            assert!(matches!(
                write_card(&mut tag, &new_card, None),
                Err(NtagError::Transport(MemoryTagError::Interrupted))
            ));
            tag.fail_after_writes(usize::MAX);
            assert_eq!(
                read_card(&mut tag).unwrap(),
                Decoded::Empty,
                "interrupted after {writes_before_failure} writes"
            );
        }
    }

    #[test]
    fn refuses_locked_unformatted_and_small_tags() {
        let mut locked = MemoryTag::ntag215();
        locked.set_page(3, [0xE1, 0x10, 0x3E, 0x0F]);
        assert!(matches!(
            write_card(&mut locked, &story(), None),
            Err(NtagError::ReadOnly)
        ));

        let mut unformatted = MemoryTag::ntag215();
        unformatted.set_page(3, [0; 4]);
        assert!(matches!(
            write_card(&mut unformatted, &story(), None),
            Err(NtagError::NotNdefFormatted)
        ));
        assert_eq!(read_card(&mut unformatted).unwrap(), Decoded::Empty);

        let mut ntag213 = MemoryTag::ntag213();
        let title: String = "x".repeat(64);
        let sources = (0..4)
            .map(|i| {
                Source::parse(&alloc::format!(
                    "https://example.org/{i}/{}",
                    "y".repeat(40)
                ))
            })
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let big = Card::Content(Content::new(Some(title), sources).unwrap());
        assert!(matches!(
            write_card(&mut ntag213, &big, None),
            Err(NtagError::TooLarge { capacity: 144, .. })
        ));
    }

    #[test]
    fn detects_failed_verification() {
        let mut tag = MemoryTag::ntag215();
        tag.corrupt_writes_to(5);
        assert!(matches!(
            write_card(&mut tag, &story(), None),
            Err(NtagError::VerifyFailed)
        ));
    }
}

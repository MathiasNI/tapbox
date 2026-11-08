//! An in-memory NTAG for tests and hardware-free development.

use alloc::vec;
use alloc::vec::Vec;

use thiserror::Error;

use crate::ntag::{PAGE_SIZE, Type2Tag};

/// Errors a [`MemoryTag`] can simulate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum MemoryTagError {
    /// The page doesn't exist on this tag.
    #[error("page {0} is out of range")]
    OutOfRange(u8),
    /// Pages 0–2 hold the UID and lock bytes and are never written by tapbox.
    #[error("page {0} is reserved")]
    ReservedPage(u8),
    /// Simulated loss of the tag mid-operation.
    #[error("tag left the field")]
    Interrupted,
}

/// An `NTAG21x` held in memory, with fault injection for tests.
#[derive(Debug, Clone)]
pub struct MemoryTag {
    pages: Vec<[u8; PAGE_SIZE]>,
    writes_left: usize,
    corrupt_page: Option<u8>,
}

impl MemoryTag {
    /// A factory-fresh NTAG215: 135 pages, 496-byte data area, empty NDEF.
    #[must_use]
    pub fn ntag215() -> Self {
        Self::new(135, 0x3E)
    }

    /// A factory-fresh NTAG213: 45 pages, 144-byte data area, empty NDEF.
    #[must_use]
    pub fn ntag213() -> Self {
        Self::new(45, 0x12)
    }

    fn new(page_count: usize, cc_size: u8) -> Self {
        let mut pages = vec![[0u8; PAGE_SIZE]; page_count];
        if let Some(uid) = pages.first_mut() {
            *uid = [0x04, 0x11, 0x22, 0x33];
        }
        let mut tag = Self {
            pages,
            writes_left: usize::MAX,
            corrupt_page: None,
        };
        tag.set_page(3, [0xE1, 0x10, cc_size, 0x00]);
        tag.set_page(4, [0x03, 0x00, 0xFE, 0x00]);
        tag
    }

    /// Overwrites a page directly, bypassing the write rules.
    pub fn set_page(&mut self, page: u8, data: [u8; PAGE_SIZE]) {
        if let Some(slot) = self.pages.get_mut(usize::from(page)) {
            *slot = data;
        }
    }

    /// Makes every write after the next `writes` fail with
    /// [`MemoryTagError::Interrupted`]. `usize::MAX` disables it.
    pub fn fail_after_writes(&mut self, writes: usize) {
        self.writes_left = writes;
    }

    /// Silently flips bits in whatever is written to `page`.
    pub fn corrupt_writes_to(&mut self, page: u8) {
        self.corrupt_page = Some(page);
    }
}

impl Type2Tag for MemoryTag {
    type Error = MemoryTagError;

    fn read_pages(&mut self, page: u8) -> Result<[u8; 16], Self::Error> {
        let count = self.pages.len();
        let start = usize::from(page);
        if start >= count {
            return Err(MemoryTagError::OutOfRange(page));
        }
        let mut out = [0u8; 16];
        for (offset, chunk) in out.chunks_exact_mut(PAGE_SIZE).enumerate() {
            // Real NTAGs roll over to page 0 past the last page.
            let source = self
                .pages
                .get((start + offset) % count)
                .copied()
                .unwrap_or_default();
            chunk.copy_from_slice(&source);
        }
        Ok(out)
    }

    fn write_page(&mut self, page: u8, mut data: [u8; PAGE_SIZE]) -> Result<(), Self::Error> {
        if page < 3 {
            return Err(MemoryTagError::ReservedPage(page));
        }
        if self.writes_left == 0 {
            return Err(MemoryTagError::Interrupted);
        }
        self.writes_left = self.writes_left.saturating_sub(1);
        if self.corrupt_page == Some(page) {
            for byte in &mut data {
                *byte ^= 0xFF;
            }
        }
        let slot = self
            .pages
            .get_mut(usize::from(page))
            .ok_or(MemoryTagError::OutOfRange(page))?;
        *slot = data;
        Ok(())
    }
}

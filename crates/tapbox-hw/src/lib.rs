//! Hardware layer for tapbox.
//!
//! This crate turns low-level tag access into "read this card" and "write this
//! card safely". It knows nothing about a specific reader chip: a reader
//! driver (PN532 today, others later) only has to implement [`Type2Tag`],
//! the two NFC Forum Type 2 commands READ and WRITE.
//!
//! [`MemoryTag`] is an in-memory NTAG215 for tests and for running the rest of
//! the software on a laptop without hardware.
//!
//! Like `tapbox-card`, the crate is `no_std` with `alloc`.

#![no_std]

extern crate alloc;

mod memory;
mod ntag;

pub use memory::{MemoryTag, MemoryTagError};
pub use ntag::{
    CapabilityContainer, DATA_AREA_START_PAGE, NtagError, PAGE_SIZE, Type2Tag, read_card,
    read_data_area, write_card,
};

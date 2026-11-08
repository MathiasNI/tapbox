//! Card reading. The PN532 driver will live here; today only the decode step
//! exists, shared by the testbench and the future hardware reader.

use tapbox_card::{TagContent, decode_tag};
use tapbox_core::CardRead;

/// Decode a tag's user memory into what the core needs to know.
/// Decode errors are logged; the core only learns that the card is invalid.
#[must_use]
pub fn read_tag(user_memory: &[u8]) -> CardRead {
    match decode_tag(user_memory) {
        Ok(TagContent::Card(card)) => CardRead::Card(card),
        Ok(TagContent::Blank) => CardRead::Blank,
        Err(err) => {
            tracing::warn!(%err, "card could not be decoded");
            CardRead::Invalid
        }
    }
}

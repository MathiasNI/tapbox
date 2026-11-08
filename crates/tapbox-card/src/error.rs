use thiserror::Error;

/// The tag's TLV container or NDEF message is malformed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum NdefError {
    /// A length field points past the end of the data.
    #[error("data ends inside a TLV block or NDEF record")]
    Truncated,
    /// Chunked NDEF records are not supported (no tapbox writer produces them).
    #[error("chunked NDEF records are not supported")]
    Chunked,
    /// The message holds more records than any sensible card would.
    #[error("NDEF message has more than {max} records")]
    TooManyRecords {
        /// The limit that was exceeded.
        max: usize,
    },
    /// A message or record is too large to encode.
    #[error("NDEF message of {len} bytes is too large")]
    TooLarge {
        /// Size of the offending message.
        len: usize,
    },
}

/// The tapbox JSON payload is valid NDEF but not a valid card.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CardError {
    /// The JSON payload exceeds [`crate::MAX_JSON_BYTES`].
    #[error("card payload is {len} bytes, limit is {max}")]
    PayloadTooLarge {
        /// Actual payload size.
        len: usize,
        /// The limit.
        max: usize,
    },
    /// The payload is not valid JSON of the expected shape.
    #[error("card payload is not valid tapbox JSON: {0}")]
    Json(String),
    /// The card was written for a newer spec major version than this box supports.
    #[error("card uses spec version {found}, this box supports up to {supported}")]
    UnsupportedVersion {
        /// Version on the card.
        found: u32,
        /// Highest supported version.
        supported: u32,
    },
    /// Exactly one of `s` (sources) and `c` (command) must be present.
    #[error("card must have exactly one of `s` (sources) and `c` (command)")]
    SourcesXorCommand,
    /// A content card must list 1 to [`crate::MAX_SOURCES`] sources.
    #[error("card lists {count} sources, allowed is 1 to {max}")]
    SourceCount {
        /// Number of sources found.
        count: usize,
        /// The limit.
        max: usize,
    },
    /// The control command is not one this box knows.
    #[error("unknown control command `{0}`")]
    UnknownCommand(String),
    /// The control command's argument is missing or out of range.
    #[error("invalid argument in control command `{0}`")]
    InvalidCommandArgument(String),
    /// A title longer than [`crate::MAX_TITLE_BYTES`] was given to a constructor.
    #[error("title is {len} bytes, limit is {max}")]
    TitleTooLong {
        /// Title size in bytes.
        len: usize,
        /// The limit.
        max: usize,
    },
}

/// Why a tag could not be decoded.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DecodeError {
    /// Container-level problem.
    #[error(transparent)]
    Ndef(#[from] NdefError),
    /// Card-level problem.
    #[error(transparent)]
    Card(#[from] CardError),
}

/// Why a card could not be encoded.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EncodeError {
    /// The encoded message does not fit a TLV block.
    #[error(transparent)]
    Ndef(#[from] NdefError),
    /// The JSON payload would exceed the spec limit.
    #[error(transparent)]
    Card(#[from] CardError),
}

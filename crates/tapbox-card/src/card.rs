//! The tapbox card payload (spec section 3).

use alloc::string::String;
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::command::{Command, CommandError};
use crate::source::{Source, SourceError};
use crate::{MAX_PAYLOAD_LEN, MAX_SOURCES, MAX_TITLE_LEN, SPEC_VERSION};

/// A decoded, validated tapbox card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Card {
    /// Plays something.
    Content(Content),
    /// Controls the box (admin card).
    Control(Command),
}

/// What a content card plays.
///
/// Built only through [`Content::new`], so a value of this type always
/// satisfies the spec's limits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Content {
    title: Option<String>,
    sources: Vec<Source>,
}

/// Why a payload is not a valid card.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CardError {
    /// The payload is larger than [`MAX_PAYLOAD_LEN`].
    #[error("payload is {0} bytes, the limit is {MAX_PAYLOAD_LEN}")]
    PayloadTooLarge(usize),
    /// The payload is not the expected JSON object.
    #[error("payload is not valid card JSON")]
    InvalidJson,
    /// `v` is 0.
    #[error("version 0 is not valid")]
    InvalidVersion,
    /// `v` is newer than this crate supports.
    #[error("card version {0} is newer than supported version {SPEC_VERSION}")]
    UnsupportedVersion(u64),
    /// The card has both `s` and `c`.
    #[error("a card must not have both sources and a command")]
    BothSourcesAndCommand,
    /// The card has neither `s` nor `c`.
    #[error("a card needs sources or a command")]
    NoSourcesOrCommand,
    /// `s` is empty.
    #[error("a content card needs at least one source")]
    NoSources,
    /// `s` has more than [`MAX_SOURCES`] entries.
    #[error("{0} sources, the limit is {MAX_SOURCES}")]
    TooManySources(usize),
    /// The title is empty.
    #[error("title must not be empty")]
    EmptyTitle,
    /// The title is longer than [`MAX_TITLE_LEN`] bytes.
    #[error("title is {0} bytes, the limit is {MAX_TITLE_LEN}")]
    TitleTooLong(usize),
    /// A source failed validation.
    #[error("source {index}: {error}")]
    InvalidSource {
        /// Position in `s`, from 0.
        index: usize,
        /// What was wrong with it.
        error: SourceError,
    },
    /// The command failed validation.
    #[error(transparent)]
    InvalidCommand(#[from] CommandError),
}

impl Content {
    /// Creates validated content.
    ///
    /// # Errors
    ///
    /// Returns a [`CardError`] when the title or the number of sources is
    /// outside the spec's limits.
    pub fn new(title: Option<String>, sources: Vec<Source>) -> Result<Self, CardError> {
        if let Some(title) = &title {
            if title.is_empty() {
                return Err(CardError::EmptyTitle);
            }
            if title.len() > MAX_TITLE_LEN {
                return Err(CardError::TitleTooLong(title.len()));
            }
        }
        match sources.len() {
            0 => Err(CardError::NoSources),
            n if n > MAX_SOURCES => Err(CardError::TooManySources(n)),
            _ => Ok(Self { title, sources }),
        }
    }

    /// The title, if the card has one.
    #[must_use]
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    /// Sources in the order a box should try them.
    #[must_use]
    pub fn sources(&self) -> &[Source] {
        &self.sources
    }
}

/// The JSON shape on the card. Unknown keys are ignored, as the spec requires.
#[derive(Serialize, Deserialize)]
struct Wire {
    v: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    t: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    s: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    c: Option<String>,
}

impl Card {
    /// Decodes and validates a tapbox record payload.
    ///
    /// # Errors
    ///
    /// Returns a [`CardError`] when the payload is too large, isn't valid
    /// JSON, has an unsupported version or breaks a rule of the spec.
    pub fn from_json(payload: &[u8]) -> Result<Self, CardError> {
        if payload.len() > MAX_PAYLOAD_LEN {
            return Err(CardError::PayloadTooLarge(payload.len()));
        }
        let wire: Wire = serde_json::from_slice(payload).map_err(|_| CardError::InvalidJson)?;
        match wire.v {
            0 => return Err(CardError::InvalidVersion),
            v if v > SPEC_VERSION => return Err(CardError::UnsupportedVersion(v)),
            _ => {}
        }
        match (wire.s, wire.c) {
            (Some(_), Some(_)) => Err(CardError::BothSourcesAndCommand),
            (None, None) => Err(CardError::NoSourcesOrCommand),
            (None, Some(command)) => Ok(Self::Control(Command::parse(&command)?)),
            (Some(raw_sources), None) => {
                let sources = raw_sources
                    .iter()
                    .enumerate()
                    .map(|(index, raw)| {
                        Source::parse(raw)
                            .map_err(|error| CardError::InvalidSource { index, error })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Self::Content(Content::new(wire.t, sources)?))
            }
        }
    }

    /// Encodes the card as compact JSON, the tapbox record payload.
    #[must_use]
    pub fn to_json(&self) -> Vec<u8> {
        let wire = match self {
            Self::Content(content) => Wire {
                v: SPEC_VERSION,
                t: content.title.clone(),
                s: Some(
                    content
                        .sources
                        .iter()
                        .map(alloc::string::ToString::to_string)
                        .collect(),
                ),
                c: None,
            },
            Self::Control(command) => Wire {
                v: SPEC_VERSION,
                t: None,
                s: None,
                c: Some(command.to_wire()),
            },
        };
        // Serializing a struct of strings and integers cannot fail.
        serde_json::to_vec(&wire).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn card(json: &str) -> Result<Card, CardError> {
        Card::from_json(json.as_bytes())
    }

    #[test]
    fn decodes_content_card() {
        let decoded = card(r#"{"v":1,"t":"Story","s":["https://example.org/a.mp3"]}"#).unwrap();
        let Card::Content(content) = decoded else {
            panic!("expected content");
        };
        assert_eq!(content.title(), Some("Story"));
        assert_eq!(content.sources().len(), 1);
    }

    #[test]
    fn decodes_control_card() {
        assert_eq!(
            card(r#"{"v":1,"c":"vol-max:60"}"#),
            Ok(Card::Control(Command::VolumeMax(60)))
        );
    }

    #[test]
    fn ignores_unknown_keys() {
        assert!(card(r#"{"v":1,"s":["https://a.org/x"],"cover":"https://a.org/c.jpg"}"#).is_ok());
    }

    #[test]
    fn enforces_spec_rules() {
        let cases = [
            (
                r#"{"v":0,"s":["https://a.org"]}"#,
                CardError::InvalidVersion,
            ),
            (
                r#"{"v":2,"s":["https://a.org"]}"#,
                CardError::UnsupportedVersion(2),
            ),
            (r#"{"v":1}"#, CardError::NoSourcesOrCommand),
            (
                r#"{"v":1,"s":[],"c":"bt-pair"}"#,
                CardError::BothSourcesAndCommand,
            ),
            (r#"{"v":1,"s":[]}"#, CardError::NoSources),
            (
                r#"{"v":1,"t":"","s":["https://a.org"]}"#,
                CardError::EmptyTitle,
            ),
            (r#"{"s":["https://a.org"]}"#, CardError::InvalidJson),
            (
                r#"{"v":1,"v":1,"s":["https://a.org"]}"#,
                CardError::InvalidJson,
            ),
            (
                r#"{"v":1,"s":["https://a.org","file:///x"]}"#,
                CardError::InvalidSource {
                    index: 1,
                    error: SourceError::ForbiddenScheme,
                },
            ),
        ];
        for (json, expected) in cases {
            assert_eq!(card(json), Err(expected), "{json}");
        }
    }

    #[test]
    fn enforces_limits() {
        let five = r#"{"v":1,"s":["https://a.org","https://a.org","https://a.org","https://a.org","https://a.org"]}"#;
        assert_eq!(card(five), Err(CardError::TooManySources(5)));

        let long_title = alloc::format!(
            r#"{{"v":1,"t":"{}","s":["https://a.org"]}}"#,
            "x".repeat(65)
        );
        assert_eq!(card(&long_title), Err(CardError::TitleTooLong(65)));

        let huge = vec![b' '; MAX_PAYLOAD_LEN + 1];
        assert_eq!(
            Card::from_json(&huge),
            Err(CardError::PayloadTooLarge(MAX_PAYLOAD_LEN + 1))
        );
    }

    #[test]
    fn json_round_trips_in_spec_key_order() {
        let json = r#"{"v":1,"t":"Story","s":["https://example.org/a.mp3","spotify:album:4aawyAB9vmqN3uQ7FjRGTy"]}"#;
        assert_eq!(card(json).unwrap().to_json(), json.as_bytes());

        let control = r#"{"v":1,"c":"sleep:30"}"#;
        assert_eq!(card(control).unwrap().to_json(), control.as_bytes());
    }
}

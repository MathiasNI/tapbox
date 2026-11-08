use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::CardError;
use crate::source::Source;

/// NDEF media type of the tapbox record.
pub const MEDIA_TYPE: &str = "application/vnd.tapbox+json";
/// Highest spec major version this crate understands.
pub const SUPPORTED_VERSION: u32 = 1;
/// Largest JSON payload accepted from a card (spec section 9).
pub const MAX_JSON_BYTES: usize = 1024;
/// Largest title, in UTF-8 bytes.
pub const MAX_TITLE_BYTES: usize = 64;
/// Most sources a card may list.
pub const MAX_SOURCES: usize = 4;

/// Longest sleep timer a card may set, in minutes (one day).
const MAX_SLEEP_MINUTES: u16 = 24 * 60;

/// A decoded, validated tapbox card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
    title: Option<Title>,
    kind: CardKind,
}

/// What a card does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CardKind {
    /// Play content from the first source that resolves.
    Content(Sources),
    /// Run a control command on the box.
    Control(ControlCommand),
}

impl Card {
    /// A content card.
    ///
    /// # Errors
    /// [`CardError::TitleTooLong`] if `title` exceeds [`MAX_TITLE_BYTES`].
    pub fn content(title: Option<&str>, sources: Sources) -> Result<Self, CardError> {
        Ok(Self { title: title.map(Title::new).transpose()?, kind: CardKind::Content(sources) })
    }

    /// A control card.
    ///
    /// # Errors
    /// [`CardError::TitleTooLong`] if `title` exceeds [`MAX_TITLE_BYTES`].
    pub fn control(title: Option<&str>, command: ControlCommand) -> Result<Self, CardError> {
        Ok(Self { title: title.map(Title::new).transpose()?, kind: CardKind::Control(command) })
    }

    /// The card's title, if any.
    #[must_use]
    pub fn title(&self) -> Option<&Title> {
        self.title.as_ref()
    }

    /// What the card does.
    #[must_use]
    pub fn kind(&self) -> &CardKind {
        &self.kind
    }

    /// Parse and validate the JSON payload of a tapbox record.
    ///
    /// Unknown keys are ignored (forward compatibility); a title longer than
    /// the limit is truncated rather than rejected.
    ///
    /// # Errors
    /// [`CardError`] if the payload is too large, not valid JSON, from a newer
    /// spec version, or breaks a rule in spec section 3.
    pub fn from_json(payload: &[u8]) -> Result<Self, CardError> {
        if payload.len() > MAX_JSON_BYTES {
            return Err(CardError::PayloadTooLarge { len: payload.len(), max: MAX_JSON_BYTES });
        }
        let wire: WireCard = serde_json::from_slice(payload).map_err(|err| CardError::Json(err.to_string()))?;
        if wire.v > SUPPORTED_VERSION || wire.v == 0 {
            return Err(CardError::UnsupportedVersion { found: wire.v, supported: SUPPORTED_VERSION });
        }
        let kind = match (wire.s, wire.c) {
            (Some(sources), None) => {
                CardKind::Content(Sources::new(sources.iter().map(|uri| Source::parse(uri)).collect())?)
            }
            (None, Some(command)) => CardKind::Control(ControlCommand::parse(&command)?),
            _ => return Err(CardError::SourcesXorCommand),
        };
        Ok(Self { title: wire.t.as_deref().map(Title::truncated), kind })
    }

    /// Compact JSON payload for the tapbox record, keys in spec order.
    #[must_use]
    pub fn to_json(&self) -> String {
        let (s, c) = match &self.kind {
            CardKind::Content(sources) => (Some(sources.iter().map(Source::to_uri).collect()), None),
            CardKind::Control(command) => (None, Some(command.to_string())),
        };
        let wire =
            WireCard { v: SUPPORTED_VERSION, t: self.title.as_ref().map(|title| title.as_str().to_owned()), s, c };
        // Serialising a struct of strings to a String cannot fail.
        serde_json::to_string(&wire).unwrap_or_default()
    }
}

/// The JSON shape on the card. Field order matches the spec's examples.
#[derive(Serialize, Deserialize)]
struct WireCard {
    v: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    t: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    s: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    c: Option<String>,
}

/// A card title of at most [`MAX_TITLE_BYTES`] bytes of UTF-8.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Title(String);

impl Title {
    /// # Errors
    /// [`CardError::TitleTooLong`] if `title` exceeds [`MAX_TITLE_BYTES`].
    pub fn new(title: &str) -> Result<Self, CardError> {
        if title.len() > MAX_TITLE_BYTES {
            return Err(CardError::TitleTooLong { len: title.len(), max: MAX_TITLE_BYTES });
        }
        Ok(Self(title.to_owned()))
    }

    /// Cut `title` to the limit at a character boundary.
    #[must_use]
    pub fn truncated(title: &str) -> Self {
        let mut end = title.len().min(MAX_TITLE_BYTES);
        while !title.is_char_boundary(end) {
            end -= 1;
        }
        Self(title[..end].to_owned())
    }

    /// The title text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Title {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A non-empty list of at most [`MAX_SOURCES`] sources, in priority order.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Sources(Vec<Source>);

impl Sources {
    /// # Errors
    /// [`CardError::SourceCount`] if `sources` is empty or longer than [`MAX_SOURCES`].
    pub fn new(sources: Vec<Source>) -> Result<Self, CardError> {
        if sources.is_empty() || sources.len() > MAX_SOURCES {
            return Err(CardError::SourceCount { count: sources.len(), max: MAX_SOURCES });
        }
        Ok(Self(sources))
    }

    /// A single source.
    #[must_use]
    pub fn one(source: Source) -> Self {
        Self(vec![source])
    }

    /// Iterate in priority order.
    pub fn iter(&self) -> std::slice::Iter<'_, Source> {
        self.0.iter()
    }

    /// The sources as a slice.
    #[must_use]
    pub fn as_slice(&self) -> &[Source] {
        &self.0
    }
}

impl<'a> IntoIterator for &'a Sources {
    type Item = &'a Source;
    type IntoIter = std::slice::Iter<'a, Source>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

/// A percentage, 0 to 100.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct Percent(u8);

impl Percent {
    /// 0 %.
    pub const ZERO: Self = Self(0);
    /// 100 %.
    pub const FULL: Self = Self(100);

    /// `None` if `value` is above 100.
    #[must_use]
    pub const fn new(value: u8) -> Option<Self> {
        if value <= 100 { Some(Self(value)) } else { None }
    }

    /// Clamp `value` into 0..=100.
    #[must_use]
    pub const fn saturating(value: u8) -> Self {
        if value > 100 { Self(100) } else { Self(value) }
    }

    /// The value, 0 to 100.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for Percent {
    type Error = String;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value).ok_or_else(|| format!("{value} is not a percentage (0-100)"))
    }
}

impl From<Percent> for u8 {
    fn from(value: Percent) -> Self {
        value.0
    }
}

impl fmt::Display for Percent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}%", self.0)
    }
}

/// Control card commands (spec section 6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ControlCommand {
    /// `bt-pair`: open Bluetooth pairing for 2 minutes.
    BluetoothPair,
    /// `wifi-setup`: enter Wi-Fi setup (hotspot) mode.
    WifiSetup,
    /// `say-name`: speak the box name and address.
    SayName,
    /// `vol-max:<0-100>`: set the maximum volume.
    MaxVolume(Percent),
    /// `sleep:<minutes>`: stop after N minutes (1 to 1440).
    Sleep {
        /// Minutes until playback stops.
        minutes: u16,
    },
}

impl ControlCommand {
    /// Parse the `c` field of a control card.
    ///
    /// # Errors
    /// [`CardError::UnknownCommand`] or [`CardError::InvalidCommandArgument`].
    pub fn parse(command: &str) -> Result<Self, CardError> {
        let (name, argument) = match command.split_once(':') {
            Some((name, argument)) => (name, Some(argument)),
            None => (command, None),
        };
        let invalid = || CardError::InvalidCommandArgument(command.to_owned());
        match (name, argument) {
            ("bt-pair", None) => Ok(Self::BluetoothPair),
            ("wifi-setup", None) => Ok(Self::WifiSetup),
            ("say-name", None) => Ok(Self::SayName),
            ("vol-max", Some(value)) => {
                value.parse::<u8>().ok().and_then(Percent::new).map(Self::MaxVolume).ok_or_else(invalid)
            }
            ("sleep", Some(value)) => value
                .parse::<u16>()
                .ok()
                .filter(|minutes| (1..=MAX_SLEEP_MINUTES).contains(minutes))
                .map(|minutes| Self::Sleep { minutes })
                .ok_or_else(invalid),
            ("bt-pair" | "wifi-setup" | "say-name" | "vol-max" | "sleep", _) => Err(invalid()),
            _ => Err(CardError::UnknownCommand(command.to_owned())),
        }
    }
}

impl fmt::Display for ControlCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BluetoothPair => f.write_str("bt-pair"),
            Self::WifiSetup => f.write_str("wifi-setup"),
            Self::SayName => f.write_str("say-name"),
            Self::MaxVolume(percent) => write!(f, "vol-max:{}", percent.get()),
            Self::Sleep { minutes } => write!(f, "sleep:{minutes}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_both_or_neither_of_s_and_c() {
        assert_eq!(Card::from_json(br#"{"v":1}"#), Err(CardError::SourcesXorCommand));
        assert_eq!(Card::from_json(br#"{"v":1,"s":["https://a/b"],"c":"bt-pair"}"#), Err(CardError::SourcesXorCommand));
    }

    #[test]
    fn rejects_newer_version() {
        assert_eq!(
            Card::from_json(br#"{"v":2,"s":["https://a/b"]}"#),
            Err(CardError::UnsupportedVersion { found: 2, supported: 1 })
        );
    }

    #[test]
    fn ignores_unknown_keys_and_truncates_title() {
        let long = "å".repeat(40); // 80 bytes
        let json = format!(r#"{{"v":1,"t":"{long}","s":["https://a/b"],"future":{{"x":1}}}}"#);
        let card = Card::from_json(json.as_bytes()).unwrap();
        assert_eq!(card.title().unwrap().as_str().len(), 64);
    }

    #[test]
    fn source_count_limits() {
        assert!(matches!(Card::from_json(br#"{"v":1,"s":[]}"#), Err(CardError::SourceCount { count: 0, .. })));
        assert!(matches!(
            Card::from_json(br#"{"v":1,"s":["a:","b:","c:","d:","e:"]}"#),
            Err(CardError::SourceCount { count: 5, .. })
        ));
    }

    #[test]
    fn rejects_oversized_payload() {
        let json = format!(r#"{{"v":1,"t":"{}","s":["https://a/b"]}}"#, "x".repeat(MAX_JSON_BYTES));
        assert!(matches!(Card::from_json(json.as_bytes()), Err(CardError::PayloadTooLarge { .. })));
    }

    #[test]
    fn control_commands() {
        assert_eq!(ControlCommand::parse("bt-pair"), Ok(ControlCommand::BluetoothPair));
        assert_eq!(ControlCommand::parse("vol-max:60"), Ok(ControlCommand::MaxVolume(Percent::saturating(60))));
        assert_eq!(ControlCommand::parse("sleep:30"), Ok(ControlCommand::Sleep { minutes: 30 }));
        assert!(matches!(ControlCommand::parse("vol-max:101"), Err(CardError::InvalidCommandArgument(_))));
        assert!(matches!(ControlCommand::parse("sleep:0"), Err(CardError::InvalidCommandArgument(_))));
        assert!(matches!(ControlCommand::parse("bt-pair:1"), Err(CardError::InvalidCommandArgument(_))));
        assert!(matches!(ControlCommand::parse("reboot"), Err(CardError::UnknownCommand(_))));
        for command in ["bt-pair", "wifi-setup", "say-name", "vol-max:0", "sleep:1440"] {
            assert_eq!(ControlCommand::parse(command).unwrap().to_string(), command);
        }
    }
}

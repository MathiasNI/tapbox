//! Control commands carried by admin cards (spec section 6).

use alloc::borrow::ToOwned as _;
use alloc::format;
use alloc::string::String;
use core::fmt;

use thiserror::Error;

/// Longest sleep timer a card may set: 12 hours.
const MAX_SLEEP_MINUTES: u16 = 720;

/// What a control card asks the box to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Open Bluetooth pairing for a short window.
    BluetoothPair,
    /// Enter Wi-Fi setup (hotspot) mode.
    WifiSetup,
    /// Speak the box's name and address.
    SayName,
    /// Set the maximum volume, in percent (0–100).
    VolumeMax(u8),
    /// Stop playback after this many minutes (1–720).
    Sleep(u16),
    /// A command from a newer spec version. Boxes ignore it.
    Other(String),
}

/// Why a command string was rejected.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("invalid command argument in {0:?}")]
pub struct CommandError(pub String);

impl Command {
    /// Parses a command such as `bt-pair` or `vol-max:60`.
    ///
    /// Unknown command names parse as [`Command::Other`], so newer cards
    /// don't break older boxes.
    ///
    /// # Errors
    ///
    /// Returns a [`CommandError`] when a known command has a bad argument,
    /// for example `vol-max:150`.
    pub fn parse(command: &str) -> Result<Self, CommandError> {
        let invalid = || CommandError(command.to_owned());
        let (name, argument) = command
            .split_once(':')
            .map_or((command, None), |(name, arg)| (name, Some(arg)));
        match (name, argument) {
            ("bt-pair", None) => Ok(Self::BluetoothPair),
            ("wifi-setup", None) => Ok(Self::WifiSetup),
            ("say-name", None) => Ok(Self::SayName),
            ("vol-max", Some(arg)) => match arg.parse::<u8>() {
                Ok(percent) if percent <= 100 => Ok(Self::VolumeMax(percent)),
                _ => Err(invalid()),
            },
            ("sleep", Some(arg)) => match arg.parse::<u16>() {
                Ok(minutes) if (1..=MAX_SLEEP_MINUTES).contains(&minutes) => {
                    Ok(Self::Sleep(minutes))
                }
                _ => Err(invalid()),
            },
            ("bt-pair" | "wifi-setup" | "say-name" | "vol-max" | "sleep", _) => Err(invalid()),
            _ => Ok(Self::Other(command.to_owned())),
        }
    }

    /// The command as written on a card.
    #[must_use]
    pub fn to_wire(&self) -> String {
        match self {
            Self::BluetoothPair => "bt-pair".into(),
            Self::WifiSetup => "wifi-setup".into(),
            Self::SayName => "say-name".into(),
            Self::VolumeMax(percent) => format!("vol-max:{percent}"),
            Self::Sleep(minutes) => format!("sleep:{minutes}"),
            Self::Other(raw) => raw.clone(),
        }
    }
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_wire())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_commands() {
        assert_eq!(Command::parse("bt-pair"), Ok(Command::BluetoothPair));
        assert_eq!(Command::parse("wifi-setup"), Ok(Command::WifiSetup));
        assert_eq!(Command::parse("say-name"), Ok(Command::SayName));
        assert_eq!(Command::parse("vol-max:60"), Ok(Command::VolumeMax(60)));
        assert_eq!(Command::parse("sleep:30"), Ok(Command::Sleep(30)));
    }

    #[test]
    fn keeps_unknown_commands() {
        assert_eq!(
            Command::parse("party-mode:on"),
            Ok(Command::Other("party-mode:on".into()))
        );
    }

    #[test]
    fn rejects_bad_arguments() {
        for bad in [
            "vol-max:150",
            "vol-max:x",
            "vol-max",
            "sleep:0",
            "sleep:721",
            "bt-pair:1",
        ] {
            assert!(Command::parse(bad).is_err(), "{bad} should be rejected");
        }
    }

    #[test]
    fn wire_form_round_trips() {
        for wire in [
            "bt-pair",
            "wifi-setup",
            "say-name",
            "vol-max:0",
            "sleep:720",
            "x-y",
        ] {
            assert_eq!(Command::parse(wire).unwrap().to_wire(), wire);
        }
    }
}

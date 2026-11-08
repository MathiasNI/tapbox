//! The testbench command language, shared by the interactive console and
//! `.tbx` scenario files. One command per line; `#` starts a comment.
//!
//! ```text
//! tap <card>              place a card on the reader
//! lift                    take it off
//! press|release <button>  button edge; buttons: play, vol-up, vol-down
//! click <button>          press and release (100 ms)
//! hold <button> <time>    press, wait, release
//! wait <time>             let time pass, e.g. 500ms, 3s, 2m
//! bluetooth|airplay start|stop
//! battery low|ok
//! wifi done               Wi-Fi setup finished (connected or timed out)
//! power off               clean shutdown (battery board's low-battery threshold, `poweroff`)
//! power cut               master switch flipped off: power gone, no warning; boots again from saved state
//! player end|fail         end or fail the current track now
//! track-length <time>     length of simulated tracks loaded from now on
//!
//! expect led <state>      idle, playing, airplay, bt-pairing, wifi-setup, battery-low, booting
//! expect flash <kind>     card, error, ack  (during the last action)
//! expect problem <kind>   invalid-card, unknown-card, control-cards-disabled,
//!                         nothing-playable, download-failed, playback-failed  (during the last action)
//! expect cmd <name>       a command ran during the last action, e.g. pause-bluetooth
//! expect playing|paused <card>
//! expect idle
//! expect volume <n>       expect max-volume <n>
//! expect position <card> <time>    saved resume position, ±1 s
//! expect loaded <text>    the player has something loaded whose path/URL contains <text>
//! expect airplay-muted|airplay-unmuted
//! expect not cmd <name>   also: expect not flash <kind>
//! ```

use std::time::Duration;

use anyhow::{Context as _, Result, bail};
use tapbox_core::{Button, Command, ExternalSource, Flash, LedState, Playback};

use super::Bench;
use crate::app::{App, command_name};

/// One parsed line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Place a card.
    Tap(String),
    /// Remove the card.
    Lift,
    /// Button down.
    Press(Button),
    /// Button up.
    Release(Button),
    /// Press and release.
    Click(Button),
    /// Press, wait, release.
    Hold(Button, Duration),
    /// Let time pass.
    Wait(Duration),
    /// Bluetooth/AirPlay started (`true`) or stopped.
    External(ExternalSource, bool),
    /// Battery low (`true`) or ok.
    Battery(bool),
    /// Wi-Fi setup finished.
    WifiDone,
    /// Clean shutdown.
    PowerOff,
    /// Power cut and restored: a fresh boot from saved state.
    PowerCut,
    /// End the current track now.
    PlayerEnd,
    /// Fail the current track now.
    PlayerFail,
    /// Simulated track length for subsequent loads.
    TrackLength(Duration),
    /// Check something. `negated` for `expect not …`.
    Expect {
        /// The check.
        expectation: Expectation,
        /// Whether it must *not* hold.
        negated: bool,
    },
}

/// Something to check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expectation {
    /// LED steady state.
    Led(LedState),
    /// A flash happened during the last action.
    Flash(Flash),
    /// A command ran during the last action.
    Command(String),
    /// A problem of this kind was logged during the last action.
    Problem(String),
    /// This card is playing.
    Playing(String),
    /// This card is loaded and paused.
    Paused(String),
    /// Nothing loaded.
    Idle,
    /// Current volume.
    Volume(u8),
    /// Current max volume.
    MaxVolume(u8),
    /// Saved position of a card.
    Position(String, Duration),
    /// Player has a path/URL containing this text loaded.
    Loaded(String),
    /// AirPlay stream muted or not.
    AirPlayMuted(bool),
}

impl Step {
    /// Whether this step acts (and so starts a new "last action" window).
    #[must_use]
    pub fn is_action(&self) -> bool {
        !matches!(self, Self::Expect { .. })
    }
}

/// Parse one line. `Ok(None)` for blank lines and comments.
///
/// # Errors
/// On an unknown command or bad argument.
pub fn parse_line(line: &str) -> Result<Option<Step>> {
    let line = line.split('#').next().unwrap_or_default().trim();
    let words: Vec<&str> = line.split_whitespace().collect();
    let step = match words.as_slice() {
        [] => return Ok(None),
        ["tap", card] => Step::Tap((*card).to_owned()),
        ["lift"] => Step::Lift,
        ["press", button] => Step::Press(parse_button(button)?),
        ["release", button] => Step::Release(parse_button(button)?),
        ["click", button] => Step::Click(parse_button(button)?),
        ["hold", button, time] => Step::Hold(parse_button(button)?, parse_duration(time)?),
        ["wait", time] => Step::Wait(parse_duration(time)?),
        ["bluetooth" | "bt", state] => Step::External(ExternalSource::Bluetooth, parse_start_stop(state)?),
        ["airplay", state] => Step::External(ExternalSource::AirPlay, parse_start_stop(state)?),
        ["battery", "low"] => Step::Battery(true),
        ["battery", "ok"] => Step::Battery(false),
        ["wifi", "done"] => Step::WifiDone,
        ["power", "off"] => Step::PowerOff,
        ["power", "cut"] => Step::PowerCut,
        ["player", "end"] => Step::PlayerEnd,
        ["player", "fail"] => Step::PlayerFail,
        ["track-length", time] => Step::TrackLength(parse_duration(time)?),
        ["expect", "not", rest @ ..] => Step::Expect { expectation: parse_expectation(rest)?, negated: true },
        ["expect", rest @ ..] => Step::Expect { expectation: parse_expectation(rest)?, negated: false },
        _ => bail!("unknown command `{line}` (type `help`)"),
    };
    Ok(Some(step))
}

fn parse_expectation(words: &[&str]) -> Result<Expectation> {
    Ok(match words {
        ["led", state] => Expectation::Led(parse_led(state)?),
        ["flash", kind] => Expectation::Flash(parse_flash(kind)?),
        ["cmd", name] => Expectation::Command((*name).to_owned()),
        ["problem", kind] => Expectation::Problem((*kind).to_owned()),
        ["playing", card] => Expectation::Playing((*card).to_owned()),
        ["paused", card] => Expectation::Paused((*card).to_owned()),
        ["idle"] => Expectation::Idle,
        ["volume", n] => Expectation::Volume(n.parse().context("volume must be 0-100")?),
        ["max-volume", n] => Expectation::MaxVolume(n.parse().context("volume must be 0-100")?),
        ["position", card, time] => Expectation::Position((*card).to_owned(), parse_duration(time)?),
        ["loaded", text] => Expectation::Loaded((*text).to_owned()),
        ["airplay-muted"] => Expectation::AirPlayMuted(true),
        ["airplay-unmuted"] => Expectation::AirPlayMuted(false),
        _ => bail!("unknown expectation `{}`", words.join(" ")),
    })
}

fn parse_button(word: &str) -> Result<Button> {
    Ok(match word {
        "play" => Button::Play,
        "vol-up" | "volume-up" | "+" => Button::VolumeUp,
        "vol-down" | "volume-down" | "-" => Button::VolumeDown,
        _ => bail!("unknown button `{word}` (play, vol-up, vol-down)"),
    })
}

fn parse_start_stop(word: &str) -> Result<bool> {
    match word {
        "start" => Ok(true),
        "stop" => Ok(false),
        _ => bail!("expected `start` or `stop`, got `{word}`"),
    }
}

fn parse_led(word: &str) -> Result<LedState> {
    Ok(match word {
        "booting" => LedState::Booting,
        "idle" => LedState::Idle,
        "playing" => LedState::Playing,
        "airplay" => LedState::AirPlay,
        "bt-pairing" => LedState::BluetoothPairing,
        "wifi-setup" => LedState::WifiSetup,
        "battery-low" => LedState::BatteryLow,
        "shutting-down" => LedState::ShuttingDown,
        _ => bail!("unknown LED state `{word}`"),
    })
}

fn parse_flash(word: &str) -> Result<Flash> {
    Ok(match word {
        "card" | "card-recognised" | "ok" => Flash::CardRecognised,
        "error" => Flash::Error,
        "ack" | "acknowledge" => Flash::Acknowledge,
        _ => bail!("unknown flash `{word}` (card, error, ack)"),
    })
}

/// Parse `500ms`, `3s`, `2m` or a bare number of seconds.
///
/// # Errors
/// On anything else.
pub fn parse_duration(word: &str) -> Result<Duration> {
    let (number, unit) = word.find(|c: char| !c.is_ascii_digit()).map_or((word, "s"), |at| word.split_at(at));
    let value: u64 = number.parse().with_context(|| format!("bad duration `{word}`"))?;
    Ok(match unit {
        "ms" => Duration::from_millis(value),
        "s" => Duration::from_secs(value),
        "m" | "min" => Duration::from_secs(value * 60),
        _ => bail!("bad duration unit in `{word}` (ms, s, m)"),
    })
}

/// Evaluate an expectation. `last` holds the commands of the last action.
///
/// # Errors
/// With a readable message when the expectation does not hold.
pub fn check(app: &App, bench: &Bench, last: &[Command], expectation: &Expectation, negated: bool) -> Result<()> {
    let (holds, actual) = evaluate(app, bench, last, expectation)?;
    if holds == negated {
        let not = if negated { "not " } else { "" };
        bail!("expected {not}{expectation:?}, but {actual}");
    }
    Ok(())
}

fn evaluate(app: &App, bench: &Bench, last: &[Command], expectation: &Expectation) -> Result<(bool, String)> {
    let core = app.core();
    let names = || last.iter().map(command_name).collect::<Vec<_>>().join(", ");
    Ok(match expectation {
        Expectation::Led(state) => (app.outputs().led() == *state, format!("LED is {:?}", app.outputs().led())),
        Expectation::Flash(flash) => {
            (last.contains(&Command::Flash(*flash)), format!("the last action ran [{}]", names()))
        }
        Expectation::Command(name) => {
            (last.iter().any(|command| command_name(command) == name), format!("the last action ran [{}]", names()))
        }
        Expectation::Problem(kind) => {
            let logged: Vec<&str> = last
                .iter()
                .filter_map(|command| match command {
                    Command::Problem(problem) => Some(problem.kind.name()),
                    _ => None,
                })
                .collect();
            (logged.contains(&kind.as_str()), format!("the last action logged problems [{}]", logged.join(", ")))
        }
        Expectation::Playing(card) => {
            let uid = &bench.card(card)?.uid;
            (core.playback() == Playback::Playing(uid.clone()), format!("playback is {:?}", core.playback()))
        }
        Expectation::Paused(card) => {
            let uid = &bench.card(card)?.uid;
            (core.playback() == Playback::Paused(uid.clone()), format!("playback is {:?}", core.playback()))
        }
        Expectation::Idle => (core.playback() == Playback::Idle, format!("playback is {:?}", core.playback())),
        Expectation::Volume(n) => (core.volume().get() == *n, format!("volume is {}", core.volume())),
        Expectation::MaxVolume(n) => (core.max_volume().get() == *n, format!("max volume is {}", core.max_volume())),
        Expectation::Position(card, want) => {
            let got = core.position(&bench.card(card)?.uid);
            let holds = got.is_some_and(|got| got.abs_diff(*want) <= Duration::from_secs(1));
            (holds, format!("saved position is {got:?}"))
        }
        Expectation::Loaded(text) => {
            let loaded = app.player().loaded().map(ToString::to_string);
            (
                loaded.as_deref().is_some_and(|loaded| loaded.contains(text.as_str())),
                format!("player has {loaded:?} loaded"),
            )
        }
        Expectation::AirPlayMuted(muted) => {
            (app.outputs().airplay_muted() == *muted, format!("AirPlay muted is {}", app.outputs().airplay_muted()))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_commands() {
        assert_eq!(parse_line("  # comment").unwrap(), None);
        assert_eq!(parse_line("tap story # inline").unwrap(), Some(Step::Tap("story".into())));
        assert_eq!(parse_line("hold play 3s").unwrap(), Some(Step::Hold(Button::Play, Duration::from_secs(3))));
        assert_eq!(parse_line("wait 250ms").unwrap(), Some(Step::Wait(Duration::from_millis(250))));
        assert_eq!(
            parse_line("expect not cmd pause").unwrap(),
            Some(Step::Expect { expectation: Expectation::Command("pause".into()), negated: true })
        );
        assert!(parse_line("dance").is_err());
        assert!(parse_line("press nose").is_err());
        assert!(parse_line("wait 3h").is_err());
    }
}

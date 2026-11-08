//! Turns raw button edges and ticks into gestures (`docs/setup-ux.md` section 5).

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use crate::types::{Button, ButtonState, Millis};

/// A press shorter than this is a click.
pub(crate) const CLICK_MAX: Duration = Duration::from_secs(1);
/// Hold play at least this long, then release, to open Bluetooth pairing.
pub(crate) const PAIRING_HOLD: Duration = Duration::from_secs(3);
/// Hold both volume buttons this long to enter Wi-Fi setup. Fires while still held.
pub(crate) const WIFI_SETUP_HOLD: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Gesture {
    PlayClick,
    VolumeUp,
    VolumeDown,
    BluetoothPairing,
    WifiSetup,
}

#[derive(Debug, Default)]
pub(crate) struct ButtonTracker {
    pressed_at: BTreeMap<Button, Millis>,
    /// Buttons whose current press already produced a gesture; their release is ignored.
    consumed: BTreeSet<Button>,
}

impl ButtonTracker {
    pub(crate) fn on_edge(&mut self, now: Millis, button: Button, state: ButtonState) -> Option<Gesture> {
        match state {
            ButtonState::Pressed => self.on_press(now, button),
            ButtonState::Released => self.on_release(now, button),
        }
    }

    fn on_press(&mut self, now: Millis, button: Button) -> Option<Gesture> {
        if self.pressed_at.insert(button, now).is_some() {
            return None; // duplicate edge
        }
        let other_volume = match button {
            Button::VolumeUp => Button::VolumeDown,
            Button::VolumeDown => Button::VolumeUp,
            Button::Play => return None,
        };
        if self.pressed_at.contains_key(&other_volume) {
            // Second half of the Wi-Fi combo: no volume step for this press.
            self.consumed.insert(button);
            return None;
        }
        Some(if button == Button::VolumeUp { Gesture::VolumeUp } else { Gesture::VolumeDown })
    }

    fn on_release(&mut self, now: Millis, button: Button) -> Option<Gesture> {
        let pressed_at = self.pressed_at.remove(&button)?;
        if self.consumed.remove(&button) || button != Button::Play {
            return None;
        }
        let held = now.since(pressed_at);
        if held < CLICK_MAX {
            Some(Gesture::PlayClick)
        } else if held >= PAIRING_HOLD {
            Some(Gesture::BluetoothPairing)
        } else {
            None
        }
    }

    pub(crate) fn on_tick(&mut self, now: Millis) -> Option<Gesture> {
        let down = self.pressed_at.get(&Button::VolumeDown).copied();
        let up = self.pressed_at.get(&Button::VolumeUp).copied();
        if let (Some(down), Some(up)) = (down, up) {
            let both_since = down.max(up);
            let already = self.consumed.contains(&Button::VolumeDown) && self.consumed.contains(&Button::VolumeUp);
            if !already && now.since(both_since) >= WIFI_SETUP_HOLD {
                self.consumed.insert(Button::VolumeDown);
                self.consumed.insert(Button::VolumeUp);
                return Some(Gesture::WifiSetup);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: ButtonState = ButtonState::Pressed;
    const R: ButtonState = ButtonState::Released;

    #[test]
    fn click_and_hold() {
        let mut t = ButtonTracker::default();
        assert_eq!(t.on_edge(Millis(0), Button::Play, P), None);
        assert_eq!(t.on_edge(Millis(200), Button::Play, R), Some(Gesture::PlayClick));

        assert_eq!(t.on_edge(Millis(1000), Button::Play, P), None);
        assert_eq!(t.on_edge(Millis(2500), Button::Play, R), None, "between click and pairing: nothing");

        t.on_edge(Millis(3000), Button::Play, P);
        assert_eq!(t.on_tick(Millis(6500)), None);
        assert_eq!(t.on_edge(Millis(6500), Button::Play, R), Some(Gesture::BluetoothPairing));

        t.on_edge(Millis(10_000), Button::Play, P);
        assert_eq!(t.on_tick(Millis(30_000)), None, "no gesture fires while play is held");
        assert_eq!(t.on_edge(Millis(30_000), Button::Play, R), Some(Gesture::BluetoothPairing));
    }

    #[test]
    fn volume_combo() {
        let mut t = ButtonTracker::default();
        assert_eq!(t.on_edge(Millis(0), Button::VolumeDown, P), Some(Gesture::VolumeDown));
        assert_eq!(t.on_edge(Millis(100), Button::VolumeUp, P), None);
        assert_eq!(t.on_tick(Millis(5000)), None);
        assert_eq!(t.on_tick(Millis(5100)), Some(Gesture::WifiSetup));
        assert_eq!(t.on_tick(Millis(6000)), None);
        assert_eq!(t.on_edge(Millis(7000), Button::VolumeUp, R), None);
        assert_eq!(t.on_edge(Millis(7000), Button::VolumeDown, R), None);
        assert_eq!(t.on_edge(Millis(8000), Button::VolumeUp, P), Some(Gesture::VolumeUp));
    }
}

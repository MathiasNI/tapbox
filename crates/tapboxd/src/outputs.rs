//! Everything the core can ask for besides audio: LED, Bluetooth, AirPlay,
//! Wi-Fi setup, speech, shutdown. Simulated for now; hardware and system
//! integrations replace these one at a time.

use std::time::Duration;

use tapbox_core::{Flash, LedState};

/// Simulated LED and system integrations. Remembers what it was told so the
/// testbench can check it.
#[derive(Debug)]
pub struct SimOutputs {
    led: LedState,
    last_flash: Option<Flash>,
    bluetooth_pauses: u32,
    name_announcements: u32,
    airplay_muted: bool,
    pairing_open_for: Option<Duration>,
    wifi_setup: bool,
    shutdown_requested: bool,
}

impl Default for SimOutputs {
    fn default() -> Self {
        Self {
            led: LedState::Booting,
            last_flash: None,
            bluetooth_pauses: 0,
            name_announcements: 0,
            airplay_muted: false,
            pairing_open_for: None,
            wifi_setup: false,
            shutdown_requested: false,
        }
    }
}

impl SimOutputs {
    /// Current LED steady state.
    #[must_use]
    pub fn led(&self) -> LedState {
        self.led
    }

    /// The most recent flash.
    #[must_use]
    pub fn last_flash(&self) -> Option<Flash> {
        self.last_flash
    }

    /// How many times the Bluetooth sender was asked to pause.
    #[must_use]
    pub fn bluetooth_pauses(&self) -> u32 {
        self.bluetooth_pauses
    }

    /// How many times the box would have spoken its name.
    #[must_use]
    pub fn name_announcements(&self) -> u32 {
        self.name_announcements
    }

    /// Whether the AirPlay stream is muted.
    #[must_use]
    pub fn airplay_muted(&self) -> bool {
        self.airplay_muted
    }

    /// Whether a shutdown was requested.
    #[must_use]
    pub fn shutdown_requested(&self) -> bool {
        self.shutdown_requested
    }

    pub(crate) fn set_led(&mut self, state: LedState) {
        self.led = state;
    }

    pub(crate) fn flash(&mut self, flash: Flash) {
        self.last_flash = Some(flash);
    }

    pub(crate) fn pause_bluetooth(&mut self) {
        self.bluetooth_pauses += 1;
    }

    pub(crate) fn mute_airplay(&mut self, muted: bool) {
        self.airplay_muted = muted;
    }

    pub(crate) fn open_bluetooth_pairing(&mut self, window: Duration) {
        self.pairing_open_for = Some(window);
    }

    pub(crate) fn enter_wifi_setup(&mut self) {
        self.wifi_setup = true;
    }

    pub(crate) fn say_name(&mut self) {
        self.name_announcements += 1;
    }

    pub(crate) fn shutdown(&mut self) {
        self.shutdown_requested = true;
    }
}

/// Human-readable LED description, matching the table in `docs/setup-ux.md`.
#[must_use]
pub fn describe_led(state: LedState) -> &'static str {
    match state {
        LedState::Booting => "white pulse (booting)",
        LedState::Idle => "dim green (idle)",
        LedState::Playing => "green (playing)",
        LedState::AirPlay => "purple (AirPlay)",
        LedState::BluetoothPairing => "blue blinking (Bluetooth pairing)",
        LedState::WifiSetup => "orange blinking (Wi-Fi setup)",
        LedState::BatteryLow => "red slow blink (battery low)",
    }
}

/// Human-readable flash description.
#[must_use]
pub fn describe_flash(flash: Flash) -> &'static str {
    match flash {
        Flash::CardRecognised => "green flash (card recognised)",
        Flash::Error => "red flash (error)",
        Flash::Acknowledge => "white flash (acknowledged)",
    }
}

//! Everything the core can ask for besides audio: LED, Bluetooth, AirPlay,
//! Wi-Fi setup, speech. Simulated for now; hardware and system
//! integrations replace these one at a time.

use std::time::Duration;

use tapbox_core::{Flash, LedState};

/// Simulated LED and system integrations. Remembers what it was told so the
/// testbench can check it.
#[derive(Debug)]
pub struct SimOutputs {
    led: LedState,
    last_flash: Option<Flash>,
    /// Increments on every flash, so a UI can show repeated identical flashes.
    flash_count: u64,
    bluetooth_pauses: u32,
    name_announcements: u32,
    airplay_muted: bool,
    pairing_open_for: Option<Duration>,
    wifi_setup: bool,
}

impl Default for SimOutputs {
    fn default() -> Self {
        Self {
            led: LedState::Booting,
            last_flash: None,
            flash_count: 0,
            bluetooth_pauses: 0,
            name_announcements: 0,
            airplay_muted: false,
            pairing_open_for: None,
            wifi_setup: false,
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

    /// Number of flashes so far.
    #[must_use]
    pub fn flash_count(&self) -> u64 {
        self.flash_count
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

    pub(crate) fn set_led(&mut self, state: LedState) {
        self.led = state;
    }

    pub(crate) fn flash(&mut self, flash: Flash) {
        self.last_flash = Some(flash);
        self.flash_count += 1;
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
        LedState::ShuttingDown => "white fading out (shutting down)",
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

/// Wi-Fi as the web app sees it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "mode", rename_all = "kebab-case")]
pub enum WifiState {
    /// No known network: the box runs its own hotspot with the setup page.
    Setup {
        /// Hotspot name to join.
        hotspot: String,
        /// Why the last attempt failed, if it did.
        error: Option<String>,
    },
    /// Trying to join a network.
    Connecting {
        /// Network name.
        ssid: String,
    },
    /// On the home network.
    Connected {
        /// Network name.
        ssid: String,
    },
}

/// A network found by a scan.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct WifiNetwork {
    /// Network name.
    pub ssid: String,
    /// Signal strength, 0 to 100.
    pub signal: u8,
    /// Whether it needs a password.
    pub secure: bool,
}

/// Simulated Wi-Fi: a fixed list of networks; any password of 8 or more
/// characters works, except `wrongpassword`. On the Pi, NetworkManager does this (see `docs/setup-ux.md`).
#[derive(Debug)]
pub struct SimWifi {
    state: WifiState,
    networks: Vec<WifiNetwork>,
}

impl SimWifi {
    /// Start connected to "Home", or in setup mode.
    #[must_use]
    pub fn new(setup: bool) -> Self {
        let networks = vec![
            WifiNetwork { ssid: "Home".into(), signal: 82, secure: true },
            WifiNetwork { ssid: "Home-5G".into(), signal: 64, secure: true },
            WifiNetwork { ssid: "Neighbour".into(), signal: 31, secure: true },
            WifiNetwork { ssid: "Cafe guest".into(), signal: 18, secure: false },
        ];
        let state = if setup {
            WifiState::Setup { hotspot: "tapbox-setup".into(), error: None }
        } else {
            WifiState::Connected { ssid: "Home".into() }
        };
        Self { state, networks }
    }

    /// Current state.
    #[must_use]
    pub fn state(&self) -> &WifiState {
        &self.state
    }

    /// Visible networks, strongest first.
    #[must_use]
    pub fn networks(&self) -> &[WifiNetwork] {
        &self.networks
    }

    /// Go back to the hotspot and setup page.
    pub fn enter_setup(&mut self) {
        self.state = WifiState::Setup { hotspot: "tapbox-setup".into(), error: None };
    }

    /// Start joining `ssid`; [`SimWifi::finish`] completes it.
    pub fn begin_connect(&mut self, ssid: &str) {
        self.state = WifiState::Connecting { ssid: ssid.to_owned() };
    }

    /// Complete a join attempt. Returns whether it worked.
    pub fn finish(&mut self, ssid: &str, password: &str) -> bool {
        let known = self.networks.iter().find(|network| network.ssid == ssid);
        let ok = match known {
            Some(network) => !network.secure || (password.len() >= 8 && password != "wrongpassword"),
            None => false,
        };
        self.state = if ok {
            WifiState::Connected { ssid: ssid.to_owned() }
        } else {
            let error = if known.is_none() { "network not found" } else { "wrong password" };
            WifiState::Setup { hotspot: "tapbox-setup".into(), error: Some(error.into()) }
        };
        ok
    }
}

//! The rules from `docs/architecture.md` and `docs/setup-ux.md`, one test each.

use super::*;
use tapbox_card::{Card, Source};

fn uid(n: u8) -> Uid {
    Uid::new(vec![0x04, n, n, n]).unwrap()
}

fn sources(name: &str) -> Sources {
    Sources::one(Source::parse(&format!("https://example.org/{name}.mp3")))
}

fn content(name: &str) -> CardRead {
    CardRead::Card(Card::content(None, sources(name)).unwrap())
}

fn control(command: &str) -> CardRead {
    CardRead::Card(Card::control(None, ControlCommand::parse(command).unwrap()).unwrap())
}

/// A core plus a clock, so tests read like a script.
struct Bench {
    core: Core,
    now: Millis,
}

impl Bench {
    fn new() -> Self {
        Self::with(Config::default())
    }

    fn with(config: Config) -> Self {
        let mut uid_map = BTreeMap::new();
        uid_map.insert(uid(0xB0), sources("blank-card"));
        let mut bench = Self { core: Core::new(config, uid_map), now: Millis(0) };
        bench.send(Event::Tick);
        bench
    }

    fn send(&mut self, event: Event) -> Vec<Command> {
        self.core.handle(self.now, event)
    }

    fn tap(&mut self, n: u8, read: CardRead) -> Vec<Command> {
        self.send(Event::CardPresent { uid: uid(n), read })
    }

    fn lift(&mut self) -> Vec<Command> {
        self.send(Event::CardRemoved)
    }

    fn wait(&mut self, ms: u64) -> Vec<Command> {
        let mut all = Vec::new();
        for _ in 0..ms / 100 {
            self.now = Millis(self.now.0 + 100);
            all.extend(self.send(Event::Tick));
        }
        all
    }

    fn press(&mut self, button: Button) -> Vec<Command> {
        self.send(Event::Button { button, state: ButtonState::Pressed })
    }

    fn release(&mut self, button: Button) -> Vec<Command> {
        self.send(Event::Button { button, state: ButtonState::Released })
    }

    fn click(&mut self, button: Button) -> Vec<Command> {
        let mut out = self.press(button);
        out.extend(self.release(button));
        out
    }

    fn position(&mut self, seconds: u64) {
        self.send(Event::Player(PlayerEvent::Position(Duration::from_secs(seconds))));
    }
}

fn has(commands: &[Command], wanted: &Command) -> bool {
    commands.contains(wanted)
}

fn play_start(commands: &[Command]) -> Option<Duration> {
    commands.iter().find_map(|c| match c {
        Command::Play { start, .. } => Some(*start),
        _ => None,
    })
}

#[test]
fn first_event_sets_volume_and_leaves_booting() {
    let mut core = Core::new(Config::default(), BTreeMap::new());
    let out = core.handle(Millis(0), Event::Tick);
    assert_eq!(out, vec![Command::SetVolume(Percent::saturating(40)), Command::Led(LedState::Idle)]);
}

#[test]
fn tap_plays_from_start() {
    let mut b = Bench::new();
    let out = b.tap(1, content("a"));
    assert!(has(&out, &Command::Flash(Flash::CardRecognised)));
    assert_eq!(play_start(&out), Some(Duration::ZERO));
    assert!(has(&out, &Command::Led(LedState::Playing)));
    assert_eq!(b.core.playback(), Playback::Playing(uid(1)));
}

#[test]
fn lift_pauses_and_same_card_resumes() {
    let mut b = Bench::new();
    b.tap(1, content("a"));
    b.position(42);
    let out = b.lift();
    assert!(has(&out, &Command::Pause));
    assert!(has(&out, &Command::Persist), "pausing saves the position");
    let out = b.tap(1, content("a"));
    assert!(has(&out, &Command::Resume));
    assert_eq!(play_start(&out), None, "no reload when resuming");
}

#[test]
fn other_card_then_back_resumes_from_saved_position() {
    let mut b = Bench::new();
    b.tap(1, content("a"));
    b.position(42);
    b.lift();
    b.tap(2, content("b"));
    b.lift();
    let out = b.tap(1, content("a"));
    assert_eq!(play_start(&out), Some(Duration::from_secs(42)));
}

#[test]
fn keep_playing_when_lifted_if_configured() {
    let mut b = Bench::with(Config { on_card_removed: OnCardRemoved::KeepPlaying, ..Config::default() });
    b.tap(1, content("a"));
    assert!(!has(&b.lift(), &Command::Pause));
    assert_eq!(b.core.playback(), Playback::Playing(uid(1)));
}

#[test]
fn ended_content_restarts_from_zero_next_time() {
    let mut b = Bench::new();
    b.tap(1, content("a"));
    b.position(100);
    b.send(Event::Player(PlayerEvent::Ended));
    assert_eq!(b.core.playback(), Playback::Idle);
    b.lift();
    assert_eq!(play_start(&b.tap(1, content("a"))), Some(Duration::ZERO));
}

#[test]
fn invalid_and_unknown_blank_cards_flash_error() {
    let mut b = Bench::new();
    assert_eq!(b.tap(1, CardRead::Invalid), vec![Command::Flash(Flash::Error)]);
    assert_eq!(b.tap(2, CardRead::Blank), vec![Command::Flash(Flash::Error)]);
    assert_eq!(b.core.playback(), Playback::Idle);
}

#[test]
fn blank_card_uses_uid_mapping() {
    let mut b = Bench::new();
    let out = b.tap(0xB0, CardRead::Blank);
    assert!(out.iter().any(|c| matches!(c, Command::Play { sources: s, .. } if *s == sources("blank-card"))));
}

#[test]
fn card_takes_speaker_from_bluetooth() {
    let mut b = Bench::new();
    b.send(Event::ExternalStarted(ExternalSource::Bluetooth));
    let out = b.tap(1, content("a"));
    assert!(has(&out, &Command::PauseBluetooth));
}

#[test]
fn card_mutes_airplay_and_unmutes_when_session_ends() {
    let mut b = Bench::new();
    assert!(has(&b.send(Event::ExternalStarted(ExternalSource::AirPlay)), &Command::Led(LedState::AirPlay)));
    let out = b.tap(1, content("a"));
    assert!(has(&out, &Command::MuteAirPlay(true)));
    assert!(has(&out, &Command::Led(LedState::Playing)));
    let out = b.send(Event::ExternalStopped(ExternalSource::AirPlay));
    assert!(has(&out, &Command::MuteAirPlay(false)));
}

#[test]
fn external_start_pauses_card_and_does_not_auto_resume() {
    let mut b = Bench::new();
    b.tap(1, content("a"));
    assert!(has(&b.send(Event::ExternalStarted(ExternalSource::Bluetooth)), &Command::Pause));
    let out = b.send(Event::ExternalStopped(ExternalSource::Bluetooth));
    assert!(!has(&out, &Command::Resume));
    assert_eq!(b.core.playback(), Playback::Paused(uid(1)));
}

#[test]
fn play_button_toggles() {
    let mut b = Bench::new();
    b.tap(1, content("a"));
    assert!(has(&b.click(Button::Play), &Command::Pause));
    assert!(has(&b.click(Button::Play), &Command::Resume));
}

#[test]
fn volume_is_capped_by_max() {
    let mut b = Bench::new();
    for _ in 0..20 {
        b.click(Button::VolumeUp);
    }
    assert_eq!(b.core.volume(), Percent::saturating(70));
    for _ in 0..20 {
        b.click(Button::VolumeDown);
    }
    assert_eq!(b.core.volume(), Percent::ZERO);
}

#[test]
fn max_volume_card_lowers_volume() {
    let mut b = Bench::new();
    let out = b.tap(9, control("vol-max:30"));
    assert!(has(&out, &Command::SetVolume(Percent::saturating(30))));
    assert!(has(&out, &Command::Persist));
    assert_eq!(b.core.persistent_state().max_volume, Some(Percent::saturating(30)));
}

#[test]
fn control_cards_can_be_disabled() {
    let mut b = Bench::with(Config { control_cards: false, ..Config::default() });
    assert_eq!(b.tap(9, control("bt-pair")), vec![Command::Flash(Flash::Error)]);
}

#[test]
fn lifting_a_control_card_does_not_pause() {
    let mut b = Bench::new();
    b.tap(1, content("a"));
    b.lift();
    b.click(Button::Play); // resume without the card
    b.tap(9, control("say-name"));
    assert!(!has(&b.lift(), &Command::Pause));
}

#[test]
fn hold_play_three_seconds_opens_pairing_for_two_minutes() {
    let mut b = Bench::new();
    b.press(Button::Play);
    b.wait(3000);
    let out = b.release(Button::Play);
    assert!(has(&out, &Command::OpenBluetoothPairing(Duration::from_secs(120))));
    assert!(has(&out, &Command::Led(LedState::BluetoothPairing)));
    assert!(has(&b.wait(120_000), &Command::Led(LedState::Idle)));
}

#[test]
fn hold_play_ten_seconds_shuts_down_after_saving() {
    let mut b = Bench::new();
    b.tap(1, content("a"));
    b.press(Button::Play);
    let out = b.wait(10_000);
    let pause = out.iter().position(|c| *c == Command::Pause).unwrap();
    let persist = out.iter().position(|c| *c == Command::Persist).unwrap();
    let shutdown = out.iter().position(|c| *c == Command::Shutdown).unwrap();
    assert!(pause < persist && persist < shutdown);
    assert!(!has(&b.release(Button::Play), &Command::OpenBluetoothPairing(Duration::from_secs(120))));
}

#[test]
fn power_off_pauses_saves_and_shows_shutdown_led() {
    let mut b = Bench::new();
    b.tap(1, content("a"));
    b.position(42);
    let out = b.send(Event::PowerOff);
    let pause = out.iter().position(|c| *c == Command::Pause).unwrap();
    let persist = out.iter().position(|c| *c == Command::Persist).unwrap();
    assert!(pause < persist);
    assert!(has(&out, &Command::Led(LedState::ShuttingDown)));
    assert!(!has(&out, &Command::Shutdown), "the system is already shutting down");
    assert_eq!(b.core.persistent_state().positions_ms.get(&uid(1)), Some(&42_000));
}

#[test]
fn hold_both_volume_buttons_enters_wifi_setup() {
    let mut b = Bench::new();
    b.press(Button::VolumeDown);
    b.press(Button::VolumeUp);
    let out = b.wait(5000);
    assert!(has(&out, &Command::EnterWifiSetup));
    assert!(has(&out, &Command::Led(LedState::WifiSetup)));
}

#[test]
fn sleep_card_pauses_after_timeout() {
    let mut b = Bench::new();
    b.tap(9, control("sleep:1"));
    b.lift();
    b.tap(1, content("a"));
    assert!(!has(&b.wait(59_000), &Command::Pause));
    assert!(has(&b.wait(1_000), &Command::Pause));
}

#[test]
fn positions_are_saved_periodically_not_on_every_report() {
    let mut b = Bench::new();
    b.tap(1, content("a"));
    let mut persists = 0;
    for second in 1..=30 {
        b.now = Millis(b.now.0 + 1000);
        let mut out = b.send(Event::Player(PlayerEvent::Position(Duration::from_secs(second))));
        out.extend(b.send(Event::Tick));
        persists += out.iter().filter(|c| **c == Command::Persist).count();
    }
    assert_eq!(persists, 3, "every 10 s over 30 s");
}

#[test]
fn state_survives_restart() {
    let mut b = Bench::new();
    b.tap(1, content("a"));
    b.position(42);
    b.click(Button::VolumeDown);
    let saved = b.core.persistent_state();

    let mut core = Core::new(Config::default(), BTreeMap::new()).with_state(saved);
    let out = core.handle(Millis(0), Event::CardPresent { uid: uid(1), read: content("a") });
    assert_eq!(play_start(&out), Some(Duration::from_secs(42)));
    assert!(has(&out, &Command::SetVolume(Percent::saturating(35))));
}

#[test]
fn low_battery_led() {
    let mut b = Bench::new();
    assert!(has(&b.send(Event::BatteryLow), &Command::Led(LedState::BatteryLow)));
    assert!(has(&b.send(Event::BatteryOk), &Command::Led(LedState::Idle)));
}

#[test]
fn player_failure_flashes_error_and_goes_idle() {
    let mut b = Bench::new();
    b.tap(1, content("a"));
    let out = b.send(Event::Player(PlayerEvent::Failed));
    assert!(has(&out, &Command::Flash(Flash::Error)));
    assert_eq!(b.core.playback(), Playback::Idle);
}

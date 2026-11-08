//! Runs scenario scripts on a virtual clock: `wait 2m` takes microseconds,
//! and every run is deterministic.

use std::path::Path;
use std::time::Duration;

use anyhow::{Context as _, Result, bail};
use tapbox_core::{ButtonState, Command, Event, Millis};
use tokio::sync::mpsc;

use super::Bench;
use super::script::{self, Step};
use crate::app::{App, AppMsg};
use crate::player::{Player, SimPlayer};
use crate::store::Store;

/// Same tick rate as the real-time loop.
pub const TICK: Duration = Duration::from_millis(100);
/// Simulated track length unless a scenario sets `track-length`.
pub const DEFAULT_TRACK_LENGTH: Duration = Duration::from_secs(300);
/// A click holds the button this long.
pub const CLICK: Duration = Duration::from_millis(100);

/// An app on a virtual clock.
#[derive(Debug)]
pub struct VirtualBench<'a> {
    bench: &'a Bench,
    app: App,
    msgs: mpsc::UnboundedReceiver<AppMsg>,
    now: Millis,
    last: Vec<Command>,
    store: Store,
}

impl<'a> VirtualBench<'a> {
    /// Fresh state (an empty in-memory store), simulated player.
    ///
    /// # Errors
    /// If the app can't be built.
    pub async fn new(bench: &'a Bench) -> Result<Self> {
        let store = Store::memory();
        let (app, msgs) = Self::boot(bench, &store)?;
        let mut virtual_bench = Self { bench, app, msgs, now: Millis(0), last: Vec::new(), store };
        virtual_bench.app.tick(virtual_bench.now).await;
        virtual_bench.app.take_log();
        Ok(virtual_bench)
    }

    fn boot(bench: &Bench, store: &Store) -> Result<(App, mpsc::UnboundedReceiver<AppMsg>)> {
        bench.build_app(Player::Sim(SimPlayer::new(DEFAULT_TRACK_LENGTH)), Some(store.clone()), false)
    }

    /// Run one step.
    ///
    /// # Errors
    /// If an expectation fails or the step refers to an unknown card.
    pub async fn step(&mut self, step: &Step) -> Result<()> {
        if step.is_action() {
            self.app.take_log();
        }
        match step {
            Step::Click(button) => {
                self.button(*button, ButtonState::Pressed).await;
                self.advance(CLICK).await;
                self.button(*button, ButtonState::Released).await;
            }
            Step::Hold(button, duration) => {
                self.button(*button, ButtonState::Pressed).await;
                self.advance(*duration).await;
                self.button(*button, ButtonState::Released).await;
            }
            Step::Wait(duration) => self.advance(*duration).await,
            Step::PowerCut => {
                // Everything in memory is gone; only what was saved survives.
                let (app, msgs) = Self::boot(self.bench, &self.store)?;
                self.app = app;
                self.msgs = msgs;
                self.app.tick(self.now).await;
            }
            Step::Expect { expectation, negated } => {
                return script::check(&self.app, self.bench, &self.last, expectation, *negated);
            }
            instant => apply_instant(&mut self.app, self.bench, self.now, instant).await?,
        }
        self.last = self.app.take_log();
        Ok(())
    }

    async fn button(&mut self, button: tapbox_core::Button, state: ButtonState) {
        self.app.handle(self.now, Event::Button { button, state }).await;
    }

    async fn advance(&mut self, duration: Duration) {
        let end = self.now.plus(duration);
        while self.now < end {
            let step = end.since(self.now).min(TICK);
            self.now = self.now.plus(step);
            tokio::task::yield_now().await;
            while let Ok(msg) = self.msgs.try_recv() {
                self.app.on_msg(self.now, msg).await;
            }
            self.app.tick(self.now).await;
        }
    }

    /// The app, for inspection.
    #[must_use]
    pub fn app(&self) -> &App {
        &self.app
    }
}

/// Carry out a step that takes no time. Shared with the interactive console,
/// which expands `click`, `hold` and `wait` into timed primitives itself.
///
/// # Errors
/// On an unknown card, or `player` commands without the simulated player.
pub async fn apply_instant(app: &mut App, bench: &Bench, now: Millis, step: &Step) -> Result<()> {
    match step {
        Step::Tap(name) => {
            let card = bench.card(name)?;
            app.tap(now, card.uid.clone(), &card.bytes).await;
        }
        Step::Lift => app.handle(now, Event::CardRemoved).await,
        Step::Press(button) => app.handle(now, Event::Button { button: *button, state: ButtonState::Pressed }).await,
        Step::Release(button) => app.handle(now, Event::Button { button: *button, state: ButtonState::Released }).await,
        Step::External(source, true) => app.handle(now, Event::ExternalStarted(*source)).await,
        Step::External(source, false) => app.handle(now, Event::ExternalStopped(*source)).await,
        Step::Battery(low) => app.handle(now, if *low { Event::BatteryLow } else { Event::BatteryOk }).await,
        Step::PowerOff => app.handle(now, Event::PowerOff).await,
        Step::WifiDone => app.handle(now, Event::WifiSetupEnded).await,
        Step::PowerCut => bail!("`power cut` is handled by the runner"),
        Step::PlayerEnd | Step::PlayerFail | Step::TrackLength(_) => {
            let Player::Sim(player) = app.player_mut() else {
                bail!("`player` and `track-length` need the simulated player (not --audio mpv)");
            };
            match step {
                Step::PlayerEnd => player.force_end(),
                Step::PlayerFail => player.force_fail(),
                Step::TrackLength(length) => player.set_track_length(*length),
                _ => {}
            }
            app.tick(now).await;
        }
        Step::Click(_) | Step::Hold(..) | Step::Wait(_) | Step::Expect { .. } => {
            bail!("not an instant step: {step:?}");
        }
    }
    Ok(())
}

/// Run a scenario file against `bench`.
///
/// # Errors
/// On the first failing line, with its line number.
pub async fn run_scenario(bench: &Bench, path: &Path) -> Result<()> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let mut runner = VirtualBench::new(bench).await?;
    for (index, line) in text.lines().enumerate() {
        let context = || format!("{}:{}: {}", path.display(), index + 1, line.trim());
        let Some(step) = script::parse_line(line).with_context(context)? else {
            continue;
        };
        runner.step(&step).await.with_context(context)?;
    }
    Ok(())
}

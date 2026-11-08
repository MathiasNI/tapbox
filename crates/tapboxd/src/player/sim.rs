use std::time::Duration;

use tapbox_card::Percent;
use tapbox_core::{Failure, Millis, PlayerEvent};

use crate::resolver::Playable;

/// A player that plays nothing but keeps time like a real one: position
/// advances while playing, a position report goes out every second and the
/// track ends after `track_length`. The testbench can also force an end or a
/// failure.
#[derive(Debug)]
pub struct SimPlayer {
    track_length: Duration,
    loaded: Option<Playable>,
    position: Duration,
    playing: bool,
    last_poll: Millis,
    last_report: Duration,
    volume: Percent,
    pending: Vec<PlayerEvent>,
}

impl SimPlayer {
    /// A player whose tracks all last `track_length`.
    #[must_use]
    pub fn new(track_length: Duration) -> Self {
        Self {
            track_length,
            loaded: None,
            position: Duration::ZERO,
            playing: false,
            last_poll: Millis(0),
            last_report: Duration::ZERO,
            volume: Percent::ZERO,
            pending: Vec::new(),
        }
    }

    pub(super) fn load(&mut self, now: Millis, playable: &Playable, start: Duration) {
        self.loaded = Some(playable.clone());
        self.position = start;
        self.last_report = start;
        self.playing = true;
        self.last_poll = now;
    }

    pub(super) fn set_paused(&mut self, now: Millis, paused: bool) {
        self.advance(now);
        self.playing = !paused && self.loaded.is_some();
    }

    pub(super) fn stop(&mut self) {
        self.loaded = None;
        self.playing = false;
    }

    pub(super) fn set_volume(&mut self, volume: Percent) {
        self.volume = volume;
    }

    /// Current volume.
    #[must_use]
    pub fn volume(&self) -> Percent {
        self.volume
    }

    /// Whether audio would be coming out.
    #[must_use]
    pub fn is_playing(&self) -> bool {
        self.playing
    }

    pub(super) fn loaded(&self) -> Option<&Playable> {
        self.loaded.as_ref()
    }

    /// Testbench: length of tracks loaded from now on.
    pub fn set_track_length(&mut self, length: Duration) {
        self.track_length = length;
    }

    /// Testbench: end the current track now.
    pub fn force_end(&mut self) {
        if self.loaded.take().is_some() {
            self.playing = false;
            self.pending.push(PlayerEvent::Ended);
        }
    }

    /// Testbench: fail the current track now.
    pub fn force_fail(&mut self) {
        if self.loaded.take().is_some() {
            self.playing = false;
            self.pending.push(PlayerEvent::Failed(Failure::Playback("simulated failure".into())));
        }
    }

    fn advance(&mut self, now: Millis) {
        if self.playing {
            self.position += now.since(self.last_poll);
        }
        self.last_poll = now;
    }

    pub(super) fn poll(&mut self, now: Millis) -> Vec<PlayerEvent> {
        self.advance(now);
        let mut events = std::mem::take(&mut self.pending);
        if !self.playing {
            return events;
        }
        if self.position >= self.track_length {
            self.stop();
            events.push(PlayerEvent::Ended);
        } else if self.position >= self.last_report + Duration::from_secs(1) {
            self.last_report = self.position;
            events.push(PlayerEvent::Position(self.position));
        }
        events
    }
}

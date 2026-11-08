//! Audio playback. [`SimPlayer`] fakes it on a virtual clock for the
//! testbench; [`MpvPlayer`] drives a real mpv process.

mod mpv;
mod sim;

use std::time::Duration;

use anyhow::Result;
use tapbox_card::Percent;
use tapbox_core::{Millis, PlayerEvent};

pub use mpv::MpvPlayer;
pub use sim::SimPlayer;

use crate::resolver::Playable;

/// The player backend in use.
#[derive(Debug)]
pub enum Player {
    /// Simulated playback, no audio.
    Sim(SimPlayer),
    /// Real playback through mpv.
    Mpv(MpvPlayer),
}

impl Player {
    /// Load and start `playable` at `start`.
    ///
    /// # Errors
    /// If the backend can't be reached.
    pub async fn load(&mut self, now: Millis, playable: &Playable, start: Duration) -> Result<()> {
        match self {
            Self::Sim(player) => {
                player.load(now, playable, start);
                Ok(())
            }
            Self::Mpv(player) => player.load(playable, start).await,
        }
    }

    /// Pause or resume.
    ///
    /// # Errors
    /// If the backend can't be reached.
    pub async fn set_paused(&mut self, now: Millis, paused: bool) -> Result<()> {
        match self {
            Self::Sim(player) => {
                player.set_paused(now, paused);
                Ok(())
            }
            Self::Mpv(player) => player.set_paused(paused).await,
        }
    }

    /// Stop and unload.
    ///
    /// # Errors
    /// If the backend can't be reached.
    pub async fn stop(&mut self) -> Result<()> {
        match self {
            Self::Sim(player) => {
                player.stop();
                Ok(())
            }
            Self::Mpv(player) => player.stop().await,
        }
    }

    /// Set the output volume.
    ///
    /// # Errors
    /// If the backend can't be reached.
    pub async fn set_volume(&mut self, volume: Percent) -> Result<()> {
        match self {
            Self::Sim(player) => {
                player.set_volume(volume);
                Ok(())
            }
            Self::Mpv(player) => player.set_volume(volume).await,
        }
    }

    /// Events since the last poll.
    pub fn poll(&mut self, now: Millis) -> Vec<PlayerEvent> {
        match self {
            Self::Sim(player) => player.poll(now),
            Self::Mpv(player) => player.poll(),
        }
    }

    /// What is loaded, for the testbench's `expect loaded`.
    #[must_use]
    pub fn loaded(&self) -> Option<&Playable> {
        match self {
            Self::Sim(player) => player.loaded(),
            Self::Mpv(player) => player.loaded(),
        }
    }
}

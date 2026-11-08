//! The tapbox state machine.
//!
//! Everything the box *decides* lives here; everything it *does* lives
//! elsewhere. [`Core::handle`] takes an [`Event`] (a card was tapped, a button
//! was pressed, a tick passed) and returns the [`Command`]s to carry out (play
//! this, light the LED like that). It does no I/O and reads no clock: time
//! arrives as a [`Millis`] timestamp with every event. That makes every rule
//! in `docs/architecture.md` a plain unit test, and lets the testbench run
//! scenarios on a virtual clock.
//!
//! ```
//! use tapbox_core::{Core, Config, Event, Command, Millis, Uid, CardRead};
//! use tapbox_card::{Card, Source, Sources};
//!
//! let mut core = Core::new(Config::default(), Default::default());
//! let card = Card::content(None, Sources::one(Source::parse("https://example.org/a.mp3")))?;
//! let uid = Uid::new(vec![0x04, 0x11, 0x22, 0x33]).unwrap();
//! let commands = core.handle(Millis(0), Event::CardPresent { uid, read: CardRead::Card(card) });
//! assert!(commands.iter().any(|c| matches!(c, Command::Play { .. })));
//! # Ok::<(), tapbox_card::CardError>(())
//! ```

mod buttons;
mod core;
mod types;

pub use crate::core::{Config, Core, OnCardRemoved, PersistentState, Playback};
pub use crate::types::{
    Button, ButtonState, CardRead, Command, Event, ExternalSource, Flash, LedState, Millis, PlayerEvent, Uid,
};

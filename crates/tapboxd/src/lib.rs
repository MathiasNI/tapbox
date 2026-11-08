//! The tapbox daemon.
//!
//! - [`app`]: the shell around `tapbox_core::Core` that carries out its commands.
//! - [`bench`](mod@bench): the testbench: simulated hardware, interactive console, scenario runner.
//! - [`player`]: simulated and mpv playback.
//! - [`resolver`]: card sources → playable file or URL, with hash verification.
//! - [`reader`], [`outputs`]: hardware-facing pieces; simulated today.
//! - [`config`], [`store`]: config file and power-loss-safe state.

pub mod app;
pub mod bench;
pub mod config;
pub mod outputs;
pub mod player;
pub mod reader;
pub mod resolver;
pub mod store;

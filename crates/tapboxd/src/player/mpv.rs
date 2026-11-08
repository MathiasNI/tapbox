//! Typed client for mpv's JSON IPC (<https://mpv.io/manual/stable/#json-ipc>).
//!
//! mpv runs as a child process with `--idle` and is killed when this is dropped.
//! Commands are fire-and-forget; mpv reports back through events, which a
//! reader task turns into [`PlayerEvent`]s.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context as _, Result, bail};
use serde::Deserialize;
use serde_json::{Value, json};
use tapbox_card::Percent;
use tapbox_core::PlayerEvent;
use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};
use tokio::net::UnixStream;
use tokio::net::unix::OwnedWriteHalf;
use tokio::process::{Child, Command};
use tokio::sync::mpsc;

use crate::resolver::Playable;

const TIME_POS_OBSERVER: u64 = 1;

/// A running mpv process.
#[derive(Debug)]
pub struct MpvPlayer {
    _child: Child,
    writer: OwnedWriteHalf,
    events: mpsc::UnboundedReceiver<PlayerEvent>,
    loaded: Option<Playable>,
}

/// The subset of mpv's IPC messages we care about.
#[derive(Debug, Deserialize)]
struct Message {
    event: Option<String>,
    id: Option<u64>,
    data: Option<Value>,
    reason: Option<String>,
}

impl MpvPlayer {
    /// Start `mpv` (from `$PATH`) with its IPC socket at `socket`.
    ///
    /// # Errors
    /// If mpv can't be started or its socket doesn't appear within 5 s.
    pub async fn spawn(socket: &Path) -> Result<Self> {
        let _ = tokio::fs::remove_file(socket).await;
        let child = Command::new("mpv")
            .arg("--idle=yes")
            .arg("--no-video")
            .arg("--no-terminal")
            .arg("--audio-display=no")
            .arg(format!("--input-ipc-server={}", socket.display()))
            .stdin(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .context("starting mpv (is it installed?)")?;

        let stream = connect_with_retry(socket).await?;
        let (reader, writer) = stream.into_split();
        let (tx, events) = mpsc::unbounded_channel();
        tokio::spawn(read_events(BufReader::new(reader), tx));

        let mut player = Self { _child: child, writer, events, loaded: None };
        player.send(json!(["observe_property", TIME_POS_OBSERVER, "time-pos"])).await?;
        Ok(player)
    }

    async fn send(&mut self, command: Value) -> Result<()> {
        let mut line = serde_json::to_vec(&json!({ "command": command }))?;
        line.push(b'\n');
        self.writer.write_all(&line).await.context("writing to mpv")
    }

    pub(super) async fn load(&mut self, playable: &Playable, start: Duration) -> Result<()> {
        let target = match playable {
            Playable::File(path) => path.display().to_string(),
            Playable::Url(url) => url.clone(),
        };
        // `start` applies to the next loadfile; setting it as a property works
        // across mpv versions, unlike loadfile's options argument.
        self.send(json!(["set_property", "start", format!("{:.3}", start.as_secs_f64())])).await?;
        self.send(json!(["loadfile", target, "replace"])).await?;
        self.send(json!(["set_property", "pause", false])).await?;
        self.loaded = Some(playable.clone());
        Ok(())
    }

    pub(super) async fn set_paused(&mut self, paused: bool) -> Result<()> {
        self.send(json!(["set_property", "pause", paused])).await
    }

    pub(super) async fn stop(&mut self) -> Result<()> {
        self.loaded = None;
        self.send(json!(["stop"])).await
    }

    pub(super) async fn set_volume(&mut self, volume: Percent) -> Result<()> {
        self.send(json!(["set_property", "volume", volume.get()])).await
    }

    pub(super) fn poll(&mut self) -> Vec<PlayerEvent> {
        let mut events = Vec::new();
        while let Ok(event) = self.events.try_recv() {
            if matches!(event, PlayerEvent::Ended | PlayerEvent::Failed) {
                self.loaded = None;
            }
            events.push(event);
        }
        events
    }

    pub(super) fn loaded(&self) -> Option<&Playable> {
        self.loaded.as_ref()
    }
}

async fn connect_with_retry(socket: &Path) -> Result<UnixStream> {
    let socket: PathBuf = socket.to_path_buf();
    for _ in 0..50 {
        if let Ok(stream) = UnixStream::connect(&socket).await {
            return Ok(stream);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    bail!("mpv IPC socket {} did not appear", socket.display())
}

async fn read_events(mut reader: BufReader<tokio::net::unix::OwnedReadHalf>, tx: mpsc::UnboundedSender<PlayerEvent>) {
    let mut line = String::new();
    let mut last_whole_second: Option<u64> = None;
    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) | Err(_) => {
                tracing::error!("mpv IPC connection closed");
                let _ = tx.send(PlayerEvent::Failed);
                return;
            }
            Ok(_) => {}
        }
        let Ok(message) = serde_json::from_str::<Message>(&line) else {
            continue;
        };
        let event = match message.event.as_deref() {
            Some("property-change") if message.id == Some(TIME_POS_OBSERVER) => {
                let Some(seconds) = message.data.as_ref().and_then(Value::as_f64).filter(|s| *s >= 0.0) else {
                    continue;
                };
                let position = Duration::from_secs_f64(seconds);
                // time-pos changes many times a second; one report per second is plenty.
                let whole = position.as_secs();
                if last_whole_second == Some(whole) {
                    continue;
                }
                last_whole_second = Some(whole);
                PlayerEvent::Position(position)
            }
            // "stop" (replaced by another loadfile), "quit" and "redirect" are not ends.
            Some("end-file") => match message.reason.as_deref() {
                Some("eof") => PlayerEvent::Ended,
                Some("error") => PlayerEvent::Failed,
                _ => continue,
            },
            _ => continue,
        };
        if tx.send(event).is_err() {
            return;
        }
    }
}

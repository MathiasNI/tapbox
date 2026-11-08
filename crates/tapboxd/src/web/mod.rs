//! The box's web app: a single page served by the daemon, plus a small JSON
//! API. The page gets the box's state pushed over server-sent events and
//! sends changes as actions.
//!
//! The web server never touches the app directly. It sends [`Request`]s
//! over a channel to the loop that owns the app, and reads [`Snapshot`]s
//! the loop publishes on a `watch` channel. Uploads are the exception: they
//! are streamed straight to the library directory (hashing on the way) and
//! then reported to the app.

pub mod snapshot;
mod upload;

use std::convert::Infallible;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context as _, Result};
use axum::Router;
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{StatusCode, header};
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::{Html, IntoResponse, Json, Redirect, Response};
use axum::routing::{get, post};
use futures_util::Stream;
use serde::Deserialize;
use tapbox_card::ContentHash;
use tapbox_core::{Button, ExternalSource, OnCardRemoved, Uid};
use tokio::sync::{mpsc, oneshot, watch};
use tokio_stream::StreamExt as _;
use tokio_stream::wrappers::WatchStream;

pub use snapshot::{Snapshot, build_snapshot};

use crate::app::WebAction;

/// The page. One file, no build step: easy to read and change.
const PAGE: &str = include_str!("ui.html");

/// Largest upload accepted (a long audiobook).
pub const MAX_UPLOAD_BYTES: usize = 2 * 1024 * 1024 * 1024;

/// What the web server asks the app loop to do.
#[derive(Debug)]
pub struct Request {
    /// The change.
    pub action: Action,
    /// Where the result goes.
    pub reply: oneshot::Sender<Result<(), String>>,
}

/// A change: for the app, or for the testbench's simulated hardware.
#[derive(Debug, Clone)]
pub enum Action {
    /// Handled by the app.
    App(WebAction),
    /// Testbench only: act on the virtual hardware.
    Sim(SimAction),
}

/// Testbench controls shown in the page's simulator panel.
#[derive(Debug, Clone)]
pub enum SimAction {
    /// Put a virtual card from `cards/` on the reader.
    Tap(String),
    /// Put a brand-new blank card on the reader.
    TapNewBlank,
    /// Take the card off.
    Lift,
    /// Press and release a button.
    Click(Button),
    /// Hold a button for this long.
    Hold(Button, Duration),
    /// Internal: the release half of a click or hold.
    Release(Button),
    /// Another source starts or stops.
    External(ExternalSource, bool),
}

#[derive(Clone)]
struct WebState {
    requests: mpsc::Sender<Request>,
    snapshot: watch::Receiver<Snapshot>,
    library_dir: PathBuf,
}

/// Start serving on `addr`. Runs until the process ends.
///
/// # Errors
/// If the address can't be bound.
pub async fn serve(
    addr: SocketAddr,
    requests: mpsc::Sender<Request>,
    snapshot: watch::Receiver<Snapshot>,
    library_dir: PathBuf,
) -> Result<()> {
    let state = WebState { requests, snapshot, library_dir };
    let app = Router::new()
        .route("/", get(page))
        .route("/api/state", get(state_json))
        .route("/api/events", get(events))
        .route("/api/action", post(action))
        .route("/api/upload", post(upload::upload).layer(DefaultBodyLimit::max(MAX_UPLOAD_BYTES)))
        // Captive-portal probes from phones on the setup hotspot.
        .route("/generate_204", get(captive_probe))
        .route("/hotspot-detect.html", get(captive_probe))
        .route("/connecttest.txt", get(captive_probe))
        .with_state(state);
    let listener =
        tokio::net::TcpListener::bind(addr).await.with_context(|| format!("binding the web app to {addr}"))?;
    tracing::info!(%addr, "web app listening");
    axum::serve(listener, app).await.context("web server")
}

async fn page() -> impl IntoResponse {
    ([(header::CACHE_CONTROL, "no-cache")], Html(PAGE))
}

async fn state_json(State(state): State<WebState>) -> Json<Snapshot> {
    Json(state.snapshot.borrow().clone())
}

async fn events(State(state): State<WebState>) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>>> {
    let stream = WatchStream::new(state.snapshot)
        .map(|snapshot| Ok(SseEvent::default().json_data(&snapshot).unwrap_or_else(|_| SseEvent::default())));
    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// On the setup hotspot every probe is sent to the setup page, which makes
/// phones open it by themselves. Once connected, answer like the real thing.
async fn captive_probe(State(state): State<WebState>) -> Response {
    let in_setup = state.snapshot.borrow().wifi_in_setup();
    if in_setup { Redirect::temporary("/").into_response() } else { StatusCode::NO_CONTENT.into_response() }
}

/// Actions as the page sends them.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
enum ApiAction {
    StartLinking,
    CancelLinking,
    Link { uid: String, title: String, hash: String },
    Unlink { uid: String },
    DeleteFile { hash: String },
    SetName { name: String },
    SetVolume { volume: u8 },
    SetMaxVolume { volume: u8 },
    SetOnCardRemoved { value: OnCardRemoved },
    PairBluetooth,
    TogglePlay,
    WifiConnect { ssid: String, password: String },
    WifiSetup,
    SimTap { card: String },
    SimTapNewBlank,
    SimLift,
    SimClick { button: ApiButton },
    SimHold { button: ApiButton, ms: u64 },
    SimExternal { source: ApiSource, start: bool },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum ApiButton {
    Play,
    VolUp,
    VolDown,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum ApiSource {
    Bluetooth,
    Airplay,
}

impl ApiAction {
    fn into_action(self) -> Result<Action, String> {
        let uid = |text: &str| Uid::from_hex(text).ok_or_else(|| format!("`{text}` is not a card UID"));
        let hash = |text: &str| {
            ContentHash::from_ni(&format!("ni:///sha-256;{text}"))
                .or_else(|| hex_hash(text))
                .ok_or_else(|| format!("`{text}` is not a file hash"))
        };
        let button = |button: ApiButton| match button {
            ApiButton::Play => Button::Play,
            ApiButton::VolUp => Button::VolumeUp,
            ApiButton::VolDown => Button::VolumeDown,
        };
        Ok(match self {
            Self::StartLinking => Action::App(WebAction::StartLinking),
            Self::CancelLinking => Action::App(WebAction::CancelLinking),
            Self::Link { uid: u, title, hash: h } => {
                let title = title.trim().to_owned();
                if title.is_empty() || title.len() > 64 {
                    return Err("the title must be 1 to 64 characters".into());
                }
                Action::App(WebAction::Link { uid: uid(&u)?, title, hash: hash(&h)? })
            }
            Self::Unlink { uid: u } => Action::App(WebAction::Unlink { uid: uid(&u)? }),
            Self::DeleteFile { hash: h } => Action::App(WebAction::DeleteFile { hash: hash(&h)? }),
            Self::SetName { name } => Action::App(WebAction::SetName(name)),
            Self::SetVolume { volume } => Action::App(WebAction::SetVolume(volume)),
            Self::SetMaxVolume { volume } => Action::App(WebAction::SetMaxVolume(volume)),
            Self::SetOnCardRemoved { value } => Action::App(WebAction::SetOnCardRemoved(value)),
            Self::PairBluetooth => Action::App(WebAction::PairBluetooth),
            Self::TogglePlay => Action::App(WebAction::TogglePlay),
            Self::WifiConnect { ssid, password } => Action::App(WebAction::WifiConnect { ssid, password }),
            Self::WifiSetup => Action::App(WebAction::WifiSetup),
            Self::SimTap { card } => Action::Sim(SimAction::Tap(card)),
            Self::SimTapNewBlank => Action::Sim(SimAction::TapNewBlank),
            Self::SimLift => Action::Sim(SimAction::Lift),
            Self::SimClick { button: b } => Action::Sim(SimAction::Click(button(b))),
            Self::SimHold { button: b, ms } => {
                Action::Sim(SimAction::Hold(button(b), Duration::from_millis(ms.min(30_000))))
            }
            Self::SimExternal { source, start } => Action::Sim(SimAction::External(
                match source {
                    ApiSource::Bluetooth => ExternalSource::Bluetooth,
                    ApiSource::Airplay => ExternalSource::AirPlay,
                },
                start,
            )),
        })
    }
}

fn hex_hash(text: &str) -> Option<ContentHash> {
    if text.len() != 64 {
        return None;
    }
    let mut digest = [0u8; 32];
    for (index, byte) in digest.iter_mut().enumerate() {
        *byte = u8::from_str_radix(text.get(index * 2..index * 2 + 2)?, 16).ok()?;
    }
    Some(ContentHash::from_digest(digest))
}

async fn action(State(state): State<WebState>, Json(body): Json<ApiAction>) -> Response {
    let action = match body.into_action() {
        Ok(action) => action,
        Err(message) => return error(StatusCode::BAD_REQUEST, &message),
    };
    match send(&state.requests, action).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(message) => error(StatusCode::UNPROCESSABLE_ENTITY, &message),
    }
}

async fn send(requests: &mpsc::Sender<Request>, action: Action) -> Result<(), String> {
    let (reply, result) = oneshot::channel();
    requests.send(Request { action, reply }).await.map_err(|_| "the box is shutting down".to_owned())?;
    result.await.map_err(|_| "no answer from the box".to_owned())?
}

fn error(status: StatusCode, message: &str) -> Response {
    (status, Json(serde_json::json!({ "error": message }))).into_response()
}

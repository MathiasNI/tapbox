//! Uploads: streamed to the library directory, hashed on the way, so memory
//! use stays flat however big the file is.

use std::path::{Path, PathBuf};

use axum::extract::{Multipart, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json, Response};
use sha2::{Digest as _, Sha256};
use tapbox_card::ContentHash;
use tokio::io::AsyncWriteExt as _;

use super::{Action, WebState, error, send};
use crate::app::WebAction;

/// Audio formats mpv plays and parents are likely to have.
const ALLOWED_EXTENSIONS: [&str; 9] = ["mp3", "m4a", "m4b", "aac", "ogg", "opus", "flac", "wav", "mp4"];

pub(super) async fn upload(State(state): State<WebState>, mut multipart: Multipart) -> Response {
    let mut added = Vec::new();
    loop {
        let field = match multipart.next_field().await {
            Ok(Some(field)) => field,
            Ok(None) => break,
            Err(err) => return error(StatusCode::BAD_REQUEST, &format!("upload interrupted: {err}")),
        };
        let Some(file_name) = field.file_name().map(str::to_owned) else { continue };
        match save_field(&state, &file_name, field).await {
            Ok(entry) => added.push(entry),
            Err(message) => return error(StatusCode::UNPROCESSABLE_ENTITY, &message),
        }
    }
    if added.is_empty() {
        return error(StatusCode::BAD_REQUEST, "no file in the upload");
    }
    (StatusCode::CREATED, Json(serde_json::json!({ "files": added }))).into_response()
}

async fn save_field(
    state: &WebState,
    original_name: &str,
    mut field: axum::extract::multipart::Field<'_>,
) -> Result<serde_json::Value, String> {
    let name = safe_file_name(original_name)
        .ok_or_else(|| format!("`{original_name}` isn't a supported audio file ({})", ALLOWED_EXTENSIONS.join(", ")))?;
    tokio::fs::create_dir_all(&state.library_dir).await.map_err(|err| err.to_string())?;
    let partial = state.library_dir.join(format!(".{name}.part"));
    let result = async {
        let mut file = tokio::fs::File::create(&partial).await.map_err(|err| err.to_string())?;
        let mut hasher = Sha256::new();
        while let Some(chunk) = field.chunk().await.map_err(|err| format!("upload interrupted: {err}"))? {
            hasher.update(&chunk);
            file.write_all(&chunk).await.map_err(|err| err.to_string())?;
        }
        file.sync_all().await.map_err(|err| err.to_string())?;
        Ok::<_, String>(ContentHash::from_digest(hasher.finalize().into()))
    }
    .await;
    let hash = match result {
        Ok(hash) => hash,
        Err(message) => {
            let _ = tokio::fs::remove_file(&partial).await;
            return Err(message);
        }
    };

    let already =
        state.snapshot.borrow().library.iter().find(|file| file.hash == hash.to_hex()).map(|f| f.name.clone());
    if let Some(existing) = already {
        let _ = tokio::fs::remove_file(&partial).await;
        return Ok(serde_json::json!({ "name": existing, "hash": hash.to_hex(), "duplicate": true }));
    }
    let path = free_path(&state.library_dir, &name).await;
    tokio::fs::rename(&partial, &path).await.map_err(|err| err.to_string())?;
    send(&state.requests, Action::App(WebAction::FileAdded { hash, path: path.clone() })).await?;
    let final_name = path.file_name().and_then(|n| n.to_str()).unwrap_or(&name).to_owned();
    Ok(serde_json::json!({ "name": final_name, "hash": hash.to_hex(), "duplicate": false }))
}

/// Keep a plain file name: no directories, no hidden files, known extension.
fn safe_file_name(original: &str) -> Option<String> {
    let base = original.rsplit(['/', '\\']).next()?.trim();
    let cleaned: String =
        base.chars()
            .map(|c| {
                if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.' | '(' | ')' | '\'' | ',') { c } else { '_' }
            })
            .collect();
    let cleaned = cleaned.trim_start_matches('.').to_owned();
    let (stem, extension) = cleaned.rsplit_once('.')?;
    let extension = extension.to_ascii_lowercase();
    (!stem.trim().is_empty() && ALLOWED_EXTENSIONS.contains(&extension.as_str()) && cleaned.len() <= 200)
        .then(|| format!("{}.{extension}", stem.trim()))
}

/// `name`, or `name (2)`, `name (3)`, … if taken.
async fn free_path(dir: &Path, name: &str) -> PathBuf {
    let (stem, extension) = name.rsplit_once('.').unwrap_or((name, ""));
    let mut candidate = dir.join(name);
    let mut n = 2;
    while tokio::fs::try_exists(&candidate).await.unwrap_or(false) {
        candidate = dir.join(format!("{stem} ({n}).{extension}"));
        n += 1;
    }
    candidate
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_are_made_safe() {
        assert_eq!(safe_file_name("Bedtime Story.MP3").as_deref(), Some("Bedtime Story.mp3"));
        assert_eq!(safe_file_name("../../etc/passwd.mp3").as_deref(), Some("passwd.mp3"));
        assert_eq!(safe_file_name("C:\\Music\\Æøå sang.flac").as_deref(), Some("Æøå sang.flac"));
        assert_eq!(safe_file_name(".hidden.mp3").as_deref(), Some("hidden.mp3"));
        assert_eq!(safe_file_name("song.exe"), None);
        assert_eq!(safe_file_name(".mp3"), None);
        assert_eq!(safe_file_name("noextension"), None);
    }
}

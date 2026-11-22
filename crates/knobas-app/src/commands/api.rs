//! The API explorer's commands (ux31, ADR-0057): send a request a person
//! composed, stop it, read its answer's body, list what was sent, hold a secret
//! typed on the page, and the saved servers. None answers a secret.

use tauri::Manager as _;

use crate::api::{held, kept, send, servers};
use crate::{IpcError, IpcErrorCode, Lifecycle};

/// Sends one request, once ([`send::send`]); a saved server's CA file is trusted for a request to its own origin.
///
/// # Errors
/// A refusal before anything was sent, by name.
#[tauri::command]
pub async fn api_send<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    send: send::ApiSend,
) -> Result<send::ApiAnswer, IpcError> {
    let mut ca_file = None;
    if let Some(id) = send.server.clone()
        && let Ok(pool) = app.state::<Lifecycle>().pool()
    {
        let saved = servers::load(&pool).await?;
        if let Some(server) = saved.servers.iter().find(|s| s.id == id) {
            let same = url::Url::parse(&server.url)
                .ok()
                .zip(url::Url::parse(&send.url).ok())
                .is_some_and(|(a, b)| send::origin_of(&a) == send::origin_of(&b));
            if same {
                ca_file.clone_from(&server.ca_file);
            }
        }
        // The server last used with this file, for the select's order.
        if saved.last.get(&send.file) != Some(&id) {
            let mut next = saved.clone();
            next.last.insert(send.file.clone(), id.clone());
            let _ = servers::save(&pool, next).await;
        }
    }
    send::send(send, ca_file).await
}

/// Stops send `id` at once.
#[tauri::command]
pub fn api_cancel(id: u64) {
    send::cancel(id);
}

/// The body of send `id` as raw bytes, JSON indented where `pretty`.
///
/// # Errors
/// `not_found` once the body is no longer kept.
#[tauri::command]
pub fn api_answer_body(id: u64, pretty: bool) -> Result<tauri::ipc::Response, IpcError> {
    send::body(id, pretty)
        .map(|bytes| tauri::ipc::Response::new(bytes.to_vec()))
        .ok_or_else(|| IpcError::new(IpcErrorCode::NotFound, "The answer is no longer kept."))
}

/// What was sent from the spec `file` since knobas started, the newest first.
#[tauri::command]
pub fn api_sent(file: String) -> Vec<kept::SentLine> {
    kept::kept().lines(&file)
}

/// Holds a secret typed on the explorer's page for `origin` and `scheme`, until knobas quits. Answers nothing.
///
/// # Errors
/// An origin that is not an `http` or `https` address's.
#[tauri::command]
pub fn api_hold_secret(
    origin: String,
    scheme: String,
    secret: String,
    user: Option<String>,
) -> Result<(), IpcError> {
    let url = url::Url::parse(&origin)
        .map_err(|_| IpcError::new(IpcErrorCode::Invalid, format!("{origin} is not an origin.")))?;
    if url.scheme() != "http" && url.scheme() != "https" {
        return Err(IpcError::new(
            IpcErrorCode::Invalid,
            format!("{origin} is not an http or https origin."),
        ));
    }
    held::held().hold(
        &send::origin_of(&url),
        &scheme,
        held::Held::new(secret, user),
    );
    Ok(())
}

/// Which schemes a secret is held for at `origin`: names only, never a secret.
#[tauri::command]
pub fn api_held(origin: String) -> Vec<String> {
    held::held().schemes(&origin)
}

/// Forgets every secret held for `origin`.
#[tauri::command]
pub fn api_forget_held(origin: String) {
    held::held().forget(&origin);
}

/// The saved servers.
///
/// # Errors
/// Before bring-up, or the read fails.
#[tauri::command]
pub async fn api_servers<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<servers::SavedServers, IpcError> {
    let pool = app.state::<Lifecycle>().pool()?;
    servers::load(&pool).await
}

/// Saves the whole list of servers and answers it as stored.
///
/// # Errors
/// A server refused by name, before bring-up, or the write fails.
#[tauri::command]
pub async fn api_set_servers<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    servers: servers::SavedServers,
) -> Result<servers::SavedServers, IpcError> {
    let pool = app.state::<Lifecycle>().pool()?;
    servers::save(&pool, servers).await
}

/// Saves the body of send `id` to the Downloads folder under `name` (a free name beside any file there); answers the path.
/// A saved file is the person's; knobas keeps no copy.
///
/// # Errors
/// `not_found` once the body is no longer kept; the folder or the write fails.
#[tauri::command]
pub async fn api_save_answer(id: u64, name: String) -> Result<String, IpcError> {
    use tokio::io::AsyncWriteExt as _;
    let bytes = send::body(id, false)
        .ok_or_else(|| IpcError::new(IpcErrorCode::NotFound, "The answer is no longer kept."))?;
    let dir = dirs::download_dir().ok_or_else(|| {
        IpcError::new(
            IpcErrorCode::Internal,
            "This computer has no Downloads folder.",
        )
    })?;
    let (path, mut file) = crate::downloads::create_free(&dir, &crate::downloads::safe_name(&name))
        .await
        .map_err(|(path, error)| IpcError::internal(format!("{}: {error}", path.display())))?;
    file.write_all(&bytes)
        .await
        .map_err(|error| IpcError::internal(format!("{}: {error}", path.display())))?;
    Ok(path.display().to_string())
}

/// Opens a spec in an application with an API spec command (ux31, section 23): knobas writes the spec and, where
/// asked, the collection into a scratch folder of its own and runs the command on them.
///
/// # Errors
/// As [`crate::api::open::open_in`].
#[tauri::command]
pub async fn api_open_in<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    application: String,
    spec: crate::api::open::SpecToOpen,
) -> Result<crate::api::open::OpenedSpec, IpcError> {
    let pool = app.state::<Lifecycle>().pool()?;
    let profile = app.state::<crate::profile::Profile>().inner().clone();
    crate::api::open::open_in(&pool, &profile.dir, &application, spec).await
}

/// Which shipped API clients are on this computer: `{ bruno }`. Read behind the menu's first frame.
#[tauri::command]
pub async fn api_clients_found() -> ClientsFound {
    let home = dirs::home_dir();
    let bruno =
        tokio::task::spawn_blocking(move || crate::api::found::bruno(home.as_deref()).is_some())
            .await
            .unwrap_or(false);
    ClientsFound { bruno }
}

/// Which shipped API clients are installed.
#[derive(Debug, Clone, Copy, serde::Serialize, ts_rs::TS)]
#[ts(export_to = "api.ts")]
pub struct ClientsFound {
    pub bruno: bool,
}

/// How many bytes the API clients' scratch folders hold, for Settings › Checkouts and Open in.
#[tauri::command]
pub async fn api_scratch_size<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> u64 {
    let dir = app.state::<crate::profile::Profile>().inner().dir.clone();
    tokio::task::spawn_blocking(move || crate::api::open::scratch_size(&dir))
        .await
        .unwrap_or(0)
}

/// Removes every API client scratch folder (*Remove them now*).
#[tauri::command]
pub async fn api_scratch_clear<R: tauri::Runtime>(app: tauri::AppHandle<R>) {
    let dir = app.state::<crate::profile::Profile>().inner().dir.clone();
    let _ = tokio::task::spawn_blocking(move || {
        crate::api::scratch::clear(&crate::api::open::scratch_root(&dir))
    })
    .await;
}

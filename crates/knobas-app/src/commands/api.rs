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

/// The body of send `id` through the file pipeline `steps` (#2533): *Pipeline* on an answer, as on a file. The bytes
/// are the kept answer's, read in memory ([`send::piped`]); `name` is the file the answer goes by. A command step runs
/// as saved.
///
/// # Errors
/// `not_found` once the body is no longer kept; the saved command steps cannot be read. A step that fails is an answer.
#[tauri::command]
pub async fn api_answer_piped<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    id: u64,
    name: String,
    steps: Vec<crate::file_pipeline::PipeStep>,
) -> Result<crate::file_pipeline::PipeRun, IpcError> {
    let commands = commands_for(&app, &steps).await?;
    send::piped(id, &name, &steps, &commands)
        .await
        .ok_or_else(|| IpcError::new(IpcErrorCode::NotFound, "The answer is no longer kept."))
}

/// [`api_answer_piped`]'s answer saved to the Downloads folder as `save_as`, only when the person presses *Download*;
/// answers the path. A saved file is the person's; knobas keeps no copy.
///
/// # Errors
/// As [`api_answer_piped`]; a failed run; the folder or the write.
#[tauri::command]
pub async fn api_answer_piped_save<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    id: u64,
    name: String,
    steps: Vec<crate::file_pipeline::PipeStep>,
    save_as: String,
) -> Result<String, IpcError> {
    let run = api_answer_piped(app, id, name, steps).await?;
    crate::commands::entity::save_run(run, &save_as).await
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

/// *Save to Downloads* on an example made from a schema (the XSD design,
/// section 23): `text` written to the Downloads folder under `name`, as an
/// answer is saved; knobas keeps no copy. Answers the path.
///
/// # Errors
/// No Downloads folder, or the file cannot be written.
#[tauri::command]
pub async fn xsd_save_example(name: String, text: String) -> Result<String, IpcError> {
    use tokio::io::AsyncWriteExt as _;
    let dir = dirs::download_dir().ok_or_else(|| {
        IpcError::new(
            IpcErrorCode::Internal,
            "This computer has no Downloads folder.",
        )
    })?;
    let (path, mut file) = crate::downloads::create_free(&dir, &crate::downloads::safe_name(&name))
        .await
        .map_err(|(path, error)| IpcError::internal(format!("{}: {error}", path.display())))?;
    file.write_all(text.as_bytes())
        .await
        .map_err(|error| IpcError::internal(format!("{}: {error}", path.display())))?;
    Ok(path.display().to_string())
}

/// A file pipeline over a document knobas holds in memory and no repository
/// has (an example made from a schema, opened in the XML view: step E1):
/// `steps` over `text`'s bytes, the file `name`, as `entity_file_piped` runs
/// them over a file's. A command step runs as saved.
///
/// # Errors
/// The saved command steps cannot be read. A step that fails is an answer.
#[tauri::command]
pub async fn text_piped<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    text: String,
    name: String,
    steps: Vec<crate::file_pipeline::PipeStep>,
) -> Result<crate::file_pipeline::PipeRun, IpcError> {
    let commands = commands_for(&app, &steps).await?;
    Ok(crate::file_pipeline::run_bytes_with(
        &steps,
        &commands,
        text.into_bytes(),
        &name,
        crate::file_pipeline::Limits::default(),
    )
    .await)
}

/// [`text_piped`]'s answer saved to the Downloads folder as `save_as`, as the
/// value viewer saves over a file; answers the path.
///
/// # Errors
/// As [`text_piped`]; a failed run; the folder or the write.
#[tauri::command]
pub async fn text_piped_save<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    text: String,
    name: String,
    steps: Vec<crate::file_pipeline::PipeStep>,
    save_as: String,
) -> Result<String, IpcError> {
    let run = text_piped(app, text, name, steps).await?;
    crate::commands::entity::save_run(run, &save_as).await
}

/// The saved command steps, read only where `steps` holds a command step.
async fn commands_for<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    steps: &[crate::file_pipeline::PipeStep],
) -> Result<Vec<crate::file_pipeline::saved::CommandStep>, IpcError> {
    if !steps
        .iter()
        .any(|step| matches!(step, crate::file_pipeline::PipeStep::Command { .. }))
    {
        return Ok(Vec::new());
    }
    let pool = app.state::<Lifecycle>().pool()?;
    crate::file_pipeline::saved::command_steps(&pool).await
}

/// Opens a jq session over a JSON document (the XML view's design, section
/// 26): the file's bytes as `entity_file_bytes` reads them, or `text` where
/// the view draws something else than the file (a pipeline's answer).
/// Answers the session's id.
///
/// # Errors
/// As `entity_file_bytes`; neither a file nor a text named.
#[tauri::command]
pub async fn json_query_open<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    entity_id: Option<String>,
    path: Option<String>,
    commit: Option<String>,
    text: Option<String>,
) -> Result<u32, IpcError> {
    let document = if let Some(text) = text {
        text.into_bytes()
    } else {
        let (Some(entity_id), Some(path)) = (entity_id, path) else {
            return Err(IpcError::invalid("A jq session needs a file or a text."));
        };
        let state = crate::sources::state(&app)?;
        crate::commands::entity::entity_file_bytes_inner(
            &state.pool,
            &state.secrets,
            &state.registry,
            &state.live,
            &entity_id,
            &path,
            commit.as_deref(),
        )
        .await?
    };
    Ok(app
        .state::<crate::file_pipeline::session::JqSessions>()
        .open(document))
}

/// A filter asked of session `id`: its first values, their count and the
/// paths of what it picked; stopped past its deadline.
///
/// # Errors
/// The session has ended.
#[tauri::command]
pub async fn json_query<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    id: u32,
    filter: String,
) -> Result<crate::file_pipeline::session::JsonQueryAnswer, IpcError> {
    app.state::<crate::file_pipeline::session::JqSessions>()
        .query(id, &filter)
        .await
}

/// Ends session `id` and its helper.
#[tauri::command]
pub fn json_query_close<R: tauri::Runtime>(app: tauri::AppHandle<R>, id: u32) {
    app.state::<crate::file_pipeline::session::JqSessions>()
        .close(id);
}

/// An image at an address in a document, loaded on the press that asks (the XML view's design, section 14): one `GET`,
/// no cookie, the bytes sniffed, kept in memory.
///
/// # Errors
/// As [`crate::api::image::fetch`].
#[tauri::command]
pub async fn image_fetch(url: String) -> Result<crate::api::image::FetchedImage, IpcError> {
    crate::api::image::fetch(&url).await
}

/// The hosts whose images load when their dialog opens.
///
/// # Errors
/// The read fails.
#[tauri::command]
pub async fn image_hosts<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<crate::api::image::ImageHosts, IpcError> {
    let pool = app.state::<Lifecycle>().pool()?;
    crate::api::image::hosts(&pool).await
}

/// Stores the hosts whose images load when their dialog opens; answers them as stored.
///
/// # Errors
/// The write fails.
#[tauri::command]
pub async fn set_image_hosts<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    hosts: crate::api::image::ImageHosts,
) -> Result<crate::api::image::ImageHosts, IpcError> {
    let pool = app.state::<Lifecycle>().pool()?;
    crate::api::image::set_hosts(&pool, hosts).await
}

/// The schema files a person pointed to, and the addresses downloaded (the XSD
/// design, section 17).
///
/// # Errors
/// The read fails.
#[tauri::command]
pub async fn xsd_locations<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<crate::api::schemas::SchemaLocations, IpcError> {
    let pool = app.state::<Lifecycle>().pool()?;
    crate::api::schemas::locations(&pool).await
}

/// The `.xsd` files of the synced repositories that declare `namespace` (the
/// XSD design, section 16, step 6), from the files index: nothing where the
/// index holds no file.
///
/// # Errors
/// `not_ready` before the app is up.
#[tauri::command]
pub async fn files_declaring<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    namespace: String,
) -> Result<Vec<crate::api::schemas::DeclaringFile>, IpcError> {
    let index = app.state::<Lifecycle>().files();
    tokio::task::spawn_blocking(move || crate::api::schemas::declaring(&index, &namespace))
        .await
        .map_err(IpcError::internal)
}

/// Downloads the schema at `address` on a press, into the profile's
/// `schemas/` (the XSD design, section 17): never by itself.
///
/// # Errors
/// As [`crate::api::schemas::download`].
#[tauri::command]
pub async fn xsd_fetch<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    address: String,
) -> Result<crate::api::schemas::DownloadedSchema, IpcError> {
    let dir = app.state::<crate::profile::Profile>().inner().dir.clone();
    crate::api::schemas::download(&dir, &address).await
}

/// A downloaded schema's text by its SHA-256; `null` where it is not kept.
///
/// # Errors
/// A name that is no SHA-256.
#[tauri::command]
pub async fn xsd_downloaded<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    sha256: String,
) -> Result<Option<String>, IpcError> {
    let dir = app.state::<crate::profile::Profile>().inner().dir.clone();
    crate::api::schemas::downloaded(&dir, &sha256).await
}

/// Removes a downloaded schema's file (Settings › Pipelines › Schemas, *Forget*).
///
/// # Errors
/// A name that is no SHA-256, or the file cannot be removed.
#[tauri::command]
pub async fn xsd_forget<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    sha256: String,
) -> Result<(), IpcError> {
    let dir = app.state::<crate::profile::Profile>().inner().dir.clone();
    crate::api::schemas::forget_download(&dir, &sha256).await
}

/// Keeps the schema files pointed to; answers them as kept.
///
/// # Errors
/// A list over the cap, or the write fails.
#[tauri::command]
pub async fn xsd_set_locations<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    list: crate::api::schemas::SchemaLocations,
) -> Result<crate::api::schemas::SchemaLocations, IpcError> {
    let pool = app.state::<Lifecycle>().pool()?;
    crate::api::schemas::set_locations(&pool, list).await
}

/// Whether an XML file is checked against its schema when one is found (the
/// XML view, section 11).
///
/// # Errors
/// The read fails.
#[tauri::command]
pub async fn xml_check<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<crate::api::schemas::XmlCheck, IpcError> {
    let pool = app.state::<Lifecycle>().pool()?;
    crate::api::schemas::check(&pool).await
}

/// Stores whether XML files are checked.
///
/// # Errors
/// The write fails.
#[tauri::command]
pub async fn set_xml_check<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    check: crate::api::schemas::XmlCheck,
) -> Result<crate::api::schemas::XmlCheck, IpcError> {
    let pool = app.state::<Lifecycle>().pool()?;
    crate::api::schemas::set_check(&pool, check).await
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

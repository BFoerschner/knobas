//! The JAR view's reads (ux19, `docs/decisions/2026-11-18-a-jar-is-drawn-as-its-contents.md`): an archive in a
//! repository's tree listed, a nested one listed, an entry's bytes, a class's outline, a class decompiled, the archive
//! or an entry saved to Downloads. Bodies in [`crate::archive`].

use crate::IpcError;

fn resources<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Vec<std::path::PathBuf> {
    use tauri::Manager;
    crate::archive::java::decompiler_dirs(app.path().resource_dir().ok())
}

/// Opens the archive at `path` in `entity_id`'s tree (a `.jar`, `.war`, `.ear` or `.aar`), at `commit` (the file
/// block's, handed back) or the entity's own reference: its entries, manifest, Maven coordinates, nested JARs, under
/// a handle the other reads name it by. Read from the clone where it holds the commit, else from the source, up to
/// 1 GiB, outside the read cache. The first one opened also looks for a Java, behind the answer.
///
/// # Errors
///
/// `invalid` for a file over the cap or one that is no archive (saying why), `not_found` for no file there, and
/// otherwise as `entity_file_bytes`.
#[tauri::command]
pub async fn archive_open<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    entity_id: String,
    path: String,
    commit: Option<String>,
) -> Result<knobas_jar::ArchiveIndex, IpcError> {
    let state = crate::sources::state(&app)?;
    let dirs = resources(&app);
    let pool = state.pool.clone();
    // Behind the answer, once: what *Decompile* stands at is known before a class is opened.
    tokio::spawn(async move {
        crate::archive::java::runtime(Some(&pool), &dirs, false).await;
    });
    crate::archive::open(
        &state.pool,
        &state.secrets,
        &state.registry,
        &state.live,
        &entity_id,
        &path,
        commit.as_deref(),
    )
    .await
}

/// The archive `entry` inside the open archive `handle`: a fat JAR's library, a WAR's.
///
/// # Errors
///
/// `not_found` for a handle no longer kept (open the archive again) or an entry not in it; `invalid` for one refused
/// (a name that leaves the archive, a zip bomb, its size) or that is no archive.
#[tauri::command]
pub async fn archive_nested(
    handle: String,
    entry: String,
) -> Result<knobas_jar::ArchiveIndex, IpcError> {
    crate::archive::nested(&handle, &entry).await
}

/// One entry's bytes, unpacked, up to 50 MiB: a resource the view draws with the file viewer's renderers. Answered
/// raw, as `entity_file_bytes` is.
///
/// # Errors
///
/// As [`archive_nested`].
#[tauri::command]
pub async fn archive_entry(
    handle: String,
    entry: String,
) -> Result<tauri::ipc::Response, IpcError> {
    crate::archive::entry_bytes(&handle, &entry)
        .await
        .map(tauri::ipc::Response::new)
}

/// A class's outline, as `javap` would give it, read in Rust.
///
/// # Errors
///
/// As [`archive_nested`], and `invalid` for bytes that are no class file.
#[tauri::command]
pub async fn archive_class(
    handle: String,
    entry: String,
) -> Result<knobas_jar::ClassOutline, IpcError> {
    crate::archive::class(&handle, &entry).await
}

/// A class decompiled by the bundled Vineflower with the Java found here; an inner class as its top-level class.
/// Kept on disk by the classes' hash, so the same class again answers at once. A newer call stops one still running.
///
/// # Errors
///
/// `invalid` where no Java or no decompiler can run (the reason said), past a bound, or where the decompiler fails.
#[tauri::command]
pub async fn archive_decompile<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    handle: String,
    entry: String,
) -> Result<String, IpcError> {
    use tauri::Manager;
    let state = crate::sources::state(&app)?;
    let dirs = resources(&app);
    let rt = crate::archive::java::runtime(Some(&state.pool), &dirs, false).await;
    let cache = app
        .path()
        .app_cache_dir()
        .map_err(|e| IpcError::internal(format!("no cache folder: {e}")))?
        .join("decompiled");
    let (top, classes) = crate::archive::class_family(&handle, &entry, 8 * 1024 * 1024).await?;
    crate::archive::java::decompile(&rt, &dirs, &cache, &top, classes).await
}

/// Saves the archive (`entry` `null`) or one entry of it into the Downloads folder under a free name; answers the
/// file's path.
///
/// # Errors
///
/// As [`archive_nested`], and `internal` where the file cannot be written or the system names no Downloads folder.
#[tauri::command]
pub async fn archive_download<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    handle: String,
    entry: Option<String>,
) -> Result<String, IpcError> {
    use tauri::Manager;
    let dir = app
        .path()
        .download_dir()
        .map_err(|error| IpcError::internal(format!("no downloads folder: {error}")))?;
    crate::archive::download(&handle, entry.as_deref(), &dir).await
}

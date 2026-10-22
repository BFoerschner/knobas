//! The capture window and its shortcut.

use super::*;

// -- the capture window and its shortcut (#503) -----------------------------
//
// Here rather than in a module pair of their own, for the reason #499's four
// and #501's three are: a capture is a note, `create_note` is in this module,
// and §10.8 freezes the `commands/` + `ipc/` layout. The behaviour is
// `crate::capture`; these five are the shims.

/// The capture shortcut as the settings pane draws it: what is stored, and why
/// it is not holding.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) if the read fails.
#[tauri::command]
pub async fn capture_shortcut(
    lifecycle: State<'_, Lifecycle>,
    capture: State<'_, crate::capture::CaptureState>,
) -> Result<crate::capture::ShortcutView, IpcError> {
    let pool = lifecycle.pool()?;
    crate::capture::shortcut(&pool, &capture).await
}

/// Set the capture shortcut, or clear it with a blank one.
///
/// A combination the plugin cannot parse, or one the operating system will not
/// hand over, is **not** a refusal of this call: it is stored and the answer
/// says why it is not registered (`crate::capture`, point 2).
///
/// Generic over the runtime for the reason `app_status` is: a bare
/// `tauri::AppHandle` means `AppHandle<Wry>`, and a command taking one cannot
/// be registered on the `tauri::test` mock app at all.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) if the write
/// fails.
#[tauri::command]
pub async fn set_capture_shortcut<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    capture: State<'_, crate::capture::CaptureState>,
    accelerator: Option<String>,
) -> Result<crate::capture::ShortcutView, IpcError> {
    let pool = lifecycle.pool()?;
    crate::capture::set_shortcut(
        &pool,
        &capture,
        std::sync::Arc::new(crate::capture::Plugin::new(app)),
        accelerator.as_deref(),
    )
    .await
}

/// Record where the reader is standing, so a capture can attach it.
///
/// Called by the **main** window whenever its room or its foreground changes.
/// Both arguments are the shell's own two answers, unchanged: `context` is the
/// stored room's `ctx:` entity (`null` for a derived room, which has none) and
/// `foreground` is `shell/timer.ts`'s `roomForeground`. Nothing here recomputes
/// either, which is the whole of why this command exists rather than the
/// capture window asking the database what the reader was looking at.
///
/// Needs no database, so it answers during bring-up like `ping` does: a window
/// that has drawn a room has something true to say about it whether or not
/// PostgreSQL is up yet.
#[tauri::command]
pub fn record_capture_context(
    capture: State<'_, crate::capture::CaptureState>,
    context: Option<String>,
    foreground: Option<String>,
) {
    crate::capture::record(
        &capture,
        crate::capture::Recorded {
            context,
            foreground,
        },
    );
}

/// What the main window last recorded -- the capture window's half of the pair
/// above.
#[tauri::command]
pub fn capture_context(
    capture: State<'_, crate::capture::CaptureState>,
) -> crate::capture::Recorded {
    crate::capture::context(&capture)
}

/// Bring the main window forward and open one note in it.
///
/// The capture window's *Open in knobas* button. The capture window closes
/// itself afterwards; this command does not close it, because the two acts have
/// different failure modes and a reader whose main window would not come
/// forward should still have the window their words are in.
///
/// # Errors
///
/// [`IpcErrorCode::NotFound`](crate::IpcErrorCode::NotFound) if the main window
/// is not open, [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) if it
/// will not come forward.
#[tauri::command]
pub fn reveal_note<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    note_id: String,
) -> Result<(), IpcError> {
    crate::capture::reveal_note(&app, &note_id)
}

/// Turn *Show notes and quick capture* on or off (#832, D10), registering or
/// releasing the stored capture shortcut to match. Answers the capture
/// shortcut's view, which carries the setting.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) if the write
/// fails.
#[tauri::command]
pub async fn set_notes_shown<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    capture: State<'_, crate::capture::CaptureState>,
    shown: bool,
) -> Result<crate::capture::ShortcutView, IpcError> {
    let pool = lifecycle.pool()?;
    crate::capture::set_shown(
        &pool,
        &capture,
        std::sync::Arc::new(crate::capture::Plugin::new(app)),
        shown,
    )
    .await
}

/// *Export notes as Markdown* (#832, D10): one `.md` per note in
/// `~/Downloads/knobas-notes`, through the block-to-Markdown converter.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) if there is no
/// Downloads folder, the read fails or a file cannot be written.
#[tauri::command]
pub async fn export_notes_markdown<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
) -> Result<crate::notes_export::NotesExport, IpcError> {
    use tauri::Manager;
    let pool = lifecycle.pool()?;
    let downloads = app.path().download_dir().map_err(IpcError::internal)?;
    crate::notes_export::export(&pool, &downloads.join(crate::notes_export::FOLDER)).await
}

//! The window's own look (ADR-0055): the page tells this side its mode and each side's page colour, so the bare
//! window has them before the page paints (`crate::window_look`). And the themes a person keeps, mirrored into the
//! database so a backup carries them and a restore brings them back (Björn, 2026-11-22: "they are included in the
//! backups"). A share export carries no setting, so none carries them.

use tauri::{AppHandle, Manager, Runtime};

use crate::IpcError;
use crate::window_look::{self, WindowLook};

/// Keeps the window's look in the profile's folder and gives it to the main window now: the title bar's side, and
/// the page colour of the side drawn behind the web view.
///
/// # Errors
///
/// `Invalid` for a mode that is not `system`, `light` or `dark`, or a colour that is not `#RRGGBB`; `Internal`
/// where the profile's folder cannot be written.
#[tauri::command]
pub fn window_look_set<R: Runtime>(
    app: AppHandle<R>,
    mode: String,
    light: String,
    dark: String,
) -> Result<(), IpcError> {
    let look = WindowLook::new(&mode, &light, &dark).map_err(IpcError::invalid)?;
    let dir = app
        .try_state::<crate::Profile>()
        .map(|profile| profile.dir.clone())
        .ok_or_else(|| IpcError::not_ready("the profile is not up yet"))?;
    window_look::write(&dir, &look).map_err(IpcError::internal)?;
    if let Some(window) = app.get_webview_window("main") {
        // Best effort: a window that will not take the colour is the page's to draw.
        let _ = window.set_theme(look.theme());
        let system_dark = matches!(window.theme(), Ok(tauri::Theme::Dark));
        let _ = window.set_background_color(Some(look.colour(system_dark)));
    }
    Ok(())
}

/// The `knobas.setting` key holding the page's theme store (`tf/themes.svelte.ts`'s snapshot) as JSON text.
pub const THEMES_KEY: &str = "themes.kept";
/// The largest snapshot kept: 64 themes of a few kilobytes each, with room to spare.
pub const LARGEST_THEMES: usize = 2 * 1024 * 1024;

/// The theme store as last kept in the database, `None` where nothing is.
///
/// # Errors
///
/// `NotReady` before the database is up; `Internal` if the read fails.
#[tauri::command]
pub async fn themes_kept<R: Runtime>(app: AppHandle<R>) -> Result<Option<String>, IpcError> {
    let pool = app.state::<crate::Lifecycle>().pool()?;
    crate::settings::read(&pool, THEMES_KEY).await
}

/// Keeps the theme store in the database, so the next backup carries it; `None` removes it.
///
/// # Errors
///
/// `Invalid` for a snapshot that is not a JSON object or is over 2 MB; `NotReady` before the database is up;
/// `Internal` if the write fails.
#[tauri::command]
pub async fn set_themes_kept<R: Runtime>(
    app: AppHandle<R>,
    snapshot: Option<String>,
) -> Result<(), IpcError> {
    if let Some(text) = &snapshot {
        check_snapshot(text).map_err(IpcError::invalid)?;
    }
    let pool = app.state::<crate::Lifecycle>().pool()?;
    crate::settings::write(&pool, THEMES_KEY, snapshot.as_deref()).await
}

/// A snapshot the page may keep: a JSON object of at most [`LARGEST_THEMES`] bytes.
fn check_snapshot(text: &str) -> Result<(), String> {
    if text.len() > LARGEST_THEMES {
        return Err(format!(
            "the themes are {} bytes, over the 2 MB kept",
            text.len()
        ));
    }
    match serde_json::from_str::<serde_json::Value>(text) {
        Ok(serde_json::Value::Object(_)) => Ok(()),
        _ => Err("the themes are not a JSON object".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    const MIRROR: &str = include_str!("../../../../app/src/lib/ipc/window.ts");

    /// The window command in the handler list is invoked from the mirror by its registered name, and with its arguments.
    #[test]
    fn the_mirror_invokes_the_command_by_its_registered_name_with_its_arguments() {
        crate::commands::mirror::assert_mirrors("window", MIRROR, "app/src/lib/ipc/window.ts", 3);
        use crate::commands::mirror::{invocation, keys};
        assert_eq!(
            keys(invocation(MIRROR, "window_look_set")),
            ["mode", "light", "dark"]
        );
        assert_eq!(keys(invocation(MIRROR, "set_themes_kept")), ["snapshot"]);
    }

    #[test]
    fn a_snapshot_is_a_json_object_of_at_most_two_megabytes() {
        assert!(super::check_snapshot(r#"{"v":1,"themes":[]}"#).is_ok());
        assert!(super::check_snapshot("[]").is_err());
        assert!(super::check_snapshot("not json").is_err());
        let big = format!(r#"{{"v":"{}"}}"#, "x".repeat(super::LARGEST_THEMES));
        assert!(super::check_snapshot(&big).is_err());
    }
}

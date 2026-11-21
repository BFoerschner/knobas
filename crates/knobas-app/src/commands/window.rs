//! The window's own look (ADR-0055): the page tells this side its mode and each side's page colour, so the bare
//! window has them before the page paints (`crate::window_look`).

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

#[cfg(test)]
mod tests {
    const MIRROR: &str = include_str!("../../../../app/src/lib/ipc/window.ts");

    /// The window command in the handler list is invoked from the mirror by its registered name, and with its arguments.
    #[test]
    fn the_mirror_invokes_the_command_by_its_registered_name_with_its_arguments() {
        crate::commands::mirror::assert_mirrors("window", MIRROR, "app/src/lib/ipc/window.ts", 1);
        use crate::commands::mirror::{invocation, keys};
        assert_eq!(
            keys(invocation(MIRROR, "window_look_set")),
            ["mode", "light", "dark"]
        );
    }
}

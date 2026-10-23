//! *Open the backup folder* (#1471, #1704): the running profile's `backups`
//! folder in the file manager, opened from here and not by the webview.
//!
//! #1471 opened it from the window with the opener plugin's `open-path`,
//! scoped in `capabilities/default.json` to both profiles' folders: one grant
//! covers the one main window whichever profile runs it, so a demo window
//! could open the real profile's archives (LIFE-10). The folder now comes from
//! the [`Profile`] this process runs as, so the boundary P13 draws is drawn in
//! code, and the window holds no path grant for it.
//!
//! The opener is a seam ([`FolderOpener`]) for the reason
//! [`crate::notify::Backend`] is one: the real one starts Finder, and a test
//! has to see which folder was asked for without starting anything.

use std::path::Path;
use std::sync::Arc;

use crate::{IpcError, Profile};

/// Shows a folder in the platform's file manager.
pub trait Opener: Send + Sync + 'static {
    /// Open `folder`, or say why it could not be opened.
    ///
    /// # Errors
    /// The platform's refusal, as text.
    fn open(&self, folder: &Path) -> Result<(), String>;
}

impl<F> Opener for F
where
    F: Fn(&Path) -> Result<(), String> + Send + Sync + 'static,
{
    fn open(&self, folder: &Path) -> Result<(), String> {
        self(folder)
    }
}

/// The opener `open_backup_folder` uses, managed once in `setup`.
#[derive(Clone)]
pub struct FolderOpener(Arc<dyn Opener>);

impl FolderOpener {
    /// Over any opener: the platform's in the app, a recording one in a test.
    #[must_use]
    pub fn new(opener: Arc<dyn Opener>) -> Self {
        Self(opener)
    }

    /// The platform's file manager, through the opener plugin's Rust API.
    ///
    /// That API checks no scope: the scope belongs to the plugin's own
    /// webview command, which this replaces.
    #[must_use]
    pub fn platform() -> Self {
        Self::new(Arc::new(|folder: &Path| {
            tauri_plugin_opener::open_path(folder, None::<&str>).map_err(|error| error.to_string())
        }))
    }

    /// Open `folder` with this opener.
    ///
    /// # Errors
    /// The opener's refusal, as text.
    pub fn open(&self, folder: &Path) -> Result<(), String> {
        self.0.open(folder)
    }
}

/// [`open`] for the profile and opener `setup` managed, or the one honest
/// refusal for a call that beat `setup` (as [`super::state`] answers for the
/// database).
///
/// # Errors
/// [`NotReady`](crate::IpcErrorCode::NotReady) before `setup` has managed the
/// profile and the opener, and whatever [`open`] refuses with.
pub fn open_managed<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Result<(), IpcError> {
    use tauri::Manager as _;

    let starting = || IpcError::not_ready("knobas is still starting".to_owned());
    let profile = app.try_state::<Profile>().ok_or_else(starting)?;
    let opener = app.try_state::<FolderOpener>().ok_or_else(starting)?;
    open(&profile, &opener)
}

/// Open `profile`'s backup folder, creating it first.
///
/// The folder is created because the first export creates it, and a profile
/// that has not exported yet would otherwise answer the button with the file
/// manager's "does not exist". It is the profile's own folder, so making it
/// writes nowhere the profile does not already own.
///
/// # Errors
/// [`Internal`](crate::IpcErrorCode::Internal) naming the folder when it
/// cannot be created or the file manager refuses it.
pub fn open(profile: &Profile, opener: &FolderOpener) -> Result<(), IpcError> {
    let folder = profile.backup_dir();
    std::fs::create_dir_all(&folder).map_err(|error| {
        IpcError::internal(format!("could not create {}: {error}", folder.display()))
    })?;
    opener.open(&folder).map_err(|error| {
        IpcError::internal(format!("could not open {}: {error}", folder.display()))
    })
}

//! The install, per platform (decisions D9).
//!
//! Each is a function over paths. Windows hands the staged installer to the app's exit, which
//! starts it after PostgreSQL stopped. On the Mac and for an AppImage the new copy takes the old
//! one's place in one rename *before* the restart, so a failure leaves the old app in place and
//! nothing closed. The steps follow `tauri-plugin-updater` 2.14.1 (MIT or Apache-2.0), without its
//! administrator path.

use std::path::{Path, PathBuf};

#[cfg(unix)]
pub mod appimage;
#[cfg(unix)]
pub mod macos;
pub mod windows;

/// Why an install did not happen. Nothing was closed in any case.
#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    /// The folder knobas is installed in refused the change.
    #[error("knobas can't replace itself where it is installed ({path}): {why}")]
    CannotReplace { path: PathBuf, why: String },
    #[error("the update does not unpack: {0}")]
    Unpack(String),
    #[error("{0}")]
    Io(String),
}

/// A folder only this user can enter (0700 on unix), for the staged copy.
///
/// # Errors
///
/// The file system's.
pub fn private_dir(in_dir: Option<&Path>) -> std::io::Result<tempfile::TempDir> {
    let mut builder = tempfile::Builder::new();
    builder.prefix("knobas-update-");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        builder.permissions(std::fs::Permissions::from_mode(0o700));
    }
    match in_dir {
        Some(dir) => builder.tempdir_in(dir),
        None => builder.tempdir(),
    }
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    #[test]
    fn the_private_folder_is_this_users_alone() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = super::private_dir(None).unwrap();
        let mode = std::fs::metadata(dir.path()).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
    }
}

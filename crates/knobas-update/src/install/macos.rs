//! The Mac: the bundle exchanged in one step (decisions D9).
//!
//! The `tar.gz` is unpacked from the private copy into a new folder on the app's own volume (the
//! system's temp folder when it is on that volume, else beside the app); `tar` refuses entries
//! that leave it. The staged bundle's root is set to 0755, then the staged bundle and the
//! installed one are exchanged in one `renameatx_np(RENAME_SWAP)`. Where the file system cannot
//! exchange, the installed bundle is renamed aside and the staged one in, and the old one put
//! back if the second rename fails. The old bundle then sits in the staging folder, which the next
//! start removes. Unix-wide: Linux's `RENAME_EXCHANGE` runs the same tests.

use std::io;
use std::os::unix::fs::MetadataExt as _;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};

use super::InstallError;

/// The exchange of two paths in one step.
pub fn exchange(a: &Path, b: &Path) -> io::Result<()> {
    rustix::fs::renameat_with(
        rustix::fs::CWD,
        a,
        rustix::fs::CWD,
        b,
        rustix::fs::RenameFlags::EXCHANGE,
    )
    .map_err(io::Error::from)
}

/// Whether an exchange failed because the file system cannot exchange, not because it refused.
fn unsupported(error: &io::Error) -> bool {
    matches!(
        rustix::io::Errno::from_io_error(error),
        Some(rustix::io::Errno::NOTSUP | rustix::io::Errno::INVAL | rustix::io::Errno::NOSYS)
    )
}

/// Where the archive is unpacked: the system temp folder when it is on the app's volume, else
/// beside the app.
#[must_use]
pub fn staging_parent(installed: &Path) -> PathBuf {
    let beside = installed
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
    let temp = std::env::temp_dir();
    let device = |path: &Path| std::fs::metadata(path).map(|meta| meta.dev()).ok();
    match (device(&temp), device(&beside)) {
        (Some(a), Some(b)) if a == b => temp,
        _ => beside,
    }
}

/// Unpack `archive` (a `tar.gz` holding one `<name>.app`) and put it in place of `installed`.
/// Answers the staging folder, which now holds the old bundle and is removed at the next start.
///
/// # Errors
///
/// [`InstallError`]; the installed bundle is as it was.
pub fn swap_bundle(archive: &Path, installed: &Path) -> Result<PathBuf, InstallError> {
    swap_bundle_with(
        archive,
        installed,
        &staging_parent(installed),
        exchange,
        |a, b| std::fs::rename(a, b),
    )
}

/// [`swap_bundle`] with the staging folder's parent and the two renames given, for the tests.
///
/// # Errors
///
/// [`InstallError`].
pub fn swap_bundle_with(
    archive: &Path,
    installed: &Path,
    parent: &Path,
    exchange: impl Fn(&Path, &Path) -> io::Result<()>,
    rename: impl Fn(&Path, &Path) -> io::Result<()>,
) -> Result<PathBuf, InstallError> {
    let cannot = |why: String| InstallError::CannotReplace {
        path: installed.to_path_buf(),
        why,
    };
    if !installed.is_dir() {
        return Err(cannot("the installed app is not there".to_owned()));
    }
    let staging = super::private_dir(Some(parent)).map_err(|error| {
        cannot(format!(
            "no staging folder in {}: {error}",
            parent.display()
        ))
    })?;
    let file = std::fs::File::open(archive).map_err(|error| InstallError::Io(error.to_string()))?;
    let mut unpacked = tar::Archive::new(flate2::read::GzDecoder::new(file));
    unpacked.set_overwrite(false);
    unpacked
        .unpack(staging.path())
        .map_err(|error| InstallError::Unpack(error.to_string()))?;
    let staged = std::fs::read_dir(staging.path())
        .map_err(|error| InstallError::Io(error.to_string()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| path.is_dir() && path.extension().is_some_and(|e| e == "app"))
        .ok_or_else(|| InstallError::Unpack("the archive holds no .app".to_owned()))?;
    std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755))
        .map_err(|error| InstallError::Io(error.to_string()))?;

    match exchange(&staged, installed) {
        Ok(()) => {}
        Err(error) if unsupported(&error) => {
            let aside = staging.path().join("previous.app");
            rename(installed, &aside).map_err(|error| cannot(error.to_string()))?;
            if let Err(error) = rename(&staged, installed) {
                if let Err(undo) = rename(&aside, installed) {
                    tracing::error!(%undo, "the installed app could not be put back");
                }
                return Err(cannot(error.to_string()));
            }
        }
        Err(error) => return Err(cannot(error.to_string())),
    }
    Ok(staging.keep())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fake `<name>.app` with one executable, and its archive as `release.yml` packs it.
    fn bundle(root: &Path, name: &str, marker: &str) -> PathBuf {
        let app = root.join(name);
        std::fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
        let exe = app.join("Contents/MacOS/knobas-app");
        std::fs::write(&exe, marker).unwrap();
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
        app
    }

    fn archive(root: &Path, app: &Path) -> PathBuf {
        let path = root.join("knobas_3.2.9-ux28_aarch64.app.tar.gz");
        let file = std::fs::File::create(&path).unwrap();
        let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
            file,
            flate2::Compression::fast(),
        ));
        builder.append_dir_all("knobas.app", app).unwrap();
        builder.into_inner().unwrap().finish().unwrap();
        path
    }

    fn marker(app: &Path) -> String {
        std::fs::read_to_string(app.join("Contents/MacOS/knobas-app")).unwrap()
    }

    #[test]
    fn the_staged_bundle_takes_the_installed_ones_place() {
        let root = tempfile::tempdir().unwrap();
        let installed = bundle(&root.path().join("Applications"), "knobas.app", "old");
        let new = bundle(&root.path().join("build"), "knobas.app", "new");
        let archive = archive(root.path(), &new);
        let staging = swap_bundle_with(&archive, &installed, root.path(), exchange, |a, b| {
            std::fs::rename(a, b)
        })
        .unwrap();
        assert_eq!(marker(&installed), "new");
        let mode = std::fs::metadata(installed.join("Contents/MacOS/knobas-app"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o111, 0o111, "the executable stays executable");
        // The old bundle sits in the staging folder, for the next start to remove.
        assert_eq!(marker(&staging.join("knobas.app")), "old");
    }

    /// Mutant: remove the old bundle before the rename, and no app is found.
    #[test]
    fn a_refused_rename_leaves_the_installed_bundle_as_it_was() {
        let root = tempfile::tempdir().unwrap();
        let apps = root.path().join("Applications");
        let installed = bundle(&apps, "knobas.app", "old");
        let new = bundle(&root.path().join("build"), "knobas.app", "new");
        let archive = archive(root.path(), &new);
        std::fs::set_permissions(&apps, std::fs::Permissions::from_mode(0o555)).unwrap();
        let outcome = swap_bundle_with(&archive, &installed, root.path(), exchange, |a, b| {
            std::fs::rename(a, b)
        });
        std::fs::set_permissions(&apps, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(
            matches!(outcome, Err(InstallError::CannotReplace { .. })),
            "{outcome:?}"
        );
        assert_eq!(marker(&installed), "old");
    }

    #[test]
    fn where_the_exchange_is_not_supported_the_two_renames_run_and_undo() {
        let root = tempfile::tempdir().unwrap();
        let installed = bundle(&root.path().join("Applications"), "knobas.app", "old");
        let new = bundle(&root.path().join("build"), "knobas.app", "new");
        let archive = archive(root.path(), &new);
        let no_exchange = |_: &Path, _: &Path| {
            Err(io::Error::from_raw_os_error(
                rustix::io::Errno::NOTSUP.raw_os_error(),
            ))
        };

        swap_bundle_with(&archive, &installed, root.path(), no_exchange, |a, b| {
            std::fs::rename(a, b)
        })
        .unwrap();
        assert_eq!(marker(&installed), "new");

        // The second rename fails: the old one is put back.
        let installed = bundle(&root.path().join("Other"), "knobas.app", "old");
        let calls = std::cell::Cell::new(0);
        let outcome = swap_bundle_with(&archive, &installed, root.path(), no_exchange, |a, b| {
            calls.set(calls.get() + 1);
            if calls.get() == 2 {
                Err(io::Error::other("refused"))
            } else {
                std::fs::rename(a, b)
            }
        });
        assert!(matches!(outcome, Err(InstallError::CannotReplace { .. })));
        assert_eq!(marker(&installed), "old");
    }

    #[test]
    fn an_archive_entry_that_leaves_the_folder_is_not_written() {
        let root = tempfile::tempdir().unwrap();
        let installed = bundle(&root.path().join("Applications"), "knobas.app", "old");
        let path = root.path().join("evil.tar.gz");
        {
            let file = std::fs::File::create(&path).unwrap();
            let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
                file,
                flate2::Compression::fast(),
            ));
            let mut header = tar::Header::new_old();
            let name = b"../escaped";
            header.as_old_mut().name[..name.len()].copy_from_slice(name);
            header.set_size(3);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append(&header, &b"bad"[..]).unwrap();
            let mut app = tar::Header::new_gnu();
            app.set_entry_type(tar::EntryType::Directory);
            app.set_mode(0o755);
            app.set_size(0);
            builder
                .append_data(&mut app, "knobas.app/", io::empty())
                .unwrap();
            builder.into_inner().unwrap().finish().unwrap();
        }
        let parent = root.path().join("stage");
        std::fs::create_dir_all(&parent).unwrap();
        let _ = swap_bundle_with(&path, &installed, &parent, exchange, |a, b| {
            std::fs::rename(a, b)
        });
        assert!(!parent.join("escaped").exists());
        assert!(!root.path().join("escaped").exists());
    }
}

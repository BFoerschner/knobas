//! An AppImage: the new file renamed over the old one (decisions D9).
//!
//! The private copy is written to `<the AppImage>.new` in the AppImage's own folder with the old
//! file's permissions, then renamed over the old file in one atomic step. `APPIMAGE` names it.

use std::path::Path;

use super::InstallError;

/// Put `staged` in place of the AppImage at `target`.
///
/// # Errors
///
/// [`InstallError::CannotReplace`]; the old file is as it was and no `.new` is left.
pub fn replace(staged: &Path, target: &Path) -> Result<(), InstallError> {
    let cannot = |why: String| InstallError::CannotReplace {
        path: target.to_path_buf(),
        why,
    };
    let permissions = std::fs::metadata(target)
        .map_err(|error| cannot(error.to_string()))?
        .permissions();
    let mut new = target.as_os_str().to_owned();
    new.push(".new");
    let new = std::path::PathBuf::from(new);
    let outcome = (|| {
        std::fs::copy(staged, &new)?;
        std::fs::set_permissions(&new, permissions)?;
        std::fs::File::open(&new)?.sync_all()?;
        std::fs::rename(&new, target)
    })();
    if let Err(error) = outcome {
        let _ = std::fs::remove_file(&new);
        return Err(cannot(error.to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt as _;

    use super::*;

    #[test]
    fn the_new_file_replaces_the_old_with_its_permissions() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("knobas.AppImage");
        std::fs::write(&target, "old").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o750)).unwrap();
        let staged = dir.path().join("staged");
        std::fs::write(&staged, "new").unwrap();
        std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o600)).unwrap();
        replace(&staged, &target).unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "new");
        assert_eq!(
            std::fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o750
        );
        assert!(!dir.path().join("knobas.AppImage.new").exists());
    }

    #[test]
    fn a_folder_that_cannot_be_written_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let apps = dir.path().join("apps");
        std::fs::create_dir(&apps).unwrap();
        let target = apps.join("knobas.AppImage");
        std::fs::write(&target, "old").unwrap();
        let staged = dir.path().join("staged");
        std::fs::write(&staged, "new").unwrap();
        std::fs::set_permissions(&apps, std::fs::Permissions::from_mode(0o555)).unwrap();
        let outcome = replace(&staged, &target);
        std::fs::set_permissions(&apps, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(matches!(outcome, Err(InstallError::CannotReplace { .. })));
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "old");
        assert!(!apps.join("knobas.AppImage.new").exists());
    }
}

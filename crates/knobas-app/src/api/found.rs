//! Whether the shipped Bruno entry's application is on this computer (ux31,
//! section 27). Not found, the entry is hidden (Björn, 2026-11-22). An
//! application a person added is never looked for: it is theirs.

use std::path::{Path, PathBuf};

/// Where Bruno is, under `home` and the platform's own places; `None` where it is not found.
#[must_use]
pub fn bruno(home: Option<&Path>) -> Option<PathBuf> {
    bruno_on(std::env::consts::OS, home, &Roots::system())
}

/// The places a lookup reads, so a test can give its own.
pub struct Roots {
    /// The folders of `PATH`.
    pub path: Vec<PathBuf>,
    /// The root the platform's absolute places hang off (`/` on a real system).
    pub root: PathBuf,
    /// Windows' `%LOCALAPPDATA%` and `%ProgramFiles%`.
    pub local_app_data: Option<PathBuf>,
    pub program_files: Option<PathBuf>,
}

impl Roots {
    fn system() -> Self {
        Self {
            path: std::env::var_os("PATH")
                .map(|p| std::env::split_paths(&p).collect())
                .unwrap_or_default(),
            root: PathBuf::from("/"),
            local_app_data: std::env::var_os("LOCALAPPDATA").map(PathBuf::from),
            program_files: std::env::var_os("ProgramFiles").map(PathBuf::from),
        }
    }
}

/// [`bruno`] for `os` over `roots`.
#[must_use]
pub fn bruno_on(os: &str, home: Option<&Path>, roots: &Roots) -> Option<PathBuf> {
    let at = |p: &str| roots.root.join(p.trim_start_matches('/'));
    let first = |list: Vec<PathBuf>| list.into_iter().find(|p| p.exists());
    match os {
        "macos" => first(
            [
                Some(at("/Applications/Bruno.app")),
                home.map(|h| h.join("Applications/Bruno.app")),
            ]
            .into_iter()
            .flatten()
            .collect(),
        ),
        "windows" => first(
            [
                roots
                    .local_app_data
                    .as_ref()
                    .map(|d| d.join("Programs/bruno/Bruno.exe")),
                roots
                    .program_files
                    .as_ref()
                    .map(|d| d.join("Bruno/Bruno.exe")),
            ]
            .into_iter()
            .flatten()
            .collect(),
        ),
        "linux" => {
            let mut list: Vec<PathBuf> = roots.path.iter().map(|d| d.join("bruno")).collect();
            list.push(at("/opt/Bruno/bruno"));
            list.push(at("/usr/share/applications/bruno.desktop"));
            list.push(at(
                "/var/lib/flatpak/exports/share/applications/com.usebruno.Bruno.desktop",
            ));
            list.push(at("/snap/bin/bruno"));
            if let Some(h) = home {
                list.push(h.join(".local/share/applications/bruno.desktop"));
                list.push(h.join(
                    ".local/share/flatpak/exports/share/applications/com.usebruno.Bruno.desktop",
                ));
            }
            first(list)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{Roots, bruno_on};

    #[test]
    fn bruno_is_found_where_each_platform_puts_it_and_not_elsewhere() {
        let dir = tempfile::tempdir().unwrap();
        let roots = Roots {
            path: vec![dir.path().join("bin")],
            root: dir.path().to_path_buf(),
            local_app_data: Some(dir.path().join("lad")),
            program_files: None,
        };
        assert!(bruno_on("macos", None, &roots).is_none());
        std::fs::create_dir_all(dir.path().join("Applications/Bruno.app")).unwrap();
        assert!(bruno_on("macos", None, &roots).is_some());
        assert!(bruno_on("linux", None, &roots).is_none());
        std::fs::create_dir_all(dir.path().join("opt/Bruno")).unwrap();
        std::fs::write(dir.path().join("opt/Bruno/bruno"), b"").unwrap();
        assert!(bruno_on("linux", None, &roots).is_some());
        assert!(bruno_on("windows", None, &roots).is_none());
        std::fs::create_dir_all(dir.path().join("lad/Programs/bruno")).unwrap();
        std::fs::write(dir.path().join("lad/Programs/bruno/Bruno.exe"), b"").unwrap();
        assert!(bruno_on("windows", None, &roots).is_some());
        assert!(bruno_on("haiku", None, &roots).is_none());
    }
}

//! The scratch folder an API client opens (ux31, section 26): in the profile,
//! `<profile>/api-clients/<name>-<8 hex>/`, one a spec file (the hex from the
//! repository and the path). It holds the spec as shown and, for Bruno, a
//! collection of requests.
//!
//! - **Written again when the spec's content changed** (Björn, 2026-11-22),
//!   compared by the SHA-256 of what knobas wrote, kept in `.knobas-spec`;
//!   else opened as it is, so what a person changed in Bruno stays.
//! - Written into `.tmp-…` and renamed, so a half-written folder is never
//!   opened. Regular files only, never the executable bit, never a `.git`.
//! - **Removed after [`KEEP_DAYS`] days unopened**, at start-up ([`sweep`]).
//!   Each open touches its marker.
//! - Not in a backup or a share export: neither reads the profile's folders.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use sha2::{Digest as _, Sha256};

/// How long a folder nobody opened is kept.
pub const KEEP_DAYS: u64 = 30;

/// [`KEEP_DAYS`] as the age [`sweep`] takes.
#[must_use]
pub fn keep() -> Duration {
    Duration::from_secs(KEEP_DAYS * 24 * 3600)
}
/// The most files and bytes knobas writes for one spec.
pub const MOST_FILES: usize = 20_000;
pub const MOST_BYTES: usize = 64 * 1024 * 1024;
/// The marker that holds what was written's hash.
pub const MARKER: &str = ".knobas-spec";

/// The folder holding every scratch folder.
#[must_use]
pub fn root(profile_dir: &Path) -> PathBuf {
    profile_dir.join("api-clients")
}

/// A name safe for a folder: letters, digits, `-` and `_`, at most 60.
fn tidy(name: &str) -> String {
    let mut out: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    while out.contains("--") {
        out = out.replace("--", "-");
    }
    let out = out.trim_matches('-').chars().take(60).collect::<String>();
    if out.is_empty() {
        "api".to_owned()
    } else {
        out
    }
}

/// The folder for the spec `key` (`repository\npath`), named after the API.
#[must_use]
pub fn folder_for(root: &Path, key: &str, name: &str) -> PathBuf {
    let hex = format!("{:x}", Sha256::digest(key.as_bytes()));
    root.join(format!("{}-{}", tidy(name), &hex[..8]))
}

/// The hash of what is written: every file's path and bytes, in order.
#[must_use]
pub fn hash_of(files: &[(String, Vec<u8>)]) -> String {
    let mut h = Sha256::new();
    for (path, bytes) in files {
        h.update(path.as_bytes());
        h.update([0]);
        h.update((bytes.len() as u64).to_le_bytes());
        h.update(bytes);
    }
    format!("{:x}", h.finalize())
}

/// Why a file was left out: a path that climbs out, names a `.git`, or a name a file system refuses.
fn refused(rel: &str) -> Option<String> {
    if let Err(why) = crate::folder_render::copy::check_path(rel) {
        return Some(why);
    }
    if rel
        .split('/')
        .any(|s| s.eq_ignore_ascii_case(".git") || s == MARKER || s.starts_with(".tmp-"))
    {
        return Some("names a folder knobas keeps out".to_owned());
    }
    None
}

/// What writing a folder did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Written {
    /// Whether the folder was (re)written; `false` where it was opened as it was.
    pub wrote: bool,
    /// Files left out, with why.
    pub left_out: Vec<(String, String)>,
}

/// Writes `files` into `folder` unless the folder already holds exactly them (by hash); touches its marker either way.
///
/// # Errors
/// Too many files or bytes, or the file system's.
pub fn write(folder: &Path, files: &[(String, Vec<u8>)]) -> Result<Written, String> {
    let mut kept = Vec::new();
    let mut left_out = Vec::new();
    for (rel, bytes) in files {
        match refused(rel) {
            Some(why) => left_out.push((rel.clone(), why)),
            None => kept.push((rel.clone(), bytes.clone())),
        }
    }
    if kept.len() > MOST_FILES {
        return Err(format!(
            "The spec makes {} files, more than the {MOST_FILES} knobas writes; nothing was written.",
            kept.len()
        ));
    }
    let size: usize = kept.iter().map(|(_, b)| b.len()).sum();
    if size > MOST_BYTES {
        return Err(format!(
            "The spec makes {} MB of files, more than the 64 MB knobas writes; nothing was written.",
            size / (1024 * 1024)
        ));
    }
    let hash = hash_of(&kept);
    let marker = folder.join(MARKER);
    if std::fs::read_to_string(&marker).is_ok_and(|held| held.trim() == hash) {
        touch(&marker);
        return Ok(Written {
            wrote: false,
            left_out,
        });
    }
    let parent = folder.parent().ok_or("the folder has no parent")?;
    std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    let tmp = parent.join(format!(
        ".tmp-{}-{}",
        std::process::id(),
        folder
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
    ));
    let _ = std::fs::remove_dir_all(&tmp);
    for (rel, bytes) in &kept {
        let path = tmp.join(rel);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    std::fs::write(tmp.join(MARKER), &hash).map_err(|e| format!("{e}"))?;
    // The spec changed (Björn): the folder is written again, whole, in place of the old.
    if folder.exists() {
        std::fs::remove_dir_all(folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    }
    std::fs::rename(&tmp, folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    Ok(Written {
        wrote: true,
        left_out,
    })
}

fn touch(path: &Path) {
    if let Ok(file) = std::fs::OpenOptions::new().write(true).open(path) {
        let _ = file.set_modified(SystemTime::now());
    }
}

/// Removes every scratch folder whose marker was last touched more than `max_age` ago, and any half-written one.
/// Answers how many went.
pub fn sweep(root: &Path, max_age: Duration) -> usize {
    let Ok(entries) = std::fs::read_dir(root) else {
        return 0;
    };
    let now = SystemTime::now();
    let mut gone = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let half = entry.file_name().to_string_lossy().starts_with(".tmp-");
        let age = std::fs::metadata(path.join(MARKER))
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| now.duration_since(t).ok());
        if (half || age.is_none_or(|a| a > max_age)) && std::fs::remove_dir_all(&path).is_ok() {
            gone += 1;
        }
    }
    gone
}

/// How many bytes the scratch folders hold.
#[must_use]
pub fn size(root: &Path) -> u64 {
    fn walk(p: &Path) -> u64 {
        let Ok(entries) = std::fs::read_dir(p) else {
            return 0;
        };
        entries
            .flatten()
            .map(|e| match e.metadata() {
                Ok(m) if m.is_dir() => walk(&e.path()),
                Ok(m) => m.len(),
                Err(_) => 0,
            })
            .sum()
    }
    walk(root)
}

/// Removes every scratch folder.
pub fn clear(root: &Path) {
    let _ = std::fs::remove_dir_all(root);
}

#[cfg(test)]
mod tests {
    use super::{MARKER, folder_for, keep, root, sweep, write};
    use std::time::Duration;

    fn files(text: &str) -> Vec<(String, Vec<u8>)> {
        vec![
            ("openapi.yaml".into(), text.as_bytes().to_vec()),
            ("Pets/List pets.bru".into(), b"meta {}".to_vec()),
        ]
    }

    #[test]
    fn the_same_spec_writes_once_and_a_changed_one_is_written_again() {
        let dir = tempfile::tempdir().unwrap();
        let folder = folder_for(&root(dir.path()), "repo\napi.yaml", "Pet Store");
        assert!(
            folder
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("Pet-Store-")
        );
        assert_eq!(
            folder,
            folder_for(&root(dir.path()), "repo\napi.yaml", "Pet Store")
        );
        assert_ne!(
            folder,
            folder_for(&root(dir.path()), "repo\nother.yaml", "Pet Store")
        );
        assert!(write(&folder, &files("a: 1")).unwrap().wrote);
        // A person's change in Bruno stays while the spec is the same.
        std::fs::write(folder.join("Pets/List pets.bru"), b"edited").unwrap();
        assert!(!write(&folder, &files("a: 1")).unwrap().wrote);
        assert_eq!(
            std::fs::read(folder.join("Pets/List pets.bru")).unwrap(),
            b"edited"
        );
        // The spec changed: written again.
        assert!(write(&folder, &files("a: 2")).unwrap().wrote);
        assert_eq!(std::fs::read(folder.join("openapi.yaml")).unwrap(), b"a: 2");
        assert_eq!(
            std::fs::read(folder.join("Pets/List pets.bru")).unwrap(),
            b"meta {}"
        );
    }

    #[test]
    fn a_name_that_climbs_out_or_names_a_git_folder_is_left_out_and_the_rest_written() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("api-clients/x");
        let mut f = files("a");
        f.push(("../escape.bru".into(), b"x".to_vec()));
        f.push((".git/config".into(), b"x".to_vec()));
        let written = write(&folder, &f).unwrap();
        assert_eq!(written.left_out.len(), 2);
        assert!(
            folder.join("openapi.yaml").exists()
                && !dir.path().join("api-clients/escape.bru").exists()
                && !folder.join(".git").exists()
        );
        let too_many: Vec<_> = (0..20_001)
            .map(|i| (format!("f{i}.bru"), Vec::new()))
            .collect();
        assert!(
            write(&dir.path().join("api-clients/y"), &too_many)
                .unwrap_err()
                .contains("20000")
        );
        assert!(!dir.path().join("api-clients/y").exists());
    }

    #[test]
    fn a_folder_older_than_the_limit_is_swept_and_a_younger_one_stays() {
        let dir = tempfile::tempdir().unwrap();
        let r = root(dir.path());
        let old = r.join("old-00000000");
        let young = r.join("young-00000000");
        write(&old, &files("o")).unwrap();
        write(&young, &files("y")).unwrap();
        let days_ago =
            |days: u64| std::time::SystemTime::now() - Duration::from_secs(days * 24 * 3600);
        let opened = |folder: &std::path::Path, at| {
            std::fs::OpenOptions::new()
                .write(true)
                .open(folder.join(MARKER))
                .unwrap()
                .set_modified(at)
                .unwrap();
        };
        // Past the 30 days: gone. A day short of them: kept.
        opened(&old, days_ago(31));
        opened(&young, days_ago(29));
        std::fs::create_dir_all(r.join(".tmp-1-half")).unwrap();
        assert_eq!(keep(), Duration::from_secs(30 * 24 * 3600));
        assert_eq!(sweep(&r, keep()), 2);
        assert!(!old.exists() && young.exists());
    }
}

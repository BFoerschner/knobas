//! Where a site keeps its session (decisions §5): a store of its own per
//! source, apart from knobas's own web view and from every other source's.
//!
//! - Windows: a WebView2 user data folder at `<profile dir>/sites/<source>`.
//! - macOS 14 and later: a `WKWebsiteDataStore` by an identifier made from
//!   the profile's keychain service and the source id.
//!
//! knobas never reads either. Forgetting removes it.

use std::fmt;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// The folder under a profile that holds every site's session.
pub const SITES_DIR: &str = "sites";
/// The list of session folders a forget could not delete while their
/// browser process still held them, deleted at the next start.
pub const FORGET_LIST: &str = "forget.list";

/// A source id a session cannot be kept under.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BadSourceId;

impl fmt::Display for BadSourceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("not a source id")
    }
}

impl std::error::Error for BadSourceId {}

/// A source id is lower-case ASCII letters, digits and `-`, starting with a
/// letter, at most 64 long (`knobas_source::instance::validate_instance_id`).
/// Checked again here so no id can name a path outside `sites/`.
fn check(source: &str) -> Result<(), BadSourceId> {
    let mut chars = source.chars();
    let first_ok = chars.next().is_some_and(|ch| ch.is_ascii_lowercase());
    let rest_ok = chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-');
    if first_ok && rest_ok && source.len() <= 64 {
        Ok(())
    } else {
        Err(BadSourceId)
    }
}

/// The session folder of `source`'s site in the profile at `profile_dir`.
///
/// # Errors
///
/// [`BadSourceId`] when `source` is not a source id.
pub fn folder(profile_dir: &Path, source: &str) -> Result<PathBuf, BadSourceId> {
    check(source)?;
    Ok(profile_dir.join(SITES_DIR).join(source))
}

/// The identifier of `source`'s `WKWebsiteDataStore`: the first 16 bytes of
/// SHA-256 over `knobas-site\n<keychain service>\n<source id>`, with the UUID
/// version (8, custom) and variant bits set. The keychain service differs
/// between the real and the demo profile, and between a release and a debug
/// build, so each has its own stores.
///
/// # Errors
///
/// [`BadSourceId`] when `source` is not a source id.
pub fn data_store_id(keychain_service: &str, source: &str) -> Result<[u8; 16], BadSourceId> {
    check(source)?;
    let mut hash = Sha256::new();
    hash.update(b"knobas-site\n");
    hash.update(keychain_service.as_bytes());
    hash.update(b"\n");
    hash.update(source.as_bytes());
    let digest = hash.finalize();
    let mut id = [0_u8; 16];
    id.copy_from_slice(&digest[..16]);
    id[6] = (id[6] & 0x0f) | 0x80;
    id[8] = (id[8] & 0x3f) | 0x80;
    Ok(id)
}

/// Add `source`'s folder to the forget list, for the next start to delete.
///
/// # Errors
///
/// The write's, or [`BadSourceId`] as an `InvalidInput`.
pub fn remember_to_forget(profile_dir: &Path, source: &str) -> std::io::Result<()> {
    check(source).map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
    let sites = profile_dir.join(SITES_DIR);
    std::fs::create_dir_all(&sites)?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(sites.join(FORGET_LIST))?;
    writeln!(file, "{source}")
}

/// Delete every session folder the forget list names, before any site is
/// made, and the list with them. A name that is not a source id is skipped.
/// Answers the folders that could not be deleted, which stay on the list.
#[must_use]
pub fn forget_listed(profile_dir: &Path) -> Vec<String> {
    let list = profile_dir.join(SITES_DIR).join(FORGET_LIST);
    let Ok(text) = std::fs::read_to_string(&list) else {
        return Vec::new();
    };
    let mut left = Vec::new();
    for source in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let Ok(dir) = folder(profile_dir, source) else {
            continue;
        };
        match std::fs::remove_dir_all(&dir) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => {
                if !left.iter().any(|kept: &String| kept == source) {
                    left.push(source.to_owned());
                }
            }
        }
    }
    if left.is_empty() {
        let _ = std::fs::remove_file(&list);
    } else {
        let _ = std::fs::write(&list, left.join("\n") + "\n");
    }
    left
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_is_under_sites_by_source() {
        let dir = folder(Path::new("/p"), "jira").unwrap();
        assert_eq!(dir, Path::new("/p/sites/jira"));
        assert_ne!(dir, folder(Path::new("/p"), "wiki").unwrap());
        assert_ne!(dir, folder(Path::new("/p/demo"), "jira").unwrap());
    }

    #[test]
    fn no_id_leaves_sites() {
        for bad in [
            "..",
            "../x",
            "a/b",
            "a\\b",
            "C:",
            "c:\\x",
            "",
            "Jira",
            "-x",
            "a b",
            "a.b",
            &"a".repeat(65),
        ] {
            assert_eq!(folder(Path::new("/p"), bad), Err(BadSourceId), "{bad:?}");
            assert_eq!(data_store_id("svc", bad), Err(BadSourceId), "{bad:?}");
        }
    }

    #[test]
    fn a_store_id_is_stable_and_differs_by_source_and_profile() {
        let real = data_store_id("dev.knobas.desktop", "jira").unwrap();
        assert_eq!(real, data_store_id("dev.knobas.desktop", "jira").unwrap());
        assert_ne!(real, data_store_id("dev.knobas.desktop", "wiki").unwrap());
        assert_ne!(
            real,
            data_store_id("dev.knobas.desktop.demo", "jira").unwrap()
        );
        assert_ne!(
            real,
            data_store_id("dev.knobas.desktop.dev", "jira").unwrap()
        );
        assert_eq!(real[6] >> 4, 8, "version 8");
        assert_eq!(real[8] >> 6, 0b10, "the RFC 4122 variant");
        // Pinned: a change here loses every person's sign-ins.
        assert_eq!(
            real,
            [
                0xf0, 0xf8, 0xab, 0xf2, 0x3a, 0xb6, 0x8c, 0xc5, 0x84, 0x5e, 0x61, 0x0e, 0x83, 0x8e,
                0x4b, 0x66
            ],
        );
    }

    #[test]
    fn the_forget_list_deletes_at_the_next_start() {
        let root = std::env::temp_dir().join(format!("knobas-site-forget-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let jira = folder(&root, "jira").unwrap();
        std::fs::create_dir_all(jira.join("EBWebView")).unwrap();
        std::fs::write(jira.join("EBWebView/Cookies"), b"x").unwrap();
        let wiki = folder(&root, "wiki").unwrap();
        std::fs::create_dir_all(&wiki).unwrap();
        remember_to_forget(&root, "jira").unwrap();
        // A line no forget wrote is skipped, never followed.
        let list = root.join(SITES_DIR).join(FORGET_LIST);
        let mut text = std::fs::read_to_string(&list).unwrap();
        text.push_str("../../outside\n");
        std::fs::write(&list, text).unwrap();
        assert!(forget_listed(&root).is_empty());
        assert!(!jira.exists());
        assert!(wiki.exists(), "only what the list names");
        assert!(!list.exists());
        assert!(forget_listed(&root).is_empty(), "no list, nothing to do");
        std::fs::remove_dir_all(&root).unwrap();
    }
}

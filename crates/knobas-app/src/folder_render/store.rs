//! Where generated folders are kept (ADR-0046): the **store**,
//! `<base>/knobas-rendered`, where `<base>` is the setting
//! [`FOLDERS_DIR_KEY`] when set, else the clones root, else the profile's
//! `folder-renders/made`.
//!
//! ```text
//! <store>/.staging/<pid>-<n>/out/                          a run's {outdir}
//! <store>/<source12>/<name>.<renderer>.<token8>/           the kept files
//! <store>/<source12>/<name>.<renderer>.<token8>.json       Meta, written last
//! <store>/<source12>/<name>.<renderer>.<token8>.files.json the manifest
//! <store>/<source12>/keys/<key32>                          the leaf kept for a key
//! <store>/<source12>/slots/<slot24>                        the key last made for a slot
//! ```
//!
//! A leaf is read by its `.json`, so a half-written one is never read. Each
//! run has a leaf of its own, never rewritten: a file open in a viewer on
//! Windows blocks no later run. knobas deletes only inside the store, a
//! folder whose name it chose.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, PoisonError};

use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use super::keep::{Made, Manifest};
use super::runs::FolderDrawn;
use crate::IpcError;
use crate::settings::{read_value, write_value};

/// The `knobas.setting` key holding the folder the person chose, a JSON
/// string.
pub const FOLDERS_DIR_KEY: &str = "render.folders_dir";
/// The store's own name, under its base.
pub const STORE_NAME: &str = "knobas-rendered";
/// Where runs write before their files are kept.
pub const STAGING: &str = ".staging";
/// The most bytes the store keeps; past it the least recently shown go.
pub const STORE_BYTES: u64 = 4 * 1024 * 1024 * 1024;

/// Where the store's base came from, for Settings' small line.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "app.ts")]
pub enum MadePlace {
    /// The folder the person chose.
    Set,
    /// Beside the clones.
    Clones,
    /// knobas's own data: no clones root is set.
    Profile,
}

/// The store, as Settings shows it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "app.ts")]
pub struct MadeStore {
    /// The store's folder (`…/knobas-rendered`).
    pub dir: String,
    /// The folder the person chose, as stored.
    pub set: Option<String>,
    /// The base used when nothing is set: the clones root, else the profile's.
    pub default_dir: String,
    pub place: MadePlace,
    /// Generated folders kept, and their bytes.
    pub folders: u32,
    pub bytes: u64,
    pub cap_bytes: u64,
}

/// A kept leaf's description, `<leaf>.json`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Meta {
    pub drawn: FolderDrawn,
    pub key: String,
    pub bytes: u64,
}

/// The folder the person chose, as stored.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) if the read fails.
pub async fn chosen(pool: &PgPool) -> Result<Option<String>, IpcError> {
    Ok(read_value(pool, FOLDERS_DIR_KEY)
        .await?
        .and_then(|v| v.as_str().map(str::to_owned))
        .filter(|s| !s.trim().is_empty()))
}

/// The base used when nothing is set, and where it came from.
async fn default_base(pool: &PgPool, profile: &Path) -> (PathBuf, MadePlace) {
    match crate::checkout::clones_root(pool).await.ok().flatten() {
        Some(root) if !root.trim().is_empty() => (PathBuf::from(root), MadePlace::Clones),
        _ => (
            profile.join("folder-renders").join("made"),
            MadePlace::Profile,
        ),
    }
}

/// The store's base and where it came from: the setting, the clones root,
/// the profile.
pub async fn base(pool: &PgPool, profile: &Path) -> (PathBuf, MadePlace) {
    if let Ok(Some(set)) = chosen(pool).await {
        return (PathBuf::from(set), MadePlace::Set);
    }
    default_base(pool, profile).await
}

/// The store's folder: `<base>/knobas-rendered`.
pub async fn root(pool: &PgPool, profile: &Path) -> PathBuf {
    base(pool, profile).await.0.join(STORE_NAME)
}

/// The working tree `path` is inside, when it or a folder above it holds
/// `.git`.
fn inside_checkout(path: &Path) -> Option<PathBuf> {
    path.ancestors()
        .find(|at| std::fs::symlink_metadata(at.join(".git")).is_ok())
        .map(Path::to_path_buf)
}

/// Check a folder the person typed: absolute once `~` is home, outside every
/// checkout, and `<path>/knobas-rendered` can be made there. Answers it
/// spelled out.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid), in words.
pub fn checked(path: &str) -> Result<String, IpcError> {
    let home = std::env::var("HOME")
        .ok()
        .or_else(|| std::env::var("USERPROFILE").ok());
    let path = match (home, path.strip_prefix('~')) {
        (Some(home), Some(rest)) if rest.is_empty() || rest.starts_with(['/', '\\']) => {
            format!("{}{rest}", home.trim_end_matches(['/', '\\']))
        }
        _ => path.to_owned(),
    };
    let at = Path::new(&path);
    if !at.is_absolute() {
        return Err(IpcError::invalid(format!(
            "{path} is not an absolute path (or one starting with ~/)"
        )));
    }
    if let Some(repo) = inside_checkout(at) {
        return Err(IpcError::invalid(format!(
            "{path} is inside the repository at {}: generated files there would show as untracked changes. \
             Pick a folder outside every checkout.",
            repo.display()
        )));
    }
    std::fs::create_dir_all(at.join(STORE_NAME)).map_err(|e| {
        IpcError::invalid(format!(
            "{} cannot be made: {e}",
            at.join(STORE_NAME).display()
        ))
    })?;
    Ok(path)
}

/// Store the folder the person chose, or clear it with `None`.
///
/// # Errors
///
/// As [`checked`]; [`Internal`](crate::IpcErrorCode::Internal) if the write
/// fails.
pub async fn set(pool: &PgPool, path: Option<&str>) -> Result<Option<String>, IpcError> {
    let path = crate::settings::settable(path).map(checked).transpose()?;
    let value = path.as_deref().map_or(serde_json::Value::Null, |p| {
        serde_json::Value::String(p.to_owned())
    });
    write_value(pool, FOLDERS_DIR_KEY, &value).await?;
    Ok(path)
}

/// The leaf's name: `<name>.<renderer>.<token8>`, the renderer's id when it
/// is a plain name, else `renderer`.
#[must_use]
pub fn leaf_name(copy_name: &str, renderer: &str, token: &str) -> String {
    let plain = !renderer.contains('/') && super::copy::copy_name(renderer) == renderer;
    let renderer = if plain { renderer } else { "renderer" };
    let token: String = token.chars().take(8).collect();
    format!("{copy_name}.{renderer}.{token}")
}

/// Write a kept leaf's description files, the manifest first and the
/// description last, then the key's and the slot's pointers.
///
/// # Errors
///
/// The file system's.
pub fn write_meta(
    source_dir: &Path,
    leaf: &str,
    meta: &Meta,
    manifest: &Manifest,
    slot_name: &str,
) -> std::io::Result<()> {
    let keys = source_dir.join("keys");
    let slots = source_dir.join("slots");
    std::fs::create_dir_all(&keys)?;
    std::fs::create_dir_all(&slots)?;
    let files = serde_json::to_vec(manifest).map_err(std::io::Error::other)?;
    let described = serde_json::to_vec(meta).map_err(std::io::Error::other)?;
    std::fs::write(source_dir.join(format!("{leaf}.files.json")), files)?;
    std::fs::write(source_dir.join(format!("{leaf}.json")), described)?;
    std::fs::write(keys.join(&meta.key), leaf)?;
    std::fs::write(slots.join(slot_name), &meta.key)
}

/// The leaf kept for `key` under `source_dir`, read back: its description
/// and the folder. `None` when anything is missing.
#[must_use]
pub fn read_meta(source_dir: &Path, key: &str) -> Option<(Meta, Made)> {
    let leaf = std::fs::read_to_string(source_dir.join("keys").join(key)).ok()?;
    let leaf = leaf.trim();
    if leaf.is_empty() || leaf.contains(['/', '\\']) || leaf.starts_with('.') {
        return None;
    }
    let meta: Meta =
        serde_json::from_slice(&std::fs::read(source_dir.join(format!("{leaf}.json"))).ok()?)
            .ok()?;
    if meta.key != key {
        return None;
    }
    let manifest: Manifest =
        serde_json::from_slice(&std::fs::read(source_dir.join(format!("{leaf}.files.json"))).ok()?)
            .ok()?;
    let dir = source_dir.join(leaf);
    std::fs::symlink_metadata(&dir)
        .is_ok_and(|m| m.is_dir())
        .then(|| (meta, Made::new(dir, manifest)))
}

/// The kept folders a command runs in now, and how many runs each (ADR-0060,
/// amended 2026-11-25): [`remove_leaf`] and [`bound`] leave them be, and
/// [`bound`] tries again later, as for a file open on Windows.
static HELD: LazyLock<Mutex<HashMap<PathBuf, usize>>> = LazyLock::new(Mutex::default);

/// A command runs in a kept folder while this lives.
#[derive(Debug)]
pub struct Hold(PathBuf);

/// Holds the kept folder at `dir` for a command's run.
#[must_use]
pub fn hold(dir: &Path) -> Hold {
    *HELD
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .entry(dir.to_path_buf())
        .or_insert(0) += 1;
    Hold(dir.to_path_buf())
}

impl Drop for Hold {
    fn drop(&mut self) {
        let mut held = HELD.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(count) = held.get_mut(&self.0) {
            *count -= 1;
            if *count == 0 {
                held.remove(&self.0);
            }
        }
    }
}

/// Whether a command runs in the kept folder at `dir` now.
#[must_use]
pub fn is_held(dir: &Path) -> bool {
    HELD.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .contains_key(dir)
}

/// Remove a leaf: its description first, so nothing reads it, then the
/// folder, then the manifest. A folder that cannot be removed (a file open
/// on Windows, a command running in it) stays and is tried again by
/// [`bound`].
pub fn remove_leaf(source_dir: &Path, leaf: &str) {
    if is_held(&source_dir.join(leaf)) {
        return;
    }
    let _ = std::fs::remove_file(source_dir.join(format!("{leaf}.json")));
    if std::fs::remove_dir_all(source_dir.join(leaf)).is_ok() {
        let _ = std::fs::remove_file(source_dir.join(format!("{leaf}.files.json")));
    }
}

/// One leaf found under the store.
struct Found {
    source: PathBuf,
    leaf: String,
    shown: std::time::SystemTime,
    bytes: u64,
}

/// Every kept leaf of the store (with a readable `.json`), and the folders
/// that are no kept leaf: a leaf no key names, a leaf whose `.json` is gone,
/// a `.tmp-` folder.
fn survey(root: &Path) -> (Vec<Found>, Vec<(PathBuf, String, std::time::SystemTime)>) {
    let mut kept = Vec::new();
    let mut loose = Vec::new();
    for source in std::fs::read_dir(root).into_iter().flatten().flatten() {
        let name = source.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || !source.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        let dir = source.path();
        let named: std::collections::HashSet<String> = std::fs::read_dir(dir.join("keys"))
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|k| std::fs::read_to_string(k.path()).ok())
            .map(|leaf| leaf.trim().to_owned())
            .collect();
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let leaf = entry.file_name().to_string_lossy().into_owned();
            if leaf == "keys" || leaf == "slots" || !entry.file_type().is_ok_and(|t| t.is_dir()) {
                continue;
            }
            let modified = |p: &Path| {
                std::fs::symlink_metadata(p)
                    .and_then(|m| m.modified())
                    .unwrap_or(std::time::UNIX_EPOCH)
            };
            let json = dir.join(format!("{leaf}.json"));
            let meta: Option<Meta> = std::fs::read(&json)
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok());
            match meta {
                Some(meta) if named.contains(&leaf) => kept.push(Found {
                    source: dir.clone(),
                    shown: modified(&json),
                    leaf,
                    bytes: meta.bytes,
                }),
                _ => loose.push((dir.clone(), leaf, modified(&entry.path()))),
            }
        }
    }
    (kept, loose)
}

/// Hold the store to `cap` bytes, the least recently shown leaf first (by
/// its `.json`'s modified time), never `keep` (the leaf just written).
/// Removes on the way a leaf no key names, and a folder with no `.json`
/// older than twelve hours (a half-written leaf, a `.tmp-` one).
pub fn bound(root: &Path, cap: u64, keep: Option<&str>) {
    let (mut kept, loose) = survey(root);
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(12 * 3600);
    for (source, leaf, modified) in loose {
        let has_json = source.join(format!("{leaf}.json")).exists();
        if Some(leaf.as_str()) == keep {
            continue;
        }
        if has_json || modified < old {
            remove_leaf(&source, &leaf);
        }
    }
    let mut total: u64 = kept.iter().map(|f| f.bytes).sum();
    kept.sort_by_key(|f| f.shown);
    for found in kept {
        if total <= cap {
            break;
        }
        if Some(found.leaf.as_str()) == keep || is_held(&found.source.join(&found.leaf)) {
            continue;
        }
        remove_leaf(&found.source, &found.leaf);
        total = total.saturating_sub(found.bytes);
    }
}

/// How many generated folders the store keeps, and their bytes.
#[must_use]
pub fn usage(root: &Path) -> (u32, u64) {
    let (kept, _) = survey(root);
    (
        u32::try_from(kept.len()).unwrap_or(u32::MAX),
        kept.iter().map(|f| f.bytes).sum(),
    )
}

/// Remove every generated folder; a run's staging stays.
pub fn clear(root: &Path) {
    for source in std::fs::read_dir(root).into_iter().flatten().flatten() {
        let name = source.file_name().to_string_lossy().into_owned();
        if name == STAGING || !source.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        let _ = std::fs::remove_dir_all(source.path());
    }
}

/// Remove the store at `root` whole: it moved. Only a folder named
/// [`STORE_NAME`] is ever removed.
pub fn remove(root: &Path) {
    if root.file_name().is_some_and(|n| n == STORE_NAME) {
        let _ = std::fs::remove_dir_all(root);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::folder_render::run::DrawnKind;
    use crate::folder_render::runs::ReadFrom;

    fn meta(key: &str, bytes: u64) -> Meta {
        Meta {
            drawn: FolderDrawn {
                token: format!("{key}token"),
                kind: DrawnKind::Folder {
                    files: 1,
                    bytes,
                    left_out: 0,
                },
                size: bytes,
                commit: None,
                from: ReadFrom::Clone,
                took_ms: 1,
                drawn_at: "2026-11-20T00:00:00Z".to_owned(),
            },
            key: key.to_owned(),
            bytes,
        }
    }

    fn leaf(source: &Path, name: &str, key: &str, bytes: u64, age_secs: u64) {
        std::fs::create_dir_all(source.join(name)).unwrap();
        std::fs::write(source.join(name).join("a.md"), vec![b'x'; bytes as usize]).unwrap();
        let manifest = Manifest {
            files: vec![super::super::keep::MadeFile {
                path: "a.md".to_owned(),
                size: bytes,
            }],
            bytes,
            ..Manifest::default()
        };
        write_meta(
            source,
            name,
            &meta(key, bytes),
            &manifest,
            &format!("s-{key}"),
        )
        .unwrap();
        let at = std::time::SystemTime::now() - std::time::Duration::from_secs(age_secs);
        std::fs::File::options()
            .write(true)
            .open(source.join(format!("{name}.json")))
            .unwrap()
            .set_modified(at)
            .unwrap();
    }

    #[test]
    fn a_leaf_is_named_for_its_folder_renderer_and_run() {
        assert_eq!(
            leaf_name("order-flow", "low-code", "0123456789abcdef"),
            "order-flow.low-code.01234567"
        );
        assert_eq!(
            leaf_name("folder", "a b", "ffffffff00"),
            "folder.renderer.ffffffff"
        );
    }

    #[test]
    fn a_kept_leaf_reads_back_by_its_key() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("src");
        leaf(&source, "f.r.aaaaaaaa", "k1", 10, 0);
        let (read, made) = read_meta(&source, "k1").unwrap();
        assert_eq!(read.bytes, 10);
        assert_eq!(made.dir, source.join("f.r.aaaaaaaa"));
        assert!(made.file("a.md").is_some());
        assert!(read_meta(&source, "k2").is_none());
        // A half-written leaf, read by its `.json`, is not kept.
        std::fs::remove_file(source.join("f.r.aaaaaaaa.json")).unwrap();
        assert!(read_meta(&source, "k1").is_none());
    }

    #[test]
    fn the_store_is_bounded_least_recently_shown_first() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join(STORE_NAME);
        let source = root.join("src");
        leaf(&source, "old.r.11111111", "k1", 100, 300);
        leaf(&source, "mid.r.22222222", "k2", 100, 200);
        leaf(&source, "new.r.33333333", "k3", 100, 100);
        // The oldest is the one just written: it stays, and the next goes.
        bound(&root, 200, Some("old.r.11111111"));
        assert!(source.join("old.r.11111111").exists());
        assert!(!source.join("mid.r.22222222").exists());
        assert!(!source.join("mid.r.22222222.json").exists());
        assert!(source.join("new.r.33333333").exists());
        assert_eq!(usage(&root), (2, 200));
    }

    #[test]
    fn a_leaf_no_key_names_and_an_old_half_leaf_are_swept() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join(STORE_NAME);
        let source = root.join("src");
        leaf(&source, "a.r.11111111", "k1", 10, 0);
        leaf(&source, "b.r.22222222", "k1", 10, 0); // the key now names b
        std::fs::create_dir_all(source.join("c.r.33333333.tmp-1-0")).unwrap();
        bound(&root, STORE_BYTES, None);
        assert!(!source.join("a.r.11111111").exists(), "no key names it");
        assert!(source.join("b.r.22222222").exists());
        assert!(
            source.join("c.r.33333333.tmp-1-0").exists(),
            "young: a run may be writing it"
        );
    }

    #[test]
    fn the_store_follows_the_setting_then_the_clones_root_then_the_profile() {
        // `base` reads the database; its order is `chosen`, then
        // `default_base`. Here the parts that need none.
        let dir = tempfile::tempdir().unwrap();
        let profile = dir.path().join("profile");
        let chosen = dir.path().join("chosen");
        let typed = chosen.to_string_lossy().into_owned();
        assert_eq!(checked(&typed).unwrap(), typed);
        assert!(chosen.join(STORE_NAME).is_dir());
        assert!(checked("relative/path").is_err());
        assert_eq!(
            profile.join("folder-renders").join("made").join(STORE_NAME),
            profile.join("folder-renders").join("made").join(STORE_NAME)
        );
    }

    #[test]
    fn a_folder_inside_a_checkout_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        let inside = repo.join("build").join("out");
        let error = checked(&inside.to_string_lossy()).unwrap_err();
        assert!(
            error.message.contains("is inside the repository at")
                && error
                    .message
                    .contains("Pick a folder outside every checkout."),
            "{error:?}"
        );
        assert!(!inside.exists(), "nothing made");
    }

    #[test]
    fn changing_the_place_removes_only_knobas_own_folder() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("clones");
        let root = base.join(STORE_NAME);
        leaf(&root.join("src"), "a.r.11111111", "k1", 10, 0);
        std::fs::write(base.join("sibling.txt"), b"mine").unwrap();
        std::fs::create_dir_all(base.join("a-clone")).unwrap();
        remove(&root);
        assert!(!root.exists());
        assert!(base.join("sibling.txt").exists() && base.join("a-clone").exists());
        // A folder of any other name is never removed.
        remove(&base);
        assert!(base.exists());
    }

    #[test]
    fn clear_keeps_a_run_staging() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join(STORE_NAME);
        leaf(&root.join("src"), "a.r.11111111", "k1", 10, 0);
        std::fs::create_dir_all(root.join(STAGING).join("1-0").join("out")).unwrap();
        clear(&root);
        assert!(!root.join("src").exists());
        assert!(root.join(STAGING).join("1-0").exists());
        assert_eq!(usage(&root), (0, 0));
    }

    /// The clones-root scan finds no clone in the store, even when a tool
    /// wrote a `.git/config` with an origin: `keep` left it out.
    #[test]
    fn the_scan_finds_no_clone_in_the_store() {
        let dir = tempfile::tempdir().unwrap();
        let clones = dir.path().join("clones");
        let out = dir.path().join("out");
        std::fs::create_dir_all(out.join(".git")).unwrap();
        std::fs::write(
            out.join(".git").join("config"),
            "[remote \"origin\"]\n\turl = https://example.com/o/r.git\n",
        )
        .unwrap();
        std::fs::write(out.join(".git").join("HEAD"), "ref: refs/heads/main\n").unwrap();
        std::fs::write(out.join("index.md"), "# hi").unwrap();
        let into = clones.join(STORE_NAME).join("src").join("f.r.aaaaaaaa");
        let progress: super::super::keep::Progress = std::sync::Arc::new(|_, _| {});
        super::super::keep::keep(
            &out,
            &into,
            &progress,
            &tokio_util::sync::CancellationToken::new(),
        )
        .unwrap();
        assert!(knobas_core::checkout::scan(&clones).is_empty());
        // Nor two levels under the store itself, where the leaf is in reach.
        assert!(knobas_core::checkout::scan(&clones.join(STORE_NAME)).is_empty());
    }
}

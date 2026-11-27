//! *Try it on* (#2562): a renderer, a pipeline, a folder renderer, a command
//! or an *Open in…* application tried from its settings page on the person's
//! own example: text they pasted under a file name, or a file or a folder they
//! chose anywhere on this disk.
//!
//! The rules are the ones each kind already runs under (ADR-0016, ADR-0033,
//! ADR-0044, ADR-0046, ADR-0060), with the example in the checkout's place:
//!
//! - **Only a saved one runs.** The webview names a saved renderer, pipeline
//!   step, command or application by its id, never a program or an argument.
//! - **What is tried on was picked or pasted by the person.** A path reaches
//!   knobas from the native picker on their press and is read on the press
//!   that tries it, never before and never again by itself. It must be
//!   absolute and name a file or a folder; nothing beside it is read, except
//!   what a folder renderer is given of a chosen folder (its files, as a
//!   folder of a working tree is given) and what a command or an application
//!   opens by itself in the folder it runs in, as in a checkout.
//! - **Nothing is kept.** A try writes nothing to the database, the mirror,
//!   the folder renders' disk cache or the generated-folders store: a folder
//!   renderer's try runs in [`runs`], a registry of its own whose drawings
//!   stay in memory and whose generated folders go under [`root`]. Pasted
//!   text a command or an application needs as a file is written there too,
//!   as a throwaway file, removed when the command ends or the next
//!   application is tried. Each knobas has its own folder there, which
//!   [`sweep`] removes at quit; [`sweep_left`] removes, at bring-up, what an
//!   earlier session could not. A command's try is no row in the list of
//!   runs.
//! - **The sizes are Code's.** A file is read up to the 50 MiB a renderer is
//!   handed in Code (`view::BYTES_CAP`), pasted text the same; a folder up to
//!   what a folder renderer copies from a working tree.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex, PoisonError};

use serde::{Deserialize, Serialize};

use crate::IpcError;
use crate::folder_render::FolderRenderer;
use crate::folder_render::copy::{self, Item, ItemKind};
use crate::folder_render::runs::{self, FolderFailure, ReadFrom, Resolved, Runs, Slot, Unresolved};

/// The entity a try's runs are filed under: no repository has it.
pub const TRY_ENTITY: &str = "~try";

/// The lane a folder renderer's try runs in.
pub const TRY_LANE: &str = "try";

/// The most bytes of a file or of pasted text a try reads: what Code hands a renderer.
pub const BYTES_CAP: u64 = knobas_source::view::BYTES_CAP;

/// What a try is run on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export_to = "app.ts")]
pub enum TryOn {
    /// Text the person pasted, under the file name they gave it: its ending
    /// picks the renderers that take it, as a file's does in Code.
    Text { name: String, text: String },
    /// A file or a folder the person chose with the native picker.
    Path { path: String },
}

/// The folder of the system's temp folder every knobas's tries go under, one
/// folder per process in it ([`root`]).
fn tries() -> PathBuf {
    std::env::temp_dir().join("knobas-try")
}

/// Where this knobas's throwaway files and tried generated folders go: its
/// own folder under the system temp folder's `knobas-try/`, never the
/// profile. Its own, because another knobas (the other profile, a demo, a
/// headless one a check started) clears what its session left without
/// touching this one's.
#[must_use]
pub fn root() -> PathBuf {
    tries().join(std::process::id().to_string())
}

/// Remove what this knobas's tries left (quit): nothing under [`root`] is
/// meant to outlive the session.
pub fn sweep() {
    let _ = std::fs::remove_dir_all(root());
}

/// At bring-up: [`sweep`], for a process number used before, and what an
/// earlier session left that it could not remove (a crash), once untouched
/// for twelve hours, as the folder renders' copies are swept
/// ([`copy::sweep`]): a knobas still running whose tries have not touched
/// its folder for that long may lose what they left, as a copy would.
pub fn sweep_left() {
    sweep();
    copy::sweep(&tries());
}

/// The folder renderers' registry for tries: copies under the same root as
/// every render's, generated folders under [`root`], nothing on disk across
/// a restart, and no store a person counts in Settings.
#[must_use]
pub fn runs() -> Arc<Runs> {
    static TRIED: LazyLock<Arc<Runs>> = LazyLock::new(|| {
        let runs = Runs::new(copy::root());
        runs.set_store(root().join("made"));
        Arc::new(runs)
    });
    Arc::clone(&TRIED)
}

/// The registry a slot of `entity_id` runs in.
#[must_use]
pub fn runs_of_entity(entity_id: &str) -> Arc<Runs> {
    if entity_id == TRY_ENTITY {
        runs()
    } else {
        Runs::global()
    }
}

/// `name` as a file name pasted text can be written under: one plain name,
/// as a copy of a folder takes it.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid), saying why.
pub fn checked_name(name: &str) -> Result<String, IpcError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(IpcError::invalid(
            "Give the text a file name, such as example.md.",
        ));
    }
    if name.contains('/') || name.len() > 255 {
        return Err(IpcError::invalid(format!("{name} is not one file name")));
    }
    copy::check_path(name).map_err(|why| IpcError::invalid(format!("{name} {why}")))?;
    Ok(name.to_owned())
}

/// A chosen path, checked: absolute, there, a file or a folder. Answers it
/// canonical, and whether it is a folder.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) for a relative path or one that
/// is neither; [`NotFound`](crate::IpcErrorCode::NotFound) for one that is gone.
pub async fn checked_path(path: &str) -> Result<(PathBuf, bool), IpcError> {
    let given = PathBuf::from(path.trim());
    if !given.is_absolute() {
        return Err(IpcError::invalid(format!(
            "{path} is not a path from the top of the disk"
        )));
    }
    let canonical = tokio::fs::canonicalize(&given)
        .await
        .map_err(|_| IpcError::not_found(format!("{} is no longer there", given.display())))?;
    let meta = tokio::fs::metadata(&canonical)
        .await
        .map_err(|error| IpcError::not_found(format!("{}: {error}", canonical.display())))?;
    if meta.is_dir() {
        Ok((canonical, true))
    } else if meta.is_file() {
        Ok((canonical, false))
    } else {
        Err(IpcError::invalid(format!(
            "{} is neither a file nor a folder",
            canonical.display()
        )))
    }
}

/// Why `size` bytes are not read, past [`BYTES_CAP`].
fn too_large(what: &str, size: u64) -> IpcError {
    IpcError::invalid(format!(
        "{what} is {} MB, past the {} MB knobas hands a renderer",
        size.div_ceil(1024 * 1024),
        BYTES_CAP / (1024 * 1024)
    ))
}

/// A chosen file's bytes, whole, refused past [`BYTES_CAP`].
///
/// # Errors
///
/// As [`checked_path`]; a folder, a file past the cap, or a read that fails.
pub async fn file_bytes(path: &str) -> Result<(String, Vec<u8>), IpcError> {
    let (file, folder) = checked_path(path).await?;
    if folder {
        return Err(IpcError::invalid(format!(
            "{} is a folder: choose a file",
            file.display()
        )));
    }
    let size = tokio::fs::metadata(&file)
        .await
        .map(|m| m.len())
        .unwrap_or(0);
    let name = leaf(&file);
    if size > BYTES_CAP {
        return Err(too_large(&name, size));
    }
    let bytes = tokio::fs::read(&file)
        .await
        .map_err(|error| IpcError::internal(format!("{}: {error}", file.display())))?;
    if bytes.len() as u64 > BYTES_CAP {
        return Err(too_large(&name, bytes.len() as u64));
    }
    Ok((name, bytes))
}

/// The name and bytes `on` stands for: pasted text as UTF-8, a chosen file read
/// now. A folder is refused: a file renderer and a pipeline take a file.
///
/// # Errors
///
/// As [`checked_name`] and [`file_bytes`].
pub async fn bytes_of(on: &TryOn) -> Result<(String, Vec<u8>), IpcError> {
    match on {
        TryOn::Text { name, text } => {
            let name = checked_name(name)?;
            if text.len() as u64 > BYTES_CAP {
                return Err(too_large("The text", text.len() as u64));
            }
            Ok((name, text.clone().into_bytes()))
        }
        TryOn::Path { path } => file_bytes(path).await,
    }
}

/// The last name of `path`, as a person reads it.
#[must_use]
pub fn leaf(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

// -- throwaway files -----------------------------------------------------------

/// A folder under [`root`] holding one file of pasted text: what a command or
/// an application is run on when the person pasted their example.
#[derive(Debug)]
pub struct Throwaway {
    /// The folder, which goes with the file.
    pub dir: PathBuf,
    /// The file, `name` in `dir`.
    pub file: PathBuf,
}

impl Throwaway {
    /// Write `text` as `name` in a fresh folder for `kind` (`command`, `open`).
    ///
    /// # Errors
    ///
    /// As [`checked_name`]; text past [`BYTES_CAP`]; the file system's.
    pub fn write(kind: &str, name: &str, text: &str) -> Result<Self, IpcError> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let name = checked_name(name)?;
        if text.len() as u64 > BYTES_CAP {
            return Err(too_large("The text", text.len() as u64));
        }
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = root().join(format!("{kind}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir)
            .map_err(|error| IpcError::internal(format!("{}: {error}", dir.display())))?;
        let file = dir.join(&name);
        std::fs::write(&file, text).map_err(|error| {
            let _ = std::fs::remove_dir_all(&dir);
            IpcError::internal(format!("{}: {error}", file.display()))
        })?;
        Ok(Self { dir, file })
    }

    /// Remove the folder and the file.
    pub fn remove(self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// The throwaway folder of the last application tried on pasted text: kept
/// while the application may still have it open, removed when the next one is
/// written (and by [`sweep`]).
static OPENED: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Hold `dir` as the last application's throwaway folder, removing the one
/// before it.
pub fn opened(dir: PathBuf) {
    let before = OPENED
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .replace(dir);
    if let Some(before) = before {
        let _ = std::fs::remove_dir_all(before);
    }
}

// -- folder renderers ----------------------------------------------------------

/// The slot a renderer's try runs in: one per renderer, so a new example stops
/// the old one's run.
#[must_use]
pub fn slot(renderer: &str) -> Slot {
    crate::folder_render::slot(TRY_ENTITY, renderer, "", TRY_LANE)
}

/// What a folder renderer is given of `on`: pasted text or a chosen file as a
/// folder holding only it, a chosen folder as it is on disk (`.git` folders
/// left out, links as the files holding their target, as in a working tree).
pub async fn resolve(renderer: &FolderRenderer, on: TryOn) -> Result<Resolved, Unresolved> {
    let fail = |failure: FolderFailure| Unresolved {
        failure,
        from: Some(ReadFrom::WorkingTree),
        commit: None,
        source_id: None,
    };
    let copy = |reason: String| fail(FolderFailure::Copy { reason });
    match on {
        TryOn::Text { name, text } => {
            let name = checked_name(&name).map_err(|e| copy(e.message))?;
            if text.len() as u64 > BYTES_CAP {
                return Err(copy(too_large("The text", text.len() as u64).message));
            }
            let item = Item {
                rel: name.clone(),
                kind: ItemKind::File,
                size: text.len() as u64,
                executable: false,
                blob: Some(digest(text.as_bytes())),
            };
            let items = vec![item];
            let copy_name = "example".to_owned();
            Ok(Resolved {
                key: runs::clone_key(renderer, &copy_name, &items),
                copy_name,
                items,
                bytes: Arc::new(Memory {
                    name,
                    bytes: text.into_bytes(),
                }),
                from: ReadFrom::WorkingTree,
                commit: None,
                source_id: None,
            })
        }
        TryOn::Path { path } => {
            let (at, folder) = checked_path(&path).await.map_err(|e| copy(e.message))?;
            let (dir, only) = if folder {
                (at.clone(), None)
            } else {
                let parent = at
                    .parent()
                    .map(Path::to_path_buf)
                    .ok_or_else(|| copy("has no folder".to_owned()))?;
                (parent, Some(leaf(&at)))
            };
            let walked = {
                let dir = dir.clone();
                tokio::task::spawn_blocking(move || walk(&dir, only.as_deref()))
                    .await
                    .map_err(|e| copy(e.to_string()))?
            };
            let items = walked.map_err(copy)?;
            if !items.iter().any(|i| i.kind != ItemKind::Dir) {
                return Err(fail(FolderFailure::NoFolder));
            }
            if let Some(too_big) = crate::folder_render::resolve::past(
                &items,
                copy::CLONE_FILES,
                copy::CLONE_BYTES,
                ReadFrom::WorkingTree,
            ) {
                return Err(fail(too_big));
            }
            let copy_name = if folder {
                copy::copy_name(&leaf(&dir))
            } else {
                "example".to_owned()
            };
            Ok(Resolved {
                key: runs::clone_key(renderer, &copy_name, &items),
                copy_name,
                items,
                bytes: Arc::new(Disk { dir }),
                from: ReadFrom::WorkingTree,
                commit: None,
                source_id: None,
            })
        }
    }
}

/// Saved folder renderer `renderer` tried on `on`, in [`runs`].
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) for a renderer that is not saved;
/// [`Internal`](crate::IpcErrorCode::Internal) for the read of the list.
pub async fn folder_view(
    pool: &sqlx::PgPool,
    renderer: &str,
    on: TryOn,
    ask: runs::Ask,
) -> Result<runs::FolderRenderView, IpcError> {
    let saved = crate::folder_render::list(pool)
        .await?
        .into_iter()
        .find(|r| r.id == renderer)
        .ok_or_else(|| {
            IpcError::invalid(format!(
                "'{renderer}' is not one of your folder renderers: see Settings › Renderers"
            ))
        })?;
    Ok(folder_view_in(&runs(), &saved, on, ask).await)
}

/// [`folder_view`] with the renderer given, in `runs`: a test's own.
pub async fn folder_view_in(
    runs: &Arc<Runs>,
    saved: &FolderRenderer,
    on: TryOn,
    ask: runs::Ask,
) -> runs::FolderRenderView {
    let resolved = resolve(saved, on).await;
    runs.view(&slot(&saved.id), saved, resolved, ask)
}

/// A file's stand-in for a blob: its size and when it was last written, so a
/// file changed on disk is a new key and the same one is drawn at once.
fn stamp_of(meta: &std::fs::Metadata) -> String {
    let at = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos());
    format!("m:{}:{at}", meta.len())
}

fn digest(bytes: &[u8]) -> String {
    use sha2::Digest as _;
    format!(
        "w:{}",
        crate::folder_render::hex(&sha2::Sha256::digest(bytes))
    )
}

/// The items of `dir` (only `only` where given), off the runtime: files and
/// links, never followed; `.git` left out; the folders on the way.
fn walk(dir: &Path, only: Option<&str>) -> Result<Vec<Item>, String> {
    let mut items = Vec::new();
    let mut stack = vec![(dir.to_path_buf(), String::new())];
    while let Some((at, rel)) = stack.pop() {
        let listed =
            std::fs::read_dir(&at).map_err(|error| format!("{}: {error}", at.display()))?;
        for entry in listed.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if rel.is_empty() && only.is_some_and(|only| only != name) {
                continue;
            }
            let Ok(meta) = entry.path().symlink_metadata() else {
                continue;
            };
            let inner = if rel.is_empty() {
                name.clone()
            } else {
                format!("{rel}/{name}")
            };
            if meta.is_dir() {
                if name.eq_ignore_ascii_case(".git") {
                    continue;
                }
                items.push(Item {
                    rel: inner.clone(),
                    kind: ItemKind::Dir,
                    size: 0,
                    executable: false,
                    blob: None,
                });
                stack.push((entry.path(), inner));
                if items.len() > copy::CLONE_FILES * 2 {
                    return Err(format!(
                        "holds more than {} files, past what knobas copies",
                        copy::CLONE_FILES
                    ));
                }
                continue;
            }
            let link = meta.file_type().is_symlink();
            if !link && !meta.is_file() {
                continue;
            }
            #[cfg(unix)]
            let executable = {
                use std::os::unix::fs::PermissionsExt;
                !link && meta.permissions().mode() & 0o111 != 0
            };
            #[cfg(not(unix))]
            let executable = false;
            items.push(Item {
                rel: inner,
                kind: if link {
                    ItemKind::Symlink
                } else {
                    ItemKind::File
                },
                size: if link { 0 } else { meta.len() },
                executable,
                blob: Some(stamp_of(&meta)),
            });
            if items.len() > copy::CLONE_FILES * 2 {
                return Err(format!(
                    "holds more than {} files, past what knobas copies",
                    copy::CLONE_FILES
                ));
            }
        }
    }
    items.sort_by(|a, b| a.rel.cmp(&b.rel));
    Ok(items)
}

/// Pasted text's bytes, held in memory.
struct Memory {
    name: String,
    bytes: Vec<u8>,
}

#[async_trait::async_trait]
impl copy::Bytes for Memory {
    async fn read(&self, item: &Item) -> Result<Vec<u8>, String> {
        if item.rel == self.name {
            Ok(self.bytes.clone())
        } else {
            Err("is not the pasted text".to_owned())
        }
    }
}

/// A chosen folder's files, read from disk on the press, never through a
/// link (a link answers its target).
struct Disk {
    dir: PathBuf,
}

#[async_trait::async_trait]
impl copy::Bytes for Disk {
    async fn read(&self, item: &Item) -> Result<Vec<u8>, String> {
        let at = self.dir.join(&item.rel);
        if item.kind == ItemKind::Symlink {
            return tokio::fs::read_link(&at)
                .await
                .map(|target| target.to_string_lossy().into_owned().into_bytes())
                .map_err(|error| error.to_string());
        }
        let meta = tokio::fs::symlink_metadata(&at)
            .await
            .map_err(|error| error.to_string())?;
        if !meta.is_file() {
            return Err("is no longer a file".to_owned());
        }
        if meta.len() > copy::CLONE_BYTES {
            return Err(format!("is {} bytes, past what knobas copies", meta.len()));
        }
        tokio::fs::read(&at)
            .await
            .map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pasted_name_is_one_plain_file_name() {
        assert_eq!(checked_name("  diagram.puml ").unwrap(), "diagram.puml");
        for bad in ["", "  ", "a/b.md", "../x", "..", "con", "a:b", "x."] {
            assert!(checked_name(bad).is_err(), "{bad:?} is refused");
        }
    }

    #[tokio::test]
    async fn a_relative_or_missing_path_is_refused() {
        assert!(checked_path("notes/a.md").await.is_err(), "relative");
        let gone = tempfile::tempdir().unwrap().path().join("gone.md");
        assert!(
            checked_path(&gone.to_string_lossy()).await.is_err(),
            "missing"
        );
    }

    #[tokio::test]
    async fn a_chosen_file_is_read_whole_and_a_folder_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.md"), "# A\n").unwrap();
        let (name, bytes) = bytes_of(&TryOn::Path {
            path: dir.path().join("a.md").to_string_lossy().into_owned(),
        })
        .await
        .unwrap();
        assert_eq!(
            (name.as_str(), bytes.as_slice()),
            ("a.md", b"# A\n".as_slice())
        );
        assert!(file_bytes(&dir.path().to_string_lossy()).await.is_err());
    }

    #[test]
    fn a_folder_is_walked_without_git_and_without_following_links() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("flows/.git")).unwrap();
        std::fs::write(dir.path().join("flows/.git/HEAD"), "x").unwrap();
        std::fs::write(dir.path().join("flows/a.lcproj"), "good").unwrap();
        std::fs::write(dir.path().join("top.txt"), "top").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("/etc/hosts", dir.path().join("hosts")).unwrap();
        let items = walk(dir.path(), None).unwrap();
        let rels: Vec<(&str, ItemKind)> = items.iter().map(|i| (i.rel.as_str(), i.kind)).collect();
        assert!(rels.contains(&("flows", ItemKind::Dir)));
        assert!(rels.contains(&("flows/a.lcproj", ItemKind::File)));
        assert!(rels.contains(&("top.txt", ItemKind::File)));
        assert!(!rels.iter().any(|(r, _)| r.contains(".git")), "{rels:?}");
        #[cfg(unix)]
        assert!(rels.contains(&("hosts", ItemKind::Symlink)));
        let only = walk(dir.path(), Some("top.txt")).unwrap();
        assert_eq!(only.len(), 1, "a chosen file is a folder holding only it");
    }

    /// Each knobas's tries are in a folder of its own under `knobas-try/`, so another knobas's quit (which removes
    /// [`root`]) never reaches them. Not swept here: the other tests of this process write under it.
    #[test]
    fn each_knobas_tries_in_a_folder_of_its_own() {
        let mine = root();
        assert_eq!(mine.parent(), Some(tries().as_path()));
        assert_eq!(
            mine.file_name().and_then(|n| n.to_str()),
            Some(std::process::id().to_string().as_str())
        );
        let made = Throwaway::write("test", "own.md", "x").unwrap();
        assert!(made.dir.starts_with(&mine), "{}", made.dir.display());
        made.remove();
    }

    #[test]
    fn a_throwaway_file_is_written_under_the_root_and_removed() {
        let made = Throwaway::write("test", "example.md", "# Hi\n").unwrap();
        assert!(made.file.starts_with(root()));
        assert_eq!(std::fs::read_to_string(&made.file).unwrap(), "# Hi\n");
        let dir = made.dir.clone();
        made.remove();
        assert!(!dir.exists());
        assert!(Throwaway::write("test", "../escape.md", "x").is_err());
    }
}

//! What the shell asks of generated folders (ADR-0046): a folder's tree and
//! files by its token, and the store's place, size and *Clear*. The
//! commands in `commands/app.rs` are these, thin.
//!
//! A token the backend no longer holds in memory answers
//! [`NotFound`](crate::IpcErrorCode::NotFound): the shell asks
//! `folder_render` with `look` once, which reads the folder back from the
//! store under the same token, and reads again.

use std::path::Path;
use std::sync::Arc;

use serde::Serialize;
use sqlx::PgPool;

use super::keep::{self, LeftOut, Made, MadeFile, ReadError};
use super::runs::Runs;
use super::store::{self, MadeStore};
use crate::IpcError;

/// A generated folder's whole tree, as `folder_render_tree` answers it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "app.ts")]
pub struct MadeTree {
    /// Where it is kept, absolute: what *Copy path* copies on the landing.
    pub dir: String,
    /// Sorted by path bytes.
    pub files: Vec<MadeFile>,
    pub bytes: u64,
    pub left_out: Vec<LeftOut>,
    pub left_out_total: u32,
}

fn gone() -> IpcError {
    IpcError::not_found(
        "that generated folder is no longer kept: run the renderer again".to_owned(),
    )
}

/// The generated folder `token` names, while memory holds it.
///
/// # Errors
///
/// [`NotFound`](crate::IpcErrorCode::NotFound) for a token not held.
pub fn made(token: &str) -> Result<Arc<Made>, IpcError> {
    Runs::global().made(token).ok_or_else(gone)
}

/// What two generated folders differ in (ux26, ADR-0049, ruling 7): the files only in one, and those in both whose bytes
/// differ, sorted by path bytes, at most [`COMPARE_FILES`]; the rest counted.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "app.ts")]
pub struct MadeCompare {
    pub files: Vec<MadeChange>,
    /// In both, byte for byte the same.
    pub same: u32,
    /// In both, the same but for line endings (only where line endings are set aside).
    pub eol_only: u32,
    pub before_files: u32,
    pub after_files: u32,
}

/// One generated file that differs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "app.ts")]
pub struct MadeChange {
    pub path: String,
    pub change: MadeChangeKind,
    pub before_size: Option<u64>,
    pub after_size: Option<u64>,
    /// A NUL in the first 8,000 bytes of either side, as git tells binary.
    pub binary: bool,
}

/// How a generated file differs. No renames: a file that moved is one deleted and one added.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "app.ts")]
pub enum MadeChangeKind {
    Added,
    Deleted,
    Modified,
}

/// The most differing files one compare lists.
pub const COMPARE_FILES: usize = 20_000;
const SNIFF: u64 = 8_000;

fn binary_at(made: &Made, path: &str) -> bool {
    keep::read(made, path, SNIFF)
        .is_ok_and(|head| head.iter().take(SNIFF as usize).any(|b| *b == 0))
}

/// `bytes` with every CR before an LF left out.
fn without_crlf(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    for (at, b) in bytes.iter().enumerate() {
        if *b == b'\r' && bytes.get(at + 1) == Some(&b'\n') {
            continue;
        }
        out.push(*b);
    }
    out
}

/// The two generated folders compared (`before` `None`: no folder on that side, every file added). Equal sizes are
/// read and compared; different sizes differ with no read. With `eol_aside`, two texts the same but for CR before LF
/// count in `eol_only` and are not listed.
#[must_use]
pub fn compare_made(before: Option<&Made>, after: Option<&Made>, eol_aside: bool) -> MadeCompare {
    let empty: Vec<MadeFile> = Vec::new();
    let a = before.map_or(&empty, |m| &m.manifest.files);
    let b = after.map_or(&empty, |m| &m.manifest.files);
    let (mut i, mut j) = (0, 0);
    let mut out = MadeCompare {
        files: Vec::new(),
        same: 0,
        eol_only: 0,
        before_files: u32::try_from(a.len()).unwrap_or(u32::MAX),
        after_files: u32::try_from(b.len()).unwrap_or(u32::MAX),
    };
    let push = |out: &mut MadeCompare, change: MadeChange| {
        if out.files.len() < COMPARE_FILES {
            out.files.push(change);
        }
    };
    while i < a.len() || j < b.len() {
        let order = match (a.get(i), b.get(j)) {
            (Some(x), Some(y)) => x.path.as_bytes().cmp(y.path.as_bytes()),
            (Some(_), None) => std::cmp::Ordering::Less,
            _ => std::cmp::Ordering::Greater,
        };
        match order {
            std::cmp::Ordering::Less => {
                let x = &a[i];
                let binary = before.is_some_and(|m| binary_at(m, &x.path));
                push(
                    &mut out,
                    MadeChange {
                        path: x.path.clone(),
                        change: MadeChangeKind::Deleted,
                        before_size: Some(x.size),
                        after_size: None,
                        binary,
                    },
                );
                i += 1;
            }
            std::cmp::Ordering::Greater => {
                let y = &b[j];
                let binary = after.is_some_and(|m| binary_at(m, &y.path));
                push(
                    &mut out,
                    MadeChange {
                        path: y.path.clone(),
                        change: MadeChangeKind::Added,
                        before_size: None,
                        after_size: Some(y.size),
                        binary,
                    },
                );
                j += 1;
            }
            std::cmp::Ordering::Equal => {
                let (x, y) = (&a[i], &b[j]);
                let (old, new) = (before.expect("a file before"), after.expect("a file after"));
                let read = |m: &Made, f: &MadeFile| keep::read(m, &f.path, f.size).ok();
                let (left, right) = if x.size == y.size || eol_aside {
                    (read(old, x), read(new, y))
                } else {
                    (None, None)
                };
                let identical = x.size == y.size && left.is_some() && left == right;
                if identical {
                    out.same += 1;
                } else {
                    let binary = [&left, &right].iter().any(|side| {
                        side.as_ref()
                            .is_some_and(|bytes| bytes.iter().take(SNIFF as usize).any(|c| *c == 0))
                    }) || binary_at(old, &x.path)
                        || binary_at(new, &y.path);
                    let eol = eol_aside
                        && !binary
                        && matches!((&left, &right), (Some(l), Some(r)) if without_crlf(l) == without_crlf(r));
                    if eol {
                        out.eol_only += 1;
                    } else {
                        push(
                            &mut out,
                            MadeChange {
                                path: y.path.clone(),
                                change: MadeChangeKind::Modified,
                                before_size: Some(x.size),
                                after_size: Some(y.size),
                                binary,
                            },
                        );
                    }
                }
                i += 1;
                j += 1;
            }
        }
    }
    out
}

/// The generated folders `before` and `after` name, compared ([`compare_made`]), off the runtime. A token no longer held
/// answers `not_found`, and the page asks each side again with `look`.
///
/// # Errors
///
/// [`NotFound`](crate::IpcErrorCode::NotFound) for a token let go.
pub async fn compare(
    before: Option<&str>,
    after: Option<&str>,
    eol_aside: bool,
) -> Result<MadeCompare, IpcError> {
    let old = before.map(made).transpose()?;
    let new = after.map(made).transpose()?;
    tokio::task::spawn_blocking(move || compare_made(old.as_deref(), new.as_deref(), eol_aside))
        .await
        .map_err(IpcError::internal)
}

/// The tree of the generated folder `token` names.
///
/// # Errors
///
/// As [`made`].
pub fn tree(token: &str) -> Result<MadeTree, IpcError> {
    let made = made(token)?;
    Ok(tree_of(&made))
}

/// [`MadeTree`] of a kept folder.
#[must_use]
pub fn tree_of(made: &Made) -> MadeTree {
    let manifest = &made.manifest;
    MadeTree {
        dir: made.dir.to_string_lossy().into_owned(),
        files: manifest.files.clone(),
        bytes: manifest.bytes,
        left_out: manifest.left_out.clone(),
        left_out_total: manifest.left_out_total,
    }
}

/// `size` bytes in MiB, as `entity_file_bytes` says it.
fn mebibytes(size: u64) -> String {
    #[allow(clippy::cast_precision_loss)]
    let mib = size as f64 / (1024.0 * 1024.0);
    if mib.fract() == 0.0 {
        format!("{mib:.0} MiB")
    } else {
        format!("{mib:.1} MiB")
    }
}

fn not_held(path: &str) -> IpcError {
    IpcError::not_found(format!("the generated folder holds no file {path}"))
}

async fn read(made: Arc<Made>, path: String, cap: u64) -> Result<Vec<u8>, IpcError> {
    let shown = path.clone();
    tokio::task::spawn_blocking(move || keep::read(&made, &path, cap))
        .await
        .map_err(IpcError::internal)?
        .map_err(|error| match error {
            ReadError::NotFound => not_held(&shown),
            ReadError::Io(why) => IpcError::internal(format!("{shown} could not be read: {why}")),
        })
}

/// A generated file as `entity_local_file` answers a file: its text cut at
/// the view's caps, binary told by a NUL, its language by its name; no
/// commit, sha or page. `path` is the file's from the folder's top.
///
/// # Errors
///
/// [`NotFound`](crate::IpcErrorCode::NotFound) for a token not held or a
/// path the manifest does not hold.
pub async fn file(token: &str, path: &str) -> Result<knobas_source::view::Block, IpcError> {
    use knobas_source::file_text::{BYTE_CAP, capped, is_binary, language_of};
    let made = made(token)?;
    let size = made.file(path).ok_or_else(|| not_held(path))?.size;
    let bytes = read(made, path.to_owned(), BYTE_CAP as u64).await?;
    let binary = is_binary(&bytes);
    let (text, truncated) = if binary {
        (String::new(), false)
    } else {
        capped(&bytes, bytes.len() > BYTE_CAP)
    };
    Ok(knobas_source::view::Block::File {
        trail: Vec::new(),
        path: path.to_owned(),
        text,
        language: language_of(path).map(str::to_owned),
        size,
        truncated,
        binary,
        url: None,
        sha: None,
        commit: None,
    })
}

/// A generated file's bytes, whole up to the view's byte cap.
///
/// # Errors
///
/// As [`file`]; [`Invalid`](crate::IpcErrorCode::Invalid) past the cap, in
/// `entity_file_bytes`' words.
pub async fn file_bytes(token: &str, path: &str) -> Result<Vec<u8>, IpcError> {
    use knobas_source::view::BYTES_CAP;
    let made = made(token)?;
    let size = made.file(path).ok_or_else(|| not_held(path))?.size;
    if size > BYTES_CAP {
        return Err(IpcError::invalid(format!(
            "{path} is {}; knobas reads a file's bytes up to {}",
            mebibytes(size),
            mebibytes(BYTES_CAP)
        )));
    }
    read(made, path.to_owned(), BYTES_CAP).await
}

/// Point the runner at the store as the settings say now: at bring-up, and
/// after the setting or the clones root changed. A store that moved is
/// removed at its old place.
pub async fn place_store(pool: &PgPool, profile: &Path) {
    Runs::global().set_store(store::root(pool, profile).await);
}

/// The store as Settings shows it.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) if the setting cannot be read.
pub async fn store_view(pool: &PgPool, profile: &Path) -> Result<MadeStore, IpcError> {
    let set = store::chosen(pool).await?;
    let (base, place) = store::base(pool, profile).await;
    let dir = base.join(store::STORE_NAME);
    let default_dir = match crate::checkout::clones_root(pool).await? {
        Some(root) if !root.trim().is_empty() => root,
        _ => profile
            .join("folder-renders")
            .join("made")
            .to_string_lossy()
            .into_owned(),
    };
    let measured = dir.clone();
    let (folders, bytes) = tokio::task::spawn_blocking(move || store::usage(&measured))
        .await
        .map_err(IpcError::internal)?;
    Ok(MadeStore {
        dir: dir.to_string_lossy().into_owned(),
        set,
        default_dir,
        place,
        folders,
        bytes,
        cap_bytes: store::STORE_BYTES,
    })
}

/// Set the store's folder, or go back to the default with `None`.
///
/// # Errors
///
/// As [`store::set`].
pub async fn set_store(
    pool: &PgPool,
    profile: &Path,
    path: Option<&str>,
) -> Result<MadeStore, IpcError> {
    store::set(pool, path).await?;
    place_store(pool, profile).await;
    store_view(pool, profile).await
}

/// Remove every generated folder, on disk and in memory.
///
/// # Errors
///
/// As [`store_view`].
pub async fn clear_store(pool: &PgPool, profile: &Path) -> Result<MadeStore, IpcError> {
    tokio::task::spawn_blocking(|| Runs::global().clear_store())
        .await
        .map_err(IpcError::internal)?;
    store_view(pool, profile).await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The read commands refuse a token not held and a path outside the
    /// manifest.
    #[tokio::test]
    async fn the_reads_refuse_an_unknown_token_and_a_path_outside_the_manifest() {
        let error = tree("no-such-token").unwrap_err();
        assert_eq!(error.code, crate::IpcErrorCode::NotFound);
        let error = file("no-such-token", "a.md").await.unwrap_err();
        assert_eq!(error.code, crate::IpcErrorCode::NotFound);

        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out");
        std::fs::create_dir_all(out.join("d")).unwrap();
        std::fs::write(out.join("a.md"), "# a\n").unwrap();
        std::fs::write(out.join("d").join("b.bin"), b"\0\x01").unwrap();
        std::fs::write(dir.path().join("secret"), "no").unwrap();
        let progress: keep::Progress = Arc::new(|_, _| {});
        let made = Arc::new(
            keep::keep(
                &out,
                &dir.path().join("kept"),
                &progress,
                &tokio_util::sync::CancellationToken::new(),
            )
            .unwrap(),
        );
        let read_back = read(made.clone(), "a.md".to_owned(), 100).await.unwrap();
        assert_eq!(read_back, b"# a\n");
        for outside in ["../secret", "d", "d/../a.md", "/etc/hosts"] {
            let error = read(made.clone(), outside.to_owned(), 100)
                .await
                .unwrap_err();
            assert_eq!(error.code, crate::IpcErrorCode::NotFound, "{outside}");
        }
        assert_eq!(tree_of(&made).files.len(), 2);
    }
}

#[cfg(test)]
mod compare_tests {
    use super::*;
    use crate::folder_render::keep::Manifest;

    /// A generated folder of `files` written under `root/name`.
    fn made(root: &Path, name: &str, files: &[(&str, &[u8])]) -> Made {
        let dir = root.join(name);
        for (path, body) in files {
            let at = dir.join(path);
            std::fs::create_dir_all(at.parent().unwrap()).unwrap();
            std::fs::write(at, body).unwrap();
        }
        let manifest = Manifest {
            files: files
                .iter()
                .map(|(path, body)| MadeFile {
                    path: (*path).to_owned(),
                    size: body.len() as u64,
                })
                .collect(),
            ..Manifest::default()
        };
        Made::new(dir, manifest)
    }

    #[test]
    fn only_what_differs_is_listed() {
        let tmp = tempfile::tempdir().unwrap();
        let mut same: Vec<(String, Vec<u8>)> = (0..12)
            .map(|n| (format!("same/{n}.txt"), format!("s{n}").into_bytes()))
            .collect();
        let before_files: Vec<(&str, &[u8])> = [
            ("gone.txt", b"g".as_slice()),
            ("grew.txt", b"ab".as_slice()),
            ("last.txt", b"abcd".as_slice()),
        ]
        .into_iter()
        .chain(same.iter().map(|(p, b)| (p.as_str(), b.as_slice())))
        .collect();
        let after_files: Vec<(&str, &[u8])> = [
            ("new.txt", b"n".as_slice()),
            ("grew.txt", b"abc".as_slice()),
            ("last.txt", b"abcX".as_slice()),
        ]
        .into_iter()
        .chain(same.iter().map(|(p, b)| (p.as_str(), b.as_slice())))
        .collect();
        let before = made(tmp.path(), "a", &before_files);
        let after = made(tmp.path(), "b", &after_files);
        let out = compare_made(Some(&before), Some(&after), false);
        let listed: Vec<(&str, MadeChangeKind)> = out
            .files
            .iter()
            .map(|f| (f.path.as_str(), f.change))
            .collect();
        assert_eq!(
            listed,
            vec![
                ("gone.txt", MadeChangeKind::Deleted),
                ("grew.txt", MadeChangeKind::Modified),
                ("last.txt", MadeChangeKind::Modified),
                ("new.txt", MadeChangeKind::Added)
            ]
        );
        assert_eq!(
            (out.same, out.eol_only, out.before_files, out.after_files),
            (12, 0, 15, 15)
        );
        same.clear();
    }

    #[test]
    fn line_endings_aside_only_when_asked_and_never_for_a_binary_file() {
        let tmp = tempfile::tempdir().unwrap();
        let before = made(
            tmp.path(),
            "a",
            &[("page.html", b"<p>\n</p>\n"), ("logo.png", b"\x89PNG\0\n")],
        );
        let after = made(
            tmp.path(),
            "b",
            &[
                ("page.html", b"<p>\r\n</p>\r\n"),
                ("logo.png", b"\x89PNG\0\r\n"),
            ],
        );
        let aside = compare_made(Some(&before), Some(&after), true);
        assert_eq!(aside.eol_only, 1);
        assert_eq!(
            aside
                .files
                .iter()
                .map(|f| (f.path.as_str(), f.binary))
                .collect::<Vec<_>>(),
            vec![("logo.png", true)]
        );
        let kept = compare_made(Some(&before), Some(&after), false);
        assert_eq!(kept.eol_only, 0);
        assert_eq!(kept.files.len(), 2);
    }

    #[test]
    fn no_folder_before_lists_every_file_added() {
        let tmp = tempfile::tempdir().unwrap();
        let after = made(tmp.path(), "b", &[("a.md", b"# A"), ("b.md", b"# B")]);
        let out = compare_made(None, Some(&after), false);
        assert!(out.files.iter().all(|f| f.change == MadeChangeKind::Added));
        assert_eq!((out.before_files, out.after_files), (0, 2));
    }
}

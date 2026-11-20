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

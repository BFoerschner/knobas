//! *Viewed* on a pull request's files.

use super::*;

// -- *Viewed* on a pull request's files (#842) ------------------------------

pub use knobas_core::viewed::VIEWED_SETTING;

/// The files of pull request `pr_id` the reader marked *Viewed* (#842), in
/// path order: [`knobas_core::viewed::files`]. `blobs` is the blob each file
/// holds in the diff drawn, by path (#2276): a file it names is viewed by its
/// blob, one it does not by the current head.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) for an id that is not an entity
/// id; [`Internal`](crate::IpcErrorCode::Internal) if a read fails.
pub async fn pr_viewed_files_inner(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    pr_id: &str,
    blobs: &std::collections::BTreeMap<String, String>,
) -> Result<Vec<String>, IpcError> {
    let entity = EntityRef::parse(pr_id).map_err(IpcError::invalid)?;
    let declarations = declared_paths(pool, registry).await?;
    Ok(knobas_core::viewed::files(pool, &declarations, &entity, blobs).await?)
}

/// Mark one file of pull request `pr_id` *Viewed* or not (#842), by its blob
/// where `blobs` names it (#2276), and answer the files now viewed, in path
/// order: [`knobas_core::viewed::set_file`].
///
/// # Errors
///
/// As [`pr_viewed_files_inner`]; [`Internal`](crate::IpcErrorCode::Internal)
/// if the write fails.
pub async fn set_pr_file_viewed_inner(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    pr_id: &str,
    path: &str,
    viewed: bool,
    blobs: &std::collections::BTreeMap<String, String>,
) -> Result<Vec<String>, IpcError> {
    let entity = EntityRef::parse(pr_id).map_err(IpcError::invalid)?;
    let declarations = declared_paths(pool, registry).await?;
    Ok(knobas_core::viewed::set_file(pool, &declarations, &entity, path, viewed, blobs).await?)
}

/// The files of a pull request marked *Viewed* on its head (#842).
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is coming
/// up, and whatever [`pr_viewed_files_inner`] refuses with.
#[tauri::command]
pub async fn pr_viewed_files(
    lifecycle: State<'_, Lifecycle>,
    pr_id: String,
    blobs: Option<std::collections::BTreeMap<String, String>>,
) -> Result<Vec<String>, IpcError> {
    let pool = lifecycle.pool()?;
    pr_viewed_files_inner(
        &pool,
        lifecycle.registry(),
        &pr_id,
        &blobs.unwrap_or_default(),
    )
    .await
}

/// Mark a pull request's file *Viewed* or not (#842).
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is coming
/// up, and whatever [`set_pr_file_viewed_inner`] refuses with.
#[tauri::command]
pub async fn set_pr_file_viewed(
    lifecycle: State<'_, Lifecycle>,
    pr_id: String,
    path: String,
    viewed: bool,
    blobs: Option<std::collections::BTreeMap<String, String>>,
) -> Result<Vec<String>, IpcError> {
    let pool = lifecycle.pool()?;
    set_pr_file_viewed_inner(
        &pool,
        lifecycle.registry(),
        &pr_id,
        &path,
        viewed,
        &blobs.unwrap_or_default(),
    )
    .await
}

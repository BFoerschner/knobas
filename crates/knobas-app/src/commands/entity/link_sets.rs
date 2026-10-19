//! Link sets.

use super::*;

// -- Link sets (#866) ----------------------------------------------------------

/// Every link set (#866, R-7.6): [`crate::link_sets::list`].
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is coming
/// up, and whatever [`crate::link_sets::list`] refuses with.
#[tauri::command]
pub async fn link_sets(
    lifecycle: State<'_, Lifecycle>,
) -> Result<Vec<crate::link_sets::LinkSetRow>, IpcError> {
    crate::link_sets::list(&lifecycle.pool()?).await
}

/// One link set and the tickets it shows on (#866, R-7.7):
/// [`crate::link_sets::get`].
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is coming
/// up, and whatever [`crate::link_sets::get`] refuses with.
#[tauri::command]
pub async fn link_set(
    lifecycle: State<'_, Lifecycle>,
    id: String,
) -> Result<crate::link_sets::LinkSetDetail, IpcError> {
    crate::link_sets::get(&lifecycle.pool()?, &id).await
}

/// Save a whole link set (#866): [`crate::link_sets::save`].
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is coming
/// up, and whatever [`crate::link_sets::save`] refuses with.
#[tauri::command]
pub async fn save_link_set(
    lifecycle: State<'_, Lifecycle>,
    draft: crate::link_sets::LinkSetDraft,
) -> Result<crate::link_sets::LinkSetDetail, IpcError> {
    crate::link_sets::save(&lifecycle.pool()?, draft).await
}

/// Delete a link set (#866): [`crate::link_sets::delete`].
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is coming
/// up, and whatever [`crate::link_sets::delete`] refuses with.
#[tauri::command]
pub async fn delete_link_set(lifecycle: State<'_, Lifecycle>, id: String) -> Result<(), IpcError> {
    crate::link_sets::delete(&lifecycle.pool()?, &id).await
}

/// The tickets one rule applies to now (#866, R-7.7):
/// [`crate::link_sets::preview`].
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is coming
/// up, and whatever [`crate::link_sets::preview`] refuses with.
#[tauri::command]
pub async fn preview_link_set_rule(
    lifecycle: State<'_, Lifecycle>,
    rule: crate::link_sets::LinkSetRule,
) -> Result<Vec<crate::link_sets::RuleTicket>, IpcError> {
    crate::link_sets::preview(&lifecycle.pool()?, &rule).await
}

/// What a pattern would match now, for the pattern dialog (#868, R-7.7):
/// [`crate::link_sets::preview_pattern`].
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is coming
/// up, and whatever [`crate::link_sets::preview_pattern`] refuses with.
#[tauri::command]
pub async fn preview_link_set_pattern(
    lifecycle: State<'_, Lifecycle>,
    pattern: crate::link_sets::LinkSetPattern,
) -> Result<crate::link_sets::PatternPreview, IpcError> {
    crate::link_sets::preview_pattern(&lifecycle.pool()?, &pattern).await
}

/// *Hide here* or *Show again* on one ticket (#866, R-7.5):
/// [`crate::link_sets::hide`].
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is coming
/// up, and whatever [`crate::link_sets::hide`] refuses with.
#[tauri::command]
pub async fn hide_link_set_member(
    lifecycle: State<'_, Lifecycle>,
    ticket_id: String,
    member_id: String,
    hidden: bool,
) -> Result<bool, IpcError> {
    crate::link_sets::hide(&lifecycle.pool()?, &ticket_id, &member_id, hidden).await
}

/// What a job shows beside its own links (#867, R-4.4, R-6.2): the link sets
/// that apply, each with its reason and members, and the parent's own links:
/// [`crate::link_sets::on_job`].
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is coming
/// up, and whatever [`crate::link_sets::on_job`] refuses with.
#[tauri::command]
pub async fn job_link_sets(
    lifecycle: State<'_, Lifecycle>,
    entity_id: String,
) -> Result<crate::link_sets::JobLinkSets, IpcError> {
    crate::link_sets::on_job(&lifecycle.pool()?, &entity_id).await
}

/// The Link sets view's hint card (#870, R-7.10): items linked by hand under
/// an epic a set's rule names, offered to that set:
/// [`crate::link_sets::hints`].
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is coming
/// up, and whatever [`crate::link_sets::hints`] refuses with.
#[tauri::command]
pub async fn link_set_hints(
    lifecycle: State<'_, Lifecycle>,
) -> Result<Vec<crate::link_sets::LinkSetHint>, IpcError> {
    crate::link_sets::hints(&lifecycle.pool()?).await
}

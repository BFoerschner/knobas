//! The projects a corpus shows.

use super::*;

// -- the projects a corpus shows (#208) -------------------------------------
//
// Here for the reason the two sections above give: §10.8 freezes the
// `commands/` + `ipc/` layout, so an entity read gets a section rather than a
// file. A project is a grouping of *entities*, read out of the same mirror
// rows this module already lists.

/// Every project the live corpus shows, ordered by source then key
/// (ADR-0010, #208).
///
/// What the switcher builds its project rooms from. Flat rather than grouped
/// per source, because the switcher wants a room list and grouping it here
/// would only be ungrouped there. It cannot come from a room's own scan
/// instead: that read is a window over the newest items and explicitly not a
/// census, so a quiet project would silently have no room.
///
/// Unscoped, and answering for the **live** corpus -- so a source the user
/// turned off reports no projects (migration `0012`), and its projects come
/// back with it. Everything it decides lives in [`knobas_core::project`];
/// there is nothing for this seam to add, so it adds nothing, the same as
/// [`mini_board`].
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the
/// database is still coming up, [`Internal`](crate::IpcErrorCode::Internal)
/// for a query failure.
#[tauri::command]
pub async fn list_projects(
    lifecycle: State<'_, Lifecycle>,
) -> Result<Vec<knobas_core::project::Project>, IpcError> {
    list_projects_inner(&lifecycle.pool()?, lifecycle.registry()).await
}

/// [`list_projects`] with the pool handed in, so a test can reach it.
///
/// The same split every other read in this module has, and for the same
/// reason: `tauri::State` cannot be constructed by hand, so a command that
/// resolved its own pool would be unreachable and a test of it would have to
/// re-type the two lines it contains -- which is a test that goes on passing
/// after the command stops doing this. Pinned by
/// `a_confluence_space_is_a_project_room_and_a_jira_project_is_another` in
/// `tests/it/entity.rs`, which dies when the declarations stop being stored
/// here.
///
/// # Errors
///
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) for a query
/// failure or a declaration write that fails.
pub async fn list_projects_inner(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
) -> Result<Vec<knobas_core::project::Project>, IpcError> {
    // The declarations' stored copy, written first as every search command
    // writes it (one read when nothing changed): the census reads
    // `sync.item_project`, which the database resolves from it (#759).
    crate::commands::search::declare(pool, registry).await?;
    Ok(knobas_core::project::list(pool).await?)
}

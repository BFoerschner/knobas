//! The entity list's commands: shims over [`knobas_core::entity_list`], which
//! holds the read, its types and the six statements (#1317).

use knobas_core::entity_list::{self, EntityFilter, EntityPage};
pub(super) use knobas_core::entity_list::{EntityRow, entity_row, state_of};

use super::*;

/// One page of the room addressed by `filter`.
///
/// The behaviour lives here rather than in the command so it is reachable from
/// a test: a `#[tauri::command]` cannot be called directly.
///
/// A role ([`EntityFilter::role`]) is answered by the process's registry, as
/// the command's declarations are; [`list_entities_with_registry`] takes the
/// registry from its caller.
///
/// # Errors
///
/// [`IpcError`] with [`Internal`](crate::IpcErrorCode::Internal) if the query
/// fails.
pub async fn list_entities_inner(
    pool: &PgPool,
    filter: &EntityFilter,
    limit: u32,
    offset: u32,
    declarations: &knobas_core::payload::Declarations,
) -> Result<EntityPage, IpcError> {
    list_entities_with_registry(
        pool,
        &crate::sources::Registry::builtin(),
        filter,
        limit,
        offset,
        declarations,
    )
    .await
}

/// [`list_entities_inner`], answering a role from `registry`'s claims: the
/// role's fillers are resolved here and handed to [`entity_list::read`].
///
/// # Errors
///
/// As [`list_entities_inner`]; `invalid` for a filter the read refuses.
pub async fn list_entities_with_registry(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    filter: &EntityFilter,
    limit: u32,
    offset: u32,
    declarations: &knobas_core::payload::Declarations,
) -> Result<EntityPage, IpcError> {
    let fillers = match filter.role.as_deref() {
        Some(role) => Some(crate::sources::crud::role_scope(pool, registry, role).await?),
        None => None,
    };
    Ok(entity_list::read(pool, filter, limit, offset, declarations, fillers).await?)
}

/// One page of a room.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the
/// database is still coming up, [`Internal`](crate::IpcErrorCode::Internal)
/// for a query failure.
#[tauri::command]
pub async fn list_entities(
    lifecycle: State<'_, Lifecycle>,
    filter: EntityFilter,
    limit: u32,
    offset: u32,
) -> Result<EntityPage, IpcError> {
    let pool = lifecycle.pool()?;
    let declarations = declared_paths(&pool).await?;
    list_entities_inner(&pool, &filter, limit, offset, &declarations).await
}

/// Every configured source's declared payload paths (#277).
///
/// **`Registry::builtin()` rather than `SourcesState`'s trait object**, for the
/// reason `Registry::templates` exists as an inherent method at all: a
/// declaration is a property of the adapter *kind*, held in a `const` table
/// compiled into this binary, so a read that wants one has nothing to wait for.
/// Reaching through `SourcesState` would make a room's list, the mini board and
/// the project census answer `not_ready` until the scheduler is up, which is
/// later than the pool -- a widening of `not_ready` bought for nothing. The
/// reads that already hold a `SourcesState` (the inbox, the merge pass) pass
/// its registry instead, so the injected one is still what a test drives.
///
/// # Errors
///
/// `internal` if the source listing fails.
pub(super) async fn declared_paths(
    pool: &PgPool,
) -> Result<knobas_core::payload::Declarations, IpcError> {
    crate::sources::paths::declared_paths(pool, &crate::sources::Registry::builtin()).await
}

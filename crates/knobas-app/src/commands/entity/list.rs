//! The entity list's commands: shims over [`knobas_core::entity_list`], which
//! holds the read, its types and the six statements (#1317).

pub(super) use knobas_core::entity_list::EntityRow;
use knobas_core::entity_list::{self, EntityFilter, EntityPage};

use super::*;

/// One page of the room addressed by `filter`, a role
/// ([`EntityFilter::role`]) answered from `registry`'s claims: the role's
/// fillers are resolved here and handed to [`entity_list::read`].
///
/// The behaviour lives here rather than in the command so it is reachable from
/// a test: a `#[tauri::command]` cannot be called directly. The clamp is here
/// for that reason too: `limit` reads at most
/// [`ROW_CEILING`](crate::commands::ROW_CEILING) rows (#1643).
///
/// # Errors
///
/// [`IpcError`] with [`Internal`](crate::IpcErrorCode::Internal) if the query
/// fails; `invalid` for a filter the read refuses.
pub async fn list_entities_inner(
    pool: &PgPool,
    registry: &dyn AdapterRegistry,
    filter: &EntityFilter,
    limit: u32,
    offset: u32,
    declarations: &knobas_core::payload::Declarations,
) -> Result<EntityPage, IpcError> {
    let fillers = match filter.role.as_deref() {
        Some(role) => Some(crate::sources::crud::role_scope(pool, registry, role).await?),
        None => None,
    };
    let limit = limit.min(crate::commands::ROW_CEILING);
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
    let registry = lifecycle.registry();
    let declarations = declared_paths(&pool, registry).await?;
    list_entities_inner(&pool, registry, &filter, limit, offset, &declarations).await
}

/// Every configured source's declared payload paths (#277), as `registry`
/// declares them.
///
/// **The lifecycle's registry rather than `SourcesState`'s** (#1322): a
/// declaration is a property of the adapter *kind*, which the lifecycle holds
/// before the database is up, so a read that wants one has nothing to wait for.
/// Reaching through `SourcesState` would make a room's list, the mini board and
/// the project census answer `not_ready` until the scheduler is up, which is
/// later than the pool -- a widening of `not_ready` bought for nothing. The
/// commands that already hold a `SourcesState` pass its registry, which is the
/// same list.
///
/// # Errors
///
/// `internal` if the source listing fails.
pub(super) async fn declared_paths(
    pool: &PgPool,
    registry: &dyn AdapterRegistry,
) -> Result<knobas_core::payload::Declarations, IpcError> {
    crate::sources::paths::declared_paths(pool, registry).await
}

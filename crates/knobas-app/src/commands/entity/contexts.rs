//! Contexts, a job's reach and parent links.

use super::*;

// -- contexts (#47) ---------------------------------------------------------
//
// In this module rather than one of their own: §10.8 freezes the `commands/`
// + `ipc/` module layout ("no new module"), and contexts are the entity
// stream's own objects — linkable, listable, and read by the same surfaces.
// #1481 retired the ad-hoc mint: nothing called it after #881 took the rooms
// UI away. A context is born by `promote_context`.

/// The switcher's list: every unarchived context, newest first.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the
/// database is still coming up, [`Internal`](crate::IpcErrorCode::Internal)
/// for a query failure.
#[tauri::command]
pub async fn list_contexts(
    lifecycle: State<'_, Lifecycle>,
) -> Result<Vec<knobas_core::context::ContextRow>, IpcError> {
    let pool = lifecycle.pool()?;
    Ok(knobas_core::context::list(&pool).await?)
}

/// Promote an entity to a context of its own (spec §7).
///
/// Idempotent by the store's own promise: promoting twice answers with the one
/// context, and only the first call writes an activity line -- the second
/// mutated nothing, so it announces nothing. A done ticket's context is
/// archived (#1501): promoting answers with it as it is, and a context first
/// made for a done ticket is born archived, by the anchor's source's declared
/// done-like set (`registry`'s template descriptors).
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) if `entity_id` is not an entity
/// id, or if it is itself a context;
/// [`NotFound`](crate::IpcErrorCode::NotFound) if nothing local carries it;
/// [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
pub async fn promote_context_inner(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    entity_id: &str,
) -> Result<knobas_core::context::Promoted, IpcError> {
    let anchor = EntityRef::parse(entity_id).map_err(IpcError::invalid)?;
    let declarations = crate::sources::paths::declared_paths(pool, registry).await?;
    let promoted = knobas_core::context::promote(pool, &anchor, &declarations)
        .await?
        .ok_or_else(|| IpcError::not_found(format!("{anchor} is not in the mirror")))?;
    // One line per *mutation*: the store says which call inserted, so a
    // re-promotion answers with the room and writes nothing.
    if promoted.fresh {
        record(pool, "promoted", &promoted.context).await?;
    }
    Ok(promoted)
}

/// Who is in this context, by the fixed rule (§16.11, ADR-0008).
///
/// The per-context inbox filter ("3 here") intersects the inbox stream with
/// this set on the frontend, so the badge and the list it filters are drawn
/// from the same rows by construction.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the
/// database is still coming up, [`Internal`](crate::IpcErrorCode::Internal)
/// for a query failure.
#[tauri::command]
pub async fn context_members(
    lifecycle: State<'_, Lifecycle>,
    ctx_id: String,
) -> Result<Vec<String>, IpcError> {
    let pool = lifecycle.pool()?;
    Ok(knobas_core::context::member_ids(&pool, &ctx_id).await?)
}

/// What a ticket reaches (#817, R-2.8, R-2.9): each entity once, with the
/// first step of its path and where the path came from -- the ticket's own
/// links, or its parent's (R-7.3). `knobas_core::reach` is the walk; a job's
/// sections read it once for their reason labels, and `list_entities`'
/// `reach` scopes their rows by the same walk.
///
/// An id nothing answers to reaches nothing.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the
/// database is still coming up, [`Internal`](crate::IpcErrorCode::Internal)
/// for a query failure.
#[tauri::command]
pub async fn job_reach(
    lifecycle: State<'_, Lifecycle>,
    entity_id: String,
) -> Result<Vec<knobas_core::reach::Reached>, IpcError> {
    let pool = lifecycle.pool()?;
    Ok(knobas_core::reach::reach(&pool, &entity_id).await?)
}

/// Whether *Sub-tasks show their parent ticket's connections* is on (R-7.3).
/// On until somebody switches it off.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`Internal`](crate::IpcErrorCode::Internal) if the read fails.
#[tauri::command]
pub async fn parent_links(lifecycle: State<'_, Lifecycle>) -> Result<bool, IpcError> {
    let pool = lifecycle.pool()?;
    Ok(knobas_core::reach::parent_links(&pool).await?)
}

/// Switch *Sub-tasks show their parent ticket's connections* (R-7.3), and
/// answer what is now stored, as `set_passive_attribution` does.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`Internal`](crate::IpcErrorCode::Internal) if the write fails.
#[tauri::command]
pub async fn set_parent_links(
    lifecycle: State<'_, Lifecycle>,
    enabled: bool,
) -> Result<bool, IpcError> {
    let pool = lifecycle.pool()?;
    Ok(knobas_core::reach::set_parent_links(&pool, enabled).await?)
}

/// One activity line per context mutation, on the context's own entity.
async fn record(
    pool: &PgPool,
    verb: &str,
    row: &knobas_core::context::ContextRow,
) -> Result<(), IpcError> {
    let entity = EntityRef::parse(&row.id).map_err(IpcError::internal)?;
    let mut detail = serde_json::json!({
        "context_id": row.id,
        "kind": row.kind,
        "title": row.title,
    });
    // Absent rather than null for an ad-hoc context, the discipline
    // `link_detail` records: a reader of the log should not meet a key that
    // says "there is nothing here".
    if let (Some(map), Some(anchor)) = (detail.as_object_mut(), row.anchor_id.as_deref()) {
        map.insert("anchor_id".to_owned(), serde_json::Value::from(anchor));
    }
    knobas_core::activity::record(pool, ACTOR, verb, Some(&entity), detail).await?;
    Ok(())
}

/// Promote an entity to a context, and announce it.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the sync
/// engine is still coming up (the done-like sets are its registry's), and
/// whatever [`promote_context_inner`] refuses with.
#[tauri::command]
pub async fn promote_context<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    entity_id: String,
) -> Result<knobas_core::context::ContextRow, IpcError> {
    let state = crate::sources::state(&app)?;
    let promoted = promote_context_inner(&state.pool, state.registry.as_ref(), &entity_id).await?;
    // Only a mutation is news: a re-promotion changed nothing the switcher
    // could learn from re-listing.
    if promoted.fresh {
        announce_context(&app, &promoted.context);
    }
    Ok(promoted.context)
}

/// Put a changed context on `contexts:changed`.
///
/// Best-effort, like every emit in this app: a failure means no window is
/// listening, which is not a reason to fail a write that already landed. What
/// rides on the event is the row itself, so a listener can splice rather than
/// re-list -- though re-listing is also correct, and is what the switcher
/// does.
fn announce_context<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    row: &knobas_core::context::ContextRow,
) {
    crate::events::emit(app, None, crate::events::CONTEXTS_CHANGED, row);
}

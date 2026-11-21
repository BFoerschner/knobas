//! A ticket's fields (ADR-0054): what a person may change on a ticket
//! besides its status, and a field's choices. Both read live from the
//! ticket's adapter and never stored; the write is `submit_write` of an
//! `EditTicket`.

use crate::IpcError;

/// What a person may change on the ticket `entity_id` besides its status,
/// as its source says **now** (`Source::ticket_fields`): each field with its
/// kind, the value the source holds, and why not where it cannot be changed.
/// `null` is a source that offers no edits, or an entity that is not a
/// ticket. Routed like `reachable_transitions`: the entity id names the
/// source.
///
/// **`behind`**: asked ahead of a press (a card rested on, a lane pickup),
/// its requests go in the background lane, as `entity_view`'s do. Left out,
/// the read is a person's.
///
/// # Errors
///
/// `invalid` if `entity_id` is not an entity id, `not_found` if its namespace
/// is not a configured source, `unauthorized` / `unreachable` / `internal` as
/// the adapter's own failure maps;
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up.
#[tauri::command]
pub async fn ticket_fields<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    entity_id: String,
    behind: Option<bool>,
) -> Result<Option<knobas_source::ticket::TicketFields>, IpcError> {
    let state = crate::sources::state(&app)?;
    if behind == Some(true) {
        crate::logging::mark_read_behind();
    }
    let read = crate::sources::crud::ticket_fields(
        &state.pool,
        &state.secrets,
        &state.registry,
        &state.live,
        &entity_id,
    );
    let answer = if behind == Some(true) {
        knobas_http::behind(read).await
    } else {
        read.await
    };
    answer.map_err(|error| crate::sources::to_ipc(&error, None))
}

/// The choices of `field` (an id `ticket_fields` named, of kind `search`, or
/// `labels` with a scope) on the ticket `entity_id` that match `typed`;
/// with `typed` empty, the first of them (`Source::field_options`). `null` is
/// a field with no list.
///
/// # Errors
///
/// As [`ticket_fields`].
#[tauri::command]
pub async fn field_options<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    entity_id: String,
    field: String,
    typed: String,
) -> Result<Option<knobas_source::ticket::FieldOptions>, IpcError> {
    let state = crate::sources::state(&app)?;
    crate::sources::crud::field_options(
        &state.pool,
        &state.secrets,
        &state.registry,
        &state.live,
        &entity_id,
        &field,
        &typed,
    )
    .await
    .map_err(|error| crate::sources::to_ipc(&error, None))
}

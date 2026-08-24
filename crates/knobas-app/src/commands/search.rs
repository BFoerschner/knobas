//! Search and the launcher -- stream E (interfaces §2.4).

use tauri::State;

use crate::{AppState, IpcError};

/// The best `limit` full-text matches for `q`.
///
/// `q` is user text in the `websearch_to_tsquery` dialect and is bound as a
/// parameter all the way down; it is never interpolated into SQL.
///
/// The returned `snippet` is an excerpt of **raw source text** -- unescaped,
/// and written by whoever filed the ticket. It is the frontend's job to render
/// it as text; see `app/src/lib/ipc/search.ts`.
#[tauri::command]
pub async fn search(
    state: State<'_, AppState>,
    q: String,
    limit: u32,
) -> Result<Vec<knobas_db::search::SearchHit>, IpcError> {
    knobas_db::search::search(&state.pool, &q, i64::from(limit))
        .await
        .map_err(IpcError::internal)
}

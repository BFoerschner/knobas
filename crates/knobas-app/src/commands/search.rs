//! Search and the launcher -- stream E (interfaces §2.4).

use tauri::State;

use crate::{AppState, IpcError};

/// Answer one launcher query.
///
/// The **backend** parses the raw box text (ruling P2): the prefix, alias and
/// `key:value` grammar of §4 is one grammar, and the same parser serves M4's
/// saved searches. The response echoes its interpretation so the UI can render
/// the chips it inferred.
///
/// `query.raw` is user text in the `websearch_to_tsquery` dialect and is bound
/// as a parameter all the way down; it is never interpolated into SQL. Every
/// `Segment.text` in the result is an excerpt of **raw source text** -- render
/// it as text, never as markup.
#[tauri::command]
pub async fn search(
    state: State<'_, AppState>,
    query: knobas_search::SearchQuery,
) -> Result<knobas_search::SearchResponse, IpcError> {
    knobas_search::search(&state.pool, &query)
        .await
        .map_err(|error| match error {
            knobas_search::SearchError::Unsupported(_) => IpcError::invalid(error),
            knobas_search::SearchError::Db(_) => IpcError::internal(error),
        })
}

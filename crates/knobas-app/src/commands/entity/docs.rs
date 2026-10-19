//! The Docs reader.

use super::*;

// -- the Docs reader (#859) -------------------------------------------------
//
// Here for the frozen layout's reason (§10.8: no new `commands/` or `ipc/`
// module). The reads are `crate::docs`; these are the shims.

/// The Docs reader's left pane: every space the mirror holds pages of, each
/// with its page tree, and the page read last (#859, ADR-0022). The tree is
/// the wiki role's by definition, so it narrows itself to the role's fillers
/// (`sources::crud::fillers`, #1296, ADR-0029 as amended on 2026-10-19)
/// with no argument: a source with a wiki miss, one whose adapter claims no
/// wiki or is not installed, and the pages a removed source left behind are
/// in no space, never the page read last and never *For you*.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
#[tauri::command]
pub async fn docs_tree(lifecycle: State<'_, Lifecycle>) -> Result<crate::docs::DocsTree, IpcError> {
    let pool = lifecycle.pool()?;
    docs_tree_inner(&pool).await
}

/// [`docs_tree`] with the pool handed in, for a test.
///
/// # Errors
///
/// As [`docs_tree`].
pub async fn docs_tree_inner(pool: &PgPool) -> Result<crate::docs::DocsTree, IpcError> {
    // The census table the spaces come from is resolved from the stored
    // declarations, which every search command writes first (#759).
    crate::commands::search::searcher(pool).await?;
    let declarations = declared_paths(pool).await?;
    let me = knobas_search::Vocabulary::load(pool, knobas_search::KindCatalog::default())
        .await
        .map_err(IpcError::internal)?
        .identity;
    let registry = crate::sources::Registry::builtin();
    let fillers = crate::sources::crud::fillers(pool, &registry, knobas_source::role::WIKI).await?;
    crate::docs::tree(
        pool,
        &declarations,
        |kind| registry.doc_projection(kind),
        &me,
        &fillers,
    )
    .await
}

/// One page as the Docs reader draws it: header fields, blocks and comments
/// (#859, ADR-0022).
///
/// # Errors
///
/// `invalid` for something that is not an entity id, `not_found` for one the
/// live mirror does not hold or that is not a page, `internal` for a query
/// failure, `not_ready` before bring-up.
#[tauri::command]
pub async fn docs_page(
    lifecycle: State<'_, Lifecycle>,
    entity_id: String,
) -> Result<crate::docs::DocPage, IpcError> {
    let pool = lifecycle.pool()?;
    docs_page_inner(&pool, &entity_id).await
}

/// [`docs_page`] with the pool handed in, for a test.
///
/// # Errors
///
/// As [`docs_page`].
pub async fn docs_page_inner(
    pool: &PgPool,
    entity_id: &str,
) -> Result<crate::docs::DocPage, IpcError> {
    crate::commands::search::searcher(pool).await?;
    let declarations = declared_paths(pool).await?;
    let me = knobas_search::Vocabulary::load(pool, knobas_search::KindCatalog::default())
        .await
        .map_err(IpcError::internal)?
        .identity;
    let registry = crate::sources::Registry::builtin();
    let templates = registry.templates();
    crate::docs::page(
        pool,
        &declarations,
        |kind| registry.doc_projection(kind),
        |kind| crate::docs::docs_writes(&templates, registry.doc_writes(kind).is_some(), kind),
        &me,
        entity_id,
    )
    .await
}

/// The Docs search (#860, R-10.3, R-10.4): the pages a query admits, best
/// first, each with its snippet, match count, space, path and editor; `None`
/// for a query with neither words nor a filter, which the pane answers with
/// its tree. `space` is the picker's space; a typed `space:` wins over it.
///
/// # Errors
///
/// `invalid` for a query past the launcher's length, `internal` for a query
/// failure, `not_ready` before bring-up.
#[tauri::command]
pub async fn docs_search(
    lifecycle: State<'_, Lifecycle>,
    query: String,
    space: Option<crate::docs::DocsScope>,
) -> Result<Option<crate::docs::DocsHits>, IpcError> {
    let pool = lifecycle.pool()?;
    docs_search_inner(&pool, &query, space.as_ref()).await
}

/// [`docs_search`] with the pool handed in, for a test.
///
/// # Errors
///
/// As [`docs_search`].
pub async fn docs_search_inner(
    pool: &PgPool,
    query: &str,
    space: Option<&crate::docs::DocsScope>,
) -> Result<Option<crate::docs::DocsHits>, IpcError> {
    let searcher = crate::commands::search::searcher(pool).await?;
    let registry = crate::sources::Registry::builtin();
    // The wiki's search, so it narrows itself to the role's fillers (#1296).
    let fillers = crate::sources::crud::fillers(pool, &registry, knobas_source::role::WIKI).await?;
    crate::docs::search(
        pool,
        &searcher,
        |kind| registry.doc_projection(kind),
        query,
        space,
        &fillers,
    )
    .await
}

/// *Export as Markdown* (#861, R-10.17): one page as `.md` in Downloads,
/// through the one block-to-Markdown converter.
///
/// # Errors
///
/// As [`docs_page`]; `invalid` for a page knobas cannot read; `internal` if
/// there is no Downloads folder or the file cannot be written.
#[tauri::command]
pub async fn docs_export_markdown<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    entity_id: String,
) -> Result<crate::docs::DocExport, IpcError> {
    use tauri::Manager;
    let pool = lifecycle.pool()?;
    let downloads = app.path().download_dir().map_err(IpcError::internal)?;
    docs_export_markdown_inner(&pool, &entity_id, &downloads).await
}

/// [`docs_export_markdown`] into `folder`, for a test.
///
/// # Errors
///
/// As [`docs_export_markdown`].
pub async fn docs_export_markdown_inner(
    pool: &PgPool,
    entity_id: &str,
    folder: &std::path::Path,
) -> Result<crate::docs::DocExport, IpcError> {
    crate::commands::search::searcher(pool).await?;
    let declarations = declared_paths(pool).await?;
    let registry = crate::sources::Registry::builtin();
    crate::docs::export(
        pool,
        &declarations,
        |kind| registry.doc_projection(kind),
        entity_id,
        folder,
    )
    .await
}

/// *Edit section* (#862, R-10.13): queue a section's new text as the page's
/// whole new body (`UpdatePage`) through the write queue. `version` is the
/// version the reader read; a page that moved since is refused.
///
/// # Errors
///
/// As [`docs_edit_section_payload`], then whatever the write queue refuses.
#[tauri::command]
pub async fn docs_edit_section<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    entity_id: String,
    section: u32,
    version: Option<u64>,
    text: String,
) -> Result<knobas_core::write_queue::QueuedWrite, IpcError> {
    let state = crate::sources::state(&app)?;
    let payload =
        docs_edit_section_payload(&state.pool, &entity_id, section, version, &text).await?;
    crate::sources::write_queue::submit_in_background(&state, payload).await
}

/// The `UpdatePage` [`docs_edit_section`] queues, for a test.
///
/// # Errors
///
/// `invalid` for a section knobas does not edit, a page that moved past
/// `version`, or a source that writes no pages; `not_found` for a page the
/// live mirror does not hold.
pub async fn docs_edit_section_payload(
    pool: &PgPool,
    entity_id: &str,
    section: u32,
    version: Option<u64>,
    text: &str,
) -> Result<serde_json::Value, IpcError> {
    let registry = crate::sources::Registry::builtin();
    crate::docs::section_edit(
        pool,
        |kind| registry.doc_writes(kind),
        entity_id,
        section,
        version,
        text,
    )
    .await
}

/// *New page* (#862, R-10.16): queue a `CreatePage` under a mirrored page,
/// its body the editor's plain text in the source's markup. With `link_to`
/// (a ticket's entity id, *Link it to <job>*), the ticket is held with the
/// write and the new page is linked to it once the write has landed and a
/// sync has brought the page in, on whichever tick that is (#1121,
/// [`create_page_in_background`]).
///
/// # Errors
///
/// As [`docs_create_page_payload`], then whatever the write queue refuses.
#[tauri::command]
pub async fn docs_create_page<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    page: crate::docs::NewPage,
    link_to: Option<String>,
) -> Result<knobas_core::write_queue::QueuedWrite, IpcError> {
    let state = crate::sources::state(&app)?;
    let (payload, _) = docs_create_page_payload(&state.pool, &page).await?;
    create_page_in_background(&state, payload, link_to).await
}

/// [`docs_create_page`] once its `CreatePage` is built: queued and sent in
/// the background, the source re-read after. With `link_to`, the ticket is
/// held with the write (`sources::write_queue::submit_linked_in_background`,
/// #1121), so a page whose write waits and lands on a later tick is still
/// linked to its job, by the first sync that brings it in.
///
/// # Errors
///
/// As `submit_linked_in_background` with `link_to`, else whatever the write
/// queue refuses.
pub async fn create_page_in_background(
    state: &crate::sources::SourcesState,
    payload: serde_json::Value,
    link_to: Option<String>,
) -> Result<knobas_core::write_queue::QueuedWrite, IpcError> {
    if let Some(ticket) = link_to {
        return crate::sources::write_queue::submit_linked_in_background(state, payload, ticket)
            .await;
    }
    let (queued, source) = crate::sources::write_queue::queue(state, payload).await?;
    crate::sources::write_queue::send_in_background_then(
        state,
        source.clone(),
        queued.id,
        source,
        |_| async {},
    );
    Ok(queued)
}

/// The `CreatePage` [`docs_create_page`] queues and the source it goes to,
/// for a test.
///
/// # Errors
///
/// `invalid` for a blank title, a parent with no space or a source that
/// writes no pages; `not_found` for a parent the live mirror does not hold.
pub async fn docs_create_page_payload(
    pool: &PgPool,
    page: &crate::docs::NewPage,
) -> Result<(serde_json::Value, String), IpcError> {
    // The space comes from the census table, kept from the stored
    // declarations every search command writes first (#759).
    crate::commands::search::searcher(pool).await?;
    let registry = crate::sources::Registry::builtin();
    crate::docs::create_page(pool, |kind| registry.doc_writes(kind), page).await
}

/// *Move to Confluence…* (#941, R-13a.6, D10): queue one `CreatePage` per
/// note under a mirrored page, through the write queue, and answer what was
/// queued. The notes stay. Every page is checked before the first is queued,
/// so a refused parent queues nothing.
///
/// # Errors
///
/// As [`move_notes_payloads`], then whatever the write queue refuses.
#[tauri::command]
pub async fn move_notes_to_confluence<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    parent: String,
) -> Result<Vec<knobas_core::write_queue::QueuedWrite>, IpcError> {
    let state = crate::sources::state(&app)?;
    let payloads = move_notes_payloads(&state.pool, &parent).await?;
    let mut queued = Vec::with_capacity(payloads.len());
    let mut last = None;
    for (payload, _) in payloads {
        let (write, source) = crate::sources::write_queue::queue(&state, payload).await?;
        last = Some((source, write.id));
        queued.push(write);
    }
    // One send drains the source's queue, so the pages go in the notes' order.
    if let Some((source, id)) = last {
        crate::sources::write_queue::send_in_background(&state, source, id);
    }
    Ok(queued)
}

/// The `CreatePage`s [`move_notes_to_confluence`] queues, one per note
/// (`notes_export::note_pages`), each with the source it goes to, for a test.
///
/// # Errors
///
/// `invalid` when there are no notes, and as [`docs_create_page_payload`]
/// for the parent.
pub async fn move_notes_payloads(
    pool: &PgPool,
    parent: &str,
) -> Result<Vec<(serde_json::Value, String)>, IpcError> {
    let notes = knobas_core::note::all(pool).await?;
    if notes.is_empty() {
        return Err(IpcError::invalid("There are no notes to move."));
    }
    crate::commands::search::searcher(pool).await?;
    let registry = crate::sources::Registry::builtin();
    let mut payloads = Vec::with_capacity(notes.len());
    for page in crate::notes_export::note_pages(&notes, parent) {
        payloads
            .push(crate::docs::create_page(pool, |kind| registry.doc_writes(kind), &page).await?);
    }
    Ok(payloads)
}

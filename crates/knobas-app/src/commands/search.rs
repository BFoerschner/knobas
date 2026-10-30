//! Search and the launcher -- stream E (interfaces §2.4).
//!
//! Its `State<'_, Lifecycle>` is not an accident and is not stream D being
//! tidy: carry-over §10.6(a). `AppState` exists only once PostgreSQL is up,
//! and a `#[tauri::command]` resolves every argument *before* its body runs,
//! so a command declaring `State<'_, AppState>` is rejected by Tauri itself
//! during bring-up with the bare string `"state not managed"` -- no code for
//! the frontend to branch on. `Lifecycle` is managed at build time and is
//! always there; `lifecycle.pool()?` is the single place `not_ready` comes
//! from.
//!
//! # Why every command has an `_inner`
//!
//! A `#[tauri::command]` cannot be called without a window, and a
//! `tauri::State` cannot be built by hand -- so a command whose body is the
//! only place a behaviour lives is a behaviour no test can reach. Each command
//! here is therefore two lines (resolve the pool, call the inner function) and
//! everything worth asserting is in the `_inner`, which takes a `&PgPool` and
//! the lifecycle's `&dyn AdapterRegistry` (#1322) and is exercised by
//! `tests/it/search_ipc.rs`. `demo_load_inner` set the pattern.

use knobas_sync::scheduler::AdapterRegistry;
use sqlx::PgPool;
use tauri::State;

#[cfg(test)]
use crate::IpcErrorCode;
use crate::{IpcError, Lifecycle};

/// What an empty launcher box shows (interfaces §2.4).
///
/// **The one DTO in this stream that composes two streams' data**, which is
/// why it lives in the command module rather than in `knobas-search`:
/// `smart_lists` and `recent` are the search crate's [`LauncherBoard`],
/// `sources` is stream F's [`CredentialHealth`], and `pending_writes` counts
/// stream G's write queue. `knobas-search` takes a `PgPool` and deliberately
/// depends on none of those crates.
///
/// [`LauncherBoard`]: knobas_search::LauncherBoard
/// [`CredentialHealth`]: knobas_sync::CredentialHealth
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[ts(export_to = "search.ts")]
pub struct LauncherHome {
    pub smart_lists: Vec<knobas_search::SmartListSummary>,
    /// `knobas_search::EntityRow`, a *different Rust struct* from
    /// `knobas_core::entity_list::EntityRow` with the same eight fields on
    /// the wire. The shell has one type for both, the room's, which is the
    /// truth about the JSON; `the_two_entity_rows_are_one_wire_shape` fails
    /// if the two ever stop agreeing.
    #[ts(as = "Vec<knobas_core::entity_list::EntityRow>")]
    pub recent: Vec<knobas_search::EntityRow>,
    /// Stream F's credential health, read straight from `knobas.source_config`
    /// -- **not** a second copy of the type (interfaces §2.2, open question
    /// **E-Q5**, resolved: `knobas_sync::CredentialHealth` has merged, so the
    /// planned field-identical stand-in is not needed).
    pub sources: Vec<knobas_sync::CredentialHealth>,
    /// What the launcher's footer counts in *"mirror · N pending writes"*:
    /// the writes knobas still owes a source and will send on its own.
    ///
    /// [`QueueCounts::pending`] alone, never the sum of the three open states.
    /// A pending write asks the user for patience; a held or refused one asks
    /// for a *decision*, and the status bar's badge is where those are put
    /// (`QueueCounts`: "3 waiting" may never absorb a held write). Folding
    /// them in would let this number fall to zero with nothing sent.
    ///
    /// [`QueueCounts::pending`]: knobas_core::write_queue::QueueCounts::pending
    pub pending_writes: u32,
    /// Whether *Show notes and quick capture* is on (#1036, R-13a.6): the one
    /// read of `capture.notes_shown` the board's own searcher already made.
    /// The launcher drops its `note:` syntax row and Help entry while this is
    /// `false`, because typing the prefix then finds nothing.
    pub notes_shown: bool,
}

/// A search failure, as something the frontend can branch on (ruling P1).
///
/// The whole mapping, in one place, because it is the only decision this
/// module makes: a malformed query is the caller's fault (`invalid`), a
/// `list:` nobody ships is `not_found` -- which is what lets the box say *no
/// such list* rather than *something went wrong* -- and a database failure is
/// `internal`, because nothing downstream can do anything about it.
impl From<knobas_search::SearchError> for IpcError {
    fn from(error: knobas_search::SearchError) -> Self {
        match error {
            knobas_search::SearchError::Invalid(_) => Self::invalid(error),
            knobas_search::SearchError::UnknownList(_) => Self::not_found(error),
            knobas_search::SearchError::Db(_) => Self::internal(error),
        }
    }
}

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
///
/// # Errors
///
/// `invalid` for a query outside the engine's bounds, `not_found` for a
/// `list:` nobody ships, `not_ready` before the database is up, `internal` for
/// anything else.
#[tauri::command]
pub async fn search(
    lifecycle: State<'_, Lifecycle>,
    query: knobas_search::SearchQuery,
) -> Result<knobas_search::SearchResponse, IpcError> {
    search_inner(&lifecycle.pool()?, lifecycle.registry(), query).await
}

/// What the launcher shows before anything is typed: the smart lists, the
/// newest items, and the health of every configured source.
///
/// # Errors
///
/// `not_ready` before the database is up, `internal` if either read fails.
#[tauri::command]
pub async fn launcher_home(lifecycle: State<'_, Lifecycle>) -> Result<LauncherHome, IpcError> {
    launcher_home_inner(&lifecycle.pool()?, lifecycle.registry()).await
}

/// Every smart list -- built-in and saved -- with its count and its change
/// badge.
///
/// The rail on its own, for a launcher refreshing it without re-reading the
/// whole board, which is what it does after a save, a rename or a delete.
///
/// # Errors
///
/// `not_ready` before the database is up, `internal` if the summary fails.
#[tauri::command]
pub async fn smart_lists(
    lifecycle: State<'_, Lifecycle>,
) -> Result<Vec<knobas_search::SmartListSummary>, IpcError> {
    smart_lists_inner(&lifecycle.pool()?, lifecycle.registry()).await
}

/// Save the launcher's current query as a smart list (#506, story 56).
///
/// `label` is what the rail shows; `query` is the **raw box text**, which is
/// the whole of §4's grammar -- the prefix, the chips and the terms are all in
/// it, and the backend is what parses it (ruling P2). Answers with the row as
/// the rail draws it, so the caller need not re-read the board to show what it
/// just made.
///
/// # Errors
///
/// `invalid` for a blank name, for a query the grammar cannot run and when the
/// cap is reached, `not_ready` before the database is up, `internal` if the
/// write fails.
#[tauri::command]
pub async fn create_smart_list(
    lifecycle: State<'_, Lifecycle>,
    label: String,
    query: String,
) -> Result<knobas_search::SmartListSummary, IpcError> {
    create_smart_list_inner(&lifecycle.pool()?, lifecycle.registry(), &label, &query).await
}

/// Rename a saved smart list (#506, story 58).
///
/// # Errors
///
/// `invalid` for a blank name, `not_found` if nobody saved a list by that id
/// -- **including every built-in id**, which is not a saved list and cannot be
/// renamed -- `not_ready` before the database is up, `internal` if the write
/// fails.
#[tauri::command]
pub async fn rename_smart_list(
    lifecycle: State<'_, Lifecycle>,
    id: String,
    label: String,
) -> Result<(), IpcError> {
    rename_smart_list_inner(&lifecycle.pool()?, lifecycle.registry(), &id, &label).await
}

/// Forget a saved smart list (#506, story 58).
///
/// # Errors
///
/// `not_found` if nobody saved a list by that id, `not_ready` before the
/// database is up, `internal` if the delete fails.
#[tauri::command]
pub async fn delete_smart_list(
    lifecycle: State<'_, Lifecycle>,
    id: String,
) -> Result<(), IpcError> {
    delete_smart_list_inner(&lifecycle.pool()?, lifecycle.registry(), &id).await
}

/// Every ticket view (#812, R-3.2, R-3.3), in list order, each with its count
/// or the part of its text today's grammar could not read. See
/// [`crate::ticket_views`].
///
/// # Errors
///
/// `not_ready` before the database is up, `internal` if a read fails.
#[tauri::command]
pub async fn ticket_views(
    lifecycle: State<'_, Lifecycle>,
) -> Result<Vec<crate::ticket_views::TicketViewRow>, IpcError> {
    crate::ticket_views::list(&lifecycle.pool()?, lifecycle.registry()).await
}

/// Save a ticket view (#812): a new one, or over the view the draft names,
/// which also puts a deleted one back (*Undo*). The text is stored exactly as
/// sent. Answers the view as stored.
///
/// # Errors
///
/// `invalid` for a blank name or group, or a display word the Tickets screen
/// does not know; `not_ready` before the database is up; `internal` if the
/// write fails.
#[tauri::command]
pub async fn save_ticket_view(
    lifecycle: State<'_, Lifecycle>,
    view: crate::ticket_views::TicketViewDraft,
) -> Result<crate::ticket_views::TicketView, IpcError> {
    crate::ticket_views::save(&lifecycle.pool()?, view).await
}

/// Delete a ticket view (#812). *Undo* saves the view back.
///
/// # Errors
///
/// `not_found` if no view has that id, `not_ready` before the database is up,
/// `internal` if the delete fails.
#[tauri::command]
pub async fn delete_ticket_view(
    lifecycle: State<'_, Lifecycle>,
    id: String,
) -> Result<(), IpcError> {
    crate::ticket_views::delete(&lifecycle.pool()?, &id).await
}

/// *Hide from the list* (`hidden: true`) or *Show* a saved view, such as a Jira view (#878, #1245,
/// R-3.25); it keeps syncing either way.
///
/// # Errors
///
/// `not_found` if no view has that id, `invalid` for a view somebody made,
/// `not_ready` before the database is up, `internal` if the write fails.
#[tauri::command]
pub async fn hide_saved_view(
    lifecycle: State<'_, Lifecycle>,
    id: String,
    hidden: bool,
) -> Result<(), IpcError> {
    crate::ticket_views::hide(&lifecycle.pool()?, &id, hidden).await
}

/// *Make an editable copy* of a saved view (#1208): its query read into the
/// search grammar by its source, live. `None` for a source that translates
/// no query. See [`crate::ticket_views::translate`].
///
/// # Errors
///
/// `not_ready` before bring-up, and whatever
/// [`crate::ticket_views::translate`] refuses with.
#[tauri::command]
pub async fn translate_ticket_view<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    id: String,
) -> Result<Option<knobas_source::query::Translation>, IpcError> {
    let state = crate::sources::state(&app)?;
    crate::ticket_views::translate(
        &state.pool,
        &state.secrets,
        &state.registry,
        &state.live,
        &id,
    )
    .await
}

/// A ticket view was opened: a saved view, such as a Jira view, stops saying
/// *new* (#878, #1245, R-3.20).
///
/// # Errors
///
/// `not_found` if no view has that id, `not_ready` before the database is
/// up, `internal` if the write fails.
#[tauri::command]
pub async fn open_saved_view(lifecycle: State<'_, Lifecycle>, id: String) -> Result<(), IpcError> {
    crate::ticket_views::open(&lifecycle.pool()?, &id).await
}

/// The values the query box suggests after `kind:`, `source:` and `under:`
/// (#601): the kinds with rows, the enabled sources, and the names the
/// mirror's and the estate's parents answer to, each with its kind. Only
/// values that exist right now.
///
/// `under` is the `under:` value being typed (#761): the parents are the
/// newest [`knobas_search::PARENT_SUGGESTIONS`] whose name starts with it,
/// ignoring case -- the newest outright when it is absent or empty. The box
/// reads the lists once per opening and asks again per keystroke of an
/// `under:` value, narrowing the last answer itself while the next is in
/// flight.
///
/// `parents_only` (#768), absent meaning false: read the parents and leave
/// `kinds` and `sources` empty. The box sets it on every read after the
/// opening's first, which already holds those two lists.
///
/// # Errors
///
/// `not_ready` before the database is up, `internal` if a read fails.
#[tauri::command]
pub async fn filter_suggestions(
    lifecycle: State<'_, Lifecycle>,
    under: Option<String>,
    parents_only: Option<bool>,
) -> Result<knobas_search::FilterSuggestions, IpcError> {
    filter_suggestions_inner(
        &lifecycle.pool()?,
        lifecycle.registry(),
        under.as_deref().unwrap_or(""),
        parents_only.unwrap_or(false),
    )
    .await
}

/// The launcher's engine, reading what `registry`'s adapters declare about
/// their payloads -- the parent `under:` scopes by among it (#595) -- and the
/// one place the app hands those declarations to the database too (#627).
///
/// **`registry` is the lifecycle's** (#1322), for the reason
/// `commands::entity`'s `declared_paths` gives: a declaration is a property
/// of the adapter kind, compiled in or installed, which the lifecycle holds
/// before the database is up, so the search has nothing to wait for and no
/// reason to answer `not_ready` until the scheduler is up.
///
/// **One function for both copies** (the deputy's ruling of 2026-10-12 on
/// #627): `Searcher::declaring` keeps the templates in memory for the path
/// line and coverage, and writes the same list to `sync.parent_declaration`,
/// from which the database keeps `sync.item_parent` -- what `under:` reads.
/// Bring-up calls this before the first sync, and every command calls it
/// again, so the stored copy is always the running process's. A call that changes
/// nothing is one read and writes nothing.
///
/// # Errors
///
/// `internal` if the declarations cannot be written.
pub async fn searcher(
    pool: &PgPool,
    registry: &dyn AdapterRegistry,
) -> Result<knobas_search::Searcher, IpcError> {
    // **Notes leave the search while *Show notes and quick capture* is off**
    // (#940, R-13a.6, D10): the one primary-key read of `knobas.setting` that
    // every command's searcher makes, so the launcher, the lists and the
    // suggestions all agree. The notes stay in the database.
    let notes_shown = crate::capture::shown(pool).await?;
    Ok(
        knobas_search::Searcher::declaring(pool.clone(), registry.descriptors())
            .await?
            .with_notes_shown(notes_shown),
    )
}

/// Store this process's declarations and build nothing else: [`searcher`]
/// for a command that wants the stored copy written (the census, `under:`,
/// the Docs spaces read it) and never searches, so it skips the
/// notes-visibility read the searcher makes (#1659).
///
/// # Errors
///
/// `internal` if the declarations cannot be written.
pub async fn declare(pool: &PgPool, registry: &dyn AdapterRegistry) -> Result<(), IpcError> {
    knobas_search::Searcher::declaring(pool.clone(), registry.descriptors()).await?;
    Ok(())
}

/// [`search`], against a pool and a registry.
///
/// # Errors
///
/// See [`search`].
pub async fn search_inner(
    pool: &PgPool,
    registry: &dyn AdapterRegistry,
    query: knobas_search::SearchQuery,
) -> Result<knobas_search::SearchResponse, IpcError> {
    Ok(searcher(pool, registry).await?.search(query).await?)
}

/// [`launcher_home`], against a pool and a registry.
///
/// Three reads, deliberately not one: the board is `knobas-search`'s, the
/// health is stream F's and the queue depth is `knobas-core`'s, and no one of
/// those crates may learn about the others to save round trips that a local
/// socket answers in microseconds.
///
/// # Errors
///
/// See [`launcher_home`].
pub async fn launcher_home_inner(
    pool: &PgPool,
    registry: &dyn AdapterRegistry,
) -> Result<LauncherHome, IpcError> {
    let board = searcher(pool, registry).await?.launcher_board().await?;
    let notes_shown = crate::capture::shown(pool).await?;
    Ok(LauncherHome {
        smart_lists: board.smart_lists,
        recent: board.recent,
        sources: knobas_sync::config::health_all(pool).await?,
        // Pending only -- see the field's own doc for why the other two open
        // states stay with `write_queue_counts`. `count(*)` is an `i64` and
        // the field is a `u32`: saturating, because a queue deep enough to
        // overflow one is a footer nobody is reading a number off any more.
        pending_writes: u32::try_from(knobas_core::write_queue::counts(pool).await?.pending)
            .unwrap_or(u32::MAX),
        notes_shown,
    })
}

/// [`filter_suggestions`], against a pool and a registry.
///
/// # Errors
///
/// See [`filter_suggestions`].
pub async fn filter_suggestions_inner(
    pool: &PgPool,
    registry: &dyn AdapterRegistry,
    under: &str,
    parents_only: bool,
) -> Result<knobas_search::FilterSuggestions, IpcError> {
    let searcher = searcher(pool, registry).await?;
    Ok(if parents_only {
        searcher.filter_parent_suggestions(under).await?
    } else {
        searcher.filter_suggestions(under).await?
    })
}

/// [`smart_lists`], against a pool and a registry.
///
/// # Errors
///
/// See [`smart_lists`].
pub async fn smart_lists_inner(
    pool: &PgPool,
    registry: &dyn AdapterRegistry,
) -> Result<Vec<knobas_search::SmartListSummary>, IpcError> {
    Ok(searcher(pool, registry).await?.smart_lists().await?)
}

/// [`create_smart_list`], against a pool and a registry.
///
/// # Errors
///
/// See [`create_smart_list`].
pub async fn create_smart_list_inner(
    pool: &PgPool,
    registry: &dyn AdapterRegistry,
    label: &str,
    query: &str,
) -> Result<knobas_search::SmartListSummary, IpcError> {
    Ok(searcher(pool, registry)
        .await?
        .save_list(label, query)
        .await?)
}

/// [`rename_smart_list`], against a pool and a registry.
///
/// # Errors
///
/// See [`rename_smart_list`].
pub async fn rename_smart_list_inner(
    pool: &PgPool,
    registry: &dyn AdapterRegistry,
    id: &str,
    label: &str,
) -> Result<(), IpcError> {
    Ok(searcher(pool, registry)
        .await?
        .rename_list(id, label)
        .await?)
}

/// [`delete_smart_list`], against a pool and a registry.
///
/// # Errors
///
/// See [`delete_smart_list`].
pub async fn delete_smart_list_inner(
    pool: &PgPool,
    registry: &dyn AdapterRegistry,
    id: &str,
) -> Result<(), IpcError> {
    Ok(searcher(pool, registry).await?.delete_list(id).await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `LauncherHome.recent` and a room's rows are **one** TypeScript type over
    /// **two** Rust structs, so the two have to keep agreeing.
    ///
    /// `knobas_search::EntityRow` (stream E's, the search corpus) and
    /// `knobas_core::entity_list::EntityRow` (stream D's, a room line) were
    /// declared independently and serialize identically. The generated
    /// `LauncherHome` types `recent` with the room's declaration
    /// (`#[ts(as = ...)]`, #1329) rather than a second one, which is only
    /// honest while that holds -- and neither struct is declared from the
    /// other, so nothing but this notices the day one of them gains a field.
    ///
    /// **`path` is carried with a value, not left `None`** (#284). Both
    /// structs would serialize `None` to `"path": null` and the two objects
    /// would still be equal, so a `None` fixture would pass while one of the
    /// two had the field spelled differently or missing behind a
    /// `skip_serializing_if`. A value on both sides is what makes the
    /// comparison say something about the field this test was extended for.
    #[test]
    fn the_two_entity_rows_are_one_wire_shape() {
        let at = chrono::Utc::now();
        let path = Some("Engineering \u{203a} Payments".to_owned());
        // #666: a value, for `path`'s reason below.
        let state = Some(knobas_core::payload::StateChip {
            text: "FAILURE".to_owned(),
            tone: knobas_core::tone::Tone::Err,
        });
        let search = serde_json::to_value(knobas_search::EntityRow {
            entity_id: "jira:PAY-231".to_owned(),
            kind: "ticket".to_owned(),
            source_id: "jira".to_owned(),
            title: "Retry failed SEPA payouts".to_owned(),
            updated_at: Some(at),
            synced_at: at,
            path: path.clone(),
            state: state.clone(),
        })
        .expect("serializes");
        let room = serde_json::to_value(knobas_core::entity_list::EntityRow {
            entity_id: "jira:PAY-231".to_owned(),
            kind: "ticket".to_owned(),
            source_id: "jira".to_owned(),
            title: "Retry failed SEPA payouts".to_owned(),
            updated_at: Some(at),
            synced_at: at,
            path,
            state,
        })
        .expect("serializes");

        assert_eq!(
            search, room,
            "the two EntityRow structs no longer share a wire shape, so \
             `LauncherHome::recent` must stop borrowing the room's TypeScript type"
        );
        assert_eq!(
            search["path"], "Engineering \u{203a} Payments",
            "the ratified §10.8 field is on the wire under its own name"
        );
        assert_eq!(
            search["state"],
            serde_json::json!({ "text": "FAILURE", "tone": "err" }),
            "#666's field is on the wire under its own name, its tone lowercase"
        );
    }

    /// A search failure arrives as a code the frontend can branch on.
    ///
    /// The database arm is covered against a real closed pool in
    /// `tests/it/search_ipc.rs`; this pins the two that a caller can provoke, and
    /// pins them here because the `From` impl is the only decision this module
    /// makes.
    #[test]
    fn search_failures_keep_their_kind() {
        assert_eq!(
            IpcError::from(knobas_search::SearchError::Invalid("too long".into())).code,
            IpcErrorCode::Invalid
        );
        assert_eq!(
            IpcError::from(knobas_search::SearchError::UnknownList("nope".into())).code,
            IpcErrorCode::NotFound
        );
        assert_eq!(
            IpcError::from(knobas_search::SearchError::Db(sqlx::Error::PoolClosed)).code,
            IpcErrorCode::Internal
        );
    }
}

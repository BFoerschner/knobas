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
//! is exercised by `tests/it/search_ipc.rs`. `demo_load_inner` set the pattern.

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
#[derive(Debug, Clone, serde::Serialize)]
pub struct LauncherHome {
    pub smart_lists: Vec<knobas_search::SmartListSummary>,
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
    search_inner(&lifecycle.pool()?, query).await
}

/// What the launcher shows before anything is typed: the smart lists, the
/// newest items, and the health of every configured source.
///
/// # Errors
///
/// `not_ready` before the database is up, `internal` if either read fails.
#[tauri::command]
pub async fn launcher_home(lifecycle: State<'_, Lifecycle>) -> Result<LauncherHome, IpcError> {
    launcher_home_inner(&lifecycle.pool()?).await
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
    smart_lists_inner(&lifecycle.pool()?).await
}

/// The rows of one smart list, shaped exactly like a search.
///
/// Opening a list is also what clears its badge.
///
/// # Errors
///
/// `not_found` if nobody ships a list by that id, `not_ready` before the
/// database is up, `internal` if the statement fails.
#[tauri::command]
pub async fn smart_list_items(
    lifecycle: State<'_, Lifecycle>,
    id: String,
    limit: u32,
) -> Result<knobas_search::SearchResponse, IpcError> {
    smart_list_items_inner(&lifecycle.pool()?, &id, limit).await
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
    create_smart_list_inner(&lifecycle.pool()?, &label, &query).await
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
    rename_smart_list_inner(&lifecycle.pool()?, &id, &label).await
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
    delete_smart_list_inner(&lifecycle.pool()?, &id).await
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
    crate::ticket_views::list(&lifecycle.pool()?).await
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

/// *Hide from the list* (`hidden: true`) or *Show* a Jira view (#878,
/// R-3.25); it keeps syncing either way.
///
/// # Errors
///
/// `not_found` if no view has that id, `invalid` for a view somebody made,
/// `not_ready` before the database is up, `internal` if the write fails.
#[tauri::command]
pub async fn hide_jira_view(
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
    crate::ticket_views::translate(&state.pool, &state.secrets, state.registry.as_ref(), &id).await
}

/// A ticket view was opened: a Jira view stops saying *new* (#878, R-3.20).
///
/// # Errors
///
/// `not_found` if no view has that id, `not_ready` before the database is
/// up, `internal` if the write fails.
#[tauri::command]
pub async fn open_jira_view(lifecycle: State<'_, Lifecycle>, id: String) -> Result<(), IpcError> {
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
        under.as_deref().unwrap_or(""),
        parents_only.unwrap_or(false),
    )
    .await
}

/// The launcher's engine, reading what the compiled-in adapters declare about
/// their payloads -- the parent `under:` scopes by among it (#595) -- and the
/// one place the app hands those declarations to the database too (#627).
///
/// **`Registry::builtin()`**, for the reason `commands::entity`'s
/// `declared_paths` gives: a declaration is a property of the adapter kind,
/// compiled into this binary, so the search has nothing to wait for and no
/// reason to answer `not_ready` until the scheduler is up.
///
/// **One function for both copies** (the deputy's ruling of 2026-10-12 on
/// #627): `Searcher::declaring` keeps the templates in memory for the path
/// line and coverage, and writes the same list to `sync.parent_declaration`,
/// from which the database keeps `sync.item_parent` -- what `under:` reads.
/// Bring-up calls this before the first sync, and every command calls it
/// again, so the stored copy is always this binary's. A call that changes
/// nothing is one read and writes nothing.
///
/// # Errors
///
/// `internal` if the declarations cannot be written.
pub async fn searcher(pool: &PgPool) -> Result<knobas_search::Searcher, IpcError> {
    // **Notes leave the search while *Show notes and quick capture* is off**
    // (#940, R-13a.6, D10): the one primary-key read of `knobas.setting` that
    // every command's searcher makes, so the launcher, the lists and the
    // suggestions all agree. The notes stay in the database.
    let notes_shown = crate::capture::shown(pool).await?;
    Ok(knobas_search::Searcher::declaring(
        pool.clone(),
        crate::sources::Registry::builtin().templates(),
    )
    .await?
    .with_notes_shown(notes_shown))
}

/// [`search`], against a pool.
///
/// # Errors
///
/// See [`search`].
pub async fn search_inner(
    pool: &PgPool,
    query: knobas_search::SearchQuery,
) -> Result<knobas_search::SearchResponse, IpcError> {
    Ok(searcher(pool).await?.search(query).await?)
}

/// [`launcher_home`], against a pool.
///
/// Three reads, deliberately not one: the board is `knobas-search`'s, the
/// health is stream F's and the queue depth is `knobas-core`'s, and no one of
/// those crates may learn about the others to save round trips that a local
/// socket answers in microseconds.
///
/// # Errors
///
/// See [`launcher_home`].
pub async fn launcher_home_inner(pool: &PgPool) -> Result<LauncherHome, IpcError> {
    let board = searcher(pool).await?.launcher_board().await?;
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

/// [`filter_suggestions`], against a pool.
///
/// # Errors
///
/// See [`filter_suggestions`].
pub async fn filter_suggestions_inner(
    pool: &PgPool,
    under: &str,
    parents_only: bool,
) -> Result<knobas_search::FilterSuggestions, IpcError> {
    let searcher = searcher(pool).await?;
    Ok(if parents_only {
        searcher.filter_parent_suggestions(under).await?
    } else {
        searcher.filter_suggestions(under).await?
    })
}

/// [`smart_lists`], against a pool.
///
/// # Errors
///
/// See [`smart_lists`].
pub async fn smart_lists_inner(
    pool: &PgPool,
) -> Result<Vec<knobas_search::SmartListSummary>, IpcError> {
    Ok(searcher(pool).await?.smart_lists().await?)
}

/// [`smart_list_items`], against a pool.
///
/// # Errors
///
/// See [`smart_list_items`].
pub async fn smart_list_items_inner(
    pool: &PgPool,
    id: &str,
    limit: u32,
) -> Result<knobas_search::SearchResponse, IpcError> {
    Ok(searcher(pool).await?.smart_list_items(id, limit).await?)
}

/// [`create_smart_list`], against a pool.
///
/// # Errors
///
/// See [`create_smart_list`].
pub async fn create_smart_list_inner(
    pool: &PgPool,
    label: &str,
    query: &str,
) -> Result<knobas_search::SmartListSummary, IpcError> {
    Ok(searcher(pool).await?.save_list(label, query).await?)
}

/// [`rename_smart_list`], against a pool.
///
/// # Errors
///
/// See [`rename_smart_list`].
pub async fn rename_smart_list_inner(pool: &PgPool, id: &str, label: &str) -> Result<(), IpcError> {
    Ok(searcher(pool).await?.rename_list(id, label).await?)
}

/// [`delete_smart_list`], against a pool.
///
/// # Errors
///
/// See [`delete_smart_list`].
pub async fn delete_smart_list_inner(pool: &PgPool, id: &str) -> Result<(), IpcError> {
    Ok(searcher(pool).await?.delete_list(id).await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The wire keys of the two DTOs this module owns, in the spelling
    /// `app/src/lib/ipc/search.ts` declares.
    ///
    /// The mirror is hand-written, so nothing but this connects the two: a
    /// renamed field compiles on both sides and renders `undefined` in the
    /// launcher. The expected list is taken from `serde_json` rather than
    /// written out, so it is the **wire** that is compared against the mirror,
    /// not a third copy of the field names that could drift from both.
    ///
    /// # The search has to be scoped, and for one round it was not
    ///
    /// This asserted `mirror.contains("sources:")` against the **whole file** —
    /// 150 lines and six interfaces. `sources:` was therefore satisfied by
    /// `SearchFilters.sources` a hundred lines away, `id:` by `entity_id:`, and
    /// `label:` by `ResultGroup.label`. Review round 1 proved the consequence
    /// twice: deleting `sources: CredentialHealth[];` from `LauncherHome` left
    /// this test green, and renaming the field to `source_health` on the
    /// TypeScript side alone — consumers and fixtures included, which is what a
    /// real rename looks like — passed this test, `svelte-check` with 0 errors
    /// and all 222 frontend tests, while `Board.svelte` read `undefined` at run
    /// time. That is verbatim the failure the paragraph above claims to
    /// prevent.
    ///
    /// So the search is scoped to the declaration and matched against a
    /// *declaration line* rather than against any occurrence of the text.
    /// [`declares`] is what makes a doc comment inside the block unable to
    /// stand in for the field it documents, and the negative control below is
    /// what makes the slice unable to quietly stop slicing.
    #[test]
    fn the_launcher_home_shape_matches_its_typescript_mirror() {
        let mirror = include_str!("../../../../app/src/lib/ipc/search.ts");
        let home = interface_body(mirror, "LauncherHome");
        let summary = interface_body(mirror, "SmartListSummary");

        // The negative control. `SearchFilters.kinds` is declared in this file
        // and is not a `LauncherHome` field, so a slice that still reaches it
        // is not a slice -- which is exactly the state this test was in, and a
        // rearrangement that did not fix it would look identical from here.
        assert!(
            !declares(home, "kinds"),
            "the LauncherHome slice still reaches SearchFilters.kinds, so it \
             is searching more than the declaration:\n{home}"
        );

        let value = LauncherHome {
            smart_lists: vec![knobas_search::SmartListSummary {
                id: "mine".to_owned(),
                label: "My items".to_owned(),
                count: 3,
                changed: true,
                description: "Yours.".to_owned(),
                saved: false,
                needs_attention: false,
            }],
            recent: Vec::new(),
            sources: Vec::new(),
            pending_writes: 0,
            notes_shown: true,
        };

        let wire = serde_json::to_value(&value).expect("LauncherHome serializes");
        let mut keys: Vec<&str> = wire
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "notes_shown",
                "pending_writes",
                "recent",
                "smart_lists",
                "sources"
            ]
        );
        for key in &keys {
            assert!(
                declares(home, key),
                "`interface LauncherHome` in app/src/lib/ipc/search.ts does \
                 not declare `{key}`, which the Rust type puts on the wire:\
                 \n{home}"
            );
        }

        let list = &wire["smart_lists"][0];
        let mut list_keys: Vec<&str> = list
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();
        list_keys.sort_unstable();
        assert_eq!(
            list_keys,
            [
                "changed",
                "count",
                "description",
                "id",
                "label",
                "needs_attention",
                "saved"
            ]
        );
        for key in &list_keys {
            assert!(
                declares(summary, key),
                "`interface SmartListSummary` in app/src/lib/ipc/search.ts \
                 does not declare `{key}`, which the Rust type puts on the \
                 wire:\n{summary}"
            );
        }
    }

    /// The query box's suggestion lists (#601) and their three element types,
    /// against the hand-written mirror -- the wire's keys, from `serde_json`,
    /// each found as a declaration line inside its own interface.
    #[test]
    fn the_filter_suggestions_shape_matches_its_typescript_mirror() {
        let mirror = include_str!("../../../../app/src/lib/ipc/search.ts");
        let value = knobas_search::FilterSuggestions {
            kinds: vec![knobas_search::KindSuggestion {
                kind: "repo".to_owned(),
                label: "Repo".to_owned(),
            }],
            sources: vec![knobas_search::SourceSuggestion {
                id: "gitea".to_owned(),
                display_name: "Gitea".to_owned(),
            }],
            parents: vec![knobas_search::ParentSuggestion {
                name: "payout-service".to_owned(),
                kind: "repo".to_owned(),
            }],
            fields: vec![knobas_search::FieldSuggestion {
                key: "project".to_owned(),
                value: "PAY".to_owned(),
                detail: "Payments".to_owned(),
            }],
        };
        let wire = serde_json::to_value(&value).expect("FilterSuggestions serializes");
        for (interface, object, expected) in [
            (
                "FilterSuggestions",
                &wire,
                &["fields", "kinds", "parents", "sources"][..],
            ),
            (
                "FieldSuggestion",
                &wire["fields"][0],
                &["detail", "key", "value"][..],
            ),
            ("KindSuggestion", &wire["kinds"][0], &["kind", "label"][..]),
            (
                "SourceSuggestion",
                &wire["sources"][0],
                &["display_name", "id"][..],
            ),
            (
                "ParentSuggestion",
                &wire["parents"][0],
                &["kind", "name"][..],
            ),
        ] {
            let body = interface_body(mirror, interface);
            let mut keys: Vec<&str> = object
                .as_object()
                .expect("an object")
                .keys()
                .map(String::as_str)
                .collect();
            keys.sort_unstable();
            assert_eq!(keys, expected, "{interface}");
            for key in &keys {
                assert!(
                    declares(body, key),
                    "`interface {interface}` in app/src/lib/ipc/search.ts does \
                     not declare `{key}`, which the Rust type puts on the wire:\
                     \n{body}"
                );
            }
        }
    }

    /// The body of `export interface <name> { … }` in the mirror.
    ///
    /// Panics rather than returning an empty slice when the interface is not
    /// there: a rename on the TypeScript side that this could not find would
    /// otherwise turn every assertion above into a vacuous one.
    fn interface_body<'a>(mirror: &'a str, name: &str) -> &'a str {
        let header = format!("export interface {name} {{");
        let start = mirror
            .find(&header)
            .unwrap_or_else(|| panic!("`{header}` is not in app/src/lib/ipc/search.ts"))
            + header.len();
        let rest = &mirror[start..];
        let end = rest
            .find("\n}")
            .unwrap_or_else(|| panic!("`interface {name}` is never closed"));
        &rest[..end]
    }

    /// Whether `body` **declares** `key` — a line whose first token is `key:`.
    ///
    /// Not `contains`. A doc comment inside the block would satisfy a
    /// substring search for the field it documents, so the field could be
    /// deleted and its comment left behind and nothing here would notice.
    fn declares(body: &str, key: &str) -> bool {
        body.lines()
            .any(|line| line.trim_start().starts_with(&format!("{key}:")))
    }

    /// `LauncherHome.recent` and a room's rows are **one** TypeScript type over
    /// **two** Rust structs, so the two have to keep agreeing.
    ///
    /// `knobas_search::EntityRow` (stream E's, the search corpus) and
    /// `knobas_app::commands::entity::EntityRow` (stream D's, a room line) were
    /// declared independently and serialize identically. `app/src/lib/ipc/
    /// search.ts` imports D's declaration rather than repeating it, which is
    /// only honest while that holds -- and neither crate compiles against the
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
        let room = serde_json::to_value(crate::commands::entity::EntityRow {
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
             app/src/lib/ipc/search.ts must stop importing entity.ts's"
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

//! The link writes, the write-back commands and the entity view, and notes.

use super::*;

// -- the link writes --------------------------------------------------------
//
// Two commands, and only two. The *read* stays on `get_entity`: `entries_of`
// is undirected, so an entity's backlinks are the same query as its links and
// need no endpoint of their own, and the target picker reuses `search`.

pub use knobas_core::link::{DEFAULT_RELATION, LinkMutation};

/// Who a link made through this surface belongs to, in `created_by` and in the
/// activity line's `actor`.
///
/// knobas has no identity system: there is one person using it, and the only
/// other actor the log knows is `sync:<source_id>`. Naming it once here keeps
/// the two spellings from drifting.
pub(super) const ACTOR: &str = "user";

/// Draw a link between two entities: [`knobas_core::link::draw`], as
/// [`ACTOR`].
///
/// The behaviour lives below rather than in the command so it is reachable
/// from a test: a `#[tauri::command]` cannot be called directly.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) if either id is not an entity id,
/// or if the two are the same entity;
/// [`NotFound`](crate::IpcErrorCode::NotFound) if either endpoint has no local
/// entity -- linking to something that has not synced yet is an ordinary event,
/// not a fault; [`Conflict`](crate::IpcErrorCode::Conflict) if that pair is
/// already actively linked under that relation;
/// [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
pub async fn create_link_inner(
    pool: &PgPool,
    from_id: &str,
    to_id: &str,
    relation: Option<&str>,
    note: Option<&str>,
) -> Result<LinkMutation, IpcError> {
    // Before any query, as `get_entity_inner` does: a malformed id is a bad
    // address, and the two want different words on screen.
    let from = EntityRef::parse(from_id).map_err(IpcError::invalid)?;
    let to = EntityRef::parse(to_id).map_err(IpcError::invalid)?;
    Ok(knobas_core::link::draw(pool, &from, &to, relation, note, ACTOR).await?)
}

/// Withdraw a link.
///
/// `Ok(None)` when the link was already withdrawn: the store is idempotent, and
/// nothing was mutated, so nothing is written to the log and nothing is
/// announced.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) if `link_id` is not a UUID;
/// [`NotFound`](crate::IpcErrorCode::NotFound) if no link carries it;
/// [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
pub async fn unlink_inner(pool: &PgPool, link_id: &str) -> Result<Option<LinkMutation>, IpcError> {
    let id: Uuid = link_id
        .parse()
        .map_err(|_| IpcError::invalid(format!("{link_id} is not a link id")))?;

    let Some(link) = knobas_core::link::unlink(pool, id).await? else {
        return Ok(None);
    };
    let activity = record_link_activity(pool, "unlinked", &link).await?;
    Ok(Some(LinkMutation {
        link,
        activity,
        superseded: None,
    }))
}

/// One line about `link`, as [`ACTOR`]: [`knobas_core::link::record_activity`].
pub(super) async fn record_link_activity(
    pool: &PgPool,
    verb: &str,
    link: &LinkRow,
) -> Result<ActivityRow, IpcError> {
    Ok(knobas_core::link::record_activity(pool, ACTOR, verb, link).await?)
}

/// Put an activity line on `activity:new`.
///
/// Best-effort, like every other emit in this app: a failure means no window is
/// listening, which is not a reason to fail a write that already landed. The
/// command emits it itself rather than re-reading the log the way the scheduler
/// does -- the write hands its row back precisely so it does not have to guess
/// which of the table's rows is its own.
pub(super) fn announce<R: tauri::Runtime>(app: &tauri::AppHandle<R>, row: ActivityRow) {
    crate::events::emit(app, None, crate::events::ACTIVITY_NEW, row);
}

/// Draw a link between two entities, and announce it.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the database
/// is still coming up, and whatever [`create_link_inner`] refuses with.
#[tauri::command]
pub async fn create_link<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    from_id: String,
    to_id: String,
    relation: Option<String>,
    note: Option<String>,
) -> Result<LinkRow, IpcError> {
    let pool = lifecycle.pool()?;
    let written = create_link_inner(
        &pool,
        &from_id,
        &to_id,
        relation.as_deref(),
        note.as_deref(),
    )
    .await?;
    // The displaced proposal first, so the strip reads in the order the two
    // things happened rather than in the order this function holds them.
    if let Some(line) = written.superseded {
        announce(&app, line);
    }
    announce(&app, written.activity);
    Ok(written.link)
}

/// Withdraw a link, and announce it.
///
/// Idempotent: withdrawing an already-withdrawn link succeeds, writes no second
/// line and announces nothing.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the database
/// is still coming up, and whatever [`unlink_inner`] refuses with.
#[tauri::command]
pub async fn unlink<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    link_id: String,
) -> Result<(), IpcError> {
    let pool = lifecycle.pool()?;
    if let Some(written) = unlink_inner(&pool, &link_id).await? {
        announce(&app, written.activity);
    }
    Ok(())
}

/// Ask a source to change something (issue #43): the one way the UI starts a
/// write-back.
///
/// `payload` is a serialized `knobas_source::WriteOp` -- the same shape
/// [`crate::commands::sources::pending_writes`] hands back and
/// [`crate::commands::sources::amend_write`] takes. Untyped on the wire for
/// #42's ratified reasoning: `WriteOp` grows per milestone (ADR-0006), and a
/// typed argument would drag the SPI's enum onto the IPC surface and make
/// every growth an IPC change.
///
/// **The write is queued, not sent** -- and the row that comes back is the
/// write *as queued*, before the attempt, because that is what happened.
/// Since issue #752 the answer comes as soon as the row exists: the send runs
/// in the background (`sources::write_queue::submit_in_background`), so a
/// dialog closes on the local insert rather than on a source's round trip.
/// Read the outcome back with `pending_writes`, or watch `activity:new`: every
/// queue transition writes a line, its `detail.write_id` naming the write. A UI that reported "sent" from this return
/// value would be reporting a hope. The queue is what decides send, pend or
/// hold (issue #42), and there is no path around it.
///
/// **There is no `source_id` argument.** Interfaces §4.1 makes the instance id
/// and the `EntityRef` namespace the same string, so the target already names
/// the source; a second argument could only agree or contradict, and a
/// contradiction would aim a write at a source the target does not belong to.
///
/// # Errors
///
/// `invalid` if the payload is not a write op, if its target is not an entity
/// id, or if the source does not offer that op; `not_found` if the target
/// names a source that is not configured; `conflict` for a `CreateMonitor`
/// whose name has a create on its way to that source, pending, held as
/// unconfirmed (#1966) or sent and not mirrored yet (#1959);
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up.
///
/// **`link_to`** (#984, a ticket's entity id) asks for a link from what a
/// `CreateBranch`, `CreatePullRequest` or `CreatePage` makes to that ticket,
/// or from the ticket a `CreateTicket` files to `link_to` (#1451, the failed
/// run it was filed from), drawn once the write has been sent and a sync of
/// the source brings it in, on this send or a later tick (#1050: the intent is
/// held on the queued row, `sources::write_queue::submit_linked_in_background`);
/// `invalid` for any other op, `not_found` for an entity the mirror does not
/// hold. Absent, the command is what it was.
#[tauri::command]
pub async fn submit_write<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    payload: serde_json::Value,
    link_to: Option<String>,
) -> Result<knobas_core::write_queue::QueuedWrite, IpcError> {
    let state = crate::sources::state(&app)?;
    match link_to {
        None => crate::sources::write_queue::submit_in_background(&state, payload).await,
        Some(ticket) => {
            crate::sources::write_queue::submit_linked_in_background(&state, payload, ticket).await
        }
    }
}

/// The tickets filed from `entity_id` that it does not link yet (#1953): each
/// `CreateTicket` queued with `entity_id` as its `link_to` (a failed run's
/// *Create ticket*, #1451) whose row still owes that link and that was not
/// withdrawn, oldest first, as the queue holds it now. A held or refused one
/// is named too, with its state and held side; the page decides how it draws
/// it, and draws one held as unconfirmed as on its way (#1966).
///
/// The read a page opens with to know which of its creates are on their way:
/// the page's own marks die with a reload, the row does not, so a reload
/// before the sync that mirrors the ticket draws *Ticket queued* rather than
/// offering *Create ticket* again.
///
/// **It resolves before it answers** ([`crate::protocol::tickets_owing`]): a
/// sent create whose ticket the mirror now holds is linked and owed no more.
///
/// # Errors
///
/// `internal` if a read or a write fails.
pub async fn owed_tickets_inner(
    pool: &PgPool,
    declarations: &knobas_core::payload::Declarations,
    entity_id: &str,
) -> Result<Vec<knobas_core::write_queue::QueuedWrite>, IpcError> {
    crate::protocol::tickets_owing(pool, declarations, entity_id).await
}

/// [`owed_tickets_inner`] behind the sync engine, over its declarations.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before bring-up, else as
/// [`owed_tickets_inner`].
#[tauri::command]
pub async fn owed_tickets<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    entity_id: String,
) -> Result<Vec<knobas_core::write_queue::QueuedWrite>, IpcError> {
    let state = crate::sources::state(&app)?;
    let declarations =
        crate::sources::paths::declared_paths(&state.pool, state.registry.as_ref()).await?;
    owed_tickets_inner(&state.pool, &declarations, &entity_id).await
}

/// The statuses this ticket's workflow offers from where it stands **right
/// now** (#498, spec #491's stream 3).
///
/// The read the status select was built without. Until this existed the select
/// offered the statuses the source's *corpus* had been seen to use -- every
/// column the mini board draws for that source -- and the adapter discovered at
/// write time that half of them were unreachable, which reached the reader as a
/// refused row in the write queue rather than as an offer that was never made.
/// This asks the source instead.
///
/// **A read straight through to the adapter, with no mirror in it and nothing
/// stored.** `CONTEXT.md`'s *reachable transition* is an answer about now, so
/// there is no row to cache it in and no event to invalidate one with; the
/// answer is good for as long as the detail is open, and the write's own
/// resolution -- unchanged -- is what still decides whether the move happens.
///
/// **A rejection is the ordinary case, not a fault.** A source with no workflow
/// refuses by name (battery clause 8), which is what a Gitea pull request, a
/// build, a page and a monitor get; a source whose credential died answers
/// `unauthorized`. The shell draws no select for a kind that has no workflow
/// and falls back to the corpus-observed offer, marked *offer unverified*, for
/// a read that failed on a ticket that does have one. Neither is an empty list:
/// an empty answer is a real workflow with no moves left, and the two must not
/// collapse.
///
/// **There is no `source_id` argument**, for `submit_write`'s reason above: the
/// entity id already names the source.
///
/// # Errors
///
/// `invalid` if `entity_id` is not an entity id, `not_found` if its namespace
/// is not a configured source, `unauthorized` / `unreachable` / `internal` as
/// the adapter's own failure maps -- including the refusal of a source with no
/// workflow, which is `internal` because it is a caller asking the wrong
/// question rather than anything the user can act on;
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up.
///
/// **`behind`** (ADR-0054): a read the board asks ahead of a press -- the
/// pointer resting on a card, a key selecting one -- goes in the background
/// lane, as [`entity_view`]'s does, so a person's own read goes first. Left
/// out, the read is a person's: a press, a pickup, a ticket page opening.
#[tauri::command]
pub async fn reachable_transitions<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    entity_id: String,
    behind: Option<bool>,
) -> Result<Vec<String>, IpcError> {
    let state = crate::sources::state(&app)?;
    if behind == Some(true) {
        crate::logging::mark_read_behind();
    }
    let read = crate::sources::crud::reachable_transitions(
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

/// The **source view** of one entity (#661, ADR-0020): what its adapter draws
/// for its kind, read live when the detail opens and never stored in the
/// database. A view the adapter says stands at a fixed commit is kept in
/// memory for the session and answered from there (ADR-0032, #2239).
///
/// `section: None` reads the whole view, deferred sections as stubs;
/// `Some(id)` reads that one deferred section when the reader opens its tab.
/// Routed like [`reachable_transitions`]: the entity id names the source, so
/// there is no `source_id` argument.
///
/// **Three answers, and the shell draws each differently** (rule 2): a view is
/// drawn as tabs with the generic projection behind *Raw*; `null` is an
/// adapter with nothing to draw for this kind, and the panel is what it was
/// before views existed; a rejection draws the same panel under one line,
/// *Showing the mirror: <message>*. An entity-list row keeps its address only
/// where the mirror holds that entity (rule 7, `sources::crud::view`).
///
/// **What knobas last read, at once** (ADR-0042): the answer is what knobas
/// held where it holds one, with `held_at` saying when it was read, and the
/// source is read behind it and told on `view:fresh`; only a view never read
/// waits for the source ([`crate::sources::crud::shown`]).
///
/// **`behind`** (the ux16 overs pass, 2026-11-17): a view the page draws
/// beside a read it waits on, not instead of it -- Compare's branch standing
/// beside the compare's list. Its requests go in the background lane
/// ([`knobas_http::Lane::Background`]), so they reach the source once the
/// page's own are answered: on Björn's PC the standing's eight requests went
/// out with the compare's two and Gitea answered the compare in 600 ms at 6x
/// where it answers it in 40 to 90 alone. What knobas holds is answered at
/// once either way. Left out, the read is a person's, as before.
///
/// # Errors
///
/// `invalid` if `entity_id` is not an entity id, `not_found` if its namespace
/// is not a configured source, `unauthorized` / `unreachable` / `internal` as
/// the adapter's own failure maps;
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up.
#[tauri::command]
pub async fn entity_view<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    entity_id: String,
    section: Option<String>,
    behind: Option<bool>,
) -> Result<Option<crate::sources::live::ShownView>, IpcError> {
    let state = crate::sources::state(&app)?;
    if behind == Some(true) {
        crate::logging::mark_read_behind();
    }
    let read = crate::sources::crud::shown(
        &state.pool,
        &state.secrets,
        &state.registry,
        &state.live,
        &entity_id,
        section.as_deref(),
    );
    let answer = if behind == Some(true) {
        knobas_http::behind(read).await
    } else {
        read.await
    };
    answer.map_err(|error| crate::sources::to_ipc(&error, None))
}

/// The diff from `base` to `head` of the repository `entity_id` (#2272,
/// ADR-0036): read by local read-only git when its checkout holds both
/// commits, with whitespace ignored when `ignore_whitespace` asks; otherwise
/// the source's compare read, kept by sha pair. `read_at` says which, and so
/// whether whitespace could be ignored.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// and what the source's read refuses with when the source answers.
#[tauri::command]
pub async fn entity_range_diff<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    entity_id: String,
    base: String,
    head: String,
    ignore_whitespace: bool,
) -> Result<crate::checkout::RangeDiff, IpcError> {
    let state = crate::sources::state(&app)?;
    crate::checkout::range_diff(
        &state.pool,
        &state.secrets,
        &state.registry,
        &state.live,
        &entity_id,
        &base,
        &head,
        ignore_whitespace,
    )
    .await
}

/// The diff from `base` to `head` of the repository `entity_id` as its
/// source's compare answers it now, never read locally (ADR-0041): what a
/// page that drew `entity_range_diff`'s predicted answer (`predicted_at`)
/// asks behind it, to learn whether a branch moved since the mirror's sync.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// and what the source's read refuses with.
#[tauri::command]
pub async fn entity_source_range_diff<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    entity_id: String,
    base: String,
    head: String,
) -> Result<crate::checkout::RangeDiff, IpcError> {
    let state = crate::sources::state(&app)?;
    crate::checkout::source_range_diff(
        &state.pool,
        &state.secrets,
        &state.registry,
        &state.live,
        &entity_id,
        &base,
        &head,
    )
    .await
}

/// The files `base`...`head` of the repository `entity_id` changes, without
/// their lines (the compare pass of PR #2479, 2026-11-13): read where
/// [`entity_range_diff`] reads the whole diff, and answered in a fraction of
/// its bytes, so a page lists a range of thousands of files at once and reads
/// each file's lines with [`entity_range_file`] as it reaches it.
///
/// `read` names the read for [`cancel_read`], which calls it off.
///
/// # Errors
///
/// As [`entity_range_diff`];
/// [`Conflict`](crate::IpcErrorCode::Conflict) for a read called off.
#[tauri::command]
pub async fn entity_range_files<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    entity_id: String,
    base: String,
    head: String,
    ignore_whitespace: bool,
    read: Option<String>,
) -> Result<crate::checkout::range_files::RangeFiles, IpcError> {
    let state = crate::sources::state(&app)?;
    let (pool, secrets, registry, live) = (
        state.pool.clone(),
        state.secrets.clone(),
        state.registry.clone(),
        state.live.clone(),
    );
    crate::reads::run(read, async move {
        crate::checkout::range_files::range_files(
            &pool,
            &secrets,
            &registry,
            &live,
            &entity_id,
            &base,
            &head,
            ignore_whitespace,
        )
        .await
    })
    .await
}

/// The names of the files `base`...`head` of the repository `entity_id`
/// changes, without their counts (the stored-compare pass of PR #2479,
/// 2026-11-14): at the checkout, between the tips the mirror holds for two
/// branch names, as [`entity_range_files`] predicts there, in one `git diff
/// --raw -M` that reads no file's content. `None` where the checkout cannot
/// answer, or [`entity_range_files`] holds the list already. A page draws it
/// while that list, read beside it, brings the counts.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before bring-up.
#[tauri::command]
pub async fn entity_range_names<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    entity_id: String,
    base: String,
    head: String,
) -> Result<Option<crate::checkout::range_files::RangeNames>, IpcError> {
    let state = crate::sources::state(&app)?;
    Ok(crate::checkout::range_files::range_names(
        &state.pool,
        state.registry.as_ref(),
        &entity_id,
        &base,
        &head,
    )
    .await)
}

/// One page of one file's lines in `base`...`head` of the repository
/// `entity_id` (the compare pass of PR #2479): the ends are the `range`
/// [`entity_range_files`] answered (the names it was asked by where it
/// settled none), `path` and `old_path` the file as it listed it, `from` the
/// line the page starts at (`0`, then each page's `next`).
///
/// # Errors
///
/// [`NotFound`](crate::IpcErrorCode::NotFound) for a file the diff does not
/// hold; otherwise as [`entity_range_files`].
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn entity_range_file<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    entity_id: String,
    base: String,
    head: String,
    path: String,
    old_path: Option<String>,
    ignore_whitespace: bool,
    from: u32,
    read: Option<String>,
) -> Result<crate::checkout::range_files::RangeFilePage, IpcError> {
    let state = crate::sources::state(&app)?;
    let (pool, secrets, registry, live) = (
        state.pool.clone(),
        state.secrets.clone(),
        state.registry.clone(),
        state.live.clone(),
    );
    crate::reads::run(read, async move {
        crate::checkout::range_files::range_file(
            &pool,
            &secrets,
            &registry,
            &live,
            &entity_id,
            &base,
            &head,
            &path,
            old_path.as_deref(),
            ignore_whitespace,
            from,
        )
        .await
    })
    .await
}

/// The first page of each of `files` in `base`...`head` of the repository
/// `entity_id`, in one read (the big-compare pass of PR #2479, 2026-11-16):
/// what [`entity_range_file`] answers for each from line 0, in the order
/// asked, one start of git for all of them; `None` for a file it could not
/// answer whole, which the page reads alone. At most
/// [`PAGES_AT_ONCE`](crate::checkout::range_files::PAGES_AT_ONCE) files.
/// `read` as there.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) past that many files;
/// otherwise as [`entity_range_file`].
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn entity_range_file_pages<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    entity_id: String,
    base: String,
    head: String,
    files: Vec<crate::checkout::range_files::RangeFileAsk>,
    ignore_whitespace: bool,
    read: Option<String>,
) -> Result<Vec<Option<crate::checkout::range_files::RangeFilePage>>, IpcError> {
    let state = crate::sources::state(&app)?;
    let (pool, secrets, registry, live) = (
        state.pool.clone(),
        state.secrets.clone(),
        state.registry.clone(),
        state.live.clone(),
    );
    crate::reads::run(read, async move {
        crate::checkout::range_files::range_file_pages(
            &pool,
            &secrets,
            &registry,
            &live,
            &entity_id,
            &base,
            &head,
            &files,
            ignore_whitespace,
        )
        .await
    })
    .await
}

/// The files `base`...`head` changes as the source's compare answers it now,
/// never local, without their lines: what a page that drew
/// [`entity_range_files`]' predicted answer asks behind it (ADR-0041), as
/// [`entity_source_range_diff`] for the whole diff. `read` as there.
///
/// # Errors
///
/// As [`entity_source_range_diff`];
/// [`Conflict`](crate::IpcErrorCode::Conflict) for a read called off.
#[tauri::command]
pub async fn entity_source_range_files<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    entity_id: String,
    base: String,
    head: String,
    read: Option<String>,
) -> Result<crate::checkout::range_files::RangeFiles, IpcError> {
    let state = crate::sources::state(&app)?;
    let (pool, secrets, registry, live) = (
        state.pool.clone(),
        state.secrets.clone(),
        state.registry.clone(),
        state.live.clone(),
    );
    crate::reads::run(read, async move {
        crate::checkout::range_files::source_range_files(
            &pool, &secrets, &registry, &live, &entity_id, &base, &head,
        )
        .await
    })
    .await
}

/// Call off the read the page named `read` (`entity_range_files`,
/// `entity_range_file`): its work stops where it stands, a request to the
/// source abandoned and a `git` it started killed. Whether it was still
/// running.
#[tauri::command]
pub fn cancel_read(read: String) -> bool {
    crate::reads::cancel(&read)
}

/// The change commit `commit_id` (whole or short `sha`) made, for the
/// commit tab (#2319, story 78): read by local read-only git when the
/// checkout of repository `entity_id` holds it, with whitespace ignored when
/// asked; otherwise the commit's own `diff` section, kept by sha.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// and what the source's read refuses with when the source answers.
#[tauri::command]
pub async fn entity_commit_diff<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    entity_id: String,
    commit_id: String,
    sha: String,
    ignore_whitespace: bool,
) -> Result<crate::checkout::RangeDiff, IpcError> {
    let state = crate::sources::state(&app)?;
    crate::checkout::commit_diff(
        &state.pool,
        &state.secrets,
        &state.registry,
        &state.live,
        &entity_id,
        &commit_id,
        &sha,
        ignore_whitespace,
    )
    .await
}

/// The bytes of the file at `path` in `entity_id`'s tree (#2256, spec #2234
/// step 5): a README's relative image, a PDF. `commit` is the `commit` of the
/// file block the reader has open, handed back, so a README's image is read
/// at the README's commit; `None` reads at the entity's own reference.
///
/// Answered raw ([`tauri::ipc::Response`]: an `ArrayBuffer` in the shell,
/// not a JSON array four times the size), whole and never cut. A file over
/// [`BYTES_CAP`](knobas_source::view::BYTES_CAP) is refused with its size.
/// Kept for the session where the adapter says the read is fixed (a whole
/// sha), so a second look costs no request (ADR-0032).
///
/// # Errors
///
/// See [`entity_file_bytes_inner`].
#[tauri::command]
pub async fn entity_file_bytes<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    entity_id: String,
    path: String,
    commit: Option<String>,
) -> Result<tauri::ipc::Response, IpcError> {
    let state = crate::sources::state(&app)?;
    entity_file_bytes_inner(
        &state.pool,
        &state.secrets,
        &state.registry,
        &state.live,
        &entity_id,
        &path,
        commit.as_deref(),
    )
    .await
    .map(tauri::ipc::Response::new)
}

/// [`entity_file_bytes`]'s body, reachable from a test.
///
/// # Errors
///
/// `invalid` for a file over the cap, its size and the cap in the message,
/// and for a source that reads no bytes; `not_found` where the source
/// answers 404 for the path (no file there); otherwise as [`entity_view`].
pub async fn entity_file_bytes_inner(
    pool: &PgPool,
    secrets: &std::sync::Arc<dyn knobas_secrets::SecretStore>,
    registry: &std::sync::Arc<dyn knobas_sync::scheduler::AdapterRegistry>,
    live: &crate::sources::live::LiveSources,
    entity_id: &str,
    path: &str,
    commit: Option<&str>,
) -> Result<Vec<u8>, IpcError> {
    use knobas_source::view::{BYTES_CAP, FileBytes};
    // The working tree's or the index's file (ux26): read at the checkout, never the source.
    if let Some(end) = commit
        .filter(|c| *c == crate::checkout::working::END || *c == crate::checkout::working::INDEX)
    {
        return working_bytes(pool, entity_id, end, path).await;
    }
    // A file at a sha the checkout holds is read there (ADR-0041): a README's
    // images, a PDF, with no request to the source.
    if let Some(commit) = commit {
        match crate::checkout::local_file_bytes(pool, entity_id, commit, path, BYTES_CAP).await {
            Some(Ok(bytes)) => return Ok(bytes),
            Some(Err(size)) => {
                return Err(IpcError::invalid(format!(
                    "{path} is {}; knobas reads a file's bytes up to {}",
                    mebibytes(size),
                    mebibytes(BYTES_CAP)
                )));
            }
            None => {}
        }
    }
    let read =
        crate::sources::crud::file_bytes(pool, secrets, registry, live, entity_id, path, commit)
            .await
            .map_err(|error| match error {
                // The source has no file at `path` there (#2279 merger, witnessed on
                // Gitea 1.27.3: a raw read of a path absent at a sha is a 404). A
                // rendered diff of a binary file reads that as "no file on that side".
                crate::sources::SourcesError::Source(knobas_source::SourceError::Protocol {
                    status: Some(404),
                    ..
                }) => IpcError::not_found(format!("{entity_id} holds no file {path}")),
                other => crate::sources::to_ipc(&other, None),
            })?;
    match read.as_deref() {
        Some(FileBytes::Read(bytes)) => Ok(bytes.clone()),
        Some(FileBytes::TooLarge { size }) => Err(IpcError::invalid(format!(
            "{path} is {}; knobas reads a file's bytes up to {}",
            mebibytes(*size),
            mebibytes(BYTES_CAP)
        ))),
        None => Err(IpcError::invalid(format!(
            "{entity_id}'s source reads no file bytes"
        ))),
    }
}

/// A pipeline over the bytes of the file at `path` in `entity_id`'s tree
/// (#2264, spec #2234 step 5): `steps` in order, each on the last one's
/// output, the last one's drawn by the renderer its name picks. Read as
/// [`entity_file_bytes`] reads; a run over a file at a whole sha is kept, so
/// the same pipeline again costs nothing (ADR-0032). Every step is capped at
/// [`SIZE_CAP`](crate::file_pipeline::SIZE_CAP) of output and
/// [`STEP_TIMEOUT`](crate::file_pipeline::STEP_TIMEOUT).
///
/// # Errors
///
/// See [`entity_file_piped_inner`].
#[tauri::command]
pub async fn entity_file_piped<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    entity_id: String,
    path: String,
    commit: Option<String>,
    steps: Vec<crate::file_pipeline::PipeStep>,
) -> Result<crate::file_pipeline::PipeRun, IpcError> {
    let state = crate::sources::state(&app)?;
    entity_file_piped_inner(
        &state.pool,
        &state.secrets,
        &state.registry,
        &state.live,
        &entity_id,
        &path,
        commit.as_deref(),
        &steps,
    )
    .await
}

/// [`entity_file_piped`]'s body, reachable from a test.
///
/// # Errors
///
/// As [`entity_file_bytes_inner`]: the read's refusals. A step that fails is
/// an answer ([`PipeRun::Failed`](crate::file_pipeline::PipeRun::Failed)).
#[allow(clippy::too_many_arguments)]
pub async fn entity_file_piped_inner(
    pool: &PgPool,
    secrets: &std::sync::Arc<dyn knobas_secrets::SecretStore>,
    registry: &std::sync::Arc<dyn knobas_sync::scheduler::AdapterRegistry>,
    live: &crate::sources::live::LiveSources,
    entity_id: &str,
    path: &str,
    commit: Option<&str>,
    steps: &[crate::file_pipeline::PipeStep],
) -> Result<crate::file_pipeline::PipeRun, IpcError> {
    use crate::file_pipeline::{Limits, Piped, run_on_file};
    // A pipeline over the working tree's or the index's file (ux26): its bytes from the checkout, run here.
    if let Some(end) = commit
        .filter(|c| *c == crate::checkout::working::END || *c == crate::checkout::working::INDEX)
    {
        let bytes = working_bytes(pool, entity_id, end, path).await?;
        let commands = if steps
            .iter()
            .any(|step| matches!(step, crate::file_pipeline::PipeStep::Command { .. }))
        {
            crate::file_pipeline::saved::command_steps(pool).await?
        } else {
            Vec::new()
        };
        return Ok(crate::file_pipeline::run_bytes_with(
            steps,
            &commands,
            bytes,
            path,
            Limits::default(),
        )
        .await);
    }
    let piped = run_on_file(
        pool,
        secrets,
        registry,
        live,
        entity_id,
        path,
        commit,
        steps,
        Limits::default(),
    )
    .await
    .map_err(|error| crate::sources::to_ipc(&error, None))?;
    match piped {
        Piped::Ran(run) => Ok(run.as_ref().clone()),
        Piped::TooLarge { size } => Err(IpcError::invalid(format!(
            "{path} is {}; knobas reads a file's bytes up to {}",
            mebibytes(size),
            mebibytes(knobas_source::view::BYTES_CAP)
        ))),
        Piped::NoBytes => Err(IpcError::invalid(format!(
            "{entity_id}'s source reads no file bytes"
        ))),
    }
}

/// *Download* in the value viewer (the XML view's design, section 19): what
/// `steps` gave over `path`, written to the Downloads folder under `name` (a
/// free name beside any file there); answers the path. The run is the kept one
/// where it is kept; nothing is written anywhere else, and a run that failed is
/// refused with its failure.
///
/// # Errors
///
/// As [`entity_file_piped_inner`]; a failed run; the folder or the write.
#[tauri::command]
pub async fn entity_file_piped_save<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    entity_id: String,
    path: String,
    commit: Option<String>,
    steps: Vec<crate::file_pipeline::PipeStep>,
    name: String,
) -> Result<String, IpcError> {
    use base64::Engine as _;
    use tokio::io::AsyncWriteExt as _;
    let state = crate::sources::state(&app)?;
    let run = entity_file_piped_inner(
        &state.pool,
        &state.secrets,
        &state.registry,
        &state.live,
        &entity_id,
        &path,
        commit.as_deref(),
        &steps,
    )
    .await?;
    let bytes = match run {
        crate::file_pipeline::PipeRun::Done { output, .. } => match output {
            crate::file_pipeline::PipeOutput::Text { text } => text.into_bytes(),
            crate::file_pipeline::PipeOutput::Bytes { base64, .. } => {
                base64::engine::general_purpose::STANDARD
                    .decode(base64)
                    .map_err(IpcError::internal)?
            }
        },
        crate::file_pipeline::PipeRun::Failed { failure } => {
            return Err(IpcError::invalid(format!(
                "step {}, {}, failed: {}",
                failure.index + 1,
                failure.name,
                failure.error
            )));
        }
    };
    let dir = dirs::download_dir().ok_or_else(|| {
        IpcError::new(
            crate::IpcErrorCode::Internal,
            "This computer has no Downloads folder.",
        )
    })?;
    let (saved, mut file) =
        crate::downloads::create_free(&dir, &crate::downloads::safe_name(&name))
            .await
            .map_err(|(at, error)| IpcError::internal(format!("{}: {error}", at.display())))?;
    file.write_all(&bytes)
        .await
        .map_err(|error| IpcError::internal(format!("{}: {error}", saved.display())))?;
    Ok(saved.display().to_string())
}

/// The bytes of `path` at the working tree or the index of `entity_id`'s
/// checkout, as [`entity_file_bytes_inner`] answers them.
async fn working_bytes(
    pool: &PgPool,
    entity_id: &str,
    end: &str,
    path: &str,
) -> Result<Vec<u8>, IpcError> {
    use crate::checkout::working::Bytes;
    let cap = knobas_source::view::BYTES_CAP;
    match crate::checkout::working_file_bytes(pool, entity_id, end, path, cap).await? {
        Bytes::Read(bytes) => Ok(bytes),
        Bytes::TooLarge(size) => Err(IpcError::invalid(format!(
            "{path} is {}; knobas reads a file's bytes up to {}",
            mebibytes(size),
            mebibytes(cap)
        ))),
        Bytes::NotThere => Err(IpcError::not_found(format!(
            "git lists no file {path} in {}",
            if end == crate::checkout::working::INDEX {
                "the index"
            } else {
                "the working tree"
            }
        ))),
    }
}

/// `size` bytes in MiB, to one place where it is not whole.
pub(super) fn mebibytes(size: u64) -> String {
    #[allow(clippy::cast_precision_loss)]
    let mib = size as f64 / (1024.0 * 1024.0);
    if mib.fract() == 0.0 {
        format!("{mib:.0} MiB")
    } else {
        format!("{mib:.1} MiB")
    }
}

/// The writes among `asks` their sources' credentials cannot make, each with
/// its reason (#2040, ADR-0020 rule 4): a surface that draws a write from the
/// template `write_ops` -- the inbox's *Re-run*, the Builds frame's *Run…* --
/// asks this after drawing, and draws each refused one disabled with the
/// reason.
///
/// Read live from each entity's own source and never stored, like
/// [`entity_view`]; nothing is sent to find out. An ask the answer leaves out
/// is no verdict and keeps its drawing: a source that cannot tell, whose read
/// failed, or that no configuration names.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// and nothing else: see [`crate::sources::crud::refused_writes`].
#[tauri::command]
pub async fn refused_writes<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    asks: Vec<knobas_source::grants::WriteAsk>,
) -> Result<Vec<knobas_source::grants::RefusedWrite>, IpcError> {
    let state = crate::sources::state(&app)?;
    Ok(crate::sources::crud::refused_writes(
        &state.pool,
        &state.secrets,
        &state.registry,
        &state.live,
        &asks,
    )
    .await)
}

/// Everything the note view draws for one note (#46).
///
/// Deliberately **not** [`EntityDetail`], and the difference is not tidiness:
/// that DTO is shaped around a mirrored item -- a source, a `payload`, a
/// `web_url`, a `synced_at` -- and a note has none of those. Half of it would
/// be filled in with placeholders that a reader could not tell from real ones.
/// A note is an entity and shares the address space; it is not a mirror row.
///
/// `refs` and `links` overlap and both are here because they answer different
/// questions. `refs` is *the body's own list*, in body order, including the
/// ones that resolve to nothing -- which is what the editor draws chips from
/// and what makes an unresolved ref visible (story 10). `links` is the panel
/// #53 built: every link this note takes part in, in either direction, so the
/// note shows what points at it as well as what it points at.
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct NoteDetail {
    pub note: knobas_core::note::NoteRow,
    /// The `[[refs]]` the body names, in body order, resolved where they
    /// resolve.
    pub refs: Vec<knobas_core::note::NoteRef>,
    /// Every link this note takes part in, each with its other end resolved --
    /// the ref links it derived, plus anything drawn by hand from either side.
    pub links: Vec<LinkEntry>,
}

/// The note at `note_id`, its refs and its links.
///
/// Three round trips, like [`get_entity_inner`]'s: the note, the refs, the
/// links.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) if `note_id` is not an entity id,
/// [`NotFound`](crate::IpcErrorCode::NotFound) if no note carries it,
/// [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
pub async fn get_note_inner(pool: &PgPool, note_id: &str) -> Result<NoteDetail, IpcError> {
    let id = EntityRef::parse(note_id).map_err(IpcError::invalid)?;
    let note = knobas_core::note::get(pool, &id)
        .await?
        .ok_or_else(|| IpcError::not_found(format!("there is no note at {id}")))?;
    Ok(NoteDetail {
        note,
        refs: knobas_core::note::refs_of(pool, &id).await?,
        links: knobas_core::link::entries_of(pool, &id).await?,
    })
}

/// One link a caller asks a new note to be born with (#502).
///
/// An **input** DTO: the frontend sends these and nothing sends them back. It
/// is a pair rather than two named fields (`capturedIn`, `capturedFrom`)
/// because the command's job is *draw these links with the note*, and the two
/// words are the caller's vocabulary, not the command's -- exactly as
/// [`create_link_inner`] takes a relation rather than knowing what `blocks`
/// means. The capture window (#503) and the in-app *New note* pass the same
/// two, and a third caller with a third relation needs no argument here.
///
/// The note is always the **from** end, so `captured-in` reads *captured in*
/// from the note and *captured here* from the context; see
/// [`knobas_core::note::BornLink`].
#[derive(Debug, Clone, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct NoteLinkInput {
    /// The other end's entity id.
    pub target_id: String,
    /// The relation the link carries, folded to lower case like every other
    /// relation this module writes.
    ///
    /// Required, and blank is refused rather than defaulted. `create_link`'s
    /// relation is an `Option` and its absence means *nobody named one*, which
    /// [`DEFAULT_RELATION`] answers; here the caller is drawing a link the
    /// reader never saw a dialog for, so `""` is a caller bug and a link
    /// labelled `related` that nobody asked for would be the wrong sentence in
    /// the panel rather than an error anybody could find.
    pub relation: String,
}

/// Write a new note, and the links it is born with.
///
/// The first two arguments are optional because the affordance is *"start
/// writing"*: a note created from an empty editor has no title and no body yet,
/// and it still has to exist -- story 2 is that a thought is never lost to a
/// closed window, which needs the row to be there before the first keystroke
/// settles.
///
/// `links` is what a **capture** attaches (`CONTEXT.md`, **Capture**; spec #491
/// stories 40--43): `captured-in` to the context of the stored room the reader
/// stands in -- which makes the note a member of it by ADR-0008, since an
/// explicit add is an ordinary link touching the context's own `ctx:` entity --
/// and `captured-from` to the **foreground**, the word as `CONTEXT.md`'s
/// **Passive attribution** defines it and as the heartbeat computes it: the
/// open detail, else the room's anchor, else nothing (the deputy's ruling of
/// 2026-09-08 on #502). A derived room has no context (`CONTEXT.md`, **Room**),
/// so a note born in one is born with nothing, and that is a caller's decision
/// rather than a rule here: this command draws the links it is handed.
///
/// They are drawn in the note's own transaction, which is what the ticket's
/// title means -- see [`knobas_core::note::create`], which also states what
/// happens to a target that no longer exists (no link, and the note is still
/// written).
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) if a link's target is not an
/// entity id, or if its relation is blank -- both are caller bugs, and both are
/// worth refusing loudly, since every id this command is handed came from a row
/// the caller was already drawing and every relation from its own vocabulary;
/// [`Internal`](crate::IpcErrorCode::Internal) for a write failure.
pub async fn create_note_inner(
    pool: &PgPool,
    title: Option<&str>,
    body_md: Option<&str>,
    links: &[NoteLinkInput],
) -> Result<NoteDetail, IpcError> {
    let born_with = links
        .iter()
        .map(|link| {
            Ok(knobas_core::note::BornLink {
                target: EntityRef::parse(&link.target_id).map_err(IpcError::invalid)?,
                relation: knobas_core::link::relation_named(&link.relation).ok_or_else(|| {
                    IpcError::invalid("a link drawn with a note needs a relation")
                })?,
            })
        })
        .collect::<Result<Vec<_>, IpcError>>()?;
    let note = knobas_core::note::create(
        pool,
        title.unwrap_or_default(),
        body_md.unwrap_or_default(),
        &born_with,
        ACTOR,
    )
    .await?;
    get_note_inner(pool, &note.id).await
}

/// Save a note's title and body, and bring its `[[refs]]` back into step.
///
/// This is what the editor's autosave calls, so it answers with the whole
/// [`NoteDetail`]: the refs it just reconciled are what the editor redraws its
/// chips from, and a second read to fetch them would race the next keystroke.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) if `note_id` is not an entity id,
/// [`NotFound`](crate::IpcErrorCode::NotFound) if no note carries it -- an
/// editor whose note was deleted elsewhere is told so rather than silently
/// resurrecting it, [`Internal`](crate::IpcErrorCode::Internal) for a write
/// failure.
pub async fn save_note_inner(
    pool: &PgPool,
    note_id: &str,
    title: &str,
    body_md: &str,
) -> Result<NoteDetail, IpcError> {
    let id = EntityRef::parse(note_id).map_err(IpcError::invalid)?;
    knobas_core::note::save(pool, &id, title, body_md, ACTOR)
        .await?
        .ok_or_else(|| IpcError::not_found(format!("there is no note at {id}")))?;
    get_note_inner(pool, note_id).await
}

/// Delete a note: the body goes, the address stays, tombstoned.
///
/// `Ok(false)` when there was nothing to delete. Idempotent, like
/// [`unlink_inner`]: a second *Delete* on a note already gone is not an error,
/// and the caller can tell the two apart -- and, like `unlink_inner`, the call
/// that mutated nothing writes no activity line.
///
/// The `deleted` line the first call *does* write is the store's
/// ([`knobas_core::note::delete`], #524), inside the transaction that took the
/// body; [`ACTOR`] is who it is attributed to, the same word every other
/// mutation in this module signs with.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) if `note_id` is not an entity id,
/// [`Internal`](crate::IpcErrorCode::Internal) for a write failure.
pub async fn delete_note_inner(pool: &PgPool, note_id: &str) -> Result<bool, IpcError> {
    let id = EntityRef::parse(note_id).map_err(IpcError::invalid)?;
    Ok(knobas_core::note::delete(pool, &id, ACTOR).await?)
}

/// One note, with its refs and its links.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the database
/// is still coming up, and whatever [`get_note_inner`] refuses with.
#[tauri::command]
pub async fn get_note(
    lifecycle: State<'_, Lifecycle>,
    note_id: String,
) -> Result<NoteDetail, IpcError> {
    let pool = lifecycle.pool()?;
    get_note_inner(&pool, &note_id).await
}

/// Write a new note, with the links it is born with.
///
/// `links` is absent for a caller that attaches none, which reads the same as
/// an empty list and is spelled that way so *New note* in a derived room sends
/// nothing rather than sending emptiness.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the database
/// is still coming up, and whatever [`create_note_inner`] refuses with.
#[tauri::command]
pub async fn create_note(
    lifecycle: State<'_, Lifecycle>,
    title: Option<String>,
    body_md: Option<String>,
    links: Option<Vec<NoteLinkInput>>,
) -> Result<NoteDetail, IpcError> {
    let pool = lifecycle.pool()?;
    create_note_inner(
        &pool,
        title.as_deref(),
        body_md.as_deref(),
        links.as_deref().unwrap_or_default(),
    )
    .await
}

/// Save a note, refs and all.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the database
/// is still coming up, and whatever [`save_note_inner`] refuses with.
#[tauri::command]
pub async fn save_note(
    lifecycle: State<'_, Lifecycle>,
    note_id: String,
    title: String,
    body_md: String,
) -> Result<NoteDetail, IpcError> {
    let pool = lifecycle.pool()?;
    save_note_inner(&pool, &note_id, &title, &body_md).await
}

/// Delete a note.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the database
/// is still coming up, and whatever [`delete_note_inner`] refuses with.
#[tauri::command]
pub async fn delete_note(
    lifecycle: State<'_, Lifecycle>,
    note_id: String,
) -> Result<bool, IpcError> {
    let pool = lifecycle.pool()?;
    delete_note_inner(&pool, &note_id).await
}

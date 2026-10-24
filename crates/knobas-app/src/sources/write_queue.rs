//! The decisions the write queue's IPC surface makes: what an *amendment* is
//! allowed to change (issue #42, story 15), and what a **new** write has to be
//! before it is queued at all (issue #43).
//!
//! Everything else in `commands::sources`'s queue block is a three-line
//! forward to `knobas_core::write_queue` or `knobas_sync::write_queue`, which
//! have their own batteries. This does not forward, so it lives here where a
//! test can reach it -- a `#[tauri::command]` cannot be called from one.

use knobas_core::write_queue::{QueuedWrite, WriteState};
use knobas_source::WriteOp;
use knobas_sync::scheduler::{Scheduler, SchedulerDeps};

use crate::IpcError;

/// Queue an outbound write and try to deliver it (issue #43), **waiting for
/// the attempt**.
///
/// Since issue #752 the `submit_write` command does not come here: it calls
/// [`submit_in_background`], which answers once the row is queued. This stays
/// for the callers that go looking for what their write made right after it
/// -- the standup protocol's page, an action item's ticket, start-work's
/// branch and pull request -- and so need the attempt behind them first.
///
/// `payload` is a serialized `knobas_source::WriteOp` -- the same shape
/// `pending_writes` hands back and `amend` takes -- rather than a typed
/// argument, on #42's ratified reasoning: `WriteOp` grows per milestone
/// (ADR-0006), and typing the argument would drag the SPI's enum onto the IPC
/// surface and make every growth an IPC change.
///
/// **The source is the entity's namespace**, not a separate argument.
/// Interfaces §4.1 makes the instance id and the `EntityRef` namespace the same
/// string, so a second argument could only ever agree with the id or
/// contradict it -- and a contradiction would route a write at a source the
/// target does not belong to. There is one answer, so it is read rather than
/// asked for.
///
/// Refused before anything is queued:
///
/// * a payload that is not a write op -- so an unreadable one is `invalid` at
///   the button rather than an undecodable row discovered at flush time;
/// * a target that is not an entity id, which nothing could ever deliver;
/// * a **source that is not configured**. Without this the write would queue
///   and wait forever with `no source with id ...` as its reason, which reads
///   as a source that is down rather than one that does not exist;
/// * an op the source's adapter does not declare. The adapter would refuse it
///   anyway -- battery clause 5 -- but only after a row was queued, a flush
///   was attempted and a refusal was recorded. `write_ops` is what the UI
///   renders its action bar from, so an op absent from it is one that should
///   never have been offered (story 13), and saying so at the call is the
///   difference between an interface that lies and one that does not.
///   A keychain that will not open says nothing about what the instance
///   offers, so an op the kind's roles allow is queued then (#1944).
///
/// # Errors
///
/// [`IpcError`] with `invalid` for a payload that is not a write op or names
/// an op the source does not declare, `not_found` for a source that is not
/// configured, `internal` if the queue's own writes fail, and `not_ready` if
/// the quit's grace ran out while the write was at the source (#1932).
pub async fn submit(
    state: &crate::sources::SourcesState,
    payload: serde_json::Value,
) -> Result<QueuedWrite, IpcError> {
    let (queued, source_id) = queue(state, payload).await?;
    flush(state, &source_id, queued.id).await?;
    Ok(queued)
}

/// Queue an outbound write and **answer at once**; the send happens in the
/// background (issue #752). What the `submit_write` command does.
///
/// Everything [`submit`] refuses is refused here too, before the answer: the
/// checks and the row are local, and they are the only things between the
/// reader pressing *Comment* and the dialog closing. The send -- the remote
/// call, a merge's bounded retry (#721), the re-read of the mirror when it
/// lands -- is [`flush`], spawned onto the runtime with a copy of the state.
///
/// **The queue's rules hold.** The spawned flush is `flush_entity_of`, the
/// scheduler's flush narrowed to this write's entity (#1797): one write of a source at a time (its per-source lock, re-read
/// under it, #1629), each entity's writes in the order they were queued, a
/// moved target held, a refusal recorded. Two writes queued back to back
/// spawn two flushes; they take turns at the lock, and neither sends a write
/// the other has already tried. The source's other writes are the flush
/// task's. A flush that fails here is logged, and the tick tries again within
/// seconds -- the row is the record, not this task.
///
/// The settle is announced as every queue transition is, on `activity:new`,
/// with the write's id in `detail.write_id`: that is what the shell waits for
/// to re-read the view.
///
/// # Errors
///
/// As [`submit`], all of them before anything is sent.
pub async fn submit_in_background(
    state: &crate::sources::SourcesState,
    payload: serde_json::Value,
) -> Result<QueuedWrite, IpcError> {
    let (queued, source_id) = queue(state, payload).await?;
    send_in_background(state, source_id, queued.id);
    Ok(queued)
}

/// [`submit_in_background`], and once the write has been sent and the source
/// re-read, a link from what it made to `ticket` (#984: *Link to <job>* on
/// *New branch…*, *Link the PR to <key>* on the compare page).
///
/// What a `CreateBranch` or `CreatePullRequest` makes has no id until the
/// source mints one and a sync brings it in (#707), so the link waits for
/// both. Since #1050 the wait is the row's, not this task's: the ticket is
/// held on the write ([`hold_arrival_link`]) and resolved by
/// [`link_arrivals`] after the send's own re-read and after every later sync
/// of the source ([`ArrivalLinks`]). So a write that waits (its source down,
/// a held write released later) is linked when it lands on a later tick, a
/// restart in between loses nothing, and a refused or discarded write links
/// nothing.
///
/// # Errors
///
/// As [`arrival_linkable`], then as [`submit`], all of them before anything
/// is sent.
pub async fn submit_linked_in_background(
    state: &crate::sources::SourcesState,
    payload: serde_json::Value,
    ticket: String,
) -> Result<QueuedWrite, IpcError> {
    arrival_linkable(&state.pool, &payload, &ticket).await?;
    let (queued, source_id) = queue(state, payload).await?;
    hold_arrival_link(&state.pool, queued.id, &ticket).await?;
    send_in_background_then(
        state,
        source_id.clone(),
        queued.id,
        source_id.clone(),
        move |state| async move {
            resolve_arrivals(&state.pool, state.registry.as_ref(), &source_id).await;
        },
    );
    Ok(queued)
}

/// Store `ticket` on write `write_id` as the job that what the write makes is
/// linked to once it arrives (#1050): `knobas.write_queue.link_to`, resolved
/// by [`link_arrivals`].
///
/// # Errors
///
/// `internal` if the write fails.
pub async fn hold_arrival_link(
    pool: &sqlx::PgPool,
    write_id: i64,
    ticket: &str,
) -> Result<(), IpcError> {
    sqlx::query("update knobas.write_queue set link_to = $2 where id = $1")
        .bind(write_id)
        .bind(ticket)
        .execute(pool)
        .await
        .map_err(IpcError::internal)?;
    Ok(())
}

/// How long after a write was sent its link is still looked for. A branch of
/// that name turning up later than this is not taken for what the write made.
const ARRIVAL_PATIENCE: &str = "7 days";

/// Resolve the links `source_id`'s sent writes still owe (#1050): each one
/// whose branch or pull request the mirror now holds is linked to its ticket
/// ([`link_on_arrival`]) and stops owing. Answers the entities linked.
///
/// A write that has not been sent keeps its intent: it may still land (a
/// refused one the reader edits and sends included, whose link then follows
/// the edited write). An intent is dropped, linking nothing, when the write is
/// discarded, when its ticket has left the mirror, or when it was sent more
/// than [`ARRIVAL_PATIENCE`] ago. Dropped once drawn, too, so a link the
/// reader removes is not drawn again by the next sync.
///
/// # Errors
///
/// `internal` if a read or a write fails.
pub async fn link_arrivals(
    pool: &sqlx::PgPool,
    declarations: &knobas_core::payload::Declarations,
    source_id: &str,
) -> Result<Vec<String>, IpcError> {
    sqlx::query(
        "update knobas.write_queue w set link_to = null
          where w.source_id = $1 and w.link_to is not null
            and (w.state = 'discarded'
                 or (w.state = 'sent' and w.settled_at < now() - $2::interval)
                 or not exists (select 1 from knobas.entity e where e.id = w.link_to))",
    )
    .bind(source_id)
    .bind(ARRIVAL_PATIENCE)
    .execute(pool)
    .await
    .map_err(IpcError::internal)?;
    let owed: Vec<(i64, String)> = sqlx::query_as(
        "select id, link_to from knobas.write_queue
          where source_id = $1 and link_to is not null and state = 'sent'
          order by id",
    )
    .bind(source_id)
    .fetch_all(pool)
    .await
    .map_err(IpcError::internal)?;
    let mut linked = Vec::new();
    for (write_id, ticket) in owed {
        if let Some(made) = link_on_arrival(pool, declarations, write_id, &ticket).await? {
            sqlx::query("update knobas.write_queue set link_to = null where id = $1")
                .bind(write_id)
                .execute(pool)
                .await
                .map_err(IpcError::internal)?;
            linked.push(made);
        }
    }
    Ok(linked)
}

/// [`link_arrivals`] with the source's declarations read first, best effort:
/// logged, never raised, since nothing waits on it and the next sync tries
/// again.
async fn resolve_arrivals(
    pool: &sqlx::PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    source_id: &str,
) {
    let linked = async {
        let declarations = crate::sources::paths::declared_paths(pool, registry).await?;
        link_arrivals(pool, &declarations, source_id).await
    };
    match linked.await {
        Ok(made) if !made.is_empty() => {
            tracing::debug!(source = source_id, ?made, "linked on arrival");
        }
        Ok(_) => {}
        Err(error) => tracing::warn!(source = source_id, %error, "a link on arrival failed"),
    }
}

/// The scheduler's events, and after every run of a source ends, that
/// source's owed links resolved ([`link_arrivals`], #1050). Wraps the events
/// `sources::start` hands the scheduler, so a write that lands on any later
/// tick is linked by the first sync that brings in what it made. Every event
/// is passed on unchanged.
pub struct ArrivalLinks<E> {
    inner: E,
    pool: sqlx::PgPool,
    registry: std::sync::Arc<dyn knobas_sync::scheduler::AdapterRegistry>,
    /// The scheduler's background set, which a resolution goes into so the
    /// quit waits for it (#1633). Handed in rather than read off the
    /// scheduler, which is built after its events are.
    background: knobas_sync::background::Background,
}

impl<E> ArrivalLinks<E> {
    pub fn new(
        inner: E,
        pool: sqlx::PgPool,
        registry: std::sync::Arc<dyn knobas_sync::scheduler::AdapterRegistry>,
        background: knobas_sync::background::Background,
    ) -> ArrivalLinks<E> {
        ArrivalLinks {
            inner,
            pool,
            registry,
            background,
        }
    }
}

impl<E: knobas_sync::scheduler::SyncEvents> knobas_sync::scheduler::SyncEvents for ArrivalLinks<E> {
    fn sync_state(&self, status: knobas_sync::SourceSyncStatus) {
        if !status.running {
            let pool = self.pool.clone();
            let registry = std::sync::Arc::clone(&self.registry);
            let source_id = status.source_id.clone();
            // Refused once the quit has begun: the next sync resolves it.
            self.background.spawn(async move {
                resolve_arrivals(&pool, registry.as_ref(), &source_id).await;
            });
        }
        self.inner.sync_state(status);
    }
    fn source_health(&self, health: knobas_sync::CredentialHealth) {
        self.inner.source_health(health);
    }
    fn activity_new(&self, row: knobas_core::activity::ActivityRow) {
        self.inner.activity_new(row);
    }
}

/// Whether `payload` may carry a link on arrival to `ticket` (#984): a
/// `CreateBranch`, a `CreatePullRequest` or a `CreatePage` (#1121), the
/// writes that make something a job reaches, or a `CreateTicket`, linked to
/// the failed run it was filed from (#1451); and an entity the mirror holds,
/// since `create_link` refuses an end it does not hold.
///
/// # Errors
///
/// `invalid` for a payload whose op draws no link on arrival, or a ticket that is
/// not an entity id; `not_found` for a ticket the mirror does not hold;
/// `internal` if the read fails.
pub async fn arrival_linkable(
    pool: &sqlx::PgPool,
    payload: &serde_json::Value,
    ticket: &str,
) -> Result<(), IpcError> {
    let op: WriteOp = serde_json::from_value(payload.clone())
        .map_err(|error| IpcError::invalid(format!("not a write operation: {error}")))?;
    if op.delivery().link.is_none() {
        return Err(IpcError::invalid(format!(
            "only a new branch, pull request, page or ticket is linked once it arrives, not {:?}",
            op.identifier()
        )));
    }
    knobas_core::entity::EntityRef::parse(ticket).map_err(IpcError::invalid)?;
    let held: Option<String> = sqlx::query_scalar("select id from knobas.entity where id = $1")
        .bind(ticket)
        .fetch_optional(pool)
        .await
        .map_err(IpcError::internal)?;
    if held.is_none() {
        return Err(IpcError::not_found(format!(
            "{ticket} is not in the mirror, so nothing can be linked to it"
        )));
    }
    Ok(())
}

/// Link what write `write_id` made to `ticket`, once it has: the branch a
/// sent `CreateBranch` named, or the pull request a sent `CreatePullRequest`'s
/// receipt named ([`entity_named`], #1949; the one open from its head into its
/// base where the source named none, #1952), as the mirror holds them (#984), or the page a
/// sent `CreatePage` made, by the id its settle stamped
/// (`docs::created_page`, #1121), or the ticket a sent `CreateTicket` filed
/// ([`ticket_filed`], #1451), drawn as the relation its op's `delivery()` states. Answers
/// the entity linked, or `None` where the write was not sent or what it made
/// is not mirrored yet.
///
/// The look-ups are start work's own (`start_work::queue`): the branch by its
/// name in the repository, the pull request open from the head into the base
/// (a merged or closed one there is not this write's). The link is the
/// `related` one the key in a branch's name proposes (`branch_name_key`), so a proposal already
/// raised is confirmed rather than doubled; one already drawn is drawn.
///
/// # Errors
///
/// `internal` if a read or the link write fails.
pub async fn link_on_arrival(
    pool: &sqlx::PgPool,
    declarations: &knobas_core::payload::Declarations,
    write_id: i64,
    ticket: &str,
) -> Result<Option<String>, IpcError> {
    let Some(write) = knobas_core::write_queue::get(pool, write_id).await? else {
        return Ok(None);
    };
    if write.state != WriteState::Sent {
        return Ok(None);
    }
    let op: WriteOp = serde_json::from_value(write.payload).map_err(IpcError::internal)?;
    let repo_of =
        |entity: &str| knobas_core::entity::EntityRef::parse(entity).map_err(IpcError::internal);
    // The relation is the op's own to state (`WriteOp::delivery`); an op that
    // draws no link on arrival has none to resolve.
    let Some(link) = op.delivery().link else {
        return Ok(None);
    };
    let made = match &op {
        WriteOp::CreateBranch { entity, name, .. } => {
            crate::start_work::queue::branch_named(pool, &repo_of(entity)?, name).await?
        }
        WriteOp::CreatePage { .. } => crate::docs::created_page(pool, write_id).await?,
        WriteOp::CreateTicket { title, .. } => {
            ticket_filed(pool, write_id, &write.source_id, title, write.queued_at).await?
        }
        WriteOp::CreatePullRequest {
            entity, head, base, ..
        } => {
            match entity_named(pool, write_id, crate::start_work::queue::KIND_PR).await? {
                Named::Made(named) => named,
                Named::Nothing => match crate::start_work::queue::pull_request_on_head(
                    pool,
                    declarations,
                    &repo_of(entity)?,
                    head,
                    base,
                )
                .await?
                {
                    crate::start_work::PullRequestOnHead::Open(id) => Some(id),
                    _ => None,
                },
            }
        }
        _ => None,
    };
    let Some(made) = made else {
        return Ok(None);
    };
    crate::protocol::draw_link(pool, &made, ticket, link.relation()).await?;
    Ok(Some(made))
}

/// How long before a write was queued the source may have stamped the ticket
/// it made: the machine's clock and the source's do not agree to the second.
const CLOCK_SLACK: &str = "5 minutes";

/// The ticket a sent `CreateTicket` made (#1451): the one its receipt named
/// ([`ticket_named`], #1754), or, where the source named none, the newest
/// ticket in `source_id` titled `title` that the source changed after the
/// write was queued at `queued`, within [`CLOCK_SLACK`].
///
/// The title is the fallback only, for a write the source answered without a
/// key (one sent before Jira's receipt named it, or an adapter whose receipt
/// is empty). A ticket of the same title the source last changed before the
/// write was queued is somebody else's -- a title the reader made generic,
/// filed before -- never this one.
///
/// **The one title rule** (#1958). A run's *Create ticket* is resolved here by
/// [`link_on_arrival`], and an action item's by
/// `commands::entity::action_item_ticket_inner`, and since #1951 both resolve
/// the same row: two rules would give two answers, and which one won would
/// depend on which reader asked first.
pub(crate) async fn ticket_filed(
    pool: &sqlx::PgPool,
    write_id: i64,
    source_id: &str,
    title: &str,
    queued: chrono::DateTime<chrono::Utc>,
) -> Result<Option<String>, IpcError> {
    if let Named::Made(named) = ticket_named(pool, write_id).await? {
        return Ok(named);
    }
    sqlx::query_scalar(
        "select entity_id from sync.live_item
          where source_id = $1 and kind = 'ticket' and title = $2
            and item_updated_at >= $3 - $4::interval
          order by item_updated_at desc, entity_id
          limit 1",
    )
    .bind(source_id)
    .bind(title)
    .bind(queued)
    .bind(CLOCK_SLACK)
    .fetch_optional(pool)
    .await
    .map_err(IpcError::internal)
}

/// What a sent create's receipt said about the entity it made.
#[derive(Debug, PartialEq, Eq)]
pub enum Named {
    /// The source named what it made: its entity id once the mirror holds it,
    /// `None` until a sync brings it in. A named entity is waited for, never
    /// looked for by a field another one may share (a title, a head).
    Made(Option<String>),
    /// The source named nothing, so only such a field can find it.
    Nothing,
}

/// The ticket write `write_id` made, by the key its receipt named (#1754):
/// [`entity_named`] for a `ticket`.
pub(crate) async fn ticket_named(pool: &sqlx::PgPool, write_id: i64) -> Result<Named, IpcError> {
    entity_named(pool, write_id, "ticket").await
}

/// The entity of kind `kind` write `write_id` made, by the id its receipt
/// named (#1754, #1949).
///
/// The settle stamps a receipt's id on `knobas.write_queue.remote_id`, and a
/// create's id is its entity's local part: Jira answers the key (`PAY-342`),
/// Gitea `owner/repo/issues/<n>` for an issue and `owner/repo#<n>` for a pull
/// request. So the entity is `<source_id>:<remote_id>` as `docs::created_page`
/// resolves a page, and it is taken only while the mirror holds it as a live
/// item of `kind`.
///
/// # Errors
///
/// `internal` if a read fails.
pub(crate) async fn entity_named(
    pool: &sqlx::PgPool,
    write_id: i64,
    kind: &str,
) -> Result<Named, IpcError> {
    let stamped: Option<(String, Option<String>)> =
        sqlx::query_as("select source_id, remote_id from knobas.write_queue where id = $1")
            .bind(write_id)
            .fetch_optional(pool)
            .await
            .map_err(IpcError::internal)?;
    let Some((source_id, Some(remote))) = stamped else {
        return Ok(Named::Nothing);
    };
    let mirrored = sqlx::query_scalar(
        "select entity_id from sync.live_item where entity_id = $1 and kind = $2",
    )
    .bind(format!("{source_id}:{remote}"))
    .bind(kind)
    .fetch_optional(pool)
    .await
    .map_err(IpcError::internal)?;
    Ok(Named::Made(mirrored))
}

/// [`deliver`] spawned onto the scheduler's background set with a copy of the
/// state: the send behind [`submit_in_background`]'s answer, and behind a
/// worklog's (#775).
///
/// **Not spawned once the quit has begun** (#1633, ruling B1): the row is
/// queued and stays pending, and the next start's tick sends it. A send
/// spawned before is awaited by the quit, under the scheduler's grace, before
/// the pool closes.
pub(crate) fn send_in_background(state: &crate::sources::SourcesState, source_id: String, id: i64) {
    spawn_send(state, source_id, id, |_| async {});
}

/// [`send_in_background`], and then, once the mirror has been re-read for the
/// write, `then` -- in the same spawned task (#779).
///
/// For the callers that go looking for what their write made: the standup
/// protocol's page and an action item's ticket have an address only once a
/// sync has seen them, so the link they owe is looked for after that read
/// rather than by the command, which has already answered. `then` is only
/// the first look: the page's link is reconciled on every read of the
/// protocol, and the ticket's is owed on the row (`link_to`), drawn by
/// [`link_arrivals`] or the protocol's read once the ticket is mirrored
/// (#1951).
///
/// Not spawned once the quit has begun, as [`send_in_background`] is not;
/// `then` is then not run either, as after a send that fails.
pub(crate) fn send_in_background_then<F, Fut>(
    state: &crate::sources::SourcesState,
    source_id: String,
    id: i64,
    reread: String,
    then: F,
) where
    F: FnOnce(crate::sources::SourcesState) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send,
{
    spawn_send(state, source_id, id, move |state| async move {
        sync_after_write(&state, &reread).await;
        then(state).await;
    });
}

/// The two above: [`deliver`] on the scheduler's background set, and once the
/// write went, `after`. Refused, and logged, once the quit has begun.
fn spawn_send<F, Fut>(state: &crate::sources::SourcesState, source_id: String, id: i64, after: F)
where
    F: FnOnce(crate::sources::SourcesState) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send,
{
    let task = state.clone();
    let spawned = state.scheduler.background().spawn(async move {
        if let Err(error) = deliver(&task, &source_id, id).await {
            tracing::warn!(
                source = %source_id,
                write_id = id,
                %error,
                "the background send of a queued write failed; the scheduler's tick retries it"
            );
            return;
        }
        after(task).await;
    });
    if !spawned {
        tracing::info!(
            write_id = id,
            "knobas is quitting: the write waits for the next start"
        );
    }
}

/// [`submit`]'s first half: everything it decides, and the row, **without
/// trying to deliver it**.
///
/// Split out for one caller, `crate::time::worklog` (issue #280), and for one
/// reason: a worklog's local copy has to exist, carrying this row's id, before
/// the flush can settle the write -- because settling is what stamps Jira's
/// worklog id onto that copy, and it stamps by that id.
/// `knobas_sync::write_queue::queue` records the whole of the reasoning; the
/// pair is `queue`, then the copy, then [`flush`].
///
/// The source id comes back with the row because the caller needs it for
/// [`flush`] and it was already worked out here -- reading it out of the
/// entity id a second time would be a second answer to a settled question.
///
/// # Errors
///
/// As [`submit`].
pub(crate) async fn queue(
    state: &crate::sources::SourcesState,
    payload: serde_json::Value,
) -> Result<(QueuedWrite, String), IpcError> {
    let (op, source_id) =
        submittable(&state.pool, &state.secrets, &state.registry, payload).await?;
    let queued = knobas_sync::write_queue::queue(state.scheduler.deps(), &source_id, op)
        .await
        .map_err(IpcError::internal)?;
    Ok((queued, source_id))
}

/// *Flush now* from the queue panel: what the `flush_writes` command does.
/// One source, or every source when `source_id` is `None`.
///
/// Sends each head write once whatever it waits on, a refused credential
/// included (#1628) -- the tick never re-sends one of those --
/// through [`knobas_sync::write_queue::flush_now`].
///
/// # Errors
///
/// `internal`, naming the source, if the queue's own reads or writes fail. A
/// source that still cannot take the write is not an error.
pub async fn flush_now(
    state: &crate::sources::SourcesState,
    source_id: Option<String>,
) -> Result<(), IpcError> {
    in_the_set(&state.scheduler, |scheduler| async move {
        knobas_sync::write_queue::flush_now(scheduler.deps(), source_id.as_deref())
            .await
            .map_err(|error| match source_id {
                Some(id) => IpcError::internal(error).with_source(id),
                None => IpcError::internal(error),
            })
    })
    .await
}

/// A flush a command waits on, run in the scheduler's background set and
/// awaited there (#1932): the quit waits for its write on the wire under the
/// same grace as a background send's, so the `sent` mark lands before the
/// pool closes, and a write still at the source when the grace ends is cut
/// and held as one the source may have. Outside the set the quit did not
/// wait, the pool closed under the write, and the next start sent it again.
/// `flush` is handed a copy of the scheduler to run on.
///
/// **Once the quit has begun** the set takes nothing, and `flush` runs here
/// instead, outside it. That is safe for a flush and nothing else: every
/// flush stops before its next write once the quit has begun (#1924), so it
/// sends nothing and only changes rows, a statement each. A command during
/// the quit therefore answers as it did before: the row changes as asked and
/// waits, pending, for the next start.
///
/// # Errors
///
/// What `flush` answers; `not_ready` if the quit's grace ran out while it was
/// still sending; `internal` if it ended without an answer otherwise (a
/// panic).
async fn in_the_set<F, Fut>(scheduler: &Scheduler, flush: F) -> Result<(), IpcError>
where
    F: FnOnce(Scheduler) -> Fut,
    Fut: std::future::Future<Output = Result<(), IpcError>> + Send + 'static,
{
    let (tell, answer) = tokio::sync::oneshot::channel();
    let flush = flush(scheduler.clone());
    let task = async move {
        let _ = tell.send(flush.await);
    };
    if let Err(task) = scheduler.background().try_spawn(task) {
        task.await;
    }
    answer.await.unwrap_or_else(|_| {
        Err(if scheduler.is_shutting_down() {
            IpcError::not_ready(
                "knobas quit while the write was at the source; it is held until \
                 you have checked the source",
            )
        } else {
            IpcError::internal("the write's send ended without an answer")
        })
    })
}

/// [`submit`]'s second half, and the worklog's awaited delivery's: [`deliver`]
/// in the scheduler's background set, waited for there ([`in_the_set`], #1932).
///
/// # Errors
///
/// As [`deliver`], or `not_ready` if the quit's grace ran out while the write
/// was at the source.
pub(crate) async fn flush(
    state: &crate::sources::SourcesState,
    source_id: &str,
    write_id: i64,
) -> Result<(), IpcError> {
    let (task, source_id) = (state.clone(), source_id.to_owned());
    in_the_set(&state.scheduler, move |_| async move {
        deliver(&task, &source_id, write_id).await
    })
    .await
}

/// Send the write's entity in queue order
/// (`knobas_sync::write_queue::flush_entity_of`, #1797), and re-read the mirror
/// if the write landed. Both callers run it in the background set: [`flush`],
/// which waits for it (and, once the quit has begun, runs it where it is),
/// and [`spawn_send`], which does not wait.
///
/// # Errors
///
/// As [`submit`]. A source that cannot take the write is **not** an error --
/// that is what the queue is for.
async fn deliver(
    state: &crate::sources::SourcesState,
    source_id: &str,
    write_id: i64,
) -> Result<(), IpcError> {
    let scheduler = &state.scheduler;
    knobas_sync::write_queue::flush_entity_of(scheduler.deps(), write_id)
        .await
        .map_err(IpcError::internal)?;

    // Story 15: a write that landed is reflected in the mirror now, rather
    // than at the next scheduled sync -- otherwise the app disagrees with
    // itself for up to five minutes right after the user acted.
    //
    // An **incremental** run, and through the scheduler rather than around it:
    // the SPI has no per-entity read (`Source::sync` is the whole of the read
    // direction), so the narrowest thing knobas can ask for is "whatever
    // changed since the cursor", which is exactly the item that was just
    // written to. `trigger` dedupes against a run already in flight, so a
    // burst of writes costs one run rather than one each.
    //
    // Only when the write actually **went**. A write that is waiting has
    // changed nothing at the source, and a run for it would re-read what
    // nobody touched.
    if landed(scheduler.deps(), write_id).await {
        refresh(scheduler, source_id).await;
    }
    Ok(())
}

/// Everything [`submit`] decides before a row exists, as one function so a
/// test can reach it: a `#[tauri::command]` cannot be called from one, and
/// neither can a path that needs a running scheduler.
///
/// Answers the decoded op and the source that owes it.
///
/// # Errors
///
/// As [`submit`].
pub(crate) async fn submittable(
    pool: &sqlx::PgPool,
    secrets: &std::sync::Arc<dyn knobas_secrets::SecretStore>,
    registry: &std::sync::Arc<dyn knobas_sync::scheduler::AdapterRegistry>,
    payload: serde_json::Value,
) -> Result<(WriteOp, String), IpcError> {
    let op: WriteOp = serde_json::from_value(payload)
        .map_err(|error| IpcError::invalid(format!("not a write operation: {error}")))?;
    let entity = knobas_core::entity::EntityRef::parse(op.target())
        .map_err(|error| IpcError::invalid(error.to_string()))?;

    let source = knobas_sync::config::get(pool, &entity.namespace)
        .await
        .map_err(IpcError::internal)?
        .ok_or_else(|| {
            IpcError::not_found(format!(
                "{} names source {:?}, which is not configured",
                entity, entity.namespace
            ))
        })?;

    // **What the adapter kind offers, plus what this instance offers.**
    //
    // The template is what `list_adapters` serves and what most action bars
    // are rendered from, and for every source but one it is the whole answer.
    // It is also the half that must stay: it needs no secret, and a write for
    // a source whose credential is gone is exactly one the queue should be
    // keeping rather than one this should refuse for want of a keychain entry.
    //
    // Since issue #452 it is no longer the whole answer. An Uptime Kuma's
    // `pause_monitor` is a property of the *instance* -- it exists only when
    // that source's keychain item carries an account -- so a check against the
    // template alone would refuse every pause the Monitors tab correctly
    // offered. The union is what keeps both readings: an op is refused here
    // only when **neither** the kind nor the instance has ever heard of it,
    // which is the case this refusal exists for (story 13: an op absent from
    // `write_ops` is one that should never have been offered).
    //
    // **A keychain that would not open is no answer about the instance**
    // (#1944). The instance half is then every write the kind's roles say a
    // source filling them may offer (`knobas_source::role`), which for a Kuma
    // is its pause, resume and create. The write is queued, and the flush
    // waits the keychain out on the sync's ladder (#1741). If the instance
    // turns out not to offer it, the flush refuses it there by name against
    // the built source's own descriptor, before any adapter sees it (battery
    // clause 5, held by the queue since #1950). An op no instance of the kind
    // could offer is still refused here.
    let template = registry
        .descriptors()
        .into_iter()
        .find(|d| d.adapter_kind == source.adapter_kind)
        .ok_or_else(|| {
            IpcError::internal(format!(
                "source {:?} is a {:?}, which no compiled-in adapter answers to",
                source.id, source.adapter_kind
            ))
        })?;
    let instance = match crate::sources::crud::instance_offers(secrets, registry, &source).await {
        crate::sources::crud::InstanceOffers::Ops(ops) => ops,
        crate::sources::crud::InstanceOffers::KeychainSilent => role_writes(&template),
    };
    let mut declared = template.write_ops;
    for offered in instance {
        if !declared.contains(&offered) {
            declared.push(offered);
        }
    }
    if !declared.iter().any(|o| o == op.identifier()) {
        return Err(IpcError::invalid(format!(
            "source {:?} does not offer {:?} -- it offers {declared:?}",
            source.id,
            op.identifier()
        )));
    }
    Ok((op, source.id))
}

/// Every write the roles `descriptor` claims say a source filling them may
/// offer (`knobas_source::role`): what an instance of the kind can offer at
/// most, for when the instance itself could not be asked (#1944).
fn role_writes(descriptor: &knobas_source::SourceDescriptor) -> Vec<String> {
    descriptor
        .roles
        .iter()
        .filter_map(|claim| knobas_source::role::minimum(&claim.role))
        .flat_map(|minimum| minimum.writes.iter().map(|&op| op.to_owned()))
        .collect()
}

/// Did this write settle as *sent*?
///
/// Re-read rather than inferred from what [`knobas_sync::write_queue::submit`]
/// returned: that row is the write **as queued**, before the attempt, which is
/// the honest shape for the caller and says nothing about the outcome.
async fn landed(deps: &SchedulerDeps, id: i64) -> bool {
    matches!(
        knobas_core::write_queue::get(&deps.pool, id).await,
        Ok(Some(row)) if row.state == WriteState::Sent
    )
}

/// Ask for an incremental sync of `source_id`, best effort.
///
/// Logged and dropped rather than raised: the write **landed**, and reporting
/// the follow-up read's failure as the write's would tell the user their edit
/// did not go when it did. The scheduler's own tick re-reads the source a few
/// minutes later either way, so the cost of a failure here is staleness, not
/// loss.
async fn refresh(scheduler: &Scheduler, source_id: &str) {
    if let Err(error) = scheduler
        .trigger(source_id, knobas_sync::SyncTrigger::Manual, None)
        .await
    {
        tracing::warn!(
            source = source_id,
            %error,
            "a write landed but the mirror could not be refreshed for it"
        );
    }
}

/// Replace a held or refused write's payload with one the user has just
/// written, then try to send it.
///
/// # Errors
///
/// `invalid` if `payload` is not a write op, or names a different op or target
/// from the row it is amending; `not_found` if no write carries `id`;
/// `conflict` if the write has already settled; `not_ready` if the quit's
/// grace ran out while the write was at the source.
pub async fn amend(
    scheduler: &Scheduler,
    id: i64,
    payload: serde_json::Value,
) -> Result<(), IpcError> {
    let deps = scheduler.deps();
    // Decoded before it is stored, so a payload the SPI cannot read is a
    // refusal the user sees now rather than an undecodable row discovered at
    // flush time, against a source, with the edit already lost from the box.
    let op: WriteOp = serde_json::from_value(payload)
        .map_err(|error| IpcError::invalid(format!("not a write operation: {error}")))?;

    let existing = knobas_core::write_queue::get(&deps.pool, id)
        .await?
        .ok_or_else(|| IpcError::not_found(format!("no queued write with id {id}")))?;
    check(&existing, &op)?;

    // The amendment and its send, in the background set (#1932): the send is
    // one a person waits on, as *Send mine anyway*'s is.
    in_the_set(scheduler, |scheduler| async move {
        knobas_sync::write_queue::amend(scheduler.deps(), id, op)
            .await
            .map_err(IpcError::internal)?
            .map(|_| ())
            .ok_or_else(|| IpcError::conflict(format!("write {id} has already settled")))
    })
    .await
}

/// *Send mine anyway* (story 13): release held write `id` and send it, in
/// the scheduler's background set, waited for there ([`in_the_set`], #1932).
///
/// # Errors
///
/// `conflict` if the write is not held; `internal` if the queue's own reads
/// or writes fail; `not_ready` if the quit's grace ran out while the write
/// was at the source.
pub async fn apply_anyway(scheduler: &Scheduler, id: i64) -> Result<(), IpcError> {
    in_the_set(scheduler, |scheduler| async move {
        knobas_sync::write_queue::apply_anyway(scheduler.deps(), id)
            .await
            .map_err(IpcError::internal)?
            .map(|_| ())
            .ok_or_else(|| IpcError::conflict(format!("write {id} is no longer held")))
    })
    .await
}

/// What an amendment may not change: the operation, and the target.
///
/// Both refusals are structural rather than defensive. A queued write holds a
/// **place in its entity's queue** and a **snapshot of that entity** taken when
/// it was made; an amendment that changed either would inherit an ordering
/// guarantee and a hold comparison that were established for a different
/// write. Turning a comment into a transition, or repointing it at another
/// ticket, is a *new* write -- and queueing one is how to make it.
///
/// It is also the only guard between an edit box and `WriteOp`: the payload
/// arrives as a `serde_json::Value` the webview has been holding, so "the same
/// write, edited" has to be checked rather than assumed.
fn check(existing: &knobas_core::write_queue::QueuedWrite, op: &WriteOp) -> Result<(), IpcError> {
    if op.identifier() != existing.op {
        return Err(IpcError::invalid(format!(
            "write {} is a {:?}, not a {:?} -- queue a new write instead",
            existing.id,
            existing.op,
            op.identifier()
        )));
    }
    let target = op.target();
    if target != existing.entity_id {
        return Err(IpcError::invalid(format!(
            "write {} targets {:?}, not {:?} -- queue a new write instead",
            existing.id, existing.entity_id, target
        )));
    }
    Ok(())
}

/// Sync a source and **wait for a run that can see what was just written**
/// (issue #289).
///
/// [`refresh`] triggers and moves on, which is right for the write that has
/// just landed: nothing is waiting on the mirror, and story 15 only asks that
/// the app stop disagreeing with itself within the second. A caller that is
/// about to *look for what its write created* needs the other behaviour, and
/// [`Scheduler::sync_after_write`](knobas_sync::scheduler::Scheduler::sync_after_write)
/// is where it lives -- including the reason one wait is not enough (#358).
///
/// A failure to trigger, or to wait, is **not** an error: it leaves a mirror
/// that may be behind, which every caller of this already has to handle --
/// there is no link yet, and the next read draws it. Logged and dropped
/// rather than raised, exactly as [`refresh`] does with its own: nothing here
/// reaches the caller, and a warning is what makes a source that could not be
/// re-read visible to whoever is reading the log.
pub(crate) async fn sync_after_write(state: &crate::sources::SourcesState, source_id: &str) {
    if let Err(error) = state.scheduler.sync_after_write(source_id).await {
        tracing::warn!(
            source = source_id,
            %error,
            "a write landed but the mirror could not be re-read for it"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::IpcErrorCode;

    fn queued(op: &str, entity: &str) -> knobas_core::write_queue::QueuedWrite {
        knobas_core::write_queue::QueuedWrite {
            id: 7,
            source_id: "jira".to_owned(),
            entity_id: entity.to_owned(),
            op: op.to_owned(),
            payload: serde_json::json!({}),
            target_snapshot: serde_json::json!({}),
            state: knobas_core::write_queue::WriteState::Held,
            wait_reason: None,
            detail: None,
            queued_at: chrono::Utc::now(),
            attempted_at: None,
            attempts: 0,
            held_snapshot: None,
            settled_at: None,
            source_enabled: true,
        }
    }

    fn comment(entity: &str, body: &str) -> WriteOp {
        WriteOp::Comment {
            entity: entity.to_owned(),
            body: body.to_owned(),
        }
    }

    /// Editing the text is the whole point, and it is allowed.
    #[test]
    fn the_same_write_with_different_words_is_an_amendment() {
        let existing = queued("comment", "jira:PAY-231");
        assert!(check(&existing, &comment("jira:PAY-231", "merged")).is_ok());
    }

    /// Repointing it is not. The write would inherit another entity's place in
    /// the queue and a snapshot taken of the first one.
    #[test]
    fn an_amendment_may_not_move_the_write_to_another_target() {
        let existing = queued("comment", "jira:PAY-231");
        let error = check(&existing, &comment("jira:PAY-999", "merged")).unwrap_err();
        assert_eq!(error.code, IpcErrorCode::Invalid);
        assert!(error.message.contains("PAY-999"), "{}", error.message);
    }

    /// Nor may it change the operation. Written against a row whose `op` is
    /// one the SPI does not define yet, because that is the shape the check
    /// has to survive when `WriteOp` grows (ADR-0006): the comparison is
    /// against the *stored* identifier, not against a table of known ops.
    #[test]
    fn an_amendment_may_not_change_the_operation() {
        let existing = queued("transition", "jira:PAY-231");
        let error = check(&existing, &comment("jira:PAY-231", "merged")).unwrap_err();
        assert_eq!(error.code, IpcErrorCode::Invalid);
        assert!(error.message.contains("transition"), "{}", error.message);
    }

    // -- what a *new* write has to be (issue #43) --------------------------

    /// The shared test database, migrated.
    async fn pool() -> sqlx::PgPool {
        let pool = knobas_db::test_util::test_pool().await;
        knobas_db::migrate::run(&pool).await.expect("migrations");
        pool
    }

    /// A name nothing else in this binary uses: `knobas.source_config` is
    /// shared across every test here.
    fn unique(prefix: &str) -> String {
        format!("{prefix}-{}", std::process::id())
    }

    /// A source of `kind`, so `submittable` has something to find.
    async fn configured(pool: &sqlx::PgPool, id: &str, kind: &str) {
        configured_with(pool, id, kind, knobas_source::AuthMethod::Pat).await;
    }

    /// The same, with the auth method spelled out.
    ///
    /// It matters for exactly one adapter: `submittable` reads an *instance's*
    /// write ops by building it (issue #452), and Uptime Kuma refuses to build
    /// against any method but `ApiToken` -- so a Kuma row carrying `Pat` would
    /// answer "offers nothing" for a reason that has nothing to do with the
    /// account this test is about.
    async fn configured_with(
        pool: &sqlx::PgPool,
        id: &str,
        kind: &str,
        auth: knobas_source::AuthMethod,
    ) {
        sqlx::query("delete from knobas.source_config where id = $1")
            .bind(id)
            .execute(pool)
            .await
            .expect("a clean slate for this id");
        knobas_sync::config::insert(
            pool,
            &knobas_sync::config::InsertConfig {
                id: id.to_owned(),
                adapter_kind: kind.to_owned(),
                display_name: id.to_owned(),
                base_url: "http://127.0.0.1:1".to_owned(),
                auth_kind: knobas_sync::config::AuthKind::Method(auth),
                config: serde_json::json!({}),
                sync_interval_secs: 300,
                enabled: true,
            },
        )
        .await
        .expect("the source row is written");
    }

    fn registry() -> std::sync::Arc<dyn knobas_sync::scheduler::AdapterRegistry> {
        std::sync::Arc::new(crate::sources::Registry::for_process())
    }

    /// The store `submittable` reads an instance's own write ops through
    /// (issue #452). Empty, which is the honest state for these fixtures: they
    /// configure a source row and no credential, and what that proves is that
    /// the *template* half of the union still carries every write these
    /// adapters have always taken.
    fn secrets() -> std::sync::Arc<dyn knobas_secrets::SecretStore> {
        std::sync::Arc::new(knobas_secrets::MemoryStore::new())
    }

    /// The happy path, and the reason there is no `source_id` argument: the
    /// target's namespace **is** the source, so one string decides both what is
    /// written and who is asked.
    #[tokio::test]
    async fn the_targets_namespace_is_the_source_that_owes_the_write() {
        let pool = pool().await;
        let id = unique("jira-submit");
        configured(&pool, &id, "jira").await;
        let (op, source) = submittable(
            &pool,
            &secrets(),
            &registry(),
            serde_json::json!({
                "Transition": { "entity": format!("{id}:PAY-231"), "status": "In Review" }
            }),
        )
        .await
        .expect("a declared op on a configured source");
        assert_eq!(op.identifier(), "transition");
        assert_eq!(source, id);
    }

    /// A payload the SPI cannot read is `invalid` at the button rather than an
    /// undecodable row discovered at flush time, with the edit already gone
    /// from the box.
    #[tokio::test]
    async fn a_payload_that_is_not_a_write_op_is_refused_before_anything_is_queued() {
        let pool = pool().await;
        let error = submittable(
            &pool,
            &secrets(),
            &registry(),
            serde_json::json!({ "Nonsense": {} }),
        )
        .await
        .expect_err("not a write op");
        assert_eq!(error.code, crate::IpcErrorCode::Invalid);
        assert!(
            error.message.contains("not a write operation"),
            "{}",
            error.message
        );
    }

    /// **The union, and why it has to be one** (issue #452).
    ///
    /// `pause_monitor` is on no adapter *kind*'s template -- an Uptime Kuma
    /// declares it only when its own keychain item carries an account -- so a
    /// check against `list_adapters` alone would refuse at the button every
    /// pause the Monitors tab correctly offered. Both directions are asserted
    /// here, against one source, with the credential as the only difference.
    #[tokio::test]
    async fn a_pause_is_queueable_exactly_when_the_source_has_an_account() {
        use knobas_secrets::SecretStore as _;
        let pool = pool().await;
        let id = unique("kuma-submit");
        configured_with(
            &pool,
            &id,
            knobas_source_kuma::ADAPTER_KIND,
            knobas_source::AuthMethod::ApiToken,
        )
        .await;
        let payload = serde_json::json!({ "PauseMonitor": { "entity": format!("{id}:8") } });

        // Key only: the instance declares nothing to write, the template
        // declares nothing to write, and the refusal names the op.
        let store = knobas_secrets::MemoryStore::new();
        store
            .put(
                &knobas_secrets::KeychainAccount::source(&id),
                &knobas_secrets::Secret::just(knobas_source::AuthMethod::ApiToken, "uk1_metrics"),
            )
            .unwrap();
        let secrets: std::sync::Arc<dyn knobas_secrets::SecretStore> = std::sync::Arc::new(store);
        let error = submittable(&pool, &secrets, &registry(), payload.clone())
            .await
            .expect_err("a Kuma with no account offers no pause");
        assert_eq!(error.code, crate::IpcErrorCode::Invalid);
        assert!(error.message.contains("pause_monitor"), "{}", error.message);

        // The same source with an account beside the key: queueable.
        let store = knobas_secrets::MemoryStore::new();
        store
            .put(
                &knobas_secrets::KeychainAccount::source(&id),
                &knobas_secrets::Secret {
                    kind: knobas_source::AuthMethod::ApiToken,
                    value: "uk1_metrics".to_owned(),
                    account: Some(knobas_source::instance::Account {
                        username: "knobas".to_owned(),
                        password: "knobas-dev".to_owned(),
                    }),
                },
            )
            .unwrap();
        let secrets: std::sync::Arc<dyn knobas_secrets::SecretStore> = std::sync::Arc::new(store);
        let (op, source) = submittable(&pool, &secrets, &registry(), payload)
            .await
            .expect("a Kuma with an account offers pause_monitor");
        assert_eq!(op.identifier(), "pause_monitor");
        assert_eq!(source, id);
    }

    /// A keychain that will not open, as a locked macOS keychain answers a
    /// read: `errSecInteractionNotAllowed`, arriving as `Backend`.
    struct Locked;

    impl knobas_secrets::SecretStore for Locked {
        fn get(
            &self,
            _account: &knobas_secrets::KeychainAccount,
        ) -> Result<Option<knobas_secrets::Secret>, knobas_secrets::SecretError> {
            Err(knobas_secrets::SecretError::Backend(
                "User interaction is not allowed.".to_owned(),
            ))
        }
        fn put(
            &self,
            _account: &knobas_secrets::KeychainAccount,
            _secret: &knobas_secrets::Secret,
        ) -> Result<(), knobas_secrets::SecretError> {
            unreachable!("submittable writes no credential")
        }
        fn delete(
            &self,
            _account: &knobas_secrets::KeychainAccount,
        ) -> Result<(), knobas_secrets::SecretError> {
            unreachable!("submittable deletes no credential")
        }
    }

    /// #1944: a locked keychain is this machine learning nothing about the
    /// instance, not the instance offering nothing. A pause pressed while it
    /// is locked is queued, and the flush waits the keychain out as the sync
    /// does, where it used to be refused at the button. The kind's roles say
    /// what an instance may offer, so an op no instance of the kind could
    /// offer is still refused.
    #[tokio::test]
    async fn a_pause_pressed_while_the_keychain_is_locked_is_queued_not_refused() {
        let pool = pool().await;
        let kuma = unique("kuma-locked-submit");
        configured_with(
            &pool,
            &kuma,
            knobas_source_kuma::ADAPTER_KIND,
            knobas_source::AuthMethod::ApiToken,
        )
        .await;
        let locked: std::sync::Arc<dyn knobas_secrets::SecretStore> = std::sync::Arc::new(Locked);

        let (op, source) = submittable(
            &pool,
            &locked,
            &registry(),
            serde_json::json!({ "PauseMonitor": { "entity": format!("{kuma}:8") } }),
        )
        .await
        .expect("a pause the keychain could not answer for is queued");
        assert_eq!(op.identifier(), "pause_monitor");
        assert_eq!(source, kuma);

        let teamcity = unique("teamcity-locked-submit");
        configured(&pool, &teamcity, "teamcity").await;
        let error = submittable(
            &pool,
            &locked,
            &registry(),
            serde_json::json!({
                "Comment": { "entity": format!("{teamcity}:build:1187"), "body": "hi" }
            }),
        )
        .await
        .expect_err("no CI source comments, keychain or not");
        assert_eq!(error.code, crate::IpcErrorCode::Invalid);
    }

    /// A target that is not an entity id names nothing the queue could ever
    /// order, hold or deliver.
    #[tokio::test]
    async fn a_target_that_is_not_an_entity_id_is_refused() {
        let pool = pool().await;
        let error = submittable(
            &pool,
            &secrets(),
            &registry(),
            serde_json::json!({ "Comment": { "entity": "PAY-231", "body": "hi" } }),
        )
        .await
        .expect_err("not an entity id");
        assert_eq!(error.code, crate::IpcErrorCode::Invalid);
    }

    /// A source that is not configured is `not_found` rather than a queued row.
    ///
    /// Without this the write would queue and wait for ever, with `no source
    /// with id ...` as its reason -- which reads in the pending-writes panel as
    /// a source that is *down*, and invites the user to wait for something that
    /// is never coming back.
    #[tokio::test]
    async fn a_target_naming_no_configured_source_is_not_found_rather_than_queued() {
        let pool = pool().await;
        let error = submittable(
            &pool,
            &secrets(),
            &registry(),
            serde_json::json!({
                "Comment": { "entity": "jira-not-configured:PAY-231", "body": "hi" }
            }),
        )
        .await
        .expect_err("no such source");
        assert_eq!(error.code, crate::IpcErrorCode::NotFound);
        assert!(
            error.message.contains("jira-not-configured"),
            "{}",
            error.message
        );
    }

    /// Story 13: an operation a source does not support is **absent** rather
    /// than offered and failing. The action bar is rendered from that source's
    /// `write_ops`, so an op outside it is one that should never have reached
    /// here -- and refusing at the call is what keeps the interface from lying,
    /// instead of letting a row be queued, flushed, refused and shown.
    ///
    /// The refusal names what the source *does* offer, so the mismatch is
    /// diagnosable from the message.
    #[tokio::test]
    async fn an_op_the_source_does_not_offer_is_refused_rather_than_queued() {
        let pool = pool().await;
        let id = unique("teamcity-submit");
        configured(&pool, &id, "teamcity").await;
        let error = submittable(
            &pool,
            &secrets(),
            &registry(),
            serde_json::json!({
                "Comment": { "entity": format!("{id}:build:1187"), "body": "hi" }
            }),
        )
        .await
        .expect_err("teamcity does not comment");
        assert_eq!(error.code, crate::IpcErrorCode::Invalid);
        assert!(error.message.contains("comment"), "{}", error.message);
        assert!(
            error.message.contains("trigger_build"),
            "the refusal must say what the source does offer: {}",
            error.message
        );
    }
}

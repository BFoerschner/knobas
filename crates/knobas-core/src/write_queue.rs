//! The write queue: what knobas still owes each source (issue #42).
//!
//! knobas has one outbound write path -- `knobas_source::Source::write` -- and
//! a source cannot always accept a write at the moment the user makes it. This
//! module owns the rows that keep the edit until it can go, and the two facts
//! that make the queue more than a retry loop:
//!
//! * **Why it is waiting**, so "the credential is rejected" and "the server did
//!   not answer" are distinguishable without guessing.
//! * **A snapshot of the target as it was when the write was queued**, so that
//!   a target which moved on in the meantime produces a [held](WriteState::Held)
//!   write rather than a silent overwrite.
//!
//! ## What is here and what is not
//!
//! The rows and their transitions are here. **Calling `Source::write` is not**
//! -- it cannot be: `knobas-source` depends on `knobas-core`, so this crate
//! cannot see the SPI at all. The flush loop lives in `knobas_sync::write_queue`
//! and is the one place in knobas that calls `Source::write`; a test in that
//! crate fails if a second call site appears.
//!
//! That split is why a queued write is stored as an op identifier plus a jsonb
//! payload rather than as a typed `WriteOp`: this module reasons *per op*
//! without decoding the enum, and the enum grows per milestone (ADR-0006).
//!
//! ## The one rule with no exceptions
//!
//! A held write is terminal until the user acts. There is no timeout, no
//! auto-apply and no auto-discard, and nothing in this module or in migration
//! `0005` could express one.

use serde::Serialize;
use sqlx::PgPool;

use crate::CoreError;
use crate::delivery::Projection;
use crate::entity::EntityRef;
use crate::payload::{StatePath, resolve_state};

crate::closed_vocabulary! {
    /// What the queue will do about a write next.
    ///
    /// The queue panel branches on this and so does the shell badge, so the
    /// three open states are three different sentences, not three shades of
    /// "waiting":
    ///
    /// - `pending`: needs **patience**. Retried automatically, and it goes on
    ///   its own when the source can take it.
    /// - `held`: needs a **decision**. The target changed after the write was
    ///   queued. Waits for the user and for nothing else: no timeout, no
    ///   auto-apply, no auto-discard. Since migration `0012` a target also
    ///   leaves the view when the user turns its source off, and that write
    ///   holds by the same mechanism. [`QueuedWrite::source_enabled`] is what
    ///   tells the two apart -- the state stays one word because the queue's
    ///   answer is the same (nothing sends until someone acts); only the
    ///   explanation differs.
    /// - `refused`: needs a **decision**. The source rejected the write
    ///   outright. Deliberately never retried -- a refusal is a decision, not
    ///   a blip (ADR-0004 is what tells the two apart).
    /// - `sent`: delivered. Terminal.
    /// - `discarded`: withdrawn by the user. Terminal.
    ///
    /// `sent` and `discarded` are settled and never appear in
    /// `pending_writes`.
    ///
    /// Stored as lowercase text in `knobas.write_queue.state`, whose
    /// `write_queue_state_chk` (migration 0005) allows exactly these
    /// spellings -- and `ALL` is what the test that pins the two together
    /// walks.
    ///
    /// The states are documented here and not on each variant because the
    /// generated TypeScript (`app/src/lib/ipc/generated/sources.ts`) carries a
    /// type's and a field's doc but not a variant's (#1325).
    #[derive(ts_rs::TS)]
    #[ts(export_to = "sources.ts")]
    pub enum WriteState {
        Pending => "pending",
        Held => "held",
        Refused => "refused",
        Sent => "sent",
        Discarded => "discarded",
    }
}

impl WriteState {
    /// Whether a write in this state is still owed to a source.
    ///
    /// The three open states are what the visible list shows and what the
    /// shell counts; the two terminal ones are history.
    #[must_use]
    pub fn is_open(self) -> bool {
        matches!(self, Self::Pending | Self::Held | Self::Refused)
    }
}

crate::closed_vocabulary! {
    /// Why a pending write has not gone yet.
    ///
    /// - `unreachable`: the server did not answer, or the keychain would not
    ///   open. The two climb the same ladder, and a keychain fault's
    ///   `detail` starts `keychain:` (#1741).
    /// - `unauthorized`: the credential was refused.
    ///
    /// Only the two *retryable* faults appear here, the ones that pass on
    /// their own or with a human's help. A refusal is not a reason to wait,
    /// it is a reason to stop, and it has its own [`WriteState::Refused`];
    /// `write_queue_reason_state_chk` in migration 0005 is what keeps a held
    /// or refused row from also claiming to be waiting on a network. So a
    /// row's `wait_reason` is `null` on every row that is not `pending`, on a
    /// pending one that has not been tried yet, which `attempted_at` is what
    /// distinguishes, and on one a person acting on the source released
    /// ([`release_credential_waits`]).
    ///
    /// The two are waited out differently (#1628): an `unreachable` write is
    /// tried again on the sync side's backoff ladder, an `unauthorized` one
    /// only after a person acts on the source or presses *Flush now*. A
    /// person acting on the source also ends a keychain fault's wait at once
    /// (#1944).
    ///
    /// The reasons are documented here and not on each variant because the
    /// generated TypeScript (`app/src/lib/ipc/generated/sources.ts`) carries a
    /// type's and a field's doc but not a variant's (#1325).
    #[derive(ts_rs::TS)]
    #[ts(export_to = "sources.ts")]
    pub enum WaitReason {
        Unreachable => "unreachable",
        Unauthorized => "unauthorized",
    }
}

/// How an `unreachable` write's `detail` starts when the keychain would not
/// open (#1741), followed by one space and the store's own text.
///
/// A string contract across the bridge: the queue panel
/// (`app/src/lib/shell/WriteQueue.svelte`) tells a keychain fault from a
/// server that did not answer by this prefix alone. One constant on this
/// side, and `knobas-sync`'s `the_panel_reads_the_keychain_prefix_the_queue_writes`
/// reads the panel's copy, so a rename on either side is a red test rather
/// than a panel that says "The server did not answer." again.
pub const KEYCHAIN_DETAIL_PREFIX: &str = "keychain:";

/// Whether a write waiting for `reason`, told `detail`, waits on its
/// source's credential: one the source refused, or a keychain that would not
/// open (#1944). What [`release_credential_waits`] releases, in Rust; its
/// statement says the same in SQL, and
/// `the_release_statement_releases_exactly_what_waits_on_credential_names`
/// (`tests/it/write_queue.rs`) holds the two to one answer.
#[must_use]
pub fn waits_on_credential(reason: WaitReason, detail: Option<&str>) -> bool {
    match reason {
        WaitReason::Unauthorized => true,
        WaitReason::Unreachable => {
            detail.is_some_and(|detail| detail.starts_with(KEYCHAIN_DETAIL_PREFIX))
        }
    }
}

/// Every column of `knobas.write_queue` that leaves this module, in one place
/// -- plus the one *derived* column, `source_enabled`, which is not a column
/// of the table at all.
///
/// The same device, for the same reason, as `link_columns!`: [`QueuedWrite`]
/// is a `FromRow`, so a column this list forgets is a decode failure at run
/// time rather than a compile error. Naming them once is what stops the
/// `returning` clauses of eight statements from drifting apart.
///
/// `source_enabled` is computed here, in the same statement as the row,
/// rather than stored or joined by a caller: whether a source is on changes
/// with a click, so a stored value is stale the moment the user re-enables,
/// and a second fetch a caller correlates can describe a different instant
/// than the row it decorates (issue #204). A scalar subquery instead of a
/// join because half these statements are `returning` clauses, which can
/// carry an expression but not a join. `coalesce(.., true)` is migration
/// `0012`'s own direction: no configuration row answers "did the user turn
/// this source off" with no -- `run_once` queues writes for unconfigured
/// sources, and those must not claim the user turned anything off.
macro_rules! queue_columns {
    () => {
        "id, source_id, entity_id, op, payload, target_snapshot, state, wait_reason, \
         detail, queued_at, attempted_at, attempts, held_snapshot, settled_at, \
         coalesce((select s.enabled from knobas.source_config s where s.id = source_id), true) \
           as source_enabled"
    };
}

/// One write knobas owes a source.
///
/// The vocabulary is `CONTEXT.md`'s: a row in [`WriteState::Pending`] is a
/// **pending write**, one in [`WriteState::Held`] is a **held write**.
#[derive(Clone, Debug, Serialize, sqlx::FromRow, ts_rs::TS)]
#[ts(export_to = "sources.ts")]
pub struct QueuedWrite {
    /// Identity and queue order in one value -- see migration `0005`.
    pub id: i64,
    /// Which source owes the write.
    pub source_id: String,
    /// The target, as an entity id.
    pub entity_id: String,
    /// `knobas_source::WriteOp::identifier()`: the stable snake_case
    /// operation name, `"comment"`.
    pub op: String,
    /// The serialized `WriteOp`; `amend_write` takes it back, edited.
    #[ts(type = "WriteOpPayload")]
    pub payload: serde_json::Value,
    /// The target as it was when the write was queued -- [`project`]'s output.
    ///
    /// One half of the two versions a held write is shown with. Its shape is
    /// per-op; for `"comment"` it carries `{op, live, text}`, where `text` is
    /// the item's indexed text -- which every adapter builds from its title,
    /// description and comment bodies, so a new reply is exactly what changes
    /// it.
    #[ts(type = "unknown")]
    pub target_snapshot: serde_json::Value,
    pub state: WriteState,
    /// Why a pending write is waiting; `None` while nothing has been tried,
    /// and again once a person acting on the source -- a new credential, a
    /// new address or configuration -- has released an `unauthorized` wait.
    pub wait_reason: Option<WaitReason>,
    /// What the source said, in its own words. Untrusted source text: render
    /// it as text, never as markup.
    pub detail: Option<String>,
    pub queued_at: chrono::DateTime<chrono::Utc>,
    pub attempted_at: Option<chrono::DateTime<chrono::Utc>>,
    pub attempts: i32,
    /// The target as it stood when the write was held -- the other half of
    /// "both versions side by side". `None` until a write is held; kept
    /// afterwards.
    #[ts(type = "unknown | null")]
    pub held_snapshot: Option<serde_json::Value>,
    pub settled_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Whether the user has this write's source turned on, **as of this read**
    /// -- derived by [`queue_columns!`], never stored (issue #204).
    ///
    /// Since migration `0012` a disabled source's items leave
    /// `sync.live_item`, so its queued writes go [held](WriteState::Held) by
    /// the same mechanism as a withdrawn target's -- [`target_of`] reads
    /// `None` either way. The two are not the same fact: one is upstream's
    /// doing and resolved by choosing a version, the other is the user's own
    /// and undone by re-enabling the source. This flag is what keeps them
    /// distinguishable downstream; storing it instead would leave the wrong
    /// answer standing after the click that re-enables.
    ///
    /// `false` only when a configuration row exists and says off. A source
    /// with no row at all reads `true`, matching `0012`'s
    /// `coalesce(enabled, true)`: absence of configuration is not a decision
    /// the user made.
    ///
    /// The remedies differ as the facts do: a target that changed wants the
    /// two-versions choice, a source that is off wants re-enabling. Say
    /// which, and never offer *Send mine anyway* for a source that cannot
    /// take it. Flips on the next read once the source is back on.
    pub source_enabled: bool,
}

/// Queue one write, returning the row that was written.
///
/// `target_snapshot` is [`project`]'s reading of the target **now**; it is
/// what a later flush compares against. Taking it here rather than inside is
/// deliberate: the caller has already read the target to decide the write is
/// possible at all, and a second read would snapshot a different moment.
///
/// The whole row comes back for the reason [`crate::activity::record`]'s does:
/// `id` and `queued_at` are the database's to choose, and the caller has to
/// announce what it queued.
///
/// # Errors
///
/// [`CoreError::Db`] if the insert fails.
pub async fn queue(
    pool: &PgPool,
    source_id: &str,
    entity: &EntityRef,
    op: &str,
    payload: serde_json::Value,
    target_snapshot: serde_json::Value,
) -> Result<QueuedWrite, CoreError> {
    let row = sqlx::query_as::<_, QueuedWrite>(concat!(
        "insert into knobas.write_queue
           (source_id, entity_id, op, payload, target_snapshot)
         values ($1, $2, $3, $4, $5)
         returning ",
        queue_columns!()
    ))
    .bind(source_id)
    .bind(entity.to_string())
    .bind(op)
    .bind(payload)
    .bind(target_snapshot)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

/// One queued write by id, or `None` if nothing carries it.
///
/// # Errors
///
/// [`CoreError::Db`] if the query fails.
pub async fn get(pool: &PgPool, id: i64) -> Result<Option<QueuedWrite>, CoreError> {
    let row = sqlx::query_as::<_, QueuedWrite>(concat!(
        "select ",
        queue_columns!(),
        " from knobas.write_queue where id = $1"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// The target of a write, as the mirror records it right now.
///
/// Read through `sync.live_item`, which has the tombstone filter built in --
/// so a target the source withdrew reads as `None`, exactly like one that was
/// never mirrored. Both are "there is nothing there to write to", and both
/// must hold a write that was queued against something that *was* there.
#[derive(Clone, Debug, Serialize, sqlx::FromRow)]
pub struct Target {
    pub entity_id: String,
    /// The item's kind (`pr`): what a source's declared payload paths are
    /// keyed by, which an `edit_pull_request`'s state hold reads (#706).
    pub kind: String,
    pub title: String,
    /// The item's indexed text. Contract §4.1 makes every adapter build this
    /// from the item's title, description and **comment texts**, which is what
    /// lets [`project`] define a comment's target having changed without
    /// reaching into any one source's payload shape.
    pub body_text: String,
    /// The source's own last-modified timestamp, never `now()`.
    pub item_updated_at: Option<chrono::DateTime<chrono::Utc>>,
    /// The raw record, verbatim.
    pub payload: serde_json::Value,
}

/// Read a write's target from the mirror. `None` if it is not there, or no
/// longer live.
///
/// # Errors
///
/// [`CoreError::Db`] if the query fails.
pub async fn target_of(pool: &PgPool, entity: &EntityRef) -> Result<Option<Target>, CoreError> {
    // Named columns, never `select *`: `sync.live_item` carries a `tsvector`
    // that must not wander into a `FromRow` (roadmap §4 gotcha 2).
    let row = sqlx::query_as::<_, Target>(
        "select entity_id, kind, title, body_text, item_updated_at, payload
           from sync.live_item
          where entity_id = $1",
    )
    .bind(entity.to_string())
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// What a write counts as its target having changed, by the `shape` its op's
/// [`Delivery`](crate::delivery::Delivery) states. `op` is the identifier, and
/// only labels the snapshot: rows queued before the shape was stated carry it,
/// so dropping it would hold every one of them on the next flush.
///
/// Taken when a write is queued and again immediately before it flushes; if
/// the two differ, the write is held rather than sent. The output carries
/// values rather than a digest because a held write has to be shown with
/// **both versions side by side**, and a hash cannot be rendered.
///
/// `None` means the mirror has no live item for the target -- never synced, or
/// withdrawn upstream. That is deliberately *not* an error: a target that
/// disappeared after a write was queued is a change like any other, and
/// projecting it as `"live": false` is what makes it hold.
///
/// ## Per op
///
/// Three shapes ([`Projection`]), and which one an op gets is a statement
/// about what that op could *overwrite* -- never about how much work the
/// comparison is. `WriteOp::delivery` states it per variant; the reasons are
/// argued here.
///
/// **The declared state** ([`Projection::DeclaredState`]) -- `"comment"`,
/// `"approve"` and `"submit_review"` (the deputy's ruling on #752, for Björn). These add to
/// a discussion rather than overwrite anything in it, and the reader works at
/// their own speed: a second comment typed right after the first must not be
/// held because the first one's re-sync moved the item's text. So what holds
/// them is only the target's state as its source **declares** it for the kind
/// (`KindPaths::state`, #666, resolved by [`resolve_state`] -- the read
/// `"edit_pull_request"`'s *Close* holds on): a pull request merged or closed
/// while the words were on their way is a reason to look again; a new reply,
/// a retitle or a push is not. A kind that declares no state (a Jira ticket,
/// a Gitea issue) holds them only on the target **leaving the mirror**, as a
/// create is held. A declaration that resolves nothing projects the **whole
/// record**, so a miss is a false hold, never a missed one (ADR-0007).
///
/// Until #752 a comment held on `body_text` (a new reply changed it) and an
/// approval or a review on the whole record; with the send behind the reader,
/// both made a quick second write hold on the first one's echo.
///
/// **The whole mirrored record** ([`Projection::Whole`]) -- `"transition"`, `"merge_pull_request"`,
/// `"delete_branch"` and `"update_pull_request_branch"`, the ops that put a
/// *judgement* onto a target whose current state is the whole reason for the
/// judgement, `"set_parameter"`, which overwrites a value, and `"source_action"`
/// (#1194), whose effect knobas cannot know. Any change to the target at
/// all holds the write. `"edit_pull_request"` takes this shape only when it
/// changes the pull request's state; see **Per field** below.
///
/// * `"transition"` is the conservative choice **and it was a choice**
///   (issue #43). What one would rather compare is the status alone, and there
///   is no adapter-independent way to read it: §4.1 guarantees `title`,
///   `body_text`, `author`, `updated_at` and a verbatim `payload`, and the
///   status lives
///   only in the last of those, under `fields.status.name` for Jira and `state`
///   for Gitea. Reading it here would mean this module -- which cannot see
///   `WriteOp` at all, let alone an adapter -- learning every source's payload
///   shape, which is the coupling `SourceDescriptor` exists to avoid. ADR-0007
///   ratifies this refusal as the *write* direction's rule -- in a hold
///   decision a payload read that misses is a missed hold, a wrong action --
///   and it is not the blanket ban it may read as: a derivation whose miss
///   costs only an absent item may read (`suggest`, `inbox`, under that ADR's
///   three requirements). The cost
///   is noise on a busy ticket: a comment arriving while the source is down
///   holds a queued transition. The cost of the other direction is moving a
///   ticket somebody else already moved, silently, which is the one thing
///   issue #42 exists to prevent. A false hold shows both versions side by side
///   and is one *Apply anyway* away; a missed hold shows nothing.
/// * `"merge_pull_request"` because it acts on a specific state of the pull
///   request (issue #667): a merge lands whatever the pull request carries *at the flush*, so
///   a commit pushed after the reader pressed *Merge…* would otherwise go into
///   the base unread. Somebody else merging or closing it first changes the
///   record too, and holds the write rather than sending a merge Gitea would
///   refuse.
/// * `"delete_branch"` for the merge's reason (issue #707): a delete takes
///   whatever the branch carries *at the flush* with it, so a commit pushed
///   after the reader pressed *Delete branch…* would otherwise be lost unread
///   to anyone but its SHA. The branch's mirrored record carries its tip, so
///   a push moves it and holds the write.
/// * `"update_pull_request_branch"` for the merge's reason (issue #708): an
///   update merges the base into the head, or rebases the head onto it, as
///   the head stands *at the flush*; a push after the reader pressed *Update
///   branch…* is a head they have not seen being rewritten or extended.
/// * `"set_parameter"` by analogy with `"merge_pull_request"`, and with a
///   limit worth stating (issue #668). The op **overwrites** a build
///   configuration's parameter, so it belongs with the ops that replace
///   something rather than with the creates that add beside it. But the
///   configuration's mirrored record is its id, name, project and description
///   (TeamCity's `BUILD_TYPE_FIELDS`), **not its parameters**, so this shape
///   cannot see a colleague changing the same parameter between the edit and
///   the flush. That window is the op's `base` to close, as `update_page`'s
///   `base_version` closes its own: the adapter reads the parameter before it
///   writes and refuses a value that is no longer the one edited against
///   (only a secret, never drawn, has no `base` and is last-write-wins). What
///   it does hold on is the configuration being renamed, moved, re-described
///   or deleted -- each a reason to look at what one is about to change. The
///   cost is small because the record is quiet: it carries no timestamp
///   (`updated_at` is `None`) and nothing a build changes, so a sync does not
///   move it on its own, and a false hold is one *Apply anyway* away.
///
/// **Per field** (`Projection::Whole { per_field: true }`) --
/// `"edit_pull_request"` (issue #706), the one op whose projection depends on
/// what it carries rather than on its delivery alone, and the reason `project`
/// takes the queued `payload` at all. The op overwrites
/// only the fields it was given (the shell sends only what the reader
/// changed), and the deputy's ruling on #706 is that it holds only when what
/// it would overwrite changed: someone retitling the pull request must not
/// hold a queued *Close*. Each field gets the narrowest normalised value that
/// still sees it change:
///
/// * **a state change** (*Close*, *Reopen*) takes the state the source
///   **declares** for the kind (`KindPaths::state`, #666; Gitea's merged flag,
///   then its `state` word), resolved by [`resolve_state`] -- the declared
///   read ADR-0007 sends a payload read to, never a guess at a shape. So a
///   retitle does not hold a queued Close, and somebody else closing,
///   reopening or merging the pull request does. Where the declaration
///   resolves nothing -- a source that declares no state, a record it does
///   not recognise -- the projection is the **whole record** instead, so a
///   miss is a false hold, never a missed one.
/// * **a description** (with or without a title) takes `body_text`, which
///   §4.1 builds from the title, the description and the comments: it moves
///   when the description does, and a retitle or a new comment holds it too
///   -- false holds, the safe direction. A close by somebody else does not.
/// * **a title alone** takes `title`, a normalised field: only a retitle
///   holds it.
///
/// What each field compares goes into the projection's `text`, one part per
/// field, so the held panel shows the two sides of exactly what was compared.
///
/// Which fields the op carries is read from the queued payload -- knobas'
/// own serde shape, so ADR-0007's requirements 2 and 3 bind it -- in one
/// statement, [`edit_writes`], and the read **fails wide**: a field counts as
/// written unless the payload says `null` for it in so many words, so a
/// drifted shape projects the whole record rather than nothing.
///
/// **Liveness alone** ([`Projection::Liveness`]) -- `"create_ticket"`, `"create_branch"`,
/// `"create_pull_request"`, `"trigger_build"`, `"rerun_build"`, `"run_build"`,
/// `"log_work"`, `"create_page"`, `"pause_monitor"`, `"resume_monitor"`,
/// `"create_monitor"`, `"commit_file"`.
/// These do not
/// overwrite anything: they add a ticket, a branch, a pull request, a queued
/// build or a worklog *beside* whatever the container holds now, so a change to the
/// container is not a change to what the write would replace -- there is
/// nothing it would replace. Holding a create because someone renamed the
/// repository would be a decision the user cannot act on and cannot learn
/// anything from. What still holds them is the target **leaving the mirror**:
/// a build configuration that is gone, a repository that was deleted, a build
/// that was purged. And a duplicate -- the branch already exists, the pull
/// request is already open -- is the source's answer to give, which arrives as
/// a refusal carrying what it said (ADR-0004), not as a hold.
///
/// `"log_work"` is in that group and the reasoning is worth stating, because
/// its target *is* a mirrored item and the conservative fallback would
/// therefore have bitten: a worklog is a statement about **hours somebody
/// worked**, and nothing that can happen to the ticket makes those hours wrong.
/// Holding a worklog because a colleague replied to the ticket while Jira was
/// unreachable would ask the reader to re-consent to their own afternoon, and
/// the two versions the hold dialog would show them would differ in a comment
/// that has nothing to do with the time. What does still hold it is the ticket
/// leaving the mirror -- there is then nothing to log against.
///
/// **`"pause_monitor"` and `"resume_monitor"` are in that group for a reason
/// of their own** (issue #452), and it is the only entry here where liveness
/// is not the weaker reading but the *exact* question. Through Uptime Kuma's
/// `/metrics` -- the one door its read half has -- **a paused monitor is not
/// published at all**, so the adapter tombstones it and it leaves the mirror
/// (`knobas_source_kuma::cursor`). "The target stopped being live" and
/// "somebody already paused it" are therefore the same observation, which is
/// precisely what a queued pause must be held on. The whole-record shape would
/// be worse than conservative, it would be unusable: Kuma writes a new
/// response time for every monitor on every heartbeat, so a `payload` snapshot
/// taken at queue time differs from the one at flush time on **every** write
/// that outlives one poll -- a hold on all of them, which is not a false hold
/// in the safe direction but a queue that never delivers.
///
/// Both ops get the same shape and it reads differently in each direction,
/// which is the intended behaviour rather than an oversight. A *pause* queued
/// against a live monitor holds if the monitor vanished in between -- somebody
/// got there first. A *resume* is queued against a monitor that is already
/// tombstoned, so it projects `{"live": false}` at queue time and again at
/// flush time, which is equal, and it sends -- the same reading a create's
/// unmirrored container gets, and for the same reason. What it does not do is
/// tell a *paused* monitor from a *deleted* one, because nothing on this
/// channel can: `/metrics` publishes neither. A resume aimed at a monitor
/// somebody deleted goes to Kuma and comes back refused by name, which is the
/// source's answer to give (ADR-0004) and not a hold to invent here.
///
/// **`"create_monitor"` is in that group and its hold clause has no teeth at
/// all** (issue #453), which is worth saying rather than leaving to be
/// discovered. Its target is the source itself
/// (`knobas_source::monitor_target`), a container knobas never mirrors -- so
/// it projects `{"live": false}` at queue time and again at flush time, the
/// two are equal, and the write always sends. That is `"create_ticket"`'s
/// reading exactly, and it is the intended one: nothing that can happen to a
/// Kuma between the button and the flush makes *watch this URL* the wrong
/// request. A duplicate is not a hold either -- Uptime Kuma holds any number
/// of monitors under one name, so a second create makes a second monitor, and
/// the queue's own per-entity ordering is what keeps two creates from racing.
///
/// **`"commit_file"` is in that group, and not because it overwrites
/// nothing** (issue #822): it replaces a file. Its target is the repository,
/// and the mirror holds no file, so nothing this module can read moves when
/// the file does -- the whole record would hold on every push to every branch
/// and still miss the one change that matters. The file's own version is the
/// op's `base_sha`, and the **source** compares it at the flush: a file that
/// changed since it was read comes back as `SourceError::Moved`, which the
/// flush turns into a hold through [`moved`] and [`hold`], both versions
/// carried. What the projection still holds it on is the repository leaving
/// the mirror.
///
/// `"create_page"` is in that group for the additive reason and not by
/// analogy: a new page goes *beside* whatever else sits under its parent, so a
/// parent whose title or discussion moved on is not a parent this write would
/// overwrite. What still holds it is the parent **leaving the mirror** -- a
/// page created under a deleted parent is a page nobody will find. It is the
/// one create whose container knobas really does mirror, so this is also the
/// one where that clause has teeth (issue #286).
///
/// **The whole mirrored record, and the version inside it** -- `"update_page"`
/// joins `"transition"` and `"merge_pull_request"` in the whole-record shape,
/// and the reason is the sharpest of them: this op *replaces a page's body*.
/// Anything at all that happened to the page since the reader started typing
/// is something their re-assembled body would silently delete.
///
/// That shape carries the verbatim `payload`, which is where a Confluence page
/// keeps `version.number` -- so the two sides of a held `update_page` differ
/// in the version number itself, and "the mirror's version has passed the one
/// the edit was made against" is not a separate check bolted on here but the
/// ordinary snapshot comparison reading a field that happens to say it. The
/// op's own `base_version` is the *source's* half of the same question:
/// Confluence is sent `base_version + 1` and aborts on conflict, which is the
/// backstop for the window between the last sync and the flush that the
/// mirror cannot see (issue #286, ADR-0012).
///
/// **Both of this shape's error directions are the safe one, and both are worth
/// stating for this op.** A colleague replying to the page bumps no version
/// number, but the reply rides in `children.comment` inside the payload and in
/// `body_text` beside it, so it holds the edit -- a *false* hold, over two
/// bodies that read alike. That is `"transition"`'s trade-off in its own
/// words: a false hold shows both versions side by side and is one *Apply
/// anyway* away, a missed hold shows nothing. The panel prints the version
/// number beside each side precisely so a reader can see at a glance that this
/// is the false one. And in the other direction, an edit whose `base_version`
/// was *already* behind the mirror when it was queued -- the reader typed for
/// two minutes while a sync landed -- is not held, because nothing changed
/// between queue and flush: the snapshot is of a page that had already moved.
/// That write goes, and Confluence refuses it on the version. Which is the
/// division of labour spec #272 asks for, not a hole in it.
///
/// For a create the target is the **container**, and knobas does not mirror
/// every container: there is no `jira:PAY` item. Such a target projects
/// `{"live": false}` at queue time and again at flush time, which is equal, so
/// it sends. That is the intended reading, not an accident of the lookup
/// failing.
///
/// **There is no fallback shape.** `WriteOp::delivery` matches with no
/// wildcard arm (ADR-0006, amended 2026-10-19), so a new op does not compile
/// until somebody writes down which of the three shapes it gets.
///
/// `payload` is the queued write's own (`WriteOp`'s serde form) and `state`
/// the target kind's declared state paths; only a per-field record reads the
/// first, and only it and the declared state the second
/// ([`Projection::reads_declared_state`]).
#[must_use]
pub fn project(
    op: &str,
    shape: Projection,
    payload: &serde_json::Value,
    target: Option<&Target>,
    state: &[StatePath],
) -> serde_json::Value {
    match shape {
        Projection::Liveness => serde_json::json!({ "op": op, "live": target.is_some() }),
        Projection::DeclaredState => added(op, target, state),
        Projection::Whole { per_field: true } => {
            match edit_text(edit_writes(payload), target, state) {
                Some(text) => {
                    serde_json::json!({ "op": op, "live": target.is_some(), "text": text })
                }
                None => whole(op, target),
            }
        }
        Projection::Whole { per_field: false } => whole(op, target),
    }
}

/// What a write that adds compares (#752): its target's declared state, or
/// only whether the target is there when its kind declares none.
fn added(op: &str, target: Option<&Target>, state: &[StatePath]) -> serde_json::Value {
    let Some(target) = target.filter(|_| !state.is_empty()) else {
        return serde_json::json!({ "op": op, "live": target.is_some() });
    };
    match resolve_state(&target.payload, state) {
        Some(chip) => serde_json::json!({ "op": op, "live": true, "text": chip.text }),
        // A declaration that reads nothing: fail wide (ADR-0007).
        None => whole(op, Some(target)),
    }
}

/// The whole mirrored record: any change to the target at all holds.
fn whole(op: &str, target: Option<&Target>) -> serde_json::Value {
    serde_json::json!({
        "op": op,
        "live": target.is_some(),
        "title": target.map(|t| t.title.as_str()),
        "text": target.map(|t| t.body_text.as_str()),
        "item_updated_at": target.and_then(|t| t.item_updated_at),
        "payload": target.map(|t| &t.payload),
    })
}

/// Which of a pull request's fields a queued `edit_pull_request` writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EditWrites {
    title: bool,
    body: bool,
    state: bool,
}

/// The one statement that reads an `edit_pull_request`'s fields out of its
/// queued payload (ADR-0007 requirement 2). It **fails wide** (requirement 3,
/// pinned by `an_edit_read_that_misses_holds_on_the_whole_record`): a field is
/// written unless its key is present and `null`. The queue stores
/// `serde_json::to_value(&op)`, which spells every field, so a field left
/// alone is always that explicit `null`; a renamed key or variant reads as
/// written, which widens the projection to a false hold, never a missed one.
/// What an `edit_pull_request` compares, one part per field it writes:
/// `Some(None)` for a target that is gone (only `live` then differs), `None`
/// for the whole-record fallback -- an edit that writes nothing, or a state
/// change whose declared state resolves nothing.
#[allow(clippy::option_option)]
fn edit_text(
    writes: EditWrites,
    target: Option<&Target>,
    state: &[StatePath],
) -> Option<Option<String>> {
    if !(writes.title || writes.body || writes.state) {
        return None;
    }
    let Some(target) = target else {
        return Some(None);
    };
    let mut parts = Vec::new();
    if writes.state {
        parts.push(resolve_state(&target.payload, state)?.text);
    }
    // `body_text` carries the title, so a description's part covers both.
    if writes.body {
        parts.push(target.body_text.clone());
    } else if writes.title {
        parts.push(target.title.clone());
    }
    Some(Some(parts.join("\n\n")))
}

fn edit_writes(payload: &serde_json::Value) -> EditWrites {
    let written = |field: &str| {
        !payload
            .pointer(&format!("/EditPullRequest/{field}"))
            .is_some_and(serde_json::Value::is_null)
    };
    EditWrites {
        title: written("title"),
        body: written("body"),
        state: written("state"),
    }
}

/// Every write knobas still owes a source, newest first.
///
/// The three open states only: a sent or discarded write is history, and the
/// list is "what knobas still owes", not an audit log. Unbounded on purpose --
/// a queue you cannot see the end of is a count, which is the thing issue #42
/// says this must not be.
///
/// # Errors
///
/// [`CoreError::Db`] if the query fails.
pub async fn open(pool: &PgPool) -> Result<Vec<QueuedWrite>, CoreError> {
    let rows = sqlx::query_as::<_, QueuedWrite>(concat!(
        "select ",
        queue_columns!(),
        " from knobas.write_queue
           where state in ('pending','held','refused')
           order by id desc"
    ))
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// How many writes are in each open state.
///
/// One statement rather than three, and separate numbers rather than a total:
/// the shell has to say that something needs a *decision* rather than
/// patience, so "3 waiting" may never absorb a held write.
#[derive(Clone, Copy, Debug, Default, Serialize, sqlx::FromRow, ts_rs::TS)]
#[ts(export_to = "sources.ts")]
pub struct QueueCounts {
    pub pending: i64,
    pub held: i64,
    pub refused: i64,
}

/// The counts the shell shows.
///
/// # Errors
///
/// [`CoreError::Db`] if the query fails.
pub async fn counts(pool: &PgPool) -> Result<QueueCounts, CoreError> {
    let counts = sqlx::query_as::<_, QueueCounts>(
        "select count(*) filter (where state = 'pending') as pending,
                count(*) filter (where state = 'held')    as held,
                count(*) filter (where state = 'refused') as refused
           from knobas.write_queue",
    )
    .fetch_one(pool)
    .await?;
    Ok(counts)
}

/// The writes a flush of `source_id` may attempt right now: **the head of each
/// entity's queue, and only when that head is pending.**
///
/// That shape is the whole of the ordering guarantee, and both halves of it
/// matter:
///
/// * *One per entity*, so a stalled write blocks its own successors and
///   nothing else -- a second entity's queue, and every other source's, keep
///   moving (story 21).
/// * *The head, whatever state it is in.* An entity whose oldest open write is
///   held or refused yields **nothing**, rather than yielding the pending
///   write behind it. Skipping to the successor would have a comment written
///   second land first the moment its predecessor stopped for a decision --
///   the ordering guarantee (story 22) broken by the very mechanism that
///   exists to protect the user.
///
/// A held write therefore never reaches the flush loop at all: not because the
/// loop remembers to skip it, but because it is not something this query can
/// return. That is what makes "a held write never flushes on its own" a
/// property rather than a rule.
///
/// A caller that flushed everything this returns in parallel would still be
/// correct.
///
/// # Errors
///
/// [`CoreError::Db`] if the query fails.
pub async fn due(pool: &PgPool, source_id: &str) -> Result<Vec<QueuedWrite>, CoreError> {
    // `distinct on` picks each entity's oldest *open* write; the outer filter
    // then drops the entities whose head is not pending. Filtering inside
    // would pick the oldest pending write instead, which is the bug above.
    let rows = sqlx::query_as::<_, QueuedWrite>(concat!(
        "select ",
        queue_columns!(),
        " from (select distinct on (entity_id) ",
        queue_columns!(),
        "        from knobas.write_queue
                where source_id = $1 and state in ('pending','held','refused')
                order by entity_id, id) as head
           where state = 'pending'
           order by id"
    ))
    .bind(source_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Every transition below is one `update ... where id = $1 and state in (..)`,
/// returning the row it changed.
///
/// Two of them wrap that update in a CTE and write `knobas.worklog` in the
/// same statement -- [`sent`] stamps the copy, [`discard`] deletes it -- and
/// both say in place why that may not be a second statement. Both expand this
/// macro inside the CTE rather than restating the update, so the shape and the
/// concurrency argument below are theirs too. What each sets and which states
/// it accepts are still its own -- that is what this macro takes as its
/// argument -- and so is the sub-statement beside it.
///
/// The state guard in the `where` clause is what makes each of these safe to
/// call concurrently with the others: two flush loops that both decided to
/// send the same write cannot both settle it, and `None` -- no row matched --
/// is how the loser finds out. It is the same `Option` `link::unlink` returns
/// and for the same reason: `Ok(())` alone would have a caller announce a
/// transition that did not happen, and every one of these transitions writes
/// an activity line.
macro_rules! transition {
    ($sql:expr) => {
        concat!(
            "update knobas.write_queue set ",
            $sql,
            " returning ",
            queue_columns!()
        )
    };
}

/// Record a flush attempt that could not be delivered: the write stays
/// pending and now says why it is waiting.
///
/// Only a *retryable* fault reaches here -- [`refuse`] is the other half, and
/// ADR-0004's structured status on `SourceError` is what tells them apart.
///
/// `None` if the write is not pending: it was held, refused or settled while
/// the attempt was in flight.
///
/// # Errors
///
/// [`CoreError::Db`] if the statement fails.
pub async fn wait(
    pool: &PgPool,
    id: i64,
    reason: WaitReason,
    detail: Option<&str>,
) -> Result<Option<QueuedWrite>, CoreError> {
    let row = sqlx::query_as::<_, QueuedWrite>(transition!(
        "wait_reason = $2, detail = $3, attempted_at = now(), attempts = attempts + 1
          where id = $1 and state = 'pending'"
    ))
    .bind(id)
    .bind(reason.as_str())
    .bind(detail)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// A person acted on `source_id` -- a new credential, a new address or
/// configuration: its writes waiting on its credential ([`waits_on_credential`])
/// are owed again: a refused one (#1628), and a keychain that would not open
/// (#1944). Each comes back pending with `wait_reason` null, which
/// the flush loop reads as "nothing known in its way", so the next flush sends
/// it with whatever credential is stored now.
///
/// Nothing else ends an `unauthorized` wait on its own: the flush loop does
/// not send such a write on a timer, because re-sending a refused credential
/// is how an instance with a login-failure lockout locks the account the user
/// is about to re-enter. `detail` is kept: it is still what the last attempt
/// was told.
///
/// A keychain wait would pass on its own on the ladder. Releasing it too is
/// for the person who just unlocked the keychain and saved: the write goes
/// with the next flush rather than up to an hour later.
///
/// Whole-source rather than `where id = $1`, so it is not one of the
/// [`transition!`]s below; the state guard is theirs.
///
/// # Errors
///
/// [`CoreError::Db`] if the statement fails.
pub async fn release_credential_waits(
    pool: &PgPool,
    source_id: &str,
) -> Result<Vec<QueuedWrite>, CoreError> {
    let rows = sqlx::query_as::<_, QueuedWrite>(concat!(
        "update knobas.write_queue set wait_reason = null
          where source_id = $1 and state = 'pending'
            and (wait_reason = 'unauthorized'
                 or (wait_reason = 'unreachable' and starts_with(detail, $2)))
         returning ",
        queue_columns!()
    ))
    .bind(source_id)
    .bind(KEYCHAIN_DETAIL_PREFIX)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// The source rejected the write outright: it stops being offered.
///
/// `detail` is what the source said, kept so a permanent failure is reportable
/// rather than merely counted (story 19). The row stays open -- the user may
/// still discard it or edit it and send again -- but nothing retries it.
///
/// `None` if the write is not pending.
///
/// # Errors
///
/// [`CoreError::Db`] if the statement fails.
pub async fn refuse(
    pool: &PgPool,
    id: i64,
    detail: &str,
) -> Result<Option<QueuedWrite>, CoreError> {
    let row = sqlx::query_as::<_, QueuedWrite>(transition!(
        "state = 'refused', wait_reason = null, detail = $2, \
         attempted_at = now(), attempts = attempts + 1
          where id = $1 and state = 'pending'"
    ))
    .bind(id)
    .bind(detail)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// The held side of a write the **source** said moved under it (issue #822):
/// the snapshot taken when the write was queued, plus what the source holds
/// now -- `text`, the target's text at that version, and `base`, the version
/// itself in the source's spelling.
///
/// Built on the queued snapshot rather than on a fresh [`project`], because
/// the mirror did not move: the source did, and the projection's own fields
/// (`live`) still say what they said. `text` is the key the held panel
/// already reads for the target as it is now. `base` is what
/// [`apply_anyway`] re-sends the write against, and its presence is what
/// marks the hold as the source's rather than the mirror's.
#[must_use]
pub fn moved(op: &str, queued: serde_json::Value, base: &str, current: &str) -> serde_json::Value {
    let mut held = held_over(op, queued);
    held.insert("text".to_owned(), serde_json::json!(current));
    held.insert("base".to_owned(), serde_json::json!(base));
    serde_json::Value::Object(held)
}

/// The queued snapshot as a map with the op named, which a hold the source
/// reported ([`moved`], [`unconfirmed`]) adds its own keys to.
fn held_over(op: &str, queued: serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
    let mut held = match queued {
        serde_json::Value::Object(map) => map,
        _ => serde_json::Map::new(),
    };
    held.insert("op".to_owned(), serde_json::json!(op));
    held
}

/// The held side of a write that reached its source and was never answered
/// (issue #1125): the snapshot taken when the write was queued, plus
/// `unconfirmed`, naming the source a person should check (`source`, its
/// display name) and what the transport said (`detail`).
///
/// The write may have landed, so the flush loop does not send it again by
/// itself. The reader checks the source, then sends it again
/// ([`apply_anyway`]) or discards it. Like [`moved`]'s `base`, the key marks
/// whose hold this is: `apply_anyway` keeps `target_snapshot` for it, since
/// the mirror did not move.
#[must_use]
pub fn unconfirmed(
    op: &str,
    queued: serde_json::Value,
    source: &str,
    detail: &str,
) -> serde_json::Value {
    let mut held = held_over(op, queued);
    held.insert(
        "unconfirmed".to_owned(),
        serde_json::json!({ "source": source, "detail": detail }),
    );
    serde_json::Value::Object(held)
}

/// Where an op's payload says which version of its target it was made
/// against, for the ops whose source checks it ([`moved`]): the path
/// [`apply_anyway`] writes the version the reader was shown into. Spelled as
/// a PostgreSQL `text[]` path for `jsonb_set`.
///
/// Only `"commit_file"` (issue #822). `update_page`'s `base_version` and
/// `set_parameter`'s `base` are refused at the source rather than reported
/// as moved, so they never reach a hold with a `base`.
///
/// It reads knobas' own `WriteOp` serde shape, so ADR-0007's requirement 3
/// binds it, and a miss **fails safe**: a path that no longer names the field
/// leaves the stale base in place, the source answers it moved again, and the
/// write is held again -- a hold the reader can see, never a silent
/// overwrite. `a_commit_over_a_file_that_moved_is_held_with_both_versions`
/// (`knobas-sync`) pins the path, since a drifted one keeps that write held.
#[must_use]
pub fn base_path(op: &str) -> Option<&'static [&'static str]> {
    match op {
        "commit_file" => Some(&["CommitFile", "base_sha"]),
        _ => None,
    }
}

/// The target changed since the write was queued: hold it instead of sending
/// it.
///
/// `current` is [`project`]'s reading of the target *now* -- the other half of
/// "both versions side by side". It is stored rather than recomputed on read
/// because what the user is asked about is the change that arose, not whatever
/// the target happens to say by the time they look.
///
/// From here nothing but the user moves the row: [`apply_anyway`], [`discard`]
/// or [`amend`]. There is no fourth exit and no timeout.
///
/// `None` if the write is not pending.
///
/// # Errors
///
/// [`CoreError::Db`] if the statement fails.
pub async fn hold(
    pool: &PgPool,
    id: i64,
    current: serde_json::Value,
) -> Result<Option<QueuedWrite>, CoreError> {
    let row = sqlx::query_as::<_, QueuedWrite>(transition!(
        "state = 'held', wait_reason = null, held_snapshot = $2, \
         attempted_at = now(), attempts = attempts + 1
          where id = $1 and state = 'pending'"
    ))
    .bind(id)
    .bind(current)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// The source accepted the write. Terminal.
///
/// `remote_id` is what the source said it made -- `WriteReceipt::remote_id`,
/// which is `None` for an op that creates nothing addressable. When it is
/// present it is written **in the same statement as the settle**, to two
/// places: the queue row itself, and the `knobas.worklog` row that names this
/// write. That is the
/// whole of how a worklog's local copy comes to carry Jira's id (issue #280).
///
/// # Why both
///
/// The copy is the one a person reads, and the queue row is the one that is
/// always there. `worklog::log` queues, writes the copy, then flushes -- but
/// the scheduler flushes on its own tick too, and a tick landing between those
/// first two steps settles the write while no copy names it. The `update`
/// below would match nothing, and the id exists nowhere else. Keeping it on
/// the queue row as well means the copy can adopt it afterwards, so the two
/// orderings agree.
///
/// # Why this statement knows about `knobas.worklog`
///
/// Because the alternative is a window. `Source::write` answers the id exactly
/// once, in the flush loop, and no transaction spans that call and this one
/// (ADR-0012) -- so a second statement here could settle the write and then
/// fail to stamp the copy, leaving a worklog that Jira holds and knobas cannot
/// name, with nothing left to re-read it from. One statement makes "the write
/// settled" and "the copy carries the id" the same event.
///
/// The `update` matches nothing for every other op and for every write that
/// carries no worklog, which is what keeps this a settle that stamps rather
/// than a settle that depends on a worklog existing.
///
/// `None` if the write is not pending -- which is what stops a flush loop that
/// raced with the user's *discard* from resurrecting a withdrawn write. A
/// write that did not settle stamps nothing, because the `update` reads the
/// settled row rather than the argument.
///
/// # Errors
///
/// [`CoreError::Db`] if the statement fails.
pub async fn sent(
    pool: &PgPool,
    id: i64,
    remote_id: Option<&str>,
) -> Result<Option<QueuedWrite>, CoreError> {
    let row = sqlx::query_as::<_, QueuedWrite>(concat!(
        "with settled as (",
        transition!(
            "state = 'sent', wait_reason = null, settled_at = now(), \
             attempted_at = now(), attempts = attempts + 1, \
             remote_id = coalesce($2, remote_id)
              where id = $1 and state = 'pending'"
        ),
        "
         ), stamped as (
           update knobas.worklog w
              set remote_id = $2
             from settled
            where w.write_queue_id = settled.id and $2 is not null
         )
         select * from settled"
    ))
    .bind(id)
    .bind(remote_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// The user withdrew the write -- cancelling a pending one (story 7) or
/// conceding a held one (story 14) are the same act on the same row.
///
/// The row stays, for the reason `knobas.link` keeps its tombstones: "it was
/// discarded" and "it never existed" are different answers, and the activity
/// stream refers to it.
///
/// `None` if there was nothing open left to withdraw, which is what makes
/// discarding twice honest rather than merely harmless.
///
/// # And the withdrawal gives the time back (issue #328)
///
/// A `log_work` write carries a `knobas.worklog` row -- knobas' **copy** of a
/// record Jira is going to hold -- and the blocks that copy was made of point
/// back at it, which is what makes them read-only. Withdrawing the write used
/// to leave both in place: the week timesheet drew the hours as *held* for
/// good, `unlogged` stayed zero, and neither *Log all* nor the day review
/// would offer or edit the blocks again. A person who cancelled a queued
/// worklog had silently made that afternoon unloggable, with no way out from
/// any surface.
///
/// So the withdrawal deletes the copy, and the `on delete set null` on
/// `block_worklog_fk` gives the blocks back -- migration `0014`'s own words,
/// "deleting a worklog must give its blocks back, never take the afternoon
/// with it". The time returns to *unlogged* and every surface offers it again,
/// because all four of them read the same two columns.
///
/// **This does not contradict ADR-0012.** The rule there is that a *sent*
/// write is never rolled back, and this statement cannot reach one: the update
/// narrows on `state in ('pending','held','refused')`, [`sent`] is the only
/// writer of `state = 'sent'`, and no transition leads back out of it. What is
/// deleted is a copy of a record that was never made.
///
/// **Except when Jira answered anyway**, which is what `remote_id is null`
/// guards. The copy carries what the source called the worklog, and a copy
/// that has one is knobas' record that the hour exists at Jira; deleting that
/// would forget a worklog knobas cannot re-read, and then offer the same hour
/// to *Log all*, which bills it twice. The guard is on the delete itself
/// rather than left to the state machine, so the statement is safe on its own
/// terms.
///
/// The one gap it cannot close is at-least-once's own (ADR-0012): a write that
/// arrived and whose settle never landed is a `pending` row with no id
/// anywhere, and nothing here can tell it from one that never left. **That is
/// not only a crash.** `knobas_sync::write_queue::attempt` names the ordinary
/// case in place -- "the row settled under us -- the user withdrew it while
/// it was in flight" -- and the flush loop's per-source lock does not hold a
/// discard back. So the window is one HTTP round-trip wide, and inside it this
/// statement gives back blocks whose hour is at Jira, which *Log all* will
/// then offer again. `attempts` cannot narrow it either: every writer of that
/// column bumps it *after* the call, never before, so a write in flight is
/// indistinguishable from one that has not been tried.
///
/// **Nothing at this end can be made to know**, which is why the disclosure
/// is not here. The same window leaves a `create_ticket` or a `create_page`
/// standing at the source with nothing in knobas claiming it -- an *unclaimed
/// write*, `CONTEXT.md`'s word for it (issue #336). The moment that fact
/// exists is one step further on, in `knobas_sync::write_queue::unclaimed`:
/// there the receipt is in hand and [`sent`] has just come back empty, and
/// only together do those two say the write landed against a row that is
/// gone. This statement is only ever handed an id.
///
/// What that buys is the alternative #328 weighed and rejected -- a copy no
/// surface can release, reading as *held* for good. The queue row is kept,
/// discarded, with its payload, so what was withdrawn is still answerable even
/// when the copy is gone.
///
/// # Why this statement knows about `knobas.worklog`
///
/// [`sent`]'s reason, from the other end: the queue owns when a write stops
/// being owed, and the copy's existence is a fact about that. One statement
/// makes "the write is withdrawn" and "the time is knobas' own again" the same
/// event, so the withdrawal is never half-done: no webview reads a settled
/// queue row beside blocks this statement was going to release.
///
/// The claim stops at this statement, deliberately. `time::worklog::commit`
/// queues the write and writes the copy in that order, so a discard landing
/// between those two steps finds no copy to delete and the insert that follows
/// attaches one to a row that is already discarded -- #328's own shape, and
/// not withdrawable a second time. That window is two adjacent awaits wide and
/// only a person can open it, so it is recorded here rather than guarded, and
/// `time/week.rs` describes the cell it produces.
///
/// # Errors
///
/// [`CoreError::Db`] if the statement fails.
pub async fn discard(pool: &PgPool, id: i64) -> Result<Option<QueuedWrite>, CoreError> {
    let row = sqlx::query_as::<_, QueuedWrite>(concat!(
        "with settled as (",
        transition!(
            "state = 'discarded', wait_reason = null, settled_at = now()
              where id = $1 and state in ('pending','held','refused')"
        ),
        "
         ), released as (
           delete from knobas.worklog w
            using settled
            where w.write_queue_id = settled.id and w.remote_id is null
         )
         select * from settled"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// *I know, and I still mean it* (story 13): a held write returns to the queue.
///
/// The version the user was shown becomes the version the write is measured
/// against -- `held_snapshot` is copied over `target_snapshot` -- so the flush
/// that follows sends it rather than holding it on the same change again.
///
/// **A hold the source reported** ([`moved`]: its held side carries a
/// `base`) is released the same way in the source's terms (issue #822): the
/// version the user was shown is written into the payload at [`base_path`],
/// so the source compares against *their* version and takes the write, and
/// `target_snapshot` stays the queued one, since the mirror never moved. A
/// write whose payload has no such path keeps its payload.
///
/// **A write that may have landed** ([`unconfirmed`], #1125) is released as
/// *Send again*: the payload and `target_snapshot` are kept, and the flush
/// sends it once more.
///
/// **A change arriving after the user looked holds it again**, deliberately.
/// The alternative is a flag that makes the next flush skip hold detection,
/// which is a door onto exactly the silent last-write-wins this feature
/// exists to close: what the user consented to overwrite is what they saw.
///
/// `None` if the write is not held.
///
/// # Errors
///
/// [`CoreError::Db`] if the statement fails.
pub async fn apply_anyway(pool: &PgPool, id: i64) -> Result<Option<QueuedWrite>, CoreError> {
    let Some(held) = get(pool, id).await? else {
        return Ok(None);
    };
    let path: Vec<&str> = base_path(&held.op)
        .map(<[&str]>::to_vec)
        .unwrap_or_default();
    // One statement, guarded on `held` like every transition: the read above
    // only picks the payload path for the row's op, which never changes.
    let row = sqlx::query_as::<_, QueuedWrite>(transition!(
        "state = 'pending', \
         payload = case when held_snapshot ? 'base' and cardinality($2::text[]) > 0 \
                        then jsonb_set(payload, $2::text[], held_snapshot -> 'base') \
                        else payload end, \
         target_snapshot = case when held_snapshot ? 'base' or held_snapshot ? 'unconfirmed' \
                                then target_snapshot \
                                else coalesce(held_snapshot, target_snapshot) end, \
         wait_reason = null, detail = null
          where id = $1 and state = 'held'"
    ))
    .bind(id)
    .bind(path)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// *Edit and send* (story 15): a new payload, measured against a target the
/// user has just seen.
///
/// The way out of a refusal as well as out of a hold -- a source that rejected
/// a comment for its content will reject it again unchanged, so "edit it" is
/// the only exit other than conceding. `queued_at` is untouched: an amended
/// write is still the edit the user made when they made it.
///
/// `None` if the write has already settled.
///
/// # Errors
///
/// [`CoreError::Db`] if the statement fails.
pub async fn amend(
    pool: &PgPool,
    id: i64,
    payload: serde_json::Value,
    target_snapshot: serde_json::Value,
) -> Result<Option<QueuedWrite>, CoreError> {
    let row = sqlx::query_as::<_, QueuedWrite>(transition!(
        "state = 'pending', payload = $2, target_snapshot = $3, \
         held_snapshot = null, wait_reason = null, detail = null
          where id = $1 and state in ('pending','held','refused')"
    ))
    .bind(id)
    .bind(payload)
    .bind(target_snapshot)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two enums and migration 0005's CHECK constraints are one list
    /// written in two places, and neither may grow without the other: a
    /// variant the constraint does not allow is an `INSERT` that fails at
    /// runtime, and a spelling the enum does not know is a row this module's
    /// decoder *refuses*, so the write can never be read back at all.
    ///
    /// Driven by `ALL`, which the `closed_vocabulary!` macro generates from
    /// the same variant list as the enum, so a new state necessarily reaches
    /// this assertion.
    #[test]
    fn the_states_and_reasons_are_exactly_what_the_migration_allows() {
        let migration = include_str!("../../knobas-db/migrations/0005_write_queue.sql");
        for (marker, spellings, len) in [
            (
                "check (state in (",
                WriteState::ALL
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>(),
                WriteState::ALL.len(),
            ),
            (
                "wait_reason in (",
                WaitReason::ALL
                    .iter()
                    .map(|r| r.as_str())
                    .collect::<Vec<_>>(),
                WaitReason::ALL.len(),
            ),
        ] {
            let line = migration
                .lines()
                .find(|line| line.contains(marker))
                .unwrap_or_else(|| panic!("0005 has no line containing {marker:?}"));
            for spelling in &spellings {
                assert!(
                    line.contains(&format!("'{spelling}'")),
                    "{spelling:?} is a variant the constraint does not allow: {line}"
                );
            }
            assert_eq!(
                line.matches('\'').count() / 2,
                len,
                "the constraint and the enum list different numbers of values: {line}"
            );
        }
    }

    #[test]
    fn a_state_roundtrips_through_its_column_value() {
        for state in WriteState::ALL {
            assert_eq!(state.as_str().parse(), Ok(*state));
        }
        for reason in WaitReason::ALL {
            assert_eq!(reason.as_str().parse(), Ok(*reason));
        }
        assert!("expired".parse::<WriteState>().is_err());
    }

    /// The distinction the visible list and the shell count are built on.
    #[test]
    fn the_open_states_are_the_three_that_still_owe_a_source() {
        let open: Vec<_> = WriteState::ALL
            .iter()
            .filter(|s| s.is_open())
            .copied()
            .collect();
        assert_eq!(
            open,
            vec![WriteState::Pending, WriteState::Held, WriteState::Refused]
        );
    }

    /// The deputy's ruling on #752, pinned: a write that adds to a discussion
    /// -- a comment, an approval, a review -- holds only on the target's
    /// declared state. A new reply (which contract §4.1 puts into
    /// `body_text`), a retitle and a push (a payload change) all go; a merge
    /// or a close holds, and so does the target leaving the mirror. A kind
    /// that declares no state holds on that last alone.
    #[test]
    fn a_write_that_adds_holds_only_on_the_declared_state() {
        let before = Target {
            entity_id: "gitea:o/r#1".to_owned(),
            kind: "pr".to_owned(),
            title: "Bound the retry jitter".to_owned(),
            body_text: "Bound the retry jitter\n\nlooks good".to_owned(),
            item_updated_at: None,
            payload: serde_json::json!({"state": "open", "head": {"sha": "a1"}}),
        };
        let replied = Target {
            body_text: "Bound the retry jitter\n\nlooks good\n\nand a second".to_owned(),
            ..before.clone()
        };
        let retitled = Target {
            title: "Bound the jitter".to_owned(),
            ..before.clone()
        };
        let pushed = Target {
            payload: serde_json::json!({"state": "open", "head": {"sha": "b2"}}),
            ..before.clone()
        };
        let closed = Target {
            payload: serde_json::json!({"state": "closed", "head": {"sha": "a1"}}),
            ..before.clone()
        };
        let merged = Target {
            payload: serde_json::json!({"state": "closed", "merged": true, "head": {"sha": "a1"}}),
            ..before.clone()
        };
        let declared = gitea_pr_state();
        let none = serde_json::Value::Null;

        let op = "comment";
        assert!(Projection::DeclaredState.reads_declared_state());
        let at = |t: Option<&Target>, s: &[StatePath]| {
            project(op, Projection::DeclaredState, &none, t, s)
        };
        for (moved, what) in [
            (&replied, "a reply"),
            (&retitled, "a retitle"),
            (&pushed, "a push"),
        ] {
            assert_eq!(
                at(Some(&before), &declared),
                at(Some(moved), &declared),
                "{op}: {what} is not what a write that adds holds on"
            );
        }
        for (moved, what) in [(&closed, "a close"), (&merged, "a merge")] {
            assert_ne!(
                at(Some(&before), &declared),
                at(Some(moved), &declared),
                "{op}: {what} makes the write moot"
            );
        }
        assert_ne!(
            at(Some(&before), &declared),
            at(None, &[]),
            "{op}: a target that is gone has changed"
        );
        // A kind that declares no state: a reply still goes, a vanishing
        // target still holds.
        assert_eq!(at(Some(&before), &[]), at(Some(&replied), &[]));
        assert_eq!(at(Some(&before), &[]), at(Some(&closed), &[]));
        assert_ne!(at(Some(&before), &[]), at(None, &[]));
    }

    /// A write that adds, over a declared state that resolves nothing,
    /// projects the whole record: the miss is a false hold (ADR-0007).
    #[test]
    fn a_comment_over_a_state_the_declaration_misses_holds_on_the_whole_record() {
        let (before, after) = payload_differs();
        let projected = project(
            "comment",
            Projection::DeclaredState,
            &serde_json::Value::Null,
            Some(&before),
            &gitea_pr_state(),
        );
        assert_eq!(projected["payload"], before.payload);
        assert_ne!(
            projected,
            project(
                "comment",
                Projection::DeclaredState,
                &serde_json::Value::Null,
                Some(&after),
                &gitea_pr_state()
            )
        );
    }

    /// A target that differs only in `payload`, which is where every source
    /// keeps the field the op actually cares about.
    fn payload_differs() -> (Target, Target) {
        let target = |payload: serde_json::Value| Target {
            entity_id: "jira:PAY-231".to_owned(),
            kind: "ticket".to_owned(),
            title: "a payout fails".to_owned(),
            body_text: "it does".to_owned(),
            item_updated_at: None,
            payload,
        };
        (
            target(serde_json::json!({"status": "open"})),
            target(serde_json::json!({"status": "done"})),
        )
    }

    /// Issue #43's decision, pinned rather than left in a doc comment: the
    /// ops that put a judgement onto a target hold on **anything** about that
    /// target moving -- including a change only the raw payload can see, which
    /// is where every source keeps the status a transition is about and the
    /// head commit a merge takes. (An approval and a review left this list
    /// with #752: they add, and hold on the declared state.)
    #[test]
    fn a_judgement_op_holds_on_the_whole_record() {
        let (before, after) = payload_differs();
        // `update_page` is here and not with the creates, and the difference is
        // the sharpest of the three shapes: this op *replaces a page's body*,
        // so anything at all that happened to the page since the reader
        // started typing is something their re-assembled body would delete.
        // The payload the shape carries is where a Confluence page keeps
        // `version.number`, which is why "the mirror's version has passed the
        // one the edit was made against" needs no separate check (#286).
        // `"transition"`, `"update_page"`, `"merge_pull_request"`,
        // `"set_parameter"`, `"delete_branch"` (#707),
        // `"update_pull_request_branch"` (#708) and `"source_action"` (#1194)
        // take this shape (`WriteOp::delivery`).
        let op = "transition";
        let at =
            |t: Option<&Target>| project(op, Projection::WHOLE, &serde_json::Value::Null, t, &[]);
        assert_ne!(
            at(Some(&before)),
            at(Some(&after)),
            "{op}: a payload-only change is exactly the change it is about"
        );
        assert_ne!(
            at(Some(&before)),
            at(None),
            "{op}: a target that is gone has changed"
        );
    }

    /// The other half of that decision: an op that *adds* something beside
    /// what the container holds overwrites nothing, so it holds only when the
    /// container has left the mirror. A create that held because someone
    /// renamed the repository would ask the user a question they cannot answer.
    #[test]
    fn an_additive_op_holds_only_when_its_container_leaves_the_mirror() {
        let (before, after) = payload_differs();
        let renamed = Target {
            title: "a payout fails, differently".to_owned(),
            body_text: "it still does".to_owned(),
            item_updated_at: Some(chrono::Utc::now()),
            ..before.clone()
        };
        // The creates, the build triggers (`"run_build"` among them, #669),
        // `"log_work"`, the monitor ops, `"create_page"` (#286) and
        // `"commit_file"`, whose overwrite its `base_sha` guards at the source
        // (#822), take this shape (`WriteOp::delivery`).
        let op = "create_ticket";
        let live = |t: Option<&Target>| {
            project(op, Projection::Liveness, &serde_json::Value::Null, t, &[])
        };
        assert_eq!(
            live(Some(&before)),
            live(Some(&after)),
            "{op}: a payload change is not something a create would overwrite"
        );
        assert_eq!(
            live(Some(&before)),
            live(Some(&renamed)),
            "{op}: neither is a retitled or re-touched container"
        );
        assert_ne!(
            live(Some(&before)),
            live(None),
            "{op}: a container that is gone must still hold the write"
        );
    }

    /// A create's container need not be in the mirror at all -- `jira:PAY` is
    /// not an item. Both projections then read `live: false`, they are equal,
    /// and the write goes: an unmirrored container is not a hold, which is the
    /// intended reading rather than a lookup quietly failing.
    #[test]
    fn a_create_into_an_unmirrored_container_is_not_held() {
        assert_eq!(
            project(
                "create_ticket",
                Projection::Liveness,
                &serde_json::Value::Null,
                None,
                &[]
            ),
            project(
                "create_ticket",
                Projection::Liveness,
                &serde_json::Value::Null,
                None,
                &[]
            )
        );
        assert_eq!(
            project(
                "create_ticket",
                Projection::Liveness,
                &serde_json::Value::Null,
                None,
                &[]
            )["live"],
            serde_json::json!(false)
        );
    }

    /// `edit_pull_request`'s shape (`WriteOp::delivery`).
    const BY_FIELD: Projection = Projection::Whole { per_field: true };

    /// A queued `edit_pull_request` as the queue stores it: every field
    /// spelled, the ones left alone `null`.
    fn edit(title: Option<&str>, body: Option<&str>, state: Option<&str>) -> serde_json::Value {
        serde_json::json!({ "EditPullRequest": {
            "entity": "gitea:o/r#1", "title": title, "body": body, "state": state
        } })
    }

    /// Gitea's pull request state as `knobas-source-gitea` declares it: the
    /// merged flag first, then the state word.
    fn gitea_pr_state() -> Vec<StatePath> {
        use crate::payload::{PayloadPath, StateValue};
        use crate::tone::Tone;
        vec![
            StatePath {
                at: PayloadPath::of(["merged"]),
                values: vec![StateValue::of(true, Tone::Ok).shown_as("merged")],
            },
            StatePath {
                at: PayloadPath::of(["state"]),
                values: vec![
                    StateValue::of("open", Tone::Info),
                    StateValue::of("closed", Tone::Muted),
                ],
            },
        ]
    }

    /// A close whose declared state resolves nothing -- a record the
    /// declaration does not recognise -- projects the whole record, so the
    /// miss is a false hold (ADR-0007 requirement 3, for the state read).
    #[test]
    fn a_close_over_a_state_the_declaration_misses_holds_on_the_whole_record() {
        let (before, after) = payload_differs();
        let close = edit(None, None, Some("closed"));
        let projected = project(
            "edit_pull_request",
            BY_FIELD,
            &close,
            Some(&before),
            &gitea_pr_state(),
        );
        assert_eq!(projected["payload"], before.payload);
        assert_ne!(
            projected,
            project(
                "edit_pull_request",
                BY_FIELD,
                &close,
                Some(&after),
                &gitea_pr_state()
            )
        );
    }

    /// The deputy's ruling on #706, pinned: an edit holds only when what it
    /// would overwrite changed. A pull request somebody else retitled,
    /// re-described, commented on or closed is set beside each kind of edit.
    #[test]
    fn an_edit_holds_only_on_what_it_would_overwrite() {
        let before = Target {
            entity_id: "gitea:o/r#1".to_owned(),
            kind: "pr".to_owned(),
            title: "Bound the retry jitter".to_owned(),
            body_text: "Bound the retry jitter\n\nas discussed".to_owned(),
            item_updated_at: None,
            payload: serde_json::json!({"title": "Bound the retry jitter", "state": "open"}),
        };
        let retitled = Target {
            title: "Bound the jitter".to_owned(),
            body_text: "Bound the jitter\n\nas discussed".to_owned(),
            payload: serde_json::json!({"title": "Bound the jitter", "state": "open"}),
            ..before.clone()
        };
        let redescribed = Target {
            body_text: "Bound the retry jitter\n\nas agreed".to_owned(),
            ..before.clone()
        };
        let closed = Target {
            payload: serde_json::json!({"title": "Bound the retry jitter", "state": "closed"}),
            ..before.clone()
        };
        let merged = Target {
            payload: serde_json::json!({
                "title": "Bound the retry jitter", "state": "closed", "merged": true
            }),
            ..before.clone()
        };
        // Gitea's declaration (#666), spelled here: `knobas-core` knows no
        // adapter, it is handed the paths.
        let declared = gitea_pr_state();
        let held = |op: &serde_json::Value, after: Option<&Target>| {
            project("edit_pull_request", BY_FIELD, op, Some(&before), &declared)
                != project("edit_pull_request", BY_FIELD, op, after, &declared)
        };

        let title = edit(Some("t"), None, None);
        assert!(
            held(&title, Some(&retitled)),
            "a title edit holds on a retitle"
        );
        assert!(
            !held(&title, Some(&redescribed)),
            "not on a new description"
        );
        assert!(!held(&title, Some(&closed)), "and not on a close");

        let body = edit(None, Some("b"), None);
        assert!(
            held(&body, Some(&redescribed)),
            "a description edit holds on a new description"
        );
        assert!(!held(&body, Some(&closed)), "not on a close");
        // body_text carries the title too: a false hold, the safe direction.
        assert!(held(&body, Some(&retitled)));

        let close = edit(None, None, Some("closed"));
        assert!(
            held(&close, Some(&closed)),
            "a close holds on a state change"
        );
        assert!(held(&close, Some(&merged)), "and on a merge");
        // The ruling's own example: a retitle does not hold a queued Close.
        assert!(!held(&close, Some(&retitled)), "not on a retitle");
        assert!(!held(&close, Some(&redescribed)), "nor a new description");

        // A source that declares no state: the whole record, so the same
        // retitle holds -- a false hold, never a missed one.
        assert_ne!(
            project("edit_pull_request", BY_FIELD, &close, Some(&before), &[]),
            project("edit_pull_request", BY_FIELD, &close, Some(&retitled), &[]),
        );

        for op in [&title, &body, &close] {
            assert!(
                held(op, None),
                "a pull request that is gone has changed: {op}"
            );
            assert!(
                !held(op, Some(&before)),
                "nothing changed, nothing holds: {op}"
            );
        }
    }

    /// ADR-0007 requirement 3 for `edit_writes`: the read fails **wide**. A
    /// payload whose shape it does not know -- a renamed variant, a missing
    /// key, nothing at all -- projects the whole record, so a drift is a
    /// false hold and never a missed one.
    #[test]
    fn an_edit_read_that_misses_holds_on_the_whole_record() {
        let (before, after) = payload_differs();
        let whole = project(
            "transition",
            Projection::WHOLE,
            &serde_json::Value::Null,
            Some(&before),
            &[],
        );
        for payload in [
            serde_json::Value::Null,
            serde_json::json!({ "EditPull": { "title": "t", "body": null, "state": null } }),
            serde_json::json!({ "EditPullRequest": { "title": "t" } }),
            edit(None, None, None),
        ] {
            let projected = project("edit_pull_request", BY_FIELD, &payload, Some(&before), &[]);
            assert_eq!(projected["payload"], whole["payload"], "{payload}");
            assert_ne!(
                projected,
                project("edit_pull_request", BY_FIELD, &payload, Some(&after), &[]),
                "{payload}: a payload-only change must hold"
            );
        }
    }

    /// Each shape projects the field set its doc names. Liveness and the
    /// declared state over a kind that declares none share one: both then
    /// compare only whether the target is there.
    #[test]
    fn every_shape_projects_its_own_fields() {
        let (target, _) = payload_differs();
        for (shape, fields) in [
            (Projection::Liveness, &["live", "op"][..]),
            (Projection::DeclaredState, &["live", "op"][..]),
            (
                Projection::WHOLE,
                &["item_updated_at", "live", "op", "payload", "text", "title"][..],
            ),
            (BY_FIELD, &["live", "op", "text"][..]),
        ] {
            let keys: std::collections::BTreeSet<String> = project(
                "op",
                shape,
                &edit(Some("t"), None, None),
                Some(&target),
                &[],
            )
            .as_object()
            .expect("a projection is an object")
            .keys()
            .cloned()
            .collect();
            let projected: Vec<&str> = keys.iter().map(String::as_str).collect();
            assert_eq!(projected, fields, "{shape:?}");
        }
    }
}

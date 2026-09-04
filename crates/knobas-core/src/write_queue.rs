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
use crate::entity::EntityRef;

crate::closed_vocabulary! {
    /// What the queue will do about a write next.
    ///
    /// Stored as lowercase text in `knobas.write_queue.state`, whose
    /// `write_queue_state_chk` (migration 0005) allows exactly these
    /// spellings -- and `ALL` is what the test that pins the two together
    /// walks.
    pub enum WriteState {
        /// Will be retried automatically when the source can take it.
        Pending => "pending",
        /// The target changed after the write was queued. Waits for the user
        /// and for nothing else: no timeout, no auto-apply, no auto-discard.
        ///
        /// Since migration `0012` a target also leaves the view when the user
        /// turns its source off, and that write holds by the same mechanism.
        /// [`QueuedWrite::source_enabled`] is what tells the two apart -- the
        /// state stays one word because the queue's answer is the same
        /// (nothing sends until someone acts); only the explanation differs.
        Held => "held",
        /// The source rejected the write outright. Not retried -- a refusal is
        /// a decision, not a blip (ADR-0004 is what tells the two apart).
        Refused => "refused",
        /// Delivered. Terminal.
        Sent => "sent",
        /// Withdrawn by the user. Terminal.
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
    /// Only the two *retryable* faults appear here. A refusal is not a reason
    /// to wait, it is a reason to stop, and it has its own
    /// [`WriteState::Refused`]; `write_queue_reason_state_chk` in migration
    /// 0005 is what keeps a held or refused row from also claiming to be
    /// waiting on a network.
    pub enum WaitReason {
        /// The server did not answer.
        Unreachable => "unreachable",
        /// The credential was refused.
        Unauthorized => "unauthorized",
    }
}

/// Decode a `text` column into a closed vocabulary, the way `link::Origin`
/// does: the column is plain `text`, so the codec borrows `str`'s rather than
/// declaring a PostgreSQL enum type that does not exist.
macro_rules! text_codec {
    ($name:ident) => {
        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl std::str::FromStr for $name {
            type Err = UnknownValue;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                // Read off `ALL` rather than a second hand-written match: the
                // point of declaring the enum as a closed vocabulary is that
                // there is one list.
                $name::ALL
                    .iter()
                    .copied()
                    .find(|value| value.as_str() == s)
                    .ok_or_else(|| UnknownValue(s.to_owned()))
            }
        }

        impl sqlx::Type<sqlx::Postgres> for $name {
            fn type_info() -> sqlx::postgres::PgTypeInfo {
                <str as sqlx::Type<sqlx::Postgres>>::type_info()
            }

            fn compatible(ty: &sqlx::postgres::PgTypeInfo) -> bool {
                <&str as sqlx::Type<sqlx::Postgres>>::compatible(ty)
            }
        }

        impl<'r> sqlx::Decode<'r, sqlx::Postgres> for $name {
            fn decode(
                value: sqlx::postgres::PgValueRef<'r>,
            ) -> Result<Self, sqlx::error::BoxDynError> {
                let text = <&str as sqlx::Decode<sqlx::Postgres>>::decode(value)?;
                Ok(text.parse()?)
            }
        }
    };
}

/// A column value that is not one of a closed vocabulary's spellings.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("unknown write-queue value {0:?}")]
pub struct UnknownValue(pub String);

text_codec!(WriteState);
text_codec!(WaitReason);

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
#[derive(Clone, Debug, Serialize, sqlx::FromRow)]
pub struct QueuedWrite {
    /// Identity and queue order in one value -- see migration `0005`.
    pub id: i64,
    /// Which source owes the write.
    pub source_id: String,
    /// The target, as an entity id.
    pub entity_id: String,
    /// `knobas_source::WriteOp::identifier()`.
    pub op: String,
    /// The serialized `WriteOp`.
    pub payload: serde_json::Value,
    /// The target as it was when the write was queued -- [`project`]'s output.
    pub target_snapshot: serde_json::Value,
    pub state: WriteState,
    /// Why a pending write is waiting; `None` while nothing has been tried.
    pub wait_reason: Option<WaitReason>,
    /// What the source said, in its own words. Untrusted source text.
    pub detail: Option<String>,
    pub queued_at: chrono::DateTime<chrono::Utc>,
    pub attempted_at: Option<chrono::DateTime<chrono::Utc>>,
    pub attempts: i32,
    /// The target as it stood when the write was held -- the other half of
    /// "both versions side by side".
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
        "select entity_id, title, body_text, item_updated_at, payload
           from sync.live_item
          where entity_id = $1",
    )
    .bind(entity.to_string())
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// The ops whose definition of "changed" is **stated** rather than inferred.
///
/// Hold detection is per-op, and a new `WriteOp` variant that reaches this
/// module without an entry here would silently get the conservative fallback
/// in [`project`] -- correct, but not *stated*, which is what issue #42 asks
/// for. `knobas-sync`'s `every_write_op_has_a_stated_projection` matches on
/// `WriteOp` with no wildcard arm, so growing the enum stops that test
/// compiling until someone edits it -- and its message says what to decide.
/// The compiler puts the decision in front of the person growing the enum; it
/// is ADR-0006's review of that growth that must insist the new arm arrives
/// with a probe and a stated projection rather than a bare arm.
///
/// It lives here rather than beside `WriteOp` because `knobas-source` depends
/// on this crate, not the other way round.
pub const PROJECTED_OPS: &[&str] = &[
    "comment",
    "transition",
    "create_ticket",
    "create_branch",
    "create_pull_request",
    "approve",
    "trigger_build",
    "rerun_build",
    "log_work",
    "create_page",
    "update_page",
];

/// What `op` counts as its target having changed.
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
/// Three shapes, and which one an op gets is a statement about what that op
/// could *overwrite* -- never about how much work the comparison is.
///
/// **The indexed text** -- `"comment"`. Contract §4.1 makes every adapter build
/// `body_text` from the item's title, description and comment texts, so *a new
/// reply necessarily changes it*: that is the "somebody replied to the thread I
/// was answering" this op is holding against. An edit to the title or the
/// description changes it too, which is a false hold rather than a missed one
/// -- the safe direction, and the only one available without teaching this
/// module every source's payload shape.
///
/// **The whole mirrored record** -- `"transition"` and `"approve"`, the two ops
/// that put a *judgement* onto a target whose current state is the whole reason
/// for the judgement. Any change to the target at all holds the write.
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
/// * `"approve"` for the sharper version of the same reason: an approval is a
///   signature on a specific state of a pull request, and a force-push, a new
///   commit and a new review comment are all reasons to look again before it
///   lands.
///
/// **Liveness alone** -- `"create_ticket"`, `"create_branch"`,
/// `"create_pull_request"`, `"trigger_build"`, `"rerun_build"`, `"log_work"`,
/// `"create_page"`. These do not
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
/// `"create_page"` is in that group for the additive reason and not by
/// analogy: a new page goes *beside* whatever else sits under its parent, so a
/// parent whose title or discussion moved on is not a parent this write would
/// overwrite. What still holds it is the parent **leaving the mirror** -- a
/// page created under a deleted parent is a page nobody will find. It is the
/// one create whose container knobas really does mirror, so this is also the
/// one where that clause has teeth (issue #286).
///
/// **The whole mirrored record, and the version inside it** -- `"update_page"`
/// joins `"transition"` and `"approve"` in the `other` arm below, and the
/// reason is the sharpest of the three: this op *replaces a page's body*.
/// Anything at all that happened to the page since the reader started typing
/// is something their re-assembled body would silently delete.
///
/// That arm carries the verbatim `payload`, which is where a Confluence page
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
/// **There is no wildcard fallback in the sense of "and everything else is
/// fine".** The `other` arm below is the whole-record shape, and
/// [`PROJECTED_OPS`] plus `knobas-sync`'s
/// `every_write_op_has_a_stated_projection` is what stops a new op resting on
/// it silently: the op has to be listed, which means somebody wrote down which
/// of the three shapes it gets and why.
#[must_use]
pub fn project(op: &str, target: Option<&Target>) -> serde_json::Value {
    let live = target.is_some();
    match op {
        "comment" => serde_json::json!({
            "op": "comment",
            "live": live,
            "text": target.map(|t| t.body_text.as_str()),
        }),
        "create_ticket"
        | "create_branch"
        | "create_pull_request"
        | "trigger_build"
        | "rerun_build"
        | "log_work"
        | "create_page" => serde_json::json!({
            "op": op,
            "live": live,
        }),
        other => serde_json::json!({
            "op": other,
            "live": live,
            "title": target.map(|t| t.title.as_str()),
            "text": target.map(|t| t.body_text.as_str()),
            "item_updated_at": target.and_then(|t| t.item_updated_at),
            "payload": target.map(|t| &t.payload),
        }),
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
#[derive(Clone, Copy, Debug, Default, Serialize, sqlx::FromRow)]
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
/// which is `None` for every op but `log_work`. When it is present it is
/// written **in the same statement as the settle**, to two places: the queue
/// row itself, and the `knobas.worklog` row that names this write. That is the
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
    let row = sqlx::query_as::<_, QueuedWrite>(transition!(
        "state = 'pending', target_snapshot = coalesce(held_snapshot, target_snapshot), \
         wait_reason = null, detail = null
          where id = $1 and state = 'held'"
    ))
    .bind(id)
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

    /// A comment holds on the target's text and on nothing else: two targets
    /// differing only in their payload project the same, and a new reply --
    /// which contract §4.1 puts into `body_text` -- projects differently.
    #[test]
    fn a_comments_target_changes_when_its_text_does() {
        let target = |body: &str, payload: serde_json::Value| Target {
            entity_id: "jira:PAY-231".to_owned(),
            title: "a payout fails".to_owned(),
            body_text: body.to_owned(),
            item_updated_at: None,
            payload,
        };

        let before = target("a payout fails\n\nit does", serde_json::json!({"v": 1}));
        let noise = target("a payout fails\n\nit does", serde_json::json!({"v": 2}));
        let replied = target(
            "a payout fails\n\nit does\n\nlooking now",
            serde_json::json!({"v": 1}),
        );

        assert_eq!(
            project("comment", Some(&before)),
            project("comment", Some(&noise)),
            "a change the op does not care about must not hold the write"
        );
        assert_ne!(
            project("comment", Some(&before)),
            project("comment", Some(&replied)),
            "a new reply is exactly what a queued comment holds against"
        );
        assert_ne!(
            project("comment", Some(&before)),
            project("comment", None),
            "a target that is gone has changed"
        );
    }

    /// A target that differs only in `payload`, which is where every source
    /// keeps the field the op actually cares about.
    fn payload_differs() -> (Target, Target) {
        let target = |payload: serde_json::Value| Target {
            entity_id: "jira:PAY-231".to_owned(),
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

    /// An op with no stated projection at all falls back to the whole record,
    /// so an op that reached the queue unlisted holds on any change rather than
    /// on nothing. `PROJECTED_OPS` is what stops one getting there.
    #[test]
    fn an_unstated_op_holds_on_the_whole_record() {
        let unlisted = "an_op_no_milestone_has_ratified";
        assert!(!PROJECTED_OPS.contains(&unlisted));
        let (before, after) = payload_differs();
        assert_ne!(
            project(unlisted, Some(&before)),
            project(unlisted, Some(&after)),
        );
    }

    /// Issue #43's decision, pinned rather than left in a doc comment: the two
    /// ops that put a judgement onto a target hold on **anything** about that
    /// target moving -- including a change only the raw payload can see, which
    /// is where every source keeps the status a transition is about and the
    /// head commit an approval is a signature on.
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
        for op in ["transition", "approve", "update_page"] {
            assert!(PROJECTED_OPS.contains(&op), "{op} must be stated");
            assert_ne!(
                project(op, Some(&before)),
                project(op, Some(&after)),
                "{op}: a payload-only change is exactly the change it is about"
            );
            assert_ne!(
                project(op, Some(&before)),
                project(op, None),
                "{op}: a target that is gone has changed"
            );
        }
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
        for op in [
            "create_ticket",
            "create_branch",
            "create_pull_request",
            "trigger_build",
            "rerun_build",
            // A new page goes *beside* whatever else sits under its parent, so
            // a parent that was retitled or replied to is not a parent this
            // write would overwrite -- but a parent that left the mirror is a
            // page nobody would find the new one under (#286).
            "create_page",
        ] {
            assert!(PROJECTED_OPS.contains(&op), "{op} must be stated");
            assert_eq!(
                project(op, Some(&before)),
                project(op, Some(&after)),
                "{op}: a payload change is not something a create would overwrite"
            );
            assert_eq!(
                project(op, Some(&before)),
                project(op, Some(&renamed)),
                "{op}: neither is a retitled or re-touched container"
            );
            assert_ne!(
                project(op, Some(&before)),
                project(op, None),
                "{op}: a container that is gone must still hold the write"
            );
        }
    }

    /// A create's container need not be in the mirror at all -- `jira:PAY` is
    /// not an item. Both projections then read `live: false`, they are equal,
    /// and the write goes: an unmirrored container is not a hold, which is the
    /// intended reading rather than a lookup quietly failing.
    #[test]
    fn a_create_into_an_unmirrored_container_is_not_held() {
        assert_eq!(
            project("create_ticket", None),
            project("create_ticket", None)
        );
        assert_eq!(
            project("create_ticket", None)["live"],
            serde_json::json!(false)
        );
    }

    /// Every op the SPI defines is projected as exactly one of the three
    /// shapes, and no two shapes for one op. Reading the discriminating field
    /// set is what catches an op added to `PROJECTED_OPS` and to no arm of
    /// `project`, which would silently take the whole-record fallback while the
    /// list claimed it was stated.
    #[test]
    fn every_projected_op_has_one_of_the_three_shapes() {
        let (target, _) = payload_differs();
        for op in PROJECTED_OPS {
            let keys: std::collections::BTreeSet<String> = project(op, Some(&target))
                .as_object()
                .expect("a projection is an object")
                .keys()
                .cloned()
                .collect();
            let shape: Vec<&str> = keys.iter().map(String::as_str).collect();
            assert!(
                shape == ["live", "op", "text"]
                    || shape == ["live", "op"]
                    || shape == ["item_updated_at", "live", "op", "payload", "text", "title"],
                "{op:?} projects an unrecognised shape: {shape:?}"
            );
        }
    }
}

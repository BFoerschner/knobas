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

/// Every column of `knobas.write_queue` that leaves this module, in one place.
///
/// The same device, for the same reason, as `link_columns!`: [`QueuedWrite`]
/// is a `FromRow`, so a column this list forgets is a decode failure at run
/// time rather than a compile error. Naming them once is what stops the
/// `returning` clauses of eight statements from drifting apart.
macro_rules! queue_columns {
    () => {
        "id, source_id, entity_id, op, payload, target_snapshot, state, wait_reason, \
         detail, queued_at, attempted_at, attempts, held_snapshot, settled_at"
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
pub const PROJECTED_OPS: &[&str] = &["comment"];

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
/// * **`"comment"`** -- the target's **indexed text**. Contract §4.1 makes
///   every adapter build `body_text` from the item's title, description and
///   comment texts, so *a new reply necessarily changes it*: that is the
///   "somebody replied to the thread I was answering" this op is holding
///   against. An edit to the title or the description changes it too, which is
///   a false hold rather than a missed one -- the safe direction, and the only
///   one available without teaching this module every source's payload shape.
///
/// * **anything else** -- the whole mirrored record. Conservative by
///   construction: any change to the target at all holds the write. This is
///   the fallback, not a definition, and [`PROJECTED_OPS`] plus the test that
///   walks it is what stops a new op resting on it by accident.
#[must_use]
pub fn project(op: &str, target: Option<&Target>) -> serde_json::Value {
    let live = target.is_some();
    match op {
        "comment" => serde_json::json!({
            "op": "comment",
            "live": live,
            "text": target.map(|t| t.body_text.as_str()),
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
/// `None` if the write is not pending -- which is what stops a flush loop that
/// raced with the user's *discard* from resurrecting a withdrawn write.
///
/// # Errors
///
/// [`CoreError::Db`] if the statement fails.
pub async fn sent(pool: &PgPool, id: i64) -> Result<Option<QueuedWrite>, CoreError> {
    let row = sqlx::query_as::<_, QueuedWrite>(transition!(
        "state = 'sent', wait_reason = null, settled_at = now(), \
         attempted_at = now(), attempts = attempts + 1
          where id = $1 and state = 'pending'"
    ))
    .bind(id)
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
/// # Errors
///
/// [`CoreError::Db`] if the statement fails.
pub async fn discard(pool: &PgPool, id: i64) -> Result<Option<QueuedWrite>, CoreError> {
    let row = sqlx::query_as::<_, QueuedWrite>(transition!(
        "state = 'discarded', wait_reason = null, settled_at = now()
          where id = $1 and state in ('pending','held','refused')"
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

    /// An op with no stated projection falls back to the whole record, so it
    /// holds on any change at all rather than on nothing.
    #[test]
    fn an_unstated_op_holds_on_the_whole_record() {
        let target = |payload: serde_json::Value| Target {
            entity_id: "jira:PAY-231".to_owned(),
            title: "a payout fails".to_owned(),
            body_text: "it does".to_owned(),
            item_updated_at: None,
            payload,
        };
        assert!(!PROJECTED_OPS.contains(&"transition"));
        assert_ne!(
            project(
                "transition",
                Some(&target(serde_json::json!({"status": "open"})))
            ),
            project(
                "transition",
                Some(&target(serde_json::json!({"status": "done"})))
            ),
        );
    }
}

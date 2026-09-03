//! The standup digest: yesterday, today and blockers, every line traceable
//! (issue #288, spec #272 story 58-63).
//!
//! `CONTEXT.md`'s **digest**: *"the standup's generated three lists --
//! yesterday, today, blockers -- drawn from the mirror and the activity stream
//! for the configured usernames, every line linking to the item it came from.
//! Yesterday is the newest day before today that has any of your activity, at
//! most seven days back. Mine only."*
//!
//! # Derived, never stored
//!
//! The decision `knobas_core::inbox` records, and this module inherits whole:
//! the digest is computed from what is already there every time it is read.
//! There is no digest table, no per-event writer and nothing for a sync to
//! keep in step -- spec #272's design-doc correction (#274) is exactly this
//! sentence, and it is what makes a digest re-read after a sync tell the truth
//! about the day rather than about the moment somebody first opened the view.
//!
//! # The three producers, and why there are three and not one
//!
//! A day's work reaches this database by three different routes, and no one of
//! them can see the other two:
//!
//! * **The mirror** -- `sync.live_item` -- carries what the *sources* saw: a
//!   commit pushed, a pull request opened, a page edited. Interfaces §4.1
//!   normalizes `author`, so "mine" is a comparison against the configured
//!   usernames and needs no declared path and no per-source spelling.
//!
//!   **What §4.1's `author` means is the adapter's to say, and it is not
//!   always "wrote it".** Gitea maps a commit's author; the Jira adapter maps
//!   the **assignee** (`knobas-source-jira/src/map.rs`). So this half is
//!   *"items the source attributes to me"*, which is what its lines say in as
//!   many words -- a line reading "you authored this ticket" would be knobas
//!   claiming a ticket somebody else moved yesterday was written by the
//!   reader. Being generous about a ticket of the reader's own is the safe
//!   direction to be wrong in; putting a colleague's name on the reader's
//!   standup is not, and no source normalizes `author` to somebody who has
//!   nothing to do with the record.
//! * **The activity stream** carries what knobas *did on the user's behalf*: a
//!   comment, a transition, a ticket created -- every one of them a write that
//!   went through the queue, which writes one line per state change. The
//!   digest reads the **`queued`** line and no other, because that is the
//!   moment the person acted; `sent`, `held` and `refused` are the queue's own
//!   story about the same act, and listing them too would draw one comment
//!   three times.
//! * **`knobas.worklog`** carries the hours. It is read here rather than
//!   through its `log_work` queue line for two reasons: the local copy exists
//!   for a worklog restored from a backup that has no queue row at all, and
//!   reading both would put every logged afternoon on the list twice. So
//!   `log_work` is the one op the activity half skips.
//!
//! **What no producer carries, stated rather than left to be discovered: a
//! comment typed in a source's own UI.** A comment's author lives in the
//! verbatim payload, in the source's own shape, and `KindPaths` has no slot
//! for it -- so reading one would mean knobas guessing a path per source,
//! which is precisely the per-source coalesce #277 spent a milestone
//! removing. The activity half therefore carries the comments and transitions
//! the reader made *through knobas*, and a comment typed into Jira is absent
//! until a `KindPaths` slot for it is ratified, at which point this read
//! expires into the declaration like every other (ADR-0007).
//!
//! # Mine only, and what that rests on
//!
//! Story 63: *"the digest describes me only, so that I am never drafting a
//! colleague's lines from their commits."* Each producer answers it its own
//! way, and none of them guesses:
//!
//! * the mirror half compares `author` against the configured usernames, the
//!   same case-sensitive `= any(...)` the inbox's own author matching uses
//!   (#82) -- a username is a username as the source spells it;
//! * the activity half narrows to `actor = 'user'`, which is the *only* actor
//!   knobas writes for a person. `sync:<source_id>` is a source's and `knobas`
//!   is knobas' own (the relaunch sweep), so a colleague cannot appear in this
//!   half by construction -- there is no actor for one;
//! * `knobas.worklog` holds this machine's worklogs and nobody else's;
//! * blockers narrow on the **declared** assignee path (#277), so "my tickets"
//!   is the source's own answer to who a ticket belongs to.
//!
//! # What the seven-day cap is, and where it lives
//!
//! [`LOOKBACK_DAYS`]. The caller hands over the days it wants considered --
//! each one a [`DayWindow`], a date and the two instants it spans -- because
//! the reader's timezone is a fact only the webview holds; `crate::time::day`
//! records that rule in full and a digest that did the arithmetic itself would
//! be wrong on every day containing a daylight-saving change.
//!
//! What the caller does **not** decide is how far back the rule looks, and the
//! cap is applied to the windows' **dates** rather than to their number. Those
//! are not the same rule: "the newest seven of whatever arrived" holds
//! `CONTEXT.md`'s *"at most seven days back"* only for a caller that happens to
//! send seven consecutive days, so one window dated a fortnight ago would
//! quietly reach a fortnight back. Reading the dates makes the sentence true of
//! any list, in any order, which is what a rule the caller cannot widen means.

use chrono::{DateTime, Duration, NaiveDate, Utc};
use knobas_core::entity::EntityRef;
use knobas_core::payload::Declarations;
use sqlx::PgPool;

use crate::IpcError;
use crate::time::week::DayWindow;

/// How many days before the digest's own date the *yesterday* rule may reach.
///
/// Seven, so a Monday reads Friday and a Monday after a week off reads nothing
/// rather than reaching back into the month before it (story 60). Enforced
/// **here**, against the dates, for the reason the module header gives.
pub const LOOKBACK_DAYS: i64 = 7;

/// The most lines any one **producer** contributes to one list.
///
/// A bound rather than a paging story: a standup list nobody can read to the
/// end is already too long, and the alternative -- an unbounded read over a
/// day of a busy corpus -- is a view that stalls on the one morning it matters.
///
/// **Per producer, and the merged list is not capped again.** A second cap over
/// the merge would be a bound that can silently delete a whole producer: two
/// hundred mirror lines all dated later than the afternoon would push every
/// worklog off the list, and the reader would be looking at a standup with
/// their own hours missing and nothing saying so. Three bounded reads are a
/// bounded list.
const MOST_LINES: i64 = 200;

/// The relation that means "blocked by", read from the end that is blocked.
///
/// A link row is directed and carries one word, and `blocks` / `blocked by`
/// are that one row read from its two ends (`app/src/lib/detail/relations.ts`
/// holds the vocabulary). So an item is blocked when it is the **`to`** end of
/// a `blocks` link, and [`BLOCKED_BY_LINK`]'s join reads exactly that.
///
/// Only this word. `depends-on` is a different relation with a different
/// reading, and a digest that treated it as a blocker would be knobas deciding
/// what a word the user chose means.
///
/// Named `_RELATION` because `CONTEXT.md`'s **block** is a stretch of time and
/// `crate::time::week` already has a `BLOCKS` of its own; a bare `BLOCKS` here
/// would be two vocabularies sharing one word in one crate.
const BLOCKS_RELATION: &str = "blocks";

/// The actor every line a person is responsible for carries.
///
/// The spelling `knobas_core::activity::ActivityRow` reserves, and the same
/// constant `crate::inbox`, `commands::entity` and the write queue each write.
const ACTOR: &str = "user";

/// The `source` a line carries when knobas is the one saying it.
const KNOBAS: &str = "knobas";

/// The verb the running timer's line carries.
const TIMER: &str = "timer";

/// The verb a mirror line carries.
///
/// *Attributed*, not *authored*, and the word is the finding: §4.1's `author`
/// is whoever the **adapter** says a record belongs to, and the Jira adapter
/// says the assignee. The module header carries the whole reasoning.
const ATTRIBUTED: &str = "attributed";

/// The verb a worklog line carries -- the same word the write op is spelled
/// with, so a reader who has seen one recognises the other.
const LOG_WORK: &str = "log_work";

/// The verb a blocker the source called stuck carries.
const BLOCKED_STATUS: &str = "blocked_status";

/// The verb a blocker a link marks carries.
const BLOCKED_BY: &str = "blocked_by";

/// One line of one list: what it is about, and why it is here.
///
/// Story 59 -- *"every digest line links to the commit, PR, worklog, comment
/// or transition it came from, so that a line is something I can check"* -- is
/// what the first three fields are for, and [`reason`](Self::reason) is what
/// the rest of the row is for. The reason is produced **here**, by the rule
/// that made the line, and not re-rendered from [`verb`](Self::verb) at
/// display time: the discipline `knobas_core::inbox::InboxItem::reason` and
/// `suggest::SuggestionEntry::reason` both record, and for the same reason --
/// a line whose provenance cannot be shown is not shippable.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct DigestLine {
    /// The item to open, as an entity id -- what the view addresses.
    ///
    /// `None` for exactly one line: a running timer on an **ad-hoc label**,
    /// which has no item behind it (`CONTEXT.md`'s timer target). The inbox's
    /// credential expiry carries the same absence for the same reason -- a
    /// line that pointed somewhere wrong would be worse than one that says it
    /// has nowhere to point.
    pub entity_id: Option<String>,
    /// The entity's kind, for the monogram. `None` with `entity_id`.
    pub kind: Option<String>,
    /// What the line is about, in the source's own words -- or the ad-hoc
    /// label, for the one line that has no entity. Untrusted text: render it
    /// as text.
    pub title: String,
    /// **Which source.** A `source_config.id` for anything the mirror saw or a
    /// write was addressed to; [`KNOBAS`] for a line that is knobas' own
    /// record of an act -- a worklog copy, a running timer -- and has no
    /// source behind it.
    pub source: String,
    /// **Which verb**, as one word: [`ATTRIBUTED`] for the mirror half,
    /// [`LOG_WORK`], [`TIMER`], [`BLOCKED_STATUS`], [`BLOCKED_BY`] -- or, for
    /// a write, **the write queue's own `op`**.
    ///
    /// A `String` and deliberately not an enum, which is where this differs
    /// from its near-twin `knobas_core::inbox::InboxItem::category`. That
    /// vocabulary is closed and this one is not: a `WriteOp` identifier is the
    /// *adapter's* (ADR-0006 grows the set, and an out-of-process adapter can
    /// name one this binary has never heard of), so an enum here would need an
    /// `Other(String)` arm and would then be a closed type pretending. The
    /// five knobas-owned words are consts above so that no reader has to
    /// spell one twice.
    pub verb: String,
    /// Why this line is here, as a sentence naming the source and the verb.
    pub reason: String,
    /// When it happened -- and for a blocker, when the item last moved, which
    /// is what the list is ordered by.
    pub at: DateTime<Utc>,
}

/// The three lists, and which day the first of them is about.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct StandupDigest {
    /// The day [`yesterday`](Self::yesterday) is really about, in the reader's
    /// own reckoning -- Friday, on a Monday.
    ///
    /// `None` when the rule found nothing inside [`LOOKBACK_DAYS`], which is a
    /// week of silence and not an error. The view says so; a date filled in
    /// anyway would be a heading over an empty list claiming that day was
    /// quiet, when what happened is that no day was found.
    pub yesterday_day: Option<NaiveDate>,
    /// The newest earlier day with any of the user's activity, newest first.
    pub yesterday: Vec<DigestLine>,
    /// What has been touched since the day's own midnight, plus the running
    /// timer's target, newest first.
    pub today: Vec<DigestLine>,
    /// The user's items a source calls stuck or a link calls blocked.
    pub blockers: Vec<DigestLine>,
}

/// The digest for one day.
///
/// `today` is the day being asked about and `earlier` the days before it, in
/// any order; the ones inside [`LOOKBACK_DAYS`] are consulted newest first and
/// the rest are ignored. `now` is what decides whether the running timer
/// belongs on the list at all -- a digest read for a past date describes that
/// date, and a timer running this afternoon is not something that happened on
/// it.
///
/// # Errors
///
/// [`IpcError::internal`] if any of the reads fails.
pub async fn digest(
    pool: &PgPool,
    identity: &[String],
    declarations: &Declarations,
    now: DateTime<Utc>,
    today: DayWindow,
    earlier: &[DayWindow],
) -> Result<StandupDigest, IpcError> {
    let mut today_lines = work_in(pool, identity, today).await?;
    if today.from <= now && now < today.to {
        today_lines.extend(running_timer(pool).await?);
        sort_newest_first(&mut today_lines);
    }

    let mut yesterday_day = None;
    let mut yesterday = Vec::new();
    for window in in_reach(today.day, earlier) {
        let lines = work_in(pool, identity, window).await?;
        if !lines.is_empty() {
            yesterday_day = Some(window.day);
            yesterday = lines;
            break;
        }
    }

    Ok(StandupDigest {
        yesterday_day,
        yesterday,
        today: today_lines,
        blockers: blockers(pool, identity, declarations).await?,
    })
}

/// The candidate days for *yesterday*, newest first: those strictly before the
/// digest's own day and no more than [`LOOKBACK_DAYS`] before it.
///
/// Sorted here rather than trusted from the caller, and filtered by date
/// rather than by position -- both halves of the module header's rule. A
/// window dated on or after the digest's own day is dropped too: that day is
/// the *today* list's, and a caller that repeated it would otherwise get the
/// same lines under both headings.
fn in_reach(day: NaiveDate, earlier: &[DayWindow]) -> Vec<DayWindow> {
    let floor = day - Duration::days(LOOKBACK_DAYS);
    let mut candidates: Vec<DayWindow> = earlier
        .iter()
        .copied()
        .filter(|window| window.day < day && window.day >= floor)
        .collect();
    candidates.sort_by(|a, b| b.day.cmp(&a.day));
    candidates
}

/// Everything the user did inside one day's window, newest first.
///
/// The window is half-open, and it is the boundary that matters: `>= from` and
/// `< to`, so an item touched at 23:59 belongs to the day it was touched on
/// and to no other. A `>` on the lower edge would lose the first second of
/// every day and a `<=` on the upper one would put yesterday's last minute on
/// today's list.
async fn work_in(
    pool: &PgPool,
    identity: &[String],
    window: DayWindow,
) -> Result<Vec<DigestLine>, IpcError> {
    let mut lines = attributed_in_the_mirror(pool, identity, window).await?;
    lines.extend(written_through_knobas(pool, window).await?);
    lines.extend(logged_as_worklogs(pool, window).await?);
    sort_newest_first(&mut lines);
    Ok(lines)
}

/// Newest first, and stable in the tie -- three producers read separately can
/// hand back the same instant, and a list that reordered itself between two
/// reads of an unchanged database would be a view that flickers.
fn sort_newest_first(lines: &mut [DigestLine]) {
    lines.sort_by(|a, b| b.at.cmp(&a.at));
}

/// The mirror's answer to "what of mine moved on this day".
///
/// `FromRow` rather than a hand-written walk over `try_get`, the discipline
/// `knobas_core::link::LinkRow` records: a column the statement forgets is
/// then a decode failure the type system points at, rather than a `map_err`
/// somebody has to write correctly five times.
#[derive(sqlx::FromRow)]
struct MirrorRow {
    entity_id: String,
    kind: String,
    source_id: String,
    title: String,
    at: DateTime<Utc>,
}

/// Items the source attributes to the user that moved inside the window.
///
/// `coalesce(item_updated_at, synced_at)` is the instant, the same fallback
/// every mirror-derived read in this codebase uses: interfaces §4.1 lets a
/// source leave `updated_at` unset, and an item with no date could not be put
/// on a day at all.
const ATTRIBUTED_TO_ME: &str = r#"select i.entity_id, i.kind, i.source_id, i.title,
              coalesce(i.item_updated_at, i.synced_at) as at
         from sync.live_item i
        where i.author = any($1)
          and coalesce(i.item_updated_at, i.synced_at) >= $2
          and coalesce(i.item_updated_at, i.synced_at) <  $3
        order by at desc
        limit $4"#;

async fn attributed_in_the_mirror(
    pool: &PgPool,
    identity: &[String],
    window: DayWindow,
) -> Result<Vec<DigestLine>, IpcError> {
    let rows = sqlx::query_as::<_, MirrorRow>(ATTRIBUTED_TO_ME)
        .bind(identity)
        .bind(window.from)
        .bind(window.to)
        .bind(MOST_LINES)
        .fetch_all(pool)
        .await
        .map_err(IpcError::internal)?;
    Ok(rows
        .into_iter()
        .map(|row| DigestLine {
            entity_id: Some(row.entity_id),
            reason: format!("{} attributes this {} to you", row.source_id, row.kind),
            kind: Some(row.kind),
            title: row.title,
            source: row.source_id,
            verb: ATTRIBUTED.to_owned(),
            at: row.at,
        })
        .collect())
}

/// One write the reader made, as the activity stream recorded it.
///
/// `kind`, `title` and `source_id` are all nullable, and each absence is a
/// real state rather than a fault: the join is a **left** one, and a write's
/// `detail` is a jsonb object whose keys are the queue's to write.
#[derive(sqlx::FromRow)]
struct WriteRow {
    at: DateTime<Utc>,
    entity_id: String,
    op: String,
    source_id: Option<String>,
    kind: Option<String>,
    title: Option<String>,
}

/// The writes the user made through knobas, at the moment they made them.
///
/// **`queued` only, and `log_work` never.** The first is what stops one
/// comment appearing three times as the queue narrates its own progress; the
/// second is what stops every logged afternoon appearing twice, once here and
/// once out of `knobas.worklog`, which is the richer copy and the one that
/// survives a pruned queue.
///
/// The join is a **left** join on `knobas.entity` rather than on
/// `sync.live_item`: a write is a fact about something the person did, and it
/// stays a fact after the source it was addressed to has been removed and its
/// mirror rows swept. The entity id is the fallback title for exactly that
/// case -- an id is a poor label and a missing line is a worse one.
const WRITTEN: &str = r#"select a.at,
              a.entity_id,
              a.detail->>'op'        as op,
              a.detail->>'source_id' as source_id,
              e.kind,
              e.title
         from knobas.activity a
         left join knobas.entity e on e.id = a.entity_id
        where a.actor = $1
          and a.verb = 'queued'
          and a.entity_id is not null
          and a.detail->>'op' is not null
          and a.detail->>'op' <> $2
          and a.at >= $3
          and a.at <  $4
        order by a.at desc
        limit $5"#;

async fn written_through_knobas(
    pool: &PgPool,
    window: DayWindow,
) -> Result<Vec<DigestLine>, IpcError> {
    let rows = sqlx::query_as::<_, WriteRow>(WRITTEN)
        .bind(ACTOR)
        .bind(LOG_WORK)
        .bind(window.from)
        .bind(window.to)
        .bind(MOST_LINES)
        .fetch_all(pool)
        .await
        .map_err(IpcError::internal)?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let source = row
                .source_id
                .unwrap_or_else(|| namespace_of(&row.entity_id));
            DigestLine {
                reason: format!("you queued a {} on it in {source}", row.op),
                title: row.title.unwrap_or_else(|| row.entity_id.clone()),
                entity_id: Some(row.entity_id),
                kind: row.kind,
                source,
                verb: row.op,
                at: row.at,
            }
        })
        .collect())
}

/// One worklog, as knobas' own copy of it holds it.
#[derive(sqlx::FromRow)]
struct WorklogRow {
    at: DateTime<Utc>,
    entity_id: String,
    kind: Option<String>,
    title: Option<String>,
}

/// The hours, out of knobas' own copy of them.
///
/// Dated by `started_at` -- when the work happened -- and not by `created_at`,
/// which is when somebody got round to logging it. A Friday afternoon logged
/// on Monday morning belongs to Friday, which is the day a standup is about.
const WORKLOGS: &str = r#"select w.started_at as at, w.entity_id, e.kind, e.title
         from knobas.worklog w
         left join knobas.entity e on e.id = w.entity_id
        where w.started_at >= $1
          and w.started_at <  $2
        order by w.started_at desc
        limit $3"#;

async fn logged_as_worklogs(pool: &PgPool, window: DayWindow) -> Result<Vec<DigestLine>, IpcError> {
    let rows = sqlx::query_as::<_, WorklogRow>(WORKLOGS)
        .bind(window.from)
        .bind(window.to)
        .bind(MOST_LINES)
        .fetch_all(pool)
        .await
        .map_err(IpcError::internal)?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let source = namespace_of(&row.entity_id);
            DigestLine {
                reason: format!("you logged work on it in {source}"),
                title: row.title.unwrap_or_else(|| row.entity_id.clone()),
                entity_id: Some(row.entity_id),
                kind: row.kind,
                source,
                verb: LOG_WORK.to_owned(),
                at: row.at,
            }
        })
        .collect())
}

/// What an entity is called, for a line that has only an id.
#[derive(sqlx::FromRow)]
struct EntityRow {
    kind: String,
    title: String,
}

/// The timer's target, named. A named statement like every other read here.
const TIMER_TARGET: &str = "select kind, title from knobas.entity where id = $1";

/// What the timer is on right now -- story 61, *"today includes the running
/// timer's target, so the digest knows what I am on right now."*
///
/// At most one line, because there is at most one timer. An ad-hoc label
/// target produces a line with no `entity_id`: the digest still has to say
/// what the clock is on, and "DB config for the migration" is a legal answer
/// with nowhere to click.
async fn running_timer(pool: &PgPool) -> Result<Vec<DigestLine>, IpcError> {
    let Some(timer) = crate::time::current(pool).await? else {
        return Ok(Vec::new());
    };
    let line = match &timer.target {
        crate::time::TimerTarget::Label { label } => DigestLine {
            entity_id: None,
            kind: None,
            title: label.clone(),
            source: KNOBAS.to_owned(),
            verb: TIMER.to_owned(),
            reason: "the timer is running on this label".to_owned(),
            at: timer.started_at,
        },
        crate::time::TimerTarget::Entity { entity_id } => {
            let named = sqlx::query_as::<_, EntityRow>(TIMER_TARGET)
                .bind(entity_id)
                .fetch_optional(pool)
                .await
                .map_err(IpcError::internal)?;
            let source = namespace_of(entity_id);
            DigestLine {
                title: named
                    .as_ref()
                    .map_or_else(|| entity_id.clone(), |row| row.title.clone()),
                kind: named.map(|row| row.kind),
                reason: format!("the timer is running on it, in {source}"),
                entity_id: Some(entity_id.clone()),
                source,
                verb: TIMER.to_owned(),
                at: timer.started_at,
            }
        }
    };
    Ok(vec![line])
}

/// The source id an entity id names, or [`KNOBAS`] for one that names none.
///
/// Entity ids are `<source_id>:<key>` (P10), so the namespace *is* the source
/// -- and knobas' own kinds (notes, contexts) live in reserved namespaces that
/// no source answers for.
fn namespace_of(entity_id: &str) -> String {
    match EntityRef::parse(entity_id) {
        Ok(entity) if !knobas_core::entity::is_reserved_namespace(&entity.namespace) => {
            entity.namespace
        }
        _ => KNOBAS.to_owned(),
    }
}

/// The `KindPaths` key the blocked-like set is declared under.
///
/// `knobas_core::payload::field` does not carry it -- that module lists the
/// fields that are *paths*, and this one is a set of names -- so the literal
/// the statement below folds is pinned to the struct here instead, by
/// `the_blocked_set_is_looked_up_at_the_key_kind_paths_serializes_it_under`.
///
/// Test-only because that pin is the whole of its job: `concat!` folds
/// literals, so the statement cannot be built from a `const` and a production
/// reference to this one would be a second spelling of a value nothing reads.
#[cfg(test)]
const BLOCKED_STATUSES: &str = "blocked_statuses";

/// The user's items whose status their own source calls blocked-like.
///
/// **The set is the adapter's, read through the declaration** (#277,
/// `KindPaths::blocked_statuses`) and matched case-insensitively, which is
/// what that field's own documentation promises its reader. There is no list
/// of English words here and there must never be one: which statuses mean
/// "stuck" is a property of a source's workflow, and a source that declares
/// none contributes no blocker rather than a wrong one.
///
/// "The user's" is the **declared assignee path**, for the same reason: who a
/// ticket belongs to is the source's answer, not knobas'. A source that
/// declares no assignee therefore contributes nothing here either -- absence,
/// which is ADR-0007's pinned failure direction.
///
/// The declaration is bound as one jsonb parameter and resolved inside the
/// statement (`declared_string!`, `declared_array!`), so nothing is pulled
/// into Rust to be filtered there and nothing is concatenated into SQL.
macro_rules! blocked_by_status {
    () => {
        concat!(
            "select s.entity_id, s.kind, s.source_id, s.title, s.at, s.status
               from (select i.entity_id, i.kind, i.source_id, i.title,
                            coalesce(i.item_updated_at, i.synced_at) as at,
                            ",
            knobas_core::declared_string!("$2", "assignee"),
            " as assignee,
                            ",
            knobas_core::declared_string!("$2", "status_name"),
            " as status,
                            ",
            knobas_core::declared_array!("$2", "blocked_statuses"),
            " as blocked
                       from sync.live_item i) s
              where s.assignee = any($1)
                and s.status is not null
                and exists (select 1
                              from jsonb_array_elements_text(s.blocked) as b(name)
                             where lower(b.name) = lower(s.status))
              order by s.at desc
              limit $3"
        )
    };
}

const BLOCKED_BY_STATUS: &str = blocked_by_status!();

/// One blocker the source itself calls stuck.
#[derive(sqlx::FromRow)]
struct BlockedStatusRow {
    entity_id: String,
    kind: String,
    source_id: String,
    title: String,
    at: DateTime<Utc>,
    status: String,
}

/// The user's items a **link** marks blocked.
///
/// `knobas.confirmed_link`, never `knobas.link`: an unconfirmed row is a
/// suggestion knobas made and nobody accepted, and a standup listing one as a
/// blocker would be reporting a guess as a fact. The same reading
/// `knobas_core::link::entries_of` records.
///
/// The direction is [`BLOCKS_RELATION`]'s: the blocked item is the link's
/// **`to`** end, so joining `l.to_id` is what makes this list "things that are
/// stuck" rather than "things that are in somebody's way".
macro_rules! blocked_by_link {
    () => {
        concat!(
            "select s.entity_id, s.kind, s.source_id, s.title, s.at,
                    o.title as other_title
               from (select i.entity_id, i.kind, i.source_id, i.title,
                            coalesce(i.item_updated_at, i.synced_at) as at,
                            ",
            knobas_core::declared_string!("$2", "assignee"),
            " as assignee
                       from sync.live_item i) s
               join knobas.confirmed_link l on l.to_id = s.entity_id and l.relation = $3
               join knobas.entity o on o.id = l.from_id
              where s.assignee = any($1)
              order by s.at desc
              limit $4"
        )
    };
}

const BLOCKED_BY_LINK: &str = blocked_by_link!();

/// One blocker a link marks, with the end that is in the way.
#[derive(sqlx::FromRow)]
struct BlockedLinkRow {
    entity_id: String,
    kind: String,
    source_id: String,
    title: String,
    at: DateTime<Utc>,
    other_title: String,
}

/// Both halves of the blockers list, merged and sorted newest first.
///
/// An item can be on both and then it is on the list twice, with a different
/// reason each time -- deliberately. The two are different facts about the
/// same ticket (its source calls it blocked; somebody drew a link saying what
/// is in its way), and collapsing them would throw away one of the two things
/// a reader came for.
///
/// **Sorted after the merge, the shape [`work_in`] has.** Two reads each
/// ordered by `at` and then concatenated are not one list ordered by `at`:
/// they are two descending runs, and the view draws an *ago* column down the
/// side of them. [`DigestLine::at`] is documented as what every list is
/// ordered by, on both sides of the wire, so the merge has to make that true
/// rather than leave the reader to notice it is not.
async fn blockers(
    pool: &PgPool,
    identity: &[String],
    declarations: &Declarations,
) -> Result<Vec<DigestLine>, IpcError> {
    let by_status = sqlx::query_as::<_, BlockedStatusRow>(BLOCKED_BY_STATUS)
        .bind(identity)
        .bind(declarations.as_param())
        .bind(MOST_LINES)
        .fetch_all(pool)
        .await
        .map_err(IpcError::internal)?;
    let by_link = sqlx::query_as::<_, BlockedLinkRow>(BLOCKED_BY_LINK)
        .bind(identity)
        .bind(declarations.as_param())
        .bind(BLOCKS_RELATION)
        .bind(MOST_LINES)
        .fetch_all(pool)
        .await
        .map_err(IpcError::internal)?;

    let mut lines: Vec<DigestLine> = by_status
        .into_iter()
        .map(|row| DigestLine {
            entity_id: Some(row.entity_id),
            kind: Some(row.kind),
            title: row.title,
            reason: format!(
                "{} calls this status blocked-like: {}",
                row.source_id, row.status
            ),
            source: row.source_id,
            verb: BLOCKED_STATUS.to_owned(),
            at: row.at,
        })
        .collect();
    lines.extend(by_link.into_iter().map(|row| DigestLine {
        entity_id: Some(row.entity_id),
        kind: Some(row.kind),
        title: row.title,
        source: row.source_id,
        verb: BLOCKED_BY.to_owned(),
        reason: format!("a link marks it blocked by {}", row.other_title),
        at: row.at,
    }));
    sort_newest_first(&mut lines);
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(iso: &str) -> NaiveDate {
        iso.parse().expect("a date")
    }

    fn window(on: &str) -> DayWindow {
        let day = day(on);
        DayWindow {
            day,
            from: day.and_hms_opt(0, 0, 0).expect("midnight").and_utc(),
            to: day
                .succ_opt()
                .expect("the next day")
                .and_hms_opt(0, 0, 0)
                .expect("midnight")
                .and_utc(),
        }
    }

    /// The cap is a rule about **dates**, not about how many windows arrived.
    ///
    /// The distinction is the whole of it: `take(LOOKBACK_DAYS)` over whatever
    /// the caller sent holds `CONTEXT.md`'s "at most seven days back" only for
    /// a caller that happens to send seven consecutive days, so a single
    /// window dated a fortnight ago would reach a fortnight back. It is also
    /// what makes the order the caller used irrelevant.
    #[test]
    fn only_the_seven_days_before_the_digests_own_are_in_reach_of_yesterday() {
        let sent = [
            window("2026-08-31"), // the digest's own day
            window("2026-09-01"), // after it
            window("2026-08-17"), // a fortnight back -- one window, still out
            window("2026-08-23"), // eight days back -- one day too far
            window("2026-08-24"), // exactly seven days back -- in
            window("2026-08-30"), // the day before -- in, and first
        ];
        let reachable: Vec<String> = in_reach(day("2026-08-31"), &sent)
            .into_iter()
            .map(|window| window.day.to_string())
            .collect();
        assert_eq!(reachable, ["2026-08-30", "2026-08-24"]);
    }

    /// The two blocker statements resolve the declaration rather than naming a
    /// status, a path or an account of their own.
    ///
    /// A guard against the one regression #277 exists to prevent: a reader
    /// that "just adds" `or lower(status) = 'blocked'` would pass every test
    /// in the battery -- the fixtures spell it that way -- while quietly
    /// making the declared set optional. The literal set below is what a
    /// second spelling has to get past.
    #[test]
    fn the_blocker_statements_read_the_declaration_and_never_a_word_of_their_own() {
        for statement in [BLOCKED_BY_STATUS, BLOCKED_BY_LINK] {
            assert!(
                statement.contains(BLOCKED_STATUSES) || statement.contains("confirmed_link"),
                "a blocker statement asks the declaration or the link graph: {statement}"
            );
            for word in ["'Blocked'", "'blocked'", "'On Hold'", "'Impediment'"] {
                assert!(
                    !statement.contains(word),
                    "{word} is a status name in a statement: the set is the \
                     adapter's declaration, never a list here -- {statement}"
                );
            }
        }
    }

    /// The literal the blocked set is looked up by is the one `KindPaths`
    /// serializes it under.
    ///
    /// `concat!` folds literals, so the statement carries the field name as a
    /// *string* and no compiler check reaches it. `knobas_core`'s own
    /// `the_field_names_the_sql_macros_use_are_the_serialized_ones` pins the
    /// struct's key set, and this pins that key set to the one call site
    /// outside that crate -- without it a rename made in both those places
    /// would leave this read permanently missing, and missing is what a
    /// blockers list looks like when nobody is blocked.
    #[test]
    fn the_blocked_set_is_looked_up_at_the_key_kind_paths_serializes_it_under() {
        let declared = serde_json::to_value(knobas_source::KindPaths {
            blocked_statuses: vec!["Waiting for support".to_owned()],
            ..knobas_source::KindPaths::default()
        })
        .expect("a declaration serializes");
        assert!(
            declared.get(BLOCKED_STATUSES).is_some(),
            "`KindPaths` does not serialize a {BLOCKED_STATUSES:?} key: {declared}"
        );
        assert!(
            BLOCKED_BY_STATUS.contains(&format!("'{BLOCKED_STATUSES}'")),
            "the statement looks the set up under some other name: {BLOCKED_BY_STATUS}"
        );
    }

    /// The relation is spelled once and the join reads the blocked end.
    #[test]
    fn the_link_half_joins_the_blocked_end_of_a_blocks_link() {
        assert_eq!(BLOCKS_RELATION, "blocks");
        assert!(
            BLOCKED_BY_LINK.contains("l.to_id = s.entity_id"),
            "the blocked item is the `to` end; joining `from_id` would list \
             what is in somebody else's way"
        );
    }

    /// The op the activity half skips is the one the **SPI** spells.
    ///
    /// [`LOG_WORK`] is the whole of the no-double-counting rule: the activity
    /// statement skips `detail->>'op' = 'log_work'` because `knobas.worklog`
    /// already carries that afternoon. But the `op` in that column is written
    /// from [`WriteOp::identifier`](knobas_source::WriteOp::identifier), so a
    /// rename there and no rename here does not fail -- it silently stops
    /// matching, and every logged afternoon is on the standup twice with
    /// nothing saying so.
    ///
    /// The same pin `crate::time::worklog`'s
    /// `the_identifier_is_the_one_the_spi_defines` holds over the same
    /// constant, and for the same reason: a value two crates have to agree on
    /// needs one test that fails when they stop.
    #[test]
    fn the_skipped_op_is_the_one_the_spi_spells() {
        let op = knobas_source::WriteOp::LogWork {
            entity: "jira:PAY-231".to_owned(),
            started: "2026-09-03T09:00:00Z".parse().expect("an instant"),
            seconds: 3600,
            comment: String::new(),
        };
        assert_eq!(op.identifier(), LOG_WORK);
        assert!(
            WRITTEN.contains("a.detail->>'op' <> $2"),
            "the skip is the statement's, and $2 is where {LOG_WORK} is bound"
        );
    }

    /// An entity id's namespace is its source, and a knobas-owned id has none.
    #[test]
    fn a_lines_source_is_the_namespace_its_entity_id_carries() {
        assert_eq!(namespace_of("jira:PAY-231"), "jira");
        assert_eq!(namespace_of("note:0f9c"), KNOBAS);
        assert_eq!(namespace_of("nonsense"), KNOBAS);
    }
}

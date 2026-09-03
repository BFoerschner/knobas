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
//! be wrong on every day containing a daylight-saving change. What the caller
//! does **not** decide is how far back the rule looks: this module consults at
//! most the newest [`LOOKBACK_DAYS`] of them and ignores the rest, so the cap
//! is a property of the digest rather than of whichever surface asked for one.

use chrono::{DateTime, NaiveDate, Utc};
use knobas_core::entity::EntityRef;
use knobas_core::payload::Declarations;
use sqlx::{PgPool, Row};

use crate::IpcError;
use crate::time::week::DayWindow;

/// How many days before the digest's own date the *yesterday* rule may reach.
///
/// Seven, so a Monday reads Friday and a Monday after a week off reads nothing
/// rather than reaching back into the month before it (story 60). The rule is
/// enforced **here** and not by the caller: a webview that computed eight
/// windows would otherwise quietly widen the digest, and the one thing this
/// number is for is that it cannot.
pub const LOOKBACK_DAYS: usize = 7;

/// The most lines any one list carries.
///
/// A bound rather than a paging story: a standup list nobody can read to the
/// end is already too long, and the alternative -- an unbounded read over a
/// day of a busy corpus -- is a view that stalls on the one morning it matters.
const MOST_LINES: i64 = 200;

/// The relation that means "blocked by", read from the end that is blocked.
///
/// A link row is directed and carries one word, and `blocks` / `blocked by`
/// are that one row read from its two ends (`app/src/lib/detail/relations.ts`
/// holds the vocabulary). So a ticket is blocked when it is the **`to`** end
/// of a `blocks` link, and `blocked_by_the_link_graph`'s statement joins on
/// exactly that.
///
/// Only this word. `depends-on` is a different relation with a different
/// reading, and a digest that treated it as a blocker would be knobas deciding
/// what a word the user chose means.
const BLOCKS: &str = "blocks";

/// The actor every line a person is responsible for carries.
///
/// The spelling `knobas_core::activity::ActivityRow` reserves, and the same
/// constant `crate::inbox`, `commands::entity` and the write queue each write.
const ACTOR: &str = "user";

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
    /// write was addressed to; `knobas` for a line that is knobas' own record
    /// of an act -- a worklog copy, a running timer -- and has no source
    /// behind it.
    pub source: String,
    /// **Which verb**, as one word: `authored` for the mirror half, the write
    /// queue's own `op` for a write (`comment`, `transition`, ...),
    /// `log_work`, `timer`, `blocked_status` or `blocked_by`.
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
/// `today` is the day being asked about and `earlier` the days before it,
/// oldest first; at most the newest [`LOOKBACK_DAYS`] of `earlier` are
/// consulted. `now` is what decides whether the running timer belongs on the
/// list at all -- a digest read for a past date describes that date, and a
/// timer running this afternoon is not something that happened on it.
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
    let mut today_lines = work_in(pool, identity, today.from, today.to).await?;
    if today.from <= now && now < today.to {
        today_lines.extend(running_timer(pool).await?);
        sort_newest_first(&mut today_lines);
    }

    let mut yesterday_day = None;
    let mut yesterday = Vec::new();
    for window in earlier.iter().rev().take(LOOKBACK_DAYS) {
        let lines = work_in(pool, identity, window.from, window.to).await?;
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

/// Everything the user did inside one half-open window, newest first.
///
/// Half-open on purpose, and it is the boundary that matters: `>= from` and
/// `< to`, so an item touched at 23:59 belongs to the day it was touched on
/// and to no other. A `>` on the lower edge would lose the first second of
/// every day and a `<=` on the upper one would put yesterday's last minute on
/// today's list.
async fn work_in(
    pool: &PgPool,
    identity: &[String],
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<Vec<DigestLine>, IpcError> {
    let mut lines = authored_in_the_mirror(pool, identity, from, to).await?;
    lines.extend(written_through_knobas(pool, from, to).await?);
    lines.extend(logged_as_worklogs(pool, from, to).await?);
    sort_newest_first(&mut lines);
    lines.truncate(MOST_LINES as usize);
    Ok(lines)
}

/// Newest first, and stable in the tie -- three producers read separately can
/// hand back the same instant, and a list that reordered itself between two
/// reads of an unchanged database would be a view that flickers.
fn sort_newest_first(lines: &mut [DigestLine]) {
    lines.sort_by(|a, b| b.at.cmp(&a.at));
}

/// Items the user authored that moved inside the window.
///
/// `coalesce(item_updated_at, synced_at)` is the instant, the same fallback
/// every mirror-derived read in this codebase uses: interfaces §4.1 lets a
/// source leave `updated_at` unset, and an item with no date could not be put
/// on a day at all.
const AUTHORED: &str = r#"select i.entity_id, i.kind, i.source_id, i.title,
              coalesce(i.item_updated_at, i.synced_at) as at
         from sync.live_item i
        where i.author = any($1)
          and coalesce(i.item_updated_at, i.synced_at) >= $2
          and coalesce(i.item_updated_at, i.synced_at) <  $3
        order by at desc
        limit $4"#;

async fn authored_in_the_mirror(
    pool: &PgPool,
    identity: &[String],
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<Vec<DigestLine>, IpcError> {
    let rows = sqlx::query(AUTHORED)
        .bind(identity)
        .bind(from)
        .bind(to)
        .bind(MOST_LINES)
        .fetch_all(pool)
        .await
        .map_err(IpcError::internal)?;
    rows.into_iter()
        .map(|row| {
            let kind: String = row.try_get("kind").map_err(IpcError::internal)?;
            let source: String = row.try_get("source_id").map_err(IpcError::internal)?;
            Ok(DigestLine {
                entity_id: Some(row.try_get("entity_id").map_err(IpcError::internal)?),
                title: row.try_get("title").map_err(IpcError::internal)?,
                reason: format!("you authored this {kind} in {source}"),
                kind: Some(kind),
                source,
                verb: "authored".to_owned(),
                at: row.try_get("at").map_err(IpcError::internal)?,
            })
        })
        .collect()
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
          and a.detail->>'op' <> 'log_work'
          and a.at >= $2
          and a.at <  $3
        order by a.at desc
        limit $4"#;

async fn written_through_knobas(
    pool: &PgPool,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<Vec<DigestLine>, IpcError> {
    let rows = sqlx::query(WRITTEN)
        .bind(ACTOR)
        .bind(from)
        .bind(to)
        .bind(MOST_LINES)
        .fetch_all(pool)
        .await
        .map_err(IpcError::internal)?;
    rows.into_iter()
        .map(|row| {
            let entity_id: String = row.try_get("entity_id").map_err(IpcError::internal)?;
            let op: String = row.try_get("op").map_err(IpcError::internal)?;
            let title: Option<String> = row.try_get("title").map_err(IpcError::internal)?;
            let source: Option<String> = row.try_get("source_id").map_err(IpcError::internal)?;
            let source = source.unwrap_or_else(|| namespace_of(&entity_id));
            Ok(DigestLine {
                title: title.unwrap_or_else(|| entity_id.clone()),
                reason: format!("you queued a {op} on it in {source}"),
                kind: row.try_get("kind").map_err(IpcError::internal)?,
                entity_id: Some(entity_id),
                source,
                verb: op,
                at: row.try_get("at").map_err(IpcError::internal)?,
            })
        })
        .collect()
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

async fn logged_as_worklogs(
    pool: &PgPool,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<Vec<DigestLine>, IpcError> {
    let rows = sqlx::query(WORKLOGS)
        .bind(from)
        .bind(to)
        .bind(MOST_LINES)
        .fetch_all(pool)
        .await
        .map_err(IpcError::internal)?;
    rows.into_iter()
        .map(|row| {
            let entity_id: String = row.try_get("entity_id").map_err(IpcError::internal)?;
            let title: Option<String> = row.try_get("title").map_err(IpcError::internal)?;
            let source = namespace_of(&entity_id);
            Ok(DigestLine {
                title: title.unwrap_or_else(|| entity_id.clone()),
                reason: format!("you logged work on it in {source}"),
                kind: row.try_get("kind").map_err(IpcError::internal)?,
                entity_id: Some(entity_id),
                source,
                verb: "log_work".to_owned(),
                at: row.try_get("at").map_err(IpcError::internal)?,
            })
        })
        .collect()
}

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
            let row = sqlx::query("select kind, title from knobas.entity where id = $1")
                .bind(entity_id)
                .fetch_optional(pool)
                .await
                .map_err(IpcError::internal)?;
            let (kind, title) = match row {
                Some(row) => (
                    row.try_get::<String, _>("kind").ok(),
                    row.try_get::<String, _>("title").ok(),
                ),
                None => (None, None),
            };
            let source = namespace_of(entity_id);
            DigestLine {
                title: title.unwrap_or_else(|| entity_id.clone()),
                reason: format!("the timer is running on it, in {source}"),
                kind,
                entity_id: Some(entity_id.clone()),
                source,
                verb: TIMER.to_owned(),
                at: timer.started_at,
            }
        }
    };
    Ok(vec![line])
}

/// The `source` a line carries when knobas is the one saying it.
const KNOBAS: &str = "knobas";

/// The verb the running timer's line carries.
const TIMER: &str = "timer";

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

/// The user's items a **link** marks blocked.
///
/// `knobas.confirmed_link`, never `knobas.link`: an unconfirmed row is a
/// suggestion knobas made and nobody accepted, and a standup listing one as a
/// blocker would be reporting a guess as a fact. The same reading
/// `knobas_core::link::entries_of` records.
///
/// The direction is stated once, here: the blocked item is the link's **`to`**
/// end. `A blocks B` and `B is blocked by A` are the same row read from its
/// two ends, so joining `l.to_id` is what makes this list "things that are
/// stuck" rather than "things that are in somebody's way".
macro_rules! blocked_by_link {
    () => {
        concat!(
            "select s.entity_id, s.kind, s.source_id, s.title, s.at,
                    o.id as other_id, o.title as other_title
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

/// Both halves of the blockers list, the declared set first.
///
/// An item can be on both and then it is on the list twice, with a different
/// reason each time -- deliberately. The two are different facts about the
/// same ticket (its source calls it blocked; somebody drew a link saying what
/// is in its way), and collapsing them would throw away one of the two things
/// a reader came for.
async fn blockers(
    pool: &PgPool,
    identity: &[String],
    declarations: &Declarations,
) -> Result<Vec<DigestLine>, IpcError> {
    let mut lines = Vec::new();

    let rows = sqlx::query(BLOCKED_BY_STATUS)
        .bind(identity)
        .bind(declarations.as_param())
        .bind(MOST_LINES)
        .fetch_all(pool)
        .await
        .map_err(IpcError::internal)?;
    for row in rows {
        let source: String = row.try_get("source_id").map_err(IpcError::internal)?;
        let status: String = row.try_get("status").map_err(IpcError::internal)?;
        lines.push(DigestLine {
            entity_id: Some(row.try_get("entity_id").map_err(IpcError::internal)?),
            kind: Some(row.try_get("kind").map_err(IpcError::internal)?),
            title: row.try_get("title").map_err(IpcError::internal)?,
            reason: format!("{source} calls this status blocked-like: {status}"),
            source,
            verb: "blocked_status".to_owned(),
            at: row.try_get("at").map_err(IpcError::internal)?,
        });
    }

    let rows = sqlx::query(BLOCKED_BY_LINK)
        .bind(identity)
        .bind(declarations.as_param())
        .bind(BLOCKS)
        .bind(MOST_LINES)
        .fetch_all(pool)
        .await
        .map_err(IpcError::internal)?;
    for row in rows {
        let other_title: String = row.try_get("other_title").map_err(IpcError::internal)?;
        lines.push(DigestLine {
            entity_id: Some(row.try_get("entity_id").map_err(IpcError::internal)?),
            kind: Some(row.try_get("kind").map_err(IpcError::internal)?),
            title: row.try_get("title").map_err(IpcError::internal)?,
            source: row.try_get("source_id").map_err(IpcError::internal)?,
            verb: "blocked_by".to_owned(),
            reason: format!("a link marks it blocked by {other_title}"),
            at: row.try_get("at").map_err(IpcError::internal)?,
        });
    }

    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

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
                statement.contains("blocked_statuses") || statement.contains("confirmed_link"),
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

    /// The relation is spelled once and the join reads the blocked end.
    #[test]
    fn the_link_half_joins_the_blocked_end_of_a_blocks_link() {
        assert_eq!(BLOCKS, "blocks");
        assert!(
            BLOCKED_BY_LINK.contains("l.to_id = s.entity_id"),
            "the blocked item is the `to` end; joining `from_id` would list \
             what is in somebody else's way"
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

//! The suggestion tray (#774): what the reader is shown of the proposals
//! [`super::detect`] writes, and the answers -- accept, dismiss, a hand-drawn
//! edge that meets a proposal.
//!
//! Its own file, apart from the pass, so the responsiveness probe's
//! after-pass snapshot, keyed on the pass's code, survives an edit here
//! (#776).

use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::CoreError;
use crate::link::{LinkEnd, LinkRow, link_columns};

/// One proposal, as the tray draws it: the row, and **both** its ends.
///
/// Both, unlike [`crate::link::LinkEntry`], which resolves the end the reader is
/// not on. The tray is not read from either end -- it is read from a room -- so
/// there is no "here" for it to leave out, and #41's story 20 wants both ends
/// openable before deciding.
#[derive(Clone, Debug, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct SuggestionEntry {
    pub link: LinkRow,
    pub from: LinkEnd,
    pub to: LinkEnd,
}

/// The row shape [`DRAWN`] returns, before the two ends are lifted out.
///
/// Two [`LinkEnd`]s cannot both be `#[sqlx(flatten)]` -- they would collide on
/// every column name -- so the statement aliases them apart and this struct
/// names the aliases.
#[derive(sqlx::FromRow)]
struct ProposalRow {
    #[sqlx(flatten)]
    link: LinkRow,
    from_entity_id: String,
    from_kind: String,
    from_title: String,
    from_deleted_at: Option<chrono::DateTime<chrono::Utc>>,
    to_entity_id: String,
    to_kind: String,
    to_title: String,
    to_deleted_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// How many rows one item may have in the tray (#774, ruled by the deputy).
pub const ROWS_PER_ITEM: usize = 3;

/// An item with proposals the tray does not draw, and how many (#774).
#[derive(Clone, Debug, Serialize, sqlx::FromRow, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct SuggestionMore {
    #[sqlx(flatten)]
    pub item: LinkEnd,
    /// The item's proposals in the room that the tray did not draw.
    pub more: i64,
}

/// A proposal is in the room: either end is an item of one of the room's
/// sources (`$1`, `NULL` for every source), and either end is a member of the
/// stored context (`$2`, `NULL` for none).
///
/// The source test is a membership in the sources' items, a subquery inside an
/// `or` that cannot be pulled up into a join and so runs hashed, once: joining
/// `sync.item` twice to learn each end's source was a primary-key probe per end
/// per proposal in the generic plan (#778).
macro_rules! in_room {
    () => {
        "($1::text[] is null
          or (l.from_id in (select i.entity_id from sync.item i
                             where i.source_id = any($1))) is true
          or (l.to_id in (select i.entity_id from sync.item i
                           where i.source_id = any($1))) is true)
         and ($2::text[] is null or l.from_id = any($2) or l.to_id = any($2))"
    };
}

/// One step of the tray's walk (#774): the room's next `$6` proposals, newest
/// first, older than the cursor (`$3`, `$4`), leaving out every proposal that
/// touches an item already holding its three rows (`$5`).
///
/// Ends only, no titles: the walk decides which rows are drawn and
/// [`DRAWN`] reads those. `link_proposed_idx` gives the order, and the cursor
/// is an index condition on it, so a step starts where the last one stopped.
const WALK: &str = concat!(
    "select l.id, l.from_id, l.to_id, l.created_at
       from knobas.proposed_link l
      where ",
    in_room!(),
    "   and l.created_at <= $3
        and (l.created_at < $3 or l.id < $4)
        and l.from_id <> all($5) and l.to_id <> all($5)
      order by l.created_at desc, l.id desc
      limit $6"
);

/// [`WALK`] for a stored context's room: every proposal touching a member,
/// newest first, found member by member through the pair indexes rather than
/// by walking the whole room's order, because a context holds a few items out
/// of the whole mirror. On tables never analyzed the planner read each
/// member's rows off `link_proposed_idx` -- the whole table, filtered -- because
/// the lookup's predicate matched that partial index. So the lookup asks only
/// `deleted_at is null` (`link_from_idx`, `link_to_idx`), `offset 0` keeps it
/// its own, and `confirmed_at is null` is tested on the rows it found.
const WALK_MEMBERS: &str = concat!(
    "select l.id, l.from_id, l.to_id, l.created_at
       from unnest($2::text[]) m(item)
       cross join lateral (
             select p.id, p.from_id, p.to_id, p.created_at, p.confirmed_at
               from knobas.link p
              where p.from_id = m.item and p.deleted_at is null
             union all
             select p.id, p.from_id, p.to_id, p.created_at, p.confirmed_at
               from knobas.link p
              where p.to_id = m.item and p.from_id <> all($2)
                and p.deleted_at is null
             offset 0) l
      where l.confirmed_at is null and ",
    in_room!(),
    " order by l.created_at desc, l.id desc"
);

/// The tray's read: the rows the walk drew, newest first, with both ends.
///
/// `knobas.proposed_link`'s predicate and nothing else, which is what makes
/// "the tray shows proposals only" structural rather than remembered. Driven
/// from the ids by primary key, `offset 0` keeping each lookup its own: on
/// tables never analyzed the planner read `knobas.link` whole off
/// `link_proposed_idx` to find twenty rows, 22 ms on a 2,700-item mirror.
///
/// The ends join `knobas.entity` and not `sync.live_item`, for the reason
/// `entries_of` does: an entity a source withdrew still has to resolve, so the
/// tray marks it rather than dropping the row.
const DRAWN: &str = concat!(
    "select ",
    link_columns!("l."),
    ", f.id as from_entity_id, f.kind as from_kind,
       f.title as from_title, f.deleted_at as from_deleted_at,
       t.id as to_entity_id, t.kind as to_kind,
       t.title as to_title, t.deleted_at as to_deleted_at
       from unnest($1::uuid[]) d(id)
       cross join lateral (select * from knobas.link p
                            where p.id = d.id offset 0) l
       join knobas.entity f on f.id = l.from_id
       join knobas.entity t on t.id = l.to_id
      where l.deleted_at is null and l.confirmed_at is null
      order by l.created_at desc, l.id desc"
);

/// How many of the room's proposals each drawn item (`$3`) holds, for those
/// holding more than the tray draws for one item (#774). One count per item
/// through `link_from_idx` and `link_to_idx`, shaped as [`WALK_MEMBERS`] is and
/// for its reason, so a hub costs a probe of its own rows and not a pass over
/// the room. Its `3` is [`ROWS_PER_ITEM`].
const OVERFLOW: &str = concat!(
    "select e.id as entity_id, e.kind, e.title, e.deleted_at, n.total as more
       from unnest($3::text[]) d(item)
       join knobas.entity e on e.id = d.item
       cross join lateral (
             select count(*) as total
               from (select p.from_id, p.to_id, p.confirmed_at from knobas.link p
                      where p.from_id = d.item and p.deleted_at is null
                     union all
                     select p.from_id, p.to_id, p.confirmed_at from knobas.link p
                      where p.to_id = d.item and p.deleted_at is null
                     offset 0) l
              where l.confirmed_at is null and ",
    in_room!(),
    "     ) n
      where n.total > 3"
);

/// One proposal as the walk sees it.
#[derive(sqlx::FromRow)]
struct Step {
    id: Uuid,
    from_id: String,
    to_id: String,
    created_at: chrono::DateTime<chrono::Utc>,
}

/// The walk's state: which rows are drawn, and how many each item holds.
#[derive(Default)]
struct Tray {
    drawn: Vec<Uuid>,
    per_item: std::collections::HashMap<String, usize>,
    full: Vec<String>,
}

impl Tray {
    /// Draw `step` if neither end holds its three rows yet.
    fn offer(&mut self, step: &Step) {
        let held = |item: &str| self.per_item.get(item).copied().unwrap_or(0);
        if held(&step.from_id) >= ROWS_PER_ITEM || held(&step.to_id) >= ROWS_PER_ITEM {
            return;
        }
        self.drawn.push(step.id);
        for item in [&step.from_id, &step.to_id] {
            let held = self.per_item.entry(item.clone()).or_insert(0);
            *held += 1;
            if *held == ROWS_PER_ITEM {
                self.full.push(item.clone());
            }
        }
    }
}

/// The proposals a room holds, newest first, at most three per item.
///
/// `sources` is the room's membership, the same convention
/// `EntityFilter::sources` uses: **empty means every source**, bound as SQL
/// `NULL`, because `= any('{}')` matches nothing and a room whose filter said
/// "no sources in particular" would come back empty. A proposal belongs to a
/// room if *either* end does -- a suggestion connecting this room to another is
/// exactly the one worth surfacing here.
///
/// `members` is the second scope, and the one #47 promised this signature: a
/// stored context's room passes its membership (ADR-0008's rule, computed by
/// `crate::context::member_ids` plus the context's own entity), and only
/// proposals touching it come back. `None` is unscoped -- the derived rooms --
/// while `Some(&[])` is a context with no members, whose tray is honestly
/// empty. Membership is computed from **confirmed** links only, so scoping
/// proposals by it cannot become circular.
///
/// **At most three rows per item (#774).** The room's proposals are walked
/// newest first and a row is drawn while neither of its items holds three
/// drawn rows, until `limit` rows are drawn or the room is exhausted. Under
/// "either item's top ten" one item can carry thousands of proposals; the walk
/// leaves an item out of its next steps once it holds three, so a hub costs
/// three rows and not a scan of its own. A ranking by window function over the
/// room was the first attempt and cost seconds on the probe's 152k proposals.
///
/// The tray holds no state: this is the whole of it.
///
/// # Errors
///
/// [`CoreError::Db`] if a query fails.
pub async fn proposals(
    pool: &PgPool,
    sources: &[String],
    members: Option<&[String]>,
    limit: i64,
) -> Result<Vec<SuggestionEntry>, CoreError> {
    let Ok(limit) = usize::try_from(limit) else {
        return Ok(Vec::new());
    };
    if limit == 0 {
        return Ok(Vec::new());
    }
    let scope = (!sources.is_empty()).then(|| sources.to_vec());
    let members = members.map(<[String]>::to_vec);
    let mut tray = Tray::default();
    if members.is_some() {
        let steps = sqlx::query_as::<_, Step>(WALK_MEMBERS)
            .bind(&scope)
            .bind(&members)
            .fetch_all(pool)
            .await?;
        for step in &steps {
            if tray.drawn.len() == limit {
                break;
            }
            tray.offer(step);
        }
    } else {
        let mut before = (chrono::DateTime::<chrono::Utc>::MAX_UTC, Uuid::max());
        let mut chunk = limit.saturating_mul(4).max(64);
        loop {
            let steps = sqlx::query_as::<_, Step>(WALK)
                .bind(&scope)
                .bind(&members)
                .bind(before.0)
                .bind(before.1)
                .bind(&tray.full)
                .bind(i64::try_from(chunk).unwrap_or(i64::MAX))
                .fetch_all(pool)
                .await?;
            for step in &steps {
                if tray.drawn.len() == limit {
                    break;
                }
                tray.offer(step);
            }
            let Some(last) = steps.last() else { break };
            if tray.drawn.len() == limit || steps.len() < chunk {
                break;
            }
            before = (last.created_at, last.id);
            chunk = chunk.saturating_mul(4).min(16_384);
        }
    }
    let rows = sqlx::query_as::<_, ProposalRow>(DRAWN)
        .bind(&tray.drawn)
        .fetch_all(pool)
        .await?;
    Ok(rows
        .into_iter()
        .map(|row| SuggestionEntry {
            link: row.link,
            from: LinkEnd {
                entity_id: row.from_entity_id,
                kind: row.from_kind,
                title: row.from_title,
                deleted_at: row.from_deleted_at,
            },
            to: LinkEnd {
                entity_id: row.to_entity_id,
                kind: row.to_kind,
                title: row.to_title,
                deleted_at: row.to_deleted_at,
            },
        })
        .collect())
}

/// The items among `drawn` that hold more of the room's proposals than the
/// tray draws for one item, and how many of those the tray did not draw, most
/// hidden first (#774).
///
/// Same scope as [`proposals`]. Only the drawn rows' items are counted, one
/// index lookup each. The tray draws one "+N more similar to <title>" line for
/// each.
///
/// # Errors
///
/// [`CoreError::Db`] if the query fails.
pub async fn proposal_overflow(
    pool: &PgPool,
    sources: &[String],
    members: Option<&[String]>,
    drawn: &[SuggestionEntry],
) -> Result<Vec<SuggestionMore>, CoreError> {
    let mut shown = std::collections::HashMap::<&str, i64>::new();
    for entry in drawn {
        for item in [&entry.from.entity_id, &entry.to.entity_id] {
            *shown.entry(item.as_str()).or_insert(0) += 1;
        }
    }
    if shown.is_empty() {
        return Ok(Vec::new());
    }
    let scope = (!sources.is_empty()).then(|| sources.to_vec());
    let items: Vec<&str> = shown.keys().copied().collect();
    let mut more = sqlx::query_as::<_, SuggestionMore>(OVERFLOW)
        .bind(scope)
        .bind(members.map(<[String]>::to_vec))
        .bind(&items)
        .fetch_all(pool)
        .await?;
    for hub in &mut more {
        hub.more -= shown.get(hub.item.entity_id.as_str()).copied().unwrap_or(0);
    }
    more.sort_by(|a, b| {
        b.more
            .cmp(&a.more)
            .then_with(|| a.item.entity_id.cmp(&b.item.entity_id))
    });
    Ok(more)
}

/// [`proposal_count`]'s statement.
///
/// **No join** (#778). It joined `sync.item` twice to learn each end's source,
/// and that join was its whole cost: analyzed, a hash join, 15 ms for the
/// probe's 25,690 proposals; never analyzed, and in the generic plan
/// PostgreSQL switched this persistent statement to at its sixth execution, a
/// primary-key probe per end per proposal, 220 ms. That switch was the
/// 220 ms maximum the probe read beside a 15 ms median in a project room.
/// Now an end is in the room by membership in the room's sources' items, a
/// subquery inside an `or` that cannot be pulled up into a join and so runs
/// hashed, once: 6 ms never analyzed, 9 ms analyzed, either plan.
///
/// Its predicate is [`in_room!`], the one [`WALK`] and [`WALK_MEMBERS`] ask.
const PROPOSAL_COUNT: &str = concat!(
    "select count(*)
       from knobas.proposed_link l
      where ",
    in_room!()
);

/// [`PROPOSAL_COUNT`], for `knobas-app`'s plan test (`tests/it/read_plans.rs`,
/// #778). It binds what [`proposal_count`] binds.
#[doc(hidden)]
#[must_use]
pub fn planned_proposal_count() -> &'static str {
    PROPOSAL_COUNT
}

/// The tray's lookups (#774) -- [`WALK_MEMBERS`] (binds as [`proposal_count`]),
/// [`DRAWN`] (`$1` the drawn ids) and [`OVERFLOW`] (as [`proposal_count`], then
/// `$3` the drawn items) -- for `knobas-app`'s plan test
/// (`tests/it/read_plans.rs`).
#[doc(hidden)]
#[must_use]
pub fn planned_tray_lookups() -> [&'static str; 3] {
    [WALK_MEMBERS, DRAWN, OVERFLOW]
}

/// How many proposals a room is holding.
///
/// Its own statement rather than `proposals(..).len()`: the tray shows a count
/// before it shows rows (#41 story 19), and a count that had to fetch every row
/// to be produced would be capped by whatever `limit` the caller happened to
/// pass.
///
/// # Errors
///
/// [`CoreError::Db`] if the query fails.
pub async fn proposal_count(
    pool: &PgPool,
    sources: &[String],
    members: Option<&[String]>,
) -> Result<i64, CoreError> {
    let scope = (!sources.is_empty()).then(|| sources.to_vec());
    let (count,): (i64,) = sqlx::query_as(PROPOSAL_COUNT)
        .bind(scope)
        .bind(members.map(<[String]>::to_vec))
        .fetch_one(pool)
        .await?;
    Ok(count)
}

/// Accept a proposal: it becomes an ordinary link.
///
/// The row does not move and nothing about it is rewritten but `confirmed_at`,
/// which is what "there are not two kinds of link to reason about afterwards"
/// (#41 story 5) means in practice -- the same id, the same ends, the same
/// relation, and from the panel's side the same query that returns every other
/// link. Its `rule` and `reason` stay: they are provenance, and a link that can
/// still say why knobas thought so is strictly more useful than one that
/// cannot.
///
/// `Some(row)` is "this call accepted that proposal"; `None` is "there was
/// nothing left to accept" -- it was already accepted, or it was dismissed. The
/// distinction is the caller's, which writes one activity line per *mutation*.
///
/// # Errors
///
/// [`CoreError::LinkNotFound`] if no link carries `id` at all; [`CoreError::Db`]
/// if the statement fails.
pub async fn accept(pool: &PgPool, id: Uuid) -> Result<Option<LinkRow>, CoreError> {
    let updated = sqlx::query_as::<_, LinkRow>(concat!(
        "update knobas.link set confirmed_at = now()
          where id = $1 and deleted_at is null and confirmed_at is null
         returning ",
        link_columns!("")
    ))
    .bind(id)
    .fetch_optional(pool)
    .await?;
    settle(pool, id, updated).await
}

/// What [`resolve_edge`] found standing in the way of a hand-drawn link.
#[derive(Debug)]
pub enum Edge {
    /// A proposal for *exactly* this triple was confirmed. It is the link now,
    /// and the caller writes nothing.
    Promoted(LinkRow),
    /// A proposal for the **reversed** triple was withdrawn to make room. The
    /// caller writes the link the user actually drew, in the same transaction.
    Superseded(LinkRow),
    /// Nothing to answer: no live proposal for this pair and relation, either
    /// way round. Whatever refuses the write now is a real link.
    Open,
}

/// Answer the live proposal for a pair and relation, if there is one, so that a
/// hand-drawn link can be written where one stands.
///
/// The unique index spans proposals and links alike -- one active edge per pair
/// per relation, whatever its state -- so without this a user drawing a link
/// knobas had already proposed is told "already linked" about a pair whose links
/// panel is empty.
///
/// ## Two answers, because the two cases are different acts
///
/// **Same direction: the user is pressing *Accept*.** Drawing by hand the link
/// knobas proposed is that gesture spelled another way, so the proposal is
/// confirmed and keeps its detector provenance -- `rule`, `rule_class` and the
/// `reason` it was proposed with all still describe it truthfully.
///
/// **Reversed: the user is contradicting the proposal's direction**, and the
/// user wins. The proposal is *withdrawn* rather than confirmed, because
/// confirming it would store the opposite of what the user drew (`A blocks B`
/// landing as `B blocks A`, which story 7's inverse labels would then faithfully
/// render back at them as "A is blocked by B"), and reorienting it in place
/// would leave a row whose stored `reason` describes ends it no longer has.
/// Withdrawal is a tombstone -- the same mechanism as [`dismiss`], and therefore
/// the same fact to detection's undirected suppression, so the proposal is not
/// proposed straight back.
///
/// Before #70 the promotion was direction-exact while the index was directed,
/// which cancelled out into a third outcome nobody chose: the reversed
/// hand-drawn link *succeeded*, and left a stale proposal sitting in the tray
/// beside a confirmed link for the same pair.
///
/// ## Why it takes an executor
///
/// [`Edge::Superseded`] is only half a mutation -- the caller's write is the
/// other half -- so the two have to be able to share one transaction. A
/// tombstone committed without the link that replaced it would have thrown away
/// a proposal for nothing.
///
/// # Errors
///
/// [`CoreError::Db`] if either statement fails.
pub async fn resolve_edge(
    conn: &mut sqlx::PgConnection,
    from: &crate::entity::EntityRef,
    to: &crate::entity::EntityRef,
    relation: &str,
) -> Result<Edge, CoreError> {
    let promoted = sqlx::query_as::<_, LinkRow>(concat!(
        "update knobas.link set confirmed_at = now()
          where from_id = $1 and to_id = $2 and relation = $3
            and deleted_at is null and confirmed_at is null
         returning ",
        link_columns!("")
    ))
    .bind(from.to_string())
    .bind(to.to_string())
    .bind(relation)
    .fetch_optional(&mut *conn)
    .await?;
    if let Some(row) = promoted {
        return Ok(Edge::Promoted(row));
    }

    // The ends swapped, and nothing else: a proposal is what this withdraws, so
    // `confirmed_at is null` is the clause that keeps a *link* out of reach. A
    // confirmed link in the way is the caller's conflict to report.
    let superseded = sqlx::query_as::<_, LinkRow>(concat!(
        "update knobas.link set deleted_at = now()
          where from_id = $2 and to_id = $1 and relation = $3
            and deleted_at is null and confirmed_at is null
         returning ",
        link_columns!("")
    ))
    .bind(from.to_string())
    .bind(to.to_string())
    .bind(relation)
    .fetch_optional(conn)
    .await?;
    Ok(match superseded {
        Some(row) => Edge::Superseded(row),
        None => Edge::Open,
    })
}

/// Dismiss a proposal by tombstoning it -- the row stays, `deleted_at` is set.
///
/// **The same mechanism as [`crate::link::unlink`], deliberately.** A dismissed
/// suggestion and an unlinked link are one fact to the detector, and the
/// tombstone is that fact: [`detect`](super::detect)'s suppression reads the table with no
/// filter, so neither can come back. Building a second store for dismissals
/// would be a second thing to keep in step with the first.
///
/// `Some(row)` is "this call dismissed that proposal"; `None` is "there was
/// nothing left to dismiss" -- already dismissed, or already accepted, in which
/// case it is a link and the panel's *Unlink* is what withdraws it. A confirmed
/// link is therefore never tombstoned through this door.
///
/// # Errors
///
/// [`CoreError::LinkNotFound`] if no link carries `id` at all; [`CoreError::Db`]
/// if the statement fails.
pub async fn dismiss(pool: &PgPool, id: Uuid) -> Result<Option<LinkRow>, CoreError> {
    let updated = sqlx::query_as::<_, LinkRow>(concat!(
        "update knobas.link set deleted_at = now()
          where id = $1 and deleted_at is null and confirmed_at is null
         returning ",
        link_columns!("")
    ))
    .bind(id)
    .fetch_optional(pool)
    .await?;
    settle(pool, id, updated).await
}

/// Tell "nothing to do" from "no such link", the way `unlink` does.
///
/// An update that matched nothing is either a proposal that has already been
/// answered -- fine, and idempotent -- or an id nothing carries, which the
/// caller wants to hear about rather than silently succeed on.
async fn settle(
    pool: &PgPool,
    id: Uuid,
    updated: Option<LinkRow>,
) -> Result<Option<LinkRow>, CoreError> {
    if updated.is_some() {
        return Ok(updated);
    }
    let existing: Option<(Uuid,)> = sqlx::query_as("select id from knobas.link where id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    existing.map(|_| None).ok_or(CoreError::LinkNotFound(id))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whitespace folded to single spaces: the statements are compared as SQL,
    /// not as indentation.
    fn folded(sql: &str) -> String {
        sql.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// The count is the walk's room, so its predicate is `in_room!` itself and
    /// not a second spelling of it that the next change to the room misses.
    #[test]
    fn the_count_asks_the_room_predicate_and_nothing_else() {
        let (head, predicate) = PROPOSAL_COUNT
            .split_once(" where ")
            .expect("PROPOSAL_COUNT has a where clause");
        assert_eq!(folded(head), "select count(*) from knobas.proposed_link l");
        assert_eq!(predicate, in_room!());
    }

    /// The statement #778 measured, as it read before the predicate was shared:
    /// sharing it changed no SQL.
    #[test]
    fn the_count_is_the_statement_the_plan_was_measured_on() {
        assert_eq!(
            folded(PROPOSAL_COUNT),
            "select count(*) from knobas.proposed_link l \
             where ($1::text[] is null \
             or (l.from_id in (select i.entity_id from sync.item i where i.source_id = any($1))) is true \
             or (l.to_id in (select i.entity_id from sync.item i where i.source_id = any($1))) is true) \
             and ($2::text[] is null or l.from_id = any($2) or l.to_id = any($2))"
        );
    }
}

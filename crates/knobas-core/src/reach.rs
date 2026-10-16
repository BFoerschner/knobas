//! What a ticket **reaches**: the read behind every *This job* (#817, spec
//! #808, requirements R-2.8, R-2.9 and R-7.3).
//!
//! Two reads exist where there was one (ADR-0025). [`crate::context::member_ids`]
//! is ADR-0008's membership, and the proposal tray, the per-context inbox and
//! the context export keep reading it and nothing else. This module is the
//! second one: what a job shows. It is computed when read and never stored.
//!
//! ## The walk
//!
//! ADR-0025: a job shows its context's members, ADR-0008's rule unchanged,
//! then the members of every link set whose rule matches the ticket, then the
//! parent ticket's own links. So the walk is membership's, with each row's
//! path kept for the reason label:
//!
//! * **the start** -- the ticket itself, and when it anchors an unarchived
//!   context, that context's explicit adds and, for an epic, its children (the
//!   same `recorded_parent_key!` read membership makes);
//! * **one link out** from each of those, then **one more**, over
//!   `knobas.confirmed_link` (never proposals, for ADR-0008's second reason)
//!   and never through a context -- the ticket's pull request, then its builds;
//! * **what is held** under anything reached, at any depth, over
//!   `knobas.asset.parent_id`.
//!
//! Then it stops, as membership does. Spec #808 and requirement R-2.8 ask for
//! more (R-4.17's "from a VM, proxy or host the walk only goes down", and the
//! VM under a container with its monitors); ADR-0025 fixes the job's read as
//! membership, so that difference is a follow-up against the ADR, not a fork
//! this read takes. `the_reach_is_membership_plus_the_parents_links` pins the
//! two together.
//!
//! ## The parent
//!
//! Behind one global switch, on by default ([`PARENT_LINKS_SETTING`], R-7.3),
//! a sub-task also reaches its parent ticket's **own** links, one layer and no
//! further. The parent is the declared `fields.parent.key`, in the ticket's own
//! source -- the first arm of `recorded_parent_key!`; the classic `epic.key`
//! arm names an epic, which is not "the parent" here. The parent counts only
//! when it is mirrored and its issue type reads as something other than an
//! epic, by the path the promote path's anchor check reads: an epic's links
//! reach its tickets through link sets (TF5), not through this switch. Both are
//! payload reads outside an adapter (ADR-0007), confined to [`reach_walk!`],
//! and both miss toward absence: a parent key or an issue type the path does
//! not find contributes no parent, so the failure is an absent row, never a
//! wrong one. `a_parent_whose_type_cannot_be_read_brings_nothing` pins the
//! second. The two paths are spelled here rather than through
//! `recorded_parent_key!` because that macro's second arm (`epic.key`) names
//! an epic, which this read must not take.
//!
//! ## The link sets
//!
//! Since #866 a ticket also reaches the fixed members of every **link set**
//! whose rule matches it, [`crate::link_sets_applying!`]: by hand, as a direct
//! child of the rule's ticket (`parent`), anywhere under it (`epic`), or in its
//! project, over the stored parent and project resolutions (ADR-0018,
//! ADR-0019, ADR-0010). Each member is its own first step, one layer and no
//! further: the walk never takes a hop from it and never descends what it
//! holds, because a set is not a seed (ADR-0025). A member hidden on this
//! ticket (`knobas.link_set_hidden`, R-7.5) is left out; hiding reaches only
//! a set's rows, never the ticket's own links or its parent's.
//!
//! ## What each row says
//!
//! Each reached entity comes back once, with the **first step** of the path
//! that brought it in -- what a row's reason label names ("via PR #142") --
//! and the [`Origin`] of that path. An entity reached both ways keeps its own
//! path: the job's own links come first (R-7.4, "duplicates show once, with
//! the first origin"), and among its own paths the shortest wins. Then a link
//! set's row, and of two sets the one whose id sorts first, then the parent's.

use serde::Serialize;
use sqlx::PgPool;

use crate::CoreError;

/// The `knobas.setting` key of R-7.3's switch, *Sub-tasks show their parent
/// ticket's connections*.
///
/// **Unset is on**, and only the stored value [`PARENT_LINKS_OFF`] turns it
/// off: a fresh database has no row, and the requirement says on by default.
pub const PARENT_LINKS_SETTING: &str = "job.parent_links";

/// The one value of [`PARENT_LINKS_SETTING`] that switches the parent off.
pub const PARENT_LINKS_OFF: &str = "off";

/// The walk after its start, as SQL text.
///
/// It expects a CTE `start(t)` before it, one row per ticket id to walk from,
/// and defines the rest of a `with recursive` statement, ending in the CTE
/// `reached(t, id, step, origin, rank, via)` with no trailing comma, so a
/// caller appends its own `select`. Every row of `reached` is one path: `t`
/// the ticket walked from, `id` what it reached, `step` the first step of the
/// path, `origin` `own`, `link_set` or `parent`, `rank` lower for the path to
/// keep, `via` the set's id (the parent's id for `parent`, null for `own`).
///
/// Two callers: [`reach`] here, and `knobas-search`'s `reach:` key, which is
/// the same walk as a filter -- one text, so the read and the filter cannot
/// come to different answers. A macro because both build it into a statement
/// at compile time or by `format!`, as [`crate::context_expansion!`] is.
///
/// The link layers and the `held` recursion are spelled as
/// [`crate::context_expansion!`] spells them, with the path's first step
/// carried along; `the_reach_is_membership_plus_the_parents_links` holds the
/// two to the same answer.
#[doc(hidden)]
#[macro_export]
macro_rules! reach_walk {
    () => {
        concat!(
            "anchored(t, c, epic) as (
        select s.t, c.id, c.kind = 'epic'
          from start s
          join knobas.context c on c.anchor_id = s.t and c.archived_at is null
    ),
    l0(t, id, step) as (
        select s.t, s.t, null::text from start s
        union
        select a.t, o.id, o.id
          from anchored a
          join knobas.confirmed_link l on l.from_id = a.c or l.to_id = a.c
          join knobas.entity o
            on o.id = case when l.from_id = a.c then l.to_id else l.from_id end
         where o.kind <> 'ctx' and o.id <> a.t
        union
        select a.t, i.entity_id, i.entity_id
          from anchored a
          join sync.live_item i
            on a.epic
           and i.source_id = split_part(a.t, ':', 1)
           and ",
            $crate::recorded_parent_key!(),
            " = substr(a.t, strpos(a.t, ':') + 1)
    ),
    l1(t, id, step, d) as (
        select t, id, step, 0 from l0
        union
        select w.t, o.id, coalesce(w.step, o.id), 1
          from l0 w
          join knobas.confirmed_link l on l.from_id = w.id or l.to_id = w.id
          join knobas.entity o
            on o.id = case when l.from_id = w.id then l.to_id else l.from_id end
         where o.kind <> 'ctx'
    ),
    l2(t, id, step, d) as (
        select t, id, step, d from l1
        union
        select w.t, o.id, coalesce(w.step, o.id), 2
          from l1 w
          join knobas.confirmed_link l on l.from_id = w.id or l.to_id = w.id
          join knobas.entity o
            on o.id = case when l.from_id = w.id then l.to_id else l.from_id end
         where o.kind <> 'ctx'
    ),
    held(t, id, step, d) as (
        select t, id, step, d from l2
        union
        select h.t, c.id, h.step, h.d + 1
          from held h
          join knobas.asset c on c.parent_id = h.id
    ),
    parent(t, p) as (
        select s.t, pe.id
          from start s
          join sync.live_item ci on ci.entity_id = s.t
          join knobas.entity pe
            on pe.id = ci.source_id || ':' || (ci.payload->'fields'->'parent'->>'key')
          join sync.live_item pi on pi.entity_id = pe.id
         where not exists (select 1 from knobas.setting st
                            where st.key = 'job.parent_links' and st.value = to_jsonb('off'::text))
           and coalesce(pi.payload->'fields'->'issuetype'->>'name' not ilike 'epic', false)
           and pe.id <> s.t
    ),
    ",
            $crate::link_sets_applying!(),
            ",
    reached(t, id, step, origin, rank, via) as (
        select t, id, step, 'own', d, null::text from held where id <> t and step is not null
        union all
        select a.t, m.entity_id, m.entity_id, 'link_set', 500, a.set_id
          from set_applies a
          join knobas.link_set_member m on m.set_id = a.set_id and m.entity_id is not null
         where m.entity_id <> a.t
           and not exists (select 1 from knobas.link_set_hidden h
                            where h.ticket_id = a.t and h.member_id = m.entity_id)
        union all
        select p.t, o.id, p.p, 'parent', 1000, p.p
          from parent p
          join knobas.confirmed_link l on l.from_id = p.p or l.to_id = p.p
          join knobas.entity o
            on o.id = case when l.from_id = p.p then l.to_id else l.from_id end
         where o.kind <> 'ctx' and o.id <> p.t
    )"
        )
    };
}

/// Where a reached entity's path came from.
///
/// Tagged on `kind`; #866's link sets joined as the third arm without
/// changing the two before it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Origin {
    /// The ticket's own links, its context's adds, and where those lead.
    Own,
    /// The parent ticket's own links (R-7.3), labelled "Parent PAY-231".
    Parent {
        /// The parent ticket's entity id.
        ticket: String,
    },
    /// A member of a link set whose rule matches the ticket (#866,
    /// ADR-0025), labelled with the set's name.
    LinkSet {
        /// The set's id, its address: `#/sets/<set>`.
        set: String,
        /// The set's name, as the reader gave it.
        name: String,
    },
}

/// One entity a ticket reaches.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Reached {
    pub entity_id: String,
    /// What `entity_id` is: `pr`, `build`, `asset`, `monitor`.
    pub kind: String,
    /// The first step of the path that brought it in: an entity id. Equal to
    /// `entity_id` when the entity is itself the first step -- linked to the
    /// ticket directly, or one of the parent's own links.
    pub step: String,
    pub origin: Origin,
}

/// The whole read: [`reach_walk!`] from one ticket, one row per entity.
const REACH: &str = concat!(
    "
    with recursive start(t) as (select $1::text),
    ",
    crate::reach_walk!(),
    "
    select id, kind, step, origin, via, set_name from (
        select distinct on (r.id) r.id, e.kind, r.step, r.origin, r.via, ls.name as set_name
          from reached r
          join knobas.entity e on e.id = r.id
          left join knobas.link_set ls on r.origin = 'link_set' and ls.id = r.via
         order by r.id, r.rank, r.via, r.step
    ) kept
    order by id"
);

/// What `ticket_id` reaches, each entity once with the first step of its path.
///
/// An id nothing answers to reaches nothing rather than failing, as
/// [`crate::context::member_ids`] answers for an unknown context: every caller
/// is a reader scoping a view. Withdrawn entities are not filtered; each view
/// reads its rows through `sync.live_item` as every mirror read does.
///
/// # Errors
///
/// [`CoreError::Db`] if the query fails.
pub async fn reach(pool: &PgPool, ticket_id: &str) -> Result<Vec<Reached>, CoreError> {
    type Row = (String, String, String, String, Option<String>, Option<String>);
    let rows: Vec<Row> = sqlx::query_as(REACH)
        .bind(ticket_id)
        .fetch_all(pool)
        .await?;
    Ok(rows
        .into_iter()
        .map(|(entity_id, kind, step, origin, via, set_name)| Reached {
            origin: match origin.as_str() {
                "parent" => Origin::Parent {
                    ticket: step.clone(),
                },
                "link_set" => Origin::LinkSet {
                    set: via.unwrap_or_default(),
                    name: set_name.unwrap_or_default(),
                },
                _ => Origin::Own,
            },
            entity_id,
            kind,
            step,
        })
        .collect())
}

/// Whether R-7.3's switch is on: unless [`PARENT_LINKS_OFF`] is stored.
///
/// # Errors
///
/// [`CoreError::Db`] if the query fails.
pub async fn parent_links(pool: &PgPool) -> Result<bool, CoreError> {
    let off: bool = sqlx::query_scalar(
        "select exists (select 1 from knobas.setting
                         where key = $1 and value = to_jsonb($2::text))",
    )
    .bind(PARENT_LINKS_SETTING)
    .bind(PARENT_LINKS_OFF)
    .fetch_one(pool)
    .await?;
    Ok(!off)
}

/// Switch R-7.3's switch, and answer what is now stored.
///
/// On is stored as no row at all, which is what a fresh database has, so there
/// is one spelling of *on*.
///
/// # Errors
///
/// [`CoreError::Db`] if the write fails.
pub async fn set_parent_links(pool: &PgPool, on: bool) -> Result<bool, CoreError> {
    if on {
        sqlx::query("delete from knobas.setting where key = $1")
            .bind(PARENT_LINKS_SETTING)
            .execute(pool)
            .await?;
    } else {
        sqlx::query(
            "insert into knobas.setting (key, value) values ($1, to_jsonb($2::text))
             on conflict (key) do update set value = excluded.value, updated_at = now()",
        )
        .bind(PARENT_LINKS_SETTING)
        .bind(PARENT_LINKS_OFF)
        .execute(pool)
        .await?;
    }
    parent_links(pool).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_walk_reads_the_setting_by_its_constants() {
        let walk = crate::reach_walk!();
        assert!(walk.contains(&format!("st.key = '{PARENT_LINKS_SETTING}'")));
        assert!(walk.contains(&format!("to_jsonb('{PARENT_LINKS_OFF}'::text)")));
    }

    #[test]
    fn an_origin_serialises_tagged_on_kind() {
        assert_eq!(
            serde_json::to_value(Origin::Own).unwrap(),
            serde_json::json!({ "kind": "own" })
        );
        assert_eq!(
            serde_json::to_value(Origin::Parent {
                ticket: "jira:PAY-231".to_owned()
            })
            .unwrap(),
            serde_json::json!({ "kind": "parent", "ticket": "jira:PAY-231" })
        );
    }
}

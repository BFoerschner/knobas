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
//! ADR-0025, as amended by Björn's ruling of 2026-10-18 on #942: what a job
//! shows is walked per R-4.17 and R-4.18. It starts where membership starts
//! and takes membership's layers, with each row's path kept for the reason
//! label:
//!
//! * **the start** -- the ticket itself, and when it anchors a context,
//!   archived or not, that context's explicit adds and, for an epic, its
//!   children (the same `recorded_parent_key!` read membership makes). A done
//!   job is archived (#1501) and opening it is reading it: it shows what it
//!   held, the way `member_ids` answers about a context whatever its state;
//!   only the inbox's alert rule and the open jobs skip archived contexts;
//! * **one link out** from each of those, then **one more**, over
//!   `knobas.confirmed_link` (never proposals, for ADR-0008's second reason)
//!   and never through a context -- the ticket's pull request, then its builds;
//! * **what is held** under anything reached, over `knobas.asset.parent_id`;
//! * then, beyond membership, **what it runs on**: every ancestor of a reached
//!   asset (a container's VM, the VM's host), and every monitor attached to a
//!   reached asset by a `monitored-by` link. R-2.8 asks for both: "every
//!   monitor watching an asset the ticket reaches, the VM under a container
//!   included".
//!
//! **From a VM, a proxy or a host the walk only goes further down** (R-4.17).
//! Things link *to* a VM because they run on it, so a link walk out of one
//! would climb back up into every other service on the machine, and from there
//! into every ticket that touches them. So from an asset whose type is in
//! [`DOWNWARD_ONLY`], a link step is taken only toward what it stands on (its
//! own `runs-on` and `depends-on`, read from the `from` end) and toward its
//! monitors, and nothing held under it is brought in.
//!
//! Membership itself is unchanged and keeps its consumers;
//! `membership_stays_adr_0008s_and_the_context_seeds_the_reach` pins where the
//! two now part.
//!
//! ## The parent
//!
//! Behind one global switch, on by default ([`PARENT_LINKS_SETTING`], R-7.3),
//! a sub-task also reaches its parent ticket's **own** links, one layer and no
//! further. The parent is the declared `fields.parent.key`, in the ticket's own
//! source -- the first arm of `recorded_parent_key!`; the classic `epic.key`
//! arm names an epic, which is not "the parent" here. The parent counts only
//! when it is mirrored and its declared `issue_type` reads as something other
//! than an epic, the test the promote path's anchor check makes: an epic's
//! links reach its tickets through link sets (TF5), not through this switch.
//!
//! The issue type goes through the declaration (#2364). The walk binds none,
//! so it reads the database's stored copy: the parent's source's adapter's
//! `sync.parent_declaration` row (ADR-0019), which bring-up and every search
//! command write, as `knobas_core::project::list` reads it.
//! `a_parents_type_is_read_where_its_kind_declares_it` pins the path.
//!
//! The parent **key** stays a payload read outside an adapter (ADR-0007),
//! confined to [`reach_walk!`]: a kind's declared parent names are what a
//! parent *answers to* (a title, a configuration's name), not the entity id
//! this join needs, and the declaration cannot say which candidate is the
//! parent and which the epic. Both reads miss toward absence: a parent key or
//! an issue type the walk does not find contributes no parent, so the failure
//! is an absent row, never a wrong one.
//! `a_parent_whose_type_cannot_be_read_brings_nothing` pins the second. The
//! key's path is spelled here rather than through `recorded_parent_key!`
//! because that macro's second arm (`epic.key`) names an epic, which this read
//! must not take.
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
//! Since #868 a set's **patterns** join the same arm: every live item a
//! pattern member matches when the read runs ([`crate::link_set_pattern_hits!`]),
//! so a pipeline that appears in the project tomorrow is on the job tomorrow
//! without anybody editing the set. Same layer, same reason, same hiding.
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

/// The asset types the walk only goes further down from (R-4.17): a VM, a
/// reverse proxy, a host, and the two kinds of thing below a host.
///
/// Ids from [`crate::asset`]'s type table, which are wire values that never
/// change. Pinned against it by `every_downward_type_is_a_built_in_type`.
pub const DOWNWARD_ONLY: [&str; 5] = ["vm", "reverse_proxy", "hypervisor", "site", "network"];

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
/// The link layers and the `held` recursion start as
/// [`crate::context_expansion!`] spells them, with the path's first step
/// carried along and the downward rule added; `under` then climbs to every
/// ancestor and `watched` adds every `monitored-by` monitor. The asset types
/// in the downward rule are a literal list, which
/// `the_walk_names_every_downward_type` pins to [`DOWNWARD_ONLY`]; the
/// relations are pinned to [`crate::link`]'s constants the same way.
#[doc(hidden)]
#[macro_export]
macro_rules! reach_walk {
    () => {
        concat!(
            "anchored(t, c, epic) as (
        select s.t, c.id, c.kind = 'epic'
          from start s
          join knobas.context c on c.anchor_id = s.t
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
           and (l.relation = 'monitored-by'
                or (l.from_id = w.id and l.relation in ('runs-on', 'depends-on'))
                or not exists (select 1 from knobas.asset x
                                where x.id = w.id
                                  and x.type_id in ('vm', 'reverse_proxy', 'hypervisor', 'site', 'network')))
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
           and (l.relation = 'monitored-by'
                or (l.from_id = w.id and l.relation in ('runs-on', 'depends-on'))
                or not exists (select 1 from knobas.asset x
                                where x.id = w.id
                                  and x.type_id in ('vm', 'reverse_proxy', 'hypervisor', 'site', 'network')))
    ),
    held(t, id, step, d) as (
        select t, id, step, d from l2
        union
        select h.t, c.id, h.step, h.d + 1
          from held h
          join knobas.asset p on p.id = h.id
          join knobas.asset c on c.parent_id = h.id
         where p.type_id not in ('vm', 'reverse_proxy', 'hypervisor', 'site', 'network')
    ),
    under(t, id, step, d) as (
        select t, id, step, d from held
        union
        select w.t, a.parent_id, w.step, w.d + 1
          from under w
          join knobas.asset a on a.id = w.id
         where a.parent_id is not null
    ),
    watched(t, id, step, d) as (
        select t, id, step, d from under
        union
        select w.t, o.id, w.step, w.d + 1
          from under w
          join knobas.asset a on a.id = w.id
          join knobas.confirmed_link l
            on (l.from_id = w.id or l.to_id = w.id) and l.relation = 'monitored-by'
          join knobas.entity o
            on o.id = case when l.from_id = w.id then l.to_id else l.from_id end
         where o.kind <> 'ctx'
    ),
    parent(t, p) as (
        select s.t, pe.id
          from start s
          join sync.live_item ci on ci.entity_id = s.t
          join knobas.entity pe
            on pe.id = ci.source_id || ':' || (ci.payload->'fields'->'parent'->>'key')
          join sync.live_item i on i.entity_id = pe.id
         cross join lateral (
           select jsonb_build_object(i.source_id, jsonb_build_object(i.kind, kp.p)) as decl
             from knobas.source_config sc
             join sync.parent_declaration pd on pd.adapter_kind = sc.kind
            cross join lateral jsonb_array_elements(
              case when jsonb_typeof(pd.payload_paths) = 'array'
                   then pd.payload_paths else '[]'::jsonb end) as kp(p)
            where sc.id = i.source_id and kp.p->>'kind' = i.kind
            limit 1) dd
         where not exists (select 1 from knobas.setting st
                            where st.key = 'job.parent_links' and st.value = to_jsonb('off'::text))
           and coalesce(",
            $crate::declared_string!("dd.decl", "issue_type"),
            " not ilike 'epic', false)
           and pe.id <> s.t
    ),
    ",
            $crate::link_sets_applying!(),
            ",
    set_patterns(set_id, position, kind, pattern) as (
        select m.set_id, m.position, m.kind, m.pattern
          from knobas.link_set_member m
         where m.pattern is not null and m.set_id in (select a.set_id from set_applies a)
    ),
    set_pattern_hits(set_id, position, id) as (
        ",
            $crate::link_set_pattern_hits!("set_patterns"),
            "
    ),
    reached(t, id, step, origin, rank, via) as (
        select t, id, step, 'own', d, null::text from watched where id <> t and step is not null
        union all
        select a.t, m.entity_id, m.entity_id, 'link_set', 500, a.set_id
          from set_applies a
          join knobas.link_set_member m on m.set_id = a.set_id and m.entity_id is not null
         where m.entity_id <> a.t
           and not exists (select 1 from knobas.link_set_hidden h
                            where h.ticket_id = a.t and h.member_id = m.entity_id)
        union all
        select a.t, h.id, h.id, 'link_set', 500, a.set_id
          from set_applies a
          join set_pattern_hits h on h.set_id = a.set_id
         where h.id <> a.t
           and not exists (select 1 from knobas.link_set_hidden x
                            where x.ticket_id = a.t and x.member_id = h.id)
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
///
/// - `own`: The ticket's own links, its context's adds, and where those lead.
/// - `parent`: The parent ticket's own links (R-7.3), labelled "Parent
///   PAY-231".
/// - `link_set`: A member of a link set whose rule matches the ticket (#866,
///   ADR-0025), labelled with the set's name.
///
/// A link set's arm is named as the reader named the set.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export_to = "entity.ts", rename = "ReachOrigin")]
pub enum Origin {
    Own,
    Parent {
        /// The parent ticket's entity id.
        ticket: String,
    },
    LinkSet {
        /// The set's id, its address: `#/sets/<set>`.
        set: String,
        /// The set's name, as the reader gave it.
        name: String,
    },
}

/// One entity a ticket reaches.
///
/// Issue #817.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct Reached {
    pub entity_id: String,
    /// What `entity_id` is: `pr`, `build`, `asset`, `monitor`.
    pub kind: String,
    /// The first step of the path that brought it in: an entity id. Equal to
    /// `entity_id` when the entity is itself the first step -- linked to the
    /// ticket directly, or one of the parent's own links.
    ///
    /// What the row's reason names.
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
    type Row = (
        String,
        String,
        String,
        String,
        Option<String>,
        Option<String>,
    );
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

    /// The walk spells the downward types as a literal; this holds the
    /// literal to the constant, and the constant to the type table.
    #[test]
    fn the_walk_names_every_downward_type() {
        let walk = crate::reach_walk!();
        let list = DOWNWARD_ONLY
            .iter()
            .map(|id| format!("'{id}'"))
            .collect::<Vec<_>>()
            .join(", ");
        assert_eq!(
            walk.matches(&format!("({list})")).count(),
            3,
            "the two link steps and the held step each name the downward types"
        );
    }

    #[test]
    fn every_downward_type_is_a_built_in_type() {
        for id in DOWNWARD_ONLY {
            assert!(
                crate::asset::TYPES.iter().any(|t| t.id == id),
                "{id} is not an asset type"
            );
        }
    }

    #[test]
    fn the_walk_reads_the_relations_and_the_setting_by_their_constants() {
        let walk = crate::reach_walk!();
        for relation in [
            crate::link::MONITORED_BY,
            crate::link::RUNS_ON,
            crate::link::DEPENDS_ON,
        ] {
            assert!(walk.contains(&format!("'{relation}'")), "{relation}");
        }
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

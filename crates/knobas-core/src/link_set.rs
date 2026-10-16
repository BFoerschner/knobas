//! Which tickets a **link set**'s rules match (#866, ADR-0025, R-7.1, R-7.11),
//! in both directions, as SQL text.
//!
//! A rule is matched over facts the database already keeps, never over links:
//!
//! * `epic` -- every ticket under that ticket at any depth, and `parent` --
//!   that ticket's direct children, both over the **child arm** of
//!   `sync.item_parent` (ADR-0018, ADR-0019): the parent each ticket declares,
//!   resolved and stored. A Jira sub-task declares its story, and the story
//!   its epic, so a sub-task is under the epic at depth two. A parent is the
//!   same source's item of that key. A stored row counts only while its item's
//!   source is configured with the adapter that wrote it, `Vocabulary::declaring`'s
//!   rule as `under:` applies it.
//! * `project` -- `<source id>:<project key>` against `sync.item_project`
//!   (ADR-0010), with the same adapter check.
//! * `manual` -- the ticket whose entity id is the value.
//!
//! **Two directions, one set of facts.** [`link_sets_applying!`] answers which
//! sets apply to a ticket (the job's read walks up from the ticket);
//! [`link_set_rule_tickets!`] answers which tickets a rule applies to (the
//! editor's counts walk down from the rule's ticket). Both read the same
//! table columns with the same joins, and `link_sets_ipc.rs` holds them to the
//! same answer on every ticket of its fixture.
//!
//! Both stop at [`DEPTH`] levels, so a cycle a source let somebody draw
//! (an epic declaring its own story as parent) ends rather than recurses.

/// How many parent levels a rule looks through: an epic, its story, the
/// story's sub-task and two more than any Jira hierarchy has.
pub const DEPTH: i32 = 5;

/// The sets whose rules match each ticket of `start(t)`, as SQL text.
///
/// Expects a CTE `start(t)` and defines `ticket_up(t, id, d)` (the ticket at
/// depth 0 and every ancestor its declared parents name) and
/// `set_applies(t, set_id)`, ending without a trailing comma. Part of
/// [`crate::reach_walk!`]; `d < 5` is [`DEPTH`], spelled as a literal because
/// `concat!` folds only literals (`the_depth_is_spelled_once` holds them).
#[doc(hidden)]
#[macro_export]
macro_rules! link_sets_applying {
    () => {
        "ticket_up(t, id, d) as (
        select s.t, s.t, 0 from start s
        union
        select u.t, ip.source_id || ':' || ip.name, u.d + 1
          from ticket_up u
          join sync.item_parent ip
            on ip.entity_id = u.id and ip.arm = 'child' and ip.kind = 'ticket'
          join knobas.source_config sc on sc.id = ip.source_id and sc.kind = ip.adapter_kind
         where u.d < 5
    ),
    set_applies(t, set_id) as (
        select u.t, r.set_id
          from ticket_up u
          join knobas.link_set_rule r
            on (r.kind = 'manual' and u.d = 0)
            or (r.kind = 'parent' and u.d = 1)
            or (r.kind = 'epic' and u.d >= 1)
         where r.value = u.id
        union
        select s.t, r.set_id
          from start s
          join sync.item_project p on p.entity_id = s.t
          join knobas.source_config sc on sc.id = p.source_id and sc.kind = p.adapter_kind
          join knobas.link_set_rule r
            on r.kind = 'project' and r.value = p.source_id || ':' || p.key
    )"
    };
}

/// The live tickets each rule of `rules(n, kind, value)` applies to, as SQL
/// text: [`link_sets_applying!`] read the other way.
///
/// Expects a CTE `rules(n, rule, kind, value)` (`n` any key the caller groups
/// by, `rule` unique per row) and defines `rule_down(rule, id, d)` and
/// `rule_ticket(n, id)`, ending without a trailing comma. The walk is kept per
/// rule, not per group: a group's epic is not its manual rule's ticket. A
/// ticket counts when the caller finds it a live item of kind `ticket`.
#[doc(hidden)]
#[macro_export]
macro_rules! link_set_rule_tickets {
    () => {
        "rule_down(rule, id, d) as (
        select r.rule, r.value, 0 from rules r where r.kind in ('epic', 'parent', 'manual')
        union
        select w.rule, ip.entity_id, w.d + 1
          from rule_down w
          join sync.item_parent ip
            on lower(ip.name) = lower(substr(w.id, strpos(w.id, ':') + 1))
           and ip.source_id = split_part(w.id, ':', 1)
           and ip.arm = 'child' and ip.kind = 'ticket'
          join knobas.source_config sc on sc.id = ip.source_id and sc.kind = ip.adapter_kind
         -- `lower(name)` reaches `item_parent_name_idx`; the exact match is the
         -- up walk's `source_id || ':' || name`, so the two directions agree.
         where w.d < 5 and ip.name = substr(w.id, strpos(w.id, ':') + 1)
    ),
    rule_ticket(n, id) as (
        select r.n, w.id
          from rule_down w
          join rules r on r.rule = w.rule
         where (r.kind = 'manual' and w.d = 0)
            or (r.kind = 'parent' and w.d = 1)
            or (r.kind = 'epic' and w.d >= 1)
        union
        select r.n, p.entity_id
          from rules r
          join sync.item_project p on r.kind = 'project' and r.value = p.source_id || ':' || p.key
          join knobas.source_config sc on sc.id = p.source_id and sc.kind = p.adapter_kind
    )"
    };
}

#[cfg(test)]
mod tests {
    use super::DEPTH;

    #[test]
    fn the_depth_is_spelled_once() {
        let bound = format!("d < {DEPTH}");
        assert!(crate::link_sets_applying!().contains(&bound));
        assert!(crate::link_set_rule_tickets!().contains(&bound));
    }
}

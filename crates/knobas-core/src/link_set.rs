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
/// depth 0 and every ancestor its declared parents name),
/// `rule_applies(t, set_id, position, kind, value)` (each rule that matches,
/// what the job's *Also on this job* names as the reason, #867) and
/// `set_applies(t, set_id)` (the sets those rules belong to), ending without a
/// trailing comma. Part of
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
    rule_applies(t, set_id, position, kind, value) as (
        select u.t, r.set_id, r.position, r.kind, r.value
          from ticket_up u
          join knobas.link_set_rule r
            on (r.kind = 'manual' and u.d = 0)
            or (r.kind = 'parent' and u.d = 1)
            or (r.kind = 'epic' and u.d >= 1)
         where r.value = u.id
        union
        select s.t, r.set_id, r.position, r.kind, r.value
          from start s
          join sync.item_project p on p.entity_id = s.t
          join knobas.source_config sc on sc.id = p.source_id and sc.kind = p.adapter_kind
          join knobas.link_set_rule r
            on r.kind = 'project' and r.value = p.source_id || ':' || p.key
    ),
    set_applies(t, set_id) as (
        select distinct t, set_id from rule_applies
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
        -- `collate \"C\"`: the recursive term reads `item_parent.entity_id`,
        -- an id in `C` (ADR-0027), and both terms must agree.
        select r.rule, r.value collate \"C\", 0 from rules r where r.kind in ('epic', 'parent', 'manual')
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

/// The entity kinds a `pipeline` member may not be: every other member kind,
/// and what is never a pipeline. A pipeline's kind is its source's own word
/// (`build_config` for TeamCity), so the rest is open. Spelled again in
/// [`link_set_member_fits!`], which `the_pipeline_exclusions_are_spelled_once`
/// holds to this list.
pub const NOT_PIPELINE_KINDS: &[&str] = &[
    "repo", "page", "asset", "monitor", "ticket", "pr", "branch", "commit", "build", "ctx", "note",
    "route",
];

/// SQL for "an entity of kind `$entity` can be a member of kind `$member`":
/// the same kind, except a `pipeline`, which is any kind
/// [`NOT_PIPELINE_KINDS`] does not name.
#[doc(hidden)]
#[macro_export]
macro_rules! link_set_member_fits {
    ($member:literal, $entity:literal) => {
        concat!(
            "(case when ",
            $member,
            " = 'pipeline' then ",
            $entity,
            " <> all(array['repo', 'page', 'asset', 'monitor', 'ticket', 'pr', 'branch', \
             'commit', 'build', 'ctx', 'note', 'route']) else ",
            $entity,
            " = ",
            $member,
            " end)"
        )
    };
}

/// SQL for "the row's key or title matches the glob `$glob`" (#868, the
/// grammar's `name:`): whole, ignoring case, `*` any run of characters and
/// `?` one, every other character itself (`%`, `_` and `\` are escaped
/// before the glob becomes a `like` pattern). `$id` is the row's entity id,
/// whose key is what follows the first `:`; `$title` its title. The one
/// spelling `name:` and a link-set pattern both expand.
#[doc(hidden)]
#[macro_export]
macro_rules! name_glob_matches {
    ($id:literal, $title:literal, $glob:literal) => {
        concat!(
            "exists (select 1 from (select replace(replace(replace(replace(replace(lower(",
            $glob,
            "), '\\', '\\\\'), '%', '\\%'), '_', '\\_'), '*', '%'), '?', '_') as l) gl \
             where lower(substr(",
            $id,
            ", strpos(",
            $id,
            ", ':') + 1)) like gl.l escape '\\' or lower(",
            $title,
            ") like gl.l escape '\\')"
        )
    };
}

/// The live items each pattern member of `$pat` matches now (#868, R-7.2),
/// as one `select` of `(set_id, position, id)`.
///
/// `$pat` is a relation with the columns of `knobas.link_set_member` it
/// reads: `set_id`, `position`, `kind` and `pattern`, the stored reading of
/// the member's query (`knobas_search::Pattern`: `sources`, `projects`,
/// `under`, `names`, each lowercased). Each list ORs its values and the lists
/// AND, over facts the database keeps: the item's source, its declared
/// project in `sync.item_project` by key or name (ADR-0010), or any project
/// of the chain stored beside it (`path`, #1019: a parent project takes in its
/// subprojects), a declared parent
/// in `sync.item_parent` (ADR-0018, ADR-0019, `under:`'s read), and
/// [`name_glob_matches!`]. Both stored tables count a row only while the
/// item's source is configured with the adapter that wrote it, as `under:`
/// does. The job's read ([`crate::reach_walk!`]), the editor's read and the
/// pattern dialog's preview all expand this, so they cannot disagree.
#[doc(hidden)]
#[macro_export]
macro_rules! link_set_pattern_hits {
    ($pat:literal) => {
        concat!(
            "select p.set_id, p.position, i.entity_id as id
          from ",
            $pat,
            " p
          join sync.live_item i on ",
            $crate::link_set_member_fits!("p.kind", "i.kind"),
            "
         where p.pattern is not null
           and (jsonb_array_length(coalesce(p.pattern->'sources', '[]')) = 0
                or i.source_id in (select jsonb_array_elements_text(p.pattern->'sources')))
           and (jsonb_array_length(coalesce(p.pattern->'projects', '[]')) = 0
                or exists (select 1 from sync.item_project ip
                             join knobas.source_config sc
                               on sc.id = ip.source_id and sc.kind = ip.adapter_kind
                            where ip.entity_id = i.entity_id
                              and (lower(ip.key) in (select jsonb_array_elements_text(p.pattern->'projects'))
                                   or lower(ip.name) in (select jsonb_array_elements_text(p.pattern->'projects'))
                                   or ",
            $crate::project_chain_has!(
                "ip.path",
                "array(select jsonb_array_elements_text(p.pattern->'projects'))"
            ),
            ")))
           and (jsonb_array_length(coalesce(p.pattern->'under', '[]')) = 0
                or exists (select 1 from sync.item_parent ip
                             join knobas.source_config sc
                               on sc.id = i.source_id and sc.kind = ip.adapter_kind
                            where ip.entity_id = i.entity_id
                              and lower(ip.name) in (select jsonb_array_elements_text(p.pattern->'under'))))
           and (jsonb_array_length(coalesce(p.pattern->'names', '[]')) = 0
                or exists (select 1 from jsonb_array_elements_text(p.pattern->'names') ng(g)
                            where ",
            $crate::name_glob_matches!("i.entity_id", "i.title", "ng.g"),
            "))"
        )
    };
}

#[cfg(test)]
mod tests {
    use super::DEPTH;

    #[test]
    fn the_pipeline_exclusions_are_spelled_once() {
        let fits = crate::link_set_member_fits!("m", "e");
        let listed = fits
            .split_once("all(array[")
            .and_then(|(_, rest)| rest.split_once("])"))
            .map(|(list, _)| list)
            .expect("the exclusions are an array literal");
        let spelled: Vec<&str> = listed
            .split(',')
            .map(|kind| kind.trim().trim_matches('\''))
            .collect();
        assert_eq!(spelled, super::NOT_PIPELINE_KINDS);
    }

    #[test]
    fn the_depth_is_spelled_once() {
        let bound = format!("d < {DEPTH}");
        assert!(crate::link_sets_applying!().contains(&bound));
        assert!(crate::link_set_rule_tickets!().contains(&bound));
    }
}

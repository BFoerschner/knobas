//! Link sets (#866, spec #808 *Link sets*, R-7.1, R-7.2, R-7.4, R-7.6,
//! R-7.7, R-7.11, ADR-0025): the knobas-owned rows of migration `0039` behind
//! six commands on `commands::entity` -- `link_sets`, `link_set`,
//! `save_link_set`, `delete_link_set`, `preview_link_set_rule` and
//! `hide_link_set_member`.
//!
//! A set is **read beside membership, never through it** (ADR-0025). Nothing
//! here writes a link, and `knobas_core::context` reads none of these tables;
//! what a job shows of a set is `knobas_core::reach`, which matches the rules
//! when it runs. Which tickets a rule matches is `knobas_core::link_set`'s SQL,
//! read the other way round here for the counts.
//!
//! **A member is a fixed item or a pattern** (R-7.2, #868): a stored query in
//! the search grammar (`project:Payouts name:Payout_*`), read through
//! [`knobas_search::Searcher::pattern`] when it is saved and kept with that
//! reading (migration `0040`). What it matches is
//! `knobas_core::link_set_pattern_hits!`, run when a job, the editor or the
//! dialog's preview reads it, so a new pipeline in the project joins without
//! an edit. Patterns are for pipelines and pages ([`PATTERN_KINDS`]).
//!
//! **A member row** (#1008) carries what the editor draws beside the name, all
//! of it read through the declarations (ADR-0007) or off `knobas.asset`: the
//! line ([`MemberRow::line`]) and the dot ([`MemberRow::state`]).
//!
//! **"Yours"** (R-7.6's "39 (10 yours)") is a ticket whose declared assignee
//! is one of the usernames the sources were configured with: the identity
//! `assignee:@me` resolves to (`knobas_search::vocab`), read through the
//! declared `assignee` path (ADR-0007), so a ticket whose assignee the path
//! cannot read is not yours.

use sqlx::{PgPool, Row};

use crate::IpcError;

/// What a member can be (R-7.7's five panels), in panel order.
pub const MEMBER_KINDS: &[&str] = &["repo", "pipeline", "page", "asset", "monitor"];
/// What a rule can match on (R-7.1).
pub const RULE_KINDS: &[&str] = &["epic", "parent", "project", "manual"];

/// [`MEMBER_KINDS`] as the generated TypeScript's `LinkSetMemberKind` union
/// (#1326): what a link-set member can be, R-7.7's five panels. Never built:
/// the wire carries the kind as a string, checked against the list, and this
/// enum only names the union a member's `kind` is in the shell.
/// `the_kind_unions_are_the_kind_lists` pins it to the list.
#[derive(ts_rs::TS)]
#[ts(
    export_to = "entity.ts",
    rename = "LinkSetMemberKind",
    rename_all = "snake_case"
)]
pub enum MemberKind {
    Repo,
    Pipeline,
    Page,
    Asset,
    Monitor,
}

/// [`RULE_KINDS`] as the generated TypeScript's `LinkSetRuleKind` union
/// (#1326): what a rule matches on, R-7.1. Never built, as [`MemberKind`]
/// is not, and pinned to the list by the same test.
#[derive(ts_rs::TS)]
#[ts(
    export_to = "entity.ts",
    rename = "LinkSetRuleKind",
    rename_all = "snake_case"
)]
pub enum RuleKind {
    Epic,
    Parent,
    Project,
    Manual,
}

/// The member kinds a pattern may be (R-7.7: *Add a pattern…* for builds and
/// pages).
pub const PATTERN_KINDS: &[&str] = &["pipeline", "page"];

/// How many matches a pattern row or a preview carries; [`PatternRow::count`]
/// is the whole number.
pub const MATCHES_SHOWN: usize = 200;

use crate::slug::is_slug;

/// The entity kinds a `pipeline` member may not be (`knobas_core::link_set`).
const NOT_PIPELINES: &[&str] = knobas_core::link_set::NOT_PIPELINE_KINDS;

/// One fixed member: an entity of one of [`MEMBER_KINDS`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct LinkSetMember {
    #[ts(as = "MemberKind")]
    pub kind: String,
    pub entity_id: String,
}

/// One pattern member (R-7.2): a kind of [`PATTERN_KINDS`] and a query in the
/// search grammar that reads as a [`knobas_search::Pattern`].
///
/// Issue #868. For pipelines and pages; the query reads like `project:Payouts
/// name:Payout_*`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct LinkSetPattern {
    #[ts(as = "MemberKind")]
    pub kind: String,
    pub query: String,
}

/// One rule (R-7.1): a ticket entity id for `epic`, `parent` and `manual`;
/// `<source id>:<project key>` for `project`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct LinkSetRule {
    #[ts(as = "RuleKind")]
    pub kind: String,
    pub value: String,
}

/// What `save_link_set` takes: the whole set, members and rules in order.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct LinkSetDraft {
    /// `None` makes a new set with an id from its name; an id saves over that
    /// set.
    pub id: Option<String>,
    pub name: String,
    pub description: String,
    pub members: Vec<LinkSetMember>,
    /// The patterns, in order, after the fixed members. Absent reads as
    /// none, so a draft written before #868 still saves.
    #[serde(default)]
    pub patterns: Vec<LinkSetPattern>,
    pub rules: Vec<LinkSetRule>,
}

/// A member as the views draw it: a fixed member, or one item a pattern
/// matches.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts", rename = "LinkSetMemberRow")]
pub struct MemberRow {
    #[ts(as = "MemberKind")]
    pub kind: String,
    pub entity_id: String,
    /// The item's title, or `None` when knobas has no entity of that id.
    pub title: Option<String>,
    /// The line beside the name (#1008): for an asset its type and
    /// environment; for a mirrored item its declared chain of groups (a
    /// pipeline's project path), else its declared project and parent chain
    /// (a page's space and path). `None` where nothing is declared.
    pub line: Option<String>,
    /// The dot (#1008): a pipeline's newest run's declared state, a mirrored
    /// item's own declared state, an asset's health. `None` draws no tone.
    pub state: Option<knobas_core::payload::StateChip>,
}

/// A pattern member as the editor draws it: its query and what it matches
/// now, "(N match now)".
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts", rename = "LinkSetPatternRow")]
pub struct PatternRow {
    #[ts(as = "MemberKind")]
    pub kind: String,
    pub query: String,
    /// How many live items it matches now.
    pub count: i64,
    /// The first [`MATCHES_SHOWN`] of them, by title.
    pub matches: Vec<MemberRow>,
}

/// A project or space the items of a pattern's kind sit in, for the
/// pattern dialog's dropdown.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts", rename = "LinkSetPatternProject")]
pub struct PatternProject {
    pub source_id: String,
    pub key: String,
    pub name: Option<String>,
    /// The project's chain of names, outermost first and ending in its own
    /// (#1019: `Payments`, `Payouts`); empty for a source whose projects do
    /// not nest.
    pub path: Vec<String>,
}

/// What the pattern dialog shows (R-7.7, "Matches now: N"): what the query
/// would match, the projects to pick from, and why the query is not a
/// pattern, if it is not.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts", rename = "LinkSetPatternPreview")]
pub struct PatternPreview {
    pub count: i64,
    pub matches: Vec<MemberRow>,
    pub projects: Vec<PatternProject>,
    /// `None` for a query that is a pattern; then *Add pattern* saves it.
    pub refusal: Option<String>,
}

/// A rule as the views draw it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts", rename = "LinkSetRuleRow")]
pub struct RuleRow {
    #[ts(as = "RuleKind")]
    pub kind: String,
    pub value: String,
    /// The rule's ticket's title, or its project's name; `None` when knobas
    /// has none.
    pub title: Option<String>,
}

/// One set, as the Link sets view lists it (R-7.6).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct LinkSetRow {
    /// A slug: `#/sets/<id>` opens the set.
    pub id: String,
    pub name: String,
    pub description: String,
    pub members: Vec<MemberRow>,
    /// Its patterns, in order (#868).
    pub patterns: Vec<PatternRow>,
    pub rules: Vec<RuleRow>,
    /// How many live tickets its rules match now.
    pub tickets: i64,
    /// How many of those are yours.
    pub mine: i64,
}

/// A ticket a rule or a set applies to.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct RuleTicket {
    pub entity_id: String,
    pub title: String,
    pub mine: bool,
}

/// One set, as its editor draws it (R-7.7): the row, and the tickets it shows
/// on now (*Shows on N tickets now*).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct LinkSetDetail {
    #[serde(flatten)]
    pub set: LinkSetRow,
    /// The tickets it shows on now, in key order.
    pub shows_on: Vec<RuleTicket>,
}

/// A link set name as an address: `set` for a name with no letter or digit.
fn slug(name: &str) -> String {
    crate::slug::slug(name, "set")
}

/// Whether `value` is `<namespace>:<rest>` with neither half blank.
fn qualified(value: &str) -> bool {
    value
        .split_once(':')
        .is_some_and(|(left, right)| !left.trim().is_empty() && !right.trim().is_empty())
}

/// Why a rule cannot be saved or previewed, or `None`.
fn rule_refusal(rule: &LinkSetRule) -> Option<String> {
    if !RULE_KINDS.contains(&rule.kind.as_str()) {
        return Some(format!(
            "rule kind `{}` is not one of {}",
            rule.kind,
            RULE_KINDS.join(", ")
        ));
    }
    if !qualified(&rule.value) {
        return Some(if rule.kind == "project" {
            format!("`{}` is not a project (`<source>:<key>`)", rule.value)
        } else {
            format!("`{}` is not a ticket id", rule.value)
        });
    }
    None
}

/// Why a draft cannot be saved, or `None`. Entities are checked against
/// `knobas.entity` in [`save`].
fn refusal(draft: &LinkSetDraft) -> Option<String> {
    if draft.name.trim().is_empty() {
        return Some("a link set needs a name".to_owned());
    }
    if let Some(id) = &draft.id
        && !is_slug(id)
    {
        return Some(format!("`{id}` is not a link set id"));
    }
    for (at, member) in draft.members.iter().enumerate() {
        if !MEMBER_KINDS.contains(&member.kind.as_str()) {
            return Some(format!(
                "member kind `{}` is not one of {}",
                member.kind,
                MEMBER_KINDS.join(", ")
            ));
        }
        if draft.members[..at]
            .iter()
            .any(|earlier| earlier.entity_id == member.entity_id)
        {
            return Some(format!("`{}` is in the set twice", member.entity_id));
        }
    }
    for (at, pattern) in draft.patterns.iter().enumerate() {
        if let Some(why) = pattern_refusal(pattern) {
            return Some(why);
        }
        if draft.patterns[..at].iter().any(|earlier| {
            earlier.kind == pattern.kind && earlier.query.trim() == pattern.query.trim()
        }) {
            return Some(format!(
                "the pattern `{}` is there twice",
                pattern.query.trim()
            ));
        }
    }
    for (at, rule) in draft.rules.iter().enumerate() {
        if let Some(why) = rule_refusal(rule) {
            return Some(why);
        }
        if draft.rules[..at].contains(rule) {
            return Some(format!(
                "the rule `{} {}` is there twice",
                rule.kind, rule.value
            ));
        }
    }
    None
}

/// Whether an entity of `entity_kind` can be a member of kind `member_kind`.
fn fits(member_kind: &str, entity_kind: &str) -> bool {
    if member_kind == "pipeline" {
        !NOT_PIPELINES.contains(&entity_kind)
    } else {
        member_kind == entity_kind
    }
}

/// What each id of `$2` draws as a member row (#1008), in that order; `$1`
/// is the declarations. Every column of a mirrored item is a declared read
/// (ADR-0007): its chain of groups, a monitor's type, a repository's teams
/// and owner (#1020), its project, its parent chain, its own state, and for a
/// pipeline (the Builds frame) the state of its newest run,
/// a run of its source whose declared `pipeline` is its own -- the rule
/// `entity::pipelines` reads runs by. That run is the head of the pipeline's
/// range of `sync.item_pipeline` (#2185, migration `0062`), one index probe
/// a member, so a quiet pipeline costs what an active one does: walking the
/// source's runs through the declared path cost about 100 ms a quiet member
/// at 30,000 runs. An asset's type and environment are its
/// columns; its health is [`crate::assets`]' rollup, added by [`member_rows`].
const MEMBER_ROWS: &str = concat!(
    "with m as (
  select u.id, u.ord, e.kind, e.title, i.source_id,
         ($1::jsonb #>> array[i.source_id, i.kind, 'frame']) as frame, ",
    knobas_core::declared_string!("$1", "pipeline"),
    " as pkey, ",
    knobas_core::project_key_read!("$1"),
    " as project, ",
    knobas_core::path_line_read!("$1"),
    " as parents,
         (select string_agg(btrim(e.elem #>> c.entry_segs), ' \u{203a} ' order by e.ord) from ",
    knobas_core::declared_list_candidates!("$1", "group_path"),
    " c cross join lateral jsonb_array_elements(
             case when jsonb_typeof(i.payload #> c.at_segs) = 'array'
                  then i.payload #> c.at_segs else '[]'::jsonb end) with ordinality as e(elem, ord)
           where jsonb_typeof(e.elem #> c.entry_segs) = 'string'
             and nullif(btrim(e.elem #>> c.entry_segs), '') is not null) as groups, ",
    knobas_core::declared_string!("$1", "monitor_type"),
    " as monitor_type,
         (select string_agg(t.value, ', ') from ",
    knobas_core::declared_list!("$1", "teams"),
    " t) as teams, ",
    knobas_core::declared_string!("$1", "owner"),
    " as owner, ",
    knobas_core::declared_state!("$1"),
    " as own_state,
         a.type_id, a.environment
    from unnest($2::text[]) with ordinality as u(id, ord)
    left join knobas.entity e on e.id = u.id
    -- `sync.live_item`'s rule, probed by the member's id: planned as the
    -- view, the live mirror was read whole before tables were analyzed.
    left join sync.item i on i.entity_id = u.id and e.deleted_at is null
          and not exists (select 1 from knobas.source_config sc
                           where sc.id = i.source_id and not sc.enabled)
    left join knobas.asset a on a.id = u.id
)
select m.id, m.kind, m.title, m.project, m.parents, m.groups, m.type_id, m.environment,
       m.monitor_type, m.teams, m.owner,
       case when m.frame = 'pipeline' and m.pkey is not null then
         (select ",
    knobas_core::declared_state!("$1"),
    " from (select ip.entity_id from sync.item_pipeline ip
                  where ip.source_id = m.source_id and ip.pkey = m.pkey
                    and ip.adapter_kind = (select s.kind from knobas.source_config s
                                            where s.id = m.source_id)
                    and exists (select 1 from knobas.entity e
                                 where e.id = ip.entity_id and e.deleted_at is null)
                  order by ip.at desc nulls last, ip.entity_id desc limit 1) n
           join sync.item i on i.entity_id = n.entity_id)
       else m.own_state end as state
  from m order by m.ord"
);

/// The member rows' statement, for the plan test (`tests/it/read_plans.rs`,
/// #2185). It binds what `member_rows` binds.
#[doc(hidden)]
#[must_use]
pub fn planned_member_rows() -> &'static str {
    MEMBER_ROWS
}

/// The line a member row draws beside its name, from [`MEMBER_ROWS`]'
/// columns: an asset's `type, environment`; a mirrored item's chain of
/// groups, a monitor's type, a repository's teams else its owner (#1020),
/// else its project and parent chain.
fn line_of(row: &sqlx::postgres::PgRow) -> Option<String> {
    let text = |column: &str| -> Option<String> {
        row.get::<Option<String>, _>(column)
            .filter(|value| !value.trim().is_empty())
    };
    if let Some(type_id) = text("type_id") {
        return Some(match text("environment") {
            Some(env) => format!("{type_id}, {env}"),
            None => type_id,
        });
    }
    if let Some(line) = ["groups", "monitor_type", "teams", "owner"]
        .into_iter()
        .find_map(text)
    {
        return Some(line);
    }
    let parts: Vec<String> = [text("project"), text("parents")]
        .into_iter()
        .flatten()
        .collect();
    (!parts.is_empty()).then(|| parts.join(knobas_core::payload::ANCESTOR_SEPARATOR))
}

/// The member rows of `ids` (`(member kind, entity id)`), in that order
/// (#1008).
async fn member_rows(
    pool: &PgPool,
    declarations: &sqlx::types::Json<serde_json::Value>,
    ids: &[(String, String)],
) -> Result<Vec<MemberRow>, IpcError> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let entity_ids: Vec<&str> = ids.iter().map(|(_, id)| id.as_str()).collect();
    let rows = sqlx::query(MEMBER_ROWS)
        .bind(declarations)
        .bind(&entity_ids)
        .fetch_all(pool)
        .await?;
    let assets: Vec<String> = rows
        .iter()
        .filter(|row| row.get::<Option<String>, _>("type_id").is_some())
        .map(|row| row.get("id"))
        .collect();
    let health = crate::assets::rollup(pool, &assets).await?;
    let mut out = Vec::with_capacity(rows.len());
    for ((kind, _), row) in ids.iter().zip(&rows) {
        let id: String = row.get("id");
        let state = match health.get(&id) {
            Some(rolled) => asset_chip(rolled.health),
            None => row
                .try_get::<Option<sqlx::types::Json<knobas_core::payload::StateChip>>, _>("state")?
                .map(|chip| chip.0),
        };
        out.push(MemberRow {
            kind: kind.clone(),
            title: row.get("title"),
            line: line_of(row),
            state,
            entity_id: id,
        });
    }
    Ok(out)
}

/// An asset's health as a dot: the words the estate uses, in its tones.
fn asset_chip(health: crate::assets::AssetStatus) -> Option<knobas_core::payload::StateChip> {
    use crate::assets::AssetStatus;
    use knobas_core::tone::Tone;
    let tone = match health {
        AssetStatus::Up => Tone::Ok,
        AssetStatus::Warn => Tone::Warn,
        AssetStatus::Down => Tone::Err,
        AssetStatus::None => return None,
    };
    Some(knobas_core::payload::StateChip {
        text: health.as_str().to_owned(),
        tone,
    })
}

/// The live items each stored pattern of the sets `$1` matches now, with the
/// pattern's set and position: `knobas_core::link_set_pattern_hits!` over the
/// stored rows, the text the job's read expands.
const STORED_PATTERN_HITS: &str = concat!(
    "with pat as (
  select set_id, position, kind, pattern from knobas.link_set_member
   where set_id = any($1) and pattern is not null
)
select h.set_id, h.position, h.id, e.title
  from (",
    knobas_core::link_set_pattern_hits!("pat"),
    ") h
  join knobas.entity e on e.id = h.id
 order by h.set_id, h.position, lower(e.title), h.id"
);

/// What one pattern (`$1` its kind, `$2` its reading) matches now: the same
/// text over one row.
const PREVIEW_PATTERN_HITS: &str = concat!(
    "with pat as (select ''::text as set_id, 0 as position, $1::text as kind, $2::jsonb as pattern)
select h.id, e.title
  from (",
    knobas_core::link_set_pattern_hits!("pat"),
    ") h
  join knobas.entity e on e.id = h.id
 order by lower(e.title), h.id"
);

/// The projects the live items of member kind `$1` sit in, by name, for the
/// pattern dialog: `sync.item_project` with `under:`'s adapter check. Every
/// project of a stored chain (`path`, #1019) is offered too, with the names
/// from the outermost down to it, so a parent project no item sits in
/// directly can be picked; an item with no chain offers its own project with
/// an empty path.
const PATTERN_PROJECTS: &str = concat!(
    "with rows as (
    select p.source_id, p.key, p.name, p.at, p.path
      from sync.live_item i
      join sync.item_project p on p.entity_id = i.entity_id
      join knobas.source_config sc on sc.id = p.source_id and sc.kind = p.adapter_kind
     where ",
    knobas_core::link_set_member_fits!("$1", "i.kind"),
    "
),
offered as (
    select r.source_id, r.key, r.name, r.at, '{}'::text[] as path
      from rows r where jsonb_array_length(r.path) = 0
    union all
    select r.source_id, e.pr->>'key', e.pr->>'name', r.at,
           array(select coalesce(u.pr->>'name', u.pr->>'key')
                   from jsonb_array_elements(r.path) with ordinality as u(pr, n)
                  where u.n <= e.n order by u.n)
      from rows r
     cross join lateral jsonb_array_elements(r.path) with ordinality as e(pr, n)
)
select distinct on (source_id, key) source_id, key, name, path
  from offered
 order by source_id, key, (name is null), at desc nulls last"
);

/// Why a pattern member cannot be saved, before its query is read, or
/// `None`.
fn pattern_refusal(pattern: &LinkSetPattern) -> Option<String> {
    if !PATTERN_KINDS.contains(&pattern.kind.as_str()) {
        return Some(format!(
            "a pattern is for {}, not a {}",
            PATTERN_KINDS.join(" and "),
            pattern.kind
        ));
    }
    None
}

/// Read a pattern's query as the grammar reads it today, or say why not.
async fn reading(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    pattern: &LinkSetPattern,
) -> Result<Result<knobas_search::Pattern, String>, IpcError> {
    if let Some(why) = pattern_refusal(pattern) {
        return Ok(Err(why));
    }
    Ok(crate::commands::search::searcher(pool, registry)
        .await?
        .pattern(pattern.query.trim())
        .await?)
}

/// The live tickets each rule of `$2`/`$3`/`$4` (the group, kind and value
/// arrays) applies to, and whether each is yours; `$1` is the declarations.
/// One row per group and ticket, in key order.
const RULE_TICKETS: &str = concat!(
    "with recursive rules(n, rule, kind, value) as (
        select n, rule::int, kind, value
          from unnest($2::int[], $3::text[], $4::text[]) with ordinality as u(n, kind, value, rule)
    ),
    ",
    knobas_core::link_set_rule_tickets!(),
    "
    select n, entity_id, title, mine from (
        select distinct on (t.n, i.entity_id) t.n, i.entity_id, i.title,
               coalesce(lower(",
    knobas_core::declared_string!("$1", "assignee"),
    ") in (select lower(btrim(sc.config->>'username')) from knobas.source_config sc
                where nullif(btrim(sc.config->>'username'), '') is not null), false) as mine
          from rule_ticket t
          join sync.live_item i on i.entity_id = t.id and i.kind = 'ticket'
         order by t.n, i.entity_id
    ) x
    order by n, length(entity_id), entity_id"
);

/// The tickets each group of rules applies to, by group.
async fn tickets_of(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    rules: &[(i32, &LinkSetRule)],
) -> Result<Vec<(i32, RuleTicket)>, IpcError> {
    if rules.is_empty() {
        return Ok(Vec::new());
    }
    let declarations = crate::sources::paths::declared_paths(pool, registry).await?;
    let groups: Vec<i32> = rules.iter().map(|(n, _)| *n).collect();
    let kinds: Vec<&str> = rules.iter().map(|(_, rule)| rule.kind.as_str()).collect();
    let values: Vec<&str> = rules.iter().map(|(_, rule)| rule.value.as_str()).collect();
    let rows = sqlx::query(RULE_TICKETS)
        .bind(declarations.as_param())
        .bind(&groups)
        .bind(&kinds)
        .bind(&values)
        .fetch_all(pool)
        .await?;
    Ok(rows
        .iter()
        .map(|row| {
            (
                row.get("n"),
                RuleTicket {
                    entity_id: row.get("entity_id"),
                    title: row.get("title"),
                    mine: row.get("mine"),
                },
            )
        })
        .collect())
}

/// The sets `ids` names, or every set for `None`, each with its members, its
/// rules and how many tickets it shows on; and per set, those tickets.
async fn read(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    ids: Option<&[String]>,
) -> Result<Vec<(LinkSetRow, Vec<RuleTicket>)>, IpcError> {
    // Make sure the stored parent resolutions are the registry's (#627), as
    // every search does before it reads them.
    crate::commands::search::declare(pool, registry).await?;
    let sets = sqlx::query(
        "select id, name, description from knobas.link_set
          where $1::text[] is null or id = any($1)
          order by lower(name), id",
    )
    .bind(ids)
    .fetch_all(pool)
    .await?;
    let ids: Vec<String> = sets.iter().map(|row| row.get("id")).collect();
    let members = sqlx::query(
        "select m.set_id, m.kind, m.entity_id, m.query, m.position
           from knobas.link_set_member m
          where m.set_id = any($1)
          order by m.set_id, m.position",
    )
    .bind(&ids)
    .fetch_all(pool)
    .await?;
    let hits = sqlx::query(STORED_PATTERN_HITS)
        .bind(&ids)
        .fetch_all(pool)
        .await?;
    let declarations = crate::sources::paths::declared_paths(pool, registry).await?;
    let declarations = declarations.as_param();
    let rules = sqlx::query(
        "select r.set_id, r.kind, r.value,
                coalesce(e.title, (select p.name from sync.item_project p
                                    where r.kind = 'project' and p.source_id || ':' || p.key = r.value
                                      and p.name is not null limit 1)) as title
           from knobas.link_set_rule r
           left join knobas.entity e on r.kind <> 'project' and e.id = r.value
          where r.set_id = any($1)
          order by r.set_id, r.position",
    )
    .bind(&ids)
    .fetch_all(pool)
    .await?;
    let mut out: Vec<(LinkSetRow, Vec<RuleTicket>)> = sets
        .iter()
        .map(|row| {
            (
                LinkSetRow {
                    id: row.get("id"),
                    name: row.get("name"),
                    description: row.get("description"),
                    members: Vec::new(),
                    patterns: Vec::new(),
                    rules: Vec::new(),
                    tickets: 0,
                    mine: 0,
                },
                Vec::new(),
            )
        })
        .collect();
    let at = |id: &str| ids.iter().position(|known| known == id);
    // Every row to draw, fixed members and the shown matches of each
    // pattern, read once (#1008), then dealt out in the stored order.
    let mut wanted: Vec<(String, String)> = Vec::new();
    for row in &members {
        if let Some(entity_id) = row.get::<Option<String>, _>("entity_id") {
            wanted.push((row.get("kind"), entity_id));
        }
    }
    let mut shown: std::collections::HashMap<(String, i32), (i64, Vec<String>)> =
        std::collections::HashMap::new();
    for hit in &hits {
        let entry = shown
            .entry((hit.get("set_id"), hit.get("position")))
            .or_default();
        entry.0 += 1;
        if entry.1.len() < MATCHES_SHOWN {
            entry.1.push(hit.get("id"));
        }
    }
    for row in &members {
        if row.get::<Option<String>, _>("query").is_some()
            && let Some((_, matched)) = shown.get(&(row.get("set_id"), row.get("position")))
        {
            let kind: String = row.get("kind");
            wanted.extend(matched.iter().map(|id| (kind.clone(), id.clone())));
        }
    }
    let mut drawn = member_rows(pool, &declarations, &wanted).await?.into_iter();
    for row in &members {
        if row.get::<Option<String>, _>("entity_id").is_none() {
            continue;
        }
        if let (Some(member), Some(index)) = (drawn.next(), at(row.get("set_id"))) {
            out[index].0.members.push(member);
        }
    }
    for row in &members {
        let Some(query) = row.get::<Option<String>, _>("query") else {
            continue;
        };
        let (count, shown_count) = shown
            .get(&(row.get("set_id"), row.get("position")))
            .map_or((0, 0), |(count, ids)| (*count, ids.len()));
        let matches: Vec<MemberRow> = drawn.by_ref().take(shown_count).collect();
        if let Some(index) = at(row.get("set_id")) {
            out[index].0.patterns.push(PatternRow {
                kind: row.get("kind"),
                query,
                count,
                matches,
            });
        }
    }
    let mut stored: Vec<(i32, LinkSetRule)> = Vec::new();
    for row in &rules {
        if let Some(index) = at(row.get("set_id")) {
            let rule = LinkSetRule {
                kind: row.get("kind"),
                value: row.get("value"),
            };
            out[index].0.rules.push(RuleRow {
                kind: rule.kind.clone(),
                value: rule.value.clone(),
                title: row.get("title"),
            });
            stored.push((i32::try_from(index).map_err(IpcError::internal)?, rule));
        }
    }
    let grouped: Vec<(i32, &LinkSetRule)> = stored.iter().map(|(n, rule)| (*n, rule)).collect();
    for (n, ticket) in tickets_of(pool, registry, &grouped).await? {
        let entry = &mut out[usize::try_from(n).map_err(IpcError::internal)?];
        entry.0.tickets += 1;
        entry.0.mine += i64::from(ticket.mine);
        entry.1.push(ticket);
    }
    Ok(out)
}

/// Every link set, by name, with its contents, rules and ticket counts
/// (R-7.6).
///
/// # Errors
///
/// `internal` if a read fails.
pub async fn list(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
) -> Result<Vec<LinkSetRow>, IpcError> {
    Ok(read(pool, registry, None)
        .await?
        .into_iter()
        .map(|(row, _)| row)
        .collect())
}

/// One link set with the tickets it shows on now (R-7.7).
///
/// # Errors
///
/// `not_found` if no set has that id; `internal` if a read fails.
pub async fn get(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    id: &str,
) -> Result<LinkSetDetail, IpcError> {
    read(pool, registry, Some(&[id.to_owned()]))
        .await?
        .into_iter()
        .next()
        .map(|(set, shows_on)| LinkSetDetail { set, shows_on })
        .ok_or_else(|| IpcError::not_found(format!("no link set is called {id}")))
}

/// The live tickets one rule applies to now, yours marked (R-7.7's live
/// counts), whether or not any set holds the rule.
///
/// # Errors
///
/// `invalid` for a rule [`rule_refusal`] names; `internal` if a read fails.
pub async fn preview(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    rule: &LinkSetRule,
) -> Result<Vec<RuleTicket>, IpcError> {
    if let Some(why) = rule_refusal(rule) {
        return Err(IpcError::invalid(why));
    }
    crate::commands::search::declare(pool, registry).await?;
    Ok(tickets_of(pool, registry, &[(0, rule)])
        .await?
        .into_iter()
        .map(|(_, ticket)| ticket)
        .collect())
}

/// What a pattern of `pattern.kind` with `pattern.query` would match now, the
/// projects its kind sits in, and why the query is not a pattern if it is not
/// (R-7.7's dialog, "Matches now: N"). A refusal is an answer, not an error:
/// the dialog shows it while somebody types.
///
/// # Errors
///
/// `internal` if a read fails.
pub async fn preview_pattern(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    pattern: &LinkSetPattern,
) -> Result<PatternPreview, IpcError> {
    let read = reading(pool, registry, pattern).await?;
    let kind = if PATTERN_KINDS.contains(&pattern.kind.as_str()) {
        pattern.kind.as_str()
    } else {
        ""
    };
    let projects =
        sqlx::query_as::<_, (String, String, Option<String>, Vec<String>)>(PATTERN_PROJECTS)
            .bind(kind)
            .fetch_all(pool)
            .await?
            .into_iter()
            .map(|(source_id, key, name, path)| PatternProject {
                source_id,
                key,
                name,
                path,
            })
            .collect();
    let read = match read {
        Ok(read) => read,
        Err(why) => {
            return Ok(PatternPreview {
                count: 0,
                matches: Vec::new(),
                projects,
                refusal: Some(why),
            });
        }
    };
    let hits: Vec<(String, String)> = sqlx::query_as(PREVIEW_PATTERN_HITS)
        .bind(kind)
        .bind(serde_json::to_value(&read).map_err(IpcError::internal)?)
        .fetch_all(pool)
        .await?;
    let count = i64::try_from(hits.len()).map_err(IpcError::internal)?;
    let wanted: Vec<(String, String)> = hits
        .into_iter()
        .take(MATCHES_SHOWN)
        .map(|(id, _)| (kind.to_owned(), id))
        .collect();
    let declarations = crate::sources::paths::declared_paths(pool, registry).await?;
    Ok(PatternPreview {
        count,
        matches: member_rows(pool, &declarations.as_param(), &wanted).await?,
        projects,
        refusal: None,
    })
}

/// Save a whole set -- a new one under an id from its name, or over the set
/// the draft names, its members and rules replaced by the draft's in order --
/// and answer it as stored. Every job it applies to reads the change on its
/// next read; nothing else is written.
///
/// # Errors
///
/// `invalid` for a draft [`refusal`] names, or a member knobas has no entity
/// for or whose entity is not of the member's kind; `not_found` for a draft
/// naming a set that is not there (one deleted while its editor was open);
/// `internal` if the write fails.
pub async fn save(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    draft: LinkSetDraft,
) -> Result<LinkSetDetail, IpcError> {
    if let Some(why) = refusal(&draft) {
        return Err(IpcError::invalid(why));
    }
    let ids: Vec<&str> = draft.members.iter().map(|m| m.entity_id.as_str()).collect();
    let known: Vec<(String, String)> =
        sqlx::query_as("select id, kind from knobas.entity where id = any($1)")
            .bind(&ids)
            .fetch_all(pool)
            .await?;
    for member in &draft.members {
        match known.iter().find(|(id, _)| *id == member.entity_id) {
            None => {
                return Err(IpcError::invalid(format!(
                    "knobas has no item `{}`",
                    member.entity_id
                )));
            }
            Some((_, kind)) if !fits(&member.kind, kind) => {
                return Err(IpcError::invalid(format!(
                    "`{}` is a {kind}, not a {}",
                    member.entity_id, member.kind
                )));
            }
            Some(_) => {}
        }
    }
    let mut readings: Vec<serde_json::Value> = Vec::with_capacity(draft.patterns.len());
    for pattern in &draft.patterns {
        match reading(pool, registry, pattern).await? {
            Ok(read) => readings.push(serde_json::to_value(read).map_err(IpcError::internal)?),
            Err(why) => {
                return Err(IpcError::invalid(format!(
                    "`{}` is not a pattern: {why}",
                    pattern.query.trim()
                )));
            }
        }
    }
    let mut tx = pool.begin().await?;
    // One writer at a time picks ids, so two saves cannot take the same slug.
    sqlx::query("lock table knobas.link_set in share row exclusive mode")
        .execute(&mut *tx)
        .await?;
    let id = match &draft.id {
        Some(id) => {
            // Under the lock, so a delete either lands first and this save is
            // refused, or waits for it: an editor still open on a deleted set
            // never brings it back.
            let exists: bool =
                sqlx::query_scalar("select exists (select 1 from knobas.link_set where id = $1)")
                    .bind(id)
                    .fetch_one(&mut *tx)
                    .await?;
            if !exists {
                return Err(IpcError::not_found(format!("no link set is called {id}")));
            }
            id.clone()
        }
        None => {
            let base = slug(&draft.name);
            let taken: Vec<String> =
                sqlx::query_scalar("select id from knobas.link_set where id = $1 or id like $2")
                    .bind(&base)
                    .bind(format!("{base}-%"))
                    .fetch_all(&mut *tx)
                    .await?;
            crate::slug::first_free(&base, &taken)
        }
    };
    sqlx::query(
        "insert into knobas.link_set as s (id, name, description) values ($1, $2, $3)
         on conflict (id) do update set name = excluded.name,
           description = excluded.description, updated_at = now()",
    )
    .bind(&id)
    .bind(draft.name.trim())
    .bind(draft.description.trim())
    .execute(&mut *tx)
    .await?;
    sqlx::query("delete from knobas.link_set_member where set_id = $1")
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("delete from knobas.link_set_rule where set_id = $1")
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    let kinds: Vec<&str> = draft.members.iter().map(|m| m.kind.as_str()).collect();
    sqlx::query(
        "insert into knobas.link_set_member (set_id, kind, entity_id, position)
         select $1, k, e, (p - 1)::int from unnest($2::text[], $3::text[]) with ordinality as u(k, e, p)",
    )
    .bind(&id)
    .bind(&kinds)
    .bind(&ids)
    .execute(&mut *tx)
    .await?;
    let pattern_kinds: Vec<&str> = draft.patterns.iter().map(|p| p.kind.as_str()).collect();
    let queries: Vec<&str> = draft.patterns.iter().map(|p| p.query.trim()).collect();
    sqlx::query(
        "insert into knobas.link_set_member (set_id, kind, query, pattern, position)
         select $1, k, q, r, ($5 + p - 1)::int
           from unnest($2::text[], $3::text[], $4::jsonb[]) with ordinality as u(k, q, r, p)",
    )
    .bind(&id)
    .bind(&pattern_kinds)
    .bind(&queries)
    .bind(&readings)
    .bind(i32::try_from(ids.len()).map_err(IpcError::internal)?)
    .execute(&mut *tx)
    .await?;
    let rule_kinds: Vec<&str> = draft.rules.iter().map(|r| r.kind.as_str()).collect();
    let values: Vec<&str> = draft.rules.iter().map(|r| r.value.as_str()).collect();
    sqlx::query(
        "insert into knobas.link_set_rule (set_id, kind, value, position)
         select $1, k, v, (p - 1)::int from unnest($2::text[], $3::text[]) with ordinality as u(k, v, p)",
    )
    .bind(&id)
    .bind(&rule_kinds)
    .bind(&values)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    get(pool, registry, &id).await
}

/// Forget a set: its members and rules go with it, and every job it applied
/// to stops showing it. Links drawn on tickets by hand are not touched.
///
/// # Errors
///
/// `not_found` if no set has that id; `internal` if the delete fails.
pub async fn delete(pool: &PgPool, id: &str) -> Result<(), IpcError> {
    let gone = sqlx::query("delete from knobas.link_set where id = $1")
        .bind(id)
        .execute(pool)
        .await?
        .rows_affected();
    if gone == 0 {
        return Err(IpcError::not_found(format!("no link set is called {id}")));
    }
    Ok(())
}

/// *Hide here* and *Show again* (R-7.5): hide one link-set member on one
/// ticket, or show it again, and answer whether it is now hidden. The set is
/// not touched. Hiding twice, or showing what is not hidden, changes nothing.
///
/// # Errors
///
/// `invalid` for an id that is not one; `internal` if the write fails.
pub async fn hide(
    pool: &PgPool,
    ticket_id: &str,
    member_id: &str,
    hidden: bool,
) -> Result<bool, IpcError> {
    for id in [ticket_id, member_id] {
        if !qualified(id) {
            return Err(IpcError::invalid(format!("`{id}` is not an entity id")));
        }
    }
    let statement = if hidden {
        "insert into knobas.link_set_hidden (ticket_id, member_id) values ($1, $2)
         on conflict do nothing"
    } else {
        "delete from knobas.link_set_hidden where ticket_id = $1 and member_id = $2"
    };
    sqlx::query(statement)
        .bind(ticket_id)
        .bind(member_id)
        .execute(pool)
        .await?;
    Ok(hidden)
}

/// One member of a set that applies to a job (#867): what the drawer's
/// group lists, and whether *Hide here* hid it on this ticket.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct JobMember {
    /// The member's kind, one of [`MEMBER_KINDS`]: the row's section.
    #[ts(as = "MemberKind")]
    pub kind: String,
    pub entity_id: String,
    /// The entity's own kind, what the row links by; `None` when knobas has
    /// no entity of that id.
    pub entity_kind: Option<String>,
    pub title: Option<String>,
    /// Hidden on this ticket by *Hide here* (R-7.5).
    pub hidden: bool,
}

/// One set that applies to a job (R-4.4, R-6.2): the chip's name, the reason
/// (the first of its rules that matches, in the set's order) and its members.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct JobSet {
    pub id: String,
    pub name: String,
    /// The first of its rules that matches the ticket, in the set's order: the
    /// reason.
    pub via: RuleRow,
    pub members: Vec<JobMember>,
}

/// One of a parent ticket's own links: the far end.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct JobLink {
    pub entity_id: String,
    pub kind: String,
    pub title: Option<String>,
}

/// The parent whose own links a sub-task shows (R-7.3).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct JobParent {
    /// The parent ticket's entity id.
    pub ticket: String,
    pub title: Option<String>,
    pub links: Vec<JobLink>,
}

/// What a job shows beside its own links (#867): the sets that apply, by
/// name, and the parent behind R-7.3's switch.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct JobLinkSets {
    /// The sets that apply, by name.
    pub sets: Vec<JobSet>,
    /// `None` with no parent, an epic for one, or R-7.3's switch off.
    pub parent: Option<JobParent>,
}

/// The sets applying to `$1`, each once with the first matching rule.
const JOB_SETS: &str = concat!(
    "with recursive start(t) as (select $1::text),
    ",
    knobas_core::link_sets_applying!(),
    "
    select s.id, s.name, a.kind, a.value,
           coalesce(e.title, (select p.name from sync.item_project p
                               where a.kind = 'project' and p.source_id || ':' || p.key = a.value
                                 and p.name is not null limit 1)) as title
      from (select distinct on (set_id) set_id, kind, value
              from rule_applies order by set_id, position) a
      join knobas.link_set s on s.id = a.set_id
      left join knobas.entity e on a.kind <> 'project' and e.id = a.value
     order by lower(s.name), s.id"
);

/// The members of the sets `$1` as the job of `$2` shows them: the fixed
/// members, then what each pattern matches now (#868, the reach's own
/// `link_set_pattern_hits!`), each item once per set, in the set's order,
/// and whether *Hide here* hid it on this ticket.
const JOB_MEMBERS: &str = concat!(
    "with pat as (
  select set_id, position, kind, pattern from knobas.link_set_member
   where set_id = any($1) and pattern is not null
),
held as (
  select m.set_id, m.position, 0 as hit, m.kind, m.entity_id
    from knobas.link_set_member m
   where m.set_id = any($1) and m.entity_id is not null
  union all
  select h.set_id, h.position, 1, p.kind, h.id
    from (",
    knobas_core::link_set_pattern_hits!("pat"),
    ") h
    join pat p on p.set_id = h.set_id and p.position = h.position
)
select x.set_id, x.kind, x.entity_id, e.kind as entity_kind, e.title,
       exists (select 1 from knobas.link_set_hidden h
                where h.ticket_id = $2 and h.member_id = x.entity_id) as hidden
  from (select distinct on (set_id, entity_id) * from held
         where entity_id <> $2
         order by set_id, entity_id, position, hit) x
  left join knobas.entity e on e.id = x.entity_id
 order by x.set_id, x.position, x.hit, lower(e.title), x.entity_id"
);

/// The parent `$1` shows the links of, and each far end of those links
/// (`entity_id` null for a parent with none): [`knobas_core::reach_walk!`]'s
/// own `parent` term, so the drawer and the reach never disagree.
const JOB_PARENT: &str = concat!(
    "with recursive start(t) as (select $1::text),
    ",
    knobas_core::reach_walk!(),
    "
    select distinct p.p as ticket, pe.title as ticket_title, o.id as entity_id, o.kind, o.title
      from parent p
      join knobas.entity pe on pe.id = p.p
      left join knobas.confirmed_link l on l.from_id = p.p or l.to_id = p.p
      left join knobas.entity o
        on o.id = case when l.from_id = p.p then l.to_id else l.from_id end
       and o.kind <> 'ctx' and o.id <> p.t
     order by o.title, o.id"
);

/// What the job of `ticket_id` shows beside its own links (#867, R-4.4,
/// R-6.2, R-7.3): every set whose rules match it, with the reason and its
/// members (hidden ones marked), and its parent's own links while R-7.3's
/// switch is on. Read when the job is read; nothing is stored. An id nothing
/// answers to has neither.
///
/// # Errors
///
/// `internal` if a read fails.
pub async fn on_job(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    ticket_id: &str,
) -> Result<JobLinkSets, IpcError> {
    crate::commands::search::declare(pool, registry).await?;
    let sets = sqlx::query(JOB_SETS)
        .bind(ticket_id)
        .fetch_all(pool)
        .await?;
    let ids: Vec<String> = sets.iter().map(|row| row.get("id")).collect();
    let members = sqlx::query(JOB_MEMBERS)
        .bind(&ids)
        .bind(ticket_id)
        .fetch_all(pool)
        .await?;
    let mut out: Vec<JobSet> = sets
        .iter()
        .map(|row| JobSet {
            id: row.get("id"),
            name: row.get("name"),
            via: RuleRow {
                kind: row.get("kind"),
                value: row.get("value"),
                title: row.get("title"),
            },
            members: Vec::new(),
        })
        .collect();
    for row in &members {
        let set_id: String = row.get("set_id");
        if let Some(set) = out.iter_mut().find(|set| set.id == set_id) {
            set.members.push(JobMember {
                kind: row.get("kind"),
                entity_id: row.get("entity_id"),
                entity_kind: row.get("entity_kind"),
                title: row.get("title"),
                hidden: row.get("hidden"),
            });
        }
    }
    let rows = sqlx::query(JOB_PARENT)
        .bind(ticket_id)
        .fetch_all(pool)
        .await?;
    let parent = rows.first().map(|first| JobParent {
        ticket: first.get("ticket"),
        title: first.get("ticket_title"),
        links: rows
            .iter()
            .filter_map(|row| {
                Some(JobLink {
                    entity_id: row.get::<Option<String>, _>("entity_id")?,
                    kind: row.get("kind"),
                    title: row.get("title"),
                })
            })
            .collect(),
    });
    Ok(JobLinkSets { sets: out, parent })
}

/// How many tickets under one epic an item must be linked to by hand before
/// the Link sets view offers it to the epic's set (R-7.10's "several").
pub const HINT_TICKETS: i64 = 2;

/// R-7.10's hint (#870): "You linked Dashboard_Build by hand to PAY-240 and 2
/// more tickets under PAY-200. Add it to Payout stack?"
///
/// An item linked by hand to two or more tickets under the epic a set's rule
/// names, which the set does not hold yet.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct LinkSetHint {
    pub set_id: String,
    pub set_name: String,
    /// The epic the set's rule names, a ticket entity id.
    pub epic: String,
    /// What *Add to …* adds: the item, as a member of its kind.
    pub member: LinkSetMember,
    /// The item's title.
    pub title: String,
    /// The tickets under the epic it is linked to by hand, in key order.
    pub tickets: Vec<String>,
}

/// One row per set, epic rule and item: the items linked by hand (a live
/// `manual` link, either end) to [`HINT_TICKETS`] or more live tickets under
/// the epic, that can be a member and that the set neither holds nor matches
/// by a pattern. Most tickets first.
const HINTS: &str = concat!(
    "with recursive epics(n, set_id, value) as (
        select row_number() over (order by set_id, position)::int, set_id, value
          from knobas.link_set_rule where kind = 'epic'
    ),
    rules(n, rule, kind, value) as (
        select n, n, 'epic', value from epics
    ),
    ",
    knobas_core::link_set_rule_tickets!(),
    ",
    under(n, ticket) as (
        select distinct t.n, t.id
          from rule_ticket t
          join sync.live_item i on i.entity_id = t.id and i.kind = 'ticket'
    ),
    by_hand(ticket, item) as (
        select from_id, to_id from knobas.confirmed_link where origin = 'manual'
        union
        select to_id, from_id from knobas.confirmed_link where origin = 'manual'
    ),
    pat as (
        select set_id, position, kind, pattern from knobas.link_set_member
         where pattern is not null
    ),
    pattern_hit as (",
    knobas_core::link_set_pattern_hits!("pat"),
    "),
    offered as (
        select e.set_id, e.value as epic, x.id as item, x.kind, x.title, u.ticket,
               case when x.kind in ('repo', 'page', 'asset', 'monitor') then x.kind
                    else 'pipeline' end as member_kind
          from under u
          join epics e on e.n = u.n
          join by_hand h on h.ticket = u.ticket
          join knobas.entity x on x.id = h.item and x.deleted_at is null
    )
    select o.set_id, s.name as set_name, o.epic, o.member_kind, o.item, o.title,
           array_agg(distinct o.ticket) as tickets
      from offered o
      join knobas.link_set s on s.id = o.set_id
     where ",
    knobas_core::link_set_member_fits!("o.member_kind", "o.kind"),
    "
       and not exists (select 1 from knobas.link_set_member m
                        where m.set_id = o.set_id and m.entity_id = o.item)
       and not exists (select 1 from pattern_hit ph where ph.set_id = o.set_id and ph.id = o.item)
     group by o.set_id, s.name, o.epic, o.member_kind, o.item, o.title
    having count(distinct o.ticket) >= $1
     order by count(distinct o.ticket) desc, lower(s.name), lower(o.title), o.item"
);

/// The hints the Link sets view offers (R-7.10, #870): see [`HINTS`].
///
/// # Errors
///
/// `internal` if a read fails.
pub async fn hints(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
) -> Result<Vec<LinkSetHint>, IpcError> {
    crate::commands::search::declare(pool, registry).await?;
    let rows = sqlx::query(HINTS)
        .bind(HINT_TICKETS)
        .fetch_all(pool)
        .await?;
    Ok(rows
        .iter()
        .map(|row| {
            let mut tickets: Vec<String> = row.get("tickets");
            tickets.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)));
            LinkSetHint {
                set_id: row.get("set_id"),
                set_name: row.get("set_name"),
                epic: row.get("epic"),
                member: LinkSetMember {
                    kind: row.get("member_kind"),
                    entity_id: row.get("item"),
                },
                title: row.get("title"),
                tickets,
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slug::MAX_ID_CHARS;

    /// The two kind unions the generated TypeScript declares (#1326) are
    /// exactly the lists the saves check against, in their order: a kind
    /// added to a list alone would be refused by the shell's types, and one
    /// added to an enum alone offered by the shell and refused on save.
    #[test]
    fn the_kind_unions_are_the_kind_lists() {
        let cfg = ts_rs::Config::new();
        let union = |kinds: &[&str]| {
            kinds
                .iter()
                .map(|kind| format!("\"{kind}\""))
                .collect::<Vec<_>>()
                .join(" | ")
        };
        assert_eq!(<MemberKind as ts_rs::TS>::inline(&cfg), union(MEMBER_KINDS));
        assert_eq!(<RuleKind as ts_rs::TS>::inline(&cfg), union(RULE_KINDS));
    }

    fn draft() -> LinkSetDraft {
        LinkSetDraft {
            id: None,
            name: "Payout stack".to_owned(),
            description: String::new(),
            members: vec![LinkSetMember {
                kind: "repo".to_owned(),
                entity_id: "gitea:tidewater/payout-service".to_owned(),
            }],
            patterns: Vec::new(),
            rules: vec![LinkSetRule {
                kind: "epic".to_owned(),
                value: "jira:PAY-200".to_owned(),
            }],
        }
    }

    #[test]
    fn a_slug_is_an_address_whatever_the_name_was() {
        for (name, expected) in [
            ("Payout stack", "payout-stack"),
            ("  On-call   kit ", "on-call-kit"),
            ("...", "set"),
        ] {
            let made = slug(name);
            assert_eq!(made, expected, "{name:?}");
            assert!(is_slug(&made), "{made:?}");
        }
        assert!(slug(&"a".repeat(200)).len() <= MAX_ID_CHARS);
    }

    #[test]
    fn a_draft_is_refused_for_what_cannot_be_a_set() {
        assert_eq!(refusal(&draft()), None);
        type Change = fn(&mut LinkSetDraft);
        let cases: [(Change, &str); 6] = [
            (|d| d.name = "  ".to_owned(), "a link set needs a name"),
            (
                |d| d.id = Some("Not A Slug".to_owned()),
                "is not a link set id",
            ),
            (
                |d| d.members[0].kind = "build".to_owned(),
                "member kind `build`",
            ),
            (
                |d| d.members.push(d.members[0].clone()),
                "is in the set twice",
            ),
            (
                |d| d.rules[0].kind = "label".to_owned(),
                "rule kind `label`",
            ),
            (
                |d| d.rules[0].value = "PAY-200".to_owned(),
                "is not a ticket id",
            ),
        ];
        for (change, why) in cases {
            let mut refused = draft();
            change(&mut refused);
            let said = refusal(&refused).unwrap_or_default();
            assert!(said.contains(why), "{said:?} should say {why:?}");
        }
    }

    #[test]
    fn a_member_fits_its_kind() {
        assert!(fits("repo", "repo"));
        assert!(!fits("repo", "page"));
        assert!(fits("pipeline", "build_config"));
        assert!(!fits("pipeline", "build"));
        assert!(!fits("pipeline", "ticket"));
    }
}

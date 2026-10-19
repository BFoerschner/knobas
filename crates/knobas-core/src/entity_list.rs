//! The entity list read: a room's rows, the filter, order and tally they
//! answer, and the six statements (#1317, spec #1303), beside the mini board
//! read ([`crate::mini_board`]), which is its precedent.
//!
//! [`read`] takes a pool, the filter, the page bounds, the declarations and
//! the role's fillers, and answers the page: rows, total, tally and groups.
//! Which statement a filter reads through and what is bound where are this
//! module's business. The fillers come from the caller, as the declarations
//! do: who fills a role is the adapter registry's answer, and the registry
//! lives in the app crate.
//!
//! ## Why there is no query builder here
//!
//! Roadmap §4 gotcha 2 asks for dynamic SQL to live in one reviewed module
//! behind `AssertSqlSafe`. This module dodges the question instead: every
//! optional predicate is a **nullable parameter** (`$1::text[] is null or ...`)
//! and every ordering and scope is its own `const` statement chosen by a
//! `match` (six since #680's two tile orders), so nothing here ever
//! concatenates a string into SQL and `sqlx::query` is only ever handed a
//! `&'static str`. A filter that grows a new optional predicate adds a
//! parameter, not a branch.
//!
//! ## Why the columns are named
//!
//! `sync.live_item` exposes `fts`, a `tsvector` (migration `0002`'s own
//! WARNING). Naming the columns each statement wants keeps that value out of
//! the result set entirely rather than relying on nobody ever asking for it by
//! name.

use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};

use crate::CoreError;

/// One line in a room: what a tile draws, and nothing more.
///
/// Deliberately not the mirror row. `body_text` and `payload` are the detail
/// view's business (the app's `EntityDetail`), and a room that fetched them would pull
/// the whole corpus into the webview to render five columns of it.
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct EntityRow {
    /// `"<namespace>:<key>"` -- `mock:PAY-231`. The kind is not part of it.
    pub entity_id: String,
    pub kind: String,
    pub source_id: String,
    /// Raw source text. Render it as text, never as markup (gotcha 7).
    pub title: String,
    /// When the *source* says the item changed.
    ///
    /// `None` where the source never said (interfaces §4.1 normalization):
    /// the mirror keeps the hole rather than filling it with `now()`, so a row
    /// that has never been dated is distinguishable from one changed this
    /// second.
    ///
    /// RFC 3339 on the wire. The room sorts an undated row last rather than
    /// first.
    pub updated_at: Option<DateTime<Utc>>,
    /// When knobas last saw it. Always known -- it is knobas' own clock.
    ///
    /// RFC 3339 on the wire.
    pub synced_at: DateTime<Utc>,
    /// Where this row sits **inside its source**, as one line -- a Confluence
    /// page's ancestor path, `Engineering \u{203a} Payments` (#284, ratified
    /// as a contract §10.8 exception under that ticket's criterion 5).
    ///
    /// Since #596 it is the **parent chain** the row's kind declares
    /// (ADR-0018) -- `owner/repo` for a Gitea branch, pull request or commit,
    /// a page's ancestors since Confluence declared them (#599) -- and nothing
    /// else since #646 retired the undeclared ancestor read.
    ///
    /// `None` for every row whose kind declares no parent or whose declared
    /// parent misses: the ADR-0007 **miss**. Joined by
    /// `crate::path_line_read!`, the one expression that spells it, so
    /// this row and the launcher's cannot disagree about what a path is.
    ///
    /// This struct and `knobas_search::EntityRow` are pinned to **one wire
    /// shape** by `knobas-app`'s `commands::search` test; the field is on both.
    pub path: Option<String>,
    /// The item's state chip -- a pull request's `open` / `merged` /
    /// `closed`, a build's `running` or outcome (#666) -- resolved by
    /// `crate::declared_state!` from the state the row's kind declares,
    /// beside the path line and wherever it is read. `None` for a kind that
    /// declares none and for a value the declaration does not name: the
    /// ADR-0007 **miss**, so a row shows no chip rather than a wrong one.
    ///
    /// On both `EntityRow`s, pinned to one wire shape like `path`.
    ///
    /// `text` is source text: render it as text.
    pub state: Option<crate::payload::StateChip>,
}

/// The `state` column every row statement selects, decoded.
///
/// # Errors
///
/// The decode error, for a `jsonb` that is not a chip. `declared_state!`
/// builds it from a declaration whose tones are typed, so that is a bug, and
/// it fails the read rather than reading as a row with no state.
pub fn state_of(
    row: &sqlx::postgres::PgRow,
) -> Result<Option<crate::payload::StateChip>, sqlx::Error> {
    Ok(row
        .try_get::<Option<sqlx::types::Json<crate::payload::StateChip>>, _>("state")?
        .map(|chip| chip.0))
}

/// One row of a statement that selects a row's columns: `entity_id`,
/// `source_id`, `kind`, `title`, `item_updated_at`, `synced_at`, `path` and
/// `state`.
pub fn entity_row(row: &sqlx::postgres::PgRow) -> Result<EntityRow, sqlx::Error> {
    Ok(EntityRow {
        entity_id: row.get("entity_id"),
        kind: row.get("kind"),
        source_id: row.get("source_id"),
        title: row.get("title"),
        updated_at: row.get("item_updated_at"),
        synced_at: row.get("synced_at"),
        path: row.get("path"),
        state: state_of(row)?,
    })
}

/// Which slice of the corpus a room wants.
///
/// Both lists are *unfiltered when empty*, which is why they are bound as
/// `NULL` rather than as an empty array: `= any('{}')` matches nothing, and a
/// room whose filter said "no sources in particular" would come back empty.
///
/// Deserialize only: a filter is something the frontend *sends*. Its
/// TypeScript is generated from this struct (#1326), so a field added here is
/// a field the shell's type has; the `Serialize` behind `test-util` that let
/// the hand-written mirror's suite round-trip it went with that suite.
///
/// Do not "helpfully" send every known id instead of an empty list.
#[derive(Debug, Clone, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct EntityFilter {
    /// Source ids to include; empty means every source.
    pub sources: Vec<String>,
    /// Kinds to include; empty means every kind.
    pub kinds: Vec<String>,
    /// Only items the source dated within this many days. Items the source
    /// never dated are excluded by the window, because the window is about the
    /// source's own timestamp.
    pub updated_within_days: Option<u32>,
    /// Only members of this stored context (`ctx:<id>`), by the fixed one-hop
    /// rule (§16.11, ADR-0008). `None` is unscoped -- every derived room. The
    /// membership is resolved server-side per read rather than shipped as an
    /// id list, so a page and its `total` always describe the same instant.
    ///
    /// The fixed rule is the seed, its direct links, one hop, and what the
    /// assets among them hold.
    pub context: Option<String>,
    /// Only items carrying this project key, in the source's own word
    /// (ADR-0010, issue #208). `None` is unscoped, the same "empty means
    /// unfiltered" the two lists above have.
    ///
    /// It narrows **within** [`Self::sources`] and never instead of them: a
    /// project key is unique only inside its own source, so a project room
    /// sets both and a filter carrying the key alone would union two sources'
    /// `PAY` into one room. Where a key is read from is the **source's** to
    /// say since #277, and it is the same read the census behind the
    /// switcher's project rooms takes -- one macro over one declaration, so a
    /// room can never disagree with the list of rooms about what a project
    /// is.
    ///
    /// A record whose payload carries no readable project key matches no
    /// value of this field, so it is in no project room and still in *All
    /// work* and its source's room: absence, never a wrong room.
    pub project: Option<String>,
    pub order: EntityOrder,
    /// Whether to reach past `sync.live_item` for entities withdrawn upstream
    /// (§5a: links and notes may point at them).
    pub include_deleted: bool,
    /// Only what this ticket reaches (#817): the job-reach read
    /// (`crate::reach`), resolved server-side per read as
    /// [`Self::context`] is. `None` is unscoped. With a context as well, a
    /// row must be in both. Absent on the wire reads as `None`, so a caller
    /// written before #817 is unscoped as it was.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub reach: Option<String>,
    /// Only what this role's surface draws (#1294, ADR-0029 as amended on
    /// 2026-10-19): the rows of the sources that **fill** the role, which the
    /// mirror answers per read (`sources::crud::fillers`). With
    /// [`Self::sources`] as well, the fillers among them. With
    /// [`Self::kinds`] empty it also narrows to the role's kinds (CI's are
    /// the run kinds its fillers declare, #1344: a pipeline is the
    /// `pipelines` read's); with kinds named, to the role's among them. An unknown role has no fillers, so its page is
    /// empty, never an error. `None` is unscoped, and absent on the wire
    /// reads as `None`, as [`Self::reach`] does.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub role: Option<String>,
}

/// The orderings a room reads in: the two plain ones, and since #680 the two
/// tile orders.
///
/// - `updated_desc`: Newest first, by the source's own timestamp. Undated items
///   last.
/// - `title_asc`: Alphabetical by title.
/// - `attention`: **What needs attention first** (#680): a room's Code tile. A
///   row whose declared state is **settled** -- tone `ok` (done and good: a
///   merged pull request) or `muted` (over: a closed one) -- is not in the set.
///   What is left is ordered by its state's tone, loudest first (`err`, `warn`,
///   `info`), then rows with no state, each run newest first.
///
///   Driven by the declared state and its tone, never by a kind or an adapter:
///   a kind whose source declares no state, and a record whose declared state
///   misses, has no tone, so nothing hides it and it sorts by recency (§3a; the
///   ADR-0007 miss fails toward drawing the row).
/// - `latest_per_parent`: **One row per parent, its latest** (#680): a room's
///   Builds tile. Where a row's kind declares a state, its rows are grouped by
///   the parent chain their kind declares (ADR-0018) -- every name each parent
///   answers to, so two configurations sharing a display name stay two -- and
///   only the newest of each group is kept: a configuration's health is its
///   latest build. The kept rows are ordered by tone, every tone drawn (`err`,
///   `warn`, `info`, `ok`, `muted`, then no state), each run newest first.
///
///   A kind that declares no state is not grouped, and a row whose parent
///   misses stands alone: both fall back to recency (§3a), so a source that
///   never said what its builds' outcomes are draws every build.
///
/// Both tile orders read the live corpus only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "entity.ts")]
pub enum EntityOrder {
    UpdatedDesc,
    TitleAsc,
    Attention,
    LatestPerParent,
}

impl EntityOrder {
    /// Whether this is one of the room tiles' orders, which count what they
    /// draw ([`EntityPage::tally`]), read the live corpus only, and are
    /// planned afresh at every read (see [`read`]).
    fn is_tile_order(self) -> bool {
        matches!(self, EntityOrder::Attention | EntityOrder::LatestPerParent)
    }
}

/// How many rows of one kind, in one tone, a tile order drew (#680).
///
/// What a tile's header phrases ("2 open pull requests · 5 branches", "1
/// failing · 7 build configurations"), counted over the whole ordered set
/// rather than the page, as [`EntityPage::total`] is.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct KindTally {
    pub kind: String,
    /// The tone of the rows' state chip; `None` for rows that have none.
    pub tone: Option<crate::tone::Tone>,
    pub count: i64,
}

/// Which of the Code tile's groups a row is drawn under (#712): what needs
/// me first. Under [`EntityOrder::Attention`], in this order.
///
/// "Me" is the row's **own source's** configured username, trimmed, and
/// compared case-sensitively as the inbox and the standup compare it. A pull request
/// and a branch are knobas' own words (`0001_init.sql`), and the inbox's
/// review rule names `pr` the same way.
///
/// - `review_requested`: Open pull requests whose declared reviewers include me
///   (Gitea's `requested_reviewers`, as the inbox reads it). Whether I reviewed
///   since the last push is not in the mirror -- it holds no reviews and no
///   push times -- so #712's clause asking it is dropped.
/// - `my_pull_requests`: Open pull requests I opened.
/// - `open_pull_requests`: Every other open pull request.
/// - `my_branches`: Branches whose last commit is mine and that no open pull
///   request of the same repo is opened from (its declared `head`).
/// - `branches`: Branches of a source with no configured identity, in a room
///   where another source has one (ruled 2026-10-13 on #712): nobody can say
///   whose they are, so they are not hidden as somebody else's. The tile draws
///   them as it drew every branch before, newest first.
/// - `other_branches`: Every other branch: somebody else's, or one an open pull
///   request already stands for. The tile draws these only maximised.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "entity.ts")]
pub enum AttentionGroup {
    ReviewRequested,
    MyPullRequests,
    OpenPullRequests,
    MyBranches,
    Branches,
    OtherBranches,
}

/// How many rows of the set one [`AttentionGroup`] holds.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct GroupTally {
    pub group: AttentionGroup,
    pub count: i64,
}

/// One page of a room, plus the size of the set it was cut from.
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct EntityPage {
    pub rows: Vec<EntityRow>,
    /// The whole filtered set, before `limit`/`offset`. `0` for an empty page.
    pub total: i64,
    /// Under a tile order ([`EntityOrder::Attention`],
    /// [`EntityOrder::LatestPerParent`]), the set counted by kind and tone,
    /// in no particular order; `None` under the plain orders, whose header
    /// is `total` alone (#680).
    pub tally: Option<Vec<KindTally>>,
    /// Under [`EntityOrder::Attention`] in a room where some row's source has
    /// a configured identity, the set's non-empty [`AttentionGroup`]s in draw
    /// order, each with its count (#712). The rows arrive group by group, so
    /// a page read from offset 0 is cut into its groups by these counts.
    /// `None` under every other order and in a room nobody is "me" in, which
    /// draws as it did before.
    pub groups: Option<Vec<GroupTally>>,
}

/// The live corpus, newest first.
///
/// `count(*) over ()` is the unpaged total in the same round trip, and the
/// window is evaluated before `limit`, so it describes the set rather than the
/// page.
const LIVE_UPDATED: &str = concat!(
    r#"
select i.entity_id, i.source_id, i.kind, i.title, i.item_updated_at, i.synced_at,
       "#,
    crate::path_line_read!("$8"),
    r#" as path,
       "#,
    crate::declared_state!("$8"),
    r#" as state,
       count(*) over () as total
  from sync.live_item i
 where ($1::text[] is null or i.source_id = any($1))
   and ($2::text[] is null or i.kind      = any($2))
   and ($3::int    is null or i.item_updated_at >= now() - make_interval(days => $3))
   and ($6::text[] is null or i.entity_id = any($6))
   and ($7::text   is null or "#,
    crate::project_key_read!("$8"),
    r#" = $7)
 order by i.item_updated_at desc nulls last, i.entity_id
 limit $4 offset $5
"#
);

/// The live corpus, alphabetically. A second statement rather than an
/// interpolated `order by`.
const LIVE_TITLE: &str = concat!(
    r#"
select i.entity_id, i.source_id, i.kind, i.title, i.item_updated_at, i.synced_at,
       "#,
    crate::path_line_read!("$8"),
    r#" as path,
       "#,
    crate::declared_state!("$8"),
    r#" as state,
       count(*) over () as total
  from sync.live_item i
 where ($1::text[] is null or i.source_id = any($1))
   and ($2::text[] is null or i.kind      = any($2))
   and ($3::int    is null or i.item_updated_at >= now() - make_interval(days => $3))
   and ($6::text[] is null or i.entity_id = any($6))
   and ($7::text   is null or "#,
    crate::project_key_read!("$8"),
    r#" = $7)
 order by i.title asc, i.entity_id
 limit $4 offset $5
"#
);

/// As [`LIVE_UPDATED`], reaching past the tombstone filter.
///
/// The join is `sync.live_item` without its hiding reasons — `deleted_at is
/// null` and, since migration `0012`, the disabled-source filter. Both are
/// dropped on purpose: an `include_deleted` read lists a disabled source's
/// rows too, ratified in the #202 §10.8 entry.
const ALL_UPDATED: &str = concat!(
    r#"
select i.entity_id, i.source_id, i.kind, i.title, i.item_updated_at, i.synced_at,
       "#,
    crate::path_line_read!("$8"),
    r#" as path,
       "#,
    crate::declared_state!("$8"),
    r#" as state,
       count(*) over () as total
  from sync.item i
  join knobas.entity e on e.id = i.entity_id
 where ($1::text[] is null or i.source_id = any($1))
   and ($2::text[] is null or i.kind      = any($2))
   and ($3::int    is null or i.item_updated_at >= now() - make_interval(days => $3))
   and ($6::text[] is null or i.entity_id = any($6))
   and ($7::text   is null or "#,
    crate::project_key_read!("$8"),
    r#" = $7)
 order by i.item_updated_at desc nulls last, i.entity_id
 limit $4 offset $5
"#
);

/// As [`LIVE_TITLE`], reaching past the tombstone filter.
const ALL_TITLE: &str = concat!(
    r#"
select i.entity_id, i.source_id, i.kind, i.title, i.item_updated_at, i.synced_at,
       "#,
    crate::path_line_read!("$8"),
    r#" as path,
       "#,
    crate::declared_state!("$8"),
    r#" as state,
       count(*) over () as total
  from sync.item i
  join knobas.entity e on e.id = i.entity_id
 where ($1::text[] is null or i.source_id = any($1))
   and ($2::text[] is null or i.kind      = any($2))
   and ($3::int    is null or i.item_updated_at >= now() - make_interval(days => $3))
   and ($6::text[] is null or i.entity_id = any($6))
   and ($7::text   is null or "#,
    crate::project_key_read!("$8"),
    r#" = $7)
 order by i.title asc, i.entity_id
 limit $4 offset $5
"#
);

/// The live corpus a tile order starts from: the same five filters as
/// [`LIVE_UPDATED`], as the CTE `ruled`, with the four columns every tile
/// order reads and `$rule`, the one more column its rule needs, led by its
/// comma. [`EntityOrder::LatestPerParent`] needs none: it reads its parent
/// key from `sync.item_parent` (#688). `$join`, where given, joins what the
/// rule reads beside the row (#712: its source's identity).
///
/// **Only what the rule needs is computed here, per row of the room.** A
/// declared read is a walk of the declarations per row: on a room of 20,000
/// builds the state chip cost about 350 ms, the path line about 180 ms and
/// the parent key about 200 ms (the #685 review's measurement, on a loaded
/// machine), against 25 ms for the whole `updated_desc` page. The path
/// line is drawn for the page alone ([`tile_page!`]), and
/// [`EntityOrder::LatestPerParent`] reads the state of each group's newest
/// row only. [`EntityOrder::Attention`]'s groups (#712) are the exception
/// that pays: the reviewers are walked for a pull request whose source has
/// a username, and the path line for a branch, because those are the rows
/// the groups match; the Code tile's room holds only those two kinds.
///
/// Liveness is [`crate::live_by_exclusion!`], not the `sync.live_item`
/// join: on tables never analyzed the planner drove that join from
/// `knobas.entity`, at 0.5 % of its real size, and planned what followed from
/// that (#778).
///
/// `materialized`, so the rule's column is computed once a row. Inlined, the
/// CTE's column is an expression, and PostgreSQL evaluated it once for each
/// place the statement names it: the state twice under `attention`, 56 ms
/// where 25 ms will do (and the parent key twice, 450 ms where 300 ms would
/// do, while it was still walked per row).
macro_rules! tile_ruled {
    () => {
        tile_ruled!("")
    };
    ($rule:expr) => {
        tile_ruled!($rule, "")
    };
    ($rule:expr, $join:expr) => {
        concat!(
            r#"
with ruled as materialized (
  select i.entity_id, i.source_id, i.kind, i.item_updated_at"#,
            $rule,
            r#"
    from sync.item i"#,
            $join,
            r#"
   where "#,
            crate::live_by_exclusion!("i"),
            r#"
     and ($1::text[] is null or i.source_id = any($1))
     and ($2::text[] is null or i.kind      = any($2))
     and ($3::int    is null or i.item_updated_at >= now() - make_interval(days => $3))
     and ($6::text[] is null or i.entity_id = any($6))
     and ($7::text   is null or "#,
            crate::project_key_read!("$8"),
            r#" = $7)
)"#
        )
    };
}

/// A tile order's page, over the CTE `drawn` (the ordered set: `entity_id`,
/// `kind`, `item_updated_at`, `state` and its `tone`, and `grp`, the
/// 1-based [`AttentionGroup`] or `null`): the rows by group, then loudest
/// tone first, then newest first, with the set's total, its [`KindTally`]
/// and its [`GroupTally`]s beside every row. `null` groups are the whole set
/// of an ungrouped read, which then orders exactly as before #712. The page
/// is cut first and joined back to `sync.item`
/// for the rest of each row, so the path line is drawn for the page's rows
/// and no others.
///
/// The tone ranks `err`, `warn`, `info`, `ok`, `muted`, and a row with no
/// state after all five: the order a reader looking for what needs attention
/// reads them in (#680, `CONTEXT.md` **Tile**). The outer `order by` restates
/// the inner one, because a join keeps no order.
macro_rules! tile_page {
    () => {
        concat!(
            r#"
select i.entity_id, i.source_id, i.kind, i.title, i.item_updated_at, i.synced_at,
       "#,
            crate::path_line_read!("$8"),
            r#" as path,
       p.state, p.total, p.tally, p.groups
  from (select d.entity_id, d.item_updated_at, d.state, d.tone, d.grp,
               count(*) over () as total,
               (select jsonb_agg(jsonb_build_object('kind', t.kind, 'tone', t.tone, 'count', t.n))
                  from (select kind, tone, count(*) as n from drawn group by kind, tone) t) as tally,
               (select jsonb_agg(jsonb_build_object(
                         'group', (array['review_requested', 'my_pull_requests', 'open_pull_requests',
                                         'my_branches', 'branches', 'other_branches'])[g.grp],
                         'count', g.n) order by g.grp)
                  from (select grp, count(*) as n from drawn where grp is not null group by grp) g) as groups
          from drawn d
         order by d.grp nulls first,
                  coalesce(array_position(array['err', 'warn', 'info', 'ok', 'muted'], d.tone), 6),
                  d.item_updated_at desc nulls last, d.entity_id
         limit $4 offset $5) p
  join sync.item i on i.entity_id = p.entity_id
 order by p.grp nulls first,
          coalesce(array_position(array['err', 'warn', 'info', 'ok', 'muted'], p.tone), 6),
          p.item_updated_at desc nulls last, p.entity_id
"#
        )
    };
}

/// [`EntityOrder::Attention`]: the live corpus without its settled rows,
/// grouped by what needs me first (#712, [`AttentionGroup`]).
///
/// `me` is each row's own source's configured username (`config.username`,
/// the one the app's `identity_of` unions for the inbox), blank read as none. A row
/// whose source has none can be nobody's: its pull requests are other open
/// ones and its branches [`AttentionGroup::Branches`], drawn and not kept
/// for the maximised tile, so that source keeps what it showed before
/// (ruled 2026-10-13 on #712). A room where **no** row has one is
/// not grouped at all (`grp` is `null` throughout), which is the order the
/// tile drew before.
///
/// Per row, `ruled` adds the §4.1 `author`, the `title` (a branch's name),
/// whether my username is among the declared reviewers, a branch's path line
/// (its declared parent, Gitea's `owner/name`) and whether it is its repo's
/// declared `default_branch`. A repo's default branch never wants a pull
/// request, so it is dropped with the settled rows (#718): in no group and
/// no count, whoever's it is. A miss (no declared default) draws it as any
/// branch. `open_head` is every open pull request's declared `head` and
/// `head_repo` across the live corpus, not the room's: a context that holds
/// a branch and not its pull request must not call the branch unproposed.
/// Each is read once, `materialized`, as `ruled` is.
///
/// **The repo is matched by the pull request's declared `head_repo`** (#718:
/// a fork's pull request is opened from the fork, so its base repo's
/// same-named branch is not the one proposed) against the branch's path
/// line, the first name of its declared parent -- Gitea's `owner/name`,
/// unique per instance -- and not by every name it answers to as
/// `sync.item_parent` keeps them: a repo's bare `name` is shared across
/// owners. A miss on either side (no `head`, no `head_repo`, no parent)
/// matches nothing, so the branch stays **unproposed** and drawn: the
/// ADR-0007 miss fails toward showing a branch, never toward hiding it.
const LIVE_ATTENTION: &str = concat!(
    tile_ruled!(
        concat!(
            ",\n         ",
            crate::declared_state!("$8"),
            " as state, i.author, i.title, me.me,
         i.kind = 'pr' and me.me is not null and coalesce((select true from ",
            crate::declared_list!("$8", "reviewers"),
            " v where v.value = me.me limit 1), false) as asked,
         case when i.kind = 'branch' then ",
            crate::path_line_read!("$8"),
            " end as branch_path,
         i.kind = 'branch' and i.title = ",
            crate::declared_string!("$8", "default_branch"),
            " as is_default"
        ),
        "
    left join lateral (select nullif(btrim(s.config->>'username'), '') as me
                         from knobas.source_config s where s.id = i.source_id) me on true"
    ),
    r#",
open_head as materialized (
  select i.source_id, "#,
    crate::declared_string!("$8", "head"),
    r#" as head,
         "#,
    crate::declared_string!("$8", "head_repo"),
    r#" as head_repo
    from sync.live_item i
   where i.kind = 'pr'
     and i.source_id in (select r.source_id from ruled r where r.kind = 'branch' and r.me is not null)
     and coalesce("#,
    crate::declared_state!("$8"),
    r#"->>'tone', '') not in ('ok', 'muted')
),
drawn as (
  select r.entity_id, r.kind, r.item_updated_at, r.state, r.state->>'tone' as tone,
         case when bool_or(r.me is not null) over () then
           case
             when r.kind = 'pr' and r.me is not null and r.asked then 1
             when r.kind = 'pr' and r.author = r.me then 2
             when r.kind = 'pr' then 3
             when r.kind = 'branch' and r.author = r.me
                  and not exists (select 1 from open_head h
                                   where h.source_id = r.source_id and h.head = r.title
                                     and h.head_repo = r.branch_path) then 4
             when r.kind = 'branch' and r.me is null then 5
             else 6
           end
         end as grp
    from ruled r
   where (r.state is null or r.state->>'tone' not in ('ok', 'muted'))
     and r.is_default is not true
)"#,
    tile_page!(),
);

/// [`EntityOrder::LatestPerParent`]: the newest row of each parent, for kinds
/// that declare a state. The state chip is read for the kept rows only.
///
/// **The parent key is the stored resolution, not a walk** (#688). The
/// parents a row names are kept in `sync.item_parent` by the database
/// (migration `0028`, ADR-0019), from the same `declared_parent_names!`
/// expansion this statement used to run per row: on a room of 20,000 builds
/// that walk was about 300 ms of a 310 ms read, against 25 ms for
/// `updated_desc`. The key reads a row's **child-arm** rows (the parents it
/// names, not the names it answers to as a parent) under its **own source's
/// adapter**, matched at read time on `knobas.source_config` as every reader
/// of the table matches it.
///
/// The stored rows are a set: each name and its parent's kind, but not the
/// chain's order nor which names belong to which parent. So the key is the
/// sorted `(parent kind, name)` pairs, joined by control characters
/// (`\x1c`-`\x1f`) that a synced name is not expected to hold; nothing
/// enforces it, and a name that did could join two keys.
/// Two configurations sharing a display name stay two, because each also
/// names its own id (#680's two configurations called `Build`). What the set
/// cannot tell apart is two chains with the same pairs arranged differently:
/// a configuration named `A` with id `B` beside one named `B` with id `A`.
///
/// No key -- no grouping -- for a kind that declares no state (asked once per
/// source and kind, in `grouped`, not per row; its subquery is aliased `i`
/// because `declares_field!` reads `i.source_id` and `i.kind`) and for a
/// record that names no parent; such a row stands alone under its own id.
///
/// Whether a kind declares a state is read from `$8`; the parents from the
/// stored copy of the declarations. Both are the app binary's
/// `Registry::for_process()`, which bring-up writes to the database before its
/// first sync (`knobas-app`'s `commands::search::searcher`), so they agree. If that write
/// failed (bring-up logs it and goes on), no row has a key and every build
/// draws alone: the tile shows too many rows, never too few.
///
/// The statement's shape follows its `explain analyze` on that room. `named`
/// is materialized and bounded by the room's sources -- `$1` repeated there,
/// redundant with `grouped` in logic, is what lets the planner use
/// `item_parent_child_idx` -- rather than probed once per row of the room,
/// which a nested loop did at about 7 µs a probe. Every key and the group
/// column compare in the `"C"` collation: a sort of 40,000 names in the
/// database's own collation was most of the remaining time. The kept rows
/// are fetched by primary key. Measured on the 20,000-build room: about
/// 80 ms.
///
/// **No join between the CTEs** (#778). `keyed` and `latest` used to join
/// `named` to `ruled` and `ruled` to `keyed`. On tables never analyzed the
/// planner estimated each CTE at one row and joined them by nested loops,
/// re-aggregating `keyed` once per build: 6.8 s for the probe's 5,000 builds,
/// against 23 ms analyzed. Now each row's parent key is found by aggregation:
/// `ruled`'s rows and `named`'s are unioned and grouped by entity, and only
/// the rows `ruled` contributed are kept. Whether a row's source and kind are
/// grouped at all is a membership test against `grouped`, a handful of rows,
/// that cannot become a join. Nothing left in the statement is planned from a
/// row estimate. `ruled` reads liveness by [`crate::live_by_exclusion!`]
/// rather than through `sync.live_item` (see [`tile_ruled!`]).
const LIVE_LATEST: &str = concat!(
    tile_ruled!(),
    r#",
grouped as (
  select i.source_id, i.kind
    from (select distinct r.source_id, r.kind from ruled r) i
   where "#,
    crate::declares_field!("$8", "state"),
    r#"
),
named as materialized (
  select ip.entity_id, ip.kind, ip.name
    from sync.item_parent ip
    join knobas.source_config s on s.id = ip.source_id and s.kind = ip.adapter_kind
   where ip.arm = 'child'
     and ($1::text[] is null or ip.source_id = any($1))
     and ip.source_id in (select g.source_id from grouped g)
),
keyed as (
  select u.entity_id,
         max(u.source_id) as source_id, max(u.kind) as kind,
         max(u.item_updated_at) as item_updated_at, bool_or(u.ruled) as ruled,
         string_agg(u.parent_kind || E'\x1f' || u.name, E'\x1e'
                    order by u.parent_kind collate "C", u.name collate "C") as parent_key
    from (select r.entity_id, r.source_id, r.kind, r.item_updated_at, true as ruled,
                 null::text as parent_kind, null::text as name
            from ruled r
          union all
          select n.entity_id, null, null, null, false, n.kind, n.name
            from named n) u
   group by u.entity_id
),
latest as (
  select distinct on (g.grp) g.entity_id
    from (select k.entity_id, k.item_updated_at,
                 coalesce(case when ((k.source_id, k.kind) in (select g.source_id, g.kind from grouped g)) is true
                               then k.source_id || E'\x1d' || k.kind || E'\x1d' || k.parent_key end,
                          E'\x1c' || k.entity_id) collate "C" as grp
            from keyed k
           where k.ruled) g
   order by g.grp, g.item_updated_at desc nulls last, g.entity_id
),
drawn as (
  select k.entity_id, k.kind, k.item_updated_at, k.state, k.state->>'tone' as tone,
         null::int as grp
    from (select i.entity_id, i.kind, i.item_updated_at,
                 "#,
    crate::declared_state!("$8"),
    r#" as state
            from sync.item i
           where i.entity_id = any(array(select l.entity_id from latest l))) k
)"#,
    tile_page!(),
);

/// [`EntityOrder::LatestPerParent`]'s statement, for the plan test
/// (`knobas-app`'s `tests/it/read_plans.rs`, #778). It binds what [`read`]
/// binds.
#[doc(hidden)]
#[must_use]
pub fn planned_latest_per_parent() -> &'static str {
    LIVE_LATEST
}

/// The statement this filter reads through. Six constants, one `match`.
///
/// # Errors
///
/// [`CoreError::Invalid`] for a tile order that asks for
/// the withdrawn too: a tile draws the live corpus, and the withdrawn are the
/// detail's way in (§5a).
fn statement(filter: &EntityFilter) -> Result<&'static str, CoreError> {
    Ok(match (filter.include_deleted, filter.order) {
        (false, EntityOrder::UpdatedDesc) => LIVE_UPDATED,
        (false, EntityOrder::TitleAsc) => LIVE_TITLE,
        (true, EntityOrder::UpdatedDesc) => ALL_UPDATED,
        (true, EntityOrder::TitleAsc) => ALL_TITLE,
        (false, EntityOrder::Attention) => LIVE_ATTENTION,
        (false, EntityOrder::LatestPerParent) => LIVE_LATEST,
        (true, order) => {
            return Err(CoreError::Invalid(format!(
                "the {order:?} order reads the live corpus only"
            )));
        }
    })
}

/// An empty list is *no filter*, which on the wire is `NULL`.
fn any_of(values: &[String]) -> Option<Vec<String>> {
    (!values.is_empty()).then(|| values.to_vec())
}

/// `scope` where nothing is `named`, else the values of `scope` that are.
fn among(scope: Vec<String>, named: &[String]) -> Vec<String> {
    if named.is_empty() {
        scope
    } else {
        scope.into_iter().filter(|v| named.contains(v)).collect()
    }
}

/// What a read asked for a role is scoped to (#1294, ADR-0029 as amended on
/// 2026-10-19): the sources that fill it, and the kinds that are its.
///
/// Resolved by the caller, which holds the adapter registry the claims come
/// from (the app's `sources::crud::role_scope`), and handed to [`read`] as
/// the declarations are.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RoleScope {
    /// The sources filling the role, id order.
    pub sources: Vec<String>,
    /// The role's kinds among its fillers, sorted.
    pub kinds: Vec<String>,
}

/// One page of the room addressed by `filter`.
///
/// `fillers` is the scope of the filter's role ([`EntityFilter::role`]):
/// who fills a role is the adapter registry's answer, which the caller holds,
/// so it resolves the scope and passes it in as it passes the declarations.
/// It is read only where the filter names a role, and a role named with no
/// fillers passed reads as a role nobody fills: an empty page, never an
/// unscoped one.
///
/// # Errors
///
/// [`CoreError::Invalid`] for an `updated_within_days` past `i32::MAX` and
/// for a tile order that asks for the withdrawn; [`CoreError::Db`] if a query
/// fails.
pub async fn read(
    pool: &PgPool,
    filter: &EntityFilter,
    limit: u32,
    offset: u32,
    declarations: &crate::payload::Declarations,
    fillers: Option<RoleScope>,
) -> Result<EntityPage, CoreError> {
    // `u32 -> i32` and not `as`: `updated_within_days` past `i32::MAX` is a
    // caller error, and silently wrapping it into a negative interval would
    // turn "the last four billion days" into "the next few".
    let within =
        match filter.updated_within_days {
            Some(days) => Some(i32::try_from(days).map_err(|_| {
                CoreError::Invalid(format!("updated_within_days {days} is too large"))
            })?),
            None => None,
        };

    // Membership is a set of ids, resolved by the store that owns the rule
    // (§16.11): the statements above stay static SQL, and a stored context
    // scopes them by one more nullable parameter. `Some` of an empty set is a
    // context with no members, whose room is honestly empty -- not unscoped.
    let members = match filter.context.as_deref() {
        Some(ctx) => Some(crate::context::member_ids(pool, ctx).await?),
        None => None,
    };
    // What a ticket reaches is a set of ids too (#817), and it narrows the
    // same parameter: a job's sections read their rows through it.
    let members = match filter.reach.as_deref() {
        Some(ticket) => {
            let reached: Vec<String> = crate::reach::reach(pool, ticket)
                .await?
                .into_iter()
                .map(|row| row.entity_id)
                .collect();
            Some(match members {
                Some(held) => reached.into_iter().filter(|id| held.contains(id)).collect(),
                None => reached,
            })
        }
        None => members,
    };

    // A role narrows the two lists every statement already binds (#1294):
    // its fillers among the named sources, its kinds among the named kinds.
    // `Some` of an empty list is a role nobody fills, whose page is honestly
    // empty -- not unscoped, which is what an empty list alone would mean.
    let (sources, kinds) = match filter.role.as_deref() {
        Some(_) => {
            let scope = fillers.unwrap_or_default();
            (
                Some(among(scope.sources, &filter.sources)),
                Some(among(scope.kinds, &filter.kinds)),
            )
        }
        None => (any_of(&filter.sources), any_of(&filter.kinds)),
    };

    let rows = sqlx::query(statement(filter)?)
        // A tile order is never a named prepared statement (#685's review).
        // PostgreSQL custom-plans a named one for five executions and may
        // then go generic, and a generic plan cannot fold the nullable
        // filters (`$7 is null or ...`): it estimates the room at one row and
        // joins `knobas.entity` by a nested loop with a scan per row. On a
        // 20,000-build room the Builds tile went from 0.45 s to 11-12 s at
        // the sixth read of a connection. The unnamed statement is planned at
        // Bind with the values in hand, for about 2 ms a read --
        // `knobas_search::sql::query_as_with`'s remedy for the same switch.
        // The plain orders' plans survive going generic, so they keep theirs.
        .persistent(!filter.order.is_tile_order())
        .bind(sources)
        .bind(kinds)
        .bind(within)
        .bind(i64::from(limit))
        .bind(i64::from(offset))
        .bind(members)
        .bind(filter.project.as_deref())
        .bind(declarations.as_param())
        .fetch_all(pool)
        .await?;

    // Off the first row, because the window function is per-row and every row
    // carries the same value. An empty page has no row to carry it, and 0 is
    // the truth for a page past the end of an empty set as much as for one
    // past the end of a full one -- the count the caller can act on is the one
    // it can see.
    let total: i64 = rows.first().map_or(0, |row| row.get("total"));
    // The tally rides on every row for the same reason; an empty page under
    // a tile order is an empty set, which counts nothing.
    let tally = if filter.order.is_tile_order() {
        Some(match rows.first() {
            Some(row) => row
                .try_get::<Option<sqlx::types::Json<Vec<KindTally>>>, _>("tally")?
                .map(|tally| tally.0)
                .unwrap_or_default(),
            None => Vec::new(),
        })
    } else {
        None
    };

    // Under `attention` alone, and off the first row like the tally.
    let groups = match (filter.order, rows.first()) {
        (EntityOrder::Attention, Some(row)) => row
            .try_get::<Option<sqlx::types::Json<Vec<GroupTally>>>, _>("groups")?
            .map(|groups| groups.0),
        _ => None,
    };

    Ok(EntityPage {
        rows: rows.iter().map(entity_row).collect::<Result<_, _>>()?,
        total,
        tally,
        groups,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A filter written before #817 carries no `reach`, and one written
    /// before #1294 no `role`: both read as unscoped. The generated
    /// TypeScript declares both optional (`reach?`, `role?`), and the shell
    /// leaves them out; this is the half of that promise the Rust keeps.
    /// Moved here from the hand-written mirror's suite, deleted by #1326.
    #[test]
    fn a_filter_without_reach_or_role_is_unscoped() {
        let decoded: EntityFilter = serde_json::from_value(serde_json::json!({
            "sources": [], "kinds": [], "updated_within_days": null, "order": "updated_desc",
            "context": null, "project": null, "include_deleted": false,
        }))
        .unwrap();
        assert_eq!(decoded.reach, None);
        assert_eq!(decoded.role, None);
    }

    /// The six statements are six, and each names its columns.
    ///
    /// This reads the constants themselves -- the strings that are sent to
    /// PostgreSQL -- not the source file that spells them, so it is an
    /// assertion about the statement rather than about how it was written.
    #[test]
    fn every_statement_names_its_columns_and_no_two_are_the_same() {
        let all = [
            LIVE_UPDATED,
            LIVE_TITLE,
            ALL_UPDATED,
            ALL_TITLE,
            LIVE_ATTENTION,
            LIVE_LATEST,
        ];
        for sql in all {
            assert!(
                !sql.contains("select *"),
                "`sync.live_item.fts` is a tsvector: name the columns (gotcha 2)"
            );
            assert!(sql.contains("count(*) over ()"), "the unpaged total");
        }
        let distinct = all.iter().collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            distinct.len(),
            6,
            "an ordering or a scope shares a statement"
        );
    }

    /// Every combination reaches its own statement, and a tile order asked
    /// for the withdrawn is refused (#680).
    #[test]
    fn every_combination_chooses_its_statement() {
        let filter = |include_deleted, order| EntityFilter {
            sources: Vec::new(),
            kinds: Vec::new(),
            updated_within_days: None,
            context: None,
            project: None,
            order,
            include_deleted,
            reach: None,
            role: None,
        };
        let chosen = |include_deleted, order| statement(&filter(include_deleted, order)).ok();
        assert_eq!(chosen(false, EntityOrder::UpdatedDesc), Some(LIVE_UPDATED));
        assert_eq!(chosen(false, EntityOrder::TitleAsc), Some(LIVE_TITLE));
        assert_eq!(chosen(true, EntityOrder::UpdatedDesc), Some(ALL_UPDATED));
        assert_eq!(chosen(true, EntityOrder::TitleAsc), Some(ALL_TITLE));
        assert_eq!(chosen(false, EntityOrder::Attention), Some(LIVE_ATTENTION));
        assert_eq!(
            chosen(false, EntityOrder::LatestPerParent),
            Some(LIVE_LATEST)
        );
        assert_eq!(chosen(true, EntityOrder::Attention), None);
        assert_eq!(chosen(true, EntityOrder::LatestPerParent), None);
    }

    /// An empty list is not an empty array.
    #[test]
    fn an_empty_list_binds_as_null() {
        assert_eq!(any_of(&[]), None);
        assert_eq!(any_of(&["jira".to_owned()]), Some(vec!["jira".to_owned()]));
    }

    /// The frontend sends `"updated_desc"`; a rename here is a room that
    /// cannot be ordered.
    #[test]
    fn the_order_deserializes_as_the_frontend_spells_it() {
        assert_eq!(
            serde_json::from_str::<EntityOrder>("\"updated_desc\"").unwrap(),
            EntityOrder::UpdatedDesc
        );
        assert_eq!(
            serde_json::from_str::<EntityOrder>("\"title_asc\"").unwrap(),
            EntityOrder::TitleAsc
        );
        assert!(serde_json::from_str::<EntityOrder>("\"UpdatedDesc\"").is_err());
    }
}

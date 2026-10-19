//! Entity and room reads, and the two link writes -- stream D
//! (interfaces §2.5).
//!
//! Its `State<'_, Lifecycle>` is not an accident and is not stream D being
//! tidy: carry-over §10.6(a). `AppState` exists only once PostgreSQL is up,
//! and a `#[tauri::command]` resolves every argument *before* its body runs,
//! so a command declaring `State<'_, AppState>` is rejected by Tauri itself
//! during bring-up with the bare string `"state not managed"` -- no code for
//! the frontend to branch on. `Lifecycle` is managed at build time and is
//! always there; `lifecycle.pool()?` is the single place `not_ready` comes
//! from.
//!
//! ## Why there is no query builder here
//!
//! Roadmap §4 gotcha 2 asks for dynamic SQL to live in one reviewed module
//! behind `AssertSqlSafe`. This module dodges the question instead: every
//! optional predicate is a **nullable parameter** (`$1::text[] is null or ...`)
//! and every ordering and scope is its own `const` statement chosen by a
//! `match` (six since #680's two tile orders), so
//! nothing here ever concatenates a string into SQL and `sqlx::query` is only
//! ever handed a `&'static str`. A filter that grows a new optional predicate
//! adds a parameter, not a branch.
//!
//! ## Why the columns are named
//!
//! `sync.live_item` exposes `fts`, a `tsvector` (migration `0002`'s own
//! WARNING). Naming the six columns each statement wants keeps that value out
//! of the result set entirely rather than relying on nobody ever asking for it
//! by name.

use chrono::{DateTime, Utc};
use knobas_core::activity::ActivityRow;
use knobas_core::entity::EntityRef;
use knobas_core::link::{LinkEntry, LinkRow, Origin};
use knobas_core::suggest::{self, SuggestionEntry};
use sqlx::{PgPool, Row};
use tauri::State;
use uuid::Uuid;

use crate::{IpcError, Lifecycle};

/// One line in a room: what a tile draws, and nothing more.
///
/// Deliberately not the mirror row. `body_text` and `payload` are the detail
/// view's business ([`EntityDetail`]), and a room that fetched them would pull
/// the whole corpus into the webview to render five columns of it.
#[derive(Debug, Clone, serde::Serialize)]
pub struct EntityRow {
    pub entity_id: String,
    pub kind: String,
    pub source_id: String,
    pub title: String,
    /// When the *source* says the item changed.
    ///
    /// `None` where the source never said (interfaces §4.1 normalization):
    /// the mirror keeps the hole rather than filling it with `now()`, so a row
    /// that has never been dated is distinguishable from one changed this
    /// second.
    pub updated_at: Option<DateTime<Utc>>,
    /// When knobas last saw it. Always known -- it is knobas' own clock.
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
    /// `knobas_core::path_line_read!`, the one expression that spells it, so
    /// this row and the launcher's cannot disagree about what a path is.
    ///
    /// This struct and `knobas_search::EntityRow` are pinned to **one wire
    /// shape** by `commands::search`'s own test; the field is on both.
    pub path: Option<String>,
    /// The item's state chip -- a pull request's `open` / `merged` /
    /// `closed`, a build's `running` or outcome (#666) -- resolved by
    /// `knobas_core::declared_state!` from the state the row's kind declares,
    /// beside the path line and wherever it is read. `None` for a kind that
    /// declares none and for a value the declaration does not name: the
    /// ADR-0007 **miss**, so a row shows no chip rather than a wrong one.
    ///
    /// On both `EntityRow`s, pinned to one wire shape like `path`.
    pub state: Option<knobas_core::payload::StateChip>,
}

/// The `state` column every row statement selects, decoded.
///
/// # Errors
///
/// The decode error, for a `jsonb` that is not a chip. `declared_state!`
/// builds it from a declaration whose tones are typed, so that is a bug, and
/// it fails the read rather than reading as a row with no state.
fn state_of(
    row: &sqlx::postgres::PgRow,
) -> Result<Option<knobas_core::payload::StateChip>, sqlx::Error> {
    Ok(row
        .try_get::<Option<sqlx::types::Json<knobas_core::payload::StateChip>>, _>("state")?
        .map(|chip| chip.0))
}

/// One row of a statement that selects a row's columns: `entity_id`,
/// `source_id`, `kind`, `title`, `item_updated_at`, `synced_at`, `path` and
/// `state`.
fn entity_row(row: &sqlx::postgres::PgRow) -> Result<EntityRow, sqlx::Error> {
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
/// `Serialize` behind `test-util`, although nothing sends a filter the other
/// way: it is what lets `tests/it/entity_mirror.rs` pin an *input* DTO through a
/// round trip, and so see a field this struct has that
/// `app/src/lib/ipc/entity.ts` never declares. A decode-only check is blind to
/// that direction -- serde reads a missing `Option` as `None`, so a Rust-only
/// optional field decodes clean and the mirror never has to declare it.
///
/// Gated rather than unconditional, for the reason the feature exists at all
/// (see `Cargo.toml`): a filter is something the frontend *sends*, and a
/// production build that can also write one invites a caller to round-trip a
/// command argument through nothing. `cargo test` turns the feature on through
/// this crate's self-dev-dependency; `tauri build` and the `clippy --lib` half
/// of the gate compile without it.
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(feature = "test-util", derive(serde::Serialize))]
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
    /// (`knobas_core::reach`), resolved server-side per read as
    /// [`Self::context`] is. `None` is unscoped. With a context as well, a
    /// row must be in both. Absent on the wire reads as `None`, so a caller
    /// written before #817 is unscoped as it was.
    #[serde(default)]
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
    pub role: Option<String>,
}

/// The orderings a room reads in: the two plain ones, and since #680 the two
/// tile orders.
///
/// `Serialize` behind `test-util` for the same reason [`EntityFilter`] carries
/// it: it rides inside the filter's round trip, and pins this union against the
/// mirror's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[cfg_attr(feature = "test-util", derive(serde::Serialize))]
#[serde(rename_all = "snake_case")]
pub enum EntityOrder {
    /// Newest first, by the source's own timestamp. Undated items last.
    UpdatedDesc,
    /// Alphabetical by title.
    TitleAsc,
    /// **What needs attention first** (#680): a room's Code tile. A row whose
    /// declared state is **settled** -- tone `ok` (done and good: a merged
    /// pull request) or `muted` (over: a closed one) -- is not in the set.
    /// What is left is ordered by its state's tone, loudest first (`err`,
    /// `warn`, `info`), then rows with no state, each run newest first.
    ///
    /// Driven by the declared state and its tone, never by a kind or an
    /// adapter: a kind whose source declares no state, and a record whose
    /// declared state misses, has no tone, so nothing hides it and it sorts
    /// by recency (§3a; the ADR-0007 miss fails toward drawing the row).
    Attention,
    /// **One row per parent, its latest** (#680): a room's Builds tile. Where
    /// a row's kind declares a state, its rows are grouped by the parent
    /// chain their kind declares (ADR-0018) -- every name each parent answers
    /// to, so two configurations sharing a display name stay two -- and only
    /// the newest of each group is kept: a configuration's health is its
    /// latest build. The kept rows are ordered by tone, every tone drawn
    /// (`err`, `warn`, `info`, `ok`, `muted`, then no state), each run newest
    /// first.
    ///
    /// A kind that declares no state is not grouped, and a row whose parent
    /// misses stands alone: both fall back to recency (§3a), so a source
    /// that never said what its builds' outcomes are draws every build.
    LatestPerParent,
}

impl EntityOrder {
    /// Whether this is one of the room tiles' orders, which count what they
    /// draw ([`EntityPage::tally`]), read the live corpus only, and are
    /// planned afresh at every read (see [`list_entities_inner`]).
    fn is_tile_order(self) -> bool {
        matches!(self, EntityOrder::Attention | EntityOrder::LatestPerParent)
    }
}

/// How many rows of one kind, in one tone, a tile order drew (#680).
///
/// What a tile's header phrases ("2 open pull requests · 5 branches", "1
/// failing · 7 build configurations"), counted over the whole ordered set
/// rather than the page, as [`EntityPage::total`] is.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct KindTally {
    pub kind: String,
    /// The tone of the rows' state chip; `None` for rows that have none.
    pub tone: Option<knobas_core::tone::Tone>,
    pub count: i64,
}

/// Which of the Code tile's groups a row is drawn under (#712): what needs
/// me first. Under [`EntityOrder::Attention`], in this order.
///
/// "Me" is the row's **own source's** configured username, trimmed, and
/// compared case-sensitively as the inbox and the standup compare it. A pull request
/// and a branch are knobas' own words (`0001_init.sql`), and the inbox's
/// review rule names `pr` the same way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttentionGroup {
    /// Open pull requests whose declared reviewers include me (Gitea's
    /// `requested_reviewers`, as the inbox reads it). Whether I reviewed
    /// since the last push is not in the mirror -- it holds no reviews and
    /// no push times -- so #712's clause asking it is dropped.
    ReviewRequested,
    /// Open pull requests I opened.
    MyPullRequests,
    /// Every other open pull request.
    OpenPullRequests,
    /// Branches whose last commit is mine and that no open pull request of
    /// the same repo is opened from (its declared `head`).
    MyBranches,
    /// Branches of a source with no configured identity, in a room where
    /// another source has one (ruled 2026-10-13 on #712): nobody can say
    /// whose they are, so they are not hidden as somebody else's. The tile
    /// draws them as it drew every branch before, newest first.
    Branches,
    /// Every other branch: somebody else's, or one an open pull request
    /// already stands for. The tile draws these only maximised.
    OtherBranches,
}

/// How many rows of the set one [`AttentionGroup`] holds.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GroupTally {
    pub group: AttentionGroup,
    pub count: i64,
}

/// One page of a room, plus the size of the set it was cut from.
#[derive(Debug, Clone, serde::Serialize)]
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
    knobas_core::path_line_read!("$8"),
    r#" as path,
       "#,
    knobas_core::declared_state!("$8"),
    r#" as state,
       count(*) over () as total
  from sync.live_item i
 where ($1::text[] is null or i.source_id = any($1))
   and ($2::text[] is null or i.kind      = any($2))
   and ($3::int    is null or i.item_updated_at >= now() - make_interval(days => $3))
   and ($6::text[] is null or i.entity_id = any($6))
   and ($7::text   is null or "#,
    knobas_core::project_key_read!("$8"),
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
    knobas_core::path_line_read!("$8"),
    r#" as path,
       "#,
    knobas_core::declared_state!("$8"),
    r#" as state,
       count(*) over () as total
  from sync.live_item i
 where ($1::text[] is null or i.source_id = any($1))
   and ($2::text[] is null or i.kind      = any($2))
   and ($3::int    is null or i.item_updated_at >= now() - make_interval(days => $3))
   and ($6::text[] is null or i.entity_id = any($6))
   and ($7::text   is null or "#,
    knobas_core::project_key_read!("$8"),
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
    knobas_core::path_line_read!("$8"),
    r#" as path,
       "#,
    knobas_core::declared_state!("$8"),
    r#" as state,
       count(*) over () as total
  from sync.item i
  join knobas.entity e on e.id = i.entity_id
 where ($1::text[] is null or i.source_id = any($1))
   and ($2::text[] is null or i.kind      = any($2))
   and ($3::int    is null or i.item_updated_at >= now() - make_interval(days => $3))
   and ($6::text[] is null or i.entity_id = any($6))
   and ($7::text   is null or "#,
    knobas_core::project_key_read!("$8"),
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
    knobas_core::path_line_read!("$8"),
    r#" as path,
       "#,
    knobas_core::declared_state!("$8"),
    r#" as state,
       count(*) over () as total
  from sync.item i
  join knobas.entity e on e.id = i.entity_id
 where ($1::text[] is null or i.source_id = any($1))
   and ($2::text[] is null or i.kind      = any($2))
   and ($3::int    is null or i.item_updated_at >= now() - make_interval(days => $3))
   and ($6::text[] is null or i.entity_id = any($6))
   and ($7::text   is null or "#,
    knobas_core::project_key_read!("$8"),
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
/// Liveness is [`knobas_core::live_by_exclusion!`], not the `sync.live_item`
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
            knobas_core::live_by_exclusion!("i"),
            r#"
     and ($1::text[] is null or i.source_id = any($1))
     and ($2::text[] is null or i.kind      = any($2))
     and ($3::int    is null or i.item_updated_at >= now() - make_interval(days => $3))
     and ($6::text[] is null or i.entity_id = any($6))
     and ($7::text   is null or "#,
            knobas_core::project_key_read!("$8"),
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
            knobas_core::path_line_read!("$8"),
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
/// the one [`identity_of`] unions for the inbox), blank read as none. A row
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
            knobas_core::declared_state!("$8"),
            " as state, i.author, i.title, me.me,
         i.kind = 'pr' and me.me is not null and coalesce((select true from ",
            knobas_core::declared_list!("$8", "reviewers"),
            " v where v.value = me.me limit 1), false) as asked,
         case when i.kind = 'branch' then ",
            knobas_core::path_line_read!("$8"),
            " end as branch_path,
         i.kind = 'branch' and i.title = ",
            knobas_core::declared_string!("$8", "default_branch"),
            " as is_default"
        ),
        "
    left join lateral (select nullif(btrim(s.config->>'username'), '') as me
                         from knobas.source_config s where s.id = i.source_id) me on true"
    ),
    r#",
open_head as materialized (
  select i.source_id, "#,
    knobas_core::declared_string!("$8", "head"),
    r#" as head,
         "#,
    knobas_core::declared_string!("$8", "head_repo"),
    r#" as head_repo
    from sync.live_item i
   where i.kind = 'pr'
     and i.source_id in (select r.source_id from ruled r where r.kind = 'branch' and r.me is not null)
     and coalesce("#,
    knobas_core::declared_state!("$8"),
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
/// stored copy of the declarations. Both are this binary's
/// `Registry::builtin()`, which bring-up writes to the database before its
/// first sync (`commands::search::searcher`), so they agree. If that write
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
/// row estimate. `ruled` reads liveness by [`knobas_core::live_by_exclusion!`]
/// rather than through `sync.live_item` (see [`tile_ruled!`]).
const LIVE_LATEST: &str = concat!(
    tile_ruled!(),
    r#",
grouped as (
  select i.source_id, i.kind
    from (select distinct r.source_id, r.kind from ruled r) i
   where "#,
    knobas_core::declares_field!("$8", "state"),
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
    knobas_core::declared_state!("$8"),
    r#" as state
            from sync.item i
           where i.entity_id = any(array(select l.entity_id from latest l))) k
)"#,
    tile_page!(),
);

/// [`EntityOrder::LatestPerParent`]'s statement, for the plan test
/// (`tests/it/read_plans.rs`, #778). It binds what [`list_entities_inner`]
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
/// [`Invalid`](crate::IpcErrorCode::Invalid) for a tile order that asks for
/// the withdrawn too: a tile draws the live corpus, and the withdrawn are the
/// detail's way in (§5a).
fn statement(filter: &EntityFilter) -> Result<&'static str, IpcError> {
    Ok(match (filter.include_deleted, filter.order) {
        (false, EntityOrder::UpdatedDesc) => LIVE_UPDATED,
        (false, EntityOrder::TitleAsc) => LIVE_TITLE,
        (true, EntityOrder::UpdatedDesc) => ALL_UPDATED,
        (true, EntityOrder::TitleAsc) => ALL_TITLE,
        (false, EntityOrder::Attention) => LIVE_ATTENTION,
        (false, EntityOrder::LatestPerParent) => LIVE_LATEST,
        (true, order) => {
            return Err(IpcError::invalid(format!(
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

/// One page of the room addressed by `filter`.
///
/// The behaviour lives here rather than in the command so it is reachable from
/// a test: a `#[tauri::command]` cannot be called directly.
///
/// A role ([`EntityFilter::role`]) is answered by the process's registry, as
/// the command's declarations are; [`list_entities_with_registry`] takes the
/// registry from its caller.
///
/// # Errors
///
/// [`IpcError`] with [`Internal`](crate::IpcErrorCode::Internal) if the query
/// fails.
pub async fn list_entities_inner(
    pool: &PgPool,
    filter: &EntityFilter,
    limit: u32,
    offset: u32,
    declarations: &knobas_core::payload::Declarations,
) -> Result<EntityPage, IpcError> {
    list_entities_with_registry(
        pool,
        &crate::sources::Registry::builtin(),
        filter,
        limit,
        offset,
        declarations,
    )
    .await
}

/// [`list_entities_inner`], answering a role from `registry`'s claims.
///
/// # Errors
///
/// As [`list_entities_inner`].
pub async fn list_entities_with_registry(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    filter: &EntityFilter,
    limit: u32,
    offset: u32,
    declarations: &knobas_core::payload::Declarations,
) -> Result<EntityPage, IpcError> {
    // `u32 -> i32` and not `as`: `updated_within_days` past `i32::MAX` is a
    // caller error, and silently wrapping it into a negative interval would
    // turn "the last four billion days" into "the next few".
    let within =
        match filter.updated_within_days {
            Some(days) => Some(i32::try_from(days).map_err(|_| {
                IpcError::invalid(format!("updated_within_days {days} is too large"))
            })?),
            None => None,
        };

    // Membership is a set of ids, resolved by the store that owns the rule
    // (§16.11): the statements above stay static SQL, and a stored context
    // scopes them by one more nullable parameter. `Some` of an empty set is a
    // context with no members, whose room is honestly empty -- not unscoped.
    let members = match filter.context.as_deref() {
        Some(ctx) => Some(knobas_core::context::member_ids(pool, ctx).await?),
        None => None,
    };
    // What a ticket reaches is a set of ids too (#817), and it narrows the
    // same parameter: a job's sections read their rows through it.
    let members = match filter.reach.as_deref() {
        Some(ticket) => {
            let reached: Vec<String> = knobas_core::reach::reach(pool, ticket)
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
        Some(role) => {
            let scope = crate::sources::crud::role_scope(pool, registry, role).await?;
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

/// One page of a room.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the
/// database is still coming up, [`Internal`](crate::IpcErrorCode::Internal)
/// for a query failure.
#[tauri::command]
pub async fn list_entities(
    lifecycle: State<'_, Lifecycle>,
    filter: EntityFilter,
    limit: u32,
    offset: u32,
) -> Result<EntityPage, IpcError> {
    let pool = lifecycle.pool()?;
    let declarations = declared_paths(&pool).await?;
    list_entities_inner(&pool, &filter, limit, offset, &declarations).await
}

/// Every configured source's declared payload paths (#277).
///
/// **`Registry::builtin()` rather than `SourcesState`'s trait object**, for the
/// reason `Registry::templates` exists as an inherent method at all: a
/// declaration is a property of the adapter *kind*, held in a `const` table
/// compiled into this binary, so a read that wants one has nothing to wait for.
/// Reaching through `SourcesState` would make a room's list, the mini board and
/// the project census answer `not_ready` until the scheduler is up, which is
/// later than the pool -- a widening of `not_ready` bought for nothing. The
/// reads that already hold a `SourcesState` (the inbox, the merge pass) pass
/// its registry instead, so the injected one is still what a test drives.
///
/// # Errors
///
/// `internal` if the source listing fails.
async fn declared_paths(pool: &PgPool) -> Result<knobas_core::payload::Declarations, IpcError> {
    crate::sources::paths::declared_paths(pool, &crate::sources::Registry::builtin()).await
}

// -- the detail view --------------------------------------------------------

/// Which source an entity came from, as the detail view names it.
///
/// Every field falls back to the source id, because `run_once` syncs sources
/// that have no `knobas.source_config` row (tests, ad-hoc imports, interfaces
/// §1). An entity whose source was deleted still has to render.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SourceRef {
    pub id: String,
    pub display_name: String,
    /// The *adapter* kind (`jira`, `mock`), not the instance id.
    pub adapter_kind: String,
    /// Whether the user has this source turned on, **as of this read** --
    /// derived in the [`DETAIL`] statement, never stored (issue #204).
    ///
    /// The detail reaches past `sync.live_item` on purpose (§5a), so it opens
    /// entities every other reader hides -- and since migration `0012` there
    /// are two reasons a reader hides one. `deleted_at` marks the first
    /// (withdrawn upstream); this marks the second (turned off by the user),
    /// and the banner needs both because the remedies differ: nothing undoes
    /// a withdrawal, one click undoes a disable -- which is also why it is
    /// derived: a stored marker would still say "off" after that click.
    ///
    /// `true` for a source with no configuration row, matching `0012`'s
    /// `coalesce(enabled, true)`: absence of configuration is not a decision
    /// the user made.
    pub enabled: bool,
}

/// Everything the slide-over draws for one entity (interfaces §2.5).
#[derive(Debug, Clone, serde::Serialize)]
pub struct EntityDetail {
    pub row: EntityRow,
    pub source: SourceRef,
    /// The adapter's own label and monogram for this kind.
    ///
    /// Resolved from the adapter registry by the row's `adapter_kind` -- see
    /// [`kind_info_for`]. `None` for a kind the adapter does not declare, and
    /// for a source with no configuration row at all (which `run_once`
    /// produces). Both cases fall through to the frontend's §3a humaniser,
    /// which is what §3a asks for when nothing declares anything.
    pub kind_info: Option<knobas_source::KindInfo>,
    /// Untrusted source text. Rendered as text, never as markup (gotcha 7).
    pub body_text: String,
    pub author: Option<String>,
    /// The source record **verbatim** (§3a): what the generic detail view
    /// projects. It is untrusted text all the way down.
    pub payload: serde_json::Value,
    /// Where the item lives in its own system, or `None` when the adapter
    /// reported no page -- in which case *Open in browser* is absent (P5).
    pub web_url: Option<String>,
    /// Set when the source withdrew the entity. The mirror row survives, so
    /// links and notes still resolve (§5a).
    pub deleted_at: Option<DateTime<Utc>>,
    /// Every link this entity takes part in, newest first, each with the end
    /// the reader is *not* on already resolved.
    ///
    /// Read undirected by [`knobas_core::link::entries_of`], so a link drawn
    /// from either end is on both ends' detail, and hydrated there, so the
    /// panel draws a kind and a title rather than a raw id -- including for a
    /// target the source withdrew. [`create_link`] and [`unlink`] are what
    /// move it.
    pub links: Vec<LinkEntry>,
    /// This entity's own history, newest first (spec §12.1).
    pub activity: Vec<ActivityRow>,
    /// The mirrored builds that ran on the commit this entity stands at
    /// (#710): for a pull request its head commit, for a branch its tip, for
    /// a commit itself -- whatever its kind declares as `commit` -- the rows
    /// whose declared `revision` names that sha. Failing first, then running,
    /// then the rest by tone, newest first within one; at most
    /// [`DETAIL_BUILDS`]. Empty for a kind that declares no commit, which is
    /// every kind but code, and for a commit nothing mirrored ran on.
    pub builds: Vec<EntityRow>,
    /// A ticket's status as its source declares it (#760), read once beside
    /// the row by the rule the mini board reads it by
    /// ([`declared_string!`](knobas_core::declared_string)); the ticket detail's
    /// select shows it as the ticket's current one.
    ///
    /// `None` for anything that is not a ticket, for a ticket whose source
    /// declares no status or whose record does not carry it (the board's
    /// terminal group, "No status"), and for a withdrawn ticket or a disabled
    /// source's, which the board does not draw either.
    pub status: Option<String>,
    /// A ticket's type in its source's own word (#816, `Story`, `Bug`), read
    /// through the declared `issue_type` path; the job card draws it. `None`
    /// where nothing is declared or the record misses it (ADR-0007).
    pub issue_type: Option<String>,
    /// A ticket's priority in its source's own word (#816), through the
    /// declared `priority` path. `None` on a miss.
    pub priority: Option<String>,
    /// A ticket's original estimate in seconds (#816), through the declared
    /// `estimate` path: a whole JSON number or nothing, never zero for a
    /// miss. The job card writes it as *est. 3h*.
    pub estimate_seconds: Option<i64>,
}

/// How many history lines the detail view is given up front.
///
/// Enough to fill the panel without a second round trip; the full history is a
/// later milestone's view, not a scroll in a slide-over.
const DETAIL_ACTIVITY: i64 = 20;

/// How many builds the detail draws for a commit (#710): enough for every
/// configuration a commit is commonly built by, failing first, so a cut
/// leaves out passed ones.
const DETAIL_BUILDS: i64 = 20;

/// The builds that ran on the commit `$1` (#710), a full object id.
///
/// The commit is the entity's declared `commit`, read by [`DETAIL`] beside
/// the row ([`declared_string!`](knobas_core::declared_string)); a build ran
/// on it where its kind's declared `revision` lists it. That walk is stored
/// (#732, migration `0030`, ADR-0019's pattern): `sync.item_revision` holds
/// one row per commit each item declares, filled by the database as items
/// are written, and this statement asks its index on `revision` and touches
/// only the builds it names -- under a millisecond at 50,000 builds, where the
/// walk over every live row took 437-442 ms. No adapter is named here, no
/// payload key: the rows are matched to the source's configured adapter on
/// `adapter_kind`, so a source whose adapter declares no revision contributes
/// nothing whatever its payloads say. Matched exactly: both sides are full
/// object ids as git spells them.
///
/// **Asked only for an entity that stands at a commit**: the detail of a
/// ticket or a page declares none, so there is no sha to bind, and skipping
/// the round trip costs nothing.
///
/// The builds come from `sync.live_item`, so a withdrawn build or a disabled
/// source's is not drawn as if it stood. Ordered as the Builds tile weighs a
/// state (#680): `err`, `warn`, `info`, `ok`, `muted`, then none; newest
/// first within one. `$2` is the declarations, for the path line and the
/// state chip.
///
/// Public so `tests/it/builds_of_a_commit_perf.rs` can read its plan.
pub const DETAIL_BUILDS_AT: &str = concat!(
    r#"
select b.* from (
  select i.entity_id, i.source_id, i.kind, i.title, i.item_updated_at, i.synced_at,
         "#,
    knobas_core::path_line_read!("$2"),
    r#" as path,
         "#,
    knobas_core::declared_state!("$2"),
    r#" as state
    from sync.item_revision r
    join sync.live_item i on i.entity_id = r.entity_id
    join knobas.source_config s on s.id = i.source_id and s.kind = r.adapter_kind
   where r.revision = $1
) b
 order by coalesce(array_position(array['err', 'warn', 'info', 'ok', 'muted'], b.state->>'tone'), 6),
          b.item_updated_at desc nulls last, b.entity_id
 limit $3
"#
);

/// One entity, deleted or not.
///
/// The join is `sync.live_item`'s minus **both** of its filters, deliberately:
/// §5a says a withdrawn entity must still open, and since migration `0012` a
/// disabled source's must too -- links and notes point at either. Each dropped
/// filter leaves its marker instead: `e.deleted_at` for the withdrawn banner,
/// `coalesce(c.enabled, true)` for the turned-off one (issue #204).
/// `source_config` is a **left** join because `run_once` syncs unconfigured
/// sources.
const DETAIL: &str = concat!(
    r#"
select i.entity_id, i.source_id, i.kind, i.title, "#,
    knobas_core::body_text_shown!("i.body_text"),
    r#" as body_text, i.author,
       i.item_updated_at, i.synced_at, i.payload, i.web_url,
       "#,
    knobas_core::path_line_read!("$2"),
    r#" as path,
       "#,
    knobas_core::declared_state!("$2"),
    r#" as state,
       "#,
    knobas_core::declared_string!("$2", "commit"),
    r#" as commit,
       "#,
    knobas_core::declared_string!("$2", "status_name"),
    r#" as status,
       "#,
    knobas_core::declared_string!("$2", "issue_type"),
    r#" as issue_type,
       "#,
    knobas_core::declared_string!("$2", "priority"),
    r#" as priority,
       "#,
    knobas_core::declared_integer!("$2", "estimate"),
    r#" as estimate_seconds,
       e.deleted_at,
       c.display_name, c.kind as adapter_kind,
       coalesce(c.enabled, true) as source_enabled
  from sync.item i
  join knobas.entity e on e.id = i.entity_id
  left join knobas.source_config c on c.id = i.source_id
 where i.entity_id = $1
"#
);

/// Everything the slide-over needs for `entity_id`, in four round trips and,
/// for an entity that stands at a commit, a fifth: the source listing the
/// declarations come from, the row (with its declared commit), its links, its
/// history and the builds of that commit (#710).
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) if `entity_id` is not an entity
/// id -- which is how a mistyped deep link reports itself rather than as a
/// 500; [`NotFound`](crate::IpcErrorCode::NotFound) if nothing carries it;
/// [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
pub async fn get_entity_inner(pool: &PgPool, entity_id: &str) -> Result<EntityDetail, IpcError> {
    // First, and before any query: `#/ticket/not-an-id` is a bad address, not
    // a missing entity, and the two want different words on screen.
    let entity = EntityRef::parse(entity_id).map_err(IpcError::invalid)?;

    // The declarations the path line reads the parent through (#596), as a
    // room's line reads them: the panel and the row draw one path.
    let declarations = declared_paths(pool).await?;
    let row = sqlx::query(DETAIL)
        .bind(entity.to_string())
        .bind(declarations.as_param())
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| IpcError::not_found(format!("{entity} is not in the mirror")))?;

    let source_id: String = row.get("source_id");
    let display_name: Option<String> = row.get("display_name");
    let adapter_kind: Option<String> = row.get("adapter_kind");

    let kind: String = row.get("kind");
    // The board draws live tickets only (`sync.live_item`); the detail opens
    // withdrawn ones too, and says nothing of a status the board would not.
    let status: Option<String> = row.get("status");
    let status = status.filter(|_| {
        kind == "ticket"
            && row.get::<Option<DateTime<Utc>>, _>("deleted_at").is_none()
            && row.get::<bool, _>("source_enabled")
    });
    let kind_info = kind_info_for(adapter_kind.as_deref(), &kind);

    // The builds of the commit this entity stands at, asked only when it
    // stands at one (see [`DETAIL_BUILDS_AT`] for why).
    let commit: Option<String> = row.get("commit");
    let builds = match commit {
        Some(sha) => sqlx::query(DETAIL_BUILDS_AT)
            .bind(sha)
            .bind(declarations.as_param())
            .bind(DETAIL_BUILDS)
            .fetch_all(pool)
            .await?
            .iter()
            .map(entity_row)
            .collect::<Result<_, _>>()?,
        None => Vec::new(),
    };

    Ok(EntityDetail {
        row: EntityRow {
            entity_id: row.get("entity_id"),
            kind,
            source_id: source_id.clone(),
            title: row.get("title"),
            updated_at: row.get("item_updated_at"),
            synced_at: row.get("synced_at"),
            // The detail panel draws the same path the launcher row does, out
            // of the same statement -- the panel *has* the payload, but
            // reading it here would be a second spelling of "where is this",
            // and ADR-0007 requirement 2 exists to stop exactly that.
            path: row.get("path"),
            state: state_of(&row)?,
        },
        source: SourceRef {
            display_name: display_name.unwrap_or_else(|| source_id.clone()),
            adapter_kind: adapter_kind.unwrap_or_else(|| source_id.clone()),
            id: source_id,
            enabled: row.get("source_enabled"),
        },
        kind_info,
        body_text: row.get("body_text"),
        author: row.get("author"),
        payload: row.get("payload"),
        web_url: row.get("web_url"),
        deleted_at: row.get("deleted_at"),
        links: knobas_core::link::entries_of(pool, &entity).await?,
        activity: knobas_core::activity::recent(pool, DETAIL_ACTIVITY, Some(&entity)).await?,
        builds,
        status,
        issue_type: row.get("issue_type"),
        priority: row.get("priority"),
        estimate_seconds: row.get("estimate_seconds"),
    })
}

/// What the adapter declares about one kind, if anything does.
///
/// §3a: *"entity kinds with display metadata drive grouping, chips and
/// labels"*. Looked up by the **adapter** kind and not the instance id, because
/// `jira` and `jira-eu` are two instances of one adapter that declare the same
/// kinds (P10).
///
/// `None` in three cases, and all three are the same answer to the frontend:
/// no configuration row for the source (`run_once` produces exactly that), no
/// adapter of that kind compiled in, and an adapter that simply does not
/// declare this kind. §3a's whole point is that an undeclared kind still
/// renders, so the humaniser on the other side is the designed path rather
/// than a degradation.
///
/// **First declaration wins**, deterministically: `ADAPTERS` is a fixed table
/// in a fixed order, so two adapters declaring one kind id resolve the same
/// way on every call rather than by whichever the iterator reached first this
/// time.
fn kind_info_for(adapter_kind: Option<&str>, kind: &str) -> Option<knobas_source::KindInfo> {
    let adapter_kind = adapter_kind?;
    crate::sources::Registry::builtin()
        .templates()
        .into_iter()
        .find(|template| template.adapter_kind == adapter_kind)?
        .entity_kinds
        .into_iter()
        .find(|info| info.id == kind)
}

/// One entity, for the detail slide-over.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the
/// database is still coming up, and whatever [`get_entity_inner`] refuses
/// with.
#[tauri::command]
pub async fn get_entity(
    lifecycle: State<'_, Lifecycle>,
    entity_id: String,
) -> Result<EntityDetail, IpcError> {
    let pool = lifecycle.pool()?;
    get_entity_inner(&pool, &entity_id).await
}

// -- the Builds frame (ADR-0022, #848) -------------------------------------

/// One run of a pipeline, as the Builds view draws it (#848): every field
/// read through the run kind's declared paths (ADR-0007), and nothing else.
///
/// `started` and `finished` are the declared ISO 8601 instants, basic or
/// extended (`knobas_core::payload::parse_instant`); a run in flight has no
/// `finished`, a queued one neither. `duration_seconds` is the one from the
/// other, only where both are read.
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(feature = "test-util", derive(serde::Deserialize))]
pub struct RunRow {
    pub entity_id: String,
    /// The run's number in the source's own spelling; a queued TeamCity
    /// build has none yet.
    pub number: Option<String>,
    /// The declared state chip (#666): its text in the source's word and the
    /// tone the shell draws it in.
    pub state: Option<knobas_core::payload::StateChip>,
    pub branch: Option<String>,
    pub started: Option<DateTime<Utc>>,
    pub finished: Option<DateTime<Utc>>,
    pub duration_seconds: Option<i64>,
    /// The source's own timestamp for the row, what runs are ordered by.
    pub updated_at: Option<DateTime<Utc>>,
    pub web_url: Option<String>,
    /// How the run was started (#961): the declared `trigger`, in the
    /// shell's words.
    pub trigger: Option<knobas_core::payload::Trigger>,
    /// Who started it, as the source spells the account: the declared
    /// `started_by`. A run a change or a schedule started has none.
    pub started_by: Option<String>,
    /// Whether `started_by` is one of the configured usernames, the identity
    /// behind `@me` (*Started by me*, *you*).
    pub started_by_me: bool,
    /// The commit it built: the first of the declared `revision`s (#710), a
    /// full sha.
    pub revision: Option<String>,
    /// The mirrored commit `revision` names (#1004): the live item of kind
    /// `commit` whose declared `commit` is that sha, what the run page's
    /// *Commit* opens. `None` where the mirror holds no such commit.
    pub commit_id: Option<String>,
    /// The machine it ran on: the declared `agent` (#1004). A queued run has
    /// none.
    pub agent: Option<String>,
    /// What it is doing now, in the source's words: the declared
    /// `current_step` (#1004). Only a run in flight has one.
    pub current_step: Option<String>,
}

/// One pipeline in the Builds list (R-9.4a, #848).
///
/// `source_name` is the configured source's display name: the one place the
/// source's word appears, in the list's header line (ADR-0022). `recent` is
/// its last runs newest first, at most [`PIPELINE_RECENT`]: the strip, the
/// latest time, and whether the latest run failed.
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(feature = "test-util", derive(serde::Deserialize))]
pub struct PipelineRow {
    pub entity_id: String,
    pub source_id: String,
    pub source_name: String,
    pub name: String,
    pub group: Option<String>,
    /// The chain of groups it sits in, outermost first (#849): the declared
    /// `group_path`. Empty where the source declares none or the record
    /// carries none; the list then draws it under its `group`.
    pub path: Vec<String>,
    /// Whether the reader pinned it (#849): knobas-local, never the
    /// source's ([`PINS_SETTING`]).
    pub pinned: bool,
    pub web_url: Option<String>,
    pub recent: Vec<RunRow>,
}

/// The `knobas.setting` key holding the pinned pipelines (#849, R-9.3): a
/// JSON array of entity ids. In `knobas.setting` rather than a table of its
/// own, for the reason the clones root is (`0002`, comment 6): a short list
/// of app-level state with no other home. [`PIPELINES`] spells it as a
/// literal, and `the_pins_are_read_by_their_constant` holds the two together.
pub const PINS_SETTING: &str = "builds.pins";

/// How many runs a pipeline row carries: the strip of the last twelve.
pub const PIPELINE_RECENT: i64 = 12;

/// How many runs `pipeline_runs` answers when the caller names no cap.
const PIPELINE_RUNS: i64 = 100;

/// The (source, kind) pairs the declarations put in the Builds frame, with
/// their part: `pipeline` or `run`. `$1` is the declarations. Read once a
/// statement, so the live corpus is joined on its `(source_id, kind)` rather
/// than every row's declaration being looked up.
macro_rules! frame_kinds_cte {
    () => {
        "frames as (
  select s.key as source_id, k.key as kind, k.value->>'frame' as frame
    from jsonb_each($1::jsonb) s
    cross join lateral jsonb_each(case when jsonb_typeof(s.value) = 'object' then s.value else '{}'::jsonb end) k
   where jsonb_typeof(k.value) = 'object' and k.value->>'frame' in ('pipeline', 'run')
)"
    };
}

/// A run's columns, read through its kind's declarations; the row alias is
/// `i` and the declarations `$1`.
macro_rules! run_columns {
    () => {
        concat!(
            "i.entity_id, i.source_id, i.web_url, i.item_updated_at, ",
            knobas_core::declared_string!("$1", "pipeline"),
            " as pkey, ",
            knobas_core::declared_string!("$1", "number"),
            " as number, ",
            knobas_core::declared_state!("$1"),
            " as state, ",
            knobas_core::declared_string!("$1", "branch"),
            " as branch, ",
            knobas_core::declared_string!("$1", "started"),
            " as started, ",
            knobas_core::declared_string!("$1", "finished"),
            " as finished, ",
            knobas_core::declared_trigger!("$1"),
            " as trig, ",
            knobas_core::declared_string!("$1", "started_by"),
            " as started_by, ",
            knobas_core::declared_string!("$1", "agent"),
            " as agent, ",
            knobas_core::declared_string!("$1", "current_step"),
            " as current_step, (select l.value from ",
            knobas_core::declared_list!("$1", "revision"),
            " l limit 1) as revision"
        )
    };
}

/// Every live pipeline, by name, each with its last [`PIPELINE_RECENT`] runs
/// as a jsonb array newest first. A run sits under the pipeline of its own
/// source whose declared `pipeline` equals its own; a pipeline whose name or
/// identity misses is not drawn. Runs are ordered by the source's own
/// timestamp, newest first. `$3` is the CI role's fillers (#1295): a
/// pipeline of any other source is not drawn, and a run is drawn only under
/// a drawn pipeline.
const PIPELINES: &str = concat!(
    "with ",
    frame_kinds_cte!(),
    ",
p as (
  select i.entity_id, i.source_id, i.web_url, coalesce(c.display_name, i.source_id) as source_name, ",
    knobas_core::declared_string!("$1", "name"),
    " as name, ",
    knobas_core::declared_string!("$1", "group"),
    " as grp, ",
    knobas_core::declared_string!("$1", "pipeline"),
    // The declared chain of groups, in order (#849). One candidate is what a
    // source declares; a second's names would follow the first's.
    " as pkey,
         (select coalesce(jsonb_agg(btrim(e.elem #>> c.entry_segs) order by e.ord), '[]'::jsonb) from ",
    knobas_core::declared_list_candidates!("$1", "group_path"),
    " c cross join lateral jsonb_array_elements(
             case when jsonb_typeof(i.payload #> c.at_segs) = 'array'
                  then i.payload #> c.at_segs else '[]'::jsonb end) with ordinality as e(elem, ord)
           where jsonb_typeof(e.elem #> c.entry_segs) = 'string'
             and nullif(btrim(e.elem #>> c.entry_segs), '') is not null) as path
    from frames f
    join sync.live_item i on i.source_id = f.source_id and i.kind = f.kind
    left join knobas.source_config c on c.id = i.source_id
   where f.frame = 'pipeline' and i.source_id = any($3::text[])
),
pins as (
  select jsonb_array_elements_text(value) as id
    from knobas.setting
   where key = 'builds.pins' and jsonb_typeof(value) = 'array'
),
r as (
  select x.*, row_number() over (partition by x.source_id, x.pkey
                                 order by x.item_updated_at desc nulls last, x.entity_id desc) as rn
    from (select ",
    run_columns!(),
    "
            from frames f
            join sync.live_item i on i.source_id = f.source_id and i.kind = f.kind
           where f.frame = 'run') x
   where x.pkey is not null
)
select p.entity_id, p.source_id, p.source_name, p.name, p.grp, p.web_url, p.path,
       exists (select 1 from pins where pins.id = p.entity_id) as pinned,
       coalesce((select jsonb_agg(jsonb_build_object(
                   'entity_id', r.entity_id, 'number', r.number, 'state', r.state,
                   'branch', r.branch, 'started', r.started, 'finished', r.finished,
                   'updated_at', r.item_updated_at, 'web_url', r.web_url,
                   'trigger', r.trig, 'started_by', r.started_by,
                   'agent', r.agent, 'current_step', r.current_step,
                   'revision', r.revision) order by r.rn)
                   from r
                  where r.source_id = p.source_id and r.pkey = p.pkey and r.rn <= $2),
                '[]'::jsonb) as recent
  from p
 where p.pkey is not null and p.name is not null
 order by lower(p.name), p.entity_id
"
);

/// The runs of the pipeline `$2`, newest first, at most `$3`.
const PIPELINE_RUNS_OF: &str = concat!(
    "with ",
    frame_kinds_cte!(),
    ",
target as (
  select i.source_id, ",
    knobas_core::declared_string!("$1", "pipeline"),
    " as pkey
    from frames f
    join sync.live_item i on i.source_id = f.source_id and i.kind = f.kind
   where f.frame = 'pipeline' and i.entity_id = $2
)
select t.pkey is not null as found, x.* from target t
  left join lateral (
    select ",
    run_columns!(),
    "
      from frames f
      join sync.live_item i on i.source_id = f.source_id and i.kind = f.kind
     where f.frame = 'run' and f.source_id = t.source_id
       and ",
    knobas_core::declared_string!("$1", "pipeline"),
    " = t.pkey
     order by i.item_updated_at desc nulls last, i.entity_id desc
     limit $3) x on true
"
);

/// A run as it leaves a statement or a `recent` array: the declared times
/// still in the source's spelling.
#[derive(serde::Deserialize)]
struct RawRun {
    entity_id: String,
    number: Option<String>,
    state: Option<knobas_core::payload::StateChip>,
    branch: Option<String>,
    started: Option<String>,
    finished: Option<String>,
    updated_at: Option<DateTime<Utc>>,
    web_url: Option<String>,
    trigger: Option<knobas_core::payload::Trigger>,
    started_by: Option<String>,
    agent: Option<String>,
    current_step: Option<String>,
    revision: Option<String>,
}

impl RunRow {
    /// A run as a statement or a source answered it, `identity` being the
    /// configured usernames ([`identity_of`]).
    fn read(raw: RawRun, identity: &[String]) -> Self {
        let instant =
            |t: Option<String>| t.as_deref().and_then(knobas_core::payload::parse_instant);
        let started = instant(raw.started);
        let finished = instant(raw.finished);
        RunRow {
            started_by_me: raw
                .started_by
                .as_ref()
                .is_some_and(|who| identity.contains(who)),
            trigger: raw.trigger,
            started_by: raw.started_by,
            revision: raw.revision,
            commit_id: None,
            agent: raw.agent,
            current_step: raw.current_step,
            entity_id: raw.entity_id,
            number: raw.number,
            state: raw.state,
            branch: raw.branch,
            duration_seconds: started
                .zip(finished)
                .map(|(from, to)| (to - from).num_seconds().max(0)),
            started,
            finished,
            updated_at: raw.updated_at,
            web_url: raw.web_url,
        }
    }
}

/// Every pipeline the sources filling the CI role declare, for the Builds
/// list. The read is CI's by definition, so it narrows itself to the role's
/// fillers (`sources::crud::fillers`, #1295, ADR-0029 as amended on
/// 2026-10-19) with no argument: a source whose records missed a CI path,
/// one whose adapter claims no CI, one whose adapter is not installed and
/// the items a removed source left behind draw no pipeline here.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
pub async fn pipelines_inner(pool: &PgPool) -> Result<Vec<PipelineRow>, IpcError> {
    let declarations = declared_paths(pool).await?;
    let identity = identity_of(pool).await?;
    let fillers = crate::sources::crud::fillers(
        pool,
        &crate::sources::Registry::builtin(),
        knobas_source::role::CI,
    )
    .await?;
    let rows = sqlx::query(PIPELINES)
        .bind(declarations.as_param())
        .bind(PIPELINE_RECENT)
        .bind(&fillers)
        .fetch_all(pool)
        .await?;
    let mut pipelines: Vec<PipelineRow> = rows
        .iter()
        .map(|row| {
            let recent: sqlx::types::Json<Vec<RawRun>> = row.get("recent");
            let path: sqlx::types::Json<Vec<String>> = row.get("path");
            Ok(PipelineRow {
                entity_id: row.get("entity_id"),
                source_id: row.get("source_id"),
                source_name: row.get("source_name"),
                name: row.get("name"),
                group: row.get("grp"),
                path: path.0,
                pinned: row.get("pinned"),
                web_url: row.get("web_url"),
                recent: recent
                    .0
                    .into_iter()
                    .map(|raw| RunRow::read(raw, &identity))
                    .collect(),
            })
        })
        .collect::<Result<_, IpcError>>()?;
    // The list reads the mirror only: a sha two repositories share links
    // nothing here, rather than one live read per pipeline (#1144).
    let runs: Vec<(&str, &mut RunRow)> = pipelines
        .iter_mut()
        .flat_map(|p| {
            let PipelineRow {
                entity_id, recent, ..
            } = p;
            let entity_id: &str = entity_id;
            recent.iter_mut().map(move |r| (entity_id, r))
        })
        .collect();
    with_commits(pool, &declarations, None, runs).await?;
    Ok(pipelines)
}

/// One pipeline's runs, newest first, at most `limit` (100 when `None`).
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) for an address that is not an
/// entity id; [`NotFound`](crate::IpcErrorCode::NotFound) when no live
/// pipeline carries it; [`Internal`](crate::IpcErrorCode::Internal) for a
/// query failure.
///
/// The mirror alone: a run whose sha two mirrored repositories share has no
/// `commit_id` here. [`pipeline_runs_live_inner`] is what the command answers.
pub async fn pipeline_runs_inner(
    pool: &PgPool,
    pipeline_id: &str,
    limit: Option<u32>,
) -> Result<Vec<RunRow>, IpcError> {
    runs_of_pipeline(pool, None, pipeline_id, limit).await
}

/// One pipeline's runs as [`pipeline_runs_inner`] reads them, with a sha two
/// mirrored repositories share linked to the commit in the repository the
/// pipeline builds, read from its settings live (#1144, [`with_commits`]).
/// A failed settings read is no failure here: those runs link nothing.
///
/// # Errors
///
/// As [`pipeline_runs_inner`].
pub async fn pipeline_runs_live_inner(
    pool: &PgPool,
    secrets: &std::sync::Arc<dyn knobas_secrets::SecretStore>,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    pipeline_id: &str,
    limit: Option<u32>,
) -> Result<Vec<RunRow>, IpcError> {
    runs_of_pipeline(pool, Some((secrets, registry)), pipeline_id, limit).await
}

/// What both `pipeline_runs` readings share; `live` is whether a shared sha
/// may be resolved through the pipeline's settings.
async fn runs_of_pipeline(
    pool: &PgPool,
    live: Option<LiveReads<'_>>,
    pipeline_id: &str,
    limit: Option<u32>,
) -> Result<Vec<RunRow>, IpcError> {
    let entity = EntityRef::parse(pipeline_id).map_err(IpcError::invalid)?;
    let declarations = declared_paths(pool).await?;
    let identity = identity_of(pool).await?;
    let rows = sqlx::query(PIPELINE_RUNS_OF)
        .bind(declarations.as_param())
        .bind(entity.to_string())
        .bind(limit.map_or(PIPELINE_RUNS, i64::from))
        .fetch_all(pool)
        .await?;
    if !rows.first().is_some_and(|row| row.get::<bool, _>("found")) {
        return Err(IpcError::not_found(format!(
            "{entity} is not a pipeline in the mirror"
        )));
    }
    let mut runs: Vec<RunRow> = rows
        .iter()
        .filter_map(|row| {
            let entity_id: Option<String> = row.get("entity_id");
            Some(RunRow::read(
                RawRun {
                    entity_id: entity_id?,
                    number: row.get("number"),
                    state: row
                        .get::<Option<sqlx::types::Json<knobas_core::payload::StateChip>>, _>(
                            "state",
                        )
                        .map(|chip| chip.0),
                    branch: row.get("branch"),
                    started: row.get("started"),
                    finished: row.get("finished"),
                    updated_at: row.get("item_updated_at"),
                    web_url: row.get("web_url"),
                    trigger: row.get::<Option<String>, _>("trig").and_then(|word| {
                        serde_json::from_value(serde_json::Value::String(word)).ok()
                    }),
                    started_by: row.get("started_by"),
                    agent: row.get("agent"),
                    current_step: row.get("current_step"),
                    revision: row.get("revision"),
                },
                &identity,
            ))
        })
        .collect();
    let pipeline = entity.to_string();
    let pairs = runs.iter_mut().map(|r| (pipeline.as_str(), r)).collect();
    with_commits(pool, &declarations, live, pairs).await?;
    Ok(runs)
}

/// Every mirrored commit of every sha in `$2`, by the declarations `$1`: the
/// live items of kind `commit` whose declared `commit` is that sha. Two rows
/// for one sha are two mirrored repositories holding it, a fork or a mirror
/// on a second forge (#1144).
const COMMITS_OF: &str = concat!(
    "select c.sha, c.entity_id from (
  select i.entity_id, ",
    knobas_core::declared_string!("$1", "commit"),
    " as sha
    from sync.live_item i
   where i.kind = 'commit') c
 where c.sha = any($2::text[])
 order by c.sha, c.entity_id"
);

/// How long [`with_commits`] waits for all its pipelines' settings together
/// (#1144). The live read only sharpens a link, and the run page's runs wait
/// on it, so a slow or unreachable source costs at most this before those
/// runs answer without the link. One HTTP call alone may take 45 s
/// (`knobas_http::SEND_BUDGET`), and a settings read makes several.
const COMMIT_READ_BUDGET: std::time::Duration = std::time::Duration::from_secs(5);

/// The live reads a run's commit may need (#1144): the secrets and adapters
/// to read its pipeline's settings with.
type LiveReads<'a> = (
    &'a std::sync::Arc<dyn knobas_secrets::SecretStore>,
    &'a dyn knobas_sync::scheduler::AdapterRegistry,
);

/// Fills each run's [`RunRow::commit_id`] from its `revision` (#1004), with
/// one read for all of them, and none when no run names a revision. Each run
/// comes with the id of its pipeline.
///
/// A commit is found by kind and declared `commit`, not by the view's
/// repository-and-sha hint: a branch and a pull request declare a `commit`
/// too, and only a commit's own item is what a commit link opens.
///
/// **A sha one mirrored commit holds** links that commit. **A sha two hold**
/// (#1144) links the one in the repository the run's pipeline builds, found
/// by [`crate::sources::crud::pipeline_commits`] from its settings, one live
/// read per pipeline and only for such a sha, all of them within
/// [`COMMIT_READ_BUDGET`]. Without `live`, or where the settings cannot be
/// read in time or name none of them, it links nothing: a commit
/// in another repository is never offered as the one the run built.
async fn with_commits<'a>(
    pool: &PgPool,
    declarations: &knobas_core::payload::Declarations,
    live: Option<LiveReads<'_>>,
    runs: Vec<(&'a str, &'a mut RunRow)>,
) -> Result<(), IpcError> {
    let mut runs: Vec<(&str, &mut RunRow)> = runs
        .into_iter()
        .filter(|(_, r)| r.revision.is_some())
        .collect();
    if runs.is_empty() {
        return Ok(());
    }
    let shas: Vec<&str> = runs
        .iter()
        .filter_map(|(_, r)| r.revision.as_deref())
        .collect();
    let mut held: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    for (sha, commit) in sqlx::query_as::<_, (String, String)>(COMMITS_OF)
        .bind(declarations.as_param())
        .bind(&shas)
        .fetch_all(pool)
        .await?
    {
        held.entry(sha).or_default().push(commit);
    }
    let mut shared: std::collections::BTreeMap<&str, Vec<String>> =
        std::collections::BTreeMap::new();
    for (pipeline, run) in &runs {
        if let Some(sha) = run.revision.as_ref()
            && held.get(sha).is_some_and(|commits| commits.len() > 1)
        {
            let wanted = shared.entry(pipeline).or_default();
            if !wanted.contains(sha) {
                wanted.push(sha.clone());
            }
        }
    }
    let mut resolved: std::collections::HashMap<(&str, String), String> =
        std::collections::HashMap::new();
    if let Some((secrets, registry)) = live {
        let deadline = tokio::time::Instant::now() + COMMIT_READ_BUDGET;
        for (pipeline, wanted) in shared {
            let read =
                crate::sources::crud::pipeline_commits(pool, secrets, registry, pipeline, &wanted);
            match tokio::time::timeout_at(deadline, read).await {
                Err(_) => {
                    tracing::warn!(pipeline, "a run's repository took too long to read");
                }
                Ok(Ok(found)) => {
                    for (sha, commit) in found {
                        if held
                            .get(&sha)
                            .is_some_and(|commits| commits.contains(&commit))
                        {
                            resolved.insert((pipeline, sha), commit);
                        }
                    }
                }
                Ok(Err(error)) => {
                    tracing::warn!(pipeline, %error, "a run's repository could not be read");
                }
            }
        }
    }
    for (pipeline, run) in &mut runs {
        run.commit_id =
            run.revision
                .as_ref()
                .and_then(|sha| match held.get(sha).map(Vec::as_slice) {
                    Some([only]) => Some(only.clone()),
                    Some(_) => resolved.get(&(*pipeline, sha.clone())).cloned(),
                    None => None,
                });
    }
    Ok(())
}

/// Whether `$2` is a live pipeline of the declarations `$1`.
const IS_PIPELINE: &str = concat!(
    "with ",
    frame_kinds_cte!(),
    "
select exists (
  select 1 from frames f
    join sync.live_item i on i.source_id = f.source_id and i.kind = f.kind
   where f.frame = 'pipeline' and i.entity_id = $2)"
);

/// `not_found` unless `entity` is a live pipeline of `declarations`.
async fn live_pipeline(
    pool: &PgPool,
    declarations: &knobas_core::payload::Declarations,
    entity: &EntityRef,
) -> Result<(), IpcError> {
    let live: bool = sqlx::query_scalar(IS_PIPELINE)
        .bind(declarations.as_param())
        .bind(entity.to_string())
        .fetch_one(pool)
        .await?;
    if live {
        Ok(())
    } else {
        Err(IpcError::not_found(format!(
            "{entity} is not a pipeline in the mirror"
        )))
    }
}

/// How many older runs one scroll asks for when the caller names no cap.
const OLDER_RUNS: u32 = 50;

/// One run the source answered, read through its kind's declarations as a
/// mirrored run is (`run_columns!`): `None` for an item of a kind the
/// declarations put outside the run frame.
fn run_of_item(
    declarations: &knobas_core::payload::Declarations,
    identity: &[String],
    item: &knobas_source::SyncItem,
) -> Option<RunRow> {
    use knobas_core::payload::{
        Frame, resolve_list, resolve_state, resolve_string, resolve_trigger,
    };
    let paths = declarations.get(&item.entity.namespace, &item.kind)?;
    if paths.frame != Some(Frame::Run) {
        return None;
    }
    let payload = &item.payload;
    Some(RunRow::read(
        RawRun {
            entity_id: item.entity.to_string(),
            number: resolve_string(payload, &paths.number),
            state: resolve_state(payload, &paths.state),
            branch: resolve_string(payload, &paths.branch),
            started: resolve_string(payload, &paths.started),
            finished: resolve_string(payload, &paths.finished),
            updated_at: item.updated_at,
            web_url: item.web_url.clone(),
            trigger: resolve_trigger(payload, &paths.trigger),
            started_by: resolve_string(payload, &paths.started_by),
            agent: resolve_string(payload, &paths.agent),
            current_step: resolve_string(payload, &paths.current_step),
            revision: resolve_list(payload, &paths.revision).into_iter().next(),
        },
        identity,
    ))
}

/// A pipeline's runs older than the run `before`, asked of its source: the
/// history beyond the window the sync keeps (ADR-0022, #851), newest first, at
/// most `limit` (50 when `None`, never more than
/// [`knobas_source::pipeline::OLDER_RUNS_MAX`]). Each run is read through the
/// run kind's declared paths, as [`pipeline_runs_inner`] reads a mirrored one,
/// and nothing is stored. An empty answer is the start of the history.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) for an address that is not an
/// entity id; [`NotFound`](crate::IpcErrorCode::NotFound) when no live
/// pipeline carries it or its source is not configured; the source's own
/// failure as [`crate::sources::to_ipc`] maps it.
pub async fn older_pipeline_runs_inner(
    pool: &PgPool,
    secrets: &std::sync::Arc<dyn knobas_secrets::SecretStore>,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    pipeline_id: &str,
    before: &str,
    limit: Option<u32>,
) -> Result<Vec<RunRow>, IpcError> {
    let entity = EntityRef::parse(pipeline_id).map_err(IpcError::invalid)?;
    let run = EntityRef::parse(before).map_err(IpcError::invalid)?;
    if run.namespace != entity.namespace {
        return Err(IpcError::invalid(format!(
            "{run} is not a run of {entity}'s source"
        )));
    }
    let declarations = crate::sources::paths::declared_paths(pool, registry).await?;
    live_pipeline(pool, &declarations, &entity).await?;
    let identity = identity_of(pool).await?;
    let count = limit
        .unwrap_or(OLDER_RUNS)
        .clamp(1, knobas_source::pipeline::OLDER_RUNS_MAX);
    let items = crate::sources::crud::older_runs(
        pool,
        secrets,
        registry,
        &entity.to_string(),
        &run.to_string(),
        count,
    )
    .await
    .map_err(|error| crate::sources::to_ipc(&error, Some(&entity.namespace)))?;
    let mut runs: Vec<RunRow> = items
        .iter()
        .filter(|item| item.entity != run)
        .filter_map(|item| run_of_item(&declarations, &identity, item))
        .take(count as usize)
        .collect();
    let pipeline = entity.to_string();
    let pairs = runs.iter_mut().map(|r| (pipeline.as_str(), r)).collect();
    with_commits(pool, &declarations, Some((secrets, registry)), pairs).await?;
    Ok(runs)
}

/// A pipeline's settings, read live from its source (ADR-0022, #851): the
/// repositories it builds (each with its mirrored repository's id where the
/// mirror holds one), its triggers and its parameters. `None` for a source
/// that keeps no pipelines.
///
/// # Errors
///
/// As [`older_pipeline_runs_inner`].
pub async fn pipeline_settings_inner(
    pool: &PgPool,
    secrets: &std::sync::Arc<dyn knobas_secrets::SecretStore>,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    pipeline_id: &str,
) -> Result<Option<knobas_source::pipeline::PipelineSettings>, IpcError> {
    let entity = EntityRef::parse(pipeline_id).map_err(IpcError::invalid)?;
    let declarations = crate::sources::paths::declared_paths(pool, registry).await?;
    live_pipeline(pool, &declarations, &entity).await?;
    crate::sources::crud::pipeline_settings(pool, secrets, registry, &entity.to_string())
        .await
        .map_err(|error| crate::sources::to_ipc(&error, Some(&entity.namespace)))
}

/// One step of the chain a run is in (#857, R-9.13): the step's pipeline,
/// its name in the source's word, and its run on the run's branch read
/// through the run kind's declarations (`None` where it has not run there).
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(feature = "test-util", derive(serde::Deserialize))]
pub struct ChainStepRow {
    pub pipeline_id: String,
    pub name: String,
    pub run: Option<RunRow>,
}

/// The chain the run `run_id` is in, read live from its source (ADR-0022,
/// #857): one row per step in running order, each with its run (see
/// [`knobas_source::pipeline::RunChainStep::run`]). Empty for a run of a
/// pipeline in no chain, and for a source that keeps no pipelines.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) for an address that is not an
/// entity id; [`NotFound`](crate::IpcErrorCode::NotFound) when its source is
/// not configured; the source's own failure as [`crate::sources::to_ipc`]
/// maps it.
pub async fn run_chain_inner(
    pool: &PgPool,
    secrets: &std::sync::Arc<dyn knobas_secrets::SecretStore>,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    run_id: &str,
) -> Result<Vec<ChainStepRow>, IpcError> {
    let run = EntityRef::parse(run_id).map_err(IpcError::invalid)?;
    let declarations = crate::sources::paths::declared_paths(pool, registry).await?;
    let identity = identity_of(pool).await?;
    let steps = crate::sources::crud::run_chain(pool, secrets, registry, &run.to_string())
        .await
        .map_err(|error| crate::sources::to_ipc(&error, Some(&run.namespace)))?;
    let mut chain: Vec<ChainStepRow> = steps
        .into_iter()
        .map(|step| ChainStepRow {
            pipeline_id: step.step.pipeline,
            name: step.step.name,
            run: step
                .run
                .as_ref()
                .and_then(|item| run_of_item(&declarations, &identity, item)),
        })
        .collect();
    let pairs = chain
        .iter_mut()
        .filter_map(|step| {
            let ChainStepRow {
                pipeline_id, run, ..
            } = step;
            let pipeline_id: &str = pipeline_id;
            run.as_mut().map(|r| (pipeline_id, r))
        })
        .collect();
    with_commits(pool, &declarations, Some((secrets, registry)), pairs).await?;
    Ok(chain)
}

/// The chain one run is in, read live (#857).
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before bring-up, and whatever
/// [`run_chain_inner`] refuses with.
#[tauri::command]
pub async fn run_chain<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    run_id: String,
) -> Result<Vec<ChainStepRow>, IpcError> {
    let state = crate::sources::state(&app)?;
    run_chain_inner(
        &state.pool,
        &state.secrets,
        state.registry.as_ref(),
        &run_id,
    )
    .await
}

/// Older runs of one pipeline, beyond the mirror's window (#851).
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before bring-up, and whatever
/// [`older_pipeline_runs_inner`] refuses with.
#[tauri::command]
pub async fn older_pipeline_runs<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    pipeline_id: String,
    before: String,
    limit: Option<u32>,
) -> Result<Vec<RunRow>, IpcError> {
    let state = crate::sources::state(&app)?;
    older_pipeline_runs_inner(
        &state.pool,
        &state.secrets,
        state.registry.as_ref(),
        &pipeline_id,
        &before,
        limit,
    )
    .await
}

/// One pipeline's settings, read live (#851).
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before bring-up, and whatever
/// [`pipeline_settings_inner`] refuses with.
#[tauri::command]
pub async fn pipeline_settings<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    pipeline_id: String,
) -> Result<Option<knobas_source::pipeline::PipelineSettings>, IpcError> {
    let state = crate::sources::state(&app)?;
    pipeline_settings_inner(
        &state.pool,
        &state.secrets,
        state.registry.as_ref(),
        &pipeline_id,
    )
    .await
}

// -- the run page's live reads (ADR-0022; #854, #855, #856) ------------------

/// The parsed run id and its live adapter, or the refusal: `invalid` for an
/// address that is not an entity id, `not_found` for a source that is not
/// configured.
async fn run_adapter(
    pool: &PgPool,
    secrets: &std::sync::Arc<dyn knobas_secrets::SecretStore>,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    run_id: &str,
) -> Result<(EntityRef, Box<dyn knobas_source::Source>), IpcError> {
    let entity = EntityRef::parse(run_id).map_err(IpcError::invalid)?;
    let source = crate::sources::crud::run_source(pool, secrets, registry, &entity.to_string())
        .await
        .map_err(|error| crate::sources::to_ipc(&error, Some(&entity.namespace)))?;
    Ok((entity, source))
}

/// A source's failure on a run read, as [`crate::sources::to_ipc`] maps it.
fn run_read_failed(entity: &EntityRef, error: knobas_source::SourceError) -> IpcError {
    crate::sources::to_ipc(&error.into(), Some(&entity.namespace))
}

/// A range of run `run_id`'s whole log and its steps (#854): `count` lines
/// (all [`knobas_source::run::LOG_LINES_MAX`] of the cap when `None`) from
/// line `from`, or the last `count` when `from` is `None`. `None` for a
/// source that keeps no runs.
///
/// # Errors
///
/// `invalid` for an address that is not an entity id, `not_found` for a
/// source that is not configured, and the source's own failure as
/// [`crate::sources::to_ipc`] maps it (a run it does not have included).
pub async fn run_log_inner(
    pool: &PgPool,
    secrets: &std::sync::Arc<dyn knobas_secrets::SecretStore>,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    run_id: &str,
    from: Option<u64>,
    count: Option<u32>,
) -> Result<Option<knobas_source::run::RunLog>, IpcError> {
    let (entity, source) = run_adapter(pool, secrets, registry, run_id).await?;
    let count = count
        .unwrap_or(knobas_source::run::LOG_LINES_MAX)
        .clamp(1, knobas_source::run::LOG_LINES_MAX);
    source
        .run_log(&entity.to_string(), from, count)
        .await
        .map_err(|error| run_read_failed(&entity, error))
}

/// Run `run_id`'s tests, failed first (#855). Errors as [`run_log_inner`].
///
/// # Errors
///
/// As [`run_log_inner`].
pub async fn run_tests_inner(
    pool: &PgPool,
    secrets: &std::sync::Arc<dyn knobas_secrets::SecretStore>,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    run_id: &str,
) -> Result<Option<knobas_source::run::RunTests>, IpcError> {
    let (entity, source) = run_adapter(pool, secrets, registry, run_id).await?;
    source
        .run_tests(&entity.to_string())
        .await
        .map_err(|error| run_read_failed(&entity, error))
}

/// How test `test_id` came out in the last ten runs of `run_id`'s pipeline,
/// newest first (#855).
///
/// # Errors
///
/// As [`run_log_inner`].
pub async fn test_history_inner(
    pool: &PgPool,
    secrets: &std::sync::Arc<dyn knobas_secrets::SecretStore>,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    run_id: &str,
    test_id: &str,
) -> Result<Option<Vec<knobas_source::run::TestOutcome>>, IpcError> {
    let (entity, source) = run_adapter(pool, secrets, registry, run_id).await?;
    source
        .test_history(&entity.to_string(), test_id)
        .await
        .map_err(|error| run_read_failed(&entity, error))
}

/// `invalid` unless `path` stays inside a run's artifacts.
fn artifact_path(path: &str) -> Result<(), IpcError> {
    if knobas_source::run::is_artifact_path(path) {
        Ok(())
    } else {
        Err(IpcError::invalid(format!(
            "{path:?} is not a path inside a run's artifacts"
        )))
    }
}

/// One folder of run `run_id`'s artifacts, the root when `path` is `None`
/// (#856).
///
/// # Errors
///
/// As [`run_log_inner`], and `invalid` for a path that leaves the artifacts.
pub async fn run_artifacts_inner(
    pool: &PgPool,
    secrets: &std::sync::Arc<dyn knobas_secrets::SecretStore>,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    run_id: &str,
    path: Option<&str>,
) -> Result<Option<Vec<knobas_source::run::ArtifactEntry>>, IpcError> {
    let path = path.unwrap_or_default();
    artifact_path(path)?;
    let (entity, source) = run_adapter(pool, secrets, registry, run_id).await?;
    source
        .run_artifacts(&entity.to_string(), path)
        .await
        .map_err(|error| run_read_failed(&entity, error))
}

/// The head of one text artifact of run `run_id`, for *Preview* (#856).
///
/// # Errors
///
/// As [`run_artifacts_inner`].
pub async fn artifact_preview_inner(
    pool: &PgPool,
    secrets: &std::sync::Arc<dyn knobas_secrets::SecretStore>,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    run_id: &str,
    path: &str,
) -> Result<Option<knobas_source::run::ArtifactPreview>, IpcError> {
    artifact_path(path)?;
    let (entity, source) = run_adapter(pool, secrets, registry, run_id).await?;
    source
        .artifact_preview(&entity.to_string(), path)
        .await
        .map_err(|error| run_read_failed(&entity, error))
}

/// Stream `target` of run `run_id` into a new file in `dir` with the stored
/// credential (D8, #856), and answer the file's path; `None` for a source
/// that keeps no runs. The file is named as the source suggests
/// ([`crate::downloads::safe_name`]), numbered where the name is taken, and a
/// download that fails leaves no file.
///
/// # Errors
///
/// As [`run_artifacts_inner`]; a file that cannot be written is `internal`.
pub async fn download_run_file_inner(
    pool: &PgPool,
    secrets: &std::sync::Arc<dyn knobas_secrets::SecretStore>,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    run_id: &str,
    target: &knobas_source::run::DownloadTarget,
    dir: &std::path::Path,
) -> Result<Option<String>, IpcError> {
    use knobas_source::run::DownloadTarget;
    if let DownloadTarget::File { path } | DownloadTarget::Folder { path } = target {
        artifact_path(path)?;
    }
    let (entity, source) = run_adapter(pool, secrets, registry, run_id).await?;
    let mut sink = crate::downloads::FileSink::new(dir);
    let streamed = source
        .download(&entity.to_string(), target, &mut sink)
        .await;
    let finished = match streamed {
        Ok(true) => sink.finish().await.map(Some),
        Ok(false) => Ok(None),
        Err(error) => Err(error),
    };
    match finished {
        Ok(path) => Ok(path.map(|p| p.display().to_string())),
        Err(error) => {
            sink.discard().await;
            Err(run_read_failed(&entity, error))
        }
    }
}

/// A range of one run's log (#854).
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) before bring-up, and whatever
/// [`run_log_inner`] refuses with.
#[tauri::command]
pub async fn run_log<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    run_id: String,
    from: Option<u64>,
    count: Option<u32>,
) -> Result<Option<knobas_source::run::RunLog>, IpcError> {
    let state = crate::sources::state(&app)?;
    run_log_inner(
        &state.pool,
        &state.secrets,
        state.registry.as_ref(),
        &run_id,
        from,
        count,
    )
    .await
}

/// One run's tests (#855).
///
/// # Errors
///
/// As [`run_log`].
#[tauri::command]
pub async fn run_tests<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    run_id: String,
) -> Result<Option<knobas_source::run::RunTests>, IpcError> {
    let state = crate::sources::state(&app)?;
    run_tests_inner(
        &state.pool,
        &state.secrets,
        state.registry.as_ref(),
        &run_id,
    )
    .await
}

/// One test's last ten outcomes (#855).
///
/// # Errors
///
/// As [`run_log`].
#[tauri::command]
pub async fn test_history<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    run_id: String,
    test_id: String,
) -> Result<Option<Vec<knobas_source::run::TestOutcome>>, IpcError> {
    let state = crate::sources::state(&app)?;
    test_history_inner(
        &state.pool,
        &state.secrets,
        state.registry.as_ref(),
        &run_id,
        &test_id,
    )
    .await
}

/// One folder of a run's artifacts (#856).
///
/// # Errors
///
/// As [`run_log`].
#[tauri::command]
pub async fn run_artifacts<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    run_id: String,
    path: Option<String>,
) -> Result<Option<Vec<knobas_source::run::ArtifactEntry>>, IpcError> {
    let state = crate::sources::state(&app)?;
    run_artifacts_inner(
        &state.pool,
        &state.secrets,
        state.registry.as_ref(),
        &run_id,
        path.as_deref(),
    )
    .await
}

/// The head of one text artifact (#856).
///
/// # Errors
///
/// As [`run_log`].
#[tauri::command]
pub async fn artifact_preview<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    run_id: String,
    path: String,
) -> Result<Option<knobas_source::run::ArtifactPreview>, IpcError> {
    let state = crate::sources::state(&app)?;
    artifact_preview_inner(
        &state.pool,
        &state.secrets,
        state.registry.as_ref(),
        &run_id,
        &path,
    )
    .await
}

/// A run's log, an artifact, a folder or every artifact, into the downloads
/// folder (D8, #856).
///
/// # Errors
///
/// As [`run_log`], and `internal` when the system names no downloads folder.
#[tauri::command]
pub async fn download_run_file<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    run_id: String,
    target: knobas_source::run::DownloadTarget,
) -> Result<Option<String>, IpcError> {
    use tauri::Manager;
    let dir = app
        .path()
        .download_dir()
        .map_err(|error| IpcError::internal(format!("no downloads folder: {error}")))?;
    let state = crate::sources::state(&app)?;
    download_run_file_inner(
        &state.pool,
        &state.secrets,
        state.registry.as_ref(),
        &run_id,
        &target,
        &dir,
    )
    .await
}

/// The Builds list (#848).
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is coming
/// up, and whatever [`pipelines_inner`] refuses with.
#[tauri::command]
pub async fn pipelines(lifecycle: State<'_, Lifecycle>) -> Result<Vec<PipelineRow>, IpcError> {
    let pool = lifecycle.pool()?;
    pipelines_inner(&pool).await
}

/// One pipeline's runs (#848).
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is coming
/// up, and whatever [`pipeline_runs_inner`] refuses with. Before the sync
/// engine is up it answers the mirror alone ([`pipeline_runs_inner`]).
#[tauri::command]
pub async fn pipeline_runs<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    pipeline_id: String,
    limit: Option<u32>,
) -> Result<Vec<RunRow>, IpcError> {
    let pool = lifecycle.pool()?;
    match crate::sources::state(&app) {
        Ok(state) => {
            pipeline_runs_live_inner(
                &pool,
                &state.secrets,
                state.registry.as_ref(),
                &pipeline_id,
                limit,
            )
            .await
        }
        Err(_) => pipeline_runs_inner(&pool, &pipeline_id, limit).await,
    }
}

// -- *Viewed* on a pull request's files (#842) ------------------------------

/// The `knobas.setting` key holding which files of which pull request the
/// reader marked *Viewed* (#842, R-8.17): a JSON object of pull request entity
/// id to `{ "head": <sha or null>, "files": [<path>, …] }`. In
/// `knobas.setting` for the pins' reason ([`PINS_SETTING`]): short,
/// knobas-local state with no other home, never the source's.
///
/// A pull request's ticks hold for the head commit they were made on, the
/// one its source declares (`KindPaths.commit`); once the mirror reads
/// another head they read as none, and the next tick replaces them. A stored
/// value that is not an object reads as none and is replaced by the next tick.
pub const VIEWED_SETTING: &str = "code.viewed";

/// The head commit the mirror's record of `entity` declares, `None` where it
/// declares none or the mirror does not hold it.
const PR_HEAD: &str = concat!(
    "select ",
    knobas_core::declared_string!("$2", "commit"),
    " as head from sync.item i where i.entity_id = $1"
);

async fn pr_head(pool: &PgPool, entity: &EntityRef) -> Result<Option<String>, IpcError> {
    let declarations = declared_paths(pool).await?;
    let head: Option<Option<String>> = sqlx::query_scalar(PR_HEAD)
        .bind(entity.to_string())
        .bind(declarations.as_param())
        .fetch_optional(pool)
        .await?;
    Ok(head.flatten())
}

/// The files `stored` holds as viewed for `entity` at `head`: none where it
/// holds them for another head, or holds nothing readable.
fn viewed_at(stored: &serde_json::Value, entity: &str, head: Option<&str>) -> Vec<String> {
    let Some(entry) = stored.get(entity) else {
        return Vec::new();
    };
    if entry.get("head").and_then(serde_json::Value::as_str) != head {
        return Vec::new();
    }
    let mut files: Vec<String> = entry
        .get("files")
        .and_then(serde_json::Value::as_array)
        .map(|files| {
            files
                .iter()
                .filter_map(|f| f.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    files.dedup();
    files
}

/// The files of pull request `pr_id` the reader marked *Viewed* on its
/// current head (#842), in path order.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) for an id that is not an entity
/// id; [`Internal`](crate::IpcErrorCode::Internal) if a read fails.
pub async fn pr_viewed_files_inner(pool: &PgPool, pr_id: &str) -> Result<Vec<String>, IpcError> {
    let entity = EntityRef::parse(pr_id).map_err(IpcError::invalid)?;
    let head = pr_head(pool, &entity).await?;
    let stored: Option<serde_json::Value> =
        sqlx::query_scalar("select value from knobas.setting where key = $1")
            .bind(VIEWED_SETTING)
            .fetch_optional(pool)
            .await?;
    Ok(stored
        .map(|stored| viewed_at(&stored, &entity.to_string(), head.as_deref()))
        .unwrap_or_default())
}

/// Mark one file of pull request `pr_id` *Viewed* or not on its current head
/// (#842), and answer the files now viewed there, in path order. A tick on a
/// head the stored ones were not made on starts that head's set afresh.
///
/// # Errors
///
/// As [`pr_viewed_files_inner`]; [`Internal`](crate::IpcErrorCode::Internal)
/// if the write fails.
pub async fn set_pr_file_viewed_inner(
    pool: &PgPool,
    pr_id: &str,
    path: &str,
    viewed: bool,
) -> Result<Vec<String>, IpcError> {
    let entity = EntityRef::parse(pr_id).map_err(IpcError::invalid)?;
    let id = entity.to_string();
    let head = pr_head(pool, &entity).await?;
    let mut tx = pool.begin().await?;
    // The row first, then held: two ticks at once each read the other's.
    sqlx::query(
        "insert into knobas.setting (key, value) values ($1, '{}'::jsonb)
         on conflict (key) do nothing",
    )
    .bind(VIEWED_SETTING)
    .execute(&mut *tx)
    .await?;
    let stored: serde_json::Value =
        sqlx::query_scalar("select value from knobas.setting where key = $1 for update")
            .bind(VIEWED_SETTING)
            .fetch_one(&mut *tx)
            .await?;
    let mut files = viewed_at(&stored, &id, head.as_deref());
    files.retain(|f| f != path);
    if viewed {
        files.push(path.to_owned());
        files.sort();
    }
    let mut all = match stored {
        serde_json::Value::Object(all) => all,
        _ => serde_json::Map::new(),
    };
    if files.is_empty() {
        all.remove(&id);
    } else {
        all.insert(id, serde_json::json!({ "head": head, "files": files }));
    }
    sqlx::query("update knobas.setting set value = $2, updated_at = now() where key = $1")
        .bind(VIEWED_SETTING)
        .bind(serde_json::Value::Object(all))
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(files)
}

/// The files of a pull request marked *Viewed* on its head (#842).
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is coming
/// up, and whatever [`pr_viewed_files_inner`] refuses with.
#[tauri::command]
pub async fn pr_viewed_files(
    lifecycle: State<'_, Lifecycle>,
    pr_id: String,
) -> Result<Vec<String>, IpcError> {
    let pool = lifecycle.pool()?;
    pr_viewed_files_inner(&pool, &pr_id).await
}

/// Mark a pull request's file *Viewed* or not (#842).
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is coming
/// up, and whatever [`set_pr_file_viewed_inner`] refuses with.
#[tauri::command]
pub async fn set_pr_file_viewed(
    lifecycle: State<'_, Lifecycle>,
    pr_id: String,
    path: String,
    viewed: bool,
) -> Result<Vec<String>, IpcError> {
    let pool = lifecycle.pool()?;
    set_pr_file_viewed_inner(&pool, &pr_id, &path, viewed).await
}

// -- pins and *This job* (#849, #850) ---------------------------------------

/// Pin or unpin one pipeline (#849, R-9.3), and answer what is now stored.
///
/// The pins are a set: pinning twice keeps one, unpinning what is not pinned
/// changes nothing. They are kept by entity id and never checked against the
/// mirror, so a pin outlives a sync that withdrew its pipeline and is drawn
/// again when it comes back. A stored value that is not an array (an older or
/// a broken knobas) reads as no pins and is replaced by the first write.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) for an id that is not an entity
/// id; [`Internal`](crate::IpcErrorCode::Internal) if the write fails.
pub async fn set_pipeline_pinned_inner(
    pool: &PgPool,
    pipeline_id: &str,
    pinned: bool,
) -> Result<bool, IpcError> {
    let entity = EntityRef::parse(pipeline_id).map_err(IpcError::invalid)?;
    let stored: serde_json::Value = sqlx::query_scalar(
        "insert into knobas.setting as s (key, value)
         values ($1, case when $3 then jsonb_build_array($2::text) else '[]'::jsonb end)
         on conflict (key) do update set
           value = (select coalesce(jsonb_agg(distinct x order by x), '[]'::jsonb)
                      from (select jsonb_array_elements_text(
                                     case when jsonb_typeof(s.value) = 'array'
                                          then s.value else '[]'::jsonb end) as x
                            union select $2::text where $3) kept
                     where $3 or x <> $2),
           updated_at = now()
         returning value",
    )
    .bind(PINS_SETTING)
    .bind(entity.to_string())
    .bind(pinned)
    .fetch_one(pool)
    .await?;
    let id = entity.to_string();
    Ok(stored
        .as_array()
        .is_some_and(|ids| ids.iter().any(|v| v.as_str() == Some(id.as_str()))))
}

/// Pin or unpin a pipeline (#849).
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is coming
/// up, and whatever [`set_pipeline_pinned_inner`] refuses with.
#[tauri::command]
pub async fn set_pipeline_pinned(
    lifecycle: State<'_, Lifecycle>,
    pipeline_id: String,
    pinned: bool,
) -> Result<bool, IpcError> {
    let pool = lifecycle.pool()?;
    set_pipeline_pinned_inner(&pool, &pipeline_id, pinned).await
}

// -- Run… presets (#852) -----------------------------------------------------

/// The Run… presets of one pipeline (#852, R-9.7): [`crate::run_presets::list`].
///
/// # Errors
///
/// As [`crate::run_presets::list`].
pub async fn run_presets_inner(
    pool: &PgPool,
    pipeline_id: &str,
) -> Result<Vec<crate::run_presets::RunPreset>, IpcError> {
    crate::run_presets::list(pool, pipeline_id).await
}

/// Save one Run… preset (#852, R-9.7): [`crate::run_presets::save`].
///
/// # Errors
///
/// As [`crate::run_presets::save`].
pub async fn save_run_preset_inner(
    pool: &PgPool,
    preset: crate::run_presets::RunPreset,
) -> Result<crate::run_presets::RunPreset, IpcError> {
    crate::run_presets::save(pool, preset).await
}

/// Remove one Run… preset (#996): [`crate::run_presets::delete`].
///
/// # Errors
///
/// As [`crate::run_presets::delete`].
pub async fn delete_run_preset_inner(
    pool: &PgPool,
    pipeline_id: &str,
    name: &str,
) -> Result<(), IpcError> {
    crate::run_presets::delete(pool, pipeline_id, name).await
}

/// The Run… presets of a pipeline (#852).
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is coming
/// up, and whatever [`run_presets_inner`] refuses with.
#[tauri::command]
pub async fn run_presets(
    lifecycle: State<'_, Lifecycle>,
    pipeline_id: String,
) -> Result<Vec<crate::run_presets::RunPreset>, IpcError> {
    let pool = lifecycle.pool()?;
    run_presets_inner(&pool, &pipeline_id).await
}

/// Save a Run… preset, replacing the pipeline's preset of that name (#852).
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is coming
/// up, and whatever [`save_run_preset_inner`] refuses with.
#[tauri::command]
pub async fn save_run_preset(
    lifecycle: State<'_, Lifecycle>,
    preset: crate::run_presets::RunPreset,
) -> Result<crate::run_presets::RunPreset, IpcError> {
    let pool = lifecycle.pool()?;
    save_run_preset_inner(&pool, preset).await
}

/// Remove a pipeline's Run… preset by name (#996).
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is coming
/// up, and whatever [`delete_run_preset_inner`] refuses with.
#[tauri::command]
pub async fn delete_run_preset(
    lifecycle: State<'_, Lifecycle>,
    pipeline_id: String,
    name: String,
) -> Result<(), IpcError> {
    let pool = lifecycle.pool()?;
    delete_run_preset_inner(&pool, &pipeline_id, &name).await
}

/// Why a pipeline is on a job (#850, R-9.1, R-9.2). Tagged on `kind`, so a
/// link set (#866) joins as one more arm without changing these.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PipelineReason {
    /// One of its runs built a branch the job reaches: a reached branch, by
    /// its title, or the branch a reached pull request is opened from (its
    /// declared `head`). *your branch*.
    Branch { branch: String },
    /// One of its runs checked a pull request the job reaches: it ran on the
    /// pull request's declared `commit` (its head), or the job reaches the run
    /// through the pull request. *PR check*.
    PrCheck { pr: String },
    /// The job reaches the pipeline, or one of its runs, some other way: a
    /// direct link, the parent's links (R-7.3), or a link set (#866). `step` and `origin` are
    /// the reach's own ([`knobas_core::reach::Reached`]).
    Linked {
        step: String,
        origin: knobas_core::reach::Origin,
    },
}

/// One pipeline on a job, with every reason it is there (#850): branches
/// first, then pull requests, then links, as prototype-6 lists them.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct JobPipeline {
    pub pipeline_id: String,
    pub reasons: Vec<PipelineReason>,
}

/// The pipelines a job's reach puts on it, with their reasons (#850).
///
/// `$1` is the declarations; `$2`, `$3` and `$4` are the reach's rows as
/// three arrays: what is reached, the first step of its path, and its
/// origin. Every field is a declared one (ADR-0007): a run's `branch` and
/// `pipeline`, a pull request's `head` and `commit`, a run's revisions as
/// `sync.item_revision` keeps them (#732). A branch's name is its title,
/// which is what Gitea titles a branch with and what a pull request's `head`
/// names (#712). A run built on `refs/heads/<name>` counts as `<name>`.
const JOB_PIPELINES: &str = concat!(
    "with ",
    frame_kinds_cte!(),
    ",
reached as (
  select * from unnest($2::text[], $3::text[], $4::text[]) as r(id, step, origin)
),
names as (
  select i.title as name
    from reached r join sync.live_item i on i.entity_id = r.id
   where i.kind = 'branch'
  union
  select ",
    knobas_core::declared_string!("$1", "head"),
    "
    from reached r join sync.live_item i on i.entity_id = r.id
),
heads as (
  select r.id as pr, ",
    knobas_core::declared_string!("$1", "commit"),
    " as sha
    from reached r join sync.live_item i on i.entity_id = r.id
   where i.kind = 'pr'
),
p as (
  select i.entity_id, i.source_id, ",
    knobas_core::declared_string!("$1", "pipeline"),
    " as pkey
    from frames f
    join sync.live_item i on i.source_id = f.source_id and i.kind = f.kind
   where f.frame = 'pipeline'
),
x as (
  select i.entity_id, i.source_id, ",
    knobas_core::declared_string!("$1", "pipeline"),
    " as pkey, ",
    knobas_core::declared_string!("$1", "branch"),
    " as branch
    from frames f
    join sync.live_item i on i.source_id = f.source_id and i.kind = f.kind
   where f.frame = 'run'
),
reasons as (
  select p.entity_id as pipeline, 1 as ord, 'branch' as kind, n.name as value, null::text as origin
    from x
    join p on p.source_id = x.source_id and p.pkey = x.pkey
    join names n on n.name is not null
                and (x.branch = n.name or x.branch = 'refs/heads/' || n.name)
  union
  select p.entity_id, 2, 'pr_check', h.pr, null
    from heads h
    join sync.item_revision rv on rv.revision = h.sha
    join x on x.entity_id = rv.entity_id
    join knobas.source_config s on s.id = x.source_id and s.kind = rv.adapter_kind
    join p on p.source_id = x.source_id and p.pkey = x.pkey
  union
  select p.entity_id,
         case when se.kind = 'pr' and r.origin = 'own' then 2 else 3 end,
         case when se.kind = 'pr' and r.origin = 'own' then 'pr_check' else 'linked' end,
         r.step,
         case when se.kind = 'pr' and r.origin = 'own' then null else r.origin end
    from reached r
    left join x on x.entity_id = r.id
    join p on p.entity_id = r.id or (p.source_id = x.source_id and p.pkey = x.pkey)
    left join knobas.entity se on se.id = r.step
)
select pipeline, kind, value, origin from reasons order by pipeline, ord, value
"
);

/// The pipelines on the job of `ticket_id` (#850), each once with its
/// reasons, by entity id. A ticket that reaches nothing has none.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
pub async fn job_pipelines_inner(
    pool: &PgPool,
    ticket_id: &str,
) -> Result<Vec<JobPipeline>, IpcError> {
    let reached = knobas_core::reach::reach(pool, ticket_id).await?;
    let ids: Vec<&str> = reached.iter().map(|r| r.entity_id.as_str()).collect();
    let steps: Vec<&str> = reached.iter().map(|r| r.step.as_str()).collect();
    let origins: Vec<&str> = reached
        .iter()
        .map(|r| match r.origin {
            knobas_core::reach::Origin::Own => "own",
            knobas_core::reach::Origin::Parent { .. } => "parent",
            knobas_core::reach::Origin::LinkSet { .. } => "link_set",
        })
        .collect();
    let declarations = declared_paths(pool).await?;
    let rows: Vec<(String, String, String, Option<String>)> = sqlx::query_as(JOB_PIPELINES)
        .bind(declarations.as_param())
        .bind(&ids)
        .bind(&steps)
        .bind(&origins)
        .fetch_all(pool)
        .await?;
    let mut out: Vec<JobPipeline> = Vec::new();
    for (pipeline, kind, value, origin) in rows {
        let reason = match kind.as_str() {
            "branch" => PipelineReason::Branch { branch: value },
            "pr_check" => PipelineReason::PrCheck { pr: value },
            _ => PipelineReason::Linked {
                origin: match origin.as_deref() {
                    Some("parent") => knobas_core::reach::Origin::Parent {
                        ticket: value.clone(),
                    },
                    // A set's member is its own first step (#866), so the
                    // reach row it came from names the set.
                    Some("link_set") => reached
                        .iter()
                        .find(|r| {
                            r.step == value
                                && matches!(r.origin, knobas_core::reach::Origin::LinkSet { .. })
                        })
                        .map_or(knobas_core::reach::Origin::Own, |r| r.origin.clone()),
                    _ => knobas_core::reach::Origin::Own,
                },
                step: value,
            },
        };
        match out.last_mut() {
            Some(last) if last.pipeline_id == pipeline => last.reasons.push(reason),
            _ => out.push(JobPipeline {
                pipeline_id: pipeline,
                reasons: vec![reason],
            }),
        }
    }
    Ok(out)
}

/// The pipelines on a job, with their reasons (#850).
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is coming
/// up, and whatever [`job_pipelines_inner`] refuses with.
#[tauri::command]
pub async fn job_pipelines(
    lifecycle: State<'_, Lifecycle>,
    entity_id: String,
) -> Result<Vec<JobPipeline>, IpcError> {
    let pool = lifecycle.pool()?;
    job_pipelines_inner(&pool, &entity_id).await
}

// -- what Code draws beside a row --------------------------------------------

/// What Code draws beside one mirrored row (#966, #973, #986): a pull
/// request's, branch's or commit's author, whether it is mine, and a
/// repository's language and teams.
///
/// Not on [`EntityRow`]: a room line and a launcher row are one wire shape
/// over two structs (`commands::search`'s pin), read by a dozen statements,
/// and these four facts are Code's alone. Code asks for the rows it holds.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CodeFacts {
    /// The row asked about.
    pub entity_id: String,
    /// The row's §4.1 author, as its source spells the account: Gitea's
    /// login of a pull request's opener, of a commit's author, of a branch
    /// tip's author. `None` where the source named none.
    pub author: Option<String>,
    /// Whether `author` is its own source's configured username, trimmed,
    /// compared as the attention read compares it (#712). `false` for a
    /// source with no username, whose rows are nobody's.
    pub mine: bool,
    /// A repository's declared language; `None` on a miss (Gitea's `""`).
    pub language: Option<String>,
    /// A repository's declared teams, in the record's order; empty on a miss.
    pub teams: Vec<String>,
}

/// One row per live item among `$1`; `$2` is the declarations.
const CODE_FACTS: &str = concat!(
    r#"
select i.entity_id, i.author, coalesce(i.author = me.me, false) as mine,
       "#,
    knobas_core::declared_string!("$2", "language"),
    r#" as language,
       coalesce((select array_agg(v.value) from "#,
    knobas_core::declared_list!("$2", "teams"),
    r#" v), '{}') as teams
  from sync.live_item i
  left join lateral (select nullif(btrim(s.config->>'username'), '') as me
                       from knobas.source_config s where s.id = i.source_id) me on true
 where i.entity_id = any($1)
"#
);

/// [`code_facts`] over a pool, for a test.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
pub async fn code_facts_inner(
    pool: &PgPool,
    entity_ids: &[String],
) -> Result<Vec<CodeFacts>, IpcError> {
    let declarations = declared_paths(pool).await?;
    let rows = sqlx::query(CODE_FACTS)
        .bind(entity_ids)
        .bind(declarations.as_param())
        .fetch_all(pool)
        .await?;
    Ok(rows
        .iter()
        .map(|row| CodeFacts {
            entity_id: row.get("entity_id"),
            author: row.get("author"),
            mine: row.get("mine"),
            language: row.get("language"),
            teams: row.get("teams"),
        })
        .collect())
}

/// What Code draws beside each of `entity_ids` (#966): an id the mirror does
/// not hold live is left out.
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is coming
/// up, [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
#[tauri::command]
pub async fn code_facts(
    lifecycle: State<'_, Lifecycle>,
    entity_ids: Vec<String>,
) -> Result<Vec<CodeFacts>, IpcError> {
    let pool = lifecycle.pool()?;
    code_facts_inner(&pool, &entity_ids).await
}

// -- the URL a paste names --------------------------------------------------

/// What a pasted URL resolved to: the entity, and the kind that says where it
/// opens (issue #496).
///
/// Deliberately not an [`EntityRow`] and not an [`EntityDetail`]. A paste
/// answers one question -- *which entity is this link* -- and the frontend
/// then navigates to that entity's own address, at which point the detail is
/// fetched by the read that already exists. Answering with a row here would
/// make every paste pay for a title and a path nothing on the way draws, and
/// would put a second statement in the tree that has to keep in step with
/// `sync.live_item`'s columns.
///
/// `kind` rather than only the id because an entity id names a *namespace*
/// and a key (`gitea:acme/svc#142`), not a kind, and the address an entity
/// opens at is `#/<kind>/<id>` -- or, for an asset or a route, the Tree's own
/// encoder. The frontend has one function that turns the pair into an
/// address; it has nothing that turns an id alone into one.
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(feature = "test-util", derive(serde::Deserialize))]
pub struct UrlMatch {
    pub entity_id: String,
    pub kind: String,
}

/// The one statement that reads `sync.item.web_url` (#496).
///
/// **`sync.item`, not `sync.live_item`**, and for the two reasons
/// [`DETAIL`] reaches past that view as well: a withdrawn entity still opens
/// and says it is gone (spec #491 story 15 -- *"a stale link explains
/// itself"*), and a disabled source's items still open. It is the **third**
/// reader to do so, and `CONTEXT.md`'s **Live item** entry is the list of
/// them: a fourth owes a reason of its own and a line there. Resolving to a
/// tombstone and letting the detail's banner do the explaining is a better
/// answer than *Not in the mirror*, which would send the reader to the
/// browser to discover the same thing.
///
/// Both sides of the comparison go through `web_url_normalized!` -- the
/// stored column and the pasted parameter -- so there is exactly one rule and
/// it cannot be applied to one side only. `crates/knobas-core/src/web_url.rs`
/// carries the rule and the reasoning; migration `0023` carries the index.
///
/// `where i.web_url is not null` is the index's own predicate, repeated so the
/// planner may use the partial index. `order by i.entity_id limit 1`: nothing
/// stops two mirrored items reporting one address, and an ambiguous paste
/// must answer the same entity on every run rather than whichever row the
/// plan reached first.
pub const RESOLVE_URL: &str = concat!(
    "select i.entity_id, i.kind
       from sync.item i
      where i.web_url is not null
        and ",
    knobas_core::web_url_normalized!("i.web_url"),
    " = ",
    knobas_core::web_url_normalized!("$1::text"),
    "
      order by i.entity_id
      limit 1"
);

/// The entity a URL names, or `None` -- *"a URL in, an entity reference or a
/// miss out"* (spec #491).
///
/// **Never a fetch from a source** (spec #491 story 16): a
/// paste while the tunnel is down is a miss and not a hang, so this is one
/// read on the local mirror and nothing else.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) if `url` is not an absolute
/// `http`/`https` URL. That is not a miss and must not read as one: a miss is
/// *the mirror does not hold this link*, and offering *Open in browser* on a
/// `file:///` or a half-typed word would offer the reader a refusal
/// (`app/src/lib/shell/open-external.ts` allows those two schemes and no
/// others). The frontend recognises the URL before it calls, so this is a
/// guard rather than a path.
///
/// [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
pub async fn resolve_url_inner(pool: &PgPool, url: &str) -> Result<Option<UrlMatch>, IpcError> {
    if !is_web_url(url) {
        return Err(IpcError::invalid(format!(
            "{url:?} is not an http or https URL"
        )));
    }

    let row = sqlx::query(RESOLVE_URL)
        .bind(url)
        .fetch_optional(pool)
        .await?;

    Ok(row.map(|row| UrlMatch {
        entity_id: row.get("entity_id"),
        kind: row.get("kind"),
    }))
}

/// Whether `url` is an absolute `http`/`https` URL.
///
/// The scheme is the only part checked, and it is checked
/// case-insensitively because a scheme is case-insensitive and the
/// normalisation down-cases it anyway. Everything past it is the
/// normalisation's business: a URL with no host simply matches no row, which
/// is the miss `web_url.rs` describes rather than a second opinion about what a
/// well-formed URL is.
pub(crate) fn is_web_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

/// The entity a pasted URL names, or `null` for one the mirror does not hold.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the
/// database is still coming up, and whatever [`resolve_url_inner`] refuses
/// with.
#[tauri::command]
pub async fn resolve_url(
    lifecycle: State<'_, Lifecycle>,
    url: String,
) -> Result<Option<UrlMatch>, IpcError> {
    let pool = lifecycle.pool()?;
    resolve_url_inner(&pool, &url).await
}

/// The `limit` most recent activity lines, globally or for one entity.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) if `entity` is not an entity id,
/// [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
pub async fn recent_activity_inner(
    pool: &PgPool,
    limit: u32,
    entity: Option<&EntityRef>,
) -> Result<Vec<ActivityRow>, IpcError> {
    Ok(knobas_core::activity::recent(pool, i64::from(limit), entity).await?)
}

/// The `limit` most recent activity-log lines, newest first.
///
/// `entityId` scopes the read to one entity's history; omitting it is the
/// global stream the status bar reads.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the
/// database is still coming up, [`Invalid`](crate::IpcErrorCode::Invalid) for
/// a malformed `entityId`, [`Internal`](crate::IpcErrorCode::Internal) for a
/// query failure.
#[tauri::command]
pub async fn recent_activity(
    lifecycle: State<'_, Lifecycle>,
    limit: u32,
    entity_id: Option<String>,
) -> Result<Vec<ActivityRow>, IpcError> {
    let pool = lifecycle.pool()?;
    let entity = entity_id
        .as_deref()
        .map(EntityRef::parse)
        .transpose()
        .map_err(IpcError::invalid)?;
    recent_activity_inner(&pool, limit, entity.as_ref()).await
}

// -- the link writes --------------------------------------------------------
//
// Two commands, and only two. The *read* stays on `get_entity`: `entries_of`
// is undirected, so an entity's backlinks are the same query as its links and
// need no endpoint of their own, and the target picker reuses `search`.

/// The relation a link takes when the caller names none.
///
/// A quick link costs no extra decisions, which is only true if "no relation"
/// is a relation rather than an empty string: the panel groups by this value
/// and an empty group header is a row nobody can read.
pub const DEFAULT_RELATION: &str = "related";

/// Who a link made through this surface belongs to, in `created_by` and in the
/// activity line's `actor`.
///
/// knobas has no identity system: there is one person using it, and the only
/// other actor the log knows is `sync:<source_id>`. Naming it once here keeps
/// the two spellings from drifting.
const ACTOR: &str = "user";

/// What a link mutation left behind: the row, and the activity line announcing
/// it.
///
/// Not a wire type -- the commands return the link and emit the line. It exists
/// so the behaviour is reachable from a test without a Tauri app: the emit is
/// the only part of these commands that a `#[tauri::command]` shell adds, and
/// everything worth asserting is in here.
#[derive(Debug)]
pub struct LinkMutation {
    pub link: LinkRow,
    pub activity: ActivityRow,
    /// The proposal this link displaced, when there was one (#70).
    ///
    /// Drawing the *reverse* of a live proposal withdraws it -- see
    /// [`knobas_core::suggest::resolve_edge`] -- and a proposal leaving the tray
    /// is a mutation, so it gets its own line rather than disappearing quietly.
    /// `None` everywhere else, which is every other path in this module.
    pub superseded: Option<ActivityRow>,
}

/// Text the user did not type is no text.
///
/// A dialog hands back `""` for a field left alone, and `Some("")` is not the
/// same fact as `None` anywhere downstream: an empty relation is an unreadable
/// group header, and an empty note is a line the panel would draw with nothing
/// in it.
fn present(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|text| !text.is_empty())
}

/// The relation a link will carry, folded to the one spelling that groups it.
///
/// A relation is a *key*, not prose: the panel groups rows by this value and
/// the duplicate rule compares it verbatim, so `Blocks` and `blocks` would be
/// two headers for one relationship and neither would see the other as a
/// duplicate. Björn's ruling on the review of #52 (2026-08-28) is that they are
/// one relation, and folding on write is what makes that true everywhere at
/// once -- the stored value, the unique index, and every later reader.
///
/// Lowercase because the curated list, [`DEFAULT_RELATION`] and the design's
/// own vocabulary (`blocks`, `implements`, `documents`, `depends-on`) are all
/// written that way; a folded value is therefore already the spelling #53's
/// inverse-label lookup will key on. Display capitalization is the panel's, and
/// it has the whole string to do it from.
///
/// [`str::to_lowercase`] rather than the ASCII form: a user typing a relation
/// in their own language should get the same folding an English one gets.
///
/// The **note** is deliberately not folded -- it is prose in the user's own
/// words and nothing groups by it.
fn relation_of(value: Option<&str>) -> String {
    present(value).map_or_else(|| DEFAULT_RELATION.to_owned(), |named| named.to_lowercase())
}

/// What an activity line says about a link: the other end, the relation, and
/// which link it was.
///
/// The line is named on the link's `from` end, so `to_id` is the end the reader
/// does not already know. One helper, because `linked` and `unlinked` describe
/// the same link and a reader of the log has to be able to pair them.
///
/// **These three keys are a shape a second writer depends on.**
/// `knobas_core::note`'s `DrawnLink` spells them again for the `born[]` entries
/// of a note's `created` line (#524), so that a born link withdrawn from the
/// panel pairs with the birth that drew it. They are not one type: this one
/// carries a fourth key for a proposal's `reason` and lives where `LinkRow`
/// does, while `born[]` is composed from an insert's `returning` inside the
/// store's own transaction, where no `LinkRow` exists. What holds them equal is
/// `a_notes_birth_is_one_line_and_a_withdrawn_born_link_pairs_with_it`, which
/// reads one against the other -- so a key **renamed** here fails that test.
/// A key *added* here would not, and would not need to: `born[]` promising less
/// than a line does is what it already promises.
fn link_detail(link: &LinkRow) -> serde_json::Value {
    let mut detail = serde_json::json!({
        "link_id": link.id,
        "to_id": link.to_id,
        "relation": link.relation,
    });
    // Only when there is one, which is only ever for a link knobas proposed:
    // "accepted" with no reason beside it is a line that says a decision was
    // made and not what it was about. A key that is absent and a key that is
    // `null` are different facts to a reader of the log, and the second is the
    // one worth avoiding.
    if let (Some(map), Some(reason)) = (detail.as_object_mut(), link.reason.as_deref()) {
        map.insert("reason".to_owned(), serde_json::Value::from(reason));
    }
    detail
}

/// Draw a link between two entities.
///
/// `relation` defaults to [`DEFAULT_RELATION`] and is folded by
/// [`relation_of`]; `note` is optional and is kept as typed; the origin is
/// always [`Origin::Manual`] -- it is deliberately not client-suppliable in v1,
/// because the other origins belong to the suggestion engine and to import.
///
/// An entity may not be linked to itself.
///
/// The behaviour lives here rather than in the command so it is reachable from
/// a test: a `#[tauri::command]` cannot be called directly.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) if either id is not an entity id,
/// or if the two are the same entity;
/// [`NotFound`](crate::IpcErrorCode::NotFound) if either endpoint has no local
/// entity -- linking to something that has not synced yet is an ordinary event,
/// not a fault; [`Conflict`](crate::IpcErrorCode::Conflict) if that pair is
/// already actively linked under that relation;
/// [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
pub async fn create_link_inner(
    pool: &PgPool,
    from_id: &str,
    to_id: &str,
    relation: Option<&str>,
    note: Option<&str>,
) -> Result<LinkMutation, IpcError> {
    // Before any query, as `get_entity_inner` does: a malformed id is a bad
    // address, and the two want different words on screen.
    let from = EntityRef::parse(from_id).map_err(IpcError::invalid)?;
    let to = EntityRef::parse(to_id).map_err(IpcError::invalid)?;
    // Also before any query: an entity linked to itself is a bad *request*, not
    // a missing thing -- both ends resolve, they are simply the same one, and
    // the row would draw as a panel entry pointing at the entity you are
    // already looking at. Björn's ruling on the review of #52 (2026-08-28).
    if from == to {
        return Err(IpcError::invalid(format!(
            "{from} cannot be linked to itself"
        )));
    }

    let relation = relation_of(relation);

    // The pair may already carry a *proposal*: `link_pair_active_idx` spans
    // proposals and links alike (one active edge per pair per relation, whatever
    // its state), so knobas suggesting this link is what would refuse it. The
    // proposal is therefore answered *before* the insert rather than after it
    // fails -- a unique violation aborts the transaction it happens in, and
    // there would be nothing left to answer it with.
    //
    // One transaction, because `Superseded` is only half a mutation: the
    // withdrawn proposal and the link that replaced it land together or not at
    // all.
    let mut tx = pool.begin().await.map_err(knobas_core::CoreError::from)?;
    let answered = suggest::resolve_edge(&mut tx, &from, &to, &relation).await?;

    let (link, superseded) = match answered {
        // Drawing by hand the link knobas proposed is the same act as pressing
        // *Accept*, and the alternative is telling the user "already linked"
        // about a pair whose links panel is empty.
        suggest::Edge::Promoted(link) => (link, None),
        // The user drew the proposal's reverse, which contradicts its direction.
        // The user wins: the proposal is withdrawn and the link they drew is
        // written with the ends they gave it.
        suggest::Edge::Superseded(proposal) => {
            let line = record_link_activity_with(&mut tx, "dismissed", &proposal).await?;
            let link = write_link(&mut tx, &from, &to, &relation, note).await?;
            (link, Some(line))
        }
        // Nothing proposed for this pair. Whatever refuses the write now is a
        // real link, which is what `conflict` has always meant here.
        suggest::Edge::Open => (
            write_link(&mut tx, &from, &to, &relation, note).await?,
            None,
        ),
    };

    let activity = record_link_activity_with(&mut tx, "linked", &link).await?;
    tx.commit().await.map_err(knobas_core::CoreError::from)?;
    Ok(LinkMutation {
        link,
        activity,
        superseded,
    })
}

/// The hand-drawn link itself, once the tray has been answered.
async fn write_link(
    conn: &mut sqlx::PgConnection,
    from: &EntityRef,
    to: &EntityRef,
    relation: &str,
    note: Option<&str>,
) -> Result<LinkRow, IpcError> {
    Ok(knobas_core::link::create_with(
        conn,
        from,
        to,
        relation,
        Origin::Manual,
        present(note),
        ACTOR,
    )
    .await?)
}

/// Withdraw a link.
///
/// `Ok(None)` when the link was already withdrawn: the store is idempotent, and
/// nothing was mutated, so nothing is written to the log and nothing is
/// announced.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) if `link_id` is not a UUID;
/// [`NotFound`](crate::IpcErrorCode::NotFound) if no link carries it;
/// [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
pub async fn unlink_inner(pool: &PgPool, link_id: &str) -> Result<Option<LinkMutation>, IpcError> {
    let id: Uuid = link_id
        .parse()
        .map_err(|_| IpcError::invalid(format!("{link_id} is not a link id")))?;

    let Some(link) = knobas_core::link::unlink(pool, id).await? else {
        return Ok(None);
    };
    let activity = record_link_activity(pool, "unlinked", &link).await?;
    Ok(Some(LinkMutation {
        link,
        activity,
        superseded: None,
    }))
}

/// One line, on the end the link was drawn from.
///
/// The to-end's own history panel therefore does not list it -- a known and
/// accepted v1 limitation; that entity's links panel still shows the link. If
/// the inbox (#45) ever needs the other side, the fix is a read-side change and
/// not a second row.
async fn record_link_activity(
    pool: &PgPool,
    verb: &str,
    link: &LinkRow,
) -> Result<ActivityRow, IpcError> {
    let from = EntityRef::parse(&link.from_id).map_err(IpcError::internal)?;
    Ok(knobas_core::activity::record(pool, ACTOR, verb, Some(&from), link_detail(link)).await?)
}

/// [`record_link_activity`], against an executor the caller chooses, so a line
/// can be written inside the transaction that made the mutation it describes.
async fn record_link_activity_with(
    conn: &mut sqlx::PgConnection,
    verb: &str,
    link: &LinkRow,
) -> Result<ActivityRow, IpcError> {
    let from = EntityRef::parse(&link.from_id).map_err(IpcError::internal)?;
    Ok(
        knobas_core::activity::record_with(conn, ACTOR, verb, Some(&from), link_detail(link))
            .await?,
    )
}

/// Put an activity line on `activity:new`.
///
/// Best-effort, like every other emit in this app: a failure means no window is
/// listening, which is not a reason to fail a write that already landed. The
/// command emits it itself rather than re-reading the log the way the scheduler
/// does -- the write hands its row back precisely so it does not have to guess
/// which of the table's rows is its own.
fn announce<R: tauri::Runtime>(app: &tauri::AppHandle<R>, row: ActivityRow) {
    crate::events::emit(app, None, crate::events::ACTIVITY_NEW, row);
}

/// Draw a link between two entities, and announce it.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the database
/// is still coming up, and whatever [`create_link_inner`] refuses with.
#[tauri::command]
pub async fn create_link<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    from_id: String,
    to_id: String,
    relation: Option<String>,
    note: Option<String>,
) -> Result<LinkRow, IpcError> {
    let pool = lifecycle.pool()?;
    let written = create_link_inner(
        &pool,
        &from_id,
        &to_id,
        relation.as_deref(),
        note.as_deref(),
    )
    .await?;
    // The displaced proposal first, so the strip reads in the order the two
    // things happened rather than in the order this function holds them.
    if let Some(line) = written.superseded {
        announce(&app, line);
    }
    announce(&app, written.activity);
    Ok(written.link)
}

/// Withdraw a link, and announce it.
///
/// Idempotent: withdrawing an already-withdrawn link succeeds, writes no second
/// line and announces nothing.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the database
/// is still coming up, and whatever [`unlink_inner`] refuses with.
#[tauri::command]
pub async fn unlink<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    link_id: String,
) -> Result<(), IpcError> {
    let pool = lifecycle.pool()?;
    if let Some(written) = unlink_inner(&pool, &link_id).await? {
        announce(&app, written.activity);
    }
    Ok(())
}

/// Ask a source to change something (issue #43): the one way the UI starts a
/// write-back.
///
/// `payload` is a serialized `knobas_source::WriteOp` -- the same shape
/// [`crate::commands::sources::pending_writes`] hands back and
/// [`crate::commands::sources::amend_write`] takes. Untyped on the wire for
/// #42's ratified reasoning: `WriteOp` grows per milestone (ADR-0006), and a
/// typed argument would drag the SPI's enum onto the IPC surface and make
/// every growth an IPC change.
///
/// **The write is queued, not sent** -- and the row that comes back is the
/// write *as queued*, before the attempt, because that is what happened.
/// Since issue #752 the answer comes as soon as the row exists: the send runs
/// in the background (`sources::write_queue::submit_in_background`), so a
/// dialog closes on the local insert rather than on a source's round trip.
/// Read the outcome back with `pending_writes`, or watch `activity:new`: every
/// queue transition writes a line, its `detail.write_id` naming the write. A UI that reported "sent" from this return
/// value would be reporting a hope. The queue is what decides send, pend or
/// hold (issue #42), and there is no path around it.
///
/// **There is no `source_id` argument.** Interfaces §4.1 makes the instance id
/// and the `EntityRef` namespace the same string, so the target already names
/// the source; a second argument could only agree or contradict, and a
/// contradiction would aim a write at a source the target does not belong to.
///
/// # Errors
///
/// `invalid` if the payload is not a write op, if its target is not an entity
/// id, or if the source does not offer that op; `not_found` if the target
/// names a source that is not configured;
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up.
///
/// **`link_to`** (#984, a ticket's entity id) asks for a link from what a
/// `CreateBranch` or `CreatePullRequest` makes to that ticket, drawn once the
/// write has been sent and a sync of the source brings it in, on this send or
/// a later tick (#1050: the intent is held on the queued row,
/// `sources::write_queue::submit_linked_in_background`); `invalid` for any
/// other op, `not_found` for a ticket the mirror does not hold. Absent, the
/// command is what it was.
#[tauri::command]
pub async fn submit_write<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    payload: serde_json::Value,
    link_to: Option<String>,
) -> Result<knobas_core::write_queue::QueuedWrite, IpcError> {
    let state = crate::sources::state(&app)?;
    match link_to {
        None => crate::sources::write_queue::submit_in_background(&state, payload).await,
        Some(ticket) => {
            crate::sources::write_queue::submit_linked_in_background(&state, payload, ticket).await
        }
    }
}

/// The statuses this ticket's workflow offers from where it stands **right
/// now** (#498, spec #491's stream 3).
///
/// The read the status select was built without. Until this existed the select
/// offered the statuses the source's *corpus* had been seen to use -- every
/// column the mini board draws for that source -- and the adapter discovered at
/// write time that half of them were unreachable, which reached the reader as a
/// refused row in the write queue rather than as an offer that was never made.
/// This asks the source instead.
///
/// **A read straight through to the adapter, with no mirror in it and nothing
/// stored.** `CONTEXT.md`'s *reachable transition* is an answer about now, so
/// there is no row to cache it in and no event to invalidate one with; the
/// answer is good for as long as the detail is open, and the write's own
/// resolution -- unchanged -- is what still decides whether the move happens.
///
/// **A rejection is the ordinary case, not a fault.** A source with no workflow
/// refuses by name (battery clause 8), which is what a Gitea pull request, a
/// build, a page and a monitor get; a source whose credential died answers
/// `unauthorized`. The shell draws no select for a kind that has no workflow
/// and falls back to the corpus-observed offer, marked *offer unverified*, for
/// a read that failed on a ticket that does have one. Neither is an empty list:
/// an empty answer is a real workflow with no moves left, and the two must not
/// collapse.
///
/// **There is no `source_id` argument**, for `submit_write`'s reason above: the
/// entity id already names the source.
///
/// # Errors
///
/// `invalid` if `entity_id` is not an entity id, `not_found` if its namespace
/// is not a configured source, `unauthorized` / `unreachable` / `internal` as
/// the adapter's own failure maps -- including the refusal of a source with no
/// workflow, which is `internal` because it is a caller asking the wrong
/// question rather than anything the user can act on;
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up.
#[tauri::command]
pub async fn reachable_transitions<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    entity_id: String,
) -> Result<Vec<String>, IpcError> {
    let state = crate::sources::state(&app)?;
    crate::sources::crud::reachable_transitions(
        &state.pool,
        &state.secrets,
        state.registry.as_ref(),
        &entity_id,
    )
    .await
    .map_err(|error| crate::sources::to_ipc(&error, None))
}

/// The **source view** of one entity (#661, ADR-0020): what its adapter draws
/// for its kind, read live when the detail opens and never stored.
///
/// `section: None` reads the whole view, deferred sections as stubs;
/// `Some(id)` reads that one deferred section when the reader opens its tab.
/// Routed like [`reachable_transitions`]: the entity id names the source, so
/// there is no `source_id` argument.
///
/// **Three answers, and the shell draws each differently** (rule 2): a view is
/// drawn as tabs with the generic projection behind *Raw*; `null` is an
/// adapter with nothing to draw for this kind, and the panel is what it was
/// before views existed; a rejection draws the same panel under one line,
/// *Showing the mirror: <message>*. An entity-list row keeps its address only
/// where the mirror holds that entity (rule 7, `sources::crud::view`).
///
/// # Errors
///
/// `invalid` if `entity_id` is not an entity id, `not_found` if its namespace
/// is not a configured source, `unauthorized` / `unreachable` / `internal` as
/// the adapter's own failure maps;
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up.
#[tauri::command]
pub async fn entity_view<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    entity_id: String,
    section: Option<String>,
) -> Result<Option<knobas_source::view::SourceView>, IpcError> {
    let state = crate::sources::state(&app)?;
    crate::sources::crud::view(
        &state.pool,
        &state.secrets,
        state.registry.as_ref(),
        &entity_id,
        section.as_deref(),
    )
    .await
    .map_err(|error| crate::sources::to_ipc(&error, None))
}

/// Everything the note view draws for one note (#46).
///
/// Deliberately **not** [`EntityDetail`], and the difference is not tidiness:
/// that DTO is shaped around a mirrored item -- a source, a `payload`, a
/// `web_url`, a `synced_at` -- and a note has none of those. Half of it would
/// be filled in with placeholders that a reader could not tell from real ones.
/// A note is an entity and shares the address space; it is not a mirror row.
///
/// `refs` and `links` overlap and both are here because they answer different
/// questions. `refs` is *the body's own list*, in body order, including the
/// ones that resolve to nothing -- which is what the editor draws chips from
/// and what makes an unresolved ref visible (story 10). `links` is the panel
/// #53 built: every link this note takes part in, in either direction, so the
/// note shows what points at it as well as what it points at.
#[derive(Debug, Clone, serde::Serialize)]
pub struct NoteDetail {
    pub note: knobas_core::note::NoteRow,
    /// The `[[refs]]` the body names, in body order, resolved where they
    /// resolve.
    pub refs: Vec<knobas_core::note::NoteRef>,
    /// Every link this note takes part in, each with its other end resolved --
    /// the ref links it derived, plus anything drawn by hand from either side.
    pub links: Vec<LinkEntry>,
}

/// The note at `note_id`, its refs and its links.
///
/// Three round trips, like [`get_entity_inner`]'s: the note, the refs, the
/// links.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) if `note_id` is not an entity id,
/// [`NotFound`](crate::IpcErrorCode::NotFound) if no note carries it,
/// [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
pub async fn get_note_inner(pool: &PgPool, note_id: &str) -> Result<NoteDetail, IpcError> {
    let id = EntityRef::parse(note_id).map_err(IpcError::invalid)?;
    let note = knobas_core::note::get(pool, &id)
        .await?
        .ok_or_else(|| IpcError::not_found(format!("there is no note at {id}")))?;
    Ok(NoteDetail {
        note,
        refs: knobas_core::note::refs_of(pool, &id).await?,
        links: knobas_core::link::entries_of(pool, &id).await?,
    })
}

/// One link a caller asks a new note to be born with (#502).
///
/// An **input** DTO: the frontend sends these and nothing sends them back. It
/// is a pair rather than two named fields (`capturedIn`, `capturedFrom`)
/// because the command's job is *draw these links with the note*, and the two
/// words are the caller's vocabulary, not the command's -- exactly as
/// [`create_link_inner`] takes a relation rather than knowing what `blocks`
/// means. The capture window (#503) and the in-app *New note* pass the same
/// two, and a third caller with a third relation needs no argument here.
///
/// The note is always the **from** end; see [`knobas_core::note::BornLink`].
///
/// `Serialize` behind `test-util` for the reason [`EntityFilter`] carries it:
/// it is what lets `tests/it/entity_mirror.rs` pin an input DTO through a round
/// trip, and so see a field the TypeScript mirror never declares.
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(feature = "test-util", derive(serde::Serialize))]
pub struct NoteLinkInput {
    /// The other end's entity id.
    pub target_id: String,
    /// The relation the link carries, folded to lower case like every other
    /// relation this module writes.
    ///
    /// Required, and blank is refused rather than defaulted. `create_link`'s
    /// relation is an `Option` and its absence means *nobody named one*, which
    /// [`DEFAULT_RELATION`] answers; here the caller is drawing a link the
    /// reader never saw a dialog for, so `""` is a caller bug and a link
    /// labelled `related` that nobody asked for would be the wrong sentence in
    /// the panel rather than an error anybody could find.
    pub relation: String,
}

/// Write a new note, and the links it is born with.
///
/// The first two arguments are optional because the affordance is *"start
/// writing"*: a note created from an empty editor has no title and no body yet,
/// and it still has to exist -- story 2 is that a thought is never lost to a
/// closed window, which needs the row to be there before the first keystroke
/// settles.
///
/// `links` is what a **capture** attaches (`CONTEXT.md`, **Capture**; spec #491
/// stories 40--43): `captured-in` to the context of the stored room the reader
/// stands in -- which makes the note a member of it by ADR-0008, since an
/// explicit add is an ordinary link touching the context's own `ctx:` entity --
/// and `captured-from` to the **foreground**, the word as `CONTEXT.md`'s
/// **Passive attribution** defines it and as the heartbeat computes it: the
/// open detail, else the room's anchor, else nothing (the deputy's ruling of
/// 2026-09-08 on #502). A derived room has no context (`CONTEXT.md`, **Room**),
/// so a note born in one is born with nothing, and that is a caller's decision
/// rather than a rule here: this command draws the links it is handed.
///
/// They are drawn in the note's own transaction, which is what the ticket's
/// title means -- see [`knobas_core::note::create`], which also states what
/// happens to a target that no longer exists (no link, and the note is still
/// written).
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) if a link's target is not an
/// entity id, or if its relation is blank -- both are caller bugs, and both are
/// worth refusing loudly, since every id this command is handed came from a row
/// the caller was already drawing and every relation from its own vocabulary;
/// [`Internal`](crate::IpcErrorCode::Internal) for a write failure.
pub async fn create_note_inner(
    pool: &PgPool,
    title: Option<&str>,
    body_md: Option<&str>,
    links: &[NoteLinkInput],
) -> Result<NoteDetail, IpcError> {
    let born_with = links
        .iter()
        .map(|link| {
            Ok(knobas_core::note::BornLink {
                target: EntityRef::parse(&link.target_id).map_err(IpcError::invalid)?,
                relation: present(Some(&link.relation))
                    .ok_or_else(|| IpcError::invalid("a link drawn with a note needs a relation"))?
                    .to_lowercase(),
            })
        })
        .collect::<Result<Vec<_>, IpcError>>()?;
    let note = knobas_core::note::create(
        pool,
        title.unwrap_or_default(),
        body_md.unwrap_or_default(),
        &born_with,
        ACTOR,
    )
    .await?;
    get_note_inner(pool, &note.id).await
}

/// Save a note's title and body, and bring its `[[refs]]` back into step.
///
/// This is what the editor's autosave calls, so it answers with the whole
/// [`NoteDetail`]: the refs it just reconciled are what the editor redraws its
/// chips from, and a second read to fetch them would race the next keystroke.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) if `note_id` is not an entity id,
/// [`NotFound`](crate::IpcErrorCode::NotFound) if no note carries it -- an
/// editor whose note was deleted elsewhere is told so rather than silently
/// resurrecting it, [`Internal`](crate::IpcErrorCode::Internal) for a write
/// failure.
pub async fn save_note_inner(
    pool: &PgPool,
    note_id: &str,
    title: &str,
    body_md: &str,
) -> Result<NoteDetail, IpcError> {
    let id = EntityRef::parse(note_id).map_err(IpcError::invalid)?;
    knobas_core::note::save(pool, &id, title, body_md, ACTOR)
        .await?
        .ok_or_else(|| IpcError::not_found(format!("there is no note at {id}")))?;
    get_note_inner(pool, note_id).await
}

/// Delete a note: the body goes, the address stays, tombstoned.
///
/// `Ok(false)` when there was nothing to delete. Idempotent, like
/// [`unlink_inner`]: a second *Delete* on a note already gone is not an error,
/// and the caller can tell the two apart -- and, like `unlink_inner`, the call
/// that mutated nothing writes no activity line.
///
/// The `deleted` line the first call *does* write is the store's
/// ([`knobas_core::note::delete`], #524), inside the transaction that took the
/// body; [`ACTOR`] is who it is attributed to, the same word every other
/// mutation in this module signs with.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) if `note_id` is not an entity id,
/// [`Internal`](crate::IpcErrorCode::Internal) for a write failure.
pub async fn delete_note_inner(pool: &PgPool, note_id: &str) -> Result<bool, IpcError> {
    let id = EntityRef::parse(note_id).map_err(IpcError::invalid)?;
    Ok(knobas_core::note::delete(pool, &id, ACTOR).await?)
}

/// One note, with its refs and its links.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the database
/// is still coming up, and whatever [`get_note_inner`] refuses with.
#[tauri::command]
pub async fn get_note(
    lifecycle: State<'_, Lifecycle>,
    note_id: String,
) -> Result<NoteDetail, IpcError> {
    let pool = lifecycle.pool()?;
    get_note_inner(&pool, &note_id).await
}

/// Write a new note, with the links it is born with.
///
/// `links` is absent for a caller that attaches none, which reads the same as
/// an empty list and is spelled that way so *New note* in a derived room sends
/// nothing rather than sending emptiness.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the database
/// is still coming up, and whatever [`create_note_inner`] refuses with.
#[tauri::command]
pub async fn create_note(
    lifecycle: State<'_, Lifecycle>,
    title: Option<String>,
    body_md: Option<String>,
    links: Option<Vec<NoteLinkInput>>,
) -> Result<NoteDetail, IpcError> {
    let pool = lifecycle.pool()?;
    create_note_inner(
        &pool,
        title.as_deref(),
        body_md.as_deref(),
        links.as_deref().unwrap_or_default(),
    )
    .await
}

/// Save a note, refs and all.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the database
/// is still coming up, and whatever [`save_note_inner`] refuses with.
#[tauri::command]
pub async fn save_note(
    lifecycle: State<'_, Lifecycle>,
    note_id: String,
    title: String,
    body_md: String,
) -> Result<NoteDetail, IpcError> {
    let pool = lifecycle.pool()?;
    save_note_inner(&pool, &note_id, &title, &body_md).await
}

/// Delete a note.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the database
/// is still coming up, and whatever [`delete_note_inner`] refuses with.
#[tauri::command]
pub async fn delete_note(
    lifecycle: State<'_, Lifecycle>,
    note_id: String,
) -> Result<bool, IpcError> {
    let pool = lifecycle.pool()?;
    delete_note_inner(&pool, &note_id).await
}

// -- suggestions and the room tray -----------------------------------------
//
// Four commands, and #836's evidence read, in this module and not a new one:
// a suggestion **is** a link row (#41), these are the writes that move it
// between the two states the link store already has, and the reads are the
// same graph the panel above reads from the other side. A
// `commands/suggest.rs` would be a second module over one table.

/// One page of a room's tray: the proposals, and how many there are in all.
///
/// `total` is the whole room, not the page -- #41's story 19 is "see how many
/// suggestions are waiting, so that I can choose when to spend attention on
/// them", and a number capped by `limit` answers a different question. Same
/// shape and same reason as [`EntityPage`].
#[derive(Debug, Clone, serde::Serialize)]
pub struct SuggestionPage {
    pub rows: Vec<SuggestionEntry>,
    /// Every proposal in the room, before `limit`.
    pub total: i64,
    /// Items holding more proposals than the tray draws for one item (#774),
    /// most hidden first. The bound is applied by `suggest::proposals`.
    pub more: Vec<suggest::SuggestionMore>,
}

/// Run a detection pass, returning how many proposals it wrote.
///
/// Idempotent, and cheap to repeat since #745: a pass over an unchanged mirror
/// writes nothing and compares nothing, and a pass after a sync writes only
/// what the new items justify. It never creates a confirmed link and never
/// writes to a source.
///
/// Through the process's one [`SuggestionPass`]: a call while a pass runs is
/// answered by the next pass rather than starting a third, and the pass runs
/// on a connection of its own, never on the UI pool (#745).
///
/// [`SuggestionPass`]: crate::suggestion_pass::SuggestionPass
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) if a statement fails or runs
/// past the pass's statement timeout.
pub async fn detect_suggestions_inner(
    pass: &crate::suggestion_pass::SuggestionPass,
) -> Result<u32, IpcError> {
    let written = pass.run().await?;
    // A pass that proposed more than four billion links is not a number the
    // frontend needs to be exact about.
    Ok(u32::try_from(written).unwrap_or(u32::MAX))
}

/// The proposals a room holds, newest first, with both ends resolved.
///
/// `sources` is a derived room's membership and follows [`EntityFilter`]'s
/// convention: **empty means every source**. A proposal belongs to a room when
/// either of its ends does.
///
/// `ctx` is a stored context's room (#47): its membership -- the fixed
/// one-hop rule, resolved by the store that owns it -- plus the context's own
/// entity, so a proposed *add to this context* surfaces in the room it would
/// add to. Membership is computed from confirmed links only (ADR-0008), which
/// is what keeps "the tray scopes by membership" from becoming circular with
/// "membership is built from links".
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) if a query fails.
pub async fn room_suggestions_inner(
    pool: &PgPool,
    sources: &[String],
    ctx: Option<&str>,
    limit: u32,
) -> Result<SuggestionPage, IpcError> {
    let members = match ctx {
        Some(ctx) => {
            let mut ids = knobas_core::context::member_ids(pool, ctx).await?;
            ids.push(ctx.to_owned());
            Some(ids)
        }
        None => None,
    };
    let members = members.as_deref();
    // The count reads the whole room and the rows read its newest end: two
    // connections at once, so the tray waits for the slower and not the sum.
    let drawn = async {
        let rows = suggest::proposals(pool, sources, members, i64::from(limit)).await?;
        let more = suggest::proposal_overflow(pool, sources, members, &rows).await?;
        Ok::<_, knobas_core::CoreError>((rows, more))
    };
    let ((rows, more), total) =
        tokio::try_join!(drawn, suggest::proposal_count(pool, sources, members))?;
    Ok(SuggestionPage { rows, total, more })
}

/// Accept a proposal, making it an ordinary link.
///
/// `Ok(None)` when there was nothing to accept -- it was accepted or dismissed
/// already. Nothing was mutated, so nothing is written to the log and nothing
/// is announced, exactly as [`unlink_inner`] does.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) if `link_id` is not a UUID;
/// [`NotFound`](crate::IpcErrorCode::NotFound) if no link carries it;
/// [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
pub async fn accept_suggestion_inner(
    pool: &PgPool,
    link_id: &str,
) -> Result<Option<LinkMutation>, IpcError> {
    let id = suggestion_id(link_id)?;
    let Some(link) = suggest::accept(pool, id).await? else {
        return Ok(None);
    };
    let activity = record_link_activity(pool, "accepted", &link).await?;
    Ok(Some(LinkMutation {
        link,
        activity,
        superseded: None,
    }))
}

/// Dismiss a proposal, and remember it.
///
/// The tombstone is the withdrawal memory Links v1 already had, so a dismissal
/// suppresses re-proposal for exactly the same reason an unlink does -- there
/// is one mechanism, not two.
///
/// `Ok(None)` when there was nothing to dismiss, which includes a link that has
/// already been accepted: that is a link, and the panel's *Unlink* is what
/// withdraws it.
///
/// # Errors
///
/// As [`accept_suggestion_inner`].
pub async fn dismiss_suggestion_inner(
    pool: &PgPool,
    link_id: &str,
) -> Result<Option<LinkMutation>, IpcError> {
    let id = suggestion_id(link_id)?;
    let Some(link) = suggest::dismiss(pool, id).await? else {
        return Ok(None);
    };
    let activity = record_link_activity(pool, "dismissed", &link).await?;
    Ok(Some(LinkMutation {
        link,
        activity,
        superseded: None,
    }))
}

/// What was said for and against one proposal (#836): the stored reason, the
/// text it was found in with the match marked, what supports it and what
/// speaks against it. The Proposed links view reads it for the proposal open.
///
/// A read beside `room_suggestions`, not a field on its rows: the tray's
/// ranking and read are unchanged, and only the open proposal pays for the
/// text scan (`knobas_core::evidence`).
///
/// # Errors
///
/// As [`accept_suggestion_inner`].
pub async fn proposal_evidence_inner(
    pool: &PgPool,
    link_id: &str,
) -> Result<knobas_core::evidence::ProposalEvidence, IpcError> {
    let id = suggestion_id(link_id)?;
    Ok(knobas_core::evidence::proposal_evidence(pool, id).await?)
}

/// A suggestion is addressed by its link id, because it is a link row.
fn suggestion_id(link_id: &str) -> Result<Uuid, IpcError> {
    link_id
        .parse()
        .map_err(|_| IpcError::invalid(format!("{link_id} is not a link id")))
}

/// Run a detection pass over the mirror.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the database
/// is still coming up, and whatever [`detect_suggestions_inner`] refuses with.
#[tauri::command]
pub async fn detect_suggestions(lifecycle: State<'_, Lifecycle>) -> Result<u32, IpcError> {
    detect_suggestions_inner(&lifecycle.suggestions()?).await
}

/// One page of a room's suggestion tray.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the database
/// is still coming up, and whatever [`room_suggestions_inner`] refuses with.
#[tauri::command]
pub async fn room_suggestions(
    lifecycle: State<'_, Lifecycle>,
    sources: Vec<String>,
    ctx: Option<String>,
    limit: u32,
) -> Result<SuggestionPage, IpcError> {
    let pool = lifecycle.pool()?;
    room_suggestions_inner(&pool, &sources, ctx.as_deref(), limit).await
}

/// Accept a proposal, and announce it.
///
/// Idempotent: accepting an already-accepted suggestion resolves, writes no
/// second line and announces nothing.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the database
/// is still coming up, and whatever [`accept_suggestion_inner`] refuses with.
#[tauri::command]
pub async fn accept_suggestion<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    link_id: String,
) -> Result<(), IpcError> {
    let pool = lifecycle.pool()?;
    if let Some(written) = accept_suggestion_inner(&pool, &link_id).await? {
        announce(&app, written.activity);
    }
    Ok(())
}

/// Dismiss a proposal, and announce it.
///
/// Idempotent, for the same reason [`accept_suggestion`] is.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the database
/// is still coming up, and whatever [`dismiss_suggestion_inner`] refuses with.
#[tauri::command]
pub async fn dismiss_suggestion<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    link_id: String,
) -> Result<(), IpcError> {
    let pool = lifecycle.pool()?;
    if let Some(written) = dismiss_suggestion_inner(&pool, &link_id).await? {
        announce(&app, written.activity);
    }
    Ok(())
}

/// What was said for and against one proposal (#836).
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the database
/// is still coming up, and whatever [`proposal_evidence_inner`] refuses with.
#[tauri::command]
pub async fn proposal_evidence(
    lifecycle: State<'_, Lifecycle>,
    link_id: String,
) -> Result<knobas_core::evidence::ProposalEvidence, IpcError> {
    let pool = lifecycle.pool()?;
    proposal_evidence_inner(&pool, &link_id).await
}

// -- the start-work flow ----------------------------------------------------
//
// Six commands, in this module and not a new one -- the `commands/` + `ipc/`
// layout is frozen, and the flow starts from a ticket *entity*, which is what
// this module is about. They are thin: everything they decide lives in
// `crate::start_work`, where a test can reach it without a Tauri app.
//
// **None of them is a write path of its own.** Every side effect the flow has
// is an existing `WriteOp` going through `submit_write`'s queue, or a link
// going through `create_link_inner` -- the same two doors the *Comment* button
// and the *Link to...* dialog use.

/// The flow for a ticket, proposing one if there is none and `repo_id` says
/// where it would go.
///
/// One command rather than a read and a create, because the address
/// `#/start-work/<key>` has to answer both questions at once: *is there a flow,
/// and if not, what would one look like?* An empty answer means there is no
/// flow and no repository was named -- which is the state where the view asks
/// the user to pick one.
///
/// Proposing does **not** dispatch anything. The whole sequence is composed and
/// stored so it can be shown before anything happens; `start_work_run` is what
/// performs it.
///
/// # Errors
///
/// `invalid` for an id that is not an entity id; `not_found` if the ticket is
/// not in the mirror; `not_ready` before bring-up.
#[tauri::command]
pub async fn start_work_flow(
    lifecycle: State<'_, Lifecycle>,
    entity_id: String,
    repo_id: Option<String>,
) -> Result<Vec<knobas_core::start_work::FlowStep>, IpcError> {
    let pool = lifecycle.pool()?;
    let ticket = EntityRef::parse(&entity_id).map_err(IpcError::invalid)?;
    let existing = knobas_core::start_work::flow(&pool, &ticket).await?;
    if !existing.is_empty() {
        return Ok(existing);
    }
    let Some(repo_id) = repo_id else {
        return Ok(Vec::new());
    };
    let repo = EntityRef::parse(&repo_id).map_err(IpcError::invalid)?;
    crate::start_work::begin(&pool, &ticket, &repo).await
}

/// Run the flow as far as it will go, **behind the answer** (#779): answers
/// the flow as it stands, and the steps run on in the background.
///
/// # Errors
///
/// `invalid` for an id that is not an entity id; `not_ready` before bring-up.
#[tauri::command]
pub async fn start_work_run<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    entity_id: String,
) -> Result<Vec<knobas_core::start_work::FlowStep>, IpcError> {
    let state = crate::sources::state(&app)?;
    let ticket = EntityRef::parse(&entity_id).map_err(IpcError::invalid)?;
    run_behind(&state, &ticket).await
}

/// The flow run on behind the answer (#779): the stepper draws each step as
/// it lands, by re-reading the flow while it moves.
async fn run_behind(
    state: &crate::sources::SourcesState,
    ticket: &EntityRef,
) -> Result<Vec<knobas_core::start_work::FlowStep>, IpcError> {
    crate::start_work::run_in_background(
        &state.pool,
        std::sync::Arc::new(crate::start_work::queue::Queue {
            state: state.clone(),
        }),
        ticket,
    )
    .await
}

/// Retry one step, without redoing the ones that succeeded. The flow runs on
/// from it behind the answer (#779).
///
/// # Errors
///
/// `not_found` if no step carries the id; `conflict` if an earlier step is
/// where the flow stopped; `not_ready` before bring-up.
#[tauri::command]
pub async fn start_work_retry<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    step_id: i64,
) -> Result<Vec<knobas_core::start_work::FlowStep>, IpcError> {
    let state = crate::sources::state(&app)?;
    let ticket = crate::start_work::ready_retry(
        &state.pool,
        &crate::start_work::queue::Queue { state: &*state },
        step_id,
    )
    .await?;
    run_behind(&state, &ticket).await
}

/// Skip one step, so a ticket that needs no branch still gets its status moved.
/// The flow runs on past it behind the answer (#779).
///
/// # Errors
///
/// `not_found` if no step carries the id; `not_ready` before bring-up.
#[tauri::command]
pub async fn start_work_skip<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    step_id: i64,
) -> Result<Vec<knobas_core::start_work::FlowStep>, IpcError> {
    let state = crate::sources::state(&app)?;
    let ticket = crate::start_work::ready_skip(&state.pool, step_id).await?;
    run_behind(&state, &ticket).await
}

/// Replace a step's proposal with the one the user edited.
///
/// `payload` is the step's own stored value, edited -- the same shape
/// `amend_write` takes, and untyped on the wire for the same reason: `WriteOp`
/// grows per milestone (ADR-0006), so typing the argument would drag the SPI's
/// enum onto the IPC surface.
///
/// # Errors
///
/// `not_found` if no step carries the id; `conflict` if the step has already
/// happened; `not_ready` before bring-up.
#[tauri::command]
pub async fn start_work_amend(
    lifecycle: State<'_, Lifecycle>,
    step_id: i64,
    payload: serde_json::Value,
) -> Result<Vec<knobas_core::start_work::FlowStep>, IpcError> {
    let pool = lifecycle.pool()?;
    crate::start_work::repropose(&pool, step_id, payload).await
}

/// The reverse direction: move every ticket whose linked pull request has been
/// merged, and answer how many moved.
///
/// A pass over the mirror, invoked the way `detect_suggestions` is -- there is
/// no per-item hook in the sync engine, and adding one to serve this would be a
/// second mechanism. The count is what the shell announces, so an automatic
/// change is visible rather than mysterious.
///
/// # Errors
///
/// `not_ready` before bring-up; `internal` if the pass's own read fails.
#[tauri::command]
pub async fn follow_merges<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> Result<u32, IpcError> {
    let state = crate::sources::state(&app)?;
    let declarations =
        crate::sources::paths::declared_paths(&state.pool, state.registry.as_ref()).await?;
    crate::start_work::merge::follow_merges(
        &state.pool,
        &crate::start_work::queue::Queue { state: &*state },
        crate::start_work::plan::IN_REVIEW,
        &declarations,
    )
    .await
}

// -- the inbox (issue #45) --------------------------------------------------
//
// Four commands, all here in the existing `entity` module: inbox items are
// derived from entities and its two write commands act on them, and the
// `commands/` + `ipc/` module layout is frozen -- no new module on either
// side. There is deliberately **no fifth command that acts on a source**: an
// inbox action that changes something at a source is a `WriteOp` through
// `submit_write` above, which is #43's command over #42's queue, and the inbox
// introduces no write path of its own.

/// The identity behind `@me`, loaded the way search loads it.
///
/// `knobas_search::Vocabulary` is where "who am I" lives -- the union of every
/// enabled source's configured username -- and the inbox is an identity
/// feature, so it reads that and not a second mechanism. The kind catalog is
/// the default one because the identity is the only field wanted here; loading
/// it costs the same single query every search already makes.
async fn identity_of(pool: &PgPool) -> Result<Vec<String>, IpcError> {
    Ok(
        knobas_search::Vocabulary::load(pool, knobas_search::KindCatalog::default())
            .await
            .map_err(IpcError::internal)?
            .identity,
    )
}

/// [`inbox_items`], against a pool and a clock.
///
/// # Errors
///
/// `internal` if the derivation, the identity read or the source listing
/// fails.
pub async fn inbox_items_inner(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    now: DateTime<Utc>,
    shelf: knobas_core::inbox::Shelf,
) -> Result<Vec<crate::inbox::InboxEntry>, IpcError> {
    let identity = identity_of(pool).await?;
    crate::inbox::stream(pool, registry, &identity, now, shelf).await
}

/// The inbox: one actionable stream, newest first.
///
/// `shelf` is `stream` -- what needs you now -- or `snoozed`, what you
/// deferred and when it comes back. Both are the same derivation with the same
/// predicate, asked for one shelf or the other.
///
/// Every entry carries the actions its source can really perform: an op the
/// adapter does not declare is absent rather than offered and failing.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the
/// database or the sync engine is still coming up, `internal` for a read
/// failure.
#[tauri::command]
pub async fn inbox_items<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    shelf: knobas_core::inbox::Shelf,
) -> Result<Vec<crate::inbox::InboxEntry>, IpcError> {
    let state = crate::sources::state(&app)?;
    inbox_items_inner(&state.pool, state.registry.as_ref(), Utc::now(), shelf).await
}

/// How many items need you now -- the number the top strip shows.
///
/// **Snoozed items are not in it**, because the number means "needs me now";
/// neither are items marked done. It is the stream's own statement, counted,
/// so the badge cannot disagree with the view it opens.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the
/// database is still coming up, `internal` for a read failure.
#[tauri::command]
pub async fn inbox_count(lifecycle: State<'_, Lifecycle>) -> Result<i64, IpcError> {
    let pool = lifecycle.pool()?;
    inbox_count_inner(&pool, &crate::sources::Registry::builtin(), Utc::now()).await
}

/// [`inbox_count`], against a pool and a clock.
///
/// # Errors
///
/// `internal` if the count or the identity read fails.
pub async fn inbox_count_inner(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    now: DateTime<Utc>,
) -> Result<i64, IpcError> {
    let identity = identity_of(pool).await?;
    let declarations = crate::sources::paths::declared_paths(pool, registry).await?;
    knobas_core::inbox::count(pool, &identity, now, &declarations)
        .await
        .map_err(IpcError::internal)
}

/// [`snooze_inbox_item`], against a pool and a clock.
///
/// # Errors
///
/// As [`snooze_inbox_item`].
pub async fn snooze_inbox_item_inner(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    now: DateTime<Utc>,
    item_key: &str,
    until: DateTime<Utc>,
) -> Result<ActivityRow, IpcError> {
    let identity = identity_of(pool).await?;
    let declarations = crate::sources::paths::declared_paths(pool, registry).await?;
    crate::inbox::answer(
        pool,
        &identity,
        now,
        item_key,
        crate::inbox::Answer::Snooze(until),
        &declarations,
    )
    .await
}

/// Not now -- come back on this date (#45, stories 13-15).
///
/// `until` is an absolute moment, because the presets (*tomorrow*, *next
/// Monday*, *after the credential expires*) are the caller's arithmetic and a
/// date picker is the general case of the same argument. Snoozing something
/// already snoozed moves its date.
///
/// Refuses an item that is on neither shelf: the key comes from a list the
/// webview has been holding, and an answer to something since resolved at the
/// source would write a durable row about work that no longer exists.
///
/// Writes one activity line and emits `activity:new`, because *every* inbox
/// action is recorded.
///
/// # Errors
///
/// `not_found` for an item on neither shelf,
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// `internal` for a write failure.
#[tauri::command]
pub async fn snooze_inbox_item<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    item_key: String,
    until: DateTime<Utc>,
) -> Result<(), IpcError> {
    let pool = lifecycle.pool()?;
    let written = snooze_inbox_item_inner(
        &pool,
        &crate::sources::Registry::builtin(),
        Utc::now(),
        &item_key,
        until,
    )
    .await?;
    announce(&app, written);
    Ok(())
}

/// [`complete_inbox_item`], against a pool and a clock.
///
/// # Errors
///
/// As [`complete_inbox_item`].
pub async fn complete_inbox_item_inner(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    now: DateTime<Utc>,
    item_key: &str,
) -> Result<ActivityRow, IpcError> {
    let identity = identity_of(pool).await?;
    let declarations = crate::sources::paths::declared_paths(pool, registry).await?;
    crate::inbox::answer(
        pool,
        &identity,
        now,
        item_key,
        crate::inbox::Answer::Done,
        &declarations,
    )
    .await
}

/// I handled this (#45, story 16).
///
/// The item leaves the stream, and **comes back if its subject moves again** --
/// *done* is stored as the moment it was answered and compared against the
/// item's own, so marking a mention done hides it until somebody says
/// something new rather than muting the ticket for ever.
///
/// Writes one activity line and emits `activity:new`.
///
/// # Errors
///
/// As [`snooze_inbox_item`].
#[tauri::command]
pub async fn complete_inbox_item<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    item_key: String,
) -> Result<(), IpcError> {
    let pool = lifecycle.pool()?;
    let written = complete_inbox_item_inner(
        &pool,
        &crate::sources::Registry::builtin(),
        Utc::now(),
        &item_key,
    )
    .await?;
    announce(&app, written);
    Ok(())
}

/// Which inbox categories may raise a desktop notification (#290).
///
/// Empty for a profile nobody has switched one on in, which is what makes the
/// feature opt-in per kind (spec #272, story 71).
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) if the read
/// fails.
#[tauri::command]
pub async fn notification_kinds(
    lifecycle: State<'_, Lifecycle>,
) -> Result<Vec<knobas_core::inbox::Category>, IpcError> {
    let pool = lifecycle.pool()?;
    crate::inbox::notification_kinds(&pool).await
}

/// Set which inbox categories may notify, and answer with what is now stored.
///
/// Takes the words rather than the enum, the shape [`unlink`] takes its id in
/// and for the same reason: a value Tauri itself cannot deserialize is
/// rejected with a bare string and no [`IpcErrorCode`](crate::IpcErrorCode),
/// so a category this build does not know reads to the settings section as a
/// window that broke rather than as `invalid` with a sentence in it.
///
/// **Nothing here asks about the OS permission.** Requesting it is the
/// settings surface's, on the click that switches the first kind on (story
/// 72); this command records a *preference*, and a preference that could only
/// be stored while some other system said yes would be a setting that
/// silently forgot itself.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`IpcErrorCode::Invalid`](crate::IpcErrorCode::Invalid) for a word that is
/// not an inbox category,
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) if the write
/// fails.
#[tauri::command]
pub async fn set_notification_kinds(
    lifecycle: State<'_, Lifecycle>,
    kinds: Vec<String>,
) -> Result<Vec<knobas_core::inbox::Category>, IpcError> {
    let pool = lifecycle.pool()?;
    crate::inbox::set_notification_kinds(&pool, &kinds).await
}

/// Show a desktop notification whose click comes back as
/// `notification:clicked` (#339).
///
/// A shim over [`crate::notify::Notifier`], where the rules live and are
/// tested: one wait per address, sixteen at once, a click emitted with the
/// address and a clear emitted as nothing. Off the async runtime with
/// `spawn_blocking` because the show blocks until the OS has the desktop
/// notification -- milliseconds, but the wait behind it is a thread of its
/// own and the runtime is not where either belongs.
///
/// Here rather than in a module pair of its own for the reason #290's
/// setting is: the inbox lives in `entity`, and §10.8's layout is frozen.
///
/// # Errors
///
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) with the
/// platform's refusal. On macOS the one to expect is `UNUserNotificationCenter`'s
/// *no bundle identifier* from a bare `tauri dev` binary, logged at `warn` so
/// a dev terminal says why nothing appeared.
#[tauri::command]
pub async fn notify(
    notifier: State<'_, std::sync::Arc<crate::notify::Notifier>>,
    draft: crate::notify::NotificationDraft,
) -> Result<(), IpcError> {
    let notifier = std::sync::Arc::clone(&notifier);
    let address = draft.address.clone();
    let shown = tauri::async_runtime::spawn_blocking(move || notifier.notify(draft))
        .await
        .map_err(|error| {
            IpcError::internal(format!("the desktop notification task ended: {error}"))
        })?;
    match shown {
        Ok(_) => Ok(()),
        Err(error) => {
            tracing::warn!(address, %error, "notification not shown");
            Err(IpcError::internal(error))
        }
    }
}

// -- checkouts (#499) -------------------------------------------------------
//
// Here rather than in a module pair of their own for the reason `notify` and
// #290's setting are: a repo and a branch are entities, `get_entity` is in this
// module, and §10.8 freezes the `commands/` + `ipc/` layout. The behaviour is
// `crate::checkout`; these four are the thin shims the layout asks for.

/// Where knobas looks for clones, or nothing.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) if the read fails.
#[tauri::command]
pub async fn clones_root(lifecycle: State<'_, Lifecycle>) -> Result<Option<String>, IpcError> {
    let pool = lifecycle.pool()?;
    crate::checkout::clones_root(&pool).await
}

/// Set the clones root, or clear it with a blank one.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) if the write
/// fails.
#[tauri::command]
pub async fn set_clones_root(
    lifecycle: State<'_, Lifecycle>,
    path: Option<String>,
) -> Result<(), IpcError> {
    let pool = lifecycle.pool()?;
    crate::checkout::set_clones_root(&pool, path.as_deref()).await
}

/// The checkout a repo or branch detail draws -- the override, else the scan,
/// else *no checkout* and the clone command to copy.
///
/// Read on every open rather than stored: a clone that moved between two opens
/// would leave a cached path pointing at nothing.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`IpcErrorCode::Invalid`](crate::IpcErrorCode::Invalid) for an address that
/// is not an entity id or not a repo or branch,
/// [`IpcErrorCode::NotFound`](crate::IpcErrorCode::NotFound) if the mirror
/// does not hold it, [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal)
/// for a query failure.
#[tauri::command]
pub async fn entity_checkout(
    lifecycle: State<'_, Lifecycle>,
    entity_id: String,
) -> Result<crate::checkout::CheckoutView, IpcError> {
    let pool = lifecycle.pool()?;
    crate::checkout::view(&pool, &entity_id).await
}

/// Set this repository's checkout path by hand, or clear it back to the scan.
///
/// Answers the fresh view rather than nothing, so the panel draws what is now
/// true without a second round trip -- and so clearing an override *shows* the
/// scan taking over, which is the whole of what clearing means.
///
/// Keyed on the repository even when a branch was the address: a per-branch
/// path would be a second answer to one question, stale the moment somebody
/// switched branches in that working tree.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`IpcErrorCode::Invalid`](crate::IpcErrorCode::Invalid) for an address that
/// is not an entity id or not a repo or branch,
/// [`IpcErrorCode::NotFound`](crate::IpcErrorCode::NotFound) if the mirror
/// holds neither it nor its repository,
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) if the write
/// fails.
#[tauri::command]
pub async fn set_checkout_override(
    lifecycle: State<'_, Lifecycle>,
    entity_id: String,
    path: Option<String>,
) -> Result<crate::checkout::CheckoutView, IpcError> {
    let pool = lifecycle.pool()?;
    crate::checkout::set_override(&pool, &entity_id, path.as_deref()).await?;
    crate::checkout::view(&pool, &entity_id).await
}

/// The uncommitted changes at this entity's checkout (#824, ADR-0026): the
/// unstaged and staged diffs and the untracked paths, read by read-only git
/// when the *Local changes* tab opens.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`IpcErrorCode::Invalid`](crate::IpcErrorCode::Invalid) for an address that
/// is not an entity id or not a repo or branch,
/// [`IpcErrorCode::NotFound`](crate::IpcErrorCode::NotFound) if the mirror
/// does not hold it or it has no checkout on this machine,
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) for a query
/// failure. What git answered is the read's own arm, never an error.
#[tauri::command]
pub async fn entity_checkout_diff(
    lifecycle: State<'_, Lifecycle>,
    entity_id: String,
) -> Result<crate::checkout::uncommitted::DiffRead, IpcError> {
    let pool = lifecycle.pool()?;
    crate::checkout::local_changes(&pool, &entity_id).await
}

// -- the job's conversation (#819) ------------------------------------------
//
// Two commands, beside the entity reads they belong with (§10.8 freezes the
// `commands/` + `ipc/` layout). The behaviour is `crate::conversation`.

/// Record that the reader opened this ticket's job, and answer when they last
/// did before now, or `null` for a first visit (#819, R-5.5's *last visit*).
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`IpcErrorCode::Invalid`](crate::IpcErrorCode::Invalid) for something that
/// is not an entity id, [`IpcErrorCode::NotFound`](crate::IpcErrorCode::NotFound)
/// for one the database does not know,
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) for a query
/// failure.
#[tauri::command]
pub async fn visit_entity(
    lifecycle: State<'_, Lifecycle>,
    entity_id: String,
) -> Result<Option<DateTime<Utc>>, IpcError> {
    let pool = lifecycle.pool()?;
    crate::conversation::visit(&pool, &entity_id).await
}

/// A ticket's thread (#819, R-5.2): its comments in the source's order, each
/// marked when it is the reader's own or mentions them, and the status moves
/// knobas delivered on it.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`IpcErrorCode::Invalid`](crate::IpcErrorCode::Invalid) for something that
/// is not an entity id, [`IpcErrorCode::NotFound`](crate::IpcErrorCode::NotFound)
/// for one the mirror does not hold,
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) for a query
/// failure.
#[tauri::command]
pub async fn entity_thread(
    lifecycle: State<'_, Lifecycle>,
    entity_id: String,
) -> Result<crate::conversation::Thread, IpcError> {
    let pool = lifecycle.pool()?;
    crate::conversation::thread(&pool, &entity_id).await
}

// -- opening a checkout (#501) ----------------------------------------------
//
// Beside #499's four and for its reason: a repo and a branch are entities, and
// §10.8 freezes the `commands/` + `ipc/` layout. The behaviour is
// `crate::checkout`; these are the shims. #844 adds `checkout_settings` and
// `set_checkout_applications` here, and `open_checkout` takes an application.

/// Every application as #501's buttons draw it, with the command it would run.
///
/// Read by the rooms UI's settings section and checkout panel. Since #844 a
/// view over the *Open in…* list: one row per application, its id as the
/// action.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) if the read fails.
#[tauri::command]
pub async fn checkout_commands(
    lifecycle: State<'_, Lifecycle>,
) -> Result<Vec<crate::checkout::OpenCommandView>, IpcError> {
    let pool = lifecycle.pool()?;
    crate::checkout::commands(&pool).await
}

/// Set one application's command, or put back the one this platform ships.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`IpcErrorCode::Invalid`](crate::IpcErrorCode::Invalid) for an application
/// that does not exist or a command that cannot be run,
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) if the write
/// fails.
#[tauri::command]
pub async fn set_checkout_command(
    lifecycle: State<'_, Lifecycle>,
    action: String,
    template: Option<String>,
) -> Result<Vec<crate::checkout::OpenCommandView>, IpcError> {
    let pool = lifecycle.pool()?;
    crate::checkout::set_command(&pool, &action, template.as_deref()).await?;
    crate::checkout::commands(&pool).await
}

/// Run an application at this entity's checkout (#501, #844), at a file in it
/// when `file` names one. Answers the argument vector it started.
///
/// `application` is an id from `checkout_settings`, the default when it is
/// `None`. `command` instead runs a command typed into *Add an application*
/// that is not saved yet (*Try it*). `file` is a path relative to the
/// checkout and must be a file inside it on disk; `line` fills `{line}`.
///
/// The one place knobas starts a program a person chose. Every value filled
/// into it is from this disk: the checkout path and a file inside it, never a
/// field of the mirrored repo (ADR-0016). It returns as soon as the program
/// has started.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`IpcErrorCode::Invalid`](crate::IpcErrorCode::Invalid) for an unknown
/// application, an address that is not a repo or a branch, a file that is not
/// inside the checkout, a command that cannot be run, or a program that would
/// not start; [`IpcErrorCode::NotFound`](crate::IpcErrorCode::NotFound) if the
/// mirror does not hold the entity or it has no checkout on this machine;
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) for a query
/// failure.
#[tauri::command]
pub async fn open_checkout(
    lifecycle: State<'_, Lifecycle>,
    entity_id: String,
    application: Option<String>,
    file: Option<String>,
    line: Option<u32>,
    command: Option<String>,
) -> Result<Vec<String>, IpcError> {
    let pool = lifecycle.pool()?;
    let run = match command.as_deref() {
        Some(command) => crate::checkout::Run::Command(command),
        None => crate::checkout::Run::Application(application.as_deref()),
    };
    crate::checkout::open::open(&pool, &entity_id, run, file.as_deref(), line).await
}

/// Settings › Checkouts (#844, R-13a.3): the clones root, the git found, the
/// *Open in…* applications in their order and the default's id. What every
/// *Open in…* menu reads.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) if a read fails.
#[tauri::command]
pub async fn checkout_settings(
    lifecycle: State<'_, Lifecycle>,
) -> Result<crate::checkout::CheckoutSettings, IpcError> {
    let pool = lifecycle.pool()?;
    crate::checkout::settings(&pool).await
}

/// Save the *Open in…* applications, in order, and which is the default
/// (#844, R-8.21, R-8.23). Each entry is `{ id?, name, command,
/// file_command? }`; a new one has no id and gets one from its name. Answers
/// the fresh settings.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`IpcErrorCode::Invalid`](crate::IpcErrorCode::Invalid) for an entry with
/// no name or a command that cannot be run,
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) if the write
/// fails.
#[tauri::command]
pub async fn set_checkout_applications(
    lifecycle: State<'_, Lifecycle>,
    applications: Vec<knobas_core::checkout::open::Draft>,
    default_application: Option<String>,
) -> Result<crate::checkout::CheckoutSettings, IpcError> {
    let pool = lifecycle.pool()?;
    crate::checkout::set_applications(&pool, &applications, default_application.as_deref()).await?;
    crate::checkout::settings(&pool).await
}

/// Clone a repository into the clones root, or stop the clone that is running
/// (#823, ADR-0023).
///
/// One command for both, because ADR-0023 names one: `stop` absent or false
/// starts (answering at once, with progress on `clone:progress`), `stop` true
/// stops and answers once the half-made folder is removed. A repo or a
/// branch; a branch clones its repository.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// and what [`crate::checkout::clone::start`] and
/// [`crate::checkout::clone::stop`] refuse with.
#[tauri::command]
pub async fn clone_repo<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    entity_id: String,
    stop: Option<bool>,
) -> Result<crate::checkout::clone::CloneAnswer, IpcError> {
    let pool = lifecycle.pool()?;
    let clones = crate::checkout::clone::Clones::global();
    if stop.unwrap_or(false) {
        return crate::checkout::clone::stop(&pool, &clones, &entity_id).await;
    }
    let registry = std::sync::Arc::clone(&crate::sources::state(&app)?.registry);
    let events = std::sync::Arc::new(crate::sources::events::TauriEvents::new(app));
    crate::checkout::clone::start(&pool, registry.as_ref(), &clones, events, &entity_id).await
}

// -- the capture window and its shortcut (#503) -----------------------------
//
// Here rather than in a module pair of their own, for the reason #499's four
// and #501's three are: a capture is a note, `create_note` is in this module,
// and §10.8 freezes the `commands/` + `ipc/` layout. The behaviour is
// `crate::capture`; these five are the shims.

/// The capture shortcut as the settings pane draws it: what is stored, and why
/// it is not holding.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) if the read fails.
#[tauri::command]
pub async fn capture_shortcut(
    lifecycle: State<'_, Lifecycle>,
    capture: State<'_, crate::capture::CaptureState>,
) -> Result<crate::capture::ShortcutView, IpcError> {
    let pool = lifecycle.pool()?;
    crate::capture::shortcut(&pool, &capture).await
}

/// Set the capture shortcut, or clear it with a blank one.
///
/// A combination the plugin cannot parse, or one the operating system will not
/// hand over, is **not** a refusal of this call: it is stored and the answer
/// says why it is not registered (`crate::capture`, point 2).
///
/// Generic over the runtime for the reason `app_status` is: a bare
/// `tauri::AppHandle` means `AppHandle<Wry>`, and a command taking one cannot
/// be registered on the `tauri::test` mock app at all.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) if the write
/// fails.
#[tauri::command]
pub async fn set_capture_shortcut<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    capture: State<'_, crate::capture::CaptureState>,
    accelerator: Option<String>,
) -> Result<crate::capture::ShortcutView, IpcError> {
    let pool = lifecycle.pool()?;
    crate::capture::set_shortcut(
        &pool,
        &capture,
        &crate::capture::Plugin::new(app),
        accelerator.as_deref(),
    )
    .await
}

/// Record where the reader is standing, so a capture can attach it.
///
/// Called by the **main** window whenever its room or its foreground changes.
/// Both arguments are the shell's own two answers, unchanged: `context` is the
/// stored room's `ctx:` entity (`null` for a derived room, which has none) and
/// `foreground` is `shell/timer.ts`'s `roomForeground`. Nothing here recomputes
/// either, which is the whole of why this command exists rather than the
/// capture window asking the database what the reader was looking at.
///
/// Needs no database, so it answers during bring-up like `ping` does: a window
/// that has drawn a room has something true to say about it whether or not
/// PostgreSQL is up yet.
#[tauri::command]
pub fn record_capture_context(
    capture: State<'_, crate::capture::CaptureState>,
    context: Option<String>,
    foreground: Option<String>,
) {
    crate::capture::record(
        &capture,
        crate::capture::Recorded {
            context,
            foreground,
        },
    );
}

/// What the main window last recorded -- the capture window's half of the pair
/// above.
#[tauri::command]
pub fn capture_context(
    capture: State<'_, crate::capture::CaptureState>,
) -> crate::capture::Recorded {
    crate::capture::context(&capture)
}

/// Bring the main window forward and open one note in it.
///
/// The capture window's *Open in knobas* button. The capture window closes
/// itself afterwards; this command does not close it, because the two acts have
/// different failure modes and a reader whose main window would not come
/// forward should still have the window their words are in.
///
/// # Errors
///
/// [`IpcErrorCode::NotFound`](crate::IpcErrorCode::NotFound) if the main window
/// is not open, [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) if it
/// will not come forward.
#[tauri::command]
pub fn reveal_note<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    note_id: String,
) -> Result<(), IpcError> {
    crate::capture::reveal_note(&app, &note_id)
}

/// Turn *Show notes and quick capture* on or off (#832, D10), registering or
/// releasing the stored capture shortcut to match. Answers the capture
/// shortcut's view, which carries the setting.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) if the write
/// fails.
#[tauri::command]
pub async fn set_notes_shown<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    capture: State<'_, crate::capture::CaptureState>,
    shown: bool,
) -> Result<crate::capture::ShortcutView, IpcError> {
    let pool = lifecycle.pool()?;
    crate::capture::set_shown(&pool, &capture, &crate::capture::Plugin::new(app), shown).await
}

/// *Export notes as Markdown* (#832, D10): one `.md` per note in
/// `~/Downloads/knobas-notes`, through the block-to-Markdown converter.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) if there is no
/// Downloads folder, the read fails or a file cannot be written.
#[tauri::command]
pub async fn export_notes_markdown<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
) -> Result<crate::notes_export::NotesExport, IpcError> {
    use tauri::Manager;
    let pool = lifecycle.pool()?;
    let downloads = app.path().download_dir().map_err(IpcError::internal)?;
    crate::notes_export::export(&pool, &downloads.join(crate::notes_export::FOLDER)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIRROR: &str = include_str!("../../../../app/src/lib/ipc/entity.ts");

    /// [`PIPELINES`] spells the pins' key as a literal (a `concat!` takes no
    /// constant), so this holds it to [`PINS_SETTING`] (#849).
    #[test]
    fn the_pins_are_read_by_their_constant() {
        assert!(PIPELINES.contains(&format!("key = '{PINS_SETTING}'")));
    }

    /// Every command this module registers is invoked by that name from the
    /// mirror, and every command the mirror invokes is registered.
    ///
    /// A `#[tauri::command]` is addressed by a *string*, so a rename on one
    /// side is not a compile error anywhere -- it is a button that rejects with
    /// Tauri's own "command not found" the first time somebody presses it. The
    /// list is read off `lib.rs`'s handler barrel rather than written out here,
    /// so a command registered and never mirrored fails this too.
    #[test]
    fn the_mirror_invokes_the_commands_by_their_registered_names() {
        let barrel = include_str!("../lib.rs");
        let registered: Vec<&str> = barrel
            .lines()
            .filter_map(|line| line.trim().strip_prefix("commands::entity::"))
            .map(|line| line.trim_end_matches(','))
            .collect();
        assert!(
            registered.len() >= 9,
            "only {} entity commands found in the handler barrel -- the parse \
             is wrong, not the barrel",
            registered.len()
        );
        for command in &registered {
            assert!(
                MIRROR.contains(&format!("\"{command}\"")),
                "{command} is registered but never invoked from entity.ts"
            );
        }
        for invoked in MIRROR.split("invoke<").skip(1) {
            let name = invoked
                .split_once('"')
                .and_then(|(_, rest)| rest.split_once('"'))
                .map(|(name, _)| name)
                .expect("every invoke names a command");
            assert!(
                registered.contains(&name),
                "entity.ts invokes {name}, which is not in the handler barrel"
            );
        }
    }

    /// `mini_board`'s saved-view argument (#1245) is the one `entity.ts` sends.
    /// Tauri reads a command's arguments by name, camel-cased, so a rename on
    /// one side alone is no compile error: the board would quietly answer the
    /// room instead of the view. Read off both sources, the way
    /// [`the_mirror_invokes_the_commands_by_their_registered_names`] reads the
    /// barrel.
    #[test]
    fn mini_board_takes_the_saved_view_the_mirror_sends() {
        let own = include_str!("entity.rs");
        let signature = own
            .split_once(concat!("pub async fn ", "mini_board(")) // split, or it finds this line
            .and_then(|(_, rest)| rest.split_once(") ->"))
            .map(|(arguments, _)| arguments)
            .expect("mini_board's signature");
        assert!(
            signature.contains("saved_view: Option<String>,"),
            "mini_board takes no `saved_view`: {signature}"
        );
        let sent = MIRROR
            .split_once("invoke<MiniBoard>(\"mini_board\", {")
            .and_then(|(_, rest)| rest.split_once("});"))
            .map(|(arguments, _)| arguments)
            .expect("entity.ts invokes mini_board");
        assert!(
            sent.contains("savedView: "),
            "entity.ts sends mini_board no `savedView`: {sent}"
        );
    }

    /// `mini_board` takes a role and no source list (#1299), and `entity.ts`
    /// sends exactly that: a `sources` left on the mirror would be dropped by
    /// Tauri unread, and the board would quietly answer every source. Read
    /// off both sources, as the saved-view test above reads them.
    #[test]
    fn mini_board_takes_the_role_the_mirror_sends() {
        let own = include_str!("entity.rs");
        let signature = own
            .split_once(concat!("pub async fn ", "mini_board(")) // split, or it finds this line
            .and_then(|(_, rest)| rest.split_once(") ->"))
            .map(|(arguments, _)| arguments)
            .expect("mini_board's signature");
        assert!(
            signature.contains("role: Option<String>,"),
            "mini_board takes no `role`: {signature}"
        );
        assert!(
            !signature.contains("sources"),
            "mini_board still takes `sources`: {signature}"
        );
        let sent = MIRROR
            .split_once("invoke<MiniBoard>(\"mini_board\", {")
            .and_then(|(_, rest)| rest.split_once("});"))
            .map(|(arguments, _)| arguments)
            .expect("entity.ts invokes mini_board");
        assert!(
            sent.contains("role: "),
            "entity.ts sends mini_board no `role`: {sent}"
        );
        assert!(
            !sent.contains("sources"),
            "entity.ts still sends mini_board `sources`: {sent}"
        );
    }

    /// §3a, as a resolution rather than as a claim.
    ///
    /// The whole promise is *"a new source's items get grouped, chipped and
    /// labeled without touching core"*, and it holds only if this lookup
    /// actually reaches the adapter's own declaration. The mock declares
    /// `ticket`, so a mock ticket must come back with the mock's words --
    /// which the frontend's humaniser could not produce (it has no `plural`
    /// and no monogram to invent).
    #[test]
    fn a_declared_kind_resolves_to_the_adapters_own_metadata() {
        let mock = crate::sources::Registry::builtin()
            .templates()
            .into_iter()
            .find(|template| template.adapter_kind == "mock")
            .expect("the mock adapter is compiled in");
        let declared = mock
            .entity_kinds
            .first()
            .expect("the mock declares at least one kind")
            .clone();

        let resolved =
            kind_info_for(Some("mock"), &declared.id).expect("a kind the mock declares resolves");
        assert_eq!(resolved.id, declared.id);
        assert_eq!(resolved.label, declared.label);
        assert_eq!(resolved.plural, declared.plural);
        assert_eq!(resolved.monogram, declared.monogram);
    }

    /// Three ways to have nothing to say, and all three say nothing.
    ///
    /// The middle one is the case that matters operationally: `run_once`
    /// writes mirror rows for a source with **no configuration row**, so
    /// `adapter_kind` is genuinely `None` there and a lookup that unwrapped it
    /// would panic on a perfectly ordinary corpus.
    #[test]
    fn nothing_declared_resolves_to_none_rather_than_to_a_guess() {
        assert!(kind_info_for(Some("mock"), "no-such-kind").is_none());
        assert!(kind_info_for(None, "ticket").is_none());
        assert!(kind_info_for(Some("not-a-compiled-in-adapter"), "ticket").is_none());
    }

    /// One kind id declared by two adapters resolves by **adapter**, not by
    /// kind.
    ///
    /// `jira` and `mock` both declare `ticket` today. A lookup that searched
    /// every template for the kind would hand a Jira ticket the mock's words
    /// (or the reverse) depending only on table order -- which is exactly the
    /// per-adapter table §3a exists to avoid, built by accident.
    #[test]
    fn two_adapters_declaring_one_kind_do_not_fight() {
        let templates = crate::sources::Registry::builtin().templates();
        let declaring: Vec<&knobas_source::SourceDescriptor> = templates
            .iter()
            .filter(|template| template.entity_kinds.iter().any(|info| info.id == "ticket"))
            .collect();
        assert!(
            declaring.len() >= 2,
            "only {} adapter(s) declare `ticket`, so this test proves nothing",
            declaring.len()
        );

        for template in declaring {
            let resolved = kind_info_for(Some(&template.adapter_kind), "ticket")
                .expect("the adapter declares `ticket`");
            let expected = template
                .entity_kinds
                .iter()
                .find(|info| info.id == "ticket")
                .expect("it is in the list this loop filtered on");
            // Field by field: `KindInfo` has no `PartialEq`, and adding one
            // would be an edit to the frozen SPI for a test's convenience.
            assert_eq!(
                (
                    resolved.label.as_str(),
                    resolved.plural.as_str(),
                    resolved.monogram.as_str(),
                    resolved.full_sync_exhaustive
                ),
                (
                    expected.label.as_str(),
                    expected.plural.as_str(),
                    expected.monogram.as_str(),
                    expected.full_sync_exhaustive
                ),
                "`ticket` from {} resolved to another adapter's declaration",
                template.adapter_kind
            );
        }
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

// -- contexts (#47) ---------------------------------------------------------
//
// In this module rather than one of their own: §10.8 freezes the `commands/`
// + `ipc/` module layout ("no new module"), and contexts are the entity
// stream's own objects — linkable, listable, and read by the same surfaces.

/// The switcher's list: every unarchived context, newest first.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the
/// database is still coming up, [`Internal`](crate::IpcErrorCode::Internal)
/// for a query failure.
#[tauri::command]
pub async fn list_contexts(
    lifecycle: State<'_, Lifecycle>,
) -> Result<Vec<knobas_core::context::ContextRow>, IpcError> {
    let pool = lifecycle.pool()?;
    Ok(knobas_core::context::list(&pool).await?)
}

/// Create an ad-hoc context from a label.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) if the label is blank -- a room
/// with no name is not addressable by a person;
/// [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
pub async fn create_context_inner(
    pool: &PgPool,
    title: &str,
) -> Result<knobas_core::context::ContextRow, IpcError> {
    let title = title.trim();
    if title.is_empty() {
        return Err(IpcError::invalid("a context needs a label"));
    }
    let row = knobas_core::context::create_adhoc(pool, title).await?;
    record(pool, "created", &row).await?;
    Ok(row)
}

/// Promote an entity to a context of its own (spec §7).
///
/// Idempotent by the store's own promise: promoting twice answers with the one
/// context, and only the first call writes an activity line -- the second
/// mutated nothing, so it announces nothing.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) if `entity_id` is not an entity
/// id, or if it is itself a context;
/// [`NotFound`](crate::IpcErrorCode::NotFound) if nothing local carries it;
/// [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
pub async fn promote_context_inner(
    pool: &PgPool,
    entity_id: &str,
) -> Result<knobas_core::context::Promoted, IpcError> {
    let anchor = EntityRef::parse(entity_id).map_err(IpcError::invalid)?;
    let promoted = knobas_core::context::promote(pool, &anchor)
        .await?
        .ok_or_else(|| IpcError::not_found(format!("{anchor} is not in the mirror")))?;
    // One line per *mutation*: the store says which call inserted, so a
    // re-promotion answers with the room and writes nothing.
    if promoted.fresh {
        record(pool, "promoted", &promoted.context).await?;
    }
    Ok(promoted)
}

/// Who is in this context, by the fixed rule (§16.11, ADR-0008).
///
/// The per-context inbox filter ("3 here") intersects the inbox stream with
/// this set on the frontend, so the badge and the list it filters are drawn
/// from the same rows by construction.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the
/// database is still coming up, [`Internal`](crate::IpcErrorCode::Internal)
/// for a query failure.
#[tauri::command]
pub async fn context_members(
    lifecycle: State<'_, Lifecycle>,
    ctx_id: String,
) -> Result<Vec<String>, IpcError> {
    let pool = lifecycle.pool()?;
    Ok(knobas_core::context::member_ids(&pool, &ctx_id).await?)
}

/// What a ticket reaches (#817, R-2.8, R-2.9): each entity once, with the
/// first step of its path and where the path came from -- the ticket's own
/// links, or its parent's (R-7.3). `knobas_core::reach` is the walk; a job's
/// sections read it once for their reason labels, and `list_entities`'
/// `reach` scopes their rows by the same walk.
///
/// An id nothing answers to reaches nothing.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the
/// database is still coming up, [`Internal`](crate::IpcErrorCode::Internal)
/// for a query failure.
#[tauri::command]
pub async fn job_reach(
    lifecycle: State<'_, Lifecycle>,
    entity_id: String,
) -> Result<Vec<knobas_core::reach::Reached>, IpcError> {
    let pool = lifecycle.pool()?;
    Ok(knobas_core::reach::reach(&pool, &entity_id).await?)
}

/// Whether *Sub-tasks show their parent ticket's connections* is on (R-7.3).
/// On until somebody switches it off.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`Internal`](crate::IpcErrorCode::Internal) if the read fails.
#[tauri::command]
pub async fn parent_links(lifecycle: State<'_, Lifecycle>) -> Result<bool, IpcError> {
    let pool = lifecycle.pool()?;
    Ok(knobas_core::reach::parent_links(&pool).await?)
}

/// Switch *Sub-tasks show their parent ticket's connections* (R-7.3), and
/// answer what is now stored, as `set_passive_attribution` does.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`Internal`](crate::IpcErrorCode::Internal) if the write fails.
#[tauri::command]
pub async fn set_parent_links(
    lifecycle: State<'_, Lifecycle>,
    enabled: bool,
) -> Result<bool, IpcError> {
    let pool = lifecycle.pool()?;
    Ok(knobas_core::reach::set_parent_links(&pool, enabled).await?)
}

/// One activity line per context mutation, on the context's own entity.
async fn record(
    pool: &PgPool,
    verb: &str,
    row: &knobas_core::context::ContextRow,
) -> Result<(), IpcError> {
    let entity = EntityRef::parse(&row.id).map_err(IpcError::internal)?;
    let mut detail = serde_json::json!({
        "context_id": row.id,
        "kind": row.kind,
        "title": row.title,
    });
    // Absent rather than null for an ad-hoc context, the discipline
    // `link_detail` records: a reader of the log should not meet a key that
    // says "there is nothing here".
    if let (Some(map), Some(anchor)) = (detail.as_object_mut(), row.anchor_id.as_deref()) {
        map.insert("anchor_id".to_owned(), serde_json::Value::from(anchor));
    }
    knobas_core::activity::record(pool, ACTOR, verb, Some(&entity), detail).await?;
    Ok(())
}

/// Create an ad-hoc context, and announce the switcher has a new room.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the
/// database is still coming up, and whatever [`create_context_inner`] refuses
/// with.
#[tauri::command]
pub async fn create_context<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    title: String,
) -> Result<knobas_core::context::ContextRow, IpcError> {
    let pool = lifecycle.pool()?;
    let row = create_context_inner(&pool, &title).await?;
    announce_context(&app, &row);
    Ok(row)
}

/// Promote an entity to a context, and announce it.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the
/// database is still coming up, and whatever [`promote_context_inner`] refuses
/// with.
#[tauri::command]
pub async fn promote_context<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    entity_id: String,
) -> Result<knobas_core::context::ContextRow, IpcError> {
    let pool = lifecycle.pool()?;
    let promoted = promote_context_inner(&pool, &entity_id).await?;
    // Only a mutation is news: a re-promotion changed nothing the switcher
    // could learn from re-listing.
    if promoted.fresh {
        announce_context(&app, &promoted.context);
    }
    Ok(promoted.context)
}

/// Put a changed context on `contexts:changed`.
///
/// Best-effort, like every emit in this app: a failure means no window is
/// listening, which is not a reason to fail a write that already landed. What
/// rides on the event is the row itself, so a listener can splice rather than
/// re-list -- though re-listing is also correct, and is what the switcher
/// does.
fn announce_context<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    row: &knobas_core::context::ContextRow,
) {
    crate::events::emit(app, None, crate::events::CONTEXTS_CHANGED, row);
}

// -- the mini board (#177) --------------------------------------------------
//
// In this module for the reason the contexts block above gives: §10.8 freezes
// the `commands/` + `ipc/` layout, so a new read gets a section rather than a
// file. It is a room read, and the shell reaches it from the room.

/// The ticket board (ADR-0009, spec #175).
///
/// `role` (#1299, ADR-0029 as amended on 2026-10-19) narrows the board to
/// the sources that fill that role (`sources::crud::fillers`), read per call:
/// the Tickets page asks for the ticket system's. Its columns and counts are
/// then the fillers' alone; a role nobody fills is an empty board, read.
/// Omitted, every source's tickets are on it. `ctx_id` is a stored
/// context and `project` a project key; every narrowing given intersects.
/// The grouping, the column order and the two payload reads all live in
/// [`knobas_core::mini_board`]; this seam adds only the role's fillers.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the
/// database is still coming up, [`Internal`](crate::IpcErrorCode::Internal)
/// for a query failure. An unknown context is an empty board, not a
/// `not_found`: the room's other tiles answer that way too, and a tile that
/// errored would say "this broke" where the truth is "nothing here".
///
/// `per_column` cuts each column to its newest cards (#753), the Tickets
/// tile's read; omitted, the board is whole, which is what the ticket detail's
/// status select reads.
///
/// `query` is the ticket-first Tickets view's filter (#810, R-2.7): search
/// grammar text, the view's base query and the filter bar's words and chips.
/// A non-blank one narrows the board to every ticket the grammar admits
/// ([`knobas_search::Searcher::matching_ids`]); omitted or blank, it narrows
/// nothing.
///
/// `saved_view` (#877; named for the role by #1245) names a ticket system's
/// saved view (`sync.saved_view`'s entity id): the board is then
/// [`saved_view_board`]'s, the tickets the source put on that view in its
/// order under the view's own columns, and the room dimensions are not read.
///
/// `lanes` (#939, #1098) puts the board in lanes by that field (R-3.5) on
/// the server: `per_column` then caps each lane's column, and only the
/// first `lanes_first` lanes, biggest first, are sent
/// ([`knobas_core::mini_board::MiniBoard::lanes`]). Omitted, the board has
/// no lanes, as before.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn mini_board(
    lifecycle: State<'_, Lifecycle>,
    ctx_id: Option<String>,
    role: Option<String>,
    project: Option<String>,
    per_column: Option<u32>,
    query: Option<String>,
    saved_view: Option<String>,
    lanes: Option<knobas_core::mini_board::LaneField>,
    lanes_first: Option<u32>,
) -> Result<knobas_core::mini_board::MiniBoard, IpcError> {
    let pool = lifecycle.pool()?;
    let lanes = lanes.map(|by| knobas_core::mini_board::Lanes {
        by,
        first: lanes_first,
    });
    if let Some(view) = saved_view {
        return saved_view_board_laned(&pool, &view, per_column, query.as_deref(), lanes).await;
    }
    mini_board_laned(
        &pool,
        Room {
            ctx_id: ctx_id.as_deref(),
            role: role.as_deref(),
            project: project.as_deref(),
        },
        per_column,
        query.as_deref(),
        lanes,
    )
    .await
}

/// A board's three dimensions, as [`mini_board`] takes them.
#[derive(Clone, Copy, Debug, Default)]
pub struct Room<'a> {
    pub ctx_id: Option<&'a str>,
    /// The role whose fillers the board is read over (#1299); `None` reads
    /// every source.
    pub role: Option<&'a str>,
    pub project: Option<&'a str>,
}

/// [`mini_board`], against a pool.
///
/// Every card says whether it is assigned to me, by the usernames the
/// enabled sources are configured with: the identity `@me` resolves to.
///
/// # Errors
///
/// See [`mini_board`]; a query past the launcher's length is `invalid`.
pub async fn mini_board_inner(
    pool: &PgPool,
    ctx_id: Option<&str>,
    role: Option<&str>,
    project: Option<&str>,
    per_column: Option<u32>,
    query: Option<&str>,
) -> Result<knobas_core::mini_board::MiniBoard, IpcError> {
    mini_board_laned(
        pool,
        Room {
            ctx_id,
            role,
            project,
        },
        per_column,
        query,
        None,
    )
    .await
}

/// [`mini_board_inner`] in lanes (#939): see [`mini_board`]'s `lanes`.
///
/// # Errors
///
/// See [`mini_board_inner`].
pub async fn mini_board_laned(
    pool: &PgPool,
    room: Room<'_>,
    per_column: Option<u32>,
    query: Option<&str>,
    lanes: Option<knobas_core::mini_board::Lanes>,
) -> Result<knobas_core::mini_board::MiniBoard, IpcError> {
    let Room {
        ctx_id,
        role,
        project,
    } = room;
    // The role's fillers (#1299). The core reads an empty source list as
    // every source, so a role nobody fills answers here, with nothing on it.
    let sources = match role {
        Some(role) => {
            let fillers =
                crate::sources::crud::fillers(pool, &crate::sources::Registry::builtin(), role)
                    .await?;
            if fillers.is_empty() {
                return Ok(knobas_core::mini_board::MiniBoard {
                    columns: Vec::new(),
                    sources: Vec::new(),
                    lanes: Vec::new(),
                    more_lanes: 0,
                });
            }
            fillers
        }
        None => Vec::new(),
    };
    let declarations = declared_paths(pool).await?;
    let only = match query.map(str::trim).filter(|query| !query.is_empty()) {
        Some(query) => {
            crate::commands::search::searcher(pool)
                .await?
                .matching_ids(query, "ticket")
                .await?
        }
        None => None,
    };
    let identity = knobas_search::Vocabulary::load(pool, knobas_search::KindCatalog::default())
        .await?
        .identity;
    Ok(knobas_core::mini_board::read_scoped(
        pool,
        knobas_core::mini_board::Scope {
            ctx_id,
            sources: &sources,
            project,
            only: only.as_deref(),
            identity: &identity,
            lanes,
        },
        &declarations,
        per_column,
    )
    .await?)
}

/// A saved view's board, such as a Jira view's (#877, #1245, R-3.21, R-3.24):
/// the live tickets the source put on the view (`sync.saved_view_member`),
/// narrowed by `query` like [`mini_board_inner`], **in the source's order**,
/// under the view's own columns -- each holding the tickets whose status is
/// one of its statuses, empty ones included, as on the source's board. A
/// status on no column does not show. A view with no columns keeps one column
/// per status. `per_column` cuts each column after its first cards in that
/// order; `total` counts them all.
///
/// Keys the source returned that knobas does not mirror (a project the source
/// does not sync) are not on the board; they stay in the member table.
///
/// # Errors
///
/// `not_found` for a view the mirror does not hold; otherwise as
/// [`mini_board_inner`].
pub async fn saved_view_board(
    pool: &PgPool,
    view: &str,
    per_column: Option<u32>,
    query: Option<&str>,
) -> Result<knobas_core::mini_board::MiniBoard, IpcError> {
    saved_view_board_laned(pool, view, per_column, query, None).await
}

/// [`saved_view_board`] in lanes (#939): the whole view put in lanes after it
/// is read ([`knobas_core::mini_board::in_lanes`]), each lane's column in
/// the source's order and capped at `per_column`.
///
/// # Errors
///
/// See [`saved_view_board`].
pub async fn saved_view_board_laned(
    pool: &PgPool,
    view: &str,
    per_column: Option<u32>,
    query: Option<&str>,
    lanes: Option<knobas_core::mini_board::Lanes>,
) -> Result<knobas_core::mini_board::MiniBoard, IpcError> {
    use knobas_core::mini_board::{MiniBoard, MiniBoardCard, MiniBoardColumn};
    let (per_column, lanes_cap) = match lanes {
        Some(_) => (None, per_column),
        None => (per_column, None),
    };
    let columns: Option<serde_json::Value> =
        sqlx::query_scalar("select columns from sync.saved_view where entity_id = $1")
            .bind(view)
            .fetch_optional(pool)
            .await?;
    let Some(columns) = columns else {
        return Err(IpcError::not_found(format!(
            "no Jira view is called {view}"
        )));
    };
    let columns: Vec<crate::ticket_views::ViewColumn> =
        serde_json::from_value(columns).map_err(IpcError::internal)?;
    let members: Vec<String> = sqlx::query_scalar(
        "select entity_id from sync.saved_view_member where view_id = $1 order by rank",
    )
    .bind(view)
    .fetch_all(pool)
    .await?;
    let rank: std::collections::HashMap<&str, usize> = members
        .iter()
        .enumerate()
        .map(|(at, id)| (id.as_str(), at))
        .collect();
    let mut only = members.clone();
    if let Some(query) = query.map(str::trim).filter(|query| !query.is_empty())
        && let Some(matched) = crate::commands::search::searcher(pool)
            .await?
            .matching_ids(query, "ticket")
            .await?
    {
        let matched: std::collections::HashSet<String> = matched.into_iter().collect();
        only.retain(|id| matched.contains(id));
    }
    let declarations = declared_paths(pool).await?;
    let identity = knobas_search::Vocabulary::load(pool, knobas_search::KindCatalog::default())
        .await?
        .identity;
    let whole = knobas_core::mini_board::read_scoped(
        pool,
        knobas_core::mini_board::Scope {
            only: Some(&only),
            identity: &identity,
            ..knobas_core::mini_board::Scope::default()
        },
        &declarations,
        None,
    )
    .await?;
    let in_order = |mut cards: Vec<MiniBoardCard>| -> MiniBoardColumn {
        cards.sort_by_key(|card| {
            rank.get(card.entity_id.as_str())
                .copied()
                .unwrap_or(usize::MAX)
        });
        let total = u32::try_from(cards.len()).unwrap_or(u32::MAX);
        if let Some(cap) = per_column {
            cards.truncate(cap as usize);
        }
        MiniBoardColumn {
            status: None,
            cards,
            total,
        }
    };
    let drawn: Vec<MiniBoardColumn> = if columns.is_empty() {
        whole
            .columns
            .into_iter()
            .map(|column| MiniBoardColumn {
                status: column.status,
                ..in_order(column.cards)
            })
            .collect()
    } else {
        let mut by_status: std::collections::HashMap<Option<String>, Vec<MiniBoardCard>> = whole
            .columns
            .into_iter()
            .map(|column| (column.status, column.cards))
            .collect();
        columns
            .iter()
            .map(|column| MiniBoardColumn {
                status: Some(column.name.clone()),
                ..in_order(
                    column
                        .statuses
                        .iter()
                        .flat_map(|status| {
                            by_status.remove(&Some(status.clone())).unwrap_or_default()
                        })
                        .collect(),
                )
            })
            .collect()
    };
    let board = MiniBoard {
        columns: drawn,
        sources: whole.sources,
        lanes: Vec::new(),
        more_lanes: 0,
    };
    Ok(match lanes {
        Some(lanes) => knobas_core::mini_board::in_lanes(board, lanes, lanes_cap),
        None => board,
    })
}

/// The statuses one source's live ticket corpus shows (#760): the ticket
/// detail's fallback offer when the workflow cannot be read (#179, #498).
///
/// Asked only then. It used to come out of a whole [`mini_board`] read of the
/// ticket's source on every open (133-137 ms and 725 KB at 5,000 tickets);
/// the current status is [`EntityDetail::status`] now.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the
/// database is still coming up; a query failure is `Internal`.
#[tauri::command]
pub async fn source_statuses(
    lifecycle: State<'_, Lifecycle>,
    source_id: String,
) -> Result<Vec<String>, IpcError> {
    let pool = lifecycle.pool()?;
    let declarations = declared_paths(&pool).await?;
    Ok(knobas_core::mini_board::source_statuses(&pool, &source_id, &declarations).await?)
}

// -- the projects a corpus shows (#208) -------------------------------------
//
// Here for the reason the two sections above give: §10.8 freezes the
// `commands/` + `ipc/` layout, so an entity read gets a section rather than a
// file. A project is a grouping of *entities*, read out of the same mirror
// rows this module already lists.

/// Every project the live corpus shows, ordered by source then key
/// (ADR-0010, #208).
///
/// What the switcher builds its project rooms from. Flat rather than grouped
/// per source, because the switcher wants a room list and grouping it here
/// would only be ungrouped there. It cannot come from a room's own scan
/// instead: that read is a window over the newest items and explicitly not a
/// census, so a quiet project would silently have no room.
///
/// Unscoped, and answering for the **live** corpus -- so a source the user
/// turned off reports no projects (migration `0012`), and its projects come
/// back with it. Everything it decides lives in [`knobas_core::project`];
/// there is nothing for this seam to add, so it adds nothing, the same as
/// [`mini_board`] and [`context_members`].
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the
/// database is still coming up, [`Internal`](crate::IpcErrorCode::Internal)
/// for a query failure.
#[tauri::command]
pub async fn list_projects(
    lifecycle: State<'_, Lifecycle>,
) -> Result<Vec<knobas_core::project::Project>, IpcError> {
    list_projects_inner(&lifecycle.pool()?).await
}

/// [`list_projects`] with the pool handed in, so a test can reach it.
///
/// The same split every other read in this module has, and for the same
/// reason: `tauri::State` cannot be constructed by hand, so a command that
/// resolved its own pool would be unreachable and a test of it would have to
/// re-type the two lines it contains -- which is a test that goes on passing
/// after the command stops doing this. Pinned by
/// `a_confluence_space_is_a_project_room_and_a_jira_project_is_another` in
/// `tests/it/entity.rs`, which dies when the declarations stop being stored
/// here.
///
/// # Errors
///
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) for a query
/// failure or a declaration write that fails.
pub async fn list_projects_inner(
    pool: &PgPool,
) -> Result<Vec<knobas_core::project::Project>, IpcError> {
    // The declarations' stored copy, written first as every search command
    // writes it (one read when nothing changed): the census reads
    // `sync.item_project`, which the database resolves from it (#759).
    crate::commands::search::searcher(pool).await?;
    Ok(knobas_core::project::list(pool).await?)
}

// -- the Docs reader (#859) -------------------------------------------------
//
// Here for the frozen layout's reason (§10.8: no new `commands/` or `ipc/`
// module). The reads are `crate::docs`; these are the shims.

/// The Docs reader's left pane: every space the mirror holds pages of, each
/// with its page tree, and the page read last (#859, ADR-0022). The tree is
/// the wiki role's by definition, so it narrows itself to the role's fillers
/// (`sources::crud::fillers`, #1296, ADR-0029 as amended on 2026-10-19)
/// with no argument: a source with a wiki miss, one whose adapter claims no
/// wiki or is not installed, and the pages a removed source left behind are
/// in no space, never the page read last and never *For you*.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
#[tauri::command]
pub async fn docs_tree(lifecycle: State<'_, Lifecycle>) -> Result<crate::docs::DocsTree, IpcError> {
    let pool = lifecycle.pool()?;
    docs_tree_inner(&pool).await
}

/// [`docs_tree`] with the pool handed in, for a test.
///
/// # Errors
///
/// As [`docs_tree`].
pub async fn docs_tree_inner(pool: &PgPool) -> Result<crate::docs::DocsTree, IpcError> {
    // The census table the spaces come from is resolved from the stored
    // declarations, which every search command writes first (#759).
    crate::commands::search::searcher(pool).await?;
    let declarations = declared_paths(pool).await?;
    let me = knobas_search::Vocabulary::load(pool, knobas_search::KindCatalog::default())
        .await
        .map_err(IpcError::internal)?
        .identity;
    let registry = crate::sources::Registry::builtin();
    let fillers = crate::sources::crud::fillers(pool, &registry, knobas_source::role::WIKI).await?;
    crate::docs::tree(
        pool,
        &declarations,
        |kind| registry.doc_projection(kind),
        &me,
        &fillers,
    )
    .await
}

/// One page as the Docs reader draws it: header fields, blocks and comments
/// (#859, ADR-0022).
///
/// # Errors
///
/// `invalid` for something that is not an entity id, `not_found` for one the
/// live mirror does not hold or that is not a page, `internal` for a query
/// failure, `not_ready` before bring-up.
#[tauri::command]
pub async fn docs_page(
    lifecycle: State<'_, Lifecycle>,
    entity_id: String,
) -> Result<crate::docs::DocPage, IpcError> {
    let pool = lifecycle.pool()?;
    docs_page_inner(&pool, &entity_id).await
}

/// [`docs_page`] with the pool handed in, for a test.
///
/// # Errors
///
/// As [`docs_page`].
pub async fn docs_page_inner(
    pool: &PgPool,
    entity_id: &str,
) -> Result<crate::docs::DocPage, IpcError> {
    crate::commands::search::searcher(pool).await?;
    let declarations = declared_paths(pool).await?;
    let me = knobas_search::Vocabulary::load(pool, knobas_search::KindCatalog::default())
        .await
        .map_err(IpcError::internal)?
        .identity;
    let registry = crate::sources::Registry::builtin();
    let templates = registry.templates();
    crate::docs::page(
        pool,
        &declarations,
        |kind| registry.doc_projection(kind),
        |kind| crate::docs::docs_writes(&templates, registry.doc_writes(kind).is_some(), kind),
        &me,
        entity_id,
    )
    .await
}

/// The Docs search (#860, R-10.3, R-10.4): the pages a query admits, best
/// first, each with its snippet, match count, space, path and editor; `None`
/// for a query with neither words nor a filter, which the pane answers with
/// its tree. `space` is the picker's space; a typed `space:` wins over it.
///
/// # Errors
///
/// `invalid` for a query past the launcher's length, `internal` for a query
/// failure, `not_ready` before bring-up.
#[tauri::command]
pub async fn docs_search(
    lifecycle: State<'_, Lifecycle>,
    query: String,
    space: Option<crate::docs::DocsScope>,
) -> Result<Option<crate::docs::DocsHits>, IpcError> {
    let pool = lifecycle.pool()?;
    docs_search_inner(&pool, &query, space.as_ref()).await
}

/// [`docs_search`] with the pool handed in, for a test.
///
/// # Errors
///
/// As [`docs_search`].
pub async fn docs_search_inner(
    pool: &PgPool,
    query: &str,
    space: Option<&crate::docs::DocsScope>,
) -> Result<Option<crate::docs::DocsHits>, IpcError> {
    let searcher = crate::commands::search::searcher(pool).await?;
    let registry = crate::sources::Registry::builtin();
    // The wiki's search, so it narrows itself to the role's fillers (#1296).
    let fillers = crate::sources::crud::fillers(pool, &registry, knobas_source::role::WIKI).await?;
    crate::docs::search(
        pool,
        &searcher,
        |kind| registry.doc_projection(kind),
        query,
        space,
        &fillers,
    )
    .await
}

/// *Export as Markdown* (#861, R-10.17): one page as `.md` in Downloads,
/// through the one block-to-Markdown converter.
///
/// # Errors
///
/// As [`docs_page`]; `invalid` for a page knobas cannot read; `internal` if
/// there is no Downloads folder or the file cannot be written.
#[tauri::command]
pub async fn docs_export_markdown<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    entity_id: String,
) -> Result<crate::docs::DocExport, IpcError> {
    use tauri::Manager;
    let pool = lifecycle.pool()?;
    let downloads = app.path().download_dir().map_err(IpcError::internal)?;
    docs_export_markdown_inner(&pool, &entity_id, &downloads).await
}

/// [`docs_export_markdown`] into `folder`, for a test.
///
/// # Errors
///
/// As [`docs_export_markdown`].
pub async fn docs_export_markdown_inner(
    pool: &PgPool,
    entity_id: &str,
    folder: &std::path::Path,
) -> Result<crate::docs::DocExport, IpcError> {
    crate::commands::search::searcher(pool).await?;
    let declarations = declared_paths(pool).await?;
    let registry = crate::sources::Registry::builtin();
    crate::docs::export(
        pool,
        &declarations,
        |kind| registry.doc_projection(kind),
        entity_id,
        folder,
    )
    .await
}

/// *Edit section* (#862, R-10.13): queue a section's new text as the page's
/// whole new body (`UpdatePage`) through the write queue. `version` is the
/// version the reader read; a page that moved since is refused.
///
/// # Errors
///
/// As [`docs_edit_section_payload`], then whatever the write queue refuses.
#[tauri::command]
pub async fn docs_edit_section<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    entity_id: String,
    section: u32,
    version: Option<u64>,
    text: String,
) -> Result<knobas_core::write_queue::QueuedWrite, IpcError> {
    let state = crate::sources::state(&app)?;
    let payload =
        docs_edit_section_payload(&state.pool, &entity_id, section, version, &text).await?;
    crate::sources::write_queue::submit_in_background(&state, payload).await
}

/// The `UpdatePage` [`docs_edit_section`] queues, for a test.
///
/// # Errors
///
/// `invalid` for a section knobas does not edit, a page that moved past
/// `version`, or a source that writes no pages; `not_found` for a page the
/// live mirror does not hold.
pub async fn docs_edit_section_payload(
    pool: &PgPool,
    entity_id: &str,
    section: u32,
    version: Option<u64>,
    text: &str,
) -> Result<serde_json::Value, IpcError> {
    let registry = crate::sources::Registry::builtin();
    crate::docs::section_edit(
        pool,
        |kind| registry.doc_writes(kind),
        entity_id,
        section,
        version,
        text,
    )
    .await
}

/// *New page* (#862, R-10.16): queue a `CreatePage` under a mirrored page,
/// its body the editor's plain text in the source's markup. With `link_to`
/// (a ticket's entity id, *Link it to <job>*), the ticket is held with the
/// write and the new page is linked to it once the write has landed and a
/// sync has brought the page in, on whichever tick that is (#1121,
/// [`create_page_in_background`]).
///
/// # Errors
///
/// As [`docs_create_page_payload`], then whatever the write queue refuses.
#[tauri::command]
pub async fn docs_create_page<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    page: crate::docs::NewPage,
    link_to: Option<String>,
) -> Result<knobas_core::write_queue::QueuedWrite, IpcError> {
    let state = crate::sources::state(&app)?;
    let (payload, _) = docs_create_page_payload(&state.pool, &page).await?;
    create_page_in_background(&state, payload, link_to).await
}

/// [`docs_create_page`] once its `CreatePage` is built: queued and sent in
/// the background, the source re-read after. With `link_to`, the ticket is
/// held with the write (`sources::write_queue::submit_linked_in_background`,
/// #1121), so a page whose write waits and lands on a later tick is still
/// linked to its job, by the first sync that brings it in.
///
/// # Errors
///
/// As `submit_linked_in_background` with `link_to`, else whatever the write
/// queue refuses.
pub async fn create_page_in_background(
    state: &crate::sources::SourcesState,
    payload: serde_json::Value,
    link_to: Option<String>,
) -> Result<knobas_core::write_queue::QueuedWrite, IpcError> {
    if let Some(ticket) = link_to {
        return crate::sources::write_queue::submit_linked_in_background(state, payload, ticket)
            .await;
    }
    let (queued, source) = crate::sources::write_queue::queue(state, payload).await?;
    crate::sources::write_queue::send_in_background_then(
        state,
        source.clone(),
        queued.id,
        source,
        |_| async {},
    );
    Ok(queued)
}

/// The `CreatePage` [`docs_create_page`] queues and the source it goes to,
/// for a test.
///
/// # Errors
///
/// `invalid` for a blank title, a parent with no space or a source that
/// writes no pages; `not_found` for a parent the live mirror does not hold.
pub async fn docs_create_page_payload(
    pool: &PgPool,
    page: &crate::docs::NewPage,
) -> Result<(serde_json::Value, String), IpcError> {
    // The space comes from the census table, kept from the stored
    // declarations every search command writes first (#759).
    crate::commands::search::searcher(pool).await?;
    let registry = crate::sources::Registry::builtin();
    crate::docs::create_page(pool, |kind| registry.doc_writes(kind), page).await
}

/// *Move to Confluence…* (#941, R-13a.6, D10): queue one `CreatePage` per
/// note under a mirrored page, through the write queue, and answer what was
/// queued. The notes stay. Every page is checked before the first is queued,
/// so a refused parent queues nothing.
///
/// # Errors
///
/// As [`move_notes_payloads`], then whatever the write queue refuses.
#[tauri::command]
pub async fn move_notes_to_confluence<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    parent: String,
) -> Result<Vec<knobas_core::write_queue::QueuedWrite>, IpcError> {
    let state = crate::sources::state(&app)?;
    let payloads = move_notes_payloads(&state.pool, &parent).await?;
    let mut queued = Vec::with_capacity(payloads.len());
    let mut last = None;
    for (payload, _) in payloads {
        let (write, source) = crate::sources::write_queue::queue(&state, payload).await?;
        last = Some((source, write.id));
        queued.push(write);
    }
    // One send drains the source's queue, so the pages go in the notes' order.
    if let Some((source, id)) = last {
        crate::sources::write_queue::send_in_background(&state, source, id);
    }
    Ok(queued)
}

/// The `CreatePage`s [`move_notes_to_confluence`] queues, one per note
/// (`notes_export::note_pages`), each with the source it goes to, for a test.
///
/// # Errors
///
/// `invalid` when there are no notes, and as [`docs_create_page_payload`]
/// for the parent.
pub async fn move_notes_payloads(
    pool: &PgPool,
    parent: &str,
) -> Result<Vec<(serde_json::Value, String)>, IpcError> {
    let notes = knobas_core::note::all(pool).await?;
    if notes.is_empty() {
        return Err(IpcError::invalid("There are no notes to move."));
    }
    crate::commands::search::searcher(pool).await?;
    let registry = crate::sources::Registry::builtin();
    let mut payloads = Vec::with_capacity(notes.len());
    for page in crate::notes_export::note_pages(&notes, parent) {
        payloads
            .push(crate::docs::create_page(pool, |kind| registry.doc_writes(kind), &page).await?);
    }
    Ok(payloads)
}

// -- the standup digest (#288) ----------------------------------------------
//
// Here for the reason the two sections above give, and the reason spec #272
// gives in as many words: §10.8 freezes the `commands/` + `ipc/` layout, and
// "standup and Confluence reads go into the entity module as usual" -- the
// `time` module pair was the one ratified exception and this is not a second
// one. What the read decides lives in `crate::standup`; this seam adds the
// pool, the identity and the declarations, the same three things
// `inbox_items_inner` adds, and nothing else.

/// The standup digest for one day (issue #288, spec #272 stories 58-63).
///
/// Three lists -- yesterday, today, blockers -- every line carrying the item
/// it came from, which source said so and which verb it was. `CONTEXT.md`'s
/// **digest**; [`crate::standup`] holds the rules and the reasoning.
///
/// **The webview computes the days**, each as a date and the two instants it
/// spans: `today` is the day being asked about and `earlier` the days before
/// it, oldest first. The rule is [`day_blocks`](crate::commands::time::day_blocks)'s
/// in full -- the machine's timezone is a fact only that side holds, and a UTC
/// offset would be the wrong shape as well as the wrong owner for a day
/// containing a daylight-saving change. How far back the *yesterday* rule may
/// reach is **not** the webview's to say: at most
/// [`standup::LOOKBACK_DAYS`](crate::standup::LOOKBACK_DAYS) of `earlier` are
/// consulted however many are sent.
///
/// **The registry is the injected one**, the shape [`inbox_items`] has and for
/// the same reason: the blockers list is read through the descriptors'
/// declared paths (#277), and a test that could not hand over a descriptor
/// could only witness the declarations the shipped adapters happen to carry --
/// which is a battery that passes just as well against a hardcoded list of
/// English status words. That is the one mistake this read must not be able to
/// make.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the
/// database or the sync engine is still coming up,
/// [`Internal`](crate::IpcErrorCode::Internal) for a read failure.
#[tauri::command]
pub async fn standup_digest<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    today: crate::time::week::DayWindow,
    earlier: Vec<crate::time::week::DayWindow>,
) -> Result<crate::standup::StandupDigest, IpcError> {
    let state = crate::sources::state(&app)?;
    standup_digest_inner(
        &state.pool,
        state.registry.as_ref(),
        Utc::now(),
        today,
        &earlier,
    )
    .await
}

/// [`standup_digest`] with the pool, the registry and the clock handed in, so
/// a test can reach it -- the split every read in this module has.
///
/// # Errors
///
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) for a read
/// failure or a source listing that fails.
pub async fn standup_digest_inner(
    pool: &PgPool,
    registry: &dyn knobas_sync::scheduler::AdapterRegistry,
    now: DateTime<Utc>,
    today: crate::time::week::DayWindow,
    earlier: &[crate::time::week::DayWindow],
) -> Result<crate::standup::StandupDigest, IpcError> {
    let identity = identity_of(pool).await?;
    let declarations = crate::sources::paths::declared_paths(pool, registry).await?;
    crate::standup::digest(pool, &identity, &declarations, now, today, earlier).await
}

// -- the standup protocol (#289) ---------------------------------------------
//
// Here for the reason the digest above it is here, and the same sentence of
// spec #272: §10.8 freezes the `commands/` + `ipc/` layout, "standup and
// Confluence reads go into the entity module as usual", and the `time` module
// pair was the one ratified exception. What these decide lives in
// `crate::protocol`; this seam adds the pool, the sources state and the
// announcement, and nothing else.

/// The protocol for a date, made if it is not there yet (issue #289, spec
/// #272 stories 64-67).
///
/// **Get-or-create, and this is the read the standup view opens with**, so it
/// is also where a publication settled in the reader's absence catches up:
/// [`protocol::reconcile`] runs on every read, and it is the whole of the path
/// by which a write that landed after a restart comes to have its link. The
/// page id it needs comes from `knobas.write_queue.remote_id`, which the
/// settle wrote and which outlives the `WriteReceipt` that carried it.
///
/// The **date is the webview's**, exactly as it is for the digest and the day
/// review: which day it is where the reader sits is a fact only that side
/// holds.
///
/// # Errors
///
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) for a read or
/// write failure.
pub async fn standup_protocol_inner(
    pool: &PgPool,
    day: chrono::NaiveDate,
) -> Result<crate::protocol::Protocol, IpcError> {
    let note = crate::protocol::get_or_create(pool, day).await?;
    // A publication is per date **and per Confluence**, so the read needs to
    // know which one -- and the stored target is the only thing that says.
    // With none stored nothing has been published from here, which is what
    // `None` says: `publish` refuses before it queues anything when the target
    // is missing, so there is no publication for a read to have missed.
    let publication = match crate::protocol::publish_target(pool).await? {
        None => None,
        Some(target) => {
            match crate::protocol::publication_of(pool, day, &target.source_id).await? {
                Some(publication) => {
                    Some(crate::protocol::reconcile(pool, &note.id, publication).await?)
                }
                None => None,
            }
        }
    };
    Ok(crate::protocol::Protocol {
        day,
        note_id: note.id,
        page_title: crate::protocol::page_title_of(day),
        publication,
    })
}

/// [`standup_protocol_inner`] behind the lifecycle.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the
/// database is still coming up; otherwise as [`standup_protocol_inner`].
#[tauri::command]
pub async fn standup_protocol(
    lifecycle: State<'_, Lifecycle>,
    day: chrono::NaiveDate,
) -> Result<crate::protocol::Protocol, IpcError> {
    standup_protocol_inner(&lifecycle.pool()?, day).await
}

/// Publish a date's protocol to Confluence (stories 65-68).
///
/// `target` is the answer to the dialog the **first** publish shows: given, it
/// is recorded as the setting and used; omitted, the stored one is used and a
/// missing one is a refusal rather than a guess. That is both directions of
/// story 67 in one command -- *asked for the first time, and changeable in
/// settings* -- with `set_standup_publish_target` as the other door onto the
/// same key.
///
/// **Which source is the caller's to decide, not this command's.** With two
/// Confluence sources configured there is no defensible default and story 68
/// says so in as many words, so the dialog asks and sends the answer here. A
/// command that picked one would be the mistake the story exists to prevent.
///
/// **A second publish for one date queues nothing.** See
/// `crate::protocol`'s ruling: the publication already on the queue is
/// answered with, so *Publish* pressed twice is one page. The one case that
/// composes a fresh op is a previous write that was refused or discarded,
/// where nothing landed.
///
/// The write goes through `sources::write_queue::submit` like every other, so
/// it queues when Confluence is down and it shows in the pending-writes panel.
/// `submit` also re-syncs the source when the write lands, which is what puts
/// the new page in the mirror in time for the link this call then draws --
/// `knobas.link`'s endpoints are `knobas.entity` rows, so the link cannot
/// precede the sync.
///
/// The publication is **re-read from the queue** rather than taken from what
/// `submit` handed back: the row is the durable record, the read is the same
/// one every later read makes, and taking the two answers from one place is
/// what stops them disagreeing.
///
/// # Errors
///
/// [`IpcErrorCode::Invalid`](crate::IpcErrorCode::Invalid) when no publish
/// target is stored and none was given, or when the one given is not a
/// coherent target; [`NotFound`](crate::IpcErrorCode::NotFound) when the
/// configured parent page is not in the mirror; otherwise whatever the write
/// queue refuses with.
pub async fn publish_standup_protocol_inner(
    state: &crate::sources::SourcesState,
    day: chrono::NaiveDate,
    target: Option<crate::protocol::PublishTarget>,
) -> Result<crate::protocol::Protocol, IpcError> {
    let pool = &state.pool;
    let target = target_for(pool, target).await?;
    if let Some(payload) = page_to_make(pool, day, &target).await? {
        crate::sources::write_queue::submit(state, payload).await?;
        // Wait for the re-read `submit` set off. The link this call is about
        // to draw points at a page that has an address only once a sync has
        // seen it, so triggering and hoping would leave every ordinary publish
        // unlinked until the reader happened to open the view again.
        crate::sources::write_queue::sync_after_write(state, &target.source_id).await;
    }
    standup_protocol_inner(pool, day).await
}

/// [`publish_standup_protocol_inner`], **answering once the page's write is
/// queued** (#779): what the `publish_standup_protocol` command does.
///
/// Every refusal the awaited path makes is made here too, before the answer:
/// a missing or incoherent target, a parent the mirror does not hold. The
/// answer's publication is the queued write -- `pending`, with no page id --
/// which the panel draws as on its way. The send, the sync that brings the
/// page into the mirror and the link ([`protocol::reconcile`], run by
/// [`standup_protocol_inner`]) follow in a spawned task, and every later read
/// of the protocol names the page and links it as well, so a panel that
/// re-reads when the write settles finds the address there.
///
/// The awaited path stays for the battery and `atlassian_live.rs`.
///
/// [`protocol::reconcile`]: crate::protocol::reconcile
///
/// # Errors
///
/// As [`publish_standup_protocol_inner`], all of them before anything is sent.
pub async fn publish_standup_protocol_in_background(
    state: &crate::sources::SourcesState,
    day: chrono::NaiveDate,
    target: Option<crate::protocol::PublishTarget>,
) -> Result<crate::protocol::Protocol, IpcError> {
    let pool = &state.pool;
    let target = target_for(pool, target).await?;
    if let Some(payload) = page_to_make(pool, day, &target).await? {
        let (queued, source_id) = crate::sources::write_queue::queue(state, payload).await?;
        crate::sources::write_queue::send_in_background_then(
            state,
            source_id,
            queued.id,
            target.source_id.clone(),
            move |state| async move {
                if let Err(error) = standup_protocol_inner(&state.pool, day).await {
                    tracing::warn!(%error, "a published protocol could not be linked yet");
                }
            },
        );
    }
    standup_protocol_inner(pool, day).await
}

/// The `CreatePage` a publish for `day` owes, or `None` when the date already
/// has a publication in the target's Confluence -- the module's one-page rule.
async fn page_to_make(
    pool: &PgPool,
    day: chrono::NaiveDate,
    target: &crate::protocol::PublishTarget,
) -> Result<Option<serde_json::Value>, IpcError> {
    let note = crate::protocol::get_or_create(pool, day).await?;
    if crate::protocol::publication_of(pool, day, &target.source_id)
        .await?
        .is_some()
    {
        return Ok(None);
    }
    let space = crate::protocol::space_of(pool, &target.parent).await?;
    Ok(Some(crate::protocol::create_page_payload(
        target,
        &space,
        day,
        &note.body_md,
    )))
}

/// The target a publish aims at: the one given, recorded as the setting, or
/// the stored one, or a refusal rather than a guess.
async fn target_for(
    pool: &PgPool,
    target: Option<crate::protocol::PublishTarget>,
) -> Result<crate::protocol::PublishTarget, IpcError> {
    Ok(match target {
        Some(chosen) => crate::protocol::set_publish_target(pool, &chosen).await?,
        None => crate::protocol::publish_target(pool)
            .await?
            .ok_or_else(|| {
                IpcError::invalid(
                    "knobas does not know where to publish standup protocols yet. Choose the \
                 Confluence source and the parent page first.",
                )
            })?,
    })
}

/// [`publish_standup_protocol_in_background`] behind the app handle: answers
/// once the page's write is queued (#779).
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up;
/// otherwise as [`publish_standup_protocol_inner`].
#[tauri::command]
pub async fn publish_standup_protocol<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    day: chrono::NaiveDate,
    target: Option<crate::protocol::PublishTarget>,
) -> Result<crate::protocol::Protocol, IpcError> {
    let state = crate::sources::state(&app)?;
    publish_standup_protocol_in_background(&state, day, target).await
}

/// Where standup protocols are published, or `null` until somebody has said
/// (story 67).
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the
/// database is still coming up, [`Internal`](crate::IpcErrorCode::Internal)
/// for a read failure.
#[tauri::command]
pub async fn standup_publish_target(
    lifecycle: State<'_, Lifecycle>,
) -> Result<Option<crate::protocol::PublishTarget>, IpcError> {
    crate::protocol::publish_target(&lifecycle.pool()?).await
}

/// Change where standup protocols are published -- settings' half of story 67.
///
/// The same key [`publish_standup_protocol`] writes when it is given a target,
/// so the dialog and the settings panel cannot disagree about where the next
/// page lands. A protocol **already published** is not moved: it has a page and
/// a link, and the setting is about the next one.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up,
/// [`Invalid`](crate::IpcErrorCode::Invalid) for a target that could not route
/// a write.
#[tauri::command]
pub async fn set_standup_publish_target(
    lifecycle: State<'_, Lifecycle>,
    target: crate::protocol::PublishTarget,
) -> Result<crate::protocol::PublishTarget, IpcError> {
    crate::protocol::set_publish_target(&lifecycle.pool()?, &target).await
}

/// What became of filing a ticket from an action item.
#[derive(Debug, serde::Serialize)]
pub struct ActionItemTicket {
    /// The write queue row carrying the create.
    pub write_id: i64,
    /// The queue's own word for where that write stands (#779): `pending`
    /// while it is on its way, `sent` once it landed, `refused` or `held`
    /// when it will not go on its own, `discarded` once withdrawn.
    pub state: knobas_core::write_queue::WriteState,
    /// The queue's or the source's sentence about the write. Untrusted text.
    pub detail: Option<String>,
    /// The ticket, as an entity id, once the mirror has it.
    ///
    /// `None` for a create that is still queued, and for one that landed at a
    /// source whose re-read has not brought the ticket back yet. Both are "not
    /// yet", and the note is unlinked in both -- there is nothing to link to.
    pub ticket_entity_id: Option<String>,
    /// Whether the note and the ticket are linked.
    pub linked: bool,
}

/// File a ticket from an action item and link it to the protocol (story 69).
///
/// **The existing op, unchanged.** `WriteOp::CreateTicket` is what the ticket
/// is made with and `submit` is what carries it, so this adds no write path --
/// it is the start-work flow's shape in miniature: dispatch an op, find what it
/// made by *reading the mirror*, draw the link.
///
/// Reading the mirror rather than a receipt, because that is the only answer
/// there is: `knobas-source-jira`'s create deliberately drops the key it was
/// given, on the reasoning `WriteReceipt` records -- a created ticket is a
/// mirrored entity, and the mirror is the reading that survives a re-send.
/// `submit` re-syncs the source when the write lands, so the ticket is
/// normally there by the time this looks.
///
/// A ticket that is **not** there yet is not an error and files nothing twice:
/// the write is queued, the caller is told the id is not known, and the link is
/// the one thing left undone. Pressing *Create ticket* again would file a
/// second ticket, which is why the answer says plainly that the create is on
/// the queue.
///
/// `project` is the project's entity id (`jira:PAY`), which is what
/// `WriteOp::CreateTicket` calls its `entity` and what `list_projects` hands
/// the picker.
///
/// # Errors
///
/// [`IpcErrorCode::Invalid`](crate::IpcErrorCode::Invalid) for a blank title
/// or a project that is not an entity id,
/// [`NotFound`](crate::IpcErrorCode::NotFound) if no note carries `note_id`;
/// otherwise whatever the write queue refuses with.
pub async fn create_action_item_ticket_inner(
    state: &crate::sources::SourcesState,
    note_id: &str,
    project: &str,
    ticket_type: &str,
    title: &str,
    body: &str,
) -> Result<ActionItemTicket, IpcError> {
    let pool = &state.pool;
    let (project_ref, payload) =
        action_item_payload(pool, note_id, project, ticket_type, title, body).await?;
    let queued = crate::sources::write_queue::submit(state, payload).await?;
    // The same wait, for the same reason: a created ticket has no address
    // until the mirror has it, and this is the call that goes looking.
    crate::sources::write_queue::sync_after_write(state, &project_ref.namespace).await;
    action_item_ticket_inner(pool, note_id, queued.id).await
}

/// [`create_action_item_ticket_inner`], **answering once the create is
/// queued** (#779): what the `create_action_item_ticket` command does.
///
/// Every refusal the awaited path makes is made before the answer. The answer
/// is the queued write with no ticket yet; the send, the sync that brings the
/// ticket into the mirror and the link follow in a spawned task, and
/// [`action_item_ticket_inner`] -- which the panel asks when the write settles
/// -- names the ticket and draws the link as well, whichever comes first.
///
/// The awaited path stays for the battery.
///
/// # Errors
///
/// As [`create_action_item_ticket_inner`], all of them before anything is sent.
pub async fn create_action_item_ticket_in_background(
    state: &crate::sources::SourcesState,
    note_id: &str,
    project: &str,
    ticket_type: &str,
    title: &str,
    body: &str,
) -> Result<ActionItemTicket, IpcError> {
    let (project_ref, payload) =
        action_item_payload(&state.pool, note_id, project, ticket_type, title, body).await?;
    let (queued, source_id) = crate::sources::write_queue::queue(state, payload).await?;
    let (note, id) = (note_id.to_owned(), queued.id);
    crate::sources::write_queue::send_in_background_then(
        state,
        source_id,
        id,
        project_ref.namespace,
        move |state| async move {
            if let Err(error) = action_item_ticket_inner(&state.pool, &note, id).await {
                tracing::warn!(%error, write_id = id, "an action item's ticket could not be linked yet");
            }
        },
    );
    Ok(ActionItemTicket {
        write_id: queued.id,
        state: queued.state,
        detail: queued.detail,
        ticket_entity_id: None,
        linked: false,
    })
}

/// The checks and the `CreateTicket` payload both paths share.
async fn action_item_payload(
    pool: &PgPool,
    note_id: &str,
    project: &str,
    ticket_type: &str,
    title: &str,
    body: &str,
) -> Result<(EntityRef, serde_json::Value), IpcError> {
    let title = title.trim();
    if title.is_empty() {
        return Err(IpcError::invalid(
            "an action item with no words is not a ticket anybody could act on",
        ));
    }
    let project_ref = EntityRef::parse(project).map_err(IpcError::invalid)?;
    // The note has to be there before the write is sent: the whole point is a
    // ticket linked to a protocol, and discovering the note was deleted after
    // Jira has the ticket leaves a ticket nothing points at.
    get_note_inner(pool, note_id).await?;
    Ok((
        project_ref,
        serde_json::json!({
            "CreateTicket": {
                "entity": project,
                "title": title,
                "body": body,
                "ticket_type": ticket_type,
            }
        }),
    ))
}

/// What became of the ticket an action item's write `write_id` files
/// (#779): where the write stands, and -- once it was sent and the mirror has
/// the ticket -- the ticket, linked to the note.
///
/// **Idempotent, like `protocol::reconcile`**: the panel asks it each time the
/// queue moves until the ticket is named, and the background task behind
/// [`create_action_item_ticket_in_background`] asks it once after its sync.
/// Whichever finds the ticket first draws the link; the other finds it drawn.
///
/// The ticket is looked for **only once the write was sent**: before that, a
/// ticket of the same title in the project is somebody else's, never this one.
///
/// # Errors
///
/// [`NotFound`](crate::IpcErrorCode::NotFound) if no write carries
/// `write_id`; [`Invalid`](crate::IpcErrorCode::Invalid) if it is not a
/// ticket's create; [`Internal`](crate::IpcErrorCode::Internal) for a read or
/// link failure.
pub async fn action_item_ticket_inner(
    pool: &PgPool,
    note_id: &str,
    write_id: i64,
) -> Result<ActionItemTicket, IpcError> {
    let write = knobas_core::write_queue::get(pool, write_id)
        .await?
        .ok_or_else(|| IpcError::not_found(format!("no queued write with id {write_id}")))?;
    let create = write
        .payload
        .get("CreateTicket")
        .filter(|_| write.op == "create_ticket")
        .ok_or_else(|| IpcError::invalid(format!("write {write_id} does not file a ticket")))?;
    let title = create
        .get("title")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let mut ticket_entity_id = None;
    let mut linked = false;
    if write.state == knobas_core::write_queue::WriteState::Sent {
        ticket_entity_id = ticket_titled(pool, &write.source_id, title).await?;
        if let Some(ticket) = ticket_entity_id.as_deref() {
            crate::protocol::draw_link(
                pool,
                note_id,
                ticket,
                crate::protocol::ACTION_ITEM_RELATION,
            )
            .await?;
            linked = true;
        }
    }
    Ok(ActionItemTicket {
        write_id,
        state: write.state,
        detail: write.detail,
        ticket_entity_id,
        linked,
    })
}

/// The newest ticket in a source's corpus with exactly this title.
///
/// The look-back-after-write `start_work::queue`'s `PULL_REQUEST_BY_HEAD` is,
/// and it carries the same warning: it is a source-shaped read outside an
/// adapter and it exists because `Source::write` answers nothing for a create.
/// §4.1 guarantees `title`, which is the only field a freshly created ticket
/// can be recognised by from outside.
///
/// Newest first, so a project that has had this exact summary before answers
/// with the one just made. A title nothing matches is `None`, never a guess.
///
/// **What this cannot do, stated rather than left to be met.** Two action items
/// worded identically, or a project that already holds a ticket with this exact
/// summary, link the protocol to the wrong ticket -- `start_work`'s precedent
/// matched a head branch, and this matches free prose. The fix is not a better
/// statement: it is the key Jira *did* answer with, which
/// `knobas-source-jira`'s create deliberately drops on `WriteReceipt`'s own
/// reasoning ("a created ticket is a mirrored entity"). Undoing that decision
/// is the adapter's to make and the orchestrator's to ratify; until then this
/// is the only answer there is, and the failure direction is a link to a
/// *sibling* ticket rather than to nothing.
///
/// The namespace is matched as a **literal prefix** through
/// `start_work::queue::escape_like`, the one escaping in this crate: `_`
/// matches any character in a `like` pattern and source ids carry them, so an
/// unescaped prefix could find another source's ticket.
async fn ticket_titled(
    pool: &PgPool,
    source_id: &str,
    title: &str,
) -> Result<Option<String>, IpcError> {
    let prefix = format!("{}:%", crate::start_work::queue::escape_like(source_id));
    let found: Option<String> = sqlx::query_scalar(
        "select entity_id from sync.live_item
          where entity_id like $1 and kind = 'ticket' and title = $2
          order by item_updated_at desc nulls last, entity_id
          limit 1",
    )
    .bind(&prefix)
    .bind(title)
    .fetch_optional(pool)
    .await
    .map_err(IpcError::internal)?;
    Ok(found)
}

/// [`create_action_item_ticket_in_background`] behind the app handle:
/// answers once the create is queued (#779).
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) before bring-up;
/// otherwise as [`create_action_item_ticket_inner`].
#[tauri::command]
pub async fn create_action_item_ticket<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    note_id: String,
    project: String,
    ticket_type: String,
    title: String,
    body: String,
) -> Result<ActionItemTicket, IpcError> {
    let state = crate::sources::state(&app)?;
    create_action_item_ticket_in_background(&state, &note_id, &project, &ticket_type, &title, &body)
        .await
}

/// Where an action item's ticket stands: its write, and the ticket once it is
/// filed and mirrored (#779). The panel asks it as the queue moves, to fill in
/// the address of a ticket it drew as on its way.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the
/// database is still coming up; otherwise as [`action_item_ticket_inner`].
#[tauri::command]
pub async fn action_item_ticket(
    lifecycle: State<'_, Lifecycle>,
    note_id: String,
    write_id: i64,
) -> Result<ActionItemTicket, IpcError> {
    action_item_ticket_inner(&lifecycle.pool()?, &note_id, write_id).await
}

// -- Link sets (#866) ----------------------------------------------------------

/// Every link set (#866, R-7.6): [`crate::link_sets::list`].
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is coming
/// up, and whatever [`crate::link_sets::list`] refuses with.
#[tauri::command]
pub async fn link_sets(
    lifecycle: State<'_, Lifecycle>,
) -> Result<Vec<crate::link_sets::LinkSetRow>, IpcError> {
    crate::link_sets::list(&lifecycle.pool()?).await
}

/// One link set and the tickets it shows on (#866, R-7.7):
/// [`crate::link_sets::get`].
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is coming
/// up, and whatever [`crate::link_sets::get`] refuses with.
#[tauri::command]
pub async fn link_set(
    lifecycle: State<'_, Lifecycle>,
    id: String,
) -> Result<crate::link_sets::LinkSetDetail, IpcError> {
    crate::link_sets::get(&lifecycle.pool()?, &id).await
}

/// Save a whole link set (#866): [`crate::link_sets::save`].
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is coming
/// up, and whatever [`crate::link_sets::save`] refuses with.
#[tauri::command]
pub async fn save_link_set(
    lifecycle: State<'_, Lifecycle>,
    draft: crate::link_sets::LinkSetDraft,
) -> Result<crate::link_sets::LinkSetDetail, IpcError> {
    crate::link_sets::save(&lifecycle.pool()?, draft).await
}

/// Delete a link set (#866): [`crate::link_sets::delete`].
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is coming
/// up, and whatever [`crate::link_sets::delete`] refuses with.
#[tauri::command]
pub async fn delete_link_set(lifecycle: State<'_, Lifecycle>, id: String) -> Result<(), IpcError> {
    crate::link_sets::delete(&lifecycle.pool()?, &id).await
}

/// The tickets one rule applies to now (#866, R-7.7):
/// [`crate::link_sets::preview`].
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is coming
/// up, and whatever [`crate::link_sets::preview`] refuses with.
#[tauri::command]
pub async fn preview_link_set_rule(
    lifecycle: State<'_, Lifecycle>,
    rule: crate::link_sets::LinkSetRule,
) -> Result<Vec<crate::link_sets::RuleTicket>, IpcError> {
    crate::link_sets::preview(&lifecycle.pool()?, &rule).await
}

/// What a pattern would match now, for the pattern dialog (#868, R-7.7):
/// [`crate::link_sets::preview_pattern`].
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is coming
/// up, and whatever [`crate::link_sets::preview_pattern`] refuses with.
#[tauri::command]
pub async fn preview_link_set_pattern(
    lifecycle: State<'_, Lifecycle>,
    pattern: crate::link_sets::LinkSetPattern,
) -> Result<crate::link_sets::PatternPreview, IpcError> {
    crate::link_sets::preview_pattern(&lifecycle.pool()?, &pattern).await
}

/// *Hide here* or *Show again* on one ticket (#866, R-7.5):
/// [`crate::link_sets::hide`].
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is coming
/// up, and whatever [`crate::link_sets::hide`] refuses with.
#[tauri::command]
pub async fn hide_link_set_member(
    lifecycle: State<'_, Lifecycle>,
    ticket_id: String,
    member_id: String,
    hidden: bool,
) -> Result<bool, IpcError> {
    crate::link_sets::hide(&lifecycle.pool()?, &ticket_id, &member_id, hidden).await
}

/// What a job shows beside its own links (#867, R-4.4, R-6.2): the link sets
/// that apply, each with its reason and members, and the parent's own links:
/// [`crate::link_sets::on_job`].
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is coming
/// up, and whatever [`crate::link_sets::on_job`] refuses with.
#[tauri::command]
pub async fn job_link_sets(
    lifecycle: State<'_, Lifecycle>,
    entity_id: String,
) -> Result<crate::link_sets::JobLinkSets, IpcError> {
    crate::link_sets::on_job(&lifecycle.pool()?, &entity_id).await
}

/// The Link sets view's hint card (#870, R-7.10): items linked by hand under
/// an epic a set's rule names, offered to that set:
/// [`crate::link_sets::hints`].
///
/// # Errors
///
/// [`NotReady`](crate::IpcErrorCode::NotReady) while the database is coming
/// up, and whatever [`crate::link_sets::hints`] refuses with.
#[tauri::command]
pub async fn link_set_hints(
    lifecycle: State<'_, Lifecycle>,
) -> Result<Vec<crate::link_sets::LinkSetHint>, IpcError> {
    crate::link_sets::hints(&lifecycle.pool()?).await
}

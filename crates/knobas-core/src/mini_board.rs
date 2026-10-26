//! The Tickets tile's **mini board**: a room's live tickets, grouped into the
//! status columns their own sources gave them (ADR-0009 names it; spec #175
//! asks for it; #177 is the one granted read behind it).
//!
//! # What this reads, and what it refuses to
//!
//! It is scoped the way every other tile in a room is, and by the same
//! dimensions the room itself carries (`app/src/lib/shell/contexts.ts`): a
//! **stored** room narrows by its context, a **derived** room -- *All work*,
//! and one per source -- narrows by its sources, and exactly one of that pair
//! is ever narrowing. A board that took only a context would be empty in the
//! room every session starts in. A **project** room (#208, ADR-0010) is a
//! derived room that narrows once more, within its source: see [`read`].
//!
//! Where a context is given, membership is [`crate::context::member_ids`]'
//! answer and nothing else -- the fixed rule of §16.11 / ADR-0008, computed
//! once and reused here rather than re-spelled, so the board can never
//! disagree with the rest of that room about who is in it. Of what the scope
//! admits, the board draws the **live tickets**: liveness
//! ([`crate::live_by_exclusion!`], the view's filter since #778) is what puts a
//! ticket the source deleted off the board while the rule still counts it a
//! member.
//!
//! There is no normalized status model. Contract §4.1 normalizes four fields
//! and a status is not one of them, so a status and a priority can only come
//! out of the [payload][crate], in the source's own shape (the database keeps
//! the status as that read answers it, `sync.item_field`, migration `0062`,
//! #2184, and the board reads it from there where it can) -- which makes both of them **payload reads outside an
//! adapter**, governed by ADR-0007.
//!
//! Since #277 they are **declared** reads: the source says where its status
//! and its priority live ([`crate::payload`]) and this statement resolves the
//! declaration, so a third source's spelling is one entry on that source's
//! descriptor rather than one more `coalesce` arm here. What did not change is
//! either failure direction, and both are still pinned by the tests named on
//! the reads below: a ticket whose declared status resolves to nothing lands
//! in the terminal group, and a card whose priority resolves to nothing simply
//! omits it. A source that declares no status path is one whose tickets are
//! all in the terminal group -- a miss, visibly, never a guess.
//!
//! The project this board narrows by is a third such read, and lives with the
//! census that reports it ([`crate::project`]) rather than being re-spelled
//! here.
//!
//! # Two consumers, one grant
//!
//! [`MiniBoard::columns`] is the tile's board. [`MiniBoard::sources`] is the
//! ticket detail's status select (#179): the statuses each source's own
//! *corpus* shows, which is deliberately wider than the board's columns -- a
//! room with nothing finished still has to be able to offer *Done*. Both come
//! out of this one read. Since #498 the select's offer is the **workflow's**
//! answer (`Source::reachable_transitions`) and this corpus is what it falls
//! back to when that read fails, marked *offer unverified*.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;

use crate::CoreError;
use crate::entity::EntityRef;

/// The statuses the board leads with when the corpus shows them, in the order
/// work moves through them (spec story 3, and the round-3 mockup's four).
///
/// Matched case-insensitively and displayed in the source's own spelling: the
/// list decides *order*, never wording.
const LEADING: [&str; 4] = ["To Do", "In Progress", "In Review", "Done"];

/// Every source's declared ticket status candidates (#277), resolved once a
/// statement (#753): the CTE `status_paths (source_id, segs, ord)`, which
/// [`status_of_ticket!`] reads per row. Through [`crate::declared_string!`]
/// each row walked the whole declaration, about 17 µs a ticket on the 15,000
/// of an All room.
///
/// The candidates are [`crate::declared_candidates!`]'s own, over a relation
/// aliased `i` that carries each declared source and the kind `ticket` -- the
/// two columns that macro reads -- so the declaration is walked as it always
/// was, once a source instead of once a row.
macro_rules! status_paths {
    ($decl:literal) => {
        concat!(
            "status_paths as materialized (",
            $crate::declared_ticket_paths!($decl, "status_name"),
            ")"
        )
    };
}

/// After `from sync.item i`: the ticket's stored status row as `sst`, which
/// [`status_of_ticket!`] reads ([`crate::stored_join!`], #2184).
macro_rules! status_join {
    ($decl:literal) => {
        $crate::stored_join!("sst", $decl, "status_name")
    };
}

/// The lane a ticket stands in (#939), read once a row like its status:
/// the field `$lane` names (`epic`, `assignee`, `priority`, `issuetype`,
/// `label`), and `''` where the ticket has none of it or `$lane` is `null`.
///
/// The same reads the card's own fields are, so a card stands in the lane
/// its fields say: the assignee, priority and type through `lane_paths`
/// ([`lane_paths!`]), resolved once a source as the status is; the first
/// label through `label_paths`, in the order [`crate::declared_list!`]
/// yields the card's labels; the epic through [`crate::path_line_read!`].
/// The assignee is read off `sync.item_field` where the database stored it
/// ([`crate::stored_string!`], #2184), as the status is.
macro_rules! lane_of_ticket {
    ($decl:literal, $lane:literal) => {
        concat!(
            "case ",
            $lane,
            "::text when 'epic' then coalesce(",
            $crate::path_line_read!($decl),
            ", '')
               when 'label' then coalesce((select nullif(btrim(e.elem #>> c.entry_segs), '')
                 from label_paths c cross join lateral jsonb_array_elements(
                   case when jsonb_typeof(i.payload #> c.at_segs) = 'array'
                        then i.payload #> c.at_segs else '[]'::jsonb end)
                   with ordinality as e(elem, pos)
                where c.source_id = i.source_id
                  and jsonb_typeof(e.elem #> c.entry_segs) = 'string'
                  and nullif(btrim(e.elem #>> c.entry_segs), '') is not null
                order by c.ord, e.pos limit 1), '')
               when 'assignee' then coalesce(",
            $crate::stored_string!($decl, "assignee", lane_string!()),
            ", '') when 'priority' then coalesce(",
            lane_string!(),
            ", '') when 'issuetype' then coalesce(",
            lane_string!(),
            ", '') else '' end"
        )
    };
}

/// A ticket's lane field, when it is a string, through `lane_paths`.
macro_rules! lane_string {
    () => {
        $crate::first_string_of!(
            "(select p.segs, p.ord from lane_paths p where p.source_id = i.source_id)"
        )
    };
}

/// The candidates of the field `$lane` names, resolved once a source: the
/// CTEs `lane_paths (source_id, segs, ord)` for a string field and
/// `label_paths (source_id, at_segs, entry_segs, ord)` for the labels.
/// Each branch is guarded by `$lane`, so only the one asked for is read.
macro_rules! lane_paths {
    ($decl:literal, $lane:literal) => {
        concat!(
            "lane_paths as materialized (
               select * from (",
            $crate::declared_ticket_paths!($decl, "assignee"),
            ") a where ",
            $lane,
            "::text = 'assignee'
               union all select * from (",
            $crate::declared_ticket_paths!($decl, "priority"),
            ") p where ",
            $lane,
            "::text = 'priority'
               union all select * from (",
            $crate::declared_ticket_paths!($decl, "issue_type"),
            ") t where ",
            $lane,
            "::text = 'issuetype'),
             label_paths as materialized (
               select i.source_id, c.at_segs, c.entry_segs, c.ord from ",
            $crate::declared_tickets!($decl),
            " i cross join lateral (select ",
            $crate::declared_segments!("cand->'at'"),
            " as at_segs, ",
            $crate::declared_segments!("cand->'entry'"),
            " as entry_segs, ord from jsonb_array_elements(",
            $crate::declared_array!($decl, "labels"),
            ") with ordinality as cands(cand, ord)) c
                where ",
            $lane,
            "::text = 'label')"
        )
    };
}

/// A ticket's status, at the path its own source declares (#277), through
/// `status_paths` ([`status_paths!`]): [`crate::declared_string!`]'s rule,
/// [`crate::first_string_of!`] being the one spelling both share. Read off
/// `sync.item_field` where the database stored it for the declarations the
/// statement binds, joined as `sst` ([`status_join!`], #2184): on 2,500 Jira
/// tickets the walk cost about 20 µs a ticket.
///
/// **Failure direction (ADR-0007 requirement 3, unchanged by the growth): a
/// miss is visible, never a guess.** A ticket whose source declares no status
/// path, or whose record does not carry the declared one, lands in the board's
/// terminal group ([`MiniBoardColumn::status`] `= None`) instead of being
/// dropped or sorted into a column somebody inferred. Pinned by
/// `a_ticket_with_no_recognizable_status_lands_in_the_terminal_group` in
/// `knobas-core/tests/it/mini_board.rs`.
macro_rules! status_of_ticket {
    () => {
        $crate::stored_or!(
            "sst",
            $crate::first_string_of!(
                "(select p.segs, p.ord from status_paths p where p.source_id = i.source_id)"
            )
        )
    };
}

/// A ticket's priority, at the path its own source declares.
///
/// **Failure direction: a miss renders nothing.** A card whose priority
/// resolves to nothing simply omits it, which is the whole of what story 8
/// asks for -- a board that never guesses at what a source did not say. Pinned
/// by `a_ticket_with_no_recognizable_priority_carries_none` in
/// `knobas-core/tests/it/mini_board.rs`.
macro_rules! priority_read {
    ($decl:literal) => {
        $crate::declared_string!($decl, "priority")
    };
}

/// The board's cards: the live tickets the room's scope admits.
///
/// Both narrowings are **nullable parameters**, the discipline
/// `commands::entity`'s room statements record: the SQL stays static and a
/// scope that does not narrow binds `null` rather than growing a branch. The
/// difference between `null` and an empty list is load-bearing on `$1` --
/// `Some` of no members is a stored room that is honestly empty, `None` is a
/// derived room that never asked about membership at all.
///
/// `$1` is tested as a set (`in (select unnest($1))`), which PostgreSQL runs
/// as one hashed subplan probed per ticket (#1149). As `= any($1)` it did on
/// analyzed tables too, but never analyzed the planner read the tickets
/// through `item_pkey`, one index search per id: the scan took 32 ms for the
/// 5,000 ids `has:blocker` matches on the 22k probe, and 11 ms hashed.
///
/// Live tickets only, which is what keeps a tombstoned ticket -- and, since
/// migration `0012`, a disabled source's ticket -- off the board (story 18):
/// `sync.item` with [`crate::live_by_exclusion!`] rather than `sync.live_item`,
/// whose join the planner drove from `knobas.entity` on tables never analyzed,
/// 225 ms for the *All* room's 15,000 tickets against 70 ms analyzed (#778).
/// Newest first within the answer, so a column reads like the recency list
/// the tile used to be; the grouping below preserves this order.
///
/// `$3` is the project dimension (#208), which narrows **within** `$2` rather
/// than beside it: a project key is unique only inside its own source, so a
/// project room names both and a board that honoured only the key would draw
/// two sources' `PAY` as one project. A record with no readable project misses
/// the predicate rather than matching some other room's -- absence, never a
/// wrong room, which is [`crate::project`]'s stated failure direction reaching
/// the board.
///
/// `$5` is the per-column cap (#753), `null` for none: each column keeps its
/// newest `$5` cards, and every row carries `total`, how many its column holds
/// before the cut. The status is read once a row (`materialized`) and the
/// priority, the type, the assignee, the path line (#810), the labels and
/// the signals (#813, #815) only for the rows kept: on a room of 15,000 tickets the uncapped board was 2 MB on the
/// bridge and a card per ticket in the webview.
///
/// The parent's title (#894) is one primary-key probe per kept card. The
/// payload macros all read the alias `i`, so the probe rebinds `i` to the
/// candidate parent item, and the lateral `par` carries the card's own
/// `source_id` out beside the parent's kind and name, where the probe can
/// still see it.
///
/// `$6` lanes the board (#939, #1098): the field a lane shares, `null` for
/// none. Each row's lane is read with its status ([`lane_of_ticket!`]), and
/// the cap and the count are then per lane and status, so a lane's column
/// is capped and counted as an unlaned column is. Lanes rank biggest first,
/// a tie to the lane with the newer ticket, then by key; `$7` keeps the
/// first `$7` lanes' rows (`null` for all), and every row carries how many
/// lanes there are and, laned, every status's count over the whole board
/// (`column_totals`), which the lanes not kept still stand in.
const CARDS: &str = concat!(
    "with ",
    status_paths!("$4"),
    ", ",
    lane_paths!("$4", "$6"),
    ",
     scoped as materialized (
       select i.entity_id, coalesce(i.item_updated_at, i.synced_at) as at, ",
    status_of_ticket!(),
    " as status, ",
    lane_of_ticket!("$4", "$6"),
    " as lane
       from sync.item i",
    status_join!("$4"),
    "
      where i.kind = 'ticket'
        and ",
    crate::live_by_exclusion!("i"),
    "
        and ($1::text[] is null or i.entity_id in (select unnest($1::text[])))
        and ($2::text[] is null or i.source_id = any($2))
        and ($3::text is null or ",
    crate::project_key_read!("$4"),
    " = $3)
     ),
     lanes as (
       select s.lane, count(*) as n,
              row_number() over (order by count(*) desc, max(s.at) desc, s.lane) as place,
              count(*) over () as lanes
         from scoped s group by s.lane
     ),
     ranked as (
       select s.entity_id, s.at, s.status, s.lane, l.n as lane_count, l.place, l.lanes,
              row_number() over (partition by s.lane, s.status
                                 order by s.at desc, s.entity_id) as rn,
              count(*) over (partition by s.lane, s.status) as total
         from scoped s join lanes l on l.lane = s.lane
        where $7::bigint is null or l.place <= $7
     )
     select i.entity_id, i.source_id, i.title, r.status, r.lane, r.lane_count, r.place,
            r.lanes, case when $6::text is null then null else
              (select jsonb_agg(jsonb_build_array(c.status, c.n))
                 from (select s.status, count(*) as n from scoped s group by s.status) c)
            end as column_totals, ",
    priority_read!("$4"),
    " as priority, ",
    crate::declared_string!("$4", "issue_type"),
    " as issue_type, ",
    crate::declared_string!("$4", "assignee"),
    " as assignee, ",
    crate::path_line_read!("$4"),
    " as path, array(select l.value from ",
    crate::declared_list!("$4", "labels"),
    " l) as labels, ",
    crate::declared_integer!("$4", "comment_count"),
    " as comment_count, (select i.title from sync.item i
         where par.name is not null
           and i.entity_id = par.source_id || ':' || par.name
           and i.source_id = par.source_id and i.kind = par.kind
           and ",
    crate::live_by_exclusion!("i"),
    "
           and par.name in (select n.name from ",
    crate::declared_names_as_parent!("$4"),
    " n)) as parent_title, i.item_updated_at as updated_at, r.total
       from ranked r
       join sync.item i on i.entity_id = r.entity_id
       left join lateral (select pn.kind, pn.name, i.source_id from ",
    crate::declared_parent_names!("$4"),
    " pn order by pn.pos desc, pn.name_ord limit 1) par on true
      where $5::bigint is null or r.rn <= $5
      order by r.at desc, r.entity_id"
);

/// The signals of the cards a board keeps (#815): [`crate::signals`]' reader
/// over the kept ids (`$3`), one statement for the whole board, the same
/// reader `has:` asks. `$1` the declarations, `$2` the configured
/// usernames as configured, folded here by the server's `lower()` as the
/// reviewers they meet are and as `has:` folds them (#1975); Rust's
/// `to_lowercase` differs from it on `İ`.
const SIGNALS: &str = concat!(
    "select s.tid, s.kind, s.text, s.tone from ",
    crate::ticket_signals!(
        "$1",
        "array(select lower(u) from unnest($2::text[]) u)",
        "(select unnest($3::text[]) as tid)",
        "null"
    ),
    " s order by s.tid, s.ord, s.text"
);

/// The board's signals statement, for `knobas-app`'s plan test. It binds
/// the declarations, the usernames and the ticket ids.
#[doc(hidden)]
#[must_use]
pub fn planned_signals() -> &'static str {
    SIGNALS
}

/// The board's card statement, for `knobas-app`'s plan test
/// (`tests/it/read_plans.rs`, #778). It binds what [`read_capped`] binds.
#[doc(hidden)]
#[must_use]
pub fn planned_statement() -> &'static str {
    CARDS
}

/// The board's status statement, for `knobas-app`'s plan test. It binds
/// the source ids and the declarations.
#[doc(hidden)]
#[must_use]
pub fn planned_observed_statuses() -> &'static str {
    OBSERVED_STATUSES
}

/// Every status these sources' live ticket corpora show -- the select's
/// offer (#179), not the room's columns.
///
/// Scoped to the sources the board actually drew a card from: the board
/// answers for the tickets on it, and a source nothing in this room came from
/// is not one the detail here can be asked about.
///
/// Live by [`crate::live_by_exclusion!`] over `sync.item`, as [`CARDS`] is,
/// rather than through `sync.live_item` (#1149): on tables never analyzed the
/// view's join probed `knobas.entity` once per ticket of the sources, 15,000
/// probes and about 90 ms of the 22k probe's narrowed board, against about
/// 25 ms with the tombstones read once.
const OBSERVED_STATUSES: &str = concat!(
    "with ",
    status_paths!("$2"),
    ",
     observed as materialized (
       select i.source_id, ",
    status_of_ticket!(),
    " as status
         from sync.item i",
    status_join!("$2"),
    "
        where i.kind = 'ticket'
          and i.source_id = any($1)
          and ",
    crate::live_by_exclusion!("i"),
    "
     )
     select distinct source_id, status
       from observed
      where status is not null"
);

/// One room's tickets as the mini board draws them.
///
/// Issue #177.
#[derive(Clone, Debug, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct MiniBoard {
    /// The observed status columns, in display order: the four of [`LEADING`]
    /// first where the corpus shows them, every other observed status after
    /// them alphabetically, and the terminal group last.
    pub columns: Vec<MiniBoardColumn>,
    /// Per source that put a card on this board, the statuses its own corpus
    /// shows -- what the ticket detail's status select falls back to when the
    /// workflow read fails (#179, and #498 for why it is the fallback rather
    /// than the offer).
    pub sources: Vec<SourceStatuses>,
    /// The board in lanes (#939), when it was asked for in lanes
    /// ([`Scope::lanes`]); empty otherwise. Biggest first. Each lane names
    /// its cards by id; the cards themselves are in [`Self::columns`], once.
    pub lanes: Vec<MiniBoardLane>,
    /// How many lanes there are past [`Self::lanes`], which a read that
    /// asked for its first lanes ([`Lanes::first`]) did not send.
    pub more_lanes: u32,
}

/// What a board's lanes split by (R-3.5): the Tickets view's *Lanes*
/// switch, spelled as a ticket view stores it.
///
/// - `epic`: The epic: the ticket's path line, as the card draws it.
/// - `issuetype`: The ticket's type.
/// - `label`: The ticket's first label.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
#[ts(export_to = "entity.ts")]
pub enum LaneField {
    Epic,
    Assignee,
    Priority,
    #[serde(rename = "issuetype")]
    IssueType,
    Label,
}

impl LaneField {
    /// The spelling the board's statement takes (`$6`).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            LaneField::Epic => "epic",
            LaneField::Assignee => "assignee",
            LaneField::Priority => "priority",
            LaneField::IssueType => "issuetype",
            LaneField::Label => "label",
        }
    }
}

/// A board read in lanes (#939): by what, and how many of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lanes {
    pub by: LaneField,
    /// Only the first this many lanes, biggest first; `None` for every lane.
    pub first: Option<u32>,
}

/// One lane of a laned board: the value its tickets share, how many they
/// are, and its own columns, each capped and counted as a board's column is.
///
/// Issue #939.
#[derive(Clone, Debug, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct MiniBoardLane {
    /// What its tickets share, in the source's words; `""` for the lane of
    /// tickets with none of it.
    pub key: String,
    /// How many tickets stand in it.
    pub count: u32,
    /// Its statuses, in the board's column order; only those it has.
    pub columns: Vec<LaneColumn>,
}

/// One status of a lane: its cards by id, newest first, and its count.
///
/// Issue #939.
#[derive(Clone, Debug, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct LaneColumn {
    /// As [`MiniBoardColumn::status`].
    ///
    /// `None` is the terminal group.
    pub status: Option<String>,
    /// The ids of the cards kept, each in [`MiniBoard::columns`].
    ///
    /// Newest first, or in the saved view's order.
    pub ids: Vec<String>,
    /// How many of the lane's tickets stand in this status, kept or not.
    pub total: u32,
}

/// One column: a status, and the cards standing in it (#177).
///
/// The count the column header shows is [`total`](Self::total) (#753): on a
/// capped read `cards` holds only the newest, so its length is not the count.
#[derive(Clone, Debug, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct MiniBoardColumn {
    /// The status, in the source's own words -- or `None` for the terminal
    /// group: tickets whose mirrored record carries no status this read
    /// recognizes. The words on screen for that group are the shell's ("No
    /// status"); what crosses the bridge is the absence itself.
    ///
    /// On a saved view's board (a Jira view's, #877) it is the view
    /// column's **name**, which gathers several statuses: read a ticket's
    /// status from [`MiniBoardCard::status`], never from here (#1423).
    pub status: Option<String>,
    /// Newest first: all of them, or the newest of them where the read was
    /// capped ([`read_capped`]).
    pub cards: Vec<MiniBoardCard>,
    /// How many tickets stand in this column, `cards` or not (#753). Equal to
    /// `cards.len()` on an uncapped read; the header draws this one.
    pub total: u32,
}

/// One ticket on the board.
///
/// Issue #177.
#[derive(Clone, Debug, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct MiniBoardCard {
    /// The ticket's stable in-app address, which is what a click opens.
    pub entity_id: String,
    /// Which source this ticket came from -- the same source whose statuses
    /// [`MiniBoard::sources`] lists.
    pub source_id: String,
    /// The source's own key for it (`PAY-231`), i.e. `entity_id` past its
    /// namespace.
    pub key: String,
    pub title: String,
    /// The ticket's own status, in the source's words (#1423): the status
    /// it stands in on [`read_scoped`]'s board, carried on the card because
    /// a saved view's column gathers several statuses under the view's name
    /// ([`MiniBoardColumn::status`]). `None` for the terminal group.
    pub status: Option<String>,
    /// `None` where the record carries no readable priority; see
    /// [`priority_read!`].
    ///
    /// The card omits it rather than guessing (ADR-0007).
    pub priority: Option<String>,
    /// The ticket's type in its source's words (`Story`, `Bug`), at the
    /// declared `issue_type` path (#810); `None` on a miss, drawn as nothing.
    pub issue_type: Option<String>,
    /// The account it is assigned to, at the declared `assignee` path, in the
    /// spelling the source uses for an account; `None` for an unassigned
    /// ticket and on a miss.
    pub assignee: Option<String>,
    /// When the source last changed it, or `None` where it never said.
    ///
    /// RFC 3339 on the wire.
    pub updated_at: Option<DateTime<Utc>>,
    /// The ticket's declared parent chain as one line (its epic, for Jira):
    /// `knobas_core::path_line_read!`, the line every other row draws.
    ///
    /// `None` where no parent is declared or it misses.
    pub path: Option<String>,
    /// The title of the item its direct parent names (#894): the epic's
    /// name beside [`Self::path`]'s key, `PAY-200 Payout reliability`. The
    /// parent's last declared position, its first name; the mirrored live
    /// item of the parent's kind at `<source>:<name>` in the same source,
    /// and only if that item answers to the name as its declarations say.
    /// `None` for a ticket with no parent, an epic the mirror does not hold,
    /// and an item that does not answer to the name (ADR-0007).
    pub parent_title: Option<String>,
    /// Whether [`Self::assignee`] is one of the configured usernames
    /// ([`Scope::identity`]), ignoring case. `false` when no identity was
    /// given ([`read`], [`read_capped`]); the `mini_board` command always
    /// gives it, and the rooms' tile simply does not draw it.
    pub mine: bool,
    /// The labels it carries, at the declared `labels` path (#811's field),
    /// in the record's order; empty on a miss. The Tickets view's *Label*
    /// lanes read the first (#813, R-3.5).
    pub labels: Vec<String>,
    /// How many comments it has, at the declared `comment_count` path
    /// (#894): a whole JSON number, `Some(0)` when the source said none, and
    /// `None` on a miss. The card draws "N comments" above zero only.
    pub comment_count: Option<i64>,
    /// The signals around it (#815, R-3.8), in the order a card draws them:
    /// [`crate::signals`]' reader, the one `has:` asks too.
    pub signals: Vec<crate::signals::Signal>,
}

/// The statuses one source's live ticket corpus shows, in the board's order.
///
/// Issue #177.
#[derive(Clone, Debug, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct SourceStatuses {
    pub source_id: String,
    /// Never carries the terminal group: "no status" is something a ticket can
    /// be *in*, not something it can be moved to.
    pub statuses: Vec<String>,
}

/// One row of [`CARDS`].
#[derive(sqlx::FromRow)]
struct CardRow {
    entity_id: String,
    source_id: String,
    title: String,
    status: Option<String>,
    priority: Option<String>,
    issue_type: Option<String>,
    assignee: Option<String>,
    path: Option<String>,
    labels: Vec<String>,
    comment_count: Option<i64>,
    parent_title: Option<String>,
    updated_at: Option<DateTime<Utc>>,
    total: i64,
    lane: String,
    lane_count: i64,
    place: i64,
    lanes: i64,
    column_totals: Option<sqlx::types::Json<ColumnTotals>>,
}

/// Every status a laned board counts, and how many stand in it.
type ColumnTotals = Vec<(Option<String>, i64)>;

/// A count from the database, as the answer's `u32`.
fn as_u32(n: i64) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// Which band of the board a column belongs to. **Declaration order is the
/// board's order** -- that is what the derived `Ord` means here, so a fourth
/// band would be placed by where it is written.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum Band {
    /// One of [`LEADING`], however the source spelled it.
    Leading,
    /// Any other status the corpus showed.
    Observed,
    /// The terminal group: no status this read could recognize.
    Terminal,
}

/// Where a column sits in the board's order.
///
/// Four parts, each breaking the tie the one before it leaves: the band, the
/// position within [`LEADING`], the status case-folded, and the status as
/// spelled. The last is only ever reached by two sources spelling one status in
/// different cases, and exists so the answer does not depend on which of them
/// was written first.
fn column_rank(status: Option<&str>) -> (Band, usize, String, String) {
    let Some(status) = status else {
        return (Band::Terminal, 0, String::new(), String::new());
    };
    let folded = status.to_lowercase();
    match LEADING
        .iter()
        .position(|leading| leading.eq_ignore_ascii_case(status))
    {
        Some(at) => (Band::Leading, at, folded, status.to_owned()),
        None => (Band::Observed, 0, folded, status.to_owned()),
    }
}

/// The mini board for one room.
///
/// `ctx_id` is a **stored** room's context, whose members are the fixed
/// one-hop rule's; `sources` is a **derived** room's source list, empty for
/// *All work*. Exactly one of the two ever narrows, which is the invariant the
/// switcher builds its rooms with; a caller that passed both would get their
/// intersection, which is a coherent answer to an incoherent room.
///
/// `project` is the third dimension (#208) and is not one of that pair: it
/// narrows **within** `sources` rather than instead of them, because a project
/// key is unique only inside its own source. `None` is unscoped, the same
/// "empty means unfiltered" the room filter's other fields have.
///
/// An unknown or empty context answers with an empty board rather than an
/// error, for the reason [`crate::context::member_ids`] does: an address can
/// outlive the thing it names, and "nothing here" is what the tile needs to be
/// able to say. A project no record names answers the same way, for the same
/// reason.
///
/// # Errors
///
/// [`CoreError::Db`] if a query fails.
pub async fn read(
    pool: &PgPool,
    ctx_id: Option<&str>,
    sources: &[String],
    project: Option<&str>,
    declarations: &crate::payload::Declarations,
) -> Result<MiniBoard, CoreError> {
    read_capped(pool, ctx_id, sources, project, declarations, None).await
}

/// [`read`], with each column cut to its newest `per_column` cards (#753).
///
/// What the Tickets tile draws: a column header says how many tickets stand
/// in it ([`MiniBoardColumn::total`]) and the column scrolls through the
/// newest of them. The statuses offered ([`MiniBoard::sources`]) are the
/// sources' whole corpora either way. `None` is [`read`].
///
/// # Errors
///
/// [`CoreError::Db`] if a query fails.
pub async fn read_capped(
    pool: &PgPool,
    ctx_id: Option<&str>,
    sources: &[String],
    project: Option<&str>,
    declarations: &crate::payload::Declarations,
    per_column: Option<u32>,
) -> Result<MiniBoard, CoreError> {
    let scope = Scope {
        ctx_id,
        sources,
        project,
        ..Scope::default()
    };
    read_scoped(pool, scope, declarations, per_column).await
}

/// Which tickets a board draws, and whose they are (#810).
///
/// The room's three dimensions ([`read`]), plus two the Tickets view of the
/// ticket-first shell adds: `only`, the tickets a search matched, and
/// `identity`, the usernames that make a card [`MiniBoardCard::mine`].
#[derive(Clone, Copy, Debug, Default)]
pub struct Scope<'a> {
    /// A stored room's context, or `None`.
    pub ctx_id: Option<&'a str>,
    /// A derived room's sources; empty for every source.
    pub sources: &'a [String],
    /// A project key, narrowing within `sources`.
    pub project: Option<&'a str>,
    /// Only these entity ids, or `None` for no such narrowing. `Some` of an
    /// empty list is a search that matched nothing, and draws nothing.
    pub only: Option<&'a [String]>,
    /// The configured usernames, which [`MiniBoardCard::mine`] is matched
    /// against ignoring case.
    pub identity: &'a [String],
    /// Lanes (#939): `per_column` then caps each lane's column, and
    /// [`MiniBoard::lanes`] says which cards stand in which lane.
    pub lanes: Option<Lanes>,
}

/// [`read_capped`] over a [`Scope`]: the room's dimensions and the Tickets
/// view's two (#810). Every narrowing intersects: a context and `only` both
/// given draw the tickets in both.
///
/// # Errors
///
/// [`CoreError::Db`] if a query fails.
pub async fn read_scoped(
    pool: &PgPool,
    scope: Scope<'_>,
    declarations: &crate::payload::Declarations,
    per_column: Option<u32>,
) -> Result<MiniBoard, CoreError> {
    let Scope {
        ctx_id,
        sources,
        project,
        only,
        identity,
        lanes,
    } = scope;
    let mut members = match ctx_id {
        Some(ctx) => Some(crate::context::member_ids(pool, ctx).await?),
        None => None,
    };
    if let Some(only) = only {
        members = Some(match members {
            Some(members) => {
                let only: std::collections::HashSet<&String> = only.iter().collect();
                members.into_iter().filter(|id| only.contains(id)).collect()
            }
            None => only.to_vec(),
        });
    }
    let scope = (!sources.is_empty()).then(|| sources.to_vec());
    let declared = declarations.as_param();
    // Planned for its parameters every time (#753): reused, the statement
    // went to PostgreSQL's generic plan on its sixth execution and an All room
    // of 15,000 tickets took 6 s where its own plan takes 0.1 s.
    let rows: Vec<CardRow> = sqlx::query_as(CARDS)
        .persistent(false)
        .bind(&members)
        .bind(&scope)
        .bind(project)
        .bind(&declared)
        .bind(per_column.map(i64::from))
        .bind(lanes.map(|lanes| lanes.by.as_str()))
        .bind(
            lanes
                .and_then(|lanes| lanes.first)
                .map(|first| i64::from(first.max(1))),
        )
        .fetch_all(pool)
        .await?;
    let kept: Vec<String> = rows.iter().map(|row| row.entity_id.clone()).collect();
    let mut signals = signals_of(pool, &kept, &declared, identity).await?;

    // Grouped in the order the rows arrived, so the newest-first ordering the
    // statement asks for survives into each column; the columns themselves are
    // put in the board's order afterwards. Laned, every status the board
    // counts is a column, whether or not a kept lane stands in it.
    let mut at: HashMap<Option<String>, usize> = HashMap::new();
    let mut columns: Vec<MiniBoardColumn> = Vec::new();
    let laned = lanes.is_some();
    if laned && let Some(totals) = rows.first().and_then(|row| row.column_totals.as_ref()) {
        for (status, n) in totals.iter() {
            at.insert(status.clone(), columns.len());
            columns.push(MiniBoardColumn {
                status: status.clone(),
                cards: Vec::new(),
                total: as_u32(*n),
            });
        }
    }
    let mut by_lane: BTreeMap<i64, MiniBoardLane> = BTreeMap::new();
    let more_lanes = rows.first().map_or(0, |row| row.lanes);
    let mut sources: BTreeSet<String> = BTreeSet::new();
    for row in rows {
        sources.insert(row.source_id.clone());
        if laned {
            let lane = by_lane.entry(row.place).or_insert_with(|| MiniBoardLane {
                key: row.lane.clone(),
                count: as_u32(row.lane_count),
                columns: Vec::new(),
            });
            match lane
                .columns
                .iter_mut()
                .find(|cell| cell.status == row.status)
            {
                Some(cell) => cell.ids.push(row.entity_id.clone()),
                None => lane.columns.push(LaneColumn {
                    status: row.status.clone(),
                    ids: vec![row.entity_id.clone()],
                    total: as_u32(row.total),
                }),
            }
        }
        let index = *at.entry(row.status.clone()).or_insert_with(|| {
            columns.push(MiniBoardColumn {
                status: row.status.clone(),
                cards: Vec::new(),
                total: as_u32(row.total),
            });
            columns.len() - 1
        });
        columns[index].cards.push(MiniBoardCard {
            // The sink refuses an item whose id does not round-trip through
            // `EntityRef` (`knobas_sync`'s `check`), so this parses for
            // everything the mirror holds; an id that somehow did not is
            // shown whole rather than not at all.
            key: EntityRef::parse(&row.entity_id)
                .map_or_else(|_| row.entity_id.clone(), |entity| entity.key),
            signals: signals.remove(&row.entity_id).unwrap_or_default(),
            entity_id: row.entity_id,
            source_id: row.source_id,
            title: row.title,
            status: row.status,
            priority: row.priority,
            issue_type: row.issue_type,
            mine: row.assignee.as_deref().is_some_and(|assignee| {
                identity.iter().any(|me| me.eq_ignore_ascii_case(assignee))
            }),
            assignee: row.assignee,
            updated_at: row.updated_at,
            path: row.path,
            parent_title: row.parent_title,
            labels: row.labels,
            comment_count: row.comment_count,
        });
    }
    columns.sort_by_key(|column| column_rank(column.status.as_deref()));
    let lanes: Vec<MiniBoardLane> = by_lane
        .into_values()
        .map(|mut lane| {
            lane.columns
                .sort_by_key(|cell| column_rank(cell.status.as_deref()));
            lane
        })
        .collect();
    let more_lanes = if laned {
        as_u32(more_lanes).saturating_sub(u32::try_from(lanes.len()).unwrap_or(u32::MAX))
    } else {
        0
    };

    Ok(MiniBoard {
        sources: observed_statuses(pool, &sources, declarations).await?,
        columns,
        lanes,
        more_lanes,
    })
}

/// The live tickets among `$1`, each with its source, its status as
/// [`CARDS`] reads it, its lane by the field `$3` names as [`CARDS`] reads
/// it (`''` for none, or when `$3` is `null`), and when the source last
/// changed it, newest first as [`CARDS`] orders them. Only the status
/// (stored where it can be, [`status_of_ticket!`]) and the lane are read,
/// which is what lets a saved view's board choose the cards it draws before
/// it reads them ([`statuses_of`]).
const STATUSES: &str = concat!(
    "with ",
    status_paths!("$2"),
    ", ",
    lane_paths!("$2", "$3"),
    "
     select i.entity_id, i.source_id, ",
    status_of_ticket!(),
    " as status, ",
    lane_of_ticket!("$2", "$3"),
    " as lane, i.item_updated_at
       from sync.item i",
    status_join!("$2"),
    "
      where i.kind = 'ticket'
        and ",
    crate::live_by_exclusion!("i"),
    "
        and i.entity_id in (select unnest($1::text[]))
      order by coalesce(i.item_updated_at, i.synced_at) desc, i.entity_id"
);

/// One ticket of [`Statuses`]: what a board places it by before its card is
/// read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placed {
    pub entity_id: String,
    /// Its lane, as [`MiniBoardCard::lane`] reads it; `""` when the board is
    /// not laned.
    pub lane: String,
    /// As [`MiniBoardCard::updated_at`].
    pub updated_at: Option<DateTime<Utc>>,
}

/// The statuses of the live tickets among `only`, grouped as
/// [`read_scoped`] groups its columns: one per status, in the board's order,
/// each column's tickets newest first. `sources` are the sources those
/// tickets come from, the ones [`MiniBoard::sources`] would answer for.
#[derive(Clone, Debug, Default)]
pub struct Statuses {
    /// Per status (`None` for none), its tickets, newest first.
    pub columns: Vec<(Option<String>, Vec<Placed>)>,
    /// The source ids those tickets come from.
    pub sources: BTreeSet<String>,
}

/// [`Statuses`] of `only`: what a board of exactly those tickets would put in
/// each column, and in which lane by `lane` (`None`: not laned), without
/// reading the cards (Björn, 2026-10-26; #2183). A saved view's board cuts
/// its columns and its lanes in the source's order and then reads only the
/// cards it kept through [`read_scoped`].
///
/// # Errors
///
/// [`CoreError::Db`] if the query fails.
pub async fn statuses_of(
    pool: &PgPool,
    only: &[String],
    declarations: &crate::payload::Declarations,
    lane: Option<LaneField>,
) -> Result<Statuses, CoreError> {
    type Row = (String, String, Option<String>, String, Option<DateTime<Utc>>);
    let rows: Vec<Row> = sqlx::query_as(STATUSES)
        .persistent(false)
        .bind(only)
        .bind(declarations.as_param())
        .bind(lane.map(LaneField::as_str))
        .fetch_all(pool)
        .await?;
    let mut at: HashMap<Option<String>, usize> = HashMap::new();
    let mut out = Statuses::default();
    for (entity_id, source, status, lane, updated_at) in rows {
        out.sources.insert(source);
        let index = *at.entry(status.clone()).or_insert_with(|| {
            out.columns.push((status, Vec::new()));
            out.columns.len() - 1
        });
        out.columns[index].1.push(Placed {
            entity_id,
            lane,
            updated_at,
        });
    }
    out.columns
        .sort_by_key(|(status, _)| column_rank(status.as_deref()));
    Ok(out)
}

impl MiniBoardCard {
    /// The lane this card stands in when a board is laned `by` (R-3.5): the
    /// field as the card carries it, `""` for none. [`CARDS`]' `lane` is
    /// the same read, made before the card is.
    #[must_use]
    pub fn lane(&self, by: LaneField) -> &str {
        let value = match by {
            LaneField::Epic => self.path.as_deref(),
            LaneField::Assignee => self.assignee.as_deref(),
            LaneField::Priority => self.priority.as_deref(),
            LaneField::IssueType => self.issue_type.as_deref(),
            LaneField::Label => self.labels.first().map(String::as_str),
        };
        value.unwrap_or("")
    }
}

/// A whole board put in lanes after it was read (#939), with the answer
/// [`read_scoped`] gives in lanes; a Jira view's board lays its tickets in
/// lanes the same way before it reads them ([`lanes_of`], #2183). Each
/// lane's column keeps its first `per_column` cards in
/// the column's order and counts them all; lanes rank biggest first, a tie
/// to the lane with the newer ticket, then by key; only the first
/// `lanes.first` are kept. The columns keep their totals and the kept cards.
#[must_use]
pub fn in_lanes(mut board: MiniBoard, lanes: Lanes, per_column: Option<u32>) -> MiniBoard {
    let (kept, more_lanes) = lanes_of(
        board
            .columns
            .iter()
            .map(|column| (&column.status, column.cards.as_slice())),
        |card| (card.entity_id.as_str(), card.lane(lanes.by), card.updated_at),
        lanes,
        per_column,
    );
    let ids = in_a_lane(&kept);
    for column in &mut board.columns {
        column
            .cards
            .retain(|card| ids.contains(card.entity_id.as_str()));
    }
    board.more_lanes = more_lanes;
    board.lanes = kept;
    board
}

/// The ids of the cards `lanes` keep.
#[must_use]
pub fn in_a_lane(lanes: &[MiniBoardLane]) -> std::collections::HashSet<&str> {
    lanes
        .iter()
        .flat_map(|lane| lane.columns.iter().flat_map(|cell| &cell.ids))
        .map(String::as_str)
        .collect()
}

/// [`in_lanes`]' lanes over any tickets, each read as its id, its lane and
/// when the source last changed it: so a board can lay its tickets in lanes
/// before it reads their cards (#2183). Answers the lanes kept and how many
/// lanes are past them.
pub fn lanes_of<'a, T: 'a>(
    columns: impl IntoIterator<Item = (&'a Option<String>, &'a [T])>,
    read: impl Fn(&'a T) -> (&'a str, &'a str, Option<DateTime<Utc>>),
    lanes: Lanes,
    per_column: Option<u32>,
) -> (Vec<MiniBoardLane>, u32) {
    struct Gathered {
        count: u32,
        newest: Option<DateTime<Utc>>,
        cells: Vec<LaneColumn>,
    }
    let mut gathered: BTreeMap<String, Gathered> = BTreeMap::new();
    for (status, tickets) in columns {
        for ticket in tickets {
            let (id, key, updated_at) = read(ticket);
            let lane = gathered
                .entry(key.to_owned())
                .or_insert_with(|| Gathered {
                    count: 0,
                    newest: None,
                    cells: Vec::new(),
                });
            lane.count += 1;
            lane.newest = lane.newest.max(updated_at);
            if lane
                .cells
                .last()
                .is_none_or(|cell| &cell.status != status)
            {
                lane.cells.push(LaneColumn {
                    status: status.clone(),
                    ids: Vec::new(),
                    total: 0,
                });
            }
            let cell = lane.cells.last_mut().expect("pushed above");
            cell.total += 1;
            if per_column.is_none_or(|cap| cell.ids.len() < cap as usize) {
                cell.ids.push(id.to_owned());
            }
        }
    }
    let mut ranked: Vec<(String, Gathered)> = gathered.into_iter().collect();
    ranked.sort_by(|(a_key, a), (b_key, b)| {
        b.count
            .cmp(&a.count)
            .then(b.newest.cmp(&a.newest))
            .then(a_key.cmp(b_key))
    });
    let total = ranked.len();
    ranked.truncate(lanes.first.map_or(total, |first| first.max(1) as usize));
    let more = u32::try_from(total - ranked.len()).unwrap_or(u32::MAX);
    let kept = ranked
        .into_iter()
        .map(|(key, lane)| MiniBoardLane {
            key,
            count: lane.count,
            columns: lane.cells,
        })
        .collect();
    (kept, more)
}

/// The signals of `tickets`, by ticket, each list in the order a card draws
/// it: [`SIGNALS`].
async fn signals_of(
    pool: &PgPool,
    tickets: &[String],
    declared: &sqlx::types::Json<serde_json::Value>,
    me: &[String],
) -> Result<HashMap<String, Vec<crate::signals::Signal>>, CoreError> {
    let mut out: HashMap<String, Vec<crate::signals::Signal>> = HashMap::new();
    if tickets.is_empty() {
        return Ok(out);
    }
    let rows: Vec<(String, String, String, String)> = sqlx::query_as(SIGNALS)
        .persistent(false)
        .bind(declared)
        .bind(me)
        .bind(tickets)
        .fetch_all(pool)
        .await?;
    for (tid, kind, text, tone) in rows {
        // The statement writes only the four kinds and their tones, so a
        // row that does not decode is a bug here; it is left out rather
        // than failing the board.
        let (Some(kind), Ok(tone)) = (
            crate::signals::SignalKind::from_spelling(&kind),
            serde_json::from_value::<crate::tone::Tone>(serde_json::Value::String(tone)),
        ) else {
            continue;
        };
        out.entry(tid)
            .or_default()
            .push(crate::signals::Signal { kind, text, tone });
    }
    Ok(out)
}

/// The second half of the grant: what each of these sources' corpora show,
/// as [`MiniBoard::sources`] answers it.
///
/// # Errors
///
/// [`CoreError::Db`] if the query fails.
pub async fn observed_statuses(
    pool: &PgPool,
    sources: &BTreeSet<String>,
    declarations: &crate::payload::Declarations,
) -> Result<Vec<SourceStatuses>, CoreError> {
    if sources.is_empty() {
        return Ok(Vec::new());
    }
    let ids: Vec<String> = sources.iter().cloned().collect();
    let rows: Vec<(String, String)> = sqlx::query_as(OBSERVED_STATUSES)
        .persistent(false)
        .bind(&ids)
        .bind(declarations.as_param())
        .fetch_all(pool)
        .await?;

    let mut by_source: BTreeMap<String, Vec<String>> =
        ids.into_iter().map(|id| (id, Vec::new())).collect();
    for (source_id, status) in rows {
        by_source.entry(source_id).or_default().push(status);
    }
    Ok(by_source
        .into_iter()
        .map(|(source_id, mut statuses)| {
            statuses.sort_by_key(|status| column_rank(Some(status)));
            SourceStatuses {
                source_id,
                statuses,
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The order the board promises, over a set deliberately shuffled: the
    /// four lead in their own order rather than alphabetically, strays follow
    /// alphabetically rather than in the order they were met, and the terminal
    /// group is last however the rest sorted.
    #[test]
    fn the_four_lead_then_the_rest_alphabetically_then_the_terminal_group() {
        let mut observed = vec![
            None,
            Some("Done"),
            Some("blocked"),
            Some("To Do"),
            Some("Awaiting deploy"),
            Some("In Review"),
            Some("In Progress"),
        ];
        observed.sort_by_key(|status| column_rank(*status));
        assert_eq!(
            observed,
            vec![
                Some("To Do"),
                Some("In Progress"),
                Some("In Review"),
                Some("Done"),
                Some("Awaiting deploy"),
                Some("blocked"),
                None,
            ]
        );
    }

    /// A source shouting its statuses still gets the board's order: the list
    /// decides where a column sits, never how it is spelled.
    #[test]
    fn a_leading_status_leads_whatever_case_it_is_spelled_in() {
        let mut observed = vec![Some("Archived"), Some("DONE"), Some("to do")];
        observed.sort_by_key(|status| column_rank(*status));
        assert_eq!(
            observed,
            vec![Some("to do"), Some("DONE"), Some("Archived")]
        );
    }

    fn card(key: &str, assignee: Option<&str>) -> MiniBoardCard {
        MiniBoardCard {
            entity_id: format!("jira:{key}"),
            source_id: "jira".to_owned(),
            key: key.to_owned(),
            title: key.to_owned(),
            status: None,
            priority: None,
            issue_type: None,
            assignee: assignee.map(str::to_owned),
            updated_at: None,
            path: None,
            parent_title: None,
            mine: false,
            labels: Vec::new(),
            comment_count: None,
            signals: Vec::new(),
        }
    }

    /// A board put in lanes after it is read (#939, a Jira view's): each
    /// lane's column keeps its first cards in the column's order and counts
    /// them all, lanes rank biggest first then by key, only the first are
    /// kept, and the columns keep their totals and only the kept cards.
    #[test]
    fn a_whole_board_put_in_lanes_caps_each_lanes_column_and_keeps_the_first_lanes() {
        let board = MiniBoard {
            columns: vec![
                MiniBoardColumn {
                    status: Some("To Do".to_owned()),
                    cards: vec![
                        card("PAY-3", Some("mara")),
                        card("PAY-1", Some("mara")),
                        card("PAY-2", Some("mara")),
                        card("PAY-5", Some("omar")),
                    ],
                    total: 4,
                },
                MiniBoardColumn {
                    status: None,
                    cards: vec![card("PAY-9", None), card("PAY-8", Some("omar"))],
                    total: 2,
                },
            ],
            sources: Vec::new(),
            lanes: Vec::new(),
            more_lanes: 0,
        };
        let by = LaneField::Assignee;
        let laned = in_lanes(board, Lanes { by, first: Some(2) }, Some(2));
        type Cell = (Option<String>, Vec<String>, u32);
        let shape: Vec<(String, u32, Vec<Cell>)> = laned
            .lanes
            .iter()
            .map(|lane| {
                let cells = lane
                    .columns
                    .iter()
                    .map(|cell| (cell.status.clone(), cell.ids.clone(), cell.total))
                    .collect();
                (lane.key.clone(), lane.count, cells)
            })
            .collect();
        let ids = |keys: &[&str]| keys.iter().map(|key| format!("jira:{key}")).collect();
        assert_eq!(
            shape,
            vec![
                (
                    "mara".to_owned(),
                    3,
                    vec![(Some("To Do".to_owned()), ids(&["PAY-3", "PAY-1"]), 3)]
                ),
                (
                    "omar".to_owned(),
                    2,
                    vec![
                        (Some("To Do".to_owned()), ids(&["PAY-5"]), 1),
                        (None, ids(&["PAY-8"]), 1)
                    ]
                ),
            ]
        );
        assert_eq!(
            laned.more_lanes, 1,
            "the unassigned lane is counted, not sent"
        );
        let kept: Vec<(u32, Vec<&str>)> = laned
            .columns
            .iter()
            .map(|c| {
                (
                    c.total,
                    c.cards.iter().map(|card| card.key.as_str()).collect(),
                )
            })
            .collect();
        assert_eq!(
            kept,
            vec![(4, vec!["PAY-3", "PAY-1", "PAY-5"]), (2, vec!["PAY-8"])]
        );
        assert_eq!(laned.columns[1].cards[0].lane(by), "omar");
    }
}

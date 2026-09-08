//! The built-in smart lists: what the launcher offers before anything is typed.
//!
//! # Scope, recorded rather than hidden
//!
//! Spec §4's catalogue -- *My tickets with a failing build · PRs waiting on me
//! · PRs idle > 5 days · Blocked tickets · Unlogged time this week · Assets
//! with open alerts* -- is written for a knobas that has **links** (M2),
//! **per-kind status** (M2's normalization work), **time** (M3) and **assets**
//! (M4). M1 has a corpus of `entity_id, kind, source_id, title, body_text,
//! author, item_updated_at, synced_at, payload` and nothing else, and
//! `knobas.link` is empty by construction. Shipping "My tickets with a failing
//! build" over that would mean inventing a per-adapter payload table, which
//! spec §3a forbids in the same breath (*"Search does not know adapter names --
//! it knows `sync.item`"*).
//!
//! So M1 ships **the registry plus four lists that are true over the generic
//! corpus**, and the catalogue lands in M2 when its inputs exist.
//!
//! # The list that was measured out, and what it costs to bring back
//!
//! A fifth list shipped in review round 1 and does not ship now: `cross-key`,
//! *"tickets another system mentions by key"* -- spec §4's *"a query JQL cannot
//! express"*, attempted without `knobas.link` (which is M2). Its predicate was
//! a correlated `exists` probing the FTS index once per ticket in a 14-day
//! window, evaluated on the `⌘K` path because the board asks every list for a
//! count.
//!
//! ## The measurement, and the trap under it
//!
//! **Read the method before the numbers.** `ANALYZE` is *not* enough to
//! benchmark a GIN index. `fastupdate` parks freshly inserted entries in an
//! unsorted **pending list** that every index scan reads linearly until a
//! `VACUUM` merges it, and `ANALYZE` does not flush it. Round 1 was measured
//! `ANALYZE`d but never `VACUUM`ed, and every probe it timed was re-scanning
//! that list.
//!
//! Measured across five fixtures, best of three, warm. The `after VACUUM`
//! column is the real cost; the `ANALYZE only` column is what the same
//! statement reports when the pending list has not been merged. **The harness
//! column is load-bearing, and it is why this table can be trusted**: two of
//! these rows were mislabelled `fresh` until the column existed and someone
//! went to check them.
//!
//! | mirror rows | tickets in window | harness | `ANALYZE` only | after `VACUUM` | ratio |
//! |---|---|---|---|---|---|
//! | 12,800 | 3,200 | first step (= fresh) | 1,652 ms | **9.1 ms** | 182× |
//! | 12,800 | 3,200 | first step (= fresh) | 6,327 ms | **52 ms** | 121× |
//! | 48,000 | 12,000 | cumulative | 3,374 ms | **52.3 ms** | 64.5× |
//! | 48,000 | 12,000 | cumulative | 14,443 ms | **115 ms** | 126× |
//! | 100,000 | 25,000 | cumulative | 2,863 ms | **133.2 ms** | 21.5× |
//! | 100,000 | 25,000 | cumulative | 176 ms | **174 ms** | **1.0×** |
//! | 100,000 | 25,000 | fresh single load | 5,722 ms | **97.4 ms** | 58.8× |
//! | 200,000 | ~40,320 | cumulative | 11,607 ms | **237.4 ms** | 48.9× |
//! | 300,000 | ~60,480 | cumulative | 17,460 ms | **514.7 ms** | 33.9× |
//! | 200,000 | ~20,160 | fresh single load | 2,170 ms | **89.6 ms** | 24.2× |
//! | 300,000 | ~20,160 | fresh single load | 7,807 ms | **90.2 ms** | 86.5× |
//!
//! That `1.0×` is the sharpest disagreement in the table and it is kept
//! deliberately, because reconciling it away would delete the finding. Three
//! runs at exactly 100,000 rows report 21.5×, 58.8× and 1.0×.
//!
//! **The last two rows are an artifact curve, not a cost curve.** Those
//! fixtures put only `g <= 20160` inside the 14-day window, so both candidate
//! sets are capped at ~20,160 however large the mirror grows -- which is why
//! both vacuumed readings sit at ~90 ms while the corpus gains half again as
//! many rows. Read them for the `ratio` column and nothing else. The same
//! arithmetic corrects the two cumulative rows above them, whose windows are
//! ~40,320 and ~60,480 rather than the 50,000 and 75,000 an earlier draft
//! assumed.
//!
//! Two things in that table are worth more than the list it is about.
//!
//! **The artifact is governed by the increment since the last merge, not by
//! corpus size.** Crossing `gin_pending_list_limit` (4 MB) makes Postgres merge
//! most of the pending list unasked; what survives is the residue accumulated
//! since, bounded by that limit rather than by how many rows the table holds.
//! So the ratio tracks *where the load happened to stop*, and **neither curve
//! is monotonic**: fresh runs 182× / 121× -> 58.8× -> 24.2× -> **86.5×**;
//! cumulative runs 64.5× -> 21.5× and 1.0× -> 48.9× -> 33.9×. It never reaches
//! zero, and **no size and no harness can be trusted clean**.
//!
//! An earlier draft said severity *declines* with corpus size. That was never
//! supported: fresh loads had only been sampled at the small end, and the two
//! large "fresh" readings that appeared to continue the trend were cumulative
//! rows wearing the wrong label. The mechanism stated just above already
//! contradicted the trend -- if the residue is bounded by a byte limit rather
//! than by corpus size, size should not govern the ratio at all -- so this
//! correction is the doc finally agreeing with itself.
//!
//! **Autovacuum does not rescue an unvacuumed GIN benchmark.** Measured as a
//! 2×2 on one fixture in one session, `TRUNCATE` + `VACUUM FULL` between
//! conditions, at 100,000 rows:
//!
//! | harness | autovacuum | `ANALYZE` only | after `VACUUM` | ratio |
//! |---|---|---|---|---|
//! | fresh single load | on | 5,733 ms | 96.9 ms | 59.2× |
//! | fresh single load | off | 5,708 ms | 98.0 ms | 58.3× |
//! | cumulative | on | 2,861 ms | 135.5 ms | 21.1× |
//! | cumulative | off | 2,857 ms | 135.0 ms | 21.2× |
//!
//! The autovacuum setting moves **nothing**; the harness shape moves
//! everything. So explicit vacuuming is not merely the safer rule, it is the
//! only one that works -- an earlier draft of this section guessed that a run
//! measuring 1.0 had been "rescued by autovacuum", and that guess is
//! falsified above.
//!
//! So the practical rule has **no size exemption and no harness exemption** --
//! vacuum before every timing. A benchmark that vacuums nothing publishes a set
//! of numbers each wrong by a different and unpredictable factor, with nothing
//! on the face of them to say which is worst; and the temptation to trust the
//! largest fixture because it looks saner is precisely the trap. The largest
//! fresh reading in the table is the **worst** one, at 86.5×.
//!
//! **Task 10 completed this model, and the missing factor is the scan count.**
//! [`crate::testing`] measured the penalty directly out of `pgstatginindex`
//! rather than from a timing: it is paid **once per index scan**, at roughly
//! `pending_pages x 0.3-0.5 us` each, over four probe counts and four pending
//! list sizes. That is why the table above -- a correlated `exists`, one scan
//! per ticket in the window -- shows 21x to 182x, while `Searcher::search`,
//! which makes **one** scan, measures 1.0-1.1x on the same dirty index. The
//! increment governs the size of the penalty; the statement's shape governs how
//! many times it is paid. Neither reading was wrong.
//!
//! An earlier draft said the artifact "has vanished by 100,000 (ratio 1.0)".
//! That was one cell of one curve promoted to a property of size, and it did
//! not reproduce. Its cause is worth naming because it is the same rule applied
//! to a harness: that curve was **cumulative** -- each size seeded on top of the
//! last, with a `VACUUM` between steps -- so its 100,000-row `ANALYZE only`
//! figure was taken against an index vacuumed one step earlier, with only the
//! increment pending. The numerators show it directly: 2,861 ms cumulative
//! against 5,733 ms fresh, almost exactly half.
//!
//! **Harness shape accounts for about two thirds of the divergence, not all of
//! it.** The numerator gap is 2× but the ratio gap is ~2.8×; the rest is the
//! denominator. A cumulative fixture re-seeds overlapping key ranges, so ticket
//! keys repeat and the honest vacuumed cost rises too -- 135 ms against 97 ms
//! for the same nominal corpus. Two variables, one number, which is the reason
//! the table names its harness.
//!
//! **Corrected, the cost is linear and the per-probe cost is flat.** Eleven
//! measurements across five corpus sizes, two harness shapes and both
//! autovacuum settings: 16.3 → 9.6 → 6.9 µs per probe on one cumulative curve;
//! 2.8 → 4.4 → 5.3 → 5.9 → 8.5 µs on the other; 3.9 → 4.4 → 4.5 µs on the
//! fresh loads. One drifts down, two drift mildly up, and the whole spread is
//! 2.8–16.3 µs across a corpus that grows 23×. That is fixture noise around a
//! flat line; nothing in any of them is quadratic. Round 1's
//! conclusion that this was *quadratic* -- and that no cap could rescue it --
//! was an artifact of the unvacuumed index and is dead twice over. M2 should
//! not read this section as "the text-probe approach is hopeless". It is not
//! hopeless; it is merely worse than the link-table form.
//!
//! ## Why it was still right to remove it
//!
//! The case needs no false exponent:
//!
//! * It was the only list that could not be expressed as a `count(*) filter`
//!   over the single scan. Removing it let the `bounded:` arm leave the
//!   `builtins!` macro entirely, so the one-pass rule below is now
//!   **structurally impossible to violate** rather than merely stated.
//! * Even corrected it costs an order of magnitude more than all four
//!   remaining lists put together, and it spends that on the `⌘K` path.
//! * At a 100k corpus with a pessimistic quarter of the mirror in the window it
//!   is **60–175 ms on its own**, and both endpoints are demonstrated: 57 ms on
//!   a sparse-text fixture, 89.6 / 90.2 / 96.9 / 97.4 / 133.2 / 135.5 ms on
//!   richer ones, 174 ms on the richest. Eight measurements, five fixtures, and
//!   the variable that moves them is how much text the mirror holds -- the one thing a fixture cannot
//!   honestly pin. That is at or over the whole board's budget for one list,
//!   and which side of it you land on is a property of the corpus rather than
//!   of the code.
//! * The link table makes the same question **exact and O(1)** in M2, so the
//!   text probe is a stopgap with a known replacement rather than the design.
//!
//! ## What M2 restores
//!
//! With links populated the predicate stops being a text probe and becomes an
//! index probe:
//!
//! ```sql
//! count(*) filter (where i.kind = 'ticket'
//!                    and exists (select 1 from knobas.confirmed_link l
//!                                 where l.from_id = i.entity_id))
//! ```
//!
//! **`knobas.confirmed_link`, and not the table under it.** Migration `0007`
//! (issue #41) put a second population in `knobas.link`: a row whose
//! `confirmed_at` is `NULL` is a **proposal**, a machine-made guess sitting in
//! the suggestion tray that nobody has accepted. A predicate over the base
//! table counts those, so this list would quietly fill with pairs the user
//! never agreed to -- #41's own stakes were *"the links panel showing an
//! unconfirmed guess would be a correctness bug, not a cosmetic one"*, and a
//! smart list is the same bug on another surface. The two views `0007` creates
//! are each other's negation over the same rows, so choosing one is where the
//! question "confirmed, or proposed?" gets asked and the answer cannot drift;
//! `knobas_core::link::entries_of` reads `knobas.confirmed_link` and
//! `knobas_core::suggest::proposals` reads `knobas.proposed_link` for exactly
//! that reason. `knobas-core/tests/link_reads.rs` is what keeps this paragraph
//! from going stale a second time.
//!
//! The old `and l.deleted_at is null` is gone from the predicate because it is
//! inside the view, along with `confirmed_at is not null`.
//!
//! `link_from_idx` still makes that O(1) per ticket -- it is partial on exactly
//! the `deleted_at is null` the view asks for, and `confirmed_at` is rechecked
//! on the few rows it hands back -- so the list collapses into the single scan
//! like every other one, which is why that is now a rule.
//!
//! # Why they are constants and not a little query language
//!
//! Every list is a hand-written `&'static str`, which is what makes this module
//! satisfy the runtime-SQL rule (roadmap §4 gotcha 2) *natively*: nothing here
//! is assembled at run time, so the assert-safe wrapper never appears and
//! `tests/sql_containment.rs` has nothing to catch. A typo in any one list's
//! SQL is caught by `tests/lists.rs`, which runs every entry of [`BUILTINS`]
//! rather than a representative one.
//!
//! # The two statements
//!
//! [`SUMMARY_SQL`] answers the whole launcher board in **one round trip**: the
//! counts (a `count(*) filter` per list, over one scan of `sync.live_item` for
//! a mirror list and one scan of `knobas.asset` for an estate list), the newest
//! stamp in each list (what the change badge compares against), and the
//! per-list "last opened" stamps out of `knobas.setting`. The identity behind
//! `@me` is the second query, and it is [`crate::vocab::Vocabulary::load`]'s --
//! the same one every search already makes.
//!
//! # Two scans, still one round trip (#504)
//!
//! Until #504 the sentence above read *"there is no exception to that single
//! pass, and after the removal above there is no way to write one: the
//! `builtins!` macro takes `scan:` entries and nothing else"*. It is no longer
//! true as written, and the rule it was protecting is: **a list is a
//! `count(*) filter` over one scan of the corpus it draws from, and the whole
//! board is still one round trip.**
//!
//! What changed is that there is now a second corpus. #504's three lists are
//! about the **estate** -- *Not monitored*, *Open alerts in my contexts*,
//! *Certificates expiring* -- and `knobas.asset` is not in `sync.live_item` and
//! never will be: an asset is a thing knobas owns, not a thing it mirrors
//! ([`crate::corpus::ASSET`] says the same about search). A predicate over the
//! mirror cannot answer any of the three, so [`SUMMARY_SQL`] grew a **second
//! CTE** -- `estate`, one scan of `knobas.asset` -- beside `scan`, and the
//! final `select` reads both. Two **scans**, one round trip, and the
//! `builtins!` macro still admits nothing else: a `scan:` entry is a filter
//! over the mirror scan and an `asset:` entry is a filter over the estate
//! scan, and there is no third arm.
//!
//! *Two scans, not two tables.* The aggregates run over two relations; the
//! **predicates** reach further -- `knobas.confirmed_link`, `knobas.entity`,
//! `knobas.monitor_alert`, `sync.live_item` again, and `knobas.context`
//! through the membership walk. What the rule fixes is what a `count(*)` is
//! counting, which is one row per thing the list is a list of. See the section
//! below for what those probes cost and what was measured.
//!
//! ## These three are correlated probes, which is the shape that was removed
//!
//! Said plainly, because the section above it is about a list that was taken
//! out for being one. Two of the three predicates are a **correlated
//! subquery per asset row** -- an open alert on this asset's monitors, a
//! certificate on this asset's monitors -- which is `cross-key`'s shape and
//! not the mirror lists'. What makes them affordable is not the shape; it is
//! the **cardinality of the thing being scanned**. `cross-key` probed a GIN
//! index once per ticket in a 14-day window of a mirror that grows to 100,000
//! rows; these probe an index once per **asset**, and the estate is the
//! smallest table knobas has -- an installation's machines, not its tickets,
//! which is migration `0019`'s own sentence.
//!
//! **Measured, not assumed** (#504, PG 18.6, `explain (analyze, verbose)` over
//! [`SUMMARY_SQL`] on a two-asset fixture): execution 0.58 ms, planning
//! 2.72 ms -- planning is the larger half at this size, and the statement is
//! prepared once per connection. The outer `knobas.asset` scan is a `Seq Scan`
//! and every probe under it is an `Index Scan` (`item_kind_updated_idx`,
//! `link_pair_active_idx`, `entity_pkey`). What the fixture is too small to
//! show is how the per-asset probes scale, and the honest statement of the
//! bound is the shape: **one probe per asset per predicate**, on a table whose
//! size is an estate.
//!
//! The context-membership walk is the one part that is **not** per-asset.
//! `a.id in `[`held_by_any_context!`] binds nothing from the outer row, so the
//! planner turns it into a `hashed SubPlan` -- the recursive walk runs once and
//! is then a hash probe per asset. It appears **twice** in the plan and not
//! once, because the planner inlines the estate CTE's inner select and so
//! evaluates the predicate separately for the count and for the badge stamp's
//! `filter`. Two walks per launcher board, not one per asset.
//!
//! [`held_by_any_context!`]: knobas_core::held_by_any_context
//!
//! # The change badge
//!
//! One `knobas.setting` row under `search.smart_list_seen`, holding
//! `{"<id>": "<rfc3339>"}` -- one row rather than a row per list, because it is
//! one read on the launcher's hot path and it is read *with* the counts. A list
//! is `changed` when the newest sync stamp in it is newer than the last time it
//! was opened, and when it holds something and has never been opened at all. A
//! list with nothing in it is never badged: a badge on an empty list is noise.

use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};

use crate::SearchError;
use crate::group::RawHit;

/// One built-in smart list.
///
/// The SQL fields are private: a list is something this module ships, not
/// something a caller composes.
#[derive(Debug)]
pub struct BuiltinList {
    /// Stable id -- what `list:<id>` names and what the seen-stamp is keyed on.
    pub id: &'static str,
    pub label: &'static str,
    /// Static description. [`SmartListSummary::description`] may replace it
    /// with the reason the list is empty: that field is display text, and a
    /// list that explains its own zero beats one that just reads 0.
    pub blurb: &'static str,
    /// Column of [`SUMMARY_SQL`] carrying this list's count. `<column>_at`
    /// carries the newest sync stamp in it and `<column>_seen` the last time
    /// it was opened.
    column: &'static str,
    /// The full statement for the rows. `$1` = identity usernames,
    /// `$2` = limit.
    rows_sql: &'static str,
}

impl BuiltinList {
    /// Whether this list means anything without a configured account.
    ///
    /// Read **off the statement** rather than declared beside it: a list needs
    /// an identity exactly when its SQL reads the identity parameter, so a
    /// declaration and a predicate cannot drift apart.
    #[must_use]
    pub fn needs_identity(&self) -> bool {
        self.rows_sql.contains("$1")
    }
}

/// One entry of the launcher board's smart-list rail (interfaces §2.4).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SmartListSummary {
    pub id: String,
    pub label: String,
    pub count: i64,
    /// Something in this list is newer than the last time it was opened.
    pub changed: bool,
    /// Display text: the list's blurb, or the reason it is empty.
    pub description: String,
}

/// The statement one list's rows come from.
///
/// `matched` is the whole list, `totals` counts it per kind and `picked` is the
/// page -- the same `totals LEFT JOIN page` shape [`crate::sql`] builds, for
/// the same reason: `ResultGroup::total` has to mean "how many of this kind are
/// in the list", not "how many fitted on the page". `rank` and `snippet` are
/// carried through `picked` rather than added at the end so that a kind the
/// limit cut comes back with them **null**, exactly like a search does.
macro_rules! rows_over {
    ($pred:literal) => {
        concat!(
            "with matched as (\n",
            "  select i.entity_id, i.kind, i.source_id, i.title, i.item_updated_at,\n",
            "         i.synced_at, ",
            knobas_core::ancestor_path_read!("i.payload"),
            " as path,\n",
            "         0::real as rank, null::text as snippet\n",
            "    from sync.live_item i\n",
            "   where ",
            $pred,
            "\n),\n",
            "totals as (select kind, count(*) as kind_total from matched group by kind),\n",
            "picked as (\n",
            "  select * from matched\n",
            "   order by item_updated_at desc nulls last, entity_id\n",
            "   limit $2\n",
            ")\n",
            "select t.kind as group_kind, t.kind_total as kind_total,\n",
            "       p.entity_id as entity_id, p.source_id as source_id, p.title as title,\n",
            "       p.item_updated_at as updated_at, p.synced_at as synced_at,\n",
            "       p.path as path,\n",
            "       p.rank as rank, p.snippet as snippet\n",
            "  from totals t\n",
            "  left join picked p on p.kind = t.kind\n",
            " order by t.kind, p.item_updated_at desc nulls last, p.entity_id\n"
        )
    };
}

/// The rows of one **estate** list -- [`rows_over!`]'s shape over
/// `knobas.asset` (#504).
///
/// A separate template rather than a parameter on [`rows_over!`], because none
/// of the six expressions the two share is the same expression: an asset has no
/// `kind` column and no `source_id` column (both are the constants
/// [`crate::corpus::ASSET`] already uses), its path is a **column** the store
/// maintains rather than an `ancestor_path_read!` over a payload (the second of
/// the two cases [`crate::corpus::Corpus::path`] names), and it has one stamp
/// where a mirrored row has two. A macro with a branch per corpus would be the
/// same two templates with a conditional over them.
///
/// **The order is the estate's own, and it is not [`rows_over!`]'s.** A mirror
/// list is newest-first because a mirrored row's `item_updated_at` is when the
/// source last moved it, and that is the reader's question of the mirror. An
/// asset's `updated_at` is when somebody last *edited the row* -- renamed it,
/// moved it, set a property -- which is not what puts it in any of these three
/// lists and would sort *Not monitored* by how recently each gap was typed. So
/// these rows come back where the estate keeps them: by path, then by name,
/// then by id, which is `knobas_app::assets`' `UNMONITORED` order and the
/// inbox's pick order, and which is total -- `id` breaks the tie two siblings
/// of the same name would leave.
macro_rules! rows_over_estate {
    ($pred:expr) => {
        concat!(
            "with matched as (\n",
            "  select a.id as entity_id, 'asset'::text as kind,\n",
            "         'asset'::text as source_id, a.name as title,\n",
            "         a.updated_at as item_updated_at, a.updated_at as synced_at,\n",
            "         nullif(a.path_text, '') as path,\n",
            "         0::real as rank, null::text as snippet\n",
            "    from knobas.asset a\n",
            "   where ",
            $pred,
            "\n),\n",
            "totals as (select kind, count(*) as kind_total from matched group by kind),\n",
            "picked as (\n",
            "  select * from matched\n",
            "   order by coalesce(path, '') asc, title asc, entity_id asc\n",
            "   limit $2\n",
            ")\n",
            "select t.kind as group_kind, t.kind_total as kind_total,\n",
            "       p.entity_id as entity_id, p.source_id as source_id, p.title as title,\n",
            "       p.item_updated_at as updated_at, p.synced_at as synced_at,\n",
            "       p.path as path,\n",
            "       p.rank as rank, p.snippet as snippet\n",
            "  from totals t\n",
            "  left join picked p on p.kind = t.kind\n",
            " order by t.kind, coalesce(p.path, '') asc, p.title asc, p.entity_id asc\n"
        )
    };
}

/// When the asset row itself was last touched -- *Not monitored*'s badge stamp.
///
/// The honest one for that list: what puts an asset on it is the asset
/// existing with nothing attached, so what is *new* about the list is a new
/// asset (or one renamed or moved into view). There is no other event; a gap
/// does not have a timestamp of its own.
macro_rules! asset_touched_at {
    () => {
        "a.updated_at"
    };
}

/// Every asset with no confirmed `monitored-by` link to a monitor (#504).
///
/// **This is `knobas_app::assets`' `UNMONITORED` predicate, in SQL, and the two
/// have to stay one rule.** That read is the Monitors tab's *Not monitored*
/// roster and this is the same roster in the launcher; a reader who saw 16 in
/// one place and 17 in the other would be right to distrust both. Neither
/// compiler will say when they stop agreeing, so
/// [`the_roster_rule_is_spelled_the_way_the_link_table_spells_it`] pins the two
/// words this statement and that one both hard-code, and
/// `crates/knobas-app/tests/search_ipc.rs`'
/// `the_launchers_not_monitored_list_is_the_monitors_tabs_roster` runs both
/// reads over one estate and compares the answers.
///
/// Everything the roster's own doc argues holds here unchanged and is not
/// re-argued: `not exists` rather than a join because the question is about a
/// set; **undirected**, because `0011` made the pair unordered; `m.kind =
/// 'monitor'` so a `monitored-by` link to a ticket is not an attachment;
/// `knobas.confirmed_link`, so a *proposal* nobody has accepted leaves the
/// asset on the list, which is the answer the reader wants. `m.id <> a.id` is
/// carried over the same way and is belt-and-braces here too -- the kind guard
/// already refuses an asset's own entity row.
///
/// The relation and the kind are **literals**, where the roster binds them,
/// because `concat!` takes literals and nothing else. That is the one
/// difference between the two statements and it is the reason for the pin
/// below.
///
/// [`the_roster_rule_is_spelled_the_way_the_link_table_spells_it`]: tests::the_roster_rule_is_spelled_the_way_the_link_table_spells_it
macro_rules! not_monitored_pred {
    () => {
        "not exists (select 1
                        from knobas.confirmed_link l
                        join knobas.entity m
                          on m.id in (l.from_id, l.to_id) and m.id <> a.id
                       where l.relation = 'monitored-by'
                         and a.id in (l.from_id, l.to_id)
                         and m.kind = 'monitor')"
    };
}

/// The join from a monitor to **the asset it watches**, written once.
///
/// Two of the three estate predicates make it -- an open alert's monitor, an
/// expiring certificate's monitor -- and it is four clauses that have to agree
/// exactly: **undirected** (`0011` made the pair unordered, so which end a link
/// was written from is not a fact any read may depend on), the relation, and
/// the *other* end of the link being the asset the outer scan is standing on.
/// Two copies of it would be two answers to "what does this monitor watch"
/// the day one was amended, which is the failure the whole registry is written
/// against.
///
/// `$monitor` is how the caller's own relation spells the monitor's entity id.
/// It binds `l` as the link alias and reads `a.id` from the outer scan, so a
/// caller supplies the monitor side and nothing else.
///
/// The clauses sit in the `join ... on` rather than in the caller's `where`
/// because they are what makes the join the join; an inner join reads the same
/// either way, and this leaves each caller a `where` that is only its own
/// question.
macro_rules! monitors_asset {
    ($monitor:literal) => {
        concat!(
            "join knobas.confirmed_link l
                          on (l.from_id = ",
            $monitor,
            " or l.to_id = ",
            $monitor,
            ")
                         and l.relation = 'monitored-by'
                         and case when l.from_id = ",
            $monitor,
            "
                                  then l.to_id else l.from_id end = a.id"
        )
    };
}

/// When the newest **open** alert on the monitors watching this asset opened,
/// or `null` if none is open.
///
/// Written once and read twice: it is *Open alerts in my contexts*' badge
/// stamp, and `is not null` is the alert half of that list's predicate. That is
/// deliberate -- an `exists` beside a `max` would be two spellings of "does
/// this asset have an open alert", and the day one grew a clause the count and
/// the badge would answer different questions about the same list.
///
/// The join is [`monitors_asset!`], which is the inbox's. Unlike the inbox this
/// does **not** pick one asset per alert -- the inbox draws one row per alert
/// and has to choose between the assets a monitor watches, while this draws one
/// row per *asset* and an asset either has an open alert or has not.
macro_rules! open_alert_opened_at {
    () => {
        concat!(
            "(select max(al.opened_at)
                        from knobas.monitor_alert al
                        ",
            monitors_asset!("al.entity_id"),
            "
                       where al.closed_at is null)"
        )
    };
}

/// An asset with an open alert that some unarchived context holds (#504).
///
/// **The context half is `knobas_core::held_by_any_context!` and not a second
/// reading of it** -- ADR-0008's membership walk, seeded from every unarchived
/// context at once, whose `held` layer *is* "directly or through an ancestor".
/// The subquery binds nothing from the outer row, so the planner hashes it.
///
/// # Why there is no `acked_at is null` here
///
/// Because **the routing rule is a defined term and the ack is not in it.**
/// Spec §12.3 (`docs/specs/2026-08-23-knobas-design.md`) puts the two in
/// consecutive sentences: *"**Alert routing rule (R3, now spec):** an alert
/// reaches the inbox only when some context holds the affected asset (directly
/// or via an ancestor or via the context's monitors); all open alerts always
/// show in the Assets views and the top-strip count. **Ack** is knobas-local:
/// clears the inbox item, writes history, the alert stays open until the
/// monitor recovers"*. `CONTEXT.md`'s **Alert** keeps the same split, and its
/// #446 amendment uses the term the same way -- *"the one place the alert's
/// routing rule and `member_ids` differ"* is about which contexts count, not
/// about the ack. So `alert!()`'s statement in `knobas_core::inbox` is the
/// routing rule **plus the inbox's own lifecycle clause**; it is not the rule,
/// and this list takes the rule.
///
/// **And *open* is defined too.** `CONTEXT.md`, **Alert**: *"open until the
/// monitor recovers … **Only a return to *up* closes one**"*. A list called
/// *Open alerts* that dropped acked ones would be false in its first word.
/// Spec §12.3's *"all open alerts always show in the Assets views and the
/// top-strip count"* is which side of the ack an estate surface sits on, and
/// this list draws assets and opens the Tree.
///
/// The reading `knobas_core::inbox`'s `alert!` doc gives -- *"an acked alert
/// leaves the inbox and stays in the Assets view"* -- says the same thing, and
/// is cited here as what it is: a doc comment reading spec #427 story 62, not
/// an authority of its own. The authority is the glossary and §12.3 above.
///
/// Pinned by `an_acked_alert_is_still_open_and_still_on_the_list` in
/// `crates/knobas-app/tests/search_ipc.rs`. Ruled for v1.5 on 2026-09-08
/// (`docs/decisions/2026-09-v1-5-unattended-rulings.md`, #504), which also
/// records why copying the clause would not have made the two counts agree:
/// #446's ack writes `acked_at` **and** `complete_with`, and the inbox's count
/// excludes snoozed items, so the disagreement is a shelf and not a clause.
macro_rules! alerts_in_context_pred {
    () => {
        concat!(
            open_alert_opened_at!(),
            " is not null
                         and a.id in ",
            knobas_core::held_by_any_context!()
        )
    };
}

/// When knobas last saw a certificate on this asset's monitors with under
/// [`CERT_DAYS`] days left, or `null` if it saw none.
///
/// # A payload read outside an adapter (ADR-0007), and the third one
///
/// `KindPaths` has no slot for "days until this certificate expires", so
/// declaring one would be a `crates/knobas-source/src/**` change and a §10.8
/// conversation this ticket does not open. The interim discipline therefore
/// applies in full, and this statement meets all three requirements:
///
/// 1. **It misses, never guesses.** `jsonb_typeof(...) = 'number'` is checked
///    *before* the cast, so a payload carrying `"nine"`, `null` or no such key
///    at all contributes nothing. Without the guard the `::numeric` cast raises
///    and the whole launcher board fails to load on one drifted row -- which is
///    the difference between a miss and an outage, not a matter of taste.
/// 2. **One named place**, this macro, read by [`certs_expiring_pred!`] for the
///    predicate and by the summary for the stamp.
/// 3. **The failure direction is absence.** A drifted key empties the list; it
///    never puts an asset on it whose certificate is fine.
///
/// # The ticket says "the samples", and the samples do not carry it
///
/// Issue #504 asks for a list that *"reads the certificate days the samples
/// carry"*. They do not: migration `0021`'s `knobas.monitor_sample` is
/// `state` and `response_time_ms` and nothing else, and the certificate
/// countdown exists only in the mirrored monitor's payload. So the read is the
/// payload's, which is the only place the number is, and the ticket's phrase is
/// loose rather than a second design nobody built. Recorded here rather than
/// left for the next reader to rediscover by grepping `monitor_sample`.
///
/// `cert_days_remaining` is the key `knobas_source_kuma::map` writes and
/// `knobas_app::assets`' `reading_of` already reads for the Monitors tab's
/// *Cert N d* chip. This is the **third** place that spelling appears and the
/// second reader of it: a change to what Kuma's certificate countdown is called
/// in a payload is a change to all three. `the_certificate_key_is_the_one_the_
/// monitors_tab_draws` is the pin on this side of it.
///
/// **`sync.live_item`, never `sync.item`.** `CONTEXT.md`'s **Live item** names
/// the four readers that reach past the live view -- the Monitors roster
/// (#448), the detail read (#204), the paste resolver (#496) and the checkout
/// read (#499) -- and says *"a fifth needs a reason of its own and a line
/// here"*. This is not one of them and owes no line there.
///
/// The consequence, stated because it is where this list and the roster part
/// company. A monitor whose **source the reader switched off** drops out of
/// both: the roster keeps the enabled clause and so does the view. A monitor
/// Kuma **paused or deleted** is tombstoned by the adapter, and it drops out of
/// *this* list while staying on the roster -- reader 1 is exempt from the
/// tombstone half precisely so the roster can draw the *Paused* chip and the
/// monitor's history. There is no equivalent thing to draw here: a countdown
/// is a number knobas is being told, and nobody is polling a paused check, so
/// the honest answer is that the asset leaves the list rather than sitting on
/// it under a reading that has stopped moving.
macro_rules! expiring_cert_seen_at {
    () => {
        concat!(
            "(select max(m.synced_at)
                        from sync.live_item m
                        ",
            monitors_asset!("m.entity_id"),
            "
                       where m.kind = 'monitor'
                         and jsonb_typeof(m.payload -> 'cert_days_remaining') = 'number'
                         and (m.payload ->> 'cert_days_remaining')::numeric < 30)"
        )
    };
}

/// How many days left counts as *expiring* -- the ticket's *"under 30 days"*.
///
/// Strict: a certificate with exactly 30 days left is **not** on the list, and
/// neither is one with 31. Written here as well as inside
/// [`expiring_cert_seen_at!`] -- which cannot read a constant, because
/// `concat!` takes literals -- so that the number a reader looks up and the
/// number the statement uses are checked against each other by
/// `the_certificate_window_is_the_one_the_label_promises`.
pub const CERT_DAYS: i64 = 30;

/// An asset one of whose monitors carries a certificate expiring inside
/// [`CERT_DAYS`] (#504).
macro_rules! certs_expiring_pred {
    () => {
        concat!(expiring_cert_seen_at!(), " is not null")
    };
}

/// Declare the built-in lists **once**, and generate both the registry and the
/// single-pass summary from that one declaration.
///
/// The point of the macro is that a list's predicate is written in exactly one
/// place. A count computed from a different predicate than the rows it claims
/// to count is the worst failure available here -- a wrong number on screen
/// that no test comparing the list against itself can see.
macro_rules! builtins {
    (
        $( scan: $id:literal, $label:literal, $column:literal, $blurb:literal, $pred:literal; )*
        $( asset: $aid:literal, $alabel:literal, $acolumn:literal, $ablurb:literal,
                  $apred:ident, $astamp:ident; )*
    ) => {
        /// Every built-in list, in the order the launcher rail draws them.
        ///
        /// The mirror lists first and the estate lists after them, which is
        /// what the two declaration groups below say and is also the order a
        /// reader wants: *what moved* before *what is wrong with the machines*.
        pub const BUILTINS: &[BuiltinList] = &[
            $( BuiltinList {
                id: $id, label: $label, blurb: $blurb, column: $column,
                rows_sql: rows_over!($pred),
            }, )*
            $( BuiltinList {
                id: $aid, label: $alabel, blurb: $ablurb, column: $acolumn,
                rows_sql: rows_over_estate!($apred!()),
            }, )*
        ];

        /// Counts, freshness and the seen-stamps, in one round trip.
        ///
        /// `$1` is the identity usernames. `scanned` is the row count the
        /// mirror pass covered and `estate_scanned` the estate pass; each also
        /// anchors its generated comma-separated list of aggregates, which is
        /// why each is first in its CTE.
        ///
        /// **Every list is a `count(*) filter` over one scan of the corpus it
        /// draws from**, and the whole board is one round trip. There are two
        /// corpora and therefore two CTEs -- `scan` over `sync.live_item` and
        /// `estate` over `knobas.asset` -- and the macro admits nothing else:
        /// a list that is not a filter over one of those two scans cannot be
        /// declared at all. What the rule fixes is what a `count(*)` counts,
        /// not how many tables the statement touches: the estate predicates
        /// probe four more relations, and the module docs say what that costs
        /// and what was measured.
        ///
        /// The estate CTE computes each list's predicate and its badge stamp
        /// **as columns of an inner select** and aggregates over those, rather
        /// than writing the predicate inside the aggregate. That is not a
        /// style choice: two of the three stamps are correlated subqueries, and
        /// this shape keeps every one of them an ordinary select-list
        /// expression over `knobas.asset a`.
        const SUMMARY_SQL: &str = concat!(
            "with scan as (\n  select count(*) as scanned",
            $(
                ",\n    count(*) filter (where ", $pred, ") as ", $column,
                ",\n    max(i.synced_at) filter (where ", $pred, ") as ", $column, "_at",
            )*
            "\n    from sync.live_item i\n),\n",
            "estate as (\n  select count(*) as estate_scanned",
            $(
                ",\n    count(*) filter (where e.", $acolumn, "_hit) as ", $acolumn,
                ",\n    max(e.", $acolumn, "_stamp) filter (where e.", $acolumn, "_hit) as ",
                $acolumn, "_at",
            )*
            "\n    from (select a.id",
            $(
                ",\n                 ", $apred!(), " as ", $acolumn, "_hit",
                ",\n                 ", $astamp!(), " as ", $acolumn, "_stamp",
            )*
            "\n            from knobas.asset a) e\n),\n",
            "seen as (\n",
            "  select coalesce(\n",
            "           (select value from knobas.setting where key = 'search.smart_list_seen'),\n",
            "           '{}'::jsonb) as v\n",
            ")\n",
            "select scan.scanned, estate.estate_scanned",
            $(
                ",\n       scan.", $column, ", scan.", $column, "_at",
                ",\n       seen.v ->> '", $id, "' as ", $column, "_seen",
            )*
            $(
                ",\n       estate.", $acolumn, ", estate.", $acolumn, "_at",
                ",\n       seen.v ->> '", $aid, "' as ", $acolumn, "_seen",
            )*
            "\n  from scan, estate, seen\n"
        );
    };
}

/// The `knobas.setting` key the per-list "last opened" stamps live under.
///
/// Spelled out again inside [`SUMMARY_SQL`] and [`MARK_SEEN_SQL`] because
/// `concat!` takes literals and nothing else. The two statements agreeing is
/// load-bearing -- a read of one key and a write to another is a badge that
/// never clears -- so `the_two_statements_agree_on_the_seen_key` checks it, and
/// `tests/lists.rs` exercises the round trip against a real database.
pub const SEEN_KEY: &str = "search.smart_list_seen";

builtins! {
    scan: "changed-today", "Changed today", "changed_today",
        "Everything a source touched since midnight.",
        "i.item_updated_at >= date_trunc('day', now())";
    scan: "mine", "My items", "mine",
        "Items your configured accounts are the author of, from the last 30 days.",
        "i.author = any($1) and i.item_updated_at >= now() - interval '30 days'";
    scan: "mine-stale", "Mine, untouched 14 days", "mine_stale",
        "Yours, and nothing has moved them in a fortnight.",
        "i.author = any($1) and i.item_updated_at < now() - interval '14 days'";
    scan: "just-synced", "Just synced", "just_synced",
        "What the last hour of syncing brought in.",
        "i.synced_at >= now() - interval '1 hour'";

    // The estate (#504). Rows are **assets**, so a row opens the Tree at the
    // asset -- `Launcher.svelte` addresses a hit of kind `asset` through
    // `assets/tree.ts`' own encoder, and these lists need nothing of their own
    // for that.
    asset: "not-monitored", "Not monitored", "not_monitored",
        "Estate assets with no monitor attached to them.",
        not_monitored_pred, asset_touched_at;
    asset: "alerts-in-context", "Open alerts in my contexts", "alerts_in_context",
        "Assets a context you have not archived holds, with a monitor in trouble.",
        alerts_in_context_pred, open_alert_opened_at;
    asset: "certs-expiring", "Certificates expiring", "certs_expiring",
        "Assets whose certificate runs out in under 30 days.",
        certs_expiring_pred, expiring_cert_seen_at;
}

/// Record that a list was just opened, so its badge clears.
///
/// One key, merged rather than replaced: `value || excluded.value` keeps the
/// other lists' stamps, which is the whole reason all five share one row.
const MARK_SEEN_SQL: &str = r"
insert into knobas.setting (key, value)
     values ('search.smart_list_seen', jsonb_build_object($1::text, to_jsonb(now())))
on conflict (key) do update
        set value = knobas.setting.value || excluded.value,
            updated_at = now()
";

/// The list with this id, if knobas ships one.
#[must_use]
pub fn find(id: &str) -> Option<&'static BuiltinList> {
    BUILTINS.iter().find(|list| list.id == id)
}

/// Every list's count, freshness and blurb, in one round trip.
///
/// # Errors
///
/// [`SearchError::Db`] if the summary cannot be read.
pub async fn summaries(
    pool: &PgPool,
    identity: &[String],
) -> Result<Vec<SmartListSummary>, SearchError> {
    let row = sqlx::query(SUMMARY_SQL)
        .bind(identity)
        .fetch_one(pool)
        .await?;

    let mut out = Vec::with_capacity(BUILTINS.len());
    for list in BUILTINS {
        let count: i64 = row.try_get(list.column)?;
        let newest: Option<DateTime<Utc>> = row.try_get(&*format!("{}_at", list.column))?;
        let seen: Option<String> = row.try_get(&*format!("{}_seen", list.column))?;
        out.push(SmartListSummary {
            id: list.id.to_owned(),
            label: list.label.to_owned(),
            count,
            changed: changed(newest, seen.as_deref()),
            description: describe(list, identity),
        });
    }
    Ok(out)
}

/// The rows of one list, as the flat shape [`crate::group::group`] folds.
///
/// # Errors
///
/// [`SearchError::Db`] if the statement fails.
pub async fn rows(
    pool: &PgPool,
    list: &BuiltinList,
    identity: &[String],
    limit: u32,
) -> Result<Vec<RawHit>, SearchError> {
    Ok(sqlx::query_as::<_, RawHit>(list.rows_sql)
        .bind(identity)
        .bind(i64::from(limit))
        .fetch_all(pool)
        .await?)
}

/// Note that a list has just been looked at.
///
/// # Errors
///
/// [`SearchError::Db`] if the setting cannot be written.
pub async fn mark_seen(pool: &PgPool, id: &str) -> Result<(), SearchError> {
    sqlx::query(MARK_SEEN_SQL).bind(id).execute(pool).await?;
    Ok(())
}

/// Whether a list has something in it that postdates the last look at it.
///
/// An **unparseable** stamp reads as "never opened" rather than as an error:
/// the value is ours to write, but it is in a shared key/value table on the
/// launcher's hot path, and a badge that is on too often beats a board that
/// refuses to load.
fn changed(newest: Option<DateTime<Utc>>, seen: Option<&str>) -> bool {
    // Nothing in the list: a badge on an empty list is noise, not news.
    let Some(newest) = newest else { return false };
    match seen.and_then(parse_stamp) {
        Some(seen) => newest > seen,
        None => true,
    }
}

fn parse_stamp(raw: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|stamp| stamp.with_timezone(&Utc))
}

/// A list's display text: its blurb, or the reason it cannot say anything.
///
/// A list that explains its own zero beats one that just reads 0 -- and "no
/// source has a username configured" is a thing the user can act on, whereas
/// an empty *My items* looks like a sync that has not run.
fn describe(list: &BuiltinList, identity: &[String]) -> String {
    if list.needs_identity() && identity.is_empty() {
        describe_missing_identity()
    } else {
        list.blurb.to_owned()
    }
}

/// What an identity list says when no source was configured with an account.
///
/// Public so a caller -- and a test -- can compare against the one wording
/// rather than a second copy of it.
#[must_use]
pub fn describe_missing_identity() -> String {
    // Phrased as *when* the fill happens, not as an instruction to press a
    // button: the only *Test connection* control in the app is inside the
    // Add-source dialog, on a draft. A saved source has Re-enter, Sync now and
    // Delete and nothing else, so "run Test connection on a source" would be
    // this message committing the very sin #82 is about -- naming an action
    // its reader cannot take. The second clause is the honest remainder, and
    // it is also the shape of the follow-up that would close it.
    "No source has a username configured, so knobas cannot tell which items \
     are yours. Test connection fills a source's username in when the source \
     is added; there is no way yet to set one on a source that already exists."
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ids are what `list:<id>` names and what the seen-stamps are keyed on, so
    /// a duplicate would be a list that can never be opened.
    #[test]
    fn every_list_has_a_distinct_id_and_summary_column() {
        for list in BUILTINS {
            assert_eq!(
                BUILTINS.iter().filter(|l| l.id == list.id).count(),
                1,
                "{}",
                list.id
            );
            assert_eq!(
                BUILTINS.iter().filter(|l| l.column == list.column).count(),
                1,
                "{}",
                list.id
            );
            assert!(find(list.id).is_some());
        }
        assert!(find("nope").is_none());
    }

    /// The summary has to project a column per list, under the name the
    /// decoder reads. Generated from one declaration, so this pins the
    /// generator rather than a hand-maintained table -- and the statement is
    /// *executed* in `tests/lists.rs`, which is what proves the columns exist.
    #[test]
    fn the_summary_projects_every_lists_three_columns() {
        for list in BUILTINS {
            for suffix in ["", "_at", "_seen"] {
                assert!(
                    SUMMARY_SQL.contains(&format!("{}{suffix}\n", list.column))
                        || SUMMARY_SQL.contains(&format!("{}{suffix},", list.column)),
                    "{}{suffix} is missing from the summary:\n{SUMMARY_SQL}",
                    list.column
                );
            }
            assert!(
                SUMMARY_SQL.contains(&format!("'{}'", list.id)),
                "{} has no seen-stamp lookup",
                list.id
            );
        }
    }

    /// The estate lists are the ones that scan `knobas.asset`, and there are
    /// exactly three of them.
    ///
    /// Read off the statement in both directions, which is
    /// [`BuiltinList::needs_identity`]'s rule and for its reason: a mirror list
    /// that started reading `knobas.asset` would be counting machines under a
    /// heading about tickets, and an estate list that stopped would answer with
    /// the mirror. The membership is asserted **by id**, so a list moved from
    /// one group to the other fails here rather than silently changing what it
    /// counts.
    ///
    /// **The alias is what tells the two apart, and that is load-bearing.**
    /// *Certificates expiring* reads `sync.live_item` too -- inside its
    /// predicate, as `m` -- so the mirror probe has to be `from sync.live_item
    /// i`, the outer scan's own alias, or that list would answer to both
    /// halves. `i` and `a` are the aliases [`rows_over!`] and
    /// [`rows_over_estate!`] give the relation a list is a list *of*; a
    /// predicate that wanted either letter would have to shadow it.
    #[test]
    fn exactly_the_estate_lists_scan_the_estate() {
        let over_the_estate: Vec<&str> = BUILTINS
            .iter()
            .filter(|l| l.rows_sql.contains("from knobas.asset a"))
            .map(|l| l.id)
            .collect();
        assert_eq!(
            over_the_estate,
            ["not-monitored", "alerts-in-context", "certs-expiring"]
        );
        let over_the_mirror: Vec<&str> = BUILTINS
            .iter()
            .filter(|l| l.rows_sql.contains("from sync.live_item i"))
            .map(|l| l.id)
            .collect();
        assert_eq!(
            over_the_mirror,
            ["changed-today", "mine", "mine-stale", "just-synced"]
        );
        // And the summary makes both passes, or one group's counts come back
        // from a statement that never scanned them.
        assert!(
            SUMMARY_SQL.contains("from sync.live_item i"),
            "{SUMMARY_SQL}"
        );
        assert!(SUMMARY_SQL.contains("from knobas.asset a"), "{SUMMARY_SQL}");
        assert!(
            SUMMARY_SQL.contains("from scan, estate, seen"),
            "{SUMMARY_SQL}"
        );
    }

    /// An estate row is an **asset**, which is what makes a row open the Tree:
    /// `Launcher.svelte` addresses a hit by its `kind`, and only `asset` gets
    /// `assets/tree.ts`' encoder. A list answering with any other kind would
    /// draw rows that open a room detail instead.
    #[test]
    fn an_estate_list_answers_with_asset_rows() {
        for list in BUILTINS
            .iter()
            .filter(|l| l.rows_sql.contains("from knobas.asset a"))
        {
            assert!(
                list.rows_sql.contains("'asset'::text as kind"),
                "{} does not answer with asset rows",
                list.id
            );
            assert!(
                list.rows_sql.contains("'asset'::text as source_id"),
                "{} does not name the estate as its source",
                list.id
            );
            // The path is the store's column, never an `ancestor_path_read!`
            // over a payload -- `corpus::Corpus::path`'s second case.
            assert!(
                list.rows_sql.contains("nullif(a.path_text, '') as path"),
                "{} does not carry where the asset sits",
                list.id
            );
        }
    }

    /// *Not monitored* is the Monitors tab's roster rule, and the two hard-code
    /// the same two words where the roster binds them.
    ///
    /// `knobas_app::assets`' `UNMONITORED` passes the relation and the monitor
    /// kind as `$1` and `$2`; this statement cannot, because `concat!` takes
    /// literals. So the relation is checked against the one constant both
    /// crates read. The kind has no such constant to check against -- it is a
    /// declared kind, not a knobas-owned one -- and is named here so the next
    /// reader knows the pin is one-sided.
    #[test]
    fn the_roster_rule_is_spelled_the_way_the_link_table_spells_it() {
        let list = find("not-monitored").unwrap();
        assert!(
            list.rows_sql.contains(&format!(
                "l.relation = '{}'",
                knobas_core::link::MONITORED_BY
            )),
            "{}",
            list.rows_sql
        );
        assert!(
            list.rows_sql.contains("m.kind = 'monitor'"),
            "{}",
            list.rows_sql
        );
        // The proposal half: an unconfirmed guess leaves the asset on the list.
        assert!(
            list.rows_sql.contains("knobas.confirmed_link"),
            "{}",
            list.rows_sql
        );
        // The base table's name is **assembled** rather than written out.
        // `knobas-core`'s `tests/link_reads.rs` scans every shipping source
        // file for a read of it and says "a doc comment counts: it is what the
        // next implementer copies" -- and a negative assertion is no
        // exception, because the scan cannot tell the two apart. Same device,
        // and same reason, as `tests/sql_containment.rs`' split needle.
        let base_table = format!("knobas.{}", "link");
        assert!(
            !list.rows_sql.contains(&format!("from {base_table} ")),
            "the roster must not count proposals: {}",
            list.rows_sql
        );
    }

    /// The context half of *Open alerts in my contexts* is ADR-0008's walk and
    /// not a second reading of it, and the alert half is open-and-not-closed.
    #[test]
    fn the_alert_list_routes_by_the_membership_walk_and_nothing_else() {
        let list = find("alerts-in-context").unwrap();
        // The walk's own text, which only `held_by_any_context!` produces:
        // seeded from every unarchived context, with the `held` recursion that
        // is "directly or through an ancestor".
        assert!(
            list.rows_sql.contains("from knobas.context c"),
            "{}",
            list.rows_sql
        );
        assert!(
            list.rows_sql
                .contains("join knobas.asset c on c.parent_id = h.id"),
            "the ancestor half of the rule is missing: {}",
            list.rows_sql
        );
        assert!(
            list.rows_sql.contains("c.archived_at is null"),
            "{}",
            list.rows_sql
        );
        assert!(
            list.rows_sql.contains("al.closed_at is null"),
            "{}",
            list.rows_sql
        );
        // Ack is the inbox's clause and not this list's -- see the predicate's
        // own doc for why.
        assert!(
            !list.rows_sql.contains("acked_at"),
            "an ack is seen, not fixed: {}",
            list.rows_sql
        );
    }

    /// The certificate window a reader looks up and the one the statement uses.
    ///
    /// `concat!` cannot read [`CERT_DAYS`], so the number is written twice and
    /// this is the only thing standing between the two. The blurb is checked as
    /// well: a label promising thirty days over a statement asking for sixty is
    /// the same failure one screen further out.
    #[test]
    fn the_certificate_window_is_the_one_the_label_promises() {
        let list = find("certs-expiring").unwrap();
        assert_eq!(CERT_DAYS, 30);
        assert!(
            list.rows_sql.contains(&format!("::numeric < {CERT_DAYS})")),
            "{}",
            list.rows_sql
        );
        assert!(
            list.blurb.contains(&format!("under {CERT_DAYS} days")),
            "{}",
            list.blurb
        );
    }

    /// The key Kuma's adapter writes and the Monitors tab already draws.
    ///
    /// Guarded before it is cast, which is the difference between an empty list
    /// and a launcher board that fails to load on one drifted payload.
    #[test]
    fn the_certificate_key_is_the_one_the_monitors_tab_draws() {
        let list = find("certs-expiring").unwrap();
        assert!(
            list.rows_sql
                .contains("jsonb_typeof(m.payload -> 'cert_days_remaining') = 'number'"),
            "{}",
            list.rows_sql
        );
        let guard = list.rows_sql.find("jsonb_typeof").expect("the guard");
        let cast = list.rows_sql.find("::numeric").expect("the cast");
        assert!(
            guard < cast,
            "the cast runs before the guard: {}",
            list.rows_sql
        );
        // The live view, never the base table: `CONTEXT.md`'s **Live item**
        // names the readers that reach past it and this is not one of them.
        assert!(
            list.rows_sql.contains("from sync.live_item m"),
            "{}",
            list.rows_sql
        );
        assert!(
            !list.rows_sql.contains("from sync.item"),
            "{}",
            list.rows_sql
        );
    }

    /// Exactly the two lists that filter by author read the identity
    /// parameter. Both directions matter: a list that stopped reading it would
    /// silently become "everybody's items", and one that started would empty
    /// itself on an install with no account configured.
    #[test]
    fn only_the_identity_lists_read_the_identity_parameter() {
        let needs: Vec<&str> = BUILTINS
            .iter()
            .filter(|l| l.needs_identity())
            .map(|l| l.id)
            .collect();
        assert_eq!(needs, ["mine", "mine-stale"]);
        // Every list reads the limit, or it is not a bounded list at all.
        assert!(BUILTINS.iter().all(|l| l.rows_sql.contains("$2")));
    }

    #[test]
    fn the_two_statements_agree_on_the_seen_key() {
        assert!(
            SUMMARY_SQL.contains(&format!("'{SEEN_KEY}'")),
            "{SUMMARY_SQL}"
        );
        assert!(
            MARK_SEEN_SQL.contains(&format!("'{SEEN_KEY}'")),
            "{MARK_SEEN_SQL}"
        );
    }

    /// The advice has to be followable, and since #82 the shortest route to a
    /// username is not typing one: the Add-source dialog fills the field from
    /// what *Test connection* reports. Advice naming only the typing is advice
    /// to redo by hand what the app already did -- the contradiction #82 was
    /// about, in its last surviving corner.
    ///
    /// It has to name that mechanism **without** telling the reader to go and
    /// operate it, because they cannot: the fill is an Add-source step, and a
    /// saved source offers Re-enter, Sync now and Delete. An imperative here
    /// would be a second false instruction replacing the first one.
    #[test]
    fn the_missing_identity_advice_names_the_thing_that_fills_it() {
        let advice = describe_missing_identity();
        assert!(advice.contains("Test connection"), "{advice}");
        // And still names the field itself, which is what a reader has to go
        // and find on a source.
        assert!(advice.contains("username"), "{advice}");
        // Not an instruction to run it on a source that already exists --
        // there is no control that does that, which is the whole point above.
        assert!(!advice.contains("Run Test connection"), "{advice}");
    }

    #[test]
    fn a_list_with_no_identity_explains_itself_instead_of_reading_zero() {
        let mine = find("mine").unwrap();
        let today = find("changed-today").unwrap();
        assert!(describe(mine, &[]).contains("username"));
        assert_eq!(describe(mine, &["mara".to_owned()]), mine.blurb);
        // A list that needs no identity says its own blurb either way.
        assert_eq!(describe(today, &[]), today.blurb);
    }

    #[test]
    fn the_badge_is_news_and_not_noise() {
        let old = Utc::now() - chrono::Duration::hours(2);
        let new = Utc::now();
        let stamp = |at: DateTime<Utc>| at.to_rfc3339();

        // Never opened, and there is something to see.
        assert!(changed(Some(new), None));
        // Opened after the newest thing in it.
        assert!(!changed(Some(old), Some(&stamp(new))));
        // Something arrived since.
        assert!(changed(Some(new), Some(&stamp(old))));
        // Nothing in the list at all: no badge, opened or not.
        assert!(!changed(None, None));
        assert!(!changed(None, Some(&stamp(old))));
        // A stamp nobody can read is "never opened", not an error.
        assert!(changed(Some(new), Some("not a timestamp")));
    }

    /// Postgres renders `to_jsonb(now())` in the session's time zone, so the
    /// stamp that comes back carries an offset rather than a `Z`. Reading it as
    /// naive UTC would move every badge by the offset.
    #[test]
    fn a_stamp_with_an_offset_is_read_as_the_instant_it_names() {
        let noon_utc = parse_stamp("2026-08-25T12:00:00+00:00").unwrap();
        let two_pm_plus_two = parse_stamp("2026-08-25T14:00:00+02:00").unwrap();
        assert_eq!(noon_utc, two_pm_plus_two);
        // And the microseconds Postgres renders survive the trip.
        let precise = parse_stamp("2026-08-25T14:00:00.123456+02:00").unwrap();
        assert!(
            precise > two_pm_plus_two && precise - two_pm_plus_two < chrono::Duration::seconds(1)
        );
        assert!(parse_stamp("").is_none());
    }
}

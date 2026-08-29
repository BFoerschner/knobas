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
//! * `knobas.link` makes the same question **exact and O(1)** in M2, so the
//!   text probe is a stopgap with a known replacement rather than the design.
//!
//! ## What M2 restores
//!
//! With `knobas.link` populated the predicate stops being a text probe and
//! becomes an index probe:
//!
//! ```sql
//! count(*) filter (where i.kind = 'ticket'
//!                    and exists (select 1 from knobas.link l
//!                                 where l.from_id = i.entity_id
//!                                   and l.deleted_at is null))
//! ```
//!
//! `link_from_idx` makes that O(1) per ticket, so it collapses into the single
//! scan like every other list -- which is why that is now a rule.
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
//! [`SUMMARY_SQL`] answers the whole launcher board in **one** pass: the counts
//! (a `count(*) filter` per list over one scan of `sync.live_item`), the newest
//! sync stamp in each list (what the change badge compares against), and the
//! per-list "last opened" stamps out of `knobas.setting`. The identity behind
//! `@me` is the second query, and it is [`crate::vocab::Vocabulary::load`]'s --
//! the same one every search already makes.
//!
//! There is no exception to that single pass, and after the removal above there
//! is no way to write one: the `builtins!` macro takes `scan:` entries and
//! nothing else, so a list that is not a `count(*) filter` over this scan
//! cannot be declared at all.
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
            "         i.synced_at, 0::real as rank, null::text as snippet\n",
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
            "       p.rank as rank, p.snippet as snippet\n",
            "  from totals t\n",
            "  left join picked p on p.kind = t.kind\n",
            " order by t.kind, p.item_updated_at desc nulls last, p.entity_id\n"
        )
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
    ) => {
        /// Every built-in list, in the order the launcher rail draws them.
        pub const BUILTINS: &[BuiltinList] = &[
            $( BuiltinList {
                id: $id, label: $label, blurb: $blurb, column: $column,
                rows_sql: rows_over!($pred),
            }, )*
        ];

        /// Counts, freshness and the seen-stamps, in one round trip.
        ///
        /// `$1` is the identity usernames. `scanned` is the row count the one
        /// pass covered; it also anchors the generated comma-separated list of
        /// aggregates, which is why it is first.
        ///
        /// Every list is a `count(*) filter` over the **same single scan**, and
        /// that is now a rule rather than a convenience -- see the module docs
        /// on what the one list that could not be written this way cost.
        const SUMMARY_SQL: &str = concat!(
            "with scan as (\n  select count(*) as scanned",
            $(
                ",\n    count(*) filter (where ", $pred, ") as ", $column,
                ",\n    max(i.synced_at) filter (where ", $pred, ") as ", $column, "_at",
            )*
            "\n    from sync.live_item i\n),\n",
            "seen as (\n",
            "  select coalesce(\n",
            "           (select value from knobas.setting where key = 'search.smart_list_seen'),\n",
            "           '{}'::jsonb) as v\n",
            ")\n",
            "select scan.scanned",
            $(
                ",\n       scan.", $column, ", scan.", $column, "_at",
                ",\n       seen.v ->> '", $id, "' as ", $column, "_seen",
            )*
            "\n  from scan, seen\n"
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

//! Which sources can answer a filter, and which merely have nothing to say
//! (issue #141).
//!
//! # The gap this closes
//!
//! `author:` and `@` shipped in #39. TeamCity authorship shipped in #33, and
//! #106 measured what it is worth: **100 of 100** newest finished builds on a
//! live server name no user, because real CI builds are VCS-triggered. The
//! field is correctly absent, not mis-read -- so `author:someone` over a corpus
//! that includes TeamCity comes back empty, and until now *nothing on the wire
//! distinguished that from a token that is broken*.
//!
//! Björn ruled option 1 on 2026-08-31: the answer belongs on the response,
//! per-source and per-query. This module is the measurement behind it.
//!
//! # Measured on the corpus, and deliberately not on the match set
//!
//! The probe asks one question per source: **does anything this source
//! contributed to this query's corpus carry an author?** The line between what
//! narrows it and what does not is the whole design, and it is not the line
//! between "filter" and "no filter":
//!
//! * **the query's structural scope narrows it** -- `source:` and `kinds`, the
//!   two dimensions that decide *which of a source's rows are in this search at
//!   all*. A report about sources the query excluded is a report about somebody
//!   else's question, and `note: @jonas` naming a build server would explain an
//!   absence that authorship had nothing to do with;
//! * **the match set does not** -- neither the text nor the `updated:` window.
//!   Narrowed by those, a source whose authored items simply did not match the
//!   words typed would report as unable to answer, which collapses the two
//!   states this report exists to keep apart. Measured over the corpus,
//!   [`FilterAnswer::NoValues`] means *"an author query cannot be answered
//!   here"*, a claim that is still true on the next keystroke.
//!
//! `updated:` is on the match-set side of that line and it is the one judgement
//! call here. It is a *recency* window rather than a scope: "nothing in the last
//! week names a person" is a fact about a week, and reporting it as "this source
//! cannot answer an author query" would be the sparsity threshold the next
//! section refuses, arrived at sideways.
//!
//! A source with **no rows at all** in that scope is left out of the report
//! rather than given a verdict. It contributed no corpus, so authorship is not
//! why it is absent from the results, and saying anything about it here would
//! explain the wrong absence -- the same reason `Searcher::search`'s
//! short-circuits report nothing for `t `, `>` and `?`. (That list held
//! `asset:` too until #436 turned the prefix on; an estate query is now
//! reported on like any other, and answers with an empty report for the
//! ordinary reason -- no *source* has synced an asset.)
//!
//! # Why existence and not sparsity
//!
//! One authored row flips a source to [`FilterAnswer::Answered`], and that is
//! the intended line. "Under n% of this corpus names a person" would be a
//! threshold -- a judgement nobody ruled, and one that would call a source
//! unable to answer a query it can in fact answer. Strict existence is the only
//! non-arbitrary boundary, and it is the boundary the ruling named.
//!
//! # What it costs, measured
//!
//! Nothing for an ordinary keystroke: all of it is asked only for a query that
//! filtered by author or by parent. A query that did pays up to four reads, and
//! the first two exist so that the third is usually not asked.
//!
//! 1. **[`NEWEST_ROWS_SQL`], one bounded walk per source.** Each source's newest
//!    64 rows, withdrawn or not. Almost every source settles there, which is
//!    about 1 ms for the four of the benchmark, whatever the mirror holds.
//! 2. **[`NEWEST_OF_EACH_KIND_SQL`], a bounded look at each kind's newest rows, over
//!    the sources the walk left open.** Skipped when there are none. It settles a
//!    source under its own left-out rows, like a Gitea under its commits, for about
//!    1 ms.
//! 3. **[`AUTHOR_COVERAGE_SQL`], one grouped read over the sources still open.**
//!    Skipped when there are none.
//! 4. **[`UNDER_COVERAGE_SQL`], for `under:` alone.** An index range per source,
//!    looked at from its far end first.
//!
//! **Proving an absence reads that source's rows however it is written.** "Does
//! this source have an authored row" has no index behind it
//! (`item_source_updated_idx` is `(source_id, item_updated_at)` and carries no
//! author), and neither does "does it have a row the launcher keeps", since
//! left-out kinds are a list to exclude. That is why the bounded reads come first.
//! One row is the answer for most sources and they find it, and the rest cost one
//! scan between them, whatever their number.
//!
//! Measured on the 100,000-row fixture with a third of it unauthored, through the
//! plan cache, in one run on a shared machine at load about 10, p50 ms:
//!
//! | query | grouped read alone | one `limit 1` per source | walk, then grouped |
//! |---|---:|---:|---:|
//! | `under:`, no kind scope | 40 | 0.2 | **1.0** |
//! | `author:`, no kind scope | 40 | 28 | **26** |
//! | `author:` and `kind:ticket` | 36 | 27 | **6.5** |
//! | a kind one source has | 14 | 60 | **4.3** |
//! | a kind nothing has | 0.4 | 77 | **1.1** |
//! | every kind left out | 23 | 95 | **18** |
//!
//! The third column is why this is not simply a `limit 1`. It reads each source
//! down its index to the first row in scope, which costs next to nothing when that
//! row is near, but a kind scope a source cannot meet walks all of it. `asset:
//! under:x` went from 0.4 ms to 77 ms, because a generic plan cannot use the kind
//! index behind `cardinality($2) = 0 or kind = any($2)`. So the walk has a budget,
//! and the grouped read stays as what a source falls back to. No row of the table
//! costs more than a millisecond over the grouped read alone.
//!
//! That table cannot see a source under its own left-out rows, since the benchmark
//! cycles five kinds through every source. Add a Gitea of 20,000 commits over 430
//! rows it keeps, and that source took the grouped read on every `under:` keystroke:
//! 25 to 44 ms at p50 once analyzed and 155 to 160 never analyzed, against about 1
//! through the look at each kind (#2503). A source another source crowds out of every
//! kind it holds still goes to the grouped read, and the look that found nothing adds
//! about 0.1 ms to it.
//!
//! `sync.item` is cheaper than `sync.live_item` for that read and is **not** used:
//! half of the statement's cost is the tombstone join, and dropping it would let an
//! authored item a source has since withdrawn vouch for a capability the live corpus
//! no longer has -- reinstating this issue's own silence in the one case where it
//! matters most, a source that has *stopped* naming people.
//!
//! The end-to-end reading is
//! `tests/coverage.rs::the_author_probe_stays_inside_the_launchers_budget`, the whole
//! keystroke over the exit criterion's 100,000-row corpus with a third of it a
//! source that names nobody. It is `#[ignore]`d and therefore **not an automatic
//! gate** -- seeding 100,000 rows costs tens of seconds and a timing on a shared
//! runner is a coin flip, which is `tests/perf.rs`'s reasoning and the same trade. It
//! is a measurement anyone can re-run, not a promise CI keeps. What the gate does
//! hold is the plans, with no clock: `tests/it/coverage_plans.rs`.

use sqlx::PgPool;

use crate::SearchError;
use crate::query::EffectiveFilters;
use crate::types::{FilterAnswer, FilterCoverage, FilterDimension, SourceAnswer};
use crate::vocab::{SourceVocab, Vocabulary};

/// A look at each named source's newest rows: one row per source, quiet ones too.
///
/// * `in_scope`: one of them is in this query's corpus.
/// * `authored`: one of those carries an author.
/// * `read_every_row`: the source ran out before the walk did, so what the walk
///   did not see is nothing and its answers are final.
///
/// [`settle`] says what follows from those. `$2` and `$3` are
/// [`AUTHOR_COVERAGE_SQL`]'s, with its reasons.
///
/// **The `order by` is what makes this bounded.** The rows are counted, so their
/// order changes no answer. It is the order `item_source_updated_idx` keeps,
/// `(source_id, item_updated_at desc nulls last)`, and it stops the planner
/// taking a table scan for this: under a `limit` a scan looks as if it would find
/// its rows soon, and for a source with no rows it reads the whole table. A scan
/// has to read everything before a sort gives its first row, so no estimate makes
/// that look cheap (#2500, and `tests/it/coverage_plans.rs`).
///
/// **The `limit` is on `sync.item`, and liveness joins after it.** So a withdrawn
/// row spends the walk's budget and vouches for nothing. Counted on
/// `sync.live_item`, the limit waits for 64 live rows and reads past every
/// withdrawn row to find them: 70 ms on every keystroke for a source of 33,333
/// withdrawn rows and 183 ms for 100,000, against about 50 ms for the whole
/// grouped read. Such a source now stops at 64 and goes on.
/// `read_every_row` counts the same rows, so it is true only when the source ran
/// out.
///
/// **Why 64.** It costs about 5 µs a row, through the item and the entity, so
/// about 1 ms for the benchmark's three sources with rows. A walk four times
/// the length costs 1 ms more over 100,000 items and took a top-N sort of the
/// table at 3,000. The length only moves how often the grouped read is asked: a
/// walk that runs out only sends its source there.
pub(crate) const NEWEST_ROWS_SQL: &str = r"
select s.id as source_id,
       walk.in_scope,
       walk.authored,
       walk.read_every_row
  from unnest($1::text[]) s(id)
 cross join lateral (
   select count(*) filter (where live.entity_id is not null
                             and (cardinality($2::text[]) = 0 or newest.kind = any($2))
                             and newest.kind <> all($3::text[])) > 0 as in_scope,
          count(*) filter (where live.entity_id is not null
                             and (cardinality($2::text[]) = 0 or newest.kind = any($2))
                             and newest.kind <> all($3::text[])
                             and nullif(newest.author, '') is not null) > 0 as authored,
          count(*) < 64 as read_every_row
     from (select i.entity_id, i.kind, i.author
             from sync.item i
            where i.source_id = s.id
            order by i.item_updated_at desc nulls last
            limit 64) newest
     left join sync.live_item live on live.entity_id = newest.entity_id
 ) walk
";

/// The newest rows of each kind this query keeps, over the sources [`NEWEST_ROWS_SQL`]
/// left open: one row per source it saw in the corpus (#2503).
///
/// A source under its own left-out rows has shown nothing to the walk down
/// `item_source_updated_idx`, however many rows it keeps beneath them. A Gitea's
/// newest rows are commits and a TeamCity's are builds. This comes at the source
/// from the other index: `item_kind_updated_idx` holds a kind's rows in one place,
/// so the first rows of `branch` or `pipeline` are that source's without reading
/// past a single commit.
///
/// * **It can say yes and never no.** Another source can hold all 64 rows of a
///   kind, so seeing nothing proves nothing, and a source still open goes on to
///   [`AUTHOR_COVERAGE_SQL`]. So `authored` here means one of the rows seen names
///   somebody, and false means only that none of them does.
/// * **The kinds are read off the index**, one descent each, and not off the
///   declarations: a row of a kind nothing declares is in the corpus too.
/// * **`$2` and `$3` filter the kinds, not the rows.** So a generic plan still has
///   an equality on the kind for the index, which `cardinality($2) = 0 or kind =
///   any($2)` on the rows would hide from it. [`NEWEST_ROWS_SQL`] says why the
///   `order by` keeps this off a table scan.
/// * **`$1` is outside the `limit`**, so the budget is 64 rows a kind whoever holds
///   them. Inside it the read would go past every row of the kind for a source
///   that has none.
/// * **Liveness is a scalar read per row, not a join.** Joined, a generic plan
///   expects every row of every kind to pass `$1` and can hash the whole of
///   `knobas.entity` to save those lookups. A scalar subquery cannot be pulled up
///   into a join, and it comes last among the filters, so only a row of an open
///   source pays it. `entity_id` is `sync.item`'s key, so it returns one row at most.
pub(crate) const NEWEST_OF_EACH_KIND_SQL: &str = r"
with recursive kinds(kind) as (
    (select i.kind from sync.item i order by i.kind limit 1)
    union all
    select (select i.kind from sync.item i where i.kind > k.kind order by i.kind limit 1)
      from kinds k
     where k.kind is not null
)
select newest.source_id                     as source_id,
       count(nullif(newest.author, '')) > 0 as authored
  from kinds k
 cross join lateral (select i.entity_id, i.source_id, i.author
                       from sync.item i
                      where i.kind = k.kind
                      order by i.item_updated_at desc nulls last
                      limit 64) newest
 where (cardinality($2::text[]) = 0 or k.kind = any($2))
   and k.kind <> all($3::text[])
   and newest.source_id = any($1)
   and (select true from sync.live_item live where live.entity_id = newest.entity_id)
 group by newest.source_id
";

/// One row per named source that put **anything** into this query's corpus. Asked
/// only for the sources the bounded reads could not settle, because it reads all
/// their rows.
///
/// A source with nothing in scope contributes no row, and [`coverage_of`] leaves
/// it out of the report entirely: the grouping saying "there was nothing here to
/// aggregate" is exactly the state where authorship is not the reason a source
/// is absent from the results.
///
/// **`$2` is the kind scope, and the `cardinality` guard is what keeps this one
/// statement.** An empty array means "every kind", so the predicate has to
/// disappear rather than match nothing -- and writing that as a conditional
/// fragment would be runtime SQL, which roadmap §4 gotcha 2 confines to
/// [`crate::sql`]. A guard the planner folds away is the cheaper answer and
/// keeps this module out of that business entirely.
///
/// `nullif(author, '')` rather than `author is not null`: an author column that
/// is present and blank is nobody, the same reading `Vocabulary::load` already
/// gives a blank configured username. A source that writes `''` where it means
/// "no user" must not read as able to answer a question it cannot.
///
/// **`$4` is the sources already known to be in the corpus**, for which only an
/// authored row can change anything. It is what makes the source this module
/// exists for cheap: a TeamCity that names nobody passes no row out of the scan, so
/// the join to the entities has nothing to do, 23 ms against 43 at the benchmark's
/// 100,000 items. A source it does not name passes every row in scope, since the
/// corpus question is still open. A known source has a row here only if it is
/// authored, so its `authored` is true whenever it appears, and [`report`] keeps
/// what the walk said when it does not.
///
/// `sync.live_item`, never `sync.item` -- see the module docs for what that
/// costs and why it is worth it. The columns are named rather than `select *`ed:
/// the view carries a `tsvector` and reading one into a `FromRow` struct panics
/// at run time (interfaces §1).
const AUTHOR_COVERAGE_SQL: &str = r"
select i.source_id                            as source_id,
       count(nullif(i.author, '')) > 0        as authored
  from sync.live_item i
 where i.source_id = any($1)
   and (cardinality($2::text[]) = 0 or i.kind = any($2))
   and i.kind <> all($3::text[])
   and (i.source_id <> all($4::text[]) or nullif(i.author, '') is not null)
 group by i.source_id
";

/// The sources among `$1` that have synced something naming a parent: a live
/// item with a **child-arm** row in `sync.item_parent` (migration `0029`) under
/// the adapter the source is configured with (Björn's ruling on #642).
///
/// * **The child arm, never both.** A row from the names-as-parent arm says an
///   item answers to a name, and every Jira ticket answers to its own key: an
///   unconfigured classic Jira, whose tickets carry neither `epic` nor
///   `fields.parent`, has those rows and can match no epic's children.
/// * **Every kind of the source, not the query's kind scope.** `under:` also
///   returns the parent itself, so `kind:repo under:payout` shows the repo,
///   and Gitea answers it because its commits name repos -- a kind-scoped test
///   would call it unable to while it showed the repo. The kind scope still
///   decides *who is listed*, through [`NEWEST_ROWS_SQL`] and
///   [`AUTHOR_COVERAGE_SQL`].
/// * **Existence**, as for `author:` (the module docs): one row answers. So a
///   source whose adapter declares a parent but whose corpus names none -- a
///   Jira project where no ticket is in an epic -- gets
///   [`FilterAnswer::NoValues`], which is true of that corpus; a source with no
///   declaring kind has no child row and gets it too.
///
/// One probe per source, which stops at that source's first live row. The
/// copied `source_id` is what lets it ask with no join (Björn's sign-off on
/// #642): through `sync.item`, a second source on one adapter read every child
/// row of the first to prove its absence, about 150 ms at the 100k benchmark.
/// * **The far end of the range first (#2504).** A withdrawn child row passes
///   nothing, so reading from the head of the range goes on past it with an item
///   and an entity lookup. The sweep keeps withdrawn rows on purpose (`SWEEP`,
///   `crates/knobas-sync/src/lib.rs`), so a run of them stays at the head for
///   good: 13 ms a keystroke for 5,000 of them and 133 for 46,668. An index holds
///   equal keys in the order of the rows' places in the table, and a sync rewrites
///   an item's parents when it changes the item (`sync.resolve_item_parents`), so
///   the far end is what the source wrote last, which is usually live. It is a
///   bounded look, so it costs next to nothing when wrong, and it only ever says
///   yes: a live row anywhere is a live row. The head's read stays behind it, as
///   the proof for a source with none. Its cost is unchanged, plus the look, which
///   is 0.5 ms for a source whose far end is the withdrawn run.
/// * **Why 256, not the walk's 64.** Equal keys share posting lists of about 220
///   rows, a sixth of a page, and a backward scan reads each one from its low end
///   (`_bt_readpage`). So the live rows at the very end of the range can sit behind
///   the first 220 rows it reads. Fewer than that can miss them, as 64 did for 40
///   live rows after 1,000 withdrawn. The budget is only spent on withdrawn rows:
///   the look stops at the first live one.
/// * **No grouped join behind the look,** which is what #2504 proposed. It reads
///   every child row of the source and has nothing to stop it at the first live one.
///   Measured over 46,668 withdrawn rows, it took 99 to 109 ms in place of 40 to 43
///   for the read from the head, and it would read a source's whole range to answer
///   for one with a short run of withdrawn rows at each end.
/// * **Each `limit` is inside a subquery of its `exists`.** Once `sync.item_parent`
///   has statistics the planner turns a correlated `exists` into a hash semi-join
///   that reads every child-arm row, every item and every entity to answer four
///   sources: 61 ms over 100,000 items, on every `under:` keystroke (#2495). An
///   `exists` under an `or` is not pulled up into a join, and the `order by` and
///   `limit` of an `exists`'s own query are thrown away, while those of a subquery
///   in its `from` are kept.
/// * **A range and an `order by`, not `ip.source_id = s.id`.** They read as one
///   equality, and the database's collation is deterministic, so they match
///   exactly the rows it would. They are what keeps the probe on
///   `item_parent_child_idx`, `(source_id, adapter_kind) where arm = 'child'`,
///   where a source with no child row answers from an empty range. Written as
///   an equality the planner drops the sort, since a column equated to the
///   outer row is already in order, sees a table scan start as fast as an index
///   scan under `limit 1` because it expects a row soon, and takes it once it
///   has statistics. A source with no child row then reads the table whole, 16 ms
///   per keystroke at two rows an item. Where one source holds nearly all the
///   child rows the plan moves to `sync.item` and reads every item's parents,
///   750 ms at two rows an item and 1.3 s at ten (#2500). A range cannot be dropped
///   from the sort, a scan has to read everything before a sort gives its first
///   row, and so no estimate makes the scan look fast.
///   `tests/it/coverage_plans.rs` holds the plan in every state that moved it.
pub(crate) const UNDER_COVERAGE_SQL: &str = r"
select s.id
  from knobas.source_config s
 where s.id = any($1)
   and (exists (select 1
                  from (select ip.entity_id
                          from sync.item_parent ip
                         where ip.arm = 'child'
                           and ip.source_id >= s.id
                           and ip.source_id <= s.id
                           and ip.adapter_kind = s.kind
                         order by ip.source_id desc
                         limit 256) last_written
                  join sync.live_item i on i.entity_id = last_written.entity_id)
        or exists (select 1
                     from (select 1
                             from sync.item_parent ip
                             join sync.live_item i on i.entity_id = ip.entity_id
                            where ip.arm = 'child'
                              and ip.source_id >= s.id
                              and ip.source_id <= s.id
                              and ip.adapter_kind = s.kind
                            order by ip.source_id
                            limit 1) first_live))
";

/// What the probe answered about one source that put rows in scope.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub(crate) struct SourceProbe {
    source_id: String,
    /// Whether any of them carries a non-empty author. Only read for a query
    /// that filtered by author, and only final for one.
    authored: bool,
}

/// One source's row of [`NEWEST_ROWS_SQL`].
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
struct NewestRows {
    source_id: String,
    in_scope: bool,
    authored: bool,
    read_every_row: bool,
}

/// What a walk settled: a probe for every source it saw in the corpus, and the
/// sources whose verdict may lie past where it stopped.
///
/// A source is settled by what the query needs of it. One row in scope answers
/// the corpus question, so `under:` needs nothing else and an `author:` query
/// needs one authored row too. A walk that read every row is final either way,
/// and a source with no row in scope then has no probe at all, which is how
/// [`coverage_of`] knows to leave it out.
///
/// A source can be both: in the corpus with no authored row yet. Its probe says
/// so until the grouped read says otherwise.
fn settle(walks: Vec<NewestRows>, by_author: bool) -> (Vec<SourceProbe>, Vec<String>) {
    let mut probes = Vec::new();
    let mut unsettled = Vec::new();
    for walk in walks {
        let answered = walk.in_scope && (walk.authored || !by_author);
        if !answered && !walk.read_every_row {
            unsettled.push(walk.source_id.clone());
        }
        if walk.in_scope {
            probes.push(SourceProbe {
                source_id: walk.source_id,
                authored: walk.authored,
            });
        }
    }
    (probes, unsettled)
}

/// What the bounded reads settle before anything has to read a source whole: a
/// probe for every source they saw in the corpus, and the sources left for
/// [`AUTHOR_COVERAGE_SQL`].
pub(crate) async fn walk_sources(
    pool: &PgPool,
    ids: &[String],
    kinds: &[String],
    hidden: &[String],
    by_author: bool,
) -> Result<(Vec<SourceProbe>, Vec<String>), SearchError> {
    let walks: Vec<NewestRows> = sqlx::query_as(NEWEST_ROWS_SQL)
        .bind(ids)
        .bind(kinds)
        .bind(hidden)
        .fetch_all(pool)
        .await?;
    let (mut probes, mut unsettled) = settle(walks, by_author);
    if !unsettled.is_empty() {
        let seen: Vec<SourceProbe> = sqlx::query_as(NEWEST_OF_EACH_KIND_SQL)
            .bind(&unsettled)
            .bind(kinds)
            .bind(hidden)
            .fetch_all(pool)
            .await?;
        take_what_was_seen(&mut probes, &mut unsettled, seen, by_author);
    }
    Ok((probes, unsettled))
}

/// What a look at each kind's newest rows adds to the walk's answers, and who it
/// settles. A row there is evidence and nothing more: a source it does not name
/// stays as it was, and a known source only ever gains an author.
///
/// The assertion is the gate's only witness that the look is asked about the open
/// sources alone. A row for any other source changes no answer, because
/// [`coverage_of`] reads only the sources in scope, and costs only the entity
/// lookups that bind exists to save.
fn take_what_was_seen(
    probes: &mut Vec<SourceProbe>,
    unsettled: &mut Vec<String>,
    seen: Vec<SourceProbe>,
    by_author: bool,
) {
    for row in seen {
        debug_assert!(
            unsettled.contains(&row.source_id),
            "$1 let {row:?} past the look at each kind"
        );
        match probes.iter_mut().find(|p| p.source_id == row.source_id) {
            Some(known) => known.authored |= row.authored,
            None => probes.push(row),
        }
    }
    unsettled.retain(|id| {
        !probes
            .iter()
            .any(|p| p.source_id == *id && (p.authored || !by_author))
    });
}

/// The grouped read's rows over what the walk already said: a source it names
/// takes its answer, and a source it does not name keeps the walk's.
///
/// The assertion is the gate's only witness that [`report`] binds `$4`. An unbound
/// `$4` changes no answer, only the cost, 43 ms against 23. No lock can show it
/// either, because both read the same relations. But without it a known source
/// with no authored row comes back as a row saying so, and that row is what `$4`
/// exists to keep out of the join.
fn merge(probes: &mut Vec<SourceProbe>, read: Vec<SourceProbe>) {
    for row in read {
        match probes.iter_mut().find(|p| p.source_id == row.source_id) {
            Some(known) => {
                debug_assert!(row.authored, "$4 let {row:?} past the scan");
                known.authored = row.authored;
            }
            None => probes.push(row),
        }
    }
}

/// The sources with a corpus here, the only ones a verdict can be about.
fn sources_of(probes: &[SourceProbe]) -> Vec<&str> {
    probes.iter().map(|p| p.source_id.as_str()).collect()
}

/// The coverage for one query, one entry per reported dimension it filtered
/// on, or nothing to report.
///
/// Empty -- and **no round trip** -- unless the query actually filtered by
/// author or by parent. `mine` counts: `@me` runs through the same
/// `author = any(...)` predicate and hits the same gap.
///
/// **`under:` (#595) shares the probe and has a verdict of its own.** Which
/// sources put rows in this query's corpus is the same question for both
/// dimensions, so it is asked once. Whether a source can answer `under:` is
/// measured on what it synced (Björn's ruling on #642, replacing #595's "read
/// off the declaration"): [`UNDER_COVERAGE_SQL`], asked only for a query that
/// filtered by parent and only about the sources with a corpus, since no other
/// gets a verdict. A source swept whole keeps its child rows, and asked about, it
/// would read every one of them to find none live: 123 ms for 46,668.
///
/// # Errors
///
/// [`SearchError::Db`] if the probe fails.
pub(crate) async fn report(
    pool: &PgPool,
    vocab: &Vocabulary,
    filters: &EffectiveFilters,
) -> Result<Vec<FilterCoverage>, SearchError> {
    let by_author = filters.filters_by_author();
    let by_under = filters.filters_by_under();
    if !by_author && !by_under {
        return Ok(Vec::new());
    }
    let scoped = in_scope(vocab, filters);
    if scoped.is_empty() {
        return Ok(Vec::new());
    }

    let ids: Vec<String> = scoped.iter().map(|source| source.id.clone()).collect();
    let kinds = filters.kind_scope();
    let (mut probes, unsettled) =
        walk_sources(pool, &ids, &kinds, &filters.hidden_kinds, by_author).await?;
    if !unsettled.is_empty() {
        let read: Vec<SourceProbe> = sqlx::query_as(AUTHOR_COVERAGE_SQL)
            .bind(&unsettled)
            .bind(&kinds)
            .bind(&filters.hidden_kinds)
            .bind(sources_of(&probes))
            .fetch_all(pool)
            .await?;
        merge(&mut probes, read);
    }

    let mut report = Vec::new();
    if by_author {
        report.push(coverage_of(
            &scoped,
            &probes,
            FilterDimension::Author,
            |probe| probe.authored,
        ));
    }
    if by_under {
        let naming: Vec<String> = sqlx::query_scalar(UNDER_COVERAGE_SQL)
            .bind(sources_of(&probes))
            .fetch_all(pool)
            .await?;
        // As `merge`'s is for `$4`, the gate's witness of the bind: no answer changes
        // without it, and neither would a lock, since the statement reads what
        // `under:` itself reads.
        debug_assert!(
            naming
                .iter()
                .all(|id| sources_of(&probes).contains(&id.as_str())),
            "the probe was asked about a source with no corpus: {naming:?}"
        );
        report.push(coverage_of(
            &scoped,
            &probes,
            FilterDimension::Under,
            |probe| naming.contains(&probe.source_id),
        ));
    }
    // A dimension whose sources in scope all contributed nothing to this
    // query's corpus has no gap to explain -- only a kind or a source filter
    // that matched nothing, which the results already say.
    report.retain(|coverage| !coverage.sources.is_empty());
    Ok(report)
}

/// The **enabled** configured sources this query is narrowed to, in vocabulary
/// order.
///
/// #200 widened this to include disabled sources, on the premise that their
/// rows stayed searchable and therefore still owed an explanation. **Issue #202
/// overturned that premise**: Björn ruled that a source the user turned off is
/// invisible to every reader, and migration `0012` moved the filter into
/// `sync.live_item` where it belongs. A disabled source now contributes nothing
/// to any corpus, so a verdict on it would explain an absence authorship had
/// nothing to do with -- the same error as verdicting a source the kind scope
/// excluded, and the same reason the short-circuits report nothing for a
/// prefix that names no corpus at all.
///
/// So the population is the one the grammar can name, and the two questions
/// #200 separated have converged on one answer again. The separation itself is
/// kept, in [`Vocabulary::nameable`] and this filter, because they are still
/// *different questions* -- they merely have the same answer now, and a later
/// reader that needs the wider list can have it without another migration.
///
/// A `source:` filter naming something the vocabulary does not have contributes
/// nothing -- the parser already reports such a token as unknown, and inventing
/// a coverage row for it would have the report claim a source exists.
fn in_scope<'a>(vocab: &'a Vocabulary, filters: &EffectiveFilters) -> Vec<&'a SourceVocab> {
    vocab
        .sources
        .iter()
        .filter(|source| source.enabled)
        .filter(|source| filters.sources.is_empty() || filters.sources.contains(&source.id))
        .collect()
}

/// Turn the probes into the report: every scoped source that has a corpus here,
/// in the scope's order.
///
/// Pure, and separate from the round trip, so every verdict is testable without
/// a database. The **scope** drives the order and the names, and the **probe**
/// decides membership: a source with no rows in scope has no verdict to give,
/// because whatever it is missing from the results, authorship is not the
/// reason. Iterating the probes instead would order the report by whatever the
/// grouping happened to return.
fn coverage_of(
    scoped: &[&SourceVocab],
    probes: &[SourceProbe],
    dimension: FilterDimension,
    answers: impl Fn(&SourceProbe) -> bool,
) -> FilterCoverage {
    FilterCoverage {
        dimension,
        sources: scoped
            .iter()
            .filter_map(|source| {
                let probe = probes.iter().find(|probe| probe.source_id == source.id)?;
                Some(SourceAnswer {
                    source_id: source.id.clone(),
                    display_name: source.display_name.clone(),
                    answer: if answers(probe) {
                        FilterAnswer::Answered
                    } else {
                        FilterAnswer::NoValues
                    },
                })
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filters(mine: bool, named: &[&str], sources: &[&str]) -> EffectiveFilters {
        EffectiveFilters {
            sources: sources.iter().map(|s| (*s).to_owned()).collect(),
            mine,
            named_authors: named.iter().map(|s| (*s).to_owned()).collect(),
            ..EffectiveFilters::default()
        }
    }

    fn probe(source_id: &str, authored: bool) -> SourceProbe {
        SourceProbe {
            source_id: source_id.to_owned(),
            authored,
        }
    }

    /// Only a query that asked an author question is reported on -- which is
    /// also what keeps the probe off the ordinary hot path.
    ///
    /// The predicate is `EffectiveFilters`' own, shared with the statement
    /// builder so the two cannot drift; this is coverage's stake in it.
    #[test]
    fn only_an_author_query_is_reported_on() {
        assert!(filters(false, &["jonas"], &[]).filters_by_author());
        // `@me` is the same predicate and the same gap, and it stays an author
        // query even when knobas does not know who the user is.
        assert!(filters(true, &[], &[]).filters_by_author());
        assert!(
            EffectiveFilters {
                mine: true,
                identity_authors: Vec::new(),
                ..EffectiveFilters::default()
            }
            .filters_by_author()
        );
        assert!(!EffectiveFilters::default().filters_by_author());
        assert!(!filters(false, &[], &["jira"]).filters_by_author());
    }

    #[test]
    fn a_source_filter_narrows_the_report_and_no_filter_covers_everything() {
        let vocab = Vocabulary::fixture();
        let all: Vec<&str> = in_scope(&vocab, &filters(false, &["jonas"], &[]))
            .iter()
            .map(|source| source.id.as_str())
            .collect();
        assert_eq!(all, ["gitea", "jira", "jira-eu", "teamcity"]);

        let narrowed: Vec<&str> = in_scope(&vocab, &filters(false, &["jonas"], &["teamcity"]))
            .iter()
            .map(|source| source.id.as_str())
            .collect();
        assert_eq!(narrowed, ["teamcity"]);

        // A source nobody configured is not invented into the report.
        assert!(in_scope(&vocab, &filters(false, &["jonas"], &["nope"])).is_empty());
    }

    /// Every way a walk can come back, written out: which sources get a probe,
    /// and which have to be asked again because the answer may lie past the walk.
    #[test]
    fn a_walk_settles_a_source_only_when_it_saw_what_the_query_needs() {
        fn walk(in_scope: bool, authored: bool, read_every_row: bool) -> NewestRows {
            NewestRows {
                source_id: "s".to_owned(),
                in_scope,
                authored,
                read_every_row,
            }
        }
        let verdict = |authored| (vec![probe("s", authored)], vec![]);
        let no_verdict = || (vec![], vec![]);
        // In the corpus, and whether anything there is authored is still open.
        let authors_open = || (vec![probe("s", false)], vec!["s".to_owned()]);
        // Not seen in the corpus, and the walk stopped before the source did.
        let corpus_open = || (vec![], vec!["s".to_owned()]);

        // An author query needs an authored row, or the whole source.
        assert_eq!(settle(vec![walk(true, true, false)], true), verdict(true));
        assert_eq!(settle(vec![walk(true, true, true)], true), verdict(true));
        assert_eq!(settle(vec![walk(true, false, false)], true), authors_open());
        assert_eq!(settle(vec![walk(true, false, true)], true), verdict(false));
        assert_eq!(settle(vec![walk(false, false, false)], true), corpus_open());
        assert_eq!(settle(vec![walk(false, false, true)], true), no_verdict());

        // `under:` alone needs one row in scope: authors are not its question.
        assert_eq!(settle(vec![walk(true, true, false)], false), verdict(true));
        assert_eq!(
            settle(vec![walk(true, false, false)], false),
            verdict(false)
        );
        assert_eq!(settle(vec![walk(true, false, true)], false), verdict(false));
        assert_eq!(
            settle(vec![walk(false, false, false)], false),
            corpus_open()
        );
        assert_eq!(settle(vec![walk(false, false, true)], false), no_verdict());
    }

    /// Every way a look at each kind can come back, written out. It settles what
    /// it saw, and a source it did not see stays exactly as open as it was.
    #[test]
    fn a_look_at_each_kind_settles_only_the_sources_it_saw_enough_of() {
        let open = |ids: &[&str]| ids.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        let take = |mut probes: Vec<SourceProbe>, seen: Vec<SourceProbe>, by_author: bool| {
            let mut unsettled = open(&["known", "unseen"]);
            take_what_was_seen(&mut probes, &mut unsettled, seen, by_author);
            (probes, unsettled)
        };

        // `under:`: one row in the corpus is the whole question.
        assert_eq!(
            take(vec![], vec![probe("unseen", false)], false),
            (vec![probe("unseen", false)], open(&["known"]))
        );
        // Seen nothing: nothing changes, and the grouped read still has to be asked.
        assert_eq!(
            take(vec![], vec![], false),
            (vec![], open(&["known", "unseen"]))
        );

        // `author:` needs an authored row to stop.
        assert_eq!(
            take(vec![], vec![probe("unseen", true)], true),
            (vec![probe("unseen", true)], open(&["known"]))
        );
        assert_eq!(
            take(vec![], vec![probe("unseen", false)], true),
            (vec![probe("unseen", false)], open(&["known", "unseen"]))
        );
        // A known source gains an author, and never loses one.
        assert_eq!(
            take(vec![probe("known", false)], vec![probe("known", true)], true),
            (vec![probe("known", true)], open(&["unseen"]))
        );
        assert_eq!(
            take(vec![probe("known", false)], vec![probe("known", false)], true),
            (vec![probe("known", false)], open(&["known", "unseen"]))
        );
    }

    #[test]
    fn the_grouped_read_answers_only_for_the_sources_it_names() {
        let mut probes = vec![probe("known-open", false), probe("known-settled", true)];
        merge(
            &mut probes,
            vec![probe("known-open", true), probe("newly-seen", false)],
        );
        assert_eq!(
            probes,
            [
                probe("known-open", true),
                probe("known-settled", true),
                probe("newly-seen", false),
            ]
        );

        // Named by nothing, a source keeps what the walk said.
        let mut probes = vec![probe("known-open", false)];
        merge(&mut probes, vec![]);
        assert_eq!(probes, [probe("known-open", false)]);
    }

    /// Both verdicts, in the scope's order -- and the sources with nothing in
    /// scope left out rather than accused.
    #[test]
    fn a_source_with_nothing_in_scope_gets_no_verdict_at_all() {
        let vocab = Vocabulary::fixture();
        let scoped = in_scope(&vocab, &filters(false, &["jonas"], &[]));
        // Deliberately out of probe order, and two of the four sources
        // deliberately absent: a source with no rows in scope has no group.
        let probes = [probe("teamcity", false), probe("jira", true)];

        let coverage = coverage_of(&scoped, &probes, FilterDimension::Author, |probe| {
            probe.authored
        });
        assert_eq!(coverage.dimension, FilterDimension::Author);
        assert_eq!(
            coverage
                .sources
                .iter()
                .map(|s| (s.source_id.as_str(), s.display_name.as_str(), s.answer))
                .collect::<Vec<_>>(),
            [
                ("jira", "Jira", FilterAnswer::Answered),
                // The one the issue is about: a corpus with nothing in it that
                // names a person, which is not the same as an answer of
                // "nobody by that name".
                ("teamcity", "Buildserver", FilterAnswer::NoValues),
            ],
            "gitea and jira-eu put no rows in this query's corpus, so \
             authorship is not why they are absent from the results"
        );
    }
}

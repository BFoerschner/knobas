//! The exit criterion, as a gate anyone can re-run.
//!
//! Spec §14: *"Local search < 100 ms for ~100 k items"*, and M1's exit
//! criterion (interfaces §6.3): *"< 100 ms measured over a ≥ 100 k-item seeded
//! corpus (a bench, not a claim)"*.
//!
//! Both tests here are `#[ignore]`d. Seeding 100 k rows costs tens of seconds,
//! and a timing assertion on a shared CI runner is a coin flip rather than a
//! signal -- so they are run deliberately:
//!
//! ```text
//! cargo test -p knobas-search --test perf -- --ignored --nocapture
//! ```
//!
//! # Read the method before the numbers
//!
//! **`ANALYZE` is not enough for a GIN benchmark; the index must be
//! `VACUUM`ed** -- [`knobas_search::testing::seed_corpus`] owns that, and its
//! module docs carry the measured reason. The severity of skipping it is
//! **not** a trend: fresh single loads in this stream's record inflated 182×
//! at 12,800 rows and 86.5× at 300,000, with 58.8× and 24.2× in between. There
//! is no size exemption; a large fixture that looks sane is the trap.
//!
//! [`the_artifact_this_harness_is_built_to_avoid`] measures that directly on
//! *this* fixture rather than citing the record, so the harness's own
//! preparation step is a thing with evidence behind it and not a comment.
//!
//! # A curve, not a point
//!
//! [`search_is_under_a_hundred_milliseconds_over_a_hundred_thousand_items`]
//! seeds three sizes and prints all three, because flat-per-unit against
//! growing is what distinguishes a real cost from an artifact -- and because
//! every claim this stream has had to retract was a single point. The
//! assertion is on the 100 k reading, which is what the exit criterion names.
//!
//! # Fixture, stated
//!
//! Deterministic, no RNG. Five kinds, three sources, three authors, a
//! two-word title and a **20-word body** per row drawn from a **512-word**
//! filler space, with the domain markers placed on top at declared
//! frequencies. Text richness is the variable that decides the headline (this
//! stream measured 57 ms and 174 ms for one statement on two fixtures
//! differing only in richness), so it is stated here rather than left to be
//! inferred -- and [`knobas_search::testing`] is where the frequencies live,
//! measured by `match_count` rather than asserted from this comment.
//!
//! This paragraph described a **25-word body from a 20-word vocabulary** until
//! the rebase that opened the PR. That was the *rejected* generator -- the one
//! on which `tombstone` ("rare word") matched 60,000 rows of 100,000. The
//! fixture had been replaced and its description had not, in the one file
//! whose stated purpose is to say what the fixture is.
//!
//! ## The rail carries the saved-list cap (#533)
//!
//! `knobas.smart_list` was **empty in this fixture** when #506 added it: with
//! nothing saved, `saved::summaries` returns on its first read, so the
//! `smart_lists` assertion below covered the **built-in half of the rail and
//! nothing else**. That is not the budget the feature is held to -- spec #491
//! story 57 says a saved list shows its count *"like the built-ins"*, on the
//! same ⌘K, so the saved half is inside the same 100 ms.
//!
//! That gap was of its own kind and is kept named as one: not a criterion that
//! cannot pass and not a witness blind in one direction, but **a budget whose
//! fixture stopped covering what the budget names** -- the same failure the
//! paragraph above records about the corpus, in a different place.
//!
//! #533 closes it, and the fixture now says so. [`fill_the_rail`] saves
//! [`saved::MAX_SAVED_LISTS`] lists through `saved::create`, cycling [`CASES`]'
//! shapes, so every branch of the `union all` that
//! `knobas_search::sql::saved_summary_sql` builds is a query the launcher
//! actually runs. Three readings of the board are taken at every corpus size:
//!
//! * `smart_lists` over an **empty** rail -- the built-in half on its own, and
//!   exactly the reading this file took before #533;
//! * `smart_lists` over a rail carrying **the cap** -- what ⌘K pays, and
//!   what [`BUDGET_MS`] gates;
//! * `launcher_board`, which *calls* `smart_lists`, so it pays for the rail
//!   too and its assertion tightened with them.
//!
//! The two `smart_lists` readings are printed side by side on the curve rather
//! than one number for the pair, because a rail that has grown expensive and a
//! board that has is the same number until they are told apart.
//!
//! ## The cap is a measured number, not a generous one
//!
//! `saved::MAX_SAVED_LISTS` was added beyond #506's ticket and ratified by the
//! deputy's ruling of 2026-09-08 as **a bound rather than a feature**: a board
//! whose cost grows with a number nobody bounds is the thing the budget exists
//! to refuse. So the cap is not asserted about here -- it is *read off* a
//! curve. [`rail_steps`] walks the rail from empty to the cap at 100 k items,
//! prints `smart_lists` at each step and the marginal cost of one saved list,
//! and the gate then holds the cap's own reading to [`BUDGET_MS`]. A cap this
//! fixture cannot carry inside the budget is a cap that comes down; the budget
//! does not move.

use std::time::Instant;

use knobas_search::corpus::LIVE_ITEM;
use knobas_search::query::EffectiveFilters;
use knobas_search::{
    KindCatalog, SearchFilters, SearchQuery, Searcher, SmartListSummary, Vocabulary, saved, sql,
    testing,
};

/// The budget, from spec §14.
const BUDGET_MS: u128 = 100;

/// The exit criterion's corpus size, and the two smaller points of the curve.
const SIZES: [i64; 3] = [25_000, 50_000, 100_000];

/// How many runs make one reading of the board's reads, and one rail step.
///
/// Ten, as for a launcher case: `⌘K` runs the board on every opening, so it is
/// a keystroke like any other and gets a keystroke's percentiles. Ten is also
/// the smallest count at which nearest-rank p90 is the **second**-worst sample
/// rather than the worst -- below it, `rank` collapses p90 onto `max` and the
/// two printed columns stop being two readings.
const BOARD_RUNS: usize = 10;

/// The launcher's shapes of query, each named for what it exercises.
///
/// **Every name here is a claim about selectivity, and the test prints the
/// measured match count next to it** -- because the plan's original case list
/// was written against a 20-word fixture on which "rare word" (`tombstone`)
/// matched 60,000 of 100,000 rows and "common word" (`payout`) matched 40,000.
/// The rare case was the commonest one on the list and nothing said so.
const CASES: &[(&str, &str)] = &[
    ("common word", "payout"),
    ("two words", "sepa retry"),
    ("phrase", "\"sepa retry\""),
    ("still typing", "pay"),
    ("rare word", "tombstone"),
    ("filtered", "/ji payout"),
    ("kind + text", "kind:pr backoff"),
    ("browse, no text", "/tc"),
    ("mine", "@me settlement"),
    ("no match", "zzzznothing"),
];

/// The case the budget cannot cover, measured and printed and **not gated**.
///
/// `ledger` is in every row of the fixture, so this is a "search" that matches
/// the whole corpus. It is here so that changing the fixture to one whose
/// queries are realistic cannot be mistaken for -- or quietly become -- tuning
/// a fixture until it passes: the pathological number is still taken, still
/// printed, and the per-match cost derived from it is what the report quotes.
const PATHOLOGICAL: (&str, &str) = ("matches everything", "ledger");

fn q(raw: &str) -> SearchQuery {
    SearchQuery {
        raw: raw.to_owned(),
        limit: 40,
        filters: SearchFilters::default(),
    }
}

async fn pool() -> sqlx::PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    pool
}

/// What one case measured.
#[derive(Debug, Clone, Copy)]
struct Timing {
    p50: u128,
    p90: u128,
    max: u128,
}

/// One row of the printed curve: what to call it, and which reading it draws.
type BoardRow = (String, fn(&Point) -> Timing);

/// One corpus size's whole reading.
struct Point {
    size: i64,
    cases: Vec<(&'static str, Timing)>,
    /// `launcher_board`, which calls `smart_lists` and so carries the rail.
    board: Timing,
    /// `smart_lists` with `knobas.smart_list` empty: the built-in half alone.
    builtin: Timing,
    /// `smart_lists` with [`saved::MAX_SAVED_LISTS`] lists on the rail.
    saved: Timing,
    /// How many rows the rail's saved lists counted between them, so that a
    /// reading over a rail that matched nothing cannot be quoted as a reading
    /// over a rail that worked.
    counted: i64,
}

/// p50, p90 and max of `runs` timings of one query, in milliseconds.
///
/// Nearest-rank percentiles: with ten samples p90 is the ninth, which is the
/// second-worst. A mean would hide exactly the case the budget is about -- one
/// keystroke in ten taking half a second is a launcher that feels broken,
/// however good the average is.
async fn timings(searcher: &Searcher, raw: &str, runs: usize) -> Timing {
    let mut samples: Vec<u128> = Vec::with_capacity(runs);
    for _ in 0..runs {
        let started = Instant::now();
        searcher.search(q(raw)).await.expect("the query answers");
        samples.push(started.elapsed().as_millis());
    }
    summarize(&mut samples)
}

/// The same percentiles over any read of the box, after one warm call.
///
/// The board's two reads are timed through this rather than taken once each,
/// which is what this file did until #533. A single sample was defensible
/// while `smart_lists` answered in a millisecond or two off an empty
/// `knobas.smart_list`; it is not once the rail carries the cap, and the
/// argument on [`timings`] -- that a mean, or a lone sample, hides exactly the
/// keystroke the budget is about -- applies to the board unchanged.
///
/// **The warm call is discarded and is not the budget.** The launcher is open
/// for a whole session, and the first call after the rail changes shape is the
/// one that assembles a statement of a size this process has not seen.
async fn timings_of<F, Fut, T, E>(runs: usize, mut once: F) -> Timing
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    E: std::fmt::Debug,
{
    once().await.expect("the read answers");
    let mut samples: Vec<u128> = Vec::with_capacity(runs);
    for _ in 0..runs {
        let started = Instant::now();
        once().await.expect("the read answers");
        samples.push(started.elapsed().as_millis());
    }
    summarize(&mut samples)
}

/// Nearest-rank p50, p90 and max of a set of samples, which this sorts.
fn summarize(samples: &mut [u128]) -> Timing {
    samples.sort_unstable();
    Timing {
        p50: samples[rank(samples.len(), 50)],
        p90: samples[rank(samples.len(), 90)],
        max: samples[samples.len() - 1],
    }
}

/// The index of the nearest-rank `p`th percentile of `n` sorted samples.
fn rank(n: usize, p: usize) -> usize {
    ((n * p).div_ceil(100)).clamp(1, n) - 1
}

/// The search text a case's raw query reduces to, for the match count.
///
/// The grammar markers are the parser's business, and `match_count` takes plain
/// text -- so a case whose whole query is a filter (`/tc`) has no text and
/// matches nothing by this measure, which is reported as `0` rather than
/// guessed at.
fn text_of(raw: &str) -> String {
    raw.split_whitespace()
        .filter(|token| !token.starts_with(['/', '@', '#']) && !token.contains(':'))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The saved-list counts the rail is measured at, ending at the cap.
///
/// Quartered rather than halved, so five points span two orders of magnitude:
/// what the curve has to show is whether the rail's cost is **linear in the
/// number of lists** -- the shape that makes a cap the right instrument at all
/// -- and a linear reading wants a wide base rather than a dense one. The last
/// step is always the cap itself, whatever the cap is, because that is the
/// point the gate asserts on.
fn rail_steps(cap: i64) -> Vec<i64> {
    let mut steps = vec![0];
    let mut count = 1;
    while count < cap {
        steps.push(count);
        count *= 4;
    }
    steps.push(cap);
    steps.dedup();
    steps
}

/// Empty `knobas.smart_list`, one list at a time through `saved::delete`.
async fn clear_the_rail(pool: &sqlx::PgPool) {
    for list in saved::all(pool).await.expect("the rail reads") {
        saved::delete(pool, &list.id)
            .await
            .expect("a saved list deletes");
    }
}

/// Save `count` lists, cycling [`CASES`]' shapes.
///
/// **Through `saved::create` and not an insert**, which is what the ticket asks
/// for and is load-bearing twice over. `create` runs `saved::plan` and refuses
/// a query today's grammar cannot answer, so a rail built this way cannot hold
/// a row that the counting statement leaves out; and the ids it generates are
/// the ones `list:` names, so the fixture is a rail a reader could have built.
///
/// Cycling [`CASES`] rather than repeating one query is what puts every branch
/// of the `union all` under the clock: a rail of sixty-four copies of
/// `zzzznothing` is sixty-four aggregates over an empty match set, and would
/// measure the statement's shape rather than its work.
async fn fill_the_rail(pool: &sqlx::PgPool, count: i64) {
    let vocab = Vocabulary::load(pool, KindCatalog::default())
        .await
        .expect("the vocabulary loads");
    for index in 0..count {
        let position = usize::try_from(index).expect("a rail smaller than a machine word");
        let (name, raw) = CASES[position % CASES.len()];
        saved::create(pool, &vocab, &format!("{name} {index}"), raw)
            .await
            .unwrap_or_else(|error| {
                panic!("saving {raw:?} as a list, which the fixture needs to run: {error}")
            });
    }
}

/// Read the rail back and prove it is the rail the timings are about.
///
/// This is the guard against the one way this measurement could report a
/// number about nothing. `saved::summaries` leaves a **refused** list out of
/// `saved_summary_sql` entirely -- that is what makes one broken row cheap
/// rather than fatal -- so a rail of rows today's grammar will not run costs
/// the board a single small select and times exactly like the empty rail this
/// ticket exists to stop measuring. `create` cannot produce such a row, and
/// this says so out of the summaries rather than trusting it.
///
/// Answers how many rows the rail counted between them, which the caller
/// prints: a rail whose every list matched nothing is a second way to time an
/// aggregate over nothing.
async fn assert_the_rail_is_counted(searcher: &Searcher, expected: i64) -> i64 {
    let rail = searcher.smart_lists().await.expect("the rail reads");
    let saved: Vec<&SmartListSummary> = rail.iter().filter(|list| list.saved).collect();
    assert_eq!(
        i64::try_from(saved.len()).expect("a rail smaller than a machine word"),
        expected,
        "the fixture saved {expected} lists and the rail draws {}",
        saved.len()
    );
    if expected == 0 {
        return 0;
    }
    let refused: Vec<&str> = saved
        .iter()
        .filter(|list| list.needs_attention)
        .map(|list| list.description.as_str())
        .collect();
    assert!(
        refused.is_empty(),
        "{} of the fixture's saved lists are refused, so they are not in the \
         counting statement at all and this times an emptier rail than it \
         claims: {refused:?}",
        refused.len()
    );
    let counted: i64 = saved.iter().map(|list| list.count).sum();
    assert!(
        counted > 0,
        "every saved list on the rail counted zero rows, so the union \
         aggregated over nothing and the reading is not of the rail"
    );
    counted
}

/// The gate: every launcher query under 100 ms at 100 k items, and the board
/// with the saved-list cap on its rail.
///
/// # What the rail contributes (#533)
///
/// The board is measured in both of the rail's states at every size -- empty,
/// which is the built-in half alone, and carrying [`saved::MAX_SAVED_LISTS`]
/// lists -- and the second is what the budget is about, because story 57 puts
/// a saved list's count on the same ⌘K as a built-in's. After the size loop
/// the rail is walked from empty to the cap at the criterion's size, which is
/// the reading the constant in `saved.rs` is chosen from. The module docs
/// carry the reasons and the record.
///
/// # The harness shape, recorded because it is load-bearing
///
/// **Cumulative**: 25 k, then +25 k, then +50 k, with a `vacuum (analyze)`
/// between every step (`seed_corpus` does it). That shape is stated because
/// this stream has one measurement that disagreed with another by 2.8× for no
/// reason but harness shape, and a table without the column could not say so.
///
/// Cumulative is the *right* shape for a budget check: every step is vacuumed,
/// so every reading is of the merged index rather than of a pending list, and
/// the three points are directly comparable to each other. A fresh single load
/// per size would be more honest about a cold start and is what
/// [`the_artifact_this_harness_is_built_to_avoid`] measures instead.
#[tokio::test]
#[ignore = "seeds 100k rows and asserts on wall-clock time; run it deliberately"]
async fn search_is_under_a_hundred_milliseconds_over_a_hundred_thousand_items() {
    let pool = pool().await;
    testing::seed_sources(&pool).await.unwrap();
    let searcher = Searcher::new(pool.clone());

    let mut curve: Vec<Point> = Vec::new();

    for size in SIZES {
        testing::seed_corpus(&pool, size).await.unwrap();

        // Warm the caches. Not part of the measurement: the launcher is open
        // for a whole session and the first keystroke is not the budget.
        for _ in 0..3 {
            searcher.search(q("payout")).await.unwrap();
        }

        let mut cases = Vec::new();
        for (name, raw) in CASES {
            let timing = timings(&searcher, raw, 10).await;
            let matches = testing::match_count(&pool, &text_of(raw)).await.unwrap();
            println!(
                "{size:>7} {name:<18} {matches:>7} matched  p50 {:>4} ms  p90 {:>4} ms  max {:>4} ms",
                timing.p50, timing.p90, timing.max
            );
            cases.push((*name, timing));
        }
        // The board's reads, in both of the rail's states. Empty first, which
        // is the built-in half on its own and the only reading this file took
        // before #533; then the cap, which is what ⌘K pays.
        clear_the_rail(&pool).await;
        let builtin = timings_of(BOARD_RUNS, || searcher.smart_lists()).await;
        fill_the_rail(&pool, saved::MAX_SAVED_LISTS).await;
        let counted = assert_the_rail_is_counted(&searcher, saved::MAX_SAVED_LISTS).await;
        let saved_reading = timings_of(BOARD_RUNS, || searcher.smart_lists()).await;
        // Last, and with the rail full: `launcher_board` calls `smart_lists`,
        // so this number contains the one above it.
        let board = timings_of(BOARD_RUNS, || searcher.launcher_board()).await;
        for (label, timing) in [
            ("smart_lists, empty rail", builtin),
            ("smart_lists, rail at cap", saved_reading),
            ("launcher_board", board),
        ] {
            println!(
                "{size:>7} {label:<26} p50 {:>4} ms  p90 {:>4} ms  max {:>4} ms",
                timing.p50, timing.p90, timing.max
            );
        }
        println!(
            "{size:>7} {:<26} {} lists, {counted} rows counted",
            "  the rail, as filled",
            saved::MAX_SAVED_LISTS
        );
        curve.push(Point {
            size,
            cases,
            board,
            builtin,
            saved: saved_reading,
            counted,
        });
        println!();
    }

    println!("--- the curve, p90 in ms (harness: cumulative, vacuumed per step) ---");
    print!("{:<16}", "case");
    for point in &curve {
        print!("{:>9}", point.size);
    }
    println!();
    for (index, (name, _)) in curve[0].cases.iter().enumerate() {
        print!("{name:<16}");
        for point in &curve {
            print!("{:>9}", point.cases[index].1.p90);
        }
        println!();
    }
    let board_rows: [BoardRow; 3] = [
        ("lists, empty rail".to_owned(), |point| point.builtin),
        (
            format!("lists, {} saved", saved::MAX_SAVED_LISTS),
            |point| point.saved,
        ),
        ("launcher_board".to_owned(), |point| point.board),
    ];
    for (label, pick) in board_rows {
        print!("{label:<16}");
        for point in &curve {
            print!("{:>9}", pick(point).p90);
        }
        println!();
    }

    // --- what the cap is worth ---
    //
    // `saved::MAX_SAVED_LISTS` is a **bound**, ratified as one: the board's
    // cost grows with the number of lists on the rail, and a number nobody
    // bounds is what the budget exists to refuse. This is the reading behind
    // it -- the same `smart_lists` the gate takes, at the size the criterion
    // names, over rails from empty to the cap -- so that the constant is read
    // off a curve rather than chosen for being generous.
    let at_size = curve.last().expect("at least one size").size;
    println!("--- the rail at {at_size} items: smart_lists by saved-list count ---");
    let mut rail: Vec<(i64, Timing)> = Vec::new();
    for step in rail_steps(saved::MAX_SAVED_LISTS) {
        clear_the_rail(&pool).await;
        fill_the_rail(&pool, step).await;
        let counted = assert_the_rail_is_counted(&searcher, step).await;
        let timing = timings_of(BOARD_RUNS, || searcher.smart_lists()).await;
        println!(
            "{step:>4} saved {counted:>8} rows counted  p50 {:>5} ms  p90 {:>5} ms  max {:>5} ms",
            timing.p50, timing.p90, timing.max
        );
        rail.push((step, timing));
    }
    let empty = rail.first().expect("a rail curve").1.p90;
    let (steps, at_cap) = *rail.last().expect("a rail curve");
    let over_empty = at_cap.p90.saturating_sub(empty);
    println!(
        "  => {steps} saved lists cost {over_empty} ms over the empty rail's {empty} ms, \
         about {:.2} ms each",
        over_empty as f64 / f64::from(u32::try_from(steps.max(1)).unwrap_or(u32::MAX))
    );
    println!();

    // The pathological case, printed and not gated. See `PATHOLOGICAL`.
    {
        let (name, raw) = PATHOLOGICAL;
        let timing = timings(&searcher, raw, 10).await;
        let matches = testing::match_count(&pool, raw).await.unwrap();
        let per_match = (timing.p90 as f64 * 1000.0) / matches.max(1) as f64;
        println!(
            "\n{name}: {matches} of 100000 rows matched, p90 {} ms ({per_match:.1} us per match)",
            timing.p90
        );
        println!(
            "  => the {BUDGET_MS} ms budget buys about {:.0} matching rows on this fixture",
            BUDGET_MS as f64 * 1000.0 / per_match.max(0.001)
        );
    }

    // The assertion is on the size the exit criterion names. The smaller
    // points are printed for the shape, not gated -- a budget that a 25 k
    // corpus had to meet would be a different, stricter claim than §14 makes.
    let last = curve.last().expect("at least one size");
    assert_eq!(
        last.size, 100_000,
        "the gate must end at the criterion's size"
    );
    let mut worst = 0_u128;
    for (name, timing) in &last.cases {
        assert!(
            timing.p90 < BUDGET_MS,
            "{name} took {} ms at p90 over {} items (budget {BUDGET_MS} ms)",
            timing.p90,
            last.size
        );
        worst = worst.max(timing.p90);
    }
    // The other two hot paths of the box, held to the same budget: `⌘K` on an
    // empty query runs both, so a slow board is a slow launcher.
    //
    // Three assertions rather than two, because the rail has two states and a
    // single number cannot say which of them went wrong. The empty-rail
    // reading is the claim this file has always made about the built-ins; the
    // rail-at-cap reading is the one #533 adds and is the one story 57 asks
    // for; and `launcher_board` contains the second, so it tightened with it.
    assert!(
        last.builtin.p90 < BUDGET_MS,
        "smart_lists took {} ms at p90 over an empty rail and {} items",
        last.builtin.p90,
        last.size
    );
    assert!(
        last.saved.p90 < BUDGET_MS,
        "smart_lists took {} ms at p90 with {} saved lists on the rail ({} rows \
         counted) over {} items. The budget does not move: the fix is a lower \
         `saved::MAX_SAVED_LISTS`, and the rail curve above says to what.",
        last.saved.p90,
        saved::MAX_SAVED_LISTS,
        last.counted,
        last.size
    );
    assert!(
        last.board.p90 < BUDGET_MS,
        "launcher_board took {} ms at p90 over {} items",
        last.board.p90,
        last.size
    );
    // And the curve's own last point, taken after the size loop against a rail
    // built up from empty: the same claim as the one above, over the fixture
    // the cap was read off, so a cap that survives only the loop's fixture
    // cannot be quoted as measured.
    assert_eq!(
        steps,
        saved::MAX_SAVED_LISTS,
        "the rail curve must end at the cap"
    );
    assert!(
        at_cap.p90 < BUDGET_MS,
        "smart_lists took {} ms at p90 at the cap of {steps} saved lists over \
         {at_size} items (budget {BUDGET_MS} ms)",
        at_cap.p90
    );
    println!("worst p90: {worst} ms at {} items", last.size);
}

/// The harness's own preparation step, shown to do something.
///
/// **This test is the mutation of the detector.** `seed_corpus` vacuums, and a
/// benchmark whose fixture preparation is only a comment is one that will one
/// day be run without it. So this creates the state the vacuum exists to
/// clear, proves the state exists, clears it, and proves it is gone.
///
/// # What is asserted, and what is only reported
///
/// **Asserted: the pending list.** Before the vacuum it is non-empty (a
/// positive control -- without it, "clean afterwards" would be equally
/// consistent with a probe that never saw anything); afterwards it is empty.
/// That is the mechanism, read straight out of `pgstatginindex`, and it is
/// stable.
///
/// **Reported, never asserted: the ratio.** Its severity is governed by the
/// increment since the last merge rather than by corpus size, so it is
/// unstable by nature -- this stream measured 182x, 58.8x, 24.2x and 86.5x on
/// fresh loads across four sizes, which is not a trend and cannot be a
/// threshold. An earlier draft of this test asserted `dirty > clean` on the
/// *timing* and passed at 1.5x with the pending list already auto-merged: it
/// would have reported "the vacuum matters" on a run where the vacuum had
/// nothing to do. Asserting the pending list instead removes that whole class
/// of false pass.
#[tokio::test]
#[ignore = "seeds a 100k corpus and dirties its index; run it deliberately"]
async fn the_artifact_this_harness_is_built_to_avoid() {
    let pool = pool().await;
    testing::seed_sources(&pool).await.unwrap();
    testing::seed_corpus(&pool, 100_000).await.unwrap();
    let searcher = Searcher::new(pool.clone());

    // `seed_corpus` ends with the vacuum, so this is the clean baseline the
    // gate above measures against.
    let (clean_pages, clean_tuples) = testing::pending_list(&pool).await.unwrap();
    assert_eq!(
        (clean_pages, clean_tuples),
        (0, 0),
        "seed_corpus is supposed to leave the index merged"
    );

    // Now the state the plan's original `analyze`-only preparation would have
    // measured in.
    testing::dirty_the_pending_list(&pool, 20_000)
        .await
        .unwrap();
    let (dirty_pages, dirty_tuples) = testing::pending_list(&pool).await.unwrap();
    assert!(
        dirty_pages > 0,
        "nothing landed in the pending list, so this test proves nothing about \
         the vacuum: {dirty_pages} pages, {dirty_tuples} tuples"
    );

    for _ in 0..2 {
        searcher.search(q("payout")).await.unwrap();
    }
    let dirty = timings(&searcher, "payout", 5).await.p90;

    testing::prepare(&pool).await.unwrap();
    let (merged_pages, merged_tuples) = testing::pending_list(&pool).await.unwrap();
    for _ in 0..2 {
        searcher.search(q("payout")).await.unwrap();
    }
    let clean = timings(&searcher, "payout", 5).await.p90;

    println!("pending before vacuum : {dirty_pages} pages, {dirty_tuples} tuples -> {dirty} ms");
    println!("pending after  vacuum : {merged_pages} pages, {merged_tuples} tuples -> {clean} ms");
    println!(
        "ratio (reported, never asserted -- see the doc comment): {:.1}x",
        dirty as f64 / clean.max(1) as f64
    );

    assert_eq!(
        (merged_pages, merged_tuples),
        (0, 0),
        "the vacuum did not merge the pending list, so `prepare` is not doing \
         the one thing the whole fixture rests on"
    );
    assert!(
        clean < BUDGET_MS,
        "even vacuumed, a common word took {clean} ms over 100k items"
    );
}

/// The two plan defects task 10 found, pinned so they cannot come back.
///
/// Both were invisible to every functional test in this crate -- the rows are
/// identical either way -- and both cost about 2.5x on a 100 k corpus. They are
/// pinned here rather than in `tests/sql_shape.rs` because neither is
/// observable at all on a small corpus: with a few thousand rows PostgreSQL
/// picks a hash join whatever it believes about selectivity, so an assertion
/// there would pin nothing. That is the same lesson `tests/home.rs` learned
/// about plan assertions in review round 2.
///
/// **1. `q` must be `not materialized`.** A materialised CTE is opaque to the
/// planner, so `fts @@ q.tsq` falls back to a default selectivity guess, the
/// planner believes the match set is tiny, and it joins `knobas.entity` with a
/// nested loop -- one `entity_pkey` probe per matching row. Measured: 40,000
/// probes, 167,554 buffers, 265 ms, against 8,386 buffers and 106 ms once the
/// CTE is inlined and the join becomes a hash join.
///
/// **2. The statement must not be a cached prepared statement.** See
/// `sql::query_as_with`. That half cannot be seen in a plan, so it is pinned by
/// [`the_plan_does_not_decay_after_the_fifth_execution`] instead.
///
/// The observable here is **buffers**, not the plan node. A per-match nested
/// loop cannot touch fewer than about four buffers per matching row; a hash
/// join over this corpus touches a fifth of one. Asserting "the text contains
/// `Hash Join`" would pin the planner's current choice rather than the cost of
/// getting it wrong, and would fail on a future PostgreSQL that found a third,
/// better plan.
#[tokio::test]
#[ignore = "needs a corpus large enough for the planner's choice to be real"]
async fn the_match_set_is_not_joined_row_by_row() {
    let pool = pool().await;
    testing::seed_sources(&pool).await.unwrap();
    testing::seed_corpus(&pool, 100_000).await.unwrap();

    let built = sql::search_sql(
        &[&LIVE_ITEM],
        Some("ledger"),
        false,
        &EffectiveFilters::default(),
        10,
        40,
    );
    let plan = sql::explain(&pool, built).await.unwrap();
    println!("{plan}");

    let matches = testing::match_count(&pool, "ledger").await.unwrap();
    assert_eq!(matches, 100_000, "`ledger` is in every row of the fixture");

    let buffers = total_buffers(&plan);
    assert!(
        buffers > 0,
        "no buffer counts in the plan, so this test measured nothing:\n{plan}"
    );
    assert!(
        buffers < matches,
        "the statement touched {buffers} buffers for {matches} matches -- about \
         {:.1} per matching row. A hash join costs a fifth of one; four or more \
         means the tombstone join went back to a nested loop, which is what \
         happens when `q` is materialised and the planner cannot see the \
         tsquery.\n{plan}",
        buffers as f64 / matches as f64
    );
}

/// The plan does not decay once PostgreSQL stops custom-planning.
///
/// A named prepared statement is custom-planned for five executions and then
/// may go generic -- and a generic plan cannot know the tsquery, so it
/// mis-estimates and reverts to the per-row join above. This is the shape of
/// bug that a test running a query three times cannot see: it appears on the
/// **sixth** keystroke of a session.
///
/// Measured before the fix, twenty consecutive searches for one word:
/// 125, 123, 124, 123, 128, **185**, 187, 185, 196, 191, 230, 284, 257, 347,
/// 502, 621, ... The step at the sixth is the switch.
///
/// The assertion is a *ratio between halves of one run*, not an absolute
/// time -- what matters is that the tenth search is not slower than the first,
/// whatever the machine.
///
/// # This test is the reading, not the pin -- and it failed green once
///
/// Mutating `.persistent(false)` away before the PR, on a machine carrying
/// another heavy build, produced 550, 493, 563, 499, 492, **725**, 691, 677,
/// 684, ... : the step at the sixth execution, exactly as documented. **And
/// this test passed.** Every timing had quadrupled against the original
/// reading, so the fixed costs dominated and the ratio came out at 1.32x
/// against the 2x threshold below. The threshold is not wrong; a wall-clock
/// ratio is simply a *representation* of which plan ran, and the two diverge
/// with load in the direction that fails green.
///
/// The deterministic pin is
/// `tests/sql_shape.rs::the_launchers_statement_is_never_a_named_prepared_statement`,
/// which reads `pg_prepared_statements` -- the thing itself, with no clock and
/// no corpus in the path. This test keeps its threshold at 2x, where it is a
/// coarse backstop for a gross regression and the source of the numbers the
/// record quotes. Tightening it to 1.2x would only calibrate it to one loaded
/// machine, which is the same mistake in the other direction.
#[tokio::test]
#[ignore = "seeds 100k rows and compares wall-clock halves; run it deliberately"]
async fn the_plan_does_not_decay_after_the_fifth_execution() {
    let pool = pool().await;
    testing::seed_sources(&pool).await.unwrap();
    testing::seed_corpus(&pool, 100_000).await.unwrap();
    let searcher = Searcher::new(pool.clone());

    let mut runs: Vec<u128> = Vec::new();
    for _ in 0..20 {
        let started = Instant::now();
        searcher.search(q("ledger")).await.unwrap();
        runs.push(started.elapsed().as_millis());
    }
    println!("twenty consecutive searches (ms): {runs:?}");

    let early: u128 = runs[..5].iter().sum::<u128>() / 5;
    let late: u128 = runs[15..].iter().sum::<u128>() / 5;
    assert!(
        late < early * 2,
        "the last five searches averaged {late} ms against the first five's \
         {early} ms. PostgreSQL switches a named prepared statement to a \
         generic plan after the fifth execution, and a generic plan cannot know \
         the tsquery -- see `sql::query_as_with`, which asks for the unnamed \
         statement precisely to stop this.\nruns: {runs:?}"
    );
}

/// The largest `Buffers: shared hit=N` in an `explain (buffers)` output.
///
/// A plan node's count already includes its children's, so the largest is the
/// whole statement's -- taking the maximum rather than the sum is what avoids
/// counting the same buffers once per level of the tree. `0` when the plan
/// carries no buffer line at all, which the caller asserts against separately:
/// a zero here would otherwise satisfy `buffers < matches` on a run that
/// measured nothing.
fn total_buffers(plan: &str) -> i64 {
    plan.lines()
        .filter_map(|line| line.split("shared hit=").nth(1))
        .filter_map(|rest| {
            rest.split(|c: char| !c.is_ascii_digit())
                .next()
                .and_then(|n| n.parse::<i64>().ok())
        })
        .max()
        .unwrap_or(0)
}

/// The launcher's estate queries, over an estate beside the 100 k mirror
/// (#436).
///
/// # Why this is a second test rather than four more cases above
///
/// The gate above measures the mirror's queries with `corpus::ASSET` and
/// `corpus::ROUTE` in the union and **nothing in either table** -- which is
/// three of the four branches measured at zero rows, and no measurement at all
/// of the thing #436 ships. The two fixtures are also different jobs: that one
/// is a curve over corpus size against spec §14's *"~100 k items"*, this one is
/// one point over the estate at a size no installation will exceed.
///
/// # What is measured, and what the criterion is
///
/// #436's acceptance criterion is *"the sub-100 ms budget still holds on the
/// **demo** corpus"*, and the demo carries no assets until #440 loads the
/// estate file into it. So this measures the thing the demo will be: a seeded
/// estate ([`testing::seed_estate`], whose shape and reasons are on that
/// function) **on top of** the 100 k mirror, because what a keystroke costs is
/// the union of the four corpora and not the estate on its own.
///
/// The cases are the ones this ticket adds a reader:
///
/// * a container **by name**, the ticket's own sentence;
/// * a **hostname property**, which is `0019`'s weight-C rung;
/// * a **port**, which on the estate's own URL shape is a lexeme of its own;
/// * a **site name**, the spec §4 case -- one query whose weight-B path match
///   is every machine under it, so it is the widest of the four by construction;
/// * `asset:`, the prefix this ticket turns on, which pins the union to the
///   estate;
/// * and the same mirror query the gate above runs, unfiltered, so the two
///   numbers are comparable and a regression can be attributed.
///
/// # The reading, 2026-09-06 (#436)
///
/// Recorded here rather than only in the PR that took it, for the reason
/// `coverage.rs`' own table is: an `#[ignore]`d benchmark whose result lives
/// in a review thread is a claim the next reader has to re-earn before they can
/// tell a regression from a slow machine.
///
/// ```text
/// estate: 3303 assets, 1500 routes, beside 100000 mirror items
/// container by name       2 drawn  p50    1 ms  p90    2 ms  max    2 ms
/// hostname property       1 drawn  p50    1 ms  p90    1 ms  max    1 ms
/// route port              1 drawn  p50    1 ms  p90    1 ms  max    1 ms
/// site name, wide        10 drawn  p50    8 ms  p90    9 ms  max    9 ms
/// asset: prefix           1 drawn  p50    1 ms  p90    1 ms  max    1 ms
/// mirror, for scale      10 drawn  p50   47 ms  p90   49 ms  max   49 ms
/// ```
///
/// The estate costs **single-digit milliseconds** and the mirror is the whole
/// budget: `site name, wide` at 9 ms is the widest of the four estate cases by
/// construction (its weight-B path match is every machine under the site), and
/// it is still five times inside the budget. The same run's mirror gate,
/// unchanged by the union growing to four corpora, was worst p90 **77 ms at
/// 100,000 items**.
#[tokio::test]
#[ignore = "seeds a 100k corpus and an estate and asserts on wall-clock time; run it deliberately"]
async fn the_estate_is_under_the_same_budget_beside_a_hundred_thousand_items() {
    let pool = pool().await;
    testing::seed_sources(&pool).await.unwrap();
    testing::seed_corpus(&pool, 100_000).await.unwrap();
    testing::seed_estate(&pool, "bench").await.unwrap();
    let searcher = Searcher::new(pool.clone());

    let (assets, routes) = testing::estate_size();
    println!("estate: {assets} assets, {routes} routes, beside 100000 mirror items");

    for _ in 0..3 {
        searcher.search(q("payout")).await.unwrap();
    }

    let cases: &[(&str, &str)] = &[
        ("container by name", "container-3300"),
        ("hostname property", "host-3300"),
        ("route port", "33300"),
        ("site name, wide", "site-1"),
        ("asset: prefix", "asset: container-3300"),
        ("mirror, for scale", "payout"),
    ];

    let mut worst = 0_u128;
    for (name, raw) in cases {
        let timing = timings(&searcher, raw, 10).await;
        let hits: usize = searcher
            .search(q(raw))
            .await
            .unwrap()
            .groups
            .iter()
            .map(|group| group.hits.len())
            .sum();
        println!(
            "{name:<20} {hits:>4} drawn  p50 {:>4} ms  p90 {:>4} ms  max {:>4} ms",
            timing.p50, timing.p90, timing.max
        );
        // **Asserted, not merely printed.** Every case above names a row of
        // `seed_estate`'s fixture by arithmetic on `ESTATE_SHAPE`, so a change
        // to that shape turns each of them into a query that matches nothing
        // -- and a query that matches nothing is comfortably inside any
        // budget. This is the line that stops the report reading "under 100 ms"
        // about six searches for a row that is not there.
        assert!(
            hits > 0,
            "{name} ({raw}) matched nothing: the fixture moved"
        );
        assert!(
            timing.p90 < BUDGET_MS,
            "{name} took {} ms at p90 (budget {BUDGET_MS} ms)",
            timing.p90
        );
        worst = worst.max(timing.p90);
    }
    println!("worst p90 over the estate: {worst} ms");
}

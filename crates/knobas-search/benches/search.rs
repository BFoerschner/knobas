//! The launcher's read path, benchmarked over the exit criterion's corpus.
//!
//! Two instruments, on purpose (interfaces §6.3 asks for *"a bench, not a
//! claim"*):
//!
//! * `tests/perf.rs` is the **gate** -- one pass/fail anyone can re-run after
//!   a change, printing a curve across three corpus sizes.
//! * this is the **measurement** -- criterion's sampling, so a regression of
//!   ten percent is visible rather than lost in the noise a wall-clock p90
//!   carries.
//!
//! Neither runs in `just check`. Seeding 100 k rows costs tens of seconds and
//! a shared runner's timings would flake.
//!
//! ```text
//! cargo bench -p knobas-search
//! ```
//!
//! # The fixture, and the step that makes it measurable
//!
//! [`knobas_search::testing::seed_corpus`] -- deterministic, 25-word bodies
//! over a 20-word vocabulary, and it **vacuums**. `ANALYZE` alone is not
//! enough for a GIN index: `fastupdate` parks new entries in an unsorted
//! pending list that every scan reads linearly, and this stream has measured
//! that inflating the same statement by between 21× and 182×, with no size and
//! no harness exempt. `testing.rs` carries the record; the point here is that
//! the bench does not get to forget it, because the generator owns it.
//!
//! Criterion is a dev-dependency with `default-features = false`, so
//! plotters and the HTML report stay out of every `cargo clippy --all-targets`
//! (open question **E-Q4**).

use criterion::{Criterion, criterion_group, criterion_main};
use knobas_search::{SearchFilters, SearchQuery, Searcher, testing};
use tokio::runtime::Runtime;

/// The corpus the exit criterion names.
const ROWS: i64 = 100_000;

/// The same ten cases the gate runs, so the two instruments measure one thing.
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

fn query(raw: &str) -> SearchQuery {
    SearchQuery {
        raw: raw.to_owned(),
        limit: 40,
        filters: SearchFilters::default(),
    }
}

fn bench(c: &mut Criterion) {
    // One runtime for the whole bench: a runtime per iteration would measure
    // thread-pool startup, which is not on the launcher's path.
    let runtime = Runtime::new().expect("a tokio runtime");
    let searcher = runtime.block_on(async {
        let pool = knobas_db::test_util::test_pool().await;
        knobas_db::migrate::run(&pool).await.expect("migrations");
        testing::seed_sources(&pool).await.expect("sources");
        testing::seed_corpus(&pool, ROWS).await.expect("corpus");
        Searcher::new(pool)
    });

    let mut group = c.benchmark_group("search-100k");
    for (name, raw) in CASES {
        group.bench_function(*name, |b| {
            b.iter(|| {
                runtime
                    .block_on(searcher.search(query(raw)))
                    .expect("the query answers");
            });
        });
    }
    // The other two reads `⌘K` runs on an empty box. They are in the same
    // group because they share the same budget: a slow board is a slow
    // launcher even when every query is fast.
    group.bench_function("launcher_board", |b| {
        b.iter(|| {
            runtime
                .block_on(searcher.launcher_board())
                .expect("the board answers");
        });
    });
    group.bench_function("smart_lists", |b| {
        b.iter(|| {
            runtime
                .block_on(searcher.smart_lists())
                .expect("the lists answer");
        });
    });
    group.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);

//! The generated statement, executed against a real PostgreSQL.
//!
//! `sql.rs`'s unit tests pin what the builder *writes*; nothing there can tell
//! whether Postgres accepts it. These run it. That matters more than usual for
//! this module, because the statement is assembled per query: a filter
//! combination nobody executed is a filter combination nobody knows parses.
//!
//! Every test in this binary shares one database (`knobas_db::test_util`), so
//! each seeds a token unique to itself and nothing truncates. It does **not**
//! outlive the run -- `test_util::run_nonce` is `{pid}-{nanos}`, so an earlier
//! run's scratch directory can never match this process's stamp and is
//! deleted.

use chrono::{DateTime, Duration, Utc};
use knobas_search::corpus::LIVE_ITEM;
use knobas_search::query::EffectiveFilters;
use knobas_search::sql::{query_as_with, search_sql};

/// One row of the launcher's result set.
///
/// Everything but the group and its count is nullable: a kind whose rows the
/// global limit cut still comes back, carrying its true total and no hit. That
/// outer join is the whole point of the shape.
#[derive(Debug, sqlx::FromRow)]
struct ResultRow {
    group_kind: String,
    kind_total: i64,
    entity_id: Option<String>,
    source_id: Option<String>,
    title: Option<String>,
    updated_at: Option<DateTime<Utc>>,
    synced_at: Option<DateTime<Utc>>,
    rank: Option<f32>,
    snippet: Option<String>,
}

async fn pool() -> sqlx::PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    pool
}

/// A timestamp `n` days back.
///
/// Seeds pass this rather than a day count, so that a test which needs several
/// rows to share one `item_updated_at` -- and they must share it *exactly*, or
/// the tie it is pinning is not a tie -- can compute it once.
fn days_ago(days: i64) -> DateTime<Utc> {
    Utc::now() - Duration::days(days)
}

fn token(tag: &str) -> String {
    format!("zs{tag}{}", uuid::Uuid::new_v4().simple())
}

#[allow(clippy::too_many_arguments)]
async fn seed(
    pool: &sqlx::PgPool,
    id: &str,
    kind: &str,
    source_id: &str,
    title: &str,
    body: &str,
    author: Option<&str>,
    updated_at: DateTime<Utc>,
) {
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,$2,$3)")
        .bind(id)
        .bind(kind)
        .bind(title)
        .execute(pool)
        .await
        .unwrap();
    // `synced_at` is seeded deliberately in the past (a mirror is as old as its
    // last run) and distinctly apart from `item_updated_at`, so a test can tell
    // the two columns apart -- §4 renders "synced 4 min ago" from the second.
    sqlx::query(
        "insert into sync.item
           (entity_id, source_id, kind, title, body_text, author, item_updated_at,
            synced_at, payload)
         values ($1,$2,$3,$4,$5,$6,$7, now() - interval '90 minutes', '{}'::jsonb)",
    )
    .bind(id)
    .bind(source_id)
    .bind(kind)
    .bind(title)
    .bind(body)
    .bind(author)
    .bind(updated_at)
    .execute(pool)
    .await
    .unwrap();
}

async fn run(pool: &sqlx::PgPool, built: knobas_search::sql::SearchSql) -> Vec<ResultRow> {
    query_as_with::<ResultRow>(built)
        .fetch_all(pool)
        .await
        .unwrap()
}

/// Rows that actually carry a hit, in the order the statement returned them.
fn hits(rows: &[ResultRow]) -> Vec<&ResultRow> {
    rows.iter().filter(|r| r.entity_id.is_some()).collect()
}

#[tokio::test]
async fn a_generated_query_runs_and_carries_every_column_the_launcher_draws() {
    let pool = pool().await;
    let tag = token("a");
    seed(
        &pool,
        &format!("sq-jira:{tag}-1"),
        "ticket",
        "sq-jira",
        &format!("Retry failed {tag} payouts"),
        "Payouts that bounce should be retried with backoff.",
        Some("mara"),
        days_ago(1),
    )
    .await;
    seed(
        &pool,
        &format!("sq-gitea:{tag}-2"),
        "pr",
        "sq-gitea",
        &format!("Add {tag} retry backoff"),
        "Implements the retry.",
        Some("jonas"),
        days_ago(2),
    )
    .await;

    let rows = run(
        &pool,
        search_sql(
            &[&LIVE_ITEM],
            Some(&tag),
            false,
            &EffectiveFilters::default(),
            10,
            50,
        ),
    )
    .await;

    let mut kinds: Vec<&str> = rows.iter().map(|r| r.group_kind.as_str()).collect();
    kinds.sort_unstable();
    assert_eq!(kinds, ["pr", "ticket"]);
    assert!(rows.iter().all(|r| r.kind_total == 1), "{rows:?}");

    let ticket = rows.iter().find(|r| r.group_kind == "ticket").unwrap();
    assert_eq!(
        ticket.entity_id.as_deref(),
        Some(&*format!("sq-jira:{tag}-1"))
    );
    assert_eq!(ticket.source_id.as_deref(), Some("sq-jira"));
    assert!(ticket.title.as_deref().unwrap().contains(&tag));
    // Two different timestamps, and not the same column twice: `updated_at` is
    // when the *source* last changed the item (a day ago, per the seed) and
    // `synced_at` is when knobas last saw it (just now). §4 draws "synced 4 min
    // ago" from the second one.
    assert!(
        ticket.updated_at.unwrap() < Utc::now() - Duration::hours(12),
        "{ticket:?}"
    );
    let synced_at = ticket.synced_at.unwrap();
    assert!(
        synced_at > Utc::now() - Duration::hours(3) && synced_at < Utc::now() - Duration::hours(1),
        "synced_at must be the seeded 90 minutes ago, not now() and not          item_updated_at: {ticket:?}"
    );
    assert!(ticket.rank.unwrap() > 0.0);
    // The excerpt is marked with the sentinel selectors, not with markup.
    let snippet = ticket.snippet.clone().unwrap();
    assert!(
        snippet.contains('\u{1}') && snippet.contains('\u{2}'),
        "{snippet:?}"
    );
    assert!(!snippet.contains("<b>"), "{snippet:?}");
}

/// Better matches come first, and "better" is what `ts_rank_cd` says.
///
/// The mirror weights the title above the body, so a title hit outranks a body
/// hit -- and the ordering has to carry that all the way through the window,
/// the page and the final join, or the launcher's first row is not its best
/// row. Timestamps here are deliberately the wrong way round: the body match is
/// the *newer* item, so only rank can produce the expected order.
#[tokio::test]
async fn the_best_match_comes_first_and_recency_only_breaks_ties() {
    let pool = pool().await;
    let tag = token("r");
    seed(
        &pool,
        &format!("sq-jira:{tag}-title"),
        "ticket",
        "sq-jira",
        &format!("The {tag} rollout"),
        "Unrelated prose about batch windows and ledgers.",
        None,
        days_ago(30),
    )
    .await;
    seed(
        &pool,
        &format!("sq-jira:{tag}-body"),
        "ticket",
        "sq-jira",
        "Quarterly planning",
        &format!("Somewhere in here we mention {tag} once."),
        None,
        days_ago(0),
    )
    .await;

    let rows = run(
        &pool,
        search_sql(
            &[&LIVE_ITEM],
            Some(&tag),
            false,
            &EffectiveFilters::default(),
            10,
            50,
        ),
    )
    .await;
    let ids: Vec<String> = hits(&rows)
        .iter()
        .map(|r| r.entity_id.clone().unwrap())
        .collect();
    assert_eq!(
        ids,
        [
            format!("sq-jira:{tag}-title"),
            format!("sq-jira:{tag}-body")
        ],
        "{rows:?}"
    );
    let ranks: Vec<f32> = hits(&rows).iter().map(|r| r.rank.unwrap()).collect();
    assert!(ranks[0] > ranks[1], "{ranks:?}");
}

/// The last ordering key: when rank *and* timestamp tie, `entity_id` decides.
///
/// I first declined to pin this, on the theory that Postgres is free to return
/// either order without the key and a test would be asserting an accident. That
/// was wrong, and the review proved it: with the key deleted the server returns
/// the rows in insertion order, deterministically, so the mutation is perfectly
/// catchable. Four rows share one title (hence one `ts_rank_cd`) and one
/// literal `item_updated_at`, and are inserted in reverse alphabetical order --
/// so only the `entity_id` key can produce the expected result.
///
/// It matters beyond tidiness: a mirror is written by one transaction, so ties
/// are the common case, and without a total order two identical queries can
/// return two different pages.
#[tokio::test]
async fn rows_tied_on_rank_and_timestamp_are_ordered_by_entity_id() {
    let pool = pool().await;
    let tag = token("t");
    // One timestamp, computed once: "both a day old" is not a tie.
    let stamp = days_ago(3);
    for suffix in ["d", "c", "b", "a"] {
        seed(
            &pool,
            &format!("sq-jira:{tag}-{suffix}"),
            "ticket",
            "sq-jira",
            &format!("{tag} identical title"),
            "identical body",
            None,
            stamp,
        )
        .await;
    }

    let rows = run(
        &pool,
        search_sql(
            &[&LIVE_ITEM],
            Some(&tag),
            false,
            &EffectiveFilters::default(),
            10,
            50,
        ),
    )
    .await;
    let found = hits(&rows);

    // The tie is real, or this test pins nothing.
    let ranks: Vec<f32> = found.iter().map(|r| r.rank.unwrap()).collect();
    assert!(
        ranks.windows(2).all(|pair| pair[0] == pair[1]),
        "the rows must tie on rank: {ranks:?}"
    );
    let stamps: Vec<DateTime<Utc>> = found.iter().map(|r| r.updated_at.unwrap()).collect();
    assert!(
        stamps.windows(2).all(|pair| pair[0] == pair[1]),
        "the rows must tie on timestamp: {stamps:?}"
    );

    let ids: Vec<String> = found.iter().map(|r| r.entity_id.clone().unwrap()).collect();
    assert_eq!(
        ids,
        [
            format!("sq-jira:{tag}-a"),
            format!("sq-jira:{tag}-b"),
            format!("sq-jira:{tag}-c"),
            format!("sq-jira:{tag}-d"),
        ],
        "insertion order was d, c, b, a -- only the entity_id key reverses it"
    );
}

/// An `updated:` window wider than Postgres can subtract must still answer.
///
/// `now() - make_interval(days => $n)` has to land inside `timestamptz`, which
/// bottoms out around 2.4 million days back. `updated:99999999d` parses to a
/// perfectly ordinary `u32`, and binding it -- or worse, saturating to
/// `i32::MAX` as the first cut did -- makes the server raise `timestamp out of
/// range`, which the launcher shows as an internal error for what is really
/// just a very wide filter.
#[tokio::test]
async fn an_absurd_updated_window_answers_instead_of_erroring() {
    let pool = pool().await;
    let tag = token("w");
    seed(
        &pool,
        &format!("sq-jira:{tag}"),
        "ticket",
        "sq-jira",
        &format!("{tag} still findable"),
        "body",
        None,
        days_ago(1),
    )
    .await;

    for days in [7_u32, 99_999_999, u32::MAX] {
        let rows = run(
            &pool,
            search_sql(
                &[&LIVE_ITEM],
                Some(&tag),
                false,
                &EffectiveFilters {
                    updated_within_days: Some(days),
                    ..EffectiveFilters::default()
                },
                10,
                50,
            ),
        )
        .await;
        assert_eq!(
            hits(&rows).len(),
            1,
            "updated_within_days = {days}: {rows:?}"
        );
    }
}

/// The carry-over this query was rewritten for: the per-kind count is the
/// number of **matches**, not the number of rows that fit on the page.
///
/// The seed's `count(*) over (partition by kind)` could only report what the
/// page contained, and it cost a second full sort of every match to do it. A
/// kind the limit cut entirely must still come back with its real total and no
/// hit.
#[tokio::test]
async fn the_limit_cuts_the_page_and_never_the_totals() {
    let pool = pool().await;
    let tag = token("b");
    for n in 0..4 {
        seed(
            &pool,
            &format!("sq-jira:{tag}-t{n}"),
            "ticket",
            "sq-jira",
            &format!("{tag} ticket {n}"),
            "body",
            Some("mara"),
            days_ago(i64::from(n)),
        )
        .await;
    }
    for n in 0..3 {
        seed(
            &pool,
            &format!("sq-gitea:{tag}-p{n}"),
            "pr",
            "sq-gitea",
            &format!("{tag} pull {n}"),
            "body",
            Some("mara"),
            // Older than every ticket, so a global limit of 1 keeps a ticket
            // and leaves the `pr` group with no hit at all.
            days_ago(10 + i64::from(n)),
        )
        .await;
    }

    let full = run(
        &pool,
        search_sql(
            &[&LIVE_ITEM],
            Some(&tag),
            false,
            &EffectiveFilters::default(),
            10,
            50,
        ),
    )
    .await;
    assert_eq!(hits(&full).len(), 7);
    assert_eq!(total_for(&full, "ticket"), 4);
    assert_eq!(total_for(&full, "pr"), 3);

    let one = run(
        &pool,
        search_sql(
            &[&LIVE_ITEM],
            Some(&tag),
            false,
            &EffectiveFilters::default(),
            10,
            1,
        ),
    )
    .await;
    assert_eq!(hits(&one).len(), 1, "{one:?}");
    // Both kinds are still reported, with their true counts -- including the
    // one the limit left nothing of.
    assert_eq!(total_for(&one, "ticket"), 4);
    assert_eq!(total_for(&one, "pr"), 3);
    let cut = one.iter().find(|r| r.group_kind == "pr").unwrap();
    assert!(cut.entity_id.is_none(), "{cut:?}");
    assert!(cut.rank.is_none() && cut.snippet.is_none(), "{cut:?}");

    // And `per_group` caps a single kind without touching the counts.
    let capped = run(
        &pool,
        search_sql(
            &[&LIVE_ITEM],
            Some(&tag),
            false,
            &EffectiveFilters::default(),
            2,
            50,
        ),
    )
    .await;
    assert_eq!(hits(&capped).len(), 4, "{capped:?}");
    assert_eq!(total_for(&capped, "ticket"), 4);
    assert_eq!(total_for(&capped, "pr"), 3);
}

fn total_for(rows: &[ResultRow], kind: &str) -> i64 {
    rows.iter()
        .find(|r| r.group_kind == kind)
        .unwrap_or_else(|| panic!("no group for {kind} in {rows:?}"))
        .kind_total
}

#[tokio::test]
async fn every_filter_narrows_the_match_and_none_of_them_is_a_literal() {
    let pool = pool().await;
    let tag = token("c");
    seed(
        &pool,
        &format!("sq-jira:{tag}-1"),
        "ticket",
        "sq-jira",
        &format!("{tag} recent mine"),
        "body",
        Some("mara"),
        days_ago(1),
    )
    .await;
    seed(
        &pool,
        &format!("sq-gitea:{tag}-2"),
        "pr",
        "sq-gitea",
        &format!("{tag} old theirs"),
        "body",
        Some("jonas"),
        days_ago(90),
    )
    .await;

    let only = |filters: EffectiveFilters| {
        let pool = pool.clone();
        let tag = tag.clone();
        async move {
            let rows = run(
                &pool,
                search_sql(&[&LIVE_ITEM], Some(&tag), false, &filters, 10, 50),
            )
            .await;
            hits(&rows)
                .iter()
                .map(|r| r.entity_id.clone().unwrap())
                .collect::<Vec<_>>()
        }
    };

    assert_eq!(
        only(EffectiveFilters {
            sources: vec!["sq-jira".to_owned()],
            ..EffectiveFilters::default()
        })
        .await,
        [format!("sq-jira:{tag}-1")]
    );
    assert_eq!(
        only(EffectiveFilters {
            kinds: vec!["pr".to_owned()],
            ..EffectiveFilters::default()
        })
        .await,
        [format!("sq-gitea:{tag}-2")]
    );
    assert_eq!(
        only(EffectiveFilters {
            updated_within_days: Some(7),
            ..EffectiveFilters::default()
        })
        .await,
        [format!("sq-jira:{tag}-1")]
    );
    assert_eq!(
        only(EffectiveFilters {
            mine: true,
            identity_authors: vec!["mara".to_owned()],
            ..EffectiveFilters::default()
        })
        .await,
        [format!("sq-jira:{tag}-1")]
    );
    // A *named* person narrows to that person and nobody else -- `mine` off,
    // so nothing the user wrote is in the answer either (ruling E-Q1).
    assert_eq!(
        only(EffectiveFilters {
            named_authors: vec!["jonas".to_owned()],
            ..EffectiveFilters::default()
        })
        .await,
        [format!("sq-gitea:{tag}-2")]
    );
    // `@me @jonas` is one predicate over both: "mine or jonas's".
    let mut both = only(EffectiveFilters {
        mine: true,
        identity_authors: vec!["mara".to_owned()],
        named_authors: vec!["jonas".to_owned()],
        ..EffectiveFilters::default()
    })
    .await;
    both.sort();
    assert_eq!(both, [format!("sq-gitea:{tag}-2"), format!("sq-jira:{tag}-1")]);
    // A name nobody wrote under matches nothing rather than everything.
    assert!(
        only(EffectiveFilters {
            named_authors: vec!["nobody".to_owned()],
            ..EffectiveFilters::default()
        })
        .await
        .is_empty()
    );
    // `mine` with nobody behind it filters everything out rather than nothing.
    assert!(
        only(EffectiveFilters {
            mine: true,
            ..EffectiveFilters::default()
        })
        .await
        .is_empty()
    );
    // All of them at once still parses and still narrows.
    assert_eq!(
        only(EffectiveFilters {
            sources: vec!["sq-jira".to_owned()],
            kinds: vec!["ticket".to_owned()],
            updated_within_days: Some(7),
            mine: true,
            identity_authors: vec!["mara".to_owned()],
            named_authors: Vec::new(),
        })
        .await,
        [format!("sq-jira:{tag}-1")]
    );
}

#[tokio::test]
async fn a_half_typed_word_matches_as_a_prefix_and_a_finished_one_does_not() {
    let pool = pool().await;
    let tag = token("d");
    seed(
        &pool,
        &format!("sq-jira:{tag}"),
        "ticket",
        "sq-jira",
        &format!("{tag}extra payouts"),
        "body",
        None,
        days_ago(1),
    )
    .await;

    let typing = run(
        &pool,
        search_sql(
            &[&LIVE_ITEM],
            Some(&tag),
            true,
            &EffectiveFilters::default(),
            10,
            50,
        ),
    )
    .await;
    assert_eq!(hits(&typing).len(), 1, "{typing:?}");

    let finished = run(
        &pool,
        search_sql(
            &[&LIVE_ITEM],
            Some(&tag),
            false,
            &EffectiveFilters::default(),
            10,
            50,
        ),
    )
    .await;
    assert!(hits(&finished).is_empty(), "{finished:?}");
}

/// A query of nothing but stopwords yields the empty tsquery, and `':*'`
/// appended to that is a **syntax error** rather than an empty match. The
/// `null::tsquery` guard is what keeps the box answering while someone types
/// "the".
#[tokio::test]
async fn a_stopword_only_query_returns_nothing_instead_of_failing() {
    let pool = pool().await;
    for prefix_last in [true, false] {
        let rows = run(
            &pool,
            search_sql(
                &[&LIVE_ITEM],
                Some("the"),
                prefix_last,
                &EffectiveFilters::default(),
                10,
                50,
            ),
        )
        .await;
        assert!(rows.is_empty(), "prefix_last={prefix_last}: {rows:?}");
    }
}

#[tokio::test]
async fn browse_mode_orders_by_recency_and_ranks_nothing() {
    let pool = pool().await;
    let tag = token("e");
    let source = format!("sq-{tag}");
    for n in 0..3 {
        seed(
            &pool,
            &format!("{source}:{n}"),
            "ticket",
            &source,
            &format!("browse {n}"),
            "body",
            None,
            days_ago(i64::from(n)),
        )
        .await;
    }

    let rows = run(
        &pool,
        search_sql(
            &[&LIVE_ITEM],
            None,
            false,
            &EffectiveFilters {
                sources: vec![source.clone()],
                ..EffectiveFilters::default()
            },
            10,
            50,
        ),
    )
    .await;
    let ids: Vec<String> = hits(&rows)
        .iter()
        .map(|r| r.entity_id.clone().unwrap())
        .collect();
    // Newest first: item 0 was updated today, item 2 two days ago.
    assert_eq!(
        ids,
        [
            format!("{source}:0"),
            format!("{source}:1"),
            format!("{source}:2")
        ]
    );
    assert!(hits(&rows).iter().all(|r| r.rank == Some(0.0)), "{rows:?}");
    assert!(hits(&rows).iter().all(|r| r.snippet.is_none()), "{rows:?}");
}

/// The corpus is `sync.live_item`: what a source deleted is not a result.
#[tokio::test]
async fn a_tombstoned_item_is_not_a_result() {
    let pool = pool().await;
    let tag = token("f");
    let id = format!("sq-jira:{tag}");
    seed(
        &pool,
        &id,
        "ticket",
        "sq-jira",
        &format!("Withdrawn {tag} work"),
        "gone upstream",
        None,
        days_ago(1),
    )
    .await;

    let query = || {
        search_sql(
            &[&LIVE_ITEM],
            Some(&tag),
            false,
            &EffectiveFilters::default(),
            10,
            50,
        )
    };
    assert_eq!(hits(&run(&pool, query()).await).len(), 1);

    sqlx::query("update knobas.entity set deleted_at = now() where id = $1")
        .bind(&id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(run(&pool, query()).await.is_empty());
}

/// The injection attempt, executed rather than merely inspected.
///
/// The unit test asserts the text never enters the statement; this asserts that
/// the statement Postgres receives is still a well-formed query, and that the
/// schema is still there afterwards.
#[tokio::test]
async fn hostile_text_is_a_search_term_and_nothing_else() {
    let pool = pool().await;
    let evil = "'; drop schema knobas cascade; --";
    let rows = run(
        &pool,
        search_sql(
            &[&LIVE_ITEM],
            Some(evil),
            true,
            &EffectiveFilters {
                sources: vec![evil.to_owned()],
                kinds: vec![evil.to_owned()],
                updated_within_days: Some(7),
                mine: true,
                identity_authors: vec![evil.to_owned()],
                named_authors: vec![evil.to_owned()],
            },
            10,
            50,
        ),
    )
    .await;
    assert!(rows.is_empty(), "{rows:?}");

    // The schema survived, which is the assertion that actually matters.
    let entities: i64 = sqlx::query_scalar("select count(*) from knobas.entity")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(entities >= 0);
}

/// The launcher's statement is never a **named** prepared statement.
///
/// PostgreSQL custom-plans a named prepared statement for its first five
/// executions and may then switch to a **generic** plan. A generic plan cannot
/// know the tsquery, so it falls back to a default selectivity guess, decides
/// the match set is tiny, and joins `knobas.entity` with a nested loop -- one
/// index probe per matching row. That is a five-fold regression which appears
/// on the *sixth* keystroke of a session, so a test that runs a query three
/// times cannot see it. `sql::query_as_with` asks for the unnamed statement
/// precisely to stop it.
///
/// **This is the deterministic half of that pin, and it exists because the
/// other half failed green.** `tests/perf.rs`'s
/// `the_plan_does_not_decay_after_the_fifth_execution` measures the
/// consequence in wall clock, and it is the reading the record quotes -- but a
/// wall-clock *ratio* is a representation of the plan, not the plan, and the
/// two diverge under load. With the fix removed, this machine reproduced the
/// step at the sixth execution exactly -- 550, 493, 563, 499, 492, **725**,
/// 691, 677, ... -- and the timing test still **passed**: its fixed costs were
/// four times the original reading, which compressed the ratio to 1.32x
/// against a threshold of 2x. The mutation survived a test named for it.
///
/// So this asserts on the thing rather than on a consequence of it. Postgres
/// lists every named prepared statement of the current session in
/// `pg_prepared_statements`; the unnamed statement is never there. No corpus
/// and no clock are involved, so nothing about the machine can change the
/// answer.
#[tokio::test]
async fn the_launchers_statement_is_never_a_named_prepared_statement() {
    let pool = pool().await;
    let mut conn = pool.acquire().await.unwrap();

    let query = || {
        search_sql(
            &[&LIVE_ITEM],
            Some("ledger"),
            false,
            &EffectiveFilters::default(),
            10,
            40,
        )
    };

    let start = named_statements(&mut conn).await;

    // Positive control. A detector that cannot be shown to catch anything is
    // not evidence there was nothing to catch: sqlx prepares and caches an
    // ordinary query by default, so this one *must* move the counter. If it
    // does not, the assertion below would pass on a session where nothing is
    // ever named and would be pinning nothing at all.
    sqlx::query("select 1").fetch_one(&mut *conn).await.unwrap();
    let control = named_statements(&mut conn).await;
    assert!(
        control > start,
        "an ordinary sqlx query left no row in pg_prepared_statements \
         ({start} -> {control}), so this test cannot tell a named statement \
         from an unnamed one and the assertion below means nothing"
    );

    query_as_with::<ResultRow>(query())
        .fetch_all(&mut *conn)
        .await
        .unwrap();

    let after = named_statements(&mut conn).await;
    assert_eq!(
        after, control,
        "the launcher's statement was left in pg_prepared_statements \
         ({control} -> {after}), so PostgreSQL is caching a plan for it and \
         will consider a generic one from the sixth execution of every \
         session. See `sql::query_as_with`, which asks for the unnamed \
         statement to stop exactly this."
    );
}

/// How many named prepared statements this session holds.
///
/// Non-persistent itself so that the numbers in the failure messages mean what
/// they say: Postgres inserts a named statement's entry at Parse, so a
/// persistent counter would count itself.
///
/// **Hygiene, not the pin** — corrected in review round 1, which checked. The
/// assertion above is a *delta* (`after == control`), so a self-counted row
/// cancels on both sides: with this line removed the test still passes on
/// healthy code and still kills the `.persistent(false)` mutant. An earlier
/// version of this comment read as though the line were load-bearing, which
/// would have made the next reader afraid to touch it for the wrong reason.
async fn named_statements(conn: &mut sqlx::PgConnection) -> i64 {
    sqlx::query_scalar("select count(*) from pg_prepared_statements")
        .persistent(false)
        .fetch_one(&mut *conn)
        .await
        .unwrap()
}

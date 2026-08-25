//! The generated statement, executed against a real PostgreSQL.
//!
//! `sql.rs`'s unit tests pin what the builder *writes*; nothing there can tell
//! whether Postgres accepts it. These run it. That matters more than usual for
//! this module, because the statement is assembled per query: a filter
//! combination nobody executed is a filter combination nobody knows parses.
//!
//! Every test shares one database (`knobas_db::test_util`), which can outlive a
//! run, so each seeds a token unique to itself and nothing truncates.

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
    updated_days_ago: i64,
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
    .bind(Utc::now() - Duration::days(updated_days_ago))
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
        1,
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
        2,
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
        30,
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
        0,
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
            i64::from(n),
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
            10 + i64::from(n),
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
        1,
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
        90,
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
            authors: vec!["mara".to_owned()],
            ..EffectiveFilters::default()
        })
        .await,
        [format!("sq-jira:{tag}-1")]
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
            authors: vec!["mara".to_owned()],
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
        1,
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
            i64::from(n),
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
        1,
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
                authors: vec![evil.to_owned()],
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

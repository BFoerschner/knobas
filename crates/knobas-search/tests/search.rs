//! Search over the synced corpus, against a real PostgreSQL.
//!
//! Every test shares one database (`knobas_db::test_util`), which can outlive
//! a run, so each seeds tokens unique to itself and nothing truncates.

use knobas_search::{SearchFilters, SearchQuery};

/// A query object with the defaults the launcher sends.
fn query(raw: &str) -> SearchQuery {
    SearchQuery {
        raw: raw.to_owned(),
        limit: 30,
        filters: SearchFilters::default(),
    }
}

async fn seed(pool: &sqlx::PgPool, id: &str, kind: &str, title: &str, body: &str) {
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,$2,$3)")
        .bind(id)
        .bind(kind)
        .bind(title)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
         values ($1,'jira',$2,$3,$4,'{}'::jsonb)",
    )
    .bind(id)
    .bind(kind)
    .bind(title)
    .bind(body)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn finds_by_fts_and_groups_by_kind() {
    let pool = &knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(pool).await.unwrap();

    let token = format!("zq{}", uuid::Uuid::new_v4().simple());
    seed(
        pool,
        &format!("jira:{token}-1"),
        "ticket",
        &format!("Retry failed {token} payouts"),
        "Payouts that bounce with a retryable error should be retried with backoff.",
    )
    .await;
    seed(
        pool,
        &format!("jira:{token}-2"),
        "pr",
        &format!("Add {token} retry backoff"),
        "Implements the retry.",
    )
    .await;

    let response = knobas_search::search(pool, &query(&token)).await.unwrap();
    assert_eq!(response.total, 2);
    assert_eq!(
        response.groups.len(),
        2,
        "one group per kind: {:?}",
        response.groups
    );
    let ticket = response
        .groups
        .iter()
        .find(|g| g.kind == "ticket")
        .expect("ticket group");
    assert_eq!(ticket.total, 1);
    assert_eq!(ticket.hits[0].row.entity_id, format!("jira:{token}-1"));
    assert_eq!(ticket.hits[0].row.source_id, "jira");
    // Display metadata travels with the group so the launcher needs no
    // hardcoded kind list (§3a).
    assert_eq!(ticket.monogram.chars().count(), 2);
    assert!(!ticket.plural.is_empty());
    // The response echoes what it made of the raw box text.
    assert_eq!(response.interpreted.text, token);
    assert!(response.interpreted.prefix.is_none());
}

/// Carry-over D: the snippet crosses the bridge as segments, not as markup --
/// `ts_headline`'s own `<b>` marks would arrive as literal tags, because the
/// excerpt is raw source text that has to be escaped wherever it is rendered.
#[tokio::test]
async fn the_snippet_marks_the_match_as_segments() {
    let pool = &knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(pool).await.unwrap();

    let token = format!("zq{}", uuid::Uuid::new_v4().simple());
    seed(
        pool,
        &format!("jira:{token}"),
        "ticket",
        "Retry failed payouts",
        &format!("The batch job hits {token} and gives up too early."),
    )
    .await;

    let response = knobas_search::search(pool, &query(&token)).await.unwrap();
    let hit = &response.groups[0].hits[0];
    let hits: Vec<&str> = hit
        .snippet
        .iter()
        .filter(|s| s.hit)
        .map(|s| s.text.as_str())
        .collect();
    assert_eq!(
        hits,
        [token.as_str()],
        "exactly the match is marked: {:?}",
        hit.snippet
    );
    let joined: String = hit.snippet.iter().map(|s| s.text.as_str()).collect();
    assert!(
        !joined.contains('<'),
        "no markup crosses the bridge: {joined:?}"
    );
    assert!(
        !joined.contains('\u{1}'),
        "the sentinels are consumed: {joined:?}"
    );
}

/// A hit that matched on the **title** must be able to quote the title.
///
/// `fts` weights the title in, so a title-only query is a perfectly good hit
/// with nothing to quote from the body -- and `ts_headline` over the body
/// alone then returns its opening words, an excerpt with no visible relation
/// to what the user typed. In a launcher that is worse than no excerpt.
#[tokio::test]
async fn a_title_only_match_is_quoted_from_the_title() {
    let pool = &knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(pool).await.unwrap();

    let token = format!("zq{}", uuid::Uuid::new_v4().simple());
    seed(
        pool,
        &format!("jira:{token}"),
        "ticket",
        &format!("Quarterly {token} rollout"),
        "Unrelated prose about batch windows, ledgers and reconciliation.",
    )
    .await;

    let response = knobas_search::search(pool, &query(&token)).await.unwrap();
    assert_eq!(response.total, 1);
    let text: String = response.groups[0].hits[0]
        .snippet
        .iter()
        .map(|s| s.text.as_str())
        .collect();
    assert!(
        text.contains(&token),
        "the excerpt must contain what matched, got {text:?}"
    );
}

/// The launcher must not offer what the source deleted: the corpus is
/// `sync.live_item`, not `sync.item` (migration 0002).
#[tokio::test]
async fn a_tombstoned_item_is_not_a_result() {
    let pool = &knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(pool).await.unwrap();

    let token = format!("zq{}", uuid::Uuid::new_v4().simple());
    let id = format!("jira:{token}");
    seed(
        pool,
        &id,
        "ticket",
        &format!("Withdrawn {token} work"),
        "gone upstream",
    )
    .await;
    assert_eq!(
        knobas_search::search(pool, &query(&token))
            .await
            .unwrap()
            .total,
        1
    );

    sqlx::query("update knobas.entity set deleted_at = now() where id = $1")
        .bind(&id)
        .execute(pool)
        .await
        .unwrap();
    let response = knobas_search::search(pool, &query(&token)).await.unwrap();
    assert_eq!(response.total, 0);
    assert!(response.groups.is_empty());
}

/// An empty box is the launcher's *board*, not a query -- and it certainly is
/// not a full-table scan. Stream E fills it with smart lists and recents.
#[tokio::test]
async fn an_empty_query_answers_without_touching_the_corpus() {
    let pool = &knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(pool).await.unwrap();
    let response = knobas_search::search(pool, &query("   ")).await.unwrap();
    assert_eq!((response.total, response.groups.len()), (0, 0));
}

/// The seed answers unfiltered queries only. Echoing a filter it did not
/// apply would be a lie the caller cannot see: results that look filtered and
/// are not. Stream E's query builder makes this arm unreachable.
#[tokio::test]
async fn a_filtered_query_is_refused_until_stream_e_lands() {
    let pool = &knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(pool).await.unwrap();
    let mut filtered = query("sepa");
    filtered.filters.kinds = vec!["ticket".to_owned()];
    assert!(matches!(
        knobas_search::search(pool, &filtered).await,
        Err(knobas_search::SearchError::Unsupported(_))
    ));
}

/// `total` is how many rows *match*, not how many came back.
///
/// The trap it pins: the count used to ride on the first returned row
/// (`count(*) over ()`), so a query that returned no rows reported no matches
/// -- and `limit: 0` returns no rows by definition. The launcher draws its
/// group headers and its board counts from this number, so a silent zero is a
/// wrong number on screen rather than a short list.
#[tokio::test]
async fn the_total_counts_matches_not_the_page() {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();

    let tag = format!("zzq{}", uuid::Uuid::new_v4().simple());
    for n in 0..3 {
        seed(
            &pool,
            &format!("mock:{tag}-{n}"),
            "ticket",
            &format!("{tag} number {n}"),
            "body",
        )
        .await;
    }

    let full = knobas_search::search(&pool, &query(&tag)).await.unwrap();
    assert_eq!(full.total, 3);
    assert_eq!(full.groups[0].hits.len(), 3);

    // One row of three: the page shrinks, the total does not.
    let mut one = query(&tag);
    one.limit = 1;
    let one = knobas_search::search(&pool, &one).await.unwrap();
    assert_eq!(one.groups[0].hits.len(), 1);
    assert_eq!(one.total, 3, "the total must count matches, not the page");

    // No rows at all, and still three matches.
    let mut none = query(&tag);
    none.limit = 0;
    let none = knobas_search::search(&pool, &none).await.unwrap();
    assert!(none.groups.is_empty());
    assert_eq!(
        none.total, 3,
        "limit 0 must report the matches, not zero -- the count cannot ride on a row"
    );
}

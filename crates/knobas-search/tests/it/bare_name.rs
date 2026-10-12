//! A repo is found by its bare name (#612, spec #591 story 1), and a slashed
//! title is also *named* by its last `/` segment (#621), so the repo whose
//! bare name is the query comes first among equal ranks.
//!
//! A Gitea repo's title is its `full_name`, and PostgreSQL's parser reads
//! `tidewater/payout-service` as one file-path lexeme, so before `0027` the
//! queries `payout-service`, `payout` and `Payout-Service` matched nothing of
//! the repo. `0027` adds, for a title that holds a `/`, the same title with
//! each `/` read as a word break, at the title's weight A.
//!
//! Through the public [`Searcher`] against a real PostgreSQL (ADR-0013), with
//! each repo stored the way `knobas-source-gitea`'s `repo_item` stores it:
//! the full name as the title, and the full name and the description as the
//! body. #621's branches are stored the way `branch_item` stores them. The
//! names are the tickets' own, literally, so every test takes a database of
//! its own (`scratch_database`) rather than a token in the file's shared one:
//! a token inside the name would change how the name lexes, which is the
//! thing under test.

use chrono::{DateTime, Duration, Utc};
use knobas_search::{SearchFilters, SearchQuery, SearchResponse, Searcher};

fn q(raw: &str) -> SearchQuery {
    SearchQuery {
        raw: raw.to_owned(),
        limit: 30,
        filters: SearchFilters::default(),
    }
}

/// A migrated database this test shares with nobody, holding one Gitea source.
async fn pool(label: &str) -> sqlx::PgPool {
    let pool = knobas_db::test_util::scratch_database(label)
        .await
        .pool(2)
        .await
        .expect("a pool on the scratch database");
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
         values ('gitea', 'gitea', 'Gitea', 'http://x', 'Pat')",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool
}

/// The searcher the app builds, with Gitea's declared kinds.
fn searcher(pool: &sqlx::PgPool) -> Searcher {
    Searcher::new(pool.clone()).with_payload_paths([knobas_source_gitea::descriptor_template()])
}

fn days_ago(n: i64) -> DateTime<Utc> {
    Utc::now() - Duration::days(n)
}

/// One mirrored Gitea item, written the way a sync run writes it.
async fn item(
    pool: &sqlx::PgPool,
    kind: &str,
    key: &str,
    title: &str,
    body: &str,
    updated_at: DateTime<Utc>,
) -> String {
    let id = format!("gitea:{key}");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,$2,$3)")
        .bind(&id)
        .bind(kind)
        .bind(title)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload,
                                item_updated_at)
         values ($1,'gitea',$2,$3,$4,'{}'::jsonb,$5)",
    )
    .bind(&id)
    .bind(kind)
    .bind(title)
    .bind(body)
    .bind(updated_at)
    .execute(pool)
    .await
    .unwrap();
    id
}

/// A repo as `repo_item` stores it: title `full_name`, body `full_name` and
/// the description, joined by a blank line.
async fn repo(
    pool: &sqlx::PgPool,
    full_name: &str,
    description: &str,
    updated_at: DateTime<Utc>,
) -> String {
    item(
        pool,
        "repo",
        full_name,
        full_name,
        &format!("{full_name}\n\n{description}"),
        updated_at,
    )
    .await
}

/// The ids of one kind's group, in the order the launcher draws them; empty
/// when the response has no such group.
fn group_ids(response: &SearchResponse, kind: &str) -> Vec<String> {
    response
        .groups
        .iter()
        .find(|group| group.kind == kind)
        .map(|group| group.hits.iter().map(|h| h.row.entity_id.clone()).collect())
        .unwrap_or_default()
}

/// The bare name, its first word and the name in another case each find the
/// repo, and the full name still does.
#[tokio::test]
async fn a_repo_is_found_by_its_bare_name_in_any_case() {
    let pool = pool("bare-name").await;
    let ours = repo(
        &pool,
        "tidewater/payout-service",
        "Payout processing",
        days_ago(1),
    )
    .await;

    for raw in [
        "payout-service",
        "payout",
        "Payout-Service",
        "tidewater/payout-service",
    ] {
        let response = searcher(&pool).search(q(raw)).await.unwrap();
        assert_eq!(
            group_ids(&response, "repo"),
            std::slice::from_ref(&ours),
            "{raw}"
        );
    }
}

/// The bare name is a title match: a newer repo whose description mentions
/// `payout-service` once ranks below the repo named for it. Were the bare name
/// lexed at B, the two would tie on rank and recency would put the newer one
/// first.
#[tokio::test]
async fn the_repo_named_for_the_query_outranks_a_newer_one_that_mentions_it() {
    let pool = pool("bare-name-rank").await;
    let named = repo(
        &pool,
        "tidewater/payout-service",
        "Payout processing",
        days_ago(30),
    )
    .await;
    let mentions = repo(
        &pool,
        "tidewater/ledger",
        "Consumes events from payout-service",
        days_ago(1),
    )
    .await;

    let response = searcher(&pool).search(q("payout-service")).await.unwrap();
    assert_eq!(group_ids(&response, "repo"), [named, mentions]);
}

/// With the repo's branch and commits mirrored beside it, the repo's name
/// alone puts the repo first in its group and returns none of its contents
/// (#595's criterion): a child's text carries its message or its branch name,
/// never the repo.
#[tokio::test]
async fn the_bare_name_finds_the_repo_and_not_its_branch_or_commits() {
    let pool = pool("bare-name-contents").await;
    let ours = repo(
        &pool,
        "tidewater/payout-service",
        "Payout processing",
        days_ago(30),
    )
    .await;
    item(
        &pool,
        "branch",
        "tidewater/payout-service@refs/heads/main",
        "main",
        "main\n\nInitial import",
        days_ago(1),
    )
    .await;
    for (n, message) in ["PAY-231: jitter in backoff", "Initial import"]
        .iter()
        .enumerate()
    {
        item(
            &pool,
            "commit",
            &format!("tidewater/payout-service@{n:040}"),
            message,
            message,
            days_ago(1),
        )
        .await;
    }

    let response = searcher(&pool).search(q("payout-service")).await.unwrap();
    assert_eq!(group_ids(&response, "repo"), [ours]);
    assert!(group_ids(&response, "commit").is_empty(), "{response:?}");
    assert!(group_ids(&response, "branch").is_empty(), "{response:?}");
}

/// A slashed title is also named by its last `/` segment (#621): for
/// `payout`, `tidewater/payout` and `tidewater/payout-service` both match at
/// the title's weight and tie on rank, and the repo whose bare name *is*
/// `payout` comes first though it is the older. Were only the whole title
/// compared, neither would be named and recency would put the newer first.
#[tokio::test]
async fn the_repo_whose_last_segment_is_the_query_comes_first() {
    let pool = pool("last-segment").await;
    let named = repo(&pool, "tidewater/payout", "Payouts", days_ago(30)).await;
    let sibling = repo(
        &pool,
        "tidewater/payout-service",
        "Payout processing",
        days_ago(1),
    )
    .await;

    let response = searcher(&pool).search(q("payout")).await.unwrap();
    assert_eq!(group_ids(&response, "repo"), [named, sibling]);
}

/// The full name still names its repo: the query `tidewater/payout` puts
/// `tidewater/payout` above the newer `tidewater/payout-service`, which its
/// prefix also matches.
#[tokio::test]
async fn the_full_name_still_puts_its_repo_first() {
    let pool = pool("last-segment-full").await;
    let named = repo(&pool, "tidewater/payout", "Payouts", days_ago(30)).await;
    let sibling = repo(
        &pool,
        "tidewater/payout-service",
        "Payout processing",
        days_ago(1),
    )
    .await;

    let response = searcher(&pool).search(q("tidewater/payout")).await.unwrap();
    assert_eq!(group_ids(&response, "repo"), [named, sibling]);
}

/// The owner names no repo: only the last segment is a name. `tidewater`
/// matches `tidewater/ledger` and `tidewater/payout` as it matches the newer
/// `harbor/tidewater-sync`, whose last segment is not `tidewater` either, so
/// the three tie and recency orders them as it did before #621. Were every `/` segment
/// a name, the owner's two repos would be named and jump above the newest.
#[tokio::test]
async fn the_owner_names_none_of_its_repos() {
    let pool = pool("last-segment-owner").await;
    let ledger = repo(&pool, "tidewater/ledger", "Ledger", days_ago(30)).await;
    let payout = repo(&pool, "tidewater/payout", "Payouts", days_ago(20)).await;
    let sync = repo(&pool, "harbor/tidewater-sync", "Sync", days_ago(1)).await;

    let response = searcher(&pool).search(q("tidewater")).await.unwrap();
    assert_eq!(group_ids(&response, "repo"), [sync, payout, ledger]);
}

/// The rule names no kind: a branch `feature/payout` is named `payout` by
/// its last segment and comes first in the branch group, above the newer
/// `feature/payout-jitter`. Neither head commit's message says `payout`, so
/// the two tie on rank and only the name key can order them.
#[tokio::test]
async fn a_branch_is_named_by_its_last_segment_too() {
    let pool = pool("last-segment-branch").await;
    let named = item(
        &pool,
        "branch",
        "tidewater/payout@refs/heads/feature/payout",
        "feature/payout",
        "feature/payout\n\nBatch the transfers",
        days_ago(30),
    )
    .await;
    let sibling = item(
        &pool,
        "branch",
        "tidewater/payout@refs/heads/feature/payout-jitter",
        "feature/payout-jitter",
        "feature/payout-jitter\n\nJitter the retry backoff",
        days_ago(1),
    )
    .await;

    let response = searcher(&pool).search(q("payout")).await.unwrap();
    assert_eq!(group_ids(&response, "branch"), [named, sibling]);
}

/// A last segment that names the query is still a tiebreak, not a tier
/// (story 26): an older repo whose description says `payout` five times
/// outranks the newer `tidewater/payout`, named `payout` by its last segment
/// and holding the word once. Were the name read before the rank, the named
/// repo would come first.
#[tokio::test]
async fn a_last_segment_name_does_not_beat_a_stronger_match() {
    let pool = pool("last-segment-tier").await;
    let strong = repo(
        &pool,
        "tidewater/ledger",
        "Payout in, payout out: payout totals, payout audits, payout reports",
        days_ago(30),
    )
    .await;
    let named = repo(&pool, "tidewater/payout", "Transfers", days_ago(1)).await;

    let response = searcher(&pool).search(q("payout")).await.unwrap();
    assert_eq!(group_ids(&response, "repo"), [strong, named]);
}

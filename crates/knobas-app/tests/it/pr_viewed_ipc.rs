//! *Viewed* on a pull request's files (#842, R-8.17): knobas-local, kept by
//! pull request and the head commit its source declares, through the two
//! commands *Files changed* calls, over the embedded PostgreSQL with the
//! declarations the compiled-in Gitea adapter makes (`Registry::for_process`).
//!
//! Each test configures a Gitea source of its own, so its pull requests are
//! told apart from anything else in this database.

use knobas_app::commands::entity::{pr_viewed_files_inner, set_pr_file_viewed_inner};

/// A Gitea source nothing else in this database uses.
async fn source(pool: &sqlx::PgPool) -> String {
    let id = format!("gt{}", &uuid::Uuid::new_v4().simple().to_string()[..10]);
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind, config)
         values ($1, 'gitea', 'Gitea', 'http://x', 'Pat', '{}')",
    )
    .bind(&id)
    .execute(pool)
    .await
    .unwrap();
    id
}

async fn pool() -> sqlx::PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    pool
}

/// A pull request in the mirror whose head is `sha`, as Gitea's record says.
async fn pull(pool: &sqlx::PgPool, source: &str, sha: &str) -> String {
    let id = format!("{source}:tidewater/payout-service#142");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,'pr',$1)")
        .bind(&id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
         values ($1,$2,'pr',$1,'',$3)",
    )
    .bind(&id)
    .bind(source)
    .bind(serde_json::json!({ "number": 142, "head": { "ref": "feature/x", "sha": sha } }))
    .execute(pool)
    .await
    .unwrap();
    id
}

/// The sync moved the head: the record now names `sha`.
async fn push(pool: &sqlx::PgPool, id: &str, sha: &str) {
    sqlx::query("update sync.item set payload = jsonb_set(payload, '{head,sha}', to_jsonb($2::text)) where entity_id = $1")
        .bind(id)
        .bind(sha)
        .execute(pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn a_file_is_viewed_until_it_is_unticked_or_the_head_moves() {
    let pool = pool().await;
    let source = source(&pool).await;
    let pr = pull(&pool, &source, "c90d11").await;

    assert!(pr_viewed_files_inner(&pool, &pr).await.unwrap().is_empty());
    assert_eq!(
        set_pr_file_viewed_inner(&pool, &pr, "src/sepa/retry.rs", true)
            .await
            .unwrap(),
        ["src/sepa/retry.rs"]
    );
    // Twice is once; a second file joins it, in path order.
    set_pr_file_viewed_inner(&pool, &pr, "src/sepa/retry.rs", true)
        .await
        .unwrap();
    assert_eq!(
        set_pr_file_viewed_inner(&pool, &pr, "docs/backoff.md", true)
            .await
            .unwrap(),
        ["docs/backoff.md", "src/sepa/retry.rs"]
    );
    assert_eq!(
        pr_viewed_files_inner(&pool, &pr).await.unwrap(),
        ["docs/backoff.md", "src/sepa/retry.rs"]
    );
    // Unticked.
    assert_eq!(
        set_pr_file_viewed_inner(&pool, &pr, "docs/backoff.md", false)
            .await
            .unwrap(),
        ["src/sepa/retry.rs"]
    );

    // The head moved: nothing is viewed of the new head, and a tick now
    // starts its own set.
    push(&pool, &pr, "e41a07").await;
    assert!(pr_viewed_files_inner(&pool, &pr).await.unwrap().is_empty());
    assert_eq!(
        set_pr_file_viewed_inner(&pool, &pr, "tests/sepa_retry.rs", true)
            .await
            .unwrap(),
        ["tests/sepa_retry.rs"]
    );
}

/// Another pull request's ticks are its own.
#[tokio::test]
async fn each_pull_request_keeps_its_own_ticks() {
    let pool = pool().await;
    let one = pull(&pool, &source(&pool).await, "c90d11").await;
    let two = pull(&pool, &source(&pool).await, "c90d11").await;
    set_pr_file_viewed_inner(&pool, &one, "a.rs", true)
        .await
        .unwrap();
    set_pr_file_viewed_inner(&pool, &two, "b.rs", true)
        .await
        .unwrap();
    assert_eq!(pr_viewed_files_inner(&pool, &one).await.unwrap(), ["a.rs"]);
    assert_eq!(pr_viewed_files_inner(&pool, &two).await.unwrap(), ["b.rs"]);
}

#[tokio::test]
async fn what_is_not_an_entity_id_is_refused() {
    let pool = pool().await;
    let refused = pr_viewed_files_inner(&pool, "not an id").await.unwrap_err();
    assert_eq!(refused.code, knobas_app::IpcErrorCode::Invalid);
    let refused = set_pr_file_viewed_inner(&pool, "not an id", "a.rs", true)
        .await
        .unwrap_err();
    assert_eq!(refused.code, knobas_app::IpcErrorCode::Invalid);
}

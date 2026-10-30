//! *Viewed* on a pull request's files (#842, R-8.17): knobas-local, kept by
//! pull request and the head commit its source declares, through the two
//! commands *Files changed* calls, over the embedded PostgreSQL with the
//! declarations the compiled-in Gitea adapter makes (`Registry::for_process`).
//!
//! Each test configures a Gitea source of its own, so its pull requests are
//! told apart from anything else in this database.

use knobas_app::commands::entity::{pr_viewed_files_inner, set_pr_file_viewed_inner};

/// No file's blob known: ticks are kept by the head (#842).
fn none() -> std::collections::BTreeMap<String, String> {
    std::collections::BTreeMap::new()
}

/// The blob each named file holds in the diff drawn (#2276).
fn blobs(pairs: &[(&str, &str)]) -> std::collections::BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(p, b)| ((*p).to_owned(), (*b).to_owned()))
        .collect()
}

/// A Gitea source nothing else in this database uses.
async fn source(pool: &sqlx::PgPool) -> String {
    let id = format!("gt{}", &uuid::Uuid::new_v4().simple().to_string()[..10]);
    crate::support::source_config(pool, &id, "gitea", "Gitea", &serde_json::json!({})).await;
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

    assert!(
        pr_viewed_files_inner(&pool, &knobas_app::sources::Registry::for_process(), &pr, &none())
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        set_pr_file_viewed_inner(
            &pool,
            &knobas_app::sources::Registry::for_process(),
            &pr,
            "src/sepa/retry.rs",
            true,
            &none(),
        )
        .await
        .unwrap(),
        ["src/sepa/retry.rs"]
    );
    // Twice is once; a second file joins it, in path order.
    set_pr_file_viewed_inner(
        &pool,
        &knobas_app::sources::Registry::for_process(),
        &pr,
        "src/sepa/retry.rs",
        true,
        &none(),
    )
    .await
    .unwrap();
    assert_eq!(
        set_pr_file_viewed_inner(
            &pool,
            &knobas_app::sources::Registry::for_process(),
            &pr,
            "docs/backoff.md",
            true,
            &none(),
        )
        .await
        .unwrap(),
        ["docs/backoff.md", "src/sepa/retry.rs"]
    );
    assert_eq!(
        pr_viewed_files_inner(&pool, &knobas_app::sources::Registry::for_process(), &pr, &none())
            .await
            .unwrap(),
        ["docs/backoff.md", "src/sepa/retry.rs"]
    );
    // Unticked.
    assert_eq!(
        set_pr_file_viewed_inner(
            &pool,
            &knobas_app::sources::Registry::for_process(),
            &pr,
            "docs/backoff.md",
            false,
            &none(),
        )
        .await
        .unwrap(),
        ["src/sepa/retry.rs"]
    );

    // The head moved: nothing is viewed of the new head, and a tick now
    // starts its own set.
    push(&pool, &pr, "e41a07").await;
    assert!(
        pr_viewed_files_inner(&pool, &knobas_app::sources::Registry::for_process(), &pr, &none())
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        set_pr_file_viewed_inner(
            &pool,
            &knobas_app::sources::Registry::for_process(),
            &pr,
            "tests/sepa_retry.rs",
            true,
            &none(),
        )
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
    set_pr_file_viewed_inner(
        &pool,
        &knobas_app::sources::Registry::for_process(),
        &one,
        "a.rs",
        true,
        &none(),
    )
    .await
    .unwrap();
    set_pr_file_viewed_inner(
        &pool,
        &knobas_app::sources::Registry::for_process(),
        &two,
        "b.rs",
        true,
        &none(),
    )
    .await
    .unwrap();
    assert_eq!(
        pr_viewed_files_inner(&pool, &knobas_app::sources::Registry::for_process(), &one, &none())
            .await
            .unwrap(),
        ["a.rs"]
    );
    assert_eq!(
        pr_viewed_files_inner(&pool, &knobas_app::sources::Registry::for_process(), &two, &none())
            .await
            .unwrap(),
        ["b.rs"]
    );
}

#[tokio::test]
async fn what_is_not_an_entity_id_is_refused() {
    let pool = pool().await;
    let refused = pr_viewed_files_inner(
        &pool,
        &knobas_app::sources::Registry::for_process(),
        "not an id",
        &none(),
    )
    .await
    .unwrap_err();
    assert_eq!(refused.code, knobas_app::IpcErrorCode::Invalid);
    let refused = set_pr_file_viewed_inner(
        &pool,
        &knobas_app::sources::Registry::for_process(),
        "not an id",
        "a.rs",
        true,
        &none(),
    )
    .await
    .unwrap_err();
    assert_eq!(refused.code, knobas_app::IpcErrorCode::Invalid);
}

/// #2276: a file whose blob the diff names is viewed by that blob, so a new
/// commit that changes one file takes *Viewed* off that file alone.
#[tokio::test]
async fn a_file_stays_viewed_through_a_new_commit_until_its_blob_changes() {
    let pool = pool().await;
    let registry = knobas_app::sources::Registry::for_process();
    let source = source(&pool).await;
    let pr = pull(&pool, &source, "c90d11").await;
    let before = blobs(&[("a.rs", "1111111"), ("b.rs", "2222222")]);
    for path in ["a.rs", "b.rs"] {
        set_pr_file_viewed_inner(&pool, &registry, &pr, path, true, &before)
            .await
            .unwrap();
    }
    assert_eq!(
        pr_viewed_files_inner(&pool, &registry, &pr, &before)
            .await
            .unwrap(),
        ["a.rs", "b.rs"]
    );
    // A new commit changes b.rs alone.
    push(&pool, &pr, "e41a07").await;
    let after = blobs(&[("a.rs", "1111111"), ("b.rs", "3333333")]);
    assert_eq!(
        pr_viewed_files_inner(&pool, &registry, &pr, &after)
            .await
            .unwrap(),
        ["a.rs"]
    );
    // Ticking b.rs again keeps it by its new blob, a.rs untouched.
    assert_eq!(
        set_pr_file_viewed_inner(&pool, &registry, &pr, "b.rs", true, &after)
            .await
            .unwrap(),
        ["a.rs", "b.rs"]
    );
    assert_eq!(
        set_pr_file_viewed_inner(&pool, &registry, &pr, "a.rs", false, &after)
            .await
            .unwrap(),
        ["b.rs"]
    );
}

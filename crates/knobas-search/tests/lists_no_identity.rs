//! An install where knobas does not know who the user is.
//!
//! A test binary of its own, and that is the point rather than an accident:
//! the identity behind `@me` is the union of every enabled source's configured
//! username, so "no identity" is a property of the **whole database**. Any test
//! sharing a database with one that configures an account cannot observe it.
//! `knobas_db::test_util` gives one database per test binary, fresh per run
//! (`run_nonce` is `{pid}-{nanos}`, so an earlier run's scratch directory can
//! never match this process's stamp and is deleted) -- so this file is the
//! isolation, and it is also why the hardcoded entity id below needs no
//! `on conflict`: it is the only test in the only run that will ever insert it.
//!
//! The other half -- an install that does have an account -- is
//! `a_configured_identity_leaves_the_blurb_alone` in `tests/lists.rs`. Both
//! states are pinned, so a fixture cannot flatter the behaviour by only ever
//! exercising one.

use knobas_search::{Searcher, lists};

#[tokio::test]
async fn a_list_with_no_identity_configured_says_so_instead_of_reading_empty() {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    // The precondition, asserted rather than forced. Disabling every source
    // "just in case" would have been theatre -- the database is fresh, so there
    // is nothing to disable -- and worse, it would hide the day someone adds a
    // second test to this binary and configures an account in it. Then the
    // failure would be a silently meaningless assertion instead of this line.
    let configured: i64 =
        sqlx::query_scalar("select count(*) from knobas.source_config where enabled")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        configured, 0,
        "this binary must hold no configured source; `@me` would resolve and the \
         test would be asserting nothing"
    );
    // A row that *would* be in `mine` if knobas knew any account -- so an
    // empty list here is the missing identity and not a missing corpus.
    sqlx::query("insert into knobas.entity (id, kind, title) values ('jira:NOID-1','ticket','x')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item
           (entity_id, source_id, kind, title, body_text, author, item_updated_at,
            synced_at, payload)
         values ('jira:NOID-1','jira','ticket','x','body','mara.lindqvist', now(), now(),
                 '{}'::jsonb)",
    )
    .execute(&pool)
    .await
    .unwrap();

    let s = Searcher::new(pool.clone());
    let summaries = s.smart_lists().await.unwrap();
    let mine = summaries.iter().find(|l| l.id == "mine").unwrap();

    assert_eq!(mine.count, 0);
    assert!(
        mine.description.contains("username"),
        "{}",
        mine.description
    );
    assert_eq!(
        mine.description,
        lists::describe_missing_identity(),
        "the explanation is the one the module offers, not a second copy"
    );
    // An empty list is not news either: no badge to clear.
    assert!(!mine.changed);
    // The row is really there -- `changed-today` sees it -- so the zero above
    // is the identity and nothing else.
    let today = summaries.iter().find(|l| l.id == "changed-today").unwrap();
    assert!(today.count >= 1, "{today:?}");
    assert_eq!(
        today.description,
        lists::find("changed-today").unwrap().blurb
    );

    // And the rows agree with the count: an identity-less `mine` is empty,
    // not "everybody's".
    let rows = s.smart_list_items("mine", 20).await.unwrap();
    assert_eq!(rows.total, 0);
    assert!(rows.groups.is_empty());
}

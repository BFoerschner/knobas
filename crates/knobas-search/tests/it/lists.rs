//! The lists a reader saved, against a real PostgreSQL.
//!
//! The built-in smart lists are gone (#2423, spec #2419), and with them the
//! rail, its counts per built-in and its change badge. A saved list is a `list`
//! hit behind `#l` now; `knobas-app`'s `tests/it/search_ipc.rs` drives that at
//! the wire. What is left here is the save path's own refusals.

use knobas_search::{SearchError, Searcher};

async fn pool() -> sqlx::PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    pool
}

/// Saving a query that cannot be a list is refused with the rule that
/// refuses it, and not with the way out a stored row is offered (#1413).
///
/// The *needs attention* sentence ends "Delete it and save the search
/// again", which is right for a stored row the grammar has since moved away
/// from and wrong at creation time, when there is nothing to delete. Refused
/// before any row is written, so this leaves nothing behind.
#[tokio::test]
async fn saving_a_query_that_cannot_be_a_list_names_the_rule_and_offers_no_delete() {
    let pool = pool().await;
    let s = Searcher::new(pool.clone());
    for (query, sentence) in [
        ("list:mine", "A saved list cannot open another list."),
        ("t today", "`t` is not a search."),
    ] {
        let refused = s.save_list("Refused", query).await.expect_err(query);
        let SearchError::Invalid(message) = refused else {
            panic!("{query}: refused as {refused:?}, not as an invalid query");
        };
        assert_eq!(message, sentence, "{query}");
    }
    // The refusals the box itself would already have shown keep their reason,
    // as a sentence of its own.
    let refused = s
        .save_list("Paste", &"x".repeat(600))
        .await
        .expect_err("a paste rather than a query");
    let SearchError::Invalid(message) = refused else {
        panic!("a paste refused as {refused:?}");
    };
    assert!(message.starts_with("The launcher refuses"), "{message}");
    assert!(message.ends_with('.'), "{message}");
    assert!(!message.contains("Delete it"), "{message}");
    assert!(!message.contains("Needs attention"), "{message}");
}

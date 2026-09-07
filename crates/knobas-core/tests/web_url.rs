//! What the URL resolver counts as the same link, against a real PostgreSQL
//! (issue #496, spec #491 stories 9-11 and 17).
//!
//! The rule has exactly one implementation -- the SQL `web_url_normalized!`
//! expands -- and it is applied to both sides of the comparison, so a Rust
//! twin written to check it would be a second rule to keep in step. That is
//! the reasoning `ancestor_path.rs` records for `ancestor_path_read!`, and
//! this file is its sibling: the normalisation's *semantics* are pinned here,
//! against the database, and `crates/knobas-app/tests/url_resolve.rs` pins
//! what the command built on it answers.
//!
//! **Every assertion is written as a pair or as an absence.** A normalisation
//! rule is a claim that two spellings collapse together, so the interesting
//! failure is the one where they stop -- and equally the one where two links
//! that name *different* things start colliding, which is what the query
//! cases below are for.

use sqlx::{PgPool, Row};

/// The expression under test, over a URL bound as a parameter.
///
/// The macro takes its argument as a literal, so this is the same
/// `&'static str` the resolver's own statement expands -- not a paraphrase.
const NORMALIZE: &str = concat!(
    "select ",
    knobas_core::web_url_normalized!("$1::text"),
    " as normalized"
);

async fn scratch() -> PgPool {
    knobas_db::test_util::scratch_database("web_url")
        .await
        .pool(2)
        .await
        .expect("a pool onto this test's own database")
}

/// What the rule makes of one URL. `None` is SQL `null` -- the miss.
async fn normalized(pool: &PgPool, url: &str) -> Option<String> {
    sqlx::query(NORMALIZE)
        .bind(url)
        .fetch_one(pool)
        .await
        .expect("the normalisation is valid SQL")
        .get::<Option<String>, _>("normalized")
}

/// The three spellings spec story 11 names, each against the URL an adapter
/// would have stored.
#[tokio::test]
async fn a_fragment_a_trailing_slash_and_a_shouted_host_are_the_same_link() {
    let pool = scratch().await;
    let stored = normalized(&pool, "https://jira.example/browse/PAY-231").await;
    assert_eq!(
        stored.as_deref(),
        Some("https://jira.example/browse/PAY-231")
    );

    for pasted in [
        "https://jira.example/browse/PAY-231#comment-42",
        "https://jira.example/browse/PAY-231/",
        "https://jira.example/browse/PAY-231/#comment-42",
        "https://JIRA.Example/browse/PAY-231",
        "HTTPS://JIRA.EXAMPLE/browse/PAY-231/#comment-42",
    ] {
        assert_eq!(
            normalized(&pool, pasted).await,
            stored,
            "{pasted} names the ticket the mirror stored"
        );
    }
}

/// The half of the rule that is *not* doing something: a Confluence page's
/// identity is its query string, so two pages of one instance must stay two.
#[tokio::test]
async fn the_query_is_kept_verbatim_and_two_page_ids_stay_two_pages() {
    let pool = scratch().await;
    let page = "https://confluence.example/pages/viewpage.action?pageId=98307";
    assert_eq!(normalized(&pool, page).await.as_deref(), Some(page));

    // The three normalisations still apply around a query.
    for pasted in [
        "https://confluence.example/pages/viewpage.action?pageId=98307#Payments",
        "https://CONFLUENCE.example/pages/viewpage.action?pageId=98307",
    ] {
        assert_eq!(
            normalized(&pool, pasted).await.as_deref(),
            Some(page),
            "{pasted} is that page"
        );
    }

    assert_ne!(
        normalized(
            &pool,
            "https://confluence.example/pages/viewpage.action?pageId=98308"
        )
        .await,
        normalized(&pool, page).await,
        "a different pageId is a different page, and the query is what says so"
    );
}

/// The trailing slash is the *path's*, not the string's: a slash inside a
/// query is a character of the query and survives.
#[tokio::test]
async fn a_slash_inside_the_query_is_part_of_the_query() {
    let pool = scratch().await;
    let with_slash = "https://kuma.example/dashboard?url=https%3A//svc/";
    assert_eq!(
        normalized(&pool, with_slash).await.as_deref(),
        Some(with_slash),
        "the query is kept verbatim, trailing slash and all"
    );

    // And the path's own trailing slash still goes, with the query in place.
    assert_eq!(
        normalized(&pool, "https://kuma.example/dashboard/?id=8")
            .await
            .as_deref(),
        Some("https://kuma.example/dashboard?id=8"),
    );
}

/// Case folding stops at the end of the authority. A Jira key is upper-case
/// and a Gitea path is case-sensitive; folding either would make two
/// different pages one.
#[tokio::test]
async fn the_path_keeps_its_case_and_the_port_survives() {
    let pool = scratch().await;
    assert_eq!(
        normalized(
            &pool,
            "https://GITEA.example:3000/Acme/Payments-SVC/issues/7"
        )
        .await
        .as_deref(),
        Some("https://gitea.example:3000/Acme/Payments-SVC/issues/7"),
    );
    assert_ne!(
        normalized(&pool, "https://gitea.example/acme/payments-svc/issues/7").await,
        normalized(&pool, "https://gitea.example/Acme/Payments-SVC/issues/7").await,
        "two paths that differ only in case are two different repositories",
    );
}

/// Two sources on two hosts never confuse each other, because the stored URL
/// carries the source's own base (spec story 17).
#[tokio::test]
async fn two_hosts_never_collapse_onto_each_other() {
    let pool = scratch().await;
    assert_ne!(
        normalized(&pool, "https://jira.example/browse/PAY-231").await,
        normalized(&pool, "https://jira-eu.example/browse/PAY-231").await,
    );
}

/// The direction, in the one place a value can be anything: the mirror
/// holds whatever an adapter reported, and a value that is not an absolute
/// URL yields `null` rather than a partial guess. `null` equals nothing, so
/// such a row is unreachable by paste rather than reachable by accident.
#[tokio::test]
async fn anything_that_is_not_an_absolute_url_misses_rather_than_guesses() {
    let pool = scratch().await;
    for unusable in [
        "",
        "/browse/PAY-231",
        "browse/PAY-231",
        "jira.example/browse/PAY-231",
        "  ",
    ] {
        assert_eq!(
            normalized(&pool, unusable).await,
            None,
            "{unusable:?} is not an absolute URL and must normalise to null"
        );
    }
}

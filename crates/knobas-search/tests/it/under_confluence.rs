//! `under:<page title>` returns every page beneath it, at any depth, against a
//! real PostgreSQL (ADR-0018, #599).
//!
//! Confluence declares a page's parent chain on the `ancestors` array its
//! records keep (#284), outermost first, each ancestor answering to its
//! `title`. That is the first **transitive** reach: a grandchild names its
//! grandparent in its own chain, so `under:` finds it with no walk of the
//! mirror. These items are shaped the way the real adapter stores them -- the
//! page record verbatim, `title` on its root and `ancestors` as
//! `{id, title, type}` objects -- and the declaration is that adapter's own
//! descriptor, not a copy of it. Every seeded Confluence title has a space, so
//! the values are typed quoted, as the box writes them (the deputy's ruling of
//! 2026-10-12 on #597). One database for the file (one per test module, #572),
//! a unique token per test, nothing truncated, as in `under.rs`.

use knobas_search::{SearchFilters, SearchQuery, SearchResponse, Searcher};

fn q(raw: &str) -> SearchQuery {
    SearchQuery {
        raw: raw.to_owned(),
        limit: 30,
        filters: SearchFilters::default(),
    }
}

fn token() -> String {
    format!("cf{}", &uuid::Uuid::new_v4().simple().to_string()[..12])
}

async fn pool() -> sqlx::PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
         values ('confluence', 'confluence', 'Confluence', 'http://x', 'Pat')
         on conflict (id) do update set kind = excluded.kind, enabled = true",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool
}

/// The searcher the app builds, with Confluence's declared payload paths --
/// written to `sync.parent_declaration` too, which `under:` reads through
/// `sync.item_parent` since #627. This file has its own database (#572), so
/// declaring Confluence alone removes nobody else's adapter kind.
async fn searcher(pool: &sqlx::PgPool) -> Searcher {
    Searcher::declaring(
        pool.clone(),
        [knobas_source_confluence::descriptor_template()],
    )
    .await
    .unwrap()
}

/// One ancestor as Confluence nests it under `ancestors`.
fn ancestor(id: &str, title: &str) -> serde_json::Value {
    serde_json::json!({ "id": id, "title": title, "type": "page" })
}

/// One page, stored the way the adapter stores it: the record verbatim as
/// payload, its title as title. `ancestors` outermost first.
async fn page(
    pool: &sqlx::PgPool,
    id: &str,
    title: &str,
    space: &str,
    ancestors: &[serde_json::Value],
) -> String {
    let entity = format!("confluence:{id}");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,'page',$2)")
        .bind(&entity)
        .bind(title)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload,
                                item_updated_at)
         values ($1,'confluence','page',$2,$2,$3, now())",
    )
    .bind(&entity)
    .bind(title)
    .bind(serde_json::json!({
        "id": id, "type": "page", "status": "current", "title": title,
        "space": { "key": space, "name": space, "type": "global" },
        "body": { "storage": { "value": "", "representation": "storage" } },
        "ancestors": ancestors,
    }))
    .execute(pool)
    .await
    .unwrap();
    entity
}

fn ids(response: &SearchResponse) -> Vec<String> {
    let mut out: Vec<String> = response
        .groups
        .iter()
        .flat_map(|g| g.hits.iter().map(|h| h.row.entity_id.clone()))
        .collect();
    out.sort();
    out
}

fn sorted(mut ids: Vec<String>) -> Vec<String> {
    ids.sort();
    ids
}

/// A three-level tree under one home page, and a sibling tree beside it:
/// `home > runbooks > {payouts > {sepa}, ledger}` and `home > handbook`.
/// Answers with the entity ids by name.
struct Tree {
    home: String,
    runbooks: String,
    payouts: String,
    sepa: String,
    ledger: String,
    handbook: String,
    t: String,
}

async fn tree(pool: &sqlx::PgPool) -> Tree {
    let t = token();
    let home = ancestor(&format!("{t}1"), &format!("Engineering {t}"));
    let runbooks = ancestor(&format!("{t}2"), &format!("Runbooks {t}"));
    let payouts = ancestor(&format!("{t}3"), &format!("Payout runbooks {t}"));
    let mut ids = Vec::new();
    for (n, title, chain) in [
        (1, format!("Engineering {t}"), vec![]),
        (2, format!("Runbooks {t}"), vec![home.clone()]),
        (
            3,
            format!("Payout runbooks {t}"),
            vec![home.clone(), runbooks.clone()],
        ),
        (
            4,
            format!("SEPA payout retry {t}"),
            vec![home.clone(), runbooks.clone(), payouts.clone()],
        ),
        (
            5,
            format!("Ledger reconciliation {t}"),
            vec![home.clone(), runbooks.clone()],
        ),
        (6, format!("On-call handbook {t}"), vec![home.clone()]),
    ] {
        ids.push(page(pool, &format!("{t}{n}"), &title, "ENG", &chain).await);
    }
    let mut ids = ids.into_iter();
    let mut next = || ids.next().unwrap();
    Tree {
        home: next(),
        runbooks: next(),
        payouts: next(),
        sepa: next(),
        ledger: next(),
        handbook: next(),
        t,
    }
}

/// `under:"<title>"`, matched ignoring case, returns the page's children
/// **and** its grandchildren **and** the page itself -- and nothing beside it
/// under the same home page.
#[tokio::test]
async fn under_an_ancestor_title_returns_children_grandchildren_and_itself() {
    let pool = pool().await;
    let tree = tree(&pool).await;
    let t = &tree.t;
    let ours = sorted(vec![
        tree.runbooks.clone(),
        tree.payouts.clone(),
        tree.sepa.clone(),
        tree.ledger.clone(),
    ]);

    for raw in [
        format!("under:\"Runbooks {t}\""),
        format!("under:\"RUNBOOKS {}\"", t.to_uppercase()),
    ] {
        let response = searcher(&pool).await.search(q(&raw)).await.unwrap();
        assert_eq!(ids(&response), ours, "{raw}");
        assert!(
            response.interpreted.unknown_tokens.is_empty(),
            "{raw} names a page and is not unknown: {:?}",
            response.interpreted.unknown_tokens
        );
    }

    // The home page reaches the whole tree, three levels down.
    let response = searcher(&pool)
        .await
        .search(q(&format!("under:\"Engineering {t}\"")))
        .await
        .unwrap();
    assert_eq!(
        ids(&response),
        sorted(vec![
            tree.home.clone(),
            tree.runbooks.clone(),
            tree.payouts.clone(),
            tree.sepa.clone(),
            tree.ledger.clone(),
            tree.handbook.clone(),
        ])
    );
}

/// An ancestor in a space this source does not sync has no row, and still
/// scopes its mirrored descendants (ADR-0018: `under:` resolves against what
/// the children declare). The ancestor is not returned, because it is not
/// there to return, and the value is not reported as unknown.
#[tokio::test]
async fn an_ancestor_that_is_not_mirrored_still_scopes_its_descendants() {
    let pool = pool().await;
    let t = token();
    let unsynced = ancestor(&format!("{t}9"), &format!("Platform {t}"));
    let middle = ancestor(&format!("{t}1"), &format!("Shared runbooks {t}"));
    let ours = vec![
        page(
            &pool,
            &format!("{t}1"),
            &format!("Shared runbooks {t}"),
            "OPS",
            std::slice::from_ref(&unsynced),
        )
        .await,
        page(
            &pool,
            &format!("{t}2"),
            &format!("Failover drill {t}"),
            "OPS",
            &[unsynced.clone(), middle],
        )
        .await,
    ];
    page(
        &pool,
        &format!("{t}3"),
        &format!("Elsewhere {t}"),
        "OPS",
        &[],
    )
    .await;

    let response = searcher(&pool)
        .await
        .search(q(&format!("under:\"Platform {t}\"")))
        .await
        .unwrap();
    assert_eq!(ids(&response), sorted(ours));
    assert!(
        response.interpreted.unknown_tokens.is_empty(),
        "an ancestor its descendants name is known though unmirrored: {:?}",
        response.interpreted.unknown_tokens
    );
}

/// The path line a page draws is unchanged: its ancestor titles, outermost
/// first, on the one separator, at every depth; a top-level page carries
/// none. The declared chain and #284's ancestor read now draw the same line
/// from the same array, so this pins the line rather than which of the two
/// drew it (`path_line_read!` takes the declared one first).
#[tokio::test]
async fn a_page_still_carries_its_ancestors_as_its_path() {
    let pool = pool().await;
    let tree = tree(&pool).await;
    let t = &tree.t;

    let response = searcher(&pool)
        .await
        .search(q(&format!("under:\"Engineering {t}\"")))
        .await
        .unwrap();
    let mut rows: Vec<(String, Option<String>)> = response
        .groups
        .iter()
        .flat_map(|g| {
            g.hits
                .iter()
                .map(|h| (h.row.entity_id.clone(), h.row.path.clone()))
        })
        .collect();
    rows.sort();
    let home = format!("Engineering {t}");
    let runbooks = format!("{home} \u{203a} Runbooks {t}");
    let mut expected = vec![
        (tree.home.clone(), None),
        (tree.runbooks.clone(), Some(home.clone())),
        (tree.payouts.clone(), Some(runbooks.clone())),
        (
            tree.sepa.clone(),
            Some(format!("{runbooks} \u{203a} Payout runbooks {t}")),
        ),
        (tree.ledger.clone(), Some(runbooks.clone())),
        (tree.handbook.clone(), Some(home.clone())),
    ];
    expected.sort();
    assert_eq!(rows, expected);
}

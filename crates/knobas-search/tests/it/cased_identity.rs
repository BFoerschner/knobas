//! The identity behind `assigned:` and `has:review-for-me` as configured, a
//! capital letter in it (#1975): the usernames are bound as typed and folded
//! by the server's `lower()`, as the assignee and the reviewers they meet are.
//!
//! A file of its own because the identity is every enabled source's
//! username, so it is a property of the whole database (`lists_no_identity`
//! says why): `ticket_keys`' all-lower-case user would match beside these.
//! Here the user is `Mara.Lindqvist` on Jira and `İlker` on Gitea, whom Rust
//! folds to `i̇lker` and the server to `ilker`.

use knobas_search::{SearchFilters, SearchQuery, Searcher};

async fn pool() -> sqlx::PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    for (id, username) in [("jira", "Mara.Lindqvist"), ("gitea", "İlker")] {
        sqlx::query(
            "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind, config)
             values ($1, $1, $1, 'http://x', 'Pat', jsonb_build_object('username', $2::text))
             on conflict (id) do update set kind = excluded.kind, enabled = true,
                                             config = excluded.config",
        )
        .bind(id)
        .bind(username)
        .execute(&pool)
        .await
        .unwrap();
    }
    pool
}

/// One mirrored item, changed today.
async fn item(pool: &sqlx::PgPool, id: &str, kind: &str, payload: serde_json::Value) {
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, $2, $1)")
        .bind(id)
        .bind(kind)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload, item_updated_at)
         values ($1, split_part($1, ':', 1), $2, $1, $1, $3, now())",
    )
    .bind(id)
    .bind(kind)
    .bind(payload)
    .execute(pool)
    .await
    .unwrap();
}

/// A Jira ticket `CASED-<n>` assigned to `assignee`, and an open Gitea pull
/// request linked to it asking `reviewer` for a review.
async fn ticket(pool: &sqlx::PgPool, n: u32, assignee: &str, reviewer: &str) -> String {
    let key = format!("CASED-{n}");
    let id = format!("jira:{key}");
    item(
        pool,
        &id,
        "ticket",
        serde_json::json!({
            "key": key,
            "fields": {
                "summary": key, "status": { "name": "To Do" },
                "assignee": { "name": assignee, "key": assignee },
                "project": { "key": "CASED", "name": "Cased" },
            },
            "sprints": [],
        }),
    )
    .await;
    let pr = format!("gitea:tide/cased#{n}");
    item(
        pool,
        &pr,
        "pr",
        serde_json::json!({
            "state": "open", "merged": false,
            "requested_reviewers": [{ "login": reviewer }],
        }),
    )
    .await;
    sqlx::query(
        "insert into knobas.link (from_id, to_id, relation, origin, created_by, confirmed_at)
         values ($1, $2, 'related', 'manual', 'user', now())",
    )
    .bind(&pr)
    .bind(&id)
    .execute(pool)
    .await
    .unwrap();
    id
}

/// The tickets `raw` answers among `CASED`'s, sorted.
async fn answer(searcher: &Searcher, raw: &str) -> Vec<String> {
    let response = searcher
        .search(SearchQuery {
            raw: format!("project:CASED {raw}"),
            limit: 50,
            filters: SearchFilters::default(),
        })
        .await
        .unwrap();
    assert!(response.interpreted.unknown_tokens.is_empty(), "{raw}");
    let mut ids: Vec<String> = response
        .groups
        .iter()
        .flat_map(|g| g.hits.iter().map(|h| h.row.entity_id.clone()))
        .filter(|id| id.starts_with("jira:"))
        .collect();
    ids.sort();
    ids
}

/// `assigned:` and `has:review-for-me` meet the user however the source
/// spells them: `Mara.Lindqvist` configured meets `mara.lindqvist` stored,
/// and `İlker` meets `İlker` under the server's fold.
#[tokio::test]
async fn a_capital_in_the_configured_username_still_names_the_user() {
    let pool = pool().await;
    let mara = ticket(&pool, 1, "mara.lindqvist", "mara.lindqvist").await;
    let ilker = ticket(&pool, 2, "İlker", "İlker").await;
    let jonas = ticket(&pool, 3, "jonas.becker", "jonas.becker").await;
    let searcher = Searcher::declaring(
        pool.clone(),
        [
            knobas_source_jira::descriptor_template(),
            knobas_source_gitea::descriptor_template(),
        ],
    )
    .await
    .unwrap();
    for (raw, expected) in [
        ("assigned:week", vec![&mara, &ilker]),
        ("-assigned:week", vec![&jonas]),
        ("has:review-for-me", vec![&mara, &ilker]),
        ("-has:review-for-me", vec![&jonas]),
    ] {
        let expected: Vec<String> = expected.into_iter().cloned().collect();
        assert_eq!(answer(&searcher, raw).await, expected, "{raw}");
    }
}

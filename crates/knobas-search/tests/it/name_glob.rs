//! `name:` (#868, spec #808 *Link sets*: "a name glob is added to the
//! grammar") against a real PostgreSQL: a glob over an item's key or title,
//! whole, ignoring case, `*` any run and `?` one character, and every other
//! character itself -- `_` and `%` included, which `like` would read as
//! wildcards.
//!
//! One database for the file (#572); every key carries a tag unique to the
//! test, so no other test's rows can answer.

use knobas_core::entity::EntityRef;
use knobas_search::{SearchFilters, SearchQuery, Searcher};

fn unique() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..8].to_uppercase()
}

async fn pool() -> sqlx::PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    pool
}

/// One live mirror item with a key and a title.
async fn item(pool: &sqlx::PgPool, key: &str, title: &str) -> String {
    let id = EntityRef::new("tc", key).to_string();
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,'build_config',$2)")
        .bind(&id)
        .bind(title)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
         values ($1,'tc','build_config',$2,$2,$3)",
    )
    .bind(&id)
    .bind(title)
    .bind(serde_json::json!({ "id": key }))
    .execute(pool)
    .await
    .unwrap();
    id
}

async fn search(pool: &sqlx::PgPool, raw: &str) -> Vec<String> {
    let response = Searcher::new(pool.clone())
        .search(SearchQuery {
            raw: raw.to_owned(),
            limit: 200,
            filters: SearchFilters::default(),
        })
        .await
        .unwrap();
    assert!(
        response.interpreted.unknown_tokens.is_empty(),
        "{raw}: {:?}",
        response.interpreted.unknown_tokens
    );
    let mut ids: Vec<String> = response
        .groups
        .iter()
        .flat_map(|g| g.hits.iter().map(|h| h.row.entity_id.clone()))
        .collect();
    ids.sort();
    ids
}

#[tokio::test]
async fn a_name_glob_matches_a_key_or_a_title_whole() {
    let pool = pool().await;
    let tag = unique();
    let build = item(&pool, &format!("Payout{tag}_Build"), "Payouts / Build").await;
    let deploy = item(&pool, &format!("Payout{tag}_Deploy"), "Payouts / Deploy").await;
    // `_` is a character here, not `like`'s one-character wildcard.
    let no_underscore = item(&pool, &format!("Payout{tag}XBuild"), "Payouts / X").await;
    let titled = item(&pool, &format!("Other{tag}"), &format!("Dashboard {tag} 100%")).await;

    let mut both = vec![build.clone(), deploy.clone()];
    both.sort();
    assert_eq!(search(&pool, &format!("name:payout{tag}_*")).await, both);
    assert_eq!(search(&pool, &format!("name:PAYOUT{tag}_B?ild")).await, [build.clone()]);
    // Whole, not a substring: no `*` at the end, no match on a prefix.
    assert!(search(&pool, &format!("name:Payout{tag}_")).await.is_empty());
    // A title, quoted for its space; `%` is itself.
    assert_eq!(
        search(&pool, &format!("name:\"dashboard {tag} 100%\"")).await,
        [titled.clone()]
    );
    assert!(search(&pool, &format!("name:\"dashboard {tag} 1%\"")).await.is_empty());
    // Repeated globs OR.
    let mut two = vec![build, no_underscore];
    two.sort();
    assert_eq!(
        search(&pool, &format!("name:Payout{tag}_Build name:Payout{tag}XB*")).await,
        two
    );
}

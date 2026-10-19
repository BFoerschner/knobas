//! What Code draws beside a row (#966, #973, #986): `code_facts`, a
//! repository's declared language and teams and a pull request's, branch's or
//! commit's author, compared with its source's configured username, over the
//! embedded PostgreSQL with the declarations the compiled-in adapters make.

use knobas_app::commands::entity::{CodeFacts, code_facts_inner};
use knobas_core::entity::EntityRef;
use sqlx::PgPool;

async fn pool() -> PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    pool
}

fn unique() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..10].to_owned()
}

/// A Gitea source configured as `username` (blank for none).
async fn gitea(pool: &PgPool, username: &str) -> String {
    let id = format!("gitea{}", unique());
    crate::support::source_config(
        pool,
        &id,
        "gitea",
        &id,
        &serde_json::json!({ "username": username }),
    )
    .await;
    id
}

async fn item(
    pool: &PgPool,
    source: &str,
    key: &str,
    kind: &str,
    author: Option<&str>,
    payload: serde_json::Value,
) -> String {
    let id = EntityRef::new(source, key).to_string();
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,$2,$3)")
        .bind(&id)
        .bind(kind)
        .bind(key)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, author, payload)
         values ($1,$2,$3,$4,'',$5,$6)",
    )
    .bind(&id)
    .bind(source)
    .bind(kind)
    .bind(key)
    .bind(author)
    .bind(payload)
    .execute(pool)
    .await
    .unwrap();
    id
}

fn of<'a>(facts: &'a [CodeFacts], id: &str) -> &'a CodeFacts {
    facts
        .iter()
        .find(|f| f.entity_id == id)
        .unwrap_or_else(|| panic!("{id} has its facts: {facts:?}"))
}

#[tokio::test]
async fn a_repository_reads_its_declared_language_and_teams() {
    let pool = pool().await;
    let src = gitea(&pool, "mara").await;
    let payout = item(
        &pool,
        &src,
        "tidewater/payout-service",
        "repo",
        Some("tidewater"),
        serde_json::json!({
            "full_name": "tidewater/payout-service", "language": "Rust",
            "teams": [{ "name": "Payments" }, { "name": "Platform" }]
        }),
    )
    .await;
    // Gitea's `""` for no language, and no `teams` beside the record: both
    // misses, never a word.
    let runbooks = item(
        &pool,
        &src,
        "tidewater/ops-runbooks",
        "repo",
        Some("tidewater"),
        serde_json::json!({ "full_name": "tidewater/ops-runbooks", "language": "" }),
    )
    .await;

    let facts = code_facts_inner(&pool, &[payout.clone(), runbooks.clone()])
        .await
        .unwrap();
    assert_eq!(of(&facts, &payout).language.as_deref(), Some("Rust"));
    assert_eq!(of(&facts, &payout).teams, vec!["Payments", "Platform"]);
    assert_eq!(of(&facts, &runbooks).language, None);
    assert!(of(&facts, &runbooks).teams.is_empty());
}

/// *Mine* is the attention read's rule (#712): the row's author is its own
/// source's configured username. A source with none makes nobody's row mine.
#[tokio::test]
async fn an_author_is_mine_when_it_is_the_sources_configured_username() {
    let pool = pool().await;
    let src = gitea(&pool, " mara ").await;
    let nobody = gitea(&pool, "").await;
    let pr = serde_json::json!({ "merged": true, "state": "closed" });
    let mine = item(
        &pool,
        &src,
        "tidewater/payout-service#139",
        "pr",
        Some("mara"),
        pr.clone(),
    )
    .await;
    let theirs = item(
        &pool,
        &src,
        "tidewater/payout-service#142",
        "pr",
        Some("tom"),
        pr.clone(),
    )
    .await;
    let branch = item(
        &pool,
        &src,
        "tidewater/payout-service@refs/heads/feature/x",
        "branch",
        Some("mara"),
        serde_json::json!({}),
    )
    .await;
    let unconfigured = item(&pool, &nobody, "other/repo#1", "pr", Some("mara"), pr).await;
    let unknown = EntityRef::new(&src, "tidewater/payout-service#999").to_string();

    let facts = code_facts_inner(
        &pool,
        &[
            mine.clone(),
            theirs.clone(),
            branch.clone(),
            unconfigured.clone(),
            unknown.clone(),
        ],
    )
    .await
    .unwrap();
    assert_eq!(
        facts.len(),
        4,
        "an id the mirror does not hold has no facts: {facts:?}"
    );
    assert_eq!(of(&facts, &mine).author.as_deref(), Some("mara"));
    assert!(of(&facts, &mine).mine);
    assert_eq!(of(&facts, &theirs).author.as_deref(), Some("tom"));
    assert!(!of(&facts, &theirs).mine);
    assert!(of(&facts, &branch).mine);
    assert!(!of(&facts, &unconfigured).mine);
}

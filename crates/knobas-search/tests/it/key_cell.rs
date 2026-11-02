//! `sync.item.key_cell` against what it claims to follow, on a real
//! PostgreSQL (migration `0068`, #2451, #2452).
//!
//! The key cell is written once, when a row is, from the declarations the
//! database holds (`sync.parent_declaration`), and `fts` holds it at weight A.
//! These tests change each thing it depends on -- the row's payload, the
//! declarations arriving after the row, the declarations going -- and read
//! the stored value, then read it as the launcher's board and Titles do.
//!
//! Its own database (one per test file), so it may write
//! `sync.parent_declaration` freely -- one test at a time, because
//! `Searcher::declaring` writes that table whole.

use std::sync::LazyLock;

use knobas_search::{SearchFilters, SearchQuery, Searcher};
use sqlx::PgPool;
use tokio::sync::Mutex;

static SERIAL: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

async fn pool() -> PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    pool
}

fn token(tag: &str) -> String {
    format!("zk{tag}{}", uuid::Uuid::new_v4().simple())
}

async fn configure(pool: &PgPool, id: &str, adapter_kind: &str) {
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
         values ($1, $2, $1, 'http://x', 'Pat') on conflict (id) do nothing",
    )
    .bind(id)
    .bind(adapter_kind)
    .execute(pool)
    .await
    .unwrap();
}

async fn item(pool: &PgPool, id: &str, source: &str, kind: &str, payload: serde_json::Value) {
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,$2,'Payouts')")
        .bind(id)
        .bind(kind)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, item_updated_at,
                                synced_at, payload)
         values ($1,$2,$3,'Payouts','',now(),now(),$4)",
    )
    .bind(id)
    .bind(source)
    .bind(kind)
    .bind(payload)
    .execute(pool)
    .await
    .unwrap();
}

async fn key_cell(pool: &PgPool, id: &str) -> Option<String> {
    sqlx::query_scalar("select key_cell from sync.item where entity_id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Whether `fts` holds `word` at the title's weight, the lexemes Titles reads.
async fn at_weight_a(pool: &PgPool, id: &str, word: &str) -> bool {
    sqlx::query_scalar(
        "select ts_filter(fts, '{a}') @@ plainto_tsquery('english', $2)
           from sync.item where entity_id = $1",
    )
    .bind(id)
    .bind(word)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// What the app declares: the real Gitea template.
async fn declare_gitea(pool: &PgPool) -> Searcher {
    Searcher::declaring(pool.clone(), [knobas_source_gitea::descriptor_template()])
        .await
        .unwrap()
}

fn pr_payload(repo: &str) -> serde_json::Value {
    serde_json::json!({ "base": { "ref": "main", "repo": {
        "full_name": format!("tidewater/{repo}"), "name": repo } } })
}

/// A pull request written before the declarations has no key cell; it gets
/// its repository when they arrive, a moved one gets the new one, and it
/// loses it when they go. Each value is what `fts` holds at weight A.
#[tokio::test]
async fn the_key_cell_follows_the_row_and_its_declarations() {
    let _serial = SERIAL.lock().await;
    let pool = pool().await;
    // No declarations yet: this file's own database starts with none.
    sqlx::query("delete from sync.parent_declaration")
        .execute(&pool)
        .await
        .unwrap();
    configure(&pool, "gitea", "gitea").await;
    let repo = token("repo");
    let id = format!("gitea:tidewater/{repo}#1");
    item(&pool, &id, "gitea", "pr", pr_payload(&repo)).await;
    assert_eq!(key_cell(&pool, &id).await, None, "nothing declared yet");
    assert!(!at_weight_a(&pool, &id, &repo).await);

    declare_gitea(&pool).await;
    assert_eq!(key_cell(&pool, &id).await.as_deref(), Some(repo.as_str()));
    assert!(at_weight_a(&pool, &id, &repo).await);

    // A stamp leaves it; a payload that names another repository moves it.
    sqlx::query("update sync.item set synced_at = now() where entity_id = $1")
        .bind(&id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(key_cell(&pool, &id).await.as_deref(), Some(repo.as_str()));
    let moved = token("moved");
    sqlx::query("update sync.item set payload = $2 where entity_id = $1")
        .bind(&id)
        .bind(pr_payload(&moved))
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(key_cell(&pool, &id).await.as_deref(), Some(moved.as_str()));
    assert!(at_weight_a(&pool, &id, &moved).await);
    assert!(!at_weight_a(&pool, &id, &repo).await);

    sqlx::query("delete from sync.parent_declaration where adapter_kind = 'gitea'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(key_cell(&pool, &id).await, None, "the declarations went");
}

/// A ticket's key cell is its key, which is its id and indexed by `0048`:
/// nothing is stored, so a ticket is not found by its repository under
/// Titles.
#[tokio::test]
async fn a_ticket_stores_no_key_cell() {
    let _serial = SERIAL.lock().await;
    let pool = pool().await;
    configure(&pool, "gitea", "gitea").await;
    declare_gitea(&pool).await;
    let repo = token("issue");
    let id = format!("gitea:tidewater/{repo}#2");
    item(
        &pool,
        &id,
        "gitea",
        "ticket",
        serde_json::json!({ "repository": { "full_name": format!("tidewater/{repo}"), "name": repo } }),
    )
    .await;
    assert_eq!(key_cell(&pool, &id).await, None);
}

/// A monitor's key cell is its declared type (#2452): on the launcher
/// board's recent row and on a search hit, and found under Titles by it.
#[tokio::test]
async fn a_monitor_carries_its_type_on_the_board_and_on_a_hit() {
    let _serial = SERIAL.lock().await;
    let pool = pool().await;
    configure(&pool, "mock", "mock").await;
    let searcher = Searcher::declaring(pool.clone(), [knobas_source_mock::descriptor_template()])
        .await
        .unwrap();
    let kind = token("type");
    let id = format!("mock:{kind}");
    item(
        &pool,
        &id,
        "mock",
        "monitor",
        serde_json::json!({ "type": kind, "state": "up" }),
    )
    .await;

    let board = searcher.launcher_board().await.unwrap();
    let row = board
        .recent
        .iter()
        .find(|row| row.entity_id == id)
        .expect("the newest row is on the board");
    assert_eq!(row.cell.as_deref(), Some(kind.as_str()));

    let answer = searcher
        .search(SearchQuery {
            titles: true,
            raw: format!("#m {kind}"),
            limit: 20,
            filters: SearchFilters::default(),
        })
        .await
        .unwrap();
    let hits: Vec<(&str, Option<&str>)> = answer
        .groups
        .iter()
        .flat_map(|g| g.hits.iter())
        .map(|h| (h.row.entity_id.as_str(), h.row.cell.as_deref()))
        .collect();
    assert_eq!(hits, [(id.as_str(), Some(kind.as_str()))]);
}

/// A declaration change rewrites `sync.item` rows under `0028`'s exclusive
/// key, so every statement that writes `sync.item` must take the shared key
/// before it locks a row. A stamp (`0064`) names none of the columns the row
/// trigger watches: were the key taken only after its rows (`0028`'s
/// statement trigger), a sync stamping a row while a declaration change
/// waited to rewrite it would deadlock with it.
#[tokio::test]
async fn a_stamp_waits_for_a_declaration_change_before_it_locks_a_row() {
    let _serial = SERIAL.lock().await;
    let pool = pool().await;
    configure(&pool, "gitea", "gitea").await;
    declare_gitea(&pool).await;
    let repo = token("lock");
    let id = format!("gitea:tidewater/{repo}#3");
    item(&pool, &id, "gitea", "pr", pr_payload(&repo)).await;

    // The declaration writer: the exclusive key, as its trigger takes it.
    let mut change = pool.begin().await.unwrap();
    sqlx::query("select pg_advisory_xact_lock(627, 0)")
        .execute(&mut *change)
        .await
        .unwrap();
    // A sync run stamps the row meanwhile.
    let stamp = {
        let pool = pool.clone();
        let id = id.clone();
        tokio::spawn(async move {
            let mut run = pool.begin().await.unwrap();
            sqlx::query("update sync.item set synced_at = now() where entity_id = $1")
                .bind(&id)
                .execute(&mut *run)
                .await?;
            run.commit().await
        })
    };
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    // The recompute rewrites the row: it must not wait on the stamp.
    sqlx::query("set local lock_timeout = '3s'")
        .execute(&mut *change)
        .await
        .unwrap();
    sqlx::query("update sync.item set key_cell = 'recomputed' where entity_id = $1")
        .bind(&id)
        .execute(&mut *change)
        .await
        .expect("the stamp holds no row lock while it waits for the key");
    change.commit().await.unwrap();
    stamp.await.unwrap().expect("the stamp runs once the change commits");
    assert_eq!(key_cell(&pool, &id).await.as_deref(), Some("recomputed"));
}

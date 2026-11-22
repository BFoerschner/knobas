//! The held store's bound with the sync's rows in it (the held-files pass of
//! PR #2479, 2026-11-15; migration `0092`): a row only the sync held leaves
//! the full store before any row a person read. In a file of its own, so
//! filling the bound touches no other test's rows (a database a file).

use std::sync::Arc;

use knobas_app::sources::{self, NewSource, Registry, SecretInput};
use knobas_secrets::{MemoryStore, SecretStore};
use knobas_source::AuthMethod;
use knobas_sync::held_read;
use serde_json::json;

/// When the store is over its bound, the rows only the sync held go first,
/// oldest first, before a row a person read, however much older that one is;
/// a page's read of a sync's row makes it a person's.
#[tokio::test]
async fn the_rows_only_the_sync_held_leave_the_full_store_first() {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    let id = format!("hs{}", &uuid::Uuid::new_v4().simple().to_string()[..10]);
    let secrets: Arc<dyn SecretStore> = Arc::new(MemoryStore::new());
    sources::crud::add(
        &pool,
        &secrets,
        &Registry::for_process(),
        NewSource {
            id: id.clone(),
            adapter_kind: "gitea".to_owned(),
            display_name: "Tidewater Git".to_owned(),
            base_url: "https://gitea.example".to_owned(),
            auth_kind: AuthMethod::Pat,
            config: json!({}),
            secret: SecretInput::of("pat"),
            sync_interval_secs: 300,
            enabled: true,
            sync_by_hand: None,
        },
    )
    .await
    .unwrap();
    let recipe = json!({ "test": true });
    let now = chrono::Utc::now();
    let hold = |entity: &'static str, at: chrono::DateTime<chrono::Utc>, speculative: bool| {
        let (pool, id, recipe) = (pool.clone(), id.clone(), recipe.clone());
        async move {
            held_read::put_as(
                &pool,
                &id,
                entity,
                "view:files",
                &recipe,
                at,
                &json!({ "entity": entity }),
                speculative,
            )
            .await
        }
    };
    // Two the sync held, one of them a page has drawn since; a person's read
    // of a day ago.
    hold("sync-old", now - chrono::TimeDelta::hours(2), true).await;
    hold("sync-new", now - chrono::TimeDelta::hours(1), true).await;
    hold("drawn", now - chrono::TimeDelta::hours(3), true).await;
    assert!(
        held_read::get(&pool, &id, "drawn", "view:files", &recipe)
            .await
            .is_some()
    );
    // A person's read so large the store is all but full: its size is what
    // the bound counts. With the two other reads a person made (25 bytes) the
    // store's people's rows fit it, and either sync row (21 bytes) puts it
    // over.
    sqlx::query(
        "insert into sync.held_read (source_id, entity_id, ask, recipe, read_at, bytes, answer)
         values ($1, 'person-old', 'view:', $2, now() - interval '1 day', $3, 'null')",
    )
    .bind(&id)
    .bind(&recipe)
    .bind(i32::try_from(held_read::BOUND - 30).unwrap())
    .execute(&pool)
    .await
    .unwrap();
    // A sync's row is never written over a person's, nor makes it one.
    hold("drawn", now, true).await;
    // A person's read now, which puts the store over.
    held_read::put(
        &pool,
        &id,
        "person-new",
        "view:files",
        &recipe,
        now,
        &json!({ "a": 1 }),
    )
    .await;
    let kept: Vec<(String, bool)> = sqlx::query_as(
        "select entity_id, speculative from sync.held_read where source_id = $1 order by 1",
    )
    .bind(&id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        kept,
        [
            ("drawn".to_owned(), false),
            ("person-new".to_owned(), false),
            ("person-old".to_owned(), false),
        ],
        "both sync rows went, the newer too; every row a person read stayed"
    );
}

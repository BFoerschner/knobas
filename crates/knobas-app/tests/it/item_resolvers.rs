//! Migration `0076`: the five resolvers read a batch through fenced
//! subqueries, and `sync.item.key_cell` is kept by an insert trigger and by
//! `sync.item_updated()`. Held here to what they replaced: the rows the
//! earlier function bodies write, over real row shapes of every adapter and
//! kind, and the key cell under each kind of write.

use serde_json::Value;
use sqlx::{PgConnection, PgPool};

use knobas_app::sources::Registry;

/// One row of every adapter and kind, as the live products' seed left them
/// in the scale mirror (2026-11-12): the smallest of each, and a second
/// Jira ticket with a parent, build configuration with a project chain and
/// build with a revision.
const ROWS: &str = include_str!("support/resolver-rows.json");

/// The migration that last stated each resolver before `0076`, and the
/// table it writes.
const BEFORE: [(&str, &str, &str); 5] = [
    (
        "resolve_item_parents",
        "item_parent",
        include_str!("../../../knobas-db/migrations/0032_the_project_an_item_belongs_to.sql"),
    ),
    (
        "resolve_item_revisions",
        "item_revision",
        include_str!("../../../knobas-db/migrations/0030_the_commits_a_build_ran_on.sql"),
    ),
    (
        "resolve_item_projects",
        "item_project",
        include_str!("../../../knobas-db/migrations/0043_the_projects_above_an_item.sql"),
    ),
    (
        "resolve_item_pipelines",
        "item_pipeline",
        include_str!("../../../knobas-db/migrations/0065_the_branch_a_run_was_built_on.sql"),
    ),
    (
        "resolve_item_fields",
        "item_field",
        include_str!("../../../knobas-db/migrations/0063_the_fields_a_ticket_is_read_by.sql"),
    ),
];

async fn pool() -> PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    // The declarations' stored copy, written as bring-up writes it (ADR-0019).
    knobas_app::commands::search::searcher(&pool, &Registry::for_process())
        .await
        .unwrap();
    pool
}

/// The body `migration` gives `sync.<name>(ids text[])`, between its `$fn$`
/// marks.
fn body_of<'a>(migration: &'a str, name: &str) -> &'a str {
    let head = format!("function sync.{name}(ids text[]) returns void\nlanguage plpgsql as $fn$");
    let (_, rest) = migration
        .rsplit_once(&head)
        .unwrap_or_else(|| panic!("the migration states sync.{name}"));
    let (body, _) = rest.split_once("$fn$").expect("the body's closing mark");
    body
}

/// Every row of `sync.<table>` for `ids`, in one order.
async fn rows_of(conn: &mut PgConnection, table: &str, ids: &[String]) -> Value {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "select coalesce(jsonb_agg(to_jsonb(t) order by to_jsonb(t)::text), '[]'::jsonb)
           from sync.{table} t where t.entity_id = any($1)"
    )))
    .bind(ids)
    .fetch_one(conn)
    .await
    .unwrap()
}

/// The fixture's rows written as the sync writer writes a batch: one source
/// per adapter, one multi-row upsert. The ids written.
async fn seed(pool: &PgPool) -> Vec<String> {
    let rows: Vec<Value> = serde_json::from_str(ROWS).unwrap();
    let tag = uuid::Uuid::new_v4().simple().to_string()[..8].to_owned();
    let mut adapters: Vec<&str> = rows
        .iter()
        .map(|r| r["adapter"].as_str().unwrap())
        .collect();
    adapters.sort_unstable();
    adapters.dedup();
    for adapter in &adapters {
        crate::support::source_config(
            pool,
            &format!("{adapter}-{tag}"),
            adapter,
            adapter,
            &serde_json::json!({}),
        )
        .await;
    }
    let text = |row: &Value, field: &str| row[field].as_str().unwrap().to_owned();
    let sources: Vec<String> = rows
        .iter()
        .map(|r| format!("{}-{tag}", text(r, "adapter")))
        .collect();
    let ids: Vec<String> = rows
        .iter()
        .zip(&sources)
        .map(|(r, source)| format!("{source}:{}", text(r, "key")))
        .collect();
    let kinds: Vec<String> = rows.iter().map(|r| text(r, "kind")).collect();
    let titles: Vec<String> = rows.iter().map(|r| text(r, "title")).collect();
    let payloads: Vec<&Value> = rows.iter().map(|r| &r["payload"]).collect();
    sqlx::query(
        "insert into knobas.entity (id, kind, title)
         select * from unnest($1::text[], $2::text[], $3::text[])",
    )
    .bind(&ids)
    .bind(&kinds)
    .bind(&titles)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload, item_updated_at)
         select t.id, t.source, t.kind, t.title, '', t.payload, now()
           from unnest($1::text[], $2::text[], $3::text[], $4::text[], $5::jsonb[])
                as t(id, source, kind, title, payload)",
    )
    .bind(&ids)
    .bind(&sources)
    .bind(&kinds)
    .bind(&titles)
    .bind(&payloads)
    .execute(pool)
    .await
    .unwrap();
    ids
}

/// Each resolver writes the rows its earlier body wrote: the batch is
/// resolved by the triggers, then again by the body the earlier migration
/// stated, and every side table holds the same rows both times. The fixture
/// gives every table a row, so none of the five is compared empty.
#[tokio::test]
async fn a_batch_resolves_to_the_rows_the_earlier_resolvers_wrote() {
    let pool = pool().await;
    let ids = seed(&pool).await;
    // The earlier bodies are session-local functions.
    let mut conn = pool.acquire().await.unwrap();
    for (name, table, migration) in BEFORE {
        let now = rows_of(&mut conn, table, &ids).await;
        assert!(
            now.as_array().is_some_and(|rows| !rows.is_empty()),
            "the fixture leaves sync.{table} a row"
        );
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "create function pg_temp.{name}(ids text[]) returns void
             language plpgsql as $before${}$before$",
            body_of(migration, name)
        )))
        .execute(&mut *conn)
        .await
        .unwrap();
        sqlx::query(sqlx::AssertSqlSafe(format!("select pg_temp.{name}($1)")))
            .bind(&ids)
            .execute(&mut *conn)
            .await
            .unwrap();
        let before = rows_of(&mut conn, table, &ids).await;
        assert_eq!(now, before, "sync.{name} writes other rows than it did");
        // And once more through the stored function, over rows that exist.
        sqlx::query(sqlx::AssertSqlSafe(format!("select sync.{name}($1)")))
            .bind(&ids)
            .execute(&mut *conn)
            .await
            .unwrap();
        assert_eq!(rows_of(&mut conn, table, &ids).await, before);
    }
}

/// `(entity id, stored key cell, sync.key_cell_of over the stored row)` of
/// every row of `ids`.
async fn key_cells(pool: &PgPool, ids: &[String]) -> Vec<(String, Option<String>, Option<String>)> {
    sqlx::query_as(
        "select i.entity_id, i.key_cell, sync.key_cell_of(i.source_id, i.kind, i.payload)
           from sync.item i where i.entity_id = any($1) order by i.entity_id",
    )
    .bind(ids)
    .fetch_all(pool)
    .await
    .unwrap()
}

/// The key cell follows `source_id`, `kind` and `payload` under every kind
/// of write, as `0068` promised: an insert, the sync writer's upsert over a
/// stored row, and an update that names the payload alone. A ticket stores
/// none, and a stamp leaves every cell alone.
#[tokio::test]
async fn the_key_cell_follows_its_row_under_any_writer() {
    let pool = pool().await;
    let ids = seed(&pool).await;
    let all_agree = |cells: &[(String, Option<String>, Option<String>)], when: &str| {
        for (id, stored, resolved) in cells {
            assert_eq!(stored, resolved, "{id}: the stored key cell, {when}");
        }
    };

    let inserted = key_cells(&pool, &ids).await;
    all_agree(&inserted, "inserted");
    assert!(
        inserted
            .iter()
            .filter(|(_, cell, _)| cell.is_some())
            .count()
            >= 5,
        "the fixture's pages, pull requests, branches, builds and monitors have a key cell: {inserted:?}"
    );
    for (id, stored, _) in &inserted {
        if id.contains("jira-") && !id.contains("filter-") {
            assert_eq!(*stored, None, "{id}: a ticket stores no key cell");
        }
    }

    // Every payload moved to where another key cell is read from: the names
    // an adapter declares for it, each given a new value.
    let moved = "update sync.item
                    set payload = payload
                      || jsonb_build_object('type', 'moved-type')
                      || case when payload ? 'projectName'
                              then jsonb_build_object('projectName', 'Moved project') else '{}'::jsonb end
                      || case when payload ? 'space'
                              then jsonb_build_object('space', jsonb_build_object('key', 'MV', 'name', 'Moved space'))
                              else '{}'::jsonb end
                  where entity_id = any($1)";
    sqlx::query(moved).bind(&ids).execute(&pool).await.unwrap();
    let updated = key_cells(&pool, &ids).await;
    all_agree(&updated, "after an update of the payload");
    assert_ne!(
        inserted, updated,
        "the update moved a key cell, or this compares nothing"
    );

    // The sync writer's shape over the stored rows: the first payloads again.
    let rows: Vec<Value> = serde_json::from_str(ROWS).unwrap();
    let payloads: Vec<&Value> = rows.iter().map(|r| &r["payload"]).collect();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
         select i.entity_id, i.source_id, i.kind, i.title, i.body_text, t.payload
           from unnest($1::text[], $2::jsonb[]) as t(id, payload)
           join sync.item i on i.entity_id = t.id
             on conflict (entity_id) do update set payload = excluded.payload",
    )
    .bind(&ids)
    .bind(&payloads)
    .execute(&pool)
    .await
    .unwrap();
    let upserted = key_cells(&pool, &ids).await;
    all_agree(&upserted, "after an upsert over the stored row");
    assert_eq!(
        upserted, inserted,
        "the first payloads give the first cells"
    );

    // A stamp names none of the three columns and changes no cell.
    sqlx::query(
        "update sync.item set synced_at = now() + interval '1 second' where entity_id = any($1)",
    )
    .bind(&ids)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(key_cells(&pool, &ids).await, inserted);
}

/// `0068`'s row trigger fires before an insert and never before an update:
/// a `before update ... for each row` trigger of any column list makes
/// PostgreSQL recompute every stored generated column on every update, the
/// stamp included, which is what cost the stamp its heap-only update.
#[tokio::test]
async fn no_row_trigger_fires_before_an_update_of_the_mirror() {
    let pool = pool().await;
    // `tgtype`: bit 0 row, bit 1 before, bit 4 update.
    let before_update_row: Vec<String> = sqlx::query_scalar(
        "select t.tgname from pg_trigger t
          where t.tgrelid = 'sync.item'::regclass and not t.tgisinternal
            and t.tgtype & 1 = 1 and t.tgtype & 2 = 2 and t.tgtype & 16 = 16",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(
        before_update_row.is_empty(),
        "a row trigger before an update of sync.item recomputes `fts` on every stamp: {before_update_row:?}"
    );
}

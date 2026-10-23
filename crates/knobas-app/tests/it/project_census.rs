//! The project census, read from `sync.item_project` (#759, migration
//! `0032`), against the walk it replaces, on a real PostgreSQL.
//!
//! `knobas_core::project::list` used to walk every live item through the
//! declared `project_key` and `project_name` paths. Since #759 the database
//! runs that walk when an item or a declaration changes and keeps the answer,
//! and the census reads the stored rows. This file holds `list_projects` to
//! the walk -- the pre-#759 statement, kept here as the oracle -- after every
//! change the trigger code claims to follow, and pins the newest name per
//! project with literal expectations of its own.
//!
//! `sync.parent_declaration` is one row per adapter kind, and every test's
//! [`pool`] writes it back, so the test that rewrites it takes a database of
//! its own ([`own_pool`], #1907).

use knobas_app::commands::entity::list_projects_inner;
use knobas_app::sources::Registry;
use sqlx::PgPool;

async fn pool() -> PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    declared(pool).await
}

async fn own_pool(label: &str) -> PgPool {
    let pool = knobas_db::test_util::scratch_database(label)
        .await
        .pool(4)
        .await
        .expect("a pool on the scratch database");
    declared(pool).await
}

/// The declarations' stored copy, written as bring-up writes it (ADR-0019).
async fn declared(pool: PgPool) -> PgPool {
    knobas_app::commands::search::searcher(&pool, &Registry::for_process())
        .await
        .unwrap();
    pool
}

fn unique() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

async fn configured(pool: &PgPool, adapter: &str) -> String {
    let source = format!("{adapter}-{}", &unique()[..12]);
    crate::support::source_config(pool, &source, adapter, adapter, &serde_json::json!({})).await;
    source
}

/// Write `rows` -- `(key, updated, payload)` tickets of `source` -- in the
/// sync writer's shape: one multi-row upsert, so a batch reaches the
/// statement-level triggers together.
async fn upsert(pool: &PgPool, source: &str, rows: &[(&str, &str, serde_json::Value)]) {
    let ids: Vec<String> = rows
        .iter()
        .map(|(k, _, _)| format!("{source}:{k}"))
        .collect();
    let updated: Vec<chrono::DateTime<chrono::Utc>> =
        rows.iter().map(|(_, at, _)| at.parse().unwrap()).collect();
    let payloads: Vec<&serde_json::Value> = rows.iter().map(|(_, _, p)| p).collect();
    sqlx::query(
        "insert into knobas.entity (id, kind, title)
         select id, 'ticket', id from unnest($1::text[]) id on conflict (id) do nothing",
    )
    .bind(&ids)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload, item_updated_at)
         select t.id, $4, 'ticket', t.id, '', t.payload, t.at
           from unnest($1::text[], $2::jsonb[], $3::timestamptz[]) as t(id, payload, at)
             on conflict (entity_id) do update set
                source_id       = excluded.source_id,
                payload         = excluded.payload,
                item_updated_at = excluded.item_updated_at",
    )
    .bind(&ids)
    .bind(&payloads)
    .bind(&updated)
    .bind(source)
    .execute(pool)
    .await
    .unwrap();
}

/// A Jira ticket's payload: its project, by key and (optionally) name.
fn in_project(key: &str, name: Option<&str>) -> serde_json::Value {
    match name {
        Some(name) => serde_json::json!({ "fields": { "project": { "key": key, "name": name } } }),
        None => serde_json::json!({ "fields": { "project": { "key": key } } }),
    }
}

/// The pre-#759 census, verbatim: every live item through the declared
/// project paths, bound as a parameter. The oracle the stored rows are held
/// to.
const WALK: &str = concat!(
    "select distinct on (observed.source_id, observed.key)
             observed.source_id, observed.key, observed.name
       from (select i.source_id as source_id, ",
    knobas_core::project_key_read!("$1"),
    " as key, ",
    knobas_core::declared_string!("$1", "project_name"),
    " as name,
                    coalesce(i.item_updated_at, i.synced_at) as at
               from sync.live_item i) observed
      where observed.key is not null
      order by observed.source_id, observed.key,
               (observed.name is null), observed.at desc, observed.name"
);

type Row = (String, String, Option<String>);

async fn walked(pool: &PgPool, sources: &[&str]) -> Vec<Row> {
    let declarations = knobas_app::sources::paths::declared_paths(pool, &Registry::for_process())
        .await
        .unwrap();
    let rows: Vec<Row> = sqlx::query_as(WALK)
        .bind(declarations.as_param())
        .fetch_all(pool)
        .await
        .unwrap();
    rows.into_iter()
        .filter(|(s, _, _)| sources.contains(&s.as_str()))
        .collect()
}

/// What `list_projects` answers for `sources`, after checking it is the walk.
async fn census(pool: &PgPool, sources: &[&str], step: &str) -> Vec<Row> {
    let read: Vec<Row> = list_projects_inner(pool, &knobas_app::sources::Registry::for_process())
        .await
        .unwrap()
        .into_iter()
        .filter(|p| sources.contains(&p.source_id.as_str()))
        .map(|p| (p.source_id, p.key, p.name))
        .collect();
    assert_eq!(
        read,
        walked(pool, sources).await,
        "after {step}: the census is not the walk"
    );
    read
}

fn row(source: &str, key: &str, name: Option<&str>) -> Row {
    (source.to_owned(), key.to_owned(), name.map(str::to_owned))
}

/// Every change the trigger code follows, in turn, each compared with the
/// walk and with what it should say. One test: the steps build on each other.
#[tokio::test]
async fn the_census_is_the_walk_after_every_change() {
    let pool = own_pool("project_census_walk").await;
    let a = configured(&pool, "jira").await;
    let b = configured(&pool, "jira").await;
    let both = [a.as_str(), b.as_str()];

    // One batch per source. `PAY` renamed upstream: the newest ticket carries
    // the new name, an older one the old, and a nameless one erases nothing.
    // `b` has a `PAY` of its own: a key is unique only inside its source.
    upsert(
        &pool,
        &a,
        &[
            (
                "PAY-1",
                "2026-01-01T00:00:00Z",
                in_project("PAY", Some("Payments")),
            ),
            (
                "PAY-2",
                "2026-03-01T00:00:00Z",
                in_project("PAY", Some("Payouts")),
            ),
            ("PAY-3", "2026-04-01T00:00:00Z", in_project("PAY", None)),
            ("OPS-1", "2026-02-01T00:00:00Z", in_project("OPS", None)),
            (
                "NOP-1",
                "2026-02-01T00:00:00Z",
                serde_json::json!({ "fields": {} }),
            ),
        ],
    )
    .await;
    upsert(
        &pool,
        &b,
        &[(
            "PAY-1",
            "2026-01-01T00:00:00Z",
            in_project("PAY", Some("Pay (EU)")),
        )],
    )
    .await;
    let mut first = vec![
        row(&a, "OPS", None),
        row(&a, "PAY", Some("Payouts")),
        row(&b, "PAY", Some("Pay (EU)")),
    ];
    first.sort();
    assert_eq!(census(&pool, &both, "the inserts").await, first);

    // A payload update: the oldest ticket is renamed newest.
    upsert(
        &pool,
        &a,
        &[(
            "PAY-1",
            "2026-05-01T00:00:00Z",
            in_project("PAY", Some("Pay Ops")),
        )],
    )
    .await;
    assert!(
        census(&pool, &both, "a rename")
            .await
            .contains(&row(&a, "PAY", Some("Pay Ops")))
    );

    // A re-sync that moves only the time: the payload is the same, so no
    // project is recomputed, and the stored time must still follow.
    upsert(
        &pool,
        &a,
        &[(
            "PAY-2",
            "2026-06-01T00:00:00Z",
            in_project("PAY", Some("Payouts")),
        )],
    )
    .await;
    assert!(
        census(&pool, &both, "a time-only update")
            .await
            .contains(&row(&a, "PAY", Some("Payouts")))
    );

    // A tombstone: its name no longer vouches. `OPS` loses its only item.
    for id in [format!("{a}:PAY-1"), format!("{a}:OPS-1")] {
        sqlx::query("update knobas.entity set deleted_at = now() where id = $1")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
    }
    let mut tombstoned = vec![
        row(&a, "PAY", Some("Payouts")),
        row(&b, "PAY", Some("Pay (EU)")),
    ];
    tombstoned.sort();
    assert_eq!(census(&pool, &both, "a tombstone").await, tombstoned);

    // A source turned off shows no projects; turned on, they come back.
    for (enabled, step) in [(false, "a source turned off"), (true, "turned back on")] {
        sqlx::query("update knobas.source_config set enabled = $2 where id = $1")
            .bind(&b)
            .bind(enabled)
            .execute(&pool)
            .await
            .unwrap();
        let shown = census(&pool, &both, step).await;
        assert_eq!(shown.iter().any(|(s, _, _)| *s == b), enabled, "{step}");
    }

    // A source given an adapter kind that declares no project: matched at
    // read time, nothing recomputed.
    for (kind, shown) in [("gitea", false), ("jira", true)] {
        sqlx::query("update knobas.source_config set kind = $2 where id = $1")
            .bind(&b)
            .bind(kind)
            .execute(&pool)
            .await
            .unwrap();
        let read = census(&pool, &both, "an adapter kind change").await;
        assert_eq!(read.iter().any(|(s, _, _)| *s == b), shown, "kind {kind}");
    }

    // The stored declaration loses `project_key`: every ticket's row goes.
    // Put back, they return. (The walk reads the compiled-in declarations, so
    // this step is held to its own expectation.)
    sqlx::query(
        "update sync.parent_declaration
            set payload_paths = (select jsonb_agg(p - 'project_key') from jsonb_array_elements(payload_paths) p)
          where adapter_kind = 'jira'",
    )
    .execute(&pool)
    .await
    .unwrap();
    let stored: i64 =
        sqlx::query_scalar("select count(*) from sync.item_project where source_id = any($1) and adapter_kind = 'jira'")
            .bind(&both[..])
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stored, 0, "a declaration change drops the rows");
    assert_eq!(
        census(&pool, &both, "the declaration restored").await,
        tombstoned,
        "list_projects stores the declarations on its way in"
    );

    // A delete cascades.
    sqlx::query("delete from knobas.entity where id = $1")
        .bind(format!("{b}:PAY-1"))
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        census(&pool, &both, "a delete").await,
        [row(&a, "PAY", Some("Payouts"))]
    );
}

/// The resolver carries the two macros' expansions as text, because a
/// migration is SQL and cannot call them. This holds that text to what they
/// expand to **now**: change either and this fails until a new migration
/// restates `sync.resolve_item_projects`.
#[tokio::test]
async fn the_stored_resolver_is_the_macros_expansion() {
    let pool = pool().await;
    let source: String = sqlx::query_scalar(
        "select p.prosrc from pg_proc p join pg_namespace n on n.oid = p.pronamespace
          where n.nspname = 'sync' and p.proname = 'resolve_item_projects'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    for expansion in [
        knobas_core::project_key_read!("dd.decl"),
        knobas_core::declared_string!("dd.decl", "project_name"),
        knobas_core::declared_project_path!("dd.decl"),
    ] {
        assert!(
            source.contains(expansion),
            "sync.resolve_item_projects no longer carries a macro's expansion; \
             restate it in a new migration:\n{expansion}"
        );
    }
}

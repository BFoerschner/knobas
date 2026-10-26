//! `sync.item_field` (#2184, migration `0062`): each item's status, assignee
//! and project as the database keeps them, held to the walk they replace,
//! and the reads that probe them instead of walking every ticket's payload.

use knobas_app::sources::Registry;
use knobas_search::Reading;
use sqlx::PgPool;

async fn pool() -> PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    // The declarations' stored copy, written as bring-up writes it (ADR-0019).
    knobas_app::commands::search::searcher(&pool, &Registry::for_process())
        .await
        .unwrap();
    pool
}

/// A Jira source nothing else in this database holds.
async fn jira(pool: &PgPool) -> String {
    let source = format!("jira-{}", &uuid::Uuid::new_v4().simple().to_string()[..12]);
    crate::support::source_config(pool, &source, "jira", "Jira", &serde_json::json!({})).await;
    source
}

/// Write `rows` -- `(key, payload)` tickets of `source` -- as one multi-row
/// upsert, the sync writer's shape.
async fn upsert(pool: &PgPool, source: &str, rows: &[(&str, serde_json::Value)]) {
    let ids: Vec<String> = rows.iter().map(|(k, _)| format!("{source}:{k}")).collect();
    let payloads: Vec<&serde_json::Value> = rows.iter().map(|(_, p)| p).collect();
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
         select t.id, $3, 'ticket', t.id, '', t.payload, now()
           from unnest($1::text[], $2::jsonb[]) as t(id, payload)
             on conflict (entity_id) do update set payload = excluded.payload",
    )
    .bind(&ids)
    .bind(&payloads)
    .bind(source)
    .execute(pool)
    .await
    .unwrap();
}

/// The resolver carries `declared_string!`'s expansion as text, because a
/// migration is SQL and cannot call it. This holds that text to what it
/// expands to **now**: change it and this fails until a new migration
/// restates `sync.resolve_item_fields`.
#[tokio::test]
async fn the_stored_resolver_is_the_macros_expansion() {
    let pool = pool().await;
    let source: String = sqlx::query_scalar(
        "select p.prosrc from pg_proc p join pg_namespace n on n.oid = p.pronamespace
          where n.nspname = 'sync' and p.proname = 'resolve_item_fields'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let expansion = knobas_core::declared_string!("dd.decl", "f");
    assert!(
        source.contains(expansion),
        "sync.resolve_item_fields no longer carries the macro's expansion; \
         restate it in a new migration:\n{expansion}"
    );
}

/// One ticket's field read both ways: `(entity id, field, walked, stored)`,
/// `stored` `None` where no row has the paths the declarations bind.
type Both = (String, String, Option<String>, Option<Option<String>>);

/// One row of [`both`]'s statement: the id, the walk's answer, and every
/// stored value with the bound paths.
type BothRow = (String, Option<String>, Option<Vec<Option<String>>>);

/// [`Both`] per ticket of `source`, per stored field.
async fn both(pool: &PgPool, source: &str) -> Vec<Both> {
    let declarations = knobas_app::sources::paths::declared_paths(pool, &Registry::for_process())
        .await
        .unwrap();
    let mut out = Vec::new();
    for (field, walk) in [
        (
            "status_name",
            knobas_core::declared_string!("$1", "status_name"),
        ),
        ("assignee", knobas_core::declared_string!("$1", "assignee")),
        (
            "project_key",
            knobas_core::declared_string!("$1", "project_key"),
        ),
        (
            "project_name",
            knobas_core::declared_string!("$1", "project_name"),
        ),
    ] {
        let rows: Vec<BothRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "select i.entity_id, {walk},
                        (select array_agg(f.value) from sync.item_field f
                          where f.entity_id = i.entity_id and f.field = $3
                            and f.paths = ($1::jsonb #> array[i.source_id, i.kind, $3]))
                   from sync.item i where i.source_id = $2 order by i.entity_id"
        )))
        .bind(declarations.as_param())
        .bind(source)
        .bind(field)
        .fetch_all(pool)
        .await
        .unwrap();
        for (id, walked, stored) in rows {
            let stored = stored.map(|values| {
                assert!(
                    values.windows(2).all(|w| w[0] == w[1]),
                    "{id} {field}: stored rows that disagree: {values:?}"
                );
                values[0].clone()
            });
            out.push((id, field.to_owned(), walked, stored));
        }
    }
    out
}

/// Every stored value is what the walk reads, for a found field, a miss of
/// every kind (absent, blank, not a string) and a later candidate; and it
/// follows the payload when a sync rewrites it.
#[tokio::test]
async fn a_stored_field_is_the_walks_answer() {
    let pool = pool().await;
    let source = jira(&pool).await;
    upsert(
        &pool,
        &source,
        &[
            (
                "PAY-1",
                serde_json::json!({ "fields": {
                    "status": { "name": "In Progress" },
                    "assignee": { "name": "mara" },
                    "project": { "key": "PAY", "name": "Payments" } } }),
            ),
            (
                "PAY-2",
                serde_json::json!({ "fields": {
                    "status": { "name": "  " },
                    "assignee": { "key": "jon" },
                    "project": { "key": "PAY" } } }),
            ),
            (
                "PAY-3",
                serde_json::json!({ "fields": { "status": { "name": 3 } } }),
            ),
            ("PAY-4", serde_json::json!({})),
        ],
    )
    .await;
    let read = both(&pool, &source).await;
    assert_eq!(read.len(), 16, "four tickets, four fields");
    for (id, field, walked, stored) in &read {
        assert_eq!(
            stored.as_ref(),
            Some(walked),
            "{id} {field}: the stored value is the walk's"
        );
    }
    let found = |read: &[Both], key: &str, field: &str| {
        read.iter()
            .find(|(id, f, _, _)| id.ends_with(key) && f == field)
            .and_then(|(_, _, walked, _)| walked.clone())
    };
    assert_eq!(
        found(&read, ":PAY-1", "status_name").as_deref(),
        Some("In Progress")
    );
    assert_eq!(found(&read, ":PAY-2", "assignee").as_deref(), Some("jon"));
    assert_eq!(found(&read, ":PAY-2", "status_name"), None);
    assert_eq!(found(&read, ":PAY-3", "status_name"), None);

    upsert(
        &pool,
        &source,
        &[(
            "PAY-1",
            serde_json::json!({ "fields": { "status": { "name": "Done" } } }),
        )],
    )
    .await;
    let read = both(&pool, &source).await;
    for (id, field, walked, stored) in &read {
        assert_eq!(
            stored.as_ref(),
            Some(walked),
            "{id} {field}: after the rewrite"
        );
    }
    assert_eq!(
        found(&read, ":PAY-1", "status_name").as_deref(),
        Some("Done")
    );
}

/// The board's statuses and a view's count read the stored value, not the
/// payload, and walk the payload where nothing is stored. Shown by a stored
/// status nothing but this test would write.
#[tokio::test]
async fn the_tickets_reads_probe_the_stored_fields_and_walk_without_them() {
    let pool = pool().await;
    let source = jira(&pool).await;
    let project = format!("P{}", &uuid::Uuid::new_v4().simple().to_string()[..8]).to_uppercase();
    upsert(
        &pool,
        &source,
        &[(
            "T-1",
            serde_json::json!({ "fields": {
                "status": { "name": "In Progress" },
                "assignee": { "name": "mara" },
                "project": { "key": project } } }),
        )],
    )
    .await;
    let id = format!("{source}:T-1");
    let registry = Registry::for_process();
    let declarations = knobas_app::sources::paths::declared_paths(&pool, &registry)
        .await
        .unwrap();
    let only = vec![id.clone()];
    let sources: std::collections::BTreeSet<String> = [source.clone()].into();
    let read = async |pool: &PgPool| {
        let statuses = knobas_core::mini_board::statuses_of(
            pool,
            &only,
            &declarations,
            Some(knobas_core::mini_board::LaneField::Assignee),
        )
        .await
        .unwrap();
        let observed = knobas_core::mini_board::observed_statuses(pool, &sources, &declarations)
            .await
            .unwrap();
        let searcher = knobas_app::commands::search::searcher(pool, &registry)
            .await
            .unwrap();
        let texts = [
            format!("project:{project} status:\"In Progress\""),
            format!("project:{project} assignee:mara"),
        ];
        let texts: Vec<&str> = texts.iter().map(String::as_str).collect();
        let counts = searcher.readings(&texts, "ticket").await.unwrap();
        (
            statuses
                .columns
                .iter()
                .map(|(status, placed)| (status.clone(), placed[0].lane.clone()))
                .collect::<Vec<_>>(),
            observed[0].statuses.clone(),
            counts,
        )
    };
    let walked = (
        vec![(Some("In Progress".to_owned()), "mara".to_owned())],
        vec!["In Progress".to_owned()],
        vec![Reading::Count(1), Reading::Count(1)],
    );
    assert_eq!(read(&pool).await, walked, "stored as the walk reads it");

    sqlx::query("update sync.item_field set value = 'Stored' where entity_id = $1 and field in ('status_name', 'assignee')")
        .bind(&id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        read(&pool).await,
        (
            vec![(Some("Stored".to_owned()), "Stored".to_owned())],
            vec!["Stored".to_owned()],
            vec![Reading::Count(0), Reading::Count(0)],
        ),
        "the reads take the stored value"
    );

    sqlx::query("delete from sync.item_field where entity_id = $1")
        .bind(&id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        read(&pool).await,
        walked,
        "nothing stored: the payload is walked"
    );
}

/// A read whose declarations put the field elsewhere than the stored ones
/// walks the payload by its own: a stored value is never read for paths
/// it was not resolved over.
#[tokio::test]
async fn a_read_binding_other_paths_walks_them() {
    use knobas_core::payload::{KindPaths, PayloadPath};
    let pool = pool().await;
    let source = jira(&pool).await;
    upsert(
        &pool,
        &source,
        &[(
            "T-1",
            serde_json::json!({ "fields": {
                "status": { "name": "In Progress" },
                "other": "Elsewhere" } }),
        )],
    )
    .await;
    let other = knobas_core::payload::Declarations::empty().with(
        source.clone(),
        vec![KindPaths {
            kind: "ticket".to_owned(),
            status_name: vec![PayloadPath::of(["fields", "other"])],
            ..KindPaths::default()
        }],
    );
    let id = format!("{source}:T-1");
    let statuses = knobas_core::mini_board::statuses_of(&pool, &[id], &other, None)
        .await
        .unwrap();
    let read: Vec<Option<String>> = statuses.columns.into_iter().map(|(s, _)| s).collect();
    assert_eq!(read, vec![Some("Elsewhere".to_owned())]);
}

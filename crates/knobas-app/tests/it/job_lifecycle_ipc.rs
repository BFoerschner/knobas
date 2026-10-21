//! A job's context is archived while its ticket is done (#1501), at the IPC
//! seam: `promote_context` (what opening a job calls), the inbox's alert rule,
//! the Monitors view's `open_alerts` and `list_contexts`, which the open-jobs
//! list reads.
//!
//! The source is a configured `jira` instance read through the real registry,
//! so the done-like set is the Jira adapter's own declaration. The mirror rows
//! are written in the Jira record's shape; `knobas-sync`'s
//! `tests/it/job_contexts.rs` lands the same moves through a sync run.

use chrono::Utc;
use knobas_app::commands::entity::{inbox_items_inner, promote_context_inner};
use knobas_app::sources::Registry;
use knobas_core::payload::Declarations;
use knobas_sync::scheduler::AdapterRegistry;
use sqlx::PgPool;

const SOURCE: &str = "jira";

async fn scratch(label: &str) -> PgPool {
    let pool = knobas_db::test_util::scratch_database(label)
        .await
        .pool(4)
        .await
        .expect("a pool onto the scratch database");
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind, config)
         values ('jira', 'jira', 'Jira', 'https://jira.example', 'Pat', '{}')",
    )
    .execute(&pool)
    .await
    .expect("the source row");
    pool
}

fn in_status(key: &str, status: &str) -> serde_json::Value {
    serde_json::json!({ "key": key, "fields": { "status": { "name": status } } })
}

/// One mirrored Jira ticket in `status`.
async fn ticket(pool: &PgPool, key: &str, status: &str) -> String {
    let id = format!("{SOURCE}:{key}");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'ticket', $2)")
        .bind(&id)
        .bind(key)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
         values ($1, $2, 'ticket', $3, '', $4)",
    )
    .bind(&id)
    .bind(SOURCE)
    .bind(key)
    .bind(in_status(key, status))
    .execute(pool)
    .await
    .unwrap();
    id
}

/// The mirror's status moves without a sync run: what opening a job reads.
async fn set_status(pool: &PgPool, id: &str, status: &str) {
    let key = id.trim_start_matches("jira:");
    sqlx::query("update sync.item set payload = $2 where entity_id = $1")
        .bind(id)
        .bind(in_status(key, status))
        .execute(pool)
        .await
        .unwrap();
}

/// What a sync run of the `jira` source does to its contexts as it lands.
async fn sync_lands(pool: &PgPool) {
    let declared = Registry::for_process()
        .descriptors()
        .into_iter()
        .find(|d| d.adapter_kind == SOURCE)
        .expect("the Jira adapter is compiled in");
    knobas_core::context::follow_status(
        pool,
        SOURCE,
        &Declarations::empty().with(SOURCE, declared.payload_paths),
    )
    .await
    .unwrap();
}

async fn open_job(pool: &PgPool, id: &str) -> knobas_core::context::Promoted {
    promote_context_inner(pool, &Registry::for_process(), id)
        .await
        .expect("the ticket is in the mirror")
}

/// A context first made for a done ticket is born archived; opening the job
/// again answers with that context, still archived, and makes nothing.
#[tokio::test]
async fn opening_a_done_job_reads_its_archived_context_and_makes_no_second() {
    let pool = scratch("job-lifecycle-born").await;
    let done = ticket(&pool, "PAY-1", "Resolved").await;

    let first = open_job(&pool, &done).await;
    assert!(first.fresh);
    assert!(
        first.context.archived_at.is_some(),
        "Resolved is in Jira's done-like set: {:?}",
        first.context
    );

    let again = open_job(&pool, &done).await;
    assert!(!again.fresh, "the second open makes nothing");
    assert_eq!(again.context.id, first.context.id);
    assert_eq!(again.context.archived_at, first.context.archived_at);
}

/// Opening a job never moves `archived_at`, done or not: a live context stays
/// live when the mirror now says Done, and an archived one stays archived
/// when the mirror now says something else. Only the sync run moves it.
#[tokio::test]
async fn opening_a_job_leaves_archived_at_as_it_was() {
    let pool = scratch("job-lifecycle-open").await;
    let open = ticket(&pool, "PAY-2", "In Progress").await;
    let done = ticket(&pool, "PAY-3", "Done").await;
    let live = open_job(&pool, &open).await.context;
    let put_away = open_job(&pool, &done).await.context;
    assert!(live.archived_at.is_none());
    assert!(put_away.archived_at.is_some());

    set_status(&pool, &open, "Done").await;
    set_status(&pool, &done, "In Progress").await;
    assert_eq!(open_job(&pool, &open).await.context.archived_at, None);
    assert_eq!(
        open_job(&pool, &done).await.context.archived_at,
        put_away.archived_at
    );

    sync_lands(&pool).await;
    assert!(open_job(&pool, &open).await.context.archived_at.is_some());
    assert!(open_job(&pool, &done).await.context.archived_at.is_none());
}

/// An asset held only by a done job's context: its open alert stays out of
/// the inbox (#446's rule over an archived context), shows in the Monitors
/// view, and the open jobs (`list_contexts`) no longer list the job. The
/// control is the same estate while the ticket is in progress.
#[tokio::test]
async fn an_alert_held_only_by_a_done_jobs_context_leaves_the_inbox_and_stays_in_monitors() {
    let pool = scratch("job-lifecycle-alert").await;
    let job = ticket(&pool, "PAY-4", "In Progress").await;
    let ctx = open_job(&pool, &job).await.context.id;

    let asset = "asset:hel1";
    let monitor = "kuma:7";
    for (id, kind, title) in [(asset, "asset", "hel1"), (monitor, "monitor", "hel1 ping")] {
        sqlx::query("insert into knobas.entity (id, kind, title) values ($1, $2, $3)")
            .bind(id)
            .bind(kind)
            .bind(title)
            .execute(&pool)
            .await
            .unwrap();
    }
    sqlx::query("insert into knobas.asset (id, type_id, name) values ($1, 'vm', 'hel1')")
        .bind(asset)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, payload)
         values ($1, 'kuma', 'monitor', 'hel1 ping', '{}'::jsonb)",
    )
    .bind(monitor)
    .execute(&pool)
    .await
    .unwrap();
    for (from, to, relation) in [
        (ctx.as_str(), asset, "related"),
        (monitor, asset, knobas_app::assets::MONITORED_BY),
    ] {
        sqlx::query(
            "insert into knobas.link (from_id, to_id, relation, origin, created_by, confirmed_at)
             values ($1, $2, $3, 'manual', 'user', now())",
        )
        .bind(from)
        .bind(to)
        .bind(relation)
        .execute(&pool)
        .await
        .unwrap();
    }
    sqlx::query(
        "insert into knobas.monitor_alert (entity_id, state, opened_at)
         values ($1, 'down', now() - interval '1 hour')",
    )
    .bind(monitor)
    .execute(&pool)
    .await
    .unwrap();

    let key = format!("alert:{monitor}");
    let inbox = |pool: PgPool| async move {
        inbox_items_inner(
            &pool,
            &Registry::for_process(),
            Utc::now(),
            knobas_core::inbox::Shelf::Stream,
        )
        .await
        .expect("the inbox reads")
        .into_iter()
        .map(|entry| entry.item.key)
        .collect::<Vec<_>>()
    };
    let monitors = |pool: PgPool| async move {
        knobas_app::assets::open_alerts(&pool)
            .await
            .expect("the Monitors view reads")
            .into_iter()
            .map(|alert| alert.monitor_id)
            .collect::<Vec<_>>()
    };
    let open_jobs = |pool: PgPool| async move {
        knobas_core::context::list(&pool)
            .await
            .expect("the contexts list")
            .into_iter()
            .filter_map(|row| row.anchor_id)
            .collect::<Vec<_>>()
    };

    assert!(inbox(pool.clone()).await.contains(&key), "the control");
    assert_eq!(open_jobs(pool.clone()).await, std::slice::from_ref(&job));

    set_status(&pool, &job, "Done").await;
    sync_lands(&pool).await;

    assert!(
        !inbox(pool.clone()).await.contains(&key),
        "a done job's context routes no alert"
    );
    assert_eq!(monitors(pool.clone()).await, [monitor]);
    assert!(
        open_jobs(pool.clone()).await.is_empty(),
        "the open jobs skip the done ticket"
    );
}

/// The one-time pass at bring-up (#1501): contexts made before the lifecycle
/// existed, on tickets the mirror already holds done, are archived; a second
/// bring-up moves nothing. The source has no credential in the store, so no
/// sync run of it can land and be what archived them.
#[tokio::test(flavor = "multi_thread")]
async fn bring_up_archives_the_contexts_of_jobs_already_done_and_a_second_does_nothing() {
    let connector = knobas_db::test_util::scratch_database("job-lifecycle-bring-up").await;
    let pool = connector
        .pool(4)
        .await
        .expect("a pool onto the scratch database");
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind, config)
         values ('jira', 'jira', 'Jira', 'https://jira.example', 'Pat', '{}')",
    )
    .execute(&pool)
    .await
    .expect("the source row");
    let done = ticket(&pool, "PAY-5", "Closed").await;
    let open = ticket(&pool, "PAY-6", "In Review").await;
    let before = |id: String| {
        let pool = pool.clone();
        async move {
            knobas_core::context::promote(
                &pool,
                &knobas_core::entity::EntityRef::parse(&id).unwrap(),
                &Declarations::empty(),
            )
            .await
            .unwrap()
            .unwrap()
            .context
            .id
        }
    };
    let done_ctx = before(done).await;
    let open_ctx = before(open).await;
    let archived_at = |id: String| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, Option<chrono::DateTime<Utc>>>(
                "select archived_at from knobas.context where id = $1",
            )
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap()
        }
    };
    assert_eq!(archived_at(done_ctx.clone()).await, None);

    let app = tauri::test::mock_builder()
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("a mock app");
    let bring_up = || {
        let handle = app.handle().clone();
        let pool = pool.clone();
        let connector = connector.clone();
        tokio::task::block_in_place(move || {
            tauri::async_runtime::block_on(async move {
                let deps = knobas_app::sources::BringUp {
                    suggestions: knobas_app::suggestion_pass::SuggestionPass::beside(&pool),
                    sync_pool: connector.pool(2).await.unwrap(),
                    pool,
                    connections: connector,
                    secrets: std::sync::Arc::new(knobas_secrets::MemoryStore::new()),
                    registry: Registry::for_process(),
                    store: std::env::temp_dir().join("knobas-1501-no-adapters"),
                };
                let state = knobas_app::sources::bring_up(&handle, deps)
                    .await
                    .expect("bring-up over the scratch database");
                state.scheduler.shutdown().await;
            });
        });
    };

    bring_up();
    let first = archived_at(done_ctx.clone()).await;
    assert!(first.is_some(), "the done job's context is archived");
    assert_eq!(archived_at(open_ctx.clone()).await, None);

    bring_up();
    assert_eq!(
        archived_at(done_ctx).await,
        first,
        "the second pass moves nothing"
    );
    assert_eq!(archived_at(open_ctx).await, None);
}

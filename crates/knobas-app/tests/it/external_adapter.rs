//! **An external adapter, through the registry and the sync engine, into the
//! mirror, and out through the launcher's search** (ADR-0028, #1198).
//!
//! The fixture component is built by `just wasm-fixture`, and goes in as an
//! installed adapter does: a [`WasmAdapter`] beside the compiled-in rows.
//! Install itself, from a manifest, is `external_adapter_install`. Every
//! assertion is on a stored row or a search
//! answer, never on a parsed item.

use knobas_app::sources::Registry;
use knobas_source::instance::SourceInstance;
use knobas_sync::scheduler::AdapterRegistry;
use sqlx::PgPool;

async fn pool() -> PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    pool
}

#[tokio::test]
async fn the_fixture_component_s_items_reach_the_mirror_and_search_finds_them() {
    let pool = pool().await;
    let id = format!("wasm-{}", &uuid::Uuid::new_v4().simple().to_string()[..12]);
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
         values ($1, 'wasm-fixture', 'Harbour surveys', '', 'none')",
    )
    .bind(&id)
    .execute(&pool)
    .await
    .unwrap();

    let registry = Registry::with_external(vec![knobas_wasm_host::load_fixture()]);
    let source = match registry.build(SourceInstance {
        id: id.clone(),
        kind: "wasm-fixture".to_owned(),
        display_name: "Harbour surveys".to_owned(),
        base_url: String::new(),
        auth: None,
        secret: None,
        account: None,
        config: serde_json::json!({}),
    }) {
        Ok(source) => source,
        Err(e) => panic!("the registry must build the fixture: {e:?}"),
    };

    let mut conn = knobas_db::test_util::test_connector()
        .await
        .connect()
        .await
        .unwrap();
    let run = knobas_sync::run_from_stored_cursor(&mut conn, &pool, source.as_ref())
        .await
        .unwrap();
    assert_eq!(
        run.upserted, 5,
        "the fixture serves three surveys, a pipeline and a run"
    );

    let (kind, title): (String, String) =
        sqlx::query_as("select kind, title from sync.item where entity_id = $1")
            .bind(format!("{id}:survey-2"))
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        (kind.as_str(), title.as_str()),
        ("survey", "Berth 7 fender survey")
    );

    let found = knobas_app::commands::search::search_inner(
        &pool,
        &Registry::for_process(),
        knobas_search::SearchQuery {
            raw: "quaywright".to_owned(),
            limit: 20,
            filters: knobas_search::SearchFilters {
                sources: vec![id.clone()],
                ..knobas_search::SearchFilters::default()
            },
        },
    )
    .await
    .unwrap();
    assert_eq!(found.total, 3, "{found:?}");
    let group = &found.groups[0];
    assert_eq!(group.kind, "survey");
    let mut titles: Vec<&str> = group.hits.iter().map(|h| h.row.title.as_str()).collect();
    titles.sort_unstable();
    assert_eq!(
        titles,
        [
            "Berth 7 fender survey",
            "Quay crane 4 inspection",
            "Reefer plug audit"
        ]
    );
}

/// The registry the reads are handed: the real one, with the fixture
/// installed, that says when a build has started. The build itself is the
/// real one's, so the fixture's `descriptor()` runs as it does in the app.
struct Entered {
    inner: Registry,
    entered: std::sync::mpsc::Sender<()>,
}

impl AdapterRegistry for Entered {
    fn descriptors(&self) -> Vec<knobas_source::SourceDescriptor> {
        self.inner.templates()
    }

    fn build(
        &self,
        instance: SourceInstance,
    ) -> Result<Box<dyn knobas_source::Source>, knobas_source::SourceError> {
        let _ = self.entered.send(());
        self.inner.build(instance)
    }
}

/// How long the fixture's `descriptor()` waits on every build below.
const DWELL_MS: u64 = 2_000;

/// A source of the fixture, `auth_kind` as stored, whose builds dwell.
async fn dwelling_source(pool: &PgPool, auth_kind: &str) -> String {
    let id = format!("dwell-{}", &uuid::Uuid::new_v4().simple().to_string()[..12]);
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind, config)
         values ($1, 'wasm-fixture', 'Harbour surveys', '', $2, $3)",
    )
    .bind(&id)
    .bind(auth_kind)
    .bind(serde_json::json!({ "descriptor_dwell_ms": DWELL_MS }))
    .execute(pool)
    .await
    .unwrap();
    id
}

/// `read` builds the fixture once, and while that build is in its
/// `descriptor()` another command answers at once (#1705).
///
/// One worker, so a build held on it would hold every other command: the
/// health poll below would wait out the dwell. The build is known to be in
/// flight when the poll is sent (the registry said so) and is still in flight
/// when it answers.
async fn a_dwelling_build_holds_no_worker<F, Fut, T>(pool: PgPool, read: F)
where
    F: FnOnce(std::sync::Arc<dyn AdapterRegistry>) -> Fut,
    Fut: std::future::Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    let (entered, built) = std::sync::mpsc::channel();
    let registry: std::sync::Arc<dyn AdapterRegistry> = std::sync::Arc::new(Entered {
        inner: Registry::with_external(vec![knobas_wasm_host::load_fixture()]),
        entered,
    });
    let reading = tokio::spawn(read(registry));
    // The test's own thread, not the worker, waits here.
    built
        .recv_timeout(std::time::Duration::from_secs(60))
        .expect("the read builds the adapter");

    let asked = std::time::Instant::now();
    let health = tokio::spawn(async move { knobas_app::sources::crud::health(&pool).await })
        .await
        .unwrap();
    let answered_in = asked.elapsed();
    assert!(health.is_ok(), "{:?}", health.err());
    assert!(
        !reading.is_finished(),
        "the build was still in its {DWELL_MS} ms descriptor when the poll answered"
    );
    assert!(
        answered_in < std::time::Duration::from_millis(1_000),
        "a command answered in {answered_in:?} while a build was in flight"
    );
    reading.await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn opening_a_detail_builds_off_the_runtime() {
    let pool = pool().await;
    let id = dwelling_source(&pool, "none").await;
    let read_pool = pool.clone();
    a_dwelling_build_holds_no_worker(pool, |registry| async move {
        let secrets: std::sync::Arc<dyn knobas_secrets::SecretStore> =
            std::sync::Arc::new(knobas_secrets::MemoryStore::new());
        let view = knobas_app::sources::crud::view(
            &read_pool,
            &secrets,
            &registry,
            &format!("{id}:survey-2"),
            None,
        )
        .await;
        assert!(view.is_ok(), "{:?}", view.err());
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn reading_a_source_s_write_ops_builds_off_the_runtime() {
    let pool = pool().await;
    let id = dwelling_source(&pool, "none").await;
    let cfg = knobas_sync::config::get(&pool, &id).await.unwrap().unwrap();
    a_dwelling_build_holds_no_worker(pool, |registry| async move {
        let secrets: std::sync::Arc<dyn knobas_secrets::SecretStore> =
            std::sync::Arc::new(knobas_secrets::MemoryStore::new());
        let ops = knobas_app::sources::crud::instance_write_ops(&secrets, &registry, &cfg).await;
        assert!(!ops.is_empty(), "the fixture declares its source action");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn testing_a_draft_builds_off_the_runtime() {
    let pool = pool().await;
    let read_pool = pool.clone();
    a_dwelling_build_holds_no_worker(pool, |registry| async move {
        let secrets: std::sync::Arc<dyn knobas_secrets::SecretStore> =
            std::sync::Arc::new(knobas_secrets::MemoryStore::new());
        let draft = knobas_app::sources::SourceDraft {
            source_id: None,
            adapter_kind: "wasm-fixture".to_owned(),
            base_url: String::new(),
            auth_kind: knobas_source::AuthMethod::Pat,
            config: serde_json::json!({ "descriptor_dwell_ms": DWELL_MS }),
            secret: Some(knobas_app::sources::SecretInput {
                value: Some("a token".to_owned()),
                ..Default::default()
            }),
        };
        let report = knobas_app::sources::crud::test(&read_pool, &secrets, &registry, draft).await;
        assert!(report.is_ok(), "{:?}", report.err());
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn saving_a_credential_builds_off_the_runtime() {
    let pool = pool().await;
    let id = dwelling_source(&pool, "pat").await;
    let read_pool = pool.clone();
    a_dwelling_build_holds_no_worker(pool, |registry| async move {
        let secrets: std::sync::Arc<dyn knobas_secrets::SecretStore> =
            std::sync::Arc::new(knobas_secrets::MemoryStore::new());
        let health = knobas_app::sources::crud::set_secret(
            &read_pool,
            &secrets,
            &registry,
            &id,
            knobas_app::sources::SecretInput {
                value: Some("a token".to_owned()),
                ..Default::default()
            },
        )
        .await;
        assert!(health.is_ok(), "{:?}", health.err());
    })
    .await;
}

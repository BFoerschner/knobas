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

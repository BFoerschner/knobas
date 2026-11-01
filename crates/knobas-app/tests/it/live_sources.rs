//! `knobas-app live-sources` (#2371): a source per live product, added or
//! updated in place through `sources::crud`, and removed again.

use std::sync::Arc;

use knobas_app::live_sources::{self, Applied};
use knobas_app::sources::live::LiveSources;
use knobas_app::sources::{NewSource, Registry, SecretInput};
use knobas_secrets::{KeychainAccount, MemoryStore, SecretStore};
use knobas_source::AuthMethod;
use knobas_sync::config::AuthState;
use knobas_sync::scheduler::AdapterRegistry;
use sqlx::PgPool;

struct Fixture {
    pool: PgPool,
    secrets: Arc<dyn SecretStore>,
    registry: Arc<dyn AdapterRegistry>,
    live: LiveSources,
    id: String,
}

async fn fixture() -> Fixture {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    Fixture {
        pool,
        secrets: Arc::new(MemoryStore::new()),
        registry: Arc::new(Registry::default()),
        live: LiveSources::default(),
        id: format!("live-{}", &uuid::Uuid::new_v4().simple().to_string()[..12]),
    }
}

/// The mock adapter: it always connects, so the verdict is the store's.
fn source(id: &str, base_url: &str, token: &str) -> NewSource {
    NewSource {
        id: id.to_owned(),
        adapter_kind: "mock".to_owned(),
        display_name: "Live Gitea".to_owned(),
        base_url: base_url.to_owned(),
        auth_kind: AuthMethod::Pat,
        config: serde_json::json!({ "tombstone": false }),
        secret: SecretInput::of(token),
        sync_interval_secs: 300,
        enabled: true,
    }
}

async fn ids_like(pool: &PgPool, id: &str) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM knobas.source_config WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn a_second_run_updates_in_place_and_picks_up_a_rotated_token() {
    let f = fixture().await;
    let first = live_sources::apply(
        &f.pool,
        &f.secrets,
        &f.registry,
        &f.live,
        source(&f.id, "http://127.0.0.1:3000", "token-one"),
    )
    .await
    .unwrap();
    assert_eq!(first.applied, Applied::Added);
    assert_eq!(first.health.state, AuthState::Ok, "the run tests what it saved");

    let second = live_sources::apply(
        &f.pool,
        &f.secrets,
        &f.registry,
        &f.live,
        source(&f.id, "http://127.0.0.1:3002", "token-two"),
    )
    .await
    .unwrap();
    assert_eq!(second.applied, Applied::Updated);
    assert_eq!(ids_like(&f.pool, &f.id).await, 1, "one source per product");

    let stored = f.secrets.get(&KeychainAccount::source(&f.id)).unwrap().unwrap();
    assert_eq!(stored.value, "token-two", "the rotated token is the stored one");
    let base_url: String =
        sqlx::query_scalar("SELECT base_url FROM knobas.source_config WHERE id = $1")
            .bind(&f.id)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(base_url, "http://127.0.0.1:3002", "a moved address is updated");
}

#[tokio::test]
async fn a_source_of_another_kind_under_the_id_is_refused_not_overwritten() {
    let f = fixture().await;
    live_sources::apply(&f.pool, &f.secrets, &f.registry, &f.live, source(&f.id, "http://a", "t"))
        .await
        .unwrap();
    let mut other = source(&f.id, "http://a", "t");
    other.adapter_kind = "gitea".to_owned();
    let refused = live_sources::apply(&f.pool, &f.secrets, &f.registry, &f.live, other).await;
    assert!(refused.is_err(), "a kind never changes under an id (P10)");
}

#[tokio::test]
async fn removing_takes_the_source_and_its_credential_and_tolerates_absence() {
    let f = fixture().await;
    live_sources::apply(&f.pool, &f.secrets, &f.registry, &f.live, source(&f.id, "http://a", "t"))
        .await
        .unwrap();

    assert!(live_sources::remove(&f.pool, &f.secrets, &f.live, &f.id).await.unwrap());
    assert_eq!(ids_like(&f.pool, &f.id).await, 0);
    assert!(f.secrets.get(&KeychainAccount::source(&f.id)).unwrap().is_none());

    assert!(
        !live_sources::remove(&f.pool, &f.secrets, &f.live, &f.id).await.unwrap(),
        "a second remove finds nothing and says so"
    );
}

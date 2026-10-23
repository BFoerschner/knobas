//! The detail view's read (`knobas_core::detail`, moved below the command by
//! #1870), against a real PostgreSQL: which ticket's status the panel shows,
//! and what it says of a source it has no configuration for.
//!
//! The app's `tests/it/entity.rs` drives the whole command; this pins the two
//! rules of the read itself that nothing there reached: a turned-off source's
//! ticket opens but says no status (the board does not draw it either), and an
//! unconfigured source falls back to its id without naming an adapter.

use knobas_core::detail;
use knobas_core::entity::EntityRef;
use knobas_core::payload::{Declarations, KindPaths, PayloadPath};
use sqlx::PgPool;

fn declarations() -> Declarations {
    let ticket = KindPaths {
        kind: "ticket".to_owned(),
        status_name: vec![PayloadPath::of(["fields", "status", "name"])],
        ..KindPaths::default()
    };
    Declarations::empty()
        .with("jira", vec![ticket.clone()])
        .with("jira-off", vec![ticket.clone()])
        .with("loose", vec![ticket])
}

async fn scratch() -> PgPool {
    knobas_db::test_util::scratch_database("detail")
        .await
        .pool(4)
        .await
        .expect("a pool onto this test's own database")
}

/// A mirrored ticket of `source` in status `In Progress`.
async fn ticket(pool: &PgPool, source: &str, key: &str) -> EntityRef {
    let entity = EntityRef::new(source, key);
    let id = entity.to_string();
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
    .bind(source)
    .bind(key)
    .bind(serde_json::json!({ "fields": { "status": { "name": "In Progress" } } }))
    .execute(pool)
    .await
    .unwrap();
    entity
}

async fn configure(pool: &PgPool, id: &str, enabled: bool) {
    sqlx::query(
        "insert into knobas.source_config
             (id, kind, display_name, base_url, auth_kind, enabled)
         values ($1, 'jira', 'Jira', 'http://localhost', 'pat', $2)",
    )
    .bind(id)
    .bind(enabled)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn a_turned_off_sources_ticket_opens_and_says_no_status() {
    let pool = scratch().await;
    configure(&pool, "jira", true).await;
    configure(&pool, "jira-off", false).await;
    let on = ticket(&pool, "jira", "PAY-1").await;
    let off = ticket(&pool, "jira-off", "PAY-2").await;

    let on = detail::read(&pool, &declarations(), &on)
        .await
        .unwrap()
        .unwrap();
    let off = detail::read(&pool, &declarations(), &off)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(on.status.as_deref(), Some("In Progress"));
    assert!(on.source.enabled);
    assert_eq!(
        off.status, None,
        "the board does not draw it, so neither does the panel"
    );
    assert!(
        !off.source.enabled,
        "it still opens, and says its source is off"
    );
}

#[tokio::test]
async fn an_unconfigured_source_falls_back_to_its_id_and_names_no_adapter() {
    let pool = scratch().await;
    let entity = ticket(&pool, "loose", "LOOSE-1").await;

    let found = detail::read(&pool, &declarations(), &entity)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(found.source.display_name, "loose");
    assert_eq!(found.source.adapter_kind, "loose");
    assert!(
        found.source.enabled,
        "absence of configuration is no decision"
    );
    assert_eq!(
        found.configured_adapter_kind, None,
        "the adapter's word for a kind is looked up by a configured kind only"
    );
}

#[tokio::test]
async fn nothing_in_the_mirror_is_none() {
    let pool = scratch().await;
    let found = detail::read(&pool, &declarations(), &EntityRef::new("jira", "NOPE-1"))
        .await
        .unwrap();
    assert!(found.is_none());
}

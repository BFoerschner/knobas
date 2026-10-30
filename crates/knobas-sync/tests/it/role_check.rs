//! The role data check in the engine (#1199, ADR-0028 *The checks*): a
//! source's first sync holds the paths its claimed roles need to the records
//! it emitted, keeps a miss without stopping the sync, and a later sync whose
//! records resolve the path fills the role again.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use knobas_core::payload::{KindPaths, ListPath, ParentPath, PayloadPath};
use knobas_source::role::data::RoleMiss;
use knobas_source::role::{RoleClaim, TICKET_SYSTEM};
use knobas_source::{
    ConnectionInfo, Cursor, KindInfo, Sink, Source, SourceDescriptor, SourceError, SyncItem,
    WriteOp,
};
use serde_json::{Value, json};
use sqlx::PgPool;

/// A ticket system whose next run emits the tickets in `next`, one payload
/// each, and whose priority is declared at `fields.priority.name`.
struct Tickets {
    id: String,
    next: Arc<Mutex<Vec<Value>>>,
}

#[async_trait]
impl Source for Tickets {
    async fn view(
        &self,
        _entity: &str,
        _section: Option<&str>,
    ) -> Result<Option<knobas_source::view::SourceView>, SourceError> {
        Ok(None)
    }

    fn descriptor(&self) -> SourceDescriptor {
        let at = |segments: &[&str]| vec![PayloadPath::of(segments.iter().copied())];
        SourceDescriptor {
            id: self.id.clone(),
            adapter_kind: "tickets".into(),
            name: "Tickets".into(),
            capabilities: Vec::new(),
            adapter_version: "0.1.0".into(),
            auth_methods: Vec::new(),
            accepts_account: false,
            min_sync_interval_secs: 60,
            write_ops: Vec::new(),
            entity_kinds: vec![KindInfo {
                id: "ticket".into(),
                label: "Ticket".into(),
                plural: "Tickets".into(),
                monogram: "TI".into(),
                full_sync_exhaustive: false,
                refreshes: true,
            }],
            config_schema: json!({ "type": "object", "properties": {} }),
            payload_paths: vec![KindPaths {
                kind: "ticket".into(),
                status_name: at(&["fields", "status", "name"]),
                priority: at(&["fields", "priority", "name"]),
                assignee: at(&["fields", "assignee", "name"]),
                issue_type: at(&["fields", "issuetype", "name"]),
                project_key: at(&["fields", "project", "key"]),
                labels: vec![ListPath {
                    at: PayloadPath::of(["fields", "labels"]),
                    entry: PayloadPath::default(),
                }],
                parent: vec![ParentPath {
                    kind: "ticket".into(),
                    at: PayloadPath::of(["fields", "parent"]),
                    names: vec![PayloadPath::of(["key"])],
                }],
                ..KindPaths::default()
            }],
            roles: vec![RoleClaim::new(TICKET_SYSTEM, ["reachable_transitions"])],
            source_actions: Vec::new(),
            clone_from: Vec::new(),
            url_forms: Vec::new(),
            code_pages: None,
        }
    }

    async fn test_connection(&self) -> Result<ConnectionInfo, SourceError> {
        Ok(ConnectionInfo::default())
    }

    async fn sync(
        &self,
        cursor: Option<Cursor>,
        sink: &mut (dyn Sink + Send),
    ) -> Result<Cursor, SourceError> {
        let payloads = std::mem::take(&mut *self.next.lock().unwrap());
        for (n, payload) in payloads.into_iter().enumerate() {
            sink.item(SyncItem {
                entity: knobas_core::entity::EntityRef::new(&self.id, &format!("T-{n}")),
                kind: "ticket".into(),
                title: format!("ticket {n}"),
                body_text: String::new(),
                author: None,
                updated_at: None,
                payload,
                web_url: None,
                deleted: false,
            })
            .await?;
        }
        let n = cursor.map_or(0, |c| c.len());
        Ok(format!("{n}x"))
    }

    async fn write(&self, _op: WriteOp) -> Result<knobas_source::WriteReceipt, SourceError> {
        Err(SourceError::protocol("read-only"))
    }

    async fn reachable_transitions(&self, _entity: &str) -> Result<Vec<String>, SourceError> {
        Ok(Vec::new())
    }
}

/// A ticket whose priority sits under `priority_key`.
fn ticket(priority_key: &str) -> Value {
    json!({ "fields": {
        "status": { "name": "Open" },
        priority_key: { "name": "High" },
        "assignee": null,
        "issuetype": { "name": "Bug" },
        "project": { "key": "PAY" },
        "labels": [],
        "parent": { "key": "PAY-1" },
    }})
}

async fn pool() -> PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    pool
}

async fn configured(pool: &PgPool) -> Tickets {
    let id = format!("rc-{}", uuid::Uuid::new_v4().simple());
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
         values ($1, 'tickets', 'Tickets', '', 'none')",
    )
    .bind(&id)
    .execute(pool)
    .await
    .unwrap();
    Tickets {
        id,
        next: Arc::default(),
    }
}

async fn run(pool: &PgPool, source: &Tickets, tickets: Vec<Value>) {
    *source.next.lock().unwrap() = tickets;
    let mut conn = knobas_db::test_util::test_connector()
        .await
        .connect()
        .await
        .expect("a connection of the run's own");
    knobas_sync::run_from_stored_cursor(&mut conn, pool, source)
        .await
        .expect("the run commits");
}

async fn misses(pool: &PgPool, source: &Tickets) -> Vec<RoleMiss> {
    knobas_sync::role_check::misses(pool)
        .await
        .unwrap()
        .remove(&source.id)
        .unwrap_or_default()
}

async fn live_items(pool: &PgPool, source: &Tickets) -> i64 {
    sqlx::query_scalar("select count(*) from sync.live_item where source_id = $1")
        .bind(&source.id)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn a_first_sync_whose_records_miss_a_path_keeps_the_miss_and_its_items() {
    let pool = pool().await;
    let source = configured(&pool).await;
    run(&pool, &source, vec![ticket("prio"), ticket("prio")]).await;

    assert_eq!(
        misses(&pool, &source).await,
        [RoleMiss {
            role: TICKET_SYSTEM.into(),
            kind: "ticket".into(),
            field: "priority".into(),
            path: "fields.priority.name".into(),
        }]
    );
    assert_eq!(live_items(&pool, &source).await, 2, "the items stay live");
}

#[tokio::test]
async fn a_later_sync_whose_records_resolve_the_path_fills_the_role_again() {
    let pool = pool().await;
    let source = configured(&pool).await;
    run(&pool, &source, vec![ticket("prio")]).await;
    assert_eq!(misses(&pool, &source).await.len(), 1);

    // An idle run is no evidence: the miss stands.
    run(&pool, &source, vec![]).await;
    assert_eq!(misses(&pool, &source).await.len(), 1);

    run(&pool, &source, vec![ticket("priority")]).await;
    assert_eq!(misses(&pool, &source).await, []);
}

/// A role once seen filled is not judged again: a later run's few records
/// are no corpus.
#[tokio::test]
async fn a_role_seen_filled_is_not_judged_again() {
    let pool = pool().await;
    let source = configured(&pool).await;
    run(&pool, &source, vec![ticket("priority")]).await;
    run(&pool, &source, vec![ticket("prio")]).await;
    assert_eq!(misses(&pool, &source).await, []);
}

/// A first sync that emitted nothing of the role's kinds judged nothing, so
/// the next one with records still judges.
#[tokio::test]
async fn a_first_sync_with_no_records_of_the_kind_leaves_the_role_unjudged() {
    let pool = pool().await;
    let source = configured(&pool).await;
    run(&pool, &source, vec![]).await;
    run(&pool, &source, vec![ticket("prio")]).await;
    assert_eq!(misses(&pool, &source).await.len(), 1);
}

#[tokio::test]
async fn removing_the_source_takes_its_misses_with_it() {
    let pool = pool().await;
    let source = configured(&pool).await;
    run(&pool, &source, vec![ticket("prio")]).await;
    assert!(
        knobas_sync::config::delete(&pool, &source.id, false)
            .await
            .unwrap()
    );
    assert_eq!(misses(&pool, &source).await, []);
}

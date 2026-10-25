//! A job's context follows its ticket's status through `knobas_sync::run_once`
//! (#1501): a run that lands a done-like status archives the context, and a
//! later run that lands a status outside the set revives it.
//!
//! The fake declares what the Jira adapter declares for a ticket's status and
//! its done-like set, and nothing else.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use knobas_core::entity::EntityRef;
use knobas_source::{
    Capability, ConnectionInfo, Cursor, KindInfo, KindPaths, PayloadPath, Sink, Source,
    SourceDescriptor, SourceError, SyncItem, WriteOp,
};
use serde_json::json;
use sqlx::PgPool;

const SOURCE: &str = "jira";

/// Each ticket's key and status, as the next run lands them.
type Statuses = Arc<Mutex<Vec<(&'static str, &'static str)>>>;

struct Tickets {
    statuses: Statuses,
}

#[async_trait]
impl Source for Tickets {
    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: SOURCE.into(),
            adapter_kind: SOURCE.into(),
            name: "Jira".into(),
            capabilities: Vec::<Capability>::new(),
            adapter_version: "0.1.0".into(),
            auth_methods: Vec::new(),
            accepts_account: false,
            min_sync_interval_secs: 60,
            roles: Vec::new(),
            source_actions: Vec::new(),
            clone_from: Vec::new(),
            url_forms: Vec::new(),
            write_ops: Vec::new(),
            entity_kinds: vec![KindInfo {
                id: "ticket".into(),
                label: "Ticket".into(),
                plural: "Tickets".into(),
                monogram: "TK".into(),
                full_sync_exhaustive: true,
                refreshes: true,
            }],
            config_schema: json!({ "type": "object", "properties": {} }),
            payload_paths: vec![KindPaths {
                kind: "ticket".into(),
                status_name: vec![PayloadPath::of(["fields", "status", "name"])],
                done_statuses: vec!["Done".into(), "Closed".into(), "Resolved".into()],
                ..KindPaths::default()
            }],
        }
    }

    async fn test_connection(&self) -> Result<ConnectionInfo, SourceError> {
        Ok(ConnectionInfo::default())
    }

    async fn sync(
        &self,
        _cursor: Option<Cursor>,
        sink: &mut (dyn Sink + Send),
    ) -> Result<Cursor, SourceError> {
        let statuses = self.statuses.lock().unwrap().clone();
        for (key, status) in statuses {
            sink.item(SyncItem {
                entity: EntityRef::new(SOURCE, key),
                kind: "ticket".to_owned(),
                title: key.to_owned(),
                body_text: String::new(),
                author: None,
                updated_at: None,
                payload: json!({ "key": key, "fields": { "status": { "name": status } } }),
                web_url: None,
                deleted: false,
            })
            .await?;
        }
        Ok("c".to_owned())
    }

    async fn write(&self, _op: WriteOp) -> Result<knobas_source::WriteReceipt, SourceError> {
        Err(SourceError::protocol("read-only"))
    }

    async fn reachable_transitions(&self, _entity: &str) -> Result<Vec<String>, SourceError> {
        Err(SourceError::protocol("no workflow"))
    }

    async fn view(
        &self,
        _entity: &str,
        _section: Option<&str>,
    ) -> Result<Option<knobas_source::view::SourceView>, SourceError> {
        Ok(None)
    }
}

async fn pool(label: &str) -> PgPool {
    let pool = knobas_db::test_util::scratch_database(label)
        .await
        .pool(2)
        .await
        .expect("a pool on the scratch database");
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind, config)
         values ('jira', 'jira', 'Jira', 'https://jira.example', 'Pat', '{}')",
    )
    .execute(&pool)
    .await
    .expect("the source row");
    pool
}

async fn archived(pool: &PgPool, ctx: &str) -> bool {
    sqlx::query_scalar::<_, bool>(
        "select archived_at is not null from knobas.context where id = $1",
    )
    .bind(ctx)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// The ticket moved Done and back, each move landed by a run: the context is
/// archived while the ticket is done, in any case, and revived when it leaves.
/// A ticket of the same source that stays open keeps its context live.
#[tokio::test]
async fn a_run_archives_a_done_tickets_context_and_a_later_run_revives_it() {
    let pool = pool("job_contexts_follow").await;
    let statuses: Statuses = Arc::new(Mutex::new(vec![
        ("PAY-1", "In Progress"),
        ("PAY-2", "In Progress"),
    ]));
    let source = Tickets {
        statuses: Arc::clone(&statuses),
    };
    knobas_sync::run_once(&pool, &source, None).await.unwrap();

    let promote = |key: &'static str| {
        let pool = pool.clone();
        async move {
            knobas_core::context::promote(
                &pool,
                &EntityRef::new(SOURCE, key),
                &knobas_core::payload::Declarations::empty(),
            )
            .await
            .unwrap()
            .unwrap()
            .context
            .id
        }
    };
    let job = promote("PAY-1").await;
    let other = promote("PAY-2").await;
    assert!(!archived(&pool, &job).await);

    *statuses.lock().unwrap() = vec![("PAY-1", "DONE"), ("PAY-2", "In Progress")];
    knobas_sync::run_once(&pool, &source, None).await.unwrap();
    assert!(archived(&pool, &job).await, "Done, in any case, archives");
    assert!(
        !archived(&pool, &other).await,
        "an open ticket's stays live"
    );

    *statuses.lock().unwrap() = vec![("PAY-1", "Reopened"), ("PAY-2", "In Progress")];
    knobas_sync::run_once(&pool, &source, None).await.unwrap();
    assert!(!archived(&pool, &job).await, "leaving Done revives it");
}

//! A write goes in the background (issue #752): `submit_write` answers once
//! the row is queued, and the send happens after.
//!
//! The source on the other end is a trait-level fake whose `write` **blocks
//! until the test releases it**, so "answered before the send" is a fact the
//! test can observe rather than a race it hopes to win: with the old submit,
//! which flushed before it answered, the call below never returns while the
//! adapter is held, and the timeout says so.
//!
//! What a source does with a write is the live suites' to witness (ADR-0013);
//! what is claimed here is knobas' own order of events: queued, answered,
//! sent, announced -- and the queue's per-source order kept by the background
//! flush.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use knobas_app::sources::SourcesState;
use knobas_core::activity::ActivityRow;
use knobas_core::write_queue::WriteState;
use knobas_secrets::{MemoryStore, Secret, SecretStore};
use knobas_source::{
    AuthMethod, ConnectionInfo, Cursor, KindInfo, Sink, Source, SourceDescriptor, SourceError,
    WriteOp, WriteReceipt,
};
use knobas_sync::scheduler::{AdapterRegistry, RunConnections, SchedulerDeps, SyncEvents};
use serde_json::json;
use tokio::sync::{Semaphore, mpsc};

const SOURCE: &str = "gitea";
const PR: &str = "gitea:knobas/app#7";

/// Long enough for a loaded gate machine; a submit that waits on the held
/// adapter waits for ever, so the margin costs nothing on the passing side.
const PATIENCE: Duration = Duration::from_secs(20);

/// What the fake was asked to write, in the order its writes **finished**.
#[derive(Default)]
struct Wrote {
    entered: Mutex<usize>,
    ops: Mutex<Vec<WriteOp>>,
}

struct Held {
    wrote: Arc<Wrote>,
    /// One permit per write the test lets through.
    gate: Arc<Semaphore>,
}

#[async_trait]
impl Source for Held {
    async fn view(
        &self,
        _entity: &str,
        _section: Option<&str>,
    ) -> Result<Option<knobas_source::view::SourceView>, SourceError> {
        Ok(None)
    }

    fn descriptor(&self) -> SourceDescriptor {
        descriptor()
    }

    async fn test_connection(&self) -> Result<ConnectionInfo, SourceError> {
        Ok(ConnectionInfo::default())
    }

    async fn sync(
        &self,
        cursor: Option<Cursor>,
        _sink: &mut (dyn Sink + Send),
    ) -> Result<Cursor, SourceError> {
        Ok(cursor.unwrap_or_default())
    }

    async fn write(&self, op: WriteOp) -> Result<WriteReceipt, SourceError> {
        *self.wrote.entered.lock().unwrap() += 1;
        self.gate
            .acquire()
            .await
            .expect("the gate is never closed")
            .forget();
        self.wrote.ops.lock().unwrap().push(op);
        Ok(WriteReceipt::none())
    }

    async fn reachable_transitions(&self, entity: &str) -> Result<Vec<String>, SourceError> {
        Err(SourceError::protocol(format!("no workflow for {entity:?}")))
    }
}

fn descriptor() -> SourceDescriptor {
    SourceDescriptor {
        id: SOURCE.to_owned(),
        adapter_kind: SOURCE.to_owned(),
        name: "Gitea".to_owned(),
        capabilities: vec![knobas_source::Capability::Write],
        adapter_version: "0".to_owned(),
        auth_methods: vec![AuthMethod::Pat],
        accepts_account: false,
        min_sync_interval_secs: 60,
        write_ops: vec![
            "comment".to_owned(),
            "create_branch".to_owned(),
            "create_page".to_owned(),
        ],
        entity_kinds: vec![KindInfo {
            id: "pull_request".to_owned(),
            label: "Pull request".to_owned(),
            plural: "Pull requests".to_owned(),
            monogram: "PR".to_owned(),
            full_sync_exhaustive: true,
        }],
        config_schema: json!({"type": "object", "properties": {}}),
        payload_paths: Vec::new(),
    }
}

struct Registry {
    wrote: Arc<Wrote>,
    gate: Arc<Semaphore>,
}

impl AdapterRegistry for Registry {
    fn descriptors(&self) -> Vec<SourceDescriptor> {
        vec![descriptor()]
    }

    fn build(
        &self,
        _instance: knobas_source::instance::SourceInstance,
    ) -> Result<Box<dyn Source>, SourceError> {
        Ok(Box::new(Held {
            wrote: Arc::clone(&self.wrote),
            gate: Arc::clone(&self.gate),
        }))
    }
}

struct Connections(knobas_db::embedded::Connector);

#[async_trait]
impl RunConnections for Connections {
    async fn open(&self) -> Result<sqlx::PgConnection, sqlx::Error> {
        self.0.connect().await
    }
}

/// `activity:new`, as the shell hears it: every line, in order.
struct Heard(mpsc::UnboundedSender<ActivityRow>);

impl SyncEvents for Heard {
    fn sync_state(&self, _status: knobas_sync::SourceSyncStatus) {}
    fn source_health(&self, _health: knobas_sync::CredentialHealth) {}
    fn activity_new(&self, row: ActivityRow) {
        let _ = self.0.send(row);
    }
}

struct Fixture {
    state: SourcesState,
    wrote: Arc<Wrote>,
    gate: Arc<Semaphore>,
    heard: mpsc::UnboundedReceiver<ActivityRow>,
}

async fn fixture(name: &str) -> Fixture {
    let connector = knobas_db::test_util::scratch_database(name).await;
    let pool = connector.pool(4).await.expect("a pool");

    let secrets = Arc::new(MemoryStore::new());
    secrets
        .put(
            &knobas_secrets::KeychainAccount::source(SOURCE),
            &Secret::just(AuthMethod::Pat, "a-token"),
        )
        .expect("the credential is stored");
    knobas_sync::config::insert(
        &pool,
        &knobas_sync::config::InsertConfig {
            id: SOURCE.to_owned(),
            adapter_kind: SOURCE.to_owned(),
            display_name: "Gitea".to_owned(),
            base_url: "https://gitea.example".to_owned(),
            auth_kind: knobas_sync::config::AuthKind::Method(AuthMethod::Pat),
            config: json!({}),
            sync_interval_secs: 86_400,
            enabled: true,
        },
    )
    .await
    .expect("the source row is written");

    let wrote = Arc::new(Wrote::default());
    let gate = Arc::new(Semaphore::new(0));
    let registry: Arc<dyn AdapterRegistry> = Arc::new(Registry {
        wrote: Arc::clone(&wrote),
        gate: Arc::clone(&gate),
    });
    let (tell, heard) = mpsc::unbounded_channel();
    let scheduler = knobas_sync::scheduler::Scheduler::start(SchedulerDeps {
        pool: pool.clone(),
        connections: Arc::new(Connections(connector)),
        registry: Arc::clone(&registry),
        secrets: secrets.clone(),
        events: Arc::new(Heard(tell)),
        // The tick held off for the whole test: it flushes the queue too, and
        // a send the tick made would pass these tests with no background send
        // at all. So whatever delivers below is `submit_in_background`'s.
        timing: knobas_sync::scheduler::SchedulerTiming {
            startup_delay: Duration::from_secs(3600),
            tick: Duration::from_secs(3600),
        },
    })
    .await
    .expect("a scheduler");

    Fixture {
        state: SourcesState {
            pool,
            scheduler,
            secrets,
            registry,
        },
        wrote,
        gate,
        heard,
    }
}

fn comment(body: &str) -> serde_json::Value {
    json!({ "Comment": { "entity": PR, "body": body } })
}

/// Wait for the `sent` line of write `id`, skipping every other line.
async fn sent_line(heard: &mut mpsc::UnboundedReceiver<ActivityRow>, id: i64) -> ActivityRow {
    tokio::time::timeout(PATIENCE, async {
        loop {
            let row = heard.recv().await.expect("the event channel stays open");
            if row.verb == "sent" && row.detail["write_id"] == json!(id) {
                return row;
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("no `sent` line for write {id}"))
}

/// The issue's first criterion: the call answers while the adapter is still
/// held, with the row as queued; the release lets the background send settle
/// it, and `activity:new` names the write.
#[tokio::test]
async fn submit_answers_before_the_adapter_is_called_and_the_settle_names_the_write() {
    let Fixture {
        state,
        wrote,
        gate,
        mut heard,
    } = fixture("background_write_answers").await;

    let queued = tokio::time::timeout(
        PATIENCE,
        knobas_app::sources::write_queue::submit_in_background(&state, comment("on my way")),
    )
    .await
    .expect("submit answered while the adapter was held -- it must not wait for the send")
    .expect("the comment is queued");

    assert_eq!(queued.state, WriteState::Pending, "the row as queued");
    assert!(
        wrote.ops.lock().unwrap().is_empty(),
        "nothing has reached the source yet"
    );

    gate.add_permits(1);
    let line = sent_line(&mut heard, queued.id).await;
    assert_eq!(line.entity_id.as_deref(), Some(PR));
    assert_eq!(line.detail["state"], json!("sent"));
    assert_eq!(wrote.ops.lock().unwrap().len(), 1);
    assert_eq!(*wrote.entered.lock().unwrap(), 1, "sent once");

    let row = knobas_core::write_queue::get(&state.pool, queued.id)
        .await
        .expect("the queue reads")
        .expect("the row is there");
    assert_eq!(row.state, WriteState::Sent);
}

/// The queue's order survives the move to the background: two comments
/// queued back to back for one source reach it in the order they were
/// queued, however the two background flushes interleave.
#[tokio::test]
async fn two_writes_queued_back_to_back_are_sent_in_order() {
    let Fixture {
        state,
        wrote,
        gate,
        mut heard,
    } = fixture("background_write_order").await;

    // Bounded, so a submit that waited for the held adapter fails here rather
    // than hanging the binary.
    let submit = |body: &'static str| {
        tokio::time::timeout(
            PATIENCE,
            knobas_app::sources::write_queue::submit_in_background(&state, comment(body)),
        )
    };
    let first = submit("first")
        .await
        .expect("answered while the adapter was held")
        .expect("the first is queued");
    let second = submit("second")
        .await
        .expect("answered while the adapter was held")
        .expect("the second is queued");

    gate.add_permits(2);
    sent_line(&mut heard, first.id).await;
    sent_line(&mut heard, second.id).await;

    let bodies: Vec<String> = wrote
        .ops
        .lock()
        .unwrap()
        .iter()
        .map(|op| match op {
            WriteOp::Comment { body, .. } => body.clone(),
            other => panic!("only comments were queued, not {other:?}"),
        })
        .collect();
    assert_eq!(bodies, ["first", "second"]);
}

/// #1050: `submit_write` with `link_to` holds the ticket on the queued row
/// before anything is sent, so a write that waits keeps what it owes. Read
/// while the adapter is held, the row already carries the ticket: no send,
/// no re-read and no sync has run yet to put it there.
#[tokio::test]
async fn a_linked_submit_holds_its_ticket_on_the_row_before_the_send() {
    const TICKET: &str = "jira:PAY-231";
    let Fixture { state, .. } = fixture("background_write_link_held").await;
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'ticket', 'Retry')")
        .bind(TICKET)
        .execute(&state.pool)
        .await
        .expect("the ticket is mirrored");

    let branch = json!({ "CreateBranch": {
        "entity": "gitea:knobas/app", "name": "feature/PAY-231", "from_ref": "main"
    }});
    let queued = tokio::time::timeout(
        PATIENCE,
        knobas_app::sources::write_queue::submit_linked_in_background(
            &state,
            branch,
            TICKET.to_owned(),
        ),
    )
    .await
    .expect("answered while the adapter was held")
    .expect("the branch is queued");

    let owed: Option<String> =
        sqlx::query_scalar("select link_to from knobas.write_queue where id = $1")
            .bind(queued.id)
            .fetch_one(&state.pool)
            .await
            .expect("the queue reads");
    assert_eq!(owed.as_deref(), Some(TICKET), "the row holds what it owes");
}

/// #1121: *New page* with *Link it to <job>* (`docs_create_page`'s
/// `link_to`) holds the ticket on the queued `CreatePage` before the send,
/// as a linked `submit_write` does, so a page whose write waits is still
/// linked when it lands; without `link_to` the row owes nothing. Both read
/// while the adapter is held, and both are sent once it is released.
#[tokio::test]
async fn a_new_page_with_link_it_to_holds_its_ticket_on_the_row_before_the_send() {
    const TICKET: &str = "jira:PAY-231";
    let Fixture {
        state,
        gate,
        mut heard,
        ..
    } = fixture("background_write_page_link_held").await;
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'ticket', 'Retry')")
        .bind(TICKET)
        .execute(&state.pool)
        .await
        .expect("the ticket is mirrored");
    let page = |title: &str| {
        json!({ "CreatePage": {
            "parent": "gitea:100", "space": "ENG", "title": title, "body": "<p>Steps</p>"
        }})
    };

    let mut owes = Vec::new();
    for (title, link_to) in [("Linked", Some(TICKET.to_owned())), ("Unlinked", None)] {
        let queued = tokio::time::timeout(
            PATIENCE,
            knobas_app::commands::entity::create_page_in_background(&state, page(title), link_to),
        )
        .await
        .expect("answered while the adapter was held")
        .expect("the page is queued");
        let owed: Option<String> =
            sqlx::query_scalar("select link_to from knobas.write_queue where id = $1")
                .bind(queued.id)
                .fetch_one(&state.pool)
                .await
                .expect("the queue reads");
        owes.push((queued.id, owed));
    }
    assert_eq!(owes[0].1.as_deref(), Some(TICKET), "the linked page owes");
    assert_eq!(owes[1].1, None, "the unlinked page owes nothing");

    gate.add_permits(2);
    for (id, _) in &owes {
        sent_line(&mut heard, *id).await;
    }
}

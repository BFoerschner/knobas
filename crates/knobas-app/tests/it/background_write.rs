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
        roles: Vec::new(),
        source_actions: Vec::new(),
        clone_from: Vec::new(),
        url_forms: Vec::new(),
        write_ops: vec![
            "comment".to_owned(),
            "create_branch".to_owned(),
            "create_pull_request".to_owned(),
            "create_page".to_owned(),
            "log_work".to_owned(),
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
    fixture_on(knobas_db::test_util::scratch_database(name).await, HELD_OFF).await
}

/// [`fixture`] on `connector`'s database, its scheduler on `timing`.
async fn fixture_on(
    connector: knobas_db::embedded::Connector,
    timing: knobas_sync::scheduler::SchedulerTiming,
) -> Fixture {
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
        timing,
        quit: knobas_sync::scheduler::Quit::default(),
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

const HELD_OFF: knobas_sync::scheduler::SchedulerTiming = knobas_sync::scheduler::SchedulerTiming {
    startup_delay: Duration::from_secs(3600),
    tick: Duration::from_secs(3600),
    shutdown_grace: knobas_sync::scheduler::SHUTDOWN_GRACE,
};

/// [`HELD_OFF`], with a quit's grace far past [`PATIENCE`] (#1920): a run the
/// quit waited out to the grace's end holds the quit for an hour, so a quit
/// that finishes inside the patience waited for nothing but what it had to.
/// A clock reading against the 3 s grace said the same thing only while the
/// machine was quick.
const HELD_OFF_PATIENT_QUIT: knobas_sync::scheduler::SchedulerTiming =
    knobas_sync::scheduler::SchedulerTiming {
        shutdown_grace: Duration::from_secs(3600),
        ..HELD_OFF
    };

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

/// LIFE-2 (#1633, spec #1619 ruling B1): Cmd-Q while a background send is at
/// the source. The quit waits for it under the scheduler's grace, so its
/// `sent` mark lands before the pool closes, and a submit made once the quit
/// has begun is queued and not sent: the next start's tick sends it, once.
///
/// Before, the quit neither waited for the send nor stopped new ones: the
/// pool closed under the send, `sent` failed, the row stayed pending and the
/// next start sent the comment again. And until #1924 the first comment's
/// flush, once it landed, took its entity's next head and sent the comment
/// submitted after the quit began: its stop never fired. On an hour's grace
/// that send holds the quit at the gate, and the hang guard below fails.
#[tokio::test(flavor = "multi_thread")]
async fn a_send_at_the_source_at_quit_lands_and_a_submit_after_waits_for_the_next_start() {
    use tauri::Manager as _;

    let connector = knobas_db::test_util::scratch_database("background_write_quit").await;
    let Fixture {
        state, wrote, gate, ..
    } = fixture_on(connector.clone(), HELD_OFF_PATIENT_QUIT).await;
    let app = tauri::test::mock_builder()
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("a mock app");
    app.manage(state.clone());

    let first =
        knobas_app::sources::write_queue::submit_in_background(&state, comment("before the quit"))
            .await
            .expect("the first comment is queued");
    tokio::time::timeout(PATIENCE, async {
        while *wrote.entered.lock().unwrap() == 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the first comment reaches the source");

    let (over, quit_over) = std::sync::mpsc::channel();
    let quitting = std::thread::spawn({
        let handle = app.handle().clone();
        move || {
            knobas_app::shutdown(&handle);
            let _ = over.send(());
        }
    });
    tokio::time::timeout(PATIENCE, async {
        while !state.scheduler.is_shutting_down() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("the quit begins");
    let after = knobas_app::sources::write_queue::submit_in_background(
        &state,
        comment("after the quit began"),
    )
    .await
    .expect("a submit during the quit is still queued");
    // One permit, for the first comment. A send of the second, by a task
    // spawned for it or by the first comment's flush taking its entity's
    // next head (#1924), waits at the gate inside the background set, and
    // the grace is an hour: a quit over inside the patience started no
    // write after it began.
    gate.add_permits(1);
    quit_over
        .recv_timeout(PATIENCE)
        .expect("the quit waited out its grace: a write started after it began");
    quitting.join().expect("the quit finishes");
    let entered = *wrote.entered.lock().unwrap();

    let reader = connector.pool(1).await.unwrap();
    let state_of = async |id: i64| {
        knobas_core::write_queue::get(&reader, id)
            .await
            .unwrap()
            .expect("the row is there")
            .state
    };
    assert_eq!(entered, 1, "the submit made after the quit began was sent");
    assert_eq!(
        state_of(first.id).await,
        WriteState::Sent,
        "the send in flight at the quit did not land its sent mark"
    );
    assert_eq!(state_of(after.id).await, WriteState::Pending);

    // The next start: its tick sends what is owed.
    let secrets = Arc::new(MemoryStore::new());
    secrets
        .put(
            &knobas_secrets::KeychainAccount::source(SOURCE),
            &Secret::just(AuthMethod::Pat, "a-token"),
        )
        .expect("the credential is stored");
    let (tell, _heard) = mpsc::unbounded_channel();
    let next = knobas_sync::scheduler::Scheduler::start(SchedulerDeps {
        pool: connector.pool(2).await.unwrap(),
        connections: Arc::new(Connections(connector.clone())),
        registry: Arc::new(Registry {
            wrote: Arc::clone(&wrote),
            gate: Arc::clone(&gate),
        }),
        secrets,
        events: Arc::new(Heard(tell)),
        timing: knobas_sync::scheduler::SchedulerTiming {
            startup_delay: Duration::from_millis(10),
            tick: Duration::from_millis(50),
            ..Default::default()
        },
        quit: knobas_sync::scheduler::Quit::default(),
    })
    .await
    .expect("the next start's scheduler");
    gate.add_permits(10);
    tokio::time::timeout(PATIENCE, async {
        while state_of(after.id).await != WriteState::Sent {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the next start's tick sends the comment the quit left queued");
    next.shutdown().await;

    let bodies: Vec<String> = wrote
        .ops
        .lock()
        .unwrap()
        .iter()
        .map(|op| match op {
            WriteOp::Comment { body, .. } => body.clone(),
            other => panic!("not a comment: {other:?}"),
        })
        .collect();
    assert_eq!(
        bodies,
        ["before the quit", "after the quit began"],
        "each comment reaches the source once across both starts"
    );
}

/// #1751: LIFE-2 for a start-work run. Cmd-Q while the run behind *Run* has
/// its branch step at the source. The run is in the scheduler's background
/// set, so the quit waits for that send and its `sent` mark lands, and the
/// run stops before its next step: the pull request stays pending for the
/// next start. A *Run* asked for once the quit has begun answers the flow and
/// sends nothing. Nothing is left `running`, so the next start's
/// `interrupt_stranded` has nothing to interrupt.
///
/// Before, the run was a bare task: the quit neither waited for it nor
/// stopped it, so the pool could close under the step's send and the next
/// start sent the branch again.
#[tokio::test(flavor = "multi_thread")]
async fn a_start_work_step_at_the_source_at_quit_lands_and_a_run_after_sends_nothing() {
    use knobas_core::entity::EntityRef;
    use knobas_core::start_work::{Step, StepOutcome};
    use tauri::Manager as _;

    const BEFORE: &str = "jira:PAY-231";
    const AFTER: &str = "jira:PAY-232";
    const REPO: &str = "gitea:knobas/app";

    let connector =
        knobas_db::test_util::scratch_database("background_write_quit_start_work").await;
    let Fixture {
        state, wrote, gate, ..
    } = fixture_on(connector.clone(), HELD_OFF_PATIENT_QUIT).await;
    for (id, kind, title, payload) in [
        (BEFORE, "ticket", "payout latency", json!({})),
        (AFTER, "ticket", "refund latency", json!({})),
        (
            REPO,
            "repo",
            "knobas/app",
            json!({"default_branch": "main"}),
        ),
    ] {
        sqlx::query("insert into knobas.entity (id, kind, title) values ($1, $2, $3)")
            .bind(id)
            .bind(kind)
            .bind(title)
            .execute(&state.pool)
            .await
            .expect("the entity is written");
        sqlx::query(
            "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
             values ($1, $2, $3, $4, '', $5)",
        )
        .bind(id)
        .bind(id.split(':').next().unwrap())
        .bind(kind)
        .bind(title)
        .bind(payload)
        .execute(&state.pool)
        .await
        .expect("the item is mirrored");
    }
    let entity = |id: &str| EntityRef::parse(id).unwrap();
    for ticket in [BEFORE, AFTER] {
        knobas_app::start_work::begin(&state.pool, &entity(ticket), &entity(REPO))
            .await
            .expect("a flow is proposed");
    }
    let app = tauri::test::mock_builder()
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("a mock app");
    app.manage(state.clone());

    knobas_app::commands::entity::start_work_run(app.handle().clone(), BEFORE.to_owned())
        .await
        .expect("the first run starts");
    tokio::time::timeout(PATIENCE, async {
        while *wrote.entered.lock().unwrap() == 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the first flow's branch reaches the source");

    let (over, quit_over) = std::sync::mpsc::channel();
    let quitting = std::thread::spawn({
        let handle = app.handle().clone();
        move || {
            knobas_app::shutdown(&handle);
            let _ = over.send(());
        }
    });
    tokio::time::timeout(PATIENCE, async {
        while !state.scheduler.is_shutting_down() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("the quit begins");
    knobas_app::commands::entity::start_work_run(app.handle().clone(), AFTER.to_owned())
        .await
        .expect("a run asked for during the quit still answers the flow");
    // Give a run that was spawned for it the time to queue its branch.
    tokio::time::sleep(Duration::from_millis(200)).await;
    gate.add_permits(1);
    // A run still in the set at the grace's end is aborted, and the grace is
    // an hour: a quit over inside the patience is one whose run ended once
    // its send landed, rather than looping in the set until the abort.
    quit_over
        .recv_timeout(PATIENCE)
        .expect("the quit waited out its grace: the run did not end after its step");
    quitting.join().expect("the quit finishes");

    let reader = connector.pool(1).await.unwrap();
    let flow_of = async |id: &str| {
        knobas_app::start_work::flow(&reader, &entity(id))
            .await
            .expect("the flow reads")
    };
    let outcome = |flow: &[knobas_core::start_work::FlowStep], kind: Step| {
        flow.iter()
            .find(|step| step.step == kind)
            .map(|step| (step.outcome, step.write_id))
            .unwrap_or_else(|| panic!("no {kind} step"))
    };
    let before = flow_of(BEFORE).await;
    let (branch, branch_write) = outcome(&before, Step::CreateBranch);
    assert_eq!(
        branch,
        StepOutcome::Succeeded,
        "the step in flight at the quit did not land"
    );
    let branch_write = branch_write.expect("the branch step names its write");
    assert_eq!(
        knobas_core::write_queue::get(&reader, branch_write)
            .await
            .unwrap()
            .expect("the row is there")
            .state,
        WriteState::Sent,
        "the send in flight at the quit did not land its sent mark"
    );
    assert_eq!(
        outcome(&before, Step::CreatePullRequest).0,
        StepOutcome::Pending,
        "the run went on past the quit to its next step"
    );
    assert!(
        flow_of(AFTER)
            .await
            .iter()
            .all(|step| step.outcome == StepOutcome::Pending && step.write_id.is_none()),
        "the run asked for after the quit began touched its flow"
    );
    let queued: i64 = sqlx::query_scalar("select count(*) from knobas.write_queue")
        .fetch_one(&reader)
        .await
        .expect("the queue reads");
    assert_eq!(queued, 1, "a write was queued after the quit began");
    assert_eq!(
        *wrote.entered.lock().unwrap(),
        1,
        "only the send in flight at the quit reached the source"
    );
    assert_eq!(
        knobas_app::start_work::interrupt_stranded(&reader)
            .await
            .expect("the start-up pass runs"),
        0,
        "the quit left a step running"
    );
}

/// The ways a write reaches the source with a command waiting on its send
/// (#1932), and the send behind `submit_write`'s answer.
#[derive(Clone, Copy, Debug)]
enum Path {
    /// *Flush now*: the `flush_writes` command.
    FlushNow,
    /// *Send mine anyway*: the `apply_held_write` command, on a held write.
    SendMineAnyway,
    /// *Edit and send*: the `amend_write` command, on a held write.
    EditAndSend,
    /// A submit that waits for its send: what the standup protocol's page and
    /// an action item's ticket are written with.
    Submit,
    /// The worklog's delivery that waits for its send, `worklog::log`.
    Worklog,
    /// `submit_write`'s send behind its answer, in the background set since
    /// #1633.
    InBackground,
}

/// The five a command waits on, each its own code path into the queue.
const FOREGROUND: [Path; 5] = [
    Path::FlushNow,
    Path::SendMineAnyway,
    Path::EditAndSend,
    Path::Submit,
    Path::Worklog,
];

/// A mock app managing `state`, for the paths that are commands, and for the
/// quit, which reads the state off the app.
fn app_for(state: &SourcesState) -> tauri::App<tauri::test::MockRuntime> {
    use tauri::Manager as _;
    let app = tauri::test::mock_builder()
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("a mock app");
    app.manage(state.clone());
    app
}

/// Start `path`'s write, on its own task as the shell's call would be, and
/// wait until the source has it. Answers the write's id: the only row in the
/// queue.
async fn at_the_source(
    path: Path,
    state: &SourcesState,
    handle: &tauri::AppHandle<tauri::test::MockRuntime>,
    wrote: &Wrote,
) -> i64 {
    // The three that act on a row already queued: held for the two that
    // release one, on its own snapshot so the release sends it.
    let mut queued = None;
    if matches!(
        path,
        Path::FlushNow | Path::SendMineAnyway | Path::EditAndSend
    ) {
        let op: WriteOp = serde_json::from_value(comment("as queued")).unwrap();
        let row = knobas_sync::write_queue::queue(state.scheduler.deps(), SOURCE, op)
            .await
            .expect("the write is queued");
        if !matches!(path, Path::FlushNow) {
            knobas_core::write_queue::hold(&state.pool, row.id, row.target_snapshot.clone())
                .await
                .expect("the hold is written")
                .expect("the write was pending");
        }
        queued = Some(row.id);
    }
    if matches!(path, Path::Worklog) {
        sqlx::query(
            "insert into knobas.block (started_at, ended_at, entity_id, kind)
             values ('2026-10-19T09:00:00Z', '2026-10-19T10:00:00Z', $1, 'manual')",
        )
        .bind(PR)
        .execute(&state.pool)
        .await
        .expect("the block is written");
    }

    let (state_, handle) = (state.clone(), handle.clone());
    tokio::spawn(async move {
        let state = &state_;
        // What each answers is not this test's: the quit may cut it short.
        let _ = match path {
            Path::FlushNow => knobas_app::commands::sources::flush_writes(handle, None).await,
            Path::SendMineAnyway => {
                knobas_app::commands::sources::apply_held_write(handle, queued.unwrap()).await
            }
            Path::EditAndSend => {
                knobas_app::commands::sources::amend_write(
                    handle,
                    queued.unwrap(),
                    comment("as edited"),
                )
                .await
            }
            Path::Submit => knobas_app::sources::write_queue::submit(state, comment("submitted"))
                .await
                .map(|_| ()),
            Path::Worklog => knobas_app::time::worklog::log(
                state,
                PR,
                chrono::NaiveDate::from_ymd_opt(2026, 10, 19).unwrap(),
                0,
                "2026-10-19T09:00:00Z".parse().unwrap(),
                3_600,
                "",
            )
            .await
            .map(|_| ()),
            Path::InBackground => {
                knobas_app::sources::write_queue::submit_in_background(state, comment("behind"))
                    .await
                    .map(|_| ())
            }
        };
    });

    tokio::time::timeout(PATIENCE, async {
        while *wrote.entered.lock().unwrap() == 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{path:?}: the write never reached the source"));
    sqlx::query_scalar("select id from knobas.write_queue")
        .fetch_one(&state.pool)
        .await
        .expect("one write is queued")
}

/// Begin the quit on its own thread, as the exit handler does, and wait until
/// it has begun. The receiver hears its end.
fn quit(
    app: &tauri::App<tauri::test::MockRuntime>,
) -> (std::thread::JoinHandle<()>, std::sync::mpsc::Receiver<()>) {
    let (over, quit_over) = std::sync::mpsc::channel();
    let handle = app.handle().clone();
    let quitting = std::thread::spawn(move || {
        knobas_app::shutdown(&handle);
        let _ = over.send(());
    });
    (quitting, quit_over)
}

async fn quit_begun(state: &SourcesState) {
    tokio::time::timeout(PATIENCE, async {
        while !state.scheduler.is_shutting_down() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("the quit begins");
}

/// #1932: Cmd-Q while a write a command waits on is at the source. The quit
/// waits for it under its grace as it waits for a background send, so its
/// `sent` lands before the pool closes and the next start owes nothing.
///
/// One row per path, each its own code path into the queue's flush. The gate
/// opens once the write's send is in the background set (the quit then waits
/// for it whenever it opens) or once the pool has closed. Before the fix it
/// was never in the set, so the gate opened on a closed pool: `sent` failed,
/// the row stayed pending, and the next start's tick sent the write again.
/// The grace is an hour, so a quit that ends inside the patience waited for
/// nothing but the send.
#[tokio::test(flavor = "multi_thread")]
async fn a_write_a_command_waits_on_at_quit_lands_and_is_not_sent_again() {
    let mut failures = Vec::new();
    for path in FOREGROUND {
        let connector = knobas_db::test_util::scratch_database(&format!(
            "background_write_quit_awaited_{}",
            format!("{path:?}").to_lowercase()
        ))
        .await;
        let Fixture {
            state, wrote, gate, ..
        } = fixture_on(connector.clone(), HELD_OFF_PATIENT_QUIT).await;
        let app = app_for(&state);
        let id = at_the_source(path, &state, app.handle(), &wrote).await;

        let (quitting, quit_over) = quit(&app);
        quit_begun(&state).await;
        tokio::time::timeout(PATIENCE, async {
            while state.scheduler.background().running() == 0 && !state.pool.is_closed() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("{path:?}: the send is neither in the set nor cut off"));
        gate.add_permits(1);
        quit_over
            .recv_timeout(PATIENCE)
            .unwrap_or_else(|_| panic!("{path:?}: the quit waited out its grace"));
        quitting.join().expect("the quit finishes");

        // The next start: its tick sends whatever is still owed.
        let reader = connector.pool(1).await.unwrap();
        let next = next_start(&connector, &wrote, &gate).await;
        gate.add_permits(10);
        let settled = tokio::time::timeout(PATIENCE, async {
            loop {
                let row = knobas_core::write_queue::get(&reader, id)
                    .await
                    .unwrap()
                    .expect("the row is there");
                if row.state != WriteState::Pending {
                    return row.state;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("{path:?}: the write never settled"));
        next.shutdown().await;
        let sends = *wrote.entered.lock().unwrap();
        if settled != WriteState::Sent || sends != 1 {
            failures.push(format!(
                "{path:?}: settled {settled:?} after {sends} sends; the write on the wire \
                 at the quit was sent again at the next start"
            ));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// #1932's second half: a write still at the source when the grace runs out
/// is cut, and is held as one the source may have, as a send that got no
/// answer is (#1125). The next start does not send it again blind; the
/// reader checks the source, then *Send again* or *Discard*.
///
/// The five a command waits on, and the background send: all six are in the
/// set by now, so the abort at the grace's end is what cuts them. Before,
/// the five were not in the set, and the background send was aborted with
/// its row pending: each was left pending, and the next start sent it again.
/// The gate never opens before the quit ends, so the grace's length decides
/// nothing here.
#[tokio::test(flavor = "multi_thread")]
async fn a_write_still_at_the_source_when_the_grace_ends_is_held_as_unconfirmed() {
    const SHORT_QUIT: knobas_sync::scheduler::SchedulerTiming =
        knobas_sync::scheduler::SchedulerTiming {
            shutdown_grace: Duration::from_millis(100),
            ..HELD_OFF
        };
    let mut failures = Vec::new();
    for path in FOREGROUND.into_iter().chain([Path::InBackground]) {
        let connector = knobas_db::test_util::scratch_database(&format!(
            "background_write_quit_cut_{}",
            format!("{path:?}").to_lowercase()
        ))
        .await;
        let Fixture {
            state, wrote, gate, ..
        } = fixture_on(connector.clone(), SHORT_QUIT).await;
        let app = app_for(&state);
        let id = at_the_source(path, &state, app.handle(), &wrote).await;

        let (quitting, quit_over) = quit(&app);
        quit_over
            .recv_timeout(PATIENCE)
            .unwrap_or_else(|_| panic!("{path:?}: the quit did not end"));
        quitting.join().expect("the quit finishes");

        let reader = connector.pool(1).await.unwrap();
        let row = knobas_core::write_queue::get(&reader, id)
            .await
            .unwrap()
            .expect("the row is there");
        let source = row
            .held_snapshot
            .as_ref()
            .and_then(|held| held["unconfirmed"]["source"].as_str());
        if row.state != WriteState::Held || source != Some("Gitea") {
            failures.push(format!(
                "{path:?}: the write cut at the grace's end is left {:?} ({:?}), not held \
                 as one Gitea may have, so the next start sends it again",
                row.state, row.held_snapshot
            ));
        }
        // A send the quit left behind holds the source's lock, which every
        // flush of the source id in this process takes (#1629): let it end.
        gate.add_permits(10);
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// A scheduler on `connector`'s database, as the next start builds one, with
/// its tick running.
async fn next_start(
    connector: &knobas_db::embedded::Connector,
    wrote: &Arc<Wrote>,
    gate: &Arc<Semaphore>,
) -> knobas_sync::scheduler::Scheduler {
    let secrets = Arc::new(MemoryStore::new());
    secrets
        .put(
            &knobas_secrets::KeychainAccount::source(SOURCE),
            &Secret::just(AuthMethod::Pat, "a-token"),
        )
        .expect("the credential is stored");
    let (tell, _heard) = mpsc::unbounded_channel();
    knobas_sync::scheduler::Scheduler::start(SchedulerDeps {
        pool: connector.pool(2).await.unwrap(),
        connections: Arc::new(Connections(connector.clone())),
        registry: Arc::new(Registry {
            wrote: Arc::clone(wrote),
            gate: Arc::clone(gate),
        }),
        secrets,
        events: Arc::new(Heard(tell)),
        timing: knobas_sync::scheduler::SchedulerTiming {
            startup_delay: Duration::from_millis(10),
            tick: Duration::from_millis(50),
            ..Default::default()
        },
        quit: knobas_sync::scheduler::Quit::default(),
    })
    .await
    .expect("the next start's scheduler")
}

/// #1932's ruling on a command made once the quit has begun: it answers as it
/// did before the flushes moved into the background set. The set takes
/// nothing then, so the flush runs where the command is, and it sends nothing
/// (#1924): an *Edit and send* keeps the reader's edit on a pending row for
/// the next start, and a *Flush now* answers that nothing went wrong.
///
/// The quit is held open by a background send at the source, on an hour's
/// grace. A command that sent would have reached the source before it
/// answered, so the count read after both answers says neither did.
#[tokio::test(flavor = "multi_thread")]
async fn a_command_made_during_the_quit_changes_its_row_and_sends_nothing() {
    let connector = knobas_db::test_util::scratch_database("background_write_during_quit").await;
    let Fixture {
        state, wrote, gate, ..
    } = fixture_on(connector.clone(), HELD_OFF_PATIENT_QUIT).await;
    let app = app_for(&state);
    let first = at_the_source(Path::InBackground, &state, app.handle(), &wrote).await;

    let other = "gitea:knobas/app#8";
    let op: WriteOp = serde_json::from_value(json!({
        "Comment": { "entity": other, "body": "as queued" }
    }))
    .unwrap();
    let held = knobas_sync::write_queue::queue(state.scheduler.deps(), SOURCE, op)
        .await
        .expect("the second write is queued");
    knobas_core::write_queue::hold(&state.pool, held.id, held.target_snapshot.clone())
        .await
        .expect("the hold is written")
        .expect("the write was pending");

    let (quitting, quit_over) = quit(&app);
    quit_begun(&state).await;
    knobas_app::commands::sources::amend_write(
        app.handle().clone(),
        held.id,
        json!({ "Comment": { "entity": other, "body": "as edited" } }),
    )
    .await
    .expect("an edit during the quit is kept");
    knobas_app::commands::sources::flush_writes(app.handle().clone(), None)
        .await
        .expect("a flush during the quit answers");
    assert_eq!(
        *wrote.entered.lock().unwrap(),
        1,
        "a command made during the quit sent a write"
    );

    gate.add_permits(1);
    quit_over
        .recv_timeout(PATIENCE)
        .expect("the quit waited out its grace");
    quitting.join().expect("the quit finishes");

    let reader = connector.pool(1).await.unwrap();
    let row = |id: i64| {
        let reader = reader.clone();
        async move {
            knobas_core::write_queue::get(&reader, id)
                .await
                .unwrap()
                .expect("the row is there")
        }
    };
    assert_eq!(row(first).await.state, WriteState::Sent);
    let edited = row(held.id).await;
    assert_eq!(
        edited.state,
        WriteState::Pending,
        "waits for the next start"
    );
    assert_eq!(edited.payload["Comment"]["body"], json!("as edited"));
}

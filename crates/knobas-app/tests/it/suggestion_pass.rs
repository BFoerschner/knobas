//! The detection pass is one at a time and never on the UI pool (#745).
//!
//! Before #745 every finished sync started a pass on the UI pool, with
//! nothing stopping the next: five sources syncing held all five UI
//! connections for minutes. These pin the three properties that ended that:
//! requests coalesce, a pass takes no UI connection, and a statement that runs
//! past the pass's limit is cancelled and reported rather than waited out.
//!
//! And since #1485, who asks: the sync scheduler, after every run that ends
//! OK, on the one pass bring-up built, without waiting for it.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use knobas_app::suggestion_pass::SuggestionPass;
use knobas_sync::scheduler::RunConnections;
use sqlx::PgPool;
use tokio::sync::Semaphore;

/// A database of this test's own, behind a UI-sized pool.
async fn scratch(label: &str, ui_connections: u32) -> (knobas_db::embedded::Connector, PgPool) {
    let connector = knobas_db::test_util::scratch_database(label).await;
    let pool = connector.pool(ui_connections).await.unwrap();
    (connector, pool)
}

/// Opens real connections, counts them -- one per pass -- and holds each open
/// until the test lets it through, so requests can pile up behind a pass.
struct Gated {
    connector: knobas_db::embedded::Connector,
    opened: AtomicUsize,
    gate: Semaphore,
}

#[async_trait::async_trait]
impl RunConnections for Gated {
    async fn open(&self) -> Result<sqlx::PgConnection, sqlx::Error> {
        self.opened.fetch_add(1, Ordering::SeqCst);
        self.gate.acquire().await.unwrap().forget();
        self.connector.connect().await
    }
}

/// `items` mirrored tickets that all read alike, so a pass proposes every
/// pair and its similarity batches take real time.
async fn corpus(pool: &PgPool, items: i32) {
    sqlx::query(
        "insert into knobas.entity (id, kind, title)
         select 'jira:PAY-' || g, 'ticket', 'payout retry storm ' || g
           from generate_series(1, $1) g",
    )
    .bind(items)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
         select id, 'jira', 'ticket', title,
                'the payout retry storm floods the ledger with duplicate transfers', '{}'
           from knobas.entity",
    )
    .execute(pool)
    .await
    .unwrap();
}

/// Ten requests while a pass runs cost two passes: the running one, and one
/// that answers all nine that arrived during it. Every request is answered.
#[tokio::test]
async fn requests_during_a_pass_coalesce_into_one_more_pass() {
    let (connector, pool) = scratch("pass_coalesce", 2).await;
    corpus(&pool, 20).await;
    let gated = Arc::new(Gated {
        connector,
        opened: AtomicUsize::new(0),
        gate: Semaphore::new(0),
    });
    let pass = SuggestionPass::new(gated.clone(), Duration::from_secs(60));

    let first = tokio::spawn({
        let pass = pass.clone();
        async move { pass.run().await }
    });
    while gated.opened.load(Ordering::SeqCst) == 0 {
        tokio::task::yield_now().await;
    }
    // The first pass is parked opening its connection; nine more arrive.
    let rest: Vec<_> = (0..9)
        .map(|_| {
            let pass = pass.clone();
            tokio::spawn(async move { pass.run().await })
        })
        .collect();
    // All nine have been made, and every one found the pass running: none
    // started a pass of its own (#1927). Before, a 200 ms look at the opened
    // connections stood for this, and a second pass that opened later than
    // the look went unseen.
    tokio::time::timeout(Duration::from_secs(20), async {
        while pass.requested() < 10 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the nine requests are made");
    assert_eq!(
        pass.started(),
        1,
        "a request during a pass must not start a second one beside it"
    );
    assert_eq!(gated.opened.load(Ordering::SeqCst), 1);

    gated.gate.add_permits(100);
    let written = first.await.unwrap().unwrap();
    assert!(written > 0, "the first pass proposed the corpus' pairs");
    for request in rest {
        assert_eq!(
            request.await.unwrap().unwrap(),
            0,
            "answered by the second pass, over a mirror the first one settled"
        );
    }
    assert_eq!(
        gated.opened.load(Ordering::SeqCst),
        2,
        "ten requests, two passes"
    );

    // And with nothing running, the next request starts a pass of its own.
    pass.run().await.unwrap();
    assert_eq!(gated.opened.load(Ordering::SeqCst), 3);
}

/// A pass takes no connection from the UI pool: with the pool's only
/// connection held by a UI command, a pass still runs to the end.
#[tokio::test]
async fn a_pass_runs_while_the_ui_pool_is_exhausted() {
    let (_connector, pool) = scratch("pass_off_pool", 1).await;
    corpus(&pool, 20).await;
    let held = pool.acquire().await.unwrap();

    let written =
        tokio::time::timeout(Duration::from_secs(20), SuggestionPass::beside(&pool).run())
            .await
            .expect("a pass waiting on the UI pool would still be waiting")
            .unwrap();
    assert!(written > 0);
    assert_eq!(pool.size(), 1, "the pass opened nothing in the UI pool");
    drop(held);
}

/// A statement that runs past the pass's limit is cancelled by the server:
/// the pass gives up and says why, the UI pool is untouched, and the next pass
/// under a sane limit finishes the work.
#[tokio::test]
async fn a_pass_past_its_statement_timeout_is_cancelled_and_the_next_one_finishes() {
    let (connector, pool) = scratch("pass_timeout", 1).await;
    corpus(&pool, 300).await;
    let held = pool.acquire().await.unwrap();

    struct Direct(knobas_db::embedded::Connector);
    #[async_trait::async_trait]
    impl RunConnections for Direct {
        async fn open(&self) -> Result<sqlx::PgConnection, sqlx::Error> {
            self.0.connect().await
        }
    }
    let hurried = SuggestionPass::new(
        Arc::new(Direct(connector.clone())),
        Duration::from_millis(1),
    );
    let refusal = tokio::time::timeout(Duration::from_secs(20), hurried.run())
        .await
        .expect("a cancelled pass answers rather than hangs")
        .unwrap_err();
    assert!(
        refusal.message.contains("ran past its"),
        "the refusal names the limit: {}",
        refusal.message
    );
    assert_eq!(pool.size(), 1, "the UI pool is untouched");
    drop(held);

    let patient = SuggestionPass::new(Arc::new(Direct(connector)), Duration::from_secs(60));
    patient
        .run()
        .await
        .expect("the same corpus under a sane limit");
}

/// A pass that panics does not wedge the queue: the requests it owed are
/// answered with an error rather than left waiting, and the next request
/// starts a pass of its own that finishes.
#[tokio::test]
async fn a_pass_that_panics_answers_its_requests_and_the_next_one_runs() {
    let (connector, pool) = scratch("pass_panic", 1).await;
    corpus(&pool, 20).await;

    /// Panics on the first open, the way a bug in a pass would, and opens
    /// real connections after that.
    struct PanicsOnce {
        connector: knobas_db::embedded::Connector,
        opened: AtomicUsize,
    }
    #[async_trait::async_trait]
    impl RunConnections for PanicsOnce {
        async fn open(&self) -> Result<sqlx::PgConnection, sqlx::Error> {
            assert!(
                self.opened.fetch_add(1, Ordering::SeqCst) > 0,
                "the injected panic"
            );
            self.connector.connect().await
        }
    }
    let pass = SuggestionPass::new(
        Arc::new(PanicsOnce {
            connector,
            opened: AtomicUsize::new(0),
        }),
        Duration::from_secs(60),
    );

    let refusal = tokio::time::timeout(Duration::from_secs(20), pass.run())
        .await
        .expect("a pass that panicked must answer, not leave its caller waiting")
        .unwrap_err();
    assert!(
        refusal.message.contains("stopped part-way"),
        "the refusal says the pass stopped: {}",
        refusal.message
    );

    let written = tokio::time::timeout(Duration::from_secs(20), pass.run())
        .await
        .expect("the queue must not stay wedged behind the pass that panicked")
        .unwrap();
    assert!(
        written > 0,
        "the next pass ran and proposed the corpus' pairs"
    );
}

/// A request that does not wait (#1485), the sync scheduler's: `request`
/// returns while the pass it started is still parked, and the nine that
/// arrive during that pass cost one more pass between them, as `run`'s do.
#[tokio::test]
async fn requests_that_do_not_wait_coalesce_into_one_more_pass() {
    let (connector, pool) = scratch("pass_request", 2).await;
    corpus(&pool, 20).await;
    let gated = Arc::new(Gated {
        connector,
        opened: AtomicUsize::new(0),
        gate: Semaphore::new(0),
    });
    let pass = SuggestionPass::new(gated.clone(), Duration::from_secs(60));

    pass.request();
    tokio::time::timeout(Duration::from_secs(20), async {
        while gated.opened.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("a request with no pass running starts one");
    for _ in 0..9 {
        pass.request();
    }
    // `request` numbers the request before it returns, so the nine are made
    // and none started a pass: counted as they were made (#1927), not looked
    // for in the opened connections 200 ms later.
    assert_eq!(pass.requested(), 10);
    assert_eq!(
        pass.started(),
        1,
        "a request during a pass must not start a second one beside it"
    );
    assert_eq!(gated.opened.load(Ordering::SeqCst), 1);

    gated.gate.add_permits(100);
    tokio::time::timeout(Duration::from_secs(20), pass.idle())
        .await
        .expect("every request is answered");
    assert_eq!(
        gated.opened.load(Ordering::SeqCst),
        2,
        "ten requests, two passes"
    );
    assert_eq!(pass.passes(), 2);
    assert!(
        proposed(&pool).await > 0,
        "the passes nobody waited for still proposed"
    );
}

/// A mock source added under `config`, the engine brought up the way the app
/// does (`sources::bring_up`) and handed `pass`, and one manual sync of it run
/// to its end. Answers once the run has ended and every pass its ending
/// requested has finished, with the outcome the run recorded.
///
/// Nothing here calls [`SuggestionPass::run`]: whatever pass runs is one the
/// scheduler asked for.
fn sync_through_bring_up(
    label: &str,
    config: serde_json::Value,
) -> (PgPool, SuggestionPass, Option<knobas_sync::SyncOutcome>) {
    use knobas_sync::SyncTrigger;
    use knobas_sync::progress::{ProgressSink, SyncPhase, SyncProgress};

    struct Ending(std::sync::Mutex<Option<tokio::sync::oneshot::Sender<()>>>);
    impl ProgressSink for Ending {
        fn report(&self, progress: SyncProgress) {
            if !matches!(progress.phase, SyncPhase::Finished | SyncPhase::Failed) {
                return;
            }
            if let Some(done) = self
                .0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take()
            {
                let _ = done.send(());
            }
        }
    }

    let app = tauri::test::mock_builder()
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("a mock app");
    let handle = app.handle().clone();
    // `block_in_place` first: `block_on` from inside the test's runtime panics.
    tokio::task::block_in_place(|| {
        tauri::async_runtime::block_on(async move {
            let (connector, pool) = scratch(label, 2).await;
            let secrets: Arc<dyn knobas_secrets::SecretStore> =
                Arc::new(knobas_secrets::MemoryStore::new());
            let registry = knobas_app::sources::Registry::default();
            let input: knobas_app::sources::NewSource = serde_json::from_value(serde_json::json!({
                "id": "mock",
                "adapter_kind": "mock",
                "display_name": "Tidewater",
                "base_url": "https://mock.example.invalid",
                "auth_kind": "Pat",
                "config": config,
                "secret": { "value": "a token the mock never sees" },
                "sync_interval_secs": 900,
                "enabled": true,
            }))
            .unwrap();
            knobas_app::sources::crud::add(&pool, &secrets, &registry, input)
                .await
                .expect("the mock source is added");

            let pass = SuggestionPass::beside(&pool);
            let store = tempfile::tempdir().unwrap();
            let state = knobas_app::sources::bring_up(
                &handle,
                knobas_app::sources::BringUp {
                    pool: pool.clone(),
                    sync_pool: connector.pool(2).await.unwrap(),
                    connections: connector,
                    secrets,
                    registry,
                    store: store.path().to_path_buf(),
                    suggestions: pass.clone(),
                },
            )
            .await
            .expect("bring-up over the scratch database");

            let (done, ended) = tokio::sync::oneshot::channel();
            state
                .scheduler
                .trigger(
                    "mock",
                    SyncTrigger::Manual,
                    Some(Arc::new(Ending(std::sync::Mutex::new(Some(done))))),
                )
                .await
                .expect("the run starts");
            ended.await.expect("the run reports its ending");
            tokio::time::timeout(Duration::from_secs(60), pass.idle())
                .await
                .expect("the pass the run's ending requested finishes");
            let outcome = knobas_sync::scheduler::status_for(&pool, "mock")
                .await
                .unwrap()
                .and_then(|status| status.last_outcome);
            state.scheduler.shutdown().await;
            (pool, pass, outcome)
        })
    })
}

async fn proposed(pool: &PgPool) -> i64 {
    sqlx::query_scalar("select count(*) from knobas.proposed_link")
        .fetch_one(pool)
        .await
        .unwrap()
}

/// The audit's finding (#1485): with the room tray gone nothing asked for a
/// pass, and Proposed links never grew. A sync run that ends OK now asks, on
/// the pass bring-up was handed, never one of the scheduler's own.
#[tokio::test(flavor = "multi_thread")]
async fn a_sync_that_ends_ok_proposes_links_without_anyone_asking() {
    let (pool, pass, outcome) = sync_through_bring_up("pass_after_sync_ok", serde_json::json!({}));
    assert_eq!(outcome, Some(knobas_sync::SyncOutcome::Ok));
    assert!(
        proposed(&pool).await > 0,
        "the Tidewater corpus the run brought in matches the rules, and no \
         pass proposed anything"
    );
    assert!(
        pass.passes() >= 1,
        "the pass that ran is not the one bring-up was handed, so the process \
         holds a second pass that could run beside it"
    );
}

/// A run that fails changed nothing a rule could match: it asks for no pass.
#[tokio::test(flavor = "multi_thread")]
async fn a_sync_that_fails_requests_no_pass() {
    let (pool, pass, outcome) = sync_through_bring_up(
        "pass_after_sync_failed",
        serde_json::json!({ "fault": "unreachable" }),
    );
    assert!(
        outcome.is_some_and(|outcome| outcome != knobas_sync::SyncOutcome::Ok),
        "the faulted mock's run must fail for this test to mean anything: {outcome:?}"
    );
    assert_eq!(pass.passes(), 0, "a failed run requested a pass");
    assert_eq!(proposed(&pool).await, 0);
}

/// What the scheduler's events request, event by event: only the emit that
/// ends a run OK. The emit that starts a run carries the *previous* run's
/// outcome, so a source whose last run was OK must not request a pass each
/// time it starts one. Every event reaches the events it wraps.
#[tokio::test]
async fn only_the_emit_that_ends_a_run_ok_requests_a_pass() {
    use knobas_app::suggestion_pass::SuggestOnSync;
    use knobas_sync::scheduler::SyncEvents;
    use knobas_sync::{SourceSyncStatus, SyncOutcome};

    struct Counted(Arc<AtomicUsize>);
    impl SyncEvents for Counted {
        fn sync_state(&self, _status: SourceSyncStatus) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
        fn source_health(&self, _health: knobas_sync::CredentialHealth) {}
        fn activity_new(&self, _row: knobas_core::activity::ActivityRow) {}
    }

    let (connector, _pool) = scratch("pass_on_sync_events", 1).await;
    let gated = Arc::new(Gated {
        connector,
        opened: AtomicUsize::new(0),
        gate: Semaphore::new(0),
    });
    let forwarded = Arc::new(AtomicUsize::new(0));
    let pass = SuggestionPass::new(gated.clone(), Duration::from_secs(60));
    let events = SuggestOnSync::new(Counted(Arc::clone(&forwarded)), pass.clone());

    let mut starting = SourceSyncStatus::started("jira", 2);
    starting.last_outcome = Some(SyncOutcome::Ok);
    events.sync_state(starting);
    for outcome in [
        SyncOutcome::Error,
        SyncOutcome::Unreachable,
        SyncOutcome::Unauthorized,
    ] {
        events.sync_state(SourceSyncStatus::finished("jira", 2, outcome));
    }
    // A request is numbered inside `sync_state`, before it returns: these
    // four made none (#1927). Before, a 200 ms look at the opened connections
    // stood for this, and a pass that opened later than the look went unseen.
    assert_eq!(
        (pass.requested(), pass.started()),
        (0, 0),
        "a run starting, or one that failed, requested a pass"
    );

    events.sync_state(SourceSyncStatus::finished("jira", 3, SyncOutcome::Ok));
    tokio::time::timeout(Duration::from_secs(20), async {
        while gated.opened.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("a run that ended OK requests a pass");
    assert_eq!((pass.requested(), pass.started()), (1, 1));
    assert_eq!(
        forwarded.load(Ordering::SeqCst),
        5,
        "every event is passed on"
    );
}

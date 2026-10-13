//! The detection pass is one at a time and never on the UI pool (#745).
//!
//! Before #745 every finished sync started a pass on the UI pool, with
//! nothing stopping the next: five sources syncing held all five UI
//! connections for minutes. These pin the three properties that ended that:
//! requests coalesce, a pass takes no UI connection, and a statement that runs
//! past the pass's limit is cancelled and reported rather than waited out.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use knobas_app::commands::entity::detect_suggestions_inner;
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
        async move { detect_suggestions_inner(&pass).await }
    });
    while gated.opened.load(Ordering::SeqCst) == 0 {
        tokio::task::yield_now().await;
    }
    // The first pass is parked opening its connection; nine more arrive.
    let rest: Vec<_> = (0..9)
        .map(|_| {
            let pass = pass.clone();
            tokio::spawn(async move { detect_suggestions_inner(&pass).await })
        })
        .collect();
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(
        gated.opened.load(Ordering::SeqCst),
        1,
        "a request during a pass must not start a second one beside it"
    );

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
    detect_suggestions_inner(&pass).await.unwrap();
    assert_eq!(gated.opened.load(Ordering::SeqCst), 3);
}

/// A pass takes no connection from the UI pool: with the pool's only
/// connection held by a UI command, a pass still runs to the end.
#[tokio::test]
async fn a_pass_runs_while_the_ui_pool_is_exhausted() {
    let (_connector, pool) = scratch("pass_off_pool", 1).await;
    corpus(&pool, 20).await;
    let held = pool.acquire().await.unwrap();

    let written = tokio::time::timeout(
        Duration::from_secs(20),
        detect_suggestions_inner(&SuggestionPass::beside(&pool)),
    )
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
    let refusal = tokio::time::timeout(Duration::from_secs(20), detect_suggestions_inner(&hurried))
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
    detect_suggestions_inner(&patient)
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

    let refusal = tokio::time::timeout(Duration::from_secs(20), detect_suggestions_inner(&pass))
        .await
        .expect("a pass that panicked must answer, not leave its caller waiting")
        .unwrap_err();
    assert!(
        refusal.message.contains("stopped part-way"),
        "the refusal says the pass stopped: {}",
        refusal.message
    );

    let written = tokio::time::timeout(Duration::from_secs(20), detect_suggestions_inner(&pass))
        .await
        .expect("the queue must not stay wedged behind the pass that panicked")
        .unwrap();
    assert!(
        written > 0,
        "the next pass ran and proposed the corpus' pairs"
    );
}

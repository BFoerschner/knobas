//! How many statements the four most-asked reads issue (#1659).
//!
//! `identity_of`, `declared_paths` and `searcher()` were each re-derived in a
//! command body, statements more than the answer needs. The count is read off
//! sqlx's own `sqlx::query` tracing events, one per statement the pool runs,
//! on a current-thread runtime so the thread-local subscriber sees every one.
//! Each ceiling is the count after the fix. Before: `inbox_items` 4,
//! `docs_tree` 10, `list_projects` 3, a searched `mini_board_laned` 7.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use chrono::Utc;
use knobas_app::commands::entity::{
    Room, docs_tree_inner, inbox_items_inner, list_projects_inner, mini_board_laned,
};
use knobas_app::sources::Registry;
use sqlx::PgPool;
use tracing_subscriber::layer::{Context, SubscriberExt};

struct Count(Arc<AtomicUsize>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Count {
    fn on_event(&self, event: &tracing::Event<'_>, _: Context<'_, S>) {
        if event.metadata().target() == "sqlx::query" {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
}

/// One counted section at a time in this file, so the steady state one test
/// counts is never another's warm-up on the same database.
static ONE_AT_A_TIME: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// A dispatcher registered for the whole process and never dropped. While the
/// counter's thread-local one is the only dispatcher tracing has registered,
/// tracing-core takes its "just one" path: an interest it computes for a
/// callsite registered on another test's thread, and the max level a rebuild
/// sets there, come from *that* thread's default (none: `never`, `OFF`), so a
/// sqlx callsite can be cached as uninteresting for the counter too and the
/// count comes up short. With this one alive the counter is never alone, and
/// every interest and max level is the union over all live dispatchers, the
/// counter's included. (`ipc.rs` installs the binary's one global subscriber,
/// so the counter cannot be one, and that test may not have run yet.)
static NEVER_ALONE: std::sync::LazyLock<tracing::Dispatch> =
    std::sync::LazyLock::new(|| tracing::Dispatch::new(tracing::subscriber::NoSubscriber::new()));

async fn pool() -> PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind, config)
         values ('jira', 'jira', 'Jira', 'http://x', 'pat', '{\"username\": \"mara.lindqvist\"}')
         on conflict (id) do update set enabled = true",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool
}

/// The statements `work` issues, after one warm-up so the declarations are
/// stored and what is counted is the steady state every command runs in.
async fn statements<F, Fut>(work: F) -> usize
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    std::sync::LazyLock::force(&NEVER_ALONE);
    let _only = ONE_AT_A_TIME.lock().await;
    work().await;
    // Every command here issues a statement, so a count of nothing can only be
    // a callsite whose first registration on another thread straddled the
    // counter's (tracing-core computes its interest before it lists it): look
    // again under a new dispatcher, whose registration rebuilds every listed
    // callsite, rather than report it.
    for _ in 0..3 {
        let seen = Arc::new(AtomicUsize::new(0));
        let guard = tracing::subscriber::set_default(
            tracing_subscriber::registry().with(Count(seen.clone())),
        );
        work().await;
        drop(guard);
        let counted = seen.load(Ordering::SeqCst);
        if counted > 0 {
            return counted;
        }
    }
    0
}

#[tokio::test]
async fn the_counter_sees_a_statement() {
    let pool = pool().await;
    let n = statements(|| async {
        sqlx::query("select 1").execute(&pool).await.unwrap();
    })
    .await;
    assert_eq!(n, 1, "the counter reads sqlx's events");
}

#[tokio::test]
async fn inbox_items_lists_the_sources_once() {
    let pool = pool().await;
    let registry = Registry::for_process();
    let n = statements(|| async {
        inbox_items_inner(
            &pool,
            &registry,
            Utc::now(),
            knobas_core::inbox::Shelf::Stream,
        )
        .await
        .unwrap();
    })
    .await;
    assert_eq!(n, CEILING_INBOX, "inbox_items issued {n}");
}

#[tokio::test]
async fn docs_tree_derives_identity_declarations_and_sources_once() {
    let pool = pool().await;
    let registry = Registry::for_process();
    let n = statements(|| async {
        docs_tree_inner(&pool, &registry).await.unwrap();
    })
    .await;
    assert_eq!(n, CEILING_DOCS, "docs_tree issued {n}");
}

#[tokio::test]
async fn list_projects_declares_without_reading_the_notes_setting() {
    let pool = pool().await;
    let registry = Registry::for_process();
    let n = statements(|| async {
        list_projects_inner(&pool, &registry).await.unwrap();
    })
    .await;
    assert_eq!(n, CEILING_PROJECTS, "list_projects issued {n}");
}

#[tokio::test]
async fn a_searched_laned_board_derives_each_once() {
    let pool = pool().await;
    let registry = Registry::for_process();
    let n = statements(|| async {
        mini_board_laned(
            &pool,
            &registry,
            Room::default(),
            None,
            Some("payout"),
            None,
        )
        .await
        .unwrap();
    })
    .await;
    assert_eq!(n, CEILING_BOARD, "mini_board_laned issued {n}");
}

const CEILING_INBOX: usize = 3;
const CEILING_DOCS: usize = 9;
const CEILING_PROJECTS: usize = 2;
const CEILING_BOARD: usize = 6;

//! The demo's repositories found by their files (the everything-search
//! decisions, 2026-11-20, D16): the mock synced, its stored trees and its
//! files read by the passes the scheduler runs after a run, and *Everything*
//! asked. Björn's renamed repository is the case: `ledger-gateway`'s name,
//! README and pom say nothing of `SettleBridge`, and only its code does.

use knobas_app::sources::{Registry, demo};
use knobas_files::FileIndex;
use knobas_search::{SearchFilters, SearchQuery, SearchResponse, Searcher};
use knobas_source_mock::MockSource;
use knobas_sync::code_files::{self, Bounds, Files};
use sqlx::PgPool;

/// Held by each test for its passes: the stored-trees pass runs one pass a
/// source id at a time in a process, and every test's source is `mock`.
static PASSES: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

struct Demo {
    pool: PgPool,
    files: Files,
    _dir: tempfile::TempDir,
}

/// The demo loaded into a database of its own, and its passes run once.
async fn demo(name: &str) -> (Demo, code_files::Passed) {
    let pool = knobas_db::test_util::scratch_database(name)
        .await
        .pool(4)
        .await
        .expect("a pool onto this test's own database");
    knobas_db::migrate::run(&pool).await.unwrap();
    demo::demo_load_inner(&pool).await.unwrap();
    let mock = MockSource::new();
    let _turn = PASSES.lock().await;
    // Bring-up stores the adapters' parent declarations before the first
    // sync: what puts a branch under its repository.
    knobas_app::commands::search::declare(&pool, &Registry::for_process())
        .await
        .unwrap();
    let trees = knobas_sync::stored_trees::refresh(&pool, "mock", &mock)
        .await
        .unwrap();
    assert_eq!(
        trees.branches, 3,
        "the default branches of the three repositories with files (a branch \
         at a fixture commit like `c90d11` names no tree): {trees:?}"
    );
    let dir = tempfile::tempdir().unwrap();
    let files = Files {
        index: FileIndex::open(&dir.path().join("files-1")).unwrap(),
        clones: None,
    };
    let passed = code_files::refresh_within(
        &pool,
        "mock",
        &mock,
        &files,
        Bounds::default(),
        u64::MAX,
        &|_| {},
    )
    .await
    .unwrap();
    (
        Demo {
            pool,
            files,
            _dir: dir,
        },
        passed,
    )
}

async fn ask(demo: &Demo, titles: bool, raw: &str) -> SearchResponse {
    Searcher::new(demo.pool.clone())
        .with_files(demo.files.index.clone())
        .search(SearchQuery {
            titles,
            raw: raw.to_owned(),
            limit: 40,
            filters: SearchFilters::default(),
        })
        .await
        .unwrap()
}

/// The titles of `kind`'s rows, in order.
fn rows<'a>(answer: &'a SearchResponse, kind: &str) -> Vec<&'a str> {
    answer
        .groups
        .iter()
        .filter(|g| g.kind == kind)
        .flat_map(|g| g.hits.iter().map(|h| h.row.title.as_str()))
        .collect()
}

#[tokio::test]
async fn the_renamed_repository_is_found_by_a_word_only_its_code_holds() {
    let (demo, passed) = demo("everything-fixture-renamed").await;
    // Three repositories with files: ledger-gateway, ledger-api and
    // payout-service, under 24 wanted files each, so a file at a time: 6, 4
    // and the 10 of 12 that the lock file's and node_modules' paths leave.
    assert_eq!(
        (passed.repositories, passed.archives, passed.reads),
        (3, 0, 20)
    );

    let answer = ask(&demo, false, "settlebridge").await;
    let repositories = rows(&answer, "repo");
    assert_eq!(
        repositories,
        ["ledger-gateway", "ledger-api"],
        "the repository its files name first, the one that only imports it after"
    );
    let gateway = &answer
        .groups
        .iter()
        .find(|g| g.kind == "repo")
        .unwrap()
        .hits[0];
    let found = gateway.files.as_ref().expect("found by its files");
    assert_eq!((found.by_text, found.by_name), (4, 3));
    assert_eq!(found.branch, "main");
    let file = gateway.excerpts[0]
        .file
        .as_ref()
        .expect("the file it is shown by");
    assert!(
        file.path.ends_with("SettleBridgeServer.java"),
        "shown by its class: {}",
        file.path
    );
    // The ticket whose comment names it, found by its text.
    assert_eq!(rows(&answer, "ticket"), ["Ledger drift on partial refunds"]);

    // Under *Titles*, nothing its files hold.
    assert!(rows(&ask(&demo, true, "settlebridge").await, "repo").is_empty());
}

#[tokio::test]
async fn the_fixtures_files_are_read_or_left_out_by_the_rules() {
    let (demo, _) = demo("everything-fixture-rules").await;
    let left_out: serde_json::Value = sqlx::query_scalar(
        "select left_out from sync.code_repo where repository_id = 'mock:payout-service'",
    )
    .fetch_one(&demo.pool)
    .await
    .unwrap();
    assert_eq!(
        left_out,
        serde_json::json!({
            "binary": 1, "folder": 1, "generated": 1, "lock_file": 1, "long_line": 1
        }),
        "the NUL, the file under node_modules, the generated head, the lock file and the \
         3,000-byte line"
    );
    // Five files say RetryQueue in payout-service and two in ledger-api: more
    // files first.
    assert_eq!(
        rows(&ask(&demo, false, "retryqueue").await, "repo"),
        ["payout-service", "ledger-api"]
    );
    // The latin-1 file is read as Windows-1252, and found by its word.
    assert_eq!(
        rows(&ask(&demo, false, "größe").await, "repo"),
        ["payout-service"]
    );
    // A file left out is still found by its name.
    assert_eq!(
        rows(&ask(&demo, false, "pspcodes").await, "repo"),
        ["payout-service"]
    );
}

#[tokio::test]
async fn a_second_pass_over_the_same_heads_reads_nothing() {
    let (demo, _) = demo("everything-fixture-again").await;
    let again = code_files::refresh_within(
        &demo.pool,
        "mock",
        &MockSource::new(),
        &demo.files,
        Bounds::default(),
        u64::MAX,
        &|_| {},
    )
    .await
    .unwrap();
    assert_eq!(again, code_files::Passed::default());
}

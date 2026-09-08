//! Demo mode against a real PostgreSQL.
//!
//! `test_util` hands every test in this binary the *same* database, and the
//! binary runs them concurrently -- so the absolute row counts below are only
//! meaningful while nothing else is syncing `mock`. Every test that touches
//! that source takes [`MOCK`] for its whole duration; a test that touches only
//! an id no adapter answers to (`jira`) needs no lock.

use knobas_app::assets::ESTATE_FILE_PRODUCER;
use knobas_app::checkout::{FoundBy, set_clones_root, view};
use knobas_app::commands::entity::get_entity_inner;
use knobas_app::sources::demo;

/// Held by every test that syncs the `mock` source.
///
/// One shared database, one shared source: two demo loads racing would make
/// the counts below depend on which test's transaction committed first, and
/// would defeat the "not registered yet" precondition the first test opens
/// with outright. `tokio`'s mutex rather than `std`'s because it is held
/// across awaits.
static MOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[tokio::test]
async fn demo_load_registers_once_and_syncs_the_same_rows_every_time() {
    let _guard = MOCK.lock().await;
    let pool = knobas_db::test_util::test_pool().await;
    let pool = &pool;
    knobas_db::migrate::run(pool).await.unwrap();

    // The lock keeps other tests out, but not an earlier one that already ran:
    // the precondition below is about an *unregistered* source, so establish
    // it rather than assume it. Only the configuration row goes -- the mirror
    // rows stay, which is what makes the idempotence assertions further down
    // real work rather than a first load into an empty table.
    sqlx::query("delete from knobas.source_config where id = 'mock'")
        .execute(pool)
        .await
        .unwrap();

    // (An unregistered source being refused outright is
    // `knobas_sync`'s `cursor::a_source_with_no_configuration_row_is_refused_...`;
    // it moved there with the M0 `sync_now_inner` this file used to drive.)

    let first = demo::demo_load_inner(pool).await.unwrap();
    assert_eq!(first.source_id, "mock");
    assert!(
        first.upserted > 10,
        "expected the full fixture, got {}",
        first.upserted
    );

    // *One* load has to be enough to leave the cursor behind, and that is what
    // pins the ordering inside `demo_load_inner`: registration first, then the
    // run. Reversed, the run finds no `source_config` row to persist into --
    // `run_once` updates a row and deliberately never invents one -- so the
    // cursor is dropped on the floor and the row the registration then creates
    // has `cursor` NULL. Reading this only after a *second* load would hide
    // exactly that: the first load would create the row and the second would
    // fill it in, and the assertion would hold either way.
    let (stored,): (Option<String>,) =
        sqlx::query_as("select cursor from knobas.source_config where id = 'mock'")
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(
        stored.as_deref(),
        Some(first.cursor.as_str()),
        "one demo load must leave a resumable cursor -- did the run happen before the registration?"
    );

    // Loading the demo twice is something a user can do by double-clicking the
    // button: it must not double the corpus, nor add a second configuration.
    let second = demo::demo_load_inner(pool).await.unwrap();
    assert_eq!(first.upserted, second.upserted);
    assert_eq!(first.cursor, second.cursor);

    let (sources,): (i64,) =
        sqlx::query_as("select count(*) from knobas.source_config where id = 'mock'")
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(sources, 1, "the mock was registered more than once");

    let (items,): (i64,) =
        sqlx::query_as("select count(*) from sync.item where source_id = 'mock'")
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(
        items as u64, first.upserted,
        "a re-run duplicated rows instead of upserting them"
    );

    // The stored cursor is exactly what the scheduler picks up: an incremental
    // run from it has nothing left to do. Driven through the engine's own entry
    // point, on a connection of its own -- the shape §10.6(c) requires and the
    // shape the scheduler uses.
    let mut conn = knobas_db::test_util::test_connector()
        .await
        .connect()
        .await
        .unwrap();
    let incremental = knobas_sync::run_from_stored_cursor(
        &mut conn,
        pool,
        &knobas_source_mock::MockSource::new(),
    )
    .await
    .unwrap();
    assert_eq!(
        incremental.upserted, 0,
        "an incremental run over a frozen fixture writes nothing"
    );
    assert_eq!(
        incremental.cursor, first.cursor,
        "and it leaves the position where the full sync put it"
    );
}

/// An id no *configured source* answers to is refused as `NotFound`, and
/// nothing is written.
///
/// M0 asserted this against `"jira"` as an id **no adapter** answered to. Jira
/// is a compiled-in adapter now, so the interesting refusal moved: what makes
/// an id unusable is having no `knobas.source_config` row, whatever adapters
/// exist. Driven through `set_secret`, which is the real path a user reaches
/// (*Re-enter password* on a row that has since been deleted); the half about
/// leaving no `sync_run` behind is the scheduler's
/// `scheduler_loop::triggering_a_source_that_does_not_exist_is_refused_without_a_log_row`.
#[tokio::test]
async fn an_unconfigured_source_cannot_be_reached() {
    use std::sync::Arc;

    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    let secrets: Arc<dyn knobas_secrets::SecretStore> =
        Arc::new(knobas_secrets::MemoryStore::new());
    let registry = knobas_app::sources::Registry::builtin();

    let missing = format!("nope-{}", &uuid::Uuid::new_v4().simple().to_string()[..12]);
    let error = knobas_app::sources::crud::set_secret(
        &pool,
        &secrets,
        &registry,
        &missing,
        knobas_app::sources::SecretInput::of("pat"),
    )
    .await
    .expect_err("there is no such source");
    assert!(
        matches!(&error, knobas_app::sources::SourcesError::NotFound(id) if *id == missing),
        "unexpected error: {error}"
    );
    assert_eq!(
        knobas_app::sources::to_ipc(&error, Some(&missing)).code,
        knobas_app::IpcErrorCode::NotFound
    );
    assert!(
        secrets
            .get(&knobas_secrets::KeychainAccount::source(&missing))
            .unwrap()
            .is_none(),
        "a source that does not exist must not get a keychain item"
    );
}

/// The README's demo-mode promise, end to end: load the fixture, then find
/// `mock:PAY-231` by searching for `sepa retry`.
///
/// This is the acceptance check the contract PR's exit checklist asks a human
/// to perform by clicking *Load demo data* and typing in the search box. Every
/// step between those two gestures is here -- the fixture through the mock
/// adapter, the mirror upsert, the generated `fts` column, `sync.live_item`,
/// and `knobas_search::search` -- so what a human still has to verify is the
/// button and the rendering, not the pipeline. It also pins the item count the
/// README states, which nothing else did.
#[tokio::test]
async fn the_loaded_fixture_is_searchable_the_way_the_readme_promises() {
    let _guard = MOCK.lock().await;
    let pool = knobas_db::test_util::test_pool().await;
    let pool = &pool;
    knobas_db::migrate::run(pool).await.unwrap();

    let report = demo::demo_load_inner(pool).await.unwrap();
    assert_eq!(
        report.upserted, 27,
        "every item the Tidewater fixture holds: 7 tickets, 3 PRs, 3 builds, \
         5 pages, 3 commits, and -- since #537 -- 3 repos and 3 branches"
    );

    let response = knobas_search::Searcher::new(pool.clone())
        .search(knobas_search::SearchQuery {
            raw: "sepa retry".to_owned(),
            limit: 20,
            filters: knobas_search::SearchFilters::default(),
        })
        .await
        .unwrap();

    let ticket = response
        .groups
        .iter()
        .find(|group| group.kind == "ticket")
        .expect("a ticket group");
    assert!(
        ticket
            .hits
            .iter()
            .any(|hit| hit.row.entity_id == "mock:PAY-231"),
        "`sepa retry` must find mock:PAY-231 in the ticket group; got {:?}",
        response
            .groups
            .iter()
            .flat_map(|g| g.hits.iter().map(|h| h.row.entity_id.clone()))
            .collect::<Vec<_>>()
    );
}

/// What the mirror holds for one entity after a demo load, read through
/// `sync.live_item` — the view every reader in the app goes through, so a row
/// that came back from here is both mirrored and live.
async fn mirrored_payload(pool: &sqlx::PgPool, entity_id: &str) -> serde_json::Value {
    let (payload,): (serde_json::Value,) =
        sqlx::query_as("select payload from sync.live_item where entity_id = $1")
            .bind(entity_id)
            .fetch_one(pool)
            .await
            .unwrap_or_else(|e| panic!("{entity_id} must be mirrored and live: {e}"));
    payload
}

/// The demo corpus carries its projects into the **mirror**, in the shape a
/// source would have written them: `fields.project`, with the source's own key
/// and name, which is where `crates/knobas-source-jira` leaves a Jira Data
/// Center issue's project and therefore the spelling a payload read outside an
/// adapter (ADR-0007) already has an arm for.
///
/// Read out of `sync.live_item` rather than out of
/// `knobas_source_mock::fixture()`: reading the fixture back would pin the
/// transcription and nothing else, and it is the **adapter-to-mirror** seam --
/// the fixture through `MockSource::sync` and the upsert -- that the rest of
/// M2.6 reads. Both projects are asserted here because the dataset splitting
/// its ticket families across two of them is what makes a project room
/// something the demo profile can show at all.
#[tokio::test]
async fn a_demo_ticket_carries_its_project_into_the_mirrored_payload() {
    let _guard = MOCK.lock().await;
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    demo::demo_load_inner(&pool).await.unwrap();

    let payouts = mirrored_payload(&pool, "mock:PAY-231").await;
    assert_eq!(payouts["fields"]["project"]["key"], "PAY");
    assert_eq!(payouts["fields"]["project"]["name"], "Payments Platform");
    assert_eq!(
        payouts.pointer("/project"),
        None,
        "the project lives at fields.project only -- a flat duplicate would be \
         a third spelling no real source writes: {payouts}"
    );

    let operations = mirrored_payload(&pool, "mock:OPS-77").await;
    assert_eq!(operations["fields"]["project"]["key"], "OPS");
    assert_eq!(operations["fields"]["project"]["name"], "Operations");
}

/// The upgrade path (#234): a profile that has been syncing since **before**
/// the fixture gained projects repairs itself on its next scheduled run.
///
/// The precondition is reconstructed rather than assumed -- the stored cursor
/// put back to `"tidewater-v1"`, the value the mock handed out until #234, and
/// the mirrored payload stripped of `fields`, which is byte-for-byte the shape
/// `ticket_payload` produced before #230 added the only key under it. Without
/// both halves the test would witness nothing: a run over an already-correct
/// mirror cannot tell a repair from a no-op.
///
/// Driven through `run_from_stored_cursor`, the entry point the **scheduler**
/// uses, because the criterion is that an existing profile heals on its own
/// next tick. Nothing here asks for a backfill and nothing clears a cursor by
/// hand -- that hand-clearing was the workaround this defect forced, and it is
/// precisely what must stop being necessary.
#[tokio::test]
async fn a_profile_stored_at_an_older_fixture_version_re_syncs_and_gains_the_project() {
    let _guard = MOCK.lock().await;
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    demo::demo_load_inner(&pool).await.unwrap();

    sqlx::query("update knobas.source_config set cursor = 'tidewater-v1' where id = 'mock'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "update sync.item set payload = payload - 'fields' where entity_id = 'mock:PAY-231'",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        mirrored_payload(&pool, "mock:PAY-231")
            .await
            .pointer("/fields/project"),
        None,
        "the precondition itself has to hold, or the assertion below is vacuous"
    );

    let mut conn = knobas_db::test_util::test_connector()
        .await
        .connect()
        .await
        .unwrap();
    let report = knobas_sync::run_from_stored_cursor(
        &mut conn,
        &pool,
        &knobas_source_mock::MockSource::new(),
    )
    .await
    .unwrap();
    assert!(
        report.upserted > 10,
        "a stale cursor must be re-sent the whole corpus, got {} item(s)",
        report.upserted
    );

    let payload = mirrored_payload(&pool, "mock:PAY-231").await;
    assert_eq!(payload["fields"]["project"]["key"], "PAY");
    assert_eq!(payload["fields"]["project"]["name"], "Payments Platform");

    let (stored,): (Option<String>,) =
        sqlx::query_as("select cursor from knobas.source_config where id = 'mock'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        stored.as_deref(),
        Some(report.cursor.as_str()),
        "and the repaired profile has to be left at the new position -- a run \
         that repairs but does not advance repeats the full corpus for ever"
    );
}

/// The miss direction, and the reason the fixture deliberately leaves one
/// ticket outside every project: a record naming no project syncs like any
/// other, and its mirrored payload carries **no** project rather than an empty
/// string, a null, or a key somebody inferred from the issue key.
///
/// This is what the rest of M2.6 pins its own failure direction against
/// (ADR-0010: absence, never a wrong room), so the demo profile has to be able
/// to produce it -- and it is asserted on the mirrored payload, because an
/// invented value is something the *emitting* step would add.
#[tokio::test]
async fn a_demo_ticket_with_no_project_syncs_and_its_payload_names_none() {
    let _guard = MOCK.lock().await;
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    demo::demo_load_inner(&pool).await.unwrap();

    let key = knobas_source_mock::UNPROJECTED_KEY;
    let payload = mirrored_payload(&pool, &format!("mock:{key}")).await;
    assert_eq!(
        payload["key"], key,
        "the ticket itself must still be mirrored"
    );
    assert_eq!(
        payload.pointer("/fields/project"),
        None,
        "a ticket naming no project must carry none: {payload}"
    );
    assert_eq!(
        payload.pointer("/project"),
        None,
        "and none under any other spelling either: {payload}"
    );
}

/// The census over the **mock's own corpus**: the demo profile shows exactly
/// the two projects the Tidewater dataset names, and shows them under the
/// source's own names for them.
///
/// The seam this closes is the one between the two halves already covered, and
/// it is narrower than #238 supposed. The ticket expected a fixture move to
/// pass every existing test; it does not.
/// `a_demo_ticket_carries_its_project_into_the_mirrored_payload` above pins the
/// *payload* the fixture produces, so moving the project one level fails that
/// test and
/// `a_profile_stored_at_an_older_fixture_version_re_syncs_and_gains_the_project`
/// with it -- both landed in #236, after the ticket was drafted.
///
/// What nothing covered is the **other direction**: the fixture and the read
/// disagreeing. `knobas-core/tests/projects.rs` pins the census over payloads
/// written by hand, so `project_key_read!` could be pointed at a path the
/// fixture never writes and every one of those tests would still pass while the
/// demo profile showed no project rooms at all. That is not hypothetical: #234
/// was this defect reaching a person's window by a different route, and the
/// repair had to be found from the outside.
///
/// Read through `project::list`, which is what `list_projects` answers with and
/// therefore what the switcher's project rooms are built from (#209) -- the
/// whole chain from `fixtures/tidewater/work.json` through `MockSource::sync`
/// and the upsert to the room a reader sees.
///
/// Scoped to `mock` because this binary shares one database and the census is a
/// pass over the whole live corpus. Every test here mirrors under `mock` today,
/// so the filter changes nothing yet; it is what keeps this an equality about
/// the demo dataset rather than about whatever a later test in this binary
/// happens to mirror beside it.
#[tokio::test]
async fn the_demo_corpus_shows_exactly_the_two_projects_its_fixture_names() {
    let _guard = MOCK.lock().await;
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    demo::demo_load_inner(&pool).await.unwrap();

    // Through the declarations the running binary's adapters make (#277): the
    // demo source is `knobas_source_mock`, and where its records keep a
    // project is that adapter's own declaration, not a path this file spells.
    let declarations = knobas_app::sources::paths::declared_paths(
        &pool,
        &knobas_app::sources::Registry::builtin(),
    )
    .await
    .expect("what the configured sources declare");
    let census = knobas_core::project::list(&pool, &declarations)
        .await
        .expect("the census the switcher's project rooms are built from");
    let shown: Vec<(String, Option<String>)> = census
        .into_iter()
        .filter(|project| project.source_id == "mock")
        .map(|project| (project.key, project.name))
        .collect();

    // Ordered by key, which is the order `project::list` promises and the order
    // the switcher offers the rooms in. Both, and exactly both: a dataset that
    // lost one of them would still draw *a* project room, which is why this is
    // an equality and not a pair of `contains`.
    assert_eq!(
        shown,
        vec![
            ("OPS".to_owned(), Some("Operations".to_owned())),
            ("PAY".to_owned(), Some("Payments Platform".to_owned())),
        ],
        "the demo profile has to show a room for each project its fixture names"
    );
}

/// **The demo profile carries the real estate, and a second start changes
/// nothing** (#440, criterion 1).
///
/// `--demo` loads `testenv/hetzner/estate.json` beside the Tidewater work, and
/// the reason it is *that* file rather than a Tidewater-shaped one is
/// ADR-0013: a made-up estate would show that the Tree draws a tree, and what
/// M4.0 has to show is that a real estate fits the model. The counts below are
/// the file's own and are read off it here rather than written down, so a file
/// that grows a server moves this test with it instead of failing it.
///
/// The idempotence half is `demo_load_registers_once_and_syncs_the_same_rows_every_time`'s
/// for the estate: a person double-clicking *Load demo data*, and a demo
/// profile that is started again tomorrow, must not get a second copy of the
/// estate. The import's own idempotence is `assets_ipc`'s
/// (`a_second_import_of_the_same_file_is_all_known_and_changes_nothing`);
/// what this asserts is that the **demo load** is the caller that inherits it.
#[tokio::test]
async fn the_demo_load_brings_the_real_estate_and_a_second_start_changes_nothing() {
    let _guard = MOCK.lock().await;
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();

    let file: serde_json::Value =
        serde_json::from_str(include_str!("../../../testenv/hetzner/estate.json"))
            .expect("the estate file parses");
    let ids = |key: &str| -> Vec<String> {
        file[key]
            .as_array()
            .expect("a list")
            .iter()
            .map(|entry| entry["id"].as_str().expect("an id").to_owned())
            .collect()
    };
    let (assets, routes) = (ids("assets"), ids("routes"));

    let report = demo::demo_load_inner(&pool).await.unwrap();
    assert_eq!(
        report.upserted, 27,
        "the work half is the whole fixture: the estate rides beside it, not instead of it"
    );

    let stored = |pool: sqlx::PgPool, statement: &'static str| async move {
        sqlx::query_scalar::<_, String>(statement)
            .fetch_all(&pool)
            .await
            .expect("the stored ids")
    };
    let after_one = (
        stored(pool.clone(), "select id from knobas.asset order by id").await,
        stored(pool.clone(), "select id from knobas.route order by id").await,
    );

    let mut expected = (assets.clone(), routes.clone());
    expected.0.sort();
    expected.1.sort();
    assert_eq!(
        after_one, expected,
        "the demo estate is the checked-in file, id for id"
    );

    // A second start of the demo profile. The whole file is known, so the
    // preview has nothing new and nothing to change -- which is the claim
    // criterion 3 makes about a re-import, made here about a restart.
    demo::demo_load_inner(&pool).await.unwrap();
    let preview = knobas_app::assets::preview_import(
        &pool,
        include_str!("../../../testenv/hetzner/estate.json"),
        ESTATE_FILE_PRODUCER,
    )
    .await
    .expect("the preview");
    assert_eq!(
        (
            preview.known.len(),
            preview.new.len(),
            preview.changes.len()
        ),
        (assets.len() + routes.len(), 0, 0),
        "a restarted demo profile previews the whole estate as already in the tree"
    );

    let after_two = (
        stored(pool.clone(), "select id from knobas.asset order by id").await,
        stored(pool.clone(), "select id from knobas.route order by id").await,
    );
    assert_eq!(
        after_two, after_one,
        "a second demo load changed the estate"
    );

    // The ids alone cannot carry that sentence, and it is worth saying why:
    // `knobas.asset.id` is the primary key, so a load that inserted the file
    // twice would fail on the conflict rather than appear here as a longer
    // list. What a re-import *can* get wrong is a column -- `ADD_MONITORS` is
    // an `array_cat`, so a plan that thought the names were missing would
    // double every one of them -- and one line per asset in the stream, where
    // the file is authoritative and nothing happened.
    let monitors: Vec<Vec<String>> =
        sqlx::query_scalar("select monitors from knobas.asset order by id")
            .fetch_all(&pool)
            .await
            .expect("the kept names");
    assert_eq!(
        monitors.concat().len(),
        7,
        "a second load appended the file's monitor names again"
    );

    let origins: Vec<(String, i64)> = sqlx::query_as(
        "select entity_id, count(*) from knobas.activity
          where actor = 'import' and entity_id is not null
          group by entity_id order by entity_id",
    )
    .fetch_all(&pool)
    .await
    .expect("the origin lines the imports wrote");
    // Per entity and not an absolute count of the table: this binary's
    // database is shared and several of its tests call `demo_load_inner`, so
    // how many *summary* lines there are depends on which of them have run.
    // How many origin lines one asset has does not -- it is one, for every
    // entry in the file, however many times the demo has been loaded.
    assert_eq!(
        origins.len(),
        assets.len() + routes.len(),
        "one origin line per entry the file names"
    );
    let twice: Vec<&(String, i64)> = origins.iter().filter(|(_, n)| *n != 1).collect();
    assert!(
        twice.is_empty(),
        "a second load wrote a second origin line: {twice:?}"
    );
}

/// The chain `open-in-editor`'s desktop driver stands on, minus the window
/// (#537, #501, #525).
///
/// The driver plants a clone under a temporary root, types that root into
/// Settings, asks the launcher for `payout-service` and presses a button on
/// the detail that opens. Everything in that sentence except the typing and
/// the pressing is here: the demo corpus through `MockSource::sync` and the
/// upsert, the entity read that has to answer under the name the driver asks
/// for, and `checkout::view` matching a `.git/config` the driver's own remote
/// is written into.
///
/// **Why the remote is spelled the driver's way and not normalised.**
/// `checkout_ipc.rs` deliberately plants an ssh remote so the match is
/// `knobas_core::checkout`'s normalisation doing its work. This one plants the
/// exact string `testenv/desktop-witness/drivers/open-in-editor.sh` writes,
/// because what it witnesses is different: that the URL the *fixture* now
/// emits and the URL the *driver* writes reduce to one repository. A repo item
/// whose `web_url` lost the `tidewater` owner segment would still normalise
/// fine and would still miss the driver's clone.
///
/// **Its own database, and therefore no [`MOCK`] guard.** The clones root is
/// one `knobas.setting` row per database, which cannot be namespaced by a
/// fixture id -- the reason `checkout_ipc.rs` gives for its own scratch
/// database. A test that set it on this binary's shared pool would be setting
/// it for every other test here.
#[tokio::test]
async fn the_demo_corpus_answers_the_checkout_the_desktop_driver_opens() {
    let pool = knobas_db::test_util::scratch_database("demo-checkout")
        .await
        .pool(4)
        .await
        .expect("a pool onto the scratch db");
    knobas_db::migrate::run(&pool).await.unwrap();
    demo::demo_load_inner(&pool).await.unwrap();

    // 1. The entity read answers under the name the driver types.
    let detail = get_entity_inner(&pool, "mock:payout-service")
        .await
        .expect("the demo corpus must carry the repository the driver opens");
    assert_eq!(detail.row.kind, "repo");
    assert_eq!(detail.row.title, "payout-service");
    assert_eq!(
        detail.kind_info.as_ref().map(|k| k.plural.as_str()),
        Some("Repositories"),
        "the launcher draws this group's header from the adapter's declaration"
    );
    assert_eq!(detail.payload["lang"], "Rust");

    // 2. The clone the driver plants, and the root it types into Settings.
    //    `clone` and not `checkout`: `CONTEXT.md` keeps the two apart -- "the
    //    clone is the directory, the checkout is what knobas knows about it",
    //    and what `view` answers below is the second.
    let root = tempfile::tempdir().unwrap();
    let clone = root.path().join("payout-service");
    std::fs::create_dir_all(clone.join(".git")).unwrap();
    std::fs::write(
        clone.join(".git").join("config"),
        "[core]\n\tbare = false\n[remote \"origin\"]\n\turl = \
         https://tidewater.example/tidewater/payout-service\n",
    )
    .unwrap();
    set_clones_root(&pool, Some(&root.path().to_string_lossy()))
        .await
        .unwrap();

    // 3. The panel the button lives on finds it.
    let answer = view(&pool, "mock:payout-service").await.unwrap();
    assert_eq!(answer.found_by, FoundBy::Scan);
    assert_eq!(
        answer.path.as_deref(),
        Some(clone.to_string_lossy().as_ref()),
        "the scan must match the fixture's repo URL against the driver's remote"
    );
    assert_eq!(
        answer.repo_entity_id.as_deref(),
        Some("mock:payout-service")
    );

    // 4. And a branch of it answers the same checkout, which is the whole
    //    reason the branch keys extend the repo's: `repo_of` finds a branch's
    //    repository by that prefix and nothing else. The launcher's group
    //    order puts branches *above* repositories (`knobas_search`'s
    //    `GROUP_ORDER`), so this is the detail the driver's own ⌘K-and-Return
    //    is at least as likely to land on.
    let branch = view(
        &pool,
        "mock:payout-service@refs/heads/feature/PAY-231-sepa-retry",
    )
    .await
    .unwrap();
    assert_eq!(
        branch.repo_entity_id.as_deref(),
        Some("mock:payout-service")
    );
    assert_eq!(branch.path, answer.path);
}

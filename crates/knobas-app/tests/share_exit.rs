//! **The M4.2 exit witness** (#455): a share export taken from a working
//! profile, restored on a clean one, read back through the surfaces a person
//! would open.
//!
//! #454 witnessed the archive: which tables each part is, that a partial one
//! restores, that no secret crosses. Those are claims about a *file*, and
//! every one of them can hold while the thing the milestone promises does
//! not -- spec #427's M4.2 exit is a sentence about the far machine ("a share
//! export restores on a clean machine with nothing personal in it"), and the
//! only way to know is to stand on the far machine and look.
//!
//! So this file is the checklist, in the order the ticket writes it:
//!
//! 1. the estate is in the Tree;
//! 2. the context's room lists its members;
//! 3. a link to a Jira ticket resolves once the Jira source is configured and
//!    synced;
//! 4. no note and no block came along.
//!
//! # One profile, one archive, one restore
//!
//! [`shared`] builds a sharer's profile, exports it **once** with the ratified
//! defaults, and restores that one archive into one clean profile; every test
//! below reads the same restored database. Six restores would be six
//! machines, and the claim would be about none of them.
//!
//! What the sharer's profile holds is the real estate --
//! `testenv/hetzner/estate.json`, the machines this repository is developed
//! on, as ADR-0013 and `tests/it/estate_exit.rs` require -- plus the things the
//! criterion names: a room with a member, a link to a Jira ticket, and the two
//! kinds of private thing a share export exists to leave behind, a note and an
//! afternoon's work.
//!
//! # Clause 3 needs a Jira, and where its two halves live
//!
//! "Resolves once the Jira source is configured and synced" is two claims. The
//! first -- that the link crosses and the *ticket behind it* does not -- is
//! decidable without a network and is asserted here in
//! [`the_shared_link_crosses_and_the_ticket_behind_it_has_not_arrived_yet`].
//! The second is a claim about a real system answering, and no fixture can
//! stand in for it: [`a_shared_link_resolves_once_the_jira_source_is_configured_and_synced`]
//! is `#[ignore]`d and runs against testenv's seeded Jira, on Hetzner, through
//! `just atlassian-live`.
//!
//! # What this cannot witness
//!
//! The pixels, as `tests/it/estate_exit.rs` records at length for M4.0: `just
//! demo` opens a Tauri window and headless Chrome cannot attach to one. The
//! *Share…* dialog and the archive list are pinned by
//! `app/src/lib/settings/BackupSection.test.svelte.ts`; what is here is
//! everything behind them.

mod live_digest;

use std::sync::Arc;

use knobas_app::assets::ESTATE_FILE_PRODUCER;
use knobas_app::commands::entity::get_entity_inner;
use knobas_app::{assets, backup};
use knobas_core::entity::EntityRef;
use knobas_core::link::Origin;
use sqlx::PgPool;

/// The estate as provisioned, embedded -- the same bytes `sources::demo`,
/// `tests/it/assets_ipc.rs` and `tests/it/estate_exit.rs` read.
const ESTATE_FILE: &str = include_str!("../../../testenv/hetzner/estate.json");

/// The container the checklist follows, the server that holds it, and where
/// the container sits by the names a reader sees.
///
/// The same three `tests/it/estate_exit.rs` follows, on purpose: M4.0's exit
/// asserted them of a freshly imported profile, and this file asserts them of
/// a profile that has been through an archive and back. Two claims about one
/// asset, and the pair is what says a share export moved the estate rather
/// than something shaped like it.
const CONTAINER: &str = "asset:knobas-teamcity";
const SERVER: &str = "asset:hetzner-teamcity";
const CONTAINER_PATH: &str =
    "knobas test estate / Hetzner Cloud nbg1 / knobas-teamcity / Docker engine (knobas-teamcity)";

/// The ticket the sharer's link names.
///
/// `PAY-231` because it is testenv's own fixture ticket -- "Retry failed SEPA
/// payouts", seeded into the real Jira by `testenv/seed-atlassian-content.sh`
/// -- so the offline half below and the live one at the bottom are about the
/// same address, and the live half can ask the real system for it by name.
const TICKET: &str = "jira:PAY-231";

/// The source id, which is also the entity namespace [`TICKET`] is in (P10).
const JIRA: &str = "jira";

/// What the sharer's room is called.
const ROOM_TITLE: &str = "the teamcity box";

/// The private things a share export exists to leave behind.
const NOTE_TITLE: &str = "How to drain the payout queue";
const NOTE_BODY: &str = "ssh to the box, then `just drain` -- and tell nobody.";

/// The one restored profile, and the directories the archive travelled
/// between.
///
/// A `OnceCell` for the reason `tests/it/estate_exit.rs` holds one: "one machine"
/// is the claim, and a helper that restored per test would make each assertion
/// true of a machine nobody else saw. It holds the **connector** rather than a
/// pool, because each `#[tokio::test]` builds a runtime of its own and drops
/// it, and a `PgPool` shared across those belongs to whichever runtime built
/// it.
///
/// The `TempDir`s ride along because dropping one deletes the archive under a
/// test still reading it.
static RESTORED: tokio::sync::OnceCell<Handover> = tokio::sync::OnceCell::const_new();

struct Handover {
    clean: knobas_db::embedded::Connector,
    _sharer_dir: tempfile::TempDir,
    _clean_dir: tempfile::TempDir,
}

/// A backup service over a database and a directory of its own.
async fn service(label: &str) -> (Arc<backup::BackupState>, tempfile::TempDir) {
    service_with(label, Arc::new(knobas_secrets::MemoryStore::new())).await
}

/// The same, over a keychain the caller holds a handle to -- which the live
/// test needs, because *configuring a source* is putting a credential in one.
async fn service_with(
    label: &str,
    secrets: Arc<dyn knobas_secrets::SecretStore>,
) -> (Arc<backup::BackupState>, tempfile::TempDir) {
    let connector = knobas_db::test_util::scratch_database(label).await;
    let pool = connector.pool(4).await.expect("a pool");
    let dir = tempfile::tempdir().expect("a backup directory");
    (
        Arc::new(backup::BackupState::new(
            pool,
            connector,
            dir.path().to_path_buf(),
            secrets,
        )),
        dir,
    )
}

/// Everything the sharer has, written into `pool`.
///
/// It hands nothing back, deliberately: every row below is addressed by a
/// constant at the top of this file or found by the read under test, and a
/// fixture that returned the ids it made would let an assertion pass on a row
/// this function invented rather than on the estate file's own.
async fn seed_the_estate_the_room_and_the_private_things(pool: &PgPool) {
    let outcome = assets::apply_import(pool, ESTATE_FILE, ESTATE_FILE_PRODUCER)
        .await
        .expect("the estate file imports into an empty profile")
        .value;
    assert_eq!(
        (outcome.assets_created, outcome.routes_created),
        (23, 9),
        "the real estate, as `tests/it/estate_exit.rs` counts it"
    );

    // A room, with the server in it. *Add to context* is an ordinary link
    // (ADR-0008), and membership counts through ancestors -- so the container
    // arrives in the room two levels below the row anybody drew.
    let room = knobas_core::context::create_adhoc(pool, ROOM_TITLE)
        .await
        .expect("a room");
    knobas_core::link::create(
        pool,
        &EntityRef::parse(&room.id).expect("a ref"),
        &EntityRef::parse(SERVER).expect("a ref"),
        "related",
        Origin::Manual,
        None,
        "user",
    )
    .await
    .expect("the server is in the room");

    // The private things -- one row in **every** table the two off-by-default
    // parts name, and not only the two the criterion says out loud. A table
    // the sharer never had is a table the assertion on the far side cannot
    // fail on, so "no worklog crossed" would be a sentence about a fixture
    // rather than about the archive.
    knobas_core::note::create(pool, NOTE_TITLE, NOTE_BODY, &[], "user")
        .await
        .expect("a note");
    sqlx::query(
        "insert into knobas.block (started_at, ended_at, entity_id, kind)
         values (now() - interval '3 hours', now() - interval '1 hour', $1, 'manual')",
    )
    .bind(CONTAINER)
    .execute(pool)
    .await
    .expect("an afternoon on the container");
    sqlx::query(
        "insert into knobas.worklog (entity_id, started_at, seconds, comment, block_ids)
         select $1, now() - interval '3 hours', 7200, 'moved the runner off the box',
                array[b.id]
           from knobas.block b where b.entity_id = $1",
    )
    .bind(CONTAINER)
    .execute(pool)
    .await
    .expect("the hours that afternoon became");
    sqlx::query("insert into knobas.timer (entity_id, label, started_at) values ($1, null, now())")
        .bind(CONTAINER)
        .execute(pool)
        .await
        .expect("a running timer");
    sqlx::query("insert into knobas.heartbeat (at, entity_id) values (now(), $1)")
        .bind(CONTAINER)
        .execute(pool)
        .await
        .expect("an observation behind it");
}

/// The ticket, the link to it, and the source it came from -- **stood in for**.
///
/// The offline half of this file has no Jira to sync, so the rows a sync would
/// have written are written here: the entity, the mirror row, and a source
/// standing at a position. The live test at the bottom does not call this; it
/// has a real Jira write all three.
async fn stand_in_for_a_sync(pool: &PgPool) {
    // The ticket's address, which is what a sync would have written.
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'ticket', $2)")
        .bind(TICKET)
        .bind("Retry failed SEPA payouts")
        .execute(pool)
        .await
        .expect("the mirrored ticket's address");
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
         values ($1, $2, 'ticket', 'Retry failed SEPA payouts', 'the sharer synced this', '{}')",
    )
    .bind(TICKET)
    .bind(JIRA)
    .execute(pool)
    .await
    .expect("the sharer's own mirror row");
    // The Jira source, standing where a synced source stands.
    sqlx::query(
        "insert into knobas.source_config
             (id, kind, display_name, base_url, auth_kind, auth_state, cursor)
         values ($1, 'jira', 'Tidewater Jira', 'https://jira.example', 'pat', 'ok', $2)",
    )
    .bind(JIRA)
    .bind(r#"{"v":1,"since":"2026-09-06T12:00:00Z"}"#)
    .execute(pool)
    .await
    .expect("a configured source");
}

/// The link a colleague is being handed: the container, and the ticket about
/// it.
async fn link_the_container_to_the_ticket(pool: &PgPool) {
    knobas_core::link::create(
        pool,
        &EntityRef::parse(CONTAINER).expect("a ref"),
        &EntityRef::parse(TICKET).expect("a ref"),
        "related",
        Origin::Manual,
        None,
        "user",
    )
    .await
    .expect("the link a colleague is being handed");
}

/// The clean profile, with the sharer's share export restored into it.
async fn shared() -> PgPool {
    let handover = RESTORED
        .get_or_init(|| async {
            let (sharer, sharer_dir) = service("m42-exit-sharer").await;
            seed_the_estate_the_room_and_the_private_things(&sharer.pool).await;
            stand_in_for_a_sync(&sharer.pool).await;
            link_the_container_to_the_ticket(&sharer.pool).await;

            let record = backup::share_export(&sharer, backup::ShareParts::default())
                .await
                .expect("a share export with the ratified defaults");
            sharer.pool.close().await;

            // A clean machine: migrated, and holding nothing at all.
            let (clean, clean_dir) = service("m42-exit-clean").await;
            std::fs::copy(
                sharer_dir.path().join(&record.file),
                clean_dir.path().join(&record.file),
            )
            .expect("hand the archive over");
            backup::restore(&clean, &record.file)
                .await
                .expect("the archive restores onto a clean machine");
            clean.pool.close().await;

            Handover {
                clean: clean.connector.clone(),
                _sharer_dir: sharer_dir,
                _clean_dir: clean_dir,
            }
        })
        .await;
    handover
        .clean
        .pool(4)
        .await
        .expect("a pool onto the restored profile")
}

// ---------------------------------------------------------------------------
// Clause 1: the estate is in the Tree
// ---------------------------------------------------------------------------

/// **The estate is in the Tree on the clean machine** (#455, criterion 2).
///
/// Through `assets::tree` and `assets::get`, which are what the Tree's columns
/// and its held-by line call -- not a row count, because "23 rows are in the
/// table" and "the Tree draws the estate" are different claims and only the
/// second one is the criterion. The path is the whole five-deep chain, in
/// names, which is the reading that would break if the archive had carried the
/// assets and lost the containment between them.
#[tokio::test]
async fn the_estate_is_in_the_tree_on_the_clean_machine() {
    let pool = &shared().await;

    let roots = assets::tree(pool, None)
        .await
        .expect("the Tree's top level");
    assert_eq!(
        roots
            .iter()
            .map(|row| row.name.as_str())
            .collect::<Vec<_>>(),
        ["knobas test estate"],
        "the estate's own root, and nothing beside it"
    );
    assert!(
        roots[0].has_children,
        "a root with no children is a Tree with one column"
    );

    let detail = assets::get(pool, CONTAINER).await.expect("the pane's read");
    assert_eq!(
        detail
            .held_by
            .iter()
            .map(|row| row.name.as_str())
            .collect::<Vec<_>>()
            .join(" / "),
        CONTAINER_PATH,
        "the containment path crossed whole, five levels deep"
    );
    let environment = detail
        .effective_environment
        .expect("an environment is in force");
    assert_eq!(
        (environment.value.as_str(), environment.source_id.as_str()),
        ("dev", "asset:knobas-estate"),
        "and the inheritance still walks the restored path"
    );

    let routes: i64 = sqlx::query_scalar("select count(*) from knobas.route")
        .fetch_one(pool)
        .await
        .expect("the routes");
    assert_eq!(routes, 9, "the estate's routes are part of the assets part");
}

// ---------------------------------------------------------------------------
// Clause 2: the context's room lists its members
// ---------------------------------------------------------------------------

/// **The room lists its members on the clean machine** (#455, criterion 2;
/// ADR-0008's ancestor clause).
///
/// One link crossed -- the server into the room -- and the tile on the far
/// side is the server *and everything it holds*, because membership is walked
/// and not stored. That is what makes this a test of the restore rather than
/// of a copied table: a machine that had the `knobas.link` row and had lost
/// `knobas.asset.parent_id` would list exactly one member.
///
/// The negative is the other half: the site above the server is not in the
/// room, so the tile is the subtree and not the estate.
#[tokio::test]
async fn the_contexts_room_lists_its_members_on_the_clean_machine() {
    let pool = &shared().await;

    let rooms = knobas_core::context::list(pool).await.expect("the rooms");
    let [room] = rooms.as_slice() else {
        panic!("one room crossed, not {}", rooms.len())
    };
    assert_eq!(room.title, ROOM_TITLE);

    let tile: Vec<(String, Option<String>)> = assets::in_context(pool, &room.id)
        .await
        .expect("the room's Assets tile")
        .into_iter()
        .map(|member| (member.asset.id, member.path))
        .collect();
    let listed: Vec<&str> = tile.iter().map(|(id, _)| id.as_str()).collect();

    assert!(
        listed.contains(&SERVER),
        "the server somebody added is not in the room it was added to: {listed:?}"
    );
    assert!(
        listed.contains(&CONTAINER),
        "membership counts through ancestors, so the container the server holds \
         is a member too: {listed:?}"
    );
    assert!(
        !listed.contains(&"asset:hetzner-nbg1"),
        "the site *above* the server is not a member: {listed:?}"
    );
    assert_eq!(
        tile.iter()
            .find(|(id, _)| id == CONTAINER)
            .and_then(|(_, path)| path.clone())
            .as_deref(),
        Some(CONTAINER_PATH),
        "and the tile draws each member's path, which is what tells the container \
         from the server of the same name"
    );
}

// ---------------------------------------------------------------------------
// Clause 4: no note and no block came along
// ---------------------------------------------------------------------------

/// **Nothing personal came along** (#455, criterion 2).
///
/// Every table the two off-by-default parts name -- the note, and the timer,
/// its block, the worklog it became and the observation behind it -- plus the
/// two that are in no part at all, the activity stream and the mirror. The
/// sharer holds a row in each of them, which is what makes the count on this
/// side an assertion about the archive and not about a table nobody filled.
///
/// Counted over the whole database rather than by id, which is the stronger
/// reading: an archive that had brought *some other* note would pass an
/// assertion about this one.
///
/// The last assertion is the narrowing #454 recorded and Björn has yet to
/// rule on, asserted rather than described: `knobas.entity` travels whole
/// because `pg_dump` restricts by table and never by row, so the note's
/// **title** is on the clean machine and its body is not. A reader of this
/// file should meet that here rather than in a colleague's knobas.
#[tokio::test]
async fn no_note_and_no_block_came_along() {
    let pool = &shared().await;

    for table in ["note", "block", "worklog", "timer", "heartbeat", "activity"] {
        let count: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "select count(*) from knobas.{table}"
        )))
        .fetch_one(pool)
        .await
        .unwrap_or_else(|error| panic!("count knobas.{table}: {error}"));
        assert_eq!(count, 0, "knobas.{table} crossed in a share export");
    }
    let mirrored: i64 = sqlx::query_scalar("select count(*) from sync.item")
        .fetch_one(pool)
        .await
        .expect("the mirror");
    assert_eq!(
        mirrored, 0,
        "the mirror is in no part: it re-syncs, and the sharer's copy of it is theirs"
    );

    // ...and the one thing that does cross, which is not the body: the note's
    // address, with no note behind it.
    let addressed: Vec<(String, i64)> = sqlx::query_as(
        "select e.title, (select count(*) from knobas.note n where n.id = e.id)
           from knobas.entity e where e.kind = 'note'",
    )
    .fetch_all(pool)
    .await
    .expect("the note's address");
    assert_eq!(
        addressed,
        vec![(NOTE_TITLE.to_owned(), 0)],
        "an entity row is an address and the whole address book travels (#454): \
         the title is here, the note it names is not"
    );
}

// ---------------------------------------------------------------------------
// Clause 3, first half: the link crosses, the ticket does not
// ---------------------------------------------------------------------------

/// **The link is on the clean machine and the ticket behind it is not**
/// (#455, criterion 2; spec #427's story 78).
///
/// Three readings of one state, because "resolves" is about to mean the
/// change between them:
///
/// * the link is in the pane, with the far end's kind and title -- those come
///   from `knobas.entity`, which crossed;
/// * the ticket itself does not open: `get_entity_inner` reads the mirror, and
///   a share export carries none;
/// * the source it would come from is here, saying it has no credential and
///   standing at no position.
///
/// That last one is the whole of why the live test below can pass. A restored
/// source that had kept the sharer's cursor would ask Jira for what changed
/// since the sharer's last sync, get nothing, and leave this link dangling for
/// ever (#455; the escape #454 recorded was that somebody had to know to order
/// a backfill by hand).
#[tokio::test]
async fn the_shared_link_crosses_and_the_ticket_behind_it_has_not_arrived_yet() {
    let pool = &shared().await;

    let detail = assets::get(pool, CONTAINER).await.expect("the pane's read");
    let [link] = detail.links.as_slice() else {
        panic!("one link on the container, not {}", detail.links.len())
    };
    assert_eq!(
        (
            link.other.entity_id.as_str(),
            link.other.kind.as_str(),
            link.other.title.as_str()
        ),
        (TICKET, "ticket", "Retry failed SEPA payouts"),
        "the link map is what a share export is for"
    );

    let refused = get_entity_inner(pool, TICKET)
        .await
        .expect_err("nothing is mirrored on a machine that has not synced");
    assert_eq!(
        refused.code,
        knobas_app::IpcErrorCode::NotFound,
        "the ticket cannot be opened yet, which is the honest state"
    );

    let (auth_state, cursor): (String, Option<String>) =
        sqlx::query_as("select auth_state, cursor from knobas.source_config where id = $1")
            .bind(JIRA)
            .fetch_one(pool)
            .await
            .expect("the restored source configuration");
    assert_eq!(
        auth_state, "missing_secret",
        "no archive carries a credential, and this keychain holds none"
    );
    assert_eq!(
        cursor, None,
        "and it stands nowhere, so its first run reads Jira from the top"
    );
}

// ---------------------------------------------------------------------------
// Clause 3, second half: the live one
// ---------------------------------------------------------------------------

/// Where testenv's seeded Jira is, and who to be at it.
///
/// The same three variables every live suite in this repository reads, so one
/// `eval "$(cd testenv && ./seed --env)"` serves all of them. Missing is a
/// panic naming the recipe rather than a skip: a suite that skips quietly
/// reports "1 passed" and has witnessed nothing (#351).
struct Jira {
    url: String,
    user: String,
    password: String,
}

fn jira() -> Jira {
    let need = |key: &str| {
        std::env::var(key)
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| {
                panic!(
                    "{key} is not set -- this test needs testenv's seeded Jira. From the \
                 repo root: `just atlassian-live`; or, inside a window already open, \
                 `eval \"$(cd testenv && ./seed --env)\"`"
                )
            })
    };
    Jira {
        url: need("KNOBAS_JIRA_URL").trim_end_matches('/').to_owned(),
        user: need("KNOBAS_JIRA_USER"),
        password: need("KNOBAS_JIRA_PASSWORD"),
    }
}

/// The engine room: a scheduler over one profile, and the state the app's own
/// sources commands take.
async fn engine(
    state: &Arc<backup::BackupState>,
    secrets: Arc<dyn knobas_secrets::SecretStore>,
) -> knobas_app::sources::SourcesState {
    let scheduler =
        knobas_sync::scheduler::Scheduler::start(knobas_sync::scheduler::SchedulerDeps {
            pool: state.pool.clone(),
            connections: Arc::new(live_digest::Connections(state.connector.clone())),
            registry: Arc::new(knobas_app::sources::Registry::builtin()),
            secrets: Arc::clone(&secrets),
            events: Arc::new(live_digest::Quiet),
            timing: knobas_sync::scheduler::SchedulerTiming::default(),
        })
        .await
        .expect("a scheduler over the profile");
    knobas_app::sources::SourcesState {
        pool: state.pool.clone(),
        scheduler,
        secrets,
        registry: Arc::new(knobas_app::sources::Registry::builtin()),
    }
}

/// **A shared link resolves once the Jira source is configured and synced**
/// (#455, criterion 2, third clause; spec #427's stories 78 and 79).
///
/// The whole sentence, against the real Jira, in the order a person lives it:
///
/// 1. the sharer configures Jira, syncs, and draws a link from a container in
///    the estate to `PAY-231`;
/// 2. they take a share export and hand it over;
/// 3. the colleague restores it into a clean profile, where the link is drawn
///    and the ticket behind it **cannot be opened** -- no archive carries a
///    mirror;
/// 4. the colleague configures the same source, which is entering the
///    credential (`sources::crud::set_secret`, the seam the *Sources* view's
///    password field calls, which also tests the connection);
/// 5. one sync, and the ticket opens with what the real Jira answered.
///
/// # What makes step 5 possible, and what it would look like without it
///
/// `backup::restore` clears every restored source's `cursor` (#455). Without
/// that, the colleague's first run resumes from where the *sharer* stood: it
/// asks Jira for what changed since somebody else's last sync, is told
/// nothing, writes no mirror rows, and reports success -- and the link stays
/// an id with no ticket behind it, for ever, until somebody knows to order a
/// backfill by hand. That is the one mutation worth running against this test,
/// and it is the reason this file exists rather than a `#[test]` over a
/// fixture.
///
/// # What it writes at the far end
///
/// Nothing. Every call it makes is a read -- two connection tests and two
/// syncs -- and it authenticates as the seeded admin with the password
/// `seed-state.json` records -- **the right one**, deliberately: this
/// server's elevated-security check locks the account out after wrong ones
/// (`testenv/README.md`), so no test here ever offers a wrong password. No
/// ticket, comment, worklog or token is created, so there is nothing to clean
/// up and no `PAY` counter is advanced.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs testenv's seeded Jira on Hetzner: `just atlassian-live`"]
async fn a_shared_link_resolves_once_the_jira_source_is_configured_and_synced() {
    let env = jira();

    // ---- 1. The sharer, with a Jira that answers ---------------------------
    let sharer_keys: Arc<dyn knobas_secrets::SecretStore> =
        Arc::new(knobas_secrets::MemoryStore::new());
    let (sharer, sharer_dir) = service_with("m42-live-sharer", Arc::clone(&sharer_keys)).await;
    seed_the_estate_the_room_and_the_private_things(&sharer.pool).await;

    knobas_sync::config::insert(
        &sharer.pool,
        &knobas_sync::config::InsertConfig {
            id: JIRA.to_owned(),
            adapter_kind: "jira".to_owned(),
            display_name: "Tidewater Jira (seeded)".to_owned(),
            base_url: env.url.clone(),
            auth_kind: knobas_sync::config::AuthKind::Method(
                knobas_source::AuthMethod::UserPassword,
            ),
            // The username is half of the pair and what `@me` matches on; the
            // Add-source dialog fills it in from *Test connection*.
            config: serde_json::json!({ "username": env.user }),
            sync_interval_secs: 86_400,
            enabled: true,
        },
    )
    .await
    .expect("the sharer configures Jira");
    let sharer_engine = engine(&sharer, Arc::clone(&sharer_keys)).await;
    knobas_app::sources::crud::set_secret(
        &sharer.pool,
        &sharer_keys,
        sharer_engine.registry.as_ref(),
        JIRA,
        knobas_app::sources::SecretInput::of(env.password.clone()),
    )
    .await
    .expect("the sharer's credential is accepted by the real Jira");
    live_digest::sync(&sharer_engine, JIRA).await;

    let stood_at: Option<String> =
        sqlx::query_scalar("select cursor from knobas.source_config where id = $1")
            .bind(JIRA)
            .fetch_one(&sharer.pool)
            .await
            .expect("the sharer's source");
    assert!(
        stood_at.is_some(),
        "a source that has synced stands somewhere -- without that this test \
         cannot witness what the position does to the far machine"
    );
    let mirrored: i64 = sqlx::query_scalar("select count(*) from sync.item where source_id = $1")
        .bind(JIRA)
        .fetch_one(&sharer.pool)
        .await
        .expect("the sharer's mirror");
    assert!(
        mirrored >= 7,
        "the seeded corpus is seven issues; the sharer mirrored {mirrored}"
    );
    link_the_container_to_the_ticket(&sharer.pool).await;

    // ---- 2. The archive ----------------------------------------------------
    let record = backup::share_export(&sharer, backup::ShareParts::default())
        .await
        .expect("a share export with the ratified defaults");
    // Last, because it closes the pool it was started over -- a `PgPool` is a
    // handle onto one pool and every clone of it closes together.
    sharer_engine.scheduler.shutdown().await;

    // ---- 3. The clean machine ----------------------------------------------
    let colleague_keys: Arc<dyn knobas_secrets::SecretStore> =
        Arc::new(knobas_secrets::MemoryStore::new());
    let (colleague, colleague_dir) =
        service_with("m42-live-clean", Arc::clone(&colleague_keys)).await;
    std::fs::copy(
        sharer_dir.path().join(&record.file),
        colleague_dir.path().join(&record.file),
    )
    .expect("hand the archive over");
    backup::restore(&colleague, &record.file)
        .await
        .expect("the archive restores onto a clean machine");

    let before = assets::get(&colleague.pool, CONTAINER)
        .await
        .expect("the container's pane on the clean machine");
    assert_eq!(
        before
            .links
            .iter()
            .map(|entry| entry.other.entity_id.as_str())
            .collect::<Vec<_>>(),
        vec![TICKET],
        "the link is what a share export is for"
    );
    assert_eq!(
        before.asset.linked_work, 0,
        "and the work it names is not here yet: the badge counts the mirror"
    );
    assert_eq!(
        get_entity_inner(&colleague.pool, TICKET)
            .await
            .expect_err("nothing is mirrored yet")
            .code,
        knobas_app::IpcErrorCode::NotFound
    );

    // ---- 4. ...configures the same source ----------------------------------
    let colleague_engine = engine(&colleague, Arc::clone(&colleague_keys)).await;
    let health = knobas_app::sources::crud::set_secret(
        &colleague.pool,
        &colleague_keys,
        colleague_engine.registry.as_ref(),
        JIRA,
        knobas_app::sources::SecretInput::of(env.password.clone()),
    )
    .await
    .expect("the colleague enters their own credential");
    assert_eq!(
        health.state,
        knobas_sync::health::AuthState::Ok,
        "the source the archive brought is reachable once it has a credential"
    );

    // ---- 5. ...and syncs ---------------------------------------------------
    live_digest::sync(&colleague_engine, JIRA).await;

    let opened = get_entity_inner(&colleague.pool, TICKET)
        .await
        .expect("the shared link's ticket opens on the clean machine");
    assert_eq!(
        opened.row.title, "Retry failed SEPA payouts",
        "and it is the real Jira's own answer, not a title the archive carried"
    );
    assert!(
        opened
            .links
            .iter()
            .any(|entry| entry.other.entity_id == CONTAINER),
        "the link reads from the ticket's side too: {:?}",
        opened.links
    );
    let after = assets::get(&colleague.pool, CONTAINER)
        .await
        .expect("the container's pane, after the sync");
    assert_eq!(
        after.asset.linked_work, 1,
        "the linked-work badge counts it now, which is the change a reader sees"
    );

    colleague_engine.scheduler.shutdown().await;
}

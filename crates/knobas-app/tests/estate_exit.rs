//! **The M4.0 exit witness** (#440): the checklist, run against the real
//! estate in a fresh profile.
//!
//! Every other M4.0 ticket witnessed its own surface over an estate it built
//! for the purpose -- `hel1` holding `vm-db-01` holding `postgres`, three or
//! four assets shaped to make one claim decidable. That is the right fixture
//! for a rule and the wrong one for a milestone: an estate written to fit the
//! model cannot say whether the model fits an estate. ADR-0013 is the rule
//! (*the real container is the witness*) and spec #427 restates it for the
//! assets stream (*nothing Tidewater-shaped is invented for them*), so what
//! this file drives is `testenv/hetzner/estate.json` -- the machines this
//! repository is developed, tested and released on, written down where they
//! are provisioned.
//!
//! # One profile, one import, the whole checklist
//!
//! The exit criterion is a sentence about *a profile*: the file imports, a
//! container is found in the launcher with its path, ⌘T times it, adding its
//! server to a context lists the container in the room's Assets tile, a tunnel
//! forward draws its wire, and a re-import previews everything as already in
//! the tree. Six tests over six databases would be six profiles, and the claim
//! would be about none of them. So [`estate`] imports the file **once** into
//! one scratch database, every test here reads that one estate, and what the
//! file asserts as a whole is that those six things are true of the same
//! twenty-three assets at the same time.
//!
//! They still do not collide: the timer is a singleton and exactly one test
//! starts one, the context test creates its own room, and every search is for
//! a name the estate file itself carries.
//!
//! # What this cannot witness, and where that half lives
//!
//! The pixels. `just demo` opens a Tauri window and headless Chrome cannot
//! attach to one (#428 established that and #429 and #439 re-established it),
//! so the drawing is witnessed by `app/src/lib/assets/*.test*.ts` over the
//! same shapes and photographed through `?fake-ipc`, whose fixture is this
//! same file (`fake-tauri.ts`). What *this* covers is everything between the
//! two: the store, the merge rule, the corpus, the membership walk and the
//! wire's two ends.

use knobas_app::assets;
use knobas_app::commands::search::search_inner;
use knobas_app::time::{self, TimerTarget};
use knobas_search::{SearchFilters, SearchQuery};
use sqlx::PgPool;

/// The estate as provisioned, embedded -- the same bytes `sources::demo` and
/// `tests/assets_ipc.rs` embed.
const ESTATE_FILE: &str = include_str!("../../../testenv/hetzner/estate.json");

/// The container the checklist follows, and the server that holds it.
///
/// `knobas-teamcity` **twice**, and that is the point rather than an
/// oversight: the estate names the Hetzner server and the TeamCity container
/// on it the same thing, which is why `testenv/hetzner/README.md` documents
/// the `hetzner-` id convention at all. A launcher hit for that name is
/// therefore ambiguous by construction, and the only thing that tells the two
/// apart on screen is the path -- which is what criterion 2's *"with its
/// path"* is asking for.
const CONTAINER: &str = "asset:knobas-teamcity";
const SERVER: &str = "asset:hetzner-teamcity";

/// Where the container sits, by the names a reader sees.
const CONTAINER_PATH: &str =
    "knobas test estate / Hetzner Cloud nbg1 / knobas-teamcity / Docker engine (knobas-teamcity)";

/// The tunnel forward that reaches it, and the notebook that exposes it.
const FORWARD: &str = "route:tunnel-teamcity";
const NOTEBOOK: &str = "asset:notebook";

/// How to reach the one imported profile every test here reads.
///
/// A `OnceCell` rather than a fixture function, because "one profile" is the
/// claim: a helper that imported per test would make each assertion true of an
/// estate nobody else saw.
///
/// It holds the **connector** and not the pool. Every `#[tokio::test]` builds
/// a runtime of its own and drops it at the end of the test, and a `PgPool`
/// shared across those is a pool whose background tasks belong to whichever
/// runtime happened to build it -- *"a Tokio 1.x context was found, but it is
/// being shutdown"*, in three of the seven tests here, decided by scheduling.
/// A connector is configuration, so it crosses runtimes safely and each test
/// opens connections of its own onto the same database, which is what
/// `test_util::test_pool` does for a binary's shared one.
static ESTATE: tokio::sync::OnceCell<knobas_db::embedded::Connector> =
    tokio::sync::OnceCell::const_new();

/// The fresh profile, with the estate file imported into it.
///
/// The import is asserted here rather than in a test of its own, because every
/// test below is downstream of it: a counted outcome that did not hold would
/// otherwise be reported five times over as five unrelated failures.
async fn estate() -> PgPool {
    let connector = ESTATE
        .get_or_init(|| async {
            let connector = knobas_db::test_util::scratch_database("m4-exit").await;
            let pool = connector.pool(4).await.expect("a fresh profile");

            let outcome = assets::apply_import(&pool, ESTATE_FILE)
                .await
                .expect("the estate file imports into an empty profile")
                .value;

            assert_eq!(
                (
                    outcome.assets_created,
                    outcome.routes_created,
                    outcome.properties_set,
                    outcome.properties_kept,
                    outcome.monitors_kept,
                    outcome.monitors_linked,
                ),
                (23, 9, 0, 0, 7, 0),
                "the first import of the real estate: every entry is an insert, so \
                 nothing is set on a row that was already there and nothing is kept \
                 from a hand edit; the seven Uptime Kuma names are kept on the assets \
                 and none of them resolves to a monitor, because no adapter emits the \
                 kind until M4.1"
            );
            pool.close().await;
            connector
        })
        .await;
    connector
        .pool(4)
        .await
        .expect("a pool onto the imported profile")
}

/// What the file says, read off the file rather than written down again.
fn file() -> serde_json::Value {
    serde_json::from_str(ESTATE_FILE).expect("the estate file parses")
}

/// Every id under `key` (`"assets"` or `"routes"`), in the file's own order.
fn file_ids(key: &str) -> Vec<String> {
    file()[key]
        .as_array()
        .expect("a list")
        .iter()
        .map(|entry| entry["id"].as_str().expect("an id").to_owned())
        .collect()
}

/// A plain launcher query, the shape the box sends.
fn query(raw: &str) -> SearchQuery {
    SearchQuery {
        raw: raw.to_owned(),
        limit: 20,
        filters: SearchFilters::default(),
    }
}

// ---------------------------------------------------------------------------
// Criterion 2, clause 1: the estate file imports
// ---------------------------------------------------------------------------

/// **The tree in the profile is the tree in the file** (#440, criterion 2).
///
/// Ids, containment and the inheritance, in one read. The path is the whole
/// five-deep chain the estate actually has -- site › site › VM › engine ›
/// container -- and it is spelled in names, because that is what the Tree's
/// held-by line draws and what tells this container from the server above it.
///
/// The environment and the owner are the *inheritance* half: the file sets
/// them on the root and on nothing else (`estate_file.rs` holds it to that),
/// so a container five levels down reporting `dev` from `asset:knobas-estate`
/// is `assets::inherited` walking the real path rather than a value copied
/// twenty-three times.
#[tokio::test]
async fn the_estate_file_imports_into_a_fresh_profile_with_its_own_ids() {
    let pool = &estate().await;

    let stored: Vec<String> = sqlx::query_scalar("select id from knobas.asset order by id")
        .fetch_all(pool)
        .await
        .expect("the assets");
    let mut expected = file_ids("assets");
    expected.sort();
    assert_eq!(stored, expected, "the estate keeps the file's own ids");

    let stored: Vec<String> = sqlx::query_scalar("select id from knobas.route order by id")
        .fetch_all(pool)
        .await
        .expect("the routes");
    let mut expected = file_ids("routes");
    expected.sort();
    assert_eq!(stored, expected, "and the file's own route ids");

    let detail = assets::get(pool, CONTAINER).await.expect("the pane's read");
    assert_eq!(
        detail
            .held_by
            .iter()
            .map(|row| row.name.as_str())
            .collect::<Vec<_>>()
            .join(" / "),
        CONTAINER_PATH,
        "the containment path is the file's, five levels deep"
    );

    let environment = detail
        .effective_environment
        .expect("an environment is in force");
    assert_eq!(
        (environment.value.as_str(), environment.source_id.as_str()),
        ("dev", "asset:knobas-estate"),
        "the environment is inherited from the root, which is the only asset that sets one"
    );
    let owner = detail.effective_owner.expect("an owner is in force");
    assert_eq!(
        (owner.value.as_str(), owner.source_id.as_str()),
        ("Björn Förschner", "asset:knobas-estate")
    );
}

/// **The seven monitor names are kept, and none of them is a link yet**
/// (#440, and spec #427's *"a name the mirror does not hold yet is kept on the
/// asset and resolved by the next import or the M4.1 sync"*).
///
/// The negative is the load-bearing half. Nothing emits the `monitor` kind
/// until M4.1, so an estate whose seven names had all resolved would mean the
/// import had invented monitors to link to -- and a reader looking at the
/// demo's Tree would be looking at seven links to nothing.
#[tokio::test]
async fn the_files_monitor_names_are_kept_on_the_assets_and_resolve_to_nothing_yet() {
    let pool = &estate().await;

    let named: Vec<(String, Vec<String>)> = sqlx::query_as(
        "select id, monitors from knobas.asset
         where cardinality(monitors) > 0 order by id",
    )
    .fetch_all(pool)
    .await
    .expect("the kept names");

    let mut expected: Vec<(String, Vec<String>)> = file()["assets"]
        .as_array()
        .expect("the assets")
        .iter()
        .filter_map(|asset| {
            let monitors: Vec<String> = asset["monitors"]
                .as_array()?
                .iter()
                .map(|name| name.as_str().expect("a name").to_owned())
                .collect();
            Some((asset["id"].as_str().expect("an id").to_owned(), monitors))
        })
        .collect();
    expected.sort();

    assert_eq!(named, expected, "the file's names, on the file's assets");
    assert_eq!(
        named.iter().map(|(_, names)| names.len()).sum::<usize>(),
        7,
        "seven names in the real estate: three pings, three product checks, one local"
    );

    let links: i64 = sqlx::query_scalar(
        "select count(*) from knobas.link where relation = $1 and deleted_at is null",
    )
    .bind(assets::MONITORED_BY)
    .fetch_one(pool)
    .await
    .expect("the monitor links");
    assert_eq!(
        links, 0,
        "no adapter emits a monitor until M4.1, so a name that resolved would be an invention"
    );
}

// ---------------------------------------------------------------------------
// Criterion 2, clause 2: found in the launcher, with its path
// ---------------------------------------------------------------------------

/// **A container is found in the launcher, and the path is what tells it from
/// the server it runs on** (#440, criterion 2; story 34).
///
/// `knobas-teamcity` names two assets in the real estate, and a reader typing
/// it gets both. The assertion is therefore on the *pairs* -- name and path --
/// because a launcher answering with two identical rows has found the
/// container and told nobody which one it is.
///
/// Through `search_inner`, which is the command's own body, so this is the
/// corpus and the ranking rather than a shape composed for the test.
#[tokio::test]
async fn a_container_is_found_in_the_launcher_with_the_path_that_tells_it_apart() {
    let pool = &estate().await;

    let found = search_inner(pool, query("knobas-teamcity"))
        .await
        .expect("the launcher's answer");

    let hits: Vec<(String, Option<String>)> = found
        .groups
        .iter()
        .flat_map(|group| &group.hits)
        .filter(|hit| hit.row.entity_id == CONTAINER || hit.row.entity_id == SERVER)
        .map(|hit| (hit.row.entity_id.clone(), hit.row.path.clone()))
        .collect();

    assert_eq!(
        hits,
        vec![
            (CONTAINER.to_owned(), Some(CONTAINER_PATH.to_owned())),
            (
                SERVER.to_owned(),
                Some("knobas test estate / Hetzner Cloud nbg1".to_owned())
            ),
        ],
        "both assets called knobas-teamcity are found, each with the path it sits at"
    );

    // And the group they arrive in is the estate's, so the launcher draws them
    // under Assets rather than beside a ticket.
    let group = found
        .groups
        .iter()
        .find(|group| group.hits.iter().any(|hit| hit.row.entity_id == CONTAINER))
        .expect("a group holding the container");
    assert_eq!(group.kind, "asset");
}

// ---------------------------------------------------------------------------
// Criterion 2, clause 3: ⌘T times it
// ---------------------------------------------------------------------------

/// **⌘T on the container times it, and the day review names the container**
/// (#440, criterion 2; story 46).
///
/// #433 witnessed an asset as a timer target over a two-asset estate. What is
/// added here is the ambiguity the real estate has: the block's title is
/// joined out of `knobas.entity`, and *knobas-teamcity* is the name of two
/// rows, so a join that lost the id would still produce the expected string.
/// The target is therefore asserted by **id** and the title by name, and the
/// two together are what say the afternoon went on the container.
#[tokio::test]
async fn cmd_t_on_the_container_times_it_and_the_day_review_names_it() {
    let pool = &estate().await;
    let on = TimerTarget::Entity {
        entity_id: CONTAINER.to_owned(),
    };

    let started = time::start(pool, on.clone(), None)
        .await
        .expect("an asset is a target the timer accepts");
    assert_eq!(started.timer.target, on);

    let stopped = time::stop(pool)
        .await
        .expect("it stops")
        .expect("it was running");
    assert_eq!(
        stopped.block.target, on,
        "the block forgot which asset the work was on"
    );

    let day = time::day::list(
        pool,
        stopped.block.started_at,
        chrono::Utc::now() + chrono::Duration::hours(1),
    )
    .await
    .expect("the day is readable")
    .blocks;
    let [drawn] = day.as_slice() else {
        panic!("one block on this profile's day, not {}", day.len())
    };
    assert_eq!(drawn.title.as_deref(), Some("knobas-teamcity"));
}

// ---------------------------------------------------------------------------
// Criterion 2, clause 4: the server in a context lists the container
// ---------------------------------------------------------------------------

/// **Adding the server to a context lists the container in the room's Assets
/// tile** (#440, criterion 2; ADR-0008's ancestor clause, story 41).
///
/// One link is drawn -- the server into the room -- and the container arrives
/// two levels below it, through the container engine, because membership
/// counts through ancestors and over nothing else. The negative is the other
/// half of that sentence and is what a fixture of one branch could not make:
/// the site **above** the server is not a member, and neither is the Jira
/// server beside it, so the tile is the subtree and not the estate.
#[tokio::test]
async fn the_server_in_a_context_brings_the_container_into_the_rooms_assets_tile() {
    let pool = &estate().await;

    let room = knobas_core::context::create_adhoc(pool, "the teamcity box")
        .await
        .expect("a room");
    knobas_core::link::create(
        pool,
        &knobas_core::entity::EntityRef::parse(&room.id).expect("a ref"),
        &knobas_core::entity::EntityRef::parse(SERVER).expect("a ref"),
        "related",
        knobas_core::link::Origin::Manual,
        None,
        "user",
    )
    .await
    .expect("Add to context is an ordinary link");

    let tile: Vec<(String, String, Option<String>)> = assets::in_context(pool, &room.id)
        .await
        .expect("the tile's read")
        .into_iter()
        .map(|row| (row.asset.id, row.asset.name, row.path))
        .collect();

    // Worst first and then by name, which is `assets::in_context`'s order.
    // Nothing in the estate carries a status yet, so the whole tile is the
    // name sort -- and it puts the **two rows called `knobas-teamcity`** next
    // to each other, the server and the container on it, separated by their id
    // and distinguishable on screen by nothing but the path.
    let drawn: Vec<&str> = tile.iter().map(|(id, _, _)| id.as_str()).collect();
    assert_eq!(
        drawn,
        [
            "asset:hetzner-teamcity-docker",
            SERVER,
            CONTAINER,
            "asset:knobas-teamcity-agent",
            "asset:db-teamcity",
        ],
        "the server and everything it holds, and nothing else"
    );

    let container = tile
        .iter()
        .find(|(id, _, _)| id == CONTAINER)
        .expect("the container is in the tile");
    assert_eq!(
        (container.1.as_str(), container.2.as_deref()),
        ("knobas-teamcity", Some(CONTAINER_PATH)),
        "and it carries the path, which is the only thing telling it from its own server"
    );

    for outside in [
        "asset:hetzner-nbg1",
        "asset:knobas-estate",
        "asset:hetzner-jira",
        "asset:knobas-jira",
    ] {
        assert!(
            !drawn.contains(&outside),
            "{outside} is not below the server and must not be a member"
        );
    }
}

// ---------------------------------------------------------------------------
// Criterion 2, clause 5: a tunnel forward draws its wire
// ---------------------------------------------------------------------------

/// **The tunnel's TeamCity forward has both ends, from both panes** (#440,
/// criterion 2; story 31).
///
/// A wire is drawn from a route's row in the pane to the row of the asset at
/// its far end, so what the store owes the view is a route that appears on
/// both panes with a target that resolves. `route:tunnel-teamcity` is the real
/// forward `testenv/hetzner/tunnel` opens: exposed by the notebook, landing on
/// a container three machines away, which is the case a same-machine route
/// could not witness -- the far end is in a different column, and on a narrow
/// window behind a spine.
///
/// The geometry itself is `app/src/lib/assets/tree.ts`'s (`wiresFor`), tested
/// there; the claim here is that the two ends the geometry needs are what the
/// real estate hands it.
#[tokio::test]
async fn the_tunnels_teamcity_forward_has_both_ends_and_the_wire_can_be_drawn() {
    let pool = &estate().await;

    let notebook = assets::get(pool, NOTEBOOK).await.expect("the notebook");
    let exposed = notebook
        .exposes
        .iter()
        .find(|route| route.id == FORWARD)
        .expect("the notebook exposes the tunnel's TeamCity forward");
    assert_eq!(
        (
            exposed.asset_id.as_str(),
            exposed.target_id.as_deref(),
            exposed.target_name.as_deref(),
            exposed.url.as_str()
        ),
        (
            NOTEBOOK,
            Some(CONTAINER),
            Some("knobas-teamcity"),
            "http://127.0.0.1:8111/"
        ),
        "the wire's near end is the notebook's row and its far end is the container"
    );

    // The other pane: from the container, the same forward is what reaches it.
    let container = assets::get(pool, CONTAINER).await.expect("the container");
    let reaching = container
        .reachable_via
        .iter()
        .find(|route| route.id == FORWARD)
        .expect("the container is reachable through the forward");
    assert_eq!(
        reaching.asset_id.as_str(),
        NOTEBOOK,
        "and from this end the wire runs back to the asset that exposes it"
    );

    // Every forward the tunnel opens, so the claim is about the tunnel rather
    // than about one of its lines. `testenv/hetzner/tunnel` opens three `-L`
    // forwards and one `-R`, and the `-R` is the one that runs the other way:
    // it is exposed by a Hetzner server and lands back on the notebook's Gitea.
    let forwards: Vec<(String, Option<String>)> = sqlx::query_as(
        "select asset_id, target_id from knobas.route
         where id like 'route:tunnel-%' order by id",
    )
    .fetch_all(pool)
    .await
    .expect("the tunnel's routes");
    assert_eq!(
        forwards,
        vec![
            (
                NOTEBOOK.to_owned(),
                Some("asset:knobas-confluence".to_owned())
            ),
            (
                "asset:hetzner-teamcity".to_owned(),
                Some("asset:knobas-gitea".to_owned())
            ),
            (NOTEBOOK.to_owned(), Some("asset:knobas-jira".to_owned())),
            (NOTEBOOK.to_owned(), Some(CONTAINER.to_owned())),
        ],
        "all four forwards land on an asset, the reverse one included"
    );
}

// ---------------------------------------------------------------------------
// Criterion 3: a re-import previews every entry as already in the tree
// ---------------------------------------------------------------------------

/// **A re-import previews every asset and every route as already in the tree**
/// (#440, criterion 3).
///
/// The preview is a read that writes nothing, so it can be run against the
/// profile the rest of this file is using without disturbing it -- which is
/// also the property `assets_ipc`'s `a_preview_writes_nothing_at_all` asserts
/// directly.
///
/// Every entry by **id**, not a count: a preview that reported thirty-two
/// known entries because it had listed one of them thirty-two times would pass
/// a count and would still be a dialog telling a reader nothing.
#[tokio::test]
async fn a_re_import_previews_every_entry_as_already_in_the_tree() {
    let pool = &estate().await;

    let preview = assets::preview_import(pool, ESTATE_FILE)
        .await
        .expect("the preview");

    assert_eq!(preview.name, "knobas test estate");
    let known: Vec<&str> = preview
        .known
        .iter()
        .map(|entry| entry.id.as_str())
        .collect();
    let mut expected = file_ids("assets");
    expected.extend(file_ids("routes"));
    assert_eq!(
        known, expected,
        "every entry in the file is known, in the file's own order"
    );

    assert!(
        preview.new.is_empty() && preview.changes.is_empty() && preview.monitor_links.is_empty(),
        "a second import of an unchanged file has nothing to create, change or link: \
         {} new, {} changed, {} links",
        preview.new.len(),
        preview.changes.len(),
        preview.monitor_links.len()
    );
}

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
//! this file drives is `testenv/estate.json` -- the machines this
//! repository is developed, tested and released on, written down as they run.
//! (Until #634 that included three Hetzner servers; they were decommissioned
//! and the file, and this checklist, followed.)
//!
//! # One profile, one import, the whole checklist
//!
//! The exit criterion is a sentence about *a profile*: the file imports, a
//! container is found in the launcher with its path, ⌘T times it, adding its
//! machine to a context makes the container a member, a route to it draws its
//! wire, and a re-import previews everything as already in the tree. Six
//! tests over six databases would be six profiles, and the claim would be
//! about none of them. So [`estate`] imports the file **once** into one
//! scratch database, every test here reads that one estate, and what the file
//! asserts as a whole is that those six things are true of the same seventeen
//! assets at the same time.
//!
//! They still do not collide: the timer is a singleton and exactly one test
//! starts one, the context test creates its own room, and every search is for
//! a name the estate file itself carries.
//!
//! # What this cannot witness, and where that half lives
//!
//! The pixels. `just demo` opens a Tauri window and headless Chrome cannot
//! attach to one (#428 established that and #429 and #439 re-established it).
//! What stands in for them is two things, and neither is in this file: the
//! frontend suites over the same shapes (`app/src/lib/assets/*.test*.ts`, and
//! `app/src/lib/shell/dev/fake-tauri.test.ts`, whose fixture reads this same
//! estate file), and a `?fake-ipc` browser pass recorded on the pull request
//! the way this repo has recorded every one since #428. What *this* covers is
//! everything between the two: the store, the merge rule, the corpus, the
//! membership walk and the wire's two ends.

use knobas_app::assets::{self, ESTATE_FILE_PRODUCER};
use knobas_app::commands::search::search_inner;
use knobas_app::sources::Registry;
use knobas_app::time::{self, TimerTarget};
use knobas_search::{SearchFilters, SearchQuery};
use sqlx::PgPool;

/// The estate as it runs, embedded -- the same bytes `sources::demo` and
/// `tests/it/assets_ipc.rs` embed.
const ESTATE_FILE: &str = include_str!("../../../../testenv/estate.json");

/// The container the checklist follows, and the machine that holds it.
///
/// The TeamCity server, on the WSL VM. Until #634 it ran on a Hetzner
/// server that shared its name, `knobas-teamcity`, so a launcher hit for the
/// name was ambiguous by construction; the server is gone, and what criterion
/// 2's *"with its path"* asks for is now the path saying where the container
/// runs.
const CONTAINER: &str = "asset:knobas-teamcity";
const MACHINE: &str = "asset:wsl";

/// Where the container sits, by the names a reader sees.
const CONTAINER_PATH: &str =
    "knobas test estate / DESKTOP-QUUAHUB / Ubuntu 26.04 (WSL 2) / Docker engine (WSL)";

/// The tunnelled port that reaches it, and the notebook that exposes it.
const FORWARD: &str = "route:notebook-teamcity";
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
/// `test_util::test_pool` does for a file's shared one.
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

            let outcome = assets::apply_import(&pool, ESTATE_FILE, ESTATE_FILE_PRODUCER)
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
                (19, 6, 0, 0, 5, 0),
                "the first import of the real estate: every entry is an insert, so \
                 nothing is set on a row that was already there and nothing is kept \
                 from a hand edit; the five Uptime Kuma names are kept on the assets \
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
/// five-deep chain the estate actually has -- site › hypervisor › VM › engine
/// › container -- and it is spelled in names, because that is what the Tree's
/// held-by line draws.
///
/// The environment and the owner are the *inheritance* half: the file sets
/// them on the root and on nothing else (`estate_file.rs` holds it to that),
/// so a container five levels down reporting `dev` from `asset:knobas-estate`
/// is `assets::inherited` walking the real path rather than a value copied
/// seventeen times.
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

/// **The five monitor names are kept, and none of them is a link yet**
/// (#440, and spec #427's *"a name the mirror does not hold yet is kept on the
/// asset and resolved by the next import or the M4.1 sync"*).
///
/// The negative is the load-bearing half. Nothing emits the `monitor` kind
/// until M4.1, so an estate whose five names had all resolved would mean the
/// import had invented monitors to link to -- and a reader looking at the
/// demo's Tree would be looking at five links to nothing.
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
        5,
        "five names in the real estate: the notebook's ping, three product checks, Gitea's"
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

/// **A container is found in the launcher, with the path that says where it
/// runs** (#440, criterion 2; story 34).
///
/// The assertion is on the *pair* -- id and path -- because a launcher
/// answering with a bare name has found the container and told nobody where it
/// is. (Until #634 a Hetzner server carried the same name, and the path was the
/// only thing telling the two hits apart; the filter below still reads every
/// `knobas-teamcity` asset, so a second one arriving would be red here.)
///
/// Through `search_inner`, which is the command's own body, so this is the
/// corpus and the ranking rather than a shape composed for the test.
#[tokio::test]
async fn a_container_is_found_in_the_launcher_with_the_path_that_tells_it_apart() {
    let pool = &estate().await;

    let found = search_inner(pool, &Registry::for_process(), query("knobas-teamcity"))
        .await
        .expect("the launcher's answer");

    let hits: Vec<(String, Option<String>)> = found
        .groups
        .iter()
        .flat_map(|group| &group.hits)
        .filter(|hit| hit.row.entity_id.starts_with("asset:knobas-teamcity"))
        .map(|hit| (hit.row.entity_id.clone(), hit.row.path.clone()))
        .collect();

    assert_eq!(
        hits,
        vec![(CONTAINER.to_owned(), Some(CONTAINER_PATH.to_owned()))],
        "the container is found, with the path it sits at"
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
/// added here is the real estate's crowding: the block's title is joined out
/// of `knobas.entity`, and the container sits beside an agent whose name
/// starts with its own. The target is therefore asserted by **id** and the
/// title by name, and the two together are what say the afternoon went on the
/// container.
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
    // Selected by target rather than taken as the day's only block: the
    // profile is shared with every other test in this file, and an assertion
    // that reads "there is one block" is one about which tests have run.
    let mine: Vec<_> = day
        .iter()
        .filter(|drawn| drawn.block.target == on)
        .collect();
    let [drawn] = mine.as_slice() else {
        panic!("one block on the container, not {}", mine.len())
    };
    assert_eq!(
        drawn.title.as_deref(),
        Some("knobas-teamcity"),
        "the day review draws the container's own name, joined out of the \
         `knobas.entity` row the import wrote"
    );
}

// ---------------------------------------------------------------------------
// Criterion 2, clause 4: the machine in a context brings the container in
// ---------------------------------------------------------------------------

/// **Adding the machine to a context makes the container a member** (#440,
/// criterion 2; ADR-0008's ancestor clause, story 41). The room's Assets tile
/// that drew the members went with the rooms UI (#1087) and its read with
/// #1578; the membership it drew is `knobas_core::context::member_ids`'.
///
/// One link is drawn -- the WSL VM into the room -- and the container
/// arrives two levels below it, through the container engine, because
/// membership counts through ancestors and over nothing else. The negative is
/// the other half of that sentence: the desktop and the site **above** the VM,
/// and the notebook beside it, are not members, so the members are the subtree
/// and not the estate.
#[tokio::test]
async fn the_machine_in_a_context_brings_the_container_into_its_members() {
    let pool = &estate().await;

    let room = knobas_core::context::create_adhoc(pool, "the docker vm")
        .await
        .expect("a room");
    knobas_core::link::create(
        pool,
        &knobas_core::entity::EntityRef::parse(&room.id).expect("a ref"),
        &knobas_core::entity::EntityRef::parse(MACHINE).expect("a ref"),
        "related",
        knobas_core::link::Origin::Manual,
        None,
        "user",
    )
    .await
    .expect("Add to context is an ordinary link");

    let members = knobas_core::context::member_ids(pool, &room.id)
        .await
        .expect("the room's members");
    let mut drawn: Vec<&str> = members
        .iter()
        .map(String::as_str)
        .filter(|id| id.starts_with("asset:"))
        .collect();
    drawn.sort_unstable();
    assert_eq!(
        drawn,
        [
            "asset:db-confluence",
            "asset:db-jira",
            "asset:db-teamcity",
            "asset:knobas-confluence",
            "asset:knobas-confluence-db",
            "asset:knobas-docker-proxy",
            "asset:knobas-forgejo",
            "asset:knobas-gitea",
            "asset:knobas-https-front",
            "asset:knobas-jira",
            "asset:knobas-jira-db",
            CONTAINER,
            "asset:knobas-teamcity-agent",
            "asset:knobas-uptime-kuma",
            MACHINE,
            "asset:wsl-docker",
        ],
        "the VM and everything it holds, and nothing else"
    );

    for outside in ["asset:knobas-estate", "asset:desktop", NOTEBOOK] {
        assert!(
            !drawn.contains(&outside),
            "{outside} is not below the VM and must not be a member"
        );
    }
}

// ---------------------------------------------------------------------------
// Criterion 2, clause 5: a route to the container draws its wire
// ---------------------------------------------------------------------------

/// **TeamCity's tunnelled port has both ends, from both panes** (#440,
/// criterion 2; story 31).
///
/// A wire is drawn from a route's row in the pane to the row of the asset at
/// its far end, so what the store owes the view is a route that appears on
/// both panes with a target that resolves. `route:notebook-teamcity` is the
/// port the ssh tunnel to the WSL box forwards: exposed by the notebook,
/// landing on a container in the desktop's branch, four levels below the
/// site, which is the case a same-asset route could not witness -- the far end
/// is in a different column, and on a narrow window behind a spine. (Until
/// #634 this was the SSH tunnel's forward to a Hetzner server, on the same
/// port; until 2026-11-01, a port published on the notebook itself.)
///
/// The geometry itself is `app/src/lib/assets/tree.ts`'s (`wiresFor`), tested
/// there; the claim here is that the two ends the geometry needs are what the
/// real estate hands it.
#[tokio::test]
async fn teamcitys_published_port_has_both_ends_and_the_wire_can_be_drawn() {
    let pool = &estate().await;

    let notebook = assets::get(pool, NOTEBOOK).await.expect("the notebook");
    let exposed = notebook
        .exposes
        .iter()
        .find(|route| route.id == FORWARD)
        .expect("the notebook exposes TeamCity's tunnelled port");
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

    // The other pane: from the container, the same route is what reaches it.
    let container = assets::get(pool, CONTAINER).await.expect("the container");
    let reaching = container
        .reachable_via
        .iter()
        .find(|route| route.id == FORWARD)
        .expect("the container is reachable through its tunnelled port");
    assert_eq!(
        reaching.asset_id.as_str(),
        NOTEBOOK,
        "and from this end the wire runs back to the asset that exposes it"
    );

    // Every route in the file, so the claim is about the estate rather than
    // about one of its lines. One runs from somewhere other than the notebook:
    // the compose network's name for Gitea, exposed by the engine.
    let routes: Vec<(String, Option<String>)> =
        sqlx::query_as("select asset_id, target_id from knobas.route order by id")
            .fetch_all(pool)
            .await
            .expect("the routes");
    let target = |id: &str| Some(id.to_owned());
    assert_eq!(
        routes,
        vec![
            ("asset:wsl-docker".to_owned(), target("asset:knobas-gitea")),
            (NOTEBOOK.to_owned(), target("asset:knobas-confluence")),
            (NOTEBOOK.to_owned(), target("asset:knobas-gitea")),
            (NOTEBOOK.to_owned(), target("asset:knobas-jira")),
            (NOTEBOOK.to_owned(), target("asset:knobas-uptime-kuma")),
            (NOTEBOOK.to_owned(), target(CONTAINER)),
        ],
        "all six routes land on an asset, the compose network's included"
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
/// Every entry by **id**, not a count: a preview that reported twenty-three
/// known entries because it had listed one of them twenty-three times would pass
/// a count and would still be a dialog telling a reader nothing.
#[tokio::test]
async fn a_re_import_previews_every_entry_as_already_in_the_tree() {
    let pool = &estate().await;

    let preview = assets::preview_import(pool, ESTATE_FILE, ESTATE_FILE_PRODUCER)
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

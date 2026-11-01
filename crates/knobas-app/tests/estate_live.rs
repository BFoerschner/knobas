//! **The two importers against the real systems** (issues #509, #510 and #634,
//! `just estate-live`).
//!
//! One claim per importer:
//!
//! * **hcloud** -- the importer's own round trip over whatever the real Hetzner
//!   project holds: produce, land what is new, apply, produce again, and the
//!   second file previews *all known, nothing changed*. Spec #491 story 64's
//!   *"a re-import previews already in the tree"*, against the real API.
//! * **Docker** -- every running container on the WSL box's engine is
//!   already in the checked-in estate (`testenv/estate.json`), under the id it
//!   gives it, with nothing changed.
//!
//! # Why the hcloud half reads the project and not the estate file (#634)
//!
//! Until 2026-10-12 three Hetzner servers held the JVM products, they were in
//! the estate file under their `hcloud_id`s, and this half asserted that the
//! produced file previewed all-known against those three. Björn decommissioned
//! them (#634), so the estate file holds no server and the comparison has
//! nothing to compare. The deputy's ruling on #634 keeps the witness real
//! without keeping a server: the half reads **whatever the project holds** and
//! checks the importer against itself on that data. When servers are present
//! the third produce has to match each of them **by its origin key**, since the
//! applied asset carries the `hcloud_id` the API itself returned -- so the
//! decode, the property spellings and the match are all witnessed against what
//! Hetzner really sent. On an empty project the round trip shrinks to an empty
//! `Ready` file and an empty preview: still a real, authenticated request whose
//! envelope and pagination are read, and no longer a parse of a server.
//!
//! **The count is declared, never inferred.** `ESTATE_LIVE_HCLOUD_SERVERS` says
//! how many servers this run expects, and a different count is red. A run at
//! `0` therefore also proves that nothing is left billing in the project; a run
//! at `1` or more is the parse witness, and it is Björn's to make: a PR that
//! touches `assets::hcloud` owes one, for which he stands up the smallest server
//! in the knobas project, runs `ESTATE_LIVE_HCLOUD_SERVERS=1 just estate-live`
//! and deletes it again. No agent and no recipe creates or deletes a server.
//!
//! # What only this can say
//!
//! `tests/it/assets_ipc.rs` runs both producers against a **recording** and a
//! **stub executable**, which certify shape and nothing else. Two things they
//! structurally cannot see:
//!
//! 1. **That Hetzner still answers in that shape, and docker still writes one
//!    JSON object per line.** A recording is green forever; only a real request
//!    can go red when a field moves.
//! 2. **That the estate file is not behind the WSL box's engine.** A container
//!    started there and never written down previews as *new* here. Read a red
//!    Docker run first as `testenv/estate.json` being behind, and only then as a
//!    bug in the producer.
//!
//! # What it touches
//!
//! One `GET /v1/servers` per produce (three at most) over HTTPS with the token
//! in the repo-root `.env`, and one `docker --context wsl ps` -- all read-only
//! -- plus scratch databases of this run's own that the imports write into.
//! **No tunnel, no `hcloud` CLI.** The only docker context the estate file
//! names is `wsl`, an ssh context onto the WSL box (`ssh://knobas-wsl`, made by
//! `testenv/wsl/connect.sh`), so the one ssh login is docker's own, through
//! that context.
//!
//! # Running it
//!
//! `#[ignore]`d, so `just check` runs none of it. `just estate-live` is the
//! door; it loads the repo-root `.env` and **refuses to start** without
//! `HETZNER_API_TOKEN` or `ESTATE_LIVE_HCLOUD_SERVERS`, because a suite that
//! skipped by name on an unset variable is one libtest counts as a pass (issue
//! #351). No variable skips either half.

use knobas_app::assets::{self, DOCKER_PRODUCER, ESTATE_FILE_PRODUCER, HCLOUD_PRODUCER};

/// The estate, embedded -- the same bytes `tests/it/assets_ipc.rs`,
/// `tests/it/estate_exit.rs` and `sources::demo` read. Embedded rather than read
/// at run time, so this suite and the file move together.
const ESTATE_FILE: &str = include_str!("../../../testenv/estate.json");

/// The variable that declares how many servers the project holds.
const EXPECTED_SERVERS: &str = "ESTATE_LIVE_HCLOUD_SERVERS";

/// The token, gated rather than skipped on.
fn token() -> String {
    match std::env::var("HETZNER_API_TOKEN") {
        Ok(token) if !token.trim().is_empty() => token,
        _ => panic!(
            "HETZNER_API_TOKEN is unset. Run this through `just estate-live`, \
             which loads the repo-root `.env` and refuses a run with nothing to \
             run against."
        ),
    }
}

/// How many servers this run says the project holds -- required, and a whole
/// number.
fn expected_servers() -> usize {
    let raw = std::env::var(EXPECTED_SERVERS).unwrap_or_else(|_| {
        panic!(
            "{EXPECTED_SERVERS} is unset. Say how many servers the knobas Hetzner \
             project holds -- `{EXPECTED_SERVERS}=0 just estate-live` after the \
             teardown (#634) -- so that a count nobody expected is red rather \
             than read off the answer."
        )
    });
    raw.trim().parse().unwrap_or_else(|_| {
        panic!("{EXPECTED_SERVERS} is {raw:?}, and it has to be a whole number of servers")
    })
}

/// A client onto the real API -- **the command's own**, not one spelled like
/// it.
///
/// `assets::hcloud::client` is the single constructor and `assets::hcloud::API`
/// the single base URL, so what this suite drives is what
/// `produce_estate_file` drives: the same timeouts, the same rate limiter, the
/// same bearer scheme and the same `User-Agent` (#509, part 4c of the deputy's
/// ruling of 2026-09-08).
fn client() -> knobas_http::HttpClient {
    assets::hcloud::client(assets::hcloud::API, &token()).expect("a client onto Hetzner Cloud")
}

/// The number of entries in a produced estate file.
fn entries(file: &str) -> usize {
    let parsed: serde_json::Value = serde_json::from_str(file).expect("the produced file is JSON");
    parsed["assets"].as_array().expect("a list of assets").len()
}

/// **The real project round-trips through the hcloud importer: produced,
/// applied, produced again -- all known, nothing changed** (#634).
///
/// Against a scratch database holding the checked-in estate:
///
/// 1. produce from the real API. The number of servers it found -- named in
///    `LandingNeeded`, or the entries of a `Ready` file -- has to be the number
///    `ESTATE_LIVE_HCLOUD_SERVERS` declares;
/// 2. when any are new (the estate holds no server, so any server at all is),
///    produce again with a landing at the top of the estate and **apply** that
///    file, which is the reader's own import;
/// 3. produce a third time. It has to come back `Ready` with nothing new, and
///    its preview has to be all known with no changes: every server matched by
///    the `hcloud_id` the API returned in step 2, and every property the
///    producer writes read back with the spelling and the value it wrote.
///
/// On an empty project step 2 does not happen and step 3 is an empty `Ready`
/// file with an empty preview.
#[tokio::test]
#[ignore = "talks to the real Hetzner Cloud API; run through `just estate-live`"]
async fn the_real_project_round_trips_through_the_importer() {
    let expected = expected_servers();
    let pool = knobas_db::test_util::scratch_database("estate-live")
        .await
        .pool(2)
        .await
        .expect("a pool on the scratch database");
    assets::apply_import(&pool, ESTATE_FILE, ESTATE_FILE_PRODUCER)
        .await
        .expect("the checked-in estate imports");
    let client = client();

    // 1. The first read, and the count.
    let first = assets::hcloud::produce(&pool, &client, None)
        .await
        .expect("the hcloud importer runs against the real API");
    let found = match &first {
        assets::Produced::LandingNeeded { servers } => servers.len(),
        assets::Produced::Ready {
            file, new_servers, ..
        } => {
            assert!(
                new_servers.is_empty(),
                "a Ready answer to a run with no landing names nothing new: {new_servers:?}"
            );
            entries(file)
        }
        assets::Produced::TokenNeeded => {
            panic!("the producer asked for a token, and this suite always sends one")
        }
    };
    assert_eq!(
        found, expected,
        "the knobas Hetzner project holds {found} server(s) and \
         {EXPECTED_SERVERS} says {expected}. A server nobody expected is one \
         still billing; one missing is a witness server that was never stood \
         up. The answer was {first:?}"
    );

    // 2. Land what is new at the top of the estate, and apply it.
    if let assets::Produced::LandingNeeded { servers } = &first {
        let landed =
            assets::hcloud::produce(&pool, &client, Some(&assets::Landing { parent: None }))
                .await
                .expect("the second read, with a landing");
        let assets::Produced::Ready {
            file, new_servers, ..
        } = landed
        else {
            panic!("a run with a landing owes nothing more: {landed:?}");
        };
        assert_eq!(
            &new_servers, servers,
            "the landed run names the servers the first run asked about"
        );
        let applied = assets::apply_import(&pool, &file, HCLOUD_PRODUCER)
            .await
            .expect("the produced file applies");
        assert_eq!(
            applied.value.assets_created,
            i64::try_from(expected).expect("a count of servers"),
            "each server is created once, at the top of the estate"
        );
    }

    // 3. Produce again: nothing new, nothing changed.
    let again = assets::hcloud::produce(&pool, &client, None)
        .await
        .expect("the third read");
    let assets::Produced::Ready {
        file,
        new_servers,
        skipped,
    } = again
    else {
        panic!(
            "every server the project holds is in the tree now, so nothing is \
             owed; a landing asked for again is a server whose origin key did \
             not match what the import wrote: {again:?}"
        );
    };
    assert!(
        new_servers.is_empty() && skipped.is_empty(),
        "new {new_servers:?}, skipped {skipped:?}"
    );
    assert_eq!(
        entries(&file),
        expected,
        "the same servers as the first read"
    );

    let preview = assets::preview_import(&pool, &file, HCLOUD_PRODUCER)
        .await
        .expect("the produced file previews");
    assert!(
        preview.new.is_empty(),
        "the preview calls these new after they were applied: {:?}",
        preview
            .new
            .iter()
            .map(|entry| (&entry.id, &entry.name))
            .collect::<Vec<_>>()
    );
    assert!(
        preview.changes.is_empty(),
        "the importer disagrees with what it wrote one read earlier: {:?}",
        preview
            .changes
            .iter()
            .map(|change| (
                change.id.clone(),
                change
                    .properties
                    .iter()
                    .map(|property| (property.key.clone(), property.to.clone()))
                    .collect::<Vec<_>>()
            ))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        preview.known.len(),
        expected,
        "every server previews as already in the tree"
    );
    println!(
        "estate-live: hcloud read {expected} server(s); produced, applied and \
         produced again: all known, no changes{}",
        if expected == 0 {
            " (an empty project: the request, the envelope and the pagination \
             are witnessed, a server's parse is not)"
        } else {
            ""
        }
    );
}

/// **The real containers are already in the tree: all known, nothing changed**
/// (#510).
///
/// The estate file is imported into a scratch database, the Docker importer is
/// run against the one engine the file names -- the WSL box's, through the
/// `wsl` context -- and
/// the file it produces is previewed. Every entry has to land in *already in
/// the tree*, under the id `estate.json` gives it and not under the producer's
/// own `asset:docker-<context>/<name>`, and no property may be listed as
/// changing.
///
/// # What only this can say
///
/// `tests/it/assets_ipc.rs` runs this producer against a **stub executable**,
/// which certifies the parse, the argument rule and the refusals and can never
/// go red. Three things it structurally cannot see:
///
/// 1. **That the docker CLI still writes one JSON object per line.**
/// 2. **That the `docker_context` in `testenv/estate.json` is a context this
///    machine has**, reaching the engine that file says it does.
/// 3. **That every running container on that engine is in that file**, under
///    the name it carries there. A container started and never written down
///    previews as *new* and this goes red.
///
/// # What it does not assert, and why
///
/// **Not an exact list of containers.** `docker ps` reads the *running* ones,
/// and which of them are running is a fact about the moment: TeamCity, Jira
/// and Confluence stay up on the WSL box but the memory budget may have
/// stopped one, and `knobas-forgejo` runs only during a Forgejo run. What is
/// asserted instead is that **the engine answered** -- its
/// context is in the produced file -- and that nothing produced is new or
/// changed.
#[tokio::test]
#[ignore = "spawns the real docker CLI against the WSL box's engine; run through `just estate-live`"]
async fn the_real_containers_are_already_in_the_tree() {
    let pool = knobas_db::test_util::scratch_database("estate-live-docker")
        .await
        .pool(2)
        .await
        .expect("a pool on the scratch database");
    assets::apply_import(&pool, ESTATE_FILE, ESTATE_FILE_PRODUCER)
        .await
        .expect("the checked-in estate imports");

    // The command's own constructor, at the program the command runs
    // (`assets::docker::PROGRAM`), so this suite drives the spawn
    // `produce_estate_file` drives rather than a second one spelled the same
    // way -- #509's part 4c, applied to the other importer.
    let produced = assets::docker::produce(&pool, &assets::docker::cli(assets::docker::PROGRAM))
        .await
        .expect("the docker importer runs against the WSL box's engine");
    let assets::Produced::Ready {
        file,
        new_servers,
        skipped,
    } = produced
    else {
        panic!(
            "the producer asked for something. The Docker importer has no \
             credential and no landing question, so neither state is \
             reachable: {produced:?}"
        );
    };
    assert!(
        skipped.is_empty(),
        "these container engines in `testenv/estate.json` carry no \
         `docker_context`, so their containers were not read: {skipped:?}"
    );
    assert!(
        new_servers.is_empty(),
        "these containers are running and are not in `testenv/estate.json` \
         under their context and name: {new_servers:?}. Read this first as that \
         file being behind the real engine, and only then as a bug in the \
         producer."
    );

    // The engine answered. Read off the produced file's own properties, because
    // *which contexts were read* is what a shrunken fixture would silently
    // change.
    let parsed: serde_json::Value = serde_json::from_str(&file).expect("the produced file is JSON");
    let mut contexts: Vec<String> = parsed["assets"]
        .as_array()
        .expect("a list of assets")
        .iter()
        .map(|entry| {
            entry["properties"]["docker_context"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect();
    contexts.sort();
    contexts.dedup();
    assert_eq!(
        contexts,
        ["wsl"],
        "the one engine `testenv/estate.json` names, which answered with at \
         least one running container"
    );

    let preview = assets::preview_import(&pool, &file, DOCKER_PRODUCER)
        .await
        .expect("the produced file previews");
    assert!(
        preview.new.is_empty(),
        "the preview calls these new: {:?}",
        preview
            .new
            .iter()
            .map(|entry| (&entry.id, &entry.name))
            .collect::<Vec<_>>()
    );
    assert!(
        preview.changes.is_empty(),
        "the real engine and `testenv/estate.json` disagree: {:?}",
        preview
            .changes
            .iter()
            .map(|change| (
                change.id.clone(),
                change
                    .properties
                    .iter()
                    .map(|property| (property.key.clone(), property.to.clone()))
                    .collect::<Vec<_>>()
            ))
            .collect::<Vec<_>>()
    );
    let mut known: Vec<&str> = preview
        .known
        .iter()
        .map(|entry| entry.id.as_str())
        .collect();
    known.sort_unstable();
    println!(
        "estate-live: {} containers produced on the WSL box's engine, all \
         known, no changes: {known:?}",
        preview.known.len()
    );
}

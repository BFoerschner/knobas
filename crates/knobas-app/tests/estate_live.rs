//! **The hcloud importer against the real Hetzner Cloud** (issue #509,
//! `just estate-live`).
//!
//! One claim, and it is the ticket's third criterion in as many words: *"`just
//! estate-live` (new): produces the file from the real hcloud and the preview
//! against the checked-in estate is all-known with no changes."*
//!
//! # What only this can say
//!
//! `tests/assets_ipc.rs` runs this producer against a **recording** of what the
//! API answered once, which certifies shape and nothing else. Two things it
//! structurally cannot see, and this suite is the only witness for both:
//!
//! 1. **That Hetzner still answers in that shape.** A recording is green
//!    forever; only a real request can go red when a field moves.
//! 2. **That the three `hcloud_id` values in `testenv/hetzner/estate.json` are
//!    the ids of the three real servers.** #508 wrote them from `hcloud server
//!    list` and no gate can check them: a *missing* or *duplicated* id is red
//!    on a mutant, but a plausible-but-wrong one is a second copy of a server,
//!    silently. Here a wrong id is unknown by id and unmatched by key, so that
//!    server previews as **new** and [`the_real_estate_is_already_in_the_tree`]
//!    goes red.
//!
//! So, for whoever meets a red run: **read it first as a wrong value in
//! `estate.json`, and only then as a bug in the producer.** That reading is
//! the orchestrator's, posted on #509 under the deputy's ruling of 2026-09-08
//! on #508, and it is the reason this file exists rather than a `#[ignore]`d
//! test buried in a sibling suite.
//!
//! # What it touches
//!
//! Nothing. One `GET /v1/servers`, read-only, against a shared fixture --
//! `testenv/README.md`'s rule -- and a scratch database of this run's own that
//! the import writes into. **No tunnel is needed**: hcloud's API is public and
//! the products behind `testenv/hetzner/tunnel` are not consulted.
//!
//! # Running it
//!
//! `#[ignore]`d, so `just check` runs none of it. `just estate-live` is the
//! door; it loads the repo-root `.env` and **refuses to start** without
//! `HETZNER_API_TOKEN`, because a suite that skipped by name on an unset
//! variable is one libtest counts as a pass (issue #351).

use knobas_app::assets::{self, ESTATE_FILE_PRODUCER, HCLOUD_PRODUCER};

/// The estate as provisioned, embedded -- the same bytes `tests/assets_ipc.rs`,
/// `tests/estate_exit.rs` and `sources::demo` read. Embedded rather than read
/// at run time, so this suite and the file move together.
const ESTATE_FILE: &str = include_str!("../../../testenv/hetzner/estate.json");

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

/// A client onto the real API -- **the command's own**, not one spelled like
/// it.
///
/// `assets::hcloud::client` is the single constructor and `assets::hcloud::API`
/// the single base URL, so what this suite drives is what
/// `produce_estate_file` drives: the same timeouts, the same rate limiter, the
/// same bearer scheme and the same `User-Agent`. It was two spellings until the
/// deputy's ruling of 2026-09-08 on #509 (part 4c), and a comment here claiming
/// otherwise was the whole of what held them together.
fn client() -> knobas_http::HttpClient {
    assets::hcloud::client(assets::hcloud::API, &token()).expect("a client onto Hetzner Cloud")
}

/// **The real estate is already in the tree: all known, nothing changed.**
///
/// The estate file is imported into a scratch database, the producer is run
/// against the real hcloud, and the file it produces is previewed. Every entry
/// has to land in *already in the tree* -- under the id `estate.json` gives it
/// and not under the producer's own `asset:hcloud-<id>` -- and no property may
/// be listed as changing.
///
/// The three assertions are three different failures and each names its own:
///
/// * a server in **new** is an `hcloud_id` in `estate.json` that is not the id
///   of the server it sits on (or a server nobody has recorded yet);
/// * a property in **changes** is the producer and the estate disagreeing about
///   a *spelling* or a *value* -- `os` against `image`, text against number, a
///   server resized in Hetzner and not written down;
/// * a count that is not three is a server added to or removed from the project
///   without `estate.json` following.
#[tokio::test]
#[ignore = "talks to the real Hetzner Cloud API; run through `just estate-live`"]
async fn the_real_estate_is_already_in_the_tree() {
    let pool = knobas_db::test_util::scratch_database("estate-live")
        .await
        .pool(2)
        .await
        .expect("a pool on the scratch database");
    assets::apply_import(&pool, ESTATE_FILE, ESTATE_FILE_PRODUCER)
        .await
        .expect("the checked-in estate imports");

    let produced = assets::hcloud::produce(&pool, &client(), None)
        .await
        .expect("the hcloud importer runs against the real API");
    let assets::hcloud::Produced::Ready { file, new_servers } = produced else {
        panic!(
            "the producer asked for something. A token is owed only when none \
             is stored, and this suite always sends one; a landing place is \
             owed only when a server is not in the tree, which is the failure \
             the next assertion names: {produced:?}"
        );
    };
    assert!(
        new_servers.is_empty(),
        "these servers are not in `testenv/hetzner/estate.json` under their \
         `hcloud_id`: {new_servers:?}. Read this first as a wrong `hcloud_id` \
         in that file -- a plausible-but-wrong id is unmatched by key and \
         previews as new -- and only then as a bug in the producer."
    );

    let preview = assets::preview_import(&pool, &file, HCLOUD_PRODUCER)
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
        "the real hcloud and `testenv/hetzner/estate.json` disagree: {:?}",
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
    assert_eq!(
        known,
        [
            "asset:hetzner-confluence",
            "asset:hetzner-jira",
            "asset:hetzner-teamcity"
        ],
        "the three servers this project holds, each under the id the estate \
         gives it -- which is what says the origin key matched rather than the \
         file's own id being lucky"
    );
    println!(
        "estate-live: {} servers produced, all known, no changes",
        preview.known.len()
    );
}

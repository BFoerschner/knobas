//! The Uptime Kuma adapter against the **real** container in `testenv/`.
//!
//! Run by `just kuma-live`, never by `just check`: every test here is
//! `#[ignore]`d and that recipe is what un-ignores them. ADR-0013 -- the real
//! container is the witness, and a mock certifies nothing -- so the wiremock
//! recording in `tests/contract.rs` exists only to keep `just check`
//! docker-free, and **if the two disagree, the recording is what is wrong**.
//!
//! # What this certifies
//!
//! Every shape the adapter encodes about `/metrics`, against the server that
//! decides them: the four gauge families and their labels, `monitor_id` as the
//! identity, `"null"` for a field a monitor has not got, `-1` for a response
//! time that did not happen, `app_version` for *Test connection*, the `401` a
//! wrong API key gets -- and the one behaviour no recording can witness, that a
//! monitor really deleted in Kuma really leaves the mirror.
//!
//! # What it deliberately does not run, and why
//!
//! **The contract battery.** Its clause 2 requires that an incremental sync
//! *immediately* after a full one emits nothing and hands its cursor back --
//! and Kuma mutates itself on its own timers, writing a new response time for
//! every monitor on every heartbeat. Against a server that changes underneath
//! the two calls, that clause is a coin toss, and a live suite that fails one
//! run in fifty teaches everyone to re-run it. The property is real and is
//! asserted here, in the one honest form it takes against a moving corpus: a
//! poll that *finds nothing changed* emits nothing and keeps its cursor
//! ([`an_idle_poll_emits_nothing_and_keeps_its_cursor`]). The battery itself
//! runs in `tests/contract.rs`, against a recording of this server.
//!
//! **The certificate countdown.** `monitor_cert_days_remaining` has no series
//! on this estate: every seeded monitor is a plain HTTP check or a ping, and
//! Kuma publishes the family only for TLS. So the *present* direction of that
//! field is unwitnessed here, in either suite -- the recording has no such
//! series either, because it is a recording. What is witnessed is the absent
//! direction, which is the one this estate has: it reads as `null` and never as
//! a zero. Closing the gap needs a TLS monitor in `testenv/monitors.json`
//! pointed at something with a certificate, which is a decision about the
//! estate rather than about this adapter.
//!
//! **The whole roster's states.** The tunnel checks are red whenever
//! `hetzner/tunnel` is down, and that is a fact about the notebook rather than
//! about the adapter, so this suite asserts that every monitor carries *a*
//! state Kuma published and never that a particular monitor is up. The seed's
//! own README makes the same rule ("asserts presence, never a particular
//! value").
//!
//! # What it leaves behind
//!
//! Nothing. One test adds a scratch monitor through `testenv/kuma-monitor.sh`
//! and deletes it again on the way out, passing or panicking alike
//! ([`Scratch`]'s `Drop`). Anything a *killed* run leaves behind is removed by
//! the next `./seed-kuma.sh`, which deletes every monitor `monitors.json` does
//! not name -- and `just kuma-live` runs that before the suite. **No shared
//! container is stopped by anything here** (spec #427).

use std::process::Command;
use std::time::{Duration, Instant};

use knobas_source::contract::VecSink;
use knobas_source::instance::SourceInstance;
use knobas_source::{AuthMethod, Source, SourceError, SyncItem};

/// The monitor this suite owns, and the only thing in Kuma that is its own.
///
/// A name `testenv/monitors.json` deliberately does not carry, so the seed
/// sweeps it and nothing else watches it -- the same argument the canary's port
/// gets. It points at the canary's URL because that is a host socket this
/// environment already knows about; the monitor's *state* is never asserted, so
/// whether the canary happens to be bound does not matter.
const SCRATCH: &str = "knobas-live-scratch";
const SCRATCH_URL: &str = "http://host.docker.internal:8299/";

/// The eight monitors `testenv/monitors.json` names.
const SEEDED: [&str; 8] = [
    "knobas-teamcity",
    "knobas-jira",
    "knobas-confluence",
    "teamcity (tunnel)",
    "jira (tunnel)",
    "confluence (tunnel)",
    "gitea",
    "canary",
];

/// Where the seeded container is, and the key to read it with.
struct Env {
    url: String,
    key: String,
}

/// Read from the environment rather than hardcoded, so whatever names testenv
/// settles on work without a code change here; `testenv/seed --env-kuma` prints
/// exactly these, and `just kuma-live` refuses to start without them (#351).
fn env() -> Env {
    let need = |key: &str| {
        std::env::var(key).unwrap_or_else(|_| {
            panic!(
                "{key} is not set -- start testenv's Uptime Kuma and seed it first, then \
                 `eval \"$(cd testenv && ./seed --env-kuma)\"` (or just run `just kuma-live`)"
            )
        })
    };
    Env {
        url: need("KNOBAS_KUMA_URL").trim_end_matches('/').to_owned(),
        key: need("KNOBAS_KUMA_API_KEY"),
    }
}

impl Env {
    fn source(&self) -> Box<dyn Source> {
        self.source_with(&self.key)
    }

    fn source_with(&self, key: &str) -> Box<dyn Source> {
        knobas_source_kuma::build(SourceInstance {
            id: "kuma".to_owned(),
            kind: knobas_source_kuma::ADAPTER_KIND.to_owned(),
            display_name: "Uptime Kuma".to_owned(),
            base_url: self.url.clone(),
            auth: Some(AuthMethod::ApiToken),
            secret: Some(key.to_owned()),
            config: serde_json::json!({}),
        })
        .expect("the adapter builds against the seeded container")
    }
}

/// `testenv/`, from this crate's own manifest -- so the suite works whatever
/// directory cargo was invoked from.
fn testenv() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testenv")
}

/// One `./kuma-monitor.sh` invocation, which is how this suite writes to Kuma.
///
/// Shelling out rather than speaking the protocol: Kuma's configuration channel
/// is socket.io, and the environment already owns a node one-shot that speaks
/// it. A second implementation in Rust would be a second thing that can be
/// wrong about the same three event names.
fn kuma_monitor(args: &[&str]) {
    let output = Command::new("./kuma-monitor.sh")
        .args(args)
        .current_dir(testenv())
        .output()
        .expect("testenv/kuma-monitor.sh runs (is docker up?)");
    assert!(
        output.status.success(),
        "kuma-monitor.sh {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The scratch monitor, removed however the test ends.
///
/// The Gitea live suite's `Litter` for the same reason: a suite that creates
/// something in a shared environment and only removes it on the happy path
/// ratchets that environment up on every failure.
struct Scratch;

impl Scratch {
    fn add() -> Self {
        kuma_monitor(&["add", SCRATCH, SCRATCH_URL]);
        Self
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let output = Command::new("./kuma-monitor.sh")
            .args(["delete", SCRATCH])
            .current_dir(testenv())
            .output();
        // A panic in `drop` during a panic aborts the process and takes the
        // real failure's message with it, so this reports and does not assert.
        // The seed is the backstop: it deletes every monitor `monitors.json`
        // does not name, and `just kuma-live` runs it before the suite.
        match output {
            Ok(done) if done.status.success() => {}
            Ok(done) => eprintln!(
                "live_kuma: could not delete {SCRATCH}: {}",
                String::from_utf8_lossy(&done.stderr)
            ),
            Err(error) => eprintln!("live_kuma: could not delete {SCRATCH}: {error}"),
        }
    }
}

/// A full sync's items, by title.
async fn full_sync(source: &dyn Source) -> (Vec<SyncItem>, String) {
    let mut sink = VecSink(Vec::new());
    let cursor = source
        .sync(None, &mut sink)
        .await
        .expect("a full sync against the seeded Kuma");
    (sink.0, cursor)
}

/// Poll `/metrics` through the adapter until `wanted` says the corpus is what
/// the test needs, or give up.
///
/// A *state*, not a duration: Kuma checks on its own schedule and a monitor
/// just added has no series until its first heartbeat lands. Measured at ~2 s
/// on the pinned image; the ceiling here is generous because a loaded machine
/// is the normal case for this repository.
async fn until(
    source: &dyn Source,
    what: &str,
    wanted: impl Fn(&[SyncItem]) -> bool,
) -> Vec<SyncItem> {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let (items, _) = full_sync(source).await;
        if wanted(&items) {
            return items;
        }
        assert!(
            Instant::now() < deadline,
            "Kuma never reached the state this test needs: {what}"
        );
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

/// **The estate, mirrored.** Every monitor `testenv/monitors.json` names, with
/// the fields spec #427 asks a monitor to carry, read off the real server.
#[tokio::test]
#[ignore = "needs the seeded Uptime Kuma; run with `just kuma-live`"]
async fn a_full_sync_mirrors_every_seeded_monitor() {
    let env = env();
    let (items, cursor) = full_sync(env.source().as_ref()).await;

    let titles: Vec<&str> = items.iter().map(|i| i.title.as_str()).collect();
    for seeded in SEEDED {
        assert!(
            titles.contains(&seeded),
            "{seeded:?} is seeded but was not mirrored: {titles:?}"
        );
    }
    assert!(!cursor.is_empty(), "a run that emitted leaves a cursor");

    let mut pings = 0;
    let mut https = 0;
    for item in &items {
        assert_eq!(item.kind, knobas_source_kuma::KIND_MONITOR);
        assert_eq!(item.entity.namespace, "kuma");
        // Kuma's own monitor id, which is what the mirror keys on -- decimal,
        // and never the name.
        assert!(
            item.entity.key.parse::<u64>().is_ok(),
            "{} is not a Kuma monitor id",
            item.entity
        );
        assert_eq!(
            item.web_url.as_deref(),
            Some(format!("{}/dashboard/{}", env.url, item.entity.key).as_str()),
            "a monitor links to its own page in Kuma"
        );
        // Undated, and honestly so: `/metrics` carries no timestamp at all.
        assert_eq!(item.updated_at, None);
        assert_eq!(item.author, None);
        assert!(!item.deleted);

        let payload = &item.payload;
        assert_eq!(payload["id"], item.entity.key.as_str());
        assert_eq!(payload["name"], item.title.as_str());
        // A state Kuma published, never a particular one: the tunnel checks are
        // red whenever the tunnel is down, which is a fact about the notebook.
        let state = payload["state"]
            .as_str()
            .unwrap_or_else(|| panic!("{} carries no state: {payload}", item.title));
        assert!(
            ["up", "down", "pending", "maintenance"].contains(&state),
            "{} is in an unpublished state {state:?}",
            item.title
        );
        assert!(item.body_text.contains(&item.title));
        assert!(
            item.body_text.contains(state),
            "the launcher finds a monitor by name *with its state*: {:?}",
            item.body_text
        );
        // The sliding windows Kuma publishes, all three of them.
        for window in ["1d", "30d", "365d"] {
            assert!(
                payload["uptime"][window].is_number(),
                "{} has no {window} uptime ratio: {payload}",
                item.title
            );
        }
        // No monitor on this estate is a TLS check, so the certificate
        // countdown is absent everywhere -- an absence, never a zero. The
        // present direction is the gap this suite's header names.
        assert_eq!(
            payload["cert_days_remaining"],
            serde_json::Value::Null,
            "{} is not a TLS check and must not claim a certificate",
            item.title
        );

        match payload["type"].as_str() {
            Some("ping") => {
                pings += 1;
                // Kuma writes the literal string "null" for a label a monitor
                // of this type has not got, and the adapter reads it as the
                // absence it is.
                assert_eq!(payload["url"], serde_json::Value::Null);
                assert!(payload["hostname"].is_string(), "{payload}");
            }
            Some("http") => {
                https += 1;
                assert!(payload["url"].is_string(), "{payload}");
                assert_eq!(payload["hostname"], serde_json::Value::Null);
            }
            other => panic!("{} is of unexpected type {other:?}", item.title),
        }
    }
    assert_eq!(pings, 3, "one ping per Hetzner server");
    assert_eq!(https, 5, "three tunnel checks, Gitea and the canary");

    // The `-1` sentinel, where the estate happens to show it: a check that did
    // not answer has no response time rather than a negative one. Not asserted
    // as *present*, because it depends on the tunnel -- but wherever a monitor
    // is down, this is the shape the mirror must hold.
    for item in &items {
        if item.payload["state"] == "down" {
            assert_eq!(
                item.payload["response_time_ms"],
                serde_json::Value::Null,
                "{}: -1 is a sentinel, not a duration",
                item.title
            );
        }
    }
}

/// **A poll that finds nothing changed emits nothing and keeps its cursor** --
/// the property the engine reads as "nothing happened", and the reason a
/// one-minute schedule does not write 1,440 activity lines a day.
///
/// Retried rather than asserted once, and the retry is the honest part: Kuma
/// writes a new response time for every monitor on every heartbeat, so *some*
/// polls legitimately see a change. What must never happen is a poll that sees
/// no change and emits anyway. Five attempts against a corpus whose fastest
/// monitor beats every 20 s is many times more than one needs; exhausting them
/// means the cursor never settles, which is a real failure and reported as one.
#[tokio::test]
#[ignore = "needs the seeded Uptime Kuma; run with `just kuma-live`"]
async fn an_idle_poll_emits_nothing_and_keeps_its_cursor() {
    let env = env();
    let source = env.source();
    let (_, mut cursor) = full_sync(source.as_ref()).await;

    for attempt in 1..=5 {
        let mut sink = VecSink(Vec::new());
        let next = source
            .sync(Some(cursor.clone()), &mut sink)
            .await
            .expect("an incremental sync against the seeded Kuma");
        if sink.0.is_empty() {
            assert_eq!(
                next, cursor,
                "a run that emitted nothing must hand back the cursor it was given"
            );
            return;
        }
        assert_ne!(
            next,
            cursor,
            "attempt {attempt} emitted {} items, so the corpus moved and the cursor must too",
            sink.0.len()
        );
        cursor = next;
    }
    panic!(
        "five consecutive polls all saw a changed corpus -- either Kuma is beating faster than \
         this suite polls, or the digest is not stable across two reads of one unchanged document"
    );
}

/// **A monitor deleted in Kuma leaves the mirror on the next run** (spec #427,
/// story 54: *the roster never shows ghosts*).
///
/// The one behaviour no recording can witness, and the reason
/// `testenv/kuma-monitor.sh` exists: the monitor is really created and really
/// deleted, through the same socket.io channel the seed uses, and what the
/// adapter reports is a real absence rather than a body somebody edited.
///
/// The tombstone is the adapter's own, not the engine's sweep: the sweep fires
/// only on a cursor-less run and a scheduled poll always resumes from a stored
/// position, so this is the path a running installation actually takes.
#[tokio::test]
#[ignore = "needs the seeded Uptime Kuma; run with `just kuma-live`"]
async fn a_monitor_deleted_through_the_seeds_helper_is_swept() {
    let env = env();
    let source = env.source();

    let scratch = Scratch::add();
    let items = until(
        source.as_ref(),
        "the scratch monitor is published",
        |items| items.iter().any(|i| i.title == SCRATCH),
    )
    .await;
    let id = items
        .iter()
        .find(|i| i.title == SCRATCH)
        .expect("just waited for it")
        .entity
        .clone();
    // The cursor as of a corpus that holds it -- which is what the next run
    // compares against.
    let (_, cursor) = full_sync(source.as_ref()).await;

    drop(scratch);

    // The deletion is Kuma's to apply, and `/metrics` stops carrying the
    // monitor as soon as it has. Poll for the state, not for a duration.
    let deadline = Instant::now() + Duration::from_secs(60);
    let tombstones = loop {
        let mut sink = VecSink(Vec::new());
        source
            .sync(Some(cursor.clone()), &mut sink)
            .await
            .expect("an incremental sync after the deletion");
        let gone: Vec<SyncItem> = sink.0.into_iter().filter(|i| i.deleted).collect();
        if !gone.is_empty() {
            break gone;
        }
        assert!(
            Instant::now() < deadline,
            "the deleted monitor was never reported as gone"
        );
        tokio::time::sleep(Duration::from_secs(1)).await;
    };

    assert_eq!(tombstones.len(), 1, "{tombstones:?}");
    assert_eq!(tombstones[0].entity, id);
    assert_eq!(
        tombstones[0].title, SCRATCH,
        "a tombstone renders with the name the last run recorded"
    );
}

/// **The credential-health path, end to end.** A key Kuma refuses is
/// `Unauthorized` carrying the `401` it came from -- from `test_connection`,
/// which is what the sources view calls, and from `sync`, which is where a
/// rotated key actually surfaces.
#[tokio::test]
#[ignore = "needs the seeded Uptime Kuma; run with `just kuma-live`"]
async fn a_wrong_api_key_is_the_credential_health_path() {
    let env = env();
    // Well-formed and wrong: `uk<id>_<secret>` is the shape Kuma mints, so this
    // is a key that was rotated away rather than a string that could never have
    // been one.
    let source = env.source_with("uk1_this-key-was-rotated-away");

    let tested = source.test_connection().await;
    assert!(
        matches!(tested, Err(SourceError::Unauthorized { status: Some(401) })),
        "{tested:?}"
    );
    let synced = source.sync(None, &mut VecSink(Vec::new())).await;
    assert!(
        matches!(synced, Err(SourceError::Unauthorized { status: Some(401) })),
        "{synced:?}"
    );

    // And the working key still works, so the failure above is about the key
    // and not about the server having stopped answering.
    env.source()
        .test_connection()
        .await
        .expect("the seeded key still connects");
}

/// What *Test connection* puts on screen, off the real server: the Kuma that
/// answered and how much it is watching. No account -- an API key belongs to
/// the instance, and `/metrics` never says whose it is.
#[tokio::test]
#[ignore = "needs the seeded Uptime Kuma; run with `just kuma-live`"]
async fn test_connection_reports_the_real_kumas_version_and_roster() {
    let env = env();
    let info = env
        .source()
        .test_connection()
        .await
        .expect("the seeded Kuma answers");

    let version = info
        .server_version
        .expect("Kuma publishes app_version on /metrics");
    assert!(
        version.starts_with('2'),
        "the pinned image is Uptime Kuma v2, got {version:?}"
    );
    let detail = info.detail.expect("the roster size is worth one line");
    assert!(detail.ends_with("monitors"), "{detail:?}");
    assert_eq!(info.account, None);
    assert_eq!(info.secret_expires_at, None);
    assert!(info.discovered.is_empty());
}

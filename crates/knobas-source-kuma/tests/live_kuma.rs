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
use knobas_source::instance::{Account, SourceInstance};
use knobas_source::{AuthMethod, Capability, Source, SourceError, SyncItem, WriteOp};

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

/// Where the seeded container is, the key to read it with, and the account to
/// write with.
struct Env {
    url: String,
    key: String,
    /// The Kuma admin account (`knobas` / `knobas-dev`), which is what the
    /// write half needs: the API key opens `/metrics` and cannot log in to
    /// socket.io at all (issue #452).
    account: Account,
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
        // Demanded, not defaulted (issue #351's lesson applied to the write
        // half): an account this suite fell back to would let the pause tests
        // run against a Kuma whose admin is something else, fail on the login,
        // and read as a broken adapter.
        account: Account {
            username: need("KNOBAS_KUMA_USER"),
            password: need("KNOBAS_KUMA_PASSWORD"),
        },
    }
}

impl Env {
    fn source(&self) -> Box<dyn Source> {
        self.source_with(&self.key)
    }

    /// The same source **with the account**, which is the one that can write.
    fn writable(&self) -> Box<dyn Source> {
        self.with_account(Some(self.account.clone()))
    }

    fn with_account(&self, account: Option<Account>) -> Box<dyn Source> {
        knobas_source_kuma::build(SourceInstance {
            id: "kuma".to_owned(),
            kind: knobas_source_kuma::ADAPTER_KIND.to_owned(),
            display_name: "Uptime Kuma".to_owned(),
            base_url: self.url.clone(),
            auth: Some(AuthMethod::ApiToken),
            secret: Some(self.key.clone()),
            account,
            config: serde_json::json!({}),
        })
        .expect("the adapter builds against the seeded container")
    }

    fn source_with(&self, key: &str) -> Box<dyn Source> {
        knobas_source_kuma::build(SourceInstance {
            id: "kuma".to_owned(),
            kind: knobas_source_kuma::ADAPTER_KIND.to_owned(),
            display_name: "Uptime Kuma".to_owned(),
            base_url: self.url.clone(),
            auth: Some(AuthMethod::ApiToken),
            secret: Some(key.to_owned()),
            account: None,
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
    //
    // Its own loop rather than `until` above, and the difference is the sync it
    // makes: this one is **incremental**, because a tombstone only exists on a
    // run that has a previous corpus to compare against, and what it waits for
    // is a property of the emitted items rather than of the corpus. `until`
    // takes neither a cursor nor the sink, so widening it to carry both would
    // make one helper with two shapes for two callers.
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
    // The **count**, not just the noun. `detail.ends_with("monitors")` is
    // satisfied by `"0 monitors"`, so a key pointed at a Kuma watching nothing
    // -- or at the wrong Kuma -- would have been green here, which is the one
    // thing this line exists to tell apart (`roster_note`'s own doc).
    let counted: usize = detail
        .split_whitespace()
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or_else(|| panic!("the roster note leads with a count: {detail:?}"));
    assert!(
        counted >= SEEDED.len(),
        "the seeded estate is {} monitors and this run saw {counted}: {detail:?}",
        SEEDED.len()
    );
    assert!(detail.ends_with("monitors"), "{detail:?}");
    assert_eq!(info.account, None);
    assert_eq!(info.secret_expires_at, None);
    assert!(info.discovered.is_empty());
}

// -- The write half (issue #452) ---------------------------------------------
//
// Everything below needs the **account**, and needs the real container twice
// over: the socket.io channel exists nowhere else, and the fact the whole
// feature rests on -- that a paused monitor stops being published by
// `/metrics` -- is a property of the server rather than of anything in this
// repository. There is no recording that could witness either.

/// Poll **incrementally**, keeping the cursor the way the sync engine does,
/// until an emitted batch satisfies `wanted`.
///
/// The sibling of [`until`], and the difference is the whole of what it is
/// for: a tombstone is the difference between the stored position and the
/// current corpus, so only a run that carries a cursor can emit one. `cursor`
/// is advanced on every poll, exactly as a scheduled run advances the stored
/// one -- which also means the batch that satisfies `wanted` is the *only*
/// one that will ever carry it, and returning it is how the assertions below
/// get to look at it.
async fn until_from(
    source: &dyn Source,
    cursor: &mut String,
    what: &str,
    wanted: impl Fn(&[SyncItem]) -> bool,
) -> Vec<SyncItem> {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let mut sink = VecSink(Vec::new());
        *cursor = source
            .sync(Some(cursor.clone()), &mut sink)
            .await
            .expect("an incremental poll against the seeded Kuma");
        if wanted(&sink.0) {
            return sink.0;
        }
        assert!(
            Instant::now() < deadline,
            "Kuma never reached the state this test needs: {what}"
        );
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

/// The scratch monitor's Kuma id, out of the mirror.
///
/// The write op's target is an entity id and the entity id's key is Kuma's own
/// monitor id, so the mirror is where a test learns which number to pause --
/// the same route the Monitors tab takes to its button.
fn entity_of(items: &[SyncItem], title: &str) -> String {
    items
        .iter()
        .find(|item| item.title == title)
        .map(|item| item.entity.to_string())
        .unwrap_or_else(|| panic!("{title:?} is not in the mirror"))
}

/// **The declaration, against the real server.** The same key, the same Kuma,
/// two instances: only the one with an account offers anything to write.
///
/// Asserted here as well as in `tests/contract.rs` because the contract
/// battery's copy runs against a recording, and what this adds is that the
/// account really is optional at *build* time against a live instance -- a
/// source configured with one is not a source that fails to start.
#[tokio::test]
#[ignore = "needs the seeded Uptime Kuma; run with `just kuma-live`"]
async fn only_a_source_with_an_account_offers_the_write_ops() {
    let env = env();

    let read_only = env.source().descriptor();
    assert!(read_only.write_ops.is_empty(), "{:?}", read_only.write_ops);
    assert!(
        !read_only.capabilities.contains(&Capability::Write),
        "{:?}",
        read_only.capabilities
    );

    let writable = env.writable().descriptor();
    assert_eq!(
        writable.write_ops,
        ["pause_monitor", "resume_monitor", "create_monitor"]
    );
    assert!(writable.capabilities.contains(&Capability::Write));

    // And the read half is untouched by the account: the same roster either
    // way. A Kuma that read differently once it could write would be a change
    // nobody asked for.
    let (with, _) = full_sync(env.writable().as_ref()).await;
    let (without, _) = full_sync(env.source().as_ref()).await;
    let names = |items: &[SyncItem]| {
        let mut titles: Vec<String> = items.iter().map(|i| i.title.clone()).collect();
        titles.sort();
        titles
    };
    assert_eq!(names(&with), names(&without));
}

/// **Pause it, poll it, resume it** -- issue #452's second criterion, end to
/// end against the real Kuma.
///
/// The scratch monitor and nothing else: every monitor `monitors.json` names
/// watches part of the estate, and pausing one would silence a check something
/// else is reading (the sibling M4.1 exit run reads this same Kuma). `Scratch`
/// deletes it however this ends, so nothing is left paused even if an
/// assertion below fails mid-way -- a deleted monitor is not a silenced one.
///
/// **What "its health contributes none" means here, and why the poll is
/// incremental.** `/metrics` does not publish a paused monitor *at all* --
/// measured on the pinned image on 2026-09-07: the eight series a monitor has
/// are gone from the next scrape after `pauseMonitor` returns. So a paused
/// monitor contributes no state, no response time and no uptime ratio to
/// anything read off the roster, which is the whole of what silencing a check
/// should do.
///
/// The **tombstone** that carries that into the mirror is emitted only by a
/// poll that resumes from a stored cursor, because it is the *difference*
/// between two corpora (`crate::cursor`): a cursor-less run simply does not
/// mention a monitor that is not published, and there is nothing for it to
/// compare against. A scheduled run is incremental, so that is what this
/// drives -- `until_from` keeps the cursor the way the engine does. Asserting
/// this over full syncs is the trap this test fell into first: it waits sixty
/// seconds for a tombstone that a cursor-less sync can never emit.
#[tokio::test]
#[ignore = "needs the seeded Uptime Kuma; run with `just kuma-live`"]
async fn a_monitor_paused_through_the_write_op_leaves_the_roster_and_comes_back() {
    let env = env();
    let source = env.writable();
    let _scratch = Scratch::add();

    // It exists and is published, which is what makes the disappearance below
    // a fact about the pause rather than about a monitor that was never there.
    until(
        source.as_ref(),
        "the scratch monitor is published",
        |items| items.iter().any(|item| item.title == SCRATCH),
    )
    .await;
    // The position the next poll resumes from, taken while the monitor is
    // still there -- this is the stored cursor a scheduled run would hold.
    let (present, mut cursor) = full_sync(source.as_ref()).await;
    let entity = entity_of(&present, SCRATCH);
    let live = present
        .iter()
        .find(|item| item.title == SCRATCH)
        .expect("the scratch monitor");
    assert!(!live.deleted, "a running monitor is not a tombstone");
    assert!(
        live.payload.get("state").is_some_and(|s| !s.is_null()),
        "a published monitor carries a state: {:?}",
        live.payload
    );

    source
        .write(WriteOp::PauseMonitor {
            entity: entity.clone(),
        })
        .await
        .expect("the account pauses its own monitor");

    let paused = until_from(
        source.as_ref(),
        &mut cursor,
        "the paused monitor is tombstoned",
        |items| {
            items
                .iter()
                .any(|item| item.title == SCRATCH && item.deleted)
        },
    )
    .await;
    let gone = paused
        .iter()
        .find(|item| item.title == SCRATCH)
        .expect("the tombstone");
    assert_eq!(gone.entity.to_string(), entity, "the same monitor");
    assert!(
        gone.payload
            .get("state")
            .is_some_and(serde_json::Value::is_null),
        "a paused monitor contributes no state: {:?}",
        gone.payload
    );
    assert!(
        gone.payload
            .get("response_time_ms")
            .is_none_or(serde_json::Value::is_null),
        "a paused monitor contributes no reading: {:?}",
        gone.payload
    );

    // And the other reading of the same fact: a *fresh* look at the whole
    // roster does not mention it at all. This is what "its health contributes
    // none" means for anything that counts the corpus -- the chips, the
    // samples, the alerts.
    let (fresh, _) = full_sync(source.as_ref()).await;
    assert!(
        !fresh.iter().any(|item| item.title == SCRATCH),
        "a paused monitor is not published at all: {:?}",
        fresh.iter().map(|i| i.title.as_str()).collect::<Vec<_>>()
    );

    source
        .write(WriteOp::ResumeMonitor {
            entity: entity.clone(),
        })
        .await
        .expect("the account resumes its own monitor");

    let back = until_from(
        source.as_ref(),
        &mut cursor,
        "the resumed monitor is published again",
        |items| {
            items
                .iter()
                .any(|item| item.title == SCRATCH && !item.deleted)
        },
    )
    .await;
    let running = back
        .iter()
        .find(|item| item.title == SCRATCH)
        .expect("the resumed monitor");
    assert_eq!(running.entity.to_string(), entity);
    assert!(
        running.payload.get("state").is_some_and(|s| !s.is_null()),
        "a resumed monitor is published with a state again: {:?}",
        running.payload
    );
}

/// The two ways a write is refused, and the fault class each takes -- which is
/// what decides whether the queue waits for a human or records a refusal.
///
/// Both are read off the real server, because both are Kuma's own answers:
/// `authIncorrectCreds` for a wrong password, *You do not own this monitor* for
/// an id that is not there. Nothing is created and nothing is changed.
#[tokio::test]
#[ignore = "needs the seeded Uptime Kuma; run with `just kuma-live`"]
async fn a_refused_account_and_a_refused_write_are_different_faults() {
    let env = env();

    // A wrong password: `Unauthorized`, so the sources view offers *Re-enter*
    // and the queue keeps the write instead of losing it.
    let wrong = env.with_account(Some(Account {
        username: env.account.username.clone(),
        password: "not-the-password".to_owned(),
    }));
    let refused = wrong
        .write(WriteOp::PauseMonitor {
            entity: "kuma:1".to_owned(),
        })
        .await;
    assert!(
        matches!(refused, Err(SourceError::Unauthorized { status: None })),
        "a refused login is Unauthorized, got {refused:?}"
    );

    // A monitor that is not there: the login worked, so re-entering the
    // account would change nothing and this write is refused for good.
    let missing = env
        .writable()
        .write(WriteOp::PauseMonitor {
            // Far above anything this environment has ever created; Kuma
            // answers for an id it cannot find with the same words it uses for
            // one belonging to somebody else.
            entity: "kuma:999999".to_owned(),
        })
        .await;
    let message = match missing {
        Err(SourceError::Protocol { message, .. }) => message,
        other => panic!("expected Protocol, got {other:?}"),
    };
    assert!(message.contains("pauseMonitor"), "{message}");
}

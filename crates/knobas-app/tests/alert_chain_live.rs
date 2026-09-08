//! **M4.1's exit witness** (#450): the alert chain, end to end, against the
//! real Uptime Kuma.
//!
//! Spec #427 asks for exactly one test and says where it goes: *"The alert
//! chain: one app-level live test, the pattern of the seeded TeamCity digest
//! test, because the inbox is an app-crate read: the canary falls, a sync runs,
//! the alert is in the inbox because the fixture's context holds the asset, ack
//! clears the item and leaves the alert open, the canary recovers, the alert
//! closes. The same test's negative: an asset in no context whose canary falls
//! yields an alert in the Assets read and none in the inbox."*
//!
//! Every M4.1 ticket before this one witnessed its own layer over a fixture
//! written for the purpose: `knobas-sync`'s `tests/alerts.rs` over samples
//! inserted by hand, `knobas-core`'s `tests/inbox.rs` over an alert row
//! inserted by hand, `tests/inbox_ipc.rs` over both. Each of those is green
//! while the layer under it is wrong about the same monitor, because each one
//! *writes* what the layer under it was supposed to produce. This file writes
//! none of it: what makes the sample is a real poll of a real Kuma, what makes
//! the alert is the engine's reconcile inside that run's transaction, what
//! clears the item is the shipped ack, and what closes the alert is the next
//! poll seeing the canary answer again.
//!
//! # One test, one fall
//!
//! The chain is a sequence over one profile and one outage, so it is one
//! `#[tokio::test]` and not six. Two reasons, and the second is the load-bearing
//! one:
//!
//! * **A fall costs wall clock.** The canary's interval is 20 s and
//!   `testenv/README.md` records the measured legs (released, `monitor_status`
//!   read `0` after 14 s and 17 s; rebound, `1` after 17 s and 17 s). Six tests
//!   would be six falls.
//! * **libtest does not promise an order.** The steps are stateful — the ack
//!   only means something after the alert opened, the close only after the ack —
//!   so split across test functions the chain would be a sequence nothing
//!   sequences. `--test-threads=1` makes runs serial, not ordered.
//!
//! The negative therefore lands *inside* the chain, at the one moment it is
//! about: after the fall and before any context holds the canary's asset. That
//! is a stronger reading than a second profile would give, because it is
//! literally *"the same fall"* — one alert row, read twice, with one difference
//! between the readings.
//!
//! And the negative is made against an estate that **does** have a context in
//! it: `CONTEXT_ELSEWHERE` holds an asset far away from the canary before the
//! canary ever falls. A profile with no contexts at all would be answered by
//! any rule that happens to return nothing when there are none; this one is
//! answered only by a rule that walks membership.
//!
//! # What knocks the monitor down
//!
//! `testenv/canary.sh`, and nothing else. Its own header states the rule this
//! ticket inherits: *"every other thing Kuma watches here is shared … Stopping
//! any of those to make a red is how one stream's live run becomes another's
//! mystery failure."* So the estate carries one check whose entire blast radius
//! is a host port nobody else binds, this suite releases and rebinds that port,
//! and **no container is stopped by anything here** (M4 spec #427; #450's fourth
//! criterion). [`Canary`] rebinds it however the test ends, so a failure leaves
//! the environment green rather than ratcheting it down.
//!
//! # Waiting for Kuma, then syncing once
//!
//! Each leg waits for **Kuma** to publish the state, by polling `/metrics`
//! through the adapter, and only then runs **one** knobas sync. The two halves
//! are kept apart on purpose: a loop that synced until an alert appeared would
//! report "knobas never opened one" for an environment where the canary's port
//! was never even released, and the first thing a reader of a red live run
//! needs to know is which side of the wire failed. `until_state`'s panic names
//! the server; everything after it is knobas'.
//!
//! # Running it
//!
//! `#[ignore]`d, so `just check` runs none of it — that is what "skipped
//! without the Kuma URL and key" means here. The variables are *gated* rather
//! than *skipped on* ([`env`] panics with the commands to run): a suite that
//! skipped by name on an unset variable is one libtest counts as a pass, which
//! is issue #351 and the reason `just kuma-live` refuses to start without
//! `KNOBAS_KUMA_URL` and `KNOBAS_KUMA_API_KEY` at all.
//!
//! ```text
//! cd testenv && docker compose up -d --wait uptime-kuma && ./seed-kuma.sh
//! cd .. && just kuma-live
//! ```
//!
//! **One environment, one owner at a time** — `testenv/README.md`.

mod live_digest;

use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use knobas_app::assets::{self, AlertState, PropertyValue, ESTATE_FILE_PRODUCER};
use knobas_app::commands::entity::{inbox_count_inner, inbox_items_inner};
use knobas_app::sources::{Registry, SourcesState};
use knobas_core::entity::EntityRef;
use knobas_core::inbox::{Category, Shelf};
use knobas_secrets::{MemoryStore, Secret, SecretStore};
use knobas_source::contract::VecSink;
use knobas_source::instance::SourceInstance;
use knobas_source::{AuthMethod, Source};
use knobas_sync::scheduler::{Scheduler, SchedulerDeps};
use live_digest::{Connections, Quiet, sync};
use serde_json::json;
use sqlx::PgPool;

/// The configured source id, which is also the monitor entities' namespace.
const KUMA: &str = "kuma";

/// The monitor this suite knocks down, by the name `testenv/monitors.json`
/// gives it and `./seed-kuma.sh` guarantees is there.
const CANARY: &str = "canary";

/// The estate as provisioned — the same bytes `estate_exit.rs`, `share_exit.rs`
/// and the demo profile load.
///
/// The estate file is the **witness** here rather than the subject: what this
/// suite needs from it is a real tree to hang the canary's asset in and seven
/// real monitor names to resolve, and it asserts both rather than editing
/// either.
const ESTATE_FILE: &str = include_str!("../../../testenv/hetzner/estate.json");

/// Where the canary's asset hangs, and what it is called.
///
/// `asset:notebook` is the development notebook, which is the machine the
/// responder's socket is bound on (`testenv/canary.sh`: `127.0.0.1:8299`,
/// reached from the Kuma container through `host.docker.internal`). A `service`
/// under a `hypervisor` is not a pair `AssetType::suggests` records, and that is
/// not a rule: *"a type convention is an **ordering, never a filter** … and
/// `assets::create` accepts any type under any parent"* (`CONTEXT.md`, **Type
/// convention**).
///
/// **Made here rather than in `testenv/hetzner/estate.json`.** The estate file's
/// own README says of the seeded list that the canary *"watches nothing and so
/// appears in no asset"* (#441), and this suite does not overturn that: what
/// the file describes is the infrastructure the products run on, and the canary
/// is a responder that exists to be knocked over. So the file's seven names are
/// resolved against the real server here — which is #445's rule witnessed live,
/// and the thing an invented asset could not have witnessed — and the canary's
/// own asset and link are drawn through the two shipped paths a reader has for
/// it: `assets::create` and a `monitored-by` link (spec #427: *"A monitor
/// attaches to an asset by a `monitored-by` link, drawn by the estate import,
/// by *Link to…*, and by a droppable suggestion ticket"*).
const NOTEBOOK: &str = "asset:notebook";
const CANARY_ASSET: &str = "canary responder";

/// The context the canary's asset joins, and the context it does not.
///
/// Two, because the negative is *"an asset in no context"* and not "a profile
/// with no contexts": `CONTEXT_ELSEWHERE` holds `asset:db-teamcity`, three
/// levels down a different branch of the estate, so a context exists and holds
/// assets while the canary's is in none of them.
///
/// *Context* and not *room* throughout, which is `CONTEXT.md`'s line
/// (**Context**, *"Avoid: workspace, project, room (that is its view)"*):
/// nothing here opens one on screen.
const CONTEXT_ELSEWHERE: &str = "the teamcity database";
const CONTEXT_CANARY: &str = "the canary";
const ELSEWHERE_ASSET: &str = "asset:db-teamcity";

/// How long any one leg may wait for Kuma to publish the state it was given.
///
/// The canary polls every 20 s and `testenv/README.md` records the measured
/// legs at 14–17 s over two cycles each way. Two minutes is therefore about
/// seven intervals of headroom, which is the shape a live suite on a laptop
/// that may also be running a gate wants: generous enough that load is not a
/// red run, short enough that a canary nobody rebound fails rather than hangs.
const LEG_BUDGET: Duration = Duration::from_secs(120);

// -- the environment --------------------------------------------------------

struct Env {
    url: String,
    key: String,
}

/// Panics with the commands to run rather than skipping (#351): an unset
/// variable that skipped would be counted a pass by libtest, which is why
/// `just kuma-live` gates on these two before it compiles anything.
fn env() -> Env {
    let need = |key: &str| {
        std::env::var(key)
            .ok()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| {
                panic!(
                    "{key} is not set -- start testenv's Uptime Kuma and seed it first \
                     (`cd testenv && docker compose up -d --wait uptime-kuma && ./seed-kuma.sh`), \
                     then run `just kuma-live`"
                )
            })
    };
    Env {
        url: need("KNOBAS_KUMA_URL").trim_end_matches('/').to_owned(),
        key: need("KNOBAS_KUMA_API_KEY"),
    }
}

impl Env {
    /// The real adapter, built the way the registry builds it — used for the
    /// *waiting* half only. What the profile syncs through is the scheduler.
    fn source(&self) -> Box<dyn Source> {
        knobas_source_kuma::build(SourceInstance {
            id: KUMA.to_owned(),
            kind: knobas_source_kuma::ADAPTER_KIND.to_owned(),
            display_name: "Uptime Kuma".to_owned(),
            base_url: self.url.clone(),
            auth: Some(AuthMethod::ApiToken),
            secret: Some(self.key.clone()),
            // The alert chain is a *read* end to end: nothing here pauses a
            // monitor, so this source needs no account (#452) -- and one here
            // would give it write ops the exit witness has no use for.
            account: None,
            config: json!({}),
        })
        .expect("the adapter builds against the seeded container")
    }
}

/// `testenv/`, from this crate's own manifest, so the suite works whatever
/// directory cargo was invoked from.
fn testenv() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testenv")
}

/// One `./canary.sh` invocation: the only thing this suite knocks over.
fn canary_sh(command: &str) -> String {
    let output = Command::new("./canary.sh")
        .arg(command)
        .current_dir(testenv())
        .output()
        .expect("testenv/canary.sh runs");
    assert!(
        output.status.success(),
        "canary.sh {command} failed: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

/// The canary's port, rebound however the test ends.
///
/// `live_kuma.rs`' `Scratch` for the same reason, one direction further: a
/// suite that only restores the environment on the happy path ratchets that
/// environment down on every failure — and here the thing left behind would be
/// a monitor the whole estate reads as red.
struct Canary;

impl Drop for Canary {
    fn drop(&mut self) {
        // Reports rather than asserts: a panic in `drop` during a panic aborts
        // the process and takes the real failure's message with it.
        match Command::new("./canary.sh")
            .arg("up")
            .current_dir(testenv())
            .output()
        {
            Ok(done) if done.status.success() => {}
            Ok(done) => eprintln!(
                "alert_chain_live: could not rebind the canary: {}",
                String::from_utf8_lossy(&done.stderr)
            ),
            Err(error) => eprintln!("alert_chain_live: could not rebind the canary: {error}"),
        }
    }
}

/// Poll Kuma through the adapter until the canary reads `wanted`, and answer
/// with how long that took.
///
/// A *state*, not a duration: `testenv/README.md` records that where in the
/// 20 s interval a release falls is what varies, *"so a recipe waits for the
/// state and not for a duration"*.
///
/// The panic names **Kuma** deliberately. Everything this file asserts after a
/// wait is about knobas; a failure here is about the environment, and the two
/// are worth telling apart at three in the morning.
async fn until_state(source: &dyn Source, wanted: &str) -> Duration {
    let started = Instant::now();
    let deadline = started + LEG_BUDGET;
    loop {
        let mut sink = VecSink(Vec::new());
        let items = match source.sync(None, &mut sink).await {
            Ok(_) => sink.0,
            Err(error) => panic!("a full sync against the seeded Kuma: {error}"),
        };
        let canary = items
            .iter()
            .find(|item| item.title == CANARY)
            .unwrap_or_else(|| {
                panic!(
                    "the seeded Kuma holds no monitor called {CANARY:?} -- run \
                     `cd testenv && ./seed-kuma.sh`: {:?}",
                    items.iter().map(|i| &i.title).collect::<Vec<_>>()
                )
            });
        let state = canary.payload["state"].as_str().unwrap_or("");
        if state == wanted {
            return started.elapsed();
        }
        assert!(
            Instant::now() < deadline,
            "Kuma never published {wanted:?} for the canary within {LEG_BUDGET:?} \
             (it reads {state:?}). The port is 127.0.0.1:8299 and \
             `testenv/canary.sh status` says who holds it"
        );
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

/// The canary's monitor entity id, as the server names it.
///
/// Asked of Kuma rather than read out of the mirror this suite is about: the id
/// is the server's to assign, and taking it from the row under test would take
/// the identity under test from the thing under test.
async fn canary_entity(source: &dyn Source) -> String {
    let mut sink = VecSink(Vec::new());
    source
        .sync(None, &mut sink)
        .await
        .expect("a full sync against the seeded Kuma");
    sink.0
        .iter()
        .find(|item| item.title == CANARY)
        .map(|item| item.entity.to_string())
        .expect("the seeded Kuma holds the canary")
}

// -- the app, wired the way the app wires it --------------------------------

/// A `SourcesState` over a fresh profile with Uptime Kuma configured and its
/// API key in the (in-memory) keychain.
///
/// `teamcity_seeded_live.rs`' `app`, with Kuma's auth method: one profile, one
/// source, the real scheduler, and the scratch database's own connections.
///
/// **The fourth copy of it**, and `live_digest/mod.rs` now says so and why it
/// is still a copy: hoisting it edits four live suites at once, each certified
/// by a recipe of its own, so it belongs in a change that is about the move and
/// not in a milestone's exit witness.
async fn app(env: &Env) -> SourcesState {
    let connector = knobas_db::test_util::scratch_database("kuma_alert_chain").await;
    let pool = connector
        .pool(4)
        .await
        .expect("a pool onto the scratch database");

    let secrets = Arc::new(MemoryStore::new());
    secrets
        .put(KUMA, &Secret::just(AuthMethod::ApiToken, env.key.clone()))
        .expect("the Kuma API key is stored");

    knobas_sync::config::insert(
        &pool,
        &knobas_sync::config::InsertConfig {
            id: KUMA.to_owned(),
            adapter_kind: knobas_source_kuma::ADAPTER_KIND.to_owned(),
            display_name: "Uptime Kuma".to_owned(),
            base_url: env.url.clone(),
            auth_kind: knobas_sync::config::AuthKind::Method(AuthMethod::ApiToken),
            config: json!({}),
            sync_interval_secs: 86_400,
            enabled: true,
        },
    )
    .await
    .expect("the source row is written");

    let scheduler = Scheduler::start(SchedulerDeps {
        pool: pool.clone(),
        connections: Arc::new(Connections(connector)),
        registry: Arc::new(Registry::builtin()),
        secrets: secrets.clone(),
        events: Arc::new(Quiet),
        timing: knobas_sync::scheduler::SchedulerTiming::default(),
    })
    .await
    .expect("a scheduler over the scratch database");

    SourcesState {
        pool,
        scheduler,
        secrets,
        registry: Arc::new(Registry::builtin()),
    }
}

/// The newest sample of one monitor, as `(state, taken_at)` — the row the run
/// appended (#443).
async fn newest_sample(
    pool: &PgPool,
    monitor: &str,
) -> (Option<String>, chrono::DateTime<chrono::Utc>) {
    sqlx::query_as(
        "select state, taken_at from knobas.monitor_sample
          where entity_id = $1 order by taken_at desc, id desc limit 1",
    )
    .bind(monitor)
    .fetch_one(pool)
    .await
    .unwrap_or_else(|e| panic!("no sample of {monitor} after a run: {e}"))
}

/// One monitor's alert, as `(acked, closed)`, or `None` when it has never had
/// one. Read straight from the table, because *open* and *acked* are two
/// columns and the wire shape only carries the open ones.
async fn alert_state(pool: &PgPool, monitor: &str) -> Option<(bool, bool)> {
    sqlx::query_as::<
        _,
        (
            Option<chrono::DateTime<chrono::Utc>>,
            Option<chrono::DateTime<chrono::Utc>>,
        ),
    >(
        "select acked_at, closed_at from knobas.monitor_alert
          where entity_id = $1 order by opened_at desc, id desc limit 1",
    )
    .bind(monitor)
    .fetch_optional(pool)
    .await
    .expect("the alert reads")
    .map(|(acked, closed)| (acked.is_some(), closed.is_some()))
}

/// The verbs on one asset's history, oldest first, with who wrote them.
async fn history(pool: &PgPool, asset: &str) -> Vec<(String, String)> {
    sqlx::query_as::<_, (String, String)>(
        "select actor, verb from knobas.activity
          where entity_id = $1 order by at asc, id asc",
    )
    .bind(asset)
    .fetch_all(pool)
    .await
    .expect("the history reads")
}

/// The inbox stream's keys, as the reader sees them.
async fn stream_keys(state: &SourcesState, now: chrono::DateTime<chrono::Utc>) -> Vec<String> {
    inbox_items_inner(&state.pool, state.registry.as_ref(), now, Shelf::Stream)
        .await
        .expect("the inbox reads")
        .into_iter()
        .map(|entry| entry.item.key)
        .collect()
}

/// A context, and one asset in it — *Add to context*, which is an ordinary link
/// (`estate_exit.rs`' own words, and its own shape).
///
/// Answers with nothing: what every caller here needs is the membership, and a
/// returned id nobody reads is one more thing to be wrong about.
async fn context_holding(pool: &PgPool, title: &str, asset: &str) {
    let context = knobas_core::context::create_adhoc(pool, title)
        .await
        .expect("a context");
    knobas_core::link::create(
        pool,
        &EntityRef::parse(&context.id).expect("a ref"),
        &EntityRef::parse(asset).expect("a ref"),
        "related",
        knobas_core::link::Origin::Manual,
        None,
        "user",
    )
    .await
    .expect("Add to context is an ordinary link");
}

// -- the chain --------------------------------------------------------------

/// **The canary falls, the alert reaches the inbox, ack clears the item and
/// leaves the alert open, recovery closes it** — and, at the one moment it can
/// be asked, the negative: the same fall for an asset in no context is an open
/// alert in the Assets read and nothing in the inbox.
///
/// M4.1's exit criterion (#450), spec #427 stories 57–64, against the real
/// Uptime Kuma container and the real canary.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs testenv's seeded Uptime Kuma and the canary: `just kuma-live`"]
async fn the_canary_falls_the_alert_reaches_the_inbox_is_acked_and_recovery_closes_it() {
    let env = env();
    let source = env.source();

    // ---- the estate, before anything is wrong -----------------------------
    //
    // The canary is bound and Kuma says so. A fall nobody watched rise is not a
    // fall: without this the first sync could open an alert on a monitor that
    // was already down, and every assertion below would be about a different
    // sentence.
    let _canary = Canary;
    println!("canary: {}", canary_sh("up"));
    let rose = until_state(source.as_ref(), "up").await;
    println!("kuma reads the canary up after {rose:?}");
    let monitor = canary_entity(source.as_ref()).await;
    println!("the canary is {monitor}");

    let state = app(&env).await;
    let pool = &state.pool;

    // ---- the first poll ---------------------------------------------------
    sync(&state, KUMA).await;
    let (sampled, at) = newest_sample(pool, &monitor).await;
    assert_eq!(
        sampled.as_deref(),
        Some("up"),
        "the run sampled the canary at {at} as {sampled:?}, and Kuma had just published `up`"
    );
    assert_eq!(
        alert_state(pool, &monitor).await,
        None,
        "an up monitor opens nothing"
    );

    // ---- the estate imported ----------------------------------------------
    //
    // #445 against the **real** server rather than the recording: seven names
    // somebody typed into `estate.json` answering to seven monitors somebody
    // typed into Uptime Kuma. `adapter_to_mirror.rs` makes the same assertion
    // against `support/metrics.txt`, which is a recording of this server and
    // therefore agrees with it by construction until somebody edits one of the
    // two files.
    let imported = assets::apply_import(pool, ESTATE_FILE, ESTATE_FILE_PRODUCER)
        .await
        .expect("the estate file imports into a fresh profile")
        .value;
    assert_eq!(
        (
            imported.assets_created,
            imported.routes_created,
            imported.monitors_kept,
            imported.monitors_linked,
        ),
        (23, 9, 7, 7),
        "the real estate, with every one of its seven monitor names resolved against \
         the monitors the real Kuma published"
    );
    // The first three counts are already pinned by `estate_exit.rs` and
    // `share_exit.rs` against the same file, so an estate edit that moves them
    // reddens those two first and this line with them -- one more place, not a
    // new claim. What is new is the last one: `estate_exit.rs` asserts
    // `monitors_linked == 0`, because no adapter emitted a monitor when it was
    // written, and `adapter_to_mirror.rs` asserts seven against a *recording* of
    // this server -- which agrees with it by construction until somebody edits
    // one of the two files. Seven, here, is seven names typed into `estate.json`
    // answering to seven monitors typed into Uptime Kuma.

    // ---- the canary's asset, and a context somewhere else ------------------
    let canary_asset = assets::create(
        pool,
        Some(NOTEBOOK),
        "service",
        CANARY_ASSET,
        &[
            (
                "url".to_owned(),
                PropertyValue::Url {
                    value: "http://127.0.0.1:8299/".to_owned(),
                },
            ),
            ("port".to_owned(), PropertyValue::Number { value: 8299.0 }),
        ],
    )
    .await
    .expect("the canary's asset")
    .value
    .id;
    knobas_core::link::create(
        pool,
        &EntityRef::parse(&canary_asset).expect("a ref"),
        &EntityRef::parse(&monitor).expect("a ref"),
        assets::MONITORED_BY,
        knobas_core::link::Origin::Manual,
        None,
        "user",
    )
    .await
    .expect("*Link to…* draws a monitored-by link");
    context_holding(pool, CONTEXT_ELSEWHERE, ELSEWHERE_ASSET).await;

    // ---- the fall ---------------------------------------------------------
    println!("canary: {}", canary_sh("down"));
    let fell = until_state(source.as_ref(), "down").await;
    println!("kuma reads the canary down after {fell:?}");

    sync(&state, KUMA).await;
    let (sampled, at) = newest_sample(pool, &monitor).await;
    assert_eq!(
        sampled.as_deref(),
        Some("down"),
        "the run sampled the canary at {at} as {sampled:?}, and Kuma had published `down`"
    );
    assert_eq!(
        alert_state(pool, &monitor).await,
        Some((false, false)),
        "the crossing into down opened one alert, un-acked and open"
    );

    // The Assets read: every open alert, with what it watches.
    let open = assets::open_alerts(pool).await.expect("the Assets read");
    let alert = open
        .iter()
        .find(|alert| alert.monitor_id == monitor)
        .unwrap_or_else(|| panic!("the canary's alert is not in the Assets read: {open:?}"));
    assert_eq!(alert.state, AlertState::Down);
    assert_eq!(alert.monitor_name, CANARY);
    assert_eq!(alert.acked_at, None);
    assert_eq!(
        alert
            .assets
            .iter()
            .map(|asset| asset.id.as_str())
            .collect::<Vec<_>>(),
        vec![canary_asset.as_str()],
        "and it names the asset it is about"
    );

    // ---- THE NEGATIVE: an asset in no context ------------------------------
    //
    // A context exists (`CONTEXT_ELSEWHERE`) and holds assets; it does not hold
    // this one. Spec #427 story 60, and the routing rule's whole point: an
    // alert reaches the inbox by *what it is about*, and nobody is working on
    // this.
    let key = format!("{}:{monitor}", Category::Alert.as_str());
    let now = chrono::Utc::now();
    let before = stream_keys(&state, now).await;
    assert!(
        !before.contains(&key),
        "no context holds the canary's asset, so its alert is not a demand on anybody: {before:?}"
    );

    // ---- the same alert, once a context holds the asset --------------------
    //
    // The negative above and this read are the **same statement over the same
    // alert**, with membership as the only difference between them -- which is
    // what the assertion below says out loud: the stream gains this key and
    // nothing else. An absence on its own could be a reader that answers
    // nothing at all; a difference of exactly one cannot.
    context_holding(pool, CONTEXT_CANARY, &canary_asset).await;
    let now = chrono::Utc::now();
    let entries = inbox_items_inner(&state.pool, state.registry.as_ref(), now, Shelf::Stream)
        .await
        .expect("the inbox reads");
    let after: Vec<String> = entries.iter().map(|entry| entry.item.key.clone()).collect();
    let gained: Vec<&String> = after.iter().filter(|k| !before.contains(k)).collect();
    assert_eq!(
        gained,
        vec![&key],
        "putting the canary's asset in a context added exactly its alert to the stream \
         (before: {before:?}, after: {after:?})"
    );
    let item = entries
        .iter()
        .find(|entry| entry.item.key == key)
        .expect("the key the difference above just named");
    assert_eq!(item.item.category, Category::Alert);
    assert_eq!(item.item.source_id, KUMA);
    assert_eq!(item.item.title, CANARY, "the item is titled by the monitor");
    assert_eq!(
        item.item.entity_id.as_deref(),
        Some(canary_asset.as_str()),
        "opening it lands in the Tree at the affected asset, never at the monitor (story 61)"
    );
    assert_eq!(
        item.item.reason,
        format!("knobas test estate / devs-MacBook-Pro / {CANARY_ASSET} is down"),
        "the row is readable without opening it: which asset, and which trouble"
    );
    assert!(
        item.actions.is_empty(),
        "an alert asks a source for nothing: {:?}",
        item.actions
    );
    assert_eq!(
        inbox_count_inner(pool, state.registry.as_ref(), now)
            .await
            .expect("the count reads"),
        after.len() as i64,
        "the badge counts the same statement the stream draws -- and there is exactly \
         one thing in this profile to count: {after:?}"
    );
    assert_eq!(
        after.len(),
        1,
        "which is the canary's alert and nothing else"
    );

    // ---- ack: seen, not fixed ---------------------------------------------
    let now = chrono::Utc::now();
    let acked = assets::ack_alert(pool, &monitor, now)
        .await
        .expect("the ack")
        .value;
    assert!(acked.acked_at.is_some());
    assert_eq!(
        acked
            .assets
            .iter()
            .map(|asset| asset.id.as_str())
            .collect::<Vec<_>>(),
        vec![canary_asset.as_str()],
        "the ack answers with the assets its history line landed on"
    );
    assert_eq!(
        alert_state(pool, &monitor).await,
        Some((true, false)),
        "acked and still open -- only a return to up closes one"
    );
    let acked_stream = stream_keys(&state, now).await;
    assert!(
        !acked_stream.contains(&key),
        "and the reader's inbox is clear of it: {acked_stream:?}"
    );
    assert!(
        assets::open_alerts(pool)
            .await
            .expect("the Assets read")
            .iter()
            .any(|alert| alert.monitor_id == monitor && alert.acked_at.is_some()),
        "while the Assets view goes on saying this thing is down"
    );
    assert_eq!(
        history(pool, &canary_asset).await,
        vec![
            ("user".to_owned(), "created".to_owned()),
            ("user".to_owned(), "acked".to_owned()),
        ],
        "the ack left one line on the asset, after the line its creation left"
    );

    // ---- recovery ----------------------------------------------------------
    println!("canary: {}", canary_sh("up"));
    let recovered = until_state(source.as_ref(), "up").await;
    println!("kuma reads the canary up again after {recovered:?}");

    sync(&state, KUMA).await;
    let (sampled, at) = newest_sample(pool, &monitor).await;
    assert_eq!(
        sampled.as_deref(),
        Some("up"),
        "the run sampled the canary at {at} as {sampled:?}, and Kuma had published `up` again"
    );
    assert_eq!(
        alert_state(pool, &monitor).await,
        Some((true, true)),
        "the return to up closed it"
    );
    assert!(
        !assets::open_alerts(pool)
            .await
            .expect("the Assets read")
            .iter()
            .any(|alert| alert.monitor_id == monitor),
        "and the Assets view stops saying it"
    );
    let now = chrono::Utc::now();
    let at_the_end = stream_keys(&state, now).await;
    assert!(
        !at_the_end.contains(&key),
        "the item is gone by construction, not by anybody deleting one: {at_the_end:?}"
    );
    assert_eq!(
        history(pool, &canary_asset).await,
        vec![
            ("user".to_owned(), "created".to_owned()),
            ("user".to_owned(), "acked".to_owned()),
            (format!("sync:{KUMA}"), knobas_sync::alerts::VERB.to_owned()),
        ],
        "and the asset's history holds both lines: who saw it, and when it healed"
    );

    // ---- the timeseries the three runs left --------------------------------
    //
    // The roadmap's other M4.1 exit clause -- *"the timeseries holds the run's
    // samples"* -- and the one thing the per-leg assertions above cannot say
    // between them: each of those reads the **newest** row, so all three would
    // pass unchanged against an engine that overwrote one row per monitor
    // instead of appending one per poll.
    //
    // Three syncs ran, so the canary has three rows and they are its outage in
    // order. And the sentence is *"one sample per poll per **monitor**"*: every
    // other monitor Kuma published has three of its own, from the same three
    // runs, without this suite ever mentioning them.
    let series: Vec<Option<String>> = sqlx::query_scalar(
        "select state from knobas.monitor_sample
          where entity_id = $1 order by taken_at asc, id asc",
    )
    .bind(&monitor)
    .fetch_all(pool)
    .await
    .expect("the canary's series");
    assert_eq!(
        series
            .iter()
            .map(std::option::Option::as_deref)
            .collect::<Vec<_>>(),
        vec![Some("up"), Some("down"), Some("up")],
        "three runs, three samples, and they are the outage this test caused"
    );

    let per_monitor: Vec<i64> = sqlx::query_scalar(
        "select count(*) from knobas.monitor_sample group by entity_id order by 1",
    )
    .fetch_all(pool)
    .await
    .expect("the sample counts");
    let live: i64 = sqlx::query_scalar(
        "select count(*) from sync.live_item where source_id = $1 and kind = 'monitor'",
    )
    .bind(KUMA)
    .fetch_one(pool)
    .await
    .expect("the live roster");
    assert!(
        live > 1,
        "a roster of one could not witness a rule about every monitor"
    );
    assert_eq!(
        per_monitor,
        vec![3; usize::try_from(live).expect("a small roster")],
        "every one of the {live} live monitors was sampled once per run, three runs over"
    );

    state.scheduler.shutdown().await;
}

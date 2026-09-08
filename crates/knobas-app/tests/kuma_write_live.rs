//! **Through the queue** (#452's second criterion), against the real Uptime
//! Kuma.
//!
//! `knobas-source-kuma`'s `tests/live_kuma.rs` proves the *adapter* pauses a
//! monitor: it calls `Source::write` and watches `/metrics`. That is the SPI
//! seam, and it is one seam short of what the criterion asks for — "pause a
//! seeded monitor **through the queue**". Everything between the button and the
//! adapter is knobas' own: the op is decoded, its target parsed, its source
//! looked up, the instance's write ops read out of the keychain, a row written
//! to `knobas.write_queue`, a snapshot taken, the queue flushed, the write
//! settled, and the mirror re-read. None of that is exercised by an adapter
//! test, and all of it is what a reader pressing *Pause* actually runs.
//!
//! So this is the app-level half, over a real profile with a real scheduler
//! and a real Postgres, the shape `alert_chain_live.rs` and
//! `teamcity_seeded_live.rs` already have. Run by `just kuma-live`.
//!
//! # What it knocks over
//!
//! **Its own scratch monitor and nothing else.** Every monitor
//! `testenv/monitors.json` names watches part of the estate, the canary
//! included — `alert_chain_live.rs` in this same recipe is reading it — and
//! pausing one would silence a check something else is asserting on. So the
//! suite adds a monitor of its own through `testenv/kuma-monitor.sh`, the same
//! helper the adapter's live suite uses, and [`Scratch`]'s `Drop` deletes it
//! however the run ends. A deleted monitor is not a silenced one, so even a
//! panic mid-pause leaves the estate as it found it. `./seed-kuma.sh` is the
//! backstop, and `just kuma-live` runs it before this.
//!
//! # One test, two writes
//!
//! Pause and resume are one sequence over one monitor — the resume only means
//! anything after the pause landed — and libtest promises no order between test
//! functions. `--test-threads=1` makes runs serial, not ordered
//! (`alert_chain_live.rs` records the same reasoning).

mod live_digest;

use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use knobas_app::assets::ESTATE_FILE_PRODUCER;
use knobas_app::sources::SourcesState;
use knobas_app::sources::{Registry, write_queue};
use knobas_core::write_queue::WriteState;
use knobas_secrets::{MemoryStore, Secret, SecretStore};
use knobas_source::AuthMethod;
use knobas_source::instance::Account;
use knobas_sync::scheduler::{Scheduler, SchedulerDeps};
use live_digest::{Connections, Quiet, sync};
use serde_json::json;
use sqlx::{PgPool, Row};

/// The configured source id, which is also the monitor entities' namespace.
const KUMA: &str = "kuma";

/// The monitor this suite owns, and the only thing in Kuma that is its own.
///
/// A name `testenv/monitors.json` deliberately does not carry, so the seed
/// sweeps it and nothing else watches it. It points at the canary's URL because
/// that is a host socket this environment already knows about; the monitor's
/// *state* is never asserted here, only whether Kuma is publishing it at all.
const SCRATCH: &str = "knobas-write-scratch";
const SCRATCH_URL: &str = "http://host.docker.internal:8299/";

/// How long a leg may wait for Kuma to publish what it was told.
///
/// A pause takes effect at once — measured on the pinned image, all eight of a
/// monitor's series are gone from the next scrape — so this is headroom for a
/// laptop that may also be running a gate rather than for Kuma's own schedule.
const LEG_BUDGET: Duration = Duration::from_secs(60);

struct Env {
    url: String,
    key: String,
    account: Account,
}

/// Panics with the commands to run rather than skipping (#351): an unset
/// variable that skipped would be counted a pass by libtest, which is why
/// `just kuma-live` gates on all four before it compiles anything.
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
        account: Account {
            username: need("KNOBAS_KUMA_USER"),
            password: need("KNOBAS_KUMA_PASSWORD"),
        },
    }
}

/// `testenv/`, from this crate's own manifest, so the suite works whatever
/// directory cargo was invoked from.
fn testenv() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testenv")
}

/// One `./kuma-monitor.sh` invocation, which is how this suite writes to Kuma
/// **outside** the path it is testing.
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

/// A monitor this suite owns, removed however the test ends.
///
/// **It carries the name**, because one test here does not add its monitor
/// through the seed's helper at all -- it adds it through the write queue,
/// which is the thing under test (issue #453). What that test still needs is
/// the removal, and a guard that only removed one fixed name would leave the
/// other monitor behind on every failing run.
struct Scratch(&'static str);

impl Scratch {
    fn add() -> Self {
        kuma_monitor(&["add", SCRATCH, SCRATCH_URL]);
        Self(SCRATCH)
    }

    /// A guard over a monitor this suite did not create through the seed:
    /// nothing is added, and the `Drop` removes whatever is there.
    fn removing(name: &'static str) -> Self {
        Self(name)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let scratch = self.0;
        // Reports rather than asserts: a panic in `drop` during a panic aborts
        // the process and takes the real failure's message with it.
        match Command::new("./kuma-monitor.sh")
            .args(["delete", scratch])
            .current_dir(testenv())
            .output()
        {
            Ok(done) if done.status.success() => {}
            Ok(done) => eprintln!(
                "kuma_write_live: could not delete {scratch}: {}",
                String::from_utf8_lossy(&done.stderr)
            ),
            Err(error) => eprintln!("kuma_write_live: could not delete {scratch}: {error}"),
        }
    }
}

/// A `SourcesState` over a fresh profile with Uptime Kuma configured, its API
/// key **and its account** in the (in-memory) keychain.
///
/// `alert_chain_live.rs`' `app` with one field more, and that field is the
/// whole subject: without the account this source declares no write ops and
/// `submit_write` refuses the pause by name.
async fn app(env: &Env) -> SourcesState {
    let connector = knobas_db::test_util::scratch_database("kuma_write").await;
    let pool = connector
        .pool(4)
        .await
        .expect("a pool onto the scratch database");

    let secrets = Arc::new(MemoryStore::new());
    secrets
        .put(
            &knobas_secrets::KeychainAccount::source(KUMA),
            &Secret {
                kind: AuthMethod::ApiToken,
                value: env.key.clone(),
                account: Some(env.account.clone()),
            },
        )
        .expect("the Kuma credential is stored");

    knobas_sync::config::insert(
        &pool,
        &knobas_sync::config::InsertConfig {
            id: KUMA.to_owned(),
            adapter_kind: knobas_source_kuma::ADAPTER_KIND.to_owned(),
            display_name: "Uptime Kuma".to_owned(),
            base_url: env.url.clone(),
            auth_kind: knobas_sync::config::AuthKind::Method(AuthMethod::ApiToken),
            config: json!({}),
            // A day, so nothing polls behind this test's back: every run here
            // is one it asked for.
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

/// The scratch monitor's row in the mirror: `(entity_id, tombstoned)`, or
/// `None` while it has never been mirrored.
///
/// The tombstone is `knobas.entity.deleted_at` and not a column of
/// `sync.item` -- *deleted at source* is a fact about the **entity**, which is
/// what `sync.live_item` joins on and what every reader in the app is really
/// asking. Read here as the raw join rather than through that view, because
/// the view also drops a disabled source's items and this test is about one
/// monitor rather than about its source.
async fn mirrored(pool: &PgPool) -> Option<(String, bool)> {
    sqlx::query(
        "select item.entity_id, entity.deleted_at is not null as tombstoned
           from sync.item item
           join knobas.entity entity on entity.id = item.entity_id
          where item.source_id = $1 and item.title = $2",
    )
    .bind(KUMA)
    .bind(SCRATCH)
    .fetch_optional(pool)
    .await
    .expect("the mirror is readable")
    .map(|row| {
        (
            row.get::<String, _>("entity_id"),
            row.get::<bool, _>("tombstoned"),
        )
    })
}

/// Sync until the mirror says what this leg needs, or give up naming the state
/// it was waiting for.
///
/// A *state*, not a duration: what varies is where in Kuma's own schedule the
/// scratch monitor's first heartbeat lands.
async fn until_mirrored(
    state: &SourcesState,
    what: &str,
    wanted: impl Fn(Option<&(String, bool)>) -> bool,
) -> Option<(String, bool)> {
    let deadline = Instant::now() + LEG_BUDGET;
    loop {
        sync(state, KUMA).await;
        let row = mirrored(&state.pool).await;
        if wanted(row.as_ref()) {
            return row;
        }
        assert!(
            Instant::now() < deadline,
            "the mirror never reached the state this test needs within {LEG_BUDGET:?}: {what} \
             (it holds {row:?})"
        );
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

/// **The criterion, end to end**: a monitor paused *through the write queue*,
/// gone from the next poll, and resumed back.
///
/// Every step is the shipped path. `write_queue::submit` is what the Monitors
/// tab's button calls: it decodes the op, parses the target, finds the source,
/// reads that **instance's** write ops out of the keychain, writes the queue
/// row, flushes it and re-reads the mirror. What this test adds beyond the
/// adapter's own live suite is all of that — and the two assertions no adapter
/// test can make: that the queue row settled `sent`, and that `sync.item`
/// itself carries the tombstone.
#[tokio::test]
#[ignore = "needs the seeded Uptime Kuma; run with `just kuma-live`"]
async fn a_pause_queued_through_the_write_queue_reaches_kuma_and_the_mirror() {
    let env = env();
    let state = app(&env).await;
    let _scratch = Scratch::add();

    // It exists and is mirrored live, which is what makes the tombstone below
    // a fact about the pause rather than about a monitor that was never there.
    let (entity, _) = until_mirrored(&state, "the scratch monitor is mirrored live", |row| {
        matches!(row, Some((_, false)))
    })
    .await
    .expect("the scratch monitor is in the mirror");

    let queued = write_queue::submit(
        &state,
        json!({ "PauseMonitor": { "entity": entity.clone() } }),
    )
    .await
    .expect("the queue takes a pause for a source with an account");
    assert_eq!(queued.op, "pause_monitor");
    assert_eq!(queued.source_id, KUMA);
    assert_eq!(queued.entity_id, entity);

    // Settled, not merely queued. `submit` flushes before it returns, so a row
    // still `pending` here means the write never left knobas -- which would
    // pass every assertion below by accident, since a monitor nobody paused
    // and a monitor knobas failed to pause look identical from the mirror.
    let settled = knobas_core::write_queue::get(&state.pool, queued.id)
        .await
        .expect("the queue row is readable")
        .expect("the queue row exists");
    assert_eq!(
        settled.state,
        WriteState::Sent,
        "the pause did not leave the queue: {:?}",
        settled.detail
    );

    let (_, tombstoned) = until_mirrored(&state, "the paused monitor is tombstoned", |row| {
        matches!(row, Some((_, true)))
    })
    .await
    .expect("the row is still there, as a tombstone");
    assert!(tombstoned);

    // And back. The same path, the other op -- and the mirror carries it live
    // again, which is the whole of "resume it back".
    let resumed = write_queue::submit(
        &state,
        json!({ "ResumeMonitor": { "entity": entity.clone() } }),
    )
    .await
    .expect("the queue takes a resume");
    assert_eq!(resumed.op, "resume_monitor");
    let settled = knobas_core::write_queue::get(&state.pool, resumed.id)
        .await
        .expect("the queue row is readable")
        .expect("the queue row exists");
    assert_eq!(settled.state, WriteState::Sent, "{:?}", settled.detail);

    until_mirrored(
        &state,
        "the resumed monitor is mirrored live again",
        |row| matches!(row, Some((_, false))),
    )
    .await
    .expect("the scratch monitor is live in the mirror again");

    state.scheduler.shutdown().await;
}

/// The negative, at the same seam: **a Kuma with only its API key refuses the
/// pause before anything is queued.**
///
/// `submit_write`'s refusal is the interface not lying (story 13): an op absent
/// from a source's `write_ops` is one nothing should have offered. Asserted
/// against the real server because the fact behind it is a real one -- the
/// instance is built, its descriptor read, and the account is what is missing.
#[tokio::test]
#[ignore = "needs the seeded Uptime Kuma; run with `just kuma-live`"]
async fn a_pause_on_a_source_with_only_its_api_key_is_refused_before_it_is_queued() {
    let env = env();
    let state = app(&env).await;
    // The same profile with the account taken back out -- one field, nothing
    // else, which is what makes the refusal about the account.
    state
        .secrets
        .put(
            &knobas_secrets::KeychainAccount::source(KUMA),
            &Secret::just(AuthMethod::ApiToken, env.key.clone()),
        )
        .expect("the API key alone is stored");

    let refused = write_queue::submit(&state, json!({ "PauseMonitor": { "entity": "kuma:1" } }))
        .await
        .expect_err("a Kuma with no account offers no pause");
    assert_eq!(refused.code, knobas_app::IpcErrorCode::Invalid);
    assert!(
        refused.message.contains("pause_monitor"),
        "{}",
        refused.message
    );

    let open = knobas_core::write_queue::open(&state.pool)
        .await
        .expect("the queue is readable");
    assert!(
        open.is_empty(),
        "a refused write must leave no row behind: {open:?}"
    );

    state.scheduler.shutdown().await;
}

/// The estate as provisioned, so the create below is *for the local Gitea
/// asset* the ticket names rather than for one this test invented.
///
/// `alert_chain_live.rs` reads the same bytes for the same reason: the file is
/// the estate the products run on, and `asset:knobas-gitea` is a container in
/// it with a route that reaches it (`route:notebook-gitea`).
const ESTATE_FILE: &str = include_str!("../../../testenv/hetzner/estate.json");
const GITEA_ASSET: &str = "asset:knobas-gitea";

/// The monitor this test makes, through the shipped path. A name
/// `testenv/monitors.json` does not carry, so the seed sweeps whatever a killed
/// run leaves.
///
/// Its **URL is not a constant**: it is the URL the pane's form would open on
/// for this asset, taken out of the estate the import just loaded. See
/// [`prefilled_url`].
const CREATED: &str = "knobas-write-created";

/// The URL *Create monitor for this asset* would open on, for this asset.
///
/// **The ticket says *"prefilled from the asset's route or hostname"*, and a
/// live test that passed a constant would witness the plumbing without
/// witnessing that.** So this is `app/src/lib/assets/create-monitor.ts`'
/// `urlFor` applied to the real estate: the first fetchable route among the
/// ones the asset exposes, then among the ones that reach it. The Gitea
/// container exposes none and is reached by `route:notebook-gitea`, which is
/// the branch a container behind a published port takes.
///
/// The rule is not re-implemented here beyond that first step -- the hostname
/// fallbacks have six unit tests of their own -- and the assertion below is
/// what makes this a reading of the file rather than a guess: it names the
/// route the estate really carries.
///
/// **Kuma cannot reach `127.0.0.1:3000` from inside its own container**, and
/// that is fine and deliberately unasserted: the criterion is that the monitor
/// is mirrored and attached, and what a monitor *reports* is a fact about the
/// network rather than about the create. A reader looking at the estate would
/// meet exactly this, which is the point of using the estate's own URL.
async fn prefilled_url(pool: &PgPool) -> String {
    let detail = knobas_app::assets::get(pool, GITEA_ASSET)
        .await
        .expect("the pane reads the asset");
    assert!(
        detail.exposes.is_empty(),
        "the Gitea container exposes routes now, so the prefill's first branch \
         applies and this test is reading the second: {:?}",
        detail.exposes
    );
    let route = detail
        .reachable_via
        .iter()
        .find(|route| route.url.starts_with("http://") || route.url.starts_with("https://"))
        .expect("the estate file gives the Gitea container an http route that reaches it");
    assert_eq!(
        route.id, "route:notebook-gitea",
        "the estate file's own route is what the form would open on"
    );
    route.url.clone()
}

/// `assets::get`'s *monitoring* list for one asset, as the pane draws it.
async fn attached(pool: &PgPool, asset_id: &str) -> Vec<(String, String)> {
    knobas_app::assets::get(pool, asset_id)
        .await
        .expect("the pane reads the asset")
        .monitoring
        .into_iter()
        .map(|watch| (watch.name, watch.entity_id))
        .collect()
}

/// **The criterion, end to end**: an HTTP monitor created for the local Gitea
/// asset, mirrored by the next poll, and listed in that asset's pane as
/// attached -- then deleted through the seed's helper so the seeded list is
/// restored.
///
/// Every step is the shipped path and the seams above the adapter are what this
/// adds. `assets::edit` with `AssetEdit::Monitors` is the first of the two
/// writes the pane's dialog makes; `write_queue::submit` is the second, and it
/// decodes the op, parses the target, finds the source, reads that *instance's*
/// write ops out of the keychain, writes the queue row and flushes it. Then a
/// real poll of the real Kuma mirrors the monitor, `knobas_sync::attach`
/// resolves the recorded name against it inside that run's transaction, and
/// `assets::get` -- the pane's own read -- is what says the asset is watched.
///
/// **The import runs first**, so the asset this attaches to is the estate
/// file's own `knobas-gitea` rather than a fixture: the ticket says *the local
/// Gitea asset*, and an asset invented here would witness the plumbing without
/// witnessing that it reaches the estate the model describes.
///
/// **The name is not one the estate file gives.** `estate.json` already names
/// `gitea` on that asset, which the import resolves at once -- so a create
/// under that name would be attached before this test wrote anything, and
/// every assertion below would pass with the create deleted.
#[tokio::test]
#[ignore = "needs the seeded Uptime Kuma; run with `just kuma-live`"]
async fn a_monitor_created_through_the_write_queue_is_mirrored_and_attached_to_its_asset() {
    let env = env();
    let state = app(&env).await;
    // Armed before anything is created, `Scratch`' rule: the guard has to
    // outlive every way this can fail.
    let _litter = Scratch::removing(CREATED);
    // And whatever a killed earlier run left under that name.
    kuma_monitor(&["delete", CREATED]);

    let imported = knobas_app::assets::apply_import(&state.pool, ESTATE_FILE, ESTATE_FILE_PRODUCER)
        .await
        .expect("the estate file imports into a fresh profile")
        .value;
    assert!(
        imported.assets_created > 0,
        "the estate file created nothing: {imported:?}"
    );
    let already = attached(&state.pool, GITEA_ASSET).await;
    assert!(
        already.iter().all(|(name, _)| name != CREATED),
        "the estate file already names this monitor, so the test would prove nothing: {already:?}"
    );

    // The pane's first write: the name, which is what attaches the monitor.
    knobas_app::assets::edit(
        &state.pool,
        GITEA_ASSET,
        &[knobas_app::assets::AssetEdit::Monitors {
            added: vec![CREATED.to_owned()],
        }],
    )
    .await
    .expect("the monitor's name is recorded on the asset");

    // And the second: the create, through the queue, at the URL the form would
    // have opened on for this asset.
    let url = prefilled_url(&state.pool).await;
    let queued = write_queue::submit(
        &state,
        json!({
            "CreateMonitor": {
                "entity": knobas_source::monitor_target(KUMA),
                "name": CREATED,
                "url": url,
            }
        }),
    )
    .await
    .expect("the queue takes a create for a source with an account");
    assert_eq!(queued.op, "create_monitor");
    assert_eq!(queued.source_id, KUMA);
    assert_eq!(queued.entity_id, format!("{KUMA}:monitors"));

    // Settled, not merely queued: a row still `pending` here would pass every
    // assertion below by accident only if Kuma had the monitor anyway, and
    // saying so now names the failure rather than leaving a timeout to.
    let settled = knobas_core::write_queue::get(&state.pool, queued.id)
        .await
        .expect("the queue row is readable")
        .expect("the queue row exists");
    assert_eq!(
        settled.state,
        WriteState::Sent,
        "the create did not leave the queue: {:?}",
        settled.detail
    );
    // The receipt, read off the row: `QueuedWrite` does not carry it (nothing
    // on the bridge needs it), and it is what a withdrawn create's disclosure
    // line would point at.
    let minted: String = sqlx::query_scalar(
        "select remote_id from knobas.write_queue where id = $1 and remote_id is not null",
    )
    .bind(queued.id)
    .fetch_one(&state.pool)
    .await
    .expect("Uptime Kuma named the monitor it minted");

    // The next poll mirrors it, and the resolution inside that run attaches it.
    let deadline = Instant::now() + LEG_BUDGET;
    let watching = loop {
        sync(&state, KUMA).await;
        let watching = attached(&state.pool, GITEA_ASSET).await;
        if watching.iter().any(|(name, _)| name == CREATED) {
            break watching;
        }
        assert!(
            Instant::now() < deadline,
            "the pane never listed the created monitor within {LEG_BUDGET:?}: {watching:?}"
        );
        tokio::time::sleep(Duration::from_secs(2)).await;
    };

    let (_, entity) = watching
        .iter()
        .find(|(name, _)| name == CREATED)
        .expect("the created monitor is attached");
    assert_eq!(
        entity,
        &format!("{KUMA}:{minted}"),
        "the pane must list the monitor the receipt named"
    );
    // The estate file's own `gitea` monitor is still attached: the resolution
    // adds, and a rule that assigned would have taken the import's link away.
    assert!(
        watching.iter().any(|(name, _)| name == "gitea"),
        "the import's own attachment was lost: {watching:?}"
    );

    state.scheduler.shutdown().await;
}

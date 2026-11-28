//! **After a branch moved, knobas looks for its code changes and its queued
//! builds for a while** (ADR-0064 rule 2, #2585): the repository is read
//! again at about 5 s, 15 s, 30 s, 60 s and 120 s, its code side (the
//! repository walked) and its builds side (its pipelines walked for their
//! newest runs) each stopping once it moved.
//!
//! Two trait-level fakes stand in for the products, with the real Gitea and
//! TeamCity templates' declarations so the mirror reads them as it reads
//! theirs:
//!
//! * **`forge`**, as Gitea: a repository is walked (`refresh_under`), and the
//!   walk emits only what changed since the forge last emitted it, as
//!   Gitea's walk from its position does.
//! * **`ci`**, as TeamCity: a build configuration is walked for its builds
//!   (#2564).
//!
//! Each source's runs pass a door of its own that the test holds shut once
//! the mirror is seeded, so a build or commit that shows was read by the
//! schedule, not by a run. The schedule keeps a clock the test moves by hand.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use knobas_app::sources::SourcesState;
use knobas_app::sources::look_again::{self, Clock};
use knobas_app::sources::write_queue;
use knobas_core::write_queue::WriteState;
use knobas_secrets::{MemoryStore, Secret, SecretStore};
use knobas_source::{
    AuthMethod, ConnectionInfo, Cursor, RefreshAnswer, Sink, Source, SourceDescriptor, SourceError,
    SyncItem, UnderAnswer, WriteOp, WriteReceipt,
};
use knobas_sync::scheduler::{
    AdapterRegistry, LookingAgain, RunConnections, SchedulerDeps, SchedulerTiming, SyncEvents,
};
use serde_json::{Value, json};
use tokio::sync::{Semaphore, watch};

const FORGE: &str = "forge";
const CI: &str = "ci";
const FULL_NAME: &str = "tide/pay";
const REPO: &str = "forge:repo:tide/pay";
const CONFIG: &str = "ci:buildType:Pay_Build";
const C1: &str = "1111111111111111111111111111111111111111";
const C2: &str = "2222222222222222222222222222222222222222";
const PATIENCE: Duration = Duration::from_secs(20);

fn branch_id(name: &str) -> String {
    format!("forge:branch:{FULL_NAME}/{name}")
}

/// One build of `Pay_Build` on `branch` at `sha`; `None` finished is queued.
#[derive(Clone, Debug)]
struct Build {
    id: i64,
    branch: String,
    sha: String,
    finished: bool,
}

/// What the two products hold.
#[derive(Clone, Debug)]
struct World {
    /// Each branch's tip, by name.
    branches: Vec<(String, String)>,
    commits: Vec<String>,
    builds: Vec<Build>,
}

impl Default for World {
    fn default() -> Self {
        Self {
            branches: vec![("main".to_owned(), C1.to_owned())],
            commits: vec![C1.to_owned()],
            builds: vec![Build {
                id: 1,
                branch: "main".to_owned(),
                sha: C1.to_owned(),
                finished: true,
            }],
        }
    }
}

/// What the test watches the fakes do.
#[derive(Default)]
struct Seen {
    walked: Mutex<Vec<String>>,
    forge_runs: AtomicUsize,
    ci_runs: AtomicUsize,
}

#[derive(Clone)]
struct Shared {
    world: Arc<Mutex<World>>,
    seen: Arc<Seen>,
    forge_door: Arc<Semaphore>,
    ci_door: Arc<Semaphore>,
    /// What the forge last emitted of each item: its walk emits only what
    /// changed since.
    emitted: Arc<Mutex<HashMap<String, Value>>>,
    /// Every `looking:again` told, in order.
    looking: Arc<Mutex<Vec<LookingAgain>>>,
}

fn item(entity: &str, kind: &str, title: &str, payload: Value) -> SyncItem {
    SyncItem {
        entity: knobas_core::entity::EntityRef::parse(entity).expect("an entity id"),
        kind: kind.to_owned(),
        title: title.to_owned(),
        body_text: String::new(),
        author: None,
        updated_at: None,
        payload,
        web_url: None,
        deleted: false,
    }
}

fn repository() -> Value {
    json!({ "full_name": FULL_NAME, "name": "pay" })
}

impl World {
    /// Everything the forge holds of `tide/pay`, as Gitea's items.
    fn forge_items(&self) -> Vec<SyncItem> {
        let mut items = vec![item(
            REPO,
            "repo",
            FULL_NAME,
            json!({ "full_name": FULL_NAME, "name": "pay", "default_branch": "main" }),
        )];
        for (name, tip) in &self.branches {
            items.push(item(
                &branch_id(name),
                "branch",
                name,
                json!({ "name": name, "commit": { "id": tip }, "repository": repository() }),
            ));
        }
        for sha in &self.commits {
            items.push(item(
                &format!("forge:commit:{FULL_NAME}/{sha}"),
                "commit",
                sha,
                json!({ "sha": sha, "repository": repository() }),
            ));
        }
        items
    }

    /// The build configuration and its builds, as TeamCity's items.
    fn ci_items(&self) -> Vec<SyncItem> {
        let mut items = vec![item(
            CONFIG,
            "build_config",
            "Pay Build",
            json!({ "id": "Pay_Build", "name": "Pay Build", "projectId": "Pay", "projectName": "Pay" }),
        )];
        for build in &self.builds {
            let mut payload = json!({
                "id": build.id, "buildTypeId": "Pay_Build", "branchName": build.branch,
                "revisions": { "revision": [{ "version": build.sha }] },
            });
            let state = if build.finished {
                json!({ "number": build.id.to_string(), "state": "finished", "status": "SUCCESS",
                        "startDate": "20261127T100500+0000", "finishDate": "20261127T100917+0000" })
            } else {
                json!({ "state": "queued" })
            };
            payload
                .as_object_mut()
                .unwrap()
                .extend(state.as_object().unwrap().clone());
            let mut build_item = item(
                &format!("ci:build:{}", build.id),
                "build",
                "Pay Build",
                payload,
            );
            // The newest build is the newest on its branch, as TeamCity's
            // queued date makes it.
            build_item.updated_at = chrono::DateTime::from_timestamp(1_800_000_000 + build.id, 0);
            items.push(build_item);
        }
        items
    }
}

struct Fake {
    id: &'static str,
    shared: Shared,
}

impl Fake {
    /// The forge's items that changed since it last emitted them.
    fn forge_changed(&self) -> Vec<SyncItem> {
        let items = self.shared.world.lock().unwrap().forge_items();
        let mut emitted = self.shared.emitted.lock().unwrap();
        items
            .into_iter()
            .filter(|each| {
                emitted
                    .insert(each.entity.to_string(), each.payload.clone())
                    .as_ref()
                    != Some(&each.payload)
            })
            .collect()
    }
}

#[async_trait]
impl Source for Fake {
    async fn view(
        &self,
        _entity: &str,
        _section: Option<&str>,
    ) -> Result<Option<knobas_source::view::SourceView>, SourceError> {
        Ok(None)
    }

    fn descriptor(&self) -> SourceDescriptor {
        descriptor(self.id)
    }

    async fn test_connection(&self) -> Result<ConnectionInfo, SourceError> {
        Ok(ConnectionInfo::default())
    }

    async fn sync(
        &self,
        _cursor: Option<Cursor>,
        sink: &mut (dyn Sink + Send),
    ) -> Result<Cursor, SourceError> {
        let (count, door) = if self.id == FORGE {
            (&self.shared.seen.forge_runs, &self.shared.forge_door)
        } else {
            (&self.shared.seen.ci_runs, &self.shared.ci_door)
        };
        count.fetch_add(1, Ordering::SeqCst);
        door.acquire()
            .await
            .expect("the door is never closed")
            .forget();
        let items = if self.id == FORGE {
            self.forge_changed()
        } else {
            self.shared.world.lock().unwrap().ci_items()
        };
        for each in items {
            sink.item(each).await?;
        }
        Ok("{}".to_owned())
    }

    async fn refresh(&self, entity: &str) -> Result<RefreshAnswer, SourceError> {
        // A build is read by id; a configuration, a repository and a branch
        // are walked instead, as TeamCity and Gitea answer.
        if !entity.starts_with("ci:build:") {
            return Ok(RefreshAnswer::NotOffered);
        }
        let items = self.shared.world.lock().unwrap().ci_items();
        Ok(items
            .into_iter()
            .find(|each| each.entity.to_string() == entity)
            .map_or(RefreshAnswer::NotOffered, RefreshAnswer::Item))
    }

    async fn refresh_under(
        &self,
        entity: &str,
        _cursor: Option<&str>,
    ) -> Result<UnderAnswer, SourceError> {
        self.shared
            .seen
            .walked
            .lock()
            .unwrap()
            .push(entity.to_owned());
        let items = match entity {
            REPO => self.forge_changed(),
            CONFIG => self.shared.world.lock().unwrap().ci_items().split_off(1),
            _ => return Ok(UnderAnswer::NotOffered),
        };
        Ok(UnderAnswer::Walked {
            items,
            cursor: None,
        })
    }

    async fn write(&self, op: WriteOp) -> Result<WriteReceipt, SourceError> {
        let mut world = self.shared.world.lock().unwrap();
        match op {
            WriteOp::CreateBranch { name, .. } => {
                let tip = world.branches[0].1.clone();
                world.branches.push((name, tip));
            }
            other => return Err(SourceError::protocol(format!("not here: {other:?}"))),
        }
        Ok(WriteReceipt::none())
    }

    async fn reachable_transitions(&self, entity: &str) -> Result<Vec<String>, SourceError> {
        Err(SourceError::protocol(format!("no workflow for {entity:?}")))
    }
}

/// The real templates' declarations, under the fakes' ids.
fn descriptor(id: &str) -> SourceDescriptor {
    let mut template = if id == FORGE {
        knobas_source_gitea::descriptor_template()
    } else {
        knobas_source_teamcity::descriptor_template()
    };
    template.id = id.to_owned();
    template
}

struct Registry(Shared);

impl AdapterRegistry for Registry {
    fn descriptors(&self) -> Vec<SourceDescriptor> {
        vec![descriptor(FORGE), descriptor(CI)]
    }

    fn build(
        &self,
        instance: knobas_source::instance::SourceInstance,
    ) -> Result<Box<dyn Source>, SourceError> {
        let id = if instance.id == FORGE { FORGE } else { CI };
        Ok(Box::new(Fake {
            id,
            shared: self.0.clone(),
        }))
    }
}

struct Connections(knobas_db::embedded::Connector);

#[async_trait]
impl RunConnections for Connections {
    async fn open(&self) -> Result<sqlx::PgConnection, sqlx::Error> {
        self.0.connect().await
    }
}

/// Hears every `looking:again`, and nothing else.
struct Heard(Arc<Mutex<Vec<LookingAgain>>>);

impl SyncEvents for Heard {
    fn sync_state(&self, _status: knobas_sync::SourceSyncStatus) {}
    fn source_health(&self, _health: knobas_sync::CredentialHealth) {}
    fn activity_new(&self, _row: knobas_core::activity::ActivityRow) {}
    fn looking_again(&self, looking: &LookingAgain) {
        self.0.lock().unwrap().push(looking.clone());
    }
}

/// A clock moved by hand.
struct HandClock(watch::Sender<Duration>);

impl Clock for HandClock {
    fn now(&self) -> Duration {
        *self.0.borrow()
    }
    fn at(&self, at: Duration) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        let mut now = self.0.subscribe();
        Box::pin(async move {
            let _ = now.wait_for(|now| *now >= at).await;
        })
    }
}

/// No tick for the whole test: only the seeding runs and the runs the test
/// starts happen.
const HELD_OFF: SchedulerTiming = SchedulerTiming {
    startup_delay: Duration::from_secs(3600),
    tick: Duration::from_secs(3600),
    shutdown_grace: knobas_sync::scheduler::SHUTDOWN_GRACE,
};

/// A scheduler started the way the app starts it, over both sources, on a
/// hand-moved clock, the mirror seeded by one run of each and both doors
/// shut behind those runs.
async fn fixture(name: &str) -> (SourcesState, Shared, Arc<HandClock>) {
    let connector = knobas_db::test_util::scratch_database(name).await;
    let pool = connector.pool(4).await.expect("a pool");
    let secrets = Arc::new(MemoryStore::new());
    for (id, kind) in [(FORGE, "gitea"), (CI, "teamcity")] {
        secrets
            .put(
                &knobas_secrets::KeychainAccount::source(id),
                &Secret::just(AuthMethod::Pat, "a-token"),
            )
            .expect("the credential is stored");
        knobas_sync::config::insert(
            &pool,
            &knobas_sync::config::InsertConfig {
                id: id.to_owned(),
                adapter_kind: kind.to_owned(),
                display_name: id.to_owned(),
                base_url: format!("https://{id}.example"),
                auth_kind: knobas_sync::config::AuthKind::Method(AuthMethod::Pat),
                config: json!({}),
                sync_interval_secs: 86_400,
                enabled: true,
                sync_by_hand: false,
            },
        )
        .await
        .expect("the source row is written");
    }
    let shared = Shared {
        world: Arc::default(),
        seen: Arc::default(),
        forge_door: Arc::new(Semaphore::new(1)),
        ci_door: Arc::new(Semaphore::new(1)),
        emitted: Arc::default(),
        looking: Arc::default(),
    };
    let registry: Arc<dyn AdapterRegistry> = Arc::new(Registry(shared.clone()));
    let scheduler = knobas_app::sources::start_scheduler(
        SchedulerDeps {
            pool: pool.clone(),
            connections: Arc::new(Connections(connector)),
            registry: Arc::clone(&registry),
            secrets: secrets.clone(),
            events: Arc::new(Heard(Arc::clone(&shared.looking))),
            timing: HELD_OFF,
            quit: knobas_sync::scheduler::Quit::default(),
        },
        knobas_sync::background::Background::new(),
    )
    .await
    .expect("a scheduler");
    let clock = Arc::new(HandClock(watch::channel(Duration::ZERO).0));
    look_again::use_clock(&scheduler, Arc::clone(&clock) as Arc<dyn Clock>);
    let state = SourcesState {
        live: Default::default(),
        pool,
        scheduler,
        secrets,
        registry,
    };
    for id in [FORGE, CI] {
        run_to_its_end(&state, id).await;
    }
    eventually("the forge's first run noted its branches", || async {
        look_again::noted(&state.scheduler, FORGE)
    })
    .await;
    (state, shared, clock)
}

/// Start a run of `id` that begins now, and answer its id. `trigger` joins a
/// run in flight, and a run's claim is still held for a moment after its row
/// reads ended, so a trigger right after one can answer that ended run: it is
/// asked again until it answers a newer one.
async fn new_run(state: &SourcesState, id: &str) -> i64 {
    let before: i64 =
        sqlx::query_scalar("select coalesce(max(id), 0) from knobas.sync_run where source_id = $1")
            .bind(id)
            .fetch_one(&state.pool)
            .await
            .unwrap();
    tokio::time::timeout(PATIENCE, async {
        loop {
            let run = state
                .scheduler
                .trigger(id, knobas_sync::SyncTrigger::Manual, None)
                .await
                .expect("a run starts");
            if run > before {
                return run;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("never: a new run of {id} starts"))
}

/// Start a run of `id` and wait for it to end `ok` (its door must let it
/// through).
async fn run_to_its_end(state: &SourcesState, id: &str) {
    let run = new_run(state, id).await;
    eventually(&format!("the run of {id} ends"), || async {
        sqlx::query_scalar::<_, bool>(
            "select finished_at is not null and outcome = 'ok' from knobas.sync_run where id = $1",
        )
        .bind(run)
        .fetch_one(&state.pool)
        .await
        .unwrap()
    })
    .await;
}

/// Wait until `check` answers true.
async fn eventually<F, Fut>(what: &str, check: F)
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    tokio::time::timeout(PATIENCE, async {
        while !check().await {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("never: {what}"));
}

/// What the schedule of the repository looks for now: `None` when none runs.
fn looking_at_repo(state: &SourcesState) -> Option<(bool, bool)> {
    look_again::looking(&state.scheduler)
        .into_iter()
        .find(|each| each.scope == REPO)
        .map(|each| (each.code, each.builds))
}

/// Whether the mirror holds `entity` live.
async fn mirrored(state: &SourcesState, entity: &str) -> Option<Value> {
    sqlx::query_scalar("select payload from sync.live_item where entity_id = $1")
        .bind(entity)
        .fetch_optional(&state.pool)
        .await
        .unwrap()
}

/// Send `op` through the queue and wait for it to land.
async fn landed(state: &SourcesState, op: Value) -> i64 {
    let id = write_queue::submit_in_background(state, op)
        .await
        .expect("queued")
        .id;
    eventually("the write lands", || async {
        knobas_core::write_queue::get(&state.pool, id)
            .await
            .unwrap()
            .is_some_and(|row| row.state == WriteState::Sent)
    })
    .await;
    id
}

fn queue_build(shared: &Shared, id: i64, branch: &str, sha: &str) {
    shared.world.lock().unwrap().builds.push(Build {
        id,
        branch: branch.to_owned(),
        sha: sha.to_owned(),
        finished: false,
    });
}

fn push(shared: &Shared, branch: &str, sha: &str) {
    let mut world = shared.world.lock().unwrap();
    world.commits.push(sha.to_owned());
    if let Some(each) = world.branches.iter_mut().find(|(name, _)| name == branch) {
        each.1 = sha.to_owned();
    }
}

/// **A branch made in knobas: its builds are looked for, then its code.**
/// The schedule starts as the write lands. CI queues the branch's build a
/// moment later: at 5 s the builds side walks the pipeline, finds it and
/// stops, while no CI run has happened. Then a bot pushes to the branch: at
/// 15 s the code side's walk brings the new tip, stops, and the schedule
/// ends. Each step is told on `looking:again`.
#[tokio::test(flavor = "multi_thread")]
async fn a_new_branch_is_looked_at_until_its_build_and_its_next_commit_show() {
    let (state, shared, clock) = fixture("look_again_write").await;
    landed(
        &state,
        json!({ "CreateBranch": { "entity": REPO, "name": "feature", "from_ref": "main" } }),
    )
    .await;
    eventually("the repository is looked at, both sides", || async {
        looking_at_repo(&state) == Some((true, true))
    })
    .await;
    assert!(
        mirrored(&state, &branch_id("feature")).await.is_some(),
        "read back at once"
    );

    queue_build(&shared, 2, "feature", C1);
    clock.0.send_replace(Duration::from_secs(4));
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(
        mirrored(&state, "ci:build:2").await.is_none(),
        "nothing is read before 5 s"
    );
    clock.0.send_replace(Duration::from_secs(5));
    eventually(
        "the queued build shows and the builds side stops",
        || async { looking_at_repo(&state) == Some((true, false)) },
    )
    .await;
    assert_eq!(
        mirrored(&state, "ci:build:2").await.unwrap()["state"],
        "queued"
    );
    assert_eq!(
        shared.seen.ci_runs.load(Ordering::SeqCst),
        1,
        "no CI run since the seeding"
    );

    // The run that follows the write ends, so the forge's lock is free.
    shared.forge_door.add_permits(1);
    eventually("the run after the write ends", || async {
        sqlx::query_scalar::<_, bool>(
            "select count(*) = 2 from knobas.sync_run where source_id = $1 and outcome = 'ok'",
        )
        .bind(FORGE)
        .fetch_one(&state.pool)
        .await
        .unwrap()
    })
    .await;
    push(&shared, "feature", C2);
    clock.0.send_replace(Duration::from_secs(15));
    eventually("the new tip shows and the schedule ends", || async {
        looking_at_repo(&state).is_none()
    })
    .await;
    assert_eq!(
        mirrored(&state, &branch_id("feature")).await.unwrap()["commit"]["id"],
        C2
    );
    assert_eq!(
        shared.seen.forge_runs.load(Ordering::SeqCst),
        2,
        "the seeding and the write's run"
    );
    let told: Vec<(bool, bool)> = shared
        .looking
        .lock()
        .unwrap()
        .iter()
        .filter(|each| each.scope == REPO)
        .map(|each| (each.code, each.builds))
        .collect();
    assert_eq!(told, [(true, true), (true, false), (false, false)]);
}

/// **A push knobas did not make**, found by a run of the forge: the run
/// that brings the moved tip starts the schedule, and at 5 s the build CI
/// queued for it shows without a CI run.
#[tokio::test(flavor = "multi_thread")]
async fn a_run_that_finds_a_branch_moved_starts_the_schedule() {
    let (state, shared, clock) = fixture("look_again_run").await;
    push(&shared, "main", C2);
    queue_build(&shared, 2, "main", C2);
    shared.forge_door.add_permits(1);
    run_to_its_end(&state, FORGE).await;
    eventually("the repository is looked at", || async {
        looking_at_repo(&state) == Some((true, true))
    })
    .await;
    clock.0.send_replace(Duration::from_secs(5));
    eventually("the queued build shows", || async {
        looking_at_repo(&state) == Some((true, false))
    })
    .await;
    assert_eq!(
        mirrored(&state, "ci:build:2").await.unwrap()["state"],
        "queued"
    );
    assert_eq!(
        shared.seen.ci_runs.load(Ordering::SeqCst),
        1,
        "no CI run since the seeding"
    );
}

/// A run of the forge that moved nothing starts nothing.
#[tokio::test(flavor = "multi_thread")]
async fn a_run_that_moves_no_branch_starts_nothing() {
    let (state, shared, _clock) = fixture("look_again_quiet_run").await;
    shared.forge_door.add_permits(1);
    run_to_its_end(&state, FORGE).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(look_again::looking(&state.scheduler).is_empty());
}

/// **A run of the CI holding its lock is not read past**: the step at 5 s
/// skips the pipeline's walk (the run reads it), and the step at 15 s,
/// once the run has ended, finds the build that run brought.
#[tokio::test(flavor = "multi_thread")]
async fn a_ci_run_in_flight_skips_the_walk_and_its_build_still_counts() {
    let (state, shared, clock) = fixture("look_again_lock").await;
    let run = new_run(&state, CI).await;
    eventually("the CI run waits at its door", || async {
        shared.seen.ci_runs.load(Ordering::SeqCst) == 2
    })
    .await;
    landed(
        &state,
        json!({ "CreateBranch": { "entity": REPO, "name": "feature", "from_ref": "main" } }),
    )
    .await;
    eventually("the repository is looked at", || async {
        looking_at_repo(&state) == Some((true, true))
    })
    .await;
    // The run that follows the write ends, so the step's code side walks
    // the repository: the witness that the step ran.
    shared.forge_door.add_permits(1);
    eventually("the run after the write ends", || async {
        sqlx::query_scalar::<_, bool>(
            "select count(*) = 2 from knobas.sync_run where source_id = $1 and outcome = 'ok'",
        )
        .bind(FORGE)
        .fetch_one(&state.pool)
        .await
        .unwrap()
    })
    .await;
    let walks = || {
        shared
            .seen
            .walked
            .lock()
            .unwrap()
            .iter()
            .filter(|each| *each == REPO)
            .count()
    };
    let before = walks();
    queue_build(&shared, 2, "feature", C1);
    clock.0.send_replace(Duration::from_secs(5));
    eventually("the step at 5 s walked the repository", || async {
        walks() > before
    })
    .await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(
        !shared
            .seen
            .walked
            .lock()
            .unwrap()
            .iter()
            .any(|each| each == CONFIG),
        "the pipeline is not walked under the CI run's lock"
    );
    assert_eq!(
        looking_at_repo(&state),
        Some((true, true)),
        "still looking for builds"
    );

    shared.ci_door.add_permits(1);
    eventually("the CI run ends", || async {
        sqlx::query_scalar::<_, bool>(
            "select finished_at is not null from knobas.sync_run where id = $1",
        )
        .bind(run)
        .fetch_one(&state.pool)
        .await
        .unwrap()
    })
    .await;
    clock.0.send_replace(Duration::from_secs(15));
    eventually("the build the run brought counts", || async {
        looking_at_repo(&state).is_some_and(|(_, builds)| !builds)
    })
    .await;
}

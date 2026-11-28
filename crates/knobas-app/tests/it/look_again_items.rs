//! **After every landed write, the written item and its linked items are
//! looked at again on the bounded schedule** (ADR-0064 rule 2 beyond
//! branches, #2595): what a source does on its own seconds after a write (a
//! bot comments on a pull request, Jira automation transitions a ticket,
//! Jira's development integration moves a merged pull request's ticket)
//! shows within about 5 s, 15 s, 30 s, 60 s and 120 s, without a run.
//!
//! Two trait-level fakes stand in for the products, with the real Gitea and
//! Jira templates' declarations so the mirror reads them as it reads theirs:
//!
//! * **`forge`**, as Gitea: a pull request is read by id, its repository
//!   walked.
//! * **`jira`**, as Jira: a ticket is read by id.
//!
//! Each source's runs pass a door of its own that the test holds shut once
//! the mirror is seeded, so what shows was read by the schedule, not by a
//! run. The schedule keeps a clock the test moves by hand.

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
    AdapterRegistry, LookingAgain, RefreshEnd, RefreshEnded, RunConnections, SchedulerDeps,
    SchedulerTiming, SyncEvents,
};
use serde_json::{Value, json};
use tokio::sync::{Semaphore, watch};

const FORGE: &str = "forge";
const JIRA: &str = "jira";
const FULL_NAME: &str = "tide/pay";
const REPO: &str = "forge:repo:tide/pay";
const PR: &str = "forge:pr:tide/pay/7";
const PAY_1: &str = "jira:ticket:PAY-1";
const PAY_2: &str = "jira:ticket:PAY-2";
const PATIENCE: Duration = Duration::from_secs(20);

/// What the two products hold.
#[derive(Clone, Debug)]
struct World {
    pr_state: String,
    pr_comments: u64,
    /// Each ticket's status and comment count, by entity id.
    tickets: HashMap<String, (String, u64)>,
}

impl Default for World {
    fn default() -> Self {
        Self {
            pr_state: "open".to_owned(),
            pr_comments: 0,
            tickets: HashMap::from([
                (PAY_1.to_owned(), ("In Progress".to_owned(), 0)),
                (PAY_2.to_owned(), ("To Do".to_owned(), 0)),
            ]),
        }
    }
}

/// What the test watches the fakes do.
#[derive(Default)]
struct Seen {
    /// Every read by id, in order.
    read: Mutex<Vec<String>>,
    forge_runs: AtomicUsize,
    jira_runs: AtomicUsize,
}

#[derive(Clone)]
struct Shared {
    world: Arc<Mutex<World>>,
    seen: Arc<Seen>,
    forge_door: Arc<Semaphore>,
    jira_door: Arc<Semaphore>,
    /// Whether the Jira template declares the ticket's read by id.
    tickets_refresh: bool,
    /// Every `looking:again` told, in order.
    looking: Arc<Mutex<Vec<LookingAgain>>>,
    /// Every `item:refreshed` told, in order.
    refreshed: Arc<Mutex<Vec<RefreshEnded>>>,
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

impl World {
    fn repo(&self) -> SyncItem {
        item(
            REPO,
            "repo",
            FULL_NAME,
            json!({ "full_name": FULL_NAME, "name": "pay", "default_branch": "main" }),
        )
    }

    fn pr(&self) -> SyncItem {
        item(
            PR,
            "pr",
            "PAY-1 pay by card",
            json!({
                "number": 7, "state": self.pr_state, "comments": self.pr_comments,
                "head": { "ref": "feature" },
                "base": { "ref": "main", "repo": { "full_name": FULL_NAME, "name": "pay" } },
            }),
        )
    }

    fn ticket(&self, entity: &str) -> Option<SyncItem> {
        let (status, comments) = self.tickets.get(entity)?;
        let key = entity.rsplit(':').next().unwrap_or_default();
        Some(item(
            entity,
            "ticket",
            key,
            json!({ "key": key, "fields": {
                "status": { "name": status }, "comment": { "total": comments },
            } }),
        ))
    }

    fn tickets(&self) -> Vec<SyncItem> {
        let mut ids: Vec<&String> = self.tickets.keys().collect();
        ids.sort();
        ids.into_iter().filter_map(|id| self.ticket(id)).collect()
    }
}

struct Fake {
    id: &'static str,
    shared: Shared,
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
        descriptor(self.id, self.shared.tickets_refresh)
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
            (&self.shared.seen.jira_runs, &self.shared.jira_door)
        };
        count.fetch_add(1, Ordering::SeqCst);
        door.acquire()
            .await
            .expect("the door is never closed")
            .forget();
        let items = {
            let world = self.shared.world.lock().unwrap();
            if self.id == FORGE {
                vec![world.repo(), world.pr()]
            } else {
                world.tickets()
            }
        };
        for each in items {
            sink.item(each).await?;
        }
        Ok("{}".to_owned())
    }

    async fn refresh(&self, entity: &str) -> Result<RefreshAnswer, SourceError> {
        self.shared
            .seen
            .read
            .lock()
            .unwrap()
            .push(entity.to_owned());
        let world = self.shared.world.lock().unwrap();
        let found = if entity == PR {
            Some(world.pr())
        } else {
            world.ticket(entity)
        };
        Ok(found.map_or(RefreshAnswer::NotOffered, RefreshAnswer::Item))
    }

    async fn refresh_under(
        &self,
        entity: &str,
        _cursor: Option<&str>,
    ) -> Result<UnderAnswer, SourceError> {
        // The repository's walk finds nothing new: the forge's own
        // bookkeeping is not what these tests watch.
        if entity == REPO {
            return Ok(UnderAnswer::Walked {
                items: Vec::new(),
                cursor: None,
            });
        }
        Ok(UnderAnswer::NotOffered)
    }

    async fn write(&self, op: WriteOp) -> Result<WriteReceipt, SourceError> {
        let mut world = self.shared.world.lock().unwrap();
        match op {
            WriteOp::Comment { entity, .. } if entity == PR => world.pr_comments += 1,
            WriteOp::Comment { entity, .. } => {
                if let Some(ticket) = world.tickets.get_mut(&entity) {
                    ticket.1 += 1;
                }
            }
            WriteOp::MergePullRequest { .. } => world.pr_state = "merged".to_owned(),
            other => return Err(SourceError::protocol(format!("not here: {other:?}"))),
        }
        Ok(WriteReceipt::none())
    }

    async fn reachable_transitions(&self, entity: &str) -> Result<Vec<String>, SourceError> {
        Err(SourceError::protocol(format!("no workflow for {entity:?}")))
    }
}

/// The real templates' declarations, under the fakes' ids; the Jira ticket's
/// read by id taken away where `tickets_refresh` is false.
fn descriptor(id: &str, tickets_refresh: bool) -> SourceDescriptor {
    let mut template = if id == FORGE {
        knobas_source_gitea::descriptor_template()
    } else {
        let mut jira = knobas_source_jira::descriptor_template();
        for kind in &mut jira.entity_kinds {
            if kind.id == "ticket" {
                kind.refreshes = tickets_refresh;
            }
        }
        jira
    };
    template.id = id.to_owned();
    template
}

struct Registry(Shared);

impl AdapterRegistry for Registry {
    fn descriptors(&self) -> Vec<SourceDescriptor> {
        vec![
            descriptor(FORGE, self.0.tickets_refresh),
            descriptor(JIRA, self.0.tickets_refresh),
        ]
    }

    fn build(
        &self,
        instance: knobas_source::instance::SourceInstance,
    ) -> Result<Box<dyn Source>, SourceError> {
        let id = if instance.id == FORGE { FORGE } else { JIRA };
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

/// Hears every `looking:again` and `item:refreshed`, and nothing else.
struct Heard(Shared);

impl SyncEvents for Heard {
    fn sync_state(&self, _status: knobas_sync::SourceSyncStatus) {}
    fn source_health(&self, _health: knobas_sync::CredentialHealth) {}
    fn activity_new(&self, _row: knobas_core::activity::ActivityRow) {}
    fn looking_again(&self, looking: &LookingAgain) {
        self.0.looking.lock().unwrap().push(looking.clone());
    }
    fn refresh_ended(&self, ended: &RefreshEnded) {
        self.0.refreshed.lock().unwrap().push(ended.clone());
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

/// No tick for the whole test: only the seeding runs and the runs the
/// writes start happen.
const HELD_OFF: SchedulerTiming = SchedulerTiming {
    startup_delay: Duration::from_secs(3600),
    tick: Duration::from_secs(3600),
    shutdown_grace: knobas_sync::scheduler::SHUTDOWN_GRACE,
};

/// A scheduler started the way the app starts it, over both sources, on a
/// hand-moved clock, the mirror seeded by one run of each, both doors shut
/// behind those runs, and the pull request linked to `PAY-1` by a confirmed
/// link.
async fn fixture(name: &str, tickets_refresh: bool) -> (SourcesState, Shared, Arc<HandClock>) {
    let connector = knobas_db::test_util::scratch_database(name).await;
    let pool = connector.pool(4).await.expect("a pool");
    let secrets = Arc::new(MemoryStore::new());
    for (id, kind) in [(FORGE, "gitea"), (JIRA, "jira")] {
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
        jira_door: Arc::new(Semaphore::new(1)),
        tickets_refresh,
        looking: Arc::default(),
        refreshed: Arc::default(),
    };
    let registry: Arc<dyn AdapterRegistry> = Arc::new(Registry(shared.clone()));
    let scheduler = knobas_app::sources::start_scheduler(
        SchedulerDeps {
            pool: pool.clone(),
            connections: Arc::new(Connections(connector)),
            registry: Arc::clone(&registry),
            secrets: secrets.clone(),
            events: Arc::new(Heard(shared.clone())),
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
    for id in [FORGE, JIRA] {
        run_to_its_end(&state, id, 1).await;
    }
    sqlx::query(
        "insert into knobas.link (from_id, to_id, relation, origin, note, created_by, confirmed_at)
         values ($1, $2, 'implements', 'manual', 'the pull request implements it', 'user', now())",
    )
    .bind(PR)
    .bind(PAY_1)
    .execute(&state.pool)
    .await
    .expect("the link");
    (state, shared, clock)
}

/// Start a run of `id` and wait for it to end `ok`, the `nth` of its source
/// to (its door must let it through).
async fn run_to_its_end(state: &SourcesState, id: &str, nth: i64) {
    state
        .scheduler
        .trigger(id, knobas_sync::SyncTrigger::Manual, None)
        .await
        .expect("a run starts");
    runs_ended(state, id, nth).await;
}

/// Wait until `nth` runs of `id` ended `ok`.
async fn runs_ended(state: &SourcesState, id: &str, nth: i64) {
    eventually(&format!("run {nth} of {id} ends"), || async {
        sqlx::query_scalar::<_, i64>(
            "select count(*) from knobas.sync_run where source_id = $1 and outcome = 'ok'",
        )
        .bind(id)
        .fetch_one(&state.pool)
        .await
        .unwrap()
            >= nth
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

/// What the schedule of `scope` looks for now: `None` when none runs.
fn looking_at(state: &SourcesState, scope: &str) -> Option<LookingAgain> {
    look_again::looking(&state.scheduler)
        .into_iter()
        .find(|each| each.scope == scope)
}

/// The mirror's payload of `entity`, live.
async fn mirrored(state: &SourcesState, entity: &str) -> Option<Value> {
    sqlx::query_scalar("select payload from sync.live_item where entity_id = $1")
        .bind(entity)
        .fetch_optional(&state.pool)
        .await
        .unwrap()
}

/// How many times `entity` was read by id so far.
fn reads_of(shared: &Shared, entity: &str) -> usize {
    shared
        .seen
        .read
        .lock()
        .unwrap()
        .iter()
        .filter(|each| *each == entity)
        .count()
}

/// Send `op` through the queue, wait for it to land, then let the run of
/// `source` that follows it through, the `nth` of its source, and wait for
/// it to end: from then on only the schedule reads.
async fn landed(state: &SourcesState, shared: &Shared, op: Value, source: &str, nth: i64) {
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
    let door = if source == FORGE {
        &shared.forge_door
    } else {
        &shared.jira_door
    };
    door.add_permits(1);
    runs_ended(state, source, nth).await;
}

/// Move the clock to `at` seconds and wait until `entity` was read
/// `reads` times in all.
async fn step(clock: &HandClock, shared: &Shared, at: u64, entity: &str, reads: usize) {
    clock.0.send_replace(Duration::from_secs(at));
    eventually(&format!("{entity} is read at {at} s"), || async {
        reads_of(shared, entity) >= reads
    })
    .await;
}

fn ticket_status(payload: &Value) -> &str {
    payload["fields"]["status"]["name"]
        .as_str()
        .unwrap_or_default()
}

/// **A bot's comment on a pull request shows within the schedule.** A
/// comment made in knobas is read back at once; a bot answers 10 s later. At
/// 5 s the pull request is read and nothing is new; at 15 s the read finds
/// the bot's comment, the mirror holds it, the window is told on
/// `item:refreshed` with no write behind it, and the pull request is not read
/// again: it moved, while its linked ticket is read on to the end. No run of
/// the forge brought it.
#[tokio::test(flavor = "multi_thread")]
async fn a_bot_comment_after_a_knobas_comment_shows_within_the_schedule() {
    let (state, shared, clock) = fixture("look_again_items_bot", true).await;
    landed(
        &state,
        &shared,
        json!({ "Comment": { "entity": PR, "body": "please re-run" } }),
        FORGE,
        2,
    )
    .await;
    eventually("the pull request is looked at", || async {
        looking_at(&state, PR).is_some_and(|each| each.items)
    })
    .await;
    assert_eq!(mirrored(&state, PR).await.unwrap()["comments"], 1);
    let before = reads_of(&shared, PR);

    step(&clock, &shared, 5, PR, before + 1).await;
    shared.world.lock().unwrap().pr_comments += 1;
    step(&clock, &shared, 15, PR, before + 2).await;
    eventually("the bot's comment shows", || async {
        mirrored(&state, PR)
            .await
            .is_some_and(|payload| payload["comments"] == 2)
    })
    .await;
    assert!(
        shared.refreshed.lock().unwrap().iter().any(|each| {
            each.entity_id == PR && each.write_id.is_none() && each.ended == RefreshEnd::Refreshed
        }),
        "the window hears the pull request moved"
    );

    // Its linked ticket, which nothing moves, is read on to the end.
    clock.0.send_replace(Duration::from_secs(300));
    eventually("the schedule ends", || async {
        looking_at(&state, PR).is_none()
    })
    .await;
    assert_eq!(
        reads_of(&shared, PR),
        before + 2,
        "a moved item is not read again"
    );
    assert_eq!(
        shared.seen.forge_runs.load(Ordering::SeqCst),
        2,
        "the seeding and the write's run"
    );
}

/// **A Jira ticket that automation transitions 20 s after a knobas comment
/// shows its new status within the schedule**: at 30 s.
#[tokio::test(flavor = "multi_thread")]
async fn a_ticket_automation_transitions_after_a_comment_shows_within_the_schedule() {
    let (state, shared, clock) = fixture("look_again_items_automation", true).await;
    landed(
        &state,
        &shared,
        json!({ "Comment": { "entity": PAY_1, "body": "done on my side" } }),
        JIRA,
        2,
    )
    .await;
    let before = reads_of(&shared, PAY_1);
    step(&clock, &shared, 5, PAY_1, before + 1).await;
    step(&clock, &shared, 15, PAY_1, before + 2).await;
    shared
        .world
        .lock()
        .unwrap()
        .tickets
        .get_mut(PAY_1)
        .unwrap()
        .0 = "Done".to_owned();
    step(&clock, &shared, 30, PAY_1, before + 3).await;
    eventually("the new status shows", || async {
        mirrored(&state, PAY_1)
            .await
            .is_some_and(|payload| ticket_status(&payload) == "Done")
    })
    .await;
    assert_eq!(
        shared.seen.jira_runs.load(Ordering::SeqCst),
        2,
        "the seeding and the write's run"
    );
}

/// **A merged pull request's linked ticket that moves 30 s later shows
/// within the schedule**, and in the one schedule the merge started: the
/// repository's (the merge moves a branch, #2585), whose sides now hold the
/// pull request and its linked ticket besides, so no two schedules read the
/// forge at the same moment.
#[tokio::test(flavor = "multi_thread")]
async fn a_merged_pull_requests_linked_ticket_that_moves_later_shows_within_the_schedule() {
    let (state, shared, clock) = fixture("look_again_items_merge", true).await;
    landed(
        &state,
        &shared,
        json!({ "MergePullRequest": { "entity": PR, "style": "merge" } }),
        FORGE,
        2,
    )
    .await;
    eventually("the repository is looked at, its items with it", || async {
        looking_at(&state, REPO).is_some_and(|each| each.code && each.items)
    })
    .await;
    assert!(
        looking_at(&state, PR).is_none(),
        "one schedule for the merge"
    );
    assert_eq!(mirrored(&state, PR).await.unwrap()["state"], "merged");
    let before = reads_of(&shared, PAY_1);
    step(&clock, &shared, 5, PAY_1, before + 1).await;
    step(&clock, &shared, 15, PAY_1, before + 2).await;
    step(&clock, &shared, 30, PAY_1, before + 3).await;
    shared
        .world
        .lock()
        .unwrap()
        .tickets
        .get_mut(PAY_1)
        .unwrap()
        .0 = "Done".to_owned();
    step(&clock, &shared, 60, PAY_1, before + 4).await;
    eventually("the linked ticket's new status shows", || async {
        mirrored(&state, PAY_1)
            .await
            .is_some_and(|payload| ticket_status(&payload) == "Done")
    })
    .await;
    assert_eq!(
        shared.seen.jira_runs.load(Ordering::SeqCst),
        1,
        "no Jira run since the seeding"
    );
    assert!(
        mirrored(&state, PAY_2)
            .await
            .is_some_and(|payload| ticket_status(&payload) == "To Do"),
        "an unlinked ticket is not read"
    );
    assert_eq!(
        reads_of(&shared, PAY_2),
        0,
        "an unlinked ticket is not read"
    );
}

/// **Bounded and shared.** An item nothing moves is read five times after
/// its read-back, at 5, 15, 30, 60 and 120 s, and then never again; a second
/// write on it restarts its schedule instead of stacking a second one.
#[tokio::test(flavor = "multi_thread")]
async fn an_item_is_read_five_times_at_most_and_a_second_write_restarts_its_schedule() {
    let (state, shared, clock) = fixture("look_again_items_bounded", true).await;
    landed(
        &state,
        &shared,
        json!({ "Comment": { "entity": PAY_2, "body": "first" } }),
        JIRA,
        2,
    )
    .await;
    let before = reads_of(&shared, PAY_2);
    step(&clock, &shared, 5, PAY_2, before + 1).await;
    step(&clock, &shared, 15, PAY_2, before + 2).await;

    clock.0.send_replace(Duration::from_secs(20));
    landed(
        &state,
        &shared,
        json!({ "Comment": { "entity": PAY_2, "body": "second" } }),
        JIRA,
        3,
    )
    .await;
    let restarted = reads_of(&shared, PAY_2);
    let schedules = look_again::looking(&state.scheduler)
        .into_iter()
        .filter(|each| each.scope == PAY_2)
        .count();
    assert_eq!(schedules, 1, "one schedule per scope");
    // The first schedule's step at 30 s does not come: the new one reads at
    // 25, 35, 50, 80 and 140 s.
    for (at, reads) in [(25, 1), (35, 2), (50, 3), (80, 4), (140, 5)] {
        step(&clock, &shared, at, PAY_2, restarted + reads).await;
    }
    eventually("the schedule ends", || async {
        looking_at(&state, PAY_2).is_none()
    })
    .await;
    clock.0.send_replace(Duration::from_secs(600));
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(
        reads_of(&shared, PAY_2),
        restarted + 5,
        "five reads at most"
    );
}

/// **A kind that can't be read by id is not scheduled**: the *say so* rule
/// (#2596) speaks for it.
#[tokio::test(flavor = "multi_thread")]
async fn a_kind_that_cannot_refresh_is_not_scheduled() {
    let (state, shared, clock) = fixture("look_again_items_not_offered", false).await;
    landed(
        &state,
        &shared,
        json!({ "Comment": { "entity": PAY_2, "body": "hello" } }),
        JIRA,
        2,
    )
    .await;
    let before = reads_of(&shared, PAY_2);
    clock.0.send_replace(Duration::from_secs(5));
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(look_again::looking(&state.scheduler).is_empty());
    assert_eq!(reads_of(&shared, PAY_2), before);
}

//! **A landed write reads back what it moves, at once** (ADR-0064, #2592):
//! after a merge or a branch update lands, the write's declared effects
//! (`WriteOp::effects`) are read at their own sources, in their own places,
//! before the run of the write's source that still follows (ADR-0031), and
//! no source is run for them.
//!
//! Two trait-level fakes stand in for the products, shaped as their adapters
//! answer:
//!
//! * **`forge`**, as Gitea: a pull request and an issue are read by id; a
//!   repository is not (`NotOffered`) and is walked instead
//!   (`refresh_under`, ADR-0047), which brings its branches' heads, a
//!   deleted branch's tombstone and its issues. A pull request declares its
//!   repository as its parent through `base.repo`, as Gitea's template does,
//!   by `full_name` and by `name`; a second repository of the same short
//!   name stands under another owner, so a parent matched by `name` alone
//!   would walk it too.
//! * **`tracker`**, as Jira: a ticket is read by id; a board or filter is
//!   not, and is walked by its key (`refresh_under`, #2593). Its tickets
//!   declare their project, as Jira's do, and its views are saved views
//!   whose `keys` are what the tracker's query answers: a Kanban board over
//!   `PAY` that leaves out *Done*, a filter on the assignee `mara`, and a
//!   board over `OPS`, which no `PAY` ticket write may read.
//!
//! Every run passes a door the test holds shut once the mirror is seeded, so
//! "read without a run" is a fact: what a run would bring cannot reach the
//! mirror until the test opens the door, after it has looked.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use knobas_app::sources::SourcesState;
use knobas_app::sources::write_queue;
use knobas_core::write_queue::WriteState;
use knobas_secrets::{MemoryStore, Secret, SecretStore};
use knobas_source::{
    AuthMethod, ConnectionInfo, Cursor, KindInfo, KindPaths, ParentPath, PayloadPath,
    RefreshAnswer, Sink, Source, SourceDescriptor, SourceError, SyncItem, UnderAnswer, WriteOp,
    WriteReceipt,
};
use knobas_sync::scheduler::{
    AdapterRegistry, RunConnections, SchedulerDeps, SchedulerTiming, SyncEvents, Unread, UnreadWhy,
    WriteUnread,
};
use serde_json::{Value, json};
use tokio::sync::Semaphore;

const FORGE: &str = "forge";
const TRACKER: &str = "tracker";
const REPO: &str = "forge:tide/pay";
const DECOY: &str = "forge:elsewhere/pay";
const PULL: &str = "forge:tide/pay#7";
const MAIN: &str = "forge:tide/pay@refs/heads/main";
const FEATURE: &str = "forge:tide/pay@refs/heads/feature";
const ISSUE: &str = "forge:tide/pay/issues/3";
const TICKET: &str = "tracker:PAY-1";
const SIBLING: &str = "tracker:PAY-2";
const FILED: &str = "tracker:PAY-3";
const OTHER: &str = "tracker:OPS-1";
const BOARD: &str = "tracker:board-1";
const MINE: &str = "tracker:filter-9";
const ELSEWHERE: &str = "tracker:board-2";
const OLD_MAIN: &str = "1111111";
const MERGED_MAIN: &str = "2222222";
const OLD_FEATURE: &str = "aaaaaaa";
const UPDATED_FEATURE: &str = "bbbbbbb";
const PATIENCE: Duration = Duration::from_secs(20);

/// What the two products hold. A write changes it as the product would; a
/// read answers it.
#[derive(Clone, Debug)]
struct World {
    merged: bool,
    main_head: String,
    /// `None` once a merge deleted the head branch.
    feature_head: Option<String>,
    issue_open: bool,
    ticket_status: String,
    /// Whom `PAY-1` is assigned to.
    assignee: Option<String>,
    /// Whether `PAY-3` has been filed.
    filed: bool,
}

impl Default for World {
    fn default() -> Self {
        Self {
            merged: false,
            main_head: OLD_MAIN.to_owned(),
            feature_head: Some(OLD_FEATURE.to_owned()),
            issue_open: true,
            ticket_status: "In Review".to_owned(),
            assignee: None,
            filed: false,
        }
    }
}

/// What the test watches the fakes do.
#[derive(Default)]
struct Seen {
    /// The parents walked, in order.
    walked: Mutex<Vec<String>>,
    /// The items read by id, in order.
    refreshed: Mutex<Vec<String>>,
    /// Runs that reached a source, of either.
    syncs_entered: AtomicUsize,
}

#[derive(Clone)]
struct Shared {
    world: Arc<Mutex<World>>,
    seen: Arc<Seen>,
    /// One permit per run the test lets through.
    door: Arc<Semaphore>,
    /// Every `write:unread` told, in order.
    unread: Arc<Mutex<Vec<WriteUnread>>>,
}

fn item(entity: &str, kind: &str, title: &str, payload: Value, deleted: bool) -> SyncItem {
    SyncItem {
        entity: knobas_core::entity::EntityRef::parse(entity).expect("an entity id"),
        kind: kind.to_owned(),
        title: title.to_owned(),
        body_text: String::new(),
        author: None,
        updated_at: None,
        payload,
        web_url: None,
        deleted,
    }
}

fn repository(full_name: &str) -> Value {
    let name = full_name.rsplit('/').next().unwrap_or(full_name);
    json!({ "full_name": full_name, "name": name })
}

impl World {
    fn pull(&self) -> SyncItem {
        item(
            PULL,
            "pull_request",
            "Retry SEPA payouts",
            json!({
                "state": if self.merged { "closed" } else { "open" },
                "merged": self.merged,
                "base": { "ref": "main", "repo": repository("tide/pay") },
                "head": { "ref": "feature", "sha": self.feature_head },
            }),
            false,
        )
    }

    fn branch(entity: &str, name: &str, head: &str) -> SyncItem {
        item(
            entity,
            "branch",
            name,
            json!({ "name": name, "commit": { "id": head }, "repository": repository("tide/pay") }),
            false,
        )
    }

    fn issue(&self) -> SyncItem {
        item(
            ISSUE,
            "ticket",
            "SEPA payouts fail on retry",
            json!({ "state": if self.issue_open { "open" } else { "closed" }, "repository": repository("tide/pay") }),
            false,
        )
    }

    /// What a walk of `tide/pay` emits: the repository, its branches (a
    /// deleted one as its tombstone) and its issue.
    fn walk(&self) -> Vec<SyncItem> {
        let feature = match &self.feature_head {
            Some(head) => Self::branch(FEATURE, "feature", head),
            None => {
                let mut gone = Self::branch(FEATURE, "feature", OLD_FEATURE);
                gone.deleted = true;
                gone
            }
        };
        vec![
            item(
                REPO,
                "repository",
                "tide/pay",
                repository("tide/pay"),
                false,
            ),
            Self::branch(MAIN, "main", &self.main_head),
            feature,
            self.issue(),
        ]
    }

    /// The tracker's tickets: `(entity, project, status, assignee)`.
    fn tickets(&self) -> Vec<(&'static str, &'static str, String, Option<String>)> {
        let mara = Some("mara".to_owned());
        let mut tickets = vec![
            (
                TICKET,
                "PAY",
                self.ticket_status.clone(),
                self.assignee.clone(),
            ),
            (SIBLING, "PAY", "To Do".to_owned(), mara.clone()),
            (OTHER, "OPS", "To Do".to_owned(), mara),
        ];
        if self.filed {
            tickets.push((FILED, "PAY", "To Do".to_owned(), None));
        }
        tickets
    }

    fn ticket_of(&self, entity: &str) -> Option<SyncItem> {
        let (entity, project, status, assignee) =
            self.tickets().into_iter().find(|t| t.0 == entity)?;
        Some(item(
            entity,
            "ticket",
            if entity == TICKET {
                "Retry SEPA payouts"
            } else {
                entity
            },
            json!({ "fields": {
                "status": { "name": status },
                "project": { "key": project },
                "assignee": assignee,
            } }),
            false,
        ))
    }

    /// A view with the keys its query answers now, in the tracker's order.
    fn view(&self, entity: &str) -> SyncItem {
        let keys: Vec<&str> = self
            .tickets()
            .into_iter()
            .filter(|(_, project, status, assignee)| match entity {
                BOARD => *project == "PAY" && status != "Done",
                MINE => assignee.as_deref() == Some("mara"),
                _ => *project == "OPS",
            })
            .map(|(ticket, ..)| ticket.split_once(':').expect("an entity id").1)
            .collect();
        let kind = if entity == MINE { "filter" } else { "board" };
        item(
            entity,
            "view",
            entity,
            json!({ "kind": kind, "keys": keys }),
            false,
        )
    }

    /// What a run of the tracker emits.
    fn tracked(&self) -> Vec<SyncItem> {
        let mut items: Vec<SyncItem> = self
            .tickets()
            .iter()
            .filter_map(|t| self.ticket_of(t.0))
            .collect();
        items.extend([BOARD, MINE, ELSEWHERE].map(|view| self.view(view)));
        items
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
        self.shared
            .seen
            .syncs_entered
            .fetch_add(1, Ordering::SeqCst);
        self.shared
            .door
            .acquire()
            .await
            .expect("the door is never closed")
            .forget();
        let world = self.shared.world.lock().unwrap().clone();
        let items = if self.id == FORGE {
            let mut items = world.walk();
            items.push(world.pull());
            items.push(item(
                DECOY,
                "repository",
                "elsewhere/pay",
                repository("elsewhere/pay"),
                false,
            ));
            items
        } else {
            world.tracked()
        };
        for each in items {
            sink.item(each).await?;
        }
        Ok("{}".to_owned())
    }

    async fn refresh(&self, entity: &str) -> Result<RefreshAnswer, SourceError> {
        self.shared
            .seen
            .refreshed
            .lock()
            .unwrap()
            .push(entity.to_owned());
        let world = self.shared.world.lock().unwrap().clone();
        Ok(match entity {
            PULL => RefreshAnswer::Item(world.pull()),
            ISSUE => RefreshAnswer::Item(world.issue()),
            TICKET | SIBLING | FILED | OTHER => world
                .ticket_of(entity)
                .map_or(RefreshAnswer::Gone, RefreshAnswer::Item),
            _ => RefreshAnswer::NotOffered,
        })
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
        let world = self.shared.world.lock().unwrap().clone();
        Ok(match entity {
            REPO => UnderAnswer::Walked {
                items: world.walk(),
                cursor: None,
            },
            DECOY => UnderAnswer::Walked {
                items: Vec::new(),
                cursor: None,
            },
            BOARD | MINE | ELSEWHERE => UnderAnswer::Walked {
                items: vec![world.view(entity)],
                cursor: None,
            },
            _ => UnderAnswer::NotOffered,
        })
    }

    async fn write(&self, op: WriteOp) -> Result<WriteReceipt, SourceError> {
        let mut world = self.shared.world.lock().unwrap();
        match op {
            WriteOp::MergePullRequest { delete_branch, .. } => {
                world.merged = true;
                world.main_head = MERGED_MAIN.to_owned();
                if delete_branch {
                    world.feature_head = None;
                }
                world.issue_open = false;
                world.ticket_status = "Done".to_owned();
            }
            WriteOp::UpdatePullRequestBranch { .. } => {
                world.feature_head = Some(UPDATED_FEATURE.to_owned());
            }
            WriteOp::Transition { status, .. } => world.ticket_status = status,
            WriteOp::EditTicket { assignee, .. } => {
                if let Some(assignee) = assignee {
                    world.assignee = assignee.account().map(str::to_owned);
                }
            }
            WriteOp::CreateTicket { .. } => {
                world.filed = true;
                return Ok(WriteReceipt::id("PAY-3"));
            }
            other => return Err(SourceError::protocol(format!("not here: {other:?}"))),
        }
        Ok(WriteReceipt::none())
    }

    async fn reachable_transitions(&self, entity: &str) -> Result<Vec<String>, SourceError> {
        Err(SourceError::protocol(format!("no workflow for {entity:?}")))
    }
}

fn kind(id: &str) -> KindInfo {
    KindInfo {
        id: id.to_owned(),
        label: id.to_owned(),
        plural: format!("{id}s"),
        monogram: "X".to_owned(),
        full_sync_exhaustive: true,
        refreshes: true,
        title_stays_in_mirror: false,
    }
}

/// The forge's declarations as Gitea's template states them: a branch and a
/// pull request name their repository by `full_name` and by `name`.
fn forge_paths() -> Vec<KindPaths> {
    let repo = |at: &[&str]| {
        vec![ParentPath {
            kind: "repository".to_owned(),
            at: PayloadPath::of(at.iter().copied()),
            names: vec![PayloadPath::of(["full_name"]), PayloadPath::of(["name"])],
        }]
    };
    vec![
        KindPaths {
            kind: "repository".to_owned(),
            ..KindPaths::default()
        },
        KindPaths {
            kind: "branch".to_owned(),
            parent: repo(&["repository"]),
            ..KindPaths::default()
        },
        KindPaths {
            kind: "pull_request".to_owned(),
            parent: repo(&["base", "repo"]),
            ..KindPaths::default()
        },
    ]
}

/// The tracker's declarations as Jira's template states them: a ticket
/// names its project, and a view is a saved view.
fn tracker_paths() -> Vec<KindPaths> {
    vec![
        KindPaths {
            kind: "ticket".to_owned(),
            project_key: vec![PayloadPath::of(["fields", "project", "key"])],
            ..KindPaths::default()
        },
        KindPaths {
            kind: "view".to_owned(),
            frame: Some(knobas_core::payload::Frame::SavedView),
            ..KindPaths::default()
        },
    ]
}

fn descriptor(id: &str) -> SourceDescriptor {
    let forge = id == FORGE;
    SourceDescriptor {
        id: id.to_owned(),
        adapter_kind: id.to_owned(),
        name: id.to_owned(),
        capabilities: vec![
            knobas_source::Capability::Write,
            knobas_source::Capability::Refresh,
        ],
        adapter_version: "0".to_owned(),
        auth_methods: vec![AuthMethod::Pat],
        accepts_account: false,
        min_sync_interval_secs: 60,
        roles: Vec::new(),
        source_actions: Vec::new(),
        clone_from: Vec::new(),
        url_forms: Vec::new(),
        web_sign_in: None,
        web_links: None,
        code_pages: None,
        by_hand_by_default: false,
        passwords: None,
        write_ops: if forge {
            vec![
                "merge_pull_request".to_owned(),
                "update_pull_request_branch".to_owned(),
            ]
        } else {
            vec![
                "transition".to_owned(),
                "create_ticket".to_owned(),
                "edit_ticket".to_owned(),
            ]
        },
        entity_kinds: if forge {
            ["repository", "branch", "pull_request", "ticket"]
                .into_iter()
                .map(kind)
                .collect()
        } else {
            vec![kind("ticket"), kind("view")]
        },
        config_schema: json!({"type": "object", "properties": {}}),
        payload_paths: if forge {
            forge_paths()
        } else {
            tracker_paths()
        },
    }
}

struct Registry(Shared);

impl AdapterRegistry for Registry {
    fn descriptors(&self) -> Vec<SourceDescriptor> {
        vec![descriptor(FORGE), descriptor(TRACKER)]
    }

    fn build(
        &self,
        instance: knobas_source::instance::SourceInstance,
    ) -> Result<Box<dyn Source>, SourceError> {
        let id = if instance.id == FORGE { FORGE } else { TRACKER };
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

/// Hears every `write:unread`, and nothing else.
struct Heard(Arc<Mutex<Vec<WriteUnread>>>);

impl SyncEvents for Heard {
    fn sync_state(&self, _status: knobas_sync::SourceSyncStatus) {}
    fn source_health(&self, _health: knobas_sync::CredentialHealth) {}
    fn activity_new(&self, _row: knobas_core::activity::ActivityRow) {}
    fn write_unread(&self, unread: &WriteUnread) {
        self.0.lock().unwrap().push(unread.clone());
    }
}

/// No tick for the whole test: only the seeding runs and a landed write's
/// run happen.
const HELD_OFF: SchedulerTiming = SchedulerTiming {
    startup_delay: Duration::from_secs(3600),
    tick: Duration::from_secs(3600),
    shutdown_grace: knobas_sync::scheduler::SHUTDOWN_GRACE,
};

/// A scheduler started the way the app starts it, over both sources, the
/// mirror seeded by one run of each, the pull request linked to the ticket,
/// and the door shut behind those runs.
async fn fixture(name: &str) -> (SourcesState, Shared) {
    let connector = knobas_db::test_util::scratch_database(name).await;
    let pool = connector.pool(4).await.expect("a pool");
    let secrets = Arc::new(MemoryStore::new());
    for id in [FORGE, TRACKER] {
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
                adapter_kind: id.to_owned(),
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
    // Exactly the two seeding runs pass; every later run waits at the door.
    let shared = Shared {
        world: Arc::default(),
        seen: Arc::default(),
        door: Arc::new(Semaphore::new(2)),
        unread: Arc::default(),
    };
    let registry: Arc<dyn AdapterRegistry> = Arc::new(Registry(shared.clone()));
    let scheduler = knobas_app::sources::start_scheduler(
        SchedulerDeps {
            pool: pool.clone(),
            connections: Arc::new(Connections(connector)),
            registry: Arc::clone(&registry),
            secrets: secrets.clone(),
            events: Arc::new(Heard(Arc::clone(&shared.unread))),
            timing: HELD_OFF,
            quit: knobas_sync::scheduler::Quit::default(),
        },
        knobas_sync::background::Background::new(),
    )
    .await
    .expect("a scheduler");
    let state = SourcesState {
        live: Default::default(),
        pool,
        scheduler,
        secrets,
        registry,
    };
    for id in [FORGE, TRACKER] {
        let run = state
            .scheduler
            .trigger(id, knobas_sync::SyncTrigger::Manual, None)
            .await
            .expect("a seeding run starts");
        eventually(&format!("the seeding run of {id} ends"), || async {
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
    sqlx::query(
        "insert into knobas.link (from_id, to_id, relation, origin, created_by, confirmed_at)
         values ($1, $2, 'related', 'manual', 'test', now())",
    )
    .bind(PULL)
    .bind(TICKET)
    .execute(&state.pool)
    .await
    .expect("the pull request is linked to its ticket");
    assert_eq!(shared.seen.syncs_entered.load(Ordering::SeqCst), 2);
    (state, shared)
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

/// The live payload the mirror holds for `entity`, if it holds it live.
async fn mirrored(state: &SourcesState, entity: &str) -> Option<Value> {
    sqlx::query_scalar("select payload from sync.live_item where entity_id = $1")
        .bind(entity)
        .fetch_optional(&state.pool)
        .await
        .unwrap()
}

/// Runs of `source` that began after write `id` settled, finished or not.
async fn runs_since(state: &SourcesState, source: &str, id: i64) -> (i64, i64) {
    sqlx::query_as(
        "select count(*), count(r.finished_at) from knobas.sync_run r, knobas.write_queue w
          where w.id = $2 and r.source_id = $1 and r.started_at > w.settled_at",
    )
    .bind(source)
    .bind(id)
    .fetch_one(&state.pool)
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

/// After the read-back, the run of the write's source still follows: once
/// the door opens, a run of `source` that began after the write finishes.
async fn the_run_still_follows(state: &SourcesState, shared: &Shared, source: &str, id: i64) {
    shared.door.add_permits(1_000);
    eventually(
        &format!("a run of {source} that began after the write finishes"),
        || async { runs_since(state, source, id).await.1 > 0 },
    )
    .await;
}

/// The tickets the mirror's saved view `view` holds, in its order.
async fn held(state: &SourcesState, view: &str) -> Vec<String> {
    sqlx::query_scalar(
        "select entity_id from sync.saved_view_member where view_id = $1 order by rank",
    )
    .bind(view)
    .fetch_all(&state.pool)
    .await
    .unwrap()
}

/// **A merge with `delete_branch` shows its effects without a run**: the
/// base branch's new head, the head branch gone, the issue the merge closed
/// shown closed, all from one walk of the pull request's repository, and the
/// linked ticket in the other source read by id there. Neither source has
/// finished a run since the write, and the tracker has not started one. The
/// repository of the same short name under another owner is not walked, and
/// the pull request is read once.
#[tokio::test(flavor = "multi_thread")]
async fn a_merge_reads_back_its_repository_and_linked_ticket_without_a_run() {
    let (state, shared) = fixture("effects_merge").await;
    assert_eq!(
        mirrored(&state, MAIN).await.unwrap()["commit"]["id"],
        OLD_MAIN
    );

    let id = landed(
        &state,
        json!({ "MergePullRequest": { "entity": PULL, "style": "merge", "delete_branch": true } }),
    )
    .await;

    eventually("the merge's effects are in the mirror", || async {
        let main = mirrored(&state, MAIN).await;
        let issue = mirrored(&state, ISSUE).await;
        let ticket = mirrored(&state, TICKET).await;
        main.is_some_and(|p| p["commit"]["id"] == MERGED_MAIN)
            && mirrored(&state, FEATURE).await.is_none()
            && issue.is_some_and(|p| p["state"] == "closed")
            && ticket.is_some_and(|p| p["fields"]["status"]["name"] == "Done")
    })
    .await;
    assert_eq!(mirrored(&state, PULL).await.unwrap()["merged"], true);
    assert_eq!(
        runs_since(&state, FORGE, id).await.1,
        0,
        "no forge run finished"
    );
    assert_eq!(
        runs_since(&state, TRACKER, id).await,
        (0, 0),
        "the tracker was not run"
    );
    assert_eq!(*shared.seen.walked.lock().unwrap(), vec![REPO.to_owned()]);
    let refreshed = shared.seen.refreshed.lock().unwrap().clone();
    assert_eq!(
        refreshed.iter().filter(|e| *e == PULL).count(),
        1,
        "the pull request is read once: {refreshed:?}"
    );
    assert_eq!(
        refreshed.iter().filter(|e| *e == TICKET).count(),
        1,
        "the linked ticket is read once, by id: {refreshed:?}"
    );

    the_run_still_follows(&state, &shared, FORGE, id).await;
    // The run follows the telling, so by now it would have been told.
    assert_eq!(
        *shared.unread.lock().unwrap(),
        [],
        "a write whose effects were all read says nothing extra (#2596)"
    );
    state.scheduler.shutdown().await;
}

/// **A branch update shows the head branch's new tip without a run**, from
/// the walk of the pull request's repository. It declares no linked tickets,
/// so the tracker is not asked.
#[tokio::test(flavor = "multi_thread")]
async fn a_branch_update_reads_back_the_head_branch_tip_without_a_run() {
    let (state, shared) = fixture("effects_update_branch").await;
    assert_eq!(
        mirrored(&state, FEATURE).await.unwrap()["commit"]["id"],
        OLD_FEATURE
    );

    let id = landed(
        &state,
        json!({ "UpdatePullRequestBranch": { "entity": PULL, "style": "merge" } }),
    )
    .await;

    eventually("the head branch's new tip is in the mirror", || async {
        mirrored(&state, FEATURE)
            .await
            .is_some_and(|p| p["commit"]["id"] == UPDATED_FEATURE)
    })
    .await;
    assert_eq!(
        runs_since(&state, FORGE, id).await.1,
        0,
        "no forge run finished"
    );
    assert_eq!(*shared.seen.walked.lock().unwrap(), vec![REPO.to_owned()]);
    assert!(
        !shared
            .seen
            .refreshed
            .lock()
            .unwrap()
            .contains(&TICKET.to_owned()),
        "a branch update links no ticket"
    );

    the_run_still_follows(&state, &shared, FORGE, id).await;
    state.scheduler.shutdown().await;
}

/// **An effect waits behind a run of its source only so long** (ADR-0064
/// rule 3, said rather than waited on): with the tracker's lock held, as a
/// run that may not stand aside holds it (one a person pressed, a first sync,
/// a backfill), the merge's repository is still read back and the run of the
/// forge after the write still begins and finishes while that lock is held.
/// The linked ticket is not read; that run or the tracker's next brings it.
#[tokio::test(flavor = "multi_thread")]
async fn an_effect_behind_a_run_that_does_not_stand_aside_holds_up_nothing() {
    let (state, shared) = fixture("effects_patience").await;
    let mut tracker_run = state.pool.begin().await.unwrap();
    sqlx::query("select pg_advisory_xact_lock(hashtext($1::text))")
        .bind(TRACKER)
        .execute(&mut *tracker_run)
        .await
        .unwrap();
    shared.door.add_permits(1_000);

    let id = landed(
        &state,
        json!({ "MergePullRequest": { "entity": PULL, "style": "merge", "delete_branch": true } }),
    )
    .await;

    eventually(
        "a run of the forge that began after the write finishes while the tracker is held",
        || async { runs_since(&state, FORGE, id).await.1 > 0 },
    )
    .await;
    assert_eq!(
        mirrored(&state, MAIN).await.unwrap()["commit"]["id"],
        MERGED_MAIN
    );
    assert!(
        !shared
            .seen
            .refreshed
            .lock()
            .unwrap()
            .contains(&TICKET.to_owned()),
        "the ticket was not read while its source was held"
    );
    // Said, not waited on (#2596): the ticket, its source, and why.
    let told = shared.unread.lock().unwrap().clone();
    assert_eq!(told.len(), 1, "one telling for the write: {told:?}");
    assert_eq!((told[0].write_id, told[0].entity_id.as_str()), (id, PULL));
    let [ticket] = told[0].unread.as_slice() else {
        panic!("only the ticket is unread: {told:?}");
    };
    assert_eq!(
        (
            ticket.entity_id.as_deref(),
            ticket.title.as_str(),
            ticket.source_id.as_str(),
            ticket.why,
            ticket.by_hand
        ),
        (
            Some(TICKET),
            "Retry SEPA payouts",
            TRACKER,
            UnreadWhy::RunInFlight,
            false
        )
    );
    assert!(
        ticket.next_run_at.is_some(),
        "the tracker's next run is known: {ticket:?}"
    );
    tracker_run.rollback().await.unwrap();
    state.scheduler.shutdown().await;
}

/// **A write to a source that syncs by hand says what else it moved shows
/// after *Sync now*** (ADR-0064 rule 3, ADR-0056, #2596): no run follows the
/// write, so its effects read at once are all that arrives until the press.
#[tokio::test(flavor = "multi_thread")]
async fn a_write_to_a_source_that_syncs_by_hand_says_the_rest_waits_for_sync_now() {
    let (state, shared) = fixture("effects_by_hand").await;
    sqlx::query("update knobas.source_config set sync_by_hand = true where id = $1")
        .bind(FORGE)
        .execute(&state.pool)
        .await
        .unwrap();

    let id = landed(
        &state,
        json!({ "UpdatePullRequestBranch": { "entity": PULL, "style": "merge" } }),
    )
    .await;

    eventually("the write's telling", || async {
        !shared.unread.lock().unwrap().is_empty()
    })
    .await;
    let told = shared.unread.lock().unwrap().clone();
    assert_eq!(
        told,
        [WriteUnread {
            write_id: id,
            entity_id: PULL.to_owned(),
            title: "Retry SEPA payouts".to_owned(),
            unread: vec![Unread {
                entity_id: None,
                title: "Retry SEPA payouts".to_owned(),
                source_id: FORGE.to_owned(),
                source_name: FORGE.to_owned(),
                by_hand: true,
                why: UnreadWhy::ByHand,
                next_run_at: None,
            }],
        }]
    );
    assert_eq!(
        runs_since(&state, FORGE, id).await,
        (0, 0),
        "no run follows"
    );
    state.scheduler.shutdown().await;
}

/// A ticket write's views were read without a run: no run of the tracker
/// finished since the write, the views of the ticket's project were walked,
/// and the board over another project was not; then the run still follows.
async fn the_views_were_read_without_a_run(state: &SourcesState, shared: &Shared, id: i64) {
    assert_eq!(
        runs_since(state, TRACKER, id).await.1,
        0,
        "no tracker run finished"
    );
    // Side by side, so one view may land before the other is walked: the caller waited for one view's change, and
    // under load the other's read can still be on its way (the gate of #2615). No run can finish meanwhile: the
    // tracker's door is shut until `the_run_still_follows`.
    eventually("both views are walked", || async {
        shared.seen.walked.lock().unwrap().len() >= 2
    })
    .await;
    let mut walked = shared.seen.walked.lock().unwrap().clone();
    walked.sort();
    assert_eq!(walked, [BOARD, MINE], "read at once, side by side");
    the_run_still_follows(state, shared, TRACKER, id).await;
}

/// **A transition that takes a ticket out of a board's query drops its card
/// without a run** (#2593): the Kanban board that leaves out *Done* holds
/// `PAY-1` no longer, read by its key at once.
#[tokio::test(flavor = "multi_thread")]
async fn a_transition_drops_the_ticket_from_a_board_that_leaves_it_out_without_a_run() {
    let (state, shared) = fixture("effects_views_transition").await;
    assert_eq!(held(&state, BOARD).await, [TICKET, SIBLING]);

    let id = landed(
        &state,
        json!({ "Transition": { "entity": TICKET, "status": "Done" } }),
    )
    .await;

    eventually("the board drops PAY-1", || async {
        held(&state, BOARD).await == [SIBLING]
    })
    .await;
    the_views_were_read_without_a_run(&state, &shared, id).await;
    state.scheduler.shutdown().await;
}

/// **A ticket filed in a project shows on that project's board without a
/// run**: the views are the new ticket's, whose project the mirror knows
/// once it is read back, though the write's target is the project.
#[tokio::test(flavor = "multi_thread")]
async fn a_ticket_filed_shows_on_its_projects_board_without_a_run() {
    let (state, shared) = fixture("effects_views_create").await;

    let id = landed(
        &state,
        json!({ "CreateTicket": {
            "entity": "tracker:PAY", "title": "Retry on 503", "body": "", "ticket_type": "Task"
        } }),
    )
    .await;

    eventually("the board holds PAY-3", || async {
        held(&state, BOARD).await == [TICKET, SIBLING, FILED]
    })
    .await;
    the_views_were_read_without_a_run(&state, &shared, id).await;
    state.scheduler.shutdown().await;
}

/// **An assignee change moves the ticket into a filter keyed on the
/// assignee without a run**: the filter holds no `PAY-1` before, and is
/// found through `PAY-2`, the project's ticket it holds.
#[tokio::test(flavor = "multi_thread")]
async fn an_assignee_change_moves_the_ticket_into_the_filter_on_the_assignee_without_a_run() {
    let (state, shared) = fixture("effects_views_assignee").await;
    assert_eq!(held(&state, MINE).await, [SIBLING, OTHER]);

    let id = landed(
        &state,
        json!({ "EditTicket": { "entity": TICKET, "assignee": { "account": "mara" } } }),
    )
    .await;

    eventually("the filter holds PAY-1", || async {
        held(&state, MINE).await == [TICKET, SIBLING, OTHER]
    })
    .await;
    the_views_were_read_without_a_run(&state, &shared, id).await;
    state.scheduler.shutdown().await;
}

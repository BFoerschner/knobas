//! **M2 exit criterion 1, end to end**: ticket → branch → pull request →
//! link → In Progress, and back again when the pull request is merged
//! (issue #44).
//!
//! Every other test of this feature stops one side short of the real thing.
//! `tests/start_work.rs` drives the orchestrator against a fake dispatcher and
//! proves the *sequence* -- which ops, in what order, and what happens when one
//! fails -- with nothing at either end of it. This is the other half: a real
//! Gitea in its container, a real Jira through `knobas-mockd`, the real write
//! queue, the real adapters, a real database. Nothing here asserts on a step's
//! outcome alone; **every assertion is against a source's own answer or a
//! stored row**, because the criterion is the round trip, not the dispatch.
//!
//! # Why this is `#[ignore]`d and `tests/start_work.rs` is not
//!
//! It needs Docker, a seeded Gitea and a token -- the treatment
//! `crates/knobas-source-gitea/tests/live_gitea.rs` gets, and for the same
//! reason. Run it with the environment up:
//!
//! ```text
//! cd testenv && docker compose up -d --wait gitea && ./seed-gitea.sh && eval "$(./seed --env)"
//! cd .. && env -u RUSTUP_TOOLCHAIN cargo test -p knobas-app --test start_work_live \
//!     -- --ignored --nocapture --test-threads=1
//! ```
//!
//! **One environment, one owner at a time** -- `testenv/README.md`. Seeding
//! re-mints the token, which 401s anyone else mid-run.
//!
//! # Litter
//!
//! Everything this creates at Gitea is named with the `knobas-` prefix
//! `live_gitea`'s `Litter` reserves, and [`Litter`] below deletes it whether the
//! test passes or panics. The prefix is not decoration: `litter_guard.rs` pins
//! that nothing the seed creates starts with it, which is what makes deleting
//! by prefix safe.

use std::sync::Arc;

use async_trait::async_trait;
use knobas_app::sources::{Registry, SourcesState};
use knobas_app::start_work;
use knobas_core::entity::EntityRef;
use knobas_core::start_work::{Step, StepOutcome};
use knobas_secrets::{MemoryStore, Secret, SecretStore};
use knobas_source::AuthMethod;
use knobas_sync::SyncTrigger;
use knobas_sync::scheduler::{RunConnections, Scheduler, SchedulerDeps, SyncEvents};
use serde_json::json;

/// The prefix `live_gitea`'s `Litter` reserves, and `litter_guard.rs` pins.
const LITTER: &str = "knobas-";

/// The mockd issue this flow starts from.
///
/// `PAY-240`, and the choice is load-bearing: the fixture puts it in **To Do**,
/// whose only transition is to In Progress -- which is the one this flow makes.
/// `PAY-231`, the issue most of this workspace's tests use, is already *In
/// Progress*, and Jira does not offer a transition to the status an issue is
/// already in, so the flow's last step would be refused by name. That refusal
/// is correct behaviour (the status is resolved against what the source says is
/// reachable, never assumed) and it is not the round trip this criterion is
/// about.
const ISSUE: &str = "PAY-240";

const JIRA: &str = "jira";
const GITEA: &str = "gitea";

// -- the environment --------------------------------------------------------

struct Env {
    url: String,
    token: String,
    owner: String,
    repo: String,
}

fn env() -> Env {
    let need = |name: &str| {
        std::env::var(name).unwrap_or_else(|_| {
            panic!("{name} is not set -- run `eval \"$(testenv/seed --env)\"` first")
        })
    };
    Env {
        url: need("KNOBAS_GITEA_URL").trim_end_matches('/').to_owned(),
        token: need("KNOBAS_GITEA_TOKEN"),
        owner: std::env::var("KNOBAS_GITEA_OWNER").unwrap_or_else(|_| "tidewater".to_owned()),
        repo: std::env::var("KNOBAS_GITEA_REPO").unwrap_or_else(|_| "payout-service".to_owned()),
    }
}

impl Env {
    fn full_name(&self) -> String {
        format!("{}/{}", self.owner, self.repo)
    }

    async fn api(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> serde_json::Value {
        let mut request = reqwest::Client::new()
            .request(method, format!("{}/api/v1{path}", self.url))
            .header("Authorization", format!("token {}", self.token));
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.expect("Gitea answered");
        if response.status() == reqwest::StatusCode::NO_CONTENT {
            return json!(null);
        }
        response.json().await.unwrap_or(json!(null))
    }

    /// Every branch of the mutated repository, by name.
    async fn branches(&self) -> Vec<String> {
        self.api(
            reqwest::Method::GET,
            &format!("/repos/{}/branches?limit=50", self.full_name()),
            None,
        )
        .await
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|row| row["name"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
    }

    /// Every pull request, as `(number, head branch, merged)`.
    async fn pulls(&self) -> Vec<(u64, String, bool)> {
        self.api(
            reqwest::Method::GET,
            &format!("/repos/{}/pulls?state=all&limit=50", self.full_name()),
            None,
        )
        .await
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|row| {
                    Some((
                        row["number"].as_u64()?,
                        row["head"]["ref"].as_str()?.to_owned(),
                        row["merged"].as_bool().unwrap_or(false),
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
    }
}

/// Deletes what this test made at Gitea, pass or panic.
///
/// The pull requests first: Gitea refuses to delete a branch an open pull
/// request points at, and a `Drop` that gave up half way would leave residue
/// for the next run to inherit -- which is the failure `litter_guard.rs` exists
/// around.
struct Litter {
    branch: String,
}

impl Drop for Litter {
    fn drop(&mut self) {
        assert!(
            self.branch.starts_with(LITTER),
            "this guard deletes by prefix, so it may only ever be given a {LITTER:?} name"
        );
        let env = env();
        let branch = self.branch.clone();
        // A blocking client in its own thread: `Drop` cannot be async, and the
        // test's runtime may already be winding down.
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("a runtime for the cleanup");
            rt.block_on(async {
                for (number, head, _) in env.pulls().await {
                    if head == branch {
                        env.api(
                            reqwest::Method::PATCH,
                            &format!("/repos/{}/pulls/{number}", env.full_name()),
                            Some(json!({ "state": "closed" })),
                        )
                        .await;
                    }
                }
                env.api(
                    reqwest::Method::DELETE,
                    &format!("/repos/{}/branches/{branch}", env.full_name()),
                    None,
                )
                .await;
            });
        })
        .join()
        .expect("the cleanup thread");
    }
}

// -- the app, wired the way the app wires it --------------------------------

/// Connections a run gets: the scratch database's own, not the shared one's.
struct Connections(knobas_db::embedded::Connector);

#[async_trait]
impl RunConnections for Connections {
    async fn open(&self) -> Result<sqlx::PgConnection, sqlx::Error> {
        self.0.connect().await
    }
}

/// Events nobody is listening for. The scheduler reports; there is no window.
struct Quiet;

impl SyncEvents for Quiet {
    fn sync_state(&self, _status: knobas_sync::SourceSyncStatus) {}
    fn source_health(&self, _health: knobas_sync::CredentialHealth) {}
    fn activity_new(&self, _row: knobas_core::activity::ActivityRow) {}
}

/// A `SourcesState` over a database of this test's own, with both sources
/// configured and their credentials in place.
async fn app(env: &Env, jira_url: &str) -> SourcesState {
    let connector = knobas_db::test_util::scratch_database("start_work_live").await;
    let pool = connector
        .pool(4)
        .await
        .expect("a pool onto the scratch database");

    let secrets = Arc::new(MemoryStore::new());
    secrets
        .put(
            GITEA,
            &Secret {
                kind: AuthMethod::Pat,
                value: env.token.clone(),
            },
        )
        .expect("the Gitea token is stored");
    secrets
        .put(
            JIRA,
            &Secret {
                kind: AuthMethod::Pat,
                value: knobas_mockd::JIRA_TOKEN.to_owned(),
            },
        )
        .expect("the Jira token is stored");

    for (id, kind, base_url, config) in [
        (
            GITEA,
            "gitea",
            env.url.clone(),
            json!({ "repos": [env.full_name()] }),
        ),
        (JIRA, "jira", jira_url.to_owned(), json!({})),
    ] {
        knobas_sync::config::insert(
            &pool,
            &knobas_sync::config::InsertConfig {
                id: id.to_owned(),
                adapter_kind: kind.to_owned(),
                display_name: id.to_owned(),
                base_url,
                auth_kind: knobas_sync::config::AuthKind::Method(AuthMethod::Pat),
                config,
                sync_interval_secs: 86_400,
                enabled: true,
            },
        )
        .await
        .expect("the source row is written");
    }

    let scheduler = Scheduler::start(SchedulerDeps {
        pool: pool.clone(),
        connections: Arc::new(Connections(connector)),
        registry: Arc::new(Registry::builtin()),
        secrets: secrets.clone(),
        events: Arc::new(Quiet),
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

/// What the configured sources declare about their own payloads (#277) --
/// resolved from the running binary's registry, so the merged flag this pass
/// reads is at the path `knobas_source_gitea` says it is.
async fn declared_paths(state: &SourcesState) -> knobas_core::payload::Declarations {
    knobas_app::sources::paths::declared_paths(&state.pool, state.registry.as_ref())
        .await
        .expect("what the configured sources declare")
}

/// Sync one source and wait for the run to end.
async fn sync(state: &SourcesState, source: &str) {
    let (done, wait) = tokio::sync::oneshot::channel();
    let sink = Arc::new(Ending {
        done: std::sync::Mutex::new(Some(done)),
    });
    state
        .scheduler
        .trigger(source, SyncTrigger::Manual, Some(sink))
        .await
        .expect("the run starts");
    wait.await.expect("the run reports its ending");
}

struct Ending {
    done: std::sync::Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
}

impl knobas_sync::progress::ProgressSink for Ending {
    fn report(&self, progress: knobas_sync::progress::SyncProgress) {
        use knobas_sync::progress::SyncPhase;
        if !matches!(progress.phase, SyncPhase::Finished | SyncPhase::Failed) {
            return;
        }
        if let Some(sender) = self
            .done
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        {
            let _ = sender.send(());
        }
    }
}

/// The issue's status, read from mockd rather than from knobas' own mirror --
/// the criterion is what the *source* holds.
async fn status_at_jira(base_url: &str) -> String {
    let body: serde_json::Value = reqwest::Client::new()
        .get(format!("{base_url}/rest/api/2/issue/{ISSUE}"))
        .header("Accept", "application/json")
        .header(
            "Authorization",
            format!("Bearer {}", knobas_mockd::JIRA_TOKEN),
        )
        .send()
        .await
        .expect("mockd answered")
        .json()
        .await
        .expect("an issue");
    body["fields"]["status"]["name"]
        .as_str()
        .expect("a status name")
        .to_owned()
}

/// Merge a pull request once Gitea says it can be, and answer what it said.
///
/// Gitea computes `mergeable` asynchronously and answers `"Please try again
/// later"` to a merge asked for before it has. Twenty attempts at 250ms is five
/// seconds, which is far longer than the local container takes and short enough
/// that a genuinely unmergeable pull request fails the test rather than hanging
/// it.
async fn merge_when_ready(env: &Env, number: u64) -> serde_json::Value {
    let mut last = json!(null);
    for _ in 0..20 {
        last = env
            .api(
                reqwest::Method::POST,
                &format!("/repos/{}/pulls/{number}/merge", env.full_name()),
                Some(json!({ "Do": "merge" })),
            )
            .await;
        if env
            .pulls()
            .await
            .iter()
            .any(|(n, _, merged)| *n == number && *merged)
        {
            return last;
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    last
}

/// Point a step's stored proposal at `value` for one field.
fn with(payload: &serde_json::Value, field: &str, value: &str) -> serde_json::Value {
    let mut payload = payload.clone();
    let tag = payload
        .as_object()
        .and_then(|map| map.keys().next().cloned())
        .expect("a serialized write op");
    payload[&tag][field] = json!(value);
    payload
}

// -- the criterion ----------------------------------------------------------

/// **Ticket → branch → pull request → link → In Progress, and back.**
///
/// One test rather than several, deliberately: the criterion is the *round
/// trip*, and a suite that split it would have each half pass over a state the
/// other half established, with the environment's one repository shared between
/// them. The assertions are numbered in the order the flow makes them true.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs testenv's seeded Gitea container -- see this file's header"]
async fn a_ticket_becomes_a_branch_a_pull_request_and_a_status_and_comes_back() {
    let env = env();
    let jira = knobas_mockd::spawn_mock_jira().await;
    let jira_url = jira.base_url();
    let state = app(&env, &jira_url).await;

    // The mirror has to hold the ticket and the repository before a flow can be
    // proposed from them: the branch name comes from the ticket's own title.
    sync(&state, JIRA).await;
    sync(&state, GITEA).await;

    let ticket = EntityRef::new(JIRA, ISSUE);
    let repo = EntityRef::new(GITEA, &env.full_name());
    assert_eq!(
        status_at_jira(&jira_url).await,
        "To Do",
        "the fixture issue must start where the flow's transition is reachable from"
    );

    // 1. The proposal, shown before anything happens.
    let flow = start_work::begin(&state.pool, &ticket, &repo)
        .await
        .expect("a flow is proposed");
    assert_eq!(flow.len(), Step::ALL.len());
    assert!(
        flow.iter().all(|step| step.outcome == StepOutcome::Pending),
        "proposing must dispatch nothing"
    );

    // 2. The reader edits the branch name -- which is story 4, and which is
    //    also what keeps everything this test creates inside the litter prefix.
    let branch = format!("{LITTER}i44-{}", std::process::id());
    let _litter = Litter {
        branch: branch.clone(),
    };
    for step in &flow {
        let payload = match step.step {
            Step::CreateBranch => with(&step.payload, "name", &branch),
            Step::CreatePullRequest => with(&step.payload, "head", &branch),
            _ => continue,
        };
        start_work::repropose(&state.pool, step.id, payload)
            .await
            .expect("the proposal is the reader's to change");
    }

    // 3. Run it.
    let steps = start_work::queue::Queue { state: &state };
    let flow = start_work::run(&state.pool, &steps, &ticket)
        .await
        .expect("the flow runs");
    for step in &flow {
        assert_eq!(
            step.outcome,
            StepOutcome::Succeeded,
            "the {} step did not finish: {:?}",
            step.step,
            step.detail
        );
    }

    // 4. **At Gitea**, not in knobas' opinion of Gitea.
    assert!(
        env.branches().await.contains(&branch),
        "the branch is not in Gitea's own listing"
    );
    let (number, _, _) = env
        .pulls()
        .await
        .into_iter()
        .find(|(_, head, _)| head == &branch)
        .expect("the pull request the flow opened is not in Gitea's own listing");

    // 5. The link, in knobas -- and it is a knobas link, never written to
    //    either source, which is what makes it survive whatever they record.
    let pr = EntityRef::new(GITEA, &format!("{}#{number}", env.full_name()));
    let entries = knobas_core::link::entries_of(&state.pool, &ticket)
        .await
        .expect("the ticket's links");
    assert!(
        entries
            .iter()
            .any(|entry| entry.other.entity_id == pr.to_string()),
        "the pull request is not linked to the ticket: {:?}",
        entries
            .iter()
            .map(|entry| &entry.other.entity_id)
            .collect::<Vec<_>>()
    );

    // 6. **At Jira**, through mockd's own workflow -- so the status was
    //    resolved against what the source said was reachable, not assumed.
    assert_eq!(
        status_at_jira(&jira_url).await,
        "In Progress",
        "the board still lies about what is being worked on"
    );

    // -- the reverse direction ---------------------------------------------

    // 7. Somebody does the work, takes it out of draft, and merges it.
    //
    //    A commit first: a pull request with no diff is not one Gitea will
    //    merge, and the reverse direction is about a pull request somebody
    //    actually finished.
    env.api(
        reqwest::Method::POST,
        &format!("/repos/{}/contents/{}.txt", env.full_name(), branch),
        Some(json!({
            "branch": branch,
            "content": "aTQ0Cg==",
            "message": format!("{branch}: the work"),
        })),
    )
    .await;

    //    The un-drafting is not ceremony: Gitea **refuses to merge** a pull
    //    request whose title carries a work-in-progress prefix, which is the
    //    live proof that `plan::DRAFT_PREFIX` really does open it as a draft.
    //    Story 8 is met by that prefix rather than by a `draft` flag, because
    //    `WriteOp::CreatePullRequest` has no such field and adding one would
    //    grow the SPI.
    let undrafted = env
        .api(
            reqwest::Method::PATCH,
            &format!("/repos/{}/pulls/{number}", env.full_name()),
            Some(json!({ "title": format!("knobas i44 {}", std::process::id()) })),
        )
        .await;
    assert!(
        !undrafted["title"]
            .as_str()
            .unwrap_or_default()
            .starts_with(start_work::plan::DRAFT_PREFIX.trim()),
        "the pull request is still a draft: {undrafted}"
    );

    //    Gitea computes mergeability in the background and answers *"Please try
    //    again later"* until it has. Polled rather than slept through, so a
    //    fast machine does not wait and a slow one does not flake.
    let merged = merge_when_ready(&env, number).await;
    assert!(
        env.pulls()
            .await
            .iter()
            .any(|(n, _, is_merged)| *n == number && *is_merged),
        "the merge did not take ({merged}), so there is nothing for the reverse \
         direction to see"
    );

    // 8. An ordinary sync is the trigger -- the mirror learns it is merged, and
    //    the pass reads the mirror rather than polling Gitea.
    sync(&state, GITEA).await;
    let moved = start_work::merge::follow_merges(
        &state.pool,
        &steps,
        start_work::plan::IN_REVIEW,
        &declared_paths(&state).await,
    )
    .await
    .expect("the pass runs");
    assert_eq!(moved, 1, "the merged pull request's ticket did not move");
    assert_eq!(
        status_at_jira(&jira_url).await,
        "In Review",
        "the ticket sat in In Progress until somebody noticed at standup"
    );

    // 9. And only once. A second pass over the same merged pull request must
    //    find nothing to do, or the ticket would be re-transitioned for as long
    //    as it stayed merged -- which is for ever.
    assert_eq!(
        start_work::merge::follow_merges(
            &state.pool,
            &steps,
            start_work::plan::IN_REVIEW,
            &declared_paths(&state).await,
        )
        .await
        .expect("the second pass runs"),
        0,
        "the same merge was followed twice"
    );

    state.scheduler.shutdown().await;
    jira.assert_no_violations();
}

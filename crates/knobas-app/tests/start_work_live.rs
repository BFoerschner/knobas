//! **The start-work round trip against a real Gitea**: ticket → branch → pull
//! request → link → In Progress, and back again when the pull request is
//! merged (issue #44).
//!
//! Every other test of this feature stops one side short of the real thing.
//! `tests/start_work.rs` drives the orchestrator against a fake dispatcher and
//! proves the *sequence* -- which ops, in what order, and what happens when one
//! fails -- with nothing at either end of it. This is the other half: the real
//! write queue, the real adapters, a real database, and a real Gitea in its
//! container. Nothing here asserts on a step's outcome alone; **every
//! assertion is against a source's own answer or a stored row**, because the
//! subject is the round trip, not the dispatch.
//!
//! # Half of it is a mock, and this file is not M2's exit certificate
//!
//! The **repository** side is the seeded container: the branch, the pull
//! request, the draft prefix in Gitea's own copy of the title, and the merge
//! are decided by a server. The **ticket** side is
//! `knobas_mockd::spawn_mock_jira()`, in process, and ADR-0013 says no mock is
//! a witness for an acceptance or exit criterion -- so the two Jira
//! transitions here are asserted against mockd's workflow and witness nothing.
//! M2 exit criterion 1 is met end to end only once the ticket side is the
//! seeded Jira `just atlassian-live` stands up, and that is not this file.
//! `just start-work-live`'s header says the same thing at more length.
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
//! # Litter, and the budget it is spending
//!
//! Everything this creates at Gitea is named with the `knobas-` prefix
//! `live_gitea`'s `Litter` reserves, and [`Litter`] below deletes it whether the
//! test passes or panics. The prefix is not decoration: `litter_guard.rs` pins
//! that nothing the seed creates starts with it, which is what makes deleting
//! by prefix safe.
//!
//! **This file writes to the same `tidewater/payout-service` that
//! `crates/knobas-source-gitea/tests/live_gitea_capped.rs` measures**, and that
//! suite's `HEADROOM` is a budget of 19 records per listing: every branch's
//! commit walk, the branch listing, the pull listing. The default branch is one
//! of the branches it walks and the seed leaves it at exactly 19 commits, so a
//! single commit added there turns `just gitea-live-capped` red -- in another
//! crate, on another day, for a reason whose cause is in this file.
//!
//! # The digest half (issue #389)
//!
//! The last assertion of the round trip is not about start-work at all: it is
//! M3.3's exit criterion, *"the digest is drawn from a day of real activity
//! across the seeded Gitea, TeamCity, Jira and Confluence"*. Jira and
//! Confluence are witnessed in `tests/atlassian_live.rs`; Gitea's route --
//! `sync.live_item` holding a pull request the source attributes to the
//! configured account -- had no live witness at all, because this is the only
//! suite in the tree where a real Gitea pull request is opened *by the account
//! knobas is configured as*. So the digest read joins the flow here rather
//! than getting a suite of its own, which could only have re-opened one.
//!
//! It adds no writes to Gitea: it reads `/user` once for the account and then
//! reads knobas' own database. The budget above is untouched.
//!
//! That is not a hypothetical: until issue #373 this test merged into the
//! default branch and left a merged pull request behind on every run, so each
//! run spent two of that budget and one of the pull listing's for good.
//! `DELETE /issues/{index}` reclaims the pull request; **nothing reclaims a
//! commit on the default branch** short of a force-push or `testenv/reset`. So
//! the flow merges into a scratch base branch of the run's own instead, and
//! [`Litter`] refuses to end a run that moved the default branch. Anything
//! added here that writes to Gitea should ask the same question first: what
//! listing does it grow, and who takes it back?

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

/// How long any one request to Gitea may take before it is a failure.
///
/// `reqwest` carries no default timeout, so a container that accepted the
/// connection and then went quiet used to stall a run for ever -- and in
/// [`Litter::drop`] with nothing on screen, because a `Drop` that has not
/// returned has not reported anything either. Ten seconds, matching
/// `live_env::REQUEST_BUDGET`, which `live_gitea`'s guard bounds its own
/// requests with for the same reason (issue #170).
///
/// **A bound on each request is the whole bound here**, where `live_env` also
/// carries a `CLEANUP_BUDGET` over the sequence. It can be: this guard makes a
/// fixed, small number of requests -- one pull listing, one delete per pull
/// request found, two branch deletes, and three re-reads, each listing costing
/// a page or two ([`Env::listing`]) -- so bounding each one bounds the `Drop`.
/// `live_env::Litter` sweeps an unknown number of leftovers, which is why the
/// sequence there needs a budget of its own.
const REQUEST_BUDGET: std::time::Duration = std::time::Duration::from_secs(10);

/// The mockd issue this flow starts from.
///
/// `PAY-240`, and the choice is load-bearing: the fixture puts it in **To Do**,
/// whose only transition is to In Progress -- which is the one this flow makes.
/// `PAY-231`, the issue most of this workspace's tests use, is already *In
/// Progress*, and Jira does not offer a transition to the status an issue is
/// already in, so the flow's last step would be refused by name. That refusal
/// is correct behaviour (the status is resolved against what the source says is
/// reachable, never assumed) and it is not the round trip this file is about.
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
        let mut request = reqwest::Client::builder()
            .timeout(REQUEST_BUDGET)
            .build()
            .expect("a bounded client")
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

    /// Every page of one listing, as raw records.
    ///
    /// Paged to the end rather than asked for with one big `limit`, because a
    /// server is free to answer fewer records than it was asked for -- the
    /// property `live_gitea_capped.rs` exists for, and the one
    /// [`Litter::opened`] refuses to assume away on the collecting side. A
    /// single page would put that blind spot straight back into the *checking*
    /// side: the deletes would go out and their confirmation would read a page
    /// that never held the residue, so the guard would pass over exactly what
    /// it exists to catch. Stops on an **empty** page rather than a short one,
    /// since a short page is what a capped server answers. Matches
    /// `live_env::listing`, for the reason its own comment gives.
    async fn listing(&self, path: &str, query: &[(&str, &str)]) -> Vec<serde_json::Value> {
        let mut all = Vec::new();
        for page in 1..=64 {
            let mut url = format!("{path}?limit=50&page={page}");
            for (key, value) in query {
                url.push_str(&format!("&{key}={value}"));
            }
            let rows = self.api(reqwest::Method::GET, &url, None).await;
            match rows.as_array() {
                Some(rows) if !rows.is_empty() => all.extend(rows.iter().cloned()),
                _ => return all,
            }
        }
        panic!("GET {path}: 64 pages and still not empty -- is `page` being honoured?");
    }

    /// Every branch of the mutated repository, by name.
    async fn branches(&self) -> Vec<String> {
        self.listing(&format!("/repos/{}/branches", self.full_name()), &[])
            .await
            .iter()
            .filter_map(|row| row["name"].as_str().map(str::to_owned))
            .collect()
    }

    /// Every pull request, as `(number, head branch, merged)`.
    async fn pulls(&self) -> Vec<(u64, String, bool)> {
        self.listing(
            &format!("/repos/{}/pulls", self.full_name()),
            &[("state", "all")],
        )
        .await
        .iter()
        .filter_map(|row| {
            Some((
                row["number"].as_u64()?,
                row["head"]["ref"].as_str()?.to_owned(),
                row["merged"].as_bool().unwrap_or(false),
            ))
        })
        .collect()
    }

    /// The repository's default branch, as Gitea's own record names it.
    ///
    /// Read from the server rather than assumed to be `main`, because it is the
    /// branch [`Litter`] pins unchanged and a guard that pinned the wrong
    /// branch would pass over exactly the residue it exists to catch.
    async fn default_branch(&self) -> String {
        self.api(
            reqwest::Method::GET,
            &format!("/repos/{}", self.full_name()),
            None,
        )
        .await["default_branch"]
            .as_str()
            .expect("Gitea named the repository's default branch")
            .to_owned()
    }

    /// The Gitea account this run's token belongs to, as the server names it.
    ///
    /// `GET /api/v1/user`, which is the call the adapter's own
    /// `test_connection` makes to fill `GiteaConfig::username` in at add time
    /// -- *"the Gitea account this token belongs to ... what `@me`-style
    /// filters match `sync.item.author` against"*. Asked of the **server**
    /// rather than read out of `seed-state.json` or out of the mirror row the
    /// digest assertion is about: the second would be circular (the identity
    /// under test taken from the field under test) and the first would pin the
    /// claim to a seed file instead of to the credential the flow really wrote
    /// with.
    async fn account(&self) -> String {
        let me = self.api(reqwest::Method::GET, "/user", None).await;
        me["login"]
            .as_str()
            .filter(|login| !login.trim().is_empty())
            .unwrap_or_else(|| panic!("GET /user named no login for this token: {me}"))
            .to_owned()
    }

    /// The commit a branch is at.
    ///
    /// Three answers, kept apart on purpose: `Ok(Some(sha))`, `Ok(None)` when
    /// Gitea says 404 and there really is no such branch, and `Err` when it
    /// said anything else. [`Litter`] accuses the run of having merged into the
    /// default branch whenever this is not the commit it recorded, and that
    /// accusation sends the next reader force-pushing a shared fixture -- so a
    /// 401 or a 500 must not be able to wear it. This is the one reader that
    /// looks at a status code, which is why it does not go through
    /// [`Env::api`].
    async fn head_of(&self, branch: &str) -> Result<Option<String>, String> {
        let url = format!(
            "{}/api/v1/repos/{}/branches/{branch}",
            self.url,
            self.full_name()
        );
        let response = reqwest::Client::builder()
            .timeout(REQUEST_BUDGET)
            .build()
            .expect("a bounded client")
            .get(&url)
            .header("Authorization", format!("token {}", self.token))
            .send()
            .await
            .map_err(|error| format!("GET {url}: {error}"))?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let status = response.status();
        if !status.is_success() {
            return Err(format!("GET {url}: {status}"));
        }
        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|error| format!("GET {url}: {error}"))?;
        body["commit"]["id"]
            .as_str()
            .map(|sha| Some(sha.to_owned()))
            .ok_or_else(|| format!("GET {url}: {status}, but no commit id in {body}"))
    }
}

/// Deletes what this test made at Gitea, pass or panic, and **checks the
/// listings agree it is gone**.
///
/// Three things happen in `Drop`, in this order, and the order is a decision.
///
/// 1. **The pull request, with `DELETE /repos/{owner}/{repo}/issues/{index}`.**
///    Closing it is not enough and never was: Gitea refuses to close a *merged*
///    pull request, and this flow merges, so every run before issue #373 left
///    one standing for ever. `DELETE` removes a merged one too -- Gitea 1.27
///    answers 204 -- and it is what takes the pull request out of the
///    `state=all` listing `live_gitea_capped.rs` counts.
/// 2. **The branches**, the flow's own and the scratch base it merged into.
///    After the pull request, because Gitea will not delete a branch an open
///    pull request points at -- which is the case on a run that failed before
///    the merge.
/// 3. **The check.** Neither branch left in the branch listing, none of the
///    pull request numbers collected in step 1 left in the pull listing, and
///    the default branch still at the commit it was at when this guard was
///    made. Numbers, because Gitea rewrites a deleted branch's `head.ref`
///    (see the comment on the collection below). The last of those is the whole of issue
///    #373: a merge commit on the default branch is the one piece of residue no
///    `DELETE` takes back (neither `issues/{index}` nor `branches/{name}`
///    rewrites history -- only a force-push or `testenv/reset` does), so this
///    guard's answer is to assert the merge never reached the default branch
///    rather than to undo one that did.
///
/// A `Drop` that gave up half way would leave residue for the next run to
/// inherit -- the failure `litter_guard.rs` exists around -- so the check
/// reports rather than trusting the calls it just made.
struct Litter {
    /// The flow's own branch: the pull request's head.
    branch: String,
    /// The scratch base branch the pull request merges into, so that the merge
    /// commit lands somewhere this guard can delete.
    base: String,
    /// The branch that must come out of this run untouched, by Gitea's name for
    /// it rather than by this file's guess at it.
    default_branch: String,
    /// The commit `default_branch` was at before the run wrote anything.
    default_head: String,
    /// The pull request the flow opened, once the test knows its number.
    ///
    /// **Told, not discovered.** The listing scan below is a fallback for a run
    /// that panicked before it got this far, and a fallback is all it can be: a
    /// server answering a short page would hand back no pull request, the guard
    /// would delete nothing, and its own check -- which looks for what the scan
    /// found -- would pass over the residue it exists to catch. A short page is
    /// exactly the property `live_gitea_capped.rs` certifies the adapter
    /// against, so it is not a hazard this file may assume away.
    opened: std::sync::Mutex<Option<u64>>,
}

impl Litter {
    /// Record the default branch and where it stands, before the run writes.
    ///
    /// Reads only: nothing is created here, so a guard exists from before the
    /// first write and covers a panic in any of them.
    async fn new(env: &Env, branch: String, base: String) -> Self {
        let default_branch = env.default_branch().await;
        let default_head = env
            .head_of(&default_branch)
            .await
            .expect("Gitea answered where the default branch stands")
            .expect("the default branch exists");
        Self {
            branch,
            base,
            default_branch,
            default_head,
            opened: std::sync::Mutex::new(None),
        }
    }

    /// Tell the guard which pull request the flow opened.
    fn opened(&self, number: u64) {
        *self
            .opened
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(number);
    }
}

impl Drop for Litter {
    fn drop(&mut self) {
        for name in [&self.branch, &self.base] {
            assert!(
                name.starts_with(LITTER),
                "this guard deletes by prefix, so it may only ever be given {LITTER:?} names, \
                 and it was given {name:?}"
            );
        }
        let env = env();
        let repo = env.full_name();
        let (branch, base) = (self.branch.clone(), self.base.clone());
        let names = [branch.clone(), base.clone()];
        let default_branch = self.default_branch.clone();
        let default_head = self.default_head.clone();
        let told = *self
            .opened
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // A blocking client in its own thread: `Drop` cannot be async, and the
        // test's runtime may already be winding down.
        let cleanup = std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("a runtime for the cleanup");
            rt.block_on(async move {
                // By number from here on, never by head ref: **Gitea rewrites
                // a pull request's `head.ref` to `refs/pull/{number}/head` once
                // the branch it pointed at is deleted**, so a check that
                // re-read the listing looking for `branch` would find nothing
                // whether the delete worked or not. Measured: with the delete
                // replaced by the old `PATCH state=closed`, which Gitea refuses
                // on a merged pull request, the by-head check passed over a
                // pull request that was still there.
                let [branch, _] = &names;
                let repo = env.full_name();
                // The number the test recorded, plus anything the listing still
                // shows on this branch -- the second for a run that panicked
                // before it could record one. See `Litter::opened` for why the
                // listing alone will not do.
                let mut opened: Vec<u64> = told.into_iter().collect();
                opened.extend(
                    env.pulls()
                        .await
                        .into_iter()
                        .filter(|(_, head, _)| head == branch)
                        .map(|(number, _, _)| number)
                        .filter(|number| Some(*number) != told),
                );
                for number in &opened {
                    env.api(
                        reqwest::Method::DELETE,
                        &format!("/repos/{repo}/issues/{number}"),
                        None,
                    )
                    .await;
                }
                for name in &names {
                    env.api(
                        reqwest::Method::DELETE,
                        &format!("/repos/{repo}/branches/{name}"),
                        None,
                    )
                    .await;
                }

                let mut failures = Vec::new();
                let still_listed: Vec<u64> = env
                    .pulls()
                    .await
                    .into_iter()
                    .map(|(number, _, _)| number)
                    .filter(|number| opened.contains(number))
                    .collect();
                if !still_listed.is_empty() {
                    failures.push(format!(
                        "pull request(s) {still_listed:?}, opened from {branch}, are still in the \
                         state=all listing that live_gitea_capped.rs counts"
                    ));
                }
                let listed = env.branches().await;
                let standing: Vec<&String> =
                    names.iter().filter(|name| listed.contains(name)).collect();
                if !standing.is_empty() {
                    failures.push(format!("branch(es) {standing:?} are still there"));
                }
                match env.head_of(&default_branch).await {
                    Ok(now) if now.as_deref() == Some(default_head.as_str()) => {}
                    Ok(now) => failures.push(format!(
                        "{default_branch} is at {now:?} and was at {default_head} when this run \
                         started, so something this run did was merged into the default branch -- \
                         which is residue no DELETE takes back, only a force-push or \
                         testenv/reset"
                    )),
                    // Not the accusation above: this says nothing about whether
                    // the default branch moved, and saying it did would send
                    // somebody force-pushing a repository nobody touched.
                    Err(error) => failures.push(format!(
                        "could not read where {default_branch} stands ({error}), so whether this \
                         run reached the default branch is unknown"
                    )),
                }
                failures
            })
        });
        // Never `expect` here. A panic inside the thread -- a request that
        // could not even be sent, say -- would panic this `Drop` too, and a
        // panic while already unwinding aborts the process, replacing a legible
        // test failure with a crash. The thread names its own cause on stderr;
        // this side only has to say what may still be standing.
        let failures = match cleanup.join() {
            Ok(failures) => failures,
            Err(_) => vec![format!(
                "the cleanup thread panicked (its own message is on stderr), so {branch:?}, \
                 {base:?} and any pull request between them may still be standing"
            )],
        };

        if failures.is_empty() {
            return;
        }
        let report = format!(
            "the start-work round trip did not leave {} as it found it, so the next run inherits \
             it and live_gitea_capped.rs's HEADROOM budget shrinks (issue #373): {}",
            repo,
            failures.join("; ")
        );
        // Panicking while already unwinding aborts the process, which would
        // replace a legible test failure with a crash. The test is already red
        // in that case; this only has to be visible.
        if std::thread::panicking() {
            eprintln!("start_work_live cleanup: {report}");
        } else {
            panic!("{report}");
        }
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
///
/// `account` is the Gitea login the token belongs to ([`Env::account`]), and
/// it goes into the Gitea source's `username`. That field is what
/// `knobas_search::Vocabulary::load` collects into the identity behind `@me`,
/// which is the `identity` the digest's mirror half matches `sync.live_item`'s
/// `author` against -- so without it this app knows of nobody, and the digest
/// assertion at the end of the round trip would be asserting an empty list is
/// empty. A real source has it filled in from *Test connection* at add time.
async fn app(env: &Env, jira_url: &str, account: &str) -> SourcesState {
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
            json!({ "repos": [env.full_name()], "username": account }),
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
/// what the *source* holds is the claim, not knobas' opinion of it. Mockd is
/// still a mock, so this witnesses the flow's reach and not the criterion
/// (ADR-0013, and this file's header).
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

/// One whole UTC day, as the digest's readers ask for it.
///
/// The same shape `tests/atlassian_live.rs`'s `day_window` has, and for the
/// same reason: which day it is where the reader sits is a fact only the
/// webview holds, so `standup_digest_inner` is handed the windows rather than
/// working them out (`crate::time::day`).
fn day_window(on: chrono::NaiveDate) -> knobas_app::time::week::DayWindow {
    knobas_app::time::week::DayWindow {
        day: on,
        from: on.and_hms_opt(0, 0, 0).expect("midnight").and_utc(),
        to: on
            .succ_opt()
            .expect("the next day")
            .and_hms_opt(0, 0, 0)
            .expect("midnight")
            .and_utc(),
    }
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

// -- the round trip ---------------------------------------------------------

/// **Ticket → branch → pull request → link → In Progress, and back -- and
/// then on the standup digest.**
///
/// One test rather than several, deliberately: the subject is the *round
/// trip*, and a suite that split it would have each half pass over a state the
/// other half established, with the environment's one repository shared between
/// them. The assertions are numbered in the order the flow makes them true.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs testenv's seeded Gitea container -- see this file's header"]
async fn a_ticket_becomes_a_branch_a_pull_request_and_a_status_and_comes_back() {
    let env = env();
    let jira = knobas_mockd::spawn_mock_jira().await;
    let jira_url = jira.base_url();
    let account = env.account().await;
    let state = app(&env, &jira_url, &account).await;

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

    // 2. The reader edits the branch name **and the base** -- which is story 4
    //    twice over, and which is also what keeps everything this test creates
    //    inside the litter prefix.
    //
    //    The base is what makes this file repeatable. Merged into
    //    `payout-service`'s *default* branch, each run left two commits there
    //    -- the work commit and the merge commit -- that no API call takes
    //    back, against a repository whose default-branch commit listing
    //    `live_gitea_capped.rs` counts against a budget of 19 (issue #373; see
    //    this file's header). So the flow is pointed at a scratch base branch
    //    of this run's own, cut from the default branch and deleted with
    //    everything else by [`Litter`].
    //
    //    Nothing the round trip certifies moves with it: Gitea decides the
    //    branch, the draft prefix in its own copy of the title, whether the
    //    merge is allowed and the merged flag the reverse direction reads,
    //    and it decides all four the same way for a pull request based on a
    //    branch as for one based on `main`. Reproposing the base is not a
    //    contrivance either -- `plan::subject` guesses it from the repository
    //    payload's `default_branch` and names "a base branch the user corrects
    //    in the review step" as the cost of guessing wrong, so this is that
    //    correction, on the real write path.
    let branch = format!("{LITTER}i44-{}", std::process::id());
    let base = format!("{branch}-base");
    let litter = Litter::new(&env, branch.clone(), base.clone()).await;
    let cut = env
        .api(
            reqwest::Method::POST,
            &format!("/repos/{}/branches", env.full_name()),
            Some(json!({
                "new_branch_name": base,
                "old_branch_name": litter.default_branch,
            })),
        )
        .await;
    assert_eq!(
        env.head_of(&base)
            .await
            .expect("Gitea answered where the scratch base branch stands")
            .as_deref(),
        Some(litter.default_head.as_str()),
        "the scratch base branch was not cut from {}, so the pull request below would be based \
         on nothing this run controls: {cut}",
        litter.default_branch,
    );
    //    What the **proposal** said the base was, asserted before it is edited
    //    away. `plan::subject` reads it from the mirrored repository payload's
    //    `default_branch`, and until this file reproposed the base that value
    //    went to Gitea on every run -- so the chain from Gitea's repository
    //    record through the adapter's stored payload to `subject`'s read was
    //    witnessed here, incidentally, by the branch creation succeeding.
    //    Reproposing the base ends that, and this is what takes its place:
    //    stated rather than incidental, and against Gitea's own name for the
    //    default branch rather than this file's guess at it.
    for step in &flow {
        let proposed = match step.step {
            Step::CreateBranch => &step.payload["CreateBranch"]["from_ref"],
            Step::CreatePullRequest => &step.payload["CreatePullRequest"]["base"],
            _ => continue,
        };
        assert_eq!(
            proposed.as_str(),
            Some(litter.default_branch.as_str()),
            "the {} step was proposed against {proposed}, not {:?}, which is what Gitea calls \
             this repository's default branch -- so what plan::subject read out of the mirror \
             is not the branch the server would have taken",
            step.step,
            litter.default_branch
        );
    }
    for step in &flow {
        let payload = match step.step {
            Step::CreateBranch => with(&with(&step.payload, "name", &branch), "from_ref", &base),
            Step::CreatePullRequest => with(&with(&step.payload, "head", &branch), "base", &base),
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
    //    Told to the guard here rather than rediscovered in `Drop`: see
    //    `Litter::opened`.
    litter.opened(number);

    //    **And it is a draft**, in Gitea's own copy of the title. Story 8 is
    //    met by `plan::DRAFT_PREFIX` rather than by a `draft` flag, because
    //    `WriteOp::CreatePullRequest` has no such field and adding one would
    //    grow the SPI -- so the prefix surviving proposal, repropose, the write
    //    queue, the adapter and the wire is the whole of the claim, and this is
    //    the only place it is on a real server.
    //
    //    Asserted **here**, before step 7 renames it. The un-drafting below
    //    used to carry this claim, and could not: it asserted that Gitea's echo
    //    of a title the test had just PATCHed in did not start with `WIP:`,
    //    which is true of a string the test composed itself, and stayed true
    //    with `DRAFT_PREFIX` deleted from `plan.rs` altogether (#347).
    let opened = env
        .api(
            reqwest::Method::GET,
            &format!("/repos/{}/pulls/{number}", env.full_name()),
            None,
        )
        .await;
    assert!(
        opened["title"]
            .as_str()
            .unwrap_or_default()
            .starts_with(start_work::plan::DRAFT_PREFIX.trim()),
        "the pull request the flow opened does not carry {:?}, so it would summon reviewers the \
         moment work started: {opened}",
        start_work::plan::DRAFT_PREFIX
    );

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
    //    request whose title carries a work-in-progress prefix, and step 4
    //    asserted that this one's does. So this PATCH is what makes the merge
    //    below reachable at all, and it is Gitea's refusal -- not this
    //    assertion -- that certifies the prefix was really read as a draft
    //    marker by the server.
    //
    //    What is asserted is therefore that the rename *took*: the title Gitea
    //    now holds is the one this call sent. The old assertion here was
    //    `!undrafted["title"].starts_with("WIP:")` over Gitea's echo of a
    //    title the test had just composed, which no implementation of
    //    `plan.rs` could make fail -- and `unwrap_or_default()` meant a PATCH
    //    that 404'd passed it too (#347).
    let renamed = format!("knobas i44 {}", std::process::id());
    let undrafted = env
        .api(
            reqwest::Method::PATCH,
            &format!("/repos/{}/pulls/{number}", env.full_name()),
            Some(json!({ "title": renamed })),
        )
        .await;
    assert_eq!(
        undrafted["title"].as_str(),
        Some(renamed.as_str()),
        "the rename that takes the pull request out of draft did not take, so the merge below \
         would be measuring Gitea's WIP refusal instead of the flow: {undrafted}"
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

    // 10. **And the standup digest lists it** (issue #389).
    //
    //     M3.3's exit criterion asks that the digest be drawn from a day of
    //     real activity across the seeded Gitea, TeamCity, Jira and Confluence.
    //     `tests/atlassian_live.rs` carries the Jira and Confluence halves;
    //     this is Gitea's, and it is here rather than in a suite of its own
    //     because this file is the only place a **real** pull request is opened
    //     by the account knobas is configured as. Nothing about it is
    //     synthetic: Gitea decided the number, the flow's own write queue
    //     opened it, `sync` above mirrored it, and the digest is read through
    //     the same `standup_digest_inner` seam the other live digest tests use.
    //
    //     The mirror row is asserted first, and separately. The digest reaches
    //     a line by two filters at once -- whose the item is, and whether the
    //     day is in the window -- so an empty list would otherwise be
    //     ambiguous between "the adapter attributed the pull request to
    //     somebody else" and "the window is wrong", and the first of those is
    //     the finding this ticket exists to make either way.
    let pr_id = pr.to_string();
    let (attributed_to, at): (Option<String>, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
        "select author, coalesce(item_updated_at, synced_at)
           from sync.live_item where entity_id = $1",
    )
    .bind(&pr_id)
    .fetch_one(&state.pool)
    .await
    .expect("the pull request this flow opened is in the mirror");
    assert_eq!(
        attributed_to.as_deref(),
        Some(account.as_str()),
        "the mirror attributes {pr_id} to {attributed_to:?} and the source is configured as \
         {account:?}, so the digest's mirror half -- `where i.author = any($1)`, matched \
         case-sensitively against the configured usernames -- cannot reach it"
    );

    //     Read for the day the mirror itself dates the pull request on, not
    //     for `today`: the two are the same on every ordinary run, and taking
    //     the day from the row is what stops a run that crosses midnight
    //     between the merge and this read from failing for the calendar rather
    //     than for the rule.
    let day = at.date_naive();
    let digest = knobas_app::commands::entity::standup_digest_inner(
        &state.pool,
        state.registry.as_ref(),
        chrono::Utc::now(),
        day_window(day),
        &[],
    )
    .await
    .expect("the digest reads");
    let listed: Vec<(Option<&str>, &str, Option<&str>, &str)> = digest
        .today
        .iter()
        .map(|line| {
            (
                line.entity_id.as_deref(),
                line.source.as_str(),
                line.kind.as_deref(),
                line.verb.as_str(),
            )
        })
        .collect();
    assert!(
        listed.contains(&(Some(pr_id.as_str()), GITEA, Some("pr"), "attributed")),
        "the pull request this flow opened is not on the digest for {day}, which is the day \
         the mirror dates it on: {listed:?}"
    );
    for line in &digest.today {
        assert!(
            !line.reason.trim().is_empty(),
            "a line whose provenance cannot be shown is not shippable: {line:?}"
        );
    }
    println!(
        "SEEDED digest for {day}: {} lines under today, including {pr_id}",
        digest.today.len()
    );

    state.scheduler.shutdown().await;
    jira.assert_no_violations();
}

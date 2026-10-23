//! **The start-work round trip against a real Gitea**: ticket → branch → pull
//! request → link → In Progress, and back again when the pull request is
//! merged (issue #44).
//!
//! Every other test of this feature stops one side short of the real thing.
//! `tests/it/start_work.rs` drives the orchestrator against a fake dispatcher and
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
//! # The `under:` witness (issue #644)
//!
//! The second test here is not about start-work: it is Gitea's app-level
//! `under:` witness, the only Gitea suite that reaches `knobas_app`'s search.
//! It writes nothing to Gitea, so the budget below is untouched by it.
//!
//! # Why this is `#[ignore]`d and `tests/it/start_work.rs` is not
//!
//! It needs Docker, a seeded Gitea and a token -- the treatment
//! `crates/knobas-source-gitea/tests/live_gitea.rs` gets, and for the same
//! reason. Run it with the environment up:
//!
//! ```text
//! cd testenv && docker compose --env-file images.env up -d --wait gitea && ./seed-gitea.sh && eval "$(./seed --env)"
//! cd .. && env -u RUSTUP_TOOLCHAIN cargo test -p knobas-app --test start_work_live \
//!     -- --ignored --nocapture --test-threads=1
//! ```
//!
//! **One environment, one owner at a time** -- `testenv/README.md`. The seed's
//! token is per worktree since #684, so seeding no longer 401s anyone; the
//! branches and pull requests below are still shared with every other run.
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

mod live_digest;
mod live_under;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use knobas_app::sources::{Registry, SourcesState};
use knobas_app::start_work;
use knobas_core::entity::EntityRef;
use knobas_core::start_work::{Step, StepOutcome};
use knobas_live_cleanup::Cleanup;
use knobas_secrets::{MemoryStore, Secret, SecretStore};
use knobas_source::AuthMethod;
use knobas_sync::scheduler::{Scheduler, SchedulerDeps};
use live_digest::{Connections, Quiet, day_window, on_digest, sync};
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

    /// Everything Gitea holds in one repository, as the entity ids the mirror
    /// should give them, by kind: the repository, every branch, every pull
    /// request of any state, and every commit on any branch -- the walk the
    /// adapter makes, asked of the server's own listings (#644).
    async fn everything_in(&self, full_name: &str) -> BTreeMap<String, BTreeSet<String>> {
        let id = |tail: &str| format!("{GITEA}:{full_name}{tail}");
        let branches: Vec<String> = self
            .listing(&format!("/repos/{full_name}/branches"), &[])
            .await
            .iter()
            .map(|row| row["name"].as_str().expect("a branch name").to_owned())
            .collect();
        let mut commits = BTreeSet::new();
        for branch in &branches {
            for row in self
                .listing(
                    &format!("/repos/{full_name}/commits"),
                    &[
                        ("sha", branch),
                        ("stat", "false"),
                        ("verification", "false"),
                    ],
                )
                .await
            {
                commits.insert(id(&format!("@{}", row["sha"].as_str().expect("a sha"))));
            }
        }
        let pulls = self
            .listing(&format!("/repos/{full_name}/pulls"), &[("state", "all")])
            .await
            .iter()
            .map(|row| id(&format!("#{}", row["number"].as_u64().expect("a number"))))
            .collect::<BTreeSet<_>>();
        let mut by_kind = BTreeMap::from([
            ("repo".to_owned(), BTreeSet::from([id("")])),
            (
                "branch".to_owned(),
                branches
                    .iter()
                    .map(|b| id(&format!("@refs/heads/{b}")))
                    .collect(),
            ),
            ("commit".to_owned(), commits),
            ("pr".to_owned(), pulls),
        ]);
        by_kind.retain(|_, ids| !ids.is_empty());
        by_kind
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
        Cleanup::new(
            format!(
                "the start-work round trip did not leave {repo} as it found it, so the next run \
                 inherits it and live_gitea_capped.rs's HEADROOM budget shrinks (issue #373)"
            ),
            format!("{branch:?}, {base:?} and any pull request between them may still be standing"),
        )
        .run(move || async move {
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
            if failures.is_empty() {
                Ok(())
            } else {
                Err(failures.join("; "))
            }
        });
    }
}

// -- the app, wired the way the app wires it --------------------------------

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
    app_timed(
        env,
        jira_url,
        account,
        knobas_sync::scheduler::SchedulerTiming::default(),
    )
    .await
}

/// [`app`] with the scheduler's clock chosen by the caller: a test that must
/// know **what** delivered a write holds the tick off.
async fn app_timed(
    env: &Env,
    jira_url: &str,
    account: &str,
    timing: knobas_sync::scheduler::SchedulerTiming,
) -> SourcesState {
    app_over(env, jira_url, account, &[env.full_name()], timing).await
}

/// [`app_timed`] with the Gitea source over `repos`.
async fn app_over(
    env: &Env,
    jira_url: &str,
    account: &str,
    repos: &[String],
    timing: knobas_sync::scheduler::SchedulerTiming,
) -> SourcesState {
    let connector = knobas_db::test_util::scratch_database("start_work_live").await;
    let pool = connector
        .pool(4)
        .await
        .expect("a pool onto the scratch database");

    let secrets = Arc::new(MemoryStore::new());
    secrets
        .put(
            &knobas_secrets::KeychainAccount::source(GITEA),
            &Secret::just(AuthMethod::Pat, env.token.clone()),
        )
        .expect("the Gitea token is stored");
    secrets
        .put(
            &knobas_secrets::KeychainAccount::source(JIRA),
            &Secret::just(AuthMethod::Pat, knobas_mockd::JIRA_TOKEN),
        )
        .expect("the Jira token is stored");

    for (id, kind, base_url, config) in [
        (
            GITEA,
            "gitea",
            env.url.clone(),
            json!({ "repos": repos, "username": account }),
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
        registry: Arc::new(Registry::for_process()),
        secrets: secrets.clone(),
        events: Arc::new(Quiet),
        timing,
    })
    .await
    .expect("a scheduler over the scratch database");

    SourcesState {
        pool,
        scheduler,
        secrets,
        registry: Arc::new(Registry::for_process()),
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
        // A clock **outside** the day being asked about, so no running timer of
        // this scratch database's own can join the list -- the precaution
        // `tests/atlassian_live.rs` states in as many words. The window is
        // half-open, so its own `to` is already outside it. There is no timer
        // here and nothing in this file starts one; the argument costs one
        // expression and the alternative is a list whose length depends on
        // something the fixture never wrote.
        day_window(day).to,
        day_window(day),
        &[],
    )
    .await
    .expect("the digest reads");
    on_digest(
        &digest.today,
        "the pull request this flow opened",
        day,
        &pr_id,
        GITEA,
        "pr",
        "attributed",
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
    // Every violation but the field-id discovery probe a source saved without
    // its Sprint and Epic Link ids makes at its next sync (#889): mockd,
    // frozen (ADR-0013), declares `GET /rest/api/2/field` and serves no
    // handler, and the adapter reads its 501 as "no field table".
    let unexpected: Vec<String> = jira
        .violations()
        .into_iter()
        .filter(|v| {
            !(v.kind == knobas_mockd::ViolationKind::Unimplemented
                && v.method == "GET"
                && v.path == "/rest/api/2/field")
        })
        .map(|v| {
            format!(
                "{:?} {} {}{}: {}",
                v.kind, v.method, v.path, v.query, v.detail
            )
        })
        .collect();
    assert!(
        unexpected.is_empty(),
        "jira: contract violation(s) beyond the field-table probe:\n  {}",
        unexpected.join("\n  ")
    );
}

/// **`under:` a repository, by its full name and by its bare name, finds it
/// and everything mirrored in it, through the search the app runs** (#644).
///
/// The source is configured with every repository `work.json` names, so the
/// three repositories' `under:` sets have to be told apart. What sits in a repository is asked of
/// **Gitea**, by the repository's own listings ([`Env::everything_in`]) --
/// not read off the mirror, and not off `repository` or `base.repo`, where the
/// descriptor declares the parent -- so a declaration that missed, or an
/// adapter that filed an item under the wrong repository, leaves this set full
/// and the search's wrong. Each fixture branch, pull request and commit of the
/// repository is checked to be in it (the last two by the index and sha
/// `seed-state.json` says Gitea and git assigned), so an empty server cannot
/// pass. Read-only at Gitea: it syncs into a database of its own, with its own
/// source configured here; the round trip above owns the litter, and runs
/// before this one.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs testenv's seeded Gitea container -- see this file's header"]
async fn under_each_seeded_repository_finds_it_and_its_branches_pulls_and_commits() {
    let env = env();
    let fixture = live_under::fixture();
    let seeded = live_under::seed_state("KNOBAS_GITEA_SEED_STATE");
    let pulls_seeded = live_under::rows(&seeded, "pull_requests");
    let repos: Vec<String> = live_under::rows(&fixture, "repos")
        .iter()
        .map(|r| r["name"].as_str().expect("a repo name").to_owned())
        .collect();
    assert_eq!(repos.len(), 3, "the fixture's three repositories");
    let full = |name: &str| format!("{}/{name}", env.owner);
    let state = gitea_app(&env, &repos.iter().map(|r| full(r)).collect::<Vec<_>>()).await;
    sync(&state, GITEA).await;

    for name in &repos {
        let full_name = full(name);
        let by_kind = env.everything_in(&full_name).await;
        let expected: BTreeSet<String> = by_kind.values().flatten().cloned().collect();
        assert!(
            expected.contains(&format!("{GITEA}:{full_name}")),
            "{full_name} itself is mirrored: {expected:?}"
        );
        for branch in live_under::rows(&fixture, "branches")
            .iter()
            .filter(|b| b["repo"] == name.as_str())
        {
            let id = format!(
                "{GITEA}:{full_name}@refs/heads/{}",
                branch["name"].as_str().expect("a branch name")
            );
            assert!(
                expected.contains(&id),
                "fixture branch {id} is mirrored: {expected:?}"
            );
        }
        for pr in live_under::rows(&fixture, "prs")
            .iter()
            .filter(|p| p["repo"] == name.as_str())
        {
            let index = &pulls_seeded
                .iter()
                .find(|s| s["fixture_number"] == pr["num"])
                .unwrap_or_else(|| panic!("the seed recorded fixture pull request {}", pr["num"]))
                ["real_index"];
            let id = format!("{GITEA}:{full_name}#{index}");
            assert!(
                expected.contains(&id),
                "fixture pull request {} ({id}) is mirrored: {expected:?}",
                pr["num"]
            );
        }
        for commit in live_under::rows(&seeded, "commits")
            .iter()
            .filter(|c| c["repo"] == name.as_str())
        {
            let id = format!(
                "{GITEA}:{full_name}@{}",
                commit["real_sha"].as_str().expect("a real sha")
            );
            assert!(
                expected.contains(&id),
                "fixture commit {} ({id}) is mirrored: {expected:?}",
                commit["fixture_sha"]
            );
        }

        live_under::each_exactly(&state.pool, GITEA, &[full_name, name.clone()], &by_kind).await;
    }
    state.scheduler.shutdown().await;
}

/// A Gitea issue on the Tickets board beside a Jira ticket (#1206, spec #1192
/// story 5): both sources synced through the app, and the board read the
/// Tickets page reads -- the sources that claim the ticket system -- drawing
/// a card of each, the Gitea one with the status, priority, assignee and
/// labels its declared paths lead to. Gitea over `ledger-api`, whose issues
/// the seed made (`seed-gitea.sh` section 7a) and no live test writes to;
/// read-only at both.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs testenv's seeded Gitea container -- see this file's header"]
async fn a_gitea_issue_is_on_the_tickets_board_beside_a_jira_ticket() {
    let env = env();
    let jira = knobas_mockd::spawn_mock_jira().await;
    let account = env.account().await;
    let ledger = format!("{}/ledger-api", env.owner);
    let state = app_over(
        &env,
        &jira.base_url(),
        &account,
        std::slice::from_ref(&ledger),
        knobas_sync::scheduler::SchedulerTiming::default(),
    )
    .await;
    sync(&state, JIRA).await;
    sync(&state, GITEA).await;

    // What Tickets reads: the sources whose claims name the ticket system,
    // and whose records filled it (#1199), which the board answers for the
    // role (#1299).
    let claimers: Vec<String> =
        knobas_app::sources::crud::list(&state.pool, state.registry.as_ref())
            .await
            .expect("the sources")
            .into_iter()
            .filter(|s| {
                s.roles.iter().any(|c| c.role == "ticket-system")
                    && !s.role_misses.iter().any(|m| m.role == "ticket-system")
            })
            .map(|s| s.id)
            .collect();
    assert_eq!(claimers, [GITEA, JIRA], "both fill the ticket system");

    let board = knobas_app::commands::entity::mini_board_inner(
        &state.pool,
        &knobas_app::sources::Registry::for_process(),
        None,
        Some(knobas_source::role::TICKET_SYSTEM),
        None,
        Some(500),
        None,
    )
    .await
    .expect("the Tickets board");
    let cards: Vec<(&Option<String>, &knobas_core::mini_board::MiniBoardCard)> = board
        .columns
        .iter()
        .flat_map(|c| c.cards.iter().map(move |card| (&c.status, card)))
        .collect();
    let (status, gitea) = cards
        .iter()
        .find(|(_, c)| c.title == "Ledger export rounds negative amounts")
        .unwrap_or_else(|| panic!("no Gitea card: {board:#?}"));
    assert_eq!(gitea.source_id, GITEA);
    assert!(
        gitea
            .entity_id
            .starts_with(&format!("{GITEA}:{ledger}/issues/")),
        "{}",
        gitea.entity_id
    );
    assert_eq!(status.as_deref(), Some("open"));
    assert_eq!(gitea.priority.as_deref(), Some("High"));
    assert_eq!(gitea.assignee.as_deref(), Some("tomasz.wojcik"));
    assert!(
        gitea.labels.iter().any(|l| l == "Kind/Bug"),
        "{:?}",
        gitea.labels
    );
    assert!(
        cards
            .iter()
            .any(|(_, c)| c.source_id == JIRA && c.entity_id == format!("{JIRA}:{ISSUE}")),
        "no Jira card beside it: {board:#?}"
    );
    state.scheduler.shutdown().await;
}

/// A `SourcesState` with only the Gitea source, over the given repositories.
async fn gitea_app(env: &Env, repos: &[String]) -> SourcesState {
    let connector = knobas_db::test_util::scratch_database("start_work_live_under").await;
    let pool = connector
        .pool(4)
        .await
        .expect("a pool onto the scratch database");
    // Bring-up's order (#627): the declarations before the first sync.
    live_under::declare(&pool).await;
    let secrets = Arc::new(MemoryStore::new());
    secrets
        .put(
            &knobas_secrets::KeychainAccount::source(GITEA),
            &Secret::just(AuthMethod::Pat, env.token.clone()),
        )
        .expect("the Gitea token is stored");
    knobas_sync::config::insert(
        &pool,
        &knobas_sync::config::InsertConfig {
            id: GITEA.to_owned(),
            adapter_kind: "gitea".to_owned(),
            display_name: GITEA.to_owned(),
            base_url: env.url.clone(),
            auth_kind: knobas_sync::config::AuthKind::Method(AuthMethod::Pat),
            config: json!({ "repos": repos }),
            sync_interval_secs: 86_400,
            enabled: true,
        },
    )
    .await
    .expect("the source row is written");
    let scheduler = Scheduler::start(SchedulerDeps {
        pool: pool.clone(),
        connections: Arc::new(Connections(connector)),
        registry: Arc::new(Registry::for_process()),
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
        registry: Arc::new(Registry::for_process()),
    }
}

// -- a write goes in the background (#752) ----------------------------------

/// Wait for the queue to settle write `id`, and answer the row as settled.
async fn settled(state: &SourcesState, id: i64) -> knobas_core::write_queue::QueuedWrite {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        let row = knobas_core::write_queue::get(&state.pool, id)
            .await
            .expect("the queue reads")
            .expect("the write is on the queue");
        if row.state != knobas_core::write_queue::WriteState::Pending {
            return row;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "write {id} was still pending after a minute: {row:?}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

/// The source's view of `entity`, as the detail reads it, flattened to text.
async fn view_text(state: &SourcesState, entity: &str) -> String {
    let view = knobas_app::sources::crud::view(
        &state.pool,
        &state.secrets,
        state.registry.as_ref(),
        entity,
        None,
    )
    .await
    .expect("the view reads");
    serde_json::to_string(&view).expect("a view serializes")
}

/// **`submit_write` answers on the queue, and the write lands behind it**
/// (issue #752), against the real Gitea: a comment and a merge each go
/// through the real queue and adapter, the call answers before the send, and
/// once the queue settles the comment is in the pull request's
/// *Conversation* and the merge at the server.
///
/// It also measures what the issue asks for, printed with `--nocapture`: the
/// time from *Submit* to the dialog closing, before and after, for Comment
/// and Merge. *Before* is the old dialog path -- the submit that waited for
/// the send, then the panel's two re-reads (`get_entity`, `entity_view`);
/// *after* is the queue insert alone. Two pull requests so each path merges
/// one, both into a scratch base branch so the default branch is never
/// written (#373); both `Litter` guards take everything back.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs testenv's seeded Gitea container -- see this file's header"]
async fn a_write_answers_on_queue_and_lands_behind_the_reader() {
    use std::time::Instant;

    let env = env();
    let account = env.account().await;
    // No Jira here: nothing below writes to or reads from it.
    let state = app(&env, "http://127.0.0.1:9", &account).await;

    let pid = std::process::id();
    let base = format!("{LITTER}i752-{pid}-base");
    let heads = [
        format!("{LITTER}i752-{pid}-a"),
        format!("{LITTER}i752-{pid}-b"),
    ];
    let litter_a = Litter::new(&env, heads[0].clone(), base.clone()).await;
    let litter_b = Litter::new(&env, heads[1].clone(), base.clone()).await;
    let repo = env.full_name();
    env.api(
        reqwest::Method::POST,
        &format!("/repos/{repo}/branches"),
        Some(json!({ "new_branch_name": base, "old_branch_name": litter_a.default_branch })),
    )
    .await;
    let mut entities = Vec::new();
    for (head, litter) in heads.iter().zip([&litter_a, &litter_b]) {
        env.api(
            reqwest::Method::POST,
            &format!("/repos/{repo}/branches"),
            Some(json!({ "new_branch_name": head, "old_branch_name": base })),
        )
        .await;
        env.api(
            reqwest::Method::POST,
            &format!("/repos/{repo}/contents/{head}.txt"),
            Some(json!({ "branch": head, "content": "aTQ0Cg==", "message": format!("{head}: the work") })),
        )
        .await;
        let opened = env
            .api(
                reqwest::Method::POST,
                &format!("/repos/{repo}/pulls"),
                Some(json!({ "head": head, "base": base, "title": format!("knobas i752 {head}") })),
            )
            .await;
        let number = opened["number"]
            .as_u64()
            .unwrap_or_else(|| panic!("Gitea opened no pull request from {head}: {opened}"));
        litter.opened(number);
        entities.push(format!("{GITEA}:{repo}#{number}"));
    }
    sync(&state, GITEA).await;
    let (before_pr, after_pr) = (&entities[0], &entities[1]);

    // The old dialog path, timed: the send waited for, then the two re-reads.
    let old_path = |payload: serde_json::Value, entity: String| {
        let state = &state;
        async move {
            let started = Instant::now();
            knobas_app::sources::write_queue::submit(state, payload)
                .await
                .expect("queued and sent");
            knobas_app::commands::entity::get_entity_inner(
                &state.pool,
                &Registry::for_process(),
                &entity,
            )
            .await
            .expect("the mirror reads");
            view_text(state, &entity).await;
            started.elapsed()
        }
    };

    // -- Comment ----------------------------------------------------------
    let comment_before = old_path(
        json!({ "Comment": { "entity": before_pr, "body": format!("before #752, {pid}") } }),
        before_pr.clone(),
    )
    .await;
    // The re-read the landed write set off, finished, so the next write is
    // not queued against a mirror about to move under it.
    sync(&state, GITEA).await;

    let words = format!("queued and sent behind the reader, {pid}");
    let started = Instant::now();
    let queued = knobas_app::sources::write_queue::submit_in_background(
        &state,
        json!({ "Comment": { "entity": after_pr, "body": words } }),
    )
    .await
    .expect("the comment is queued");
    let comment_after = started.elapsed();
    assert_eq!(
        queued.state,
        knobas_core::write_queue::WriteState::Pending,
        "the answer is the row as queued, before the send"
    );
    let row = settled(&state, queued.id).await;
    assert_eq!(
        row.state,
        knobas_core::write_queue::WriteState::Sent,
        "{row:?}"
    );
    assert!(
        view_text(&state, after_pr).await.contains(&words),
        "the comment sent in the background is not in the pull request's view"
    );
    sync(&state, GITEA).await;

    // -- Merge ------------------------------------------------------------
    let merge = |entity: &String| json!({ "MergePullRequest": { "entity": entity, "style": "merge", "delete_branch": false } });
    let merge_before = old_path(merge(before_pr), before_pr.clone()).await;
    sync(&state, GITEA).await;

    let started = Instant::now();
    let queued = knobas_app::sources::write_queue::submit_in_background(&state, merge(after_pr))
        .await
        .expect("the merge is queued");
    let merge_after = started.elapsed();
    let row = settled(&state, queued.id).await;
    assert_eq!(
        row.state,
        knobas_core::write_queue::WriteState::Sent,
        "{row:?}"
    );
    let merged: Vec<bool> = {
        let pulls = env.pulls().await;
        entities
            .iter()
            .map(|entity| {
                pulls
                    .iter()
                    .any(|(number, _, merged)| *merged && entity.ends_with(&format!("#{number}")))
            })
            .collect()
    };
    assert_eq!(
        merged,
        [true, true],
        "both pull requests are merged at Gitea"
    );

    eprintln!(
        "#752 Submit -> dialog closed: Comment {comment_before:?} before, {comment_after:?} after; \
         Merge {merge_before:?} before, {merge_after:?} after"
    );
}

// -- append-only writes hold only on a declared state change (#758) ----------

/// Open a pull request of the test's own, from a fresh head into a fresh
/// scratch base (never the default branch, #373), and hand back the guard that
/// takes both branches and the pull request back, plus the entity id.
async fn scratch_pull(env: &Env, name: &str) -> (Litter, String) {
    let pid = std::process::id();
    let base = format!("{LITTER}i758-{pid}-{name}-base");
    let head = format!("{LITTER}i758-{pid}-{name}-head");
    let litter = Litter::new(env, head.clone(), base.clone()).await;
    let repo = env.full_name();
    env.api(
        reqwest::Method::POST,
        &format!("/repos/{repo}/branches"),
        Some(json!({ "new_branch_name": base, "old_branch_name": litter.default_branch })),
    )
    .await;
    env.api(
        reqwest::Method::POST,
        &format!("/repos/{repo}/branches"),
        Some(json!({ "new_branch_name": head, "old_branch_name": base })),
    )
    .await;
    env.api(
        reqwest::Method::POST,
        &format!("/repos/{repo}/contents/{head}.txt"),
        Some(json!({ "branch": head, "content": "aTQ0Cg==", "message": format!("{head}: the work") })),
    )
    .await;
    let opened = env
        .api(
            reqwest::Method::POST,
            &format!("/repos/{repo}/pulls"),
            Some(json!({ "head": head, "base": base, "title": format!("knobas i758 {name}") })),
        )
        .await;
    let number = opened["number"]
        .as_u64()
        .unwrap_or_else(|| panic!("Gitea opened no pull request from {head}: {opened}"));
    litter.opened(number);
    (litter, format!("{GITEA}:{repo}#{number}"))
}

/// The tick held off for the whole test: it flushes the queue too, and a send
/// the tick made would witness nothing about `submit_in_background`'s flush.
fn tick_held_off() -> knobas_sync::scheduler::SchedulerTiming {
    knobas_sync::scheduler::SchedulerTiming {
        startup_delay: std::time::Duration::from_secs(3600),
        tick: std::time::Duration::from_secs(3600),
    }
}

fn comment_on(entity: &str, body: &str) -> serde_json::Value {
    json!({ "Comment": { "entity": entity, "body": body } })
}

/// **Two comments queued back to back on one pull request both reach Gitea,
/// in order, unheld** (issue #758; the live half of #757's fake-adapter test),
/// and **a merge between two comments holds the second** -- the one state
/// change an append-only write still holds on.
///
/// Both halves run through the real queue (`submit_in_background`), the real
/// Gitea adapter and the real mirror, on pull requests this test opens itself.
/// The tick is held off, so whatever delivers is the background flush.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs testenv's seeded Gitea container -- see this file's header"]
async fn back_to_back_comments_land_in_order_unheld_and_a_merge_between_holds_the_second() {
    use knobas_core::write_queue::WriteState;

    let env = env();
    let account = env.account().await;
    let state = app_timed(&env, "http://127.0.0.1:9", &account, tick_held_off()).await;
    let pid = std::process::id();

    let (_litter_a, back_to_back) = scratch_pull(&env, "a").await;
    let (_litter_b, merged_between) = scratch_pull(&env, "b").await;
    sync(&state, GITEA).await;

    // -- Back to back, no wait between the two submits ----------------------
    let (first_words, second_words) = (
        format!("first of two, back to back, {pid}"),
        format!("second of two, back to back, {pid}"),
    );
    let first = knobas_app::sources::write_queue::submit_in_background(
        &state,
        comment_on(&back_to_back, &first_words),
    )
    .await
    .expect("the first comment is queued");
    let second = knobas_app::sources::write_queue::submit_in_background(
        &state,
        comment_on(&back_to_back, &second_words),
    )
    .await
    .expect("the second comment is queued");
    let (first_row, second_row) = (
        settled(&state, first.id).await,
        settled(&state, second.id).await,
    );
    // The re-sync the first landing set off, finished: a hold on its echo
    // would be on the second write already, but the view below is read after.
    sync(&state, GITEA).await;
    assert_eq!(first_row.state, WriteState::Sent, "{first_row:?}");
    assert_eq!(
        second_row.state,
        WriteState::Sent,
        "the second comment was not sent: {second_row:?}"
    );
    let view = view_text(&state, &back_to_back).await;
    let (at_first, at_second) = (view.find(&first_words), view.find(&second_words));
    assert!(
        at_first.is_some() && at_second.is_some(),
        "both comments are in the pull request's Conversation at Gitea: {view}"
    );
    assert!(
        at_first < at_second,
        "the comments arrived out of order: {at_first:?} then {at_second:?}"
    );

    // -- And with the first one's echo in between ---------------------------
    // The pair above is sent by one flush, before any re-sync can land. So
    // here the second is queued as a row the moment the first has settled --
    // against a mirror the first's re-sync has not yet reached -- and tried
    // only after that re-sync has landed: the echo a whole-record hold
    // would trip on. (The tick is held off, so the flush below is the only
    // sender.) If the re-sync wins the race to the queueing, this proves less
    // and still passes; it never fails on the product's account.
    let (third_words, fourth_words) = (
        format!("third, whose echo lands first, {pid}"),
        format!("fourth, queued before that echo, {pid}"),
    );
    let third = knobas_app::sources::write_queue::submit_in_background(
        &state,
        comment_on(&back_to_back, &third_words),
    )
    .await
    .expect("the third comment is queued");
    assert_eq!(settled(&state, third.id).await.state, WriteState::Sent);
    let fourth_op: knobas_source::WriteOp =
        serde_json::from_value(comment_on(&back_to_back, &fourth_words)).expect("a write op");
    let fourth = knobas_sync::write_queue::queue(state.scheduler.deps(), GITEA, fourth_op)
        .await
        .expect("the fourth comment is queued");
    sync(&state, GITEA).await;
    knobas_sync::write_queue::flush_source(state.scheduler.deps(), GITEA)
        .await
        .expect("the flush runs");
    let fourth_row = settled(&state, fourth.id).await;
    assert_eq!(
        fourth_row.state,
        WriteState::Sent,
        "a comment held on the echo of the one before it: {fourth_row:?}"
    );
    let view = view_text(&state, &back_to_back).await;
    assert!(
        view.find(&third_words).is_some() && view.find(&third_words) < view.find(&fourth_words),
        "the third and fourth comments are not both there, in order: {view}"
    );

    // -- A merge between the queueing and the send holds the second --------
    // The first lands; the second is queued against an open pull request
    // (and not flushed: `queue` is the row alone); then the pull request is
    // merged at Gitea and the mirror catches up. Its flush must hold.
    let landed_words = format!("lands before the merge, {pid}");
    let landed = knobas_app::sources::write_queue::submit_in_background(
        &state,
        comment_on(&merged_between, &landed_words),
    )
    .await
    .expect("the comment is queued");
    assert_eq!(
        settled(&state, landed.id).await.state,
        WriteState::Sent,
        "the comment before the merge is sent"
    );
    sync(&state, GITEA).await;
    let held_words = format!("queued before the merge, sent after it, {pid}");
    let held_op: knobas_source::WriteOp =
        serde_json::from_value(comment_on(&merged_between, &held_words)).expect("a write op");
    let held = knobas_sync::write_queue::queue(state.scheduler.deps(), GITEA, held_op)
        .await
        .expect("the second comment is queued");
    assert_eq!(held.state, WriteState::Pending);

    let number: u64 = merged_between
        .rsplit('#')
        .next()
        .and_then(|n| n.parse().ok())
        .expect("a pull request number");
    // No pause before the merge, on purpose (#764). Gitea stamps `updated_at`
    // to the second, so the merge usually lands in the second the sync above
    // read this pull request at; the adapter's incremental run re-reads that
    // second and must see the merge by the record's fingerprint. With the
    // number-only boundary it did not, 3 runs in 4, and #758 had a 1.1 s pause
    // here to step around it.
    // Gitea computes mergeability on its own time, and a loaded machine can
    // outlast one call's five seconds: a few calls, and then say so if the
    // pull request is still open, rather than blame the mirror for it.
    let mut merged_at_gitea = false;
    for _ in 0..4 {
        merge_when_ready(&env, number).await;
        merged_at_gitea = env
            .pulls()
            .await
            .iter()
            .any(|(n, _, merged)| *n == number && *merged);
        if merged_at_gitea {
            break;
        }
    }
    assert!(merged_at_gitea, "Gitea never merged #{number}");
    // `trigger` joins a run already in flight, and the refresh the first
    // comment's landing set off may still be one that began before the merge:
    // so re-sync until the mirror itself says merged, rather than trust one.
    let mut mirrored = String::new();
    for _ in 0..20 {
        sync(&state, GITEA).await;
        mirrored = knobas_app::commands::entity::get_entity_inner(
            &state.pool,
            &Registry::for_process(),
            &merged_between,
        )
        .await
        .map(|entity| {
            entity
                .row
                .state
                .map(|chip| chip.text.to_string())
                .unwrap_or_default()
        })
        .unwrap_or_default();
        if mirrored == "merged" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    assert_eq!(
        mirrored, "merged",
        "the mirror never caught up with the merge"
    );
    knobas_sync::write_queue::flush_source(state.scheduler.deps(), GITEA)
        .await
        .expect("the flush runs");

    let row = settled(&state, held.id).await;
    assert_eq!(
        row.state,
        WriteState::Held,
        "a comment queued before the pull request was merged goes out anyway: {row:?}"
    );
    let view = view_text(&state, &merged_between).await;
    assert!(view.contains(&landed_words), "the first comment is there");
    assert!(
        !view.contains(&held_words),
        "the held comment reached Gitea: {view}"
    );
}

/// One file's blob sha and base64 text on `branch`, as Gitea's contents API
/// answers them (newlines stripped from the text; Gitea may wrap it).
async fn blob_on(env: &Env, branch: &str, path: &str) -> (String, String) {
    let record = env
        .api(
            reqwest::Method::GET,
            &format!("/repos/{}/contents/{path}?ref={branch}", env.full_name()),
            None,
        )
        .await;
    let sha = record["sha"]
        .as_str()
        .unwrap_or_else(|| panic!("no sha for {path} at {branch}: {record}"))
        .to_owned();
    let text = record["content"]
        .as_str()
        .unwrap_or_default()
        .replace('\n', "");
    (sha, text)
}

/// **A held `commit_file` against the real Gitea** (issue #888, the live half
/// of #822's "a stale `base_sha` held with both versions").
///
/// On a scratch branch the test reads `README.md`'s blob sha, changes the file
/// behind the queue's back, then queues a `CommitFile` against the old sha.
/// The row must be `held`: the reader's text in the payload, Gitea's new blob
/// sha (`base`) and text (`text`) in the held snapshot. *Apply anyway* then
/// sends it against their blob and the branch holds the reader's text.
///
/// Only the scratch branches are written; the [`Litter`] guard deletes both
/// and checks the default branch did not move. No pull request is opened.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs testenv's seeded Gitea container -- see this file's header"]
async fn a_commit_over_a_file_that_moved_is_held_with_both_versions_at_gitea() {
    use knobas_core::write_queue::WriteState;
    // base64 of "moved behind knobas back\n" and "the reader wrote this\n".
    const THEIRS_B64: &str = "bW92ZWQgYmVoaW5kIGtub2JhcyBiYWNrCg==";
    const THEIRS: &str = "moved behind knobas back\n";
    const MINE_B64: &str = "dGhlIHJlYWRlciB3cm90ZSB0aGlzCg==";
    const MINE: &str = "the reader wrote this\n";

    let env = env();
    let account = env.account().await;
    let state = app_timed(&env, "http://127.0.0.1:9", &account, tick_held_off()).await;
    let pid = std::process::id();
    let repo = env.full_name();
    let head = format!("{LITTER}i888-{pid}-head");
    let base = format!("{LITTER}i888-{pid}-base");
    let litter = Litter::new(&env, head.clone(), base.clone()).await;
    for (new, old) in [(&base, &litter.default_branch), (&head, &base)] {
        env.api(
            reqwest::Method::POST,
            &format!("/repos/{repo}/branches"),
            Some(json!({ "new_branch_name": new, "old_branch_name": old })),
        )
        .await;
    }

    // The reader looks: the blob the commit will be measured against.
    let (read_sha, _) = blob_on(&env, &head, "README.md").await;
    // Somebody commits behind the queue's back.
    env.api(
        reqwest::Method::PUT,
        &format!("/repos/{repo}/contents/README.md"),
        Some(json!({
            "branch": head, "content": THEIRS_B64, "sha": read_sha,
            "message": format!("{head}: moved behind the reader"),
        })),
    )
    .await;
    let (their_sha, their_text) = blob_on(&env, &head, "README.md").await;
    assert_ne!(their_sha, read_sha, "the file moved");
    assert_eq!(their_text, THEIRS_B64);

    let queued = knobas_app::sources::write_queue::submit_in_background(
        &state,
        json!({ "CommitFile": {
            "repo": format!("{GITEA}:{repo}"), "branch": head, "new_branch": null,
            "path": "README.md", "content": MINE,
            "message": format!("{head}: the reader's edit"), "base_sha": read_sha,
        } }),
    )
    .await
    .expect("the commit is queued");
    let row = settled(&state, queued.id).await;
    assert_eq!(row.state, WriteState::Held, "{row:?}");
    assert_eq!(
        row.payload["CommitFile"]["content"], MINE,
        "the reader's half"
    );
    assert_eq!(row.payload["CommitFile"]["base_sha"], read_sha.as_str());
    let theirs = row.held_snapshot.clone().expect("the held side");
    assert_eq!(theirs["base"], their_sha.as_str(), "{theirs}");
    assert_eq!(theirs["text"], THEIRS, "{theirs}");
    assert_eq!(
        blob_on(&env, &head, "README.md").await.0,
        their_sha,
        "a held write sent nothing"
    );

    knobas_sync::write_queue::apply_anyway(state.scheduler.deps(), queued.id)
        .await
        .expect("apply anyway runs")
        .expect("the write was held");
    let row = settled(&state, queued.id).await;
    assert_eq!(row.state, WriteState::Sent, "{row:?}");
    let (_, now) = blob_on(&env, &head, "README.md").await;
    assert_eq!(now, MINE_B64, "the branch holds the reader's text");
    drop(litter);
}

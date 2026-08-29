//! What both live suites need: where the seeded container is, and an adapter
//! pointed at it.
//!
//! # Two suites, two compose configurations, two properties
//!
//! Everything else in this crate runs against a wiremock stand-in, because
//! `just check` and CI must stay docker-free (roadmap §3). The live suites are
//! where that fake is measured against the server that decides the answers --
//! **if the two disagree, the fake is wrong.** There are two of them because
//! one compose configuration cannot express both properties:
//!
//! * [`live_gitea`] -- `just gitea-live`, `testenv/docker-compose.yml` as it
//!   stands. Certifies the *shapes* interfaces §4.2 fixes: the key forms, the
//!   `{ok,data}` envelope, `state=all`, the discussion living on the issue of
//!   the same index, `sort=recentupdate` really ordering newest-first, `since=`
//!   being server-side and inclusive, and a revoked token's 401.
//! * [`live_gitea_capped`] -- `just gitea-live-capped`, that file plus
//!   `testenv/docker-compose.capped.yml`. Certifies exactly one property, and
//!   one the default configuration structurally cannot: that a listing survives
//!   a server answering **fewer** records than the `limit` the adapter asked
//!   for. Every corpus the seed creates fits in one page of 50, so against the
//!   default file a short first page and an exhausted listing are the same
//!   answer.
//!
//! # Leaving the container as it was found
//!
//! `live_gitea` is not read-only: three of its tests open a branch through
//! Gitea's own API, two of them a pull request on top of it, and one of those
//! fifty-one comments. Until issue #143 nothing took any of it away, so every
//! run of `just gitea-live` ratcheted the seeded corpus up by three branches
//! and two pull requests and the next reader's remedy was `testenv/reset` --
//! which destroys every testenv volume, Uptime Kuma's included. [`Litter`] is
//! what removes it instead; the file header of `live_gitea.rs` states the
//! whole contract, including what happens on the failure path.
//!
//! [`live_gitea`]: ../live_gitea.rs
//! [`live_gitea_capped`]: ../live_gitea_capped.rs

// Compiled separately into each live test binary, and each uses a different
// part of it -- so without this, `clippy --all-targets -- -D warnings` fails on
// whatever one of them happens not to call.
#![allow(dead_code)]

use knobas_source::contract::VecSink;
use knobas_source::instance::SourceInstance;
use knobas_source::{AuthMethod, Source, SyncItem};

/// Where the seeded container is and what to read in it.
///
/// Read from the environment rather than hardcoded so whatever names testenv
/// settles on work without a code change here; `testenv/seed --env` prints
/// exactly these.
pub struct Env {
    pub url: String,
    pub token: String,
    pub owner: String,
    pub repo: String,
}

pub fn env() -> Env {
    let need = |key: &str| {
        std::env::var(key).unwrap_or_else(|_| {
            panic!(
                "{key} is not set -- start testenv's Gitea and seed it first, \
                 then `eval \"$(cd testenv && ./seed --env)\"` (or run `just gitea-live`)"
            )
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
    pub fn full_name(&self) -> String {
        format!("{}/{}", self.owner, self.repo)
    }

    /// An adapter over this container, configured as `config` says.
    pub fn source(&self, config: serde_json::Value) -> Box<dyn Source> {
        self.source_with(&self.token, config)
    }

    pub fn source_with(&self, token: &str, config: serde_json::Value) -> Box<dyn Source> {
        knobas_source_gitea::build(SourceInstance {
            id: "gitea".to_owned(),
            kind: "gitea".to_owned(),
            display_name: "Tidewater Git".to_owned(),
            base_url: self.url.clone(),
            auth: Some(AuthMethod::Pat),
            secret: Some(token.to_owned()),
            config,
        })
        .expect("the adapter builds")
    }

    /// Scoped to the one seeded repository these assertions are written for.
    pub fn one_repo(&self) -> Box<dyn Source> {
        self.source(serde_json::json!({ "repos": [self.full_name()] }))
    }

    /// Scoped to every repository of the seeded organisation, which is what
    /// makes the run walk the repository *listing* rather than name its
    /// members.
    pub fn whole_owner(&self) -> Box<dyn Source> {
        self.source(serde_json::json!({ "owners": [self.owner.clone()] }))
    }

    /// The seeded repositories **no live test writes to** -- the scope for an
    /// assertion whose antecedent is "when nothing changed" (issue #146).
    ///
    /// Only [`Env::repo`] is ever mutated: the three tests that create a
    /// branch, a pull request or a commit all do it there. So these two are
    /// quiescent by construction from the end of the seed onwards, and an
    /// idle run against them is idle for a structural reason rather than a
    /// timing one. Both of the other seeded repositories are named rather than
    /// one, deliberately: `ledger-api` carries the fixture's merged pull
    /// request and `ops-runbooks` has nothing but its default branch, and a
    /// battery should chew on both shapes.
    ///
    /// Named through `owner` rather than spelled whole, and checked against
    /// [`Env::repo`], so pointing the suite at another repository through
    /// `KNOBAS_GITEA_REPO` cannot silently make the quiet scope the mutated
    /// one.
    pub fn quiet_repos(&self) -> Vec<String> {
        let quiet: Vec<String> = ["ledger-api", "ops-runbooks"]
            .iter()
            .map(|name| format!("{}/{}", self.owner, name))
            .collect();
        assert!(
            !quiet.contains(&self.full_name()),
            "KNOBAS_GITEA_REPO is {}, which is one of the repositories this suite counts on \
             nothing writing to ({quiet:?}). Point it at the repository the mutating tests are \
             written for, or teach `quiet_repos` about the new one.",
            self.full_name()
        );
        quiet
    }
}

pub async fn full(source: &dyn Source) -> (Vec<SyncItem>, String) {
    let mut sink = VecSink(Vec::new());
    let cursor = source
        .sync(None, &mut sink)
        .await
        .expect("full sync against the container");
    (sink.0, cursor)
}

pub fn of_kind<'a>(items: &'a [SyncItem], kind: &str) -> Vec<&'a SyncItem> {
    items.iter().filter(|i| i.kind == kind).collect()
}

/// The prefix every branch the live suite creates carries.
///
/// Load-bearing twice: [`Litter::branch_off_main`] refuses a name without it,
/// and [`Litter::new`]'s sweep recognises an earlier run's leftovers by it.
/// Nothing `testenv/seed-gitea.sh` creates begins with it, so the sweep can
/// never take fixture content with it.
pub const LITTER: &str = "knobas-";

/// Everything one test creates in the seeded container, removed when that test
/// ends -- **including when it ends by panicking**.
///
/// Issue #143: before this, `just gitea-live` left a branch and a pull request
/// per test behind, the corpus ratcheted up every run, and the remedy the next
/// failure named was `testenv/reset`. Three properties make that not happen
/// again, and each one is worth stating because each is a decision:
///
/// 1. **Creation and cleanup are the same call.** A test does not create a
///    branch and then remember to register it; it asks this guard to create
///    one. There is no "forgot to track it" state to reach.
/// 2. **Cleanup runs from [`Drop`], so the failure path is the success path.**
///    A panicking test unwinds through here exactly as a passing one returns
///    through it. What it cannot survive is a process that never unwinds --
///    a `SIGKILL`, or a Ctrl-C at the wrong moment -- which is what 3 is for.
/// 3. **Every guard sweeps before it builds.** Whatever a killed run left
///    behind is removed by the next run's first mutating test, so recovery is
///    "run the suite again" rather than `testenv/reset`. The sweep matches on
///    [`LITTER`] and is why these tests must not run in parallel with each
///    other: it cannot tell a sibling's live branch from a corpse. The
///    `gitea-live` recipe passes `--test-threads=1`, which it already had to
///    for the shared server's sake.
///
/// The removal is checked rather than hoped for: [`remove`] re-reads the
/// listings afterwards and a branch or pull request still standing fails the
/// test. A cleanup that quietly stopped deleting is the whole defect, so it
/// may not be the one thing here that goes unasserted.
pub struct Litter {
    http: reqwest::Client,
    auth: String,
    /// `<url>/api/v1/repos/<owner>/<repo>` -- the mutated repository, and the
    /// only one this guard ever touches.
    api: String,
    branches: Vec<String>,
}

impl Litter {
    /// A guard over the repository the mutating tests write to, with anything
    /// an earlier run left behind already gone.
    pub async fn new(env: &Env) -> Litter {
        let litter = Litter {
            http: reqwest::Client::new(),
            auth: format!("token {}", env.token),
            api: format!("{}/api/v1/repos/{}", env.url, env.full_name()),
            branches: Vec::new(),
        };
        let stale: Vec<String> = branch_names(&litter.http, &litter.auth, &litter.api)
            .await
            .into_iter()
            .filter(|name| name.starts_with(LITTER))
            .collect();
        if !stale.is_empty() {
            println!(
                "live suite: sweeping {} leftover branch(es) from a run that was killed rather \
                 than failed: {stale:?}",
                stale.len()
            );
            let failures = remove(&litter.http, &litter.auth, &litter.api, &stale).await;
            assert!(
                failures.is_empty(),
                "the leftovers of an earlier run could not be swept, so this run would add to \
                 them (issue #143): {}",
                failures.join("; ")
            );
        }
        litter
    }

    /// Create a branch off `main` through Gitea's own API, and own its removal
    /// from this line onwards.
    ///
    /// Recorded *before* the request goes out, so a create that half-succeeded
    /// -- the branch made, the response lost -- is still cleaned up.
    pub async fn branch_off_main(&mut self, name: &str) {
        assert!(
            name.starts_with(LITTER),
            "every branch this suite creates must start with {LITTER:?} so the sweep in \
             Litter::new can recognise it: {name:?}"
        );
        self.branches.push(name.to_owned());
        let created = self
            .http
            .post(format!("{}/branches", self.api))
            .header("Authorization", &self.auth)
            .json(&serde_json::json!({ "new_branch_name": name, "old_branch_name": "main" }))
            .send()
            .await
            .expect("create the branch");
        assert!(
            created.status().is_success(),
            "branch {name}: {}",
            created.text().await.unwrap_or_default()
        );
    }
}

impl Drop for Litter {
    fn drop(&mut self) {
        let branches = std::mem::take(&mut self.branches);
        if branches.is_empty() {
            return;
        }
        let (auth, api) = (self.auth.clone(), self.api.clone());
        // `Drop` cannot await, and this one runs on a tokio worker thread, so
        // it cannot block on the current runtime either. A thread with a
        // runtime of its own can do both; joining it keeps the cleanup ordered
        // before the next test starts, which is what the sweep in
        // `Litter::new` counts on.
        //
        // The client is **built inside that runtime** rather than cloned from
        // `self`: a `reqwest::Client`'s connections are registered with the
        // reactor of whichever runtime created them, and driving one from a
        // second runtime hangs rather than failing (measured: the first test
        // never returned).
        let outcome = std::thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("a runtime for the cleanup")
                .block_on(
                    async move { remove(&reqwest::Client::new(), &auth, &api, &branches).await },
                )
        })
        .join();
        let failures = match outcome {
            Ok(failures) => failures,
            Err(_) => vec!["the cleanup thread panicked".to_owned()],
        };
        if failures.is_empty() {
            return;
        }
        let report = format!(
            "the live suite did not remove everything it created, so the next run inherits it \
             and the corpus ratchets (issue #143): {}",
            failures.join("; ")
        );
        // Panicking while already unwinding aborts the process, which would
        // replace a legible test failure with a crash. The test is already red
        // in that case; this only has to be visible.
        if std::thread::panicking() {
            eprintln!("live suite cleanup: {report}");
        } else {
            panic!("{report}");
        }
    }
}

/// Delete every pull request opened from `branches`, then the branches, then
/// **check the listings agree they are gone**.
///
/// Returns one line per thing still standing, and an empty vector when the
/// repository is back to the shape it had before these branches existed.
/// `DELETE /repos/{owner}/{repo}/issues/{index}` is what removes a pull
/// request and its whole discussion in one call; Gitea 1.27 answers 204.
async fn remove(http: &reqwest::Client, auth: &str, api: &str, branches: &[String]) -> Vec<String> {
    let mut tried = Vec::new();
    for (number, head) in pulls(http, auth, api).await {
        if branches.contains(&head) {
            tried.push(format!(
                "DELETE issues/{number} (from {head}) -> {}",
                delete(http, auth, &format!("{api}/issues/{number}")).await
            ));
        }
    }
    for branch in branches {
        tried.push(format!(
            "DELETE branches/{branch} -> {}",
            delete(http, auth, &format!("{api}/branches/{branch}")).await
        ));
    }

    let mut failures = Vec::new();
    let left: Vec<u64> = pulls(http, auth, api)
        .await
        .into_iter()
        .filter(|(_, head)| branches.contains(head))
        .map(|(number, _)| number)
        .collect();
    if !left.is_empty() {
        failures.push(format!("pull request(s) {left:?} are still there"));
    }
    let names = branch_names(http, auth, api).await;
    let standing: Vec<&String> = branches.iter().filter(|b| names.contains(b)).collect();
    if !standing.is_empty() {
        failures.push(format!("branch(es) {standing:?} are still there"));
    }
    if !failures.is_empty() {
        failures.push(format!("what was attempted: {}", tried.join(", ")));
    }
    failures
}

async fn delete(http: &reqwest::Client, auth: &str, url: &str) -> String {
    match http.delete(url).header("Authorization", auth).send().await {
        Ok(response) => response.status().to_string(),
        Err(error) => format!("{error}"),
    }
}

/// Every page of one listing, as raw records. Paged to the end rather than
/// asked with one big `limit`, because the server is free to answer fewer than
/// it was asked for -- the property `live_gitea_capped.rs` exists for.
async fn listing(http: &reqwest::Client, auth: &str, url: &str) -> Vec<serde_json::Value> {
    let mut all = Vec::new();
    for page in 1..=64 {
        let response = http
            .get(url)
            .header("Authorization", auth)
            .query(&[("limit", "50".to_owned()), ("page", page.to_string())])
            .send()
            .await
            .unwrap_or_else(|error| panic!("GET {url}: {error}"));
        let status = response.status();
        let rows: Vec<serde_json::Value> = response
            .json()
            .await
            .unwrap_or_else(|error| panic!("GET {url} -> {status}: {error}"));
        if rows.is_empty() {
            return all;
        }
        all.extend(rows);
    }
    panic!("GET {url}: 64 pages and still not empty -- is `page` being honoured?");
}

/// Every pull request of the repository, as `(number, head branch)`.
async fn pulls(http: &reqwest::Client, auth: &str, api: &str) -> Vec<(u64, String)> {
    listing(http, auth, &format!("{api}/pulls?state=all"))
        .await
        .iter()
        .filter_map(|p| Some((p["number"].as_u64()?, p["head"]["ref"].as_str()?.to_owned())))
        .collect()
}

async fn branch_names(http: &reqwest::Client, auth: &str, api: &str) -> Vec<String> {
    listing(http, auth, &format!("{api}/branches"))
        .await
        .iter()
        .filter_map(|b| b["name"].as_str().map(str::to_owned))
        .collect()
}

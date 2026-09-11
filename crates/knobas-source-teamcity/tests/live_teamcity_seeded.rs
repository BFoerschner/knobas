//! The adapter against the **seeded, self-hosted** TeamCity from
//! `testenv/seed --teamcity` -- a server we own (issue #266).
//!
//! `tests/live_teamcity.rs` points at JetBrains' public instance and, because
//! that corpus is somebody else's and changes between two requests, it is
//! read-only by rule and asserts by *form*. **Both rules invert here**, and the
//! inversion is the whole point: a corpus we seeded ourselves is one we can
//! hold a locator *to* -- not "the page is well-shaped" but "the build that
//! should be on it is on it". The first hour against this server found what
//! the public suite structurally could not: TeamCity's default filter narrows
//! every finished-state locator to the **default branch**, so the adapter's two
//! item-producing queries had never returned a feature-branch build, and the
//! fixture's own failed build 1187 on `feature/PAY-231-sepa-retry` was never
//! mirrored from a real TeamCity at all. `branch:default:any` is the fix, and
//! [`the_feature_branch_build_is_served_by_both_item_producing_locators`]
//! certifies it from both ends.
//!
//! `#[ignore]`d, so `just check` runs none of it:
//!
//! ```text
//! cd testenv
//! docker compose --profile real-teamcity up -d teamcity teamcity-agent
//! ./seed && ./seed --teamcity        # Gitea first: the VCS roots point at it
//! cd .. && just teamcity-live-seeded
//! ```
//!
//! # What this file asserts that nothing else can
//!
//! * The seeded content **by content**: the two projects and three
//!   configurations by the ids `testenv/seed-teamcity-builds.sh` derives, and
//!   builds 412 and 1187 by number, state, status and branch -- with their ids
//!   looked up in `testenv/seed-state.json`, because a real server assigns
//!   ids and the seed cannot dictate them (testenv/README.md, *What the seed
//!   cannot reproduce*).
//! * **Contract battery clause 2** against a real server: an incremental run
//!   after no changes emits nothing and hands back the same cursor. The public
//!   suite deliberately does not run it -- builds finish there between any two
//!   requests -- and mockd runs it against a fake that agrees with the adapter
//!   by construction. Here the corpus is quiet because it is ours.
//! * **The `sinceBuild` watermark on a build this suite queues**: full run,
//!   idle run, one `POST /app/rest/buildQueue`, and the next run returns
//!   exactly that build and moves the position to it; the run after that is
//!   idle again; and once the build is deleted, a cursor pointing at it is
//!   refused as issue #91's replaced-server case, on a real 404.
//!
//! # What this file writes, and what it takes away
//!
//! It is not read-only. One test queues one build through TeamCity's own REST
//! API and waits for the one agent to run it; that build is **deleted again**
//! when the test ends, passing or panicking alike -- [`Queued`] does it from
//! `Drop`, on a thread with a runtime of its own, and checks afterwards that
//! the build is really gone rather than assuming so. What a *killed* run left
//! behind is cleared by the next run: [`Seeded::clear_leftovers`] removes
//! every build whose id is not in `seed-state.json`, canceling it first if it
//! is still queued or running, and every test that asserts an exact set calls
//! it before taking its baseline. Recovery from a dirty environment is "run
//! the suite again". After a green run the server holds exactly the seeded
//! builds -- the plain-seed state -- and nothing else.
//!
//! A queued build takes the configuration's next number (413 after a fresh
//! seed) and the counter is not wound back; that is harmless, because the seed
//! sets a counter only while the build with the fixture's number is absent.
//!
//! **One owner at a time.** The leftover clearing cannot tell a sibling's
//! build from a corpse, and clause 2 needs a server on which nothing is running:
//! testenv/README.md, *One environment, one owner at a time*. `./seed
//! --teamcity --running` and this suite are therefore mutually exclusive on
//! one environment, and the suite **refuses to start** while a seeded build is
//! in flight ([`Seeded::clear_leftovers`]) rather than failing three tests on
//! diffs that would not name the cause. A build somebody else queues mid-run
//! is the case no check can catch.
//!
//! # A red run here is never answered by running it again
//!
//! The rule this suite exists under: when the fake and the server disagree
//! the **fake** is wrong, and when the adapter and the server disagree the
//! adapter is. A failure here therefore names a defect at one of those two
//! ends, and a re-run is for capturing the emission, nothing else.
//!
//! # What the seed cannot reproduce, and so what is not asserted
//!
//! Timestamps are when the build actually ran, so `updated_at` is asserted to
//! be the record's own `finishDate` and after its `startDate`, never a value.
//! Every seeded build was queued through the seed's token, so its triggerer
//! is `knobas` rather than the fixture's `mara` or a VCS trigger; the
//! `author` assertion says so. The build **log** is not on the REST record
//! and the adapter does not read it: what a failed build mirrors is its
//! `statusText`, which a real server composes from the failing step
//! (`Exit code 1 (Step: …)`) rather than from the log's first line as mockd's
//! fixture transcription does. Personal builds cannot be seeded, so the
//! personal facet stays certified on the public instance only.

use std::path::PathBuf;
use std::time::Duration;

use knobas_source::contract::{Fault, VecSink, battery};
use knobas_source::instance::SourceInstance;
use knobas_source::{AuthMethod, Source, SourceError, SyncItem};

/// How long one HTTP exchange with the container may take (same reasoning as
/// `live_env::REQUEST_BUDGET` in the Gitea suite: a localhost container
/// answers in milliseconds, and a request that reaches ten seconds is never
/// coming back).
const REQUEST_BUDGET: Duration = Duration::from_secs(10);

/// How long a queued build may take to finish on the one agent. The step
/// itself runs for a second; the agent takes a build within about fifteen,
/// and a fresh agent that still has to clone the repository within a minute.
const BUILD_BUDGET: Duration = Duration::from_secs(180);

/// How long [`Queued`]'s `Drop` waits for its cleanup before reporting that it
/// did not finish rather than stalling the run.
const CLEANUP_BUDGET: Duration = Duration::from_secs(90);

/// The configurations the seed always leaves with a **finished** build: the
/// scope of the battery and of the incremental run from below everything.
/// `Payout_Build` is left out because the seed makes a build in it only under
/// `--running`, so its contents are not the plain seed's -- and the plain seed
/// is what this suite certifies ([`Seeded::clear_leftovers`] refuses anything
/// else).
const QUIET_CONFIGURATIONS: [&str; 2] = ["Ledger_Deploy_Staging", "Payout_IntegrationTests"];

/// The configuration the mutating test queues on. It prints one line and
/// exits 0, so the agent is done with it in seconds.
const QUICK_CONFIGURATION: &str = "Ledger_Deploy_Staging";

/// A branch in `Ledger_Deploy_Staging`'s branch specification that is not
/// its default, so the incremental query is witnessed on a feature branch --
/// the class the default filter hides.
const FEATURE_BRANCH: &str = "fix/PAY-228-partial-refund-drift";

/// The `teamcity` block of `testenv/seed-state.json`: what the seed actually
/// got from the server.
#[derive(Debug, Clone, serde::Deserialize)]
struct Seed {
    version: String,
    builds: Vec<SeededBuild>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct SeededBuild {
    fixture_number: u32,
    build_type: String,
    real_id: i64,
}

/// Where the seeded server is, how to talk to it, and what the seed put in it.
struct Seeded {
    url: String,
    token: String,
    http: reqwest::Client,
    seed: Seed,
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(REQUEST_BUDGET)
        .build()
        .expect("a reqwest client with a timeout")
}

/// Panics with the three commands to run rather than skipping: this suite is
/// only ever run by name, and a silent skip would read as a green
/// certification of nothing.
fn seeded() -> Seeded {
    let need = |key: &str| {
        std::env::var(key)
            .ok()
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| {
                panic!(
                    "{key} is not set -- start testenv's TeamCity and seed it first \
                     (`docker compose --profile real-teamcity up -d teamcity teamcity-agent`, \
                     `./seed`, `./seed --teamcity`), then `eval \"$(cd testenv && ./seed --env)\"` \
                     or run `just teamcity-live-seeded`"
                )
            })
    };
    let url = need("KNOBAS_TEAMCITY_URL").trim_end_matches('/').to_owned();
    let token = need("KNOBAS_TEAMCITY_TOKEN");
    let state = std::env::var("KNOBAS_TEAMCITY_SEED_STATE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testenv/seed-state.json")
        });
    let raw = std::fs::read_to_string(&state).unwrap_or_else(|e| {
        panic!(
            "{}: {e} -- `./seed --teamcity` writes the number-to-id map this suite reads \
             (or point KNOBAS_TEAMCITY_SEED_STATE at it)",
            state.display()
        )
    });
    let whole: serde_json::Value = serde_json::from_str(&raw).expect("seed-state.json is JSON");
    let seed: Seed = serde_json::from_value(whole["teamcity"].clone()).unwrap_or_else(|e| {
        panic!(
            "{}: no `teamcity` block with `version` and `builds` -- run `./seed --teamcity`: {e}",
            state.display()
        )
    });
    assert!(
        !seed.builds.is_empty(),
        "{}: the seed recorded no builds; `./seed --teamcity` did not finish",
        state.display()
    );
    Seeded {
        url,
        token,
        http: client(),
        seed,
    }
}

impl Seeded {
    /// One raw `GET`, as the adapter sends them.
    async fn get(&self, path_and_query: &str) -> (u16, serde_json::Value) {
        let response = self
            .http
            .get(format!("{}/{path_and_query}", self.url))
            .header("Accept", "application/json")
            .bearer_auth(&self.token)
            .send()
            .await
            .unwrap_or_else(|e| panic!("GET {path_and_query}: {e}"));
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        let json = serde_json::from_str(&body).unwrap_or(serde_json::Value::String(body));
        (status, json)
    }

    /// One `GET` at an **absolute** URL, with the credential the adapter is
    /// configured with -- the request a browser would make for the URL the
    /// mirror holds, minus the browser. `Seeded::get` cannot serve this: it
    /// joins onto `self.url`, and what is under test here is whether the
    /// whole URL the adapter composed reaches a page on the real server.
    async fn status_of(&self, url: &str) -> u16 {
        self.http
            .get(url)
            .bearer_auth(&self.token)
            .send()
            .await
            .unwrap_or_else(|e| panic!("GET {url}: {e}"))
            .status()
            .as_u16()
    }

    /// `GET /app/rest/builds?locator=…`, unwrapped to the ids on the page.
    async fn build_ids(&self, locator: &str) -> Vec<i64> {
        let (status, body) = self
            .get(&format!(
                "app/rest/builds?locator={locator}&fields=count,build(id)"
            ))
            .await;
        assert_eq!(status, 200, "locator {locator:?} answered {body}");
        body["build"]
            .as_array()
            .map(|rows| rows.iter().map(id_of).collect())
            .unwrap_or_default()
    }

    /// One build's record, as the adapter's own selector asks for it.
    async fn build(&self, id: i64) -> serde_json::Value {
        let (status, body) = self
            .get(&format!(
                "app/rest/builds/id:{id}?fields=id,number,state,status,statusText,branchName,\
                 defaultBranch,webUrl,queuedDate,startDate,finishDate,triggered(user(username))"
            ))
            .await;
        assert_eq!(status, 200, "build {id}: {body}");
        body
    }

    /// One build configuration's record, `webUrl` included -- which the
    /// adapter's own selector stopped asking for in issue #516, so this is
    /// deliberately *not* "as the adapter asks for it": it is what the server
    /// would have said, so that a test can show the two strings differ.
    async fn build_type(&self, id: &str) -> serde_json::Value {
        let (status, body) = self
            .get(&format!(
                "app/rest/buildTypes/id:{id}?fields=id,name,projectId,projectName,description,\
                 webUrl"
            ))
            .await;
        assert_eq!(status, 200, "buildType {id}: {body}");
        body
    }

    fn source(&self, config: serde_json::Value) -> Box<dyn Source> {
        self.source_with(&self.url, &self.token, config)
    }

    fn source_with(
        &self,
        base_url: &str,
        token: &str,
        config: serde_json::Value,
    ) -> Box<dyn Source> {
        match knobas_source_teamcity::build(SourceInstance {
            id: "teamcity".to_owned(),
            kind: knobas_source_teamcity::ADAPTER_KIND.to_owned(),
            display_name: "Tidewater CI (seeded)".to_owned(),
            base_url: base_url.to_owned(),
            auth: Some(AuthMethod::Pat),
            secret: Some(token.to_owned()),
            account: None,
            config,
        }) {
            Ok(s) => s,
            // `Box<dyn Source>` is not `Debug`, so `expect` is unavailable.
            Err(e) => panic!("the adapter must build against the seeded URL: {e:?}"),
        }
    }

    fn scoped_to(&self, build_type_ids: &[&str]) -> Box<dyn Source> {
        self.source(serde_json::json!({ "build_type_ids": build_type_ids }))
    }

    /// The ids the seed recorded, ascending.
    fn seeded_ids(&self) -> Vec<i64> {
        let mut ids: Vec<i64> = self.seed.builds.iter().map(|b| b.real_id).collect();
        ids.sort_unstable();
        ids
    }

    /// The same ids as `/app/rest/builds` lists them: newest first.
    fn seeded_ids_newest_first(&self) -> Vec<i64> {
        let mut ids = self.seeded_ids();
        ids.reverse();
        ids
    }

    /// The seed's record for the fixture build with this number.
    fn fixture_build(&self, fixture_number: u32) -> &SeededBuild {
        self.seed
            .builds
            .iter()
            .find(|b| b.fixture_number == fixture_number)
            .unwrap_or_else(|| {
                panic!(
                    "seed-state.json records no build {fixture_number}: {:?}",
                    self.seed.builds
                )
            })
    }

    /// Remove every build the seed did not make -- what a run that was
    /// **killed** rather than failed left behind, since only a process that
    /// unwinds reaches [`Queued`]'s `Drop` -- and refuse to go on while a
    /// build the seed *did* make is still in flight.
    ///
    /// Scoped by `seed-state.json` rather than by anything a run remembers,
    /// which is the only way to reach the leftovers of a run that is gone. It
    /// runs *before* a test takes its baseline, so the corpus it measures is
    /// already clean. Read-only in the ordinary case: a clean server costs one
    /// listing -- `defaultFilter:false` with no `state:`, which lists queued
    /// and running builds along with the finished ones (measured 2026-09-02
    /// with two builds held on the queue: both on the page, `state: queued`).
    ///
    /// Not "sweep": CONTEXT.md spends that word on the engine pass that
    /// tombstones what a full sync no longer emitted.
    ///
    /// The refusal is the one-owner rule made a check: `./seed --teamcity
    /// --running` holds 1188 on `Payout_Build`, a running build is re-emitted
    /// on every run by design, and the exact-set assertions here (and clause
    /// 2 beside it) mean nothing while one is up. Refusing names the build and
    /// the remedy rather than failing three tests on unrelated-looking diffs.
    async fn clear_leftovers(&self) {
        let seeded = self.seeded_ids();
        let (status, body) = self
            .get("app/rest/builds?locator=defaultFilter:false,count:100&fields=count,build(id,state)")
            .await;
        assert_eq!(status, 200, "{body}");
        let rows: Vec<(i64, String)> = body["build"]
            .as_array()
            .map(|rows| {
                rows.iter()
                    .map(|b| (id_of(b), b["state"].as_str().unwrap_or_default().to_owned()))
                    .collect()
            })
            .unwrap_or_default();
        let held: Vec<i64> = rows
            .iter()
            .filter(|(id, state)| seeded.contains(id) && state != "finished")
            .map(|(id, _)| *id)
            .collect();
        assert!(
            held.is_empty(),
            "seeded build(s) {held:?} are still in flight -- `./seed --teamcity --running` and \
             this suite are mutually exclusive on one environment: a running build is re-emitted \
             on every run by design, so nothing here that asserts an exact set or an idle run \
             can be measured beside it. Cancel it (testenv/README.md, *The running build is \
             opt-in*) or wait for it to finish, then run the suite again."
        );
        let left: Vec<i64> = rows
            .iter()
            .filter(|(id, _)| !seeded.contains(id))
            .map(|(id, _)| *id)
            .collect();
        if left.is_empty() {
            return;
        }
        println!(
            "live suite: clearing {} leftover build(s) from a run that was killed rather than \
             failed: {left:?}",
            left.len()
        );
        let failures = remove(&self.http, &self.url, &self.token, &left).await;
        assert!(
            failures.is_empty(),
            "the leftovers of an earlier run could not be cleared, so this run would measure a \
             corpus that is not the seed's: {}",
            failures.join("; ")
        );
        assert_eq!(
            self.build_ids("defaultFilter:false,count:100").await,
            self.seeded_ids_newest_first(),
            "after the leftovers are cleared the server holds exactly the seeded builds"
        );
    }
}

/// One build's `state`: `Ok(None)` when the server **says** it has no such
/// build (a 404 -- a delete that went through, or a build that never
/// existed), `Err` when the server did not answer or answered something
/// else. The two are kept apart because the cleanup's "really gone" check is
/// only a check while they are: a container that stopped answering mid-run
/// must not read as a build removed.
async fn state_of(
    http: &reqwest::Client,
    url: &str,
    token: &str,
    id: i64,
) -> Result<Option<String>, String> {
    let response = http
        .get(format!("{url}/app/rest/builds/id:{id}?fields=state"))
        .header("Accept", "application/json")
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| format!("GET build {id}: {e}"))?;
    match response.status().as_u16() {
        404 => Ok(None),
        200 => {
            let body: serde_json::Value = response
                .json()
                .await
                .map_err(|e| format!("GET build {id}: unreadable body: {e}"))?;
            body["state"]
                .as_str()
                .map(|state| Some(state.to_owned()))
                .ok_or_else(|| format!("GET build {id}: no state on {body}"))
        }
        other => Err(format!(
            "GET build {id} -> {other}: {}",
            response.text().await.unwrap_or_default()
        )),
    }
}

/// Cancel (if still in flight) and delete every build in `ids`, and say which
/// could not be. The one place the suite deletes anything, used by the
/// leftover clearing and by [`Queued`]'s `Drop` alike.
async fn remove(http: &reqwest::Client, url: &str, token: &str, ids: &[i64]) -> Vec<String> {
    let mut failures = Vec::new();
    for &id in ids {
        let mut state = match state_of(http, url, token, id).await {
            Ok(Some(state)) => state,
            // The server itself says it has no such build.
            Ok(None) => continue,
            Err(e) => {
                failures.push(e);
                continue;
            }
        };
        if state != "finished" {
            // A build in flight cannot be deleted; cancel it first and wait
            // for the agent to let go of it. `POST /app/rest/builds/id:{id}`
            // with `readdIntoQueue: false` cancels a **queued** build as well
            // as a running one -- measured 2026-09-02 on two builds held on
            // the queue with the agent disabled, which finished
            // `UNKNOWN`/`Canceled` -- and is the request testenv/README.md
            // gives for releasing a held 1188. A refusal is reported here,
            // not after the wait below runs out.
            let canceled = http
                .post(format!("{url}/app/rest/builds/id:{id}"))
                .header("Accept", "application/json")
                .bearer_auth(token)
                .json(&serde_json::json!({
                    "comment": "canceled by the knobas TeamCity live suite's cleanup",
                    "readdIntoQueue": false
                }))
                .send()
                .await;
            match canceled {
                Ok(r) if r.status().is_success() => {}
                Ok(r) => {
                    failures.push(format!(
                        "cancel build {id} -> {}: {}",
                        r.status(),
                        r.text().await.unwrap_or_default()
                    ));
                    continue;
                }
                Err(e) => {
                    failures.push(format!("cancel build {id}: {e}"));
                    continue;
                }
            }
            let deadline = std::time::Instant::now() + BUILD_BUDGET;
            loop {
                tokio::time::sleep(Duration::from_secs(2)).await;
                match state_of(http, url, token, id).await {
                    Ok(Some(s)) => state = s,
                    Ok(None) => break,
                    Err(e) => {
                        failures.push(format!("while waiting for build {id} to stop: {e}"));
                        break;
                    }
                }
                if state == "finished" {
                    break;
                }
                if std::time::Instant::now() > deadline {
                    failures.push(format!(
                        "build {id} did not finish within {BUILD_BUDGET:?} after being canceled: \
                         still {state}"
                    ));
                    break;
                }
            }
        }
        let deleted = http
            .delete(format!("{url}/app/rest/builds/id:{id}"))
            .bearer_auth(token)
            .send()
            .await;
        match deleted {
            Ok(r) if r.status().is_success() || r.status().as_u16() == 404 => {}
            Ok(r) => failures.push(format!(
                "DELETE build {id} -> {}: {}",
                r.status(),
                r.text().await.unwrap_or_default()
            )),
            Err(e) => failures.push(format!("DELETE build {id}: {e}")),
        }
        // Checked, not hoped for: the build must really be gone, and "gone"
        // is the server's 404, not a request that never came back.
        match state_of(http, url, token, id).await {
            Ok(None) => {}
            Ok(Some(state)) => {
                failures.push(format!(
                    "build {id} still answers ({state}) after its delete"
                ));
            }
            Err(e) => failures.push(format!("build {id} was not confirmed gone: {e}")),
        }
    }
    failures
}

/// The one build the mutating test queues, deleted again when the guard
/// drops -- passing or panicking alike -- and the deletion checked.
///
/// Built before the test's baseline sync, because clearing an earlier run's
/// leftovers is itself a change to the corpus and the baseline must be taken
/// after it.
struct Queued {
    url: String,
    token: String,
    id: Option<i64>,
}

impl Queued {
    async fn new(seeded: &Seeded) -> Queued {
        seeded.clear_leftovers().await;
        Queued {
            url: seeded.url.clone(),
            token: seeded.token.clone(),
            id: None,
        }
    }

    /// `POST /app/rest/buildQueue` for one build of `build_type` on `branch`,
    /// and own its removal from this line onwards. Answers the record the
    /// server queued, in full -- what it says about a build on the queue is
    /// the test's to assert, not the guard's.
    async fn queue(
        &mut self,
        seeded: &Seeded,
        build_type: &str,
        branch: &str,
    ) -> serde_json::Value {
        assert!(self.id.is_none(), "this guard owns exactly one build");
        let response = seeded
            .http
            .post(format!("{}/app/rest/buildQueue", seeded.url))
            .header("Accept", "application/json")
            .bearer_auth(&seeded.token)
            .json(&serde_json::json!({
                "buildType": { "id": build_type },
                "branchName": branch,
                "comment": { "text": "queued by the knobas TeamCity live suite (issue #266)" }
            }))
            .send()
            .await
            .expect("queue a build");
        let status = response.status();
        let body: serde_json::Value = response.json().await.unwrap_or_default();
        assert!(status.is_success(), "POST buildQueue -> {status}: {body}");
        self.id = Some(id_of(&body));
        body
    }

    /// Poll the guard's build until it is finished, or fail after
    /// [`BUILD_BUDGET`].
    async fn wait_finished(&self, seeded: &Seeded) -> serde_json::Value {
        let id = self.id.expect("a build was queued");
        let deadline = std::time::Instant::now() + BUILD_BUDGET;
        loop {
            let record = seeded.build(id).await;
            if record["state"] == "finished" {
                return record;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "build {id} did not finish within {BUILD_BUDGET:?}; is the agent connected and \
                 enabled? (`/app/rest/agents`) {record}"
            );
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }
}

impl Drop for Queued {
    fn drop(&mut self) {
        let Some(id) = self.id.take() else {
            return;
        };
        let (url, token) = (self.url.clone(), self.token.clone());
        // `Drop` cannot await and runs on a tokio worker thread, so the
        // cleanup gets a thread with a runtime of its own -- and a client
        // built inside it, because a `reqwest::Client` driven from a second
        // runtime hangs rather than failing (the Gitea suite measured that).
        // The wait is bounded so a server that stops answering ends the run
        // with a sentence rather than a stall.
        let (done, waiting) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let failures = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("a runtime for the cleanup")
                .block_on(async move { remove(&client(), &url, &token, &[id]).await });
            let _ = done.send(failures);
        });
        let failures = match waiting.recv_timeout(CLEANUP_BUDGET) {
            Ok(failures) => failures,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => vec![format!(
                "the cleanup thread panicked (its own message is on stderr), so build {id} may \
                 still be standing; the next run's leftover clearing removes it"
            )],
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => vec![format!(
                "the cleanup did not finish within {CLEANUP_BUDGET:?} and was abandoned, so \
                 build {id} may still be standing; check the container is healthy, then re-run \
                 -- the next run's leftover clearing removes it"
            )],
        };
        if failures.is_empty() {
            return;
        }
        let report = format!(
            "the live suite did not remove the build it queued, so the server is no longer in \
             the plain-seed state: {}",
            failures.join("; ")
        );
        // Panicking while already unwinding aborts the process; the test is
        // already red in that case and this only has to be visible.
        if std::thread::panicking() {
            eprintln!("live suite cleanup: {report}");
        } else {
            panic!("{report}");
        }
    }
}

async fn full(source: &dyn Source) -> (Vec<SyncItem>, String) {
    sync_from(source, None).await
}

async fn sync_from(source: &dyn Source, cursor: Option<String>) -> (Vec<SyncItem>, String) {
    let mut sink = VecSink(Vec::new());
    let next = source
        .sync(cursor, &mut sink)
        .await
        .expect("a sync against the seeded server");
    (sink.0, next)
}

fn of_kind<'a>(items: &'a [SyncItem], kind: &str) -> Vec<&'a SyncItem> {
    items.iter().filter(|i| i.kind == kind).collect()
}

fn keys(items: &[SyncItem]) -> Vec<String> {
    items.iter().map(|i| i.entity.key.clone()).collect()
}

fn id_of(build: &serde_json::Value) -> i64 {
    build["id"]
        .as_i64()
        .unwrap_or_else(|| panic!("a build with no numeric id: {build}"))
}

/// The watermark inside the adapter's cursor.
fn since_build_id(cursor: &str) -> i64 {
    serde_json::from_str::<serde_json::Value>(cursor)
        .ok()
        .and_then(|v| v["since_build_id"].as_i64())
        .unwrap_or_else(|| panic!("not a cursor this adapter wrote: {cursor}"))
}

/// A port nothing listens on: bound to learn the number, then dropped.
fn dead_url() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    format!("http://127.0.0.1:{port}")
}

/// A token that never existed, salted with the process id so a run cannot
/// accidentally collide with a real one.
fn revoked() -> String {
    format!("revoked-{}", std::process::id())
}

/// The cleanup's "really gone" check is a check only if a server that does
/// not answer reads as a failure rather than as a build removed. Witnessed on
/// a port nothing listens on: [`remove`] reports the build it could not reach
/// instead of returning clean. Needs no container, but lives in this file
/// because [`remove`] does.
#[tokio::test]
#[ignore = "needs testenv's seeded TeamCity: `just teamcity-live-seeded`"]
async fn the_cleanup_reports_a_server_it_cannot_reach_rather_than_calling_the_build_gone() {
    let failures = remove(&client(), &dead_url(), &revoked(), &[1]).await;
    assert_eq!(failures.len(), 1, "{failures:?}");
    assert!(failures[0].starts_with("GET build 1: "), "{failures:?}");
}

/// *Test connection* against the server the seed set up: the version the seed
/// recorded, and the account the token belongs to.
#[tokio::test]
#[ignore = "needs testenv's seeded TeamCity: `just teamcity-live-seeded`"]
async fn test_connection_names_the_seeded_server_and_the_seed_account() {
    let seeded = seeded();
    let info = seeded
        .source(serde_json::json!({}))
        .test_connection()
        .await
        .expect("the seeded server answers /app/rest/server");
    assert_eq!(
        info.server_version.as_deref(),
        Some(seeded.seed.version.as_str()),
        "the version `seed-teamcity.sh` recorded is the one the adapter reports"
    );
    assert_eq!(
        info.account.as_deref(),
        Some("knobas"),
        "the token is the seed administrator's (`knobas`), and `/app/rest/users/current` says \
         so: {info:?}"
    );
    assert_eq!(info.secret_expires_at, None);
    println!(
        "SEEDED server: version={:?} detail={:?} account={:?}",
        info.server_version, info.detail, info.account
    );
}

/// The two projects and three configurations, by the ids and names
/// `seed-teamcity-builds.sh` derives from the fixture -- the same rules
/// `knobas-mockd`'s `tc_state.rs` applies, so the three ids are a cross-stream
/// contract and are asserted literally.
#[tokio::test]
#[ignore = "needs testenv's seeded TeamCity: `just teamcity-live-seeded`"]
async fn the_seeded_projects_and_configurations_land_by_id_and_name() {
    let seeded = seeded();
    seeded.clear_leftovers().await;
    let (items, _) = full(&*seeded.source(serde_json::json!({}))).await;

    let configs = of_kind(&items, "build_config");
    let mut keys: Vec<&str> = configs.iter().map(|i| i.entity.key.as_str()).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "buildType:Ledger_Deploy_Staging",
            "buildType:Payout_Build",
            "buildType:Payout_IntegrationTests",
        ],
        "the three configurations the seed derives from the fixture's `cfg`, and no other"
    );
    for (id, title, project) in [
        ("Ledger_Deploy_Staging", "Ledger / Deploy Staging", "Ledger"),
        ("Payout_Build", "Payout / Build", "Payout"),
        (
            "Payout_IntegrationTests",
            "Payout / IntegrationTests",
            "Payout",
        ),
    ] {
        let it = configs
            .iter()
            .find(|i| i.entity.key == format!("buildType:{id}"))
            .expect("listed above");
        assert_eq!(
            it.title, title,
            "<project> / <name>, as the seed named them"
        );
        assert_eq!(it.payload["projectId"], project);
        assert_eq!(it.payload["projectName"], project);
        assert!(
            it.payload.get("description").is_none(),
            "the fixture describes no configuration and the seed invents none; a real server \
             omits the key rather than sending null: {}",
            it.payload
        );
        // **The whole URL, not its tail** (issue #516). A suffix match passes
        // whatever host the string is rooted at, which is exactly the bug: the
        // server fills its own `webUrl` in from its *Server URL* setting and
        // answers `http://localhost:8111/...` however it is reached, so a
        // source configured at any other spelling stored a URL naming a machine
        // the reader may not be sitting at. Equality against the configured
        // base is the assertion a mapping that read the record cannot pass.
        assert_eq!(
            it.web_url.as_deref(),
            Some(format!("{}/buildConfiguration/{id}?mode=builds", seeded.url).as_str()),
            "the configuration's URL is composed from the configured base URL, in the \
             `/buildConfiguration/<id>?mode=builds` shape a TeamCity 2026.1 serves"
        );
        assert_eq!(it.updated_at, None, "TeamCity dates no configuration");
        assert!(
            it.body_text.contains(id),
            "the id is searchable: {:?}",
            it.body_text
        );
    }
}

/// Builds 412 and 1187 by number, state, status and branch, under the ids the
/// server assigned -- and what a failed build mirrors: its `statusText`, its
/// branch and its number, because the log is not on the REST record.
#[tokio::test]
#[ignore = "needs testenv's seeded TeamCity: `just teamcity-live-seeded`"]
async fn the_seeded_builds_land_by_number_state_status_and_branch() {
    let seeded = seeded();
    seeded.clear_leftovers().await;
    let (items, cursor) = full(&*seeded.source(serde_json::json!({}))).await;
    let builds = of_kind(&items, "build");

    let success = seeded.fixture_build(412);
    let failure = seeded.fixture_build(1187);
    assert_eq!(success.build_type, "Ledger_Deploy_Staging");
    assert_eq!(failure.build_type, "Payout_IntegrationTests");

    let item = |real_id: i64| {
        builds
            .iter()
            .find(|i| i.entity.key == format!("build:{real_id}"))
            .unwrap_or_else(|| panic!("build id {real_id} missing; got {:?}", keys(&items)))
    };

    // 412: the successful deploy on the default branch.
    let ok = item(success.real_id);
    assert_eq!(ok.title, "Ledger / Deploy Staging #412");
    assert_eq!(ok.payload["number"], "412");
    assert_eq!(ok.payload["state"], "finished");
    assert_eq!(ok.payload["status"], "SUCCESS");
    assert_eq!(ok.payload["branchName"], "main");
    assert!(
        ok.body_text.contains("finished SUCCESS") && ok.body_text.contains("main"),
        "{:?}",
        ok.body_text
    );

    // 1187: the failed integration-test run on the feature branch -- the
    // fixture's story, and the build the default filter hid until #266.
    let failed = item(failure.real_id);
    assert_eq!(failed.title, "Payout / IntegrationTests #1187");
    assert_eq!(failed.payload["number"], "1187");
    assert_eq!(failed.payload["state"], "finished");
    assert_eq!(failed.payload["status"], "FAILURE");
    assert_eq!(failed.payload["branchName"], "feature/PAY-231-sepa-retry");
    assert!(
        failed.body_text.contains("finished FAILURE")
            && failed.body_text.contains("feature/PAY-231-sepa-retry"),
        "{:?}",
        failed.body_text
    );
    // What a real server puts in `statusText` for a command-line step that
    // exits non-zero. The fixture's log lines are in the build log, which is
    // not on the REST record and which this adapter does not read -- so they
    // are asserted *absent* from the mirror, as the statement of what is and
    // is not indexed. A future log reader would flip this on purpose.
    let status_text = failed.payload["statusText"]
        .as_str()
        .expect("a finished build has a statusText");
    assert!(
        status_text.starts_with("Exit code 1 (Step: IntegrationTests"),
        "the seed reproduces the failure with a command-line step exiting 1, and the server \
         composes statusText from it: {status_text:?}"
    );
    assert!(
        failed.body_text.contains(status_text),
        "{:?}",
        failed.body_text
    );
    assert!(
        !failed.body_text.contains("gives_up_after_max_attempts"),
        "the build log is not on the REST record and the adapter does not read it; if it now \
         does, this suite should assert the log lines rather than their absence: {:?}",
        failed.body_text
    );
    println!(
        "SEEDED 1187: statusText={status_text:?} -- the log's first line is not mirrored (mockd \
         transcribes it into statusText; a real server does not)"
    );

    for it in [ok, failed] {
        // Timestamps are the server's: asserted against the record, never a
        // value. `finishDate` is what `updated_at` is read from.
        let stamp = |key: &str| {
            let raw = it.payload[key]
                .as_str()
                .unwrap_or_else(|| panic!("{key}: {}", it.payload));
            chrono::DateTime::parse_from_str(raw, "%Y%m%dT%H%M%S%z")
                .unwrap_or_else(|e| panic!("{key} {raw:?}: {e}"))
                .with_timezone(&chrono::Utc)
        };
        assert_eq!(it.updated_at, Some(stamp("finishDate")), "{}", it.payload);
        assert!(stamp("finishDate") >= stamp("startDate"), "{}", it.payload);
        assert!(stamp("startDate") >= stamp("queuedDate"), "{}", it.payload);
        // Every seeded build was queued through the seed's token, so its
        // triggerer is the seed administrator -- not the fixture's `mara`
        // and not a VCS trigger (testenv/README.md, *What the seed cannot
        // reproduce*). The adapter reads the username the server names.
        assert_eq!(it.author.as_deref(), Some("knobas"), "{}", it.payload);
        assert!(
            it.body_text.contains("triggered by knobas"),
            "{:?}",
            it.body_text
        );
        let real_id = it.entity.key.strip_prefix("build:").expect("build keys");
        let build_type = it.payload["buildTypeId"].as_str().expect("buildTypeId");
        assert_eq!(
            it.web_url,
            Some(format!(
                "{}/buildConfiguration/{build_type}/{real_id}",
                seeded.url
            )),
            "a build's URL is `<the configured base URL>/buildConfiguration/<buildTypeId>/<id>` \
             -- see `a_builds_web_url_is_composed_from_the_configured_base_url_and_answers_200`"
        );
        assert!(!it.deleted);
    }

    assert_eq!(
        since_build_id(&cursor),
        *seeded.seeded_ids().last().expect("seeded builds"),
        "a full sync over a quiet server lands on the newest seeded build"
    );
    println!("SEEDED cursor after a full sync: {cursor}");
}

/// **A build's `web_url` is knobas' own composition, and it opens a page on
/// the real server** (issue #495).
///
/// Two claims, and the second is the one only a live server can make.
///
/// *The shape*: `<the configured base URL>/buildConfiguration/<buildTypeId>/<id>`,
/// asserted whole rather than by suffix. The seeded container is configured
/// with a *Server URL* of its own and fills every `webUrl` in from that -- it
/// answers `http://localhost:8111/...` whatever host the request arrived on --
/// so the record's URL and the mirror's are two different strings whenever the
/// source is reached by any other spelling, and only a composed URL follows
/// the source. The suite prints both, and runs the same corpus a second time
/// through a **second spelling of the same host** so that the "follows the
/// configuration" half is a measurement rather than a coincidence of this
/// container's setting.
///
/// *The fixture guard* (issue #564): the shape claim is a measurement only
/// while the server's own spelling is rooted somewhere other than the
/// configured base, so the suite asserts that and goes red the day it stops
/// being true. Before #564 this half had no guard: run with
/// `KNOBAS_TEAMCITY_URL=http://localhost:8111`, the container's own *Server
/// URL*, it passed green while witnessing nothing, and only the configuration
/// test below went red.
///
/// The guard compares **hosts**, not whole strings. Measured against the seeded
/// server on 2026-09-11 (TeamCity 2026.1.3, build 222742): this server composes
/// a build's `webUrl` on the same path knobas does, so the two strings agree
/// character for character whenever the hosts coincide --
/// `http://localhost:8111/buildConfiguration/Payout_IntegrationTests/1` from
/// both ends, in every transcript since PR #514. Whole-string inequality
/// therefore *is* host inequality here, and only while that path coincidence
/// holds: a server that changed the path would make the two strings differ on
/// every fixture, and a whole-string guard would stay green while guarding
/// nothing -- the failure this guard exists to catch. So what it asks is
/// whether the served URL sits under the configured base, and nothing about
/// the path.
///
/// *That it opens*: each composed URL is fetched at its absolute address with
/// the credential the adapter uses. The adapter's own HTTP client is not
/// reachable from a test -- `HttpRest` is crate-private and joins every path
/// onto the base URL, so it cannot be handed a whole URL -- so what stands in
/// for it is this suite's client with the same bearer token, **plus**
/// `test_connection`, which does go through the adapter's client: it is what
/// says the host these URLs are rooted at is a host the adapter itself
/// reaches. Together, the closest a test can get to the criterion's "through
/// the source's own client". Each fetch must answer 200 -- and that is a
/// weaker statement than it looks and is worth being exact about. TeamCity
/// 2026.1
/// serves the same single-page-application shell for every path under
/// `/buildConfiguration/`, including one naming a build that does not exist,
/// while an unknown *prefix* (`/nonsense/path`) is a 404 and an unauthenticated
/// request is a 401. So a 200 certifies the route, the host and the credential,
/// and not the build. The build itself is certified beside it, by
/// [`Seeded::build`] reading `/app/rest/builds/id:<id>` for the very id the
/// composed URL carries and asserting 200 on it: together they say that the
/// URL names a route this server serves and a build this server has.
#[tokio::test]
#[ignore = "needs testenv's seeded TeamCity: `just teamcity-live-seeded`"]
async fn a_builds_web_url_is_composed_from_the_configured_base_url_and_answers_200() {
    let seeded = seeded();
    seeded.clear_leftovers().await;
    let source = seeded.scoped_to(&QUIET_CONFIGURATIONS);
    // Through the adapter's own client, and first: it is what makes the base
    // URL below one the adapter reaches rather than one a test asserted about.
    let connected = source
        .test_connection()
        .await
        .unwrap_or_else(|e| panic!("the adapter's own client must reach {}: {e:?}", seeded.url));
    println!("SEEDED connected as {:?}", connected.account);
    let (items, _) = full(&*source).await;
    let builds = of_kind(&items, "build");
    assert_eq!(
        builds.len(),
        QUIET_CONFIGURATIONS.len(),
        "one finished build per quiet configuration: {:?}",
        keys(&items)
    );

    for it in &builds {
        let id: i64 = it
            .entity
            .key
            .strip_prefix("build:")
            .expect("build keys are build:<id>")
            .parse()
            .expect("the key carries the numeric id");
        let build_type = it.payload["buildTypeId"].as_str().expect("buildTypeId");
        let composed = format!("{}/buildConfiguration/{build_type}/{id}", seeded.url);
        assert_eq!(
            it.web_url,
            Some(composed.clone()),
            "the mirror holds the URL knobas composed from the configured base URL"
        );
        // What the server would have said. Asserted about only in the negative
        // below, and that negative is what keeps the equality above a
        // measurement rather than a coincidence of this container's *Server
        // URL* setting (issue #564).
        // A placeholder here would pass the guard below on the placeholder
        // itself, so the guard would stop guarding without ever going red --
        // exactly the failure mode the guard exists to prevent. A server that
        // stopped serving `webUrl` changes this suite's premise and must say so
        // in red.
        let served = seeded.build(id).await["webUrl"]
            .as_str()
            .unwrap_or_else(|| {
                panic!(
                    "the seeded server serves no `webUrl` for build {id}, so the guard \
                     below cannot say the server's spelling still differs from the \
                     configured one -- this suite's premise has changed and the \
                     composition's shape is no longer pinned to anything the server says"
                )
            })
            .to_owned();
        println!("SEEDED {id}: composed {composed} -- server's own webUrl {served}");
        // The guard, and it compares **hosts** rather than whole strings, for
        // the reason the doc comment above gives.
        assert!(
            !served.starts_with(&format!("{}/", seeded.url)),
            "this suite can only witness the composition while the server's own spelling is \
             rooted somewhere other than the configured base. The server's own {served} sits \
             under {}, so KNOBAS_TEAMCITY_URL is the container's own *Server URL* and the \
             assertion above would pass either way -- point the suite at the other spelling \
             of this host",
            seeded.url
        );

        let status = seeded.status_of(&composed).await;
        assert_eq!(
            status, 200,
            "the composed URL must reach a page on the real server: {composed}"
        );
    }

    // The same corpus under a second spelling of the loopback host. Both reach
    // the same container, so the *only* thing that can differ between the two
    // runs is which URL the adapter was configured with -- which is the claim.
    let Some(alternate) = alternate_host(&seeded.url) else {
        panic!(
            "KNOBAS_TEAMCITY_URL is {:?} and this suite knows no second spelling of its \
             host, so the half of this test that separates a composed URL from the \
             server's own cannot run. Point it at 127.0.0.1 or localhost, or teach \
             `alternate_host` this host.",
            seeded.url
        );
    };
    let (alt_items, _) = full(&*seeded.source_with(
        &alternate,
        &seeded.token,
        serde_json::json!({ "build_type_ids": QUIET_CONFIGURATIONS }),
    ))
    .await;
    let alt_builds = of_kind(&alt_items, "build");
    assert_eq!(alt_builds.len(), builds.len(), "the same corpus");
    // Whole-URL equality, not a prefix (issue #564): a prefix passes a URL whose
    // path is wrong under the second spelling, and the path is half of what the
    // composition decides. What this half cannot do is kill a mapping that read
    // the record, because the alternate spelling here *is* the server's own --
    // that is the first loop's equality against the configured base, plus the
    // guard above it.
    for it in &alt_builds {
        let id: i64 = it
            .entity
            .key
            .strip_prefix("build:")
            .expect("build keys are build:<id>")
            .parse()
            .expect("the key carries the numeric id");
        let build_type = it.payload["buildTypeId"].as_str().expect("buildTypeId");
        let url = it.web_url.as_deref().expect("every build has a URL");
        assert_eq!(
            url,
            format!("{alternate}/buildConfiguration/{build_type}/{id}"),
            "a source configured with {alternate} must mirror URLs under it, not under \
             the server's own root URL: {url}"
        );
        assert_eq!(
            seeded.status_of(url).await,
            200,
            "and the second spelling reaches the server too: {url}"
        );
    }
    println!(
        "SEEDED: {} builds re-mirrored under {alternate}",
        alt_builds.len()
    );
}

/// **The same certificate for a build configuration** (issue #516), which
/// #495 deliberately left reading the server's own `webUrl`.
///
/// The reason the two are separate tests is that they were separate defects:
/// #495 fixed builds and recorded, in `docs/contract.md` §9, that a
/// configuration's URL was unchanged by it and that a link copied out of the
/// TeamCity UI would therefore miss against a source configured with another
/// spelling. This is that sentence being closed, and it is measured the same
/// way -- whole-URL equality against the configured base, each URL fetched, and
/// the corpus re-synced under a **second spelling of the same host** so that
/// "the URL follows the configuration" is a measurement rather than a
/// coincidence of this container's *Server URL* setting.
///
/// The query is not decoration. Migration `0023` normalises a pasted URL with
/// its **query kept verbatim**, so a composition that dropped `?mode=builds`
/// would never equal the address a reader copies out of the UI, and the paste
/// would miss. The suite prints the server's own `webUrl` beside the composed
/// one and guards on it in the negative, in the host shape the build test's doc
/// comment above reasons out: nothing is asserted about the server's *path*,
/// which is its own setting's business, only that its root is not the
/// configured base -- because while it is, the equality above passes either
/// way.
///
/// A 200 here says the same weaker thing the build test's does -- TeamCity
/// 2026.1 serves its single-page-application shell for any path under
/// `/buildConfiguration/` -- so what it certifies is the host, the route and the
/// credential. That the configuration itself exists is certified beside it, by
/// the ids this suite reads back from `/app/rest/buildTypes`.
#[tokio::test]
#[ignore = "needs testenv's seeded TeamCity: `just teamcity-live-seeded`"]
async fn a_build_configurations_web_url_is_composed_from_the_configured_base_url_and_answers_200() {
    let seeded = seeded();
    seeded.clear_leftovers().await;
    // Through the adapter's own client, and first: it is what makes the base
    // URL below one the adapter reaches rather than one a test asserted about.
    let connected = seeded
        .source(serde_json::json!({}))
        .test_connection()
        .await
        .unwrap_or_else(|e| panic!("the adapter's own client must reach {}: {e:?}", seeded.url));
    println!("SEEDED connected as {:?}", connected.account);
    let (items, _) = full(&*seeded.source(serde_json::json!({}))).await;
    let configs = of_kind(&items, "build_config");
    assert_eq!(configs.len(), 3, "the three seeded configurations");

    for it in &configs {
        let id = it
            .entity
            .key
            .strip_prefix("buildType:")
            .expect("configuration keys are buildType:<id>");
        let composed = format!("{}/buildConfiguration/{id}?mode=builds", seeded.url);
        assert_eq!(
            it.web_url.as_deref(),
            Some(composed.as_str()),
            "the mirror holds the URL knobas composed from the configured base URL"
        );
        // What the server would have said. Asserted about only in the negative
        // below, and that negative is the whole point of the ticket: the two
        // strings differ, so a mapping that read the record could not have
        // produced the one above.
        // A placeholder here would pass the `assert_ne!` below on the
        // placeholder itself, so the guard would stop guarding without ever
        // going red -- exactly the failure mode the guard exists to prevent.
        // A server that stopped serving `webUrl` changes this ticket's premise
        // and must say so in red.
        let served = seeded.build_type(id).await["webUrl"]
            .as_str()
            .unwrap_or_else(|| {
                panic!(
                    "the seeded server serves no `webUrl` for {id}, so the guard below \
                     cannot say the server's spelling still differs from the configured \
                     one -- this suite's premise has changed and the composition's shape \
                     is no longer pinned to anything the server says"
                )
            })
            .to_owned();
        println!("SEEDED {id}: composed {composed} -- server's own webUrl {served}");
        // A **host** comparison since #564, where it was a whole-string
        // `assert_ne!(served, composed)`. The two say the same thing on this
        // server only because the paths coincide -- see the guard's reasoning in
        // `a_builds_web_url_is_composed_from_the_configured_base_url_and_answers_200`.
        assert!(
            !served.starts_with(&format!("{}/", seeded.url)),
            "this suite can only witness the composition while the server's own spelling \
             differs from the configured one. The server's own {served} sits under {}, so \
             KNOBAS_TEAMCITY_URL is the container's own *Server URL* and the assertion above \
             would pass either way -- point the suite at the other spelling of this host",
            seeded.url
        );

        assert_eq!(
            seeded.status_of(&composed).await,
            200,
            "the composed URL must reach a page on the real server: {composed}"
        );
    }

    // The same corpus under a second spelling of the loopback host. Both reach
    // the same container, so the *only* thing that can differ between the two
    // runs is which URL the adapter was configured with -- which is the claim.
    let Some(alternate) = alternate_host(&seeded.url) else {
        panic!(
            "KNOBAS_TEAMCITY_URL is {:?} and this suite knows no second spelling of its \
             host, so the half of this test that separates a composed URL from the \
             server's own cannot run. Point it at 127.0.0.1 or localhost, or teach \
             `alternate_host` this host.",
            seeded.url
        );
    };
    let (alt_items, _) =
        full(&*seeded.source_with(&alternate, &seeded.token, serde_json::json!({}))).await;
    let alt_configs = of_kind(&alt_items, "build_config");
    assert_eq!(alt_configs.len(), configs.len(), "the same corpus");
    for it in &alt_configs {
        let id = it
            .entity
            .key
            .strip_prefix("buildType:")
            .expect("configuration keys are buildType:<id>");
        assert_eq!(
            it.web_url.as_deref(),
            Some(format!("{alternate}/buildConfiguration/{id}?mode=builds").as_str()),
            "a source configured with {alternate} must mirror URLs under it, not under the \
             server's own root URL"
        );
        assert_eq!(
            seeded
                .status_of(it.web_url.as_deref().expect("just asserted"))
                .await,
            200,
            "and the second spelling reaches the server too"
        );
    }
    println!(
        "SEEDED: {} configurations re-mirrored under {alternate}",
        alt_configs.len()
    );
}

/// A second spelling of the loopback host, so that "the URL follows the
/// configuration" can be measured against one container. `None` for a host
/// with no second spelling this suite knows.
fn alternate_host(url: &str) -> Option<String> {
    if url.contains("127.0.0.1") {
        Some(url.replace("127.0.0.1", "localhost"))
    } else if url.contains("localhost") {
        Some(url.replace("localhost", "127.0.0.1"))
    } else {
        None
    }
}

/// **The finding of the first hour, certified from both ends** (issue #266):
/// TeamCity's default filter narrows every finished-state locator to the
/// default branch, `branch:default:any` is what re-opens it, and both of the
/// adapter's item-producing queries carry the dimension.
///
/// Raw first, on the adapter's own two locators with and without the
/// dimension, so the dimension is the only thing between the pages; then
/// through the adapter, so what is certified is what reaches the mirror. Both
/// halves name the seeded ids: a suite that could only say "a page came back"
/// is how this survived the public instance.
#[tokio::test]
#[ignore = "needs testenv's seeded TeamCity: `just teamcity-live-seeded`"]
async fn the_feature_branch_build_is_served_by_both_item_producing_locators() {
    let seeded = seeded();
    seeded.clear_leftovers().await;
    let on_main = seeded.fixture_build(412).real_id;
    let on_feature = seeded.fixture_build(1187).real_id;

    // The per-configuration full-sync locator, as `sync::execute` sends it.
    let per_config = "buildType:(id:Payout_IntegrationTests),state:finished,canceled:any,\
                      failedToStart:any";
    assert_eq!(
        seeded
            .build_ids(&format!("{per_config},branch:default:any,count:100"))
            .await,
        vec![on_feature],
        "the adapter's locator serves the feature-branch build"
    );
    assert_eq!(
        seeded.build_ids(&format!("{per_config},count:100")).await,
        Vec::<i64>::new(),
        "...and the same locator without `branch:default:any` answers nothing: the default \
         filter hides every non-default-branch build from a finished-state locator. A build \
         here means TeamCity's default filter changed and the dimension has become redundant \
         rather than wrong -- re-read `rest::Locator::branch_any` before deleting it."
    );

    // The incremental locator, as `sync::since` sends it.
    let incremental = "state:finished,sinceBuild:(id:0),canceled:any,failedToStart:any";
    let mut both = vec![on_feature, on_main];
    both.sort_unstable_by(|a, b| b.cmp(a));
    assert_eq!(
        seeded
            .build_ids(&format!("{incremental},branch:default:any,count:100"))
            .await,
        both,
        "both seeded builds, newest first"
    );
    assert_eq!(
        seeded.build_ids(&format!("{incremental},count:100")).await,
        vec![on_main],
        "without the dimension only the default-branch build answers: 412 is on `main`, which \
         the seed queues as `<default>`"
    );

    // Through the adapter: the full sync of that one configuration, and an
    // incremental run from a watermark below everything.
    let (items, _) = full(&*seeded.scoped_to(&["Payout_IntegrationTests"])).await;
    assert_eq!(
        keys(&items),
        [
            "buildType:Payout_IntegrationTests".to_owned(),
            format!("build:{on_feature}")
        ],
        "a full sync scoped to the failed build's configuration mirrors it"
    );
    let (items, cursor) = sync_from(
        &*seeded.scoped_to(&QUIET_CONFIGURATIONS),
        Some(r#"{"v":1,"since_build_id":0}"#.to_owned()),
    )
    .await;
    let mut seen: Vec<i64> = of_kind(&items, "build")
        .iter()
        .map(|i| i.entity.key["build:".len()..].parse().expect("numeric"))
        .collect();
    seen.sort_unstable();
    assert_eq!(
        seen,
        seeded.seeded_ids(),
        "an incremental run from below everything mirrors every seeded build, the \
         feature-branch one included"
    );
    assert_eq!(
        since_build_id(&cursor),
        *seeded.seeded_ids().last().expect("seeded")
    );
}

/// The contract battery -- the suite every adapter must pass -- against the
/// server that decides, over the configurations nothing in this file queues
/// on while it runs.
///
/// Clause 2 is the one this server was seeded for: an incremental run after
/// no changes emits nothing and returns the same cursor. The public instance
/// cannot be held to it and mockd agrees with the adapter by construction; a
/// quiet corpus we own is where the clause is actually measured. Faults are
/// real: a token that never existed draws the server's own 401, and a port
/// nothing listens on is unreachable.
#[tokio::test]
#[ignore = "needs testenv's seeded TeamCity: `just teamcity-live-seeded`"]
async fn passes_the_contract_battery_against_the_seeded_server() {
    let seeded = seeded();
    seeded.clear_leftovers().await;
    let scope = serde_json::json!({ "build_type_ids": QUIET_CONFIGURATIONS });
    battery(move |fault| {
        let (base, token) = match fault {
            Fault::None => (seeded.url.clone(), seeded.token.clone()),
            Fault::Unauthorized => (seeded.url.clone(), revoked()),
            Fault::Unreachable => (dead_url(), seeded.token.clone()),
        };
        seeded.source_with(&base, &token, scope.clone())
    })
    .await;
}

/// **The `sinceBuild` watermark on a build this suite queued**, and what the
/// deleted build proves afterwards.
///
/// Full run, idle run (clause 2 on the position a full run wrote), one build
/// queued through TeamCity's own REST API on a feature branch and waited for,
/// then: the next run returns exactly that build and its configuration and
/// moves the position to it; the run after that is idle again. The build is
/// then deleted, which turns the position into one the server disowns -- and
/// the adapter refuses it as issue #91's replaced-server case, on a real 404
/// from `GET /app/rest/builds/id:{id}`.
///
/// On a feature branch deliberately: the incremental query is the one that
/// would have lost such a build for good before #266, so the build this test
/// makes is of the class the fix is about.
#[tokio::test]
#[ignore = "needs testenv's seeded TeamCity: `just teamcity-live-seeded`"]
async fn a_build_queued_through_rest_moves_the_watermark_and_the_next_run_stands_still() {
    let seeded = seeded();
    let mut guard = Queued::new(&seeded).await;
    let source = seeded.scoped_to(&[QUICK_CONFIGURATION]);
    let baseline = seeded.fixture_build(412).real_id;

    let (_, cursor) = full(&*source).await;
    assert_eq!(
        since_build_id(&cursor),
        baseline,
        "a full sync scoped to {QUICK_CONFIGURATION} lands on its seeded build"
    );
    let (idle, same) = sync_from(&*source, Some(cursor.clone())).await;
    assert!(idle.is_empty(), "nothing changed: {:?}", keys(&idle));
    assert_eq!(same, cursor, "byte-identical");

    let queued = guard
        .queue(&seeded, QUICK_CONFIGURATION, FEATURE_BRANCH)
        .await;
    let id = id_of(&queued);
    assert!(id > baseline, "ids are monotonic: {id} after {baseline}");
    assert_eq!(
        queued["state"], "queued",
        "a real server answers the queued build in full: {queued}"
    );
    assert!(
        queued.get("number").is_none() && queued.get("status").is_none(),
        "a build on the queue has neither a number nor a status yet -- the adapter titles it by \
         id and renders the bare state, and mockd serves the same shape: {queued}"
    );
    let record = guard.wait_finished(&seeded).await;
    let number: u32 = record["number"]
        .as_str()
        .and_then(|n| n.parse().ok())
        .unwrap_or_else(|| panic!("a finished build has a number: {record}"));
    assert!(
        number > 412,
        "the build takes the configuration's next number (413 after a fresh seed): {record}"
    );
    assert_eq!(record["status"], "SUCCESS", "{record}");
    assert_eq!(record["branchName"], FEATURE_BRANCH, "{record}");
    assert_eq!(record["defaultBranch"], false, "{record}");
    println!("SEEDED queued build: id {id}, number {number}, on {FEATURE_BRANCH}");

    let (items, moved) = sync_from(&*source, Some(cursor.clone())).await;
    assert_eq!(
        keys(&items),
        [
            format!("buildType:{QUICK_CONFIGURATION}"),
            format!("build:{id}")
        ],
        "the next incremental run returns exactly the build that finished and the configuration \
         it touched"
    );
    let it = of_kind(&items, "build")[0];
    assert_eq!(it.payload["number"], number.to_string());
    assert!(
        it.body_text.contains("finished SUCCESS") && it.body_text.contains(FEATURE_BRANCH),
        "{:?}",
        it.body_text
    );
    // The whole watermark claim, by identity -- and a bare `assert_ne!(moved,
    // cursor)` stood under it until #347. `since_build_id(&cursor)` is pinned
    // to `baseline` above, `id > baseline` is asserted above that, and this
    // equality pins `since_build_id(&moved)` to `id`; two cursors whose
    // watermarks differ are not the same string, so the inequality could not
    // fail on top of the three assertions that precede it.
    assert_eq!(
        since_build_id(&moved),
        id,
        "a finished build is exactly what moves the watermark"
    );

    let (idle, still) = sync_from(&*source, Some(moved.clone())).await;
    assert!(
        idle.is_empty(),
        "the run after the event skips it: {:?}",
        keys(&idle)
    );
    assert_eq!(still, moved, "byte-identical");

    // Deleted now, explicitly, so the rest of the test can use the hole it
    // leaves. `Drop` would have done the same at the end; it has nothing left
    // to do afterwards.
    drop(guard);
    let (status, _) = seeded
        .get(&format!("app/rest/builds/id:{id}?fields=id"))
        .await;
    assert_eq!(status, 404, "the deleted build is gone by id");
    assert_eq!(
        seeded.build_ids("defaultFilter:false,count:100").await,
        seeded.seeded_ids_newest_first(),
        "the server is back in the plain-seed state"
    );

    // Issue #91's replaced-server refusal, on a real 404: nothing the run
    // witnesses reaches the watermark, `GET /app/rest/builds/id:{id}` says
    // the build is not there, and the run refuses rather than syncing on.
    let mut sink = VecSink(Vec::new());
    let refused = source
        .sync(Some(moved.clone()), &mut sink)
        .await
        .expect_err("a watermark on a build the server no longer has");
    assert!(
        matches!(&refused, SourceError::Protocol { message, .. }
            if message.contains(&format!("`/app/rest/builds/id:{id}` answers 404"))
                && message.contains("reset the source's cursor")),
        "{refused:?}"
    );
    assert!(sink.0.is_empty(), "nothing is emitted from a refused run");
    println!("SEEDED refusal on the deleted build: {refused:?}");
}

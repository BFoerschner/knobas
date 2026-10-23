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
//! docker compose --env-file images.env --profile real-teamcity up -d teamcity teamcity-agent
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
//! * **A custom run through the adapter** (#669): `run_build` queues a build
//!   on a feature branch with one parameter overridden; the build runs on
//!   that branch with the override in its resulting parameters, the
//!   configuration keeps its own value, and the next incremental run returns
//!   exactly that build.
//! * **A trigger and a re-run through the adapter** (#1699): `trigger_build`
//!   puts exactly one build of the named configuration on the queue, and
//!   `rerun_build` one of the seeded build's own configuration; a
//!   configuration the server does not have, and a build that was deleted,
//!   are each refused on TeamCity's 404 with nothing queued. These were the
//!   mockd suite's until then; ADR-0013 retired them there.
//! * **The `sinceBuild` watermark on a build this suite queues**: full run,
//!   idle run, one `POST /app/rest/buildQueue`, and the next run returns
//!   exactly that build and moves the position to it; the run after that is
//!   idle again; and once the build is deleted, a cursor pointing at it is
//!   refused as issue #91's replaced-server case, on a real 404.
//!
//! # What this file writes, and what it takes away
//!
//! It is not read-only. One test queues one build through TeamCity's own REST
//! API, and one through the adapter's `run_build`, and each waits for the one
//! agent to run it; the trigger and re-run cases queue theirs through the
//! adapter with the agents parked, so the builds never leave the queue; each
//! build is **deleted again**
//! when the test ends, passing or panicking alike -- [`Queued`] does it from
//! `Drop`, through `knobas_live_cleanup::Cleanup`, and checks afterwards that
//! the build is really gone rather than assuming so. What a *killed* run left
//! behind is cleared by the next run: [`Seeded::clear_leftovers`] removes
//! every build whose id is not in `seed-state.json`, canceling it first if it
//! is still queued or running (page after page, until one holds none), then
//! deletes every project the seed does not own with its configurations
//! (#1560), and every test that asserts an exact set or creates a project or
//! configuration calls it before taking its baseline. Recovery from a dirty environment is "run
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
//! and the sync does not read it (the build's source view reads its tail
//! live, #665): what a failed build mirrors is its `statusText`, which a real
//! server composes from the failed test the step reports (`Tests failed: 1
//! (1 new), passed: 0`) rather than from the log's first line as mockd's
//! fixture transcription does. Personal builds cannot be seeded, so the
//! personal facet stays certified on the public instance only.

use std::path::PathBuf;
use std::time::Duration;

use knobas_live_cleanup::Cleanup;
use knobas_source::contract::{Fault, VecSink, Walking, battery};
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

/// How long each guard's `Drop` waits for its cleanup before reporting that it
/// did not finish rather than stalling the run: what this suite hands
/// `knobas_live_cleanup::Cleanup::within`, below the runner's own default
/// because no cleanup here waits on an index.
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
    /// What the seed set on one configuration for its source view (#664).
    /// Optional so the suites that predate it still read an older
    /// seed-state.json; the view's test asks for it by name.
    settings: Option<SeededSettings>,
    /// The change witness's two builds (#683): seeded builds like the
    /// fixture's, so the exact sets here count them and the leftover
    /// clearing keeps them. Optional for the reason `settings` is.
    changes: Option<SeededChanges>,
    /// The build chain the seed leaves (#1010): three configurations in a
    /// project of their own and one build of each on a feature branch. Its
    /// builds are seeded builds -- the exact sets count them and the leftover
    /// clearing keeps them -- but its configurations are not the fixture's
    /// three, which the tests that count those leave it out of
    /// ([`Seeded::fixture_configs`]). Optional for the reason `settings` is.
    chain: Option<SeededChain>,
}

/// `teamcity.chain` in seed-state.json.
#[derive(Debug, Clone, serde::Deserialize)]
struct SeededChain {
    project: String,
    build_types: Vec<String>,
    builds: Vec<SeededChainBuild>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct SeededChainBuild {
    build_type: String,
    real_id: i64,
}

/// `teamcity.changes` in seed-state.json: a configuration built twice on a
/// branch that moved between, so the second build lists a change.
#[derive(Debug, Clone, serde::Deserialize)]
struct SeededChanges {
    build_type: String,
    before_id: i64,
    after_id: i64,
}

/// `teamcity.settings` in seed-state.json: the configuration that carries the
/// seeded parameters and trigger, and the password parameter's value -- the
/// string no view may contain.
#[derive(Debug, Clone, serde::Deserialize)]
struct SeededSettings {
    build_type: String,
    secret_parameter: String,
    secret_value: String,
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
                     (`docker compose --env-file images.env --profile real-teamcity up -d teamcity teamcity-agent`, \
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
        ids.extend(
            self.seed
                .changes
                .iter()
                .flat_map(|c| [c.before_id, c.after_id]),
        );
        ids.extend(
            self.seed
                .chain
                .iter()
                .flat_map(|c| c.builds.iter().map(|b| b.real_id)),
        );
        ids.sort_unstable();
        ids
    }

    /// The configurations the fixture makes: what `items` lists less the
    /// chain's, which the seed adds beside them (#1010) and which the tests
    /// that assert "these three and no other" must not count.
    fn fixture_configs<'a>(&self, items: &'a [SyncItem]) -> Vec<&'a SyncItem> {
        of_kind(items, "build_config")
            .into_iter()
            .filter(|i| {
                !self.seed.chain.iter().any(|c| {
                    c.build_types
                        .iter()
                        .any(|t| i.entity.key == format!("buildType:{t}"))
                })
            })
            .collect()
    }

    /// The seeded ids of one configuration, ascending: its fixture builds
    /// and, on the change witness's configuration, the witness's two.
    fn seeded_ids_of(&self, build_type: &str) -> Vec<i64> {
        let mut ids: Vec<i64> = self
            .seed
            .builds
            .iter()
            .filter(|b| b.build_type == build_type)
            .map(|b| b.real_id)
            .collect();
        ids.extend(
            self.seed
                .changes
                .iter()
                .filter(|c| c.build_type == build_type)
                .flat_map(|c| [c.before_id, c.after_id]),
        );
        ids.extend(
            self.seed
                .chain
                .iter()
                .flat_map(|c| &c.builds)
                .filter(|b| b.build_type == build_type)
                .map(|b| b.real_id),
        );
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
        // One page holds 100 builds, and a run that was killed repeatedly
        // leaves more than that (#1560: 98 per test, 1,425 over a run), so
        // the listing is repeated until a page holds nothing the seed did not
        // make. A page that still holds leftovers after the previous one was
        // cleared is a removal that did not take, which `remove`'s failures
        // report on the way, so the loop cannot spin on one.
        let mut cleared = 0usize;
        loop {
            let (status, body) = self
                .get(
                    "app/rest/builds?locator=defaultFilter:false,count:100\
                     &fields=count,build(id,state)",
                )
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
                 this suite are mutually exclusive on one environment: a running build is \
                 re-emitted on every run by design, so nothing here that asserts an exact set or \
                 an idle run can be measured beside it. Cancel it (testenv/README.md, *The \
                 running build is opt-in*) or wait for it to finish, then run the suite again."
            );
            let left: Vec<i64> = rows
                .iter()
                .filter(|(id, _)| !seeded.contains(id))
                .map(|(id, _)| *id)
                .collect();
            if left.is_empty() {
                break;
            }
            println!(
                "live suite: clearing {} leftover build(s) from a run that was killed rather \
                 than failed: {left:?}",
                left.len()
            );
            let failures = remove(&self.http, &self.url, &self.token, &left).await;
            assert!(
                failures.is_empty(),
                "the leftovers of an earlier run could not be cleared, so this run would \
                 measure a corpus that is not the seed's: {}",
                failures.join("; ")
            );
            cleared += left.len();
        }
        if cleared > 0 {
            assert_eq!(
                self.build_ids("defaultFilter:false,count:100").await,
                self.seeded_ids_newest_first(),
                "after the leftovers are cleared the server holds exactly the seeded builds"
            );
        }
        self.clear_stray_projects().await;
    }

    /// The projects the seed owns, by id: the project of every configuration
    /// it recorded (a configuration id is `<project>_<name>`, the seed's own
    /// `project_of`), and the chain's.
    fn owned_projects(&self) -> std::collections::BTreeSet<String> {
        let mut configs: Vec<&str> = self
            .seed
            .builds
            .iter()
            .map(|b| b.build_type.as_str())
            .collect();
        configs.extend(self.seed.settings.iter().map(|s| s.build_type.as_str()));
        configs.extend(self.seed.changes.iter().map(|c| c.build_type.as_str()));
        let mut owned: std::collections::BTreeSet<String> = configs
            .into_iter()
            .filter_map(|id| id.split('_').next())
            .map(str::to_owned)
            .collect();
        owned.extend(self.seed.chain.iter().map(|c| c.project.clone()));
        owned.insert("_Root".to_owned());
        owned
    }

    /// Delete every project the seed does not own, with its configurations and
    /// builds (#1560: ~94 `Witness_Alpha_Cfg…` configurations a branch left
    /// behind reached the licence's limit, so `POST app/rest/buildTypes`
    /// answered 403, and made the exact-set assertions see 97 configurations
    /// where they expect 3). Runs after the leftover builds are gone, so
    /// nothing in a stray project is in flight. A nested project of an owned
    /// one that a killed run left (`KnobasNested849`) goes the same way; the
    /// owned projects themselves are never touched.
    async fn clear_stray_projects(&self) {
        let (status, body) = self
            .get("app/rest/projects?fields=project(id,parentProjectId)")
            .await;
        assert_eq!(status, 200, "{body}");
        let owned = self.owned_projects();
        let stray: Vec<String> = body["project"]
            .as_array()
            .map(|rows| {
                rows.iter()
                    .filter_map(|p| p["id"].as_str())
                    .filter(|id| !owned.contains(*id))
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        if stray.is_empty() {
            return;
        }
        println!(
            "live suite: deleting {} project(s) the seed does not own: {stray:?}",
            stray.len()
        );
        for id in &stray {
            // Deleting a project deletes the ones inside it, so a nested stray
            // is already gone by the time its own turn comes: a 404 is done.
            // A project with thousands of builds takes longer than a read.
            let response = self
                .http
                .delete(format!("{}/app/rest/projects/id:{id}", self.url))
                .bearer_auth(&self.token)
                .timeout(CLEANUP_BUDGET)
                .send()
                .await
                .unwrap_or_else(|e| panic!("DELETE project {id}: {e}"));
            let status = response.status().as_u16();
            assert!(
                (200..300).contains(&status) || status == 404,
                "a project the seed does not own could not be deleted: {id} -> {status}: {}",
                response.text().await.unwrap_or_default()
            );
        }
        let (status, body) = self.get("app/rest/projects?fields=project(id)").await;
        assert_eq!(status, 200, "{body}");
        let mut now: Vec<&str> = body["project"]
            .as_array()
            .map(|rows| rows.iter().filter_map(|p| p["id"].as_str()).collect())
            .unwrap_or_default();
        now.retain(|id| !owned.contains(*id));
        assert!(
            now.is_empty(),
            "after the stray projects are deleted the server holds only the seed's: {now:?}"
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

    /// Own the removal of a build something else queued -- the adapter's
    /// `run_build`, whose receipt names it (#669) -- from this line onwards.
    fn adopt(&mut self, id: i64) {
        assert!(self.id.is_none(), "this guard owns exactly one build");
        self.id = Some(id);
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
        Cleanup::new(
            "the live suite did not remove the build it queued, so the server is no longer in \
             the plain-seed state",
            format!(
                "build {id} may still be standing; the next run's leftover clearing removes it"
            ),
        )
        .within(CLEANUP_BUDGET)
        .run(move || async move { joined(remove(&client(), &url, &token, &[id]).await) });
    }
}

/// One cleanup's failures as the runner takes them: none is `Ok`.
fn joined(failures: Vec<String>) -> Result<(), String> {
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("; "))
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

    let configs = seeded.fixture_configs(&items);
    let mut keys: Vec<&str> = configs.iter().map(|i| i.entity.key.as_str()).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "buildType:Ledger_Deploy_Staging",
            "buildType:Payout_Build",
            "buildType:Payout_IntegrationTests",
        ],
        "the three configurations the seed derives from the fixture's `cfg`, and no other \
         but the chain's (#1010), which the seed adds in a project of its own"
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
    assert_eq!(
        status_text, "Tests failed: 1 (1 new), passed: 0",
        "the seed's step reports the fixture's failed test (#665) and exits 1, and the server \
         composes statusText from the test counts, which outrank the exit code"
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
/// What the guard asks is whether the served URL sits **under the configured
/// base**, and nothing about the path the server appends. A prefix, so it
/// covers scheme, host, port and whatever path the base itself carries (the
/// public instance's `/guestAuth`); on a base without one, which is every
/// fixture this suite runs against, that is a host comparison and the ruling
/// names it as one.
///
/// Measured against the seeded server on 2026-09-11 (TeamCity 2026.1.3, build
/// 222742): this server appends the same path knobas composes, so the two URLs
/// differ in the host and nowhere else. Every transcript since PR #514 prints
/// the pair, e.g. `composed
/// http://127.0.0.1:8111/buildConfiguration/Payout_IntegrationTests/1 --
/// server's own webUrl
/// http://localhost:8111/buildConfiguration/Payout_IntegrationTests/1`. So a
/// whole-string `assert_ne!` would say the same thing as this guard, and only
/// while that path coincidence holds: a server that changed the path would make
/// the two strings differ on every fixture, and a whole-string guard would stay
/// green while guarding nothing -- the failure this guard exists to catch.
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
    let quiet: usize = QUIET_CONFIGURATIONS
        .iter()
        .map(|c| seeded.seeded_ids_of(c).len())
        .sum();
    assert_eq!(
        builds.len(),
        quiet,
        "every seeded build of the quiet configurations (one fixture build each, and the \
         change witness's two): {:?}",
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
            "a source configured with {alternate} must mirror the whole URL it composes \
             under that base -- path and all, not merely a URL beginning with it, and not \
             one under the server's own root URL: {url}"
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
    assert_eq!(
        seeded.fixture_configs(&items).len(),
        3,
        "the three seeded configurations, and the chain's beside them"
    );

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
        // below, and that negative is the whole point of the ticket: the server
        // roots its own spelling elsewhere, so a mapping that read the record
        // could not have produced the one above.
        // A placeholder here would pass the guard below on the placeholder
        // itself, so the guard would stop guarding without ever going red --
        // exactly the failure mode the guard exists to prevent.
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
    assert_eq!(
        seeded
            .build_ids(&format!("{incremental},branch:default:any,count:100"))
            .await,
        seeded.seeded_ids_newest_first(),
        "every seeded build, newest first: both fixture builds and the change witness's, \
         which are on its own branch"
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
        Some(r#"{"v":2,"since_build_id":0}"#.to_owned()),
    )
    .await;
    let mut seen: Vec<i64> = of_kind(&items, "build")
        .iter()
        .map(|i| i.entity.key["build:".len()..].parse().expect("numeric"))
        .collect();
    seen.sort_unstable();
    // The quiet configurations' builds only: the chain's are in no scope here.
    let mut quiet: Vec<i64> = QUIET_CONFIGURATIONS
        .iter()
        .flat_map(|c| seeded.seeded_ids_of(c))
        .collect();
    quiet.sort_unstable();
    assert_eq!(
        seen, quiet,
        "an incremental run from below everything mirrors every seeded build of the scope, \
         the feature-branch one included"
    );
    // The watermark is the scope's: a source scoped to a configuration list
    // asks per configuration (#1626), so the chain's builds, newer and out of
    // scope, are never in its answers.
    assert_eq!(
        since_build_id(&cursor),
        *quiet.last().expect("seeded"),
        "an incremental run scoped to a configuration list lands on its own newest build"
    );
    let asked: Vec<String> = QUIET_CONFIGURATIONS
        .iter()
        .map(|c| format!("buildType:(id:{c}),{incremental},branch:default:any,count:100"))
        .collect();
    for (locator, c) in asked.iter().zip(QUIET_CONFIGURATIONS) {
        let mut ids = seeded.build_ids(locator).await;
        ids.sort_unstable();
        let mut own = seeded.seeded_ids_of(c);
        own.sort_unstable();
        assert_eq!(
            ids, own,
            "the per-configuration incremental locator, as `sync::since` sends it, answers \
             exactly {c}'s builds"
        );
    }
}

/// A synced build carries its configuration, and `under:` finds it by both
/// names TeamCity declares for it (#597, ADR-0018).
///
/// The build reads its configuration off the nested `buildType` the real
/// server sends, through the descriptor's own declaration and
/// `knobas_core::payload`'s resolver -- the Rust half of the rule the SQL
/// `under:` predicate is the other half of, held together by `knobas-core`'s
/// agreement test. **Both** declared names are asserted on every build, and on
/// every configuration's own record, which is how `under:` returns the
/// configuration itself: clause 6 gives a parent's names one evidence slot, so
/// a misspelled second name would pass the battery below while `under:<id>`
/// silently missed (the deputy's ruling of 2026-10-12 on #595). The names are
/// held to the seed's literal ids and names, not to what the resolver read.
/// That `under:` matches them ignoring case is the search seam's to witness
/// (`knobas-search`'s `under_teamcity.rs`).
#[tokio::test]
#[ignore = "needs testenv's seeded TeamCity: `just teamcity-live-seeded`"]
async fn a_synced_build_carries_its_configuration_and_is_found_by_under_its_name_and_id() {
    use knobas_core::payload::{resolve_names_as_parent, resolve_parents};

    let seeded = seeded();
    seeded.clear_leftovers().await;
    let (items, _) = full(&*seeded.source(serde_json::json!({}))).await;
    let declarations = knobas_source_teamcity::descriptor_template().payload_paths;
    let declared = &declarations
        .iter()
        .find(|paths| paths.kind == "build")
        .expect("a build declares its parent")
        .parent;
    // Name, then id: what the seed called each configuration. The same pairs
    // `the_seeded_projects_and_configurations_land_by_id_and_name` asserts.
    let names_of = |id: &str| -> [String; 2] {
        let name = match id {
            "Ledger_Deploy_Staging" => "Deploy Staging",
            "Payout_Build" => "Build",
            "Payout_IntegrationTests" => "IntegrationTests",
            other => seeded
                .seed
                .chain
                .iter()
                .find(|c| c.build_types.iter().any(|t| t == other))
                .and_then(|c| other.strip_prefix(&format!("{}_", c.project)))
                .unwrap_or_else(|| panic!("a configuration the seed does not make: {other}")),
        };
        [name.to_owned(), id.to_owned()]
    };

    let builds = of_kind(&items, "build");
    assert!(!builds.is_empty(), "the seed leaves finished builds");
    for build in &builds {
        let config = build.payload["buildTypeId"]
            .as_str()
            .unwrap_or_else(|| panic!("{} names its configuration", build.entity));
        let parents = resolve_parents(&build.payload, declared);
        assert_eq!(
            parents.len(),
            1,
            "{} sits in one configuration: {parents:?}",
            build.entity
        );
        assert_eq!(parents[0].kind, "build_config");
        assert_eq!(
            parents[0].names,
            names_of(config),
            "{} answers to its configuration's name first, then its id",
            build.entity
        );
    }

    let configs = seeded.fixture_configs(&items);
    assert_eq!(configs.len(), 3, "the seed's three configurations");
    for config in configs {
        let id = config
            .entity
            .key
            .strip_prefix("buildType:")
            .expect("a configuration's key");
        assert_eq!(
            resolve_names_as_parent(&config.payload, "build_config", &declarations),
            names_of(id),
            "{} answers, as a parent, to the names its builds call it by",
            config.entity
        );
    }
}

/// A synced build carries the state chip its descriptor declares (#666),
/// read off the real server's records: the seed's failed integration run is
/// `FAILURE` in the error tone, its successful deploy `SUCCESS` in the ok
/// tone, and every build the seed leaves draws a chip. A configuration draws
/// none.
#[tokio::test]
#[ignore = "needs testenv's seeded TeamCity: `just teamcity-live-seeded`"]
async fn a_synced_build_carries_its_state_chip() {
    use knobas_core::payload::{StateChip, resolve_state};
    use knobas_core::tone::Tone;

    let seeded = seeded();
    seeded.clear_leftovers().await;
    let (items, _) = full(&*seeded.source(serde_json::json!({}))).await;
    let declarations = knobas_source_teamcity::descriptor_template().payload_paths;
    let state = |kind: &str| {
        declarations
            .iter()
            .find(|paths| paths.kind == kind)
            .map(|paths| paths.state.clone())
            .unwrap_or_default()
    };
    let build_state = state("build");
    let chip_of = |real_id: i64| {
        let build = of_kind(&items, "build")
            .into_iter()
            .find(|i| i.entity.key == format!("build:{real_id}"))
            .unwrap_or_else(|| panic!("build id {real_id} missing; got {:?}", keys(&items)));
        resolve_state(&build.payload, &build_state)
    };

    let failed = chip_of(seeded.fixture_build(1187).real_id);
    let passed = chip_of(seeded.fixture_build(412).real_id);
    println!("SEEDED 1187 state chip: {failed:?}");
    println!("SEEDED 412 state chip: {passed:?}");
    assert_eq!(
        failed,
        Some(StateChip {
            text: "FAILURE".to_owned(),
            tone: Tone::Err
        })
    );
    assert_eq!(
        passed,
        Some(StateChip {
            text: "SUCCESS".to_owned(),
            tone: Tone::Ok
        })
    );
    for build in of_kind(&items, "build") {
        assert!(
            resolve_state(&build.payload, &build_state).is_some(),
            "every seeded build draws a chip; {} drew none: state {} status {}",
            build.entity,
            build.payload["state"],
            build.payload["status"]
        );
    }
    let config_state = state("build_config");
    for config in of_kind(&items, "build_config") {
        assert_eq!(resolve_state(&config.payload, &config_state), None);
    }
}

/// A synced build carries the commits it ran on (#710): the declared
/// `revision` resolves on the seeded 1187's record to full shas, read by the
/// sync's own builds request, so the mirror can find a commit's builds in SQL.
/// What asking costs is printed: the same builds page read with and without
/// `revisions(revision(version))`.
#[tokio::test]
#[ignore = "needs testenv's seeded TeamCity: `just teamcity-live-seeded`"]
async fn a_synced_build_carries_the_revisions_it_ran_on() {
    use knobas_core::payload::resolve_list;

    let seeded = seeded();
    seeded.clear_leftovers().await;
    let (items, _) = full(&*seeded.source(serde_json::json!({}))).await;
    let declared = knobas_source_teamcity::descriptor_template()
        .payload_paths
        .into_iter()
        .find(|paths| paths.kind == "build")
        .expect("a build declares its paths")
        .revision;
    let revisions_of = |real_id: i64| {
        let build = of_kind(&items, "build")
            .into_iter()
            .find(|i| i.entity.key == format!("build:{real_id}"))
            .unwrap_or_else(|| panic!("build id {real_id} missing; got {:?}", keys(&items)));
        resolve_list(&build.payload, &declared)
    };
    let failed = revisions_of(seeded.fixture_build(1187).real_id);
    println!("SEEDED 1187 revisions: {failed:?}");
    assert!(!failed.is_empty(), "the seeded 1187 ran on a commit");
    for sha in &failed {
        assert!(
            sha.len() == 40 && sha.chars().all(|c| c.is_ascii_hexdigit()),
            "a revision is a full object id: {sha:?}"
        );
    }
    let with_revision = of_kind(&items, "build")
        .into_iter()
        .filter(|b| !resolve_list(&b.payload, &declared).is_empty())
        .count();
    println!(
        "SEEDED builds with a revision: {with_revision} of {}",
        of_kind(&items, "build").len()
    );

    // The cost of asking, measured: one page of the builds the sync reads,
    // each selector three times, the fastest kept.
    let without = "count,nextHref,build(id,number,buildTypeId,state,status,statusText,branchName,\
                   queuedDate,startDate,finishDate,buildType(id,name,projectId,projectName),\
                   running-info(percentageComplete,currentStageText),triggered(user(username)))";
    // The same, with the revisions asked inside `build(...)`.
    let with = format!(
        "{},revisions(revision(version)))",
        &without[..without.len() - 1]
    );
    for (name, fields) in [("without", without.to_owned()), ("with", with)] {
        let mut best = std::time::Duration::MAX;
        let mut bytes = 0;
        for _ in 0..3 {
            let started = std::time::Instant::now();
            let body = seeded
                .http
                .get(format!("{}/app/rest/builds", seeded.url))
                .query(&[
                    ("locator", "defaultFilter:false,count:100"),
                    ("fields", fields.as_str()),
                ])
                .header("Authorization", format!("Bearer {}", seeded.token))
                .header("Accept", "application/json")
                .send()
                .await
                .expect("the builds read")
                .error_for_status()
                .expect("answered")
                .bytes()
                .await
                .expect("a body");
            best = best.min(started.elapsed());
            bytes = body.len();
        }
        println!("SEEDED builds page {name} revisions: {bytes} bytes, best of 3 {best:?}");
    }
}

/// The Builds frame (ADR-0022, #848) on the seeded server's own records: every
/// seeded configuration is a pipeline with a name and a group, and the seeded
/// 1187 is a run of `Payout_IntegrationTests` -- its declared `pipeline` is the
/// configuration's own -- with its number, its branch, and a start and finish
/// in an ISO 8601 spelling `parse_instant` reads, the finish after the start,
/// and its test summary (#1448).
#[tokio::test]
#[ignore = "needs testenv's seeded TeamCity: `just teamcity-live-seeded`"]
async fn the_seeded_records_fill_the_builds_frame() {
    use knobas_core::payload::{Frame, parse_instant, resolve_string};

    let seeded = seeded();
    seeded.clear_leftovers().await;
    let (items, _) = full(&*seeded.source(serde_json::json!({}))).await;
    let declared = knobas_source_teamcity::descriptor_template().payload_paths;
    let of = |kind: &str| {
        declared
            .iter()
            .find(|paths| paths.kind == kind)
            .cloned()
            .expect("the kind declares its paths")
    };
    let (pipeline, run) = (of("build_config"), of("build"));
    assert_eq!(pipeline.frame, Some(Frame::Pipeline));
    assert_eq!(run.frame, Some(Frame::Run));

    let configs = of_kind(&items, "build_config");
    assert!(!configs.is_empty(), "the seed makes configurations");
    for config in &configs {
        let name = resolve_string(&config.payload, &pipeline.name);
        let group = resolve_string(&config.payload, &pipeline.group);
        println!(
            "SEEDED pipeline {}: name {name:?}, group {group:?}",
            config.entity
        );
        assert!(name.is_some(), "{} has a name", config.entity);
        assert!(group.is_some(), "{} has a group", config.entity);
        assert!(resolve_string(&config.payload, &pipeline.pipeline).is_some());
    }

    let failed = seeded.fixture_build(1187);
    let build = of_kind(&items, "build")
        .into_iter()
        .find(|i| i.entity.key == format!("build:{}", failed.real_id))
        .unwrap_or_else(|| panic!("the seeded 1187 is missing; got {:?}", keys(&items)));
    let field = |paths: &[knobas_core::payload::PayloadPath]| resolve_string(&build.payload, paths);
    println!(
        "SEEDED 1187 as a run: pipeline {:?}, number {:?}, branch {:?}, started {:?}, finished {:?}",
        field(&run.pipeline),
        field(&run.number),
        field(&run.branch),
        field(&run.started),
        field(&run.finished)
    );
    assert_eq!(
        field(&run.pipeline).as_deref(),
        Some(failed.build_type.as_str())
    );
    let own = configs
        .iter()
        .find(|c| {
            resolve_string(&c.payload, &pipeline.pipeline).as_deref()
                == Some(failed.build_type.as_str())
        })
        .expect("the run's pipeline is a mirrored configuration");
    assert_eq!(
        resolve_string(&own.payload, &pipeline.pipeline),
        field(&run.pipeline),
        "a run joins its pipeline on the declared identity"
    );
    assert_eq!(field(&run.number).as_deref(), Some("1187"));
    assert_eq!(
        field(&run.branch).as_deref(),
        Some("feature/PAY-231-sepa-retry")
    );
    let started = field(&run.started)
        .as_deref()
        .and_then(parse_instant)
        .expect("a start instant");
    let finished = field(&run.finished)
        .as_deref()
        .and_then(parse_instant)
        .expect("a finish instant");
    assert!(finished >= started, "{started} .. {finished}");

    // How it was started (#961). The seed queues every build through
    // `/app/rest/buildQueue` with a token, which the server records as a
    // person: so the declared trigger is a person and the declared
    // `started_by` names that account, on 1187 and on every seeded run.
    let trigger = knobas_core::payload::resolve_trigger(&build.payload, &run.trigger);
    println!(
        "SEEDED 1187 trigger {trigger:?}, started by {:?}; triggered {}",
        field(&run.started_by),
        build.payload["triggered"]
    );
    assert_eq!(trigger, Some(knobas_core::payload::Trigger::Person));
    assert!(
        field(&run.started_by).is_some(),
        "a person names their account"
    );
    for b in of_kind(&items, "build") {
        let how = knobas_core::payload::resolve_trigger(&b.payload, &run.trigger);
        let who = resolve_string(&b.payload, &run.started_by);
        if b.payload["triggered"]["type"] == "snapshotDependency" {
            // A step the chain queued for a later step (#1010): the chain
            // started it (#1146), and the account that queued the chain is
            // still on the record, so `started_by` reads.
            assert_eq!(
                how,
                Some(knobas_core::payload::Trigger::Chain),
                "{}",
                b.entity
            );
            assert!(who.is_some(), "{} names who queued the chain", b.entity);
            continue;
        }
        assert!(
            how.is_some(),
            "{} says how it was started: {}",
            b.entity,
            b.payload["triggered"]
        );
        assert_eq!(
            how == Some(knobas_core::payload::Trigger::Person),
            who.is_some(),
            "{}: a person and only a person names an account",
            b.entity
        );
    }

    // Where it ran and what it is doing now (#1004). The test environment
    // has one agent, `knobas-agent` (`docker-compose.yml`), so 1187 and
    // every seeded run that started names it; a finished run is doing
    // nothing now, and a run that has not started names no agent.
    println!(
        "SEEDED 1187 agent {:?}, current step {:?}; agent {}",
        field(&run.agent),
        field(&run.current_step),
        build.payload["agent"]
    );
    assert_eq!(field(&run.agent).as_deref(), Some("knobas-agent"));
    assert_eq!(field(&run.current_step), None, "1187 has finished");
    for b in of_kind(&items, "build") {
        let started = resolve_string(&b.payload, &run.started).is_some();
        let finished = resolve_string(&b.payload, &run.finished).is_some();
        let agent = resolve_string(&b.payload, &run.agent);
        if started {
            assert_eq!(
                agent.as_deref(),
                Some("knobas-agent"),
                "{} ran on the one agent",
                b.entity
            );
        } else {
            assert_eq!(agent, None, "{} has not been taken", b.entity);
        }
        if finished || !started {
            assert_eq!(
                resolve_string(&b.payload, &run.current_step),
                None,
                "{} is not in flight",
                b.entity
            );
        }
    }

    // Its test summary (#1448). The seed's step reports the one failed test
    // 1187's log names and no other, so 1187 reads 1 failed, 0 passed, 0
    // ignored. A seeded run either has a summary with all three counts, or
    // ran no tests and has none.
    let counts = |payload: &serde_json::Value| {
        use knobas_core::payload::resolve_number;
        (
            resolve_number(payload, &run.tests_failed),
            resolve_number(payload, &run.tests_passed),
            resolve_number(payload, &run.tests_ignored),
        )
    };
    println!(
        "SEEDED 1187 tests {:?}; testOccurrences {}",
        counts(&build.payload),
        build.payload["testOccurrences"]
    );
    assert_eq!(counts(&build.payload), (Some(1.0), Some(0.0), Some(0.0)));
    for b in of_kind(&items, "build") {
        let (failed, passed, ignored) = counts(&b.payload);
        println!(
            "SEEDED {} tests {failed:?} {passed:?} {ignored:?}",
            b.entity
        );
        assert!(
            (failed.is_some() && passed.is_some() && ignored.is_some())
                || (failed.is_none() && passed.is_none() && ignored.is_none()),
            "{} has all three counts or none: {}",
            b.entity,
            b.payload["testOccurrences"]
        );
    }
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
/// That deletion is #1477's reproduction: a person deleting the newest build.
/// The refusal names *Backfill* as the way out, and the test takes it: a run
/// with no position (what `knobas_sync::run_backfill` hands the adapter) lands
/// back on the newest seeded build, and the incremental run after it is idle.
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
    // 412, or the change witness's second build where the seed made one on
    // this configuration (#683): the newest seeded build either way.
    let baseline = *seeded
        .seeded_ids_of(QUICK_CONFIGURATION)
        .last()
        .expect("a seeded build");

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
                && message.contains("Backfill on this source's page")),
        "{refused:?}"
    );
    assert!(sink.0.is_empty(), "nothing is emitted from a refused run");
    println!("SEEDED refusal on the deleted build: {refused:?}");

    // #1477: the way out the refusal names. A backfill runs with no position
    // and stores the one it ends on: the newest build that survived.
    let (_, backfilled) = full(&*source).await;
    assert_eq!(
        since_build_id(&backfilled),
        baseline,
        "the backfill starts over and lands on the newest seeded build"
    );
    let (idle, resumed) = sync_from(&*source, Some(backfilled.clone())).await;
    assert!(
        idle.is_empty(),
        "incremental sync carries on from the backfill's position: {:?}",
        keys(&idle)
    );
    assert_eq!(resumed, backfilled, "byte-identical");
    println!("SEEDED backfill after the refusal: {backfilled}");
}

/// The build configuration's source view against the real server (#664,
/// ADR-0013): the seeded configuration lists its seeded build with its
/// outcome, its seeded parameters -- own, inherited and a password that is
/// never drawn -- its step's script, its trigger and its VCS root.
///
/// Read-only. Run the battery beside it for clause 9 over every emitted
/// entity; this test is about what the view *says*.
#[tokio::test]
#[ignore = "needs testenv's seeded TeamCity: `just teamcity-live-seeded`"]
async fn the_seeded_configurations_view_lists_its_builds_parameters_and_steps() {
    use knobas_source::view::{Block, Fact, SourceView, Tone, ViewSection};

    let seeded = seeded();
    seeded.clear_leftovers().await;
    let settings = seeded.seed.settings.clone().unwrap_or_else(|| {
        panic!("seed-state.json has no teamcity.settings -- re-run `./seed --teamcity` (#664)")
    });
    let source = seeded.source(serde_json::json!({}));
    let view_of = async |config: &str| -> SourceView {
        source
            .view(&format!("teamcity:buildType:{config}"), None)
            .await
            .unwrap_or_else(|e| panic!("the view of {config} reads: {e:?}"))
            .unwrap_or_else(|| panic!("TeamCity draws a build configuration ({config})"))
    };
    let section = |view: &SourceView, id: &str| -> ViewSection {
        view.sections
            .iter()
            .find(|s| s.id == id)
            .cloned()
            .unwrap_or_else(|| panic!("no section {id}: {view:#?}"))
    };
    let facts = |block: &Block| -> Vec<Fact> {
        match block {
            Block::Facts { facts } => facts.clone(),
            other => panic!("expected facts, got {other:#?}"),
        }
    };
    let value = |facts: &[Fact], label: &str| -> (String, Option<Tone>) {
        facts
            .iter()
            .find(|f| f.label == label)
            .map(|f| (f.value.clone(), f.tone))
            .unwrap_or_else(|| panic!("no fact {label}: {facts:#?}"))
    };

    let view = view_of(&settings.build_type).await;
    println!(
        "SEEDED VIEW {}:\n{}",
        settings.build_type,
        serde_json::to_string_pretty(&view).expect("serializes")
    );
    let tabs: Vec<_> = view
        .sections
        .iter()
        .map(|s| (s.id.as_str(), s.count))
        .collect();
    assert_eq!(
        tabs,
        [
            ("overview", None),
            ("parameters", Some(4)),
            ("steps", Some(1)),
            ("triggers", Some(1)),
            ("vcs_roots", Some(1)),
        ]
    );

    // Overview: the seeded failed build, by number, outcome and address.
    let overview = section(&view, "overview");
    let overview_facts = facts(&overview.blocks[0]);
    assert_eq!(value(&overview_facts, "Project").0, "Payout");
    assert_eq!(
        value(&overview_facts, "Last build"),
        ("#1187 failed".to_owned(), Some(Tone::Err))
    );
    assert_eq!(
        value(&overview_facts, "Success rate").0,
        "0 % of 1 finished"
    );
    let rows = overview
        .blocks
        .iter()
        .find_map(|b| match b {
            Block::Entities { rows, .. } => Some(rows.clone()),
            _ => None,
        })
        .expect("recent builds");
    let failed = seeded.fixture_build(1187).real_id;
    let row = rows
        .iter()
        .find(|r| r.chip.as_deref() == Some("#1187"))
        .unwrap_or_else(|| panic!("build 1187 is listed: {rows:#?}"));
    assert_eq!(row.tone, Some(Tone::Err));
    assert_eq!(row.entity_id, Some(format!("teamcity:build:{failed}")));
    assert!(
        row.meta
            .as_deref()
            .is_some_and(|m| m.starts_with("feature/PAY-231-sepa-retry · ")),
        "{row:#?}"
    );
    assert!(
        row.title.starts_with("Tests failed: 1"),
        "the real statusText: {row:#?}"
    );

    // Parameters: own, inherited, and the password -- secret, empty, and
    // nowhere in the view.
    let parameters = section(&view, "parameters");
    let [Block::Settings { values, .. }] = parameters.blocks.as_slice() else {
        panic!("one settings block: {parameters:#?}");
    };
    let drawn: Vec<_> = values
        .iter()
        .map(|v| {
            (
                v.name.as_str(),
                v.value.as_str(),
                v.inherited,
                v.secret,
                v.editable,
            )
        })
        .collect();
    assert_eq!(
        drawn,
        [
            ("env.LOG_LEVEL", "info", false, false, true),
            ("env.SEPA_RETRIES", "5", false, false, true),
            (settings.secret_parameter.as_str(), "", false, true, true),
            ("tidewater.region", "eu-central-1", true, false, true),
        ]
    );
    let serialized = serde_json::to_string(&view).expect("serializes");
    assert!(
        !serialized.contains(&settings.secret_value),
        "the password's value is in the view"
    );
    // ...and the server masks it too: the view's mask is not the only one.
    let (status, raw) = seeded
        .get(&format!(
            "app/rest/buildTypes/id:{}/parameters/{}?fields=name,value",
            settings.build_type, settings.secret_parameter
        ))
        .await;
    assert_eq!(status, 200, "{raw}");
    println!("SEEDED password parameter as TeamCity answers it: {raw}");
    assert!(
        !raw.to_string().contains(&settings.secret_value),
        "TeamCity served the password's value: {raw}"
    );

    // Steps: the seeded command-line step and its script.
    let steps = section(&view, "steps");
    let step = facts(&steps.blocks[0]);
    assert_eq!(value(&step, "Runner").0, "Command Line");
    assert!(
        matches!(&steps.blocks[1], Block::Log { text, truncated: false }
            if text.contains("KNOBAS_FIXTURE_LOG") && text.ends_with("exit 1")),
        "{:#?}",
        steps.blocks[1]
    );

    // Trigger and VCS root.
    let trigger = facts(&section(&view, "triggers").blocks[0]);
    assert_eq!(value(&trigger, "Trigger").0, "VCS");
    assert_eq!(value(&trigger, "Branch filter").0, "+:feature/*");
    assert_eq!(
        value(&trigger, "Disabled"),
        ("yes".to_owned(), Some(Tone::Muted))
    );
    let root = facts(&section(&view, "vcs_roots").blocks[0]);
    assert_eq!(value(&root, "Root").0, "Payout / payout-service");
    assert_eq!(value(&root, "Type").0, "Git");
    assert_eq!(
        value(&root, "URL").0,
        "http://gitea:3000/tidewater/payout-service.git"
    );

    // The other seeded configuration's build, which passed.
    let ledger = view_of("Ledger_Deploy_Staging").await;
    let ledger_overview = section(&ledger, "overview");
    // 412, or the change witness's second build, which also passed (#683).
    let newest = *seeded
        .seeded_ids_of("Ledger_Deploy_Staging")
        .last()
        .expect("a seeded build");
    let number = seeded.build(newest).await["number"]
        .as_str()
        .expect("a number")
        .to_owned();
    assert_eq!(
        value(&facts(&ledger_overview.blocks[0]), "Last build"),
        (format!("#{number} success"), Some(Tone::Ok))
    );
    let passed = seeded.fixture_build(412).real_id;
    assert!(
        ledger_overview
            .blocks
            .iter()
            .any(|b| matches!(b, Block::Entities { rows, .. }
            if rows.iter().any(|r| r.entity_id == Some(format!("teamcity:build:{passed}"))
                && r.tone == Some(Tone::Ok)))),
        "{ledger_overview:#?}"
    );
}

/// The failed build's source view against the real server (#665, ADR-0013):
/// it opens on its outcome, lists the test the seed's step reported as failed
/// with its details, shows a log tail holding a line the step printed, lists
/// the artifact the step published, and draws the parameters it ran with
/// without the password's value. The first read asks for no log.
///
/// Read-only. Run the battery beside it for clause 9 over every emitted
/// entity, builds included.
#[tokio::test]
#[ignore = "needs testenv's seeded TeamCity: `just teamcity-live-seeded`"]
async fn the_seeded_failed_builds_view_shows_its_failed_test_and_log_tail() {
    use knobas_source::view::{Block, SourceView, Tone};

    let seeded = seeded();
    seeded.clear_leftovers().await;
    let settings = seeded.seed.settings.clone().unwrap_or_else(|| {
        panic!("seed-state.json has no teamcity.settings -- re-run `./seed --teamcity` (#664)")
    });
    let failed = seeded.fixture_build(1187).real_id;
    let entity = format!("teamcity:build:{failed}");
    let source = seeded.source(serde_json::json!({}));
    let read = async |section: Option<&str>| -> SourceView {
        source
            .view(&entity, section)
            .await
            .unwrap_or_else(|e| panic!("the view of {entity} ({section:?}) reads: {e:?}"))
            .unwrap_or_else(|| panic!("TeamCity draws a build ({entity})"))
    };

    let view = read(None).await;
    println!(
        "SEEDED BUILD VIEW {entity}:\n{}",
        serde_json::to_string_pretty(&view).expect("serializes")
    );
    let tabs: Vec<_> = view
        .sections
        .iter()
        .map(|s| (s.id.as_str(), s.count, s.deferred))
        .collect();
    assert_eq!(
        tabs,
        [
            ("overview", None, false),
            // The configuration's first build: TeamCity lists no change for it.
            ("changes", Some(0), false),
            ("tests", Some(1), true),
            ("log", None, true),
            // The file, and the folder holding it again (#856).
            ("artifacts", Some(2), false),
            // #852 seeded a fourth, `env.LOG_LEVEL`.
            ("parameters", Some(4), false),
        ]
    );
    let Block::Facts { facts } = &view.sections[0].blocks[0] else {
        panic!("{view:#?}");
    };
    let fact = |label: &str| {
        facts
            .iter()
            .find(|f| f.label == label)
            .map(|f| (f.value.clone(), f.tone))
            .unwrap_or_else(|| panic!("no fact {label}: {facts:#?}"))
    };
    assert_eq!(fact("Outcome"), ("failed".to_owned(), Some(Tone::Err)));
    assert_eq!(fact("Number").0, "#1187");
    assert_eq!(fact("Branch").0, "feature/PAY-231-sepa-retry");
    assert_eq!(fact("Triggered by").0, "knobas");
    assert_eq!(fact("Agent").0, "knobas-agent");
    assert!(
        matches!(&view.sections[0].blocks[1], Block::Entities { rows, .. }
            if rows[0].entity_id.as_deref() == Some("teamcity:buildType:Payout_IntegrationTests")),
        "the configuration is an address: {view:#?}"
    );

    let tests = read(Some("tests")).await;
    let blocks = &tests.sections[0].blocks;
    println!(
        "SEEDED BUILD TESTS: {}",
        serde_json::to_string(blocks).expect("serializes")
    );
    assert!(
        matches!(&blocks[0], Block::Entities { rows, .. }
            if rows.len() == 1
                && rows[0].title == "sepa::retry::gives_up_after_max_attempts"
                && rows[0].tone == Some(Tone::Err)),
        "{blocks:#?}"
    );
    assert!(
        matches!(&blocks[1], Block::Text { text, .. }
            if text == "assertion failed: attempts == 5 (left: 6, right: 5)"),
        "{blocks:#?}"
    );

    let log = read(Some("log")).await;
    let [Block::Log { text, truncated }] = log.sections[0].blocks.as_slice() else {
        panic!("{log:#?}");
    };
    println!(
        "SEEDED BUILD LOG TAIL ({} lines, truncated: {truncated}):\n{text}",
        text.lines().count()
    );
    assert!(
        text.lines()
            .any(|l| l.ends_with("test sepa::retry::gives_up_after_max_attempts ... FAILED")),
        "the tail holds a line the seed's step printed"
    );
    assert!(text.lines().count() <= 400);

    let artifacts = &view.sections[4].blocks;
    let Block::Entities { rows, .. } = &artifacts[0] else {
        panic!("{artifacts:#?}");
    };
    assert_eq!(rows.len(), 2, "{artifacts:#?}");
    let file = rows
        .iter()
        .find(|r| r.title == "sepa-report.txt")
        .unwrap_or_else(|| panic!("{artifacts:#?}"));
    let download = file.url.clone().expect("a download URL");
    assert_eq!(seeded.status_of(&download).await, 200, "{download}");

    let parameters = &view.sections[5].blocks;
    let [Block::Settings { values, .. }] = parameters.as_slice() else {
        panic!("{parameters:#?}");
    };
    let secret = values
        .iter()
        .find(|v| v.name == settings.secret_parameter)
        .expect("the password parameter is listed");
    assert!(secret.secret && secret.value.is_empty(), "{secret:#?}");
    let wire = serde_json::to_string(&view).expect("serializes");
    assert!(
        !wire.contains(&settings.secret_value),
        "the password's value is in the view"
    );
}

/// The seeded parameters [`a_seeded_parameter_set_through_the_adapter_reads_back_in_the_next_view`]
/// changes, put back as they were when the guard drops -- passing or
/// panicking alike, [`Queued`]'s way.
///
/// The restore point is **recorded**, not assumed: the own parameter's value
/// is read from the server before anything is written. The inherited one is
/// restored by deleting the own override the test created, which TeamCity
/// answers 204 whether or not there was one, and which brings the project's
/// value back. The password is written only with the seed's own value, so
/// its restore is the same write again.
struct ParameterRestore {
    url: String,
    token: String,
    config: String,
    own: (String, String),
    inherited: String,
    secret: (String, String),
}

impl ParameterRestore {
    fn put(&self) -> Vec<(reqwest::Method, String, Option<String>)> {
        let at = |name: &str| {
            format!(
                "{}/app/rest/buildTypes/id:{}/parameters/{name}",
                self.url, self.config
            )
        };
        vec![
            (
                reqwest::Method::PUT,
                format!("{}/value", at(&self.own.0)),
                Some(self.own.1.clone()),
            ),
            (reqwest::Method::DELETE, at(&self.inherited), None),
            (
                reqwest::Method::PUT,
                format!("{}/value", at(&self.secret.0)),
                Some(self.secret.1.clone()),
            ),
        ]
    }
}

impl Drop for ParameterRestore {
    fn drop(&mut self) {
        let requests = self.put();
        let token = self.token.clone();
        Cleanup::new(
            format!(
                "the live suite did not restore the seeded parameters of {}, so the server is no \
                 longer in the plain-seed state (re-run `./seed --teamcity` to put them back)",
                self.config
            ),
            "they may still hold the values the test wrote",
        )
        .within(CLEANUP_BUDGET)
        .run(move || async move {
            let http = client();
            let mut failures = Vec::new();
            for (verb, url, body) in requests {
                let mut request = http
                    .request(verb.clone(), &url)
                    .bearer_auth(&token)
                    .header("Accept", "text/plain");
                if let Some(body) = body {
                    request = request.header("Content-Type", "text/plain").body(body);
                }
                match request.send().await {
                    Ok(r) if r.status().is_success() => {}
                    Ok(r) => failures.push(format!("{verb} {url} -> {}", r.status())),
                    Err(e) => failures.push(format!("{verb} {url}: {e}")),
                }
            }
            joined(failures)
        });
    }
}

/// Story 19 (spec #660, issue #668), against the real server: a seeded
/// parameter changed through the adapter's `set_parameter` reads back changed
/// in the next view. The same write through knobas' write queue is
/// `knobas-app`'s `teamcity_seeded_live.rs`; this one owns the three cases. Three parameters, the
/// three cases the edit has:
///
/// 1. **An own value** is changed in place.
/// 2. **An inherited value** gets an own override on the configuration: the
///    view now reads it as the configuration's own, and the project's value
///    is untouched.
/// 3. **A password** is written blind and stays a password: its spec is
///    still `password` afterwards, and the view still carries no value.
///
/// # What it changes, and who puts it back
///
/// [`ParameterRestore`], from `Drop`, restores all three to the restore
/// point recorded before the first write, and the test reads them back
/// afterwards to show the fixture is as the seed left it.
#[tokio::test]
#[ignore = "needs testenv's seeded TeamCity: `just teamcity-live-seeded`"]
async fn a_seeded_parameter_set_through_the_adapter_reads_back_in_the_next_view() {
    use knobas_source::WriteOp;
    use knobas_source::view::{Block, Setting};

    const OWN: &str = "env.SEPA_RETRIES";
    const INHERITED: &str = "tidewater.region";

    let seeded = seeded();
    let settings = seeded.seed.settings.clone().unwrap_or_else(|| {
        panic!("seed-state.json has no teamcity.settings -- re-run `./seed --teamcity` (#664)")
    });
    let config = settings.build_type.clone();
    let entity = format!("teamcity:buildType:{config}");
    let param = async |name: &str| -> serde_json::Value {
        let (status, body) = seeded
            .get(&format!(
                "app/rest/buildTypes/id:{config}/parameters/{name}?fields=name,value,inherited,type(rawValue)"
            ))
            .await;
        assert_eq!(status, 200, "{name}: {body}");
        body
    };

    // The restore point, read before anything is written.
    let own_before = param(OWN).await["value"]
        .as_str()
        .expect("the own parameter has a value")
        .to_owned();
    let inherited_before = param(INHERITED).await;
    assert_eq!(
        inherited_before["inherited"], true,
        "the seed's {INHERITED} is the project's, seen on the configuration: {inherited_before}"
    );
    let project_id = seeded.build_type(&config).await["projectId"]
        .as_str()
        .expect("a configuration names its project")
        .to_owned();
    let restore = ParameterRestore {
        url: seeded.url.clone(),
        token: seeded.token.clone(),
        config: config.clone(),
        own: (OWN.to_owned(), own_before.clone()),
        inherited: INHERITED.to_owned(),
        secret: (
            settings.secret_parameter.clone(),
            settings.secret_value.clone(),
        ),
    };

    let source = seeded.source(serde_json::json!({}));
    let parameters = async || -> Vec<Setting> {
        let view = source
            .view(&entity, None)
            .await
            .expect("the view reads")
            .expect("TeamCity draws a build configuration");
        let section = view
            .sections
            .iter()
            .find(|s| s.id == "parameters")
            .unwrap_or_else(|| panic!("no parameters: {view:#?}"));
        let [Block::Settings { values, .. }] = section.blocks.as_slice() else {
            panic!("one settings block: {section:#?}");
        };
        values.clone()
    };
    let named = |values: &[Setting], name: &str| -> Setting {
        values
            .iter()
            .find(|s| s.name == name)
            .cloned()
            .unwrap_or_else(|| panic!("no parameter {name}: {values:#?}"))
    };
    // Each write is edited against the value the view drew, as the shell's
    // is: `None` for a secret, whose value it never drew.
    let set_from = async |name: &str, base: Option<String>, value: &str| {
        source
            .write(WriteOp::SetParameter {
                entity: entity.clone(),
                name: name.to_owned(),
                value: value.to_owned(),
                base,
            })
            .await
    };
    let set = async |name: &str, value: &str| {
        let drawn = named(&parameters().await, name);
        let base = (!drawn.secret).then_some(drawn.value);
        set_from(name, base, value)
            .await
            .unwrap_or_else(|e| panic!("TeamCity takes {name} = {value:?}: {e:?}"));
    };

    let before = parameters().await;
    assert_eq!(named(&before, OWN).value, own_before);
    assert!(before.iter().all(|s| s.editable), "{before:#?}");

    // 1. An own value, changed in place.
    let own_after = "knobas-668 own value";
    set(OWN, own_after).await;
    let after = parameters().await;
    let own = named(&after, OWN);
    assert_eq!(
        (own.value.as_str(), own.inherited),
        (own_after, false),
        "{own:#?}"
    );

    // 1b. The same value edited against what it held before step 1 -- a
    // colleague's change the reader never saw -- is refused, naming the value
    // it holds now, and nothing is written.
    let stale = set_from(OWN, Some(own_before.clone()), "knobas-668 stale edit")
        .await
        .expect_err("an edit against a value that has changed since is refused");
    assert!(
        matches!(&stale, knobas_source::SourceError::Protocol { message, .. }
            if message.contains(&format!("now {own_after:?}"))),
        "{stale:?}"
    );
    assert_eq!(named(&parameters().await, OWN).value, own_after);

    // 2. An inherited value: an own override on the configuration, and the
    // project's value where it was.
    let region_after = "knobas-668-override";
    set(INHERITED, region_after).await;
    let after = parameters().await;
    let region = named(&after, INHERITED);
    assert_eq!(
        (region.value.as_str(), region.inherited),
        (region_after, false),
        "an edit of an inherited value is the configuration's own now: {region:#?}"
    );
    let (status, project_value) = seeded
        .get(&format!(
            "app/rest/projects/id:{project_id}/parameters/{INHERITED}?fields=name,value"
        ))
        .await;
    assert_eq!(status, 200, "{project_value}");
    assert_eq!(
        project_value["value"], inherited_before["value"],
        "the project's own value moved: {project_value}"
    );

    // 3. A password, written blind with the seed's own value, stays a
    // password and is never drawn.
    set(&settings.secret_parameter, &settings.secret_value).await;
    let spec = param(&settings.secret_parameter).await;
    assert!(
        spec["type"]["rawValue"]
            .as_str()
            .is_some_and(|raw| raw.starts_with("password")),
        "a value written to a password parameter left it a password: {spec}"
    );
    let after = parameters().await;
    let secret = named(&after, &settings.secret_parameter);
    assert_eq!(
        (secret.value.as_str(), secret.secret),
        ("", true),
        "{secret:#?}"
    );
    assert!(
        !format!("{after:?}").contains(&settings.secret_value),
        "the password's value is in the view"
    );
    println!(
        "SEEDED {config}: {OWN} {own_before:?} -> {own_after:?} (a stale edit refused: {stale:?}), {INHERITED} inherited {:?} -> own \
         {region_after:?} (project still {:?}), {} written and still a password",
        inherited_before["value"], project_value["value"], settings.secret_parameter
    );

    // The restore, and the proof it happened.
    drop(restore);
    assert_eq!(
        param(OWN).await["value"].as_str(),
        Some(own_before.as_str())
    );
    let region = param(INHERITED).await;
    assert_eq!(
        (&region["value"], &region["inherited"]),
        (&inherited_before["value"], &serde_json::json!(true)),
        "{INHERITED} is the project's again: {region}"
    );
    let restored = parameters().await;
    assert_eq!(
        restored
            .iter()
            .map(|s| (s.name.as_str(), s.value.as_str(), s.inherited, s.secret))
            .collect::<Vec<_>>(),
        before
            .iter()
            .map(|s| (s.name.as_str(), s.value.as_str(), s.inherited, s.secret))
            .collect::<Vec<_>>(),
        "the view after the restore is the view before the first write"
    );
    println!("SEEDED {config}: restored to the recorded point, and read back so");
}

/// Story 21 (#669) against the real server: *Run…* on the seeded
/// configuration, on 1187's feature branch with `env.SEPA_RETRIES`
/// overridden.
///
/// The view offers the action with that branch among its suggestions; the
/// adapter's `run_build` queues the build and names it in its receipt; the
/// build runs **on that branch** with the override in its **resulting
/// parameters**, while the configuration keeps its own value (an override is
/// for one build); and the next incremental run returns exactly that build
/// and its configuration, with the branch. The build is deleted again by
/// [`Queued`], and the server is back in the plain-seed state.
#[tokio::test]
#[ignore = "needs testenv's seeded TeamCity: `just teamcity-live-seeded`"]
async fn a_run_with_an_override_is_built_on_its_branch_and_synced_next() {
    use knobas_source::view::Block;
    use knobas_source::{BuildParameter, WriteOp};

    const BRANCH: &str = "feature/PAY-231-sepa-retry";
    const OVERRIDDEN: &str = "env.SEPA_RETRIES";

    let seeded = seeded();
    let mut guard = Queued::new(&seeded).await;
    let settings = seeded.seed.settings.clone().unwrap_or_else(|| {
        panic!("seed-state.json has no teamcity.settings -- re-run `./seed --teamcity` (#664)")
    });
    let config = settings.build_type.clone();
    let entity = format!("teamcity:buildType:{config}");
    let source = seeded.scoped_to(&[config.as_str()]);
    let own_value = async || -> String {
        let (status, body) = seeded
            .get(&format!(
                "app/rest/buildTypes/id:{config}/parameters/{OVERRIDDEN}?fields=value"
            ))
            .await;
        assert_eq!(status, 200, "{body}");
        body["value"].as_str().unwrap_or_default().to_owned()
    };
    let before = own_value().await;

    let (_, cursor) = full(&*source).await;
    let (idle, _) = sync_from(&*source, Some(cursor.clone())).await;
    assert!(idle.is_empty(), "nothing changed: {:?}", keys(&idle));

    // The view offers Run…, suggesting the branch 1187 ran on.
    let view = source
        .view(&entity, None)
        .await
        .expect("the view reads")
        .expect("TeamCity draws a build configuration");
    let run = view
        .actions
        .iter()
        .find(|a| a.write_op == "run_build")
        .unwrap_or_else(|| panic!("no Run… in the toolbar: {:#?}", view.actions));
    println!("SEEDED Run… action: {run:?}");
    assert!(
        !view
            .sections
            .iter()
            .flat_map(|s| &s.blocks)
            .any(|b| matches!(b, Block::Actions { .. })),
        "the run is the toolbar's, never a section's too: {view:#?}"
    );
    let suggested = run.fields.iter().find_map(|f| match &f.kind {
        knobas_source::view::FieldKind::Text { suggestions } if f.name == "branch" => {
            Some(suggestions)
        }
        _ => None,
    });
    assert!(
        suggested.is_some_and(|s| s.iter().any(|o| o == BRANCH)),
        "1187's branch is suggested: {run:?}"
    );
    assert!(run.with_settings, "the run offers its overrides: {run:?}");

    let receipt = source
        .write(WriteOp::RunBuild {
            entity: entity.clone(),
            branch: Some(BRANCH.to_owned()),
            parameters: vec![BuildParameter {
                name: OVERRIDDEN.to_owned(),
                value: "9".to_owned(),
            }],
            clean_checkout: false,
            first_in_queue: false,
        })
        .await
        .unwrap_or_else(|e| panic!("the run is queued: {e:?}"));
    let id: i64 = receipt
        .remote_id
        .as_deref()
        .and_then(|id| id.parse().ok())
        .unwrap_or_else(|| panic!("the receipt names the queued build: {receipt:?}"));
    guard.adopt(id);
    assert!(
        !seeded.seeded_ids().contains(&id),
        "a new build, not a seeded one: {id}"
    );

    let record = guard.wait_finished(&seeded).await;
    println!("SEEDED run build: {record}");
    assert_eq!(record["branchName"], BRANCH, "{record}");
    assert_eq!(record["defaultBranch"], false, "{record}");

    let (status, resulting) = seeded
        .get(&format!(
            "app/rest/builds/id:{id}/resulting-properties?fields=property(name,value)"
        ))
        .await;
    assert_eq!(status, 200, "{resulting}");
    let ran_with = resulting["property"]
        .as_array()
        .and_then(|ps| ps.iter().find(|p| p["name"] == OVERRIDDEN))
        .map(|p| p["value"].clone());
    println!("SEEDED run build's resulting {OVERRIDDEN}: {ran_with:?}");
    assert_eq!(
        ran_with,
        Some(serde_json::json!("9")),
        "the build ran with the override: {resulting}"
    );
    assert_eq!(
        own_value().await,
        before,
        "the configuration keeps its own value: an override is for one build"
    );

    let (items, _) = sync_from(&*source, Some(cursor.clone())).await;
    assert_eq!(
        keys(&items),
        [format!("buildType:{config}"), format!("build:{id}")],
        "the next incremental run returns exactly the build the run made and its configuration"
    );
    let build = of_kind(&items, "build")[0];
    assert_eq!(build.payload["branchName"], BRANCH, "{:?}", build.payload);
    println!(
        "SEEDED next sync after the run: {:?}, branch {}",
        keys(&items),
        build.payload["branchName"]
    );

    drop(guard);
    assert_eq!(
        seeded.build_ids("defaultFilter:false,count:100").await,
        seeded.seeded_ids_newest_first(),
        "the server is back in the plain-seed state"
    );
}

/// Every agent disabled for as long as the guard lives, so a build queued
/// meanwhile stays queued whatever the machine's speed or the cases before
/// it (#1042: a free agent took the "waiting" build before the run was
/// queued). Built empty, before anything is queued, and told each agent it
/// disabled the moment it did, so a failure half-way still restores them.
/// Only agents that were enabled are touched, and only those are re-enabled
/// -- passing or panicking alike, [`Queued`]'s way. Dropping the guard is
/// how a test lets the agent work again.
struct AgentsParked {
    url: String,
    token: String,
    /// Ids of the agents this guard disabled.
    disabled: Vec<String>,
}

impl AgentsParked {
    fn new(seeded: &Seeded) -> AgentsParked {
        AgentsParked {
            url: seeded.url.clone(),
            token: seeded.token.clone(),
            disabled: Vec::new(),
        }
    }

    async fn park(&mut self, seeded: &Seeded) {
        let (status, body) = seeded
            .get("app/rest/agents?locator=authorized:any&fields=agent(id,enabled)")
            .await;
        assert_eq!(status, 200, "{body}");
        let agents: Vec<(String, bool)> = body["agent"]
            .as_array()
            .map(|rows| {
                rows.iter()
                    .map(|row| {
                        let id = row["id"]
                            .as_i64()
                            .map(|n| n.to_string())
                            .or_else(|| row["id"].as_str().map(str::to_owned))
                            .unwrap_or_default();
                        (id, row["enabled"].as_bool().unwrap_or(true))
                    })
                    .collect()
            })
            .unwrap_or_default();
        assert!(!agents.is_empty(), "the server lists an agent: {body}");
        for (id, enabled) in agents {
            if !enabled {
                continue;
            }
            // Recorded first: a failed request may still have landed.
            self.disabled.push(id.clone());
            let response = seeded
                .http
                .put(format!("{}/app/rest/agents/id:{id}/enabled", seeded.url))
                .header("Content-Type", "text/plain")
                .header("Accept", "text/plain")
                .bearer_auth(&seeded.token)
                .body("false")
                .send()
                .await
                .unwrap_or_else(|e| panic!("disable agent {id}: {e}"));
            assert!(
                response.status().is_success(),
                "disable agent {id} -> {}",
                response.status()
            );
        }
    }
}

impl Drop for AgentsParked {
    fn drop(&mut self) {
        if self.disabled.is_empty() {
            return;
        }
        let (url, token, ids) = (self.url.clone(), self.token.clone(), self.disabled.clone());
        Cleanup::new(
            "the live suite did not re-enable the agents it disabled, so no later build will \
             run (enable them under Agents in TeamCity, or re-seed)",
            format!("agents {ids:?} may still be disabled"),
        )
        .within(CLEANUP_BUDGET)
        .run(move || async move {
            let http = client();
            let mut failures = Vec::new();
            for id in ids {
                let at = format!("{url}/app/rest/agents/id:{id}/enabled");
                match http
                    .put(&at)
                    .header("Content-Type", "text/plain")
                    .header("Accept", "text/plain")
                    .bearer_auth(&token)
                    .body("true")
                    .send()
                    .await
                {
                    Ok(r) if r.status().is_success() => {}
                    Ok(r) => failures.push(format!("PUT {at} -> {}", r.status())),
                    Err(e) => failures.push(format!("PUT {at}: {e}")),
                }
            }
            joined(failures)
        });
    }
}

/// The run options (#852, R-9.7) against the real server. TeamCity takes
/// `triggeringOptions` on the queue and answers none of it back on a read
/// (measured on the seeded 2026.1.3 server, 2026-10-16: `fields=
/// triggeringOptions(..)` answers the build without it), so each option is
/// witnessed by what it does:
///
/// * **first in the queue**: the agent is disabled (#1042: a free one took
///   the earlier builds before the run was queued), two plain builds go on
///   the queue and both stay there; the adapter's run with `first_in_queue`
///   then stands ahead of them in `/app/rest/buildQueue`. The agent is
///   enabled again before the run is waited for;
/// * **clean checkout**: the finished run's log says the checkout was clean.
///
/// Every build is deleted again by its [`Queued`] guard.
#[tokio::test]
#[ignore = "needs testenv's seeded TeamCity: `just teamcity-live-seeded`"]
async fn a_run_with_a_clean_checkout_first_in_the_queue_reaches_teamcity_as_its_options() {
    use knobas_source::WriteOp;

    const BRANCH: &str = "feature/PAY-231-sepa-retry";
    let seeded = seeded();
    // The guard before anything else, so a failure anywhere re-enables the
    // agents; then all three build guards before anything is queued.
    let mut parked = AgentsParked::new(&seeded);
    let mut busy = Queued::new(&seeded).await;
    let mut waiting = Queued::new(&seeded).await;
    let mut ours = Queued::new(&seeded).await;
    let config = seeded.fixture_build(1187).build_type.clone();
    let entity = format!("teamcity:buildType:{config}");
    let source = seeded.scoped_to(&[config.as_str()]);

    parked.park(&seeded).await;
    let busy_id = id_of(&busy.queue(&seeded, &config, BRANCH).await);
    let waiting_id = id_of(&waiting.queue(&seeded, &config, BRANCH).await);
    let queued_now = |order: &[i64]| order.contains(&busy_id) && order.contains(&waiting_id);
    let (status, queue) = seeded
        .get(&format!(
            "app/rest/buildQueue?locator=buildType:(id:{config})&fields=build(id)"
        ))
        .await;
    assert_eq!(status, 200, "{queue}");
    let before: Vec<i64> = queue["build"]
        .as_array()
        .map(|rows| rows.iter().map(id_of).collect())
        .unwrap_or_default();
    assert!(
        queued_now(&before),
        "precondition broken: both plain builds ({busy_id}, {waiting_id}) must be waiting on \
         the queue before the run is queued, with the agents disabled; the queue reads {before:?}"
    );
    // An override, so TeamCity's queue optimisation cannot fold the run into
    // the waiting build as its duplicate (a run equal to a queued one replaces
    // it: measured, 2026-10-16, with the clean checkout mutated away).
    let receipt = source
        .write(WriteOp::RunBuild {
            entity,
            branch: Some(BRANCH.to_owned()),
            parameters: vec![knobas_source::BuildParameter {
                name: "env.SEPA_RETRIES".to_owned(),
                value: "852".to_owned(),
            }],
            clean_checkout: true,
            first_in_queue: true,
        })
        .await
        .unwrap_or_else(|e| panic!("the run is queued: {e:?}"));
    let id: i64 = receipt
        .remote_id
        .as_deref()
        .and_then(|id| id.parse().ok())
        .unwrap_or_else(|| panic!("the receipt names the queued build: {receipt:?}"));
    ours.adopt(id);

    let (status, queue) = seeded
        .get(&format!(
            "app/rest/buildQueue?locator=buildType:(id:{config})&fields=build(id)"
        ))
        .await;
    println!("SEEDED queue after the run: {status} {queue}");
    assert_eq!(status, 200, "{queue}");
    let order: Vec<i64> = queue["build"]
        .as_array()
        .map(|rows| rows.iter().map(id_of).collect())
        .unwrap_or_default();
    let at = |wanted: i64| order.iter().position(|&queued| queued == wanted);
    assert!(
        at(waiting_id).is_some(),
        "the second plain build waits behind the first: {order:?}"
    );
    assert!(
        at(id) < at(waiting_id),
        "first in the queue stands ahead of the build that waited: {order:?}"
    );

    // The agent works again before the run is waited for.
    drop(parked);
    let record = ours.wait_finished(&seeded).await;
    println!("SEEDED run with options: {record}");
    let log = seeded
        .http
        .get(format!("{}/downloadBuildLog.html?buildId={id}", seeded.url))
        .bearer_auth(&seeded.token)
        .send()
        .await
        .expect("the log")
        .text()
        .await
        .unwrap_or_default();
    // The agent's own sentence for `cleanSources` (2026.1.3, 2026-10-16):
    // "Clean build enabled: removing old files from <checkout dir>".
    let clean: Vec<&str> = log
        .lines()
        .filter(|line| line.contains("Clean build enabled"))
        .collect();
    println!("SEEDED log lines naming a clean checkout: {clean:#?}");
    assert!(
        !clean.is_empty(),
        "the agent's log names the clean checkout:\n{log}"
    );
    drop((busy, waiting, ours));
}

/// The whole build queue, each row with the fields a trigger or a re-run is
/// asserted on (#1699). `GET /app/rest/buildQueue` rather than the builds
/// locator, so a build put on the queue for **any** configuration is on the
/// page: a write aimed at the wrong configuration must show up here to be
/// caught.
async fn queue(seeded: &Seeded) -> Vec<serde_json::Value> {
    let (status, body) = seeded
        .get("app/rest/buildQueue?fields=build(id,state,buildType(id),branchName,defaultBranch)")
        .await;
    assert_eq!(status, 200, "GET buildQueue: {body}");
    body["build"].as_array().cloned().unwrap_or_default()
}

/// Hand the guard the first build on the queue, if there is one, before
/// anything is asserted about it: a write that queued something it should not
/// have is still cleaned up when the assertion after this fails.
fn adopt_first(guard: &mut Queued, queue: &[serde_json::Value]) {
    if let Some(row) = queue.first() {
        guard.adopt(id_of(row));
    }
}

/// The restore point a write test records before it writes (#1699): every
/// build on the server, which a green run leaves as exactly the seeded ones.
async fn restore_point(seeded: &Seeded, when: &str) -> Vec<i64> {
    let ids = seeded.build_ids("defaultFilter:false,count:100").await;
    println!(
        "SEEDED restore point {when}: {} build(s) {ids:?}",
        ids.len()
    );
    assert_eq!(
        ids,
        seeded.seeded_ids_newest_first(),
        "{when}, the server holds exactly the seeded builds"
    );
    ids
}

/// **Story 9 against the real server** (#1699, ADR-0013): the adapter's
/// `TriggerBuild` puts exactly one build on TeamCity's queue, of the
/// configuration the entity named; and a configuration the server does not
/// have is a refusal (a 404 the write queue does not retry), with nothing
/// queued in its place.
///
/// The agents are parked ([`AgentsParked`]) so the build stays on the queue
/// where the whole queue can be read, rather than being taken by the agent
/// between the write and the read; the [`Queued`] guards cancel and delete
/// whatever the writes queued, and the build counts are back at the restore
/// point afterwards. The source is scoped to another configuration first, so
/// a trigger that ran whatever was first in scope queues the wrong one.
#[tokio::test]
#[ignore = "needs testenv's seeded TeamCity: `just teamcity-live-seeded`"]
async fn a_trigger_through_the_adapter_queues_one_build_of_the_configuration_it_named() {
    use knobas_source::WriteOp;

    let seeded = seeded();
    // The agent guard first, so a failure anywhere re-enables the agents; the
    // build guards drop before it, while their builds are still queued.
    let mut parked = AgentsParked::new(&seeded);
    let mut refused_guard = Queued::new(&seeded).await;
    let mut guard = Queued::new(&seeded).await;
    let named = QUICK_CONFIGURATION;
    let other = seeded.fixture_build(1187).build_type.clone();
    let source = seeded.scoped_to(&[other.as_str(), named]);
    let before = restore_point(&seeded, "before the trigger").await;
    parked.park(&seeded).await;
    assert_eq!(
        queue(&seeded).await,
        Vec::<serde_json::Value>::new(),
        "the queue starts empty"
    );

    let refused = source
        .write(WriteOp::TriggerBuild {
            entity: "teamcity:buildType:Knobas_NoSuchConfiguration1699".to_owned(),
        })
        .await;
    let after_refusal = queue(&seeded).await;
    adopt_first(&mut refused_guard, &after_refusal);
    println!("SEEDED trigger of a configuration the server does not have: {refused:?}");
    assert!(
        matches!(
            refused,
            Err(SourceError::Protocol {
                status: Some(404),
                ..
            })
        ),
        "a configuration that is gone is a refusal, not a retry: {refused:?}"
    );
    assert!(
        after_refusal.is_empty(),
        "a refused trigger queues nothing: {after_refusal:?}"
    );

    let receipt = source
        .write(WriteOp::TriggerBuild {
            entity: format!("teamcity:buildType:{named}"),
        })
        .await
        .unwrap_or_else(|e| panic!("the trigger is queued: {e:?}"));
    let queued = queue(&seeded).await;
    adopt_first(&mut guard, &queued);
    println!("SEEDED queue after the trigger: {queued:?} (receipt {receipt:?})");
    assert_eq!(queued.len(), 1, "exactly one build was queued: {queued:?}");
    let build = &queued[0];
    assert_eq!(
        build["buildType"]["id"], named,
        "the build is of the configuration the trigger named: {build}"
    );
    assert_eq!(build["state"], "queued", "{build}");
    // A trigger names no branch (`write.rs`, *One endpoint, two operations*),
    // so TeamCity builds the configuration's default one.
    assert_eq!(build["defaultBranch"], true, "{build}");
    assert!(
        !seeded.seeded_ids().contains(&id_of(build)),
        "a new build, not a seeded one: {build}"
    );

    drop((guard, refused_guard, parked));
    let after = restore_point(&seeded, "after the trigger").await;
    assert_eq!(after, before);
}

/// **Story 10 against the real server** (#1699, ADR-0013): the adapter's
/// `RerunBuild` on the seeded failed build 1187 asks the server which
/// configuration the build belonged to and queues exactly one new build of
/// **that** configuration; and once that build is deleted, re-running it is a
/// refusal on TeamCity's own 404 for a build that is gone, with nothing
/// queued.
///
/// Parked agents, guards and restore point as in the trigger's case above.
/// The source is scoped to another configuration first, so a re-run that ran
/// whatever was first in scope rather than the build's own configuration
/// queues the wrong one; the build's entity id carries nothing but its id.
#[tokio::test]
#[ignore = "needs testenv's seeded TeamCity: `just teamcity-live-seeded`"]
async fn a_rerun_through_the_adapter_queues_the_builds_own_configuration_and_a_gone_build_is_refused()
 {
    use knobas_source::WriteOp;

    let seeded = seeded();
    let mut parked = AgentsParked::new(&seeded);
    let mut refused_guard = Queued::new(&seeded).await;
    let mut guard = Queued::new(&seeded).await;
    let failed = seeded.fixture_build(1187).clone();
    let source = seeded.scoped_to(&[QUICK_CONFIGURATION, failed.build_type.as_str()]);
    let before = restore_point(&seeded, "before the re-run").await;
    parked.park(&seeded).await;
    assert_eq!(
        queue(&seeded).await,
        Vec::<serde_json::Value>::new(),
        "the queue starts empty"
    );

    let receipt = source
        .write(WriteOp::RerunBuild {
            entity: format!("teamcity:build:{}", failed.real_id),
        })
        .await
        .unwrap_or_else(|e| panic!("the re-run is queued: {e:?}"));
    let queued = queue(&seeded).await;
    adopt_first(&mut guard, &queued);
    println!(
        "SEEDED queue after re-running build {} (1187): {queued:?} (receipt {receipt:?})",
        failed.real_id
    );
    assert_eq!(queued.len(), 1, "exactly one build was queued: {queued:?}");
    let build = &queued[0];
    let rerun = id_of(build);
    assert_eq!(
        build["buildType"]["id"], failed.build_type,
        "a re-run runs the build's own configuration: {build}"
    );
    assert_eq!(build["state"], "queued", "{build}");
    // 1187 ran on a feature branch, and a re-run still names none: the
    // configuration's default branch is built, as `write.rs` rules.
    assert_eq!(build["defaultBranch"], true, "{build}");
    assert_ne!(rerun, failed.real_id, "a re-run is a new build");
    assert!(
        !seeded.seeded_ids().contains(&rerun),
        "a new build, not a seeded one: {build}"
    );

    // The re-run's build deleted (and checked gone by the guard): re-running
    // it now is re-running a build the server does not have.
    drop(guard);
    let refused = source
        .write(WriteOp::RerunBuild {
            entity: format!("teamcity:build:{rerun}"),
        })
        .await;
    let after_refusal = queue(&seeded).await;
    adopt_first(&mut refused_guard, &after_refusal);
    println!("SEEDED re-run of deleted build {rerun}: {refused:?}");
    assert!(
        matches!(
            refused,
            Err(SourceError::Protocol {
                status: Some(404),
                ..
            })
        ),
        "a build that is gone is a refusal, not a retry: {refused:?}"
    );
    assert!(
        after_refusal.is_empty(),
        "a refused re-run queues nothing: {after_refusal:?}"
    );

    drop((refused_guard, parked));
    let after = restore_point(&seeded, "after the re-run").await;
    assert_eq!(after, before);
}

/// **The cleanup keeps the fixture the seed's** (#1560): a project the seed
/// does not own, with a configuration in it, is gone after `clear_leftovers`,
/// and the seed's own projects are still there. Nothing else witnesses the
/// project half of the guard, since a clean server has no stray to delete. A
/// run killed between the create and the check leaves the stray for the next
/// run's guard, which is the thing under test.
#[tokio::test]
#[ignore = "needs testenv's seeded TeamCity: `just teamcity-live-seeded`"]
async fn a_project_the_seed_does_not_own_is_deleted_by_the_cleanup() {
    const STRAY: &str = "KnobasStray1560";
    let seeded = seeded();
    seeded.clear_leftovers().await;
    let post = |path: &str, body: serde_json::Value| {
        seeded
            .http
            .post(format!("{}/{path}", seeded.url))
            .bearer_auth(&seeded.token)
            .header("Accept", "application/json")
            .json(&body)
            .send()
    };
    let r = post(
        "app/rest/projects",
        serde_json::json!({ "id": STRAY, "name": STRAY, "parentProject": { "id": "_Root" } }),
    )
    .await
    .expect("create the stray project");
    assert!(r.status().is_success(), "create {STRAY}: {}", r.status());
    let r = post(
        "app/rest/buildTypes",
        serde_json::json!({
            "id": format!("{STRAY}_Cfg"), "name": "Cfg", "project": { "id": STRAY }
        }),
    )
    .await
    .expect("create a configuration in the stray");
    assert!(
        r.status().is_success(),
        "create the configuration: {}",
        r.status()
    );

    seeded.clear_leftovers().await;

    let (status, _) = seeded.get(&format!("app/rest/projects/id:{STRAY}")).await;
    assert_eq!(status, 404, "the stray project is gone");
    let (status, _) = seeded
        .get(&format!("app/rest/buildTypes/id:{STRAY}_Cfg"))
        .await;
    assert_eq!(status, 404, "and its configuration with it");
    for owned in seeded.owned_projects().iter().filter(|p| *p != "_Root") {
        let (status, body) = seeded.get(&format!("app/rest/projects/id:{owned}")).await;
        assert_eq!(status, 200, "the seed's project {owned} is kept: {body}");
    }
}

/// The nested project [`a_configured_project_takes_in_its_nested_projects_and_writes_their_path`]
/// makes under the seeded `Payout`, deleted again when the guard drops --
/// passing or panicking alike, [`Queued`]'s way. Deleting the outer project
/// deletes the one inside it and the configuration with them.
struct NestedProject {
    url: String,
    token: String,
}

/// The outer project's id; the inner one is `<this>_Deeper`.
const NESTED: &str = "KnobasNested849";

impl NestedProject {
    async fn delete(http: &reqwest::Client, url: &str, token: &str) -> Result<(), String> {
        let at = format!("{url}/app/rest/projects/id:{NESTED}");
        match http.delete(&at).bearer_auth(token).send().await {
            Ok(r) if r.status().is_success() || r.status().as_u16() == 404 => Ok(()),
            Ok(r) => Err(format!("DELETE {at} -> {}", r.status())),
            Err(e) => Err(format!("DELETE {at}: {e}")),
        }
    }

    /// Clear what a killed run left, then make `Payout › KnobasNested849 ›
    /// Deeper` with one configuration in the innermost.
    async fn new(seeded: &Seeded) -> NestedProject {
        Self::delete(&seeded.http, &seeded.url, &seeded.token)
            .await
            .expect("a leftover nested project is cleared");
        let post = |path: &str, body: serde_json::Value| {
            seeded
                .http
                .post(format!("{}/{path}", seeded.url))
                .bearer_auth(&seeded.token)
                .header("Accept", "application/json")
                .json(&body)
                .send()
        };
        for (id, name, parent) in [
            (NESTED.to_owned(), "KnobasNested849", "Payout".to_owned()),
            (format!("{NESTED}_Deeper"), "Deeper", NESTED.to_owned()),
        ] {
            let r = post(
                "app/rest/projects",
                serde_json::json!({ "id": id, "name": name, "parentProject": { "id": parent } }),
            )
            .await
            .expect("create a project");
            assert!(
                r.status().is_success(),
                "create project {id}: {}",
                r.status()
            );
        }
        let r = post(
            "app/rest/buildTypes",
            serde_json::json!({
                "id": format!("{NESTED}_Deeper_Probe"), "name": "Probe",
                "project": { "id": format!("{NESTED}_Deeper") }
            }),
        )
        .await
        .expect("create a configuration");
        assert!(r.status().is_success(), "create the probe: {}", r.status());
        NestedProject {
            url: seeded.url.clone(),
            token: seeded.token.clone(),
        }
    }
}

impl Drop for NestedProject {
    fn drop(&mut self) {
        let (url, token) = (self.url.clone(), self.token.clone());
        Cleanup::new(
            "the live suite did not delete the nested project it made",
            format!("{NESTED} may still be standing; the next run's leftover clearing deletes it"),
        )
        .within(CLEANUP_BUDGET)
        .run(move || async move { NestedProject::delete(&client(), &url, &token).await });
    }
}

/// **The project tree** (#849, R-9.3): a source scoped to the seeded `Payout`
/// takes in a configuration two projects further down, and every
/// configuration carries its chain of projects, which the pipeline's declared
/// `group_path` reads outermost first. `Ledger`, beside `Payout`, stays out.
#[tokio::test]
#[ignore = "needs testenv's seeded TeamCity: `just teamcity-live-seeded`"]
async fn a_configured_project_takes_in_its_nested_projects_and_writes_their_path() {
    let seeded = seeded();
    seeded.clear_leftovers().await;
    let _nested = NestedProject::new(&seeded).await;
    let (items, _) = full(&*seeded.source(serde_json::json!({ "project_ids": ["Payout"] }))).await;
    let declared = knobas_source_teamcity::descriptor_template().payload_paths;
    let pipeline = declared
        .iter()
        .find(|paths| paths.kind == "build_config")
        .expect("the configuration declares its paths");

    let configs = of_kind(&items, "build_config");
    let mut got: Vec<&str> = configs.iter().map(|i| i.entity.key.as_str()).collect();
    got.sort_unstable();
    assert_eq!(
        got,
        [
            "buildType:KnobasNested849_Deeper_Probe",
            "buildType:Payout_Build",
            "buildType:Payout_IntegrationTests",
        ],
        "Payout's own and its nested project's, and none of Ledger's"
    );
    let path = |key: &str| {
        let it = configs.iter().find(|i| i.entity.key == key).expect(key);
        knobas_core::payload::resolve_list(&it.payload, &pipeline.group_path)
    };
    let deep = path("buildType:KnobasNested849_Deeper_Probe");
    println!("SEEDED nested probe path: {deep:?}");
    assert_eq!(deep, ["Payout", "KnobasNested849", "Deeper"]);
    assert_eq!(path("buildType:Payout_Build"), ["Payout"]);
}

/// The pipeline page's older history (#851, R-9.5) on the seeded server:
/// asked for the runs of the seeded configuration with the most builds older
/// than its newest seeded build, the adapter answers every other seeded build of that
/// configuration, newest first, without the one it was asked from, each the
/// build item the sync makes -- a run of that configuration by the declared
/// `pipeline` path. Asked from the oldest, it answers nothing: the start of
/// the history. A count of one answers one.
#[tokio::test]
#[ignore = "needs testenv's seeded TeamCity: `just teamcity-live-seeded`"]
async fn older_runs_page_back_through_a_configurations_history() {
    use knobas_core::payload::resolve_string;

    let seeded = seeded();
    seeded.clear_leftovers().await;
    // The seeded configuration with the most builds: the change witness's
    // (its fixture build and the witness's two) on the current seed.
    let config = seeded
        .seed
        .builds
        .iter()
        .map(|b| b.build_type.clone())
        .max_by_key(|c| seeded.seeded_ids_of(c).len())
        .expect("the seed made builds");
    let mut ids = seeded.seeded_ids_of(&config);
    ids.reverse();
    assert!(
        ids.len() >= 2,
        "{config} has at least two seeded builds: {ids:?}"
    );
    let source = seeded.source(serde_json::json!({}));
    let pipeline = format!("teamcity:buildType:{config}");
    let run = |id: i64| format!("teamcity:build:{id}");

    let older = source
        .older_runs(&pipeline, &run(ids[0]), 50)
        .await
        .expect("the older runs read")
        .expect("TeamCity keeps pipelines");
    println!(
        "SEEDED older than {}: {:?}",
        ids[0],
        older
            .iter()
            .map(|i| i.entity.key.clone())
            .collect::<Vec<_>>()
    );
    let declared = knobas_source_teamcity::descriptor_template().payload_paths;
    let run_paths = declared
        .iter()
        .find(|p| p.kind == "build")
        .expect("a build declares its paths");
    assert_eq!(
        older
            .iter()
            .map(|i| i.entity.key.clone())
            .collect::<Vec<_>>(),
        ids[1..]
            .iter()
            .map(|id| format!("build:{id}"))
            .collect::<Vec<_>>(),
        "every older seeded build, newest first, not the one asked from"
    );
    for item in &older {
        assert_eq!(item.kind, "build");
        assert_eq!(
            resolve_string(&item.payload, &run_paths.pipeline).as_deref(),
            Some(config.as_str())
        );
    }

    let one = source
        .older_runs(&pipeline, &run(ids[0]), 1)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(one.len(), 1);
    let none = source
        .older_runs(&pipeline, &run(*ids.last().unwrap()), 50)
        .await
        .unwrap()
        .unwrap();
    assert!(none.is_empty(), "the start of the history: {none:?}");
}

/// The pipeline page's settings (#851, R-9.4/R-9.6) on the seeded server:
/// `Payout_IntegrationTests` answers its repositories, triggers and
/// parameters, and nothing in the answer carries a credential -- no URL with
/// a `user@`, no password parameter's value.
#[tokio::test]
#[ignore = "needs testenv's seeded TeamCity: `just teamcity-live-seeded`"]
async fn a_configurations_settings_come_back_without_a_credential() {
    let seeded = seeded();
    let config = seeded.fixture_build(1187).build_type.clone();
    let source = seeded.source(serde_json::json!({}));
    let settings = source
        .pipeline_settings(&format!("teamcity:buildType:{config}"))
        .await
        .expect("the settings read")
        .expect("TeamCity keeps pipelines");
    println!("SEEDED settings of {config}: {settings:#?}");
    for repository in &settings.repositories {
        if let Some(url) = &repository.url {
            let authority = url.split("://").nth(1).unwrap_or(url);
            let authority = authority.split('/').next().unwrap_or(authority);
            assert!(!authority.contains('@'), "{url}");
        }
    }
    for parameter in settings.parameters.iter().filter(|p| p.secret) {
        assert!(parameter.value.is_empty(), "{}", parameter.name);
    }
    assert_eq!(
        settings.parameter_write_op.as_deref(),
        Some("set_parameter")
    );
    // The seeded select (#852): its spec as the real server keeps it, parsed.
    assert_eq!(
        settings
            .parameter_specs
            .iter()
            .map(|p| p.name.clone())
            .collect::<Vec<_>>(),
        settings
            .parameters
            .iter()
            .map(|p| p.name.clone())
            .collect::<Vec<_>>(),
        "one spec per parameter, in the same order"
    );
    let level = settings
        .parameter_specs
        .iter()
        .find(|p| p.name == "env.LOG_LEVEL")
        .unwrap_or_else(|| panic!("the seed's select: {:#?}", settings.parameter_specs));
    assert_eq!(
        level.kind,
        knobas_source::pipeline::ParameterKind::Select {
            options: vec![
                knobas_source::pipeline::ParameterOption {
                    value: "info".into(),
                    label: None,
                },
                knobas_source::pipeline::ParameterOption {
                    value: "debug".into(),
                    label: Some("Debug".into()),
                },
            ]
        }
    );
    assert!(
        settings.chain.is_empty(),
        "the seeded configurations wait on nothing: {:?}",
        settings.chain
    );
    let wire = serde_json::to_string(&settings).unwrap();
    assert!(
        !wire.contains(&seeded.token),
        "the token is nowhere in the answer"
    );
}

/// The steps a run takes (#1016, R-9.10) on the seeded server:
/// `Payout_Build` names its five in order, the ones a running run's log has
/// not reached yet; and a finished run's log heads its steps with the names
/// the settings give, which is what lets the run page put the two together.
#[tokio::test]
#[ignore = "needs testenv's seeded TeamCity: `just teamcity-live-seeded`"]
async fn a_pipelines_settings_name_the_steps_its_runs_log_heads() {
    let seeded = seeded();
    let source = seeded.source(serde_json::json!({}));
    let settings_of = async |config: &str| {
        source
            .pipeline_settings(&format!("teamcity:buildType:{config}"))
            .await
            .expect("the settings read")
            .expect("TeamCity keeps pipelines")
    };
    let build = settings_of("Payout_Build").await;
    println!("SEEDED steps of Payout_Build: {:?}", build.steps);
    assert_eq!(
        build.steps,
        ["step 1", "step 2", "cargo test", "step 4", "step 5"],
        "the seed's five steps, the fixture's `cargo test` third"
    );

    let failed = seeded.fixture_build(1187);
    let tests = settings_of(&failed.build_type).await;
    let log = source
        .run_log(
            &format!("teamcity:build:{}", failed.real_id),
            None,
            knobas_source::run::LOG_LINES_MAX,
        )
        .await
        .expect("the log reads")
        .expect("TeamCity keeps runs");
    let headed: Vec<&str> = log.steps.iter().map(|s| s.name.as_str()).collect();
    println!(
        "SEEDED steps of {}: settings {:?}, log {headed:?}",
        failed.build_type, tests.steps
    );
    assert_eq!(headed, tests.steps, "the log heads each step by its name");
}

/// The chain [`a_chain_names_its_steps_and_each_runs_build_on_the_branch`]
/// makes under the seeded `Payout`: `Build`, `Docker` waiting on it, `Deploy`
/// waiting on `Docker`, each building payout-service with one `echo` step.
/// Deleted again when the guard drops, [`NestedProject`]'s way; deleting the
/// project deletes its configurations and their builds.
struct ChainProject {
    url: String,
    token: String,
}

/// The chain's project id; its configurations are `<this>_<step>`.
const CHAIN: &str = "KnobasChain857";

impl ChainProject {
    async fn delete(http: &reqwest::Client, url: &str, token: &str) -> Result<(), String> {
        let at = format!("{url}/app/rest/projects/id:{CHAIN}");
        match http.delete(&at).bearer_auth(token).send().await {
            Ok(r) if r.status().is_success() || r.status().as_u16() == 404 => Ok(()),
            Ok(r) => Err(format!("DELETE {at} -> {}", r.status())),
            Err(e) => Err(format!("DELETE {at}: {e}")),
        }
    }

    async fn new(seeded: &Seeded) -> ChainProject {
        Self::delete(&seeded.http, &seeded.url, &seeded.token)
            .await
            .expect("a leftover chain is cleared");
        // The guard comes first, so a setup that fails half-way (the project
        // made, a configuration refused) is deleted on the unwind too, and no
        // half chain is left under `Payout` for the suite's later tests.
        let guard = ChainProject {
            url: seeded.url.clone(),
            token: seeded.token.clone(),
        };
        let post = async |path: String, body: serde_json::Value| {
            let r = seeded
                .http
                .post(format!("{}/{path}", seeded.url))
                .bearer_auth(&seeded.token)
                .header("Accept", "application/json")
                .json(&body)
                .send()
                .await
                .expect("the server answers");
            let status = r.status();
            let text = r.text().await.unwrap_or_default();
            assert!(status.is_success(), "POST {path} -> {status}: {text}");
            text
        };
        post(
            "app/rest/projects".into(),
            serde_json::json!({ "id": CHAIN, "name": CHAIN, "parentProject": { "id": "Payout" } }),
        )
        .await;
        let mut before: Option<String> = None;
        for step in ["Build", "Docker", "Deploy"] {
            let id = format!("{CHAIN}_{step}");
            post(
                "app/rest/buildTypes".into(),
                serde_json::json!({ "id": id, "name": step, "project": { "id": CHAIN } }),
            )
            .await;
            post(
                format!("app/rest/buildTypes/id:{id}/vcs-root-entries"),
                serde_json::json!({ "vcs-root": { "id": "Payout_PayoutService" } }),
            )
            .await;
            post(
                format!("app/rest/buildTypes/id:{id}/steps"),
                serde_json::json!({ "name": "echo", "type": "simpleRunner", "properties": { "property": [
                    { "name": "script.content", "value": format!("echo {step}") },
                    { "name": "use.custom.script", "value": "true" } ] } }),
            )
            .await;
            if let Some(dep) = &before {
                post(
                    format!("app/rest/buildTypes/id:{id}/snapshot-dependencies"),
                    serde_json::json!({ "id": dep, "type": "snapshot_dependency",
                                        "source-buildType": { "id": dep } }),
                )
                .await;
            }
            before = Some(id);
        }
        guard
    }

    /// Queue one build of the chain's `step` on `branch` and wait for it to
    /// finish, with every build the chain queued for it. Answers its id.
    async fn run(seeded: &Seeded, step: &str, branch: &str) -> i64 {
        let r = seeded
            .http
            .post(format!("{}/app/rest/buildQueue", seeded.url))
            .bearer_auth(&seeded.token)
            .header("Accept", "application/json")
            .json(&serde_json::json!({
                "buildType": { "id": format!("{CHAIN}_{step}") }, "branchName": branch,
                "comment": { "text": "queued by the knobas TeamCity live suite (issue #857)" }
            }))
            .send()
            .await
            .expect("queue a build");
        let body: serde_json::Value = r.json().await.unwrap_or_default();
        let id = id_of(&body);
        let deadline = std::time::Instant::now() + BUILD_BUDGET;
        loop {
            let record = seeded.build(id).await;
            if record["state"] == "finished" {
                return id;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "build {id} did not finish within {BUILD_BUDGET:?}: {record}"
            );
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }
}

impl Drop for ChainProject {
    fn drop(&mut self) {
        let (url, token) = (self.url.clone(), self.token.clone());
        Cleanup::new(
            "the live suite did not delete the chain project it made",
            format!("{CHAIN} may still be standing; the next run's leftover clearing deletes it"),
        )
        .within(CLEANUP_BUDGET)
        .run(move || async move { ChainProject::delete(&client(), &url, &token).await });
    }
}

/// **Build chains** (#857, R-9.13) on the seeded server: each configuration of
/// a chain of snapshot dependencies names the whole chain in running order,
/// and a build's chain gives each step its build -- the one it built on, the
/// one that built on it, else the step's newest on the branch, else none.
#[tokio::test]
#[ignore = "needs testenv's seeded TeamCity: `just teamcity-live-seeded`"]
async fn a_chain_names_its_steps_and_each_runs_build_on_the_branch() {
    const FEATURE: &str = "feature/PAY-231-sepa-retry";
    const FIX: &str = "fix/PAY-228-partial-refund-drift";
    let seeded = seeded();
    seeded.clear_leftovers().await;
    let _chain = ChainProject::new(&seeded).await;
    let source = seeded.source(serde_json::json!({}));
    let pipeline = |step: &str| format!("teamcity:buildType:{CHAIN}_{step}");
    let run = |id: i64| format!("teamcity:build:{id}");

    for step in ["Build", "Docker", "Deploy"] {
        let settings = source
            .pipeline_settings(&pipeline(step))
            .await
            .expect("the settings read")
            .expect("TeamCity keeps pipelines");
        let got: Vec<(String, String)> = settings
            .chain
            .iter()
            .map(|s| (s.pipeline.clone(), s.name.clone()))
            .collect();
        assert_eq!(
            got,
            [
                (pipeline("Build"), "Build".to_owned()),
                (pipeline("Docker"), "Docker".to_owned()),
                (pipeline("Deploy"), "Deploy".to_owned()),
            ],
            "{step} names the whole chain, first step first"
        );
    }

    // Deploy on the feature branch runs Build and Docker first.
    let deploy = ChainProject::run(&seeded, "Deploy", FEATURE).await;
    let chained = seeded
        .build_ids(&format!(
            "snapshotDependency:(to:(id:{deploy}),includeInitial:true),branch:default:any,count:10"
        ))
        .await;
    println!("SEEDED chain builds: {chained:?}");
    assert_eq!(chained.len(), 3, "Deploy ran Build and Docker: {chained:?}");
    // A newer Build on the same branch that nothing built on, and one on
    // another branch.
    let newer = ChainProject::run(&seeded, "Build", FEATURE).await;
    let alone = ChainProject::run(&seeded, "Build", FIX).await;

    let steps_of = async |id: i64| -> Vec<(String, Option<String>, Option<String>)> {
        let steps = source
            .run_chain(&run(id))
            .await
            .expect("the chain read")
            .expect("TeamCity keeps pipelines");
        println!("SEEDED chain of build {id}: {steps:#?}");
        steps
            .iter()
            .map(|s| {
                (
                    s.step.name.clone(),
                    s.run.as_ref().map(|r| r.entity.to_string()),
                    s.run
                        .as_ref()
                        .and_then(|r| r.payload["branchName"].as_str().map(str::to_owned)),
                )
            })
            .collect()
    };
    let mut of_step = std::collections::HashMap::new();
    for id in &chained {
        let (_, record) = seeded
            .get(&format!("app/rest/builds/id:{id}?fields=buildTypeId"))
            .await;
        of_step.insert(
            record["buildTypeId"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            *id,
        );
    }
    let (build, docker) = (
        of_step[&format!("{CHAIN}_Build")],
        of_step[&format!("{CHAIN}_Docker")],
    );
    let feature = Some(FEATURE.to_owned());

    // Docker's chain: the Build it built on, itself, the Deploy built on it.
    assert_eq!(
        steps_of(docker).await,
        [
            ("Build".to_owned(), Some(run(build)), feature.clone()),
            ("Docker".to_owned(), Some(run(docker)), feature.clone()),
            ("Deploy".to_owned(), Some(run(deploy)), feature.clone()),
        ]
    );
    // The newer Build: itself, then the newest Docker and Deploy on its branch.
    assert_eq!(
        steps_of(newer).await,
        [
            ("Build".to_owned(), Some(run(newer)), feature.clone()),
            ("Docker".to_owned(), Some(run(docker)), feature.clone()),
            ("Deploy".to_owned(), Some(run(deploy)), feature.clone()),
        ]
    );
    // On the fix branch only Build ran.
    assert_eq!(
        steps_of(alone).await,
        [
            ("Build".to_owned(), Some(run(alone)), Some(FIX.to_owned())),
            ("Docker".to_owned(), None, None),
            ("Deploy".to_owned(), None, None),
        ]
    );
    // A seeded build in no chain has none.
    let lone = seeded.fixture_build(1187).real_id;
    assert!(
        source
            .run_chain(&run(lone))
            .await
            .expect("the chain read")
            .expect("TeamCity keeps pipelines")
            .is_empty()
    );
}

/// **The chain the seed leaves** (#1010): `./seed --teamcity` makes three
/// configurations in a project of their own, each waiting on the one before,
/// and one build of each on a feature branch, so the Builds view's chain line
/// and the run's *Chain on <branch>* panel have something to show without a
/// test building it first. Read through the adapter exactly as
/// [`a_chain_names_its_steps_and_each_runs_build_on_the_branch`] reads its own,
/// and kept apart from that one's `KnobasChain857`, which that test creates and
/// deletes.
#[tokio::test]
#[ignore = "needs testenv's seeded TeamCity: `just teamcity-live-seeded`"]
async fn the_seeded_chain_names_its_steps_and_each_step_has_its_run_on_the_feature_branch() {
    let seeded = seeded();
    seeded.clear_leftovers().await;
    let chain = seeded
        .seed
        .chain
        .clone()
        .expect("seed-state.json records no `teamcity.chain` -- run `./seed --teamcity`");
    assert!(chain.build_types.len() >= 3, "a chain of at least three");
    assert_eq!(
        chain.builds.len(),
        chain.build_types.len(),
        "one build each"
    );
    assert!(
        chain
            .build_types
            .iter()
            .all(|t| t.starts_with(&format!("{}_", chain.project))),
        "every step is in the chain's own project, which holds no fixture configuration: {chain:?}"
    );
    assert_ne!(
        chain.project, CHAIN,
        "not the suite's own, which it deletes"
    );

    let source = seeded.source(serde_json::json!({}));
    let pipeline = |t: &str| format!("teamcity:buildType:{t}");
    let run = |id: i64| format!("teamcity:build:{id}");
    let names: Vec<(String, String)> = chain
        .build_types
        .iter()
        .map(|t| {
            (
                pipeline(t),
                t.strip_prefix(&format!("{}_", chain.project))
                    .expect("checked above")
                    .to_owned(),
            )
        })
        .collect();
    for t in &chain.build_types {
        let settings = source
            .pipeline_settings(&pipeline(t))
            .await
            .expect("the settings read")
            .expect("TeamCity keeps pipelines");
        let got: Vec<(String, String)> = settings
            .chain
            .iter()
            .map(|s| (s.pipeline.clone(), s.name.clone()))
            .collect();
        assert_eq!(got, names, "{t} names the whole chain, first step first");
    }

    // Each step's build, on one branch that is not the default, and the chain
    // of any one of them names all three runs.
    let mut branch = None;
    for b in &chain.builds {
        let record = seeded.build(b.real_id).await;
        assert_eq!(record["state"], "finished", "{record}");
        assert_eq!(record["status"], "SUCCESS", "{record}");
        let on = record["branchName"].as_str().expect("a branch").to_owned();
        assert_ne!(on, "main", "a feature branch, not the default");
        assert_eq!(*branch.get_or_insert(on.clone()), on, "one branch");
        assert_eq!(seeded.seeded_ids_of(&b.build_type), [b.real_id]);
    }
    let first = chain.builds.first().expect("a build").real_id;
    let steps = source
        .run_chain(&run(first))
        .await
        .expect("the chain read")
        .expect("TeamCity keeps pipelines");
    let runs: Vec<Option<String>> = steps
        .iter()
        .map(|s| s.run.as_ref().map(|r| r.entity.to_string()))
        .collect();
    let want: Vec<Option<String>> = chain.builds.iter().map(|b| Some(run(b.real_id))).collect();
    assert_eq!(runs, want, "every step has its run on {branch:?}");
}

/// The run page's reads on the seeded failed build (#854, #855, #856): the
/// whole log split into its step, the failed test with its id and its
/// history, the artifact folder the seed publishes and the file inside it,
/// a preview, and a download of the folder, a file and the log -- each the
/// bytes the server streamed, under the name the adapter suggests.
#[tokio::test]
#[ignore = "needs testenv's seeded TeamCity: `just teamcity-live-seeded`"]
async fn a_seeded_runs_log_tests_and_artifacts_read_whole() {
    use knobas_source::run::{DownloadSink, DownloadTarget, TestStatus};

    #[derive(Default)]
    struct Kept {
        name: Option<String>,
        bytes: Vec<u8>,
    }
    #[async_trait::async_trait]
    impl DownloadSink for Kept {
        async fn start(&mut self, file_name: &str) -> Result<(), knobas_source::SourceError> {
            self.name = Some(file_name.to_owned());
            Ok(())
        }
        async fn write(&mut self, bytes: &[u8]) -> Result<(), knobas_source::SourceError> {
            self.bytes.extend_from_slice(bytes);
            Ok(())
        }
    }

    let seeded = seeded();
    seeded.clear_leftovers().await;
    let failed = seeded.fixture_build(1187).real_id;
    let run = format!("teamcity:build:{failed}");
    let source = seeded.source(serde_json::json!({}));

    // -- the whole log (#854) --
    let log = source
        .run_log(&run, None, knobas_source::run::LOG_LINES_MAX)
        .await
        .expect("the log reads")
        .expect("TeamCity keeps runs");
    println!(
        "SEEDED RUN LOG {run}: {} lines, steps {:?}",
        log.total, log.steps
    );
    for line in &log.lines {
        println!(
            "{:>4} {:?} {:<8} {:?} {}",
            line.number,
            line.step,
            line.time.as_deref().unwrap_or(""),
            line.tone,
            line.text
        );
    }
    assert_eq!(
        log.lines.len() as u64,
        log.total,
        "the whole seeded log is under the cap"
    );
    assert_eq!(
        log.steps.len(),
        1,
        "the configuration has one step: {:?}",
        log.steps
    );
    let step = &log.steps[0];
    let failing = log
        .lines
        .iter()
        .find(|l| {
            l.text
                .ends_with("test sepa::retry::gives_up_after_max_attempts ... FAILED")
        })
        .expect("the step's own output is in the log");
    assert_eq!(failing.step, Some(0), "its line is inside the step");
    assert!(step.first < failing.number && failing.number <= step.last);
    assert!(
        log.lines
            .iter()
            .any(|l| l.tone == Some(knobas_source::Tone::Err)),
        "the failure is an error line"
    );
    let head = source.run_log(&run, Some(1), 3).await.unwrap().unwrap();
    assert_eq!(
        head.lines.iter().map(|l| l.number).collect::<Vec<_>>(),
        [1, 2, 3]
    );
    assert_eq!(head.total, log.total);

    // -- the tests and one test's history (#855) --
    let tests = source
        .run_tests(&run)
        .await
        .expect("the tests read")
        .expect("runs");
    println!("SEEDED RUN TESTS: {tests:?}");
    let test = &tests.tests[0];
    assert_eq!(test.name, "sepa::retry::gives_up_after_max_attempts");
    assert_eq!(
        (test.status, test.module.as_deref()),
        (TestStatus::Failed, Some("sepa::retry"))
    );
    let id = test
        .id
        .clone()
        .expect("TeamCity names the test across runs");
    let history = source
        .test_history(&run, &id)
        .await
        .expect("the history reads")
        .expect("runs");
    println!("SEEDED TEST HISTORY {id}: {history:?}");
    let this = history
        .iter()
        .find(|o| o.run == run)
        .unwrap_or_else(|| panic!("the run itself is in its test's history: {history:?}"));
    assert_eq!(this.status, TestStatus::Failed);
    assert!(history.len() <= 10);

    // -- the artifacts, a folder of them, a preview and the downloads (#856) --
    let root = source
        .run_artifacts(&run, "")
        .await
        .expect("the root reads")
        .expect("runs");
    println!("SEEDED ARTIFACTS: {root:?}");
    let folder = root
        .iter()
        .find(|e| e.folder)
        .expect("the seed publishes a folder");
    assert_eq!(folder.path, "reports");
    let inside = source
        .run_artifacts(&run, &folder.path)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        inside.iter().map(|e| e.path.as_str()).collect::<Vec<_>>(),
        ["reports/sepa-report.txt"]
    );
    let preview = source
        .artifact_preview(&run, "reports/sepa-report.txt")
        .await
        .expect("the preview reads")
        .expect("runs");
    let text = preview.text.expect("the report is text");
    assert!(
        text.contains("test sepa::retry::gives_up_after_max_attempts ... FAILED"),
        "{text}"
    );

    let mut file = Kept::default();
    assert!(
        source
            .download(
                &run,
                &DownloadTarget::File {
                    path: "sepa-report.txt".into()
                },
                &mut file
            )
            .await
            .expect("the file downloads")
    );
    assert_eq!(
        (file.name.as_deref(), file.bytes.as_slice()),
        (Some("sepa-report.txt"), text.as_bytes())
    );
    let mut zip = Kept::default();
    assert!(
        source
            .download(
                &run,
                &DownloadTarget::Folder {
                    path: "reports".into()
                },
                &mut zip
            )
            .await
            .expect("the folder downloads")
    );
    assert_eq!(zip.name.as_deref(), Some("reports.zip"));
    assert!(
        zip.bytes.starts_with(b"PK"),
        "a zip: {:?}",
        &zip.bytes[..zip.bytes.len().min(16)]
    );
    let mut whole = Kept::default();
    assert!(
        source
            .download(&run, &DownloadTarget::Log, &mut whole)
            .await
            .expect("the log downloads")
    );
    assert_eq!(
        whole.name.as_deref(),
        Some("Payout_IntegrationTests-1187.log")
    );
    assert_eq!(
        String::from_utf8_lossy(&whole.bytes).lines().count() as u64,
        log.total,
        "the download is the log the read counted"
    );
    let mut all = Kept::default();
    assert!(
        source
            .download(&run, &DownloadTarget::All, &mut all)
            .await
            .expect("every artifact downloads")
    );
    assert!(all.bytes.starts_with(b"PK"), "a zip");
}

/// The role data check (#1199, ADR-0028) on the seeded corpus: CI, the role
/// the adapter claims, is filled by the configurations and builds a full sync
/// emits.
#[tokio::test]
#[ignore = "needs testenv's seeded TeamCity: `just teamcity-live-seeded`"]
async fn fills_every_role_it_claims_on_the_seeded_corpus() {
    let seeded = seeded();
    let scope = serde_json::json!({ "build_type_ids": QUIET_CONFIGURATIONS });
    let source = seeded.source_with(&seeded.url, &seeded.token, scope);
    knobas_source::contract::fills_its_roles(&*source).await;
}

/// The walk reading against the real server (#1382): a full run reports 0 to N
/// `build configurations`, N being the configurations it then emits, and an
/// incremental run reports its walk over the builds it listed.
#[tokio::test]
#[ignore = "needs testenv's seeded TeamCity: `just teamcity-live-seeded`"]
async fn a_run_reports_its_walk_over_the_real_configurations_and_listed_builds() {
    let seeded = seeded();
    seeded.clear_leftovers().await;
    let source = seeded.source(serde_json::json!({}));

    let mut first = Walking::default();
    let cursor = source.sync(None, &mut first).await.expect("full sync");
    let n = of_kind(&first.items, "build_config").len() as u64;
    println!("walk: full run {n} configurations {:?}", first.readings);
    assert!(n > 0);
    let expected: Vec<_> = (0..=n)
        .map(|d| (d, Some(n), "build configurations".to_owned()))
        .collect();
    assert_eq!(first.readings, expected);

    let mut next = Walking::default();
    source
        .sync(Some(cursor), &mut next)
        .await
        .expect("incremental sync");
    println!(
        "walk: incremental run {} items {:?}",
        next.items.len(),
        next.readings
    );
    let of = next.readings.last().and_then(|r| r.1).expect("a reading");
    let expected: Vec<_> = (0..=of)
        .map(|d| (d, Some(of), "builds".to_owned()))
        .collect();
    assert_eq!(next.readings, expected);
}

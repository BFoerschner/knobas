//! The adapter against the **seeded, real Jira Data Center** that
//! `testenv/seed-atlassian-content.sh` fills with the Tidewater dataset
//! (issue #276, ADR-0013).
//!
//! Until this file existed the Jira adapter had only ever been run against
//! `knobas-mockd` -- a mock built from the vendored WADL and from assumptions
//! about the product, certifying the adapter against a reading of the contract
//! rather than against Jira. `tests/it/mockd.rs` still runs in `just check` and is
//! still worth having: it is fast, deterministic, and it records a *violation*
//! for a path, verb or query parameter the WADL does not declare, which no real
//! server does. What it cannot do is disagree with the adapter, because the
//! shapes on both sides were written by the same reading. This file is where
//! Jira gets a vote.
//!
//! `#[ignore]`d, so `just check` runs none of it:
//!
//! ```text
//! just atlassian-live      # from the repo root: stands the pair up, seeds,
//!                          # runs this, tears the pair down again
//! ```
//!
//! or, inside a licence window already open (`testenv/README.md`, *Jira and
//! Confluence, end to end*):
//!
//! ```text
//! cd testenv && eval "$(./seed --env)"
//! cd .. && env -u RUSTUP_TOOLCHAIN cargo test -p knobas-source-jira \
//!     --test live_jira_seeded -- --ignored --nocapture --test-threads=1
//! ```
//!
//! # What this file asserts that nothing else can
//!
//! * The seeded corpus **by content**: every fixture issue across `PAY` and
//!   `OPS`, at the fixture's own keys, with the comments and worklogs the seed
//!   posted -- PAY-231's 270-minute worklog as `16200` seconds -- and the
//!   project key and name at the paths ADR-0010's census reads them from.
//! * **Battery clause 2 against a real index**: an incremental run after
//!   nothing changed emits nothing and hands back the byte-identical cursor.
//!   mockd runs that clause against a fake that agrees with the adapter by
//!   construction; here Jira's own search index decides.
//! * **The watermark on an edit this suite makes**: one issue is edited through
//!   Jira's REST API, the next run returns exactly that issue, and the cursor
//!   advances to *that issue's* `updated` and no further -- never to `now()`,
//!   which is the `CONTEXT.md` *Watermark* rule and the one the search index's
//!   own lag would punish.
//! * **What a real Jira does with a credential it does not accept**, which is
//!   not one thing: a wrong *password* is a 401 with
//!   `X-Seraph-LoginReason: AUTHENTICATED_FAILED`, and a wrong *bearer token*
//!   never reaches Seraph at all -- it is a 401 with no such header on most
//!   endpoints and a **200 with `total: 0`** on `/search`, because an
//!   anonymous search is allowed to ask. The engine-side half -- the
//!   credential-health state the sources view renders -- is `knobas-app`'s
//!   `tests/atlassian_live.rs`.
//! * **Where epic membership actually is** on a classic Data Center project:
//!   the Epic Link custom field, whose id is this instance's own, and *not*
//!   `fields.parent`, which is absent from every issue in the corpus. mockd
//!   serves both spellings, which is what hid this.
//! * **That `fields.parent` reaches the payload when there is one** (#643): a
//!   sub-task this suite files resolves to exactly one parent through the
//!   descriptor's *first* candidate, and `under:` reaches it. mockd serves
//!   `fields.parent` from a shape written by the same reading as the adapter;
//!   this is the one run where Jira itself answers it.
//! * **That the adapter can find that id by itself** (#297): `test_connection`
//!   reads `GET /rest/api/2/field` and reports the same id the seed recorded,
//!   and a source configured from *that* answer -- never from
//!   `seed-state.json` -- mirrors PAY-219's membership of PAY-200. mockd
//!   serves no field table at all, so this endpoint has no other witness.
//!
//! Every divergence from mockd that this file found is written down in
//! `knobas-mockd`'s *Documented deviations* list, where the next reader of
//! that mock will meet it.
//!
//! # What this file writes, and what it takes away
//!
//! One label, [`LITTER_LABEL`], added to one seeded issue to make it the
//! *edited* issue of the incremental test, and removed again when the test
//! ends -- passing or panicking alike, from [`Labeled`]'s `Drop`, through
//! `knobas_live_cleanup::Cleanup`, and checked afterwards rather than assumed. A
//! label is the smallest reversible edit Jira has: it moves `updated`, which is
//! the whole point, and it changes no field the fixture describes.
//!
//! One **sub-task**, filed under PAY-219 with [`LITTER_LABEL`] so that a
//! `fields.parent` exists on this server at all -- the corpus has none -- and
//! deleted again from [`SubTask`]'s `Drop`, which checks the issue answers 404
//! and polls `/search` until it stops naming it (#643). A second one, filed and
//! deleted the same way, is moved through the workflow 120 times so that a
//! long status history exists (#1083). Each moves the `PAY` key counter, which
//! nothing may assume stands still.
//!
//! One **version on PAY**, a favourite filter naming it and a board on that
//! filter, so that a query Jira refuses exists (#1893): the version is deleted
//! by the test, and the board and the filter, or all three on a failure, by
//! [`Probes`]' `Drop`, which checks each is gone afterwards. A
//! killed run's probe is taken back by the next run before it starts.
//!
//! Three **boards on PAY**, each on a filter of its own, so that the board
//! listing's order can be read (#2003); one is renamed. [`Probes`]' `Drop`
//! takes them back and checks, and the next run takes back a killed run's
//! boards with the filters they stand on (a filter whose board was never
//! made is not a favourite, and nothing reads it).
//!
//! One **user and one permission scheme** (#2052): the user minted with Jira
//! Software access and no group but the default one, the scheme a copy of
//! OPS's with *Add Comments*, *Work On Issues* and *Create Issues* given to
//! `jira-administrators` alone, and OPS put on it for one test. [`Narrowed`]'s
//! `Drop` puts OPS back on its scheme, deletes the copy and the user, and
//! checks each; the next run puts back a killed run's first.
//!
//! What a *killed* run left behind is put back by the next one, and by this
//! suite for **both** of them: [`Seeded::clear_leftovers`] restores the corpus
//! from `seed-state.json` -- a stray issue deleted, a stray comment deleted, a
//! label removed, a status moved back through the workflow -- which is the
//! union of what either suite writes. Every test that asserts an exact set
//! calls it before taking its baseline. Recovery from a dirty environment is
//! "run the suite again".
//!
//! **One owner at a time**, as for the seeded TeamCity (`testenv/README.md`):
//! the exact-set assertions and clause 2 mean nothing while somebody else is
//! writing to this Jira.
//!
//! # A red run here is never answered by running it again
//!
//! When the fake and the server disagree the **fake** is wrong; when the
//! adapter and the server disagree the **adapter** is. A failure here names a
//! defect at one of those two ends.
//!
//! # What the seed cannot reproduce, and so what is not asserted
//!
//! `created` and `updated` are when the seed ran, so timestamps are asserted
//! against the record rather than against a value. Comments and worklogs are
//! authored by the admin account (`jira.author` in `seed-state.json`) because
//! Jira DC's REST takes no author on either; the fixture's people exist as
//! users so that *assignees* are the fixture's, and `SyncItem::author` -- which
//! this adapter reads from the assignee -- therefore is. The fixture's
//! `spent_week_m` is a sum its own worklogs do not add up to and is not a Jira
//! field at all.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use knobas_live_cleanup::{Cleanup, DEFAULT_BUDGET};
use knobas_source::contract::{Fault, VecSink, Walking, battery};
use knobas_source::instance::SourceInstance;
use knobas_source::{AuthMethod, Source, SourceError, SyncItem};

/// How long one HTTP exchange with the container may take. A localhost Jira
/// answers a `/search` in tens of milliseconds; a request that reaches thirty
/// seconds is not coming back. Longer than the Gitea and TeamCity suites'
/// budgets because Jira is a heavier product on a shared 8 GB VM.
const REQUEST_BUDGET: Duration = Duration::from_secs(30);

/// How long the search index may lag an edit made through the REST API.
///
/// Jira's `/search` reads a Lucene index the write path updates
/// asynchronously, so the run that first sees an edit is not necessarily the
/// one straight after it. Every incremental assertion here therefore polls to
/// this deadline rather than sleeping a guessed amount; measured on this
/// container the index caught up within a second.
const INDEX_BUDGET: Duration = Duration::from_secs(60);

/// The label this suite and `knobas-app`'s Atlassian live suite mark
/// everything they write with, so that a person looking at the server can tell
/// whose litter it is.
///
/// A constant rather than a per-run salt, deliberately: the leftovers that
/// matter are the ones a *killed* run left, and a process that is gone cannot
/// be asked what it salted with. The cost is the one-owner rule, which this
/// environment is under anyway.
///
/// It is a marker and not the mechanism: [`Seeded::clear_leftovers`] restores
/// the corpus from `seed-state.json`, so a leftover that could not carry the
/// label -- the ticket a create files, which `WriteOp::CreateTicket` has no
/// field for -- is reached anyway.
const LITTER_LABEL: &str = "knobas-live-suite";

/// The `jira` block of `testenv/seed-state.json`: what the seed actually got
/// from the server.
#[derive(Debug, Clone, serde::Deserialize)]
struct Seed {
    /// The product version `seed-atlassian.sh` read off `/rest/api/2/serverInfo`.
    version: String,
    /// The admin account every comment and worklog is authored by.
    author: String,
    /// This instance's "Epic Link" custom field id -- where a **classic** Data
    /// Center project keeps epic membership. Per-instance, which is why the
    /// adapter's `epic_link_field` names it rather than guessing, and why the
    /// seed records the id it found rather than a suite hard-coding one.
    epic_link_field: String,
    /// `[{key, name}]` for each project the seed created.
    projects: Vec<SeededProject>,
    /// One row per fixture issue, with the ids Jira assigned.
    issues: Vec<SeededIssue>,
    /// Fixture statuses the template's workflow does not have. Empty on the
    /// *Basic software development* template, whose four statuses are exactly
    /// the fixture's -- and asserted empty below, because a non-empty list
    /// would mean issues are not where this suite expects them.
    unreachable_statuses: Vec<serde_json::Value>,
    /// The seed's Scrum board on PAY and its active sprint (#811), with this
    /// instance's Sprint field id. `None` in a `seed-state.json` written
    /// before the seed made one; the sprint tests then say to re-seed.
    #[serde(default)]
    sprint: Option<SeededSprint>,
    /// The seed's Kanban board over PAY and OPS and its favourite filter
    /// (#877). `None` in a `seed-state.json` written before the seed made them.
    #[serde(default)]
    views: Option<SeededViews>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct SeededViews {
    kanban: SeededKanban,
    /// The seed's favourite filter (#877's seed, read by #878).
    favourite_filter: SeededFilter,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct SeededFilter {
    id: u64,
    name: String,
    jql: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct SeededKanban {
    board_id: u64,
    name: String,
    sub_query: String,
    limit_column: String,
    limit: u32,
    jql: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct SeededSprint {
    /// The Sprint custom field's id, found by the seed by its plugin key.
    field: String,
    id: u64,
    name: String,
    /// The keys the seed put in the sprint.
    issues: Vec<String>,
    /// The Scrum board the sprint is on (#877's Jira views read it).
    board_id: u64,
}

impl Seed {
    fn sprint(&self) -> &SeededSprint {
        self.sprint.as_ref().expect(
            "seed-state.json records no sprint: it was written before the seed made one \
             (#811). Re-run `./seed-atlassian-content.sh jira`.",
        )
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
struct SeededProject {
    key: String,
    /// `None` where no fixture ticket of that project names one.
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct SeededIssue {
    key: String,
    status: String,
    comments: Vec<SeededComment>,
    worklogs: Vec<SeededWorklog>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct SeededComment {
    id: String,
    body: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct SeededWorklog {
    id: String,
    #[serde(default)]
    comment: Option<String>,
    #[serde(rename = "timeSpentSeconds")]
    time_spent_seconds: i64,
}

/// Where the seeded server is, how to talk to it, and what the seed put in it.
struct Seeded {
    url: String,
    user: String,
    password: String,
    http: reqwest::Client,
    seed: Seed,
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(REQUEST_BUDGET)
        .build()
        .expect("a reqwest client with a timeout")
}

/// Panics with the command to run rather than skipping: this suite is only ever
/// run by name, and a silent skip would read as a green certification of
/// nothing.
fn seeded() -> Seeded {
    let need = |key: &str| {
        std::env::var(key)
            .ok()
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| {
                panic!(
                    "{key} is not set -- this suite needs testenv's seeded Jira. From the repo \
                     root: `just atlassian-live`, which stands the pair up, seeds it, runs every \
                     Atlassian-gated suite and tears it down again; or, inside a licence window \
                     already open, `eval \"$(cd testenv && ./seed --env)\"`"
                )
            })
    };
    let url = need("KNOBAS_JIRA_URL").trim_end_matches('/').to_owned();
    let user = need("KNOBAS_JIRA_USER");
    let password = need("KNOBAS_JIRA_PASSWORD");
    let state = std::env::var("KNOBAS_JIRA_SEED_STATE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testenv/seed-state.json")
        });
    let raw = std::fs::read_to_string(&state).unwrap_or_else(|e| {
        panic!(
            "{}: {e} -- `./seed-atlassian-content.sh` writes the key-to-id map this suite reads \
             (or point KNOBAS_JIRA_SEED_STATE at it)",
            state.display()
        )
    });
    let whole: serde_json::Value = serde_json::from_str(&raw).expect("seed-state.json is JSON");
    let seed: Seed = serde_json::from_value(whole["jira"].clone()).unwrap_or_else(|e| {
        panic!(
            "{}: no `jira` block with `version`, `projects` and `issues` -- run \
             `./seed-atlassian-content.sh`: {e}",
            state.display()
        )
    });
    assert!(
        !seed.issues.is_empty(),
        "{}: the seed recorded no issues; `./seed-atlassian-content.sh` did not finish",
        state.display()
    );
    assert!(
        seed.unreachable_statuses.is_empty(),
        "the seed could not put every issue in its fixture status: {:?}. Every assertion below \
         is written against the fixture's statuses, so this suite would fail in ways that do not \
         name the cause. The template's workflow changed -- re-read \
         `testenv/seed-atlassian-content.sh`'s header.",
        seed.unreachable_statuses
    );
    Seeded {
        url,
        user,
        password,
        http: client(),
        seed,
    }
}

impl Seeded {
    /// One raw `GET`, with the credential `./seed --env` prints.
    async fn get(&self, path_and_query: &str) -> (u16, serde_json::Value) {
        self.request(reqwest::Method::GET, path_and_query, None)
            .await
    }

    async fn request(
        &self,
        method: reqwest::Method,
        path_and_query: &str,
        body: Option<serde_json::Value>,
    ) -> (u16, serde_json::Value) {
        let mut request = self
            .http
            .request(method.clone(), format!("{}/{path_and_query}", self.url))
            .header("Accept", "application/json")
            .basic_auth(&self.user, Some(&self.password));
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request
            .send()
            .await
            .unwrap_or_else(|e| panic!("{method} {path_and_query}: {e}"));
        let status = response.status().as_u16();
        let text = response.text().await.unwrap_or_default();
        let json = serde_json::from_str(&text).unwrap_or(serde_json::Value::String(text));
        (status, json)
    }

    /// One JQL query, with the fields it needs, as raw rows.
    async fn jql(&self, jql: &str, fields: &str) -> Vec<serde_json::Value> {
        let (status, body) = self
            .get(&format!(
                "rest/api/2/search?jql={}&maxResults=100&fields={fields}",
                url_encode(jql)
            ))
            .await;
        assert_eq!(status, 200, "JQL {jql:?} answered {body}");
        body["issues"].as_array().cloned().unwrap_or_default()
    }

    /// The adapter over this instance, authenticating as `./seed --env` says:
    /// user and password, which is what the setup wizard's admin account has
    /// (it holds no personal access token).
    fn source(&self, config: serde_json::Value) -> Box<dyn Source> {
        self.source_at(&self.url, config)
    }

    /// The same, against another base URL -- the [`Fault::Unreachable`] case.
    fn source_at(&self, base_url: &str, config: serde_json::Value) -> Box<dyn Source> {
        self.build_source(base_url, AuthMethod::UserPassword, &self.password, config)
    }

    /// A source whose credential this Jira will not accept, as a **bearer
    /// token** and never as a wrong password.
    ///
    /// The scheme is not incidental and this is the one place in the repo that
    /// knows why. A real Jira counts failed *password* logins per account and,
    /// on this container's default of three, starts answering `403 Basic
    /// Authentication Failure - Reason : AUTHENTICATION_DENIED` -- to the
    /// **correct** password as well, until an administrator clears the elevated
    /// security check. A suite that drew its 401s from a wrong password would
    /// therefore lock the seed's admin account out part-way through its own
    /// run and fail every test after it on a cause none of them names; measured
    /// exactly that way on 2026-09-03 before this helper existed.
    ///
    /// A bearer token Jira cannot resolve is not a login attempt at all: it
    /// never reaches Seraph, it is a clean 401 on `/serverInfo` and `/myself`,
    /// and it counts against nothing.
    /// [`a_rejected_credential_is_a_real_401_and_only_basic_auth_carries_the_seraph_header`]
    /// is the one test that sends a wrong password, one request at a time and
    /// under a username that does not exist.
    fn refused_source(&self, config: serde_json::Value) -> Box<dyn Source> {
        self.build_source(&self.url, AuthMethod::Pat, &bad_token(), config)
    }

    fn build_source(
        &self,
        base_url: &str,
        auth: AuthMethod,
        secret: &str,
        mut config: serde_json::Value,
    ) -> Box<dyn Source> {
        config["username"] = serde_json::Value::String(self.user.clone());
        // The issue walk's tests count the items a run delivers, and the
        // seed's favourite filter would be one more at every first run and at
        // every run after an edit it holds (#878). A test of the Jira views
        // turns favourite filters on itself; the default is pinned by the
        // adapter's unit tests.
        if config.get("favourite_filters").is_none() {
            config["favourite_filters"] = serde_json::Value::Bool(false);
        }
        match knobas_source_jira::build(SourceInstance {
            id: "jira".to_owned(),
            kind: knobas_source_jira::ADAPTER_KIND.to_owned(),
            display_name: "Tidewater Jira (seeded)".to_owned(),
            base_url: base_url.to_owned(),
            auth: Some(auth),
            secret: Some(secret.to_owned()),
            account: None,
            config,
        }) {
            Ok(s) => s,
            // `Box<dyn Source>` is not `Debug`, so `expect` is unavailable.
            Err(e) => panic!("the adapter must build against the seeded URL: {e:?}"),
        }
    }

    /// The fixture's keys, ascending, as the seed recorded them.
    fn seeded_keys(&self) -> Vec<String> {
        let mut keys: Vec<String> = self.seed.issues.iter().map(|i| i.key.clone()).collect();
        keys.sort();
        keys
    }

    fn seeded_issue(&self, key: &str) -> &SeededIssue {
        self.seed
            .issues
            .iter()
            .find(|i| i.key == key)
            .unwrap_or_else(|| panic!("seed-state.json records no issue {key}"))
    }

    /// Put the corpus back to what the seed left, and say what had to be put
    /// back -- **the recovery path for both Atlassian live suites**, since only
    /// a process that unwinds reaches a `Drop` and a run that was *killed*
    /// reaches none.
    ///
    /// Scoped by `seed-state.json` and by nothing a run remembers, which is the
    /// only way to reach the leftovers of a run that is gone -- the same rule
    /// the seeded TeamCity suite's clearing works under. Four things can be
    /// wrong, and it is the union of what either suite writes:
    ///
    /// * an issue the seed did not create -- the ticket `knobas-app`'s suite
    ///   files through the write queue -- is **deleted**;
    /// * [`LITTER_LABEL`] on a seeded issue -- this suite's own edit -- is
    ///   removed;
    /// * a comment whose id the seed did not record is **deleted**, so
    ///   [`each_issue_carries_its_comments_and_worklogs_as_seeded`]'s exact id
    ///   list means something;
    /// * an issue in a status other than the one the seed put it in is moved
    ///   back through the workflow, so
    ///   [`a_full_sync_mirrors_every_seeded_issue_across_both_projects`]'s
    ///   per-issue status assertion does too.
    ///
    /// The last two are what a killed `knobas-app` run leaves, and without them
    /// the next run would fail two tests here on diffs that never name the
    /// cause -- which is the failure the TeamCity suite built its own refusal
    /// check around. Every test that asserts an exact set calls this before
    /// taking its baseline. Read-only in the ordinary case: a clean server
    /// costs one search.
    ///
    /// Not "sweep": `CONTEXT.md` spends that word on the engine pass that
    /// tombstones what a full sync no longer emitted.
    async fn clear_leftovers(&self) {
        let rows = self.jql("ORDER BY key ASC", "status,labels,comment").await;
        let mut cleared: Vec<String> = Vec::new();
        for row in &rows {
            let key = row["key"].as_str().expect("an issue has a key").to_owned();
            let Some(seeded) = self.seed.issues.iter().find(|i| i.key == key) else {
                let (status, body) = self
                    .request(
                        reqwest::Method::DELETE,
                        &format!("rest/api/2/issue/{key}"),
                        None,
                    )
                    .await;
                // 404: the search index still named an issue a `Drop` had
                // already deleted. Jira updates that index asynchronously, so a
                // key it hands back is not a promise the issue is still there.
                assert!(
                    status == 204 || status == 404,
                    "deleting the leftover issue {key}: {status} {body}"
                );
                cleared.push(format!("deleted {key}"));
                continue;
            };

            if row["fields"]["labels"]
                .as_array()
                .is_some_and(|l| l.iter().any(|v| v == LITTER_LABEL))
            {
                let (status, body) = self
                    .request(
                        reqwest::Method::PUT,
                        &format!("rest/api/2/issue/{key}"),
                        Some(serde_json::json!({
                            "update": { "labels": [{ "remove": LITTER_LABEL }] }
                        })),
                    )
                    .await;
                assert_eq!(status, 204, "unlabelling {key}: {body}");
                cleared.push(format!("unlabelled {key}"));
            }

            for id in row["fields"]["comment"]["comments"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|c| c["id"].as_str())
                .filter(|id| !seeded.comments.iter().any(|c| c.id == *id))
            {
                let (status, body) = self
                    .request(
                        reqwest::Method::DELETE,
                        &format!("rest/api/2/issue/{key}/comment/{id}"),
                        None,
                    )
                    .await;
                assert!(
                    status == 204 || status == 404,
                    "deleting the leftover comment {id} on {key}: {status} {body}"
                );
                cleared.push(format!("deleted comment {id} on {key}"));
            }

            let status_now = row["fields"]["status"]["name"].as_str().unwrap_or_default();
            if status_now != seeded.status {
                self.move_to(&key, &seeded.status).await;
                cleared.push(format!(
                    "moved {key} back from {status_now:?} to {:?}",
                    seeded.status
                ));
            }
        }
        if cleared.is_empty() {
            return;
        }
        println!(
            "live suite: put back {} thing(s) a run that was killed rather than failed left \
             behind: {}",
            cleared.len(),
            cleared.join("; ")
        );
    }

    /// Move one issue through the workflow to `status`, the only way a status
    /// changes. The seeded workflow reaches all four of its statuses from every
    /// one of them, so the move always exists; a workflow that stopped offering
    /// it is a changed template and says so.
    async fn move_to(&self, key: &str, status: &str) {
        let (code, body) = self
            .get(&format!("rest/api/2/issue/{key}/transitions"))
            .await;
        assert_eq!(code, 200, "transitions of {key}: {body}");
        let id = body["transitions"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|t| t["to"]["name"].as_str() == Some(status))
            .and_then(|t| t["id"].as_str().map(str::to_owned))
            .unwrap_or_else(|| panic!("the workflow offers {key} no way to {status:?}: {body}"));
        let (code, body) = self
            .request(
                reqwest::Method::POST,
                &format!("rest/api/2/issue/{key}/transitions"),
                Some(serde_json::json!({ "transition": { "id": id } })),
            )
            .await;
        assert_eq!(code, 204, "moving {key} to {status:?}: {body}");
    }
}

/// Percent-encode what a JQL clause may contain. Deliberately tiny: the only
/// queries here are composed from constants and from keys Jira itself
/// answered.
fn url_encode(raw: &str) -> String {
    raw.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

/// The one label this suite adds, removed again when the guard drops --
/// passing or panicking alike -- and the removal checked.
///
/// The smallest reversible edit Jira has. It moves the issue's `updated`, which
/// is what the incremental run is measured on, and touches no field the fixture
/// describes.
struct Labeled {
    url: String,
    user: String,
    password: String,
    key: Option<String>,
}

impl Labeled {
    /// Clears an earlier run's leftovers first, so the corpus a test measures
    /// its baseline over is already clean.
    async fn new(seeded: &Seeded) -> Labeled {
        seeded.clear_leftovers().await;
        Labeled {
            url: seeded.url.clone(),
            user: seeded.user.clone(),
            password: seeded.password.clone(),
            key: None,
        }
    }

    /// Add or remove [`LITTER_LABEL`] on `key` -- **repeatable**, unlike
    /// [`Self::label`], so a test that has to try a timing window more than
    /// once can reset between attempts. `Drop` removes the label either way,
    /// and removing an absent one is a no-op that `unlabel` still verifies.
    async fn set(&mut self, seeded: &Seeded, key: &str, on: bool) {
        self.key = Some(key.to_owned());
        let change = if on { "add" } else { "remove" };
        let (status, body) = seeded
            .request(
                reqwest::Method::PUT,
                &format!("rest/api/2/issue/{key}"),
                Some(serde_json::json!({
                    "update": { "labels": [{ change: LITTER_LABEL }] }
                })),
            )
            .await;
        assert_eq!(status, 204, "{change} {LITTER_LABEL} on {key}: {body}");
    }

    /// Add [`LITTER_LABEL`] to `key`, and own its removal from this line on.
    async fn label(&mut self, seeded: &Seeded, key: &str) {
        assert!(self.key.is_none(), "this guard owns exactly one edit");
        self.key = Some(key.to_owned());
        let (status, body) = seeded
            .request(
                reqwest::Method::PUT,
                &format!("rest/api/2/issue/{key}"),
                Some(serde_json::json!({
                    "update": { "labels": [{ "add": LITTER_LABEL }] }
                })),
            )
            .await;
        assert_eq!(status, 204, "labelling {key}: {body}");
    }
}

impl Drop for Labeled {
    fn drop(&mut self) {
        let Some(key) = self.key.take() else {
            return;
        };
        let (url, user, password) = (self.url.clone(), self.user.clone(), self.password.clone());
        Cleanup::new(
            "the live suite did not undo the edit it made, so the server is no longer in the \
             plain-seed state",
            format!(
                "{key} may still carry {LITTER_LABEL}; the next run's leftover clearing removes it"
            ),
        )
        .run(move || async move { unlabel(&client(), &url, &user, &password, &key).await });
    }
}

/// Remove [`LITTER_LABEL`] from one issue and **check** it is gone -- the check
/// is what keeps a container that stopped answering from reading as a cleanup
/// that worked.
async fn unlabel(
    http: &reqwest::Client,
    url: &str,
    user: &str,
    password: &str,
    key: &str,
) -> Result<(), String> {
    let response = http
        .put(format!("{url}/rest/api/2/issue/{key}"))
        .header("Accept", "application/json")
        .basic_auth(user, Some(password))
        .json(&serde_json::json!({
            "update": { "labels": [{ "remove": LITTER_LABEL }] }
        }))
        .send()
        .await
        .map_err(|e| format!("PUT issue {key}: {e}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "PUT issue {key} -> {}: {}",
            response.status(),
            response.text().await.unwrap_or_default()
        ));
    }
    let response = http
        .get(format!("{url}/rest/api/2/issue/{key}?fields=labels"))
        .header("Accept", "application/json")
        .basic_auth(user, Some(password))
        .send()
        .await
        .map_err(|e| format!("GET issue {key}: {e}"))?;
    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("GET issue {key}: unreadable body: {e}"))?;
    let labels = body["fields"]["labels"]
        .as_array()
        .ok_or_else(|| format!("GET issue {key}: no labels on {body}"))?;
    if labels.iter().any(|l| l == LITTER_LABEL) {
        return Err(format!("{key} still carries {LITTER_LABEL}: {body}"));
    }
    Ok(())
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

fn keys(items: &[SyncItem]) -> Vec<String> {
    let mut keys: Vec<String> = items.iter().map(|i| i.entity.key.clone()).collect();
    keys.sort();
    keys
}

fn item<'a>(items: &'a [SyncItem], key: &str) -> &'a SyncItem {
    items
        .iter()
        .find(|i| i.entity.key == key)
        .unwrap_or_else(|| panic!("{key} is not in the run: {:?}", keys(items)))
}

/// The watermark inside the adapter's cursor.
fn updated_to(cursor: &str) -> chrono::DateTime<chrono::Utc> {
    let raw = serde_json::from_str::<serde_json::Value>(cursor)
        .ok()
        .and_then(|v| v["updated_to"].as_str().map(str::to_owned))
        .unwrap_or_else(|| panic!("not a cursor this adapter wrote: {cursor}"));
    raw.parse().unwrap_or_else(|e| panic!("{raw:?}: {e}"))
}

/// Wait until the wall clock has moved into a **later second** than `stamp`.
///
/// **What this used to be for, and no longer is.** Jira stamps `updated` to the
/// second, and the adapter's cursor used to recognise a re-delivered issue by
/// its `(key, updated)` pair -- so an edit inside the same second as the value a
/// baseline run recorded was, to the cursor, the same version of that issue and
/// was skipped. This helper existed to keep that off the incremental test,
/// which it failed about one run in eight (measured 2026-09-03), and the
/// docstring here called it a documented limitation rather than a defect.
///
/// It was a defect, and issue #345 is where it was measured properly and fixed:
/// the skipped edit was not "missed until the next one", it was **lost for
/// ever**, because `updated` does not move again on its own. `seen` now
/// recognises a *record*, by fingerprint, so an edit inside one second is a
/// different record and is delivered --
/// `two_changes_inside_one_second_with_a_run_between_them_both_reach_the_sink`
/// is the test that says so against this server.
///
/// **It is still needed, for a different assertion.** A test that asserts an
/// edit moved `updated` *forward* (`witnessed > baseline`) is comparing values
/// `/search` renders to the second, and two edits inside one second render
/// equal. That is a fact about the timestamp, not about the cursor, and waiting
/// for the second to turn is what makes such an assertion meaningful. It costs
/// at most a second.
async fn after_the_second_of(stamp: chrono::DateTime<chrono::Utc>) {
    while chrono::Utc::now().timestamp() <= stamp.timestamp() {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// A port nothing listens on: bound to learn the number, then dropped.
fn dead_url() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    format!("http://127.0.0.1:{port}")
}

/// A bearer token that never existed, salted with the process id so a run
/// cannot accidentally collide with a real one.
fn bad_token() -> String {
    format!("revoked-{}", std::process::id())
}

/// A username this Jira does not have, for the one test that sends a wrong
/// password: a failed login under it counts against an account that does not
/// exist, so the seed's admin cannot be locked out by it
/// ([`Seeded::refused_source`] has the whole reasoning).
fn nobody() -> String {
    format!("knobas-live-nobody-{}", std::process::id())
}

/// The `ticket` kind's parent candidates, as the descriptor declares them --
/// `fields.parent`, then `epic`.
fn ticket_parent_declaration() -> Vec<knobas_source::ParentPath> {
    knobas_source_jira::descriptor_template()
        .payload_paths
        .into_iter()
        .find(|paths| paths.kind == "ticket")
        .expect("a ticket declares its parent")
        .parent
}

/// The keys `under:value` returns over one run, ascending: every item one of
/// whose parents is named `value`, and every item that answers to `value` as a
/// parent itself.
///
/// Computed with `knobas_core::payload`'s resolvers over the descriptor's own
/// declaration -- the Rust half of the rule the SQL `under:` predicate is the
/// other half of, held together by `knobas-core`'s agreement test -- and
/// matched ignoring case, as `under:` matches.
fn under(items: &[SyncItem], value: &str) -> Vec<String> {
    use knobas_core::payload::{resolve_names_as_parent, resolve_parents};

    let declarations = knobas_source_jira::descriptor_template().payload_paths;
    let declared = ticket_parent_declaration();
    let mut found: Vec<String> = items
        .iter()
        .filter(|it| {
            resolve_parents(&it.payload, &declared)
                .into_iter()
                .flat_map(|p| p.names)
                .chain(resolve_names_as_parent(
                    &it.payload,
                    &it.kind,
                    &declarations,
                ))
                .any(|n| n.eq_ignore_ascii_case(value))
        })
        .map(|it| it.entity.key.clone())
        .collect();
    found.sort();
    found
}

// ---------------------------------------------------------------------------

/// The cleanup's "really gone" check is a check only if a server that does not
/// answer reads as a failure rather than as an edit undone.
///
/// Witnessed on a port nothing listens on: [`unlabel`] reports the issue it
/// could not reach instead of returning clean. Needs no container -- but it
/// lives here, and is `#[ignore]`d with the rest, because [`unlabel`] does and
/// because the claim it pins is [`Labeled`]'s. The seeded TeamCity suite
/// carries the same test for the same reason.
#[tokio::test]
#[ignore = "needs testenv's seeded Jira: `just atlassian-live`"]
async fn the_cleanup_reports_a_server_it_cannot_reach_rather_than_calling_the_edit_undone() {
    let failure = unlabel(&client(), &dead_url(), "knobas", "irrelevant", "PAY-231")
        .await
        .expect_err("a port nothing listens on is not an edit undone");
    assert!(failure.starts_with("PUT issue PAY-231: "), "{failure}");
}

/// *Test connection* against the server the seed set up: the version the seed
/// recorded, and the account the credential belongs to.
#[tokio::test]
#[ignore = "needs testenv's seeded Jira: `just atlassian-live`"]
async fn test_connection_names_the_seeded_server_and_the_seed_account() {
    let seeded = seeded();
    let info = seeded
        .source(serde_json::json!({}))
        .test_connection()
        .await
        .expect("the seeded server answers /rest/api/2/serverInfo");
    assert_eq!(
        info.server_version.as_deref(),
        Some(seeded.seed.version.as_str()),
        "the version `seed-atlassian.sh` recorded is the one the adapter reports"
    );
    assert_eq!(
        info.account.as_deref(),
        Some(seeded.user.as_str()),
        "`/rest/api/2/myself` names the account the credential belongs to: {info:?}"
    );
    assert_eq!(
        info.secret_expires_at, None,
        "PAT expiry needs /rest/pat/latest/tokens, which is outside this adapter's endpoint set"
    );
    println!(
        "SEEDED server: version={:?} detail={:?} account={:?}",
        info.server_version, info.detail, info.account
    );
}

/// **Every seeded issue, across both Tidewater projects, by key.**
///
/// The exact set and nothing else: the `ticket` kind claims
/// `full_sync_exhaustive: true`, which is the engine's licence to tombstone
/// every row a `cursor: None` run did not return -- so a full sync that
/// quietly returned six of seven issues would *delete* the seventh from the
/// mirror rather than merely sync fewer.
#[tokio::test]
#[ignore = "needs testenv's seeded Jira: `just atlassian-live`"]
async fn a_full_sync_mirrors_every_seeded_issue_across_both_projects() {
    let seeded = seeded();
    seeded.clear_leftovers().await;
    let (items, cursor) = full(&*seeded.source(serde_json::json!({}))).await;

    assert_eq!(
        keys(&items),
        seeded.seeded_keys(),
        "a full sync mirrors exactly the issues the seed created"
    );
    let projects: std::collections::BTreeSet<&str> = items
        .iter()
        .filter_map(|i| i.payload["fields"]["project"]["key"].as_str())
        .collect();
    assert_eq!(
        projects,
        seeded
            .seed
            .projects
            .iter()
            .map(|p| p.key.as_str())
            .collect(),
        "both Tidewater projects are represented"
    );

    for it in &items {
        assert_eq!(it.entity.namespace, "jira");
        assert_eq!(it.kind, knobas_source_jira::KIND_TICKET);
        assert!(!it.deleted, "Jira's search cannot report deletions");
        assert!(
            it.web_url
                .as_deref()
                .is_some_and(|u| u == format!("{}/browse/{}", seeded.url, it.entity.key)),
            "P5: `/browse/<key>` on the configured base URL: {:?}",
            it.web_url
        );
        // The record's own stamp, never `now()`: asserted against the payload
        // rather than against a value, because the seed ran when it ran.
        let stamped = it.payload["fields"]["updated"]
            .as_str()
            .unwrap_or_else(|| panic!("{}: no updated on {}", it.entity.key, it.payload));
        assert_eq!(
            it.updated_at.map(|t| t.to_rfc3339()),
            chrono::DateTime::parse_from_str(stamped, "%Y-%m-%dT%H:%M:%S%.3f%z")
                .map(|t| t.with_timezone(&chrono::Utc).to_rfc3339())
                .ok(),
            "{}: {stamped}",
            it.entity.key
        );
        assert!(
            it.body_text.starts_with(&it.title),
            "body_text is what FTS matches on and starts with the summary: {:?}",
            it.body_text
        );
    }

    // The status the seed put each issue in, read back through the payload:
    // what makes this the *fixture's* corpus and not merely seven issues.
    for seeded_issue in &seeded.seed.issues {
        let it = item(&items, &seeded_issue.key);
        assert_eq!(
            it.payload["fields"]["status"]["name"], seeded_issue.status,
            "{}",
            seeded_issue.key
        );
    }

    // **The watermark, against the issues this run actually emitted.** This
    // was `updated_to(&cursor) <= Utc::now()` until #347, which is the one
    // shape that cannot witness the sentence it carried: a clock reading
    // satisfies `<= now()` by construction, so an adapter that stamped the
    // cursor with `now()` -- the exact fault named here, and the one
    // `CONTEXT.md`'s **Watermark** rule exists for -- passed it. The position a
    // full sync stores is the newest `updated` among the issues it delivered,
    // and that value is in hand: every item's `updated_at` was pinned to its
    // own payload stamp in the loop above.
    let newest = items
        .iter()
        .filter_map(|it| it.updated_at)
        .max()
        .expect("every seeded issue carries an `updated`, asserted item by item above");
    assert_eq!(
        updated_to(&cursor),
        newest,
        "a full sync's watermark is an issue's own `updated` -- the newest one the run emitted \
         -- and never a clock reading: {cursor}"
    );
    println!("SEEDED cursor after a full sync: {cursor}");
}

/// **The comments and worklogs the seed posted, by content** -- and PAY-231's
/// 270 fixture minutes as the `16200` seconds Jira stores.
///
/// `fields=comment,worklog` is what makes both containers arrive with the
/// search page instead of costing a request per issue; the completion path in
/// `sync::complete` exists for a server that truncates them. Asserted through
/// the *adapter*, so what is certified is what reaches the mirror.
#[tokio::test]
#[ignore = "needs testenv's seeded Jira: `just atlassian-live`"]
async fn each_issue_carries_its_comments_and_worklogs_as_seeded() {
    let seeded = seeded();
    seeded.clear_leftovers().await;
    let (items, _) = full(&*seeded.source(serde_json::json!({}))).await;

    for seeded_issue in &seeded.seed.issues {
        let it = item(&items, &seeded_issue.key);
        let comments = it.payload["fields"]["comment"]["comments"]
            .as_array()
            .unwrap_or_else(|| panic!("{}: no comment container", seeded_issue.key));
        assert_eq!(
            comments
                .iter()
                .filter_map(|c| c["id"].as_str())
                .collect::<Vec<_>>(),
            seeded_issue
                .comments
                .iter()
                .map(|c| c.id.as_str())
                .collect::<Vec<_>>(),
            "{}: the comments the seed recorded, by the ids Jira assigned",
            seeded_issue.key
        );
        for (got, want) in comments.iter().zip(&seeded_issue.comments) {
            assert_eq!(got["body"], want.body, "{}", seeded_issue.key);
            assert_eq!(
                got["author"]["name"], seeded.seed.author,
                "{}: Jira DC's REST takes no author on a comment, so the seed's admin account \
                 wrote it (testenv/README.md)",
                seeded_issue.key
            );
            // The discussion reaches `body_text`, which is what FTS indexes --
            // the whole reason `comment` is in the requested field list.
            assert!(
                it.body_text.contains(&want.body),
                "{}: {:?}",
                seeded_issue.key,
                it.body_text
            );
        }

        let worklogs = it.payload["fields"]["worklog"]["worklogs"]
            .as_array()
            .unwrap_or_else(|| panic!("{}: no worklog container", seeded_issue.key));
        assert_eq!(
            worklogs
                .iter()
                .map(|w| (
                    w["id"].as_str().unwrap_or_default(),
                    w["timeSpentSeconds"].as_i64().unwrap_or_default(),
                    w["comment"].as_str()
                ))
                .collect::<Vec<_>>(),
            seeded_issue
                .worklogs
                .iter()
                .map(|w| (w.id.as_str(), w.time_spent_seconds, w.comment.as_deref()))
                .collect::<Vec<_>>(),
            "{}: the worklogs the seed recorded, by id, seconds and the fixture's own note",
            seeded_issue.key
        );
    }

    // The one the acceptance criterion names, spelled out: the fixture's 270
    // minutes on PAY-231, which M3.1 will write more of.
    let pay231 = seeded.seeded_issue("PAY-231");
    assert_eq!(
        pay231
            .worklogs
            .iter()
            .map(|w| w.time_spent_seconds)
            .collect::<Vec<_>>(),
        vec![16_200],
        "the fixture's 270-minute worklog, in the seconds Jira stores it as"
    );
    let it = item(&items, "PAY-231");
    assert_eq!(
        it.payload["fields"]["worklog"]["worklogs"][0]["timeSpentSeconds"],
        16_200
    );
    assert_eq!(
        it.payload["fields"]["timespent"], 16_200,
        "Jira's own roll-up of the same worklog"
    );
    println!(
        "SEEDED PAY-231: {} comment(s), worklog {:?} -- comment author {:?}",
        pay231.comments.len(),
        it.payload["fields"]["worklog"]["worklogs"][0],
        seeded.seed.author
    );
}

/// **Each issue carries its status history** (#929): `expand=changelog` on
/// `/search` puts Jira's `changelog.histories` in the record, whole, so the
/// conversation can draw a move made in Jira's own UI. The seed moved its
/// issues to the fixture's statuses through the workflow, as its admin
/// account; each move is a `status` item there, and an issue's newest one
/// lands on the status it stands in.
#[tokio::test]
#[ignore = "needs testenv's seeded Jira: `just atlassian-live`"]
async fn each_issue_carries_its_status_history() {
    let seeded = seeded();
    seeded.clear_leftovers().await;
    let (items, _) = full(&*seeded.source(serde_json::json!({}))).await;

    let mut moved = 0;
    for seeded_issue in &seeded.seed.issues {
        let it = item(&items, &seeded_issue.key);
        let changelog = &it.payload["changelog"];
        let histories = changelog["histories"].as_array().unwrap_or_else(|| {
            panic!(
                "{}: no changelog.histories in the record: {changelog}",
                seeded_issue.key
            )
        });
        assert_eq!(
            changelog["total"].as_u64(),
            Some(histories.len() as u64),
            "{}: /search returned only part of the changelog",
            seeded_issue.key
        );
        let moves: Vec<(&serde_json::Value, &serde_json::Value)> = histories
            .iter()
            .flat_map(|h| {
                h["items"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|i| i["field"] == "status")
                    .map(move |i| (h, i))
            })
            .collect();
        let Some((history, last)) = moves.last() else {
            continue;
        };
        moved += 1;
        assert_eq!(
            last["toString"], seeded_issue.status,
            "{}: the newest status move lands where the issue stands",
            seeded_issue.key
        );
        assert!(
            last["fromString"].is_string(),
            "{}: {last}",
            seeded_issue.key
        );
        assert!(
            history["created"].is_string() && history["author"]["name"].is_string(),
            "{}: who and when: {history}",
            seeded_issue.key
        );
    }
    assert!(
        moved > 0,
        "the seed moves issues off the workflow's first status, so some issue has a status \
         history"
    );
    println!("SEEDED status histories: {moved} issue(s) with a status move");
}

/// **ADR-0010's project read, against the real payload.**
///
/// `knobas_core::project_key_read!` reads a Jira ticket's project from
/// `payload.fields.project.key` and its name from `.name`. Those two paths are
/// SQL in another crate: nothing in the adapter's own suite would notice Jira
/// moving them, and a census that read nothing would show *no project rooms*
/// rather than fail. Asserted here, where the payload is the server's.
#[tokio::test]
#[ignore = "needs testenv's seeded Jira: `just atlassian-live`"]
async fn the_project_key_and_name_are_where_adr_0010_reads_them() {
    let seeded = seeded();
    seeded.clear_leftovers().await;
    let (items, _) = full(&*seeded.source(serde_json::json!({}))).await;

    let mut seen: BTreeMap<String, String> = BTreeMap::new();
    for it in &items {
        let key = it.payload["fields"]["project"]["key"]
            .as_str()
            .unwrap_or_else(|| {
                panic!(
                    "{}: ADR-0010 reads the project key at fields.project.key, and it is not a \
                     string there: {}",
                    it.entity.key, it.payload["fields"]["project"]
                )
            });
        let name = it.payload["fields"]["project"]["name"]
            .as_str()
            .unwrap_or_else(|| {
                panic!(
                    "{}: ADR-0010 reads the project name at fields.project.name: {}",
                    it.entity.key, it.payload["fields"]["project"]
                )
            });
        assert!(
            it.entity.key.starts_with(&format!("{key}-")),
            "{}: the project key is the issue key's prefix",
            it.entity.key
        );
        seen.insert(key.to_owned(), name.to_owned());
    }

    for project in &seeded.seed.projects {
        let name = seen
            .get(&project.key)
            .unwrap_or_else(|| panic!("no mirrored issue names project {}: {seen:?}", project.key));
        if let Some(seeded_name) = &project.name {
            assert_eq!(
                name, seeded_name,
                "{}: the name the seed created the project with",
                project.key
            );
        }
    }
    println!("SEEDED projects, as ADR-0010's census reads them: {seen:?}");
}

/// **Battery clause 2, then the watermark on an edit this suite makes.**
///
/// Full run; idle run (clause 2 against a real search index, which no other
/// suite can measure -- mockd's fake agrees with the adapter by construction);
/// one label added to one issue through Jira's own REST API; then the next run
/// returns *exactly* that issue, the cursor moves to that issue's own
/// `updated`, and the run after that is idle again.
///
/// The last part is `CONTEXT.md`'s **Watermark** rule made a test: the position
/// advances only as far as the run witnessed. Advancing to `now()` instead
/// would look identical on a quiet server and would skip, permanently, anything
/// edited in the seconds Jira's search index lags its own writes -- which is
/// precisely the window this test's own polling exists because of.
///
/// The idle poll at the end is not ceremony either: it is the only shape that
/// shows a cursor whose `seen` set forgot what the run *skipped*. Full sync
/// then idle is stable under that bug, because a full sync skips nothing.
#[tokio::test]
#[ignore = "needs testenv's seeded Jira: `just atlassian-live`"]
async fn an_incremental_run_after_one_edit_returns_that_issue_and_moves_the_watermark_to_it() {
    let seeded = seeded();
    let mut guard = Labeled::new(&seeded).await;
    let source = seeded.source(serde_json::json!({}));
    const EDITED: &str = "OPS-77";

    let (items, cursor) = full(&*source).await;
    assert_eq!(keys(&items), seeded.seeded_keys());
    let baseline = item(&items, EDITED)
        .updated_at
        .expect("a real Jira always sets updated");

    let (idle, same) = sync_from(&*source, Some(cursor.clone())).await;
    assert!(
        idle.is_empty(),
        "nothing changed, so nothing is emitted: {:?}",
        keys(&idle)
    );
    assert_eq!(same, cursor, "byte-identical");

    // See [`after_the_second_of`]: an edit inside the same second as the
    // baseline's stamp is the same version of the issue as far as the cursor
    // is concerned, which is a documented adapter limitation and not the thing
    // under test here.
    after_the_second_of(baseline).await;
    guard.label(&seeded, EDITED).await;

    // Jira's search reads an index the write path updates asynchronously, so
    // the run that first sees the edit is polled for rather than assumed to be
    // the very next one. Every poll before that one is an idle run, and hands
    // the cursor straight back.
    let deadline = std::time::Instant::now() + INDEX_BUDGET;
    let (changed, moved) = loop {
        let (items, next) = sync_from(&*source, Some(cursor.clone())).await;
        if !items.is_empty() {
            break (items, next);
        }
        assert_eq!(
            next, cursor,
            "an idle poll hands back the cursor it was given"
        );
        assert!(
            std::time::Instant::now() < deadline,
            "the edit to {EDITED} did not reach Jira's search index within {INDEX_BUDGET:?}"
        );
        tokio::time::sleep(Duration::from_secs(1)).await;
    };

    assert_eq!(
        keys(&changed),
        vec![EDITED.to_owned()],
        "the next incremental run returns exactly the issue that was edited"
    );
    let edited = item(&changed, EDITED);
    assert!(
        edited.payload["fields"]["labels"]
            .as_array()
            .is_some_and(|l| l.iter().any(|v| v == LITTER_LABEL)),
        "the edit is in the payload the run delivered: {}",
        edited.payload["fields"]["labels"]
    );
    let witnessed = edited.updated_at.expect("a real Jira always sets updated");
    // The whole watermark claim, by identity. A third assertion stood here
    // until #347 -- `updated_to(&moved) <= Utc::now()`, carrying the sentence
    // "a watermark after `now()` would mean the run advanced past what it
    // witnessed" -- and it could not fail on top of this one: the equality
    // below pins the position to a stamp the run read off an issue it
    // delivered, so anything that makes the inequality false makes this
    // `assert_eq!` false first. It measured a wall clock as a stand-in for
    // "what this run saw"; this measures what this run saw.
    assert_eq!(
        updated_to(&moved),
        witnessed,
        "the position advances to the edited issue's own `updated` -- not to `now()`, and not \
         past what this run saw"
    );
    assert!(
        witnessed > baseline,
        "the label moved {EDITED}'s own `updated` forward: {witnessed} after {baseline}"
    );

    let (idle, still) = sync_from(&*source, Some(moved.clone())).await;
    assert!(
        idle.is_empty(),
        "the run after the edit skips it -- the pair stayed in the cursor's `seen` set: {:?}",
        keys(&idle)
    );
    assert_eq!(still, moved, "byte-identical");
    println!("SEEDED cursor after the edit to {EDITED}: {moved}");
}

/// **The adapter finds this instance's Epic Link field id by itself, and the
/// id it finds is the one that works** (#297).
///
/// Both halves in one test, because either alone proves nothing worth having.
/// That the discovery *agrees with the seed* would be satisfied by a lookup
/// that returned the right string and was then dropped on the floor; that a
/// configured source mirrors epic membership is already
/// [`epic_membership_lives_in_the_epic_link_field_and_parent_is_absent`]'s
/// claim, and that test reads the id out of `seed-state.json`. What is new
/// here is the **round trip**: the id comes from the server's own answer to
/// the adapter, goes into a `JiraConfig` as the Add-source dialog would put
/// it there, and comes back as `PAY-200` on the payload of an issue in the
/// mirror.
///
/// The seed's record is the control. It is written by
/// `seed-atlassian-content.sh`, which finds the field its own way -- a
/// separate reading of the same server -- so an adapter that discovered
/// `customfield_10102` (*Epic Status* on one of the three seeds) fails here
/// rather than syncing a workflow state as though it were an epic.
#[tokio::test]
#[ignore = "needs testenv's seeded Jira: `just atlassian-live`"]
async fn test_connection_discovers_the_epic_link_field_and_that_id_is_the_one_that_works() {
    let seeded = seeded();
    seeded.clear_leftovers().await;

    let info = seeded
        .source(serde_json::json!({}))
        .test_connection()
        .await
        .expect("the seeded server answers the field table");
    let found = info
        .discovered
        .get("epic_link_field")
        .unwrap_or_else(|| {
            panic!(
                "test_connection discovered no Epic Link field on an instance that has \
                 one ({}): {info:?}",
                seeded.seed.epic_link_field
            )
        })
        .clone();
    assert_eq!(
        found, seeded.seed.epic_link_field,
        "the adapter and `seed-atlassian-content.sh` read the same server and must name \
         the same field; this instance's siblings sit one id along and Epic Status is one \
         of them"
    );
    assert!(
        info.detail
            .as_deref()
            .is_some_and(|d| d.contains(&format!("Epic Link {found}"))),
        "the connection detail names what was found, which is what the sources view \
         shows: {info:?}"
    );

    // The round trip: configured from the *discovered* id, not from the seed's
    // record, so the value under test is the one a reader would have been
    // handed by the dialog.
    let (items, _) = full(&*seeded.source(serde_json::json!({ "epic_link_field": found }))).await;
    assert_eq!(
        item(&items, "PAY-219").payload["fields"][&found],
        serde_json::json!("PAY-200"),
        "an epic child synced through the discovered id must carry its epic"
    );
    println!("SEEDED discovery: test_connection found {found}, and PAY-219 -> PAY-200 through it");
}

/// **The Sprint field is discovered, and a ticket synced through it carries
/// its active sprint** (#811).
///
/// The same round trip as the Epic Link's above: `test_connection` reads the
/// field table and names the Sprint field, the id must be the one
/// `seed-atlassian-content.sh` found by its own reading (the control), and a
/// source configured from the *discovered* id stores each ticket's sprints
/// beside the record, where the descriptor's `sprint_state` reads them. The
/// seed's sprint holds PAY-231, started; every other ticket is in none.
#[tokio::test]
#[ignore = "needs testenv's seeded Jira: `just atlassian-live`"]
async fn test_connection_discovers_the_sprint_field_and_a_synced_ticket_carries_its_active_sprint()
{
    let seeded = seeded();
    seeded.clear_leftovers().await;
    let sprint = seeded.seed.sprint();

    let info = seeded
        .source(serde_json::json!({}))
        .test_connection()
        .await
        .expect("the seeded server answers the field table");
    let found = info
        .discovered
        .get("sprint_field")
        .unwrap_or_else(|| {
            panic!(
                "test_connection discovered no Sprint field on an instance that has one ({}): \
                 {info:?}",
                sprint.field
            )
        })
        .clone();
    assert_eq!(
        found, sprint.field,
        "the adapter and the seed must name the same field"
    );
    assert_ne!(
        found, seeded.seed.epic_link_field,
        "the Sprint field is not Epic Link"
    );

    let (items, _) = full(&*seeded.source(serde_json::json!({ "sprint_field": found }))).await;
    let declared = knobas_source_jira::descriptor_template()
        .payload_paths
        .into_iter()
        .find(|p| p.kind == "ticket")
        .expect("Jira declares its tickets")
        .sprint_state;
    for key in seeded.seeded_keys() {
        let stored = &item(&items, &key).payload["sprints"];
        let states = knobas_core::payload::resolve_list(&item(&items, &key).payload, &declared);
        if sprint.issues.contains(&key) {
            assert_eq!(
                stored,
                &serde_json::json!([{ "id": sprint.id, "name": sprint.name, "state": "active" }]),
                "{key} is in the seed's active sprint"
            );
            assert_eq!(states, ["active"], "{key}: what `sprint:current` reads");
        } else {
            assert_eq!(stored, &serde_json::json!([]), "{key} is in no sprint");
            assert!(states.is_empty(), "{key}");
        }
    }
    // What Jira sent, kept as it was beside the copy: the spelling `map` read.
    println!(
        "SEEDED sprint: test_connection found {found}; {} carries {}",
        sprint.issues[0],
        item(&items, &sprint.issues[0]).payload["fields"][&found]
    );
}

/// **Epic membership lives in the Epic Link custom field, and `fields.parent`
/// is not served at all** -- the largest single divergence this certification
/// found, and the one the Contexts work reads.
///
/// `knobas-mockd` serves the fixture's one epic relationship in *both* of
/// Jira's spellings (its deviation 13): `fields.parent` as a next-gen or a
/// recent company-managed project would, and the Epic Link custom field as a
/// classic one would. A real Data Center project made from the *Basic software
/// development* template is **classic**, and it serves exactly one of them:
/// `fields.parent` is the sub-task relation and is **absent from every issue
/// in this corpus**, epic children included, while the Epic Link field carries
/// `PAY-200` on all five PAY children. Its id is the instance's own and
/// differed on every seed (`customfield_10101`, `_10102`, `_10109`), which is
/// why this test reads it from `seed-state.json` rather than naming one.
///
/// So against this server the `epic_link_field` option is not a fallback for
/// old instances -- it is the only path to epic membership, and a source
/// configured without it mirrors none. That is the claim, and mockd's
/// two-spellings convenience is what hid it. The adapter now also stores the
/// relation as `epic: {"key": …}` beside the record, which is what `under:`
/// reads on this server -- `fields.parent` stays absent, as asserted here (the
/// deputy's ruling of 2026-10-12 on #598).
#[tokio::test]
#[ignore = "needs testenv's seeded Jira: `just atlassian-live`"]
async fn epic_membership_lives_in_the_epic_link_field_and_parent_is_absent() {
    let seeded = seeded();
    seeded.clear_leftovers().await;
    let field = seeded.seed.epic_link_field.clone();
    let (configured, _) =
        full(&*seeded.source(serde_json::json!({ "epic_link_field": field }))).await;
    let (bare, _) = full(&*seeded.source(serde_json::json!({}))).await;

    for it in &configured {
        assert!(
            it.payload["fields"].get("parent").is_none(),
            "{}: a classic Data Center project serves no `fields.parent` -- if this instance now \
             does, epic membership has two readable spellings here and `epic_link_field` has \
             stopped being the only one: {}",
            it.entity.key,
            it.payload["fields"]
        );
    }

    // The epic's five children, from the fixture, by the key the field carries.
    let epic_of = |items: &[SyncItem], key: &str| {
        item(items, key).payload["fields"][&seeded.seed.epic_link_field].clone()
    };
    for key in ["PAY-219", "PAY-228", "PAY-231", "PAY-236", "PAY-240"] {
        assert_eq!(
            epic_of(&configured, key),
            serde_json::json!("PAY-200"),
            "{key} belongs to the fixture's epic"
        );
    }
    // Null rather than absent where there is none -- Jira always answers a
    // custom field a request named, which is the one shape difference between
    // the two spellings and the half mockd already documents.
    for key in ["PAY-200", "OPS-77"] {
        assert_eq!(
            epic_of(&configured, key),
            serde_json::Value::Null,
            "{key} belongs to no epic, and a custom field the request named comes back null \
             rather than absent"
        );
    }

    // The control: without the option the field is not requested, so nothing
    // in the mirror carries the relationship. Without this half, an adapter
    // that fetched the field unconditionally would pass the assertions above
    // and the option would be certifying nothing.
    assert!(
        item(&bare, "PAY-231").payload["fields"]
            .get(&seeded.seed.epic_link_field)
            .is_none(),
        "an unconfigured source does not ask for the field, so the payload does not carry it"
    );
    println!(
        "SEEDED epic membership: {} = PAY-200 on the epic's children; fields.parent absent \
         throughout",
        seeded.seed.epic_link_field
    );
}

/// **What a real Jira does with a credential it does not accept**, and it is
/// not one thing.
///
/// `docs/contract.md` promises "an invalid/absent Bearer returns 401 +
/// `X-Seraph-LoginReason: AUTHENTICATED_FAILED`", and `knobas-mockd` sends
/// that on every rejection. Both were written from the documentation. Measured
/// on Jira 10.3.24 (2026-09-03) the product does three different things:
///
/// | credential | `/serverInfo`, `/myself`, `/issue/…` | `/search` |
/// | --- | --- | --- |
/// | wrong **password** | 401 + `X-Seraph-LoginReason: AUTHENTICATED_FAILED`, and an HTML login page for a body | the same |
/// | wrong **bearer token** | 401, **no Seraph header**, `errorMessages` envelope | **200**, `total: 0` |
/// | none at all | as the wrong bearer token | as the wrong bearer token |
///
/// A bearer token Jira cannot resolve is not a failed login: Seraph never
/// runs, the request proceeds **anonymously**, and `/search` -- which needs no
/// permission to *ask* -- answers 200 with an empty result set. That is the
/// dangerous one, and [`the_search_that_reads_as_an_empty_corpus_is_never_the_first_call`]
/// is about what stops it reaching the mirror.
///
/// Nothing in the adapter reads the header. The fault classification is by
/// status alone (401 **and** 403 ⇒ [`SourceError::Unauthorized`], interfaces
/// §4.1), which is exactly what makes it survive a product that sends the
/// header on one scheme and not the other. The credential-health state the
/// sources view renders from that is asserted end to end in `knobas-app`'s
/// `tests/atlassian_live.rs`.
#[tokio::test]
#[ignore = "needs testenv's seeded Jira: `just atlassian-live`"]
async fn a_rejected_credential_is_a_real_401_and_only_basic_auth_carries_the_seraph_header() {
    let seeded = seeded();

    async fn probe(request: reqwest::RequestBuilder) -> (u16, Option<String>, String) {
        let response = request.send().await.expect("Jira answered");
        let status = response.status().as_u16();
        let seraph = response
            .headers()
            .get("X-Seraph-LoginReason")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        (status, seraph, response.text().await.unwrap_or_default())
    }
    let get = |path: &str| {
        seeded
            .http
            .get(format!("{}/{path}", seeded.url))
            .header("Accept", "application/json")
    };
    let search = "rest/api/2/search?jql=ORDER%20BY%20updated%20ASC&maxResults=1&fields=key";

    // A wrong password, under a username this Jira does not have -- see
    // `Seeded::refused_source` for why it is never the seed's own account.
    // Three requests, one per endpoint, and no more than that.
    let nobody = nobody();
    for path in ["rest/api/2/serverInfo", "rest/api/2/myself", search] {
        let (status, seraph, body) = probe(get(path).basic_auth(&nobody, Some("nope"))).await;
        assert_eq!(status, 401, "{path}: {body}");
        assert_eq!(
            seraph.as_deref(),
            Some("AUTHENTICATED_FAILED"),
            "{path}: basic auth is the scheme that goes through Seraph, so this is the one \
             refusal carrying its header"
        );
        assert!(
            body.contains("Unauthorized (401)"),
            "{path}: a failed login answers Jira's HTML error page, not the `errorMessages` \
             envelope -- which is exactly the body `http::error_envelope` reads nothing out of, \
             so the sources view shows knobas-http's bounded excerpt: {}",
            &body[..body.len().min(200)]
        );
    }
    println!("SEEDED wrong password: 401 + X-Seraph-LoginReason: AUTHENTICATED_FAILED everywhere");

    // A wrong bearer token: anonymous, and therefore *not* a failed login.
    let (status, seraph, body) = probe(get("rest/api/2/serverInfo").bearer_auth(bad_token())).await;
    assert_eq!(status, 401, "{body}");
    assert_eq!(
        seraph, None,
        "a bearer token Jira cannot resolve never reaches Seraph, so no login reason is reported"
    );
    println!("SEEDED wrong bearer token on /serverInfo: 401, no X-Seraph-LoginReason, body {body}");

    // Whatever the scheme, and whatever the header, the adapter classifies it
    // the one way the engine acts on.
    let refused = seeded.refused_source(serde_json::json!({}));
    let connected = refused.test_connection().await;
    assert!(
        matches!(connected, Err(SourceError::Unauthorized { .. })),
        "{connected:?}"
    );
    let synced = refused.sync(None, &mut VecSink(Vec::new())).await;
    assert!(
        matches!(synced, Err(SourceError::Unauthorized { .. })),
        "{synced:?}"
    );

    // And the seed's own account still works. Not decoration: this is the
    // assertion that fails the moment somebody "simplifies" the wrong-password
    // probe above onto the real username, which locks the account out and
    // takes every later test in this run -- and in the app crate's -- with it.
    seeded
        .source(serde_json::json!({}))
        .test_connection()
        .await
        .expect(
            "the seeded admin account must still authenticate after this test: a real Jira \
             answers 403 AUTHENTICATION_DENIED to the *correct* password once an account has \
             failed a few logins, so nothing here may send a wrong password under it",
        );
}

/// **The search that reads as an empty corpus is never the first call a run
/// makes** -- and on this server that ordering is the only thing between a
/// wrong bearer token and an emptied mirror.
///
/// `/rest/api/2/search` needs no permission to *ask*: an unauthenticated
/// request answers **200 with `total: 0`**, which is indistinguishable from a
/// Jira whose issues were all deleted. The `ticket` kind claims
/// `full_sync_exhaustive: true`, so a `cursor: None` run reported `Ok` with no
/// items is the engine's licence to tombstone every ticket in the mirror. A
/// run that started at `/search` would therefore answer a bad credential by
/// deleting the corpus.
///
/// It does not, because the sync run asks `/serverInfo` first -- for the
/// server's UTC offset, which JQL date literals are read in -- and that call
/// is a 401 for the same credential. The ordering was chosen for the timezone
/// and is load-bearing for this; this test is what stops a later reordering (a
/// cached offset, say) from quietly removing the guard.
#[tokio::test]
#[ignore = "needs testenv's seeded Jira: `just atlassian-live`"]
async fn the_search_that_reads_as_an_empty_corpus_is_never_the_first_call() {
    let seeded = seeded();

    // The premise, at the wire: anonymous is a 200 and an empty page.
    let response = seeded
        .http
        .get(format!(
            "{}/rest/api/2/search?jql=ORDER%20BY%20updated%20ASC&maxResults=1&fields=key",
            seeded.url
        ))
        .header("Accept", "application/json")
        .bearer_auth(bad_token())
        .send()
        .await
        .expect("Jira answered");
    assert_eq!(
        response.status().as_u16(),
        200,
        "if this is now a 401 the product has changed and the hazard below is gone -- re-read \
         this test before deleting it"
    );
    let page: serde_json::Value = response.json().await.expect("a search page");
    assert_eq!(
        page["total"], 0,
        "an unresolvable bearer token searches anonymously, and anonymous sees no issue: {page}"
    );

    // The consequence, through the adapter: the run is refused, and refused
    // with *nothing* in the sink. An `Ok` with an empty sink here is the
    // tombstoning case.
    let mut sink = VecSink(Vec::new());
    let mut instance = SourceInstance {
        id: "jira".to_owned(),
        kind: knobas_source_jira::ADAPTER_KIND.to_owned(),
        display_name: "Tidewater Jira (seeded)".to_owned(),
        base_url: seeded.url.clone(),
        auth: Some(AuthMethod::Pat),
        secret: Some(bad_token()),
        account: None,
        config: serde_json::json!({}),
    };
    instance.config["username"] = serde_json::Value::String(seeded.user.clone());
    let source = match knobas_source_jira::build(instance) {
        Ok(s) => s,
        Err(e) => panic!("the adapter must build with a PAT: {e:?}"),
    };
    let refused = source.sync(None, &mut sink).await;
    assert!(
        matches!(refused, Err(SourceError::Unauthorized { .. })),
        "a wrong bearer token must be refused before the search, not reported as a source with \
         no issues: {refused:?}"
    );
    assert!(
        sink.0.is_empty(),
        "nothing is emitted from a refused run: {:?}",
        keys(&sink.0)
    );
    println!(
        "SEEDED anonymous /search: 200 with total 0 -- refused at /serverInfo before the run \
         could report an empty corpus"
    );
}

/// **`under:PAY-200` returns the epic's five children and the epic itself,
/// and nothing else** (#598, ADR-0018).
///
/// On this classic project epic membership is only in the Epic Link field, so
/// the children reach PAY-200 through the `epic` object `map` stores when that
/// field is configured -- the descriptor's second parent candidate -- and
/// PAY-200 answers to its own key through `resolve_names_as_parent`. The set is
/// computed with `knobas_core::payload`'s resolvers over the descriptor's own
/// declaration, the Rust half of the rule the SQL `under:` predicate is the
/// other half of, held together by `knobas-core`'s agreement test; matched
/// ignoring case, as `under:` matches. The expected keys are the fixture's,
/// not what the resolver read. The search seam over these shapes is
/// `knobas-search`'s `under_jira.rs`.
#[tokio::test]
#[ignore = "needs testenv's seeded Jira: `just atlassian-live`"]
async fn under_the_seeded_epic_returns_its_five_children_and_the_epic() {
    use knobas_core::payload::resolve_parents;

    let seeded = seeded();
    seeded.clear_leftovers().await;
    let (items, _) = full(
        &*seeded.source(serde_json::json!({ "epic_link_field": seeded.seed.epic_link_field })),
    )
    .await;
    let declared = ticket_parent_declaration();
    let under_these = |value: &str| under(&items, value);
    let expected = [
        "PAY-200", "PAY-219", "PAY-228", "PAY-231", "PAY-236", "PAY-240",
    ];
    assert_eq!(under_these("PAY-200"), expected);
    assert_eq!(
        under_these("pay-200"),
        expected,
        "under: matches ignoring case"
    );

    for key in &expected[1..] {
        let child = item(&items, key);
        assert_eq!(
            child.payload["epic"],
            serde_json::json!({ "key": "PAY-200" }),
            "{key} carries the epic the adapter stored from {}",
            seeded.seed.epic_link_field
        );
        let parents = resolve_parents(&child.payload, &declared);
        assert_eq!(parents.len(), 1, "{key} sits in one epic: {parents:?}");
        assert_eq!(parents[0].kind, "ticket");
        assert_eq!(parents[0].names, vec!["PAY-200".to_owned()]);
    }
    assert!(
        item(&items, "PAY-200").payload.get("epic").is_none(),
        "the epic belongs to no epic, so nothing is stored for it"
    );
    println!("SEEDED under:PAY-200 -> {:?}", under_these("PAY-200"));
}

/// A sub-task this suite files under a seeded issue, deleted again when the
/// guard drops -- passing or panicking alike -- and the deletion checked, the
/// way [`Labeled`] checks its unlabel.
///
/// The seed puts no sub-task in the corpus (`fixtures/tidewater/work.json` has
/// none), so this is the one place a `fields.parent` exists on this server.
/// Marked with [`LITTER_LABEL`]; a killed run's sub-task is an issue the seed
/// did not create, which [`Seeded::clear_leftovers`] deletes.
struct SubTask {
    url: String,
    user: String,
    password: String,
    parent: String,
    key: Option<String>,
}

impl SubTask {
    /// File a Sub-task under `parent`, and own its deletion from this line on.
    ///
    /// `POST /rest/api/2/issue` with `parent: {key}` and the issue type by
    /// name: the *Basic software development* template's scheme has
    /// Sub-task (`testenv/seed-atlassian-content.sh`'s header). A refusal here
    /// is the instance saying sub-tasks are off or have no create screen, and
    /// the assertion prints its answer.
    async fn file(seeded: &Seeded, parent: &str) -> SubTask {
        let project = parent.split('-').next().expect("a key has a project");
        let mut guard = SubTask {
            url: seeded.url.clone(),
            user: seeded.user.clone(),
            password: seeded.password.clone(),
            parent: parent.to_owned(),
            key: None,
        };
        let (status, body) = seeded
            .request(
                reqwest::Method::POST,
                "rest/api/2/issue",
                Some(serde_json::json!({
                    "fields": {
                        "project": { "key": project },
                        "parent": { "key": parent },
                        "issuetype": { "name": "Sub-task" },
                        "summary": format!("knobas live suite: a sub-task of {parent}"),
                        "labels": [LITTER_LABEL],
                    }
                })),
            )
            .await;
        assert_eq!(status, 201, "filing a Sub-task under {parent}: {body}");
        guard.key = Some(
            body["key"]
                .as_str()
                .unwrap_or_else(|| panic!("a created issue has a key: {body}"))
                .to_owned(),
        );
        guard
    }

    fn key(&self) -> &str {
        self.key.as_deref().expect("filed")
    }
}

impl Drop for SubTask {
    fn drop(&mut self) {
        let Some(key) = self.key.take() else {
            return;
        };
        let (url, user, password) = (self.url.clone(), self.user.clone(), self.password.clone());
        let (filed, parent) = (key.clone(), self.parent.clone());
        Cleanup::new(
            "the live suite did not delete the sub-task it filed, so the server is no longer in \
             the plain-seed state",
            format!("{key} may still exist; the next run's leftover clearing deletes it"),
        )
        .run(move || async move {
            let named_by = format!("parent = {parent}");
            delete_issue(&client(), &url, &user, &password, &filed, &named_by).await
        });
    }
}

/// Delete one issue and **check** it is gone: the issue itself answers 404,
/// and `/search` over `named_by` stops naming it within [`INDEX_BUDGET`] --
/// the index is updated asynchronously and can name an issue already deleted,
/// which is what [`Seeded::clear_leftovers`]' 404 tolerance is for.
async fn delete_issue(
    http: &reqwest::Client,
    url: &str,
    user: &str,
    password: &str,
    key: &str,
    named_by: &str,
) -> Result<(), String> {
    let response = http
        .delete(format!("{url}/rest/api/2/issue/{key}"))
        .header("Accept", "application/json")
        .basic_auth(user, Some(password))
        .send()
        .await
        .map_err(|e| format!("DELETE issue {key}: {e}"))?;
    if response.status().as_u16() != 204 {
        return Err(format!(
            "DELETE issue {key} -> {}: {}",
            response.status(),
            response.text().await.unwrap_or_default()
        ));
    }
    let response = http
        .get(format!("{url}/rest/api/2/issue/{key}?fields=key"))
        .header("Accept", "application/json")
        .basic_auth(user, Some(password))
        .send()
        .await
        .map_err(|e| format!("GET issue {key}: {e}"))?;
    if response.status().as_u16() != 404 {
        return Err(format!(
            "GET issue {key} after its deletion -> {}, not 404",
            response.status()
        ));
    }
    let jql = url_encode(named_by);
    let deadline = tokio::time::Instant::now() + INDEX_BUDGET;
    loop {
        let response = http
            .get(format!(
                "{url}/rest/api/2/search?jql={jql}&maxResults=100&fields=key"
            ))
            .header("Accept", "application/json")
            .basic_auth(user, Some(password))
            .send()
            .await
            .map_err(|e| format!("searching for {key}: {e}"))?;
        let status = response.status().as_u16();
        let text = response.text().await.unwrap_or_default();
        if status != 200 {
            return Err(format!("searching for {key} -> {status}: {text}"));
        }
        let body: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| format!("searching for {key}: unreadable body: {e}: {text}"))?;
        let named = body["issues"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|i| i["key"] == key);
        if !named {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format!(
                "{key} is deleted but /search still names it after {INDEX_BUDGET:?}"
            ));
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/// **A sub-task's parent is `fields.parent`, and `under:` reaches it** -- the
/// live witness for the `ticket` kind's *first* parent candidate (#643).
///
/// Every other `under:` assertion here runs through the second candidate,
/// `epic`, because a classic project keeps epic membership in the Epic Link
/// field and the corpus has no sub-task. Clause 6 gives both candidates one
/// evidence slot (#301), so the live battery passes while `fields.parent`
/// resolves on nothing here; the golden test and `knobas-search`'s
/// `under_jira.rs` read hand-written shapes, and mockd's records were written
/// by the same reading as the adapter. This is the live witness of the wire:
/// `BASE_FIELDS` has to request `parent`, or the payload never carries it.
///
/// So the test files a Sub-task under PAY-219 -- a Task in PAY-200 --
/// full-syncs with the source configured as the Add-source dialog leaves it,
/// and deletes the sub-task again from [`SubTask`]'s `Drop` (the deputy's
/// ruling of 2026-10-12 on #643). Exactly one parent, PAY-219, whatever this
/// instance does with the sub-task's Epic Link: the first candidate that
/// resolves wins. Measured on Jira 10.3.24, 2026-10-12: the sub-task's Epic
/// Link comes back null, so the adapter stores no `epic` for it and
/// `fields.parent` is the only candidate that resolves.
#[tokio::test]
#[ignore = "needs testenv's seeded Jira: `just atlassian-live`"]
async fn a_sub_task_names_its_parent_in_fields_parent_and_under_reaches_it() {
    use knobas_core::payload::resolve_parents;

    let seeded = seeded();
    seeded.clear_leftovers().await;
    let sub_task = SubTask::file(&seeded, "PAY-219").await;
    let key = sub_task.key().to_owned();
    let source =
        seeded.source(serde_json::json!({ "epic_link_field": seeded.seed.epic_link_field }));

    // Filed is not yet searchable: the index is updated asynchronously.
    let deadline = tokio::time::Instant::now() + INDEX_BUDGET;
    let items = loop {
        let (items, _) = full(&*source).await;
        if items.iter().any(|i| i.entity.key == key) {
            break items;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "{key} was filed but no full sync named it within {INDEX_BUDGET:?}: {:?}",
            keys(&items)
        );
        tokio::time::sleep(Duration::from_millis(250)).await;
    };

    let stored = &item(&items, &key).payload;
    assert_eq!(
        stored["fields"]["parent"]["key"],
        serde_json::json!("PAY-219"),
        "{key} carries its parent in `fields.parent`, which only reaches the payload if the \
         run asked for it: {}",
        stored["fields"]
    );
    let parents = resolve_parents(stored, &ticket_parent_declaration());
    assert_eq!(parents.len(), 1, "{key} has one parent: {parents:?}");
    assert_eq!(parents[0].kind, "ticket");
    assert_eq!(parents[0].names, vec!["PAY-219".to_owned()]);

    let mut expected = vec!["PAY-219".to_owned(), key.clone()];
    expected.sort();
    assert_eq!(under(&items, "PAY-219"), expected);
    println!(
        "SEEDED sub-task {key}: fields.parent.key = PAY-219, Epic Link {} = {}, stored epic = \
         {}; under:PAY-219 -> {:?}",
        seeded.seed.epic_link_field,
        stored["fields"][&seeded.seed.epic_link_field],
        stored.get("epic").cloned().unwrap_or_default(),
        under(&items, "PAY-219")
    );
    drop(sub_task);
}

/// How many status moves [`a_long_history_comes_back_whole_from_search`]
/// makes: past 100, the cap Jira Cloud puts on a search's changelog (#1083).
const LONG_HISTORY_MOVES: usize = 120;

/// How many description edits it makes, and how long each new text is: the
/// changelog item of a description edit carries the old and the new text
/// whole, which is the payload growth #1083 asks about.
const LONG_HISTORY_EDITS: usize = 3;
const LONG_HISTORY_EDIT_CHARS: usize = 1_000;

/// **A long status history comes back from `/search` whole** (#1083).
///
/// `expand=changelog` on `/search` (#929) answers a `changelog` object with
/// `startAt`, `maxResults` and `total`, and nothing in the adapter or in the
/// conversation's `HISTORIES` read notices a `total` larger than the
/// histories that came back. Jira Cloud cuts a search's changelog at 100;
/// [`each_issue_carries_its_status_history`] holds `total ==
/// histories.len()` only on seeded issues with a handful of moves each.
///
/// So this files a sub-task under PAY-219 (the corpus stays as seeded; the
/// sub-task is [`SubTask`]'s to delete), moves it through the workflow
/// [`LONG_HISTORY_MOVES`] times and edits its description
/// [`LONG_HISTORY_EDITS`] times, full-syncs through the adapter, and asserts
/// the stored record holds every one of those histories, in order, with
/// `total` saying the same. It also prints what the changelog adds to the
/// stored payload, the seeded corpus's and this issue's, in `MEASURED` lines.
#[tokio::test]
#[ignore = "needs testenv's seeded Jira: `just atlassian-live`"]
async fn a_long_history_comes_back_whole_from_search() {
    let seeded = seeded();
    seeded.clear_leftovers().await;
    let sub_task = SubTask::file(&seeded, "PAY-219").await;
    let key = sub_task.key().to_owned();

    let (code, body) = seeded
        .get(&format!("rest/api/2/issue/{key}?fields=status"))
        .await;
    assert_eq!(code, 200, "{key}: {body}");
    let mut at = body["fields"]["status"]["name"]
        .as_str()
        .unwrap_or_else(|| panic!("{key} stands in a status: {body}"))
        .to_owned();
    let mut statuses: Vec<String> = seeded
        .seed
        .issues
        .iter()
        .map(|i| i.status.clone())
        .collect();
    statuses.sort();
    statuses.dedup();
    assert!(
        statuses.len() >= 2,
        "the corpus stands in one status only: {statuses:?}"
    );
    let mut sent = Vec::with_capacity(LONG_HISTORY_MOVES);
    for n in 0..LONG_HISTORY_MOVES {
        let others: Vec<&String> = statuses.iter().filter(|s| **s != at).collect();
        let to = others[n % others.len()].clone();
        seeded.move_to(&key, &to).await;
        sent.push((at.clone(), to.clone()));
        at = to;
    }
    for n in 0..LONG_HISTORY_EDITS {
        let text = char::from(b'a' + n as u8)
            .to_string()
            .repeat(LONG_HISTORY_EDIT_CHARS);
        let (code, body) = seeded
            .request(
                reqwest::Method::PUT,
                &format!("rest/api/2/issue/{key}"),
                Some(serde_json::json!({ "fields": { "description": text } })),
            )
            .await;
        assert_eq!(code, 204, "editing {key}'s description: {body}");
    }
    let expected = LONG_HISTORY_MOVES + LONG_HISTORY_EDITS;

    // Moved is not yet searchable: the index catches up asynchronously, and
    // until it has, the record's `updated` and changelog may be behind.
    let source = seeded.source(serde_json::json!({}));
    let deadline = tokio::time::Instant::now() + INDEX_BUDGET;
    let items = loop {
        let (items, _) = full(&*source).await;
        let caught_up = items.iter().find(|i| i.entity.key == key).is_some_and(|i| {
            i.payload["fields"]["status"]["name"].as_str() == Some(at.as_str())
                && i.payload["changelog"]["total"].as_u64() == Some(expected as u64)
        });
        if caught_up {
            break items;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "{key} was moved {LONG_HISTORY_MOVES} times and edited {LONG_HISTORY_EDITS} times, \
             but no full sync within {INDEX_BUDGET:?} showed it in {at:?} with a changelog \
             total of {expected}: {:?}",
            items.iter().find(|i| i.entity.key == key).map(|i| (
                &i.payload["fields"]["status"]["name"],
                &i.payload["changelog"]["total"]
            ))
        );
        tokio::time::sleep(Duration::from_millis(250)).await;
    };

    let stored = &item(&items, &key).payload;
    let changelog = &stored["changelog"];
    let histories = changelog["histories"]
        .as_array()
        .unwrap_or_else(|| panic!("{key}: no changelog.histories in the record"));
    println!(
        "SEEDED long history {key}: changelog startAt {} maxResults {} total {}, {} histories \
         returned",
        changelog["startAt"],
        changelog["maxResults"],
        changelog["total"],
        histories.len()
    );
    assert_eq!(
        histories.len(),
        expected,
        "/search returned {} of {key}'s {expected} histories (startAt {}, maxResults {}, \
         total {}): a long history is cut short, and the conversation is missing moves",
        histories.len(),
        changelog["startAt"],
        changelog["maxResults"],
        changelog["total"]
    );
    let moves: Vec<(String, String)> = histories
        .iter()
        .flat_map(|h| h["items"].as_array().into_iter().flatten())
        .filter(|i| i["field"] == "status")
        .map(|i| {
            (
                i["fromString"].as_str().unwrap_or_default().to_owned(),
                i["toString"].as_str().unwrap_or_default().to_owned(),
            )
        })
        .collect();
    assert_eq!(
        moves, sent,
        "{key}: every move, oldest first, as it was made"
    );

    let bytes = |v: &serde_json::Value| serde_json::to_vec(v).expect("JSON").len();
    let corpus: Vec<&SyncItem> = items.iter().filter(|i| i.entity.key != key).collect();
    let corpus_payload: usize = corpus.iter().map(|i| bytes(&i.payload)).sum();
    let corpus_changelog: usize = corpus.iter().map(|i| bytes(&i.payload["changelog"])).sum();
    let corpus_histories: usize = corpus
        .iter()
        .map(|i| {
            i.payload["changelog"]["histories"]
                .as_array()
                .map_or(0, Vec::len)
        })
        .sum();
    println!(
        "MEASURED seeded corpus: {} issues, {corpus_histories} histories, stored payload \
         {corpus_payload} B of which changelog {corpus_changelog} B ({:.1}%)",
        corpus.len(),
        100.0 * corpus_changelog as f64 / corpus_payload as f64
    );
    // Each history this test made holds one change: a status move, or a
    // description edit.
    let (edit_count, edit_bytes, move_count, move_bytes) =
        histories.iter().fold((0, 0, 0, 0), |(ec, eb, mc, mb), h| {
            if h["items"][0]["field"] == "description" {
                (ec + 1, eb + bytes(h), mc, mb)
            } else {
                (ec, eb, mc + 1, mb + bytes(h))
            }
        });
    println!(
        "MEASURED {key}: stored payload {} B of which changelog {} B; {move_count} status moves \
         {move_bytes} B ({} B each), {edit_count} description edits of \
         {LONG_HISTORY_EDIT_CHARS} chars {edit_bytes} B ({} B each)",
        bytes(stored),
        bytes(changelog),
        move_bytes / move_count.max(1),
        edit_bytes / edit_count.max(1),
    );
    drop(sub_task);
}

/// The contract battery -- the suite every adapter must pass -- against the
/// server that decides.
///
/// Clause 2 is the one this corpus was seeded for: an incremental run after no
/// changes emits nothing and returns the same cursor. mockd runs it against a
/// fake that agrees with the adapter by construction; here the index decides.
/// Faults are real: a password that never existed draws Jira's own 401, and a
/// port nothing listens on is unreachable.
#[tokio::test]
#[ignore = "needs testenv's seeded Jira: `just atlassian-live`"]
async fn passes_the_contract_battery_against_the_seeded_server() {
    let seeded = seeded();
    seeded.clear_leftovers().await;
    // Configured the way the Add-source dialog leaves a source: with the Epic
    // Link field *Test connection* discovered, which is "what every source
    // created through the Add-source dialog ends up in" (#381). An
    // unconfigured classic source fails clause 6, and correctly: it stores no
    // `epic`, serves no `fields.parent`, and so every `under:` read on it
    // misses (the deputy's ruling of 2026-10-12 on #598).
    // And the Sprint field it discovered (#811): the declared `sprint_state`
    // then resolves on the seed's active sprint, which clause 3 weighs.
    let configured = serde_json::json!({
        "epic_link_field": seeded.seed.epic_link_field,
        "sprint_field": seeded.seed.sprint().field,
    });
    battery(move |fault| match fault {
        Fault::None => seeded.source(configured.clone()),
        // A bearer token, not a wrong password: the battery builds a faulted
        // adapter twice and then calls the healthy one again (clauses 5 and 6),
        // and a wrong password would have locked the account out by then.
        Fault::Unauthorized => seeded.refused_source(serde_json::json!({})),
        Fault::Unreachable => seeded.source_at(&dead_url(), serde_json::json!({})),
    })
    .await;
}

/// The suite's account owns a seeded issue's **assignee** for the length of the
/// test, and gives it back -- checked, the way [`Labeled`] checks its unlabel.
///
/// A separate guard from `Labeled` rather than a second mode on it: the field
/// is different, the restore is "back to whoever it was" rather than "remove
/// what we added", and an issue whose seeded assignee was *nobody* has to come
/// back unassigned rather than assigned to the seed's admin.
struct Reassigned {
    url: String,
    user: String,
    password: String,
    /// `(key, the name it had, or `None` for unassigned)`.
    was: Option<(String, Option<String>)>,
}

impl Reassigned {
    fn new(seeded: &Seeded) -> Reassigned {
        Reassigned {
            url: seeded.url.clone(),
            user: seeded.user.clone(),
            password: seeded.password.clone(),
            was: None,
        }
    }

    /// `PUT /rest/api/2/issue/{key}/assignee` -- Jira's **dedicated** assignee
    /// endpoint, which is the one issue #345 is about, and not a `fields` edit
    /// through the generic issue `PUT`.
    async fn take(&mut self, seeded: &Seeded, key: &str) {
        let before = self.remember(seeded, key).await;
        assert_ne!(
            before.as_deref(),
            Some(seeded.user.as_str()),
            "{key} is already the suite's, so becoming its assignee would witness nothing"
        );
        self.set(seeded, key, Some(&seeded.user.clone())).await;
    }

    /// Assign `key` to `to`, or to nobody -- **repeatable**, so a test that has
    /// to try a timing window more than once can put the issue back and go
    /// again. The *first* call is what records the seeded owner, so however
    /// many times this runs, [`Drop`] restores the state the suite found.
    async fn set(&mut self, seeded: &Seeded, key: &str, to: Option<&str>) {
        self.remember(seeded, key).await;
        let (status, body) = seeded
            .request(
                reqwest::Method::PUT,
                &format!("rest/api/2/issue/{key}/assignee"),
                Some(serde_json::json!({ "name": to })),
            )
            .await;
        assert_eq!(status, 204, "assigning {key} to {to:?}: {body}");
    }

    /// The assignee `key` had when this guard first touched it.
    async fn remember(&mut self, seeded: &Seeded, key: &str) -> Option<String> {
        if let Some((owned, was)) = &self.was {
            assert_eq!(owned, key, "this guard owns exactly one issue");
            return was.clone();
        }
        let (status, body) = seeded
            .get(&format!("rest/api/2/issue/{key}?fields=assignee"))
            .await;
        assert_eq!(status, 200, "reading {key}'s assignee: {body}");
        let before = body["fields"]["assignee"]["name"]
            .as_str()
            .map(str::to_owned);
        self.was = Some((key.to_owned(), before.clone()));
        before
    }
}

impl Drop for Reassigned {
    fn drop(&mut self) {
        let Some((key, was)) = self.was.take() else {
            return;
        };
        let (url, user, password) = (self.url.clone(), self.user.clone(), self.password.clone());
        let restore = key.clone();
        Cleanup::new(
            format!(
                "the live suite did not give {key}'s assignee back, so the server is no longer in \
                 the plain-seed state"
            ),
            format!("{key} may still be assigned to the suite"),
        )
        .run(move || async move {
            reassign(&client(), &url, &user, &password, &restore, was.as_deref()).await
        });
    }
}

/// Put one issue's assignee back and **check** it landed -- a server that
/// stopped answering must not read as a cleanup that worked.
async fn reassign(
    http: &reqwest::Client,
    url: &str,
    user: &str,
    password: &str,
    key: &str,
    to: Option<&str>,
) -> Result<(), String> {
    let body = serde_json::json!({ "name": to });
    let response = http
        .put(format!("{url}/rest/api/2/issue/{key}/assignee"))
        .header("Accept", "application/json")
        .basic_auth(user, Some(password))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("PUT {key}/assignee: {e}"))?;
    let status = response.status().as_u16();
    if status != 204 {
        let text = response.text().await.unwrap_or_default();
        return Err(format!("PUT {key}/assignee answered {status}: {text}"));
    }
    let response = http
        .get(format!("{url}/rest/api/2/issue/{key}?fields=assignee"))
        .header("Accept", "application/json")
        .basic_auth(user, Some(password))
        .send()
        .await
        .map_err(|e| format!("re-reading {key}: {e}"))?;
    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("re-reading {key}: {e}"))?;
    let now = json["fields"]["assignee"]["name"].as_str();
    if now == to {
        Ok(())
    } else {
        Err(format!(
            "{key} should be assigned to {to:?} again and Jira reports {now:?}"
        ))
    }
}

/// **A reassignment through Jira's dedicated assignee endpoint reaches the next
/// incremental run's `author`** (issue #345).
///
/// The ticket's hypothesis was that `PUT /rest/api/2/issue/{key}/assignee` does
/// not move the issue's `updated`, which would put the issue permanently below
/// every later incremental query's lower bound. That was measured against this
/// container and is **false**: the PUT moves `updated` and Jira's search index
/// carries the new stamp within a second. This test is what keeps that answer
/// from having to be re-measured by hand, and it is the one shape
/// [`an_incremental_run_after_one_edit_returns_that_issue_and_moves_the_watermark_to_it`]
/// cannot cover -- that test edits `labels` through the generic issue `PUT`,
/// and the whole question here was whether the *dedicated* endpoint behaves
/// differently.
///
/// It asserts `author` and not just delivery, because `author` is what the
/// standup digest's mirror half, the inbox's author matching (#82) and every
/// `@me` filter key on: an adapter that re-delivered the issue while mapping
/// the old assignee would satisfy "the run returned it" and still leave every
/// one of those readers wrong.
#[tokio::test]
#[ignore = "needs testenv's seeded Jira: `just atlassian-live`"]
async fn a_reassignment_through_the_assignee_endpoint_reaches_the_next_incremental_run() {
    let seeded = seeded();
    seeded.clear_leftovers().await;
    let mut guard = Reassigned::new(&seeded);
    let source = seeded.source(serde_json::json!({}));
    const BORROWED: &str = "PAY-240";

    let (items, cursor) = full(&*source).await;
    let before = item(&items, BORROWED);
    let baseline = before.updated_at.expect("a real Jira always sets updated");
    let was = before.author.clone();
    assert_ne!(
        was.as_deref(),
        Some(seeded.user.as_str()),
        "the full sync must start with {BORROWED} as somebody else's"
    );

    // The same second-boundary wait the label test needs: an edit inside the
    // second the baseline recorded is, to the cursor, the same version of the
    // issue. See `after_the_second_of`.
    after_the_second_of(baseline).await;
    guard.take(&seeded, BORROWED).await;

    // Polled rather than assumed, for the reason #325 records and #289's live
    // run repeated: the adapter enumerates through Lucene, which the write
    // path updates asynchronously.
    let deadline = std::time::Instant::now() + INDEX_BUDGET;
    let (changed, moved) = loop {
        let (items, next) = sync_from(&*source, Some(cursor.clone())).await;
        if !items.is_empty() {
            break (items, next);
        }
        assert_eq!(
            next, cursor,
            "an idle poll hands back the cursor it was given"
        );
        assert!(
            std::time::Instant::now() < deadline,
            "the reassignment of {BORROWED} did not reach Jira's search index within \
             {INDEX_BUDGET:?}"
        );
        tokio::time::sleep(Duration::from_secs(1)).await;
    };

    assert_eq!(
        keys(&changed),
        vec![BORROWED.to_owned()],
        "the next incremental run returns exactly the issue that was reassigned"
    );
    let now = item(&changed, BORROWED);
    assert_eq!(
        now.author.as_deref(),
        Some(seeded.user.as_str()),
        "the run delivered {BORROWED} still attributed to {was:?} -- `author` is the assignee on \
         Jira, and the standup digest, the inbox and every `@me` filter read it"
    );
    let witnessed = now.updated_at.expect("a real Jira always sets updated");
    assert!(
        witnessed > baseline,
        "the assignee endpoint moved {BORROWED}'s own `updated` forward: {witnessed} after \
         {baseline} -- this is the measurement #345's hypothesis got backwards"
    );
    assert_eq!(
        updated_to(&moved),
        witnessed,
        "the position advances to the reassigned issue's own `updated`"
    );
    println!(
        "SEEDED {BORROWED} reassigned {was:?} -> {:?}; updated {baseline} -> {witnessed}",
        now.author
    );
}

/// **Two changes to one issue inside one second, with a run between them, and
/// both reach the sink** (issue #345, the acceptance criterion).
///
/// This is the bug in its own habitat, and the one thing no fake can settle:
/// that Jira's `/search` really does report `updated` to the second while the
/// issue underneath it changes in milliseconds, so two changes can share a
/// stamp *at the only resolution a run can observe*.
///
/// The run in the middle is what made the loss permanent rather than transient.
/// `knobas_app::sources::write_queue::after_landing` runs one after every landed
/// write, so *comment through knobas, then reassign in Jira* put the issue into
/// `seen` at the shared second, and no later run ever reached it -- `updated`
/// does not move again on its own, so only a backfill recovered it.
///
/// # Why this retries, and what it does *not* retry
///
/// The collision cannot be commanded: it needs the reassignment to land in the
/// same wall-clock second as the change the intervening run recorded, and
/// between them sits a wait on Jira's asynchronous search index -- the one this
/// file budgets [`INDEX_BUDGET`] for and #325 records. Measured on this
/// container that wait is usually zero polls, so the pair usually shares a
/// second; on a loaded machine it need not. An earlier version of this test
/// asserted the shared second outright, which meant a slow index failed the
/// suite with no bug present.
///
/// So each attempt resets the issue, tries again, and **only the collision is
/// retried**. Once one is achieved the delivery assertion is made once and is
/// not retried at all: a reassignment that did not arrive is a failure, never
/// something to poll past. Exhausting the attempts is also a failure -- it
/// means this environment cannot produce the shape the criterion is about --
/// and it reports every gap it measured so the reason is on the log.
#[tokio::test]
#[ignore = "needs testenv's seeded Jira: `just atlassian-live`"]
async fn two_changes_inside_one_second_with_a_run_between_them_both_reach_the_sink() {
    /// Each attempt costs a full sync of seven issues plus four small writes.
    const ATTEMPTS: usize = 6;

    let seeded = seeded();
    // `Labeled::new` clears an earlier run's leftovers, so each attempt starts
    // from the plain corpus.
    let mut labelled = Labeled::new(&seeded).await;
    let mut assigned = Reassigned::new(&seeded);
    let source = seeded.source(serde_json::json!({}));
    const BORROWED: &str = "PAY-240";

    let mut gaps: Vec<i64> = Vec::new();
    let mut collided: Option<(String, chrono::DateTime<chrono::Utc>)> = None;

    for attempt in 1..=ATTEMPTS {
        // Back to the baseline, so every attempt asks the identical question:
        // no litter label, and the seeded owner on the ticket.
        labelled.set(&seeded, BORROWED, false).await;
        let owner = assigned.remember(&seeded, BORROWED).await;
        assigned.set(&seeded, BORROWED, owner.as_deref()).await;

        // A position that already holds this issue's current record.
        let (items, cursor) = full(&*source).await;
        assert_eq!(keys(&items), seeded.seeded_keys());
        let baseline = item(&items, BORROWED)
            .updated_at
            .expect("a real Jira always sets updated");
        after_the_second_of(baseline).await;

        // Change one: a label, through the generic issue PUT.
        labelled.set(&seeded, BORROWED, true).await;

        // The run in between -- the one that records the issue at that second.
        let deadline = std::time::Instant::now() + INDEX_BUDGET;
        let (recorded, between) = loop {
            let (items, next) = sync_from(&*source, Some(cursor.clone())).await;
            if !items.is_empty() {
                let seen = item(&items, BORROWED);
                assert!(
                    seen.payload["fields"]["labels"]
                        .as_array()
                        .is_some_and(|l| l.iter().any(|v| v == LITTER_LABEL)),
                    "the run in between must be the one that saw the label, or the \
                     collision below is not the one being tested"
                );
                break (
                    seen.updated_at.expect("a real Jira always sets updated"),
                    next,
                );
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the label did not reach the index within {INDEX_BUDGET:?}"
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        };

        // Change two, immediately: the reassignment, through the dedicated
        // endpoint.
        assigned
            .set(&seeded, BORROWED, Some(&seeded.user.clone()))
            .await;
        let after = stamp_of(&seeded, BORROWED).await;

        // The premise, from the server rather than from hope. `/search`
        // truncates to whole seconds, so this is the comparison a run can
        // actually make -- and `recorded` is the value that really went into
        // the cursor, read back off the item the run delivered.
        let gap = (after - recorded).num_milliseconds();
        gaps.push(gap);
        println!(
            "SEEDED attempt {attempt}: the run recorded {BORROWED} at {recorded}, the \
             reassignment landed at {after} ({gap} ms later) -- same second: {}",
            recorded.timestamp() == after.timestamp()
        );
        if recorded.timestamp() == after.timestamp() {
            collided = Some((between, after));
            break;
        }
    }

    let Some((between, _)) = collided else {
        panic!(
            "{ATTEMPTS} attempts and the reassignment never landed in the second the \
             previous run recorded, so this run cannot witness the collision #345 is \
             about. Gaps in ms: {gaps:?}. Each gap is one wait on Jira's asynchronous \
             search index plus a sync and a PUT, so a machine this slow is one where the \
             assertion below would pass for the wrong reason."
        )
    };

    // And now the assertion, made once: the second change is delivered although
    // the only timestamp a run can see is unchanged. Not polled -- a
    // reassignment that did not arrive is the bug, not a slow index, and the
    // index has already been shown to be current by the run above.
    let (delivered, _) = sync_from(&*source, Some(between)).await;
    assert_eq!(
        keys(&delivered),
        vec![BORROWED.to_owned()],
        "the reassignment shares a second with the change the previous run recorded, and \
         that is exactly the record the cursor's fingerprint exists to tell apart"
    );
    let now = item(&delivered, BORROWED);
    assert_eq!(
        now.author.as_deref(),
        Some(seeded.user.as_str()),
        "the second change reached the sink but not its content"
    );
    assert!(
        now.payload["fields"]["labels"]
            .as_array()
            .is_some_and(|l| l.iter().any(|v| v == LITTER_LABEL)),
        "and the first change is still on the record it delivered -- an adapter that \
         delivered the reassignment while losing the label would be the same bug wearing \
         the other shoe"
    );
}

/// One issue's `updated` as the **issue endpoint** reports it -- to the
/// millisecond, which `/search` does not.
async fn stamp_of(seeded: &Seeded, key: &str) -> chrono::DateTime<chrono::Utc> {
    let (status, body) = seeded
        .get(&format!("rest/api/2/issue/{key}?fields=updated"))
        .await;
    assert_eq!(status, 200, "reading {key}'s updated: {body}");
    let raw = body["fields"]["updated"]
        .as_str()
        .unwrap_or_else(|| panic!("{key} has no updated: {body}"));
    chrono::DateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S%.3f%z")
        .unwrap_or_else(|e| panic!("{raw:?}: {e}"))
        .with_timezone(&chrono::Utc)
}

/// **An untouched source stays quiet, poll after poll after poll** (battery
/// clause 2, against the real index).
///
/// The suite already asserts one idle poll. This asserts [`IDLE_POLLS`] of
/// them, and it exists for one specific risk that arrived with issue #345's
/// fingerprint identity: `seen` now recognises a record by a fingerprint of
/// everything `/search` returned for it, so **any** field that differs between
/// two reads of an unchanged issue makes every poll re-emit the whole window.
/// Correctness would survive that -- upserts are idempotent -- and clause 2
/// would not, and neither would the user's activity log, which would grow a
/// line per source per poll for ever.
///
/// No fake can find that: a fake answers whatever it was seeded with, so a
/// volatile field is by construction invisible to it. It takes the real server
/// answering the same query repeatedly, which is what this does. It fails on
/// the **first** poll that emits anything, and names the issue, so a flapping
/// field is loud rather than a quiet doubling of every sync.
#[tokio::test]
#[ignore = "needs testenv's seeded Jira: `just atlassian-live`"]
async fn an_untouched_source_is_still_quiet_after_many_polls() {
    /// Enough to mean it, and still under a second in total against a local
    /// container: the failure this looks for is deterministic, so the number is
    /// about conviction rather than about catching a rare event.
    const IDLE_POLLS: usize = 12;

    let seeded = seeded();
    seeded.clear_leftovers().await;
    let source = seeded.source(serde_json::json!({}));

    let (items, mut cursor) = full(&*source).await;
    assert_eq!(keys(&items), seeded.seeded_keys());

    for poll in 1..=IDLE_POLLS {
        let (items, next) = sync_from(&*source, Some(cursor.clone())).await;
        assert!(
            items.is_empty(),
            "poll {poll} of {IDLE_POLLS} emitted {:?} from a source nothing has touched. \
             Every one of those issues was recognised on the polls before it, so what \
             changed is Jira's answer and not the corpus: some field in the `/search` \
             record differs between two reads, and the cursor's fingerprint is over the whole \
             record. Find it by diffing two consecutive `/search` responses for that key.",
            keys(&items)
        );
        assert_eq!(
            next, cursor,
            "poll {poll} emitted nothing and still handed back different bytes"
        );
        cursor = next;
    }
    println!("SEEDED {IDLE_POLLS} idle polls, all quiet, cursor unchanged: {cursor}");
}

/// The cursor as it reads an hour on (#1700): an idle run reads the boards at
/// most hourly, so a test that changes a board in Jira and syncs at once ages
/// the cursor's board-read stamp instead of waiting an hour.
fn an_hour_on(cursor: &str) -> String {
    let mut envelope: serde_json::Value = serde_json::from_str(cursor).expect("a JSON cursor");
    if let Some(read) = envelope.get_mut("board_read") {
        read["at"] = serde_json::json!("2000-01-01T00:00:00Z");
    }
    envelope.to_string()
}

/// **The Jira views (#877) against the seeded boards**, from a source that
/// syncs PAY alone. The Scrum board holds what its active sprint holds; the
/// Kanban board over PAY and OPS carries Jira's sub-filter, the limit the seed
/// put on In Progress, and every key Jira answers -- OPS ones included, which
/// this source does not sync -- in exactly the order the same query answers
/// over plain `/search` (the control). An idle poll emits no view; a board
/// made and then deleted in Jira arrives, then arrives as gone.
#[tokio::test]
#[ignore = "needs testenv's seeded Jira: `just jira-live`"]
async fn the_seeded_boards_arrive_as_jira_views_and_a_board_deleted_in_jira_is_gone_at_the_next_sync()
 {
    const PROBE: &str = "knobas live probe board";
    let seeded = seeded();
    seeded.clear_leftovers().await;
    let sprint = seeded.seed.sprint();
    let kanban = &seeded
        .seed
        .views
        .as_ref()
        .expect("seed-state.json records no Jira views: re-run `./seed-atlassian-content.sh jira` (#877)")
        .kanban;
    // A probe a killed run left behind.
    let (_, listed) = seeded
        .get(&format!("rest/agile/1.0/board?name={}", url_encode(PROBE)))
        .await;
    for board in listed["values"].as_array().into_iter().flatten() {
        seeded
            .request(
                reqwest::Method::DELETE,
                &format!("rest/agile/1.0/board/{}", board["id"]),
                None,
            )
            .await;
    }

    let source = seeded.source(serde_json::json!({ "projects": ["PAY"] }));
    let (items, cursor) = full(&*source).await;
    let views: Vec<&SyncItem> = items.iter().filter(|i| i.kind == "jira_view").collect();
    let view = |id: u64| {
        views
            .iter()
            .find(|v| v.entity.key == format!("board-{id}"))
            .unwrap_or_else(|| panic!("no view of board {id} among {:?}", keys(&items)))
    };

    let scrum = view(sprint.board_id);
    assert_eq!(scrum.payload["type"], "scrum");
    assert_eq!(scrum.payload["sprint"]["name"], sprint.name.as_str());
    assert_eq!(
        scrum.payload["keys"],
        serde_json::json!(sprint.issues),
        "the active sprint's tickets"
    );

    let flow = view(kanban.board_id);
    assert_eq!(flow.title, kanban.name);
    assert_eq!(flow.payload["type"], "kanban");
    assert_eq!(flow.payload["jql"], kanban.jql.as_str());
    assert_eq!(flow.payload["sub_query"], kanban.sub_query.as_str());
    let limited = flow.payload["columns"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| {
            c["statuses"]
                .as_array()
                .unwrap()
                .iter()
                .any(|s| s == kanban.limit_column.as_str())
        })
        .unwrap_or_else(|| {
            panic!(
                "no column holds {}: {}",
                kanban.limit_column, flow.payload["columns"]
            )
        });
    assert_eq!(limited["limit"], kanban.limit);
    let query = flow.payload["query"].as_str().unwrap();
    let control: Vec<String> = seeded
        .jql(query, "key")
        .await
        .iter()
        .map(|row| row["key"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        flow.payload["keys"],
        serde_json::json!(control),
        "Jira's answer, in Jira's order"
    );
    assert!(
        control.iter().any(|key| key.starts_with("OPS-")),
        "the board reaches OPS, which this source does not sync: {control:?}"
    );

    let (idle, same) = sync_from(&*source, Some(cursor.clone())).await;
    assert!(
        idle.iter().all(|i| i.kind != "jira_view"),
        "an idle poll emits no view: {:?}",
        keys(&idle)
    );
    assert_eq!(same, cursor);

    // A board made in Jira, then deleted there.
    let (status, filter) = seeded
        .request(
            reqwest::Method::POST,
            "rest/api/2/filter",
            Some(serde_json::json!({ "name": format!("{PROBE} filter {}", uuid_like()), "jql": "project = PAY ORDER BY Rank ASC", "favourite": false })),
        )
        .await;
    assert_eq!(status, 200, "{filter}");
    let filter_id: u64 = filter["id"].as_str().unwrap().parse().unwrap();
    let (status, board) = seeded
        .request(
            reqwest::Method::POST,
            "rest/agile/1.0/board",
            Some(serde_json::json!({ "name": PROBE, "type": "kanban", "filterId": filter_id })),
        )
        .await;
    assert!(status == 200 || status == 201, "{status} {board}");
    let probe = format!("board-{}", board["id"]);
    let (made, after_made) = sync_from(&*source, Some(an_hour_on(&cursor))).await;
    assert!(
        made.iter().any(|i| i.entity.key == probe && !i.deleted),
        "{:?}",
        keys(&made)
    );
    let (status, _) = seeded
        .request(
            reqwest::Method::DELETE,
            &format!("rest/agile/1.0/board/{}", board["id"]),
            None,
        )
        .await;
    assert_eq!(status, 204);
    seeded
        .request(
            reqwest::Method::DELETE,
            &format!("rest/api/2/filter/{filter_id}"),
            None,
        )
        .await;
    let (gone, _) = sync_from(&*source, Some(an_hour_on(&after_made))).await;
    let gone: Vec<&SyncItem> = gone.iter().filter(|i| i.kind == "jira_view").collect();
    assert_eq!(gone.len(), 1, "{gone:?}");
    assert_eq!(gone[0].entity.key, probe);
    assert!(gone[0].deleted);
    println!(
        "SEEDED Jira views: {} boards on PAY; {} holds {} keys ({} from OPS); the probe {probe} came and went",
        views.len(),
        kanban.name,
        control.len(),
        control.iter().filter(|key| key.starts_with("OPS-")).count()
    );
}

/// **Favourite filters and the *Boards and filters* setting (#878)** against
/// the seed's favourite filter: it arrives as a Jira view holding what its
/// JQL answers over plain `/search`, in that order (the control), with Jira's
/// `fixVersion is EMPTY` run by Jira and never translated. The setting is
/// read at the next sync: turned to no boards and no filters, every view the
/// last sync delivered arrives as gone.
#[tokio::test]
#[ignore = "needs testenv's seeded Jira: `just jira-live`"]
async fn the_seeded_favourite_filter_arrives_as_a_jira_view_and_the_setting_takes_effect_at_the_next_sync()
 {
    let seeded = seeded();
    let fav = &seeded
        .seed
        .views
        .as_ref()
        .expect("seed-state.json records no Jira views: re-run `./seed-atlassian-content.sh jira` (#877)")
        .favourite_filter;
    let source =
        seeded.source(serde_json::json!({ "projects": ["PAY"], "favourite_filters": true }));
    let (items, cursor) = full(&*source).await;
    let key = format!("filter-{}", fav.id);
    let filter = items
        .iter()
        .find(|i| i.kind == "jira_view" && i.entity.key == key)
        .unwrap_or_else(|| panic!("no view of filter {} among {:?}", fav.id, keys(&items)));
    assert_eq!(filter.title, fav.name);
    assert_eq!(filter.payload["kind"], "filter");
    assert_eq!(filter.payload["jql"], fav.jql.as_str());
    assert_eq!(
        filter.payload["query"],
        fav.jql.as_str(),
        "searched as Jira holds it"
    );
    let control: Vec<String> = seeded
        .jql(&fav.jql, "key")
        .await
        .iter()
        .map(|row| row["key"].as_str().unwrap().to_owned())
        .collect();
    assert!(
        !control.is_empty(),
        "the seed's PAY tickets have no fix version"
    );
    assert_eq!(
        filter.payload["keys"],
        serde_json::json!(control),
        "Jira's answer, in Jira's order"
    );
    let delivered: Vec<String> = items
        .iter()
        .filter(|i| i.kind == "jira_view")
        .map(|i| i.entity.key.clone())
        .collect();
    assert!(
        delivered.iter().any(|k| k.starts_with("board-")),
        "the boards on PAY came too: {delivered:?}"
    );

    let off = seeded.source(serde_json::json!({
        "projects": ["PAY"], "boards": "none", "favourite_filters": false,
    }));
    let (gone, _) = sync_from(&*off, Some(cursor)).await;
    let mut gone_keys: Vec<String> = gone
        .iter()
        .filter(|i| i.kind == "jira_view")
        .inspect(|i| assert!(i.deleted, "{} is not delivered as gone", i.entity.key))
        .map(|i| i.entity.key.clone())
        .collect();
    gone_keys.sort();
    let mut expected = delivered.clone();
    expected.sort();
    assert_eq!(
        gone_keys, expected,
        "every view the last sync delivered, as gone"
    );
    println!(
        "SEEDED favourite filter {} holds {} keys; {} views went when the setting turned them off",
        fav.name,
        control.len(),
        gone_keys.len()
    );
}

/// **The *Only boards built on my favourite filters* choice (#1096, #1136)**:
/// with `boards: "favourites"`, a board whose saved filter is a favourite
/// filter comes in as a Jira view, and the seeded boards, whose filters are
/// not favourites, do not. The test makes its own favourite filter and board
/// and takes both back before it asserts, so the shared fixture keeps no
/// favourite-filter board (the seed's favourite filter has none).
#[tokio::test]
#[ignore = "needs testenv's seeded Jira: `just jira-live`"]
async fn only_the_boards_built_on_my_favourite_filters_come_in_when_the_choice_is_set() {
    const PROBE: &str = "knobas live probe board on a favourite filter";
    let seeded = seeded();
    let sprint = seeded.seed.sprint();
    let kanban = &seeded
        .seed
        .views
        .as_ref()
        .expect("seed-state.json records no Jira views: re-run `./seed-atlassian-content.sh jira` (#877)")
        .kanban;
    // A probe a killed run left behind: the board, then its filter.
    let (_, listed) = seeded
        .get(&format!("rest/agile/1.0/board?name={}", url_encode(PROBE)))
        .await;
    for board in listed["values"].as_array().into_iter().flatten() {
        seeded
            .request(
                reqwest::Method::DELETE,
                &format!("rest/agile/1.0/board/{}", board["id"]),
                None,
            )
            .await;
    }
    let (_, favourites) = seeded.get("rest/api/2/filter/favourite").await;
    for filter in favourites.as_array().into_iter().flatten() {
        if filter["name"]
            .as_str()
            .is_some_and(|n| n.starts_with(PROBE))
        {
            seeded
                .request(
                    reqwest::Method::DELETE,
                    &format!("rest/api/2/filter/{}", filter["id"].as_str().unwrap()),
                    None,
                )
                .await;
        }
    }

    let (status, filter) = seeded
        .request(
            reqwest::Method::POST,
            "rest/api/2/filter",
            Some(serde_json::json!({ "name": format!("{PROBE} {}", uuid_like()), "jql": "project = PAY ORDER BY Rank ASC", "favourite": true })),
        )
        .await;
    assert_eq!(status, 200, "{filter}");
    assert_eq!(filter["favourite"], true, "{filter}");
    let filter_id: u64 = filter["id"].as_str().unwrap().parse().unwrap();
    let (status, board) = seeded
        .request(
            reqwest::Method::POST,
            "rest/agile/1.0/board",
            Some(serde_json::json!({ "name": PROBE, "type": "kanban", "filterId": filter_id })),
        )
        .await;
    let made = status == 200 || status == 201;
    let items = if made {
        let source = seeded.source(serde_json::json!({
            "projects": ["PAY"], "boards": "favourites", "favourite_filters": false,
        }));
        full(&*source).await.0
    } else {
        Vec::new()
    };
    // Taken back before anything is asserted, so a failure leaves nothing.
    let mut taken_back = Vec::new();
    if made {
        let (status, _) = seeded
            .request(
                reqwest::Method::DELETE,
                &format!("rest/agile/1.0/board/{}", board["id"]),
                None,
            )
            .await;
        taken_back.push(status);
    }
    let (status_filter, _) = seeded
        .request(
            reqwest::Method::DELETE,
            &format!("rest/api/2/filter/{filter_id}"),
            None,
        )
        .await;
    taken_back.push(status_filter);
    assert!(made, "{status} {board}");
    assert!(
        taken_back.iter().all(|s| (200..300).contains(s)),
        "the probe board and filter were not taken back: {taken_back:?}"
    );

    let probe = format!("board-{}", board["id"]);
    let views: Vec<&str> = items
        .iter()
        .filter(|i| i.kind == "jira_view")
        .map(|i| i.entity.key.as_str())
        .collect();
    for seeded_board in [sprint.board_id, kanban.board_id] {
        let key = format!("board-{seeded_board}");
        assert!(
            !views.contains(&key.as_str()),
            "{key}'s filter is not a favourite, yet it came in: {views:?}"
        );
    }
    assert_eq!(
        views,
        [probe.as_str()],
        "exactly the board built on a favourite filter, and no filter view"
    );
    println!(
        "SEEDED favourite boards: {probe} on favourite filter {filter_id} came in; boards {} and {} did not",
        sprint.board_id, kanban.board_id
    );
}

/// The names this suite's refused-query probe gives its board, its favourite
/// filter and its version on PAY (#1893). Not a substring of any other
/// probe's name: the board listing's `name=` matches on a part of the name.
const REFUSED_PROBE: &str = "knobas live refused-query probe";

/// One thing a probe made on the seeded Jira, by its id.
#[derive(Debug, Clone, PartialEq)]
enum Probed {
    Board(u64),
    Filter(u64),
    Version(String),
}

impl Probed {
    fn path(&self) -> String {
        match self {
            Probed::Board(id) => format!("rest/agile/1.0/board/{id}"),
            Probed::Filter(id) => format!("rest/api/2/filter/{id}"),
            Probed::Version(id) => format!("rest/api/2/version/{id}"),
        }
    }

    /// Whether Jira's answer about it says it is not there. Jira Data Center
    /// 10 answers a filter that is not there 400 ("not available to you,
    /// perhaps it has been deleted"), to a `GET` and a `DELETE` alike, where
    /// a board or a version that is not there is a 404.
    fn gone(&self, status: u16) -> bool {
        status == 404 || (status == 400 && matches!(self, Probed::Filter(_)))
    }

    /// The request that removes it. Jira Data Center 10 answers `DELETE` on a
    /// version 405, so a version goes through `removeAndSwap` (no issue holds
    /// it).
    fn removal(&self, http: &reqwest::Client, url: &str) -> reqwest::RequestBuilder {
        match self {
            Probed::Version(_) => http
                .post(format!("{url}/{}/removeAndSwap", self.path()))
                .json(&serde_json::json!({})),
            Probed::Board(_) | Probed::Filter(_) => http.delete(format!("{url}/{}", self.path())),
        }
    }
}

/// What one probe made on the seeded Jira: the refused-query probe's version
/// on PAY, favourite filter naming it and Kanban board on that filter
/// (#1893), or the board-order probe's boards and their filters (#2003).
/// [`Drop`] takes back whatever of it is still there, passing or panicking
/// alike, through `knobas_live_cleanup::Cleanup`, and checks each one is
/// gone afterwards.
struct Probes {
    url: String,
    user: String,
    password: String,
    /// Which probe, for the cleanup's messages.
    what: &'static str,
    /// In the order it was made.
    made: Vec<Probed>,
}

impl Probes {
    /// None made yet.
    fn none(seeded: &Seeded, what: &'static str) -> Probes {
        Probes {
            url: seeded.url.clone(),
            user: seeded.user.clone(),
            password: seeded.password.clone(),
            what,
            made: Vec::new(),
        }
    }

    /// The refused-query probe. Takes back what a killed run left first, so
    /// the views a test counts hold no stale probe.
    async fn refused(seeded: &Seeded) -> Probes {
        let mine = |rows: serde_json::Value| -> Vec<String> {
            rows.as_array()
                .into_iter()
                .flatten()
                .filter(|row| {
                    row["name"]
                        .as_str()
                        .is_some_and(|name| name.starts_with(REFUSED_PROBE))
                })
                .map(|row| match &row["id"] {
                    serde_json::Value::String(id) => id.clone(),
                    other => other.to_string(),
                })
                .collect()
        };
        // A listing that failed would read as "nothing left behind".
        let listed = |(status, rows): (u16, serde_json::Value), what: &str| {
            assert_eq!(
                status, 200,
                "listing {what} for a killed run's probe: {rows}"
            );
            rows
        };
        let boards = listed(
            seeded
                .get(&format!(
                    "rest/agile/1.0/board?name={}",
                    url_encode(REFUSED_PROBE)
                ))
                .await,
            "the boards",
        );
        let favourites = listed(
            seeded.get("rest/api/2/filter/favourite").await,
            "the favourite filters",
        );
        let versions = listed(
            seeded.get("rest/api/2/project/PAY/versions").await,
            "PAY's versions",
        );
        let number = |id: String| id.parse::<u64>().expect("a numeric id");
        let stale: Vec<Probed> = mine(boards["values"].clone())
            .into_iter()
            .map(|id| Probed::Board(number(id)))
            .chain(
                mine(favourites)
                    .into_iter()
                    .map(|id| Probed::Filter(number(id))),
            )
            .chain(mine(versions).into_iter().map(Probed::Version))
            .collect();
        if let Err(failure) = take_back(
            &seeded.http,
            &seeded.url,
            &seeded.user,
            &seeded.password,
            &stale,
        )
        .await
        {
            panic!("a killed run's refused-query probe is still on the server: {failure}");
        }
        Probes::none(seeded, "refused-query probe")
    }

    /// Own `thing`'s removal from this line on.
    fn made(&mut self, thing: Probed) {
        self.made.push(thing);
    }

    /// Take `thing` back now, and stop owning it once it is gone.
    async fn take_back_now(&mut self, thing: &Probed) -> Result<(), String> {
        take_back(
            &client(),
            &self.url,
            &self.user,
            &self.password,
            std::slice::from_ref(thing),
        )
        .await?;
        self.made.retain(|made| made != thing);
        Ok(())
    }
}

impl Drop for Probes {
    fn drop(&mut self) {
        // The last made first: a board holds its filter, and the filter
        // names the version.
        let mut left: Vec<Probed> = std::mem::take(&mut self.made);
        if left.is_empty() {
            return;
        }
        left.reverse();
        let (url, user, password) = (self.url.clone(), self.user.clone(), self.password.clone());
        Cleanup::new(
            format!(
                "the live suite did not take back its {}, so the server is no longer in the \
                 plain-seed state",
                self.what
            ),
            "the probe may still be standing; the next run's sweep takes it back",
        )
        .run(move || async move { take_back(&client(), &url, &user, &password, &left).await });
    }
}

/// Take back each thing in order and **check** it is gone afterwards. A thing
/// already gone is not a failure.
async fn take_back(
    http: &reqwest::Client,
    url: &str,
    user: &str,
    password: &str,
    things: &[Probed],
) -> Result<(), String> {
    for thing in things {
        let path = thing.path();
        let response = thing
            .removal(http, url)
            .header("Accept", "application/json")
            .basic_auth(user, Some(password))
            .send()
            .await
            .map_err(|e| format!("taking back {path}: {e}"))?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) && !thing.gone(status) {
            return Err(format!(
                "taking back {path} -> {status}: {}",
                response.text().await.unwrap_or_default()
            ));
        }
        let status = http
            .get(format!("{url}/{path}"))
            .header("Accept", "application/json")
            .basic_auth(user, Some(password))
            .send()
            .await
            .map_err(|e| format!("GET {path}: {e}"))?
            .status()
            .as_u16();
        if !thing.gone(status) {
            return Err(format!("{path} answers {status} after it was taken back"));
        }
    }
    Ok(())
}

/// **A board and a favourite filter whose query Jira can no longer run
/// (#1893, #1698)** are left out, and the sync goes on. The test makes a
/// version on PAY, a favourite filter `project = PAY AND fixVersion =
/// "<it>"`, and a Kanban board on that filter: one sync with every board and
/// the favourite filters brings both in. Then it deletes the version. Jira
/// keeps the filter's JQL as saved, still lists the board and still answers
/// its configuration, and answers the query itself 400, which is the arm
/// `views::read` and `views::boards` leave a view out on. The next full sync
/// succeeds without the probe board and filter, and with the seed's boards
/// and favourite filter (the control: the views were read). [`Probes`]
/// takes the board, the filter and the version back, passing or failing.
#[tokio::test]
#[ignore = "needs testenv's seeded Jira: `just jira-live`"]
async fn a_board_and_a_favourite_filter_whose_version_was_deleted_are_left_out_and_the_sync_goes_on()
 {
    let seeded = seeded();
    let views = seeded.seed.views.as_ref().expect(
        "seed-state.json records no Jira views: re-run `./seed-atlassian-content.sh jira` (#877)",
    );
    let seeded_views = [
        format!("board-{}", seeded.seed.sprint().board_id),
        format!("board-{}", views.kanban.board_id),
        format!("filter-{}", views.favourite_filter.id),
    ];
    let mut probe = Probes::refused(&seeded).await;
    let name = format!("{REFUSED_PROBE} {}", uuid_like());

    let (status, version) = seeded
        .request(
            reqwest::Method::POST,
            "rest/api/2/version",
            Some(serde_json::json!({ "name": name, "project": "PAY" })),
        )
        .await;
    assert_eq!(status, 201, "{version}");
    let version_id = version["id"].as_str().expect("a version id").to_owned();
    probe.made(Probed::Version(version_id.clone()));
    let jql = format!("project = PAY AND fixVersion = \"{name}\" ORDER BY Rank ASC");
    let (status, filter) = seeded
        .request(
            reqwest::Method::POST,
            "rest/api/2/filter",
            Some(serde_json::json!({ "name": name, "jql": jql, "favourite": true })),
        )
        .await;
    assert_eq!(status, 200, "{filter}");
    let filter_id: u64 = filter["id"]
        .as_str()
        .and_then(|id| id.parse().ok())
        .expect("a filter id");
    probe.made(Probed::Filter(filter_id));
    let (status, board) = seeded
        .request(
            reqwest::Method::POST,
            "rest/agile/1.0/board",
            Some(serde_json::json!({ "name": name, "type": "kanban", "filterId": filter_id })),
        )
        .await;
    assert!(status == 200 || status == 201, "{status} {board}");
    let board_id = board["id"].as_u64().expect("a board id");
    probe.made(Probed::Board(board_id));
    let (board_view, filter_view) = (format!("board-{board_id}"), format!("filter-{filter_id}"));
    let source = seeded.source(serde_json::json!({
        "projects": ["PAY"], "boards": "all", "favourite_filters": true,
    }));
    let view_keys = |items: &[SyncItem]| -> Vec<String> {
        let mut views: Vec<String> = items
            .iter()
            .filter(|i| i.kind == "jira_view")
            .map(|i| i.entity.key.clone())
            .collect();
        views.sort();
        views
    };

    let (before, _) = full(&*source).await;
    let before = view_keys(&before);
    for key in [&board_view, &filter_view] {
        assert!(
            before.contains(key),
            "{key} did not come in while its version was there: {before:?}"
        );
    }

    let deleted = probe
        .take_back_now(&Probed::Version(version_id.clone()))
        .await;
    assert_eq!(deleted, Ok(()), "deleting the version");
    let (status, saved) = seeded.get(&format!("rest/api/2/filter/{filter_id}")).await;
    assert_eq!(status, 200, "{saved}");
    assert_eq!(
        saved["jql"].as_str(),
        Some(jql.as_str()),
        "Jira rewrote the filter when its version went"
    );
    let (status, favourites) = seeded.get("rest/api/2/filter/favourite").await;
    assert_eq!(status, 200, "{favourites}");
    assert!(
        favourites
            .as_array()
            .into_iter()
            .flatten()
            .any(|f| f["id"].as_str() == Some(filter_id.to_string().as_str())),
        "the filter left the favourites with its version, so the run never reaches its \
         query: {favourites}"
    );
    let (status, listed) = seeded
        .get(&format!("rest/agile/1.0/board?name={}", url_encode(&name)))
        .await;
    assert_eq!(status, 200, "{listed}");
    assert!(
        listed["values"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|b| b["id"].as_u64() == Some(board_id)),
        "the board left the listing with its version, so the run never reaches its query: {listed}"
    );
    let (status, config) = seeded
        .get(&format!("rest/agile/1.0/board/{board_id}/configuration"))
        .await;
    assert_eq!(status, 200, "{config}");
    let (status, refused) = seeded
        .get(&format!(
            "rest/api/2/search?jql={}&maxResults=0&fields=key",
            url_encode(&jql)
        ))
        .await;
    assert_eq!(
        status, 400,
        "Jira answers a filter whose version is gone with something other than 400: {refused}"
    );

    let (after, _) = full(&*source).await;
    let after = view_keys(&after);
    for key in [&board_view, &filter_view] {
        assert!(
            !after.contains(key),
            "{key} came in although Jira refuses its query: {after:?}"
        );
    }
    for key in &seeded_views {
        assert!(after.contains(key), "{key} did not come in: {after:?}");
    }
    println!(
        "SEEDED refused query: with version {version_id} deleted, Jira answered {jql:?} 400 \
         ({refused}); {board_view} and {filter_view} came in before and were left out after, \
         beside {seeded_views:?}"
    );
    drop(probe);
}

/// **Why a Jira view has no quick filters (#879, R-3.24, R-3.26)**: the
/// ticket asked to check `rest/agile/1.0/board/{id}/quickfilter` against
/// testenv's Data Center and to leave quick filters out if it is missing.
/// Jira Software DC 10.3.24 answers it 404 on both seeded boards, whose
/// configuration answers 200 (the control: the board is there, the endpoint
/// is not). If a Jira image ever answers it, this fails, and quick filters
/// can be built against what it answers.
#[tokio::test]
#[ignore = "needs testenv's seeded Jira: `just jira-live`"]
async fn the_seeded_data_center_has_no_quick_filter_endpoint() {
    let seeded = seeded();
    let kanban = &seeded
        .seed
        .views
        .as_ref()
        .expect("seed-state.json records no Jira views: re-run `./seed-atlassian-content.sh jira` (#877)")
        .kanban;
    for board in [seeded.seed.sprint().board_id, kanban.board_id] {
        let (config, _) = seeded
            .get(&format!("rest/agile/1.0/board/{board}/configuration"))
            .await;
        assert_eq!(config, 200, "board {board} is there");
        let (status, answer) = seeded
            .get(&format!("rest/agile/1.0/board/{board}/quickfilter"))
            .await;
        println!("SEEDED board {board}: GET .../quickfilter answered {status}: {answer}");
        assert_eq!(
            status, 404,
            "this Jira answers quick filters now (#879 left them out): {answer}"
        );
    }
}

/// A name suffix no other run shares: the clock in nanoseconds.
fn uuid_like() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
}

/// The role data check (#1199, ADR-0028) on the seeded corpus: the ticket
/// system the adapter claims is filled by the records a full sync emits.
/// Configured as the battery is, the way the Add-source dialog leaves a
/// source (the discovered Epic Link and Sprint fields).
#[tokio::test]
#[ignore = "needs testenv's seeded Jira: `just atlassian-live`"]
async fn fills_every_role_it_claims_on_the_seeded_corpus() {
    let seeded = seeded();
    let configured = serde_json::json!({
        "epic_link_field": seeded.seed.epic_link_field,
        "sprint_field": seeded.seed.sprint().field,
    });
    knobas_source::contract::fills_its_roles(&*seeded.source(configured)).await;
}

/// The walk reading against the real server (#1381): a full run over both
/// seeded projects reports 0 to N of N `issues`, N being the search's own
/// total, and an unchanged run after it still reports to the end.
#[tokio::test]
#[ignore = "needs testenv's seeded Jira: `just atlassian-live`"]
async fn a_run_reports_its_walk_over_the_real_search_total() {
    let seeded = seeded();
    seeded.clear_leftovers().await;
    let source = seeded.source(serde_json::json!({}));
    let mut first = Walking::default();
    let cursor = source.sync(None, &mut first).await.expect("full sync");
    let mut idle = Walking::default();
    source
        .sync(Some(cursor), &mut idle)
        .await
        .expect("idle sync");
    println!(
        "walk: full run {} items, {} readings, first {:?}, last {:?}; idle run {} items, last {:?}",
        first.items.len(),
        first.readings.len(),
        first.readings.first(),
        first.readings.last(),
        idle.items.len(),
        idle.readings.last()
    );
    let total = seeded.seeded_keys().len() as u64;
    let expected: Vec<_> = (0..=total)
        .map(|done| (done, Some(total), "issues".to_owned()))
        .collect();
    assert_eq!(first.readings, expected);
    let (done, of, unit) = idle.readings.last().cloned().expect("the idle run reports");
    assert_eq!((of, unit.as_str()), (Some(done), "issues"));
}

/// One raw `/search` page, as the adapter's walk asks for it: `(total, keys)`.
async fn search_page(seeded: &Seeded, jql: &str, start_at: u32, max: u32) -> (u64, Vec<String>) {
    let (status, body) = seeded
        .get(&format!(
            "rest/api/2/search?jql={}&startAt={start_at}&maxResults={max}&fields=key",
            url_encode(jql)
        ))
        .await;
    assert_eq!(status, 200, "JQL {jql:?} at {start_at} answered {body}");
    let keys = body["issues"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|i| i["key"].as_str().expect("an issue has a key").to_owned())
        .collect();
    (
        body["total"].as_u64().expect("a search page has a total"),
        keys,
    )
}

/// **The walk's order is one an edit cannot move** (#1623). The adapter pages
/// `/search` with `startAt` over `ORDER BY created ASC`, and its unit tests
/// stand a fake in for how Jira orders; this is where Jira says whether the
/// fake is right. One label edit sends the oldest-updated issue to the end of
/// `ORDER BY updated ASC` -- the shift that used to step over an unread issue
/// -- and leaves `ORDER BY created ASC` exactly as it was.
#[tokio::test]
#[ignore = "needs testenv's seeded Jira: `just jira-live`"]
async fn an_edit_moves_an_issue_in_the_updated_order_and_nowhere_in_the_created_one() {
    let seeded = seeded();
    let mut guard = Labeled::new(&seeded).await;
    let total = seeded.seeded_keys().len() as u32;
    let order = |rows: Vec<serde_json::Value>| -> Vec<String> {
        rows.iter()
            .map(|r| r["key"].as_str().expect("a key").to_owned())
            .collect()
    };

    let created = order(seeded.jql("ORDER BY created ASC", "key").await);
    let mut every = created.clone();
    every.sort();
    assert_eq!(
        every,
        seeded.seeded_keys(),
        "one page holds the whole corpus"
    );
    let by_updated = order(seeded.jql("ORDER BY updated ASC", "key").await);
    let edited = by_updated[0].clone();
    // Later than every stamp, so the edit lands at the end and nowhere else.
    after_the_second_of(stamp_of(&seeded, &by_updated[by_updated.len() - 1]).await).await;
    guard.label(&seeded, &edited).await;

    // The index is updated asynchronously: poll for the edit to show.
    let deadline = std::time::Instant::now() + INDEX_BUDGET;
    loop {
        let (_, last) = search_page(&seeded, "ORDER BY updated ASC", total - 1, 1).await;
        if last == std::slice::from_ref(&edited) {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the edit to {edited} did not reach the end of the updated order within \
             {INDEX_BUDGET:?}; the last row is {last:?}"
        );
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    println!("order: {edited} edited; created order {created:?}");
    assert_eq!(
        order(seeded.jql("ORDER BY created ASC", "key").await),
        created,
        "the edit to {edited} moved nothing in the created order"
    );
}

/// **A deletion under the walk shrinks the total the next page reports**
/// (#1623) and shifts the rows behind it: the hole the walk's anchored pages
/// close (#1979), which no longer read the total for it. A sub-task is filed,
/// and the page at offset N (the seeded corpus's size) names it alone, at the
/// end of the created order, with a total of N + 1. Once it is deleted the
/// same page answers `200` with no issues and a total of N, not a refusal:
/// past a deletion the walk meets an empty page where it expected rows, and
/// asks again from further back.
#[tokio::test]
#[ignore = "needs testenv's seeded Jira: `just jira-live`"]
async fn a_deletion_under_the_walk_shrinks_the_total_the_next_page_reports() {
    const WALK: &str = "ORDER BY created ASC";
    let seeded = seeded();
    seeded.clear_leftovers().await;
    let seeded_total = seeded.seeded_keys().len() as u64;
    let sub_task = SubTask::file(&seeded, "PAY-219").await;
    let key = sub_task.key().to_owned();

    let deadline = std::time::Instant::now() + INDEX_BUDGET;
    let (grown, last) = loop {
        let page = search_page(&seeded, WALK, seeded_total as u32, 1).await;
        if page.0 == seeded_total + 1 {
            break page;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "{key} was filed but the search total stayed {} within {INDEX_BUDGET:?}",
            page.0
        );
        tokio::time::sleep(Duration::from_millis(250)).await;
    };
    assert_eq!(
        last,
        std::slice::from_ref(&key),
        "a filed issue joins the end of the created order"
    );

    // The guard deletes it and waits for the index to stop naming it.
    drop(sub_task);
    let (shrunk, past_the_end) = search_page(&seeded, WALK, seeded_total as u32, 1).await;
    println!(
        "shrink: total {grown} with {key}, {shrunk} after its deletion; page past the end {past_the_end:?}"
    );
    assert_eq!(shrunk, seeded_total, "the deletion shrank the total");
    assert!(past_the_end.is_empty(), "{past_the_end:?}");
}

/// **A walk in pages of two returns the corpus a one-page walk does**
/// (#1979). Every page after the first is asked one row early, from the last
/// issue read, and one row longer (`crate::walk` in the adapter); this is the
/// real `/search` answering those `startAt`s, over both seeded projects.
#[tokio::test]
#[ignore = "needs testenv's seeded Jira: `just jira-live`"]
async fn a_walk_in_pages_of_two_returns_the_whole_corpus_once() {
    let seeded = seeded();
    seeded.clear_leftovers().await;
    let tickets = |items: &[SyncItem]| -> Vec<String> {
        let mut keys: Vec<String> = items
            .iter()
            .filter(|i| i.kind == knobas_source_jira::KIND_TICKET)
            .map(|i| i.entity.key.clone())
            .collect();
        keys.sort();
        keys
    };
    let (paged, _) = full(&*seeded.source(serde_json::json!({ "page_size": 2 }))).await;
    let (whole, _) = full(&*seeded.source(serde_json::json!({}))).await;
    println!("paged walk: {:?}", tickets(&paged));
    assert!(
        seeded.seeded_keys().len() > 2,
        "the corpus spans more than one page of two"
    );
    assert_eq!(tickets(&paged), seeded.seeded_keys(), "each issue once");
    assert_eq!(tickets(&paged), tickets(&whole));
}

/// **JQL's `id > N` is no keyset** (#1979), which is why the walk anchors its
/// offset pages instead (`crate::walk`'s module docs). `id` is an alias of
/// `issuekey`, so the range stays inside issue N's own project: asked from
/// the seeded OPS issue, it answers no PAY issue, though every one was filed
/// after it and carries a higher id. And an `N` that names no issue is
/// refused, so a keyset on a deleted issue could not ask at all.
#[tokio::test]
#[ignore = "needs testenv's seeded Jira: `just jira-live`"]
async fn a_jql_id_range_stays_inside_one_project() {
    let seeded = seeded();
    seeded.clear_leftovers().await;
    let rows = seeded.jql("ORDER BY id ASC", "key").await;
    let id_of = |row: &serde_json::Value| -> u64 {
        row["id"]
            .as_str()
            .and_then(|id| id.parse().ok())
            .expect("an issue has a numeric id")
    };
    let ops = rows
        .iter()
        .find(|r| r["key"].as_str().is_some_and(|k| k.starts_with("OPS-")))
        .expect("the seed files an OPS issue");
    let later_pay: Vec<&str> = rows
        .iter()
        .filter(|r| id_of(r) > id_of(ops))
        .filter_map(|r| r["key"].as_str())
        .filter(|k| k.starts_with("PAY-"))
        .collect();
    assert!(
        !later_pay.is_empty(),
        "the seed files PAY issues after the OPS one: {rows:?}"
    );
    let ranged = seeded
        .jql(&format!("id > {} ORDER BY id ASC", id_of(ops)), "key")
        .await;
    let ranged: Vec<&str> = ranged.iter().filter_map(|r| r["key"].as_str()).collect();
    println!(
        "id > {}: {ranged:?}; PAY ids above it: {later_pay:?}",
        ops["key"]
    );
    assert!(
        ranged.iter().all(|k| k.starts_with("OPS-")),
        "the range left OPS: {ranged:?}"
    );
    let (status, body) = seeded
        .get(&format!(
            "rest/api/2/search?jql={}&maxResults=1&fields=key",
            url_encode("id > 1 ORDER BY id ASC")
        ))
        .await;
    println!("id > 1: {status} {body}");
    assert_eq!(status, 400, "an id that names no issue is refused: {body}");
}

/// The seeded Jira's database container (`testenv/docker-compose.yml`'s
/// `jira-db`, whose user and database are both `jira`), which [`Tied::file`]
/// writes one `created` stamp into: Jira's REST API sets `created` to the
/// moment of the request and lets nothing change it.
const JIRA_DB_CONTAINER: &str = "knobas-jira-db";

/// Issues filed in one batch and given **one `created` stamp**, the state a
/// CSV or tracker import at minute resolution leaves (#1720), and owned until
/// they are deleted again.
///
/// Filed in turn, each issue gets its own millisecond: `/search` renders
/// `created` to the second, so bulk-filed issues *look* tied, but the column
/// and the index hold milliseconds and sort them apart (measured 2026-10-22).
/// So the stamp is written into `jiraissue.created` through `psql` in the
/// database container, truncated to the minute, and each issue is reindexed
/// through `POST /rest/api/2/reindex/issue`, which is synchronous. Marked with
/// [`LITTER_LABEL`], so a killed run's issues are ones the seed did not create,
/// which [`Seeded::clear_leftovers`] deletes.
struct Tied {
    url: String,
    user: String,
    password: String,
    /// `(id, key)` in the order Jira filed them, which is ascending id.
    filed: Vec<(u64, String)>,
}

impl Tied {
    async fn file(seeded: &Seeded, count: usize) -> Tied {
        let updates: Vec<_> = (0..count)
            .map(|n| {
                serde_json::json!({ "fields": {
                    "project": { "key": "PAY" },
                    "issuetype": { "name": "Task" },
                    "summary": format!("knobas live suite: tied on created, {n}"),
                    "labels": [LITTER_LABEL],
                }})
            })
            .collect();
        let (status, body) = seeded
            .request(
                reqwest::Method::POST,
                "rest/api/2/issue/bulk",
                Some(serde_json::json!({ "issueUpdates": updates })),
            )
            .await;
        let mut guard = Tied {
            url: seeded.url.clone(),
            user: seeded.user.clone(),
            password: seeded.password.clone(),
            filed: body["issues"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|i| {
                    let id = i["id"].as_str().and_then(|id| id.parse().ok());
                    let key = i["key"].as_str().map(str::to_owned);
                    (
                        id.expect("a filed issue has an id"),
                        key.expect("and a key"),
                    )
                })
                .collect(),
        };
        assert_eq!(status, 201, "bulk-filing {count} issues: {body}");
        assert_eq!(guard.filed.len(), count, "{body}");
        guard.filed.sort();

        let stamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:00+00");
        let ids: Vec<String> = guard.filed.iter().map(|(id, _)| id.to_string()).collect();
        let sql = format!(
            "UPDATE jiraissue SET created = '{stamp}' WHERE id IN ({})",
            ids.join(", ")
        );
        let out = std::process::Command::new("docker")
            .args([
                "exec",
                JIRA_DB_CONTAINER,
                "psql",
                "-U",
                "jira",
                "-d",
                "jira",
            ])
            .args(["-v", "ON_ERROR_STOP=1", "-At", "-c", &sql])
            .output()
            .expect("docker runs");
        let said = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        assert!(
            out.status.success() && said == format!("UPDATE {count}"),
            "{sql}: {said} {}",
            String::from_utf8_lossy(&out.stderr)
        );
        guard
    }

    /// Reindex the issues one at a time in `order`: a group tied on `created`
    /// comes back in the order its issues reached the index.
    async fn reindex(&self, seeded: &Seeded, order: &[&(u64, String)]) {
        for (id, key) in order {
            let (status, body) = seeded
                .request(
                    reqwest::Method::POST,
                    &format!("rest/api/2/reindex/issue?issueId={id}"),
                    None,
                )
                .await;
            assert!(
                status == 200 && body["success"] == true,
                "reindexing {key}: {status} {body}"
            );
        }
    }
}

impl Drop for Tied {
    fn drop(&mut self) {
        let filed = std::mem::take(&mut self.filed);
        if filed.is_empty() {
            return;
        }
        let (url, user, password) = (self.url.clone(), self.user.clone(), self.password.clone());
        // Each delete may wait out INDEX_BUDGET for the index, one after
        // another, so the default bound alone could abandon a healthy run.
        let budget = DEFAULT_BUDGET.max(INDEX_BUDGET * 2 * filed.len() as u32);
        Cleanup::new(
            "the live suite did not delete the tied issues it filed",
            "some may still exist; the next run's leftover clearing deletes what is left",
        )
        .within(budget)
        .run(move || async move {
            let (http, named_by) = (client(), format!("labels = {LITTER_LABEL}"));
            let mut failures = Vec::new();
            for (_, key) in &filed {
                if let Err(e) = delete_issue(&http, &url, &user, &password, key, &named_by).await {
                    failures.push(e);
                }
            }
            if failures.is_empty() {
                Ok(())
            } else {
                Err(failures.join("; "))
            }
        });
    }
}

/// **An edit reorders issues tied on `created`, and `id` holds them still**
/// (#1720). `created` alone leaves a tie group to Jira's own tiebreak, and
/// this is where Jira says what that is, for `sync::tests`' `LiveIndex` to
/// model: the order the issues reached the index, with an edited issue
/// reindexed to the end of its group. Across a page boundary that is the
/// shift that steps the walk over an unread issue, so the walk sorts by
/// `created ASC, id ASC`, and that order is the same before and after.
#[tokio::test]
#[ignore = "needs testenv's seeded Jira: `just jira-live`"]
async fn an_edit_reorders_issues_tied_on_created_and_id_holds_them_still() {
    let scope = format!("labels = {LITTER_LABEL}");
    let seeded = seeded();
    seeded.clear_leftovers().await;
    let tied = Tied::file(&seeded, 4).await;
    let by_id: Vec<String> = tied.filed.iter().map(|(_, key)| key.clone()).collect();
    // Reindexed newest first, so the index order is the reverse of the ids'.
    let newest_first: Vec<&(u64, String)> = tied.filed.iter().rev().collect();
    tied.reindex(&seeded, &newest_first).await;

    let rows = seeded
        .jql(&format!("{scope} ORDER BY created ASC"), "created")
        .await;
    let stamps: std::collections::BTreeSet<_> = rows
        .iter()
        .map(|r| {
            r["fields"]["created"]
                .as_str()
                .expect("a created stamp")
                .to_owned()
        })
        .collect();
    assert_eq!(stamps.len(), 1, "one created stamp for the group: {rows:?}");
    let keys = |rows: Vec<serde_json::Value>| -> Vec<String> {
        rows.iter()
            .map(|r| r["key"].as_str().expect("a key").to_owned())
            .collect()
    };
    let in_index_order: Vec<String> = by_id.iter().rev().cloned().collect();
    assert_eq!(
        keys(rows),
        in_index_order,
        "a tie on created comes back in the order the index took the issues"
    );
    let walk = format!("{scope} ORDER BY created ASC, id ASC");
    assert_eq!(keys(seeded.jql(&walk, "key").await), by_id);

    let edited = in_index_order[0].clone();
    let (status, body) = seeded
        .request(
            reqwest::Method::PUT,
            &format!("rest/api/2/issue/{edited}"),
            Some(serde_json::json!({
                "fields": { "summary": "knobas live suite: tied on created, edited" }
            })),
        )
        .await;
    assert_eq!(status, 204, "editing {edited}: {body}");
    // The index is updated asynchronously: poll for the edit to show.
    let deadline = std::time::Instant::now() + INDEX_BUDGET;
    loop {
        let rows = seeded
            .jql(&format!("{scope} AND summary ~ \"edited\""), "key")
            .await;
        if keys(rows) == std::slice::from_ref(&edited) {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the edit to {edited} did not reach the index within {INDEX_BUDGET:?}"
        );
        tokio::time::sleep(Duration::from_millis(250)).await;
    }

    let rows = seeded
        .jql(&format!("{scope} ORDER BY created ASC"), "created")
        .await;
    // Still one stamp: the edit moved the issue in the index, and did not
    // write back a `created` of its own, which would sort it last just the
    // same and be no tie at all.
    let stamps_after: std::collections::BTreeSet<_> = rows
        .iter()
        .map(|r| {
            r["fields"]["created"]
                .as_str()
                .expect("a created stamp")
                .to_owned()
        })
        .collect();
    assert_eq!(stamps_after, stamps, "the edit left the one created stamp");
    let after = keys(rows);
    println!("tie: {edited} edited; created ASC {in_index_order:?} -> {after:?}");
    let mut moved = in_index_order[1..].to_vec();
    moved.push(edited.clone());
    assert_eq!(
        after, moved,
        "the edit reindexed {edited} to the end of its tie group"
    );
    assert_eq!(
        keys(seeded.jql(&walk, "key").await),
        by_id,
        "created ASC, id ASC is where it was"
    );
}

/// The names this suite's board-order probe gives its boards (#2003). Not a
/// substring of any other probe's name: the board listing's `name=` matches
/// on a part of the name.
const ORDER_PROBE: &str = "knobas live board-order probe";

impl Probes {
    /// The board-order probe. Takes back the boards a killed run left first,
    /// each with the filter it stands on.
    async fn board_order(seeded: &Seeded) -> Probes {
        let (status, listed) = seeded
            .get(&format!(
                "rest/agile/1.0/board?name={}",
                url_encode(ORDER_PROBE)
            ))
            .await;
        assert_eq!(status, 200, "listing a killed run's probe boards: {listed}");
        let mut stale = Vec::new();
        for board in listed["values"].as_array().into_iter().flatten() {
            let id = board["id"].as_u64().expect("a board id");
            let (status, config) = seeded
                .get(&format!("rest/agile/1.0/board/{id}/configuration"))
                .await;
            // A failed read would leave the filter behind without a word.
            assert_eq!(status, 200, "a killed run's probe board {id}: {config}");
            stale.push(Probed::Board(id));
            if let Some(filter) = config["filter"]["id"].as_str() {
                stale.push(Probed::Filter(filter.parse().expect("a filter id")));
            }
        }
        if let Err(failure) = take_back(
            &seeded.http,
            &seeded.url,
            &seeded.user,
            &seeded.password,
            &stale,
        )
        .await
        {
            panic!("a killed run's board-order probe is still on the server: {failure}");
        }
        Probes::none(seeded, "board-order probe")
    }

    /// A Kanban board on PAY named `<ORDER_PROBE> <suffix>`, on a filter of
    /// its own: its id.
    async fn order_board(&mut self, seeded: &Seeded, suffix: &str) -> u64 {
        let (status, filter) = seeded
            .request(
                reqwest::Method::POST,
                "rest/api/2/filter",
                Some(serde_json::json!({
                    "name": format!("{ORDER_PROBE} filter {suffix} {}", uuid_like()),
                    "jql": "project = PAY ORDER BY Rank ASC",
                    "favourite": false,
                })),
            )
            .await;
        assert_eq!(status, 200, "{filter}");
        let filter_id: u64 = filter["id"].as_str().unwrap().parse().unwrap();
        self.made.push(Probed::Filter(filter_id));
        let (status, board) = seeded
            .request(
                reqwest::Method::POST,
                "rest/agile/1.0/board",
                Some(serde_json::json!({
                    "name": format!("{ORDER_PROBE} {suffix}"),
                    "type": "kanban",
                    "filterId": filter_id,
                })),
            )
            .await;
        assert!(status == 200 || status == 201, "{status} {board}");
        let id = board["id"].as_u64().expect("a board id");
        self.made.push(Probed::Board(id));
        id
    }
}

/// PAY's board listing as `(id, name)`, in Jira's order, from one request.
async fn pay_boards(seeded: &Seeded, extra: &str) -> Vec<(u64, String)> {
    let (status, listed) = seeded
        .get(&format!(
            "rest/agile/1.0/board?projectKeyOrId=PAY&maxResults=50{extra}"
        ))
        .await;
    assert_eq!(status, 200, "{listed}");
    assert_eq!(
        listed["isLast"], true,
        "one page holds PAY's boards: {listed}"
    );
    listed["values"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| {
            (
                b["id"].as_u64().unwrap(),
                b["name"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

/// **The order the agile board listing pages over (#2003)**, which
/// `views::listing` reads twice because of it. Three boards made on PAY in
/// the order `m`, `A`, `z` (so their ids rise that way) are listed `A`, `m`,
/// `z`: by name, ignoring case, not by id, and `orderBy=id` changes nothing.
/// A rename moves a row: `A` renamed `zz` is listed last. So no order an
/// edit cannot move is on offer for the issue walk's anchoring (`walk.rs`).
/// The source's views read, which lists twice for that reason, brings every
/// probe board in under its name as it stands.
#[tokio::test]
#[ignore = "needs testenv's seeded Jira: `just jira-live`"]
async fn the_board_listing_is_in_name_order_and_a_rename_moves_a_row() {
    let seeded = seeded();
    seeded.clear_leftovers().await;
    let mut probe = Probes::board_order(&seeded).await;
    let m = probe.order_board(&seeded, "m").await;
    let upper_a = probe.order_board(&seeded, "A").await;
    let z = probe.order_board(&seeded, "z").await;
    assert!(m < upper_a && upper_a < z, "ids rise in the order made");
    let probes = |listed: &[(u64, String)]| -> Vec<u64> {
        listed
            .iter()
            .filter(|(_, name)| name.starts_with(ORDER_PROBE))
            .map(|(id, _)| *id)
            .collect()
    };

    let listed = pay_boards(&seeded, "").await;
    assert_eq!(
        probes(&listed),
        [upper_a, m, z],
        "by name, ignoring case: {listed:?}"
    );
    let mut by_name = listed.clone();
    by_name.sort_by_key(|(_, name)| name.to_lowercase());
    assert_eq!(listed, by_name, "the whole listing is in name order");
    assert_eq!(
        pay_boards(&seeded, "&orderBy=id").await,
        listed,
        "orderBy is not read"
    );

    let renamed = format!("{ORDER_PROBE} zz");
    let (status, body) = seeded
        .request(
            reqwest::Method::PUT,
            "rest/greenhopper/1.0/rapidviewconfig/name",
            Some(serde_json::json!({ "id": upper_a, "name": renamed })),
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let after = pay_boards(&seeded, "").await;
    assert_eq!(
        probes(&after),
        [m, z, upper_a],
        "the rename moved a row: {after:?}"
    );

    let source = seeded.source(serde_json::json!({ "projects": ["PAY"] }));
    let (items, _) = full(&*source).await;
    for (id, name) in [
        (m, format!("{ORDER_PROBE} m")),
        (z, format!("{ORDER_PROBE} z")),
        (upper_a, renamed),
    ] {
        let view = items
            .iter()
            .find(|i| i.kind == "jira_view" && i.entity.key == format!("board-{id}"))
            .unwrap_or_else(|| panic!("no view of board {id} among {:?}", keys(&items)));
        assert_eq!(view.title, name);
    }
    println!(
        "SEEDED board listing: {} boards on PAY by name ignoring case; probes {m} m, {upper_a} A, {z} z \
         listed A, m, z, and A renamed zz listed last",
        listed.len()
    );
}

// ---- what the credential may not write (#2052) ----

/// The user [`a_user_the_permission_scheme_refuses_is_refused_those_writes_and_nothing_is_written`] mints.
const MINTED: &str = "knobas-live-2052";
const MINTED_PASSWORD: &str = "knobas-live-2052-Pw";
/// The copy of OPS's permission scheme it puts OPS on.
const NARROWED_SCHEME: &str = "knobas live suite: narrowed (#2052)";
/// The permissions the copy gives `jira-administrators` alone.
const NARROWED: [&str; 3] = ["ADD_COMMENTS", "WORK_ON_ISSUES", "CREATE_ISSUES"];

/// One admin request for [`Narrowed`]'s cleanup, which runs outside the test's
/// runtime: the status and the body, or what failed.
async fn admin(
    http: &reqwest::Client,
    (url, user, password): (&str, &str, &str),
    method: reqwest::Method,
    path: &str,
    body: Option<serde_json::Value>,
) -> Result<(u16, serde_json::Value), String> {
    let mut request = http
        .request(method.clone(), format!("{url}/{path}"))
        .header("Accept", "application/json")
        .basic_auth(user, Some(password));
    if let Some(body) = body {
        request = request.json(&body);
    }
    let response = request
        .send()
        .await
        .map_err(|e| format!("{method} {path}: {e}"))?;
    let status = response.status().as_u16();
    let text = response.text().await.unwrap_or_default();
    Ok((
        status,
        serde_json::from_str(&text).unwrap_or(serde_json::Value::String(text)),
    ))
}

/// OPS on its own scheme again, the copy and the minted user gone, each
/// checked. `scheme` is OPS's own scheme; `None` reads it off PAY, which
/// the seed made from the same template.
async fn unnarrow(
    http: &reqwest::Client,
    login: (&str, &str, &str),
    scheme: Option<u64>,
) -> Result<(), String> {
    let id_of = |body: &serde_json::Value| body["id"].as_u64();
    let scheme = match scheme {
        Some(id) => id,
        None => {
            let (_, pay) = admin(
                http,
                login,
                reqwest::Method::GET,
                "rest/api/2/project/PAY/permissionscheme",
                None,
            )
            .await?;
            id_of(&pay).ok_or_else(|| format!("PAY names no permission scheme: {pay}"))?
        }
    };
    let (_, on) = admin(
        http,
        login,
        reqwest::Method::GET,
        "rest/api/2/project/OPS/permissionscheme",
        None,
    )
    .await?;
    if id_of(&on) != Some(scheme) {
        let (status, body) = admin(
            http,
            login,
            reqwest::Method::PUT,
            "rest/api/2/project/OPS/permissionscheme",
            Some(serde_json::json!({ "id": scheme })),
        )
        .await?;
        if status != 200 {
            return Err(format!(
                "putting OPS back on scheme {scheme} -> {status}: {body}"
            ));
        }
    }
    let (_, schemes) = admin(
        http,
        login,
        reqwest::Method::GET,
        "rest/api/2/permissionscheme",
        None,
    )
    .await?;
    for copy in schemes["permissionSchemes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|each| each["name"] == NARROWED_SCHEME)
    {
        let id = id_of(copy).ok_or_else(|| format!("a scheme without an id: {copy}"))?;
        let (status, body) = admin(
            http,
            login,
            reqwest::Method::DELETE,
            &format!("rest/api/2/permissionscheme/{id}"),
            None,
        )
        .await?;
        if status != 204 {
            return Err(format!("deleting scheme {id} -> {status}: {body}"));
        }
    }
    let user = format!("rest/api/2/user?username={MINTED}");
    if admin(http, login, reqwest::Method::GET, &user, None)
        .await?
        .0
        == 200
    {
        let (status, body) = admin(http, login, reqwest::Method::DELETE, &user, None).await?;
        if status != 204 {
            return Err(format!("deleting {MINTED} -> {status}: {body}"));
        }
    }
    // Checked, not assumed.
    let (_, on) = admin(
        http,
        login,
        reqwest::Method::GET,
        "rest/api/2/project/OPS/permissionscheme",
        None,
    )
    .await?;
    if id_of(&on) != Some(scheme) {
        return Err(format!("OPS is on {on}, not on scheme {scheme}"));
    }
    let (_, schemes) = admin(
        http,
        login,
        reqwest::Method::GET,
        "rest/api/2/permissionscheme",
        None,
    )
    .await?;
    if schemes.to_string().contains(NARROWED_SCHEME) {
        return Err(format!("{NARROWED_SCHEME:?} is still there"));
    }
    let (status, _) = admin(http, login, reqwest::Method::GET, &user, None).await?;
    if status != 404 {
        return Err(format!("{MINTED} answers {status}, not 404"));
    }
    Ok(())
}

/// OPS on a copy of its scheme that refuses [`NARROWED`] to everyone but
/// `jira-administrators`, and [`MINTED`] a user; both taken back by `Drop`.
struct Narrowed {
    url: String,
    user: String,
    password: String,
    scheme: u64,
}

impl Narrowed {
    async fn new(seeded: &Seeded) -> Narrowed {
        let login = (
            seeded.url.as_str(),
            seeded.user.as_str(),
            seeded.password.as_str(),
        );
        // A killed run's leftovers first.
        unnarrow(&client(), login, None)
            .await
            .unwrap_or_else(|e| panic!("putting back a killed run's narrowing: {e}"));
        let (status, on) = seeded.get("rest/api/2/project/OPS/permissionscheme").await;
        assert_eq!(status, 200, "{on}");
        let scheme = on["id"]
            .as_u64()
            .unwrap_or_else(|| panic!("OPS's scheme: {on}"));
        let guard = Narrowed {
            url: seeded.url.clone(),
            user: seeded.user.clone(),
            password: seeded.password.clone(),
            scheme,
        };
        let (status, own) = seeded
            .get(&format!(
                "rest/api/2/permissionscheme/{scheme}?expand=permissions"
            ))
            .await;
        assert_eq!(status, 200, "{own}");
        let admins = serde_json::json!({ "type": "group", "parameter": "jira-administrators" });
        let mut grants: Vec<serde_json::Value> = Vec::new();
        for grant in own["permissions"].as_array().expect("a scheme's grants") {
            let permission = grant["permission"].as_str().expect("a grant's permission");
            let holder = if NARROWED.contains(&permission) {
                admins.clone()
            } else {
                let mut holder = serde_json::json!({ "type": grant["holder"]["type"] });
                if let Some(parameter) = grant["holder"]["parameter"].as_str() {
                    holder["parameter"] = parameter.into();
                }
                holder
            };
            let entry = serde_json::json!({ "holder": holder, "permission": permission });
            if !grants.contains(&entry) {
                grants.push(entry);
            }
        }
        let (status, copy) = seeded
            .request(
                reqwest::Method::POST,
                "rest/api/2/permissionscheme",
                Some(serde_json::json!({
                    "name": NARROWED_SCHEME,
                    "description": "knobas live suite (#2052): OPS without comments, worklogs and creates",
                    "permissions": grants,
                })),
            )
            .await;
        assert_eq!(status, 201, "creating {NARROWED_SCHEME:?}: {copy}");
        let copy = copy["id"]
            .as_u64()
            .unwrap_or_else(|| panic!("a created scheme's id: {copy}"));
        let (status, body) = seeded
            .request(
                reqwest::Method::PUT,
                "rest/api/2/project/OPS/permissionscheme",
                Some(serde_json::json!({ "id": copy })),
            )
            .await;
        assert_eq!(status, 200, "putting OPS on {copy}: {body}");
        let (status, body) = seeded
            .request(
                reqwest::Method::POST,
                "rest/api/2/user",
                Some(serde_json::json!({
                    "name": MINTED,
                    "password": MINTED_PASSWORD,
                    "emailAddress": format!("{MINTED}@tidewater.test"),
                    "displayName": "knobas live suite (#2052)",
                    "applicationKeys": ["jira-software"],
                })),
            )
            .await;
        assert_eq!(status, 201, "minting {MINTED}: {body}");
        guard
    }

    /// The adapter as [`MINTED`], with its password.
    fn source(&self) -> Box<dyn Source> {
        match knobas_source_jira::build(SourceInstance {
            id: "jira".to_owned(),
            kind: knobas_source_jira::ADAPTER_KIND.to_owned(),
            display_name: "Tidewater Jira (minted)".to_owned(),
            base_url: self.url.clone(),
            auth: Some(AuthMethod::UserPassword),
            secret: Some(MINTED_PASSWORD.to_owned()),
            account: None,
            config: serde_json::json!({ "username": MINTED, "favourite_filters": false }),
        }) {
            Ok(s) => s,
            Err(e) => panic!("the adapter must build for {MINTED}: {e:?}"),
        }
    }
}

impl Drop for Narrowed {
    fn drop(&mut self) {
        let (url, user, password, scheme) = (
            self.url.clone(),
            self.user.clone(),
            self.password.clone(),
            self.scheme,
        );
        Cleanup::new(
            "the live suite did not take back the user and permission scheme it made, so the \
             server is no longer in the plain-seed state",
            format!(
                "OPS may still be on {NARROWED_SCHEME:?} and {MINTED} may still exist; the next \
                 run of this test puts both back"
            ),
        )
        .run(move || async move {
            unnarrow(&client(), (&url, &user, &password), Some(scheme)).await
        });
    }
}

/// What a write would leave on `ticket` and in OPS: its `updated`, its
/// status, comments and worklogs, and the project's issue count.
async fn narrow_marks(seeded: &Seeded, ticket: &str) -> serde_json::Value {
    let (status, issue) = seeded
        .get(&format!(
            "rest/api/2/issue/{ticket}?fields=updated,status,comment,worklog"
        ))
        .await;
    assert_eq!(status, 200, "{issue}");
    let issues = seeded.jql("project = OPS", "key").await.len();
    serde_json::json!({
        "updated": issue["fields"]["updated"],
        "status": issue["fields"]["status"]["name"],
        "comments": issue["fields"]["comment"]["total"],
        "worklogs": issue["fields"]["worklog"]["total"],
        "issues": issues,
    })
}

/// **What the credential may not write, read and never probed** (#2052,
/// ADR-0013). A user minted with Jira Software access, on OPS narrowed to
/// refuse comments, worklogs and creates to all but administrators, is
/// refused exactly those three there with the reasons, and refused nothing
/// on PAY or on OPS's moves. The seed's administrator, asked the same, is
/// refused nothing. OPS's seeded ticket and OPS are as they were afterwards: nothing
/// was written to find out.
#[tokio::test]
#[ignore = "needs testenv's seeded Jira: `just atlassian-live`"]
async fn a_user_the_permission_scheme_refuses_is_refused_those_writes_and_nothing_is_written() {
    use knobas_source::grants::{RefusedWrite, WriteAsk};
    let seeded = seeded();
    let narrowed = Narrowed::new(&seeded).await;
    let ticket = seeded
        .seed
        .issues
        .iter()
        .map(|issue| issue.key.clone())
        .find(|key| key.starts_with("OPS-"))
        .expect("the seed files a ticket in OPS");
    let before = narrow_marks(&seeded, &ticket).await;
    let on = format!("jira:{ticket}");
    let ask = |entity: &str, op: &str| WriteAsk {
        entity: entity.to_owned(),
        op: op.to_owned(),
        field: None,
    };
    let asks = [
        ask(&on, "comment"),
        ask(&on, "transition"),
        ask(&on, "log_work"),
        ask("jira:OPS", "create_ticket"),
        ask("jira:PAY-231", "comment"),
        ask("jira:PAY-231", "log_work"),
        ask("jira:PAY", "create_ticket"),
    ];
    let refused = |entity: &str, op: &str, reason: &str| RefusedWrite {
        entity: entity.to_owned(),
        op: op.to_owned(),
        field: None,
        reason: reason.to_owned(),
    };

    let minted = narrowed
        .source()
        .refused_writes(&asks)
        .await
        .expect("the minted user's permissions are an answer");
    assert_eq!(
        minted,
        [
            refused(
                &on,
                "comment",
                "This Jira user cannot comment in OPS: it lacks the Add Comments permission"
            ),
            refused(
                &on,
                "log_work",
                "This Jira user cannot log work in OPS: it lacks the Work On Issues permission"
            ),
            refused(
                "jira:OPS",
                "create_ticket",
                "This Jira user cannot create tickets in OPS: it lacks the Create Issues permission"
            ),
        ]
    );

    let administrator = seeded
        .source(serde_json::json!({}))
        .refused_writes(&asks)
        .await
        .expect("the administrator's permissions are an answer");
    assert_eq!(
        administrator,
        [],
        "the seed's administrator may write all of it"
    );

    assert_eq!(
        narrow_marks(&seeded, &ticket).await,
        before,
        "asking wrote nothing to {ticket} or OPS"
    );
    println!("SEEDED mypermissions: {MINTED} refused {minted:?}; the administrator nothing");
    drop(narrowed);
}

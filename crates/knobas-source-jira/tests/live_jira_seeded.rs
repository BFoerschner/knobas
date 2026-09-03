//! The adapter against the **seeded, real Jira Data Center** that
//! `testenv/seed-atlassian-content.sh` fills with the Tidewater dataset
//! (issue #276, ADR-0013).
//!
//! Until this file existed the Jira adapter had only ever been run against
//! `knobas-mockd` -- a mock built from the vendored WADL and from assumptions
//! about the product, certifying the adapter against a reading of the contract
//! rather than against Jira. `tests/mockd.rs` still runs in `just check` and is
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
//! ends -- passing or panicking alike, from [`Labeled`]'s `Drop`, on a thread
//! with a runtime of its own, and checked afterwards rather than assumed. A
//! label is the smallest reversible edit Jira has: it moves `updated`, which is
//! the whole point, and it changes no field the fixture describes.
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

use knobas_source::contract::{Fault, VecSink, battery};
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
        match knobas_source_jira::build(SourceInstance {
            id: "jira".to_owned(),
            kind: knobas_source_jira::ADAPTER_KIND.to_owned(),
            display_name: "Tidewater Jira (seeded)".to_owned(),
            base_url: base_url.to_owned(),
            auth: Some(auth),
            secret: Some(secret.to_owned()),
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
        let edited = key.clone();
        // `Drop` cannot await and runs on a tokio worker thread, so the cleanup
        // gets a thread with a runtime of its own -- and a client built inside
        // it, because a `reqwest::Client` driven from a second runtime hangs
        // rather than failing (the Gitea suite measured that).
        let report = std::thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("a runtime for the cleanup")
                .block_on(async move { unlabel(&client(), &url, &user, &password, &edited).await })
        })
        .join();
        let failure = match report {
            Ok(Ok(())) => return,
            Ok(Err(e)) => e,
            Err(_) => format!(
                "the cleanup thread panicked (its own message is on stderr), so {key} may still \
                 carry {LITTER_LABEL}; the next run's leftover clearing removes it"
            ),
        };
        let report = format!(
            "the live suite did not undo the edit it made, so the server is no longer in the \
             plain-seed state: {failure}"
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
/// Jira stamps `updated` to the second, and the adapter's cursor recognises a
/// re-delivered issue by its `(key, updated)` pair -- so an edit made inside
/// the same second as the value a baseline run recorded is, to the cursor,
/// *the same version of that issue*, and the next run correctly skips it as
/// already delivered. `JiraCursor::already_delivered`'s own doc says as much
/// ("an issue edited twice within the same second is missed here"), and it is
/// a documented limitation of a minute-resolution query language, not a
/// defect.
///
/// It is also not what the incremental test is about, and it made that test
/// fail one run in eight (measured 2026-09-03) -- always the runs where the
/// leftover clearing had just touched the issue, so its stamp *was* the
/// current second. Waiting for the second to turn is what makes the edit one
/// the cursor can tell apart, and it costs at most a second.
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
/// two-spellings convenience is what hid it.
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
    battery(move |fault| match fault {
        Fault::None => seeded.source(serde_json::json!({})),
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
        assert!(self.was.is_none(), "this guard owns exactly one issue");
        let (status, body) = seeded
            .get(&format!("rest/api/2/issue/{key}?fields=assignee"))
            .await;
        assert_eq!(status, 200, "reading {key}'s assignee: {body}");
        let before = body["fields"]["assignee"]["name"]
            .as_str()
            .map(str::to_owned);
        assert_ne!(
            before.as_deref(),
            Some(seeded.user.as_str()),
            "{key} is already the suite's, so becoming its assignee would witness nothing"
        );
        self.was = Some((key.to_owned(), before));
        let (status, body) = seeded
            .request(
                reqwest::Method::PUT,
                &format!("rest/api/2/issue/{key}/assignee"),
                Some(serde_json::json!({ "name": seeded.user })),
            )
            .await;
        assert_eq!(status, 204, "assigning {key} to {}: {body}", seeded.user);
    }
}

impl Drop for Reassigned {
    fn drop(&mut self) {
        let Some((key, was)) = self.was.take() else {
            return;
        };
        let (url, user, password) = (self.url.clone(), self.user.clone(), self.password.clone());
        let restore = key.clone();
        // The same shape as `Labeled`'s, and for the same measured reason: a
        // `reqwest::Client` driven from a second runtime hangs rather than
        // failing, so the cleanup gets its own thread and its own runtime.
        let report = std::thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("a runtime for the cleanup")
                .block_on(async move {
                    reassign(&client(), &url, &user, &password, &restore, was.as_deref()).await
                })
        })
        .join();
        let failure = match report {
            Ok(Ok(())) => return,
            Ok(Err(e)) => e,
            Err(_) => format!(
                "the cleanup thread panicked (its own message is on stderr), so {key} may still \
                 be assigned to the suite"
            ),
        };
        let report = format!(
            "the live suite did not give {key}'s assignee back, so the server is no longer in \
             the plain-seed state: {failure}"
        );
        if std::thread::panicking() {
            eprintln!("live suite cleanup: {report}");
        } else {
            panic!("{report}");
        }
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
    let baseline = before
        .updated_at
        .expect("a real Jira always sets updated");
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
         Jira, and the digest, the inbox and every `@me` filter read it"
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

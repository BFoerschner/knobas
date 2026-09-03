//! The **app**, end to end, against the seeded real Jira (issue #276,
//! ADR-0013) -- the half of the M3.0 certification that the adapter's own live
//! suite cannot reach.
//!
//! `knobas-source-jira/tests/live_jira_seeded.rs` certifies the adapter: the
//! wire, the payload, the cursor. Two of M3.0's claims are not about the
//! adapter at all, and both are asserted here through the real engine over a
//! real database:
//!
//! * **Credential health.** "A wrong PAT yields the credential-health path" is
//!   a claim about `knobas.source_config.auth_state` and the `source:health`
//!   event the sources view renders from -- three crates downstream of the
//!   `SourceError` the adapter raises. And on this server it carries a real
//!   hazard with it: an unresolvable bearer token searches **anonymously**, and
//!   an anonymous `/rest/api/2/search` answers `200` with `total: 0`. The
//!   `ticket` kind claims `full_sync_exhaustive: true`, so a run that reported
//!   that as a completed sync would hand the engine a licence to tombstone
//!   every ticket in the mirror. [`a_revoked_pat_reaches_the_credential_health_surface_and_the_mirror_survives`]
//!   is that whole sentence, measured.
//! * **The three write ops through the write queue.** `tests/mockd.rs` calls
//!   `Source::write` directly; the queue is what the *app* calls, and what
//!   turns an adapter's refusal into the `refused` row the pending-writes panel
//!   shows. Both halves are asserted against **Jira's own answer**, never
//!   against knobas' mirror of it.
//!
//! `#[ignore]`d, so `just check` runs none of it. `just atlassian-live` from
//! the repo root stands the pair up, seeds it, runs this, and tears it down
//! again; inside a licence window already open:
//!
//! ```text
//! cd testenv && eval "$(./seed --env)"
//! cd .. && env -u RUSTUP_TOOLCHAIN cargo test -p knobas-app \
//!     --test atlassian_live -- --ignored --nocapture --test-threads=1
//! ```
//!
//! # What this file writes, and what it takes away
//!
//! It is the suite that writes: a personal access token, a comment on PAY-231,
//! one transition of PAY-240 and back, and one new ticket in `PAY`. Every one
//! of them is undone when the test ends, passing or panicking alike, by a
//! `Drop` that checks rather than assumes -- [`Litter`] and [`Pat`]. What a
//! *killed* run left behind is cleared before the next one takes a baseline:
//! [`Env::clear_leftovers`] deletes every issue and revokes every token
//! carrying [`LITTER_LABEL`]. The **full** restore is
//! `knobas-source-jira`'s live suite's `Seeded::clear_leftovers`, which works
//! from `seed-state.json` rather than from a label and so also puts back a
//! comment or a status this suite left behind -- either suite's leftovers are
//! the other's to clear, because whichever runs next is the one that can.
//!
//! The two things it cannot take away are the `PAY` key counter -- Jira never
//! rewinds one, so a created ticket costs the project one key for ever -- and
//! the `updated` stamps of what it touched. Neither is fixture content, and the
//! environment is torn down at the end of the window regardless.
//!
//! **Never a wrong password.** A real Jira counts failed password logins per
//! account and answers `403 AUTHENTICATION_DENIED` -- to the *correct* password
//! too -- once an account has failed a few. Everything here that needs a
//! credential Jira will not accept uses a **bearer token**, which is not a
//! login attempt; `Seeded::refused_source` in the adapter's live suite carries
//! the full reasoning.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use knobas_app::sources::{Registry, SourcesState};
use knobas_secrets::{MemoryStore, Secret, SecretStore};
use knobas_source::AuthMethod;
use knobas_sync::SyncTrigger;
use knobas_sync::health::AuthState;
use knobas_sync::scheduler::{RunConnections, Scheduler, SchedulerDeps, SyncEvents};
use serde_json::json;

/// The source id, which is also the `EntityRef` namespace every mirrored
/// ticket is in (P10).
const JIRA: &str = "jira";

/// The label everything this suite writes is marked with, spelled the same as
/// `knobas-source-jira/tests/live_jira_seeded.rs`'s. A marker, so a person
/// looking at the server can tell whose litter it is; the restore that matters
/// is that suite's, and it works from `seed-state.json`.
const LITTER_LABEL: &str = "knobas-live-suite";

/// How long one HTTP exchange with the container may take.
const REQUEST_BUDGET: Duration = Duration::from_secs(30);

/// How long Jira's search index may lag a write made through the REST API --
/// the create is read back by JQL, which is an index read.
const INDEX_BUDGET: Duration = Duration::from_secs(60);

/// The issue a comment is posted on: the fixture's own story, and the one
/// whose worklog M3.1 will add to.
const COMMENTED: &str = "PAY-231";

/// The issue that is transitioned. `PAY-240` is the fixture's **To Do** task,
/// so the legal move is out of To Do and the guard's undo is back into it --
/// and it is the issue `tests/start_work_live.rs` uses against mockd for the
/// same reason.
const TRANSITIONED: &str = "PAY-240";

/// A status the template's workflow does not have at all.
///
/// Not a status the issue merely cannot reach *from here*: this workflow
/// ("Software Simplified Workflow for Project `<KEY>`") offers all four of its
/// statuses from every one of them, the issue's own included -- which is
/// itself a divergence from mockd, whose fixture workflow does not. So the
/// only way to witness a refusal against the real server is a status that is
/// not in the workflow, and `seed-state.json`'s `jira.statuses` is what this
/// is checked against rather than assumed.
const UNREACHABLE_STATUS: &str = "Blocked";

/// The project a ticket is created in, and the type it is created as -- one
/// the template's issue type scheme has.
const CREATE_PROJECT: &str = "PAY";
const CREATE_TYPE: &str = "Task";

// -- the environment --------------------------------------------------------

struct Env {
    url: String,
    user: String,
    password: String,
    http: reqwest::Client,
    /// `jira.statuses` from `seed-state.json`: what the seeded workflow offers.
    statuses: Vec<String>,
}

fn env() -> Env {
    let need = |key: &str| {
        std::env::var(key)
            .ok()
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| {
                panic!(
                    "{key} is not set -- this suite needs testenv's seeded Jira. From the repo \
                     root: `just atlassian-live`; or, inside a licence window already open, \
                     `eval \"$(cd testenv && ./seed --env)\"`"
                )
            })
    };
    let state = std::env::var("KNOBAS_JIRA_SEED_STATE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testenv/seed-state.json")
        });
    let raw = std::fs::read_to_string(&state).unwrap_or_else(|e| {
        panic!(
            "{}: {e} -- run `./seed-atlassian-content.sh`",
            state.display()
        )
    });
    let whole: serde_json::Value = serde_json::from_str(&raw).expect("seed-state.json is JSON");
    let statuses: Vec<String> = whole["jira"]["statuses"][CREATE_PROJECT]
        .as_array()
        .unwrap_or_else(|| {
            panic!(
                "{}: no jira.statuses.{CREATE_PROJECT} -- run `./seed-atlassian-content.sh`",
                state.display()
            )
        })
        .iter()
        .filter_map(|s| s.as_str().map(str::to_owned))
        .collect();
    Env {
        url: need("KNOBAS_JIRA_URL").trim_end_matches('/').to_owned(),
        user: need("KNOBAS_JIRA_USER"),
        password: need("KNOBAS_JIRA_PASSWORD"),
        http: client(),
        statuses,
    }
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(REQUEST_BUDGET)
        .build()
        .expect("a reqwest client with a timeout")
}

impl Env {
    /// One raw request as the seed's admin, answering status and body -- what
    /// every assertion below is made against, because the criterion is what
    /// **Jira** holds and not what knobas thinks it does.
    async fn api(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> (u16, serde_json::Value) {
        api(
            &self.http,
            &self.url,
            &self.user,
            &self.password,
            method,
            path,
            body,
        )
        .await
    }

    async fn issue(&self, key: &str, fields: &str) -> serde_json::Value {
        let (status, body) = self
            .api(
                reqwest::Method::GET,
                &format!("rest/api/2/issue/{key}?fields={fields}"),
                None,
            )
            .await;
        assert_eq!(status, 200, "GET issue {key}: {body}");
        body
    }

    async fn status_at_jira(&self, key: &str) -> String {
        self.issue(key, "status").await["fields"]["status"]["name"]
            .as_str()
            .expect("a status name")
            .to_owned()
    }

    /// The keys a JQL query answers with.
    async fn jql(&self, jql: &str) -> Vec<String> {
        let encoded: String = url_encode(jql);
        let (status, body) = self
            .api(
                reqwest::Method::GET,
                &format!("rest/api/2/search?jql={encoded}&maxResults=100&fields=key"),
                None,
            )
            .await;
        assert_eq!(status, 200, "JQL {jql:?} answered {body}");
        body["issues"]
            .as_array()
            .map(|rows| {
                rows.iter()
                    .filter_map(|row| row["key"].as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Delete every issue and revoke every token a **killed** run left behind
    /// -- the ones no `Drop` ever reached. Read-only in the ordinary case.
    async fn clear_leftovers(&self) {
        for key in self.jql(&format!("labels = {LITTER_LABEL}")).await {
            let (status, body) = self
                .api(
                    reqwest::Method::DELETE,
                    &format!("rest/api/2/issue/{key}"),
                    None,
                )
                .await;
            assert_eq!(status, 204, "deleting the leftover issue {key}: {body}");
            println!("live suite: deleted leftover issue {key}");
        }
        let (status, body) = self
            .api(reqwest::Method::GET, "rest/pat/latest/tokens", None)
            .await;
        assert_eq!(status, 200, "listing personal access tokens: {body}");
        for id in body
            .as_array()
            .into_iter()
            .flatten()
            .filter(|t| {
                t["name"]
                    .as_str()
                    .is_some_and(|n| n.starts_with(LITTER_LABEL))
            })
            .filter_map(|t| t["id"].as_i64())
        {
            let (status, body) = self
                .api(
                    reqwest::Method::DELETE,
                    &format!("rest/pat/latest/tokens/{id}"),
                    None,
                )
                .await;
            assert_eq!(status, 204, "revoking the leftover token {id}: {body}");
            println!("live suite: revoked leftover personal access token {id}");
        }
    }
}

/// Free-standing so `Drop`'s cleanup thread, which has no [`Env`], can use it.
async fn api(
    http: &reqwest::Client,
    url: &str,
    user: &str,
    password: &str,
    method: reqwest::Method,
    path: &str,
    body: Option<serde_json::Value>,
) -> (u16, serde_json::Value) {
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
        .unwrap_or_else(|e| panic!("{method} {path}: {e}"));
    let status = response.status().as_u16();
    let text = response.text().await.unwrap_or_default();
    let json = serde_json::from_str(&text).unwrap_or(serde_json::Value::String(text));
    (status, json)
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

// -- what the suite writes, and gives back -----------------------------------

/// A personal access token of this suite's own, revoked when the guard drops.
///
/// The credential-health criterion is about a **PAT**, and it is about one that
/// *worked* and stopped working -- so the suite makes a real one rather than
/// asserting over a token that was never valid. It also certifies, against the
/// product, the claim `knobas-source-jira`'s `http` module makes from the
/// documentation: a Jira DC personal access token is a Bearer token.
///
/// `/rest/pat/latest/tokens` is outside the adapter's own endpoint set (it is
/// why `ConnectionInfo::secret_expires_at` is always `None`), which is exactly
/// why it is reached here with raw requests and not through the adapter.
struct Pat {
    url: String,
    user: String,
    password: String,
    id: i64,
    raw: String,
}

impl Pat {
    async fn issue(env: &Env) -> Pat {
        let name = format!("{LITTER_LABEL}-{}", std::process::id());
        let (status, body) = env
            .api(
                reqwest::Method::POST,
                "rest/pat/latest/tokens",
                Some(json!({ "name": name, "expirationDuration": 1 })),
            )
            .await;
        assert_eq!(status, 201, "creating a personal access token: {body}");
        Pat {
            url: env.url.clone(),
            user: env.user.clone(),
            password: env.password.clone(),
            id: body["id"].as_i64().expect("a token id"),
            raw: body["rawToken"]
                .as_str()
                .expect("Jira answers the raw token exactly once, at creation")
                .to_owned(),
        }
    }
}

impl Drop for Pat {
    fn drop(&mut self) {
        let (url, user, password, id) = (
            self.url.clone(),
            self.user.clone(),
            self.password.clone(),
            self.id,
        );
        undo("the personal access token", move || async move {
            let (status, body) = api(
                &client(),
                &url,
                &user,
                &password,
                reqwest::Method::DELETE,
                &format!("rest/pat/latest/tokens/{id}"),
                None,
            )
            .await;
            if status != 204 && status != 404 {
                return Err(format!("DELETE token {id} -> {status}: {body}"));
            }
            let (status, body) = api(
                &client(),
                &url,
                &user,
                &password,
                reqwest::Method::GET,
                "rest/pat/latest/tokens",
                None,
            )
            .await;
            if status != 200 {
                return Err(format!(
                    "listing tokens after the delete -> {status}: {body}"
                ));
            }
            if body
                .as_array()
                .into_iter()
                .flatten()
                .any(|t| t["id"].as_i64() == Some(id))
            {
                return Err(format!("token {id} is still listed after its delete"));
            }
            Ok(())
        });
    }
}

/// Everything the write tests put into Jira, taken back out when the guard
/// drops -- passing or panicking alike, and each undo **checked** rather than
/// assumed.
#[derive(Default)]
struct Litter {
    /// `(issue key, comment id)`.
    comment: Option<(String, String)>,
    /// `(issue key, the status it was in before)`.
    moved: Option<(String, String)>,
    /// The key of the ticket the create filed.
    created: Option<String>,
}

impl Drop for Litter {
    fn drop(&mut self) {
        let (comment, moved, created) =
            (self.comment.take(), self.moved.take(), self.created.take());
        let env = env();
        undo("what the write queue sent", move || async move {
            let http = client();
            let call = |method: reqwest::Method, path: String, body: Option<serde_json::Value>| {
                let http = http.clone();
                let (url, user, password) =
                    (env.url.clone(), env.user.clone(), env.password.clone());
                async move { api(&http, &url, &user, &password, method, &path, body).await }
            };
            let mut failures: Vec<String> = Vec::new();

            if let Some((key, id)) = comment {
                let (status, body) = call(
                    reqwest::Method::DELETE,
                    format!("rest/api/2/issue/{key}/comment/{id}"),
                    None,
                )
                .await;
                if status != 204 && status != 404 {
                    failures.push(format!("DELETE comment {id} on {key} -> {status}: {body}"));
                }
            }

            // The status is put back through the workflow, which is the only
            // way a status changes: this workflow offers every status from
            // every status, so the move back always exists.
            if let Some((key, was)) = moved {
                let (status, body) = call(
                    reqwest::Method::GET,
                    format!("rest/api/2/issue/{key}/transitions"),
                    None,
                )
                .await;
                let id = body["transitions"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|t| t["to"]["name"].as_str() == Some(was.as_str()))
                    .and_then(|t| t["id"].as_str().map(str::to_owned));
                match id {
                    None => failures.push(format!(
                        "the workflow no longer offers {key} a way back to {was:?} \
                         ({status}): {body}"
                    )),
                    Some(id) => {
                        let (status, body) = call(
                            reqwest::Method::POST,
                            format!("rest/api/2/issue/{key}/transitions"),
                            Some(json!({ "transition": { "id": id } })),
                        )
                        .await;
                        if status != 204 {
                            failures
                                .push(format!("moving {key} back to {was:?} -> {status}: {body}"));
                        }
                    }
                }
            }

            if let Some(key) = created {
                let (status, body) = call(
                    reqwest::Method::DELETE,
                    format!("rest/api/2/issue/{key}"),
                    None,
                )
                .await;
                if status != 204 && status != 404 {
                    failures.push(format!("DELETE issue {key} -> {status}: {body}"));
                }
                let (status, _) = call(
                    reqwest::Method::GET,
                    format!("rest/api/2/issue/{key}?fields=key"),
                    None,
                )
                .await;
                if status != 404 {
                    failures.push(format!("{key} still answers {status} after its delete"));
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

/// Run one cleanup on a thread with a runtime of its own -- `Drop` cannot
/// await, and a `reqwest::Client` driven from a second runtime hangs rather
/// than failing (the Gitea suite measured that) -- and report what it could
/// not undo.
///
/// Panicking while already unwinding aborts the process; the test is already
/// red in that case and the report only has to be visible.
fn undo<F, Fut>(what: &str, cleanup: F)
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<(), String>>,
{
    let joined = std::thread::spawn(move || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime for the cleanup")
            .block_on(cleanup())
    })
    .join();
    let failure = match joined {
        Ok(Ok(())) => return,
        Ok(Err(e)) => e,
        Err(_) => "the cleanup thread panicked (its own message is on stderr)".to_owned(),
    };
    let report = format!(
        "the live suite did not take back {what}, so the server is no longer in the plain-seed \
         state -- the next run's leftover clearing removes what carries {LITTER_LABEL:?}: {failure}"
    );
    if std::thread::panicking() {
        eprintln!("live suite cleanup: {report}");
    } else {
        panic!("{report}");
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

/// The `source:health` events the shell would render, kept so the assertion
/// can be on the **event** and not only on the stored column. The two are
/// different claims: `set_health` reports a change once, and a sources view
/// that never heard would show a stale monogram until something else redrew it.
#[derive(Default)]
struct Events {
    health: Mutex<Vec<knobas_sync::CredentialHealth>>,
}

impl SyncEvents for Events {
    fn sync_state(&self, _status: knobas_sync::SourceSyncStatus) {}
    fn source_health(&self, health: knobas_sync::CredentialHealth) {
        self.health
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(health);
    }
    fn activity_new(&self, _row: knobas_core::activity::ActivityRow) {}
}

impl Events {
    fn states(&self) -> Vec<AuthState> {
        self.health
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .map(|h| h.state)
            .collect()
    }

    fn last(&self) -> knobas_sync::CredentialHealth {
        self.health
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .last()
            .cloned()
            .expect("at least one health event")
    }
}

/// A `SourcesState` over a database of this test's own, with the Jira source
/// configured and its credential in the (in-memory) keychain.
async fn app(name: &str, env: &Env, auth: AuthMethod, secret: &str) -> (SourcesState, Arc<Events>) {
    let connector = knobas_db::test_util::scratch_database(name).await;
    let pool = connector
        .pool(4)
        .await
        .expect("a pool onto the scratch database");
    let secrets = Arc::new(MemoryStore::new());
    secrets
        .put(
            JIRA,
            &Secret {
                kind: auth,
                value: secret.to_owned(),
            },
        )
        .expect("the Jira credential is stored");

    knobas_sync::config::insert(
        &pool,
        &knobas_sync::config::InsertConfig {
            id: JIRA.to_owned(),
            adapter_kind: "jira".to_owned(),
            display_name: "Tidewater Jira (seeded)".to_owned(),
            base_url: env.url.clone(),
            auth_kind: knobas_sync::config::AuthKind::Method(auth),
            // The username is half of a Basic pair and, for a PAT source, what
            // `@me` filters match against; the Add-source dialog fills it in
            // from `test_connection`.
            config: json!({ "username": env.user }),
            sync_interval_secs: 86_400,
            enabled: true,
        },
    )
    .await
    .expect("the source row is written");

    let events = Arc::new(Events::default());
    let scheduler = Scheduler::start(SchedulerDeps {
        pool: pool.clone(),
        connections: Arc::new(Connections(connector)),
        registry: Arc::new(Registry::builtin()),
        secrets: secrets.clone(),
        events: events.clone(),
    })
    .await
    .expect("a scheduler over the scratch database");

    (
        SourcesState {
            pool,
            scheduler,
            secrets,
            registry: Arc::new(Registry::builtin()),
        },
        events,
    )
}

/// Sync the source and wait for the run to end, whichever way it ends.
async fn sync(state: &SourcesState) {
    let (done, wait) = tokio::sync::oneshot::channel();
    let sink = Arc::new(Ending {
        done: std::sync::Mutex::new(Some(done)),
    });
    state
        .scheduler
        .trigger(JIRA, SyncTrigger::Manual, Some(sink))
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

/// The keys the mirror holds live for this source -- the thing a wrongly
/// reported empty sync would erase.
async fn mirrored(pool: &sqlx::PgPool) -> Vec<String> {
    sqlx::query_scalar::<_, String>(
        "select entity_id from sync.live_item where source_id = $1 order by entity_id",
    )
    .bind(JIRA)
    .fetch_all(pool)
    .await
    .expect("the mirror is readable")
}

/// Queue one write through the app's own submit path -- the same call the
/// *Comment* button makes -- and answer the row as it settled.
async fn write(
    state: &SourcesState,
    op: serde_json::Value,
) -> knobas_core::write_queue::QueuedWrite {
    let queued = knobas_app::sources::write_queue::submit(state, op)
        .await
        .expect("the write is queued");
    // The row came back **as queued**, before the attempt, which says nothing
    // about the outcome. Read it again for what actually happened.
    knobas_core::write_queue::get(&state.pool, queued.id)
        .await
        .expect("the queue row is readable")
        .expect("the row this call just wrote")
}

// -- the criteria -----------------------------------------------------------

/// **A personal access token that worked and stopped working**, all the way
/// to what the sources view renders -- and the mirror still standing
/// afterwards.
///
/// The sequence is the one that actually happens to people: a source is added
/// with a PAT, syncs, and the token is later revoked. What makes it worth a
/// live test rather than a fake is what this Jira does with the revoked one:
/// an unresolvable bearer token is **not** a failed login. It never reaches
/// Seraph, and the request proceeds anonymously -- so
/// `GET /rest/api/2/search` answers **200 with `total: 0`**, an answer
/// indistinguishable from a Jira whose issues have all been deleted.
///
/// The `ticket` kind claims `full_sync_exhaustive: true`. A run that reported
/// that empty page as a completed full sync would therefore have the engine
/// tombstone every ticket knobas holds -- silently, and on nothing worse than
/// an expired token. It does not, because the run asks `/serverInfo` first for
/// the server's UTC offset and that call is a 401. The last assertion here is
/// the one that would fail if that ever stopped being true.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs testenv's seeded Jira: `just atlassian-live`"]
async fn a_revoked_pat_reaches_the_credential_health_surface_and_the_mirror_survives() {
    let env = env();
    env.clear_leftovers().await;
    let pat = Pat::issue(&env).await;

    let (state, events) = app("atlassian_live_health", &env, AuthMethod::Pat, &pat.raw).await;

    // 1. The token works, which is what makes revoking it mean anything -- and
    //    is the product's own answer to `http::credential`'s claim that a Jira
    //    DC personal access token is a Bearer token.
    sync(&state).await;
    let synced = mirrored(&state.pool).await;
    assert!(
        synced.len() >= 7,
        "the seeded corpus is mirrored under a personal access token: {synced:?}"
    );
    let healthy = knobas_sync::config::get(&state.pool, JIRA)
        .await
        .expect("the source row")
        .expect("the source this test configured");
    assert_eq!(healthy.health.state, AuthState::Ok, "{:?}", healthy.health);
    assert_eq!(
        events.states(),
        vec![AuthState::Ok],
        "the sources view is told once that the credential works"
    );

    // 2. The token is revoked -- here by swapping what the keychain holds,
    //    which is the same thing from the adapter's side and leaves the real
    //    token for the guard to clean up.
    state
        .secrets
        .put(
            JIRA,
            &Secret {
                kind: AuthMethod::Pat,
                value: format!("revoked-{}", std::process::id()),
            },
        )
        .expect("the replacement credential is stored");

    sync(&state).await;

    // 3. The credential-health path, at both ends of it: the stored column the
    //    sources view polls, and the event it re-renders on.
    let refused = knobas_sync::config::get(&state.pool, JIRA)
        .await
        .expect("the source row")
        .expect("the source this test configured");
    assert_eq!(
        refused.health.state,
        AuthState::Unauthorized,
        "a credential the server refuses is `unauthorized`, which is what puts *Re-enter* on the \
         row (interfaces §3): {:?}",
        refused.health
    );
    assert!(refused.health.checked_at.is_some(), "{:?}", refused.health);
    let event = events.last();
    assert_eq!(event.state, AuthState::Unauthorized);
    assert_eq!(event.source_id, JIRA);
    let detail = event.detail.clone().unwrap_or_default();
    assert!(
        detail.contains("unauthorized"),
        "the detail line names the fault the row is in: {detail:?}"
    );
    // The **status** behind it is on `SourceError::status` (ADR-0004) and is
    // deliberately not on this line -- `SourceError::Unauthorized`'s own
    // `Display` is the bare word, because 401 and 403 are one state to the
    // person being asked to re-enter a credential. Asserted as it is rather
    // than wished otherwise: putting the status here is an IPC-surface change
    // (§10.8) and no criterion asks for one.
    assert!(
        !detail.contains(&pat.raw),
        "spec §14: a health detail is never a place a secret can reach"
    );
    println!("SEEDED credential health after the revoke: {:?}", event);

    // 4. **And the mirror is untouched.** This is the assertion the anonymous
    //    empty search is dangerous for: a `cursor: None` run reported `Ok` with
    //    no items authorises the sweep to tombstone the lot.
    assert_eq!(
        mirrored(&state.pool).await,
        synced,
        "a refused sync must leave the mirror exactly as it was -- an anonymous \
         /rest/api/2/search answers 200 with total 0 on this server, and reporting that as a \
         completed full sync would tombstone every ticket knobas holds"
    );

    state.scheduler.shutdown().await;
}

/// **The three write ops M2 ratified, through the write queue and back out of
/// Jira** -- comment, a transition the workflow offers, one it does not, and a
/// create.
///
/// One test rather than four: they share one source, one mirror and one
/// cleanup guard, and splitting them would have each pass over a state the
/// others established. Every assertion is against **Jira's own answer** or
/// against the queue row, never against knobas' mirror of either.
///
/// The refusal is the interesting one. `tests/mockd.rs` asserts that the
/// adapter refuses an unreachable status *by name*; what the queue does with
/// that refusal -- `refused`, terminal, never retried, with the adapter's
/// sentence on the row for the pending-writes panel to show -- is this file's,
/// and it is the difference between an edit a person can act on and one that
/// disappears.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs testenv's seeded Jira: `just atlassian-live`"]
async fn the_three_write_ops_go_through_the_queue_and_come_back_from_jira() {
    use knobas_core::write_queue::WriteState;

    let env = env();
    env.clear_leftovers().await;
    let mut litter = Litter::default();
    // Under a **personal access token**, not the seed's password: a PAT is the
    // credential a real deployment configures and the one M3.1's `LogWork`
    // will write worklogs with, and nothing else in the repo has ever sent
    // Jira a write over one. The read direction under user + password is what
    // the adapter's own live suite certifies, and the seed script itself
    // writes over Basic, so neither scheme is left unwitnessed.
    let pat = Pat::issue(&env).await;
    let (state, _events) = app("atlassian_live_writes", &env, AuthMethod::Pat, &pat.raw).await;

    // The mirror has to hold the tickets first: the queue snapshots its target
    // when a write is queued and re-reads it before sending, which is how a
    // write over a ticket that moved is held rather than sent.
    sync(&state).await;
    let held = mirrored(&state.pool).await;
    for key in [COMMENTED, TRANSITIONED] {
        assert!(
            held.contains(&format!("{JIRA}:{key}")),
            "{key} is not in the mirror: {held:?}"
        );
    }

    // -- 1. Comment ---------------------------------------------------------
    let body = format!(
        "{LITTER_LABEL}: knobas wrote this through the write queue (pid {})",
        std::process::id()
    );
    let before = env.issue(COMMENTED, "comment").await["fields"]["comment"]["total"]
        .as_i64()
        .expect("a comment total");
    let row = write(
        &state,
        json!({ "Comment": { "entity": format!("{JIRA}:{COMMENTED}"), "body": body } }),
    )
    .await;
    assert_eq!(row.state, WriteState::Sent, "{:?}", row.detail);

    // Read back, and **owned before anything is asserted**: from here on the
    // comment exists at Jira, so a failing assertion below must still leave the
    // guard something to delete.
    let comments = env.issue(COMMENTED, "comment").await["fields"]["comment"].clone();
    let posted = comments["comments"]
        .as_array()
        .and_then(|c| c.last())
        .cloned()
        .expect("the comment just added");
    if let Some(id) = posted["id"].as_str() {
        litter.comment = Some((COMMENTED.to_owned(), id.to_owned()));
    }
    assert_eq!(
        comments["total"].as_i64(),
        Some(before + 1),
        "the comment is on the ticket at Jira: {comments}"
    );
    assert_eq!(posted["body"], body);
    assert_eq!(
        posted["author"]["name"], env.user,
        "story 17: the source attributes the write to the credential's own account"
    );

    // -- 2. A transition the workflow offers --------------------------------
    let was = env.status_at_jira(TRANSITIONED).await;
    let to = env
        .statuses
        .iter()
        .find(|s| *s != &was)
        .expect("the seeded workflow has more than one status")
        .clone();
    let row = write(
        &state,
        json!({ "Transition": { "entity": format!("{JIRA}:{TRANSITIONED}"), "status": to } }),
    )
    .await;
    // Recorded before the assertion: the guard has to put the status back even
    // if the queue reports something other than `sent`.
    litter.moved = Some((TRANSITIONED.to_owned(), was.clone()));
    assert_eq!(row.state, WriteState::Sent, "{:?}", row.detail);
    assert_eq!(
        env.status_at_jira(TRANSITIONED).await,
        to,
        "the board still lies about what is being worked on"
    );

    // -- 3. One it does not -------------------------------------------------
    assert!(
        !env.statuses.iter().any(|s| s == UNREACHABLE_STATUS),
        "this workflow does have {UNREACHABLE_STATUS:?} after all ({:?}), so the refusal below \
         would be testing nothing -- pick a status it has not got",
        env.statuses
    );
    let row = write(
        &state,
        json!({
            "Transition": {
                "entity": format!("{JIRA}:{TRANSITIONED}"),
                "status": UNREACHABLE_STATUS
            }
        }),
    )
    .await;
    assert_eq!(
        row.state,
        WriteState::Refused,
        "a status the workflow does not have is refused, not retried for ever: {:?}",
        row.detail
    );
    let detail = row.detail.clone().unwrap_or_default();
    assert!(
        detail.contains(UNREACHABLE_STATUS),
        "the refusal names the status that was asked for: {detail:?}"
    );
    assert!(
        detail.contains(&to),
        "...and what the workflow does offer instead, which is what makes it actionable: \
         {detail:?}"
    );
    assert_eq!(
        env.status_at_jira(TRANSITIONED).await,
        to,
        "a refused transition must not have moved anything"
    );
    println!("SEEDED refused transition: {detail}");

    // -- 4. Create ----------------------------------------------------------
    //
    // The target is the **project** (`jira:PAY`), a container knobas does not
    // mirror -- which is what the queue's hold detection is built for: an
    // unmirrored container is `live: false` at queue time and at flush time
    // alike, so a create never holds.
    let title = format!(
        "{LITTER_LABEL}: filed by the knobas live suite (pid {})",
        std::process::id()
    );
    let scope = format!("project = {CREATE_PROJECT}");
    let before: std::collections::BTreeSet<String> = env.jql(&scope).await.into_iter().collect();
    let row = write(
        &state,
        json!({
            "CreateTicket": {
                "entity": format!("{JIRA}:{CREATE_PROJECT}"),
                "title": title,
                "body": "the batch job times out and the payouts are lost",
                "ticket_type": CREATE_TYPE
            }
        }),
    )
    .await;
    assert_eq!(row.state, WriteState::Sent, "{:?}", row.detail);

    // `Source::write` answers `()` -- widening it is a frozen-SPI change and no
    // criterion asks for one -- so the new key is found the way a person would
    // find it, and then **confirmed by a read of the issue itself**.
    //
    // Both halves are needed. JQL is an index read and Jira's index lags its
    // own writes, so the key may not be there yet; and the index also lags
    // *deletes*, so a key it hands back may be a ticket an earlier run's
    // cleanup already removed. Taking the newest key on trust would then read
    // back a 404. `GET /rest/api/2/issue/{key}` goes to the database, so a
    // ghost fails the summary check and the poll goes round again.
    let deadline = std::time::Instant::now() + INDEX_BUDGET;
    let created = 'found: loop {
        for key in env.jql(&scope).await {
            if before.contains(&key) {
                continue;
            }
            let (status, body) = env
                .api(
                    reqwest::Method::GET,
                    &format!("rest/api/2/issue/{key}?fields=summary"),
                    None,
                )
                .await;
            if status == 200 && body["fields"]["summary"] == title.as_str() {
                break 'found key;
            }
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the created ticket did not reach Jira's search index within {INDEX_BUDGET:?}"
        );
        tokio::time::sleep(Duration::from_secs(1)).await;
    };
    litter.created = Some(created.clone());
    // Labelled as soon as it exists, so a run that is *killed* after this line
    // still leaves something the next run's leftover clearing can find.
    let (status, labelled) = env
        .api(
            reqwest::Method::PUT,
            &format!("rest/api/2/issue/{created}"),
            Some(json!({ "update": { "labels": [{ "add": LITTER_LABEL }] } })),
        )
        .await;
    assert_eq!(status, 204, "labelling the created ticket: {labelled}");

    let filed = env
        .issue(&created, "summary,description,issuetype,project,reporter")
        .await;
    assert_eq!(filed["fields"]["summary"], title);
    assert_eq!(
        filed["fields"]["description"], "the batch job times out and the payouts are lost",
        "a create that dropped the description would be reported as a success"
    );
    assert_eq!(filed["fields"]["issuetype"]["name"], CREATE_TYPE);
    assert_eq!(filed["fields"]["project"]["key"], CREATE_PROJECT);
    assert_eq!(
        filed["fields"]["reporter"]["name"], env.user,
        "story 17: attributed to me"
    );
    println!("SEEDED created ticket: {created} ({title})");

    state.scheduler.shutdown().await;
    drop(litter);
}

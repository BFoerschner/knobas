//! The Confluence **write path**, end to end, against the seeded real
//! container (issue #286, ADR-0013).
//!
//! ```text
//! just atlassian-live      # from the repo root
//! ```
//!
//! # What this file certifies that nothing else can
//!
//! `crates/knobas-source-confluence/tests/live_confluence_seeded.rs` is the
//! adapter's witness and it holds a socket in one hand and a `Source` in the
//! other. This file holds the **application**: a scratch database, the real
//! sync engine, the real write queue, the real scheduler. So the claims here
//! are the ones only the whole stack can make.
//!
//! * **A section edit lands and the rest of the page is intact.** The op
//!   carries the *whole* body -- Confluence's content `PUT` replaces the
//!   record -- so "intact" is not a hope about a patch, it is the assertion
//!   that the body knobas re-assembled is byte for byte the seeded page with
//!   one paragraph changed.
//! * **An edit made against a version the mirror has passed is held.** The
//!   page is edited out of band, a sync mirrors that, and the queued write
//!   goes to [`WriteState::Held`] with both versions in the row -- which is
//!   `CONTEXT.md`'s **held write**, "resolved by choosing between the two
//!   versions, shown side by side", and not a silent overwrite. Nothing but
//!   the real queue over a real mirror can say this.
//! * **A comment lands on a page**, as the SPI's own `Comment` op with the
//!   page as its container, and comes back at `children.comment` where the
//!   detail reads it -- with the author and instant `EXPAND` now asks for.
//! * **A created page appears under its parent**, which is the one claim
//!   `CreatePage`'s `parent` field exists to make.
//!
//! # What this file writes, and what it takes away
//!
//! One paragraph of one seeded page's body, one comment, and one page. All
//! three are undone in [`Litter`]'s `Drop` -- passing or panicking alike, on a
//! thread with a runtime of its own -- and the removal is checked rather than
//! assumed. The page body is restored from the bytes read **before** the edit,
//! so a failure part-way leaves the seed as it was rather than as this suite
//! guessed it was.
//!
//! What a *killed* run left behind is not recoverable from here, and does not
//! have to be: `live_confluence_seeded.rs`'s `Seeded::clear_leftovers` puts
//! every seeded page's title back, and the body this suite edits is restored
//! by re-running `./seed-atlassian-content.sh`, which is idempotent.
//!
//! **One owner at a time**, as for every other seeded suite
//! (`testenv/README.md`).
//!
//! # A red run here is never answered by running it again
//!
//! When knobas and the server disagree, **knobas** is wrong.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use knobas_app::sources::{Registry, SourcesState};
use knobas_core::write_queue::WriteState;
use knobas_secrets::{MemoryStore, Secret, SecretStore};
use knobas_source::AuthMethod;
use knobas_sync::SyncTrigger;
use knobas_sync::scheduler::{RunConnections, Scheduler, SchedulerDeps, SyncEvents};
use serde_json::json;

/// The source id, which is also the `EntityRef` namespace every mirrored page
/// is in (P10).
const CONFLUENCE: &str = "confluence";

/// The fixture page this suite edits and comments on: the only seeded page
/// with a body, and its two `<h2>` sections hold no macro and no table -- so
/// it is the one page whose section rule answers *editable*.
const EDITED: &str = "sepa-design";

/// The seeded page a new page is created under, so `parent` is a page the
/// mirror really holds.
const PARENT: &str = "standup-protocols";

/// The marker on everything this suite creates, so a person looking at the
/// server can tell whose litter it is.
const LITTER: &str = "knobas-live-suite";

/// How long one HTTP exchange with the container may take.
const REQUEST_BUDGET: Duration = Duration::from_secs(30);

/// How long Confluence's search index may lag a write made through its REST
/// API. CQL reads a Lucene index the write path updates asynchronously, so the
/// sync straight after an edit is not necessarily the one that sees it.
const INDEX_BUDGET: Duration = Duration::from_secs(120);

// -- the environment --------------------------------------------------------

struct Env {
    url: String,
    user: String,
    password: String,
    space: String,
    pages: Vec<SeededPage>,
}

#[derive(Clone, serde::Deserialize)]
struct SeededPage {
    fixture_id: String,
    title: String,
    id: String,
}

/// Panics with the command to run rather than skipping: this suite is only ever
/// run by name, and a silent skip would read as a green certification of
/// nothing.
fn env() -> Env {
    let need = |key: &str| {
        std::env::var(key)
            .ok()
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| {
                panic!(
                    "{key} is not set -- this suite needs testenv's seeded Confluence. From the \
                     repo root: `just atlassian-live`; or, inside a licence window already open, \
                     `eval \"$(cd testenv && ./seed --env)\"`"
                )
            })
    };
    let state = std::env::var("KNOBAS_CONFLUENCE_SEED_STATE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testenv/seed-state.json")
        });
    let raw = std::fs::read_to_string(&state).unwrap_or_else(|e| {
        panic!(
            "{}: {e} -- `./seed-atlassian-content.sh` writes the title-to-id map this suite reads",
            state.display()
        )
    });
    let whole: serde_json::Value = serde_json::from_str(&raw).expect("seed-state.json is JSON");
    let block = &whole["confluence"];
    let pages: Vec<SeededPage> = serde_json::from_value(block["pages"].clone())
        .unwrap_or_else(|e| panic!("{}: no `confluence.pages`: {e}", state.display()));
    assert!(
        !pages.is_empty(),
        "{}: the seed recorded no pages",
        state.display()
    );
    Env {
        url: need("KNOBAS_CONFLUENCE_URL")
            .trim_end_matches('/')
            .to_owned(),
        user: need("KNOBAS_CONFLUENCE_USER"),
        password: need("KNOBAS_CONFLUENCE_PASSWORD"),
        space: block["space"].as_str().unwrap_or("ENG").to_owned(),
        pages,
    }
}

impl Env {
    fn page(&self, fixture_id: &str) -> SeededPage {
        self.pages
            .iter()
            .find(|p| p.fixture_id == fixture_id)
            .unwrap_or_else(|| panic!("seed-state.json records no page {fixture_id}"))
            .clone()
    }

    async fn call(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> (u16, serde_json::Value) {
        api(
            &client(),
            &self.url,
            &self.user,
            &self.password,
            method,
            path,
            body,
        )
        .await
    }

    /// One page's record straight from Confluence, with whatever `expand` asks.
    async fn content(&self, id: &str, expand: &str) -> serde_json::Value {
        let (status, body) = self
            .call(
                reqwest::Method::GET,
                &format!("rest/api/content/{id}?expand={expand}"),
                None,
            )
            .await;
        assert_eq!(status, 200, "GET content {id}: {body}");
        body
    }

    /// The storage format and version number this page stands at right now.
    async fn body_and_version(&self, id: &str) -> (String, i64) {
        let record = self.content(id, "body.storage,version").await;
        (
            record["body"]["storage"]["value"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            record["version"]["number"]
                .as_i64()
                .unwrap_or_else(|| panic!("page {id} has no version number: {record}")),
        )
    }

    /// Replace a page's body through Confluence's own API -- what "somebody
    /// else edited it" looks like from outside knobas.
    async fn put_body(&self, id: &str, title: &str, body: &str, version: i64) {
        let (status, answer) = self
            .call(
                reqwest::Method::PUT,
                &format!("rest/api/content/{id}"),
                Some(json!({
                    "id": id,
                    "type": "page",
                    "title": title,
                    "version": { "number": version + 1 },
                    "body": { "storage": { "value": body, "representation": "storage" } },
                })),
            )
            .await;
        assert_eq!(status, 200, "PUT content {id}: {answer}");
    }
}

/// Percent-encode what a query parameter may contain. Deliberately tiny: the
/// only value encoded here is a title this suite composed itself.
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

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(REQUEST_BUDGET)
        .build()
        .expect("a reqwest client with a timeout")
}

/// One raw request against the seeded instance. Free-standing so a `Drop`
/// thread with no `Env` in hand can still call it.
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

// -- what the suite writes, and gives back -----------------------------------

/// Everything this suite put into the wiki, taken back when it drops.
#[derive(Default)]
struct Litter {
    /// `(id, title, the storage format read **before** the edit)`.
    body: Option<(String, String, String)>,
    /// Content ids to delete: the comment, and the created page.
    created: Vec<String>,
}

impl Drop for Litter {
    fn drop(&mut self) {
        let (body, created) = (self.body.take(), std::mem::take(&mut self.created));
        if body.is_none() && created.is_empty() {
            return;
        }
        let env = env();
        undo("what it wrote to Confluence", move || async move {
            let http = client();
            let mut failures: Vec<String> = Vec::new();

            for id in created {
                let (status, answer) = api(
                    &http,
                    &env.url,
                    &env.user,
                    &env.password,
                    reqwest::Method::DELETE,
                    &format!("rest/api/content/{id}"),
                    None,
                )
                .await;
                // 204 deleted, 200 trashed, 404 gone already -- all three are
                // "it is not there any more", which is the criterion.
                if !matches!(status, 200 | 204 | 404) {
                    failures.push(format!("DELETE content {id} -> {status}: {answer}"));
                }
            }

            if let Some((id, title, original)) = body {
                // Read the version *now*: this suite's own write moved it, and
                // a PUT with a stale number is refused -- which would leave the
                // edit standing, which is the thing being undone.
                let (status, record) = api(
                    &http,
                    &env.url,
                    &env.user,
                    &env.password,
                    reqwest::Method::GET,
                    &format!("rest/api/content/{id}?expand=version"),
                    None,
                )
                .await;
                match record["version"]["number"].as_i64() {
                    _ if status != 200 => {
                        failures.push(format!("GET content {id} -> {status}: {record}"));
                    }
                    None => failures.push(format!("GET content {id}: no version in {record}")),
                    Some(version) => {
                        let (status, answer) = api(
                            &http,
                            &env.url,
                            &env.user,
                            &env.password,
                            reqwest::Method::PUT,
                            &format!("rest/api/content/{id}"),
                            Some(json!({
                                "id": id,
                                "type": "page",
                                "title": title,
                                "version": { "number": version + 1 },
                                "body": {
                                    "storage": {
                                        "value": original,
                                        "representation": "storage",
                                    },
                                },
                            })),
                        )
                        .await;
                        if status != 200 {
                            failures.push(format!("PUT content {id} -> {status}: {answer}"));
                        } else {
                            // Checked, not assumed: a container that stopped
                            // answering must not read as an edit undone.
                            let (_, back) = api(
                                &http,
                                &env.url,
                                &env.user,
                                &env.password,
                                reqwest::Method::GET,
                                &format!("rest/api/content/{id}?expand=body.storage"),
                                None,
                            )
                            .await;
                            if back["body"]["storage"]["value"].as_str() != Some(original.as_str())
                            {
                                failures.push(format!("page {id} did not come back to its body"));
                            }
                        }
                    }
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
/// than failing -- and report what it could not undo.
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
         state -- `./seed-atlassian-content.sh` is idempotent and puts a page body back: {failure}"
    );
    if std::thread::panicking() {
        eprintln!("live suite cleanup: {report}");
    } else {
        panic!("{report}");
    }
}

// -- the app, wired the way the app wires it --------------------------------

struct Connections(knobas_db::embedded::Connector);

#[async_trait]
impl RunConnections for Connections {
    async fn open(&self) -> Result<sqlx::PgConnection, sqlx::Error> {
        self.0.connect().await
    }
}

#[derive(Default)]
struct Silent;

impl SyncEvents for Silent {
    fn sync_state(&self, _status: knobas_sync::SourceSyncStatus) {}
    fn source_health(&self, _health: knobas_sync::CredentialHealth) {}
    fn activity_new(&self, _row: knobas_core::activity::ActivityRow) {}
}

/// A `SourcesState` over a database of this test's own, with the seeded
/// Confluence configured and its credential in the (in-memory) keychain.
///
/// User and password, which is what the setup wizard's admin account has: it
/// holds no personal access token, and `live_confluence_seeded.rs` records why
/// a suite must not draw its 401s from a wrong password.
async fn app(name: &str, env: &Env) -> SourcesState {
    let connector = knobas_db::test_util::scratch_database(name).await;
    let pool = connector
        .pool(4)
        .await
        .expect("a pool onto the scratch database");
    let secrets = Arc::new(MemoryStore::new());
    secrets
        .put(
            CONFLUENCE,
            &Secret {
                kind: AuthMethod::UserPassword,
                value: env.password.clone(),
            },
        )
        .expect("the Confluence credential is stored");

    knobas_sync::config::insert(
        &pool,
        &knobas_sync::config::InsertConfig {
            id: CONFLUENCE.to_owned(),
            adapter_kind: "confluence".to_owned(),
            display_name: "Tidewater Confluence (seeded)".to_owned(),
            base_url: env.url.clone(),
            auth_kind: knobas_sync::config::AuthKind::Method(AuthMethod::UserPassword),
            // Scoped to the seeded space, which is what a real configuration
            // looks like and what keeps these assertions about *this* corpus.
            config: json!({ "username": env.user, "spaces": [env.space] }),
            sync_interval_secs: 86_400,
            enabled: true,
        },
    )
    .await
    .expect("the source row is written");

    let scheduler = Scheduler::start(SchedulerDeps {
        pool: pool.clone(),
        connections: Arc::new(Connections(connector)),
        registry: Arc::new(Registry::builtin()),
        secrets: secrets.clone(),
        events: Arc::new(Silent),
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

/// Sync the source and wait for the run to end, whichever way it ends.
async fn sync(state: &SourcesState) {
    let (done, wait) = tokio::sync::oneshot::channel();
    let sink = Arc::new(Ending {
        done: Mutex::new(Some(done)),
    });
    state
        .scheduler
        .trigger(CONFLUENCE, SyncTrigger::Manual, Some(sink))
        .await
        .expect("the run starts");
    wait.await.expect("the run reports its ending");
}

struct Ending {
    done: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
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

/// Queue one write through the app's own submit path -- the same call the
/// detail's buttons make -- and answer the row as it settled.
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

/// The mirrored payload of one page, or `None` while the index still lags.
async fn mirrored(pool: &sqlx::PgPool, id: &str) -> Option<serde_json::Value> {
    sqlx::query_scalar::<_, serde_json::Value>(
        "select payload from sync.live_item where entity_id = $1",
    )
    .bind(format!("{CONFLUENCE}:{id}"))
    .fetch_optional(pool)
    .await
    .expect("the mirror is readable")
}

/// Sync until `wanted` says the mirror has caught up, or the index budget runs
/// out. CQL reads a Lucene index the write path updates asynchronously.
async fn sync_until(
    state: &SourcesState,
    id: &str,
    what: &str,
    wanted: impl Fn(&serde_json::Value) -> bool,
) -> serde_json::Value {
    let deadline = std::time::Instant::now() + INDEX_BUDGET;
    loop {
        sync(state).await;
        if let Some(payload) = mirrored(&state.pool, id).await
            && wanted(&payload)
        {
            return payload;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the mirror never showed {what} for page {id} within {INDEX_BUDGET:?}"
        );
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}

// -- the tests ---------------------------------------------------------------

/// **The three writes, through the queue, into the real Confluence** -- and
/// the one that matters most is the first: a section edit that leaves the rest
/// of the page exactly as it was.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs testenv's seeded Confluence: `just atlassian-live`"]
async fn the_three_page_writes_go_through_the_queue_and_come_back_from_confluence() {
    let env = env();
    let edited = env.page(EDITED);
    let parent = env.page(PARENT);
    let mut litter = Litter::default();

    let (before, base_version) = env.body_and_version(&edited.id).await;
    assert!(
        before.contains("<h2>Backoff policy</h2>"),
        "the seeded page is not the one this suite edits: {before}"
    );
    // Recorded **before** the write, so the guard can undo an edit that landed
    // and then failed its assertion.
    litter.body = Some((edited.id.clone(), edited.title.clone(), before.clone()));

    let state = app("confluence_live_writes", &env).await;
    sync(&state).await;

    // -- a section edit ------------------------------------------------------
    //
    // The body knobas would compose: the whole page, with the first section's
    // prose replaced. Spelled here as the string it is, so what this asserts
    // about the server is the *bytes* and not a re-derivation of them.
    let section = "<h2>Backoff policy</h2><p>";
    let start = before.find(section).expect("the seeded first section") + section.len();
    let end = before[start..].find("</p>").expect("its paragraph closes") + start;
    let edit = format!("base 45 s, factor 3 -- {LITTER}.");
    let whole = format!("{}{edit}{}", &before[..start], &before[end..]);

    let sent = write(
        &state,
        json!({
            "UpdatePage": {
                "entity": format!("{CONFLUENCE}:{}", edited.id),
                "base_version": base_version,
                "body": whole,
            }
        }),
    )
    .await;
    assert_eq!(
        sent.state,
        WriteState::Sent,
        "the section edit did not land: {:?} {:?}",
        sent.state,
        sent.detail
    );

    let (after, after_version) = env.body_and_version(&edited.id).await;
    assert_eq!(
        after, whole,
        "Confluence stored something other than the body knobas sent"
    );
    assert_eq!(
        after_version,
        base_version + 1,
        "the edit did not advance the version by exactly one"
    );
    // The rest of the page, intact: the second section is untouched, character
    // for character.
    let tail = &before[end..];
    assert!(
        after.ends_with(tail),
        "the rest of the page did not survive"
    );
    assert!(after.contains(&edit), "the edit is not in the page");

    // -- a comment -----------------------------------------------------------
    let words = format!("does the SLA still hold? -- {LITTER}");
    let commented = write(
        &state,
        json!({
            "Comment": { "entity": format!("{CONFLUENCE}:{}", edited.id), "body": words },
        }),
    )
    .await;
    assert_eq!(
        commented.state,
        WriteState::Sent,
        "the comment did not land: {:?}",
        commented.detail
    );

    // Read back through the mirror, at the path the detail reads it from, with
    // the author and instant `EXPAND` now asks for (#286).
    let payload = sync_until(&state, &edited.id, "the new comment", |payload| {
        payload["children"]["comment"]["results"]
            .as_array()
            .is_some_and(|rows| {
                rows.iter().any(|row| {
                    row["body"]["storage"]["value"]
                        .as_str()
                        .is_some_and(|v| v.contains(LITTER))
                })
            })
    })
    .await;
    let mine = payload["children"]["comment"]["results"]
        .as_array()
        .expect("the comments are a list")
        .iter()
        .find(|row| {
            row["body"]["storage"]["value"]
                .as_str()
                .is_some_and(|v| v.contains(LITTER))
        })
        .expect("the comment this suite posted")
        .clone();
    litter.created.push(
        mine["id"]
            .as_str()
            .expect("the comment has an id")
            .to_owned(),
    );
    assert_eq!(
        mine["body"]["storage"]["value"].as_str(),
        Some(format!("<p>{words}</p>").as_str()),
        "the adapter's text-to-storage rendering is not what the server stored"
    );
    assert_eq!(
        mine["version"]["by"]["username"].as_str(),
        Some(env.user.as_str()),
        "the widened EXPAND did not bring the comment's author: {mine}"
    );
    assert!(
        mine["version"]["when"].as_str().is_some(),
        "the widened EXPAND did not bring the comment's instant: {mine}"
    );

    // -- a created page ------------------------------------------------------
    let title = format!("Standup 2026-09-03 [{LITTER}]");
    let created = write(
        &state,
        json!({
            "CreatePage": {
                "parent": format!("{CONFLUENCE}:{}", parent.id),
                "space": env.space,
                "title": title,
                "body": "<h2>Yesterday</h2><p>the payout retry.</p>",
            }
        }),
    )
    .await;
    assert_eq!(
        created.state,
        WriteState::Sent,
        "the page was not created: {:?}",
        created.detail
    );
    // The queue row does not carry the id -- `remote_id` is stamped on the
    // `write_queue` table and is not one of the columns `QueuedWrite` reads
    // (#280 put it there for the worklog's local copy). So the page is found
    // the way knobas itself finds a created thing: by reading it back. The
    // title is this suite's own and is unique in the space.
    let (status, found) = env
        .call(
            reqwest::Method::GET,
            &format!(
                "rest/api/content?spaceKey={}&type=page&title={}",
                env.space,
                url_encode(&title)
            ),
            None,
        )
        .await;
    assert_eq!(status, 200, "searching for the new page: {found}");
    let new_id = found["results"][0]["id"]
        .as_str()
        .unwrap_or_else(|| panic!("no page titled {title:?} came back: {found}"))
        .to_owned();
    litter.created.push(new_id.clone());

    // **Under its parent**, which is the claim `CreatePage::parent` exists to
    // make -- read from Confluence's own ancestors, not from what was sent.
    let record = env.content(&new_id, "ancestors,body.storage").await;
    let ancestors: Vec<&str> = record["ancestors"]
        .as_array()
        .expect("the new page has ancestors")
        .iter()
        .filter_map(|a| a["id"].as_str())
        .collect();
    assert!(
        ancestors.contains(&parent.id.as_str()),
        "the new page is not under {}: {ancestors:?}",
        parent.title
    );
    assert_eq!(
        record["body"]["storage"]["value"].as_str(),
        Some("<h2>Yesterday</h2><p>the payout retry.</p>"),
        "the storage format was not stored verbatim"
    );

    drop(state);
}

/// **An edit made against a version the mirror has passed is held** --
/// `CONTEXT.md`'s held write, with both versions in the row and nothing sent.
///
/// The sequence is the one a person really hits: knobas mirrors the page,
/// somebody edits it in Confluence, the source is unreachable for a moment so
/// the write waits, a sync brings the new version in, and the flush that
/// follows finds the target has moved.
///
/// The source is made unreachable by **disabling nothing and pointing at
/// nothing**: the write is queued while the page is still at the version it
/// was read at, which is what makes the snapshot the edit's own -- then the
/// out-of-band edit and one more sync are what move the target under it.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs testenv's seeded Confluence: `just atlassian-live`"]
async fn an_edit_made_against_a_version_the_mirror_has_passed_is_held() {
    let env = env();
    let edited = env.page(EDITED);
    let mut litter = Litter::default();

    let (before, base_version) = env.body_and_version(&edited.id).await;
    litter.body = Some((edited.id.clone(), edited.title.clone(), before.clone()));

    let state = app("confluence_live_hold", &env).await;
    sync_until(&state, &edited.id, "the seeded body", |payload| {
        payload["version"]["number"].as_i64() == Some(base_version)
    })
    .await;

    // Queued against a source that cannot take it yet, so the row waits rather
    // than flushing straight through: the credential is removed for the moment
    // the write is submitted. This is #42's *pending* state, reached the way a
    // person reaches it (their token expired), and it is what leaves a write on
    // the queue for a later flush to reconsider.
    knobas_secrets::spawn::delete(&state.secrets, CONFLUENCE)
        .await
        .expect("the credential is removed");
    let mine = format!("{before}<p>knobas was here -- {LITTER}.</p>");
    let queued = write(
        &state,
        json!({
            "UpdatePage": {
                "entity": format!("{CONFLUENCE}:{}", edited.id),
                "base_version": base_version,
                "body": mine,
            }
        }),
    )
    .await;
    assert_eq!(
        queued.state,
        WriteState::Pending,
        "the write should be waiting on the credential, not settled: {:?}",
        queued.detail
    );

    // Somebody else edits the page, and a sync brings that in.
    env.put_body(
        &edited.id,
        &edited.title,
        &format!("{before}<p>somebody else was here -- {LITTER}.</p>"),
        base_version,
    )
    .await;
    sync_until(&state, &edited.id, "the out-of-band edit", |payload| {
        payload["version"]["number"].as_i64() == Some(base_version + 1)
    })
    .await;

    // The credential comes back, and the flush finds the target moved.
    knobas_secrets::spawn::put(
        &state.secrets,
        CONFLUENCE,
        Secret {
            kind: AuthMethod::UserPassword,
            value: env.password.clone(),
        },
    )
    .await
    .expect("the credential is back");
    knobas_sync::write_queue::flush_source(state.scheduler.deps(), CONFLUENCE)
        .await
        .expect("the flush runs");

    let held = knobas_core::write_queue::get(&state.pool, queued.id)
        .await
        .expect("the queue row is readable")
        .expect("the row is still there");
    assert_eq!(
        held.state,
        WriteState::Held,
        "an edit over a page that moved on must be held, not sent: {:?}",
        held.detail
    );
    // **Both versions**, which is what the reader is shown side by side.
    assert_eq!(
        held.target_snapshot["payload"]["version"]["number"].as_i64(),
        Some(base_version),
        "the queued snapshot is not the version the edit was made against"
    );
    assert_eq!(
        held.held_snapshot.as_ref().expect("a held snapshot")["payload"]["version"]["number"]
            .as_i64(),
        Some(base_version + 1),
        "the held snapshot is not the version the mirror now holds"
    );
    // The reason is the target's, not the disabled source's (#204): the source
    // is on by now, and the two explanations are never collapsed.
    assert!(held.source_enabled);

    // And nothing knobas queued reached the page: the body is the one the other
    // writer left.
    let (now, _) = env.body_and_version(&edited.id).await;
    assert!(
        now.contains("somebody else was here") && !now.contains("knobas was here"),
        "a held write reached the server"
    );

    drop(state);
}

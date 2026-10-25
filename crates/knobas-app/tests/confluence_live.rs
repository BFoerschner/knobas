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
//! * **An edit made against a version the server has passed is refused**, with
//!   Confluence's own sentence, and the other writer's text is still on the
//!   page. That is the abort-on-conflict backstop at the wire. Its sibling --
//!   the *hold*, which fires when the **mirror** moves between queue and flush
//!   -- is witnessed offline in `crates/knobas-sync/tests/it/write_queue.rs`, and
//!   the second test below says at length why it cannot be witnessed here.
//! * **A comment lands on a page**, as the SPI's own `Comment` op with the
//!   page as its container, and comes back at `children.comment` where the
//!   detail reads it -- with the author and instant `EXPAND` now asks for.
//! * **A created page appears under its parent**, which is the one claim
//!   `CreatePage`'s `parent` field exists to make.
//! * **Moved notes become pages under the picked parent** (#941), one per
//!   note, and the notes stay.
//!
//! # What this file writes, and what it takes away
//!
//! One seeded page's body, one comment, and one page. All three are undone in
//! [`Litter`]'s `Drop` -- passing or panicking alike, through
//! `knobas_live_cleanup::Cleanup` -- and the removal is checked rather than
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

mod live_digest;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use knobas_app::sources::{Registry, SourcesState};
use knobas_core::write_queue::WriteState;
use knobas_live_cleanup::Cleanup;
use knobas_secrets::{MemoryStore, Secret, SecretStore};
use knobas_source::AuthMethod;
use knobas_sync::scheduler::{RefreshEnd, RefreshEnded, Scheduler, SchedulerDeps, SyncEvents};
use live_digest::{Connections, Quiet, sync};
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
    let seeded = &whole["confluence"];
    let pages: Vec<SeededPage> = serde_json::from_value(seeded["pages"].clone())
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
        space: seeded["space"].as_str().unwrap_or("ENG").to_owned(),
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

/// Run one cleanup through the shared runner, `knobas_live_cleanup::Cleanup`,
/// and report what it could not undo in this suite's words.
fn undo<F, Fut>(what: &str, cleanup: F)
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<(), String>>,
{
    Cleanup::new(
        format!(
            "the live suite did not take back {what}, so the server is no longer in the \
             plain-seed state -- `./seed-atlassian-content.sh` is idempotent and puts a page \
             body back"
        ),
        format!("{what} may still be standing"),
    )
    .run(cleanup);
}

// -- the app, wired the way the app wires it --------------------------------

/// A `SourcesState` over a database of this test's own, with the seeded
/// Confluence configured and its credential in the (in-memory) keychain.
///
/// User and password, which is what the setup wizard's admin account has: it
/// holds no personal access token, and `live_confluence_seeded.rs` records why
/// a suite must not draw its 401s from a wrong password.
async fn app(name: &str, env: &Env) -> SourcesState {
    app_with(name, env, None).await
}

/// [`app`], or, given `events`, the app's own scheduler
/// (`knobas_app::sources::start_scheduler`) telling them: there a landed
/// write is followed as in the app, by its refresh and then a run (#2094,
/// ADR-0031).
async fn app_with(
    name: &str,
    env: &Env,
    events: Option<&dyn Fn(sqlx::PgPool) -> Arc<dyn SyncEvents>>,
) -> SourcesState {
    let connector = knobas_db::test_util::scratch_database(name).await;
    let pool = connector
        .pool(4)
        .await
        .expect("a pool onto the scratch database");
    let secrets = Arc::new(MemoryStore::new());
    secrets
        .put(
            &knobas_secrets::KeychainAccount::source(CONFLUENCE),
            &Secret::just(AuthMethod::UserPassword, env.password.clone()),
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

    let deps = |events: Arc<dyn SyncEvents>| SchedulerDeps {
        pool: pool.clone(),
        connections: Arc::new(Connections(connector.clone())),
        registry: Arc::new(Registry::for_process()),
        secrets: secrets.clone(),
        events,
        timing: knobas_sync::scheduler::SchedulerTiming::default(),
        quit: knobas_sync::scheduler::Quit::default(),
    };
    let scheduler = match events {
        None => Scheduler::start(deps(Arc::new(Quiet))).await,
        Some(events) => {
            knobas_app::sources::start_scheduler(
                deps(events(pool.clone())),
                knobas_sync::background::Background::new(),
            )
            .await
        }
    }
    .expect("a scheduler over the scratch database");

    SourcesState {
        pool,
        scheduler,
        secrets,
        registry: Arc::new(Registry::for_process()),
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
        sync(state, CONFLUENCE).await;
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
    sync(&state, CONFLUENCE).await;

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
        after_version,
        base_version + 1,
        "the edit did not advance the version by exactly one"
    );
    // **The rest of the page, intact** -- the criterion, and asserted as the
    // two halves rather than only as the whole, so a failure says *which* half
    // moved. Should these hold while the whole-body assertion below does not,
    // the server normalised storage format it was handed: that is a finding
    // about the product to write down before any assertion here is changed,
    // never a run to repeat (ADR-0013).
    assert!(
        after.starts_with(&before[..start]),
        "everything before the edited section did not survive"
    );
    assert!(
        after.ends_with(&before[end..]),
        "everything after the edited section did not survive"
    );
    assert!(after.contains(&edit), "the edit is not in the page");
    assert_eq!(
        after, whole,
        "Confluence stored something other than the body knobas sent"
    );

    // **And it reads back through knobas**, not only through a raw REST call:
    // the assertions above are about what the server stored, and this one is
    // about the round trip the reader actually makes -- adapter, sync engine,
    // mirror, and the payload the detail re-renders the page from.
    let mirrored_after = sync_until(&state, &edited.id, "the edited body", |payload| {
        payload["version"]["number"].as_i64() == Some(base_version + 1)
    })
    .await;
    assert_eq!(
        mirrored_after["body"]["storage"]["value"].as_str(),
        Some(whole.as_str()),
        "the mirror does not hold the body the edit wrote"
    );

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

    // Read back **through the adapter's own expansion** first: the comment
    // is on the page, it is stored as the storage format the adapter
    // rendered, and it carries the author and the instant `EXPAND` asks for.
    // Those two fields are #286's widening, and this is the only place the
    // real server confirms it. The mirror comes after, below.
    let record = env
        .content(&edited.id, knobas_source_confluence::EXPAND_FOR_TESTS)
        .await;
    let mine = record["children"]["comment"]["results"]
        .as_array()
        .expect("the page's comments came back expanded")
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
    // Cleared by #347's sweep rather than rewritten: `version.by` and
    // `version.when` ride on the one `children.comment.version` token in
    // `api::EXPAND`, so this cannot fail while the assertion above passes --
    // it is a second reading of the same widening, not a second witness, and
    // it is kept because the two fields are read by different callers.
    assert!(
        mine["version"]["when"].as_str().is_some(),
        "the widened EXPAND did not bring the comment's instant: {mine}"
    );

    // **And the mirror has it**, from the sync the write's own re-read runs
    // and the incremental ones after it while the index catches up (#2073).
    // A comment does not move its page's `lastmodified`, so the page walk
    // cannot bring it; until #2073 the second walk took only comments that
    // mention this account, which this one deliberately does not, and the
    // thread showed it only after the next page edit.
    let comment_id = mine["id"]
        .as_str()
        .expect("the comment has an id")
        .to_owned();
    // **The receipt names that comment** (#2074): the settle stamped the id
    // Confluence answered the write with, witnessed here by Confluence's own
    // read of the page's thread rather than by the receipt itself. It is what
    // Docs drops its queued comment by once the words were edited.
    let stamped: Option<String> =
        sqlx::query_scalar("select remote_id from knobas.write_queue where id = $1")
            .bind(commented.id)
            .fetch_one(&state.pool)
            .await
            .expect("the comment's queue row");
    assert_eq!(
        stamped.as_deref(),
        Some(comment_id.as_str()),
        "the comment write's row is not stamped with the id Confluence gave the comment"
    );
    let mirrored_after = sync_until(&state, &edited.id, "the comment knobas posted", |payload| {
        payload["children"]["comment"]["results"]
            .as_array()
            .is_some_and(|thread| thread.iter().any(|c| c["id"] == json!(comment_id)))
    })
    .await;
    assert_eq!(
        mirrored_after["version"]["number"].as_i64(),
        Some(base_version + 1),
        "the page was edited again, so the page walk could have brought the comment"
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

/// **An edit made against a version the server has passed is refused by
/// Confluence** — the abort-on-conflict backstop, at the wire, which is the
/// half of the version guard nothing offline can measure.
///
/// # Why the *held* half is not here, and where it is instead
///
/// The ticket's fourth criterion has two mechanisms in it, and they are caught
/// in two places on purpose.
///
/// The **hold** is knobas' own: it fires when the mirror moves between the
/// moment a write is queued and the moment it flushes, and it is witnessed by
/// `crates/knobas-sync/tests/it/write_queue.rs`'s
/// `a_page_whose_version_moved_past_the_edit_holds_it_with_both_versions`,
/// against a real Postgres, a real queue and a real flush.
///
/// It cannot be witnessed *here*, and the reason is a fact about the app
/// rather than a gap in the suite: the scheduler drains the write queue every
/// `TICK` — five seconds — so producing a hold live would need a window in
/// which the write is undeliverable while a sync is still able to run, and
/// there is no such window. Removing the credential stops the flush and the
/// sync together; leaving it in place lets a drain fire before Confluence's
/// search index has caught up, and the write goes. Manufacturing the window by
/// writing `sync.item` by hand is exactly what the offline test does, better,
/// with no container in the way.
///
/// What is left is the half only the server can answer, and it was worth
/// coming here for: `base_version + 1` against a page that has already
/// advanced is a **409**, it reaches the queue as `Refused` carrying
/// Confluence's own sentence rather than as a retry (ADR-0004), and — the
/// point of the whole guard — the other writer's text is still on the page.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs testenv's seeded Confluence: `just atlassian-live`"]
async fn an_edit_made_against_a_version_the_server_has_passed_is_refused_by_confluence() {
    let env = env();
    let edited = env.page(EDITED);
    let mut litter = Litter::default();

    let (before, base_version) = env.body_and_version(&edited.id).await;
    litter.body = Some((edited.id.clone(), edited.title.clone(), before.clone()));

    let state = app("confluence_live_conflict", &env).await;
    sync_until(&state, &edited.id, "the seeded body", |payload| {
        payload["version"]["number"].as_i64() == Some(base_version)
    })
    .await;

    // Somebody else edits the page. knobas does **not** sync afterwards, so the
    // mirror still holds `base_version` and the queue has nothing to hold
    // against -- which is the window the queue cannot see and the server can.
    let theirs = format!("{before}<p>somebody else was here -- {LITTER}.</p>");
    env.put_body(&edited.id, &edited.title, &theirs, base_version)
        .await;

    let mine = format!("{before}<p>knobas was here -- {LITTER}.</p>");
    let refused = write(
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
        refused.state,
        WriteState::Refused,
        "an edit over a version the server has passed must be refused, not sent or kept: {:?}",
        refused.detail
    );
    // **A refusal, not a wait** (ADR-0004): a 409 is a decision, so the queue
    // must never go round again over it.
    //
    // `assert_eq!(refused.wait_reason, None)` stood here until #347 and could
    // not fail: migration `0005`'s `write_queue_reason_state_chk check
    // (wait_reason is null or state = 'pending')` puts a refused row carrying
    // a wait reason outside what any implementation can store, so once the
    // assertion above pinned the state, this one was Postgres restating
    // itself. What it *meant* -- a decided write is never sent again -- is
    // what is asserted instead, in the two places it is visible.
    //
    // **First, the offer.** `write_queue::due` is the whole of the decision:
    // whatever it hands back is what the next flush sends. Asked directly,
    // because the row alone cannot answer it -- measured under a mutant during
    // #347's live window, widening `due`'s outer filter to
    // `state in ('pending','refused')` re-sent this very write to Confluence
    // and drew a second live 409, and *every column of the row was unchanged
    // afterwards*, because `refuse`'s own `and state = 'pending'` guard means
    // no transition matches a settled row. A test that watched only the row
    // saw a redelivery it could not report.
    let still_due = knobas_core::write_queue::due(&state.pool, CONFLUENCE)
        .await
        .expect("the queue's due list reads");
    assert!(
        !still_due.iter().any(|w| w.id == refused.id),
        "a refused write is terminal: the queue still offers it to the next flush, so a decision \
         the server already made would be re-sent for ever: {:?}",
        still_due
            .iter()
            .map(|w| (w.id, w.state))
            .collect::<Vec<_>>()
    );
    // **Then the end-to-end half**: flush again and confirm nothing moved.
    knobas_sync::write_queue::flush_source(state.scheduler.deps(), CONFLUENCE)
        .await
        .expect("a second flush of the source runs");
    let again = knobas_core::write_queue::get(&state.pool, refused.id)
        .await
        .expect("the queue row is readable")
        .expect("the row the refusal settled");
    assert_eq!(
        (again.state, again.attempts, again.wait_reason),
        (refused.state, refused.attempts, refused.wait_reason),
        "a refused write is terminal: a flush moved the settled row, which is what a retryable \
         classification would do to a person's edit for ever: {again:?}"
    );
    let detail = refused.detail.clone().unwrap_or_default();
    println!("SEEDED refused edit: {detail}");
    assert!(
        detail.contains("409") || detail.to_lowercase().contains("version"),
        "the refusal must carry Confluence's own sentence about the version: {detail:?}"
    );

    // And the point of the guard: the other writer's text is still there and
    // knobas' is not.
    let (now, now_version) = env.body_and_version(&edited.id).await;
    assert_eq!(now, theirs, "the refused write reached the page anyway");
    assert_eq!(
        now_version,
        base_version + 1,
        "the refused write moved the version"
    );

    drop(state);
}

/// **The Docs reader's blocks, from a page Confluence itself stored** (#859,
/// ADR-0022): each mapped macro comes back from the real server's storage
/// format as its block, a macro the reader does not draw is the named
/// placeholder, and the declarations around the body -- the space, the
/// parent chain, the version, the labels and a mention of the account --
/// resolve on the record the adapter really fetched.
///
/// The page is made through Confluence's REST API, not through knobas, so the
/// storage format is the server's own after its own normalisation; it is
/// deleted again by [`Litter`].
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs testenv's seeded Confluence: `just atlassian-live`"]
async fn a_page_with_every_mapped_macro_reads_as_the_readers_blocks() {
    use knobas_source::docs::{DocBlock, Inline, PanelKind};

    let env = env();
    let parent = env.page(PARENT);
    let mut litter = Litter::default();

    let (status, me) = env
        .call(reqwest::Method::GET, "rest/api/user/current", None)
        .await;
    assert_eq!(status, 200, "who am I: {me}");
    let my_key = me["userKey"]
        .as_str()
        .expect("the account has a user key")
        .to_owned();

    let title = format!("Docs reader blocks [{LITTER}]");
    let storage = format!(
        concat!(
            r#"<p>Owner <ac:link><ri:user ri:userkey="{key}" /></ac:link>, see "#,
            r#"<ac:link><ri:page ri:content-title="{parent}" /></ac:link> and "#,
            r#"<ac:structured-macro ac:name="jira" ac:schema-version="1"><ac:parameter ac:name="key">PAY-231</ac:parameter></ac:structured-macro>.</p>"#,
            r#"<h2>Backoff</h2>"#,
            r#"<ac:structured-macro ac:name="code" ac:schema-version="1"><ac:parameter ac:name="language">sql</ac:parameter><ac:plain-text-body><![CDATA[select 1 < 2;]]></ac:plain-text-body></ac:structured-macro>"#,
            r#"<ac:structured-macro ac:name="info" ac:schema-version="1"><ac:rich-text-body><p>In review.</p></ac:rich-text-body></ac:structured-macro>"#,
            r#"<ac:structured-macro ac:name="warning" ac:schema-version="1"><ac:rich-text-body><p>No SLA yet.</p></ac:rich-text-body></ac:structured-macro>"#,
            r#"<ac:structured-macro ac:name="note" ac:schema-version="1"><ac:rich-text-body><p>A note.</p></ac:rich-text-body></ac:structured-macro>"#,
            r#"<ac:structured-macro ac:name="tip" ac:schema-version="1"><ac:rich-text-body><p>A tip.</p></ac:rich-text-body></ac:structured-macro>"#,
            r#"<table><tbody><tr><th>Code</th><th>Retried?</th></tr><tr><td>HTTP 503</td><td>yes</td></tr></tbody></table>"#,
            r#"<ac:structured-macro ac:name="toc" ac:schema-version="1" />"#,
        ),
        key = my_key,
        parent = parent.title,
    );
    let (status, made) = env
        .call(
            reqwest::Method::POST,
            "rest/api/content",
            Some(json!({
                "type": "page",
                "title": title,
                "space": { "key": env.space },
                "ancestors": [{ "id": parent.id }],
                "body": { "storage": { "value": storage, "representation": "storage" } },
            })),
        )
        .await;
    assert_eq!(status, 200, "creating the page: {made}");
    let id = made["id"].as_str().expect("the new page's id").to_owned();
    litter.created.push(id.clone());
    let (status, labelled) = env
        .call(
            reqwest::Method::POST,
            &format!("rest/api/content/{id}/label"),
            Some(json!([{ "prefix": "global", "name": "knobas-docs" }])),
        )
        .await;
    assert_eq!(status, 200, "labelling the page: {labelled}");

    let state = app("confluence_live_docs", &env).await;
    sync_until(&state, &id, "the new page with its label", |payload| {
        payload["metadata"]["labels"]["results"]
            .as_array()
            .is_some_and(|l| l.iter().any(|x| x["name"] == "knobas-docs"))
    })
    .await;

    let entity = format!("{CONFLUENCE}:{id}");
    let page = knobas_app::commands::entity::docs_page_inner(
        &state.pool,
        &Registry::for_process(),
        &entity,
    )
    .await
    .expect("the reader reads the page");
    assert!(page.readable);
    assert_eq!(
        page.space.as_ref().map(|s| s.key.as_str()),
        Some(env.space.as_str())
    );
    assert_eq!(
        page.ancestors.last().map(|a| a.title.as_str()),
        Some(parent.title.as_str()),
        "the declared chain ends at the parent"
    );
    assert_eq!(
        page.ancestors.last().and_then(|a| a.entity_id.clone()),
        Some(format!("{CONFLUENCE}:{}", parent.id)),
        "the parent is the mirrored page"
    );
    assert_eq!(page.version, Some(1));
    assert_eq!(page.labels, vec!["knobas-docs"]);
    assert!(page.web_url.is_some());

    let DocBlock::Paragraph { text } = &page.blocks[0] else {
        panic!("the first block is the paragraph: {:#?}", page.blocks)
    };
    assert!(
        text.iter().any(
            |i| matches!(i, Inline::Mention { user: Some(u), me: true, .. } if *u == env.user)
        ),
        "the mention of the account resolves by key to it, and is mine: {text:?}"
    );
    assert!(
        text.iter().any(|i| matches!(i, Inline::PageLink { entity_id: Some(e), .. } if *e == format!("{CONFLUENCE}:{}", parent.id))),
        "the page link names the mirrored parent: {text:?}"
    );
    assert!(
        text.iter()
            .any(|i| matches!(i, Inline::Issue { key, .. } if key == "PAY-231")),
        "the Jira macro is a ticket chip: {text:?}"
    );
    let rest: Vec<&DocBlock> = page.blocks.iter().skip(1).collect();
    assert!(matches!(rest[0], DocBlock::Heading { .. }), "{rest:#?}");
    assert_eq!(
        rest[1],
        &DocBlock::Code {
            code: "select 1 < 2;".into(),
            language: Some("sql".into())
        }
    );
    let tones: Vec<PanelKind> = rest
        .iter()
        .filter_map(|b| match b {
            DocBlock::Panel { kind, .. } => Some(*kind),
            _ => None,
        })
        .collect();
    assert_eq!(
        tones,
        vec![
            PanelKind::Info,
            PanelKind::Warning,
            PanelKind::Note,
            PanelKind::Tip
        ]
    );
    let DocBlock::Table { rows } = rest[6] else {
        panic!("the table: {rest:#?}")
    };
    assert!(rows[0].iter().all(|c| c.header) && !rows[1][0].header);
    assert_eq!(
        rest[7],
        &DocBlock::Placeholder { name: "toc".into() },
        "the unknown macro is named"
    );

    // The tree: the new page hangs under its parent.
    let tree = knobas_app::commands::entity::docs_tree_inner(&state.pool, &Registry::for_process())
        .await
        .expect("the tree reads");
    let node = tree
        .spaces
        .iter()
        .flat_map(|s| &s.pages)
        .find(|p| p.entity_id == entity)
        .expect("the new page is in the tree");
    assert_eq!(node.parent, Some(format!("{CONFLUENCE}:{}", parent.id)));

    drop(state);
}

/// **The Docs search's sync-time marks against the real Confluence** (#860,
/// #861): a page made through Confluence's own REST API, with a heading, a
/// link to the seeded parent and a comment, syncs into a `body_text` whose
/// section heading sits beside its words and whose page link is extracted.
/// So the search's snippet names its section (and a comment's author), the
/// parent reads the new page as a backlink under *Linked from*, and the plain
/// section reads as editable while the linking one is refused.
///
/// What Confluence normalises (the storage format it hands back, the comment's
/// author record) is what the adapter read; the page is deleted again by
/// [`Litter`].
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs testenv's seeded Confluence: `just atlassian-live`"]
async fn a_synced_page_keeps_its_sections_and_links_for_search_and_backlinks() {
    let env = env();
    let parent = env.page(PARENT);
    let mut litter = Litter::default();
    let word = format!("zq{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);

    let title = format!("Docs search marks {word} [{LITTER}]");
    let storage = format!(
        concat!(
            r#"<p>Lead text.</p>"#,
            r#"<h2>Backoff policy</h2>"#,
            r#"<p>Retries stop after five {word} attempts. Then the batch is parked.</p>"#,
            r#"<h2>See also</h2>"#,
            r#"<p>The parent: <ac:link><ri:page ri:content-title="{parent}" /></ac:link>.</p>"#,
        ),
        word = word,
        parent = parent.title,
    );
    let (status, made) = env
        .call(
            reqwest::Method::POST,
            "rest/api/content",
            Some(json!({
                "type": "page",
                "title": title,
                "space": { "key": env.space },
                "ancestors": [{ "id": parent.id }],
                "body": { "storage": { "value": storage, "representation": "storage" } },
            })),
        )
        .await;
    assert_eq!(status, 200, "creating the page: {made}");
    let id = made["id"].as_str().expect("the new page's id").to_owned();
    litter.created.push(id.clone());
    let (status, commented) = env
        .call(
            reqwest::Method::POST,
            "rest/api/content",
            Some(json!({
                "type": "comment",
                "container": { "id": id, "type": "page" },
                "body": { "storage": { "value": format!("<p>Is {word} the right cap?</p>"), "representation": "storage" } },
            })),
        )
        .await;
    assert_eq!(status, 200, "commenting: {commented}");

    let state = app("confluence_live_docs_search", &env).await;
    sync_until(&state, &id, "the new page with its comment", |payload| {
        payload["children"]["comment"]["results"]
            .as_array()
            .is_some_and(|c| !c.is_empty())
    })
    .await;

    let entity = format!("{CONFLUENCE}:{id}");
    let found = knobas_app::commands::entity::docs_search_inner(
        &state.pool,
        &Registry::for_process(),
        &format!("{word} attempts"),
        None,
    )
    .await
    .expect("the search runs")
    .expect("words are a query");
    let hit = found
        .hits
        .iter()
        .find(|h| h.entity_id == entity)
        .unwrap_or_else(|| panic!("the new page is a hit: {found:#?}"));
    assert_eq!(
        hit.snippet.section.as_deref(),
        Some("Backoff policy"),
        "the section heading was kept beside the indexed text at sync"
    );
    let snippet: String = hit
        .snippet
        .segments
        .iter()
        .map(|s| s.text.as_str())
        .collect();
    assert_eq!(snippet, format!("Retries stop after five {word} attempts."));
    assert_eq!(
        hit.space.as_ref().map(|s| s.key.as_str()),
        Some(env.space.as_str())
    );

    let found = knobas_app::commands::entity::docs_search_inner(
        &state.pool,
        &Registry::for_process(),
        &format!("{word} cap"),
        None,
    )
    .await
    .expect("the search runs")
    .expect("words are a query");
    let hit = found
        .hits
        .iter()
        .find(|h| h.entity_id == entity)
        .expect("the comment's words find the page");
    assert!(
        hit.snippet
            .comment_by
            .as_deref()
            .is_some_and(|by| !by.is_empty()),
        "the comment's author was kept beside its words: {:?}",
        hit.snippet
    );

    // The link to the parent was extracted at sync: the parent reads the new
    // page as a backlink under *Linked from*.
    let parent_page = knobas_app::commands::entity::docs_page_inner(
        &state.pool,
        &Registry::for_process(),
        &format!("{CONFLUENCE}:{}", parent.id),
    )
    .await
    .expect("the parent reads");
    assert!(
        parent_page.backlinks.iter().any(|b| b.entity_id == entity),
        "the new page links here: {:?}",
        parent_page.backlinks
    );

    // The sections as Confluence normalised them: the plain one is editable,
    // the one holding a page link is not.
    let page = knobas_app::commands::entity::docs_page_inner(
        &state.pool,
        &Registry::for_process(),
        &entity,
    )
    .await
    .expect("the page reads");
    assert_eq!(page.sections.len(), 2, "{:?}", page.sections);
    assert_eq!(page.sections[0].refusal, None);
    assert_eq!(
        page.sections[0].text.as_deref(),
        Some(
            format!("Retries stop after five {word} attempts. Then the batch is parked.").as_str()
        )
    );
    assert_eq!(
        page.sections[1].refusal,
        Some(knobas_source::docs::SectionRefusal::Macro)
    );
    assert!(page.writes.iter().any(|w| w == "update_page"));

    drop(state);
}

/// **Edit section keeps a panel** (R-10.13, #975): a section of text and an
/// info panel opens in the editor with the panel as a quote naming its kind,
/// is saved through the queue with the panel's text changed, and comes back
/// from Confluence as an info panel, with its title, holding the new text.
/// The page is a new one, deleted again by [`Litter`].
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs testenv's seeded Confluence: `just atlassian-live`"]
async fn an_edited_section_keeps_its_info_panel_in_confluence() {
    use knobas_source::docs::{DocBlock, PanelKind};
    let env = env();
    let parent = env.page(PARENT);
    let mut litter = Litter::default();
    let word = format!("zq{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);

    let title = format!("Panel section {word} [{LITTER}]");
    let storage = concat!(
        r#"<h2>Rollout</h2><p>Lead.</p>"#,
        r#"<ac:structured-macro ac:name="info">"#,
        r#"<ac:parameter ac:name="title">Before you deploy</ac:parameter>"#,
        r#"<ac:rich-text-body><p>Drain the queue.</p></ac:rich-text-body>"#,
        r#"</ac:structured-macro>"#,
    );
    let (status, made) = env
        .call(
            reqwest::Method::POST,
            "rest/api/content",
            Some(json!({
                "type": "page",
                "title": title,
                "space": { "key": env.space },
                "ancestors": [{ "id": parent.id }],
                "body": { "storage": { "value": storage, "representation": "storage" } },
            })),
        )
        .await;
    assert_eq!(status, 200, "creating the page: {made}");
    let id = made["id"].as_str().expect("the new page's id").to_owned();
    litter.created.push(id.clone());

    let state = app("confluence_live_panel_section", &env).await;
    sync_until(&state, &id, "the new page", |p| {
        p["version"]["number"].as_i64() == Some(1)
    })
    .await;
    let entity = format!("{CONFLUENCE}:{id}");
    let page = knobas_app::commands::entity::docs_page_inner(
        &state.pool,
        &Registry::for_process(),
        &entity,
    )
    .await
    .expect("the page reads");
    assert_eq!(page.sections.len(), 1, "{:?}", page.sections);
    assert_eq!(
        page.sections[0].refusal, None,
        "a panel section is editable"
    );
    assert_eq!(
        page.sections[0].text.as_deref(),
        Some("Lead.\n\n> **Info** Before you deploy\n>\n> Drain the queue.")
    );

    let edited = format!(
        "Lead.\n\n> **Info** Before you deploy\n>\n> Drain the {word} queue.\n>\n> - then deploy"
    );
    let payload = knobas_app::commands::entity::docs_edit_section_payload(
        &state.pool,
        &Registry::for_process(),
        &entity,
        1,
        page.version,
        &edited,
    )
    .await
    .expect("the section edit is made");
    let sent = write(&state, payload).await;
    assert_eq!(
        sent.state,
        WriteState::Sent,
        "the section edit did not land: {:?}",
        sent.detail
    );

    // What Confluence stored: an info macro, its title, the new text inside.
    let (after, version) = env.body_and_version(&id).await;
    assert_eq!(version, 2, "{after}");
    assert!(
        after.contains(r#"ac:name="info""#),
        "no info macro: {after}"
    );
    assert!(
        after.contains(r#"<ac:parameter ac:name="title">Before you deploy</ac:parameter>"#),
        "the title was lost: {after}"
    );
    let inside = after
        .split_once("<ac:rich-text-body>")
        .and_then(|(_, rest)| rest.split_once("</ac:rich-text-body>"))
        .map(|(inside, _)| inside)
        .unwrap_or_else(|| panic!("the panel has no body: {after}"));
    assert!(
        inside.contains(&format!("Drain the {word} queue."))
            && inside.contains("<li>then deploy</li>"),
        "the new text is not inside the panel: {after}"
    );

    // And through knobas: the reader draws an info panel, and the section
    // opens with the text that was saved.
    sync_until(&state, &id, "the edited body", |p| {
        p["version"]["number"].as_i64() == Some(2)
    })
    .await;
    let page = knobas_app::commands::entity::docs_page_inner(
        &state.pool,
        &Registry::for_process(),
        &entity,
    )
    .await
    .expect("the page reads");
    let panels: Vec<(PanelKind, Option<&str>)> = page
        .blocks
        .iter()
        .filter_map(|b| match b {
            DocBlock::Panel { kind, title, .. } => Some((*kind, title.as_deref())),
            _ => None,
        })
        .collect();
    assert_eq!(panels, [(PanelKind::Info, Some("Before you deploy"))]);
    assert_eq!(page.sections[0].text.as_deref(), Some(edited.as_str()));

    drop(state);
}

/// **Move to Confluence…** (#941, D10): each note becomes a page under the
/// picked parent, its Markdown in storage format, and the notes stay. Two
/// notes, one with an empty body, since a note often has none and the wiki
/// has to take that page too.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs testenv's seeded Confluence: `just confluence-live`"]
async fn moved_notes_become_pages_under_the_picked_parent() {
    let env = env();
    let parent = env.page(PARENT);
    let mut litter = Litter::default();
    let state = app("confluence_live_notes_move", &env).await;
    sync(&state, CONFLUENCE).await;

    let written = format!("Payout retries [{LITTER}]");
    let empty = format!("Empty note [{LITTER}]");
    knobas_core::note::create(
        &state.pool,
        &written,
        "Why we retry.\n\n## Steps\n- park the batch",
        &[],
        "user",
    )
    .await
    .unwrap();
    knobas_core::note::create(&state.pool, &empty, "", &[], "user")
        .await
        .unwrap();

    let payloads = knobas_app::commands::entity::move_notes_payloads(
        &state.pool,
        &Registry::for_process(),
        &format!("{CONFLUENCE}:{}", parent.id),
    )
    .await
    .expect("the parent is a mirrored page");
    assert_eq!(payloads.len(), 2);
    for (payload, _) in payloads {
        let sent = write(&state, payload).await;
        assert_eq!(
            sent.state,
            WriteState::Sent,
            "a note's page was not created: {:?}",
            sent.detail
        );
    }

    for (title, body) in [
        (
            &written,
            "<p>Why we retry.</p><h2>Steps</h2><ul><li>park the batch</li></ul>",
        ),
        (&empty, ""),
    ] {
        let (status, found) = env
            .call(
                reqwest::Method::GET,
                &format!(
                    "rest/api/content?spaceKey={}&type=page&title={}",
                    env.space,
                    url_encode(title)
                ),
                None,
            )
            .await;
        assert_eq!(status, 200, "searching for {title:?}: {found}");
        let id = found["results"][0]["id"]
            .as_str()
            .unwrap_or_else(|| panic!("no page titled {title:?} came back: {found}"))
            .to_owned();
        litter.created.push(id.clone());
        let record = env.content(&id, "ancestors,body.storage").await;
        let ancestors: Vec<&str> = record["ancestors"]
            .as_array()
            .expect("the new page has ancestors")
            .iter()
            .filter_map(|a| a["id"].as_str())
            .collect();
        assert!(
            ancestors.contains(&parent.id.as_str()),
            "{title:?} is not under {}: {ancestors:?}",
            parent.title
        );
        assert_eq!(
            record["body"]["storage"]["value"].as_str(),
            Some(body),
            "{title:?} was not stored as the note's Markdown in storage format"
        );
    }
    assert_eq!(
        knobas_core::note::all(&state.pool).await.unwrap().len(),
        2,
        "the notes stay in knobas"
    );

    drop(state);
}

/// The account a mention of somebody else names (#953): a user nothing on the
/// test's page is by, created once and kept, like the seeded space.
const MENTIONED: &str = "knobas-mentioned";

/// That user's key, creating the user through Confluence's admin API the first
/// time.
async fn mentioned_user_key(env: &Env) -> String {
    let path = format!("rest/api/user?username={MENTIONED}");
    let (mut status, mut user) = env.call(reqwest::Method::GET, &path, None).await;
    if status == 404 {
        let (made, answer) = env
            .call(
                reqwest::Method::POST,
                "rest/api/admin/user",
                Some(json!({
                    "userName": MENTIONED,
                    "fullName": "Mentioned Person",
                    // Confluence refuses an address in the reserved
                    // `.invalid` domain as "Invalid email address".
                    "email": "mentioned@example.com",
                    "password": "knobas-mentioned-953",
                    "notifyViaEmail": false,
                })),
            )
            .await;
        assert!(
            matches!(made, 200 | 201 | 204),
            "creating {MENTIONED}: {made} {answer}"
        );
        (status, user) = env.call(reqwest::Method::GET, &path, None).await;
    }
    assert_eq!(status, 200, "GET {path}: {user}");
    user["userKey"]
        .as_str()
        .unwrap_or_else(|| panic!("{MENTIONED} has a user key: {user}"))
        .to_owned()
}

/// Move page `id` to just before its sibling `before`, the way the page
/// tree's drag does. Data Center 9.2 has no REST call for it
/// (`rest/api/content/{id}/move/…` answers 404 there), so this is the page
/// tree's own action, with the header that waives its XSRF token for a
/// basic-auth caller. Each attempt's answer is printed, so a Confluence that
/// moved the action says where.
async fn move_before(env: &Env, id: &str, before: &str) {
    let http = client();
    let mut tried = Vec::new();
    for method in [reqwest::Method::POST, reqwest::Method::GET] {
        let response = http
            .request(method.clone(), format!("{}/pages/movepage.action", env.url))
            .query(&[("pageId", id), ("targetId", before), ("position", "above")])
            .header("X-Atlassian-Token", "no-check")
            .basic_auth(&env.user, Some(&env.password))
            .send()
            .await
            .unwrap_or_else(|e| panic!("{method} movepage.action: {e}"));
        let status = response.status().as_u16();
        let text = response.text().await.unwrap_or_default();
        println!(
            "{method} movepage.action -> {status}: {}",
            &text[..text.len().min(300)]
        );
        if (200..300).contains(&status) {
            return;
        }
        tried.push(format!("{method} -> {status}"));
    }
    panic!("moving {id} before {before}: {}", tried.join(", "));
}

/// **Every mention key resolves, and the tree is in Confluence's order**
/// (#953), against the real container. A page mentions a user who neither
/// made, edited nor commented on it, so only the sync's user lookup can name
/// them; its two children are made in title order and then the later title is
/// moved before the earlier one, so only `extensions.position` can put them
/// in the order Confluence shows. Then the earlier title is moved back, and
/// the next **incremental** sync has to bring that too (#1138): a move makes
/// no new version, so only the positions sweep reaches it.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs testenv's seeded Confluence: `just atlassian-live`"]
async fn a_mention_of_anyone_resolves_and_children_keep_confluences_order() {
    use knobas_source::docs::{DocBlock, Inline};

    let env = env();
    let parent = env.page(PARENT);
    let mut litter = Litter::default();
    let key = mentioned_user_key(&env).await;

    let create = |title: String, under: String, storage: String| {
        let env = &env;
        async move {
            let (status, made) = env
                .call(
                    reqwest::Method::POST,
                    "rest/api/content",
                    Some(json!({
                        "type": "page",
                        "title": title,
                        "space": { "key": env.space },
                        "ancestors": [{ "id": under }],
                        "body": { "storage": { "value": storage, "representation": "storage" } },
                    })),
                )
                .await;
            assert_eq!(status, 200, "creating {title}: {made}");
            made["id"].as_str().expect("the new page's id").to_owned()
        }
    };
    let top = create(
        format!("Ordered parent [{LITTER}]"),
        parent.id.clone(),
        format!(r#"<p>Ask <ac:link><ri:user ri:userkey="{key}" /></ac:link>.</p>"#),
    )
    .await;
    litter.created.push(top.clone());
    let alpha = create(
        format!("Alpha child [{LITTER}]"),
        top.clone(),
        "<p>a</p>".into(),
    )
    .await;
    litter.created.insert(0, alpha.clone());
    let zeta = create(
        format!("Zeta child [{LITTER}]"),
        top.clone(),
        "<p>z</p>".into(),
    )
    .await;
    litter.created.insert(0, zeta.clone());
    move_before(&env, &zeta, &alpha).await;
    for id in [&alpha, &zeta] {
        let record = env.content(id, "version").await;
        println!("page {id}: extensions = {}", record["extensions"]);
    }

    let state = app("confluence_live_order", &env).await;
    sync_until(&state, &top, "the mentioned user beside the page", |p| {
        p["mentioned_users"].is_array()
    })
    .await;
    for id in [&alpha, &zeta] {
        sync_until(&state, id, "the child", serde_json::Value::is_object).await;
    }

    let page = knobas_app::commands::entity::docs_page_inner(
        &state.pool,
        &Registry::for_process(),
        &format!("{CONFLUENCE}:{top}"),
    )
    .await
    .expect("the reader reads the page");
    let DocBlock::Paragraph { text } = &page.blocks[0] else {
        panic!("the first block is the paragraph: {:#?}", page.blocks)
    };
    assert!(
        text.iter().any(|i| matches!(
            i,
            Inline::Mention { user: Some(u), name: Some(n), me: false }
                if u == MENTIONED && n == "Mentioned Person"
        )),
        "the mention names the user the page record does not: {text:?}"
    );

    let tree = knobas_app::commands::entity::docs_tree_inner(&state.pool, &Registry::for_process())
        .await
        .expect("the tree reads");
    let parent_id = format!("{CONFLUENCE}:{top}");
    let children: Vec<String> = tree
        .spaces
        .iter()
        .flat_map(|s| &s.pages)
        .filter(|p| p.parent.as_deref() == Some(parent_id.as_str()))
        .map(|p| p.entity_id.clone())
        .collect();
    assert_eq!(
        children,
        vec![
            format!("{CONFLUENCE}:{zeta}"),
            format!("{CONFLUENCE}:{alpha}")
        ],
        "Zeta was moved before Alpha, and the tree says so"
    );

    // #1138: a reorder alone reaches an **incremental** run. The source has a
    // cursor now, so the next sync is bounded by `lastmodified` -- and a move
    // makes no new version, so only the positions sweep can see it.
    let cursor: Option<String> =
        sqlx::query_scalar("select cursor from knobas.source_config where id = $1")
            .bind(CONFLUENCE)
            .fetch_one(&state.pool)
            .await
            .expect("the source row is readable");
    let cursor = cursor.expect("the first sync left a cursor, so the next one is incremental");
    assert!(
        cursor.contains("\"positions\""),
        "the cursor records the places: {cursor}"
    );
    let before = env.content(&alpha, "version").await["version"]["number"].clone();
    move_before(&env, &alpha, &zeta).await;
    let after = env.content(&alpha, "version").await;
    assert_eq!(
        after["version"]["number"], before,
        "a move makes no new version, which is the whole of #1138: {after}"
    );
    let alpha_at = sync_until(&state, &alpha, "Alpha's new place", |p| {
        p["position"] == json!(0)
    })
    .await;
    let zeta_at = mirrored(&state.pool, &zeta)
        .await
        .expect("Zeta is mirrored");
    assert_eq!(
        (alpha_at["position"].clone(), zeta_at["position"].clone()),
        (json!(0), json!(1)),
        "both siblings took their new places from one incremental run"
    );
    let tree = knobas_app::commands::entity::docs_tree_inner(&state.pool, &Registry::for_process())
        .await
        .expect("the tree reads");
    let children: Vec<String> = tree
        .spaces
        .iter()
        .flat_map(|s| &s.pages)
        .filter(|p| p.parent.as_deref() == Some(parent_id.as_str()))
        .map(|p| p.entity_id.clone())
        .collect();
    assert_eq!(
        children,
        vec![
            format!("{CONFLUENCE}:{alpha}"),
            format!("{CONFLUENCE}:{zeta}")
        ],
        "Alpha was moved back before Zeta in Confluence alone, and the tree says so"
    );

    drop(state);
}

/// **A pasted address opens its page in every form Confluence serves it
/// under** (#1475): the stored `_links.webui`, `viewpage.action?pageId=`,
/// `/display/<space>/<title>` and the tiny link, against a mirror the real
/// sync filled.
///
/// Each address is fetched from the server first, with the seed's account, and
/// must answer the page itself (`ajs-page-id`): a form the descriptor declares
/// is a claim about Confluence, and this is where the claim meets it. The
/// tiny link is the one `_links.tinyui` the sync stored, which is also what
/// says the record carries it. Reads only; the shared fixture is untouched.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs testenv's seeded Confluence: `just confluence-live`"]
async fn a_pasted_address_opens_its_page_in_every_form_confluence_serves() {
    let env = env();
    let page = env.page(PARENT);
    let state = app("confluence_live_url_forms", &env).await;
    sync(&state, CONFLUENCE).await;

    let entity_id = format!("{CONFLUENCE}:{}", page.id);
    let payload = mirrored(&state.pool, &page.id)
        .await
        .unwrap_or_else(|| panic!("{entity_id} is not mirrored after a full sync"));
    let tiny = payload
        .pointer("/_links/tinyui")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_else(|| panic!("{entity_id}'s record carries no `_links.tinyui`: {payload}"));
    let stored: String = sqlx::query_scalar("select web_url from sync.item where entity_id = $1")
        .bind(&entity_id)
        .fetch_one(&state.pool)
        .await
        .expect("the page's stored address");

    let url = &env.url;
    let addresses = [
        stored,
        format!("{url}/pages/viewpage.action?pageId={}", page.id),
        format!(
            "{url}/display/{}/{}",
            env.space,
            url_encode(&page.title).replace("%20", "+")
        ),
        format!("{url}{tiny}"),
    ];
    for address in &addresses {
        let response = client()
            .get(address)
            .basic_auth(&env.user, Some(&env.password))
            .send()
            .await
            .unwrap_or_else(|e| panic!("GET {address}: {e}"));
        let status = response.status().as_u16();
        let html = response.text().await.unwrap_or_default();
        assert!(
            status == 200 && html.contains(&format!(r#"name="ajs-page-id" content="{}""#, page.id)),
            "Confluence does not serve page {} at {address} (HTTP {status})",
            page.id
        );

        let found =
            knobas_app::commands::entity::resolve_url_inner(&state.pool, &*state.registry, address)
                .await
                .expect("a web URL is resolved, not refused");
        assert_eq!(
            found.map(|m| (m.entity_id, m.kind)),
            Some((entity_id.clone(), "page".to_owned())),
            "{address} must open {entity_id}"
        );
        eprintln!("resolved: {address} -> {entity_id}");
    }

    drop(state);
}

// -- a landed write's page, refreshed before the run after it (#2096) --------

/// What the mirror held the moment a refresh ended: the payload of the item
/// refreshed, and how many runs of its source had started since the write it
/// followed settled.
struct Snapshot {
    ended: RefreshEnded,
    payload: Option<serde_json::Value>,
    runs_since_the_write: i64,
}

/// Hears `item:refreshed` and reads the mirror **inside** the call. It is
/// told before the run after the write is asked for
/// (`sources::write_queue::after_landing`), so what it reads is what the
/// refresh wrote, not what that run did; the run count checks it rather than
/// trusting the order.
#[derive(Clone)]
struct Snapshots {
    pool: sqlx::PgPool,
    taken: Arc<std::sync::Mutex<Vec<Snapshot>>>,
}

impl SyncEvents for Snapshots {
    fn sync_state(&self, _status: knobas_sync::SourceSyncStatus) {}
    fn source_health(&self, _health: knobas_sync::CredentialHealth) {}
    fn activity_new(&self, _row: knobas_core::activity::ActivityRow) {}
    fn refresh_ended(&self, ended: &RefreshEnded) {
        let pool = self.pool.clone();
        let id = ended.entity_id.clone();
        let write = ended.write_id;
        let (payload, runs_since_the_write) = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                let payload: Option<serde_json::Value> =
                    sqlx::query_scalar("select payload from sync.live_item where entity_id = $1")
                        .bind(&id)
                        .fetch_optional(&pool)
                        .await
                        .expect("the mirror is readable");
                let runs: i64 = sqlx::query_scalar(
                    "select count(*) from knobas.sync_run r, knobas.write_queue w
                      where w.id = $1 and r.source_id = w.source_id
                        and r.started_at > w.settled_at",
                )
                .bind(write)
                .fetch_one(&pool)
                .await
                .expect("the runs are readable");
                (payload, runs)
            })
        });
        self.taken.lock().unwrap().push(Snapshot {
            ended: ended.clone(),
            payload,
            runs_since_the_write,
        });
    }
}

impl Snapshots {
    /// The snapshot of write `id`'s refresh, waited for: the refresh follows
    /// the send in the background.
    async fn of(&self, id: i64) -> Snapshot {
        let deadline = std::time::Instant::now() + Duration::from_secs(60);
        loop {
            {
                let mut taken = self.taken.lock().unwrap();
                if let Some(at) = taken.iter().position(|s| s.ended.write_id == Some(id)) {
                    return taken.remove(at);
                }
            }
            assert!(
                std::time::Instant::now() < deadline,
                "write {id} landed but no refresh of it ended within a minute"
            );
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }
}

/// The id Confluence answered write `id` with, as the settle stamped it.
async fn remote_id(pool: &sqlx::PgPool, id: i64) -> String {
    sqlx::query_scalar::<_, Option<String>>(
        "select remote_id from knobas.write_queue where id = $1",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .expect("the queue row")
    .unwrap_or_else(|| panic!("write {id} carries no remote id"))
}

/// **A section edit, a comment and a new page, each through the write
/// queue, are in the mirror as Confluence holds them before the run after
/// the write starts** (#2096, ADR-0031): the refresh reads the page by id,
/// so it needs neither a run nor the search index to have caught up.
///
/// The comment is found in the refreshed thread by the id Confluence gave
/// it (#2074), and the new page by the id its receipt names.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs testenv's seeded Confluence: `just atlassian-live`"]
async fn a_landed_writes_page_is_mirrored_as_confluence_holds_it_before_the_run_after_it() {
    let env = env();
    let edited = env.page(EDITED);
    let parent = env.page(PARENT);
    let mut litter = Litter::default();

    let (before, base_version) = env.body_and_version(&edited.id).await;
    litter.body = Some((edited.id.clone(), edited.title.clone(), before.clone()));

    let taken = Arc::new(std::sync::Mutex::new(Vec::new()));
    let heard = std::sync::Mutex::new(None);
    let state = app_with(
        "confluence_live_refresh",
        &env,
        Some(&|pool: sqlx::PgPool| -> Arc<dyn SyncEvents> {
            let snapshots = Snapshots {
                pool,
                taken: Arc::clone(&taken),
            };
            *heard.lock().unwrap() = Some(snapshots.clone());
            Arc::new(snapshots)
        }),
    )
    .await;
    let snapshots = heard.lock().unwrap().take().expect("the events were built");
    sync(&state, CONFLUENCE).await;
    let page_entity = format!("{CONFLUENCE}:{}", edited.id);

    // -- a section edit ------------------------------------------------------
    let section = "<h2>Backoff policy</h2><p>";
    let start = before.find(section).expect("the seeded first section") + section.len();
    let end = before[start..].find("</p>").expect("its paragraph closes") + start;
    let whole = format!(
        "{}base 60 s, factor 2 -- {LITTER}.{}",
        &before[..start],
        &before[end..]
    );
    let sent = write(
        &state,
        json!({ "UpdatePage": {
            "entity": page_entity,
            "base_version": base_version,
            "body": whole,
        } }),
    )
    .await;
    assert_eq!(sent.state, WriteState::Sent, "{:?}", sent.detail);
    let shot = snapshots.of(sent.id).await;
    assert_eq!(shot.ended.entity_id, page_entity);
    assert_eq!(shot.ended.ended, RefreshEnd::Refreshed);
    assert_eq!(
        shot.runs_since_the_write, 0,
        "a run started before the refresh ended"
    );
    let (held, held_version) = env.body_and_version(&edited.id).await;
    let payload = shot.payload.expect("the edited page is mirrored");
    assert_eq!(payload["version"]["number"].as_i64(), Some(held_version));
    assert_eq!(held_version, base_version + 1);
    assert_eq!(
        payload["body"]["storage"]["value"].as_str(),
        Some(held.as_str())
    );
    assert_eq!(held, whole, "Confluence holds the body the edit sent");

    // -- a comment -----------------------------------------------------------
    let commented = write(
        &state,
        json!({ "Comment": {
            "entity": page_entity,
            "body": format!("read back before the run? -- {LITTER}"),
        } }),
    )
    .await;
    assert_eq!(commented.state, WriteState::Sent, "{:?}", commented.detail);
    let comment_id = remote_id(&state.pool, commented.id).await;
    litter.created.push(comment_id.clone());
    let shot = snapshots.of(commented.id).await;
    assert_eq!(
        shot.ended.entity_id, page_entity,
        "a comment refreshes its page"
    );
    assert_eq!(shot.ended.ended, RefreshEnd::Refreshed);
    assert_eq!(
        shot.runs_since_the_write, 0,
        "a run started before the refresh ended"
    );
    let thread = shot.payload.expect("the page is mirrored")["children"]["comment"]["results"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let mine = thread
        .iter()
        .find(|c| c["id"] == json!(comment_id))
        .unwrap_or_else(|| panic!("comment {comment_id} is not in the refreshed thread"));
    let record = env
        .content(&edited.id, knobas_source_confluence::EXPAND_FOR_TESTS)
        .await;
    let theirs = record["children"]["comment"]["results"]
        .as_array()
        .and_then(|t| t.iter().find(|c| c["id"] == json!(comment_id)))
        .expect("Confluence holds the comment")
        .clone();
    assert_eq!(mine["body"], theirs["body"], "as Confluence holds it");
    assert_eq!(mine["version"]["number"], theirs["version"]["number"]);

    // -- a created page ------------------------------------------------------
    let title = format!("Refreshed standup [{LITTER}]");
    let created = write(
        &state,
        json!({ "CreatePage": {
            "parent": format!("{CONFLUENCE}:{}", parent.id),
            "space": env.space,
            "title": title,
            "body": "<p>in the mirror before the run.</p>",
        } }),
    )
    .await;
    assert_eq!(created.state, WriteState::Sent, "{:?}", created.detail);
    let new_id = remote_id(&state.pool, created.id).await;
    litter.created.push(new_id.clone());
    let shot = snapshots.of(created.id).await;
    assert_eq!(
        shot.ended.entity_id,
        format!("{CONFLUENCE}:{new_id}"),
        "a created page is refreshed by the id its receipt names"
    );
    assert_eq!(shot.ended.ended, RefreshEnd::Refreshed);
    assert_eq!(
        shot.runs_since_the_write, 0,
        "a run started before the refresh ended"
    );
    let payload = shot.payload.expect("the new page is mirrored");
    let theirs = env.content(&new_id, "ancestors,body.storage,version").await;
    assert_eq!(payload["title"], json!(title));
    assert_eq!(payload["body"]["storage"], theirs["body"]["storage"]);
    assert_eq!(payload["version"]["number"], theirs["version"]["number"]);
    assert!(
        payload["ancestors"]
            .as_array()
            .is_some_and(|a| a.iter().any(|p| p["id"] == json!(parent.id))),
        "under {}: {}",
        parent.title,
        payload["ancestors"]
    );

    state.scheduler.shutdown().await;
}

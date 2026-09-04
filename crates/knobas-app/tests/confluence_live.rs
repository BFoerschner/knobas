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
//!   -- is witnessed offline in `crates/knobas-sync/tests/write_queue.rs`, and
//!   the second test below says at length why it cannot be witnessed here.
//! * **A comment lands on a page**, as the SPI's own `Comment` op with the
//!   page as its container, and comes back at `children.comment` where the
//!   detail reads it -- with the author and instant `EXPAND` now asks for.
//! * **A created page appears under its parent**, which is the one claim
//!   `CreatePage`'s `parent` field exists to make.
//!
//! # What this file writes, and what it takes away
//!
//! One seeded page's body, one comment, and one page. All three are undone in
//! [`Litter`]'s `Drop` -- passing or panicking alike, on a
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

mod live_digest;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use knobas_app::sources::{Registry, SourcesState};
use knobas_core::write_queue::WriteState;
use knobas_secrets::{MemoryStore, Secret, SecretStore};
use knobas_source::AuthMethod;
use knobas_sync::scheduler::{Scheduler, SchedulerDeps};
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

    // Read back **through the adapter's own expansion**, not through the
    // mirror -- and the reason is a measured fact about this product rather
    // than a shortcut.
    //
    // A comment is separate content: posting one does not move its page's
    // `lastmodified`, which is what the page walk's CQL matches, so no
    // incremental sync can reach a page whose only change is a fresh comment
    // (contract §4.2 D, "mentions"; #287's own live suite prints the two
    // identical timestamps that prove it). The one path that *does* reach one
    // is the mention query, and it reaches only comments that mention this
    // account -- which this one deliberately does not, because a suite that
    // could only assert a comment it had also @-mentioned itself in would be
    // asserting the mention feature rather than the write.
    //
    // So what is asserted here is the write and the expansion: the comment is
    // on the page, it is stored as the storage format the adapter rendered,
    // and it carries the author and the instant `EXPAND` asks for. Those two
    // fields are #286's widening, and this is the only place the real server
    // confirms it.
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
/// `crates/knobas-sync/tests/write_queue.rs`'s
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

//! The adapter against the **seeded, real Confluence Data Center** that
//! `testenv/seed-atlassian-content.sh` fills with the Tidewater dataset
//! (issue #284, ADR-0013).
//!
//! This adapter has never had a mock and never will. Atlassian publishes no
//! machine-readable specification for Confluence DC, which is exactly why
//! ADR-0013 refused a `knobas-mockd` half for it: a mock built from reading
//! the documentation would have been this crate's own assumptions checked
//! against themselves. **This file is the only witness there is.** Every claim
//! the crate's unit tests make about a response shape -- that a content search
//! answers `_links.next` and no total, that `version.when` carries the
//! instance's UTC offset, that `children.comment` is where an expanded
//! discussion lands -- is a claim re-made here against Confluence itself.
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
//! cd .. && env -u RUSTUP_TOOLCHAIN cargo test -p knobas-source-confluence \
//!     --test live_confluence_seeded -- --ignored --nocapture --test-threads=1
//! ```
//!
//! # What this file asserts that nothing else can
//!
//! * **The seeded corpus by content**: every fixture page of the ENG space, at
//!   the ids Confluence assigned, with the comments the seed posted, the
//!   storage format the seed built, and the space key and name at the paths
//!   ADR-0010's census reads them from.
//! * **Battery clause 2 against a real search index**: an incremental run
//!   after nothing changed emits nothing and hands back the byte-identical
//!   cursor. Nothing else in the repo can measure that for this adapter.
//! * **A rename keeps the id.** The suite renames a seeded page through
//!   Confluence's own REST API and restores it afterwards; the next
//!   incremental run returns *that page*, under the *same* entity id, and the
//!   watermark advances to that page's own `version.when` and no further --
//!   which is `CONTEXT.md`'s **Watermark** rule and the criterion at once.
//! * **What a real Confluence does with a credential it does not accept**,
//!   measured rather than assumed. The Jira certification (#276) found that a
//!   bad *bearer* token there never reaches Seraph and lets `/search` answer
//!   **200 with an empty result set** -- which on an exhaustive kind is a
//!   licence to tombstone the mirror. The same question is asked here, at the
//!   wire, and `the_search_that_could_read_as_an_empty_corpus_is_never_the_first_call`
//!   is what stops the answer mattering.
//! * **The `_links.next` walk**, driven with a page size of two over a corpus
//!   of five, so the continuation link is followed for real and the
//!   expansions are checked to survive it.
//!
//! # What this file writes, and what it takes away
//!
//! One **title**, on one seeded page, changed to make it the *renamed* page of
//! the incremental test and changed back when the test ends -- passing or
//! panicking alike, from [`Renamed`]'s `Drop`, on a thread with a runtime of
//! its own, and checked afterwards rather than assumed. A title is the
//! smallest edit that moves `version.when` while touching no field the
//! fixture's *content* describes, and it is the one edit that makes the
//! id-not-title criterion visible.
//!
//! What a *killed* run left behind is put back by the next one:
//! [`Seeded::clear_leftovers`] restores every seeded page's title from
//! `seed-state.json`, which is the union of what this suite writes. Every test
//! that asserts an exact set calls it before taking its baseline. Recovery
//! from a dirty environment is "run the suite again".
//!
//! **One owner at a time**, as for the seeded Jira and TeamCity
//! (`testenv/README.md`): the exact-set assertions and clause 2 mean nothing
//! while somebody else is writing to this Confluence.
//!
//! # A red run here is never answered by running it again
//!
//! When the adapter and the server disagree, the **adapter** is wrong. A
//! failure here names a defect in this crate, or a product that has changed --
//! and in the second case the change is written down before the assertion is.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use knobas_source::contract::{Fault, VecSink, battery};
use knobas_source::instance::SourceInstance;
use knobas_source::{AuthMethod, Source, SourceError, SyncItem};

/// How long one HTTP exchange with the container may take. A localhost
/// Confluence answers a content search in tens of milliseconds; a request that
/// reaches thirty seconds is not coming back.
const REQUEST_BUDGET: Duration = Duration::from_secs(30);

/// How long Confluence's search index may lag an edit made through its REST
/// API.
///
/// CQL reads a Lucene index the write path updates asynchronously, so the run
/// that first sees an edit is not necessarily the one straight after it. Every
/// incremental assertion here polls to this deadline rather than sleeping a
/// guessed amount.
const INDEX_BUDGET: Duration = Duration::from_secs(90);

/// The marker this suite puts in the title it changes, so a person looking at
/// the server can tell whose litter it is.
///
/// A constant rather than a per-run salt, deliberately: the leftovers that
/// matter are the ones a *killed* run left, and a process that is gone cannot
/// be asked what it salted with. The cost is the one-owner rule, which this
/// environment is under anyway.
///
/// It is a marker and not the mechanism: [`Seeded::clear_leftovers`] restores
/// every title from `seed-state.json`, so a leftover that lost the marker is
/// reached anyway.
const LITTER_SUFFIX: &str = " [knobas-live-suite]";

/// The `confluence` block of `testenv/seed-state.json`: what the seed actually
/// got from the server.
#[derive(Debug, Clone, serde::Deserialize)]
struct Seed {
    /// The space key the seed created (`ENG`).
    space: String,
    /// The space's home page, which every seeded page sits under because the
    /// fixture names no ancestors of its own.
    home_page_id: String,
    /// The admin account every comment is authored by.
    author: String,
    /// One row per fixture page, with the id Confluence assigned.
    pages: Vec<SeededPage>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct SeededPage {
    /// The fixture's own id (`sepa-design`), for messages.
    fixture_id: String,
    title: String,
    id: String,
    comments: Vec<SeededComment>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct SeededComment {
    id: String,
    text: String,
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

/// Panics with the command to run rather than skipping: this suite is only
/// ever run by name, and a silent skip would read as a green certification of
/// nothing.
fn seeded() -> Seeded {
    let need = |key: &str| {
        std::env::var(key)
            .ok()
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| {
                panic!(
                    "{key} is not set -- this suite needs testenv's seeded Confluence. From the \
                     repo root: `just atlassian-live`, which stands the pair up, seeds it, runs \
                     every Atlassian-gated suite and tears it down again; or, inside a licence \
                     window already open, `eval \"$(cd testenv && ./seed --env)\"`"
                )
            })
    };
    let url = need("KNOBAS_CONFLUENCE_URL")
        .trim_end_matches('/')
        .to_owned();
    let user = need("KNOBAS_CONFLUENCE_USER");
    let password = need("KNOBAS_CONFLUENCE_PASSWORD");
    let state = std::env::var("KNOBAS_CONFLUENCE_SEED_STATE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testenv/seed-state.json")
        });
    let raw = std::fs::read_to_string(&state).unwrap_or_else(|e| {
        panic!(
            "{}: {e} -- `./seed-atlassian-content.sh` writes the title-to-id map this suite \
             reads (or point KNOBAS_CONFLUENCE_SEED_STATE at it)",
            state.display()
        )
    });
    let whole: serde_json::Value = serde_json::from_str(&raw).expect("seed-state.json is JSON");
    let seed: Seed = serde_json::from_value(whole["confluence"].clone()).unwrap_or_else(|e| {
        panic!(
            "{}: no `confluence` block with `space`, `home_page_id`, `author` and `pages` -- \
             run `./seed-atlassian-content.sh`: {e}",
            state.display()
        )
    });
    assert!(
        !seed.pages.is_empty(),
        "{}: the seed recorded no pages; `./seed-atlassian-content.sh` did not finish",
        state.display()
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
    /// One raw request, with the credential `./seed --env` prints.
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

    async fn get(&self, path_and_query: &str) -> (u16, serde_json::Value) {
        self.request(reqwest::Method::GET, path_and_query, None)
            .await
    }

    /// One page's record, with whatever `expand` asks for.
    async fn content(&self, id: &str, expand: &str) -> serde_json::Value {
        let (status, body) = self
            .get(&format!("rest/api/content/{id}?expand={expand}"))
            .await;
        assert_eq!(status, 200, "GET content {id}: {body}");
        body
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

    /// A source whose credential this Confluence will not accept, as a
    /// **bearer token** and never as a wrong password.
    ///
    /// The scheme is not incidental, and the reasoning is the Jira
    /// certification's (#276), transplanted because the product family is the
    /// same: a real Atlassian server counts failed *password* logins per
    /// account and, past a small threshold, starts refusing the **correct**
    /// password too until an administrator clears the check. A suite that drew
    /// its 401s from a wrong password would lock the seed's admin account out
    /// part-way through its own run and fail every test after it on a cause
    /// none of them names.
    ///
    /// A bearer token the server cannot resolve is not a login attempt and
    /// counts against nothing.
    /// [`a_rejected_credential_is_refused_and_the_seed_account_still_works`]
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
        match knobas_source_confluence::build(SourceInstance {
            id: "confluence".to_owned(),
            kind: knobas_source_confluence::ADAPTER_KIND.to_owned(),
            display_name: "Tidewater Confluence (seeded)".to_owned(),
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

    /// A source scoped to the seeded space -- what a real configuration looks
    /// like, and what keeps the assertions below about *this* corpus.
    fn scoped(&self) -> serde_json::Value {
        serde_json::json!({ "spaces": [self.seed.space.clone()] })
    }

    /// The ids of the seeded pages, ascending.
    fn seeded_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.seed.pages.iter().map(|p| p.id.clone()).collect();
        ids.sort();
        ids
    }

    fn seeded_page(&self, fixture_id: &str) -> &SeededPage {
        self.seed
            .pages
            .iter()
            .find(|p| p.fixture_id == fixture_id)
            .unwrap_or_else(|| panic!("seed-state.json records no page {fixture_id}"))
    }

    /// Put every seeded page's title back to what the seed created it with,
    /// and say what had to be put back -- **the recovery path**, since only a
    /// process that unwinds reaches a `Drop` and a run that was *killed*
    /// reaches none.
    ///
    /// Scoped by `seed-state.json` and by nothing a run remembers, which is
    /// the only way to reach the leftovers of a run that is gone. Read-only in
    /// the ordinary case: a clean server costs one request per seeded page.
    ///
    /// Not "sweep": `CONTEXT.md` spends that word on the engine pass that
    /// tombstones what a full sync no longer emitted.
    async fn clear_leftovers(&self) {
        let mut cleared: Vec<String> = Vec::new();
        for page in &self.seed.pages {
            let record = self.content(&page.id, "version").await;
            let title = record["title"].as_str().unwrap_or_default();
            if title == page.title {
                continue;
            }
            self.retitle(&page.id, &page.title).await;
            cleared.push(format!("{} back from {title:?}", page.id));
        }
        if cleared.is_empty() {
            return;
        }
        println!(
            "live suite: put back {} title(s) a run that was killed rather than failed left \
             behind: {}",
            cleared.len(),
            cleared.join("; ")
        );
    }

    /// Rename one page, keeping its body byte for byte.
    ///
    /// Confluence's content `PUT` **replaces** the record, so a request that
    /// omitted the body would blank the page -- this suite would then have
    /// destroyed the fixture it is asserting against. The body is read back
    /// and sent again for exactly that reason.
    async fn retitle(&self, id: &str, title: &str) {
        let current = self.content(id, "body.storage,version").await;
        let version = current["version"]["number"]
            .as_u64()
            .unwrap_or_else(|| panic!("page {id} has no version number: {current}"));
        let body = current["body"]["storage"]["value"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let (status, answer) = self
            .request(
                reqwest::Method::PUT,
                &format!("rest/api/content/{id}"),
                Some(serde_json::json!({
                    "id": id,
                    "type": "page",
                    "title": title,
                    "version": { "number": version + 1 },
                    "body": { "storage": { "value": body, "representation": "storage" } }
                })),
            )
            .await;
        assert_eq!(status, 200, "renaming {id} to {title:?}: {answer}");
    }
}

/// The one rename this suite makes, undone when the guard drops -- passing or
/// panicking alike -- and the removal checked.
struct Renamed {
    url: String,
    user: String,
    password: String,
    /// `(id, the title the seed created it with)`.
    page: Option<(String, String)>,
}

impl Renamed {
    /// Clears an earlier run's leftovers first, so the corpus a test measures
    /// its baseline over is already clean.
    async fn new(seeded: &Seeded) -> Renamed {
        seeded.clear_leftovers().await;
        Renamed {
            url: seeded.url.clone(),
            user: seeded.user.clone(),
            password: seeded.password.clone(),
            page: None,
        }
    }

    /// Append [`LITTER_SUFFIX`] to `id`'s title, and own the restoration from
    /// this line on. Returns the new title.
    async fn rename(&mut self, seeded: &Seeded, id: &str, original: &str) -> String {
        assert!(self.page.is_none(), "this guard owns exactly one edit");
        self.page = Some((id.to_owned(), original.to_owned()));
        let renamed = format!("{original}{LITTER_SUFFIX}");
        seeded.retitle(id, &renamed).await;
        renamed
    }
}

impl Drop for Renamed {
    fn drop(&mut self) {
        let Some((id, original)) = self.page.take() else {
            return;
        };
        let (url, user, password) = (self.url.clone(), self.user.clone(), self.password.clone());
        let (edited, title) = (id.clone(), original.clone());
        // `Drop` cannot await and runs on a tokio worker thread, so the
        // cleanup gets a thread with a runtime of its own -- and a client
        // built inside it, because a `reqwest::Client` driven from a second
        // runtime hangs rather than failing (the Gitea suite measured that).
        let report = std::thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("a runtime for the cleanup")
                .block_on(async move {
                    restore_title(&client(), &url, &user, &password, &edited, &title).await
                })
        })
        .join();
        let failure = match report {
            Ok(Ok(())) => return,
            Ok(Err(e)) => e,
            Err(_) => format!(
                "the cleanup thread panicked (its own message is on stderr), so page {id} may \
                 still be renamed; the next run's leftover clearing puts it back"
            ),
        };
        let report = format!(
            "the live suite did not undo the rename it made, so the server is no longer in the \
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

/// Put one page's title back and **check** it is back -- the check is what
/// keeps a container that stopped answering from reading as an edit undone.
async fn restore_title(
    http: &reqwest::Client,
    url: &str,
    user: &str,
    password: &str,
    id: &str,
    title: &str,
) -> Result<(), String> {
    let get = |expand: &str| {
        http.get(format!("{url}/rest/api/content/{id}?expand={expand}"))
            .header("Accept", "application/json")
            .basic_auth(user, Some(password))
            .send()
    };
    let response = get("body.storage,version")
        .await
        .map_err(|e| format!("GET content {id}: {e}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "GET content {id} -> {}: {}",
            response.status(),
            response.text().await.unwrap_or_default()
        ));
    }
    let current: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("GET content {id}: unreadable body: {e}"))?;
    let version = current["version"]["number"]
        .as_u64()
        .ok_or_else(|| format!("GET content {id}: no version number in {current}"))?;
    let body = current["body"]["storage"]["value"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let response = http
        .put(format!("{url}/rest/api/content/{id}"))
        .header("Accept", "application/json")
        .basic_auth(user, Some(password))
        .json(&serde_json::json!({
            "id": id,
            "type": "page",
            "title": title,
            "version": { "number": version + 1 },
            "body": { "storage": { "value": body, "representation": "storage" } }
        }))
        .send()
        .await
        .map_err(|e| format!("PUT content {id}: {e}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "PUT content {id} -> {}: {}",
            response.status(),
            response.text().await.unwrap_or_default()
        ));
    }
    let back: serde_json::Value = get("version")
        .await
        .map_err(|e| format!("GET content {id}: {e}"))?
        .json()
        .await
        .map_err(|e| format!("GET content {id}: unreadable body: {e}"))?;
    if back["title"].as_str() != Some(title) {
        return Err(format!(
            "{id} is still titled {:?} rather than {title:?}",
            back["title"]
        ));
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

fn ids(items: &[SyncItem]) -> Vec<String> {
    let mut ids: Vec<String> = items.iter().map(|i| i.entity.key.clone()).collect();
    ids.sort();
    ids
}

fn item<'a>(items: &'a [SyncItem], id: &str) -> &'a SyncItem {
    items
        .iter()
        .find(|i| i.entity.key == id)
        .unwrap_or_else(|| panic!("{id} is not in the run: {:?}", ids(items)))
}

/// The watermark inside the adapter's cursor.
fn modified_to(cursor: &str) -> chrono::DateTime<chrono::Utc> {
    let raw = serde_json::from_str::<serde_json::Value>(cursor)
        .ok()
        .and_then(|v| v["modified_to"].as_str().map(str::to_owned))
        .unwrap_or_else(|| panic!("not a cursor this adapter wrote: {cursor}"));
    raw.parse().unwrap_or_else(|e| panic!("{raw:?}: {e}"))
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

/// A username this Confluence does not have, for the one test that sends a
/// wrong password: a failed login under it counts against an account that does
/// not exist, so the seed's admin cannot be locked out by it
/// ([`Seeded::refused_source`] has the whole reasoning).
fn nobody() -> String {
    format!("knobas-live-nobody-{}", std::process::id())
}

// ---------------------------------------------------------------------------

/// The cleanup's "really back" check is a check only if a server that does not
/// answer reads as a failure rather than as an edit undone.
///
/// Witnessed on a port nothing listens on. Needs no container -- but it lives
/// here, and is `#[ignore]`d with the rest, because [`restore_title`] does and
/// because the claim it pins is [`Renamed`]'s. The seeded Jira and TeamCity
/// suites carry the same test for the same reason.
#[tokio::test]
#[ignore = "needs testenv's seeded Confluence: `just atlassian-live`"]
async fn the_cleanup_reports_a_server_it_cannot_reach_rather_than_calling_the_edit_undone() {
    let failure = restore_title(
        &client(),
        &dead_url(),
        "knobas",
        "irrelevant",
        "98307",
        "SEPA payout retry design",
    )
    .await
    .expect_err("a port nothing listens on is not an edit undone");
    assert!(failure.starts_with("GET content 98307: "), "{failure}");
}

/// *Test connection* against the server the seed set up: the account the
/// credential belongs to, which is the criterion.
///
/// The version is deliberately **absent**. Confluence DC publishes it through
/// `/rest/api/settings/systemInfo`, which is administrators-only, so an
/// adapter that reported one would report *unreachable* for an ordinary
/// account. `ConnectionInfo` is explicitly "an adapter fills only what its API
/// actually exposes".
#[tokio::test]
#[ignore = "needs testenv's seeded Confluence: `just atlassian-live`"]
async fn test_connection_names_the_seed_account() {
    let seeded = seeded();
    let info = seeded
        .source(seeded.scoped())
        .test_connection()
        .await
        .expect("the seeded server answers /rest/api/user/current");
    assert_eq!(
        info.account.as_deref(),
        Some(seeded.user.as_str()),
        "`/rest/api/user/current` names the account the credential belongs to: {info:?}"
    );
    assert_eq!(
        info.server_version, None,
        "the version endpoint is administrators-only, so this adapter says nothing rather than \
         something wrong"
    );
    assert_eq!(
        info.secret_expires_at, None,
        "PAT expiry needs /rest/pat/latest/tokens, which is outside this adapter's endpoint set"
    );
    println!(
        "SEEDED Confluence: account={:?} detail={:?}",
        info.account, info.detail
    );
}

/// **Every seeded page of the ENG space, by the id Confluence assigned.**
///
/// The exact set and nothing else: the `page` kind claims
/// `full_sync_exhaustive: true`, which is the engine's licence to tombstone
/// every row a `cursor: None` run did not return -- so a full sync that
/// quietly returned four of five pages would *delete* the fifth from the
/// mirror rather than merely sync fewer.
///
/// The space home page is deliberately **not** in the expected set: the seed
/// records the pages it created, the home page is the space's own, and this
/// assertion is about the fixture. It is checked to be present as an
/// *ancestor* instead, which is the thing the launcher path reads.
#[tokio::test]
#[ignore = "needs testenv's seeded Confluence: `just atlassian-live`"]
async fn a_full_sync_mirrors_every_seeded_page_of_the_space() {
    let seeded = seeded();
    seeded.clear_leftovers().await;
    let (items, cursor) = full(&*seeded.source(seeded.scoped())).await;

    let got = ids(&items);
    for id in seeded.seeded_ids() {
        assert!(
            got.contains(&id),
            "the seed created page {id} and the full sync did not return it: {got:?}"
        );
    }
    // The home page is the only thing the seed did not create and the space
    // does have, so this is the whole corpus of the space, not a window.
    assert!(
        got.len() <= seeded.seeded_ids().len() + 1,
        "the run returned pages the seed knows nothing about: {got:?}"
    );

    for it in &items {
        assert_eq!(it.entity.namespace, "confluence");
        assert_eq!(it.kind, "page", "{}", it.entity);
        assert!(!it.deleted, "a CQL search cannot report deletions");
        assert!(
            it.entity.key.chars().all(|c| c.is_ascii_digit()),
            "the entity key is Confluence's content id: {:?}",
            it.entity.key
        );
        // P5: the page Confluence itself named, on the URL the user typed.
        let web = it.web_url.as_deref().unwrap_or_else(|| {
            panic!(
                "{}: no web_url, so the detail view offers no Open in browser",
                it.entity
            )
        });
        assert!(web.starts_with(&seeded.url), "{web}");
        assert!(!it.title.trim().is_empty(), "{}", it.entity);
        assert!(
            it.body_text.starts_with(&it.title),
            "body_text is what FTS matches on and starts with the title: {:?}",
            it.body_text
        );
        // The record's own stamp, never `now()`.
        let stamped = it.payload["version"]["when"]
            .as_str()
            .unwrap_or_else(|| panic!("{}: no version.when on {}", it.entity, it.payload));
        assert_eq!(
            it.updated_at.map(|t| t.to_rfc3339()),
            chrono::DateTime::parse_from_rfc3339(stamped)
                .map(|t| t.with_timezone(&chrono::Utc).to_rfc3339())
                .ok(),
            "{}: {stamped}",
            it.entity
        );
    }

    assert!(
        modified_to(&cursor) <= chrono::Utc::now(),
        "a full sync's watermark is a page's own version time, never a clock reading"
    );
    println!(
        "SEEDED full sync: {} page(s) {:?}; cursor {cursor}",
        items.len(),
        got
    );
}

/// **The storage format verbatim in the payload, and the same body stripped to
/// text** -- the two halves of the criterion, on the page the fixture gives a
/// body to.
///
/// The verbatim half is what the next ticket renders; the stripped half is
/// what FTS indexes and what the detail view shows today. Asserted through the
/// *adapter*, so what is certified is what reaches the mirror.
#[tokio::test]
#[ignore = "needs testenv's seeded Confluence: `just atlassian-live`"]
async fn the_storage_format_is_verbatim_in_the_payload_and_stripped_in_the_body() {
    let seeded = seeded();
    seeded.clear_leftovers().await;
    let design = seeded.seeded_page("sepa-design").clone();
    let (items, _) = full(&*seeded.source(seeded.scoped())).await;
    let it = item(&items, &design.id);

    // What the server itself holds, fetched raw -- so "verbatim" is measured
    // against Confluence and not against the adapter's own reading of it.
    let record = seeded.content(&design.id, "body.storage").await;
    let storage = record["body"]["storage"]["value"]
        .as_str()
        .expect("the seeded page has a storage body");
    assert_eq!(
        it.payload["body"]["storage"]["value"], storage,
        "the payload keeps the storage format byte for byte"
    );
    assert_eq!(it.payload["body"]["storage"]["representation"], "storage");
    assert!(
        storage.contains("<h2>") && storage.contains("<p>"),
        "the seed builds <h2>/<p> sections, so this page is a real test of the stripper: \
         {storage}"
    );

    // The normalized half: the same words, no markup, headings on their own
    // lines.
    assert!(!it.body_text.contains('<'), "{:?}", it.body_text);
    assert!(
        it.body_text.contains("Backoff policy"),
        "the heading survives as text: {:?}",
        it.body_text
    );
    assert!(
        it.body_text.contains("TEMP_UNAVAILABLE"),
        "the <code> span's word survives, unbroken: {:?}",
        it.body_text
    );
    println!(
        "SEEDED storage ({} bytes) -> body_text:\n{}",
        storage.len(),
        it.body_text
    );
}

/// **The comments the seed posted, in the payload the way Jira's are** -- at
/// Confluence's own `children.comment` path, by the ids Confluence assigned,
/// and reaching `body_text` so the discussion is searchable.
#[tokio::test]
#[ignore = "needs testenv's seeded Confluence: `just atlassian-live`"]
async fn each_page_carries_its_comments_as_seeded() {
    let seeded = seeded();
    seeded.clear_leftovers().await;
    let (items, _) = full(&*seeded.source(seeded.scoped())).await;

    let mut with_comments = 0_usize;
    for page in &seeded.seed.pages {
        let it = item(&items, &page.id);
        let results = it.payload["children"]["comment"]["results"]
            .as_array()
            .unwrap_or_else(|| {
                panic!(
                    "{} ({}): no comment container at children.comment -- a page with no \
                     comments must carry an empty one, or 'the server was not asked' is \
                     indistinguishable from 'there are none': {}",
                    page.id, page.title, it.payload["children"]
                )
            });
        let got: Vec<&str> = results.iter().filter_map(|c| c["id"].as_str()).collect();
        let want: Vec<&str> = page.comments.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(
            got, want,
            "{} ({}): the comments the seed recorded",
            page.id, page.title
        );

        for (record, seeded_comment) in results.iter().zip(&page.comments) {
            with_comments += 1;
            let body = record["body"]["storage"]["value"]
                .as_str()
                .unwrap_or_default();
            assert!(
                body.contains(&seeded_comment.text),
                "{}: the comment's own text, in its storage body: {body:?}",
                page.id
            );
            // The discussion reaches `body_text`, which is what FTS indexes --
            // the whole reason the comments are expanded at all.
            assert!(
                it.body_text.contains(&seeded_comment.text),
                "{}: {:?}",
                page.id,
                it.body_text
            );
        }
    }
    assert!(
        with_comments > 0,
        "no seeded page has a comment, so this test asserted nothing -- \
         `testenv/seed-atlassian-content.sh` no longer posts the fixture's comments"
    );
    println!(
        "SEEDED comments: {with_comments} across {} page(s), authored by {:?}",
        seeded.seed.pages.len(),
        seeded.seed.author
    );
}

/// **ADR-0010's project read, and the ancestor path, against the real
/// payload.**
///
/// A space is what a Jira project is (ADR-0010), and a census outside this
/// crate reads its key and name out of the payload; the launcher reads the
/// ancestor titles out of the same payload. Neither read lives in this crate,
/// so nothing in the adapter's own suite would notice Confluence moving them
/// -- and a reader that found nothing would show *no path* rather than fail.
/// Asserted here, where the payload is the server's.
#[tokio::test]
#[ignore = "needs testenv's seeded Confluence: `just atlassian-live`"]
async fn the_space_and_the_ancestors_are_where_their_readers_look() {
    let seeded = seeded();
    seeded.clear_leftovers().await;
    let (items, _) = full(&*seeded.source(seeded.scoped())).await;

    let mut spaces: BTreeMap<String, String> = BTreeMap::new();
    for page in &seeded.seed.pages {
        let it = item(&items, &page.id);
        let key = it.payload["space"]["key"].as_str().unwrap_or_else(|| {
            panic!(
                "{}: ADR-0010 reads the project key at space.key, and it is not a string \
                 there: {}",
                page.id, it.payload["space"]
            )
        });
        let name = it.payload["space"]["name"].as_str().unwrap_or_else(|| {
            panic!(
                "{}: ADR-0010 reads the project name at space.name: {}",
                page.id, it.payload["space"]
            )
        });
        assert_eq!(key, seeded.seed.space, "{}", page.id);
        spaces.insert(key.to_owned(), name.to_owned());

        // The launcher's path: ancestor **titles**, outermost first. The seed
        // puts every fixture page directly under the space home, so the path
        // is exactly one deep and its last element is that home page.
        let ancestors = it.payload["ancestors"].as_array().unwrap_or_else(|| {
            panic!(
                "{}: no ancestors array: {}",
                page.id, it.payload["ancestors"]
            )
        });
        let last = ancestors
            .last()
            .unwrap_or_else(|| panic!("{} ({}): no ancestors at all", page.id, page.title));
        assert_eq!(
            last["id"].as_str(),
            Some(seeded.seed.home_page_id.as_str()),
            "{}: the seed puts every page under the space home",
            page.id
        );
        assert!(
            last["title"].as_str().is_some_and(|t| !t.trim().is_empty()),
            "{}: an ancestor with no title is a path segment the launcher cannot draw: {last}",
            page.id
        );
    }
    println!("SEEDED spaces, as ADR-0010's census reads them: {spaces:?}");
}

/// **The `_links.next` walk, for real.**
///
/// A page size of two over a corpus of five means the continuation link is
/// followed at least twice, and the expansions have to survive it: Confluence
/// builds that link itself, and a walk that re-composed the query instead
/// would be paging something subtly different. Without this the adapter would
/// be certified only on corpora that fit in one page -- which every fixture
/// does.
#[tokio::test]
#[ignore = "needs testenv's seeded Confluence: `just atlassian-live`"]
async fn the_walk_follows_the_next_link_and_the_expansions_survive_it() {
    let seeded = seeded();
    seeded.clear_leftovers().await;
    let mut config = seeded.scoped();
    config["page_size"] = serde_json::json!(2);

    let (paged, _) = full(&*seeded.source(config)).await;
    let (whole, _) = full(&*seeded.source(seeded.scoped())).await;
    assert_eq!(
        ids(&paged),
        ids(&whole),
        "a walk in pages of two returns the same corpus as one in pages of fifty"
    );
    assert!(
        paged.len() > 2,
        "the corpus fits in one page of two, so no continuation link was followed and this \
         test asserted nothing: {:?}",
        ids(&paged)
    );
    for it in &paged {
        assert!(
            it.payload["body"]["storage"].get("value").is_some(),
            "{}: the body expansion did not survive the continuation link: {}",
            it.entity,
            it.payload["body"]
        );
        assert!(
            it.payload["space"].get("key").is_some(),
            "{}: the space expansion did not survive the continuation link",
            it.entity
        );
        assert!(
            it.payload["children"]["comment"].get("results").is_some(),
            "{}: the comment container did not survive the continuation link",
            it.entity
        );
    }
    println!(
        "SEEDED next-link walk: {} page(s) in pages of two",
        paged.len()
    );
}

/// **Battery clause 2 against a real index, then a rename.**
///
/// Full run; idle run (clause 2, which no other suite can measure for this
/// adapter); one seeded page renamed through Confluence's own REST API; then
/// the next run returns *exactly* that page, **under the same entity id**, the
/// watermark moves to that page's own `version.when`, and the run after that
/// is idle again.
///
/// Three criteria in one sequence, because they are one behaviour: the id
/// survives the rename, the position advances only as far as the run
/// witnessed, and the overlap does not turn into "everything arrives twice".
///
/// The idle poll at the end is not ceremony: it is the only shape that shows a
/// cursor whose `seen` set forgot what the run *skipped*. Full sync then idle
/// is stable under that bug, because a full sync skips nothing.
#[tokio::test]
#[ignore = "needs testenv's seeded Confluence: `just atlassian-live`"]
async fn a_renamed_page_keeps_its_id_and_moves_the_watermark_to_itself() {
    let seeded = seeded();
    let mut guard = Renamed::new(&seeded).await;
    let source = seeded.source(seeded.scoped());
    let target = seeded.seeded_page("sepa-design").clone();

    let (items, cursor) = full(&*source).await;
    let before = item(&items, &target.id);
    assert_eq!(before.title, target.title, "the seed's own title, to start");
    let baseline = before
        .updated_at
        .expect("a real Confluence always stamps version.when");

    let (idle, same) = sync_from(&*source, Some(cursor.clone())).await;
    assert!(
        idle.is_empty(),
        "nothing changed, so nothing is emitted: {:?}",
        ids(&idle)
    );
    assert_eq!(same, cursor, "byte-identical");

    let renamed_title = guard.rename(&seeded, &target.id, &target.title).await;

    // Confluence's CQL reads an index its write path updates asynchronously,
    // so the run that first sees the rename is polled for rather than assumed
    // to be the very next one. Every poll before that one is idle and hands
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
            "the rename of {} did not reach Confluence's search index within {INDEX_BUDGET:?}",
            target.id
        );
        tokio::time::sleep(Duration::from_secs(1)).await;
    };

    assert_eq!(
        ids(&changed),
        vec![target.id.clone()],
        "the next incremental run returns exactly the page that was renamed"
    );
    let after = item(&changed, &target.id);
    // **The criterion.** A rename in Confluence is not a new entity: the id is
    // the content id, so every link, note and timer knobas drew to this page
    // survives the rename.
    assert_eq!(
        after.entity, before.entity,
        "a renamed page keeps its id -- {:?} became {renamed_title:?}",
        target.title
    );
    assert_eq!(after.title, renamed_title, "and the new title is mirrored");

    let witnessed = after
        .updated_at
        .expect("a real Confluence always stamps version.when");
    assert_eq!(
        modified_to(&moved),
        witnessed,
        "the position advances to the renamed page's own version time -- not to `now()`, and \
         not past what this run saw"
    );
    assert!(
        witnessed > baseline,
        "the rename moved the page's own version time forward: {witnessed} after {baseline}"
    );
    assert!(
        modified_to(&moved) <= chrono::Utc::now(),
        "a version time is in the past by the time the run reads it; a watermark after `now()` \
         would mean the run advanced past what it witnessed"
    );

    let (idle, still) = sync_from(&*source, Some(moved.clone())).await;
    assert!(
        idle.is_empty(),
        "the run after the rename skips it -- the record stayed in the cursor's `seen` set: \
         {:?}",
        ids(&idle)
    );
    assert_eq!(still, moved, "byte-identical");
    println!("SEEDED cursor after renaming {}: {moved}", target.id);
}

/// **What a real Confluence does with a credential it does not accept**,
/// measured at the wire and then through the adapter.
///
/// The Jira half of this certification (#276) found the product does three
/// different things depending on the scheme, and that a bad *bearer* token
/// there searches **anonymously** and answers 200 with an empty result set.
/// The same question has to be asked of this product rather than assumed from
/// the other, which is what the raw probes below are for -- they print what
/// they found, so a reviewer reads the answer rather than the guess.
///
/// Whatever the statuses turn out to be, the adapter's job is fixed: 401 and
/// 403 alike are [`SourceError::Unauthorized`] (contract §4.1), which is what
/// puts *Re-enter* on screen, and the classification must be the same from
/// `test_connection` and from mid-`sync`.
#[tokio::test]
#[ignore = "needs testenv's seeded Confluence: `just atlassian-live`"]
async fn a_rejected_credential_is_refused_and_the_seed_account_still_works() {
    let seeded = seeded();

    async fn probe(request: reqwest::RequestBuilder) -> (u16, String) {
        let response = request.send().await.expect("Confluence answered");
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        (status, body)
    }
    let get = |path: &str| {
        seeded
            .http
            .get(format!("{}/{path}", seeded.url))
            .header("Accept", "application/json")
    };
    let search = "rest/api/content/search?cql=type%20%3D%20page&limit=1";

    // A wrong bearer token. Never a wrong password on the seed's own account:
    // see `Seeded::refused_source`.
    for path in ["rest/api/user/current", search] {
        let (status, body) = probe(get(path).bearer_auth(bad_token())).await;
        println!(
            "SEEDED wrong bearer token on /{path}: {status} {}",
            &body[..body.len().min(200)]
        );
        assert_ne!(
            status, 200,
            "/{path} answered a token this Confluence cannot resolve with 200. If that is now \
             the product's behaviour, `the_search_that_could_read_as_an_empty_corpus_is_never_\
             the_first_call` is the test that has to hold the line -- read it before changing \
             this one: {body}"
        );
    }

    // A wrong password, under a username this Confluence does not have, one
    // request and no more: a failed login under a name that does not exist
    // counts against no account.
    let (status, body) =
        probe(get("rest/api/user/current").basic_auth(nobody(), Some("nope"))).await;
    println!(
        "SEEDED wrong password on /rest/api/user/current: {status} {}",
        &body[..body.len().min(200)]
    );
    assert!(
        status == 401 || status == 403,
        "a credential this Confluence refuses must be a refusal: {status} {body}"
    );

    // Whatever the scheme and whatever the body, the adapter classifies it the
    // one way the engine acts on -- and identically from both entry points,
    // which is what makes the sources view offer *Re-enter* for a credential
    // that expired after the source was added.
    let refused = seeded.refused_source(seeded.scoped());
    let connected = refused.test_connection().await;
    assert!(
        matches!(connected, Err(SourceError::Unauthorized { .. })),
        "{connected:?}"
    );
    let mut sink = VecSink(Vec::new());
    let synced = refused.sync(None, &mut sink).await;
    assert!(
        matches!(synced, Err(SourceError::Unauthorized { .. })),
        "{synced:?}"
    );
    assert!(
        sink.0.is_empty(),
        "nothing is emitted from a refused run: {:?}",
        ids(&sink.0)
    );

    // And the seed's own account still works. Not decoration: this is the
    // assertion that fails the moment somebody "simplifies" the wrong-password
    // probe above onto the real username, which locks the account out and
    // takes every later test in this run with it.
    seeded
        .source(seeded.scoped())
        .test_connection()
        .await
        .expect(
            "the seeded admin account must still authenticate after this test: an Atlassian \
             server answers a refusal to the *correct* password once an account has failed a \
             few logins, so nothing here may send a wrong password under it",
        );
}

/// **The search that could read as an empty corpus is never the first call a
/// run makes.**
///
/// A content search is a read a server may allow anonymously. Where it does,
/// an unresolvable credential answers 200 with an empty result set -- which is
/// indistinguishable from a wiki whose pages were all deleted. The `page` kind
/// claims `full_sync_exhaustive: true`, so a `cursor: None` run reported `Ok`
/// with no items is the engine's licence to tombstone every page in the
/// mirror. A run that started at the search would therefore answer a bad
/// credential by deleting the corpus.
///
/// It does not, because the run asks `/rest/api/user/current` first -- a call
/// that has no anonymous answer. This test is what stops a later
/// "optimization" (a cached identity, say) from quietly removing the guard,
/// and it holds whether or not this particular instance allows anonymous
/// reads: the ordering is asserted, not the permission.
#[tokio::test]
#[ignore = "needs testenv's seeded Confluence: `just atlassian-live`"]
async fn the_search_that_could_read_as_an_empty_corpus_is_never_the_first_call() {
    let seeded = seeded();
    let mut sink = VecSink(Vec::new());
    let refused = seeded
        .refused_source(seeded.scoped())
        .sync(None, &mut sink)
        .await;
    assert!(
        matches!(refused, Err(SourceError::Unauthorized { .. })),
        "a credential this Confluence will not resolve must be refused before the search, not \
         reported as a source with no pages: {refused:?}"
    );
    assert!(
        sink.0.is_empty(),
        "nothing is emitted from a refused run -- an `Ok` with an empty sink here is the \
         tombstoning case: {:?}",
        ids(&sink.0)
    );
    println!("SEEDED refused run: {refused:?}, sink empty");
}

/// The contract battery -- the suite every adapter must pass -- against the
/// server that decides.
///
/// Clause 2 is the one this corpus was seeded for: an incremental run after no
/// changes emits nothing and returns the same cursor. Clause 5 is the one this
/// adapter's read-only descriptor rests on: it declares no write ops, so the
/// battery calls `write` with **every** op the SPI knows and each must be
/// refused with `Protocol`. Faults are real: a bearer token that never existed
/// draws Confluence's own refusal, and a port nothing listens on is
/// unreachable.
#[tokio::test]
#[ignore = "needs testenv's seeded Confluence: `just atlassian-live`"]
async fn passes_the_contract_battery_against_the_seeded_server() {
    let seeded = seeded();
    seeded.clear_leftovers().await;
    battery(move |fault| match fault {
        Fault::None => seeded.source(seeded.scoped()),
        Fault::Unauthorized => seeded.refused_source(seeded.scoped()),
        Fault::Unreachable => seeded.source_at(&dead_url(), seeded.scoped()),
    })
    .await;
}

/// A source with **no** space list mirrors every space the account can see --
/// which is what the criterion says an empty list means, and the direction
/// that is easy to get backwards (an empty list read as "nothing" would make
/// an unconfigured source sync silently nothing at all).
#[tokio::test]
#[ignore = "needs testenv's seeded Confluence: `just atlassian-live`"]
async fn an_empty_space_list_means_every_space_the_account_can_see() {
    let seeded = seeded();
    seeded.clear_leftovers().await;
    let (unscoped, _) = full(&*seeded.source(serde_json::json!({}))).await;
    let (scoped, _) = full(&*seeded.source(seeded.scoped())).await;

    for id in seeded.seeded_ids() {
        assert!(
            ids(&unscoped).contains(&id),
            "an unconfigured source must see the seeded pages too: {:?}",
            ids(&unscoped)
        );
    }
    assert!(
        unscoped.len() >= scoped.len(),
        "every space is not fewer pages than one space: {} vs {}",
        unscoped.len(),
        scoped.len()
    );
    println!(
        "SEEDED scope: {} page(s) with no space list, {} scoped to {}",
        unscoped.len(),
        scoped.len(),
        seeded.seed.space
    );
}

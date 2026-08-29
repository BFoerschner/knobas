//! A wiremock stand-in for Gitea, serving Tidewater-shaped records.
//!
//! Stream B's contract source is the **real** pinned container (interfaces
//! §4.2), and `tests/live_gitea.rs` is where the exit criteria are measured.
//! This fake exists for one reason: `just check` and CI must stay docker-free
//! (roadmap §3). Every shape it encodes is re-asserted against the real
//! container, so if the two ever disagree, the fake is what is wrong.
//!
//! Records are `serde_json::Value` literals in Gitea's own field names, taken
//! from its OpenAPI document (`/swagger.v1.json`: `Repository`, `Branch`,
//! `PullRequest`, `Commit`, `Comment`).

// This module is compiled separately into every test binary that declares
// `mod support;`, and each of them uses a different part of it -- so without
// this, `clippy --all-targets -- -D warnings` fails on whatever one of them
// happens not to call.
#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};

use knobas_source::instance::SourceInstance;
use knobas_source::{AuthMethod, Source};
use serde_json::{Value, json};
use wiremock::matchers::{any, header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// The only token the fake accepts. Anything else gets Gitea's 401.
pub const TOKEN: &str = "tidewater-pat";

/// Wiremock matches lower numbers first; the defaults sit at 5.
const FORBIDDEN_PRIORITY: u8 = 3;
const EMPTY_PAGE_PRIORITY: u8 = 8;
const BAD_TOKEN_PRIORITY: u8 = 9;

/// Whether this instance guards its repository reads or serves them to anyone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reads {
    TokenOnly,
    Anonymous,
}

/// What the fake serves. Mutate it and call [`Fake::remount`] to make the
/// remote system change between two sync runs.
#[derive(Debug, Clone)]
pub struct State {
    /// Raw repository records, in listing order (sort=updated, order=desc).
    pub repos: Vec<Value>,
    /// `owner/repo` -> branch records.
    pub branches: BTreeMap<String, Vec<Value>>,
    /// `owner/repo` -> pull-request records, most recently updated first.
    pub pulls: BTreeMap<String, Vec<Value>>,
    /// `owner/repo#index` -> comment records.
    pub comments: BTreeMap<String, Vec<Value>>,
    /// `owner/repo#index` -> the `X-Total-Count` that discussion is served
    /// with, when it must differ from the number of records actually served.
    ///
    /// Gitea sends the header on every discussion and it has always agreed with
    /// the body; a fixture that makes it disagree is describing a Gitea that
    /// truncated the discussion without saying so in the payload -- the only
    /// shape in which `sync::fetch_comments`'s completeness check can be
    /// witnessed, and the reason it is a check rather than an assumption
    /// (issue #131).
    pub discussion_total: BTreeMap<String, usize>,
    /// `owner/repo#index` -> the HTTP status that discussion is refused with.
    /// A 404 is what Gitea answers for a repository with its issue unit
    /// disabled and a 403 is a token without issue scope -- both cost the
    /// discussion and nothing more. A 401 is a revoked credential and ends the
    /// run. The three used to arrive at the adapter as one value; ADR-0004
    /// carries the status, which is what lets this fixture mean three things.
    pub discussion_status: BTreeMap<String, u16>,
    /// `owner/repo@branch` -> commit records, newest first.
    pub commits: BTreeMap<String, Vec<Value>>,
    /// `owner/repo` entries every request under is answered 403 for -- a
    /// repository the listing offers and the token may not read (ruling B4).
    pub forbidden: BTreeSet<String>,
    /// `owner/repo` entries every request under is answered **401** for -- what
    /// a token revoked mid-run gets, as opposed to `forbidden`'s 403. The two
    /// used to be one thing here because they were one thing in the adapter;
    /// ADR-0004 carries the status, so the fixture has to be able to say which
    /// it is serving.
    pub revoked: BTreeSet<String>,
}

pub fn repo(owner: &str, name: &str, updated: &str) -> Value {
    json!({
        "id": 1, "name": name, "full_name": format!("{owner}/{name}"),
        "owner": { "login": owner, "id": 7 },
        "description": "Payout processing service",
        "html_url": format!("https://gitea.example/{owner}/{name}"),
        "default_branch": "main", "empty": false, "archived": false,
        "language": "Rust", "updated_at": updated, "created_at": "2026-01-04T09:00:00Z"
    })
}

pub fn branch(name: &str, sha: &str, message: &str, timestamp: &str) -> Value {
    json!({
        "name": name, "protected": false,
        "commit": {
            "id": sha, "message": message, "timestamp": timestamp,
            "url": format!("https://gitea.example/commit/{sha}"),
            "author": { "name": "Mara Lindqvist", "email": "mara@tidewater.example", "username": "mara" }
        }
    })
}

pub fn pull(number: u64, title: &str, body: &str, updated: &str, comments: u64) -> Value {
    json!({
        "number": number, "title": title, "body": body, "state": "open",
        "draft": false, "merged": false, "comments": comments,
        "user": { "login": "mara", "id": 7 },
        "html_url": format!("https://gitea.example/tidewater/payout-service/pulls/{number}"),
        "head": { "label": "feature/PAY-231-sepa-retry", "ref": "feature/PAY-231-sepa-retry" },
        "base": { "label": "main", "ref": "main" },
        "created_at": "2026-08-21T17:41:00Z", "updated_at": updated
    })
}

pub fn comment(body: &str, who: &str, when: &str) -> Value {
    json!({ "id": 1, "body": body, "user": { "login": who }, "created_at": when, "updated_at": when })
}

pub fn commit(sha: &str, message: &str, created: &str) -> Value {
    json!({
        "sha": sha, "created": created,
        "html_url": format!("https://gitea.example/tidewater/payout-service/commit/{sha}"),
        "author": { "login": "mara", "id": 7 },
        "commit": {
            "message": message,
            "author": { "name": "Mara Lindqvist", "email": "mara@tidewater.example", "date": created }
        }
    })
}

impl State {
    /// One repository, two branches, two pull requests, three commits -- the
    /// Tidewater storyline, in Gitea's shapes.
    pub fn tidewater() -> Self {
        let full = "tidewater/payout-service".to_owned();
        Self {
            repos: vec![repo("tidewater", "payout-service", "2026-08-22T11:42:00Z")],
            branches: BTreeMap::from([(
                full.clone(),
                vec![
                    branch(
                        "main",
                        "1111111111111111111111111111111111111111",
                        "PAY-228 partial refund drift",
                        "2026-08-21T16:00:00Z",
                    ),
                    branch(
                        "feature/PAY-231-sepa-retry",
                        "c90d11a3f5e2b7c4d9018e6a2b3c4d5e6f708192",
                        "PAY-231: jitter in backoff, cap at 5 attempts",
                        "2026-08-22T11:42:00Z",
                    ),
                ],
            )]),
            // Most recently updated first: what `sort=recentupdate` answers,
            // and what the incremental walk relies on.
            pulls: BTreeMap::from([(
                full.clone(),
                vec![
                    pull(144, "Add payout CSV export", "", "2026-08-22T13:50:00Z", 0),
                    pull(
                        142,
                        "SEPA retry with exponential backoff",
                        "Retries transient PSP errors.",
                        "2026-08-22T10:20:00Z",
                        2,
                    ),
                ],
            )]),
            comments: BTreeMap::from([(
                "tidewater/payout-service#142".to_owned(),
                vec![
                    comment(
                        "Should the jitter be bounded? +/-20 % feels wide.",
                        "jonas",
                        "2026-08-22T09:30:00Z",
                    ),
                    comment(
                        "Bounded to +/-10 % in c90d11.",
                        "mara",
                        "2026-08-22T10:20:00Z",
                    ),
                ],
            )]),
            commits: BTreeMap::from([
                (
                    format!("{full}@feature/PAY-231-sepa-retry"),
                    vec![
                        commit(
                            "c90d11a3f5e2b7c4d9018e6a2b3c4d5e6f708192",
                            "PAY-231: jitter in backoff, cap at 5 attempts",
                            "2026-08-22T11:42:00Z",
                        ),
                        commit(
                            "a41f2c8b7d6e5f403192837465a0b1c2d3e4f506",
                            "PAY-231 backoff jitter",
                            "2026-08-22T10:02:00Z",
                        ),
                    ],
                ),
                (
                    format!("{full}@main"),
                    vec![commit(
                        "1111111111111111111111111111111111111111",
                        "PAY-228 partial refund drift",
                        "2026-08-21T16:00:00Z",
                    )],
                ),
            ]),
            forbidden: BTreeSet::new(),
            revoked: BTreeSet::new(),
            discussion_status: BTreeMap::new(),
            discussion_total: BTreeMap::new(),
        }
    }

    /// A second repository under another owner, with one branch of its own.
    pub fn with_elsewhere(mut self) -> Self {
        self.repos
            .push(repo("elsewhere", "unrelated", "2026-08-20T09:00:00Z"));
        self.branches.insert(
            "elsewhere/unrelated".to_owned(),
            vec![branch(
                "main",
                "9999999999999999999999999999999999999999",
                "initial",
                "2026-08-20T09:00:00Z",
            )],
        );
        self
    }

    /// Sorted newest-updated first, the way `sort=recentupdate` answers.
    pub fn touch_pull(&mut self, full_name: &str, number: u64, updated: &str) {
        let pulls = self
            .pulls
            .get_mut(full_name)
            .expect("repository has pull requests");
        for p in pulls.iter_mut() {
            if p["number"] == json!(number) {
                p["updated_at"] = json!(updated);
            }
        }
        pulls.sort_by(|a, b| b["updated_at"].as_str().cmp(&a["updated_at"].as_str()));
    }
}

pub struct Fake {
    server: MockServer,
}

impl Fake {
    pub async fn start(state: &State) -> Self {
        let fake = Self {
            server: MockServer::start().await,
        };
        fake.mount(state, 1_000).await;
        fake
    }

    /// The same fake, but every list is served in pages of [`PAGE`] -- what the
    /// adapter meets on a real instance with more branches than fit in one
    /// answer.
    pub async fn start_paged(state: &State) -> Self {
        let fake = Self {
            server: MockServer::start().await,
        };
        fake.mount(state, PAGE).await;
        fake
    }

    /// A server that **ignores the `limit` the adapter asked for** and answers
    /// at most `cap` records per page.
    ///
    /// Every other constructor here serves exactly what it was asked for, so on
    /// them a page shorter than [`PAGE`] can only mean the collection ran out.
    /// That is the one reading issue #81 is about, and a fake that cannot
    /// express the other one cannot show the bug: an admin-lowered
    /// `MAX_RESPONSE_ITEMS`, a per-endpoint maximum, or a partial page under
    /// load all answer short with more still to come. Gitea's 50 is a
    /// **default**, and knobas is aimed at self-hosted instances where defaults
    /// get changed.
    ///
    /// With `cap` below [`PAGE`] every page of a non-empty listing is short, so
    /// a walk that reads short as last mirrors `cap` records per listing and
    /// reports success over everything after them.
    pub async fn start_capped(state: &State, cap: usize) -> Self {
        assert!(
            0 < cap && cap < PAGE,
            "a cap only says anything below the page size the adapter asks for"
        );
        let fake = Self {
            server: MockServer::start().await,
        };
        fake.mount(state, cap).await;
        fake
    }

    /// An instance that serves its **public** repositories to anyone and only
    /// guards `/user`, which is what a Gitea with public repos actually does.
    ///
    /// This is the fixture the identity preflight can be witnessed on: against
    /// a server that 401s everything, dropping the preflight changes nothing,
    /// because the next request fails the same way. Here a run with a dead
    /// token would quietly mirror whatever is public instead.
    pub async fn start_public(state: &State) -> Self {
        let fake = Self {
            server: MockServer::start().await,
        };
        fake.mount_as(state, 1_000, Reads::Anonymous).await;
        fake
    }

    pub fn base_url(&self) -> String {
        self.server.uri()
    }

    /// Replace everything the fake serves -- how a test makes the remote system
    /// change between two runs.
    pub async fn remount(&self, state: &State) {
        self.server.reset().await;
        self.mount(state, 1_000).await;
    }

    /// How many requests the fake has answered so far, which is how a test
    /// asserts that an idle run costs what the module docs claim.
    pub async fn requests(&self) -> usize {
        self.server
            .received_requests()
            .await
            .map_or(0, |received| received.len())
    }

    /// Every path the fake has been asked for, in order -- how a test asserts
    /// that a request was *not* made.
    pub async fn paths(&self) -> Vec<String> {
        self.server
            .received_requests()
            .await
            .unwrap_or_default()
            .iter()
            .map(|r| r.url.path().to_owned())
            .collect()
    }

    async fn mount(&self, state: &State, page_size: usize) {
        self.mount_as(state, page_size, Reads::TokenOnly).await;
    }

    async fn mount_as(&self, state: &State, page_size: usize, reads: Reads) {
        let ok = |body: Value| ResponseTemplate::new(200).set_body_json(body);
        let token = format!("token {TOKEN}");
        // `/user` is guarded whatever the instance does with its repositories:
        // that is the whole difference between the two fixtures.
        let identity = |m: wiremock::MockBuilder| m.and(header("Authorization", token.as_str()));
        let authed = |m: wiremock::MockBuilder| match reads {
            Reads::TokenOnly => m.and(header("Authorization", token.as_str())),
            Reads::Anonymous => m,
        };

        // A repository the token may not read (403), and one a revoked token
        // is refused outright for (401). Mounted first so they win over the
        // records below, which is what lets one `State` describe both -- and
        // kept apart, because the adapter's answer differs: 403 skips that
        // repository, 401 ends the run (ADR-0004).
        for (full_name, status) in state
            .forbidden
            .iter()
            .map(|name| (name, 403))
            .chain(state.revoked.iter().map(|name| (name, 401)))
        {
            authed(
                Mock::given(method("GET")).and(path_prefix(&format!("/api/v1/repos/{full_name}"))),
            )
            .respond_with(
                ResponseTemplate::new(status)
                    .set_body_json(json!({ "message": "user does not have permission" })),
            )
            .with_priority(FORBIDDEN_PRIORITY)
            .mount(&self.server)
            .await;
        }

        identity(Mock::given(method("GET")).and(path("/api/v1/version")))
            .respond_with(ok(json!({ "version": "1.24.3" })))
            .mount(&self.server)
            .await;
        identity(Mock::given(method("GET")).and(path("/api/v1/user")))
            .respond_with(ok(
                json!({ "login": "mara", "id": 7, "full_name": "Mara Lindqvist" }),
            ))
            .mount(&self.server)
            .await;
        for (index, chunk) in pages(&state.repos, page_size) {
            authed(
                Mock::given(method("GET"))
                    .and(path("/api/v1/repos/search"))
                    .and(query_param("page", index.to_string().as_str())),
            )
            .respond_with(ok(json!({ "ok": true, "data": chunk })))
            .mount(&self.server)
            .await;
        }
        // The same records one at a time, for a source configured with an
        // explicit `repos[]` allowlist -- which does no listing at all.
        for repo in &state.repos {
            let full_name = repo["full_name"]
                .as_str()
                .expect("a repository record has a full_name");
            authed(Mock::given(method("GET")).and(path(format!("/api/v1/repos/{full_name}"))))
                .respond_with(ok(repo.clone()))
                .mount(&self.server)
                .await;
        }

        for (full_name, branches) in &state.branches {
            for (index, chunk) in pages(branches, page_size) {
                authed(
                    Mock::given(method("GET"))
                        .and(path(format!("/api/v1/repos/{full_name}/branches")))
                        .and(query_param("page", index.to_string().as_str())),
                )
                .respond_with(ok(json!(chunk)))
                .mount(&self.server)
                .await;
            }
        }
        for (full_name, pulls) in &state.pulls {
            for (index, chunk) in pages(pulls, page_size) {
                authed(
                    Mock::given(method("GET"))
                        .and(path(format!("/api/v1/repos/{full_name}/pulls")))
                        .and(query_param("page", index.to_string().as_str())),
                )
                .respond_with(ok(json!(chunk)))
                .mount(&self.server)
                .await;
            }
        }
        // Mounted before the discussions themselves so a refusal wins over
        // the records it stands in for -- the same trick `forbidden` uses.
        for (key, status) in &state.discussion_status {
            let (full_name, index) = key
                .split_once('#')
                .expect("discussion key is owner/repo#index");
            authed(Mock::given(method("GET")).and(path(format!(
                "/api/v1/repos/{full_name}/issues/{index}/comments"
            ))))
            .respond_with(
                ResponseTemplate::new(*status)
                    .set_body_json(json!({ "message": "no permission to read issues" })),
            )
            .with_priority(FORBIDDEN_PRIORITY)
            .mount(&self.server)
            .await;
        }
        // **The discussion is served whole, whatever the request said, and
        // whatever `page_size` the rest of this fake is honouring.** That is
        // not laziness, it is the endpoint: Gitea's `issueGetComments` declares
        // no `page` and no `limit` (its OpenAPI document says so, and the
        // repository-wide `issueGetRepoComments` next to it declares both), and
        // measured against the pinned container it ignores them -- 51 comments
        // came back for `limit=2` and for `page=9` alike. A fake that paged
        // this route would be a fake asserting a server that does not exist,
        // and `just check` would go green over an adapter re-reading the same
        // discussion until its budget ran out. Issue #131, measured 2026-08-29
        // on Gitea 1.27.2; `live_gitea::the_discussion_endpoint_does_not_page`
        // is what re-asserts it against the server that decides it.
        //
        // `X-Total-Count` rides along because the adapter's only completeness
        // check reads it -- and `discussion_total` is how a fixture makes the
        // header disagree with the body, which no real Gitea has been seen to
        // do and which the adapter must refuse rather than mirror.
        for (key, comments) in &state.comments {
            let (full_name, index) = key
                .split_once('#')
                .expect("comment key is owner/repo#index");
            let total = state
                .discussion_total
                .get(key)
                .copied()
                .unwrap_or(comments.len());
            authed(Mock::given(method("GET")).and(path(format!(
                "/api/v1/repos/{full_name}/issues/{index}/comments"
            ))))
            .respond_with(
                ok(json!(comments)).insert_header("X-Total-Count", total.to_string().as_str()),
            )
            .mount(&self.server)
            .await;
        }
        for (key, commits) in &state.commits {
            let (full_name, branch) = key
                .split_once('@')
                .expect("commit key is owner/repo@branch");
            for (index, chunk) in pages(commits, page_size) {
                authed(
                    Mock::given(method("GET"))
                        .and(path(format!("/api/v1/repos/{full_name}/commits")))
                        .and(query_param("sha", branch))
                        .and(query_param("page", index.to_string().as_str())),
                )
                .respond_with(ok(json!(chunk)))
                .mount(&self.server)
                .await;
            }
        }

        // Anything past the last page answers empty, the way Gitea does --
        // never a 401, which would read as a credential fault. `/user` and
        // `/version` are excluded: they are not lists, and a `[]` there would
        // let a run past the identity preflight with no account at all.
        //
        // `/repos/search` is excluded too, and answered just below, because its
        // empty page is **not** `[]`: it is the `{ok,data}` envelope with an
        // empty `data`, which is what the real server returns for a page past
        // the end. A `[]` there is not a repository listing at all and reads as
        // an unreadable body, not as an end -- and nothing noticed, because no
        // walk asked for the page past the last one until they started paging
        // until *empty* rather than until short (issue #81).
        authed(
            Mock::given(method("GET")).and(|request: &wiremock::Request| {
                !matches!(
                    request.url.path(),
                    "/api/v1/user" | "/api/v1/version" | "/api/v1/repos/search"
                )
            }),
        )
        .respond_with(ok(json!([])))
        .with_priority(EMPTY_PAGE_PRIORITY)
        .mount(&self.server)
        .await;
        authed(Mock::given(method("GET")).and(path("/api/v1/repos/search")))
            .respond_with(ok(json!({ "ok": true, "data": [] })))
            .with_priority(EMPTY_PAGE_PRIORITY)
            .mount(&self.server)
            .await;
        // Lowest priority: no token, or the wrong one. This is what Gitea
        // answers an invalid token with, and what the battery's Unauthorized
        // case relies on.
        Mock::given(any())
            .respond_with(
                ResponseTemplate::new(401)
                    .set_body_json(json!({ "message": "token does not exist" })),
            )
            .with_priority(BAD_TOKEN_PRIORITY)
            .mount(&self.server)
            .await;
    }
}

/// The adapter's page size, mirrored here so a paging test asks for the
/// boundary the adapter actually walks.
pub const PAGE: usize = 50;

/// `(page number starting at 1, the records on it)`. An empty list still has a
/// first page, which is what an empty repository's branch listing looks like.
fn pages(records: &[Value], page_size: usize) -> Vec<(usize, Vec<Value>)> {
    if records.is_empty() {
        return vec![(1, Vec::new())];
    }
    records
        .chunks(page_size)
        .enumerate()
        .map(|(index, chunk)| (index + 1, chunk.to_vec()))
        .collect()
}

/// Match every path under `prefix`, which is how one `forbidden` entry covers a
/// repository's whole subtree.
fn path_prefix(prefix: &str) -> impl wiremock::Match + use<> {
    let prefix = prefix.to_owned();
    move |request: &wiremock::Request| request.url.path().starts_with(&prefix)
}

/// A port nothing listens on: bound to learn the number, then dropped.
pub fn dead_url() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    format!("http://127.0.0.1:{port}")
}

/// One configured source pointing at `base_url`.
pub fn instance(base_url: String, token: &str, config: Value) -> SourceInstance {
    SourceInstance {
        id: "gitea".to_owned(),
        kind: "gitea".to_owned(),
        display_name: "Tidewater Git".to_owned(),
        base_url,
        auth: Some(AuthMethod::Pat),
        secret: Some(token.to_owned()),
        config,
    }
}

/// The adapter under test, built against the fake.
pub fn source(base_url: String, config: Value) -> Box<dyn Source> {
    knobas_source_gitea::build(instance(base_url, TOKEN, config)).expect("the adapter builds")
}

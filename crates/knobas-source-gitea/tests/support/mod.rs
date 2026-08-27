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
/// Between the good `/user` (default 5) and the catch-alls, so it takes over
/// exactly when the good one's allowance is spent.
const BROKEN_IDENTITY_PRIORITY: u8 = 6;
const EMPTY_PAGE_PRIORITY: u8 = 8;
const BAD_TOKEN_PRIORITY: u8 = 9;

/// Whether this instance guards its repository reads or serves them to anyone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reads {
    TokenOnly,
    Anonymous,
}

/// What `/user` does over the life of a run: the three outcomes the credential
/// probe has to tell apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Identity {
    /// Always answers -- every fixture but the two below.
    Always,
    /// One answer, then a revoked token.
    OnceThen401,
    /// One answer, then a 500: the credential is *unknown*, not dead.
    OnceThen500,
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
    /// `owner/repo@branch` -> commit records, newest first.
    pub commits: BTreeMap<String, Vec<Value>>,
    /// `owner/repo` entries every request under is answered 403 for -- a
    /// repository the listing offers and the token may not read (ruling B4).
    pub forbidden: BTreeSet<String>,
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

    /// The same instance, but the token stops working **after the first
    /// `/user`** -- a credential revoked mid-run.
    ///
    /// `/user` answers once (the run's identity preflight, which must succeed
    /// or the run never gets far enough to be interesting) and then falls
    /// through to the fake's 401. Everything else is mounted as usual, so which
    /// repositories still answer is `State::forbidden`'s business: a
    /// repository listed as forbidden stands in for the repositories a
    /// revoked token can no longer read, and the adapter cannot tell that 403
    /// from the 401 it would really get -- which is the whole point.
    pub async fn remount_revoked_after_preflight(&self, state: &State) {
        self.server.reset().await;
        self.mount_with_identity(state, 1_000, Reads::TokenOnly, Identity::OnceThen401)
            .await;
    }

    /// `/user` answers once and then **breaks** -- a 500, which is not in
    /// `status_is_transient` and so is not retried.
    ///
    /// The third outcome of the credential probe: not "alive", not "revoked",
    /// but *unknown*. A revoked token and a broken `/user` are different events
    /// and the run must not report them as the same one.
    pub async fn remount_identity_broken_after_preflight(&self, state: &State) {
        self.server.reset().await;
        self.mount_with_identity(state, 1_000, Reads::TokenOnly, Identity::OnceThen500)
            .await;
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
        self.mount_with_identity(state, page_size, reads, Identity::Always)
            .await;
    }

    async fn mount_with_identity(
        &self,
        state: &State,
        page_size: usize,
        reads: Reads,
        identity_mode: Identity,
    ) {
        let ok = |body: Value| ResponseTemplate::new(200).set_body_json(body);
        let token = format!("token {TOKEN}");
        // `/user` is guarded whatever the instance does with its repositories:
        // that is the whole difference between the two fixtures.
        let identity = |m: wiremock::MockBuilder| m.and(header("Authorization", token.as_str()));
        let authed = |m: wiremock::MockBuilder| match reads {
            Reads::TokenOnly => m.and(header("Authorization", token.as_str())),
            Reads::Anonymous => m,
        };

        // A repository the token may not read. Mounted first so it wins over
        // the records below, which is what lets one `State` describe both.
        for full_name in &state.forbidden {
            authed(
                Mock::given(method("GET")).and(path_prefix(&format!("/api/v1/repos/{full_name}"))),
            )
            .respond_with(
                ResponseTemplate::new(403)
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
        let user = identity(Mock::given(method("GET")).and(path("/api/v1/user"))).respond_with(ok(
            json!({ "login": "mara", "id": 7, "full_name": "Mara Lindqvist" }),
        ));
        match identity_mode {
            Identity::Always => user.mount(&self.server).await,
            // One good answer, then whatever the fixture wants `/user` to do.
            // The 401 case simply falls through to the catch-all below; the
            // 500 needs its own mock, mounted between the two so it wins once
            // the allowance above is spent.
            Identity::OnceThen401 => user.up_to_n_times(1).mount(&self.server).await,
            Identity::OnceThen500 => {
                user.up_to_n_times(1).mount(&self.server).await;
                Mock::given(method("GET"))
                    .and(path("/api/v1/user"))
                    .respond_with(
                        ResponseTemplate::new(500)
                            .set_body_json(json!({ "message": "internal server error" })),
                    )
                    .with_priority(BROKEN_IDENTITY_PRIORITY)
                    .mount(&self.server)
                    .await;
            }
        }
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
        for (key, comments) in &state.comments {
            let (full_name, index) = key
                .split_once('#')
                .expect("comment key is owner/repo#index");
            authed(Mock::given(method("GET")).and(path(format!(
                "/api/v1/repos/{full_name}/issues/{index}/comments"
            ))))
            .respond_with(ok(json!(comments)))
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
        authed(
            Mock::given(method("GET")).and(|request: &wiremock::Request| {
                !matches!(request.url.path(), "/api/v1/user" | "/api/v1/version")
            }),
        )
        .respond_with(ok(json!([])))
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

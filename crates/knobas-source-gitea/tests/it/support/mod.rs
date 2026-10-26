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

// Every module of the crate's test binary that uses this (`client`, `contract`,
// `sync`, `write`) uses a different part of it -- so without this,
// `clippy --all-targets -- -D warnings` fails on whatever none of them happens
// to call.
#![allow(dead_code)]

pub mod held;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use knobas_source::instance::SourceInstance;
use knobas_source::{AuthMethod, Source};
use serde_json::{Value, json};
use wiremock::matchers::{any, header, method, path, path_regex, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// The only token the fake accepts. Anything else gets Gitea's 401.
pub const TOKEN: &str = "tidewater-pat";

/// Gitea's answer to a write sent with the body `{}` under `path` (#2030,
/// `view::grants`), as measured on 1.27.3: the scope refusal (403) for a token
/// without the route's write scope, the body's validation (422) for one with
/// it, another 403 for an account that may not push.
pub async fn write_probe(server: &MockServer, path_: String, status: u16, message: &str) {
    Mock::given(method("POST"))
        .and(path(path_))
        .and(header("Authorization", format!("token {TOKEN}").as_str()))
        .and(wiremock::matchers::body_json(json!({})))
        .respond_with(ResponseTemplate::new(status).set_body_json(json!({
            "message": message,
            "url": "http://gitea/api/swagger",
        })))
        .mount(server)
        .await;
}

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
    /// `owner/repo` -> what `GET /repos/{owner}/{repo}/teams` answers: a
    /// status and its body (#966). Gitea 1.27.3, measured 2026-10-17: `200`
    /// with the team records for an organization's repository, *Owners*
    /// (`includes_all_repositories: true`) among them, and `405` for a
    /// repository a user owns. A repository not named here answers 404.
    pub teams: BTreeMap<String, (u16, Value)>,
    /// `owner/repo` -> branch records.
    pub branches: BTreeMap<String, Vec<Value>>,
    /// `owner/repo` -> pull-request records, most recently updated first.
    pub pulls: BTreeMap<String, Vec<Value>>,
    /// `owner/repo` -> issue records (#1206), in the listing's order: newest
    /// created first, which is what Gitea 1.27.3 answers whatever `sort`
    /// says. Served filtered by `since`, inclusive, as Gitea does.
    pub issues: BTreeMap<String, Vec<Value>>,
    /// `owner/repo` entries whose issue listing answers this status (#1206):
    /// 403 for a token without issue scope, 404 for the unit switched off.
    pub issues_status: BTreeMap<String, u16>,
    /// `owner/repo#index` -> comment records. An issue's discussion is keyed
    /// the same way: Gitea keeps both on `/issues/{index}/comments`.
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
    /// `owner/repo` entries every request under is answered **404** for -- a
    /// repository the listing offered and that was gone by the time the walk
    /// reached it.
    pub missing: BTreeSet<String>,
    /// `missing` entries whose **first** `GET /repos/{owner}/{repo}` still
    /// answers the record: a repository deleted after that read, between a
    /// configured `repos[]` entry's selection and its walk (issue #728). Every
    /// read after the first answers `missing`'s 404. A fake for the same
    /// reason as the fields below (ADR-0013): the deletion falls between two
    /// requests inside one `sync` call, which nothing opens on demand on the
    /// pinned container.
    pub read_once: BTreeSet<String>,
    /// Why a fake witnesses this and the three fields below (ADR-0013, "a
    /// fault the real instance cannot produce is witnessed by a fake"): the
    /// window is between two requests inside one `sync` call, and no seed, flag
    /// or clock opens it on the pinned container. The answers served are the
    /// ones measured against it; `just gitea-live` stays the certificate.
    ///
    /// `owner/repo@branch` entries the listing still offers but that are gone
    /// by the time the walk reads them: deleted between the two requests, as a
    /// merge with delete-branch does on a busy repository (issue #724). Their
    /// commit read answers what Gitea 1.27.3 answers for a ref that no longer
    /// exists, measured 2026-10-13: `404 object does not exist [id: <branch>]`,
    /// and so does the branch's own read, `GET /branches/{name}`: `404 not
    /// found`.
    pub vanished_branches: BTreeSet<String>,
    /// `owner/repo@branch` entries whose commit read answers that same 404
    /// while the branch itself still reads: the control for
    /// `vanished_branches`, a 404 that is not "the branch is gone".
    pub history_missing: BTreeSet<String>,
    /// `owner/repo#index` pull requests the listing still offers but that were
    /// deleted before their discussion was read (issue #724). Measured on
    /// Gitea 1.27.3, 2026-10-13: the discussion read answers **500** `issue
    /// does not exist` -- not a 404 -- and `GET /pulls/{index}` answers 404.
    pub vanished_pulls: BTreeSet<String>,
    /// `owner/repo#index`, `owner/repo@branch` or `owner/repo` -> the status
    /// the confirming read (`GET /pulls/{index}`, `GET /branches/{name}`,
    /// `GET /repos/{owner}/{repo}`, issue #728) answers instead of
    /// what the rest of the state implies: a transient server fault on the one
    /// read that decides "deleted" (issue #724). Only a 404 there may skip.
    pub confirm_status: BTreeMap<String, u16>,
}

pub fn repo(owner: &str, name: &str, updated: &str) -> Value {
    json!({
        "id": 1, "name": name, "full_name": format!("{owner}/{name}"),
        "owner": { "login": owner, "id": 7 },
        "description": "Payout processing service",
        "html_url": format!("https://gitea.example/{owner}/{name}"),
        "default_branch": "main", "empty": false, "archived": false,
        "mirror": false, "has_pull_requests": true,
        "language": "Rust", "updated_at": updated, "created_at": "2026-01-04T09:00:00Z"
    })
}

/// One team as `GET /repos/{owner}/{repo}/teams` lists it (Gitea 1.27.3).
pub fn team(name: &str, includes_all_repositories: bool) -> Value {
    json!({
        "id": 1, "name": name, "description": "", "organization": null,
        "includes_all_repositories": includes_all_repositories,
        "permission": if includes_all_repositories { "owner" } else { "write" },
        "units": ["repo.code", "repo.pulls"], "can_create_org_repo": includes_all_repositories
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

/// One pull request as Gitea's `/pulls` listing serves it.
///
/// `requested_reviewers` is here because Gitea's own `PullRequest` model
/// carries it on every pull request -- an array of users, or `null` where
/// nobody has been asked -- and it is the path this adapter's descriptor
/// declares its reviewers at (#277). The fake standing in for the real
/// container has to carry the field the declaration names, or the shape this
/// suite certifies is not the shape the live one does; `tests/live_gitea.rs`
/// is where the two are held together.
pub fn pull(number: u64, title: &str, body: &str, updated: &str, comments: u64) -> Value {
    json!({
        "number": number, "title": title, "body": body, "state": "open",
        // Gitea writes `merged_at` on every pull request, `null` until it is
        // merged: where `merged_at` is declared (#929).
        "draft": false, "merged": false, "merged_at": null, "comments": comments,
        "requested_reviewers": [{ "login": "tom", "id": 8 }],
        "user": { "login": "mara", "id": 7 },
        "html_url": format!("https://gitea.example/tidewater/payout-service/pulls/{number}"),
        // `sha` is the head commit, where the descriptor declares a pull
        // request's commit (#710).
        "head": {
            "label": "feature/PAY-231-sepa-retry", "ref": "feature/PAY-231-sepa-retry",
            "sha": "c90d11a3f5e2b7c4d9018e6a2b3c4d5e6f708192",
            // The repository the branch is in, which for a fork's pull
            // request is the fork's (#718): where `head_repo` is declared.
            "repo": {
                "id": 1, "name": "payout-service", "full_name": "tidewater/payout-service",
                "owner": { "login": "tidewater", "id": 7 }
            }
        },
        "base": {
            "label": "main", "ref": "main",
            // Gitea's `PRBranchInfo` carries the repository it is in, and the
            // descriptor declares a pull request's parent there (#595).
            "repo": {
                "id": 1, "name": "payout-service", "full_name": "tidewater/payout-service",
                "owner": { "login": "tidewater", "id": 7 }
            }
        },
        "created_at": "2026-08-21T17:41:00Z", "updated_at": updated
    })
}

/// One issue as Gitea 1.27.3's `/issues?type=issues` listing serves it
/// (measured 2026-10-19), with the labels named.
pub fn issue(number: u64, title: &str, state: &str, labels: &[&str], updated: &str) -> Value {
    json!({
        "id": 100 + number, "number": number, "title": title, "body": "",
        "state": state, "comments": 0, "is_locked": false, "pull_request": null,
        "user": { "login": "mara", "id": 7 }, "assignee": null, "assignees": null,
        "labels": labels.iter().map(|name| json!({ "id": 1, "name": name, "exclusive": true })).collect::<Vec<_>>(),
        "milestone": null,
        "html_url": format!("https://gitea.example/tidewater/payout-service/issues/{number}"),
        "repository": {
            "id": 1, "name": "payout-service", "owner": "tidewater",
            "full_name": "tidewater/payout-service"
        },
        "created_at": "2026-08-20T09:00:00Z", "updated_at": updated,
        "closed_at": if state == "closed" { json!(updated) } else { Value::Null }
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
        },
        // What a listing asked with `files=false` carries (1.27.3, measured
        // 2026-10-12): `null`, which a branch's and a repository's views read.
        "files": null
    })
}

impl State {
    /// One repository, two branches, two pull requests, three commits -- the
    /// Tidewater storyline, in Gitea's shapes.
    pub fn tidewater() -> Self {
        let full = "tidewater/payout-service".to_owned();
        Self {
            repos: vec![repo("tidewater", "payout-service", "2026-08-22T11:42:00Z")],
            teams: BTreeMap::from([(
                full.clone(),
                (200, json!([team("Owners", true), team("Payments", false)])),
            )]),
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
            issues: BTreeMap::from([(
                full.clone(),
                vec![
                    {
                        let mut open = issue(
                            7,
                            "Payout CSV drops the currency column",
                            "open",
                            &["Priority/High", "Kind/Bug"],
                            "2026-08-22T09:00:00Z",
                        );
                        open["assignee"] = json!({ "login": "tom", "id": 8 });
                        open["comments"] = json!(1);
                        open
                    },
                    issue(
                        3,
                        "Document the retry budget",
                        "closed",
                        &[],
                        "2026-08-21T12:00:00Z",
                    ),
                ],
            )]),
            issues_status: BTreeMap::new(),
            comments: BTreeMap::from([
                (
                    "tidewater/payout-service#7".to_owned(),
                    vec![comment(
                        "Reproduced on the EUR batch.",
                        "jonas",
                        "2026-08-22T09:00:00Z",
                    )],
                ),
                (
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
                ),
            ]),
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
            missing: BTreeSet::new(),
            read_once: BTreeSet::new(),
            vanished_branches: BTreeSet::new(),
            history_missing: BTreeSet::new(),
            vanished_pulls: BTreeSet::new(),
            confirm_status: BTreeMap::new(),
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

    /// Change issue `number` of `full_name`: `edit` is handed its record.
    pub fn edit_issue(&mut self, full_name: &str, number: u64, edit: impl FnOnce(&mut Value)) {
        let issue = self
            .issues
            .get_mut(full_name)
            .and_then(|issues| issues.iter_mut().find(|i| i["number"] == json!(number)))
            .expect("the repository has that issue");
        edit(issue);
    }
}

/// Gitea's issue listing: `type=issues` required, `state=all`, filtered by
/// `since` inclusively, paged by `page` and the fake's page size.
struct Issues {
    records: Vec<Value>,
    page_size: usize,
}

impl wiremock::Respond for Issues {
    fn respond(&self, request: &wiremock::Request) -> ResponseTemplate {
        let query: BTreeMap<String, String> = request.url.query_pairs().into_owned().collect();
        if query.get("type").map(String::as_str) != Some("issues")
            || query.get("state").map(String::as_str) != Some("all")
        {
            return ResponseTemplate::new(400)
                .set_body_json(json!({ "message": "this fake serves type=issues&state=all" }));
        }
        let since = query
            .get("since")
            .map(|t| chrono::DateTime::parse_from_rfc3339(t).expect("since is RFC 3339"));
        let admitted: Vec<Value> = self
            .records
            .iter()
            .filter(|issue| {
                since.is_none_or(|since| {
                    let updated = issue["updated_at"]
                        .as_str()
                        .expect("an issue has updated_at");
                    chrono::DateTime::parse_from_rfc3339(updated).expect("RFC 3339") >= since
                })
            })
            .cloned()
            .collect();
        let page: usize = query.get("page").and_then(|p| p.parse().ok()).unwrap_or(1);
        let chunk: Vec<Value> = admitted
            .chunks(self.page_size)
            .nth(page - 1)
            .map(<[Value]>::to_vec)
            .unwrap_or_default();
        ResponseTemplate::new(200).set_body_json(chunk)
    }
}

pub struct Fake {
    server: MockServer,
}

impl Fake {
    pub async fn start(state: &State) -> Self {
        let fake = Self {
            server: MockServer::builder().start().await,
        };
        fake.mount(state, 1_000).await;
        fake
    }

    /// The same fake, but every list is served in pages of [`PAGE`] -- what the
    /// adapter meets on a real instance with more branches than fit in one
    /// answer.
    pub async fn start_paged(state: &State) -> Self {
        let fake = Self {
            server: MockServer::builder().start().await,
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
            server: MockServer::builder().start().await,
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
            server: MockServer::builder().start().await,
        };
        fake.mount_as(state, 1_000, Reads::Anonymous).await;
        fake
    }

    /// The server itself, for a test that mounts an answer of its own over
    /// the state's.
    pub fn server(&self) -> &MockServer {
        &self.server
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

        // A repository the token may not read (403), one a revoked token is
        // refused outright for (401), and one that is gone (404). Mounted first so they win over the
        // records below, which is what lets one `State` describe both -- and
        // kept apart, because the adapter's answer differs: 403 skips that
        // repository, 401 ends the run (ADR-0004).
        for (full_name, status, message) in state
            .forbidden
            .iter()
            .map(|name| (name, 403, "user does not have permission"))
            .chain(
                state
                    .revoked
                    .iter()
                    .map(|name| (name, 401, "user does not have permission")),
            )
            .chain(state.missing.iter().map(|name| (name, 404, "not found")))
        {
            authed(
                Mock::given(method("GET")).and(path_prefix(&format!("/api/v1/repos/{full_name}"))),
            )
            .respond_with(
                ResponseTemplate::new(status).set_body_json(json!({ "message": message })),
            )
            .with_priority(FORBIDDEN_PRIORITY)
            .mount(&self.server)
            .await;
        }

        for full_name in &state.read_once {
            let record = state
                .repos
                .iter()
                .find(|r| r["full_name"] == json!(full_name))
                .expect("a read_once entry names a repository in the state")
                .clone();
            authed(Mock::given(method("GET")).and(path(format!("/api/v1/repos/{full_name}"))))
                .respond_with(ResponseTemplate::new(200).set_body_json(record))
                .up_to_n_times(1)
                .with_priority(FORBIDDEN_PRIORITY - 1)
                .mount(&self.server)
                .await;
        }

        // What Gitea 1.27.3 answers a repository that cannot have pull
        // requests -- the unit switched off, a pull mirror, or no commits yet
        // (`CanEnablePulls` is `!IsMirror && !IsEmpty`): `/pulls` is a bare
        // 404, the same body as a repository that is not there. An empty
        // repository's branch listing is `200 null`, not `[]`. Both measured
        // against the container, 2026-10-12, and both read from the record
        // itself so a fixture cannot describe a repository the server would
        // answer differently.
        for repo in &state.repos {
            let full_name = repo["full_name"]
                .as_str()
                .expect("a repository record has a full_name");
            let empty = repo["empty"] == json!(true);
            if empty || repo["mirror"] == json!(true) || repo["has_pull_requests"] == json!(false) {
                authed(
                    Mock::given(method("GET"))
                        .and(path(format!("/api/v1/repos/{full_name}/pulls"))),
                )
                .respond_with(
                    ResponseTemplate::new(404).set_body_json(json!({ "message": "not found" })),
                )
                .with_priority(FORBIDDEN_PRIORITY)
                .mount(&self.server)
                .await;
            }
            if empty {
                authed(
                    Mock::given(method("GET"))
                        .and(path(format!("/api/v1/repos/{full_name}/branches"))),
                )
                .respond_with(ok(Value::Null))
                .with_priority(FORBIDDEN_PRIORITY)
                .mount(&self.server)
                .await;
            }
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
        for (full_name, (status, body)) in &state.teams {
            authed(
                Mock::given(method("GET")).and(path(format!("/api/v1/repos/{full_name}/teams"))),
            )
            .respond_with(ResponseTemplate::new(*status).set_body_json(body.clone()))
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
            // Every name at once, as git answers it whatever the page size
            // (#1716), and 404 where there is none; a walk of more than one
            // page reads it.
            let refs: Vec<Value> = branches
                .iter()
                .map(|b| {
                    json!({
                        "ref": format!("refs/heads/{}", b["name"].as_str().unwrap_or_default()),
                        "object": { "type": "commit", "sha": b["commit"]["id"] },
                    })
                })
                .collect();
            let answer = if refs.is_empty() {
                ResponseTemplate::new(404).set_body_json(json!({ "message": "not found" }))
            } else {
                ok(json!(refs))
            };
            authed(
                Mock::given(method("GET"))
                    .and(path(format!("/api/v1/repos/{full_name}/git/refs/heads"))),
            )
            .respond_with(answer)
            .mount(&self.server)
            .await;
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
            // One pull request read whole, as Gitea's `GET /pulls/{index}`
            // answers the record the listing serves (#2098's refresh).
            for record in pulls {
                let number = &record["number"];
                authed(
                    Mock::given(method("GET"))
                        .and(path(format!("/api/v1/repos/{full_name}/pulls/{number}"))),
                )
                .respond_with(ok(record.clone()))
                .mount(&self.server)
                .await;
            }
        }
        for (full_name, status) in &state.issues_status {
            authed(
                Mock::given(method("GET")).and(path(format!("/api/v1/repos/{full_name}/issues"))),
            )
            .respond_with(
                ResponseTemplate::new(*status).set_body_json(json!({ "message": "refused" })),
            )
            .with_priority(FORBIDDEN_PRIORITY)
            .mount(&self.server)
            .await;
        }
        for (full_name, issues) in &state.issues {
            authed(
                Mock::given(method("GET")).and(path(format!("/api/v1/repos/{full_name}/issues"))),
            )
            .respond_with(Issues {
                records: issues.clone(),
                page_size,
            })
            .mount(&self.server)
            .await;
            for record in issues {
                let number = &record["number"];
                authed(
                    Mock::given(method("GET"))
                        .and(path(format!("/api/v1/repos/{full_name}/issues/{number}"))),
                )
                .respond_with(ok(record.clone()))
                .mount(&self.server)
                .await;
            }
        }
        // Ahead of the vanished items' own 404s, so a fault wins over them.
        for (key, status) in &state.confirm_status {
            let route = match (key.split_once('#'), key.split_once('@')) {
                (Some((full_name, index)), _) => format!("/api/v1/repos/{full_name}/pulls/{index}"),
                (None, Some((full_name, branch))) => format!(
                    "/api/v1/repos/{full_name}/branches/{}",
                    branch.replace('/', "%2F")
                ),
                // A bare `owner/repo`: the repository's own read (#728).
                (None, None) if key.contains('/') => format!("/api/v1/repos/{key}"),
                _ => panic!("confirm key is owner/repo#index, owner/repo@branch or owner/repo"),
            };
            authed(Mock::given(method("GET")).and(path(route)))
                .respond_with(
                    ResponseTemplate::new(*status)
                        .set_body_json(json!({ "message": "internal server error" })),
                )
                .with_priority(FORBIDDEN_PRIORITY - 1)
                .mount(&self.server)
                .await;
        }
        for key in &state.vanished_pulls {
            let (full_name, index) = key
                .split_once('#')
                .expect("vanished pull key is owner/repo#index");
            authed(Mock::given(method("GET")).and(path(format!(
                "/api/v1/repos/{full_name}/issues/{index}/comments"
            ))))
            .respond_with(ResponseTemplate::new(500).set_body_json(json!({
                "message": format!("issue does not exist [id: 0, repo_id: 1, index: {index}]")
            })))
            .with_priority(FORBIDDEN_PRIORITY)
            .mount(&self.server)
            .await;
            authed(
                Mock::given(method("GET"))
                    .and(path(format!("/api/v1/repos/{full_name}/pulls/{index}"))),
            )
            .respond_with(
                ResponseTemplate::new(404).set_body_json(json!({ "message": "not found" })),
            )
            .with_priority(FORBIDDEN_PRIORITY)
            .mount(&self.server)
            .await;
        }
        for key in &state.vanished_branches {
            let (full_name, branch) = key
                .split_once('@')
                .expect("vanished branch key is owner/repo@branch");
            let wire = branch.replace('/', "%2F");
            authed(
                Mock::given(method("GET"))
                    .and(path(format!("/api/v1/repos/{full_name}/branches/{wire}"))),
            )
            .respond_with(
                ResponseTemplate::new(404).set_body_json(json!({ "message": "not found" })),
            )
            .with_priority(FORBIDDEN_PRIORITY)
            .mount(&self.server)
            .await;
        }
        for key in state.vanished_branches.iter().chain(&state.history_missing) {
            let (full_name, branch) = key
                .split_once('@')
                .expect("vanished branch key is owner/repo@branch");
            authed(
                Mock::given(method("GET"))
                    .and(path(format!("/api/v1/repos/{full_name}/commits")))
                    .and(query_param("sha", branch)),
            )
            .respond_with(ResponseTemplate::new(404).set_body_json(json!({
                "message": format!("object does not exist [id: {branch}, rel_path: ]")
            })))
            .with_priority(FORBIDDEN_PRIORITY)
            .mount(&self.server)
            .await;
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

        // Each commit on its own, and its diff, for the source view (#661).
        // The record is the listing's, which carries everything the view
        // reads but `parents` -- absent here, so the view draws no parent
        // rows; `tests/it/view.rs` serves its own record for the rest.
        for (key, commits) in &state.commits {
            let (full_name, _) = key
                .split_once('@')
                .expect("commit key is owner/repo@branch");
            for record in commits {
                let sha = record["sha"].as_str().expect("a commit record has a sha");
                authed(
                    Mock::given(method("GET"))
                        .and(path(format!("/api/v1/repos/{full_name}/git/commits/{sha}"))),
                )
                .respond_with(ok(record.clone()))
                .mount(&self.server)
                .await;
                authed(
                    Mock::given(method("GET"))
                        .and(path(format!("/api/v1/repos/{full_name}/git/commits/{sha}.diff"))),
                )
                .respond_with(ResponseTemplate::new(200).set_body_string(format!(
                    "diff --git a/CHANGELOG.md b/CHANGELOG.md\nindex 1111111..2222222 100644\n\
                     --- a/CHANGELOG.md\n+++ b/CHANGELOG.md\n@@ -1 +1,2 @@\n # Changelog\n+- {sha}\n"
                )))
                .mount(&self.server)
                .await;
            }
        }

        // A branch and a repository's source views (#663): each branch on its
        // own, its compare with `main` both ways and as a diff, and a README
        // in the top directory. `tests/it/view_branch.rs` and `view_repo.rs`
        // serve their own records for the edge cases; this is what the
        // contract battery's clause 9 reads every branch and repository by.
        for (full_name, branches) in &state.branches {
            let commits_of = |name: &str| {
                state
                    .commits
                    .get(&format!("{full_name}@{name}"))
                    .cloned()
                    .unwrap_or_default()
            };
            for record in branches {
                let name = record["name"].as_str().expect("a branch record has a name");
                let wire = name.replace('/', "%2F");
                authed(
                    Mock::given(method("GET"))
                        .and(path(format!("/api/v1/repos/{full_name}/branches/{wire}"))),
                )
                .respond_with(ok(record.clone()))
                .mount(&self.server)
                .await;
                if name == "main" {
                    continue;
                }
                let ahead = commits_of(name);
                authed(Mock::given(method("GET")).and(path(format!(
                    "/api/v1/repos/{full_name}/compare/main...{wire}"
                ))))
                .and(query_param("output", "diff"))
                .respond_with(ResponseTemplate::new(200).set_body_string(format!(
                    "diff --git a/{name}.txt b/{name}.txt\nnew file mode 100644\n\
                     index 0000000..2222222\n--- /dev/null\n+++ b/{name}.txt\n@@ -0,0 +1 @@\n+{name}\n"
                )))
                .with_priority(4)
                .mount(&self.server)
                .await;
                authed(Mock::given(method("GET")).and(path(format!(
                    "/api/v1/repos/{full_name}/compare/main...{wire}"
                ))))
                .respond_with(ok(
                    json!({ "total_commits": ahead.len(), "commits": ahead }),
                ))
                .mount(&self.server)
                .await;
                // Behind, as Gitea counts it: `X-Total-Count` on a
                // one-commit page of `/commits?sha=main&not={branch}`.
                let behind = commits_of("main");
                authed(
                    Mock::given(method("GET"))
                        .and(path(format!("/api/v1/repos/{full_name}/commits")))
                        .and(query_param("sha", "main"))
                        .and(query_param("not", name))
                        .and(query_param("limit", "1")),
                )
                .respond_with(
                    ok(json!(behind.iter().take(1).collect::<Vec<_>>()))
                        .insert_header("X-Total-Count", behind.len().to_string().as_str()),
                )
                .with_priority(4)
                .mount(&self.server)
                .await;
            }
            authed(
                Mock::given(method("GET")).and(path(format!("/api/v1/repos/{full_name}/contents"))),
            )
            .respond_with(ok(json!([
                { "name": "README.md", "type": "file", "path": "README.md", "size": 20 }
            ])))
            .mount(&self.server)
            .await;
            authed(
                Mock::given(method("GET"))
                    .and(path(format!("/api/v1/repos/{full_name}/raw/README.md"))),
            )
            .respond_with(ResponseTemplate::new(200).set_body_string(format!("# {full_name}\n")))
            .mount(&self.server)
            .await;
        }

        // A branch read on its own that the state does not hold is Gitea's
        // 404, not the empty page below: the walk re-reads a branch the
        // listing did not return before tombstoning it (#1624), and a `[]`
        // here would read as "still there".
        authed(
            Mock::given(method("GET")).and(path_regex(r"^/api/v1/repos/[^/]+/[^/]+/branches/.+$")),
        )
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({ "message": "not found" })))
        .with_priority(EMPTY_PAGE_PRIORITY - 1)
        .mount(&self.server)
        .await;

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

/// A URL nothing answers on (#1815): port 1 on the loopback, refused at once.
///
/// A fixed port, because a port nobody else can be handed is the point. One
/// bound to `0` and dropped was free again at once, and a mock server another
/// test started could take it and answer `404` where the test expected nothing
/// at all. Port 1 is below every range a bind to `0` hands out (Linux and
/// macOS alike), and privileged on Linux. Holding a port bound and never
/// listened on instead is refused on Linux but silently dropped on macOS,
/// where every connect then waits out its timeout.
///
/// Checked once per process, so a machine where something does answer there
/// fails here, naming the port, rather than in whichever test asked.
pub fn dead_url() -> String {
    static REFUSED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    REFUSED.get_or_init(|| {
        let refused = std::net::TcpStream::connect(("127.0.0.1", 1))
            .is_err_and(|error| error.kind() == std::io::ErrorKind::ConnectionRefused);
        assert!(
            refused,
            "something answers on 127.0.0.1:1, the port dead_url names"
        );
    });
    "http://127.0.0.1:1".to_owned()
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
        account: None,
        config,
    }
}

/// The adapter under test, built against the fake.
pub fn source(base_url: String, config: Value) -> Box<dyn Source> {
    knobas_source_gitea::build(instance(base_url, TOKEN, config)).expect("the adapter builds")
}

/// How many of a fake's requests are in flight at once (#703): the witness
/// that a view reads its sections side by side, which does not depend on how
/// fast the machine is.
///
/// Every answer is held for [`InFlight::delay`], so a request is in flight
/// for at least that long, and a client that waits for each answer before
/// its next request cannot have two arrive within it: read one after another,
/// [`InFlight::most`] is exactly 1. Arrivals are counted together only within
/// half the delay, so a busy machine can make side-by-side reads look fewer
/// but never makes reads in turn look side by side.
pub struct InFlight {
    delay: Duration,
    arrivals: Mutex<Vec<Instant>>,
    most: AtomicUsize,
}

impl InFlight {
    pub fn new(delay: Duration) -> Arc<Self> {
        Arc::new(Self {
            delay,
            arrivals: Mutex::new(Vec::new()),
            most: AtomicUsize::new(0),
        })
    }

    /// How long every answer of the fake watched is held.
    pub fn delay(&self) -> Duration {
        self.delay
    }

    /// The most requests that were in flight at the same time.
    pub fn most(&self) -> usize {
        self.most.load(Ordering::SeqCst)
    }

    /// Watch every request `server` gets. Mounted before the fake's own mocks:
    /// it is the first one wiremock asks, once per request, and it never
    /// answers.
    pub async fn watch(self: &Arc<Self>, server: &MockServer) {
        let flight = Arc::clone(self);
        Mock::given(move |_: &wiremock::Request| {
            flight.arrive();
            false
        })
        .respond_with(ResponseTemplate::new(500))
        .with_priority(1)
        .mount(server)
        .await;
    }

    fn arrive(&self) {
        let now = Instant::now();
        let mut arrivals = self.arrivals.lock().expect("not poisoned");
        arrivals.retain(|at| now.duration_since(*at) < self.delay / 2);
        arrivals.push(now);
        self.most.fetch_max(arrivals.len(), Ordering::SeqCst);
    }
}

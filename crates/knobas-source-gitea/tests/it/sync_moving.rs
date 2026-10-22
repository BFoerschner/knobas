//! Walks over a Gitea whose listings change while they are read (#1624, spec
//! #1618): a push reorders the repository and branch listings between two
//! pages, and an incremental run meets more changes than its budget affords.
//!
//! The fake here sorts every answer the way Gitea does, by the request's own
//! `sort`/`order`, and runs a hook after a named page has been served -- so a
//! walk over an order that a push can move shows the push, and a walk over an
//! order it cannot move does not. `support::Fake` serves fixed pages, which
//! is why these tests carry their own.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use knobas_source::contract::VecSink;
use knobas_source::{Source, SyncItem};
use serde_json::{Value, json};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

use crate::support::{self, TOKEN, source};

/// A change the remote makes once the walk has read `page` of `path`.
type Change = Box<dyn FnOnce(&mut World) + Send>;

/// Everything the fake serves, mutable between pages and between runs.
#[derive(Default)]
pub struct World {
    pub repos: Vec<Value>,
    /// `owner/repo` -> branch records, served newest commit first as Gitea
    /// orders its branch listing.
    pub branches: BTreeMap<String, Vec<Value>>,
    /// `owner/repo` -> pull requests, served `sort=recentupdate`.
    pub pulls: BTreeMap<String, Vec<Value>>,
    /// `owner/repo@branch` -> commits, newest first, filtered by `since` and
    /// `until` (both inclusive, on `created`).
    pub commits: BTreeMap<String, Vec<Value>>,
    /// Every listing request served, as `path?page=N`.
    pub served: Vec<String>,
    /// At most this many records a page whatever the request asked for, as
    /// a Gitea with a lowered `MAX_RESPONSE_ITEMS` answers (issue #81). Keeps
    /// a three-page listing small enough to walk under the rate limiter.
    pub cap: Option<usize>,
    hooks: Vec<(String, u32, Change)>,
}

impl World {
    /// Run `change` once the walk has read `page` of `path` (under `/api/v1`).
    pub fn after(&mut self, path: &str, page: u32, change: impl FnOnce(&mut World) + Send + 'static) {
        self.hooks.push((path.to_owned(), page, Box::new(change)));
    }
}

#[derive(Clone)]
pub struct Moving(Arc<Mutex<World>>);

impl Moving {
    pub async fn start(world: World) -> (MockServer, Self) {
        let server = MockServer::builder().start().await;
        let moving = Self(Arc::new(Mutex::new(world)));
        Mock::given(wiremock::matchers::any())
            .respond_with(moving.clone())
            .mount(&server)
            .await;
        (server, moving)
    }

    pub fn with<T>(&self, f: impl FnOnce(&mut World) -> T) -> T {
        f(&mut self.0.lock().expect("the world is not poisoned"))
    }
}

fn ok(body: Value) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(body)
}

fn not_found() -> ResponseTemplate {
    ResponseTemplate::new(404).set_body_json(json!({ "message": "not found" }))
}

fn text<'a>(record: &'a Value, pointer: &str) -> &'a str {
    record.pointer(pointer).and_then(Value::as_str).unwrap_or_default()
}

fn page_of(records: Vec<Value>, page: u32, size: usize) -> Value {
    json!(
        records
            .chunks(size)
            .nth(page as usize - 1)
            .map(<[Value]>::to_vec)
            .unwrap_or_default()
    )
}

impl wiremock::Respond for Moving {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let authed = request
            .headers
            .get("Authorization")
            .is_some_and(|v| v.to_str().ok() == Some(&format!("token {TOKEN}")));
        if !authed {
            return ResponseTemplate::new(401).set_body_json(json!({ "message": "token does not exist" }));
        }
        let query: BTreeMap<String, String> = request.url.query_pairs().into_owned().collect();
        let page: u32 = query.get("page").and_then(|p| p.parse().ok()).unwrap_or(1);
        let asked: usize = query.get("limit").and_then(|l| l.parse().ok()).unwrap_or(50);
        let path = request.url.path().trim_start_matches("/api/v1/").to_owned();
        let segments: Vec<String> = path
            .split('/')
            .map(|s| s.replace("%2F", "/"))
            .collect();
        let mut world = self.0.lock().expect("the world is not poisoned");
        let size = world.cap.map_or(asked, |cap| cap.min(asked));

        let answer = match segments.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
            ["user"] => ok(json!({ "login": "mara", "id": 7 })),
            ["repos", "search"] => {
                let mut repos = world.repos.clone();
                let key = query.get("sort").map_or("alpha", String::as_str).to_owned();
                repos.sort_by(|a, b| match key.as_str() {
                    "id" => a["id"].as_u64().cmp(&b["id"].as_u64()),
                    "updated" => text(a, "/updated_at").cmp(text(b, "/updated_at")),
                    "created" => text(a, "/created_at").cmp(text(b, "/created_at")),
                    _ => text(a, "/full_name").cmp(text(b, "/full_name")),
                });
                if query.get("order").map(String::as_str) == Some("desc") {
                    repos.reverse();
                }
                ok(json!({ "ok": true, "data": page_of(repos, page, size) }))
            }
            ["repos", owner, name] => world
                .repos
                .iter()
                .find(|r| text(r, "/full_name") == format!("{owner}/{name}"))
                .map_or_else(not_found, |r| ok(r.clone())),
            ["repos", owner, name, "branches"] => {
                let mut branches = world
                    .branches
                    .get(&format!("{owner}/{name}"))
                    .cloned()
                    .unwrap_or_default();
                branches.sort_by(|a, b| {
                    text(b, "/commit/timestamp")
                        .cmp(text(a, "/commit/timestamp"))
                        .then_with(|| text(a, "/name").cmp(text(b, "/name")))
                });
                ok(page_of(branches, page, size))
            }
            ["repos", owner, name, "branches", branch] => world
                .branches
                .get(&format!("{owner}/{name}"))
                .and_then(|all| all.iter().find(|b| text(b, "/name") == *branch))
                .map_or_else(not_found, |b| ok(b.clone())),
            ["repos", owner, name, "pulls"] => {
                let mut pulls = world
                    .pulls
                    .get(&format!("{owner}/{name}"))
                    .cloned()
                    .unwrap_or_default();
                pulls.sort_by(|a, b| {
                    text(b, "/updated_at")
                        .cmp(text(a, "/updated_at"))
                        .then_with(|| b["number"].as_u64().cmp(&a["number"].as_u64()))
                });
                ok(page_of(pulls, page, size))
            }
            ["repos", owner, name, "commits"] => {
                let sha = query.get("sha").cloned().unwrap_or_default();
                let since = query.get("since").cloned();
                let until = query.get("until").cloned();
                let commits: Vec<Value> = world
                    .commits
                    .get(&format!("{owner}/{name}@{sha}"))
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|c| {
                        let at = text(c, "/created").to_owned();
                        since.as_ref().is_none_or(|s| at >= *s) && until.as_ref().is_none_or(|u| at <= *u)
                    })
                    .collect();
                ok(page_of(commits, page, size))
            }
            // Teams, issues and anything else: nothing to list.
            _ => ok(json!([])),
        };

        world.served.push(format!("{path}?page={page}"));
        let due: Vec<Change> = {
            let hooks = std::mem::take(&mut world.hooks);
            let (now, later): (Vec<_>, Vec<_>) =
                hooks.into_iter().partition(|(at, p, _)| *at == path && *p == page);
            world.hooks = later;
            now.into_iter().map(|(_, _, change)| change).collect()
        };
        for change in due {
            change(&mut world);
        }
        answer
    }
}

/// An instant on the fixture's clock, `seconds` after its start.
pub fn at(seconds: i64) -> String {
    (chrono::DateTime::parse_from_rfc3339("2026-10-01T08:00:00Z").expect("RFC 3339")
        + chrono::Duration::seconds(seconds))
    .with_timezone(&chrono::Utc)
    .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

pub fn sha(n: u64) -> String {
    format!("{n:040x}")
}

pub async fn run(source: &dyn Source, cursor: Option<&str>) -> (Vec<SyncItem>, String) {
    let mut sink = VecSink(Vec::new());
    let next = source
        .sync(cursor.map(str::to_owned), &mut sink)
        .await
        .expect("the run succeeds");
    (sink.0, next)
}

/// The entities of `kind` emitted, each as often as it was emitted.
pub fn emitted(items: &[SyncItem], kind: &str) -> Vec<String> {
    let mut out: Vec<String> = items
        .iter()
        .filter(|i| i.kind == kind && !i.deleted)
        .map(|i| i.entity.to_string())
        .collect();
    out.sort();
    out
}

pub fn tombstoned(items: &[SyncItem], kind: &str) -> BTreeSet<String> {
    items
        .iter()
        .filter(|i| i.kind == kind && i.deleted)
        .map(|i| i.entity.to_string())
        .collect()
}

/// One repository's position in a cursor, as stored.
pub fn position(cursor: &str, full_name: &str) -> Value {
    let cursor: Value = serde_json::from_str(cursor).expect("the cursor is JSON");
    cursor["repos"][full_name].clone()
}

pub fn gitea(server: &MockServer, config: Value) -> Box<dyn Source> {
    source(server.uri(), config)
}

pub fn repository(id: u64, name: &str, updated: &str) -> Value {
    let mut record = support::repo("tidewater", name, updated);
    record["id"] = json!(id);
    record["created_at"] = json!(at(i64::try_from(id).expect("small")));
    record
}

/// 30 repositories, `svc-001` to `svc-030`, ids in that order and each
/// updated a minute after the one before, listed ten to a page.
fn thirty() -> World {
    World {
        cap: Some(10),
        repos: (1..=30)
            .map(|id| repository(id, &format!("svc-{id:03}"), &at(60 * i64::try_from(id).expect("small"))))
            .collect(),
        ..World::default()
    }
}

fn repo_entity(id: u64) -> String {
    format!("gitea:tidewater/svc-{id:03}")
}

/// ADPT-3, the reproduction. A push lands in `svc-005` while page 1 of the
/// listing is being read. Under `sort=updated&order=desc` that moves it to the
/// top, onto the page already read, and shifts `svc-021` onto page 2: the run
/// would walk `svc-021` twice and `svc-005` not at all, return `Ok`, and hand
/// the sweep a corpus without it. Over a key a push cannot move, every
/// repository is walked once.
#[tokio::test]
async fn a_push_during_the_repository_walk_hides_no_repository() {
    let mut world = thirty();
    world.after("repos/search", 1, |world| {
        let pushed = world
            .repos
            .iter_mut()
            .find(|r| r["full_name"] == "tidewater/svc-005")
            .expect("svc-005 is in the fixture");
        pushed["updated_at"] = json!(at(100_000));
    });
    let (server, _) = Moving::start(world).await;

    let (items, _) = run(&*gitea(&server, json!({})), None).await;
    let expected: Vec<String> = {
        let mut all: Vec<String> = (1..=30).map(repo_entity).collect();
        all.sort();
        all
    };
    assert_eq!(emitted(&items, "repo"), expected, "every repository, once each");
}

/// A row the walk meets on two pages is walked once. Here a repository older
/// than every other one (`id` 0) is shared with the token while page 1 is
/// read: it lands ahead of the page already read and pushes `svc-010` onto
/// page 2, where the walk meets it again.
#[tokio::test]
async fn a_repository_met_on_two_pages_is_walked_once() {
    let mut world = thirty();
    world.after("repos/search", 1, |world| {
        world.repos.push(repository(0, "svc-000", &at(0)));
    });
    let (server, moving) = Moving::start(world).await;

    let (items, cursor) = run(&*gitea(&server, json!({})), None).await;
    let walked = emitted(&items, "repo");
    assert_eq!(
        walked.iter().filter(|e| **e == repo_entity(10)).count(),
        1,
        "svc-010 is met on pages 1 and 2 and walked once"
    );
    assert_eq!(walked.len(), 30, "{walked:?}");

    // The one shared mid-walk is the next run's.
    let (next, _) = run(&*gitea(&server, json!({})), Some(&cursor)).await;
    assert_eq!(emitted(&next, "repo"), vec![repo_entity(0)]);
    assert!(
        moving.with(|w| w.served.iter().any(|s| s == "repos/search?page=4")),
        "the walk ends on the empty page past the last"
    );
}

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
    pub fn after(
        &mut self,
        path: &str,
        page: u32,
        change: impl FnOnce(&mut World) + Send + 'static,
    ) {
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
    record
        .pointer(pointer)
        .and_then(Value::as_str)
        .unwrap_or_default()
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
            return ResponseTemplate::new(401)
                .set_body_json(json!({ "message": "token does not exist" }));
        }
        let query: BTreeMap<String, String> = request.url.query_pairs().into_owned().collect();
        let page: u32 = query.get("page").and_then(|p| p.parse().ok()).unwrap_or(1);
        let asked: usize = query
            .get("limit")
            .and_then(|l| l.parse().ok())
            .unwrap_or(50);
        let path = request.url.path().trim_start_matches("/api/v1/").to_owned();
        let segments: Vec<String> = path.split('/').map(|s| s.replace("%2F", "/")).collect();
        let mut world = self.0.lock().expect("the world is not poisoned");
        let size = world.cap.map_or(asked, |cap| cap.min(asked));

        let answer = match segments
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .as_slice()
        {
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
                        since.as_ref().is_none_or(|s| at >= *s)
                            && until.as_ref().is_none_or(|u| at <= *u)
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
            let (now, later): (Vec<_>, Vec<_>) = hooks
                .into_iter()
                .partition(|(at, p, _)| *at == path && *p == page);
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
            .map(|id| {
                repository(
                    id,
                    &format!("svc-{id:03}"),
                    &at(60 * i64::try_from(id).expect("small")),
                )
            })
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
    assert_eq!(
        emitted(&items, "repo"),
        expected,
        "every repository, once each"
    );
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

const MONOREPO: &str = "tidewater/monorepo";

/// `tidewater/monorepo` with 25 branches, `b-00` to `b-24`, each a minute
/// newer than the one before, listed ten to a page newest commit first.
fn twenty_five_branches() -> World {
    World {
        cap: Some(10),
        repos: vec![repository(1, "monorepo", &at(0))],
        branches: BTreeMap::from([(
            MONOREPO.to_owned(),
            (0..25u64)
                .map(|k| {
                    support::branch(
                        &format!("b-{k:02}"),
                        &sha(k + 1),
                        "work",
                        &at(60 * i64::try_from(k).expect("small")),
                    )
                })
                .collect(),
        )]),
        ..World::default()
    }
}

fn branch_entity(name: &str) -> String {
    format!("gitea:{MONOREPO}@refs/heads/{name}")
}

/// A push to `b-08`, which page 2 would list, while page 1 is read: Gitea's
/// branch listing is ordered by commit time, so `b-08` moves onto page 1 and
/// the walk never meets it. It is in the previous cursor, so it is read on
/// its own before anything is said about it -- and it is there, at its new
/// head: emitted, not tombstoned, and remembered.
#[tokio::test]
async fn a_branch_a_push_moved_off_the_listing_is_re_read_and_not_tombstoned() {
    let (server, moving) = Moving::start(twenty_five_branches()).await;
    let gitea = gitea(&server, json!({}));
    let (_, cursor) = run(&*gitea, None).await;

    moving.with(|world| {
        world.after(&format!("repos/{MONOREPO}/branches"), 1, |world| {
            let pushed = world
                .branches
                .get_mut(MONOREPO)
                .and_then(|all| all.iter_mut().find(|b| b["name"] == "b-08"))
                .expect("b-08 is in the fixture");
            *pushed = support::branch("b-08", &sha(900), "pushed", &at(100_000));
        });
    });
    let (items, after) = run(&*gitea, Some(&cursor)).await;

    assert!(
        tombstoned(&items, "branch").is_empty(),
        "{:?}",
        tombstoned(&items, "branch")
    );
    assert_eq!(
        emitted(&items, "branch"),
        vec![branch_entity("b-08")],
        "it moved"
    );
    assert_eq!(
        position(&after, MONOREPO)["branches"]["b-08"],
        json!(sha(900)),
        "and its new head is remembered"
    );
    assert!(
        moving.with(|w| w
            .served
            .iter()
            .any(|s| s.starts_with(&format!("repos/{MONOREPO}/branches/b-08")))),
        "read on its own"
    );
}

/// A branch the walk meets twice with two heads -- `b-24`, read on page 1,
/// force-pushed to an older commit that sorts it onto page 3 -- is mirrored at
/// the head the cursor remembers: the mirror is never left on one head while
/// the cursor holds the other, which would leave the move unnoticed for good.
/// `b-00` takes an ordinary push in the same run.
#[tokio::test]
async fn a_branch_met_twice_with_two_heads_is_mirrored_at_the_head_remembered() {
    let (server, moving) = Moving::start(twenty_five_branches()).await;
    let gitea = gitea(&server, json!({}));
    let (_, cursor) = run(&*gitea, None).await;

    moving.with(|world| {
        world.after(&format!("repos/{MONOREPO}/branches"), 1, |world| {
            let forced = world
                .branches
                .get_mut(MONOREPO)
                .and_then(|all| all.iter_mut().find(|b| b["name"] == "b-24"))
                .expect("b-24 is in the fixture");
            *forced = support::branch("b-24", &sha(700), "rewound", &at(60 * 2 + 30));
            // And an ordinary push on page 3, so the run emits something and
            // its cursor is written (an idle run returns the one it was
            // handed).
            let pushed = world
                .branches
                .get_mut(MONOREPO)
                .and_then(|all| all.iter_mut().find(|b| b["name"] == "b-00"))
                .expect("b-00 is in the fixture");
            *pushed = support::branch("b-00", &sha(800), "work", &at(1));
        });
    });
    let (during, cursor) = run(&*gitea, Some(&cursor)).await;
    let (after, cursor) = run(&*gitea, Some(&cursor)).await;

    let last = during
        .iter()
        .chain(&after)
        .filter(|i| i.entity.to_string() == branch_entity("b-24"))
        .last()
        .expect("the force-push is mirrored by one of the two runs");
    assert_eq!(last.payload["commit"]["id"], json!(sha(700)));
    assert_eq!(position(&cursor, MONOREPO)["branches"]["b-24"], json!(sha(700)));
}

/// A branch the listing did not return and its own read answers 404 for is
/// gone, and tombstoned.
#[tokio::test]
async fn a_branch_the_server_no_longer_has_is_tombstoned() {
    let (server, moving) = Moving::start(twenty_five_branches()).await;
    let gitea = gitea(&server, json!({}));
    let (_, cursor) = run(&*gitea, None).await;

    moving.with(|world| {
        world
            .branches
            .get_mut(MONOREPO)
            .expect("the fixture has branches")
            .retain(|b| b["name"] != "b-03");
    });
    let (items, after) = run(&*gitea, Some(&cursor)).await;

    assert_eq!(
        tombstoned(&items, "branch"),
        BTreeSet::from([branch_entity("b-03")])
    );
    assert!(position(&after, MONOREPO)["branches"].get("b-03").is_none());
}

const PAYOUT: &str = "tidewater/payout-service";
const DAY: i64 = 86_400;

fn instant(value: &Value) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339(value.as_str().expect("a timestamp"))
        .expect("RFC 3339")
        .with_timezone(&chrono::Utc)
}

fn pr_entity(number: u64) -> String {
    format!("gitea:{PAYOUT}#{number}")
}

fn seconds(n: u64) -> i64 {
    i64::try_from(n).expect("small")
}

/// When each of the 260 was last updated after the week away: 1-40 merged on
/// day one, 41-260 updated across days two to six.
fn edited(n: u64) -> String {
    if n <= 40 {
        at(DAY + seconds(n))
    } else {
        at(2 * DAY + 600 * seconds(n))
    }
}

/// 260 pull requests synced once (the budget of 200 kept the newest), then a
/// week away in which every one of them changed. The cursor is the one the
/// full sync left.
async fn a_week_away(cap: Option<usize>) -> (MockServer, Moving, Box<dyn Source>, String) {
    let world = World {
        cap,
        repos: vec![repository(1, "payout-service", &at(0))],
        pulls: BTreeMap::from([(
            PAYOUT.to_owned(),
            (1..=260u64)
                .map(|n| support::pull(n, "Work", "", &at(seconds(n)), 0))
                .collect(),
        )]),
        ..World::default()
    };
    let (server, moving) = Moving::start(world).await;
    let gitea = gitea(
        &server,
        json!({ "prs_per_repo": 200, "commits_per_repo": 0 }),
    );
    let (_, cursor) = run(&*gitea, None).await;
    moving.with(|world| {
        for pull in world.pulls.get_mut(PAYOUT).expect("the fixture has pulls") {
            let n = pull["number"].as_u64().expect("a number");
            pull["updated_at"] = json!(edited(n));
            if n <= 40 {
                pull["state"] = json!("closed");
                pull["merged"] = json!(true);
                pull["merged_at"] = json!(edited(n));
            }
        }
    });
    (server, moving, gitea, cursor)
}

fn every_pr() -> Vec<String> {
    let mut all: Vec<String> = (1..=260).map(pr_entity).collect();
    all.sort();
    all
}

/// ADPT-4 for pull requests. A week away: 260 pull requests updated since the
/// watermark, the 40 merged on day one the oldest of them, and a budget of
/// 200. The first incremental run reads the newest 200 and stops; its
/// position continues from the oldest it examined, so the second reads the
/// other 60 -- the 40 merged ones among them -- and the third is idle.
#[tokio::test]
async fn an_incremental_run_the_budget_stopped_is_continued_by_the_next() {
    let (_server, _moving, gitea, cursor) = a_week_away(None).await;

    let (first, cursor) = run(&*gitea, Some(&cursor)).await;
    let first_prs = emitted(&first, "pr");
    assert_eq!(first_prs.len(), 200, "the budget bounds the run");
    assert!(
        !first_prs.contains(&pr_entity(60)) && first_prs.contains(&pr_entity(61)),
        "the newest 200: 61 to 260"
    );
    assert_eq!(
        instant(&position(&cursor, PAYOUT)["pulls_catching_up"]["below"]),
        instant(&json!(edited(61))),
        "the next run continues below the oldest record this one examined"
    );
    assert_eq!(
        instant(&position(&cursor, PAYOUT)["pulls_updated_to"]),
        instant(&json!(at(260))),
        "and the watermark stays until the 60 below it are read"
    );

    let (second, cursor) = run(&*gitea, Some(&cursor)).await;
    let mut all: Vec<String> = first_prs;
    all.extend(emitted(&second, "pr"));
    all.sort();
    assert_eq!(all, every_pr(), "all 260 mirrored, each once");
    for n in 1..=40 {
        let item = second
            .iter()
            .find(|i| i.entity.to_string() == pr_entity(n))
            .unwrap_or_else(|| panic!("#{n} is read on the second run"));
        assert_eq!(item.payload["merged"], json!(true), "#{n} reads as merged");
    }

    let (idle, same) = run(&*gitea, Some(&cursor)).await;
    assert!(
        emitted(&idle, "pr").is_empty(),
        "{:?}",
        emitted(&idle, "pr")
    );
    assert_eq!(same, cursor, "an idle run returns its cursor");
    assert_eq!(
        instant(&position(&cursor, PAYOUT)["pulls_updated_to"]),
        instant(&json!(edited(260))),
        "caught up: the watermark is the newest delivered"
    );
    assert!(position(&cursor, PAYOUT).get("pulls_catching_up").is_none());
}

/// The same week against a Gitea answering ten a page: the 200 the first run
/// read fill the whole page cap, so a second run that walked down to its
/// window from page 1 would spend the cap skipping them and never get
/// there. It starts on the page the cut fell on.
#[tokio::test]
async fn a_catch_up_starts_on_the_page_the_budget_stopped_on() {
    let (_server, moving, gitea, cursor) = a_week_away(Some(10)).await;
    let (first, cursor) = run(&*gitea, Some(&cursor)).await;
    assert_eq!(
        position(&cursor, PAYOUT)["pulls_catching_up"]["page"],
        json!(20)
    );

    moving.with(|world| world.served.clear());
    let (second, _) = run(&*gitea, Some(&cursor)).await;
    let mut all = emitted(&first, "pr");
    all.extend(emitted(&second, "pr"));
    all.sort();
    assert_eq!(all, every_pr());
    let pages = moving.with(|world| {
        world
            .served
            .iter()
            .filter(|s| s.starts_with(&format!("repos/{PAYOUT}/pulls?")))
            .cloned()
            .collect::<Vec<_>>()
    });
    assert_eq!(
        pages.first().map(String::as_str),
        Some(format!("repos/{PAYOUT}/pulls?page=20").as_str()),
        "{pages:?}"
    );
}

/// A hint the server no longer honours is not trusted: the same week, but
/// the instance stops capping its pages between the two runs, so page 20 is
/// past the end of the listing. The catch-up starts again from page 1 and
/// still reads the 60.
#[tokio::test]
async fn a_catch_up_whose_page_moved_starts_from_the_first_page() {
    let (_server, moving, gitea, cursor) = a_week_away(Some(10)).await;
    let (first, cursor) = run(&*gitea, Some(&cursor)).await;
    moving.with(|world| world.cap = None);

    let (second, _) = run(&*gitea, Some(&cursor)).await;
    let mut all = emitted(&first, "pr");
    all.extend(emitted(&second, "pr"));
    all.sort();
    assert_eq!(all, every_pr());
}

/// A hint that lands below unread records is not trusted either: the
/// instance now answers twelve a page, so page 20 starts at the 229th pull
/// request and the 201st to the 228th sit above it, unread. Its first record
/// is older than the resume point, so the catch-up starts again from page 1
/// (and, the cap reached on the way, finishes on the run after).
#[tokio::test]
async fn a_catch_up_whose_page_lands_below_unread_records_starts_from_the_first_page() {
    let (_server, moving, gitea, cursor) = a_week_away(Some(10)).await;
    let (first, mut cursor) = run(&*gitea, Some(&cursor)).await;
    moving.with(|world| world.cap = Some(12));

    let mut all = emitted(&first, "pr");
    for _ in 0..3 {
        let (more, next) = run(&*gitea, Some(&cursor)).await;
        all.extend(emitted(&more, "pr"));
        cursor = next;
    }
    all.sort();
    assert_eq!(all, every_pr());
}

/// A pull request updated while the catch-up is under way is not held back
/// by it: the run that closes the window walks on from the top with what is
/// left of its budget.
#[tokio::test]
async fn the_run_that_closes_a_catch_up_also_reads_what_changed_since() {
    let (_server, moving, gitea, cursor) = a_week_away(None).await;
    let (_, cursor) = run(&*gitea, Some(&cursor)).await;
    moving.with(|world| {
        let pull = world
            .pulls
            .get_mut(PAYOUT)
            .and_then(|all| all.iter_mut().find(|p| p["number"] == 250))
            .expect("#250 is in the fixture");
        pull["updated_at"] = json!(at(9 * DAY));
        pull["title"] = json!("Work, reviewed");
    });

    let (second, cursor) = run(&*gitea, Some(&cursor)).await;
    let second_prs = emitted(&second, "pr");
    assert_eq!(second_prs.len(), 61, "the 60 and #250: {second_prs:?}");
    assert!(second_prs.contains(&pr_entity(250)));
    assert_eq!(
        instant(&position(&cursor, PAYOUT)["pulls_updated_to"]),
        instant(&json!(at(9 * DAY)))
    );
}

/// ADPT-4 for commits: a push of 150 commits on `main` against a budget of
/// 100. The first incremental run reads the newest 100 and stops; the second
/// reads the other 50 although `main`'s head has not moved since; the third
/// is idle.
#[tokio::test]
async fn an_incremental_commit_walk_the_budget_stopped_is_continued_by_the_next() {
    let key = format!("{PAYOUT}@main");
    let world = World {
        repos: vec![repository(1, "payout-service", &at(0))],
        branches: BTreeMap::from([(
            PAYOUT.to_owned(),
            vec![support::branch("main", &sha(1), "start", &at(0))],
        )]),
        commits: BTreeMap::from([(key.clone(), vec![support::commit(&sha(1), "start", &at(0))])]),
        ..World::default()
    };
    let (server, moving) = Moving::start(world).await;
    let gitea = gitea(
        &server,
        json!({ "prs_per_repo": 0, "commits_per_repo": 100 }),
    );
    let (_, cursor) = run(&*gitea, None).await;

    let pushed = |k: u64| (sha(1000 + k), at(60 * seconds(k)));
    moving.with(|world| {
        let history = world.commits.get_mut(&key).expect("main has history");
        for k in 1..=150 {
            let (id, when) = pushed(k);
            history.insert(0, support::commit(&id, "work", &when));
        }
        let (head, when) = pushed(150);
        world.branches.insert(
            PAYOUT.to_owned(),
            vec![support::branch("main", &head, "work", &when)],
        );
    });

    let (first, cursor) = run(&*gitea, Some(&cursor)).await;
    let first_commits = emitted(&first, "commit");
    assert_eq!(first_commits.len(), 100, "the budget bounds the run");
    assert_eq!(
        instant(&position(&cursor, PAYOUT)["commits_catching_up"]["below"]),
        instant(&json!(pushed(51).1)),
        "the next run continues below the oldest commit this one examined"
    );
    assert_eq!(
        instant(&position(&cursor, PAYOUT)["commits_since"]),
        instant(&json!(at(0))),
        "and the watermark stays until the 50 below it are read"
    );

    let (second, cursor) = run(&*gitea, Some(&cursor)).await;
    let mut all: Vec<String> = first_commits;
    all.extend(emitted(&second, "commit"));
    all.sort();
    let mut expected: Vec<String> = (1..=150)
        .map(|k| format!("gitea:{PAYOUT}@{}", pushed(k).0))
        .collect();
    expected.sort();
    assert_eq!(all, expected, "all 150 mirrored, each once");

    let (idle, same) = run(&*gitea, Some(&cursor)).await;
    assert!(idle.is_empty(), "{idle:?}");
    assert_eq!(same, cursor);
    assert_eq!(
        instant(&position(&cursor, PAYOUT)["commits_since"]),
        instant(&json!(pushed(150).1)),
        "caught up: the watermark is the newest delivered"
    );
}

/// The same cut, with a second branch the cut-short walk never started and a
/// push to `main` while the catch-up is pending. The next run finishes `main`'s
/// window, walks `feature` from its head, and then reads `main`'s new commit
/// from where the cut-short walk's newest commit stood -- not from `feature`'s
/// newer commits, which would pass over it.
#[tokio::test]
async fn a_commit_catch_up_finishes_every_branch_and_then_reads_what_moved() {
    let main = format!("{PAYOUT}@main");
    let feature = format!("{PAYOUT}@feature");
    let base = support::commit(&sha(1), "start", &at(0));
    let world = World {
        repos: vec![repository(1, "payout-service", &at(0))],
        branches: BTreeMap::from([(
            PAYOUT.to_owned(),
            vec![
                support::branch("main", &sha(1), "start", &at(0)),
                support::branch("feature", &sha(1), "start", &at(0)),
            ],
        )]),
        commits: BTreeMap::from([
            (main.clone(), vec![base.clone()]),
            (feature.clone(), vec![base]),
        ]),
        ..World::default()
    };
    let (server, moving) = Moving::start(world).await;
    let gitea = gitea(
        &server,
        json!({ "prs_per_repo": 0, "commits_per_repo": 100 }),
    );
    let (_, cursor) = run(&*gitea, None).await;

    // 150 on main up to 09:30, 30 on feature after that.
    let on_main = |k: u64| (sha(1000 + k), at(60 * seconds(k)));
    let on_feature = |j: u64| (sha(2000 + j), at(9_000 + 60 * seconds(j)));
    moving.with(|world| {
        for (key, name, pushed) in [
            (&main, "main", (1..=150).map(on_main).collect::<Vec<_>>()),
            (
                &feature,
                "feature",
                (1..=30).map(on_feature).collect::<Vec<_>>(),
            ),
        ] {
            let history = world.commits.get_mut(key).expect("a history");
            for (id, when) in &pushed {
                history.insert(0, support::commit(id, "work", when));
            }
            let (head, when) = pushed.last().expect("pushed").clone();
            let branches = world.branches.get_mut(PAYOUT).expect("branches");
            let branch = branches
                .iter_mut()
                .find(|b| b["name"] == name)
                .expect("listed");
            *branch = support::branch(name, &head, "work", &when);
        }
    });
    let (first, cursor) = run(&*gitea, Some(&cursor)).await;
    assert_eq!(emitted(&first, "commit").len(), 100);
    assert_eq!(
        position(&cursor, PAYOUT)["commits_catching_up"]["branches"],
        json!(["main", "feature"])
    );

    // A commit on main dated between main's newest and feature's.
    let late = (sha(3000), at(9_500));
    moving.with(|world| {
        world
            .commits
            .get_mut(&main)
            .expect("main")
            .insert(0, support::commit(&late.0, "late", &late.1));
        let branches = world.branches.get_mut(PAYOUT).expect("branches");
        let branch = branches
            .iter_mut()
            .find(|b| b["name"] == "main")
            .expect("main");
        *branch = support::branch("main", &late.0, "late", &late.1);
    });
    let (second, cursor) = run(&*gitea, Some(&cursor)).await;

    let mut all = emitted(&first, "commit");
    all.extend(emitted(&second, "commit"));
    all.sort();
    let mut expected: Vec<String> = (1..=150)
        .map(|k| on_main(k).0)
        .chain((1..=30).map(|j| on_feature(j).0))
        .chain([late.0.clone()])
        .map(|id| format!("gitea:{PAYOUT}@{id}"))
        .collect();
    expected.sort();
    assert_eq!(all, expected, "every commit, once each");
    assert!(
        position(&cursor, PAYOUT)
            .get("commits_catching_up")
            .is_none()
    );

    let (idle, same) = run(&*gitea, Some(&cursor)).await;
    assert!(idle.is_empty(), "{idle:?}");
    assert_eq!(same, cursor);
}

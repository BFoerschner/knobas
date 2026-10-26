//! Repositories walked side by side (#2159): `concurrent_repos` of them have
//! requests in flight at once, 1 is the walk one after another, and a fatal
//! error in one ends the run with nothing left walking.
//!
//! The witness is [`Held`], which holds each repository's first request
//! (`/teams`, read for a repository the run writes) open until the test lets
//! it go, so "in flight together" is counted, not timed. Each arrival is
//! stamped with how many repositories the run had finished by then (its walk
//! reading, #1379), so a walk that began before a slot was free is caught
//! whatever the machine's speed: at N at once, the i-th repository to begin
//! may only begin once i - N + 1 have finished.

use std::sync::Arc;
use std::time::Duration;

use crate::support::held::Held;
use crate::support::{Fake, State, repo, source};
use knobas_source::{Sink, Source, SourceError, SyncItem};

/// A deadline on waits that should end at once, so a regression fails instead
/// of hanging. Nothing is measured against it.
const DEADLINE: Duration = Duration::from_secs(20);

/// `tidewater/<name>` for every name, nothing in them but the repository.
fn repositories(names: &[&str]) -> State {
    let mut state = State::tidewater();
    state.repos = names
        .iter()
        .map(|name| repo("tidewater", name, "2026-08-22T11:42:00Z"))
        .collect();
    state
}

/// Hold every repository's `/teams` read, under the repository's name.
fn teams(path: &str) -> Option<String> {
    let rest = path.strip_prefix("/api/v1/repos/tidewater/")?;
    let name = rest.strip_suffix("/teams")?;
    Some(name.to_owned())
}

async fn within<T>(what: &str, work: impl Future<Output = T>) -> T {
    tokio::time::timeout(DEADLINE, work)
        .await
        .unwrap_or_else(|_| panic!("{what} did not happen"))
}

/// Collects what a run emits and tells [`Held`] each walk reading.
struct Stamping {
    held: Arc<Held>,
    items: Vec<SyncItem>,
}

#[async_trait::async_trait]
impl Sink for Stamping {
    async fn item(&mut self, item: SyncItem) -> Result<(), SourceError> {
        self.items.push(item);
        Ok(())
    }

    async fn walked(&mut self, done: u64, _of: Option<u64>, _unit: &str) {
        self.held.walked(done);
    }
}

/// Start a full sync of `source` on a task of its own, returning the repo
/// entities it emitted, sorted, or its error.
fn spawn_full(
    source: Box<dyn Source>,
    held: &Arc<Held>,
) -> tokio::task::JoinHandle<Result<Vec<String>, SourceError>> {
    let held = Arc::clone(held);
    tokio::spawn(async move {
        let mut sink = Stamping {
            held,
            items: Vec::new(),
        };
        source.sync(None, &mut sink).await?;
        let mut repos: Vec<String> = sink
            .items
            .iter()
            .filter(|item| item.kind == "repo")
            .map(|item| item.entity.to_string())
            .collect();
        repos.sort();
        Ok(repos)
    })
}

/// No repository's walk began before enough others had finished to free a
/// slot for it.
fn assert_never_more_than(held: &Held, at_once: usize) {
    for (i, (name, finished)) in held.arrivals().into_iter().enumerate() {
        let needed = u64::try_from((i + 1).saturating_sub(at_once)).expect("fits");
        assert!(
            finished >= needed,
            "{name} began walk {} with {finished} finished; at {at_once} at once that needs {needed}",
            i + 1
        );
    }
}

/// Release the held repositories one at a time, each time waiting for the
/// walk to open the next one, until every repository has been let through.
async fn release_all(held: &Held, total: usize, at_once: usize) {
    let mut released = 0;
    while released < total {
        let open = held.holding();
        let next = open.first().expect("a repository is held").clone();
        held.release(&next);
        released += 1;
        let arrived = (released + at_once).min(total);
        let holding = arrived - released;
        within(
            "the next repository's walk",
            held.until(|h| h.arrived().len() == arrived && h.holding().len() == holding),
        )
        .await;
    }
}

#[tokio::test]
async fn up_to_concurrent_repos_repositories_are_walked_at_once() {
    let names = ["r1", "r2", "r3", "r4", "r5", "r6", "r7"];
    let fake = Fake::start(&repositories(&names)).await;
    let held = Held::start(fake.base_url(), teams).await;
    let run = spawn_full(
        source(
            held.base_url(),
            serde_json::json!({ "concurrent_repos": 3 }),
        ),
        &held,
    );

    within(
        "three repositories in flight",
        held.until(|h| h.holding().len() == 3),
    )
    .await;
    release_all(&held, names.len(), 3).await;

    let repos = within("the run", run).await.expect("joins").expect("syncs");
    let expected: Vec<String> = names
        .iter()
        .map(|name| format!("gitea:tidewater/{name}"))
        .collect();
    assert_eq!(repos, expected, "every repository is mirrored once");
    assert_eq!(held.most(), 3, "three at once");
    assert_never_more_than(&held, 3);
}

/// At 1 the run is today's: one repository at a time, in name order.
#[tokio::test]
async fn at_one_the_repositories_are_walked_one_after_another_in_name_order() {
    let names = ["r1", "r2", "r3", "r4"];
    let fake = Fake::start(&repositories(&names)).await;
    let held = Held::start(fake.base_url(), teams).await;
    let run = spawn_full(
        source(
            held.base_url(),
            serde_json::json!({ "concurrent_repos": 1 }),
        ),
        &held,
    );

    within(
        "the first repository",
        held.until(|h| h.holding().len() == 1),
    )
    .await;
    release_all(&held, names.len(), 1).await;

    within("the run", run).await.expect("joins").expect("syncs");
    assert_eq!(held.most(), 1);
    assert_never_more_than(&held, 1);
    assert_eq!(held.arrived(), names.map(str::to_owned));
}

/// A fatal error in one repository ends the run with that error, and the
/// repositories still in flight are dropped: their held requests are hung up
/// on, rather than left to walk on behind a run that already failed.
#[tokio::test]
async fn a_fatal_error_in_one_repository_ends_the_run_and_every_other_walk() {
    let names = ["alpha", "bravo", "charlie"];
    let mut state = repositories(&names);
    // A 401 under one repository: a revoked credential, which ends the run.
    state.revoked.insert("tidewater/alpha".to_owned());
    let fake = Fake::start(&state).await;
    let held = Held::start(fake.base_url(), teams).await;
    let run = spawn_full(
        source(
            held.base_url(),
            serde_json::json!({ "concurrent_repos": 3 }),
        ),
        &held,
    );

    within(
        "all three in flight",
        held.until(|h| h.holding().len() == 3),
    )
    .await;
    held.release("alpha");

    let error = within("the run", run)
        .await
        .expect("joins")
        .expect_err("a revoked credential ends the run");
    assert!(
        matches!(error, SourceError::Unauthorized { .. }),
        "{error:?}"
    );
    within(
        "the other walks hung up",
        held.until(|h| h.abandoned().len() == 2),
    )
    .await;
    let mut abandoned = held.abandoned();
    abandoned.sort();
    assert_eq!(abandoned, ["bravo", "charlie"]);
}

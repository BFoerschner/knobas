//! Docker-free coverage for [`live_env::Litter`], the guard the live suites
//! clean up after themselves with.
//!
//! # Why this file exists next to a suite that already exercises the guard
//!
//! `tests/live_gitea.rs` drives `Litter` against the real container, but it is
//! `#[ignore]`d and needs Docker, a seeded Gitea and a token -- so nothing it
//! proves is proved by `just check`. Two of the guard's properties are load
//! bearing enough that they must not wait for someone to run `just gitea-live`:
//!
//! * **[`LITTER`] decides what gets deleted.** `Litter::clear_leftovers`
//!   deletes every branch of the mutated repository whose name starts with it,
//!   and every pull request opened from one of those. That is safe
//!   only while nothing `testenv/seed-gitea.sh` creates starts with it, and
//!   that was a fact somebody checked by hand once (PR #153) rather than a
//!   property anything re-checks. A fixture branch named `knobas-anything`
//!   would be deleted by the next live run and nothing would say so.
//! * **The removal of an earlier run's leftovers has no other witness.**
//!   Deleting the call from `Litter::new` breaks no live test: residue simply
//!   accumulates until `live_gitea_capped`'s `HEADROOM` refuses to start at 19
//!   pull requests, runs later and in another suite. A `Litter::new` that
//!   *asserted* on residue would be red on the run that inherits it and green
//!   on the retry, which is the shape Fable's #143 ruling refuses in advance --
//!   so the witness belongs here, against a stand-in whose state the test
//!   dictates, where there is no retry to be green on.
//!
//! * **The cleanup is bounded, so a wedged server says so.** `Litter::drop`
//!   waits on a thread that talks HTTP, and `reqwest` carries no default
//!   timeout, so a container that accepted the connection and then went quiet
//!   used to stall the run forever -- with nothing on screen, because a `Drop`
//!   that has not returned has not reported anything either. `just check` is
//!   the only gate now that CI is disabled, and a gate that hangs is the one
//!   failure shape a reader cannot act on. [`live_env::CLEANUP_BUDGET`] is the
//!   bound; the third test here is what proves it reports rather than stalls.
//!
//! Both run in `just check`: no container, no token, no `#[ignore]`.
//!
//! # What dropping a guard actually needs, since it is easy to get wrong
//!
//! Issue #162 was filed on the reading that `Litter::drop` waiting on a thread
//! would deadlock a `#[tokio::test]`, which is current-thread by default: the
//! test's only worker is parked, so a fake served on that same runtime can
//! never answer. **That is a real hazard and it is not this one.** wiremock
//! 0.6.5 runs each `MockServer` on a thread of its own with its own runtime
//! (`mock_server/bare_server.rs`, `std::thread::spawn` around a
//! `new_current_thread` runtime), so the fake here answers whether or not the
//! test's runtime is parked -- measured, not assumed: a guard holding a branch
//! drops against this fake in 20 ms.
//!
//! So a test in this file may own branches and let `Drop` run. What it may not
//! assume is that the server will answer: that is the case the bound covers,
//! and the one the third test drives. A fake served with `tokio::spawn` on the
//! *ambient* runtime would bring the original hazard back, and the symptom
//! would be [`live_env::CLEANUP_BUDGET`] elapsing -- a message, now, rather
//! than a hang.

mod live_env;

use std::sync::{Arc, Mutex};

use live_env::{Env, LITTER, Litter};
use serde_json::{Value, json};
use wiremock::{Mock, MockServer, Request, ResponseTemplate, matchers::any};

/// The repository root, from this crate's manifest directory.
fn root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the workspace root is two levels above this crate")
}

fn fixture() -> serde_json::Value {
    let path = root().join("fixtures/tidewater/work.json");
    serde_json::from_str(
        &std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display())),
    )
    .expect("fixtures/tidewater/work.json is JSON")
}

/// The head branches `seed-gitea.sh` invents for the two pull requests the
/// fixture gives no `from` for.
///
/// Read out of the script rather than copied here, so a third synthesized name
/// is covered the day it is added. The parse is deliberately narrow -- the
/// `echo`ed word of each arm of one `case` -- and asserts it found something,
/// so a rewrite that moves the names elsewhere fails loudly instead of
/// silently checking nothing.
fn synthesized_branch_names(script: &str) -> Vec<String> {
    let body = script
        .split_once("synth_branch() {")
        .expect("testenv/seed-gitea.sh defines synth_branch()")
        .1
        .split_once("\n}")
        .expect("synth_branch()'s body ends at a closing brace")
        .0;
    let names: Vec<String> = body
        .lines()
        .filter_map(|line| line.split_once("echo \""))
        .filter_map(|(_, rest)| rest.split_once('"'))
        .map(|(name, _)| name.to_owned())
        .filter(|name| !name.is_empty())
        .collect();
    assert!(
        !names.is_empty(),
        "no synthesized branch name was found in seed-gitea.sh's synth_branch(); the parse below \
         is checking nothing"
    );
    names
}

/// Every branch name `testenv/seed-gitea.sh` puts in the container, from the
/// three places it takes them from: the fixture's own `branches[]`, the head
/// branch each pull request names, and the two it synthesizes for the pull
/// requests that name none. Those are the script's two `ensure_branch` call
/// sites, and between them they are the only way a branch is created.
///
/// `main` comes from `auto_init`, not from a list. `repos[].default_branch` is
/// read anyway, as the one over-inclusion here: the script hardcodes
/// `default_branch:"main"` today rather than reading the field, so this covers
/// the day it starts reading it. Being a superset only makes the check
/// stricter, never blinder.
fn seeded_branch_names() -> Vec<String> {
    let fixture = fixture();
    let named = |rows: &serde_json::Value, field: &str| -> Vec<String> {
        rows.as_array()
            .expect("the fixture's collections are arrays")
            .iter()
            .filter_map(|row| row[field].as_str().map(str::to_owned))
            .collect()
    };
    let script = root().join("testenv/seed-gitea.sh");
    let script = std::fs::read_to_string(&script)
        .unwrap_or_else(|e| panic!("read {}: {e}", script.display()));

    let mut names = vec!["main".to_owned()];
    names.extend(named(&fixture["branches"], "name"));
    names.extend(named(&fixture["prs"], "from"));
    names.extend(named(&fixture["repos"], "default_branch"));
    names.extend(synthesized_branch_names(&script));
    names
}

/// Every string anywhere in a JSON document, however deeply nested.
fn strings(value: &serde_json::Value, into: &mut Vec<String>) {
    match value {
        serde_json::Value::String(text) => into.push(text.clone()),
        serde_json::Value::Array(rows) => rows.iter().for_each(|row| strings(row, into)),
        serde_json::Value::Object(fields) => fields.values().for_each(|field| strings(field, into)),
        _ => {}
    }
}

/// **The destructive rule, pinned.** `Litter::clear_leftovers` deletes every
/// branch of the mutated repository whose name starts with [`LITTER`], so a seeded branch
/// that started with it would be destroyed by the next `just gitea-live` and
/// re-created by the next seed, silently, forever.
///
/// Two assertions, because they fail for different reasons. The first knows
/// how `seed-gitea.sh` decides on branch names and checks exactly those. The
/// second knows nothing and checks every string in the fixture: `knobas-` is
/// this application's name, and the fixture is a fictional freight company, so
/// nothing in it should carry that prefix under any key at all. It is the one
/// that still holds when the fixture grows a field the first does not know
/// about -- which is precisely how a `knobas-`-prefixed branch would arrive.
#[test]
fn nothing_the_seed_puts_in_the_container_could_be_taken_for_this_suites_litter() {
    let seeded = seeded_branch_names();
    let collides: Vec<&String> = seeded.iter().filter(|n| n.starts_with(LITTER)).collect();
    assert!(
        collides.is_empty(),
        "testenv/seed-gitea.sh seeds {collides:?}, which start with {LITTER:?} -- the prefix \
         live_env::Litter::clear_leftovers deletes branches by. The next `just gitea-live` would \
         destroy them and the next seed would put them back, on every run, silently. Rename the \
         branch."
    );

    let mut everything = Vec::new();
    strings(&fixture(), &mut everything);
    let suspects: Vec<&String> = everything
        .iter()
        .filter(|s| s.starts_with(LITTER))
        .collect();
    assert!(
        suspects.is_empty(),
        "fixtures/tidewater/work.json carries {suspects:?}, which start with {LITTER:?} -- the \
         prefix live_env::Litter::clear_leftovers deletes branches by. Tidewater is a fictional freight \
         company and knobas- is this application's own name, so nothing in the fixture should \
         begin with it; if one of these is a branch name, the live suite will delete it."
    );
}

// ---------------------------------------------------------------------------
// The stand-in the guard's own deletes run against.
//
// `tests/support/mod.rs` cannot carry this: it serves the *adapter's* read
// routes and is deliberately static, and what has to be witnessed here is a
// server whose listings CHANGE because of a DELETE. So this is a small stateful
// fake of exactly four routes -- the two listings the guard reads and the two
// deletes it issues -- and the state it mutates is what the assertions read.
//
// Its fidelity is not assumed: `tests/live_gitea.rs` drives the same guard
// against the real pinned container, and the standing rule is that when a fake
// and the server disagree, the fake is wrong.
// ---------------------------------------------------------------------------

const TOKEN: &str = "tidewater-pat";
const OWNER: &str = "tidewater";
const REPO: &str = "payout-service";

/// One repository, as the four routes see it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Repository {
    /// Branch names, in listing order.
    branches: Vec<String>,
    /// `(index, head branch)` per pull request, whatever its state.
    pulls: Vec<(u64, String)>,
    /// Every DELETE the server was asked for, in the order it arrived --
    /// `issues/<index>` or `branches/<name>`. The order is a property here,
    /// not bookkeeping: see [`live_env`]'s `remove`.
    deleted: Vec<String>,
}

impl Repository {
    /// `tidewater/payout-service` as `testenv/seed-gitea.sh` leaves it, in the
    /// shape these four routes serve: the four branches and two pull requests
    /// `just gitea-live` measures before and after every run.
    fn seeded() -> Self {
        Self {
            branches: [
                "main",
                "feature/PAY-231-sepa-retry",
                "fix/PAY-228-partial-refund-drift",
                "feature/PAY-236-payout-csv-export",
            ]
            .map(str::to_owned)
            .to_vec(),
            pulls: vec![
                (142, "feature/PAY-231-sepa-retry".to_owned()),
                (144, "feature/PAY-236-payout-csv-export".to_owned()),
            ],
            deleted: Vec::new(),
        }
    }

    /// The same repository after a run that was killed mid-test: two branches
    /// it never took away, one of them carrying a pull request.
    fn after_a_killed_run() -> Self {
        let mut left = Self::seeded();
        left.branches.push(format!("{LITTER}live-9"));
        left.branches.push(format!("{LITTER}commit-9"));
        left.pulls.push((310, format!("{LITTER}live-9")));
        left
    }
}

struct Gitea {
    server: MockServer,
    state: Arc<Mutex<Repository>>,
}

impl Gitea {
    async fn holding(repository: Repository) -> Gitea {
        let state = Arc::new(Mutex::new(repository));
        let server = MockServer::start().await;
        let handler = Arc::clone(&state);
        let auth = format!("token {TOKEN}");
        let prefix = format!("/api/v1/repos/{OWNER}/{REPO}");
        Mock::given(any())
            .respond_with(move |request: &Request| {
                let presented = request
                    .headers
                    .get("Authorization")
                    .and_then(|value| value.to_str().ok());
                if presented != Some(auth.as_str()) {
                    return refused(401, "token does not exist");
                }
                let Some(route) = request.url.path().strip_prefix(prefix.as_str()) else {
                    return refused(404, "the guard reached outside the repository it was given");
                };
                let mut repository = handler.lock().expect("the fake's state is not poisoned");
                match (request.method.as_str(), route) {
                    ("GET", "/branches") => {
                        let names = page(&repository.branches, request);
                        ResponseTemplate::new(200)
                            .set_body_json(names.iter().map(|n| json!({ "name": n })).collect::<Vec<Value>>())
                    }
                    ("GET", "/pulls") => {
                        let pulls = page(&repository.pulls, request);
                        ResponseTemplate::new(200).set_body_json(
                            pulls
                                .iter()
                                .map(|(number, head)| json!({ "number": number, "head": { "ref": head } }))
                                .collect::<Vec<Value>>(),
                        )
                    }
                    ("DELETE", route) => {
                        repository.deleted.push(route.trim_start_matches('/').to_owned());
                        if let Some(name) = route.strip_prefix("/branches/") {
                            match repository.branches.iter().position(|b| b == name) {
                                Some(at) => {
                                    repository.branches.remove(at);
                                    ResponseTemplate::new(204)
                                }
                                None => refused(404, "no such branch"),
                            }
                        } else if let Some(index) = route.strip_prefix("/issues/") {
                            let index: u64 = index.parse().unwrap_or(0);
                            match repository.pulls.iter().position(|(n, _)| *n == index) {
                                Some(at) => {
                                    repository.pulls.remove(at);
                                    ResponseTemplate::new(204)
                                }
                                None => refused(404, "no such issue"),
                            }
                        } else {
                            refused(404, "not a route this fake serves")
                        }
                    }
                    _ => refused(404, "not a route this fake serves"),
                }
            })
            .mount(&server)
            .await;
        Gitea { server, state }
    }

    fn env(&self) -> Env {
        Env {
            url: self.server.uri(),
            token: TOKEN.to_owned(),
            owner: OWNER.to_owned(),
            repo: REPO.to_owned(),
        }
    }

    fn repository(&self) -> Repository {
        self.state
            .lock()
            .expect("the fake's state is not poisoned")
            .clone()
    }
}

fn refused(status: u16, message: &str) -> ResponseTemplate {
    ResponseTemplate::new(status).set_body_json(json!({ "message": message }))
}

/// The slice of `records` the request's `page`/`limit` asks for. The guard
/// pages until a page comes back empty, so a fake that ignored `page` would
/// hang it for 64 requests and then panic.
fn page<T: Clone>(records: &[T], request: &Request) -> Vec<T> {
    let number = |key: &str, fallback: usize| {
        request
            .url
            .query_pairs()
            .find(|(name, _)| name == key)
            .and_then(|(_, value)| value.parse().ok())
            .unwrap_or(fallback)
    };
    let (limit, page) = (number("limit", 50), number("page", 1).max(1));
    records
        .chunks(limit.max(1))
        .nth(page - 1)
        .map(<[T]>::to_vec)
        .unwrap_or_default()
}

/// **The removal of a killed run's leftovers, witnessed.** Deleting the call
/// from `Litter::new` reddens nothing in the live suite: residue only
/// accumulates, until `live_gitea_capped`'s `HEADROOM` refuses to start at 19
/// pull requests -- a different suite, several runs later. This is that
/// witness, and it costs no container.
///
/// Three assertions, three separate ways it could go wrong:
///
/// 1. the leftovers are gone -- the branches *and* the pull request opened
///    from one of them, which carries no `knobas-` name of its own;
/// 2. the seeded content is untouched, which is the destructive half: a
///    prefix match that widened would take fixture branches with it;
/// 3. the pull request is deleted **before** the branch it hangs off, because
///    the branch is the only marker a later run can find either of them by.
#[tokio::test]
async fn a_guard_clears_what_a_killed_run_left_and_takes_nothing_else_with_it() {
    let gitea = Gitea::holding(Repository::after_a_killed_run()).await;

    let guard = Litter::new(&gitea.env()).await;

    let after = gitea.repository();
    let seeded = Repository::seeded();
    assert_eq!(
        after.branches, seeded.branches,
        "after a guard was built, the branch listing should be back to the seeded one"
    );
    assert_eq!(
        after.pulls, seeded.pulls,
        "the pull request opened from a leftover branch is found by its head ref and must go with it"
    );

    let at = |what: &str| after.deleted.iter().position(|d| d == what);
    let pull = at("issues/310").unwrap_or_else(|| {
        panic!(
            "the leftover pull request was never deleted; the guard sent {:?}",
            after.deleted
        )
    });
    let branch = at(&format!("branches/{LITTER}live-9")).unwrap_or_else(|| {
        panic!(
            "the leftover branch was never deleted; the guard sent {:?}",
            after.deleted
        )
    });
    assert!(
        pull < branch,
        "the pull request must be deleted before the branch it was opened from -- the branch is \
         the only thing a later run can recognise either of them by. Order sent: {:?}",
        after.deleted
    );

    drop(guard);
}

/// A server that answers one empty listing and then never writes another byte,
/// as an address to point a guard at.
///
/// The first answer is what lets `Litter::new` finish: its leftover sweep reads
/// the branch listing and stops at the first empty page. Every connection after
/// that is **accepted and held open** -- not refused, which would come back as
/// an error in milliseconds and prove nothing. This is the shape a container
/// that has wedged presents to a client, and the only shape that makes
/// [`Litter`]'s cleanup wait forever: `reqwest` has no default timeout.
fn a_server_that_answers_once_then_goes_quiet() -> String {
    use std::io::{Read as _, Write as _};

    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a free port");
    let address = listener.local_addr().expect("the bound address");
    std::thread::spawn(move || {
        let mut answered = false;
        // Held rather than dropped: closing them would answer with a hangup,
        // which `reqwest` reports in milliseconds -- the opposite of the
        // silence this exists to present.
        let mut quiet = Vec::new();
        for mut stream in listener.incoming().flatten() {
            if answered {
                quiet.push(stream);
                continue;
            }
            answered = true;
            // Read the request out first. Answering a client that is still
            // writing and then closing both halves resets the connection, and
            // an error is not the empty listing `Litter::new` needs.
            let mut request = Vec::new();
            let mut byte = [0_u8; 1];
            while !request.ends_with(b"\r\n\r\n") {
                match stream.read(&mut byte) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => request.push(byte[0]),
                }
            }
            let _ = stream.write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\
                  Content-Length: 2\r\nConnection: close\r\n\r\n[]",
            );
            let _ = stream.flush();
            let _ = stream.shutdown(std::net::Shutdown::Write);
        }
    });
    format!("http://{address}")
}

/// **The cleanup is bounded, so a server that stops answering produces a
/// message instead of a stall** (issue #162).
///
/// `Litter::drop` waits for a thread that talks to the server over HTTP, and
/// `reqwest` carries no default timeout, so before the bound a container that
/// accepted the connection and then said nothing wedged the run *permanently*
/// -- with nothing on screen, because a `Drop` that has not returned has not
/// reported anything either. `just check` is the only gate now that CI is off,
/// and a gate that stalls with no output is the one failure shape a reader
/// cannot act on.
///
/// The budget is shortened here so the test costs milliseconds; what is being
/// checked is that the bound exists and that what it says is worth reading, not
/// how long it is.
#[tokio::test]
async fn a_guard_whose_server_stops_answering_reports_rather_than_stalling() {
    let env = Env {
        url: a_server_that_answers_once_then_goes_quiet(),
        token: TOKEN.to_owned(),
        owner: OWNER.to_owned(),
        repo: REPO.to_owned(),
    };

    let mut guard = Litter::new(&env).await;
    guard.cleanup_budget(std::time::Duration::from_millis(250));
    guard.will_create(&format!("{LITTER}wedged-1"));

    let started = std::time::Instant::now();
    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || drop(guard)))
        .expect_err("a cleanup that could not run must fail the test, not pass it quietly");
    let report = panicked
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panicked.downcast_ref::<&str>().copied())
        .expect("the guard reports by panicking with a message")
        .to_owned();

    assert!(
        started.elapsed() < std::time::Duration::from_secs(5),
        "the guard took {:?} to give up on a server that never answers -- the bound is not \
         bounding anything",
        started.elapsed()
    );
    assert!(
        report.contains("250ms"),
        "the report must name the budget that ran out, so a reader knows what to raise: {report}"
    );
    assert!(
        report.contains(&format!("{LITTER}wedged-1")),
        "the report must name what was left behind, since nobody else will: {report}"
    );
}

/// The other half of the destructive rule: a repository with no leftovers is
/// left alone entirely. Not one DELETE, so a prefix match that started
/// matching everything -- or a guard that deleted first and read afterwards --
/// cannot pass as a clean run.
#[tokio::test]
async fn a_guard_over_a_clean_repository_deletes_nothing_at_all() {
    let gitea = Gitea::holding(Repository::seeded()).await;

    let guard = Litter::new(&gitea.env()).await;

    let after = gitea.repository();
    assert_eq!(
        after.deleted,
        Vec::<String>::new(),
        "nothing in the seeded repository is this suite's litter, so nothing may be deleted"
    );
    assert_eq!(after.branches, Repository::seeded().branches);
    assert_eq!(after.pulls, Repository::seeded().pulls);

    drop(guard);
}

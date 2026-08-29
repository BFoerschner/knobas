//! One property, against a **real** Gitea that answers fewer records than the
//! `limit` this adapter asked for: every listing is still walked to the end.
//!
//! # Which profile certifies what
//!
//! * `just gitea-live` -- `testenv/docker-compose.yml`. The shapes interfaces
//!   §4.2 fixes: key forms, the `{ok,data}` envelope, `state=all`, the
//!   discussion path, `sort=recentupdate`, `since=`, a revoked token's 401.
//! * `just gitea-live-capped` -- that file **plus**
//!   `testenv/docker-compose.capped.yml`, which sets Gitea's
//!   `[api] MAX_RESPONSE_ITEMS` to 1. This file, and nothing else.
//!
//! Both are `#[ignore]`d, so `just check` stays offline and docker-free.
//!
//! # Why a second compose configuration was needed
//!
//! Issue #81: all four paged walks in `sync.rs` used to end on a page *shorter*
//! than the one they asked for, which is an end-of-collection signal only if
//! the server honoured `limit` exactly. It is not a promise Gitea makes -- 50
//! is a default an admin of a self-hosted instance can lower -- so a short page
//! means "the collection ran out" *or* "the server capped us", with nothing in
//! the answer to say which. Reading it as the first is a run that returns `Ok`
//! with its watermark advanced past every record after the stop: silent
//! truncation, of exactly the kind ADR-0003's exhaustive sweep turns into
//! tombstones. PR #108 made every listing page until it comes back **empty**.
//!
//! That fix was certified against the crate's wiremock fake and nothing else,
//! because every corpus `testenv/seed-gitea.sh` creates fits in one page of 50:
//! against the default compose file the old rule and the new one agree on every
//! request `tests/live_gitea.rs` makes. The standing rule for this adapter is
//! that when the fake and the server disagree the **fake** is wrong (#35 task
//! 8) -- and until this file the fake's capped-pages mode had never been held
//! against a real Gitea at all.
//!
//! # The oracle
//!
//! "Nothing is lost" is asserted against a listing walked **by hand** in this
//! file, not against a count: [`Raw::walk`] pages `limit=50&page=1..` until a
//! page comes back empty and collects what it saw, and the adapter's own emitted
//! keys must equal it. That hand-walk deliberately shares no code with
//! `sync::last_page` -- an oracle built out of the thing under test proves
//! nothing -- and it makes the assertions independent of how many records the
//! seed happens to have created, which is the live suite's standing discipline
//! (Gitea assigns the pull-request indices and git the object ids, so a fixed
//! count or a literal id is a test that fails for a reason nobody can act on).
//!
//! One test over all four walks rather than four, for the reason PR #108 gives
//! for its fake-side twin: the decision is one rule applied at four sites, and
//! four separate tests would let three of them drift back to
//! `batch.len() < PAGE_SIZE` while the fourth kept the suite green.

mod live_env;

use std::collections::BTreeSet;

use live_env::{Env, env, full, of_kind};
use serde_json::Value;

/// The `limit` every listing in `client.rs` asks for (`client::PAGE_SIZE`).
///
/// Spelled again rather than imported -- it is `pub(crate)` -- and that is the
/// right shape anyway: this is the *server's* side of the number, and what the
/// checks below want to know is that the server answered fewer than the adapter
/// requested, whatever the adapter requests.
const REQUESTED: usize = 50;

/// The largest listing this suite can measure, in records.
///
/// The capped profile serves one record per request, so `sync.rs`'s page
/// budgets stop being page counts and become record counts:
/// `MAX_LIST_PAGES`/`MAX_BRANCH_PAGES` afford 21 requests and
/// `MAX_PR_PAGES`/`MAX_COMMIT_PAGES` 20. Past them the two exhaustive walks
/// fail loudly with `cap_reached` and the two budgeted ones stop *silently* on
/// their own budget -- correct behaviour, but it would arrive at the assertions
/// below as missing records and read as the defect under test.
///
/// It is reachable: `just gitea-live` leaves a branch and a pull request behind
/// on every run, and the seeded volume outlives them. When a listing here grows
/// past this, the answer is `testenv/reset` and a fresh seed, not a bigger
/// number.
const HEADROOM: usize = 19;

/// One listing as the server actually serves it.
struct Listing {
    what: String,
    /// How many records came back on page 1 -- the server's cap, when it caps.
    first_page: usize,
    /// Every record's key, in the order the pages delivered them.
    keys: Vec<String>,
}

impl Listing {
    /// The precondition this whole suite rests on: the listing did not fit in
    /// one page, and the page it did not fit in was **shorter than the limit
    /// the adapter asked for**.
    ///
    /// Both halves are load-bearing, and they fail for different reasons.
    /// Without the first nothing is being certified at all -- a walk that stops
    /// on the first short page and one that stops on the first empty page both
    /// return the whole listing, and the mutation this suite exists to kill
    /// stays green. Without the second the listing is merely long: a server
    /// honouring `limit=50` over 60 records pages too, and ends every page but
    /// the last on the full 50, which is ordinary paging and not the defect.
    fn must_be_capped_with_more_behind_it(&self) {
        assert!(
            self.keys.len() > self.first_page,
            "{}: {} records in total, {} of them on page 1 -- the whole listing arrived in one \
             page, so nothing here is being exercised. Either this Gitea is not capping (start \
             it with testenv/docker-compose.capped.yml -- `just gitea-live-capped`) or the \
             seeded corpus is gone (testenv/seed-gitea.sh).",
            self.what,
            self.keys.len(),
            self.first_page,
        );
        assert!(
            self.first_page < REQUESTED,
            "{}: page 1 carried the full {REQUESTED} records the adapter asked for, so this \
             listing is merely long and the server is honouring `limit`. The property under test \
             needs a server answering FEWER than it was asked for: \
             testenv/docker-compose.capped.yml.",
            self.what,
        );
        assert!(
            self.keys.len() <= HEADROOM,
            "{}: {} records is past the {HEADROOM} this profile can measure -- one record per \
             request turns sync.rs's page budgets into record budgets, and the walk would stop \
             on its own budget rather than on the end of the listing. Run testenv/reset and \
             re-seed.",
            self.what,
            self.keys.len(),
        );
    }
}

/// Gitea's API, spoken directly, so the adapter's walk has something to be
/// wrong against.
struct Raw {
    http: reqwest::Client,
    auth: String,
    api: String,
}

impl Raw {
    fn new(env: &Env) -> Self {
        Raw {
            http: reqwest::Client::new(),
            auth: format!("token {}", env.token),
            api: format!("{}/api/v1", env.url),
        }
    }

    /// One page, as the records on it.
    async fn page(&self, path: &str, query: &[(&str, String)]) -> Vec<Value> {
        let response = self
            .http
            .get(format!("{}{path}", self.api))
            .header("Authorization", &self.auth)
            .query(query)
            .send()
            .await
            .unwrap_or_else(|error| panic!("GET {path}: {error}"));
        let status = response.status();
        let body: Value = response
            .json()
            .await
            .unwrap_or_else(|error| panic!("GET {path} -> {status}: unreadable body: {error}"));
        assert!(status.is_success(), "GET {path} -> {status}: {body}");
        match body {
            Value::Array(rows) => rows,
            // `/repos/search` is the one endpoint wrapped in `{ ok, data }`,
            // and its page past the last one answers `{"ok":true,"data":[]}`
            // rather than a bare `[]` -- which is the fake-fidelity bug PR #108
            // found, here against the server that decides it.
            Value::Object(mut object) => match object.remove("data") {
                Some(Value::Array(rows)) => rows,
                other => panic!("GET {path}: expected a data array, got {other:?}"),
            },
            other => panic!("GET {path}: expected a listing, got {other}"),
        }
    }

    /// Page `limit=50&page=1..` until a page comes back **empty**, and collect
    /// `key` from every record.
    ///
    /// Hand-written on purpose. This is the oracle the adapter is measured
    /// against, so it must share no line with `sync::last_page`; and it must
    /// not stop on a short page either, or it would be exactly as wrong as the
    /// bug and the two would agree.
    async fn walk(
        &self,
        what: &str,
        path: &str,
        query: &[(&str, &str)],
        key: fn(&Value) -> String,
    ) -> Listing {
        let mut keys = Vec::new();
        let mut first_page = 0;
        // One more than any budget in sync.rs, so a server ignoring `page`
        // ends this loop as a failed assertion rather than as a hung suite.
        for page in 1..=64 {
            let mut query: Vec<(&str, String)> =
                query.iter().map(|(k, v)| (*k, (*v).to_owned())).collect();
            query.push(("limit", REQUESTED.to_string()));
            query.push(("page", page.to_string()));
            let batch = self.page(path, &query).await;
            if page == 1 {
                first_page = batch.len();
            }
            if batch.is_empty() {
                return Listing {
                    what: what.to_owned(),
                    first_page,
                    keys,
                };
            }
            keys.extend(batch.iter().map(key));
        }
        panic!("{what}: 64 pages of {path} and still not empty -- is `page` being honoured?");
    }
}

fn text(value: &Value, field: &str) -> String {
    value[field]
        .as_str()
        .unwrap_or_else(|| panic!("a record with no string {field}: {value}"))
        .to_owned()
}

/// The keys the adapter emitted for one kind, inside one repository, sorted.
fn emitted(items: &[knobas_source::SyncItem], kind: &str, repo: &str) -> Vec<String> {
    let mut keys: Vec<String> = of_kind(items, kind)
        .iter()
        .map(|i| i.entity.key.clone())
        .filter(|k| {
            k == repo || k.starts_with(&format!("{repo}@")) || k.starts_with(&format!("{repo}#"))
        })
        .collect();
    keys.sort();
    keys
}

fn sorted(keys: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut out: Vec<String> = keys.into_iter().collect();
    out.sort();
    out
}

/// Issue #115. All four paged walks, against a real Gitea capping every page
/// below the `limit=50` they ask for.
///
/// Reverting `sync::last_page` to `batch.len() < PAGE_SIZE` -- the reading
/// issue #81 removed -- reddens all four assertions here, which is the whole
/// reason this suite exists: a live test that passes either way certifies
/// nothing.
#[tokio::test]
#[ignore = "needs testenv's Gitea started with docker-compose.capped.yml -- `just gitea-live-capped`"]
async fn every_listing_is_walked_to_the_end_against_a_server_that_caps_its_pages() {
    let env = env();
    let raw = Raw::new(&env);
    let repo = env.full_name();

    // -- the oracle: what is really there, paged by hand ---------------------
    let repos = raw
        .walk(
            "the repository listing",
            "/repos/search",
            &[("sort", "updated"), ("order", "desc")],
            |r| text(r, "full_name"),
        )
        .await;
    repos.must_be_capped_with_more_behind_it();

    let branches = raw
        .walk(
            &format!("the branch listing of {repo}"),
            &format!("/repos/{repo}/branches"),
            &[],
            |b| text(b, "name"),
        )
        .await;
    branches.must_be_capped_with_more_behind_it();

    let pulls = raw
        .walk(
            &format!("the pull-request listing of {repo}"),
            &format!("/repos/{repo}/pulls"),
            &[("state", "all"), ("sort", "recentupdate")],
            |p| {
                p["number"]
                    .as_u64()
                    .unwrap_or_else(|| panic!("a pull request with no number: {p}"))
                    .to_string()
            },
        )
        .await;
    pulls.must_be_capped_with_more_behind_it();

    // Commits are walked per branch, so the oracle is the union over every
    // branch -- and at least one of those branches has to have needed a second
    // page, or the commit walk is not being exercised either.
    let mut commits: BTreeSet<String> = BTreeSet::new();
    let mut a_branch_that_paged = None;
    for branch in &branches.keys {
        let listing = raw
            .walk(
                &format!("the commit listing of {repo}@{branch}"),
                &format!("/repos/{repo}/commits"),
                &[
                    ("sha", branch),
                    ("stat", "false"),
                    ("verification", "false"),
                    ("files", "false"),
                ],
                |c| text(c, "sha"),
            )
            .await;
        assert!(
            listing.keys.len() <= HEADROOM,
            "{}: {} commits is past the {HEADROOM} this profile can measure",
            listing.what,
            listing.keys.len(),
        );
        if listing.keys.len() > listing.first_page {
            a_branch_that_paged = Some(listing.what.clone());
        }
        commits.extend(listing.keys);
    }
    let paged = a_branch_that_paged.expect(
        "no branch of this repository has more commits than one capped page, so the commit walk \
         is not being exercised; the seed puts three on feature/PAY-231-sepa-retry",
    );
    println!("the commit walk is exercised by {paged}");

    // -- and what the adapter walked ----------------------------------------
    //
    // Scoped to the owner rather than to the one repository, because naming
    // repositories in the config skips the repository *listing* -- one of the
    // four walks under test.
    let (items, _) = full(&*env.whole_owner()).await;

    let owned: Vec<String> = repos
        .keys
        .iter()
        .filter(|full| full.starts_with(&format!("{}/", env.owner)))
        .cloned()
        .collect();
    assert_eq!(
        sorted(of_kind(&items, "repo").iter().map(|i| i.entity.key.clone())),
        sorted(owned),
        "the repository listing lost records to a capped page",
    );

    assert_eq!(
        emitted(&items, "branch", &repo),
        sorted(
            branches
                .keys
                .iter()
                .map(|b| format!("{repo}@refs/heads/{b}"))
        ),
        "the branch listing of {repo} lost records to a capped page",
    );

    assert_eq!(
        emitted(&items, "pr", &repo),
        sorted(pulls.keys.iter().map(|n| format!("{repo}#{n}"))),
        "the pull-request listing of {repo} lost records to a capped page",
    );

    assert_eq!(
        emitted(&items, "commit", &repo),
        sorted(commits.into_iter().map(|sha| format!("{repo}@{sha}"))),
        "the commit walk of {repo} lost records to a capped page",
    );
}

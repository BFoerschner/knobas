//! The suite exit criterion **B** is measured on: the real, pinned, seeded
//! Gitea container from `testenv/docker-compose.yml`.
//!
//! # Why this file is the contract and `tests/support/mod.rs` is not
//!
//! Everything else in this crate runs against a wiremock stand-in, because
//! `just check` and CI must stay docker-free (roadmap §3). That fake encodes a
//! reading of Gitea's API -- the `{ok,data}` envelope on `/repos/search`, the
//! `sort=recentupdate` ordering the pull-request walk terminates on, the fact
//! that a pull request's discussion lives on the *issue* of the same index --
//! and a fake is only ever as right as whoever wrote it. This suite re-asserts
//! each of those against the real server. **If the two disagree, the fake is
//! wrong**, and the fix goes there.
//!
//! `#[ignore]`d, so `just check` runs none of it:
//!
//! ```text
//! cd testenv && docker compose up -d gitea && ./seed
//! eval "$(cd testenv && ./seed --env)"
//! cargo test -p knobas-source-gitea --test live_gitea -- --ignored --nocapture
//! ```
//!
//! or, in one step, `just gitea-live`.
//!
//! # What this file does NOT certify
//!
//! Every corpus `testenv/seed-gitea.sh` creates fits in one page of the 50 the
//! adapter asks for, so against this compose file a page shorter than 50 and a
//! listing that ran out are the same answer -- and the termination rule issue
//! #81 removed (stop on a *short* page) agrees with the one that replaced it
//! (stop on an *empty* page) on every request this suite makes. The property
//! that separates them needs a server that caps its pages below the requested
//! limit, and that is a different compose configuration: see
//! `tests/live_gitea_capped.rs` and `just gitea-live-capped`.
//!
//! **The contract battery no longer runs at owner scope here.** Since issue
//! #146 it runs against the two seeded repositories no test in this file
//! writes to, because its clause 2 -- "an incremental sync yields no items
//! *when nothing changed*" -- needs a quiet corpus for its antecedent to hold,
//! and this file's own mutations are changes. So the live battery certifies
//! the contract against the real server over a quiescent scope, and no longer
//! over the whole organisation; the doc comment on
//! [`passes_the_contract_battery_against_the_real_container`] names the
//! mechanism, the measurement, and where each piece of the lost coverage is
//! still held. Nothing in `crates/knobas-source/src/**` changed: the clause is
//! correct, and an adapter that went quiet about a genuinely changed entity
//! would be the defect it exists to catch.
//!
//! # What this file creates in the container, and what it takes away
//!
//! It is not read-only, and that is the point: exit criterion B is about what
//! the *real* server does with something it has just been told. Three tests
//! open a branch through Gitea's own API, two of those open a pull request on
//! it, and one of those writes `PAGE + 1` comments. Every assertion is made
//! against content that is really there.
//!
//! **All of it is removed again** -- afterwards, never before an assertion --
//! by [`Litter`], which creates each branch and later deletes it together with
//! every pull request opened from it, and so every comment on those, from
//! `Drop`. The *branch* is what the guard is asked for and what it tracks; a
//! pull request is opened by the test itself and found again by matching its
//! head ref against a tracked branch, which is why the branch is deleted last.
//! Three consequences, each a decision:
//!
//! * **The failure path is the success path.** A test that panics unwinds
//!   through the same cleanup a passing test returns through. A cleanup that
//!   ran only on success would leave the residue on exactly the runs that were
//!   already going badly.
//! * **The removal is checked, not hoped for.** `Litter`'s `Drop` re-reads the
//!   listings afterwards and fails the test if anything it created is still
//!   standing, so a cleanup that quietly stopped deleting cannot pass as a
//!   clean run.
//! * **What a killed process left is cleared, not mourned.** `Drop` cannot
//!   survive a `SIGKILL` or a Ctrl-C at the wrong moment, so the next run's
//!   first mutating test removes whatever such a run left, by the `knobas-`
//!   prefix every branch this suite creates carries. Recovery from a dirty
//!   environment is "run the suite again". None of that needs a container to
//!   be checked, and `tests/litter_guard.rs` checks it in `just check`: that
//!   the prefix matches nothing the seed creates, that the leftovers really go,
//!   and that a clean repository is left alone.
//!
//! So `testenv/reset` -- which destroys every testenv volume, Uptime Kuma's
//! included -- is back to being the deliberate remedy rather than the routine
//! one. Before issue #143 there was no cleanup at all: `payout-service` grew by
//! three branches, two pull requests and 51 comments *every run*, and seven
//! runs took its branch listing to 25 and made `just gitea-live-capped` refuse
//! to start.
//!
//! # A red run here is never answered by running it again
//!
//! The rule this suite exists under (#35 task 8): when the fake and the server
//! disagree the **fake** is wrong. The corollary is that a failure in a live
//! suite means the adapter or the fake is wrong, and **re-running is not a
//! resolution** -- a re-run is for capturing the emission, nothing else. Two
//! flakes were fixed by finding their mechanism (#140, #146) and neither would
//! have been found by anyone who treated the red as noise. A tolerance or a
//! retry loop around an assertion in this file is re-running with extra steps.
//!
//! # What cannot be asserted here, and why
//!
//! Gitea numbers pull requests from a per-repository counter and git derives
//! object ids from content, so the fixture's `#142` and its `c90d11` **cannot**
//! be dictated by a seed script (testenv/README.md says so at length; the seed
//! burns issue indices to land the numbers, and records what it actually got in
//! `seed-state.json`). These tests therefore assert the key *forms* interfaces
//! §4.2 fixes and the seeded *titles*, never a literal id.

mod live_env;

use knobas_source::contract::{Fault, VecSink, battery};
use knobas_source::{SourceError, SyncItem};
use live_env::{Env, Litter, env, full, of_kind};

/// A port nothing listens on: bound to learn the number, then dropped.
fn dead_url() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    format!("http://127.0.0.1:{port}")
}

/// What an idle run emitted that it should not have: everything except the one
/// repository's own entity, as `(kind, key)` pairs.
///
/// # Why the repository entity is exempt, and why asserting it away was a flake
///
/// Gitea moves a repository's `updated_at` as its own bookkeeping catches up
/// with a branch or a pull request just created through its API, and that write
/// can land *after* the create call returned. So a run a second or two later
/// legitimately reads a newer repository than the run before it did, and the
/// adapter emits it -- `sync::repository` emits a repository exactly when its
/// `updated_at` moved past the cursor, which is a changed entity, not a
/// re-delivery of what the previous run already sent.
///
/// Asserting the whole idle run empty therefore failed for a reason nobody
/// could act on: measured on the pinned container before this scoping, 5 of 25
/// runs of the pull-request test and 4 of 25 of the commit one, every one of
/// them the repository entity alone (#140). A live suite whose failures are
/// supposed to mean "the fake is wrong" cannot afford to cry wolf, so what the
/// two tests assert is what they are actually for: **the walk each one drives
/// does not re-deliver what it just delivered.**
///
/// # What the exemption gives up
///
/// Stating it, because #140 asked for the choice rather than only its result: a
/// repository entity re-delivered on *every* run now passes both tests
/// unnoticed. Delete `after.repo_updated_at = updated_at` from
/// `sync::repository` and that is exactly what happens -- this filter drops the
/// item, and the cursor clause beside it only asks that a run which emitted
/// something moved its position, which such a run does.
///
/// That property is certified where the timing does not move under it, and
/// deliberately not here: docker-free by
/// `tests/sync.rs::an_idle_run_emits_nothing_and_returns_the_same_bytes` and
/// `::the_position_after_one_change_is_itself_idle_stable`, against a fake whose
/// `updated_at` stands still unless the test moves it; and live by
/// `passes_the_contract_battery_against_the_real_container`, whose clause 2
/// asserts the whole run empty and exempts nothing.
///
/// That live half used to be weaker than it looked -- the battery test flaked
/// on this same mechanism, 4 serial runs of this file in 6, which was #146.
/// **#146 was closed by narrowing the battery's scope to repositories nothing
/// writes to, not by weakening the clause**, so the cover this exemption rests
/// on is intact and is now reliable rather than intermittent. Anyone tempted to
/// weaken clause 2 in `crates/knobas-source/src/contract.rs` should read this
/// paragraph as the reason not to: the exemption above is only affordable
/// because something else still asserts the unexempted form against a real
/// server.
fn re_delivered<'a>(items: &'a [SyncItem], full_name: &str) -> Vec<(&'a str, &'a str)> {
    items
        .iter()
        .filter(|i| !(i.kind == "repo" && i.entity.key == full_name))
        .map(|i| (i.kind.as_str(), i.entity.key.as_str()))
        .collect()
}

/// A token that never existed. Salted with the process id so a run cannot
/// accidentally collide with a real one.
fn revoked() -> String {
    format!("revoked-{}", std::process::id())
}

/// One commit on `branch`, made through Gitea's own contents endpoint, and the
/// object id the server answers with.
///
/// The content is fixed and pre-encoded, so this needs no base64 encoder:
/// `a25vYmFzIGxpdmUgY2hlY2sK` is "knobas live check\n". The **path** is what
/// makes each call a new commit -- writing the same path twice is a 422, not a
/// second commit -- so every caller passes a fresh one.
async fn push_file(
    http: &reqwest::Client,
    env: &Env,
    branch: &str,
    path: &str,
    message: &str,
) -> String {
    let wrote = http
        .post(format!(
            "{}/api/v1/repos/{}/contents/{path}",
            env.url,
            env.full_name()
        ))
        .header("Authorization", format!("token {}", env.token))
        .json(&serde_json::json!({
            "branch": branch,
            "content": "a25vYmFzIGxpdmUgY2hlY2sK",
            "message": message,
        }))
        .send()
        .await
        .expect("write the file");
    assert!(
        wrote.status().is_success(),
        "contents: {}",
        wrote.text().await.unwrap_or_default()
    );
    wrote.json::<serde_json::Value>().await.unwrap()["commit"]["sha"]
        .as_str()
        .expect("Gitea answers with the commit it made")
        .to_owned()
}

/// The contract battery -- the suite every adapter must pass -- against the
/// server rather than against the fake, and scoped to the seeded repositories
/// **nothing in this file writes to** (issue #146).
///
/// # Why the scope is not the whole organisation
///
/// The battery's clause 2 reads "incremental sync from the returned cursor
/// yields no items *when nothing changed*". Run at owner scope, at the end of
/// a file whose other tests have just opened a branch and a pull request in
/// `payout-service`, its full->idle pair straddles a write that is still
/// landing: **Gitea moves a repository's `updated_at` as its own bookkeeping
/// catches up, and that write can arrive after the create call returned.** The
/// idle run then emits the repository -- correctly. Measured on the pinned
/// container, the battery failed **4 serial runs of this file in 6** that way.
///
/// So the antecedent was false, not the consequent. An adapter that stayed
/// silent about a repository whose `updated_at` had genuinely moved would be
/// *suppressing a change*, which is the defect clause 2 exists to catch from
/// the other side. Establishing quiescence is this harness's job, and
/// [`Env::quiet_repos`] establishes it by construction: only `env.repo` is ever
/// mutated, so from the end of the seed onwards nothing moves these two.
///
/// **The clause was never the problem, and it is not weakened here.** Nothing
/// under `crates/knobas-source/src/**` changes; no tolerance, no retry, and no
/// re-run. A red run of this test still means the adapter is wrong.
///
/// # What this narrowing costs, and where each piece is still held
///
/// The battery no longer runs its full->idle pair at **owner** scope against a
/// live server. Every part of that is certified elsewhere:
///
/// * the owner-scoped repository *listing* walk, by
///   [`the_shapes_the_fake_only_assumes_are_certified_here`] here and by
///   `live_gitea_capped.rs`'s `whole_owner()` walk;
/// * idle behaviour of the walks this file mutates, by the idle clauses of
///   [`a_pull_request_opened_through_the_api_appears_in_the_next_incremental_run`]
///   and
///   [`a_commit_pushed_through_the_api_arrives_once_and_only_once_and_so_does_the_next_push`];
/// * the full->idle pair over the repository-*listing* walk, against the
///   docker-free fake in `tests/sync.rs` --
///   `an_idle_run_emits_nothing_and_returns_the_same_bytes` and
///   `the_position_after_one_change_is_itself_idle_stable`, both on the
///   unfiltered selection, which walks the same listing an `owners[]` scope
///   does (`sync.rs`'s module doc: an empty `owners[]` means every repository
///   the token can see). Quiescence there is by construction.
///
/// Said exactly, because a record of a coverage loss is worth nothing if it
/// overstates what is left: **no docker-free test runs an idle pair with
/// `owners[]` actually set.** `an_owner_filter_drops_everything_else` in
/// `tests/sync.rs` certifies that the filter is a filter over that same walk,
/// and it does one full sync rather than a pair. So what this narrowing gives
/// up outright is the idle pair with the owner filter applied, live or fake;
/// what it keeps is the idle pair over the walk the filter sits on.
#[tokio::test]
#[ignore = "needs testenv's seeded Gitea container"]
async fn passes_the_contract_battery_against_the_real_container() {
    let env = env();
    let scope = serde_json::json!({ "repos": env.quiet_repos() });
    battery(move |fault| {
        let (token, base) = match fault {
            Fault::None => (env.token.clone(), None),
            Fault::Unauthorized => (revoked(), None),
            Fault::Unreachable => (env.token.clone(), Some(dead_url())),
        };
        let mut at = Env {
            url: env.url.clone(),
            token: env.token.clone(),
            owner: env.owner.clone(),
            repo: env.repo.clone(),
        };
        if let Some(dead) = base {
            at.url = dead;
        }
        at.source_with(&token, scope.clone())
    })
    .await;
}

/// Interfaces §4.2's key forms, against ids the *real* server issued.
#[tokio::test]
#[ignore = "needs testenv's seeded Gitea container"]
async fn the_seeded_content_lands_under_the_documented_key_forms() {
    let env = env();
    let (items, cursor) = full(&*env.one_repo()).await;

    let repos: Vec<&str> = of_kind(&items, "repo")
        .iter()
        .map(|i| i.entity.key.as_str())
        .collect();
    assert_eq!(
        repos,
        vec![env.full_name().as_str()],
        "the repository key is owner/name"
    );

    for item in of_kind(&items, "branch") {
        let (repo, tail) = item
            .entity
            .key
            .split_once('@')
            .expect("branch keys carry an @");
        assert_eq!(repo, env.full_name());
        assert!(
            tail.starts_with("refs/heads/"),
            "branch key {:?}",
            item.entity.key
        );
    }
    assert!(
        of_kind(&items, "branch")
            .iter()
            .any(|i| i.entity.key.ends_with("refs/heads/main")),
        "the seed creates a default branch"
    );

    for item in of_kind(&items, "pr") {
        let (repo, number) = item
            .entity
            .key
            .rsplit_once('#')
            .expect("pull-request keys carry a #");
        assert_eq!(repo, env.full_name());
        assert!(
            number.parse::<u64>().is_ok(),
            "pull-request key {:?}",
            item.entity.key
        );
        assert!(
            item.web_url.is_some(),
            "every pull request is openable in the browser (P5)"
        );
    }

    assert!(
        !of_kind(&items, "pr").is_empty(),
        "the seed opens pull requests -- without this the loop above certifies \
         the key form of an empty list"
    );

    for item in of_kind(&items, "commit") {
        let (repo, oid) = item
            .entity
            .key
            .rsplit_once('@')
            .expect("commit keys carry an @");
        assert_eq!(repo, env.full_name());
        // §4.2 writes this form as `@<sha40>`, but Gitea supports SHA-256
        // repositories whose ids are 64 characters. The adapter uses whatever
        // the server reports and never truncates.
        assert!(
            oid.len() >= 40 && oid.chars().all(|c| c.is_ascii_hexdigit()),
            "commit key {:?} must carry the full object id",
            item.entity.key
        );
    }
    assert!(
        !of_kind(&items, "commit").is_empty(),
        "the seed pushes commits"
    );

    // Everything must be addressable the way the sink reads it back.
    for item in &items {
        let id = item.entity.to_string();
        assert_eq!(
            knobas_core::entity::EntityRef::parse(&id).as_ref(),
            Ok(&item.entity),
            "{id}"
        );
    }
    println!("cursor after the initial sync: {cursor}");
}

/// The three shapes the wiremock fake asserts by construction, re-asserted
/// against the server that decides them.
///
/// Each of these is a place where the fake could be confidently wrong and every
/// docker-free test would still pass:
///
/// * **`state=all`.** Gitea defaults `/pulls` to `state=open`, and a *merged*
///   pull request is exactly where the ticket-to-PR story ends.
/// * **the discussion path.** There is no `/pulls/{n}/comments`; Gitea keeps it
///   on the issue of the same index. Get that wrong and every pull request is
///   indexed without the review text this adapter's fifth endpoint exists for.
/// * **the source's own author string.** §4.1 forbids inventing one, and the
///   seed authors its content as the fixture's people.
#[tokio::test]
#[ignore = "needs testenv's seeded Gitea container"]
async fn the_shapes_the_fake_only_assumes_are_certified_here() {
    let env = env();
    // Every repository the token can see, because the merged pull request the
    // fixture carries is in `ledger-api` rather than in `payout-service`.
    let (items, _) = full(&*env.source(serde_json::json!({ "owners": [env.owner.clone()] }))).await;
    let prs = of_kind(&items, "pr");
    let titles: Vec<&str> = prs.iter().map(|i| i.title.as_str()).collect();

    // `state=all`: the fixture's merged pull request.
    assert!(
        titles.contains(&"Fix ledger drift on partial refunds"),
        "a merged pull request must be mirrored, so `state` cannot be left at \
         Gitea's `open` default: {titles:?}"
    );

    // The discussion path: the fixture's own review comment, in the indexed
    // text of the pull request it belongs to.
    let sepa = prs
        .iter()
        .find(|i| i.title.contains("SEPA retry"))
        .expect("the seed opens the SEPA retry pull request");
    assert!(
        sepa.body_text.contains("jitter"),
        "the review discussion is what FTS has to find; body_text was {:?}",
        sepa.body_text
    );
    // ...and the title is still the first thing in it. `body_text.len() >
    // title.len()` stood here until #347 and could not fail: the assertion
    // above had already put a 46-character review comment inside a `body_text`
    // whose title is 35, so the length was greater whether or not the title
    // was ever folded in. `map::pr_item` builds the text as title, then body,
    // then every comment, `join`ed -- so a fold that dropped `Some(title)`
    // passed the length check and fails this one.
    assert!(
        sepa.body_text.starts_with(&sepa.title),
        "the fold starts with the title: an item whose FTS text does not contain what the item \
         is called is not findable by its own name: {:?}",
        sepa.body_text
    );

    // The author is the source's word for who did it, not knobas's.
    assert!(
        prs.iter().all(|i| i.author.is_some()),
        "every seeded pull request has an author: {:?}",
        prs.iter()
            .map(|i| (&i.title, &i.author))
            .collect::<Vec<_>>()
    );
    // Guarded for the reason the sibling test states at its own commit loop:
    // `all()` over an empty list is `true`, and nothing above this line puts a
    // commit in `items` (the narrowing above is over pull requests). Without
    // the guard, an owner-scoped run that stopped walking commits altogether
    // certified "every seeded commit has an author" (#347).
    let commits = of_kind(&items, "commit");
    assert!(
        !commits.is_empty(),
        "the seed pushes commits, and an empty list would make the assertion below certify the \
         key form of nothing"
    );
    assert!(
        commits.iter().all(|i| i.author.is_some()),
        "every seeded commit has an author"
    );
}

/// The incremental walk stops at the first pull request below the watermark,
/// which is only correct if the server really answers `sort=recentupdate`
/// newest-first. Checked, not assumed -- a Gitea release that changed it would
/// silently truncate every sync, and the fake is written to the same
/// assumption, so nothing docker-free could catch it.
#[tokio::test]
#[ignore = "needs testenv's seeded Gitea container"]
async fn pull_requests_come_back_newest_updated_first() {
    let env = env();
    // One repository: across repositories the walk interleaves by repository,
    // and the ordering under test is per listing.
    let (items, _) = full(&*env.one_repo()).await;
    let prs = of_kind(&items, "pr");
    assert!(
        prs.len() >= 2,
        "the seed opens more than one pull request in {}: {:?}",
        env.full_name(),
        prs.iter().map(|i| &i.title).collect::<Vec<_>>()
    );
    let updated: Vec<_> = prs
        .iter()
        .map(|i| {
            i.updated_at
                .unwrap_or_else(|| panic!("{:?} carries no updated_at", i.title))
        })
        .collect();
    assert!(
        updated.windows(2).all(|w| w[0] >= w[1]),
        "pull requests arrived out of order: {updated:?}"
    );
}

/// Exit criterion B's middle clause: something is opened through Gitea's own
/// API, and the very next incremental run returns it -- and the run after that
/// is silent again.
///
/// # What this still rests on
///
/// Two ordering facts, and both are the server's own guarantee rather than a
/// race, because this run asserts that both arrive: a branch and a pull request
/// Gitea's create calls have answered are visible to the very next `/branches`
/// and `?state=all&sort=recentupdate` listings. Everything else timed has been
/// taken out. In particular the repository entity may ride along in **any** of
/// the three runs, or in none of them, depending on when Gitea's bookkeeping
/// lands (see `re_delivered`) -- this test deliberately says nothing about
/// which, and saying something about it was #140.
///
/// One wall-clock *name*, which is not a timing dependency but is the one thing
/// that would make two of these collide: the branch is named from the epoch
/// **second**. Two runs of this test starting inside the same second would ask
/// Gitea for the same branch twice, and the second create would fail. Nothing
/// runs this file concurrently -- `just gitea-live` is serial by recipe -- so a
/// second is enough.
///
/// The branch and the pull request it opens are removed again when the test
/// ends, whether it passes or panics; see this file's header and
/// [`live_env::Litter`].
#[tokio::test]
#[ignore = "needs testenv's seeded Gitea container"]
async fn a_pull_request_opened_through_the_api_appears_in_the_next_incremental_run() {
    let env = env();
    // Built before the baseline sync: clearing what a killed run left behind is
    // itself a change to the repository, and this run's cursor must be taken after
    // it rather than before.
    let mut litter = Litter::new(&env).await;
    let source = env.one_repo();
    let (_, cursor) = full(&*source).await;

    // A unique name, so a re-run does not collide with the last one.
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let branch = format!("knobas-live-{stamp}");
    let api = format!("{}/api/v1/repos/{}", env.url, env.full_name());
    let http = reqwest::Client::new();
    let auth = format!("token {}", env.token);

    litter.branch_off_main(&branch).await;

    let opened = http
        .post(format!("{api}/pulls"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "head": branch, "base": "main",
            "title": format!("knobas live check {stamp}"),
            "body": "Opened by the knobas Gitea adapter's live suite."
        }))
        .send()
        .await
        .expect("open the pull request");
    assert!(
        opened.status().is_success(),
        "pull: {}",
        opened.text().await.unwrap_or_default()
    );
    let number = opened.json::<serde_json::Value>().await.unwrap()["number"]
        .as_u64()
        .expect("Gitea answers with the new pull request's number");

    let mut sink = VecSink(Vec::new());
    let moved = source
        .sync(Some(cursor.clone()), &mut sink)
        .await
        .expect("incremental sync");
    let ids: Vec<String> = sink.0.iter().map(|i| i.entity.to_string()).collect();
    assert!(
        ids.contains(&format!("gitea:{}#{number}", env.full_name())),
        "the new pull request is missing from {ids:?}"
    );
    assert!(
        ids.contains(&format!("gitea:{}@refs/heads/{branch}", env.full_name())),
        "the new branch is missing from {ids:?}"
    );
    assert_ne!(
        moved, cursor,
        "the position must move when something was emitted"
    );

    // And the run after it re-delivers none of it -- not the pull request, not
    // the branch, not a commit (battery clause 2 on a position this run wrote
    // rather than on a fresh one). The repository entity is the one thing
    // allowed to ride along, and `re_delivered` says why.
    let mut idle = VecSink(Vec::new());
    let same = source
        .sync(Some(moved.clone()), &mut idle)
        .await
        .expect("idle sync");
    assert_eq!(
        re_delivered(&idle.0, &env.full_name()),
        Vec::<(&str, &str)>::new(),
        "the run after the incremental one re-delivered what it had already sent"
    );
    // Clause 2's cursor half, as the equivalence rather than as one arm of it:
    // a run that emitted nothing hands its position back byte-identical, and a
    // run that emitted the repository has to move it. Written this way neither
    // half goes vacuous on the runs where the repository rides along.
    assert_eq!(
        same == moved,
        idle.0.is_empty(),
        "the run emitted {:?} and its cursor {}",
        idle.0
            .iter()
            .map(|i| i.entity.to_string())
            .collect::<Vec<_>>(),
        if same == moved {
            "stood still"
        } else {
            "moved"
        }
    );
}

/// Exit criterion B for the **commit** walk, which is where the docker-free
/// fake is least able to speak for the real server: it matches no `since=` at
/// all, serves every branch's list in one page, and answers `sha=<branch>` with
/// whatever the fixture mounted under that key. So `since=` being server-side
/// *and* inclusive, `sha=` really selecting that branch's history, and
/// `commits_at_watermark` closing the boundary the inclusive filter re-delivers
/// are three assumptions only this container can settle.
///
/// A commit is pushed through Gitea's own API and the next incremental run must
/// return **exactly** it -- not the branch's inherited history, and not it
/// twice. Then the same branch is pushed to **again**, and the run after that
/// must return exactly the second commit.
///
/// # Why the branch is pushed to twice (issue #152)
///
/// `sync::commits` returns on `moved.is_empty()` **before it lists anything**,
/// and an idle run's `moved` is empty. So run 3 below never enters the commit
/// walk, and whatever it asserts it cannot observe where run 2 left
/// `commits_since` and `commits_at_watermark`. Measured: suppress
/// `close_watermark` on incremental runs -- so an incremental run's commit
/// watermark never advances -- and runs 1 to 3 stay green, as does every
/// docker-free test in this crate. Run 4 is the one that goes red, and its
/// message names the re-delivered object id rather than reporting a bare diff.
///
/// A run that walks the branch a **second** time is the only thing that reaches
/// it, because only then is the watermark run 2 wrote the `since=` the server is
/// asked. That is run 4. It rides on the branch this test already opened rather
/// than opening another, so it adds one *commit* and no further branch, pull
/// request or comment -- and since issue #143 that branch, with everything on
/// it, is deleted again when the test ends ([`Litter`]). So the second push
/// costs the fixture nothing that outlives the run.
///
/// # What this still rests on
///
/// That Gitea's `since=` is second-resolution and inclusive, which is the whole
/// point of `commits_at_watermark`; a second commit landing in the same second
/// as this one would arrive with it, and this test pushes one.
///
/// And that the `/branches` listing in run 2 already reports the head this push
/// moved. `sync::commits` walks only the branches `sync::branches` handed it as
/// moved, so a listing still serving the old head would leave the commit walk
/// unentered -- which this test catches, because it asserts the commit arrives,
/// not merely that nothing extra did.
///
/// Run 4 rests on both facts one run later, and on nothing further. In
/// particular it does **not** rest on the two pushes landing in different
/// seconds, which is the kind of throughput dependency this suite has been
/// bitten by. Run 2 leaves `commits_since` at the *first* push's own second, so
/// an inclusive `since=` hands that commit back on run 4 whether or not the two
/// share a second, and `commits_at_watermark` is what drops it in both cases.
/// What a shared second changes is only how the *second* commit is kept: past
/// the mark when the seconds differ, at the mark but absent from
/// `commits_at_watermark` when they do not. Run 4's answer is the same either
/// way. It also does not rest on run 3's cursor being byte-identical to run
/// 2's -- the repository entity may have moved it -- because an idle run
/// returns early from the commit walk and carries `commits_since` and
/// `commits_at_watermark` across untouched, which is the very property run 4
/// then measures.
///
/// As above, the repository entity may ride along in any run and nothing here
/// asserts it away (#140); the branch name carries the same epoch-second
/// caveat; and the branch this pushes onto is removed again when the test ends,
/// passing or panicking alike -- see the header.
#[tokio::test]
#[ignore = "needs testenv's seeded Gitea container"]
async fn a_commit_pushed_through_the_api_arrives_once_and_only_once_and_so_does_the_next_push() {
    let env = env();
    // Built before the baseline sync: clearing what a killed run left behind is
    // itself a change to the repository, and this run's cursor must be taken after
    // it rather than before.
    let mut litter = Litter::new(&env).await;
    let source = env.one_repo();
    let (_, cursor) = full(&*source).await;

    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let branch = format!("knobas-commit-{stamp}");
    let http = reqwest::Client::new();

    litter.branch_off_main(&branch).await;

    // A unique path, so a re-run cannot collide.
    let sha = push_file(
        &http,
        &env,
        &branch,
        &format!("knobas-live-{stamp}.txt"),
        &format!("knobas live check {stamp}"),
    )
    .await;

    let mut sink = VecSink(Vec::new());
    let moved = source
        .sync(Some(cursor.clone()), &mut sink)
        .await
        .expect("incremental sync");
    let commits: Vec<&str> = of_kind(&sink.0, "commit")
        .iter()
        .map(|i| i.entity.key.as_str())
        .collect();
    let only = format!("{}@{sha}", env.full_name());
    assert_eq!(
        commits,
        vec![only.as_str()],
        "the new branch inherits main's whole history; `since=` and \
         commits_at_watermark are what have to leave all of it out"
    );

    // …and the run after it re-delivers nothing, which is the inclusive
    // `since=` boundary being closed rather than merely narrow. Same exemption
    // and same reason as the pull-request test above: this walk's own flake
    // rate before the scoping was 4 runs in 25, always the repository alone.
    let mut idle = VecSink(Vec::new());
    let same = source
        .sync(Some(moved.clone()), &mut idle)
        .await
        .expect("idle sync");
    assert_eq!(
        re_delivered(&idle.0, &env.full_name()),
        Vec::<(&str, &str)>::new(),
        "the run after the incremental one re-delivered what it had already sent"
    );
    // Clause 2's cursor half as the equivalence, for the reason spelled out on
    // the pull-request test above: byte-identical when the run emitted nothing,
    // moved when the repository rode along, and neither half vacuous.
    assert_eq!(
        same == moved,
        idle.0.is_empty(),
        "the run emitted {:?} and its cursor {}",
        idle.0
            .iter()
            .map(|i| i.entity.to_string())
            .collect::<Vec<_>>(),
        if same == moved {
            "stood still"
        } else {
            "moved"
        }
    );

    // Run 4: the same branch, pushed to a second time. Everything above stops
    // at a run that never entered the commit walk, so this is the one that
    // makes run 2's watermark observable at all -- see the doc comment.
    let second = push_file(
        &http,
        &env,
        &branch,
        &format!("knobas-live-{stamp}-again.txt"),
        &format!("knobas live check {stamp}, again"),
    )
    .await;

    let mut walked = VecSink(Vec::new());
    let onward = source
        .sync(Some(same.clone()), &mut walked)
        .await
        .expect("the incremental sync after the second push");
    let arrived: Vec<&str> = of_kind(&walked.0, "commit")
        .iter()
        .map(|i| i.entity.key.as_str())
        .collect();
    let only_the_second = format!("{}@{second}", env.full_name());
    assert_eq!(
        arrived,
        vec![only_the_second.as_str()],
        "the run after the second push must return exactly the commit that push \
         made. The first push ({sha}) back as well means an incremental run's \
         commit watermark never advanced past it (issue #152); nothing at all \
         means the walk was not entered, so this assertion is measuring nothing"
    );
    assert_ne!(
        onward, same,
        "the position must move when something was emitted"
    );
}

/// Exit criterion B's last clause. Also the one thing the docker-free fake
/// cannot certify at all: it answers 401 because it was told to.
#[tokio::test]
#[ignore = "needs testenv's seeded Gitea container"]
async fn a_revoked_token_is_unauthorized() {
    let env = env();
    let source = env.source_with(&revoked(), serde_json::json!({}));
    assert!(matches!(
        source.test_connection().await,
        Err(SourceError::Unauthorized { .. })
    ));
    let mut sink = VecSink(Vec::new());
    assert!(matches!(
        source.sync(None, &mut sink).await,
        Err(SourceError::Unauthorized { .. })
    ));
    // What the empty sink measures, stated as what it is (#347). It said "a
    // revoked token must not sync whatever this instance serves anonymously",
    // and that is a claim this run cannot present: `Env::source_with` always
    // sets a secret, so every request carries `Authorization: token
    // revoked-<pid>` and none of them is anonymous. The anonymous fallback --
    // this instance really does serve signed-out readers,
    // `GITEA__service__REQUIRE_SIGNIN_VIEW: "false"` -- is caught by the
    // assertion above instead: an adapter that dropped the header would get a
    // 200 and answer `Ok`, not `Unauthorized`. What is left for the sink is
    // narrower and worth keeping: a refused run commits nothing, not even the
    // items it had streamed before the listing that failed.
    assert!(
        sink.0.is_empty(),
        "a refused run must emit nothing at all -- this adapter streams into the sink as it \
         walks, so a run that pushed items before the 401 would leave the engine half a corpus"
    );
}

/// Interfaces §4.2 calls ETags "an **optimization to verify against the real
/// container**, not a contract". This verifies. It asserts only that the
/// requests work either way -- what it produces is the finding, printed, for
/// the M2 decision. Nothing in the adapter depends on ETags and the `v:1`
/// cursor has no field for one.
#[tokio::test]
#[ignore = "needs testenv's seeded Gitea container"]
async fn etag_support_probe() {
    let env = env();
    let http = reqwest::Client::new();
    let auth = format!("token {}", env.token);
    for path in [
        "repos/search".to_owned(),
        format!("repos/{}/branches", env.full_name()),
        format!(
            "repos/{}/pulls?state=all&sort=recentupdate",
            env.full_name()
        ),
    ] {
        let url = format!("{}/api/v1/{path}", env.url);
        let first = http
            .get(&url)
            .header("Authorization", &auth)
            .send()
            .await
            .expect("probe");
        assert!(first.status().is_success(), "{path}: {}", first.status());
        let etag = first
            .headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        println!("ETAG PROBE {path}: etag={etag:?}");
        let Some(etag) = etag else { continue };
        let second = http
            .get(&url)
            .header("Authorization", &auth)
            .header("If-None-Match", &etag)
            .send()
            .await
            .expect("conditional probe");
        println!("ETAG PROBE {path}: If-None-Match -> {}", second.status());
    }
}

/// The page size the adapter's *listing* requests ask for, mirrored here
/// because `client::PAGE_SIZE` is crate-private and an integration test cannot
/// see it. The discussion below is deliberately longer than one, which is the
/// only size at which "the endpoint does not page" says anything.
const PAGE: usize = 50;

/// Issue #131, and the assumption the adapter now rests on, against the server
/// that decides it.
///
/// #131 read `issue_comments` as a listing asked without a `limit`, and
/// therefore truncated at Gitea's `DEFAULT_PAGING_NUM` -- thirty comments, on a
/// stock install, today. If that were so, the fix would be to page it. It is
/// not so, and the fix is the opposite: `issueGetComments` **is not a paged
/// endpoint**, so a walk over it would re-read the same discussion until it ran
/// out of budget and fold every comment into `body_text` once per request.
///
/// Nothing docker-free can settle that, which is exactly what this file is for.
/// Four things are checked here, and the adapter is wrong in a different way if
/// any of them stops holding:
///
/// 1. **The endpoint's own OpenAPI declaration carries no `page` and no
///    `limit`** -- so the adapter sends neither.
/// 2. **The repository-wide comments endpoint next to it declares and honours
///    both.** The control that makes 1 a fact about the endpoint rather than
///    about this instance's configuration.
/// 3. **`limit` and `page` are ignored**: a discussion of `PAGE + 1` comes back
///    whole for no query at all, for `limit=50&page=1`, for `limit=2&page=1`
///    and for `limit=50&page=9`.
/// 4. **`X-Total-Count` equals what the body carried**, which is the signal
///    `sync::fetch_comments` refuses a short discussion on. A server where it
///    did not would fail every run, so this is also the check that the guard
///    cannot fire against a healthy Gitea.
///
/// And then the whole thing end to end: the adapter mirrors all `PAGE + 1`
/// comments into the pull request's indexed text in one sync.
///
/// The discussion is written through Gitea's own API, like the pull request in
/// the test above. `PAGE + 1` comment POSTs is what this test costs.
#[tokio::test]
#[ignore = "needs testenv's seeded Gitea container"]
async fn the_discussion_endpoint_does_not_page() {
    let env = env();
    // Built before the baseline sync: clearing what a killed run left behind is
    // itself a change to the repository, and this run's cursor must be taken after
    // it rather than before.
    let mut litter = Litter::new(&env).await;
    let source = env.one_repo();
    let (_, cursor) = full(&*source).await;

    let http = reqwest::Client::new();
    let auth = format!("token {}", env.token);
    let api = format!("{}/api/v1/repos/{}", env.url, env.full_name());

    // 1 and 2: what the server says about itself, before anything is written.
    let swagger: serde_json::Value = http
        .get(format!("{}/swagger.v1.json", env.url))
        .send()
        .await
        .expect("the container publishes its OpenAPI document")
        .json()
        .await
        .expect("swagger.v1.json is JSON");
    let params = |path: &str| -> Vec<String> {
        swagger["paths"][path]["get"]["parameters"]
            .as_array()
            .unwrap_or_else(|| panic!("{path} is in the document"))
            .iter()
            .filter_map(|p| p["name"].as_str().map(str::to_owned))
            .collect()
    };
    let discussion = params("/repos/{owner}/{repo}/issues/{index}/comments");
    assert!(
        !discussion.contains(&"page".to_owned()) && !discussion.contains(&"limit".to_owned()),
        "Gitea has given the discussion endpoint paging parameters: {discussion:?}. \
         The adapter reads it in one request on the strength of their absence -- \
         re-measure it and see client::issue_comments"
    );
    let repo_wide = params("/repos/{owner}/{repo}/issues/comments");
    assert!(
        repo_wide.contains(&"page".to_owned()) && repo_wide.contains(&"limit".to_owned()),
        "the repository-wide comments endpoint is the control for the assertion above, \
         and it has stopped declaring paging too: {repo_wide:?}"
    );

    // A pull request of this run's own, with a discussion longer than any
    // listing page.
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let branch = format!("knobas-live-discussion-{stamp}");
    let note = |n: usize| format!("knobas live discussion note #{n:03} of run {stamp}");

    litter.branch_off_main(&branch).await;
    let opened = http
        .post(format!("{api}/pulls"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "head": branch, "base": "main",
            "title": format!("knobas live discussion check {stamp}"),
            "body": "Opened by the knobas Gitea adapter's live suite (issue #131)."
        }))
        .send()
        .await
        .expect("open the pull request");
    assert!(
        opened.status().is_success(),
        "pull: {}",
        opened.text().await.unwrap_or_default()
    );
    let number = opened.json::<serde_json::Value>().await.unwrap()["number"]
        .as_u64()
        .expect("Gitea answers with the new pull request's number");

    for n in 1..=PAGE + 1 {
        let posted = http
            .post(format!("{api}/issues/{number}/comments"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "body": note(n) }))
            .send()
            .await
            .expect("comment on the pull request");
        assert!(
            posted.status().is_success(),
            "comment {n}: {}",
            posted.text().await.unwrap_or_default()
        );
    }

    // 3 and 4: what the endpoint does with paging parameters, and what it says
    // about its own completeness.
    for query in [
        "",
        "?limit=50&page=1",
        "?limit=2&page=1",
        "?limit=50&page=9",
    ] {
        let answered = http
            .get(format!("{api}/issues/{number}/comments{query}"))
            .header("Authorization", &auth)
            .send()
            .await
            .expect("read the discussion back");
        let total = answered
            .headers()
            .get("x-total-count")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<usize>().ok());
        let body: Vec<serde_json::Value> = answered.json().await.expect("a comment array");
        assert_eq!(
            body.len(),
            PAGE + 1,
            "query {query:?} paged the discussion; the adapter reads it in one request"
        );
        assert_eq!(
            total,
            Some(PAGE + 1),
            "query {query:?}: X-Total-Count is what sync::fetch_comments refuses a short \
             discussion on, and it must agree with the body on a healthy server"
        );
    }

    // End to end: the mirror carries all of it.
    let mut sink = VecSink(Vec::new());
    source
        .sync(Some(cursor), &mut sink)
        .await
        .expect("incremental sync");
    let key = format!("gitea:{}#{number}", env.full_name());
    let pr = sink
        .0
        .iter()
        .find(|i| i.entity.to_string() == key)
        .unwrap_or_else(|| {
            panic!(
                "the new pull request is missing from {:?}",
                sink.0
                    .iter()
                    .map(|i| i.entity.to_string())
                    .collect::<Vec<_>>()
            )
        });
    let missing: Vec<usize> = (1..=PAGE + 1)
        .filter(|n| !pr.body_text.contains(&note(*n)))
        .collect();
    assert!(
        missing.is_empty(),
        "the discussion came back truncated: {} of {} comments are missing from body_text, \
         first {:?}",
        missing.len(),
        PAGE + 1,
        missing.first()
    );
    // Each comment exactly once: a walk over an endpoint that ignores `page`
    // would have folded the whole discussion in once per request.
    assert_eq!(
        pr.body_text.matches(&note(1)).count(),
        1,
        "the first comment is in body_text more than once"
    );
}

// -- M2's write-back set, against the real server (issue #43) -----------------
//
// The fake in `tests/write.rs` encodes a reading of four endpoints -- the field
// names Gitea's `CreateBranchRepoOption`, `CreatePullRequestOption`,
// `CreateIssueCommentOption` and `CreatePullReviewOptions` declare, and the
// fact that `event: "APPROVED"` is what separates an approval from a pending
// review. **If this suite and that fake disagree, the fake is wrong.**
//
// One test rather than four: the four ops compose -- there is nothing to open a
// pull request from until a branch exists, and nothing to approve until a pull
// request does -- and splitting them would mean three tests each re-creating
// the others' preconditions through Gitea's own API, which is precisely the
// path this is supposed to be certifying.

/// Stories 5-8: the adapter creates a branch, opens a pull request on it,
/// replies to it and approves it -- and the **server** says so afterwards.
///
/// Every assertion is read back through Gitea's own API, not through the
/// adapter: what is being certified is that the far end received a request it
/// understood, and an assertion made through the same code that sent it would
/// certify nothing.
///
/// The branch and the pull request opened from it are removed again when this
/// returns, whether it passes or panics ([`live_env::Litter`]).
#[tokio::test]
#[ignore = "needs testenv's seeded Gitea container"]
async fn the_adapter_creates_a_branch_a_pull_request_a_comment_and_an_approval() {
    let env = env();
    let mut litter = Litter::new(&env).await;
    let source = env.one_repo();
    let repo_id = format!("gitea:{}", env.full_name());

    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let branch = format!("knobas-write-{stamp}");

    // Recorded before the create, so a branch made by a request whose answer
    // was lost is still swept.
    litter.will_create(&branch);
    source
        .write(knobas_source::WriteOp::CreateBranch {
            entity: repo_id.clone(),
            name: branch.clone(),
            from_ref: "main".to_owned(),
        })
        .await
        .expect("the adapter creates a branch");
    assert!(
        litter.branch_names().await.contains(&branch),
        "the branch the adapter created is not in the server's own listing"
    );

    source
        .write(knobas_source::WriteOp::CreatePullRequest {
            entity: repo_id.clone(),
            title: format!("knobas write-back check {stamp}"),
            body: "Opened by the knobas Gitea adapter's write path.".to_owned(),
            head: branch.clone(),
            base: "main".to_owned(),
        })
        .await
        .expect("the adapter opens a pull request");
    let number = litter
        .pulls()
        .await
        .into_iter()
        .find(|(_, head)| head == &branch)
        .map(|(number, _)| number)
        .expect("the pull request the adapter opened is not in the server's own listing");

    let pull_id = format!("gitea:{}#{number}", env.full_name());
    let note = format!("a reply from the knobas write path, {stamp}");
    source
        .write(knobas_source::WriteOp::Comment {
            entity: pull_id.clone(),
            body: note.clone(),
        })
        .await
        .expect("the adapter comments");
    assert!(
        litter.comments(number).await.contains(&note),
        "the comment the adapter posted is not on the pull request's discussion -- Gitea keeps \
         it on the issue of the same index, which is what `issues/{{index}}/comments` relies on"
    );

    // The approval needs a pull request **somebody else** opened: Gitea answers
    // `approve your own pull is not allowed` with a 422, and everything above
    // was authored by this suite's own token. So a second branch, and a pull
    // request opened on it as one of the fixture's people.
    let other_branch = format!("knobas-write-{stamp}-other");
    litter.will_create(&other_branch);
    source
        .write(knobas_source::WriteOp::CreateBranch {
            entity: repo_id,
            name: other_branch.clone(),
            from_ref: "main".to_owned(),
        })
        .await
        .expect("the adapter creates the second branch");
    let theirs = litter
        .open_pull_as(
            "jonas.becker",
            &other_branch,
            &format!("knobas write-back check {stamp}, opened by somebody else"),
        )
        .await;

    source
        .write(knobas_source::WriteOp::Approve {
            entity: format!("gitea:{}#{theirs}", env.full_name()),
            body: "approved by the knobas write path".to_owned(),
        })
        .await
        .expect("the adapter approves");
    let reviews = litter.reviews(theirs).await;
    assert!(
        reviews
            .iter()
            .any(|(state, body)| state == "APPROVED" && body == "approved by the knobas write path"),
        "the server did not record an APPROVED review -- a `POST .../reviews` with no `event` \
         files a PENDING one, which unblocks nobody: {reviews:?}"
    );
}

/// The server's own rule, certified rather than assumed: **Gitea refuses an
/// approval of your own pull request**, with a 422.
///
/// Recorded here because it is the reason the test above needs a second
/// author, and because of what it means for the queue: 422 is not a fault that
/// passes, so it arrives as a `Protocol` refusal that is **not retried** and
/// carries Gitea's own sentence for the user to read. A knobas that retried it
/// would ask forever.
#[tokio::test]
#[ignore = "needs testenv's seeded Gitea container"]
async fn approving_your_own_pull_request_is_refused_by_the_server() {
    let env = env();
    let mut litter = Litter::new(&env).await;
    let source = env.one_repo();
    let repo_id = format!("gitea:{}", env.full_name());
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let branch = format!("knobas-write-own-{stamp}");

    litter.will_create(&branch);
    source
        .write(knobas_source::WriteOp::CreateBranch {
            entity: repo_id.clone(),
            name: branch.clone(),
            from_ref: "main".to_owned(),
        })
        .await
        .expect("the adapter creates a branch");
    source
        .write(knobas_source::WriteOp::CreatePullRequest {
            entity: repo_id,
            title: format!("knobas self-approval check {stamp}"),
            body: String::new(),
            head: branch.clone(),
            base: "main".to_owned(),
        })
        .await
        .expect("the adapter opens a pull request");
    let number = litter
        .pulls()
        .await
        .into_iter()
        .find(|(_, head)| head == &branch)
        .map(|(number, _)| number)
        .expect("the pull request the adapter opened");

    let refused = source
        .write(knobas_source::WriteOp::Approve {
            entity: format!("gitea:{}#{number}", env.full_name()),
            body: String::new(),
        })
        .await;
    let Err(error) = &refused else {
        panic!("Gitea allowed a self-approval; the suite above no longer needs a second author");
    };
    assert_eq!(error.status(), Some(422), "{error:?}");
    assert!(
        matches!(error, SourceError::Protocol { message: m, .. } if m.contains("approve your own")),
        "the server's own sentence must reach the queue: {error:?}"
    );
}

/// The other half of story 13, certified against the server that would
/// otherwise have answered: an op this adapter does not declare is refused
/// **before** a request is made.
///
/// Against the real container rather than the fake, because the claim is that
/// nothing reached a server that was perfectly willing to answer.
#[tokio::test]
#[ignore = "needs testenv's seeded Gitea container"]
async fn an_op_gitea_does_not_declare_never_reaches_the_real_server() {
    let env = env();
    let source = env.one_repo();
    let refused = source
        .write(knobas_source::WriteOp::Transition {
            entity: format!("gitea:{}#1", env.full_name()),
            status: "Done".to_owned(),
        })
        .await;
    assert!(
        matches!(refused, Err(SourceError::Protocol { message: ref m, .. }) if m.contains("transition")),
        "{refused:?}"
    );
}

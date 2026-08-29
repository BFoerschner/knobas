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
//! `Drop`. Three consequences, each a decision:
//!
//! * **The failure path is the success path.** A test that panics unwinds
//!   through the same cleanup a passing test returns through. A cleanup that
//!   ran only on success would leave the residue on exactly the runs that were
//!   already going badly.
//! * **The removal is checked, not hoped for.** `Litter`'s `Drop` re-reads the
//!   listings afterwards and fails the test if anything it created is still
//!   standing, so a cleanup that quietly stopped deleting cannot pass as a
//!   clean run.
//! * **A killed process is swept, not mourned.** `Drop` cannot survive a
//!   `SIGKILL` or a Ctrl-C at the wrong moment, so the next run's first
//!   mutating test removes whatever such a run left. Recovery from a dirty
//!   environment is "run the suite again".
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
///   and [`a_commit_pushed_through_the_api_arrives_once_and_only_once`];
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
    assert!(
        sepa.body_text.len() > sepa.title.len(),
        "body_text is the title alone, so nothing was folded in: {:?}",
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
    assert!(
        of_kind(&items, "commit").iter().all(|i| i.author.is_some()),
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
    // Built before the baseline sync: sweeping what a killed run left behind is
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
/// twice.
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
/// As above, the repository entity may ride along in any run and nothing here
/// asserts it away (#140); the branch name carries the same epoch-second
/// caveat; and the branch this pushes onto is removed again when the test ends,
/// passing or panicking alike -- see the header.
#[tokio::test]
#[ignore = "needs testenv's seeded Gitea container"]
async fn a_commit_pushed_through_the_api_arrives_once_and_only_once() {
    let env = env();
    // Built before the baseline sync: sweeping what a killed run left behind is
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
    let api = format!("{}/api/v1/repos/{}", env.url, env.full_name());
    let http = reqwest::Client::new();
    let auth = format!("token {}", env.token);

    litter.branch_off_main(&branch).await;

    // A unique path, so a re-run cannot collide; fixed content, so this needs
    // no base64 encoder. `a25vYmFzIGxpdmUgY2hlY2sK` is "knobas live check\n".
    let wrote = http
        .post(format!("{api}/contents/knobas-live-{stamp}.txt"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "branch": branch,
            "content": "a25vYmFzIGxpdmUgY2hlY2sK",
            "message": format!("knobas live check {stamp}")
        }))
        .send()
        .await
        .expect("write the file");
    assert!(
        wrote.status().is_success(),
        "contents: {}",
        wrote.text().await.unwrap_or_default()
    );
    let sha = wrote.json::<serde_json::Value>().await.unwrap()["commit"]["sha"]
        .as_str()
        .expect("Gitea answers with the commit it made")
        .to_owned();

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
    assert!(
        sink.0.is_empty(),
        "a revoked token must not sync whatever this instance serves anonymously"
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
    // Built before the baseline sync: sweeping what a killed run left behind is
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

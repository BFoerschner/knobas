//! The suite exit criterion **B** is measured on: the real, pinned, seeded
//! Gitea container from `testenv/docker-compose.yml`.
//!
//! # Why this file is the contract and `tests/it/support/mod.rs` is not
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
//! cd testenv && docker compose --env-file images.env up -d gitea && ./seed
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

use knobas_source::contract::{Fault, VecSink, Walking, battery};
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
/// unnoticed. Delete `after.repo_fingerprint = Some(fingerprint)` from
/// `sync::sync_repo` and that is exactly what happens -- this filter drops the
/// item, and the cursor clause beside it only asks that a run which emitted
/// something moved its position, which such a run does.
///
/// That property is certified where the timing does not move under it, and
/// deliberately not here: docker-free by
/// `tests/it/sync.rs::an_idle_run_emits_nothing_and_returns_the_same_bytes` and
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
///   docker-free fake in `tests/it/sync.rs` --
///   `an_idle_run_emits_nothing_and_returns_the_same_bytes` and
///   `the_position_after_one_change_is_itself_idle_stable`, both on the
///   unfiltered selection, which walks the same listing an `owners[]` scope
///   does (`sync.rs`'s module doc: an empty `owners[]` means every repository
///   the token can see). Quiescence there is by construction.
///
/// Said exactly, because a record of a coverage loss is worth nothing if it
/// overstates what is left: **no docker-free test runs an idle pair with
/// `owners[]` actually set.** `an_owner_filter_drops_everything_else` in
/// `tests/it/sync.rs` certifies that the filter is a filter over that same walk,
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

/// A seeded commit's source view carries the diff the seed pushed (#661,
/// ADR-0020), read from the real server's `.diff`.
///
/// **What the seed pushes, and so what each view must say.** `seed-gitea.sh`'s
/// `ensure_commit` makes every commit of `fixtures/tidewater/work.json`
/// through Gitea's contents endpoint, writing **one new file** whose whole
/// content is the commit message and a newline. So each of those must draw: a
/// *Diff* tab counting one file; that file added, `+1 −0`; its one line the
/// commit's message, numbered 1 on the new side. Every commit the sync emits
/// -- the fixture's, `auto_init`'s, and whatever other live suites merged
/// into this repository -- must draw the files the server itself lists for it
/// (`files=true`, an endpoint the view does not read). The *Overview* must name the commit's
/// full sha and its seeded author, and each parent must be the address of a
/// commit this sync emitted where the sync holds it.
///
/// Prints each view's diff summary, which is the output the PR quotes.
#[tokio::test]
#[ignore = "needs testenv's seeded Gitea container"]
async fn a_seeded_commits_view_carries_the_diff_the_seed_pushed() {
    use knobas_source::view::{Block, DiffChange, DiffLineKind};

    let env = env();
    let source = env.one_repo();
    let (items, _) = full(&*source).await;
    let commits = of_kind(&items, "commit");
    let emitted: std::collections::HashSet<String> =
        commits.iter().map(|c| c.entity.to_string()).collect();
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../../fixtures/tidewater/work.json"))
            .expect("the fixture is JSON");
    let seeded: std::collections::HashSet<&str> = fixture["commits"]
        .as_array()
        .expect("the fixture lists commits")
        .iter()
        .filter(|c| c["repo"].as_str() == Some(env.repo.as_str()))
        .filter_map(|c| c["msg"].as_str())
        .collect();
    let http = reqwest::Client::new();
    let mut checked = 0;
    for commit in &commits {
        let id = commit.entity.to_string();
        let sha = commit
            .entity
            .key
            .rsplit_once('@')
            .map(|(_, sha)| sha.to_owned())
            .expect("a commit key is owner/repo@sha");
        let record: serde_json::Value = http
            .get(format!(
                "{}/api/v1/repos/{}/git/commits/{sha}?files=true&stat=true",
                env.url,
                env.full_name()
            ))
            .header("Authorization", format!("token {}", env.token))
            .send()
            .await
            .expect("the server answers")
            .json()
            .await
            .expect("a commit record");
        let message = record["commit"]["message"]
            .as_str()
            .expect("a message")
            .trim_end()
            .to_owned();
        let parents = record["parents"].as_array().map_or(0, Vec::len);

        let view = source
            .view(&id, None)
            .await
            .unwrap_or_else(|error| panic!("the view of {id} failed: {error:?}"))
            .unwrap_or_else(|| panic!("Gitea draws a commit, and drew nothing for {id}"));
        let ids: Vec<_> = view.sections.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["overview", "diff"], "{id}");

        let overview = &view.sections[0].blocks;
        let Block::Facts { facts } = &overview[0] else {
            panic!("{id}'s overview opens on facts: {overview:#?}");
        };
        assert!(
            facts.iter().any(|f| f.label == "SHA" && f.value == sha),
            "{id}: {facts:#?}"
        );
        let author = record["commit"]["author"]["name"].as_str().unwrap_or("");
        assert!(
            facts
                .iter()
                .any(|f| f.label == "Author" && f.value.starts_with(author)),
            "{id} names its author {author:?}: {facts:#?}"
        );
        let rows: Vec<_> = overview
            .iter()
            .filter_map(|b| match b {
                Block::Entities { rows, .. } => Some(rows),
                _ => None,
            })
            .flatten()
            .collect();
        assert_eq!(rows.len(), parents, "{id}: one row per parent");
        for row in &rows {
            let parent = row
                .entity_id
                .as_deref()
                .expect("the adapter names its parents");
            assert!(parent.starts_with("gitea:"), "{parent}");
            if emitted.contains(parent) {
                assert!(row.url.is_some(), "{parent} has a web page");
            }
        }

        let diff = &view.sections[1];
        let Block::Diff {
            files, truncated, ..
        } = &diff.blocks[0]
        else {
            panic!("{id}'s diff tab is a diff: {diff:#?}");
        };
        assert!(!truncated);
        assert_eq!(diff.count, Some(files.len() as u64));
        let listed: Vec<&str> = record["files"]
            .as_array()
            .expect("files=true lists the files")
            .iter()
            .filter_map(|f| f["filename"].as_str())
            .collect();
        println!(
            "{} {:<52} {:?}",
            &sha[..10],
            message,
            files
                .iter()
                .map(|f| format!(
                    "{:?} {} +{} -{}",
                    f.change, f.path, f.additions, f.deletions
                ))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
            listed,
            "{id}: the files the view draws are the files the server lists"
        );
        if !seeded.contains(message.as_str()) {
            // `auto_init`'s first commit, a merge another live suite made:
            // not the seed's, so held only to the server's own listing above.
            continue;
        }
        assert_eq!(files.len(), 1, "{id}: the seed writes one file per commit");
        let file = &files[0];
        assert_eq!(file.change, DiffChange::Added, "{id}");
        assert_eq!((file.additions, file.deletions), (1, 0), "{id}");
        let lines: Vec<_> = file.hunks.iter().flat_map(|h| &h.lines).collect();
        assert_eq!(lines.len(), 1, "{id}: {lines:#?}");
        assert_eq!(lines[0].kind, DiffLineKind::Add);
        assert_eq!((lines[0].old, lines[0].new), (None, Some(1)));
        assert_eq!(lines[0].text, message, "{id}: the seed writes the message");
        checked += 1;
    }
    assert_eq!(
        checked,
        seeded.len(),
        "every fixture commit of {} is synced and drawn",
        env.repo
    );
    println!("{checked} seeded commits drew the diff the seed pushed");
}

/// The seeded pull requests' source views (#662, ADR-0020), against the real
/// server.
///
/// **What the seed made, and so what each view must say.** `seed-gitea.sh`
/// opens `SEPA retry with exponential backoff` from
/// `feature/PAY-231-sepa-retry` into `main` as `mara`, with the body
/// `Refs PAY-231`; puts the fixture's three commits of that branch on it, each
/// writing one new file (the PR's file list in fixture order) whose content is
/// the commit message; comments as `jonas` then `mara`; has `priya` approve
/// and asks `jonas` for a review. It merges `Fix ledger drift on partial
/// refunds` in `ledger-api`.
///
/// So the SEPA view has all four tabs, *Files* deferred; its overview is
/// open, `head → base`, priya's approval and jonas's request; *Files* is the
/// server's own file listing (`/pulls/{n}/files`, an endpoint the view does
/// not read), with each seeded commit's file added as its one line; *Commits*
/// is the server's commit listing, the seeded messages among them;
/// *Conversation* is the server's comments in order, the fixture's two among
/// them. The ledger view reads merged.
///
/// Prints each tab, which is the output the PR quotes.
#[tokio::test]
#[ignore = "needs testenv's seeded Gitea container"]
async fn a_seeded_pull_requests_view_carries_its_files_commits_and_discussion() {
    use knobas_source::view::{Block, DiffChange, DiffLineKind, Tone};

    let env = env();
    let source = env.one_repo();
    let (items, _) = full(&*source).await;
    let sepa = of_kind(&items, "pr")
        .into_iter()
        .find(|i| i.title == "SEPA retry with exponential backoff")
        .expect("the seed opens the SEPA retry pull request");
    let id = sepa.entity.to_string();
    let number = sepa
        .entity
        .key
        .rsplit_once('#')
        .map(|(_, n)| n.to_owned())
        .expect("a pull request key is owner/repo#n");
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../../fixtures/tidewater/work.json"))
            .expect("the fixture is JSON");
    let person = |who: &str| -> String {
        fixture["people"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["id"] == who)
            .and_then(|p| p["username"].as_str())
            .unwrap_or_else(|| panic!("the fixture names {who}"))
            .to_owned()
    };
    let pr = fixture["prs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["title"] == "SEPA retry with exponential backoff")
        .unwrap();
    let mut seeded: Vec<&serde_json::Value> = fixture["commits"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["branch"] == pr["from"])
        .collect();
    seeded.sort_by_key(|c| c["when"].as_str().unwrap_or_default().to_owned());

    let http = reqwest::Client::new();
    let get = |path: String| {
        let http = http.clone();
        let url = format!("{}/api/v1/repos/{}/{path}", env.url, env.full_name());
        let auth = format!("token {}", env.token);
        async move {
            http.get(&url)
                .header("Authorization", auth)
                .send()
                .await
                .expect("the server answers")
                .json::<serde_json::Value>()
                .await
                .unwrap_or_else(|e| panic!("{url}: {e}"))
        }
    };

    let view = source
        .view(&id, None)
        .await
        .unwrap_or_else(|error| panic!("the view of {id} failed: {error:?}"))
        .unwrap_or_else(|| panic!("Gitea draws a pull request, and drew nothing for {id}"));
    let tabs: Vec<_> = view
        .sections
        .iter()
        .map(|s| (s.id.as_str(), s.count, s.deferred))
        .collect();
    println!("{id} tabs: {tabs:?}");
    assert_eq!(
        tabs.iter().map(|t| (t.0, t.2)).collect::<Vec<_>>(),
        [
            ("overview", false),
            ("files", true),
            ("commits", false),
            ("conversation", false)
        ]
    );
    assert!(view.sections[1].blocks.is_empty(), "files is a stub");

    // Overview.
    let overview = &view.sections[0].blocks;
    let Block::Facts { facts } = &overview[0] else {
        panic!("{id}'s overview opens on facts: {overview:#?}");
    };
    for fact in facts {
        println!("  {:<10} {:<40} {:?}", fact.label, fact.value, fact.tone);
    }
    let has = |label: &str, value: &str, tone: Option<Tone>| {
        assert!(
            facts
                .iter()
                .any(|f| f.label == label && f.value == value && f.tone == tone),
            "{id}: no {label} = {value:?} ({tone:?}) in {facts:#?}"
        );
    };
    has("State", "open", Some(Tone::Info));
    has("Author", &person("mara"), None);
    has(
        "Branches",
        &format!(
            "{} → {}",
            pr["from"].as_str().unwrap(),
            pr["to"].as_str().unwrap()
        ),
        None,
    );
    has(
        "Review",
        &format!("approved by {}", person("priya")),
        Some(Tone::Ok),
    );
    has(
        "Review",
        &format!("requested from {}", person("jonas")),
        Some(Tone::Muted),
    );
    assert!(
        facts.iter().any(|f| f.label == "Mergeable"),
        "{id}: an open pull request says whether it can be merged"
    );
    assert!(
        overview.contains(&Block::Text {
            text: format!("Refs {}", pr["ticket"].as_str().unwrap()),
            markdown: true
        }),
        "{id}: the description: {overview:#?}"
    );

    // Files: read on its own, as the tab does.
    let files_view = source
        .view(&id, Some("files"))
        .await
        .expect("the files tab reads")
        .expect("drawn");
    let Block::Diff {
        files, truncated, ..
    } = &files_view.sections[0].blocks[0]
    else {
        panic!("{id}'s files tab is a diff: {files_view:#?}");
    };
    assert!(!truncated);
    for file in files {
        println!(
            "  files: {:?} {} +{} -{}",
            file.change, file.path, file.additions, file.deletions
        );
    }
    let listed: Vec<String> = get(format!("pulls/{number}/files"))
        .await
        .as_array()
        .expect("a file listing")
        .iter()
        .filter_map(|f| f["filename"].as_str().map(str::to_owned))
        .collect();
    let mut drawn: Vec<String> = files.iter().map(|f| f.path.clone()).collect();
    let mut listed_sorted = listed.clone();
    drawn.sort();
    listed_sorted.sort();
    assert_eq!(drawn, listed_sorted, "{id}: the files are the server's");
    assert_eq!(
        view.sections[1].count,
        Some(files.len() as u64),
        "the stub's count is the files the tab draws"
    );
    let paths: Vec<&str> = pr["files"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|f| f["path"].as_str())
        .collect();
    for (commit, path) in seeded.iter().zip(&paths) {
        let message = commit["msg"].as_str().unwrap();
        let file = files
            .iter()
            .find(|f| f.path == *path)
            .unwrap_or_else(|| panic!("{id}: the seed wrote {path}"));
        assert_eq!(file.change, DiffChange::Added, "{path}");
        assert_eq!((file.additions, file.deletions), (1, 0), "{path}");
        let lines: Vec<_> = file.hunks.iter().flat_map(|h| &h.lines).collect();
        assert_eq!(lines.len(), 1, "{path}: {lines:#?}");
        assert_eq!(lines[0].kind, DiffLineKind::Add);
        assert_eq!(
            lines[0].text, message,
            "{path}: the seed writes the message"
        );
    }

    // Commits.
    let Block::Entities { rows, .. } = &view.sections[2].blocks[0] else {
        panic!(
            "{id}'s commits tab is an entity list: {:#?}",
            view.sections[2]
        );
    };
    for row in rows {
        println!("  commit: {:?} {} ({:?})", row.chip, row.title, row.meta);
    }
    let server: Vec<String> = get(format!(
        "pulls/{number}/commits?files=false&verification=false"
    ))
    .await
    .as_array()
    .expect("a commit listing")
    .iter()
    .filter_map(|c| c["sha"].as_str().map(str::to_owned))
    .collect();
    let addressed: Vec<String> = rows
        .iter()
        .map(|r| {
            r.entity_id
                .as_deref()
                .and_then(|e| e.rsplit_once('@'))
                .map(|(_, sha)| sha.to_owned())
                .expect("each commit names its address")
        })
        .collect();
    assert_eq!(
        addressed, server,
        "{id}: the commits are the server's, in its order"
    );
    assert_eq!(view.sections[2].count, Some(server.len() as u64));
    let titles: Vec<&str> = rows.iter().map(|r| r.title.as_str()).collect();
    for commit in &seeded {
        let message = commit["msg"].as_str().unwrap();
        assert!(titles.contains(&message), "{id}: {message:?} in {titles:?}");
    }

    // Conversation: the comments and, since #705, the reviews among them,
    // each its author's facts (a review's with its verdict) and then its text.
    let conversation = &view.sections[3].blocks;
    let mut entries: Vec<(String, Option<String>, String)> = Vec::new();
    for block in conversation {
        match block {
            Block::Facts { facts } => entries.push((
                facts[0].label.clone(),
                facts
                    .iter()
                    .find(|f| f.label == "Review")
                    .map(|f| f.value.clone()),
                String::new(),
            )),
            Block::Text { text, .. } => {
                let last = entries
                    .last_mut()
                    .unwrap_or_else(|| panic!("{id}: text before its author: {conversation:#?}"));
                assert!(last.2.is_empty(), "{id}: two texts for one entry");
                last.2.clone_from(text);
            }
            other => panic!("{id}: a conversation is facts and text: {other:#?}"),
        }
    }
    let said: Vec<(String, String)> = entries
        .iter()
        .filter(|(_, verdict, _)| verdict.is_none())
        .map(|(who, _, text)| (who.clone(), text.clone()))
        .collect();
    for (who, verdict, text) in &entries {
        println!("  said: {who} {verdict:?}: {text}");
    }
    let bodies: Vec<String> = get(format!("issues/{number}/comments"))
        .await
        .as_array()
        .expect("a comment listing")
        .iter()
        .filter_map(|c| c["body"].as_str().map(str::to_owned))
        .collect();
    assert_eq!(
        said.iter().map(|(_, t)| t.clone()).collect::<Vec<_>>(),
        bodies,
        "{id}: the comments are the server's, in order"
    );
    let reviews: Vec<(String, String)> = get(format!("pulls/{number}/reviews"))
        .await
        .as_array()
        .expect("a review listing")
        .iter()
        .filter(|r| !matches!(r["state"].as_str(), Some("PENDING" | "REQUEST_REVIEW")))
        .map(|r| {
            (
                r["user"]["login"].as_str().unwrap_or_default().to_owned(),
                r["body"].as_str().unwrap_or_default().trim_end().to_owned(),
            )
        })
        .collect();
    assert_eq!(
        entries
            .iter()
            .filter(|(_, verdict, _)| verdict.is_some())
            .map(|(who, _, text)| (who.clone(), text.clone()))
            .collect::<Vec<_>>(),
        reviews,
        "{id}: the reviews are the server's, in order, among the comments (#705)"
    );
    let expected: Vec<(String, String)> = pr["comments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            (
                person(c["who"].as_str().unwrap()),
                c["text"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    let seeded_said: Vec<_> = said.iter().filter(|s| expected.contains(s)).collect();
    assert_eq!(
        seeded_said,
        expected.iter().collect::<Vec<_>>(),
        "{id}: the fixture's comments, by their authors, in order"
    );

    // The fixture's merged pull request reads merged, not Gitea's `closed`.
    let ledger = env.source(serde_json::json!({ "repos": [format!("{}/ledger-api", env.owner)] }));
    let (items, _) = full(&*ledger).await;
    let merged = of_kind(&items, "pr")
        .into_iter()
        .find(|i| i.title == "Fix ledger drift on partial refunds")
        .expect("the seed merges the ledger pull request");
    let view = ledger
        .view(&merged.entity.to_string(), None)
        .await
        .expect("the ledger view reads")
        .expect("drawn");
    let Block::Facts { facts } = &view.sections[0].blocks[0] else {
        panic!("facts first: {view:#?}");
    };
    let state = facts.iter().find(|f| f.label == "State").expect("a state");
    println!("{}: State {} {:?}", merged.entity, state.value, state.tone);
    assert_eq!(
        (state.value.as_str(), state.tone),
        ("merged", Some(Tone::Ok))
    );
    assert!(!facts.iter().any(|f| f.label == "Mergeable"), "{facts:#?}");
}

/// The seeded feature branch's source view (#663, ADR-0020), read from the
/// real server: it is ahead of `main` by at least the fixture's commits on
/// it, its *Commits* tab lists each of them by message, its open pull request
/// is the seeded one, and its deferred *Diff* -- the compare's
/// `?output=diff` -- carries a file for every one of those commits (the seed
/// writes one file per commit, holding the message).
///
/// Prints the view's facts, rows and diff summary, which is the output the
/// PR quotes.
#[tokio::test]
#[ignore = "needs testenv's seeded Gitea container"]
async fn a_seeded_branchs_view_reads_ahead_with_its_seeded_commits() {
    use knobas_source::view::Block;

    let env = env();
    let source = env.one_repo();
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../../fixtures/tidewater/work.json"))
            .expect("the fixture is JSON");
    let branch = "feature/PAY-231-sepa-retry";
    let seeded: Vec<&str> = fixture["commits"]
        .as_array()
        .expect("the fixture lists commits")
        .iter()
        .filter(|c| c["repo"].as_str() == Some(env.repo.as_str()))
        .filter(|c| c["branch"].as_str() == Some(branch))
        .filter_map(|c| c["msg"].as_str())
        .collect();
    assert!(!seeded.is_empty(), "the fixture commits to {branch}");
    let pr_title = fixture["prs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["from"].as_str() == Some(branch))
        .and_then(|p| p["title"].as_str())
        .expect("the fixture opens a pull request from the branch");

    let id = format!("gitea:{}@refs/heads/{branch}", env.full_name());
    let view = source
        .view(&id, None)
        .await
        .unwrap_or_else(|error| panic!("the view of {id} failed: {error:?}"))
        .expect("Gitea draws a branch");
    let ids: Vec<_> = view.sections.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["overview", "commits", "diff", "files"], "{id}");

    let overview = &view.sections[0];
    // Its open pull request leads the overview (#707), so the facts are the
    // block after it.
    let Some(facts) = overview.blocks.iter().find_map(|b| match b {
        Block::Facts { facts } => Some(facts),
        _ => None,
    }) else {
        panic!("{overview:#?}");
    };
    for f in facts {
        println!("fact  {:<14} {}", f.label, f.value);
    }
    let read = |label: &str| {
        facts
            .iter()
            .find(|f| f.label == label)
            .map(|f| f.value.clone())
            .unwrap_or_else(|| panic!("no {label} fact: {facts:#?}"))
    };
    assert_eq!(read("Base"), "main");
    let ahead: usize = read("Ahead").parse().expect("a count");
    assert!(
        ahead >= seeded.len(),
        "{branch} is ahead by at least its {} seeded commits, read {ahead}",
        seeded.len()
    );
    let entity_rows = |section: &knobas_source::view::ViewSection, title: Option<&str>| {
        section
            .blocks
            .iter()
            .find_map(|b| match b {
                Block::Entities { title: t, rows } if t.as_deref() == title => Some(rows.clone()),
                _ => None,
            })
            .unwrap_or_default()
    };
    let pulls = entity_rows(overview, Some("Pull request"));
    for row in &pulls {
        println!("pull  {} {:?} {:?}", row.title, row.meta, row.chip);
    }
    assert!(
        pulls.iter().any(|r| r.title == pr_title
            && r.chip.as_deref() == Some("open")
            && r.entity_id.as_deref().is_some_and(|e| e.contains('#'))),
        "the seeded pull request {pr_title:?} is the branch's: {pulls:#?}"
    );
    let last = entity_rows(overview, Some("Last commit"));
    assert_eq!(last.len(), 1, "{overview:#?}");

    let commits = &view.sections[1];
    assert_eq!(
        commits.count,
        Some(ahead as u64),
        "the count is what it lists: all it is ahead, under the cap"
    );
    let rows = entity_rows(commits, None);
    for row in &rows {
        println!("commit {:<52} {:?} {:?}", row.title, row.chip, row.meta);
    }
    for message in &seeded {
        assert!(
            rows.iter().any(|r| r.title == *message),
            "the seeded commit {message:?} is listed: {rows:#?}"
        );
    }
    // One commit row (#678): the tip from `/branches` and the same commit
    // from the compare read the same.
    assert!(
        rows.contains(&last[0]),
        "the last commit reads as its row in the list: {last:#?} in {rows:#?}"
    );
    // And the pull request's Commits, from `/pulls/{n}/commits`, list the
    // same commits in the same rows.
    let pr_id = pulls
        .iter()
        .find(|r| r.title == pr_title)
        .and_then(|r| r.entity_id.clone())
        .expect("the seeded pull request has an address");
    let in_pr = source
        .view(&pr_id, Some("commits"))
        .await
        .unwrap_or_else(|error| panic!("the commits of {pr_id} failed: {error:?}"))
        .expect("Gitea draws a pull request");
    let in_pr = entity_rows(&in_pr.sections[0], None);
    let mut shared = 0;
    for row in &in_pr {
        if let Some(same) = rows.iter().find(|r| r.entity_id == row.entity_id) {
            assert_eq!(same, row, "one commit, one row in both views");
            shared += 1;
        }
    }
    assert!(
        shared >= seeded.len(),
        "the pull request lists the branch's seeded commits: {in_pr:#?}"
    );

    let stub = &view.sections[2];
    assert!(stub.deferred && stub.blocks.is_empty());
    let diff = source
        .view(&id, Some("diff"))
        .await
        .expect("the diff tab reads")
        .expect("drawn");
    let Block::Diff {
        files, truncated, ..
    } = &diff.sections[0].blocks[0]
    else {
        panic!("{diff:#?}");
    };
    assert!(!truncated);
    for file in files {
        println!(
            "diff  {:?} {} +{} -{}",
            file.change, file.path, file.additions, file.deletions
        );
    }
    // The tab's count is read from the compare's commits, not the diff
    // (`Compare::files`); on the seeded branch, which restores nothing, the
    // two agree, and this is what says 1.27.3's compare carries the files.
    assert_eq!(
        stub.count,
        Some(files.len() as u64),
        "the Diff tab counts the diff's files"
    );
    let lines: Vec<&str> = files
        .iter()
        .flat_map(|f| &f.hunks)
        .flat_map(|h| &h.lines)
        .map(|l| l.text.as_str())
        .collect();
    for message in &seeded {
        assert!(
            lines.contains(message),
            "the diff carries the line the seed wrote for {message:?}"
        );
    }
    assert!(
        stub.count.is_some_and(|n| n >= files.len() as u64),
        "the stub's count ({:?}) counts every file the diff draws ({})",
        stub.count,
        files.len()
    );
}

/// The seeded repository's source view (#663), read from the real server:
/// its open pull requests include the fixture's open ones, its branches the
/// fixture's with `main` first, its README is the one `auto_init` wrote, and
/// its *Commits* are `main`'s.
///
/// Prints the view, which is the output the PR quotes.
#[tokio::test]
#[ignore = "needs testenv's seeded Gitea container"]
async fn the_seeded_repos_view_lists_its_pull_requests_and_branches() {
    use knobas_source::view::Block;

    let env = env();
    let source = env.one_repo();
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../../fixtures/tidewater/work.json"))
            .expect("the fixture is JSON");
    let in_repo = |list: &str| -> Vec<serde_json::Value> {
        fixture[list]
            .as_array()
            .unwrap()
            .iter()
            .filter(|x| x["repo"].as_str() == Some(env.repo.as_str()))
            .cloned()
            .collect()
    };
    let open_titles: Vec<String> = in_repo("prs")
        .iter()
        .filter(|p| p["state"].as_str() == Some("open"))
        .filter_map(|p| p["title"].as_str().map(str::to_owned))
        .collect();
    let branch_names: Vec<String> = in_repo("branches")
        .iter()
        .filter_map(|b| b["name"].as_str().map(str::to_owned))
        .collect();
    assert!(!open_titles.is_empty() && branch_names.len() > 1);

    let id = format!("gitea:{}", env.full_name());
    let view = source
        .view(&id, None)
        .await
        .unwrap_or_else(|error| panic!("the view of {id} failed: {error:?}"))
        .expect("Gitea draws a repository");
    let ids: Vec<_> = view.sections.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(
        ids,
        ["overview", "pulls", "branches", "commits", "files"],
        "{id}"
    );
    let rows_of = |index: usize| {
        view.sections[index]
            .blocks
            .iter()
            .find_map(|b| match b {
                Block::Entities { rows, .. } => Some(rows.clone()),
                _ => None,
            })
            .expect("a list")
    };

    let overview = &view.sections[0];
    let Block::Facts { facts } = &overview.blocks[0] else {
        panic!("{overview:#?}");
    };
    for f in facts {
        println!("fact   {:<14} {}", f.label, f.value);
    }
    assert!(
        facts
            .iter()
            .any(|f| f.label == "Default branch" && f.value == "main")
    );
    let Some(Block::Text { text, markdown }) = overview.blocks.get(1) else {
        panic!("auto_init writes a README.md: {overview:#?}");
    };
    println!("readme {text:?} markdown={markdown}");
    assert!(*markdown);
    assert!(
        text.starts_with(&format!("# {}", env.repo)),
        "auto_init's README is headed by the repository's name: {text:?}"
    );

    let pulls = rows_of(1);
    for row in &pulls {
        println!("pull   {:<40} {:?} {:?}", row.title, row.meta, row.chip);
    }
    assert_eq!(view.sections[1].count, Some(pulls.len() as u64));
    for title in &open_titles {
        assert!(
            pulls
                .iter()
                .any(|r| &r.title == title && r.chip.as_deref() == Some("open")),
            "the seeded open pull request {title:?} is listed: {pulls:#?}"
        );
    }

    let branches = rows_of(2);
    for row in &branches {
        println!("branch {:<40} {:?} {:?}", row.title, row.meta, row.chip);
    }
    assert_eq!(view.sections[2].count, Some(branches.len() as u64));
    assert_eq!(branches[0].title, "main", "the default branch first");
    assert_eq!(branches[0].chip.as_deref(), Some("default"));
    for name in &branch_names {
        assert!(
            branches.iter().any(|r| &r.title == name
                && r.entity_id.as_deref() == Some(format!("{id}@refs/heads/{name}").as_str())),
            "the seeded branch {name:?} is listed under its own address: {branches:#?}"
        );
    }

    let commits = rows_of(3);
    for row in &commits {
        println!("commit {:<52} {:?}", row.title, row.meta);
    }
    assert!(!commits.is_empty() && commits.len() <= 20);
}

/// Reading code (#711): the seeded repository's *Files* tab at `main` lists
/// the README `auto_init` wrote and draws it; the SEPA branch's tab walks
/// down its seeded directories, one section id at a time, to a file the seed
/// committed, whose text is that commit's message. A path that is not there
/// is Gitea's 404, not an empty listing.
///
/// Prints every listing and the file, which is the output the PR quotes.
#[tokio::test]
#[ignore = "needs testenv's seeded Gitea container"]
async fn the_seeded_trees_are_walked_and_a_seeded_file_is_drawn() {
    use knobas_source::view::{Block, EntryKind};

    let env = env();
    let source = env.one_repo();
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../../fixtures/tidewater/work.json"))
            .expect("the fixture is JSON");
    let messages: Vec<String> = fixture["commits"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["repo"].as_str() == Some(env.repo.as_str()))
        .filter_map(|c| c["msg"].as_str().map(str::to_owned))
        .collect();

    let read = |entity: String, section: String| {
        let source = &source;
        async move {
            let view = source
                .view(&entity, Some(&section))
                .await
                .unwrap_or_else(|error| panic!("{entity} {section}: {error:?}"))
                .expect("drawn");
            assert_eq!(view.sections.len(), 1);
            assert_eq!(view.sections[0].id, section);
            view.sections[0].blocks[0].clone()
        }
    };

    // The repository, at its default branch.
    let repo = format!("gitea:{}", env.full_name());
    let Block::Tree { trail, entries, .. } = read(repo.clone(), "files".into()).await else {
        panic!("a listing");
    };
    for e in &entries {
        println!(
            "main  {:<10?} {:<24} {:?} {:?}",
            e.kind, e.name, e.size, e.section
        );
    }
    assert_eq!(trail[0].name, "main");
    let readme = entries
        .iter()
        .find(|e| e.name == "README.md")
        .expect("auto_init's README is listed");
    let Block::File {
        text,
        language,
        size,
        truncated,
        binary,
        url,
        trail,
        ..
    } = read(repo.clone(), readme.section.clone().expect("a file opens")).await
    else {
        panic!("a file");
    };
    println!("file  README.md {size} bytes, {language:?}, url {url:?}");
    println!("{text}");
    assert!(text.starts_with(&format!("# {}", env.repo)), "{text:?}");
    assert_eq!(language.as_deref(), Some("markdown"));
    assert_eq!(size, text.len() as u64);
    assert!(!truncated && !binary);
    assert!(url.is_some_and(|u| u.ends_with("/src/branch/main/README.md")));
    assert_eq!(
        trail.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
        ["main", "README.md"]
    );

    // A branch, walked down one directory at a time to a seeded file.
    let branch = "feature/PAY-231-sepa-retry";
    let entity = format!("gitea:{}@refs/heads/{branch}", env.full_name());
    let mut at = "files".to_owned();
    let mut depth = 0;
    let file = loop {
        let Block::Tree { trail, entries, .. } = read(entity.clone(), at.clone()).await else {
            panic!("a listing at {at}");
        };
        let names: Vec<_> = trail.iter().map(|c| c.name.as_str()).collect();
        println!("{}", names.join(" / "));
        for e in &entries {
            println!(
                "      {:<10?} {:<24} {:?} {:?}",
                e.kind, e.name, e.size, e.section
            );
        }
        assert_eq!(trail.len(), depth + 1, "one step per directory");
        assert_eq!(trail[0].name, branch);
        let dirs = entries
            .iter()
            .take_while(|e| e.kind == EntryKind::Dir)
            .count();
        assert!(
            entries[dirs..].iter().all(|e| e.kind != EntryKind::Dir),
            "directories first: {entries:#?}"
        );
        // Down the first directory while there is one, then the first file
        // that is not the README.
        if let Some(dir) = entries.iter().find(|e| e.kind == EntryKind::Dir) {
            at = dir.section.clone().unwrap();
            depth += 1;
            continue;
        }
        break entries
            .iter()
            .find(|e| e.kind == EntryKind::File && e.name != "README.md")
            .unwrap_or_else(|| panic!("a seeded file at {at}: {entries:#?}"))
            .section
            .clone()
            .unwrap();
    };
    assert!(depth >= 1, "the seed commits below the top directory");
    let Block::File { path, text, .. } = read(entity.clone(), file.clone()).await else {
        panic!("a file");
    };
    println!("file  {path}: {text:?}");
    assert!(
        messages.iter().any(|m| text == format!("{m}\n")),
        "{path} holds the message of the commit that wrote it: {text:?}"
    );

    let missing = source
        .view(&entity, Some("files:no/such/dir"))
        .await
        .expect_err("a path that is not there");
    println!("missing  {missing:?}");
    assert_eq!(missing.status(), Some(404));
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

/// A synced commit carries its repo, and `under:` finds it (#595, ADR-0018).
///
/// Gitea's commit and branch records name no repository, so the adapter writes
/// one in; a pull request names it at `base.repo`, which only the real server
/// can say it does. Every child the seeded repository syncs is read here
/// through the descriptor's own declaration and `knobas_core::payload`'s
/// resolver -- the Rust half of the one rule the SQL `under:` predicate is the
/// other half of, held together by `knobas-core`'s agreement test -- and must
/// answer to the full name and the bare name, ignoring case, which is what
/// `under:tidewater/payout-service` and `under:Payout-Service` match on.
///
/// The battery above runs clause 6 against the same server, so a declaration
/// whose `at` or names this server's records do not carry fails there too.
#[tokio::test]
#[ignore = "needs testenv's seeded Gitea container"]
async fn a_synced_commit_carries_its_repo_and_is_found_by_under() {
    use knobas_core::payload::resolve_parents;

    let env = env();
    let (items, _) = full(&*env.one_repo()).await;
    let full_name = env.full_name();
    let bare = full_name
        .rsplit_once('/')
        .map(|(_, name)| name.to_owned())
        .expect("a full name is owner/name");
    let declarations = knobas_source_gitea::descriptor_template().payload_paths;

    let commits = of_kind(&items, "commit");
    assert!(!commits.is_empty(), "the seed pushes commits");
    for commit in &commits {
        assert_eq!(
            commit.payload["repository"],
            // With its default branch since #718: the seed's is `main`.
            serde_json::json!({ "full_name": full_name, "name": bare, "default_branch": "main" }),
            "commit {} carries the repository it sits in",
            commit.entity
        );
    }

    for kind in ["branch", "pr", "commit"] {
        let declared = &declarations
            .iter()
            .find(|paths| paths.kind == kind)
            .unwrap_or_else(|| panic!("{kind} declares its parent"))
            .parent;
        let children = of_kind(&items, kind);
        assert!(!children.is_empty(), "the seed creates a {kind}");
        for child in children {
            let parents = resolve_parents(&child.payload, declared);
            assert_eq!(
                parents.len(),
                1,
                "{} sits in one repo: {parents:?}",
                child.entity
            );
            assert_eq!(parents[0].kind, "repo");
            assert_eq!(
                parents[0].names,
                [full_name.clone(), bare.clone()],
                "{} answers to the full name first",
                child.entity
            );
            for typed in [full_name.to_uppercase(), bare.to_uppercase()] {
                assert!(
                    parents[0]
                        .names
                        .iter()
                        .any(|name| name.to_lowercase() == typed.to_lowercase()),
                    "under:{typed} must find {}",
                    child.entity
                );
            }
            assert!(
                !child.body_text.contains(&full_name),
                "{}'s text no longer names its repo",
                child.entity
            );
        }
    }
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

    // **What the start-work look-before-write reads, against the server that
    // decides it** (#277 for the declared flag, #359 for the rule over it).
    //
    // Since #359, `knobas_app::start_work::queue` calls a pull request open
    // only where the record's own `state` says `open` and the declared merged
    // flag does not say otherwise. Both halves therefore gate the ordinary
    // flow: a record missing either one settles no step and links no ticket.
    // Nothing docker-free can see that -- the wiremock fixture writes
    // `"state"` and `"merged": false` by construction, which is this test's
    // whole premise about a fake being confidently wrong.
    //
    // The contract battery does not cover the flag either, and the direction
    // it misses is exactly the one that matters here: its clause 3 is
    // satisfied as soon as *some* item of the kind resolves the path, and the
    // fixture's one merged pull request does that alone.
    let merged_path = [knobas_source::PayloadPath::of(["merged"])];
    let read: Vec<(&str, Option<bool>, Option<&str>)> = prs
        .iter()
        .map(|i| {
            (
                i.title.as_str(),
                knobas_core::payload::resolve_flag(&i.payload, &merged_path),
                i.payload.get("state").and_then(serde_json::Value::as_str),
            )
        })
        .collect();
    assert!(
        read.iter()
            .all(|(_, merged, state)| merged.is_some() && state.is_some()),
        "every mirrored pull request must carry both facts the look-before-write reads; \
         one missing either settles no step and links no ticket: {read:?}"
    );
    assert_eq!(
        read.iter()
            .find(|(title, ..)| title.contains("Fix ledger drift"))
            .map(|(_, merged, state)| (*merged, *state)),
        Some((Some(true), Some("closed"))),
        "the fixture's merged pull request has to read as merged and closed: {read:?}"
    );
    assert_eq!(
        read.iter()
            .find(|(title, ..)| title.contains("SEPA retry"))
            .map(|(_, merged, state)| (*merged, *state)),
        Some((Some(false), Some("open"))),
        "an open pull request has to read as `open` and `false`, not as absent keys -- \
         the difference between the start-work flow linking it and refusing to: {read:?}"
    );

    // The state chip (#666), through the descriptor's own declaration, on the
    // real server's records: the merged pull request is `merged` though Gitea
    // calls it `closed`, the open one `open`, and every pull request draws one.
    {
        use knobas_core::payload::{StateChip, resolve_state};
        use knobas_core::tone::Tone;
        let declared = knobas_source_gitea::descriptor_template()
            .payload_paths
            .into_iter()
            .find(|paths| paths.kind == "pr")
            .expect("a pull request declares its paths")
            .state;
        let chips: Vec<(&str, Option<StateChip>)> = prs
            .iter()
            .map(|i| (i.title.as_str(), resolve_state(&i.payload, &declared)))
            .collect();
        println!("LIVE pull request state chips: {chips:?}");
        let chip = |title: &str| {
            chips
                .iter()
                .find(|(t, _)| t.contains(title))
                .and_then(|(_, c)| c.clone())
        };
        let drawn = |text: &str, tone: Tone| {
            Some(StateChip {
                text: text.to_owned(),
                tone,
            })
        };
        assert_eq!(
            chip("Fix ledger drift"),
            drawn("merged", Tone::Ok),
            "{chips:?}"
        );
        assert_eq!(chip("SEPA retry"), drawn("open", Tone::Info), "{chips:?}");
        assert!(chips.iter().all(|(_, c)| c.is_some()), "{chips:?}");
    }

    // The head a pull request declares (#712) names a mirrored branch of its
    // own repo by that branch's title: what the Code tile matches to tell a
    // branch nobody has proposed from one an open pull request stands for.
    // The wiremock fixture writes `head.ref` and titles branches by name
    // because the adapter's own author wrote both; only the real server can
    // say the two spellings meet.
    {
        use knobas_core::payload::{parent_path, resolve_parents, resolve_string};
        let declared = knobas_source_gitea::descriptor_template().payload_paths;
        let paths_of = |kind: &str| {
            declared
                .iter()
                .find(|paths| paths.kind == kind)
                .unwrap_or_else(|| panic!("{kind} declares its paths"))
                .clone()
        };
        let (pr_paths, branch_paths) = (paths_of("pr"), paths_of("branch"));
        let sepa_head = resolve_string(&sepa.payload, &pr_paths.head)
            .expect("the SEPA retry pull request declares the branch it is opened from");
        // The repo the head is in (#718): `head.repo.full_name`, which the
        // Code tile compares with a branch's path line. Not seeded from a
        // fork, so here it must name the base repo, spelled the same.
        let sepa_repo = resolve_string(&sepa.payload, &pr_paths.head_repo)
            .expect("the SEPA retry pull request declares the repo it is opened from");
        assert_eq!(
            Some(&sepa_repo),
            parent_path(&resolve_parents(&sepa.payload, &pr_paths.parent)).as_ref(),
            "a same-repo pull request's head repo is spelled as its base repo's path line"
        );
        let branches = of_kind(&items, "branch");
        let proposed: Vec<String> = branches
            .iter()
            .filter(|b| {
                b.title == sepa_head
                    && parent_path(&resolve_parents(&b.payload, &branch_paths.parent)).as_ref()
                        == Some(&sepa_repo)
            })
            .map(|b| b.entity.to_string())
            .collect();
        println!("LIVE SEPA retry head {sepa_head:?} in {sepa_repo:?} -> branches {proposed:?}");
        assert_eq!(
            proposed.len(),
            1,
            "the declared head names exactly one mirrored branch of the pull request's repo"
        );
        assert!(
            branches.iter().all(|b| b.author.is_some()),
            "every seeded branch has a last committer, which is what `mine` compares"
        );

        // Every live branch declares its repo's default (#718), and in each
        // repo exactly one branch is titled so: what keeps a repo's default
        // branch out of the Code tile's *my branches*.
        let mut defaults: std::collections::BTreeMap<String, Vec<String>> =
            std::collections::BTreeMap::new();
        for branch in branches.iter().filter(|b| !b.deleted) {
            let repo = parent_path(&resolve_parents(&branch.payload, &branch_paths.parent))
                .expect("a live branch names its repo");
            let default = resolve_string(&branch.payload, &branch_paths.default_branch)
                .unwrap_or_else(|| panic!("{} declares its repo's default branch", branch.entity));
            let entry = defaults.entry(repo).or_default();
            if branch.title == default {
                entry.push(branch.entity.to_string());
            }
        }
        println!("LIVE default branches per repo: {defaults:?}");
        assert!(
            !defaults.is_empty() && defaults.values().all(|d| d.len() == 1),
            "each repo's declared default names exactly one of its mirrored branches: {defaults:?}"
        );
    }

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
/// *and* inclusive (on Gitea; Forgejo 17 ignores it, #2034, and the walk's own
/// filter holds the bound there), `sha=` really selecting that branch's history, and
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
// The fake in `tests/it/write.rs` encodes a reading of four endpoints -- the field
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

    let opened = source
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
    println!("LIVE create_pull_request: {opened:?}");
    // The receipt names the pull request the server listed, by its key (#1949).
    assert_eq!(
        opened.remote_id,
        Some(format!("{}#{number}", env.full_name())),
        "the receipt names the pull request the adapter opened"
    );
    // The same write sent again, as after a lost answer: Gitea's 409 for the
    // pair is this write's own (#1129), and the receipt names the pull
    // request found open for it, read off the server's real listing.
    let resent = source
        .write(knobas_source::WriteOp::CreatePullRequest {
            entity: repo_id.clone(),
            title: format!("knobas write-back check {stamp}"),
            body: "Opened by the knobas Gitea adapter's write path.".to_owned(),
            head: branch.clone(),
            base: "main".to_owned(),
        })
        .await
        .expect("a re-sent create of an open pull request is delivered");
    println!("LIVE create_pull_request, re-sent: {resent:?}");
    assert_eq!(
        resent.remote_id, opened.remote_id,
        "the re-sent create names the pull request the first one opened"
    );
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
    // `log_work`: Gitea declares `transition` since #1206, and keeps no
    // worklogs.
    let refused = source
        .write(knobas_source::WriteOp::LogWork {
            entity: format!("gitea:{}/issues/1", env.full_name()),
            started: chrono::Utc::now(),
            seconds: 60,
            comment: String::new(),
        })
        .await;
    assert!(
        matches!(refused, Err(SourceError::Protocol { message: ref m, .. }) if m.contains("log_work")),
        "{refused:?}"
    );
}

/// The three repositories Gitea answers `/pulls` with a bare 404 for -- empty,
/// pull requests switched off, a pull mirror (`CanEnablePulls` is `!IsMirror &&
/// !IsEmpty`) -- used to end a **first** sync outright: a cursor-less run reads
/// a repository-scoped 404 as a hole. The empty one also answers its branch
/// listing with `200 null`, which did not decode at all. This creates the first
/// two under the token's own account, syncs them with no cursor, and removes
/// them again before asserting. A mirror needs a migration from a remote and is
/// held by the wiremock fixture and Gitea's own `CanEnablePulls` alone.
///
/// Under the account, not the seeded organisation, so `Env::whole_owner` and
/// every owner-scoped assertion beside this one never see them.
#[tokio::test]
#[ignore = "needs testenv's seeded Gitea container"]
async fn repositories_gitea_refuses_pull_requests_for_do_not_end_a_full_sync() {
    let env = env();
    let http = reqwest::Client::builder()
        .timeout(live_env::REQUEST_BUDGET)
        .build()
        .expect("a client with a timeout");
    let auth = format!("token {}", env.token);
    let api = format!("{}/api/v1", env.url);
    let me: serde_json::Value = http
        .get(format!("{api}/user"))
        .header("Authorization", &auth)
        .send()
        .await
        .expect("GET /user")
        .json()
        .await
        .expect("a user record");
    let me = me["login"].as_str().expect("a login").to_owned();
    let empty = "knobas-live-empty-repo";
    let no_pulls = "knobas-live-no-pull-requests";
    let remove = async |name: &str| {
        let _ = http
            .delete(format!("{api}/repos/{me}/{name}"))
            .header("Authorization", &auth)
            .send()
            .await;
    };
    // An earlier run that was killed rather than failed left these behind.
    remove(empty).await;
    remove(no_pulls).await;

    for (name, init) in [(empty, false), (no_pulls, true)] {
        let status = http
            .post(format!("{api}/user/repos"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "name": name, "auto_init": init }))
            .send()
            .await
            .expect("POST /user/repos")
            .status();
        assert_eq!(status.as_u16(), 201, "create {name}");
    }
    let status = http
        .patch(format!("{api}/repos/{me}/{no_pulls}"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "has_pull_requests": false }))
        .send()
        .await
        .expect("PATCH the repository")
        .status();

    // The premise, measured rather than remembered: both really are refused.
    let mut refused = Vec::new();
    for name in [empty, no_pulls] {
        let pulls = http
            .get(format!(
                "{api}/repos/{me}/{name}/pulls?state=all&limit=50&page=1"
            ))
            .header("Authorization", &auth)
            .send()
            .await
            .expect("GET pulls")
            .status()
            .as_u16();
        refused.push((name, pulls));
    }

    let source = env.source(serde_json::json!({
        "repos": [format!("{me}/{empty}"), format!("{me}/{no_pulls}")]
    }));
    let mut sink = VecSink(Vec::new());
    let outcome = source.sync(None, &mut sink).await;

    remove(empty).await;
    remove(no_pulls).await;

    assert_eq!(status.as_u16(), 200, "switch pull requests off");
    assert_eq!(
        refused,
        vec![(empty, 404), (no_pulls, 404)],
        "the premise: Gitea refuses /pulls on both"
    );
    assert!(outcome.is_ok(), "{outcome:?}");
    let repos: Vec<String> = of_kind(&sink.0, "repo")
        .iter()
        .map(|item| item.entity.to_string())
        .collect();
    assert_eq!(repos.len(), 2, "both repositories are mirrored: {repos:?}");
    assert!(
        of_kind(&sink.0, "branch")
            .iter()
            .any(|item| item.entity.to_string() == format!("gitea:{me}/{no_pulls}@refs/heads/main")),
        "the rest of the repository is still walked"
    );
}

/// Story 10 (spec #660, issue #667), against the real server: a pull request
/// Gitea can merge offers *Merge…* in its view, the adapter's
/// `merge_pull_request` -- the call the write queue's flush makes -- merges it,
/// and the next incremental sync reads it merged. The view then offers
/// nothing.
///
/// # What it spends, and who takes it back
///
/// A merge is the one write here whose residue `DELETE` cannot reach on the
/// default branch: a merge commit on `main` stays until a force-push or
/// `testenv/reset`, and `main` is a branch `live_gitea_capped.rs` budgets
/// (`start_work_live.rs` paid for that until #373). So this merges **into a
/// scratch base branch of its own**, never `main`: both branches carry the
/// `knobas-` prefix and [`Litter`] deletes the pull request (merged ones too,
/// `DELETE issues/{n}`) and both branches whether the test passes or panics.
/// The fixture needs no restore point, and the test asserts `main` did not
/// move. No seeded pull request is consumed: each run opens a fresh one.
///
/// # Why the view is polled
///
/// Gitea computes `mergeable` in the background and answers `false` (and a
/// merge *"Please try again later"*) until it has. The adapter offers the
/// action only on `true`, so the test waits for the offer rather than for a
/// clock -- which is also the assertion that the offer appears. A `true` is
/// not the end of it: the check of the head's push can still start after it
/// and refuse the merge for a second or two (#721, which this test failed on
/// twice), and the adapter's merge waits that out itself
/// ([`a_pull_request_merged_the_moment_it_opens_lands_ten_times_in_ten`]).
#[tokio::test]
#[ignore = "needs testenv's seeded Gitea container"]
async fn a_mergeable_pull_request_is_merged_from_its_view_and_the_next_sync_reads_it_merged() {
    use knobas_source::view::Block;

    let env = env();
    let mut litter = Litter::new(&env).await;
    let source = env.one_repo();
    let http = reqwest::Client::new();
    let auth = format!("token {}", env.token);
    let api = format!("{}/api/v1/repos/{}", env.url, env.full_name());
    let main_head = || async {
        let branch: serde_json::Value = http
            .get(format!("{api}/branches/main"))
            .header("Authorization", &auth)
            .send()
            .await
            .expect("read main")
            .json()
            .await
            .expect("main is a branch record");
        branch["commit"]["id"]
            .as_str()
            .expect("main has a head commit")
            .to_owned()
    };
    let main_before = main_head().await;

    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let base = format!("knobas-merge-{stamp}-base");
    let head = format!("knobas-merge-{stamp}");
    litter.branch_off_main(&base).await;
    litter.branch_off_main(&head).await;
    // A pull request with no diff is not one Gitea will merge.
    push_file(
        &http,
        &env,
        &head,
        &format!("{head}.txt"),
        &format!("{head}: the work"),
    )
    .await;

    let opened = http
        .post(format!("{api}/pulls"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "head": head, "base": base,
            "title": format!("knobas merge check {stamp}"),
            "body": "Opened by the knobas Gitea adapter's live suite, to be merged from its view."
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
    let id = format!("gitea:{}#{number}", env.full_name());
    let (_, cursor) = full(&*source).await;

    // 1. The view's toolbar offers *Merge…*, enabled once Gitea has computed
    //    that it can merge (#702: until then it is drawn disabled, saying why).
    let merge_of = |view: &knobas_source::view::SourceView| {
        assert!(
            !view
                .sections
                .iter()
                .flat_map(|s| &s.blocks)
                .any(|b| matches!(b, Block::Actions { .. })),
            "the merge is the toolbar's, never a section's too: {view:#?}"
        );
        view.actions
            .iter()
            .find(|a| a.write_op == "merge_pull_request")
            .cloned()
    };
    let mut offered = None;
    let mut seen = None;
    for _ in 0..40 {
        let view = source
            .view(&id, None)
            .await
            .expect("the view reads")
            .expect("Gitea draws a pull request");
        offered = merge_of(&view);
        seen = Some(view);
        if offered.as_ref().is_some_and(|a| a.disabled.is_none()) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    let offered = offered.expect("the pull request's toolbar has no Merge…");
    // *Ready to merge?*'s facts (#994): the fresh base is unprotected, so no
    // approval is required, and Gitea says it can merge.
    let Some(Block::Facts { facts }) = seen.as_ref().and_then(|v| v.sections[0].blocks.first())
    else {
        panic!("the overview opens on facts: {seen:#?}");
    };
    let read = |label: &str| {
        facts
            .iter()
            .find(|f| f.label == label)
            .map(|f| (f.value.clone(), f.tone))
    };
    let ok = Some(knobas_source::view::Tone::Ok);
    assert_eq!(
        read("Approvals"),
        Some(("none required".to_owned(), ok)),
        "{facts:?}"
    );
    assert_eq!(
        read("Conflicts"),
        Some((format!("none with {base}"), ok)),
        "{facts:?}"
    );
    println!("LIVE Merge… action: {offered:?}");
    assert_eq!(
        offered.disabled, None,
        "Gitea never said the fresh pull request was mergeable"
    );
    assert!(offered.primary, "{offered:?}");
    let [field, delete, message] = offered.fields.as_slice() else {
        panic!("three fields, the style, the delete flag and the message: {offered:?}")
    };
    // A squash's message (#843), prefilled as Gitea's own squash title.
    assert_eq!(message.name, "message");
    assert_eq!(
        message.value.as_deref(),
        Some(format!("knobas merge check {stamp} (#{number})").as_str()),
        "{message:?}"
    );
    // The head is this repository's and unprotected, so Gitea could delete
    // it: the flag is drawn ticked (#708). This test merges with it unticked.
    assert_eq!(delete.name, "delete_branch");
    assert_eq!(delete.value.as_deref(), Some("true"), "{delete:?}");
    let knobas_source::view::FieldKind::Choice { options } = &field.kind else {
        panic!("the style is a choice: {field:?}")
    };
    assert_eq!(field.name, "style");
    assert_eq!(
        options.iter().map(|o| o.value.as_str()).collect::<Vec<_>>(),
        ["merge", "rebase", "squash"]
    );
    assert_eq!(
        offered.confirm.as_deref(),
        Some(format!("Merge {head} into {base}?").as_str())
    );

    // 2. The merge, through the op the queue flushes: a squash with the
    //    reader's own message (#843), which is the base's new head commit's.
    let title = format!("knobas squash {stamp} (#{number})");
    source
        .write(knobas_source::WriteOp::MergePullRequest {
            entity: id.clone(),
            style: knobas_source::MergeStyle::Squash,
            delete_branch: false,
            message: Some(format!("{title}\n\nSquashed by the live suite.")),
        })
        .await
        .expect("Gitea merges a pull request it said it could");
    let landed: serde_json::Value = http
        .get(format!("{api}/branches/{base}"))
        .header("Authorization", &auth)
        .send()
        .await
        .expect("read the base back")
        .json()
        .await
        .expect("a branch record");
    let said = landed["commit"]["message"].as_str().unwrap_or_default();
    println!("LIVE squash commit message: {said:?}");
    assert!(
        said.starts_with(&format!("{title}\n\nSquashed by the live suite.")),
        "the squash commit does not carry the reader's message: {said:?}"
    );
    let record: serde_json::Value = http
        .get(format!("{api}/pulls/{number}"))
        .header("Authorization", &auth)
        .send()
        .await
        .expect("read the pull request back")
        .json()
        .await
        .expect("a pull request record");
    assert_eq!(
        record["merged"], true,
        "the server says it is not merged: {record}"
    );

    // 3. The next incremental sync reads it merged.
    let mut sink = VecSink(Vec::new());
    source
        .sync(Some(cursor), &mut sink)
        .await
        .expect("incremental sync");
    let pr = sink
        .0
        .iter()
        .find(|i| i.entity.to_string() == id)
        .unwrap_or_else(|| panic!("the merged pull request did not come back with the next sync"));
    assert_eq!(pr.payload["merged"], true, "{}", pr.payload);

    // 4. And the view stops offering what has happened: a merged pull
    //    request can never be merged again, so *Merge…* is gone from its
    //    toolbar (the #701 ruling: disabled only where it can still apply).
    let view = source.view(&id, None).await.unwrap().unwrap();
    assert!(
        merge_of(&view).is_none(),
        "a merged pull request offers no Merge…: {:#?}",
        view.actions
    );
    let Block::Facts { facts } = &view.sections[0].blocks[0] else {
        panic!("the overview opens on facts: {view:#?}");
    };
    assert!(
        facts
            .iter()
            .any(|f| f.label == "State" && f.value == "merged"),
        "{facts:?}"
    );
    // Nor *Close* or *Reopen* (#706): Gitea refuses a merged pull request's
    // state, and says so when asked anyway; its title it still takes.
    let edits: Vec<&str> = view
        .actions
        .iter()
        .filter(|a| a.write_op == "edit_pull_request")
        .map(|a| a.label.as_str())
        .collect();
    assert_eq!(edits, ["Edit…"], "{:#?}", view.actions);
    let refused = source
        .write(knobas_source::WriteOp::EditPullRequest {
            entity: id.clone(),
            title: None,
            body: None,
            state: Some(knobas_source::PullRequestState::Open),
        })
        .await
        .expect_err("Gitea reopened a merged pull request");
    assert_eq!(refused.status(), Some(412), "{refused:?}");
    assert!(
        matches!(&refused, SourceError::Protocol { message: m, .. }
            if m.contains("it was already merged")),
        "{refused:?}"
    );

    assert_eq!(
        main_head().await,
        main_before,
        "the merge reached the default branch, which no cleanup takes back"
    );
    println!("merged #{number} ({head} into {base}) from its view; the next sync read it merged");
}

/// Issue #721, against the real server: a pull request merged **the moment
/// it is opened** -- no view read, no wait -- lands, ten times in ten. Gitea
/// answers such a merge 405 *"Please try again later"* while it is still
/// checking the pull request (measured before the fix: 3 of 5), and the
/// adapter's merge waits that out with backoff rather than failing the write.
///
/// Spends what [`a_mergeable_pull_request_is_merged_from_its_view_and_the_next_sync_reads_it_merged`]
/// spends, ten times: each round merges into a scratch base of its own, and
/// [`Litter`] removes every branch and pull request whether the test passes
/// or panics. `main` does not move.
#[tokio::test]
#[ignore = "needs testenv's seeded Gitea container"]
async fn a_pull_request_merged_the_moment_it_opens_lands_ten_times_in_ten() {
    let env = env();
    let mut litter = Litter::new(&env).await;
    let source = env.one_repo();
    let http = reqwest::Client::new();
    let auth = format!("token {}", env.token);
    let api = format!("{}/api/v1/repos/{}", env.url, env.full_name());
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    for round in 1..=10 {
        let base = format!("knobas-merge-now-{stamp}-{round}-base");
        let head = format!("knobas-merge-now-{stamp}-{round}");
        litter.branch_off_main(&base).await;
        litter.branch_off_main(&head).await;
        push_file(
            &http,
            &env,
            &head,
            &format!("{head}.txt"),
            &format!("{head}: the work"),
        )
        .await;
        let opened = http
            .post(format!("{api}/pulls"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "head": head, "base": base,
                "title": format!("knobas merge-at-once check {stamp} #{round}"),
                "body": "Opened by the knobas Gitea adapter's live suite, merged at once (#721)."
            }))
            .send()
            .await
            .expect("open the pull request");
        assert!(opened.status().is_success(), "round {round}: open");
        let number = opened.json::<serde_json::Value>().await.unwrap()["number"]
            .as_u64()
            .expect("Gitea answers with the new pull request's number");

        let started = std::time::Instant::now();
        source
            .write(knobas_source::WriteOp::MergePullRequest {
                entity: format!("gitea:{}#{number}", env.full_name()),
                style: knobas_source::MergeStyle::Merge,
                delete_branch: false,
                message: None,
            })
            .await
            .unwrap_or_else(|e| panic!("round {round}: the merge did not land: {e:?}"));
        let took = started.elapsed();
        let record: serde_json::Value = http
            .get(format!("{api}/pulls/{number}"))
            .header("Authorization", &auth)
            .send()
            .await
            .expect("read the pull request back")
            .json()
            .await
            .expect("a pull request record");
        assert_eq!(record["merged"], true, "round {round}: {record}");
        println!("LIVE round {round}/10: #{number} merged at once, the write took {took:?}");
    }
}

/// Issue #721's scope addition, against the real server: a base whose
/// protection requires a status check keeps *Merge…* disabled, saying what
/// the check is doing, until the head commit reports it passed -- and Gitea
/// agrees, refusing the merge until then with a 405 that the adapter does
/// **not** wait out (it is not *"Please try again later"*).
///
/// The protection rule is named for the scratch base branch, so [`Litter`]
/// removes it with the branch (a protected branch cannot be deleted).
#[tokio::test]
#[ignore = "needs testenv's seeded Gitea container"]
async fn a_required_status_check_holds_the_merge_until_it_passes() {
    use knobas_source::view::SourceView;

    const CHECK: &str = "knobas/live-check";
    let env = env();
    let mut litter = Litter::new(&env).await;
    let source = env.one_repo();
    let http = reqwest::Client::new();
    let auth = format!("token {}", env.token);
    let api = format!("{}/api/v1/repos/{}", env.url, env.full_name());
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let base = format!("knobas-checks-{stamp}-base");
    let head = format!("knobas-checks-{stamp}");
    litter.branch_off_main(&base).await;
    litter.branch_off_main(&head).await;
    let sha = push_file(
        &http,
        &env,
        &head,
        &format!("{head}.txt"),
        &format!("{head}: the work"),
    )
    .await;
    let protected = http
        .post(format!("{api}/branch_protections"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "rule_name": base,
            "enable_status_check": true,
            "status_check_contexts": [CHECK],
        }))
        .send()
        .await
        .expect("protect the base");
    assert!(
        protected.status().is_success(),
        "protection: {}",
        protected.text().await.unwrap_or_default()
    );
    let opened = http
        .post(format!("{api}/pulls"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "head": head, "base": base,
            "title": format!("knobas required-check check {stamp}"),
            "body": "Opened by the knobas Gitea adapter's live suite (#721)."
        }))
        .send()
        .await
        .expect("open the pull request");
    assert!(opened.status().is_success(), "open");
    let number = opened.json::<serde_json::Value>().await.unwrap()["number"]
        .as_u64()
        .expect("Gitea answers with the new pull request's number");
    let id = format!("gitea:{}#{number}", env.full_name());

    let merge_reason = |view: &SourceView| {
        view.actions
            .iter()
            .find(|a| a.write_op == "merge_pull_request")
            .expect("an open pull request offers Merge…")
            .disabled
            .clone()
    };
    // What *Merge…* says once Gitea's own mergeability check has settled.
    let settled = |want: Option<String>| {
        let source = &source;
        let id = &id;
        async move {
            let mut said = None;
            for _ in 0..40 {
                let view = source.view(id, None).await.unwrap().unwrap();
                said = merge_reason(&view);
                if said == want {
                    return said;
                }
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            }
            said
        }
    };
    let report = |state: &str| {
        let (http, auth, api, sha) = (&http, &auth, &api, &sha);
        let state = state.to_owned();
        async move {
            let posted = http
                .post(format!("{api}/statuses/{sha}"))
                .header("Authorization", auth)
                .json(&serde_json::json!({ "state": state, "context": CHECK }))
                .send()
                .await
                .expect("report a status");
            assert!(posted.status().is_success(), "status {state}");
        }
    };

    let waiting = Some(format!("Required check {CHECK} has not reported"));
    assert_eq!(settled(waiting.clone()).await, waiting);
    println!("LIVE no status: Merge… says {waiting:?}");

    // Gitea agrees, and its 405 is a refusal at once, not a wait.
    let started = std::time::Instant::now();
    let refused = source
        .write(knobas_source::WriteOp::MergePullRequest {
            entity: id.clone(),
            style: knobas_source::MergeStyle::Merge,
            delete_branch: false,
            message: None,
        })
        .await
        .expect_err("Gitea merged past a required check that never reported");
    assert_eq!(refused.status(), Some(405), "{refused:?}");
    // The refusal is the check's, not *"Please try again later"*: the adapter
    // believed it rather than giving up on a wait.
    assert!(
        refused
            .to_string()
            .contains("Not all required status checks successful"),
        "{refused:?}"
    );
    // And it was not waited out. Waiting it out costs the adapter's whole
    // merge patience (15 s, `write::MERGE_PATIENCE`); a refusal believed at
    // once can still take a few seconds, because Gitea answers *"try again
    // later"* while it checks the push behind the pull request (1--2.5 s
    // measured in #721), which is waited out first. 3 s was the bound until
    // #766's review, and a run took 3.9 s.
    assert!(
        started.elapsed() < std::time::Duration::from_secs(12),
        "a refusal that is not 'try again later' is not waited out: {:?}",
        started.elapsed()
    );
    println!("LIVE Gitea refused the merge: {refused:?}");

    report("pending").await;
    let pending = Some(format!("Required check {CHECK} is pending"));
    assert_eq!(settled(pending.clone()).await, pending);
    report("failure").await;
    let failed = Some(format!("Required check {CHECK} failed"));
    assert_eq!(settled(failed.clone()).await, failed);
    report("success").await;
    assert_eq!(
        settled(None).await,
        None,
        "a passed check leaves Merge… enabled"
    );
    // The check's row carries its time (#994): reported a moment ago, with
    // no description, so its meta is the age alone.
    let view = source.view(&id, None).await.unwrap().unwrap();
    let row = view.sections[0]
        .blocks
        .iter()
        .find_map(|b| match b {
            knobas_source::view::Block::Entities { rows, .. } => {
                rows.iter().find(|r| r.title == CHECK).cloned()
            }
            _ => None,
        })
        .expect("the overview's checks list the required check");
    println!("LIVE check row: {row:?}");
    let meta = row.meta.unwrap_or_default();
    assert!(
        is_age(&meta),
        "the check's row says when it was reported: {meta:?}"
    );

    source
        .write(knobas_source::WriteOp::MergePullRequest {
            entity: id.clone(),
            style: knobas_source::MergeStyle::Merge,
            delete_branch: false,
            message: None,
        })
        .await
        .expect("Gitea merges once the required check passed");
    println!("LIVE #{number}: held by {CHECK} through pending and failure, merged on success");
}

// -- landing: update a branch from its base, merge and delete it (#708) -----

/// Workflow 9 of spec #701 (issue #708), against the real server: a pull
/// request whose base has moved on offers *Update branch…*; the adapter's
/// `update_pull_request_branch` brings the head level, after which the view
/// no longer offers it; and `merge_pull_request` with `delete_branch` merges
/// it and Gitea deletes the head.
///
/// # What it spends, and who takes it back
///
/// Everything is its own: a scratch base branch and a head branch, both
/// `knobas-`-prefixed, a pull request between them, and one commit on each
/// made through the contents API. Nothing touches `main` or a seeded pull
/// request; [`Litter`] deletes the pull request and both branches (the head
/// is already gone by then, which its check reads as gone) whether the test
/// passes or panics.
#[tokio::test]
#[ignore = "needs testenv's seeded Gitea container"]
async fn a_pull_request_behind_its_base_is_updated_then_merged_deleting_its_branch() {
    use knobas_source::view::{FieldKind, SourceView, ViewAction};

    let env = env();
    let mut litter = Litter::new(&env).await;
    let source = env.one_repo();
    let http = reqwest::Client::new();
    let auth = format!("token {}", env.token);
    let api = format!("{}/api/v1/repos/{}", env.url, env.full_name());
    let get = |url: String| {
        let (http, auth) = (http.clone(), auth.clone());
        async move {
            http.get(url)
                .header("Authorization", auth)
                .send()
                .await
                .expect("a read")
        }
    };

    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let base = format!("knobas-land-{stamp}-base");
    let head = format!("knobas-land-{stamp}");
    litter.branch_off_main(&base).await;
    litter.branch_off_main(&head).await;
    push_file(
        &http,
        &env,
        &head,
        &format!("{head}.txt"),
        &format!("{head}: the work"),
    )
    .await;
    let opened = http
        .post(format!("{api}/pulls"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "head": head, "base": base,
            "title": format!("knobas landing check {stamp}"),
            "body": "Opened by the knobas Gitea adapter's live suite, to be updated and merged."
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
    // Merged with its head deleted, Gitea lists it as `refs/pull/{n}/head`, so
    // only its number finds it again (#738).
    litter.will_open_pull(number);
    let id = format!("gitea:{}#{number}", env.full_name());

    // 1. The base moves on: a commit on it, through the contents API.
    let moved_on = push_file(
        &http,
        &env,
        &base,
        &format!("{base}.txt"),
        &format!("{base}: the base moves on"),
    )
    .await;

    let find = |view: &SourceView, op: &str| -> Option<ViewAction> {
        view.actions.iter().find(|a| a.write_op == op).cloned()
    };
    let view = source.view(&id, None).await.unwrap().unwrap();
    let update = find(&view, "update_pull_request_branch")
        .unwrap_or_else(|| panic!("a head behind its base offers Update branch…: {view:#?}"));
    println!("LIVE Update branch… action: {update:?}");
    assert_eq!(update.disabled, None, "{update:?}");
    let FieldKind::Choice { options } = &update.fields[0].kind else {
        panic!("the style is a choice: {update:?}")
    };
    assert_eq!(
        options.iter().map(|o| o.value.as_str()).collect::<Vec<_>>(),
        ["merge", "rebase"]
    );
    let labels: Vec<&str> = view.actions.iter().map(|a| a.label.as_str()).collect();
    let at = |label: &str| labels.iter().position(|l| *l == label);
    assert!(
        at("Update branch…") < at("Merge…") && at("Close") < at("Update branch…"),
        "maintenance, then Update branch…, then Merge… last: {labels:?}"
    );

    // 2. The update, through the op the queue flushes, brings the head level:
    //    the base's commit is in the head, and the view no longer offers it.
    source
        .write(knobas_source::WriteOp::UpdatePullRequestBranch {
            entity: id.clone(),
            style: knobas_source::UpdateStyle::Merge,
        })
        .await
        .expect("Gitea updates a head that is behind and has no conflict");
    let carried = get(format!(
        "{api}/commits?sha={head}&limit=50&stat=false&files=false&verification=false"
    ))
    .await
    .json::<Vec<serde_json::Value>>()
    .await
    .expect("the head's commits");
    assert!(
        carried.iter().any(|c| c["sha"] == moved_on.as_str()),
        "the base's commit {moved_on} is not in the head after the update"
    );
    let view = source.view(&id, None).await.unwrap().unwrap();
    assert!(
        find(&view, "update_pull_request_branch").is_none(),
        "a level head offers no Update branch…: {:#?}",
        view.actions
    );

    // 3. Merge… with *Delete branch after merge* drawn ticked; it is enabled
    //    once Gitea has re-checked the moved head (#721 is its refusal while
    //    it has not).
    let mut merge = None;
    for _ in 0..40 {
        let view = source.view(&id, None).await.unwrap().unwrap();
        merge = find(&view, "merge_pull_request");
        if merge.as_ref().is_some_and(|a| a.disabled.is_none()) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    let merge = merge.expect("an open pull request offers Merge…");
    println!("LIVE Merge… action: {merge:?}");
    assert_eq!(
        merge.disabled, None,
        "Gitea never said the updated head was mergeable"
    );
    let delete = merge
        .fields
        .iter()
        .find(|f| f.name == "delete_branch")
        .unwrap_or_else(|| panic!("Merge… asks whether to delete the branch: {merge:?}"));
    assert_eq!(delete.value.as_deref(), Some("true"), "{delete:?}");

    source
        .write(knobas_source::WriteOp::MergePullRequest {
            entity: id.clone(),
            style: knobas_source::MergeStyle::Merge,
            delete_branch: true,
            message: None,
        })
        .await
        .expect("Gitea merges the updated pull request");
    let record: serde_json::Value = get(format!("{api}/pulls/{number}"))
        .await
        .json()
        .await
        .expect("a pull request record");
    assert_eq!(record["merged"], true, "not merged: {record}");
    let gone = get(format!("{api}/branches/{head}")).await.status();
    assert_eq!(
        gone,
        reqwest::StatusCode::NOT_FOUND,
        "the head branch is still there"
    );
    assert_eq!(
        get(format!("{api}/branches/{base}")).await.status(),
        reqwest::StatusCode::OK,
        "the merge took the base with it"
    );
    println!("updated #{number} from {base}, merged it and Gitea deleted {head}");
}

// -- comment and review from the pull request's view (issue #705) ------------

/// What a view's *Conversation* and overview say, as lines: each facts block
/// as `label=value` pairs, each text as its text.
fn said(view: &knobas_source::view::SourceView, section: &str) -> Vec<String> {
    use knobas_source::view::Block;
    let section = view
        .sections
        .iter()
        .find(|s| s.id == section)
        .unwrap_or_else(|| panic!("no {section} in {view:#?}"));
    section
        .blocks
        .iter()
        .flat_map(|b| match b {
            Block::Facts { facts } => facts
                .iter()
                .map(|f| format!("{}={}", f.label, f.value))
                .collect(),
            Block::Text { text, .. } => vec![text.clone()],
            _ => vec![],
        })
        .collect()
}

/// The toolbar action labelled `label`.
fn toolbar(view: &knobas_source::view::SourceView, label: &str) -> knobas_source::view::ViewAction {
    view.actions
        .iter()
        .find(|a| a.label == label)
        .cloned()
        .unwrap_or_else(|| panic!("no {label} in the toolbar: {:#?}", view.actions))
}

/// Workflows 6 and 7 of spec #701, against the real server: on a pull request
/// somebody else opened, the toolbar offers *Comment…*, *Approve…* and
/// *Request changes…*; the adapter's `comment` and `submit_review` -- the
/// calls the write queue's flush makes -- comment, request changes and
/// approve; and the view, read again, shows each in *Conversation* and the
/// standing verdict on the overview. On the token's own pull request the two
/// verdicts are not offered, and Gitea refuses them with its sentence.
///
/// Never a seeded pull request: both are opened here on `knobas-` branches of
/// their own, and [`Litter`] deletes them, their reviews and the branches
/// whether this passes or panics.
#[tokio::test]
#[ignore = "needs testenv's seeded Gitea container"]
async fn a_pull_request_is_commented_on_and_reviewed_from_its_view() {
    use knobas_source::{ReviewVerdict, WriteOp};

    let env = env();
    let mut litter = Litter::new(&env).await;
    let source = env.one_repo();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let branch = format!("knobas-review-{stamp}");
    litter.branch_off_main(&branch).await;
    let theirs = litter
        .open_pull_as(
            "jonas.becker",
            &branch,
            &format!("knobas review check {stamp}"),
        )
        .await;
    let id = format!("gitea:{}#{theirs}", env.full_name());
    let view = || async {
        source
            .view(&id, None)
            .await
            .expect("the view reads")
            .expect("Gitea draws a pull request")
    };

    // 1. Somebody else's open pull request: all three offered, enabled.
    let before = view().await;
    for label in ["Comment…", "Approve…", "Request changes…"] {
        let action = toolbar(&before, label);
        assert_eq!(action.disabled, None, "{action:?}");
    }
    assert_eq!(
        toolbar(&before, "Approve…")
            .values
            .get("verdict")
            .map(String::as_str),
        Some("approve")
    );

    // 2. A comment, then a request for changes, then an approval -- each read
    //    back from the view.
    let note = format!("a reply from the knobas toolbar, {stamp}");
    source
        .write(WriteOp::Comment {
            entity: id.clone(),
            body: note.clone(),
        })
        .await
        .expect("the adapter comments");
    let conversation = said(&view().await, "conversation");
    assert!(conversation.contains(&note), "{conversation:#?}");

    // Gitea keeps these times to the second, and *Conversation* orders by
    // them: two things said within one second are a tie, which keeps Gitea's
    // order of the two lists (reviews first) rather than the order they were
    // written in. A reader does not write twice a second; this test would.
    let next_second = || tokio::time::sleep(std::time::Duration::from_millis(1100));
    next_second().await;
    let words = format!("bound the jitter, {stamp}");
    source
        .write(WriteOp::SubmitReview {
            entity: id.clone(),
            verdict: ReviewVerdict::RequestChanges,
            body: words.clone(),
            comments: vec![],
        })
        .await
        .expect("Gitea takes a request for changes from somebody else");
    let after = view().await;
    let conversation = said(&after, "conversation");
    let at = conversation
        .iter()
        .position(|l| l == &words)
        .unwrap_or_else(|| panic!("the review's words are not in Conversation: {conversation:#?}"));
    assert_eq!(
        conversation[at - 1],
        "Review=changes requested",
        "{conversation:#?}"
    );
    assert!(
        said(&after, "overview").contains(&format!(
            "Review=changes requested by {}",
            env.login().await
        )),
        "{:#?}",
        said(&after, "overview")
    );

    next_second().await;
    source
        .write(WriteOp::SubmitReview {
            entity: id.clone(),
            verdict: ReviewVerdict::Approve,
            body: String::new(),
            comments: vec![],
        })
        .await
        .expect("Gitea takes an approval with no words from somebody else");
    let after = view().await;
    let conversation = said(&after, "conversation");
    assert_eq!(
        conversation.last().map(String::as_str),
        Some("Review=approved"),
        "the approval is the newest thing said: {conversation:#?}"
    );
    assert!(
        said(&after, "overview").contains(&format!("Review=approved by {}", env.login().await)),
        "the latest verdict is the standing one: {:#?}",
        said(&after, "overview")
    );
    // Gitea marks a reviewer's earlier verdict dismissed when they give a new
    // one, and the view says what the server says.
    assert!(
        conversation.contains(&"Review=changes requested, dismissed".to_owned()),
        "{conversation:#?}"
    );
    let recorded = litter.reviews(theirs).await;
    assert_eq!(
        recorded.iter().map(|(s, _)| s.as_str()).collect::<Vec<_>>(),
        ["REQUEST_CHANGES", "APPROVED"],
        "{recorded:?}"
    );

    // 3. The token's own pull request: the verdicts are not offered (the
    //    #701 ruling), and Gitea refuses them with its own sentence when sent
    //    anyway.
    let own_branch = format!("knobas-review-{stamp}-own");
    litter.branch_off_main(&own_branch).await;
    let me = env.login().await;
    let own = litter
        .open_pull_as(
            &me,
            &own_branch,
            &format!("knobas own review check {stamp}"),
        )
        .await;
    let own_id = format!("gitea:{}#{own}", env.full_name());
    let own_view = source.view(&own_id, None).await.unwrap().unwrap();
    assert_eq!(toolbar(&own_view, "Comment…").disabled, None);
    let labels: Vec<&str> = own_view.actions.iter().map(|a| a.label.as_str()).collect();
    // *Submit review…* stays (#709): Gitea takes a review that only comments
    // from the author, and the shell draws it only with comments pending.
    assert_eq!(
        labels,
        [
            "Submit review…",
            "Comment…",
            "Edit…",
            "Close",
            "Add label…",
            "Merge…"
        ],
        "{:#?}",
        own_view.actions
    );
    source
        .write(WriteOp::SubmitReview {
            entity: own_id.clone(),
            verdict: ReviewVerdict::Comment,
            body: format!("the author's own note, {stamp}"),
            comments: vec![],
        })
        .await
        .expect("Gitea takes a comment-only review from the pull request's author");
    for (label, verdict, sentence) in [
        (
            "Approve…",
            ReviewVerdict::Approve,
            "approve your own pull is not allowed",
        ),
        (
            "Request changes…",
            ReviewVerdict::RequestChanges,
            "reject your own pull is not allowed",
        ),
    ] {
        let refused = source
            .write(WriteOp::SubmitReview {
                entity: own_id.clone(),
                verdict,
                body: "my own".to_owned(),
                comments: vec![],
            })
            .await;
        let Err(error) = &refused else {
            panic!("Gitea took {label} on the author's own pull request");
        };
        assert_eq!(error.status(), Some(422), "{error:?}");
        assert!(
            matches!(error, SourceError::Protocol { message: m, .. } if m.contains(sentence)),
            "{error:?}"
        );
    }
    println!("commented, requested changes and approved #{theirs} from its view; #{own} refused");
}

/// A branch's life from its views (#707, spec #701 workflows 2, 4 and 10),
/// against the real server: the repository's *New branch…* makes a branch
/// from the default branch; with a commit on it, the branch's *Open pull
/// request…* is offered prefilled and opens one; the branch's overview then
/// opens on that pull request and offers no second one; with the pull
/// request gone, *Delete branch…* says what is lost and the delete lands;
/// the next sync tombstones the branch. The default branch is refused.
///
/// Only this test's own branch and pull request are touched, and nothing
/// seeded is written to. [`Litter`] removes both on a panic up to the delete;
/// the pull request is removed by the test itself before the branch goes,
/// because a pull request whose head branch was deleted is listed with the
/// head `refs/pull/{n}/head`, which no longer names the branch `Litter` finds
/// it by (measured on 1.27.3: the first run left #412 standing).
#[tokio::test]
#[ignore = "needs testenv's seeded Gitea container"]
async fn a_branch_lives_from_its_views_new_branch_pull_request_and_delete() {
    use knobas_source::view::{Block, FieldKind, SourceView, ViewAction};

    let env = env();
    let mut litter = Litter::new(&env).await;
    let source = env.one_repo();
    let http = reqwest::Client::new();
    let auth = format!("token {}", env.token);
    let api = format!("{}/api/v1/repos/{}", env.url, env.full_name());
    let repo_id = format!("gitea:{}", env.full_name());
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let branch = format!("knobas-life-{stamp}");
    let branch_id = format!("gitea:{}@refs/heads/{branch}", env.full_name());

    let action = |view: &SourceView, op: &str| -> Option<ViewAction> {
        view.actions.iter().find(|a| a.write_op == op).cloned()
    };
    let options = |action: &ViewAction, name: &str| -> Vec<String> {
        let field = action.fields.iter().find(|f| f.name == name).unwrap();
        let FieldKind::Choice { options } = &field.kind else {
            panic!("{name} is a choice: {field:?}")
        };
        options.iter().map(|o| o.value.clone()).collect()
    };
    let prefill = |action: &ViewAction, name: &str| -> Option<String> {
        action
            .fields
            .iter()
            .find(|f| f.name == name)
            .and_then(|f| f.value.clone())
    };

    // 1. The repository's New branch…, from the default branch it offers first.
    let repo_view = source.view(&repo_id, None).await.unwrap().unwrap();
    let new = action(&repo_view, "create_branch").expect("the repository offers New branch…");
    println!("LIVE repository New branch…: {new:?}");
    assert_eq!(new.disabled, None);
    let from = options(&new, "from_ref");
    assert_eq!(from[0], "main", "the default branch first: {from:?}");
    assert!(
        action(&repo_view, "create_pull_request").is_some(),
        "the repository offers New pull request…"
    );
    litter.will_create(&branch);
    source
        .write(knobas_source::WriteOp::CreateBranch {
            entity: new.entity.clone().unwrap_or_else(|| repo_id.clone()),
            name: branch.clone(),
            from_ref: from[0].clone(),
        })
        .await
        .expect("the adapter creates the branch");
    let subject = format!("knobas life check {stamp}");
    push_file(&http, &env, &branch, &format!("{branch}.txt"), &subject).await;

    // 2. The branch offers Open pull request…, prefilled from its one commit.
    let view = source.view(&branch_id, None).await.unwrap().unwrap();
    let open = action(&view, "create_pull_request").expect("the branch offers a pull request");
    println!("LIVE branch Open pull request…: {open:?}");
    assert_eq!(open.disabled, None, "one commit ahead");
    assert_eq!(open.entity.as_deref(), Some(repo_id.as_str()));
    assert_eq!(prefill(&open, "title").as_deref(), Some(subject.as_str()));
    assert_eq!(
        prefill(&open, "body").as_deref(),
        Some(format!("- {subject}").as_str())
    );
    assert_eq!(options(&open, "head"), std::slice::from_ref(&branch));
    assert_eq!(options(&open, "base")[0], "main");
    source
        .write(knobas_source::WriteOp::CreatePullRequest {
            entity: repo_id.clone(),
            title: subject.clone(),
            body: format!("- {subject}"),
            head: branch.clone(),
            base: "main".to_owned(),
        })
        .await
        .expect("the adapter opens the pull request");
    let number = litter
        .pulls()
        .await
        .into_iter()
        .find(|(_, head)| head == &branch)
        .map(|(number, _)| number)
        .expect("the pull request is in the server's listing");

    // 3. The overview opens on it, and nothing proposes a second.
    let view = source.view(&branch_id, None).await.unwrap().unwrap();
    let Some(Block::Entities { title, rows }) = view.sections[0].blocks.first() else {
        panic!("the overview opens on its pull request: {view:#?}");
    };
    assert_eq!(title.as_deref(), Some("Pull request"));
    assert_eq!(
        rows[0].entity_id.as_deref(),
        Some(format!("gitea:{}#{number}", env.full_name()).as_str())
    );
    assert!(action(&view, "create_pull_request").is_none());

    // 4. Cleanup's half: the pull request removed through the API (closed and
    //    gone, see above), then the delete the view offers, saying what is
    //    lost.
    let removed = http
        .delete(format!("{api}/issues/{number}"))
        .header("Authorization", &auth)
        .send()
        .await
        .expect("remove the pull request");
    assert!(
        removed.status().is_success(),
        "remove: {}",
        removed.status()
    );
    let view = source.view(&branch_id, None).await.unwrap().unwrap();
    let delete = action(&view, "delete_branch").expect("the branch offers Delete branch…");
    println!("LIVE branch Delete branch…: {delete:?}");
    assert!(delete.danger && delete.disabled.is_none(), "{delete:?}");
    assert_eq!(
        delete.confirm.as_deref(),
        Some(
            format!(
                "Delete {branch}? Not merged into main: 1 commit will only be reachable by SHA."
            )
            .as_str()
        )
    );
    let (before, cursor) = full(&*source).await;
    assert!(
        before.iter().any(|i| i.entity.to_string() == branch_id),
        "the mirror holds the branch before its delete"
    );
    source
        .write(knobas_source::WriteOp::DeleteBranch {
            entity: branch_id.clone(),
        })
        .await
        .expect("the adapter deletes the branch");
    assert!(
        !litter.branch_names().await.contains(&branch),
        "the server still lists the branch"
    );

    // 5. The next sync carries its tombstone, and a full one no longer the branch.
    let mut sink = VecSink(Vec::new());
    source
        .sync(Some(cursor), &mut sink)
        .await
        .expect("incremental sync");
    let gone = sink
        .0
        .iter()
        .find(|i| i.entity.to_string() == branch_id)
        .unwrap_or_else(|| panic!("the next sync said nothing of the deleted branch"));
    assert!(gone.deleted, "{gone:?}");
    let (after, _) = full(&*source).await;
    assert!(
        !after.iter().any(|i| i.entity.to_string() == branch_id),
        "a full sync still emits the deleted branch, so the sweep would keep it"
    );

    // 6. The default branch is refused before a request deletes anything.
    let refused = source
        .write(knobas_source::WriteOp::DeleteBranch {
            entity: format!("gitea:{}@refs/heads/main", env.full_name()),
        })
        .await;
    assert!(
        matches!(&refused, Err(SourceError::Protocol { status: None, message }) if message.contains("default branch")),
        "{refused:?}"
    );
    assert!(litter.branch_names().await.contains(&"main".to_owned()));
    println!("{branch}: created, proposed as #{number}, removed, deleted and tombstoned");
}

// -- edit, close and reopen from the pull request's view (issue #706) --------

/// Workflow 8 of spec #701, against the real server: on a pull request this
/// test opens, the toolbar's *Edit…* is prefilled with what Gitea holds; the
/// adapter's `edit_pull_request` -- the call the write queue's flush makes --
/// retitles it **without touching a description somebody else changed since**
/// (only the fields given are sent, the #706 ruling), rewrites the
/// description, closes it and reopens it; and each is read back, from the
/// view and from the server.
///
/// Never a seeded pull request: it is opened here on a `knobas-` branch of its
/// own, and [`Litter`] deletes it and the branch whether this passes or panics.
#[tokio::test]
#[ignore = "needs testenv's seeded Gitea container"]
async fn a_pull_request_is_edited_closed_and_reopened_from_its_view() {
    use knobas_source::{PullRequestState, WriteOp};

    let env = env();
    let mut litter = Litter::new(&env).await;
    let source = env.one_repo();
    let http = reqwest::Client::new();
    let auth = format!("token {}", env.token);
    let api = format!("{}/api/v1/repos/{}", env.url, env.full_name());
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let branch = format!("knobas-edit-{stamp}");
    litter.branch_off_main(&branch).await;
    let title = format!("knobas edit check {stamp}");
    let number = litter
        .open_pull_as(&env.login().await, &branch, &title)
        .await;
    let id = format!("gitea:{}#{number}", env.full_name());
    let view = || async {
        source
            .view(&id, None)
            .await
            .expect("the view reads")
            .expect("Gitea draws a pull request")
    };
    let record = || async {
        http.get(format!("{api}/pulls/{number}"))
            .header("Authorization", &auth)
            .send()
            .await
            .expect("read the pull request back")
            .json::<serde_json::Value>()
            .await
            .expect("a pull request record")
    };
    let edits = |view: &knobas_source::view::SourceView| -> Vec<String> {
        view.actions
            .iter()
            .filter(|a| a.write_op == "edit_pull_request")
            .map(|a| a.label.clone())
            .collect()
    };
    let state_of = |view: &knobas_source::view::SourceView| {
        said(view, "overview")
            .into_iter()
            .find(|l| l.starts_with("State="))
    };

    // 1. Open: *Edit…* prefilled with what the server holds, then *Close*.
    let before = view().await;
    assert_eq!(edits(&before), ["Edit…", "Close"], "{:#?}", before.actions);
    let edit = toolbar(&before, "Edit…");
    let held = record().await;
    assert_eq!(edit.fields[0].value.as_deref(), Some(title.as_str()));
    assert_eq!(edit.fields[1].value.as_deref(), held["body"].as_str());
    let close = toolbar(&before, "Close");
    assert!(close.danger && close.confirm.is_some(), "{close:?}");

    // 2. Somebody else rewrites the description after the view was read; the
    //    reader then changes only the title. Their description survives.
    let theirs = format!("rewritten in Gitea meanwhile, {stamp}");
    let patched = http
        .patch(format!("{api}/pulls/{number}"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "body": theirs }))
        .send()
        .await
        .expect("edit the description behind knobas' back");
    assert!(patched.status().is_success(), "{}", patched.status());
    let retitled = format!("knobas retitled {stamp}");
    source
        .write(WriteOp::EditPullRequest {
            entity: id.clone(),
            title: Some(retitled.clone()),
            body: None,
            state: None,
        })
        .await
        .expect("Gitea takes a new title");
    let now = record().await;
    assert_eq!(now["title"], retitled.as_str(), "{now}");
    assert_eq!(
        now["body"],
        theirs.as_str(),
        "a title edit sent a description: {now}"
    );
    assert_eq!(
        toolbar(&view().await, "Edit…").fields[0].value.as_deref(),
        Some(retitled.as_str())
    );

    // 3. The description, read back from the overview.
    let described = format!("described from the knobas toolbar, {stamp}");
    source
        .write(WriteOp::EditPullRequest {
            entity: id.clone(),
            title: None,
            body: Some(described.clone()),
            state: None,
        })
        .await
        .expect("Gitea takes a new description");
    assert!(
        said(&view().await, "overview").contains(&described),
        "{:#?}",
        said(&view().await, "overview")
    );
    assert_eq!(record().await["title"], retitled.as_str());

    // 4. Close: the state reads closed and the toolbar offers *Reopen*.
    source
        .write(WriteOp::EditPullRequest {
            entity: id.clone(),
            title: None,
            body: None,
            state: Some(PullRequestState::Closed),
        })
        .await
        .expect("Gitea closes the pull request");
    let closed = view().await;
    assert_eq!(state_of(&closed).as_deref(), Some("State=closed"));
    assert_eq!(edits(&closed), ["Edit…", "Reopen"], "{:#?}", closed.actions);
    assert_eq!(record().await["state"], "closed");

    // 5. Reopen: open again, and *Close* is back.
    source
        .write(WriteOp::EditPullRequest {
            entity: id.clone(),
            title: None,
            body: None,
            state: Some(PullRequestState::Open),
        })
        .await
        .expect("Gitea reopens the pull request");
    let reopened = view().await;
    assert_eq!(state_of(&reopened).as_deref(), Some("State=open"));
    assert_eq!(
        edits(&reopened),
        ["Edit…", "Close"],
        "{:#?}",
        reopened.actions
    );
    let last = record().await;
    assert_eq!(last["state"], "open");
    assert_eq!(last["body"], described.as_str(), "{last}");
    println!("edited, closed and reopened #{number} from its view");
}

// -- line comments on a pull request's diff (issue #709) ---------------------

/// Write `README.md` on `branch` as `content` (pre-encoded base64), through
/// Gitea's contents endpoint: an edit of a file `main` has, so the diff has a
/// removed line and an added one.
async fn rewrite_readme(http: &reqwest::Client, env: &Env, branch: &str, content: &str) {
    let api = format!("{}/api/v1/repos/{}", env.url, env.full_name());
    let auth = format!("token {}", env.token);
    let now: serde_json::Value = http
        .get(format!("{api}/contents/README.md?ref={branch}"))
        .header("Authorization", &auth)
        .send()
        .await
        .expect("read README.md")
        .json()
        .await
        .expect("a contents record");
    let sha = now["sha"].as_str().expect("README.md has a blob sha");
    let wrote = http
        .put(format!("{api}/contents/README.md"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "branch": branch, "content": content, "sha": sha,
            "message": format!("{branch}: README for the line-comment check"),
        }))
        .send()
        .await
        .expect("write README.md");
    assert!(
        wrote.status().is_success(),
        "contents: {}",
        wrote.text().await.unwrap_or_default()
    );
}

/// Workflow 6 of spec #701 (#709), against the real server: on a pull request
/// opened here, a review with two line comments -- one on a removed line (the
/// old side), one on an added line (the new side) -- is sent through the
/// adapter's `submit_review`, the call the write queue's flush makes, and the
/// *Files* tab read again draws each on its line. A later push that rewrites
/// the added line leaves its comment outdated, and the removed line's where it
/// was.
///
/// Never a seeded pull request: it is opened on a `knobas-` branch of its
/// own, and [`Litter`] deletes both whether this passes or panics.
#[tokio::test]
#[ignore = "needs testenv's seeded Gitea container"]
async fn line_comments_are_sent_with_a_review_and_drawn_on_their_lines() {
    use knobas_source::view::{Block, DiffFile, DiffLineKind, LineComments, SourceView};
    use knobas_source::{ReviewComment, ReviewSide, ReviewVerdict, WriteOp};

    let env = env();
    let mut litter = Litter::new(&env).await;
    let source = env.one_repo();
    let http = reqwest::Client::new();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let branch = format!("knobas-line-comments-{stamp}");
    litter.branch_off_main(&branch).await;
    // "# payout-service\nreviewed from knobas\nRust\n"
    rewrite_readme(
        &http,
        &env,
        &branch,
        "IyBwYXlvdXQtc2VydmljZQpyZXZpZXdlZCBmcm9tIGtub2JhcwpSdXN0Cg==",
    )
    .await;
    let number = litter
        .open_pull_as(
            "jonas.becker",
            &branch,
            &format!("knobas line comment check {stamp}"),
        )
        .await;
    let id = format!("gitea:{}#{number}", env.full_name());
    let files = || async {
        let view = source
            .view(&id, Some("files"))
            .await
            .expect("the files tab reads")
            .expect("Gitea draws a pull request");
        let Some(Block::Diff {
            files, commentable, ..
        }) = view.sections[0].blocks.first().cloned()
        else {
            panic!("the files tab is a diff: {view:#?}");
        };
        let readme = files
            .into_iter()
            .find(|f| f.path == "README.md")
            .unwrap_or_else(|| panic!("README.md is not in the diff: {view:#?}"));
        (readme, commentable)
    };

    // 1. The diff takes comments, and has a removed and an added line.
    let (readme, commentable) = files().await;
    assert_eq!(
        commentable.map(|c| c.write_op).as_deref(),
        Some("submit_review"),
        "an open pull request's diff takes line comments"
    );
    let first = |file: &DiffFile, kind: DiffLineKind| {
        file.hunks
            .iter()
            .flat_map(|h| &h.lines)
            .find(|l| l.kind == kind)
            .cloned()
            .unwrap_or_else(|| panic!("no {kind:?} line in {file:#?}"))
    };
    let removed = first(&readme, DiffLineKind::Del)
        .old
        .expect("an old number");
    let added = first(&readme, DiffLineKind::Add).new.expect("a new number");
    assert!(readme.threads.is_empty(), "{readme:#?}");
    let toolbar: SourceView = source.view(&id, None).await.unwrap().unwrap();
    let submit = toolbar
        .actions
        .iter()
        .find(|a| a.label == "Submit review…")
        .unwrap_or_else(|| panic!("no Submit review… in {:#?}", toolbar.actions));
    assert_eq!(submit.line_comments, LineComments::Requires);
    assert_eq!(
        submit.values.get("verdict").map(String::as_str),
        Some("comment")
    );

    // 2. One review, two line comments, one per side -- Gitea 1.27 takes
    //    them as `old_position` / `new_position` line numbers.
    let on_old = format!("why drop this line? {stamp}");
    let on_new = format!("say what reviewed it, {stamp}");
    source
        .write(WriteOp::SubmitReview {
            entity: id.clone(),
            verdict: ReviewVerdict::Comment,
            body: String::new(),
            comments: vec![
                ReviewComment {
                    path: "README.md".into(),
                    line: u64::from(removed),
                    side: ReviewSide::Old,
                    body: on_old.clone(),
                },
                ReviewComment {
                    path: "README.md".into(),
                    line: u64::from(added),
                    side: ReviewSide::New,
                    body: on_new.clone(),
                },
            ],
        })
        .await
        .expect("Gitea takes a review of line comments");

    // 3. Read back on their lines, by side, as the token's own words.
    let me = env.login().await;
    let (readme, _) = files().await;
    // Each thread as its side, line, whether outdated, and (author, words).
    type Placed = (ReviewSide, u32, bool, Vec<(String, String)>);
    let placed: Vec<Placed> = readme
        .threads
        .iter()
        .map(|t| {
            (
                t.side,
                t.line,
                t.outdated,
                t.comments
                    .iter()
                    .map(|c| (c.author.clone(), c.body.clone()))
                    .collect(),
            )
        })
        .collect();
    println!("placed: {placed:?}");
    assert_eq!(
        placed,
        [
            (
                ReviewSide::Old,
                removed,
                false,
                vec![(me.clone(), on_old.clone())]
            ),
            (
                ReviewSide::New,
                added,
                false,
                vec![(me.clone(), on_new.clone())]
            ),
        ],
        "{readme:#?}"
    );
    assert!(
        readme.threads.iter().all(|t| t.comments[0].at.is_some()),
        "Gitea dates its comments: {readme:#?}"
    );

    // 4. A push rewrites the added line: its comment is about a line the diff
    //    no longer holds as it was, so it is outdated; the removed line is
    //    still the removed line.
    // "# payout-service\nreviewed again from knobas\nRust\n"
    rewrite_readme(
        &http,
        &env,
        &branch,
        "IyBwYXlvdXQtc2VydmljZQpyZXZpZXdlZCBhZ2FpbiBmcm9tIGtub2JhcwpSdXN0Cg==",
    )
    .await;
    // Gitea's pull request diff is current at once; Forgejo updates a pull
    // request's head through a queue (#1195, measured on 17.0.0: a few seconds
    // behind the push), so read until the diff holds the pushed line.
    let mut waited = 0;
    let (readme, _) = loop {
        let (readme, rest) = files().await;
        let pushed = readme
            .hunks
            .iter()
            .flat_map(|h| &h.lines)
            .any(|l| l.text == "reviewed again from knobas");
        if pushed || waited >= 40 {
            break (readme, rest);
        }
        waited += 1;
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    };
    let state: Vec<(ReviewSide, bool)> = readme
        .threads
        .iter()
        .map(|t| (t.side, t.outdated))
        .collect();
    println!("after the push: {state:?}");
    assert_eq!(
        state,
        [(ReviewSide::Old, false), (ReviewSide::New, true)],
        "{readme:#?}"
    );
    println!("#{number}: two line comments sent, drawn on their lines, the rewritten one outdated");
}

/// A check row's meta is `<description> · <age>`, or the age alone when the
/// status has no description. Splits it into the description and the age.
fn split_check_meta(meta: &str) -> (&str, &str) {
    match meta.rsplit_once(" · ") {
        Some((text, age)) => (text, age),
        None => ("", meta),
    }
}

/// Whether `age` is a shape `view::ago` draws: `just now`, `N min ago`,
/// `N h ago`, or a `YYYY-MM-DD` date. The test cannot control how long the
/// queue delays it, so none of the four is refused.
fn is_age(age: &str) -> bool {
    let counted = |suffix: &str| {
        age.strip_suffix(suffix)
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
    };
    age == "just now"
        || counted(" min ago")
        || counted(" h ago")
        || (age.len() == 10
            && age.bytes().enumerate().all(|(i, b)| {
                if i == 4 || i == 7 {
                    b == b'-'
                } else {
                    b.is_ascii_digit()
                }
            }))
}

/// The statuses a commit carries are drawn as its *Checks* on the real server
/// (#710): reported on a commit this test makes on a branch of its own (so no
/// seeded commit gains a status), then read back through the commit's, the
/// branch's and a pull request's views -- the branch's asking by its name, a
/// slash and all, the others by sha. Failing first; the branch and its pull
/// request go with the guard.
#[tokio::test]
#[ignore = "needs testenv's seeded Gitea container"]
async fn a_commits_statuses_are_drawn_as_its_checks() {
    use knobas_source::view::{Block, SourceView, Tone, ViewRow};

    let env = env();
    let mut litter = Litter::new(&env).await;
    let source = env.one_repo();
    let http = reqwest::Client::new();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    // A `/` in the name, as a feature branch has: the branch's view asks for
    // its tip's statuses by the escaped name (`%2F`), and only a real server
    // can say it resolves that to the tip.
    let branch = format!("knobas-checks/{stamp}");
    litter.branch_off_main(&branch).await;
    let sha = push_file(
        &http,
        &env,
        &branch,
        &format!("checks-{stamp}.txt"),
        &format!("knobas checks {stamp}"),
    )
    .await;
    let api = format!("{}/api/v1/repos/{}", env.url, env.full_name());
    for (state, context, description) in [
        ("success", "ci/build", "Build passed"),
        ("failure", "ci/tests", "1 test failed"),
        ("pending", "ci/deploy", "Waiting"),
    ] {
        let posted = http
            .post(format!("{api}/statuses/{sha}"))
            .header("Authorization", format!("token {}", env.token))
            .json(&serde_json::json!({
                "state": state, "context": context, "description": description,
                "target_url": format!("https://ci.example/{context}"),
            }))
            .send()
            .await
            .expect("post a status");
        assert!(
            posted.status().is_success(),
            "status: {}",
            posted.text().await.unwrap_or_default()
        );
    }
    let number = litter
        .open_pull_as("jonas.becker", &branch, &format!("knobas checks {stamp}"))
        .await;

    let checks = |view: &SourceView| -> Vec<ViewRow> {
        let overview = view
            .sections
            .iter()
            .find(|s| s.id == "overview")
            .expect("an overview");
        overview
            .blocks
            .iter()
            .find_map(|b| match b {
                Block::Entities {
                    title: Some(t),
                    rows,
                } if t == "Checks" => Some(rows.clone()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no Checks in {overview:#?}"))
    };
    let full = env.full_name();
    for id in [
        format!("gitea:{full}@{sha}"),
        format!("gitea:{full}@refs/heads/{branch}"),
        format!("gitea:{full}#{number}"),
    ] {
        let view = source.view(&id, None).await.expect("reads").expect("drawn");
        let rows = checks(&view);
        println!("LIVE checks of {id}: {rows:?}");
        // A row's meta is its description plus the age (#1002); the age
        // depends on the clock, so assert it separately from the text.
        let drawn: Vec<_> = rows
            .iter()
            .map(|r| {
                let (text, age) = split_check_meta(r.meta.as_deref().unwrap_or_default());
                assert!(is_age(age), "{id}: {} has no age in {:?}", r.title, r.meta);
                (r.title.as_str(), r.chip.as_deref(), r.tone, Some(text))
            })
            .collect();
        assert_eq!(
            drawn,
            [
                (
                    "ci/tests",
                    Some("failure"),
                    Some(Tone::Err),
                    Some("1 test failed")
                ),
                (
                    "ci/deploy",
                    Some("pending"),
                    Some(Tone::Info),
                    Some("Waiting")
                ),
                (
                    "ci/build",
                    Some("success"),
                    Some(Tone::Ok),
                    Some("Build passed")
                ),
            ],
            "{id}"
        );
        assert_eq!(rows[0].url.as_deref(), Some("https://ci.example/ci/tests"));
    }

    // And a commit nobody reported on draws none: `main`'s seeded tip.
    let main = source
        .view(&format!("gitea:{full}@refs/heads/main"), None)
        .await
        .unwrap()
        .unwrap();
    let overview = &main.sections[0];
    assert!(
        !overview
            .blocks
            .iter()
            .any(|b| matches!(b, Block::Entities { title: Some(t), .. } if t == "Checks")),
        "{overview:#?}"
    );
}

/// The testenv's admin, as `testenv/seed-gitea.sh` creates it. Gitea mints an
/// access token only over basic auth, so the read-only token below needs it;
/// it is the fixed local development account, never a real credential.
const TESTENV_ADMIN: (&str, &str) = ("knobas", "knobas-dev");

/// Issue #722, against the real server: a 403 that refuses **the write** is a
/// refusal carrying Gitea's sentence, and the source stays healthy; the one
/// 403 that refuses **the token** -- a token without the write scope -- still
/// waits for a new one. Every sentence the adapter's 403 reading
/// (`client::forbidden_refusal`) depends on is measured here.
///
/// * the seeded pull mirror (`knobas/ops-runbooks-mirror`, seed section 4b):
///   its commit view offers *New branch…* disabled, and a branch created on
///   it anyway, through the adapter, is refused with *Git Repository is a
///   mirror.* while `test_connection` still passes;
/// * a scratch repository of the account's own (`knobas-live-refusals`):
///   Gitea's sentences for deleting a protected branch and the default one,
///   read off the wire -- the adapter's delete never sends these, its
///   pre-reads refuse them first (#707);
/// * a read-only token (`knobas-live-readonly`): its write is `Unauthorized`
///   with the 403, though `GET /user` answers it -- which is why an
///   authenticated read is not the signal.
///
/// The scratch repository and the token are removed before the run (a killed
/// run leaves them) and again before anything is asserted.
#[tokio::test]
#[ignore = "needs testenv's seeded Gitea container"]
async fn a_403_refusing_the_write_is_a_refusal_and_the_token_stays_good() {
    use knobas_source::WriteOp;

    let env = env();
    let http = reqwest::Client::builder()
        .timeout(live_env::REQUEST_BUDGET)
        .build()
        .expect("a client with a timeout");
    let auth = format!("token {}", env.token);
    let api = format!("{}/api/v1", env.url);
    let me = env.login().await;
    let scratch = "knobas-live-refusals";
    let token_name = "knobas-live-readonly";
    let (admin, password) = TESTENV_ADMIN;
    // The seed creates the mirror under its admin, whose token this is.
    assert_eq!(me, admin, "the seeded token is the seed's admin's");
    let mirror = format!("{admin}/ops-runbooks-mirror");

    let remove = async || {
        // Nothing to remove unless Gitea took the branch the test expects refused.
        let _ = http
            .delete(format!(
                "{api}/repos/{mirror}/branches/knobas-mirror-refusal"
            ))
            .header("Authorization", &auth)
            .send()
            .await;
        let _ = http
            .delete(format!("{api}/repos/{me}/{scratch}"))
            .header("Authorization", &auth)
            .send()
            .await;
        let _ = http
            .delete(format!("{api}/users/{admin}/tokens/{token_name}"))
            .basic_auth(admin, Some(password))
            .send()
            .await;
    };
    remove().await;

    // -- the mirror ----------------------------------------------------------
    let record: serde_json::Value = http
        .get(format!("{api}/repos/{mirror}"))
        .header("Authorization", &auth)
        .send()
        .await
        .expect("GET the mirror")
        .json()
        .await
        .expect("the mirror's record -- seed section 4b creates it");
    assert_eq!(record["mirror"], true, "the seeded mirror is one: {record}");
    let default = record["default_branch"].as_str().expect("a default branch");
    let tip: serde_json::Value = http
        .get(format!("{api}/repos/{mirror}/branches/{default}"))
        .header("Authorization", &auth)
        .send()
        .await
        .expect("GET the mirror's default branch")
        .json()
        .await
        .expect("a branch record");
    let sha = tip["commit"]["id"].as_str().expect("a tip").to_owned();

    let source = env.source(serde_json::json!({ "repos": [mirror] }));
    let view = source
        .view(&format!("gitea:{mirror}@{sha}"), None)
        .await
        .expect("the commit view reads")
        .expect("Gitea draws a commit");
    let new = view
        .actions
        .iter()
        .find(|a| a.write_op == "create_branch")
        .expect("a mirror's commit shows New branch…");
    println!("LIVE mirror commit New branch…: {new:?}");

    let refused = source
        .write(WriteOp::CreateBranch {
            entity: format!("gitea:{mirror}"),
            name: "knobas-mirror-refusal".to_owned(),
            from_ref: default.to_owned(),
        })
        .await;
    println!("LIVE create_branch on a mirror: {refused:?}");
    let healthy = source.test_connection().await;

    // -- the scratch repository: protected and default branch ---------------
    let created = http
        .post(format!("{api}/user/repos"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "name": scratch, "auto_init": true, "default_branch": "main" }))
        .send()
        .await
        .expect("POST /user/repos")
        .status()
        .as_u16();
    let mut setup = vec![created];
    for (path, body) in [
        (
            "branches",
            serde_json::json!({ "new_branch_name": "keep", "old_ref_name": "main" }),
        ),
        (
            "branch_protections",
            serde_json::json!({ "rule_name": "keep" }),
        ),
    ] {
        setup.push(
            http.post(format!("{api}/repos/{me}/{scratch}/{path}"))
                .header("Authorization", &auth)
                .json(&body)
                .send()
                .await
                .expect("set up the scratch repository")
                .status()
                .as_u16(),
        );
    }
    let mut said = Vec::new();
    for branch in ["keep", "main"] {
        let response = http
            .delete(format!("{api}/repos/{me}/{scratch}/branches/{branch}"))
            .header("Authorization", &auth)
            .send()
            .await
            .expect("DELETE a branch");
        let status = response.status().as_u16();
        let body: serde_json::Value = response.json().await.unwrap_or_default();
        said.push((
            status,
            body["message"].as_str().unwrap_or_default().to_owned(),
        ));
    }

    // -- a read-only token ---------------------------------------------------
    let minted: serde_json::Value = http
        .post(format!("{api}/users/{admin}/tokens"))
        .basic_auth(admin, Some(password))
        .json(&serde_json::json!({
            "name": token_name,
            "scopes": ["read:repository", "read:user"]
        }))
        .send()
        .await
        .expect("mint a read-only token")
        .json()
        .await
        .expect("the token record");
    let readonly = minted["sha1"].as_str().unwrap_or_default().to_owned();
    let narrow = env.source_with(&readonly, serde_json::json!({ "repos": [env.full_name()] }));
    let narrow_reads = narrow.test_connection().await;
    let narrow_write = narrow
        .write(WriteOp::CreateBranch {
            entity: format!("gitea:{}", env.full_name()),
            name: "knobas-scope-refusal".to_owned(),
            from_ref: "main".to_owned(),
        })
        .await;
    println!("LIVE create_branch with a read-only token: {narrow_write:?}");

    remove().await;

    assert_eq!(
        new.disabled.as_deref(),
        Some("A mirror: its branches come from upstream"),
        "{new:?}"
    );
    assert!(
        matches!(&refused, Err(SourceError::Protocol { status: Some(403), message })
                 if message.contains("Git Repository is a mirror.")),
        "a branch on a mirror is refused with Gitea's sentence, not a dead token: {refused:?}"
    );
    assert!(
        healthy.is_ok(),
        "the source stays healthy after the refusal: {healthy:?}"
    );
    assert_eq!(
        setup,
        vec![201, 201, 201],
        "the scratch repository's set-up"
    );
    // The second sentence is the server's own wording: Gitea says "default or
    // pull request target branch", Forgejo 17 only "default branch" (#1195).
    // Nothing in the adapter reads either; both are a 403 passed through.
    assert_eq!(said.len(), 2, "{said:?}");
    assert_eq!(said[0], (403, "branch protected".to_owned()), "{said:?}");
    assert_eq!(said[1].0, 403, "{said:?}");
    assert!(
        [
            "can not delete default or pull request target branch",
            "can not delete default branch",
        ]
        .contains(&said[1].1.as_str()),
        "Gitea's or Forgejo's sentence for a default branch's delete: {said:?}"
    );
    assert!(
        !readonly.is_empty(),
        "a read-only token was minted: {minted}"
    );
    assert!(
        narrow_reads.is_ok(),
        "the premise: an authenticated read passes on a read-only token: {narrow_reads:?}"
    );
    assert!(
        matches!(
            narrow_write,
            Err(SourceError::Unauthorized { status: Some(403) })
        ),
        "a token without the write scope still waits for a new one: {narrow_write:?}"
    );
}

// -- a file commit through the contents API (issue #822) ----------------------

/// A file's blob and its text on `branch`, read the way the Files tab reads
/// it: what an edit is made against, and what a commit reads back.
async fn file_on(http: &reqwest::Client, env: &Env, branch: &str, path: &str) -> (String, String) {
    let record: serde_json::Value = http
        .get(format!(
            "{}/api/v1/repos/{}/contents/{path}?ref={branch}",
            env.url,
            env.full_name()
        ))
        .header("Authorization", format!("token {}", env.token))
        .send()
        .await
        .expect("read the file")
        .json()
        .await
        .expect("a contents record");
    let raw = http
        .get(format!(
            "{}/api/v1/repos/{}/raw/{path}?ref={branch}",
            env.url,
            env.full_name()
        ))
        .header("Authorization", format!("token {}", env.token))
        .send()
        .await
        .expect("read the raw file")
        .text()
        .await
        .unwrap();
    (
        record["sha"]
            .as_str()
            .unwrap_or_else(|| panic!("a blob sha: {record}"))
            .to_owned(),
        raw,
    )
}

/// Spec #808 D4 and story 93, against the real server (issue #822): the
/// adapter's `commit_file` -- the call the write queue's flush makes -- on a
/// file this test makes on a `knobas-` branch of its own:
///
/// 1. **onto the branch**, against the blob the file was read at: the branch
///    carries the new text, at the commit the receipt names;
/// 2. **onto a new branch** made from it by Gitea's `new_branch`, which a
///    pull request is then opened from, the base branch left as it was;
/// 3. **over a file somebody changed since it was read**: Gitea's 422 on the
///    stale `sha`, answered as `SourceError::Moved` carrying the blob the
///    branch holds now and its text -- the other half of the held write,
///    whose queue half `knobas-sync`'s
///    `a_commit_over_a_file_that_moved_is_held_with_both_versions` pins --
///    and the file left as they made it;
/// 4. **onto a protected branch**: refused before the request, with the
///    reason and what to do instead, the file untouched -- while a new branch
///    from it still goes. The file's view declares *Commit change…* taken
///    before the protection and disabled with that same reason after (#985).
///
/// [`Litter`] deletes both branches, the protection named for the first and
/// the pull request, whether this passes or panics. Nothing touches `main`.
#[tokio::test]
#[ignore = "needs testenv's seeded Gitea container"]
async fn a_file_is_committed_onto_a_branch_a_new_branch_held_when_it_moved_and_refused_when_protected()
 {
    use knobas_source::WriteOp;

    let env = env();
    let mut litter = Litter::new(&env).await;
    let source = env.one_repo();
    let http = reqwest::Client::new();
    let auth = format!("token {}", env.token);
    let api = format!("{}/api/v1/repos/{}", env.url, env.full_name());
    let repo_id = format!("gitea:{}", env.full_name());
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let branch = format!("knobas-commit-{stamp}");
    let fresh = format!("{branch}-new");
    let path = format!("docs/{branch}.md");
    litter.branch_off_main(&branch).await;
    push_file(&http, &env, &branch, &path, "a file to edit").await;
    let (read_at, before) = file_on(&http, &env, &branch, &path).await;
    assert_eq!(before, "knobas live check\n");
    let commit =
        |branch: &str, new_branch: Option<&str>, content: &str, base: &str| WriteOp::CommitFile {
            repo: repo_id.clone(),
            branch: branch.to_owned(),
            new_branch: new_branch.map(str::to_owned),
            path: path.clone(),
            content: content.to_owned(),
            message: format!("knobas commit check {stamp}"),
            base_sha: base.to_owned(),
        };

    // 1. Onto the branch.
    let receipt = source
        .write(commit(&branch, None, "mine, first\n", &read_at))
        .await
        .expect("the adapter commits the file");
    println!("LIVE commit_file onto {branch}: {receipt:?}");
    let (first, text) = file_on(&http, &env, &branch, &path).await;
    assert_eq!(text, "mine, first\n");
    let tip: serde_json::Value = http
        .get(format!("{api}/branches/{branch}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    // By the commit's key, the local part of its entity id (#1949).
    assert_eq!(
        receipt.remote_id,
        tip["commit"]["id"]
            .as_str()
            .map(|oid| format!("{}@{oid}", env.full_name())),
        "the receipt names the commit the branch is now at"
    );

    // 2. Onto a new branch, and a pull request from it.
    litter.will_create(&fresh);
    source
        .write(commit(
            &branch,
            Some(&fresh),
            "mine, on a new branch\n",
            &first,
        ))
        .await
        .expect("the adapter commits to a new branch");
    assert_eq!(
        file_on(&http, &env, &fresh, &path).await.1,
        "mine, on a new branch\n"
    );
    assert_eq!(
        file_on(&http, &env, &branch, &path).await,
        (first.clone(), "mine, first\n".to_owned()),
        "the branch it was made from is left as it was"
    );
    source
        .write(WriteOp::CreatePullRequest {
            entity: repo_id.clone(),
            title: format!("knobas commit check {stamp}"),
            body: "Opened by the knobas Gitea adapter's live suite (#822).".to_owned(),
            head: fresh.clone(),
            base: branch.clone(),
        })
        .await
        .expect("a pull request from the new branch");
    assert!(
        litter.pulls().await.iter().any(|(_, head)| head == &fresh),
        "the pull request is in the server's listing"
    );

    // 3. Somebody commits to the file first; the edit made against `first`
    //    comes back moved, with what the branch holds now.
    let theirs = http
        .put(format!("{api}/contents/{path}"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "branch": branch,
            // "theirs\n"
            "content": "dGhlaXJzCg==",
            "message": "somebody else's edit",
            "sha": first,
        }))
        .send()
        .await
        .expect("their commit");
    assert!(theirs.status().is_success(), "theirs: {}", theirs.status());
    let (theirs_at, _) = file_on(&http, &env, &branch, &path).await;
    let moved = source
        .write(commit(&branch, None, "mine, too late\n", &first))
        .await;
    println!("LIVE commit_file over a moved file: {moved:?}");
    assert!(
        matches!(&moved, Err(SourceError::Moved { base, current })
                 if *base == theirs_at && current == "theirs\n"),
        "a stale base is held with what Gitea holds now: {moved:?}"
    );
    assert_eq!(
        file_on(&http, &env, &branch, &path).await.1,
        "theirs\n",
        "their commit stands"
    );

    // 4. The branch is protected: refused with the reason, before a request;
    //    a new branch from it still goes. The file's view declares its commit
    //    taken before, and disabled with the same reason after (#985).
    let branch_entity = format!("{repo_id}@refs/heads/{branch}");
    let file_section = format!("file:{path}");
    let declared_commit = || async {
        let view = source
            .view(&branch_entity, Some(&file_section))
            .await
            .expect("the file's view")
            .expect("drawn");
        view.sections[0]
            .blocks
            .iter()
            .find_map(|block| match block {
                knobas_source::view::Block::Actions { actions } => actions
                    .iter()
                    .find(|a| a.write_op == "commit_file")
                    .cloned(),
                _ => None,
            })
            .expect("the file declares its commit")
    };
    assert_eq!(
        declared_commit().await.disabled,
        None,
        "an unprotected branch takes the commit"
    );
    let protected = http
        .post(format!("{api}/branch_protections"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "rule_name": branch }))
        .send()
        .await
        .expect("protect the branch");
    assert!(
        protected.status().is_success(),
        "protection: {}",
        protected.text().await.unwrap_or_default()
    );
    let refused = source
        .write(commit(
            &branch,
            None,
            "mine, on a protected branch\n",
            &theirs_at,
        ))
        .await;
    println!("LIVE commit_file onto a protected branch: {refused:?}");
    assert!(
        matches!(&refused, Err(SourceError::Protocol { status: None, message })
                 if message.contains("protected") && message.contains("new branch")),
        "{refused:?}"
    );
    let reason = declared_commit()
        .await
        .disabled
        .expect("a protected branch's commit is declared disabled");
    println!("LIVE the file's commit on a protected branch: {reason:?}");
    assert!(
        matches!(&refused, Err(SourceError::Protocol { message, .. }) if message.ends_with(&reason)),
        "the declared reason is the flush's: {reason:?} vs {refused:?}"
    );
    assert_eq!(file_on(&http, &env, &branch, &path).await.1, "theirs\n");
    let off_protected = format!("{branch}-off");
    litter.will_create(&off_protected);
    source
        .write(commit(
            &branch,
            Some(&off_protected),
            "mine, off a protected branch\n",
            &theirs_at,
        ))
        .await
        .expect("a new branch from a protected one takes the commit");
    assert_eq!(
        file_on(&http, &env, &off_protected, &path).await.1,
        "mine, off a protected branch\n"
    );
    println!(
        "{branch}: committed onto, to {fresh} with a pull request, held when moved, refused when protected"
    );
}

/// Issue #883, against the real server: the Files tab draws a file's blob
/// sha, and it is the one an edit commits against.
///
/// 1. The seeded `README.md` (`auto_init`'s) read through the adapter's file
///    view at `main` carries the `sha` Gitea's contents API answers for it.
///    Read only; nothing touches `main`.
/// 2. On a `knobas-` branch of this test's own, made off `main`, the file
///    view's sha for the same file is the base of a `WriteOp::CommitFile`,
///    which lands: the branch holds the new text, and the file view drawn
///    again carries the blob the commit made, not the one it replaced.
///
/// [`Litter`] deletes the branch whether this passes or panics.
#[tokio::test]
#[ignore = "needs testenv's seeded Gitea container"]
async fn the_file_view_draws_the_blob_a_commit_is_made_against() {
    use knobas_source::WriteOp;
    use knobas_source::view::Block;

    let env = env();
    let mut litter = Litter::new(&env).await;
    let source = env.one_repo();
    let http = reqwest::Client::new();
    let repo_id = format!("gitea:{}", env.full_name());
    let drawn_file = |entity: String, section: &'static str| {
        let source = &source;
        async move {
            let view = source
                .view(&entity, Some(section))
                .await
                .unwrap_or_else(|error| panic!("{entity} {section}: {error:?}"))
                .expect("drawn");
            let Block::File { sha, text, .. } = view.sections[0].blocks[0].clone() else {
                panic!("a file block at {entity} {section}");
            };
            (sha, text)
        }
    };

    // 1. The seeded file at `main`.
    let (on_main, _) = file_on(&http, &env, "main", "README.md").await;
    let (sha, text) = drawn_file(repo_id.clone(), "file:README.md").await;
    println!("LIVE README.md at main: drawn {sha:?}, contents API {on_main}");
    assert_eq!(
        sha.as_deref(),
        Some(on_main.as_str()),
        "the drawn blob is the contents API's"
    );
    assert!(text.starts_with(&format!("# {}", env.repo)), "{text:?}");

    // 2. A commit made against the drawn blob, on a branch of this test's own.
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let branch = format!("knobas-blob-{stamp}");
    litter.branch_off_main(&branch).await;
    let entity = format!("{repo_id}@refs/heads/{branch}");
    let (base, _) = drawn_file(entity.clone(), "file:README.md").await;
    let base = base.expect("the branch's file carries its blob");
    assert_eq!(base, on_main, "a branch off main holds main's blob");
    let content = format!("edited against the drawn blob {stamp}\n");
    let receipt = source
        .write(WriteOp::CommitFile {
            repo: repo_id.clone(),
            branch: branch.clone(),
            new_branch: None,
            path: "README.md".to_owned(),
            content: content.clone(),
            message: format!("knobas blob check {stamp}"),
            base_sha: base.clone(),
        })
        .await
        .expect("a commit against the drawn blob lands");
    println!("LIVE commit_file against the drawn blob: {receipt:?}");
    let (now, text) = file_on(&http, &env, &branch, "README.md").await;
    assert_eq!(text, content, "the branch holds the edit");
    let (redrawn, drawn_text) = drawn_file(entity, "file:README.md").await;
    assert_eq!(drawn_text, content);
    assert_eq!(
        redrawn.as_deref(),
        Some(now.as_str()),
        "drawn again, the file carries the commit's blob"
    );
    assert_ne!(now, base, "the commit made a new blob");
    assert_eq!(
        file_on(&http, &env, "main", "README.md").await.0,
        on_main,
        "main is left as it was"
    );
}

/// A repository's language and teams (#966), through the descriptor's own
/// declarations, on the real server's records.
///
/// The seeded repositories cannot witness either: their default branches
/// hold no source file Gitea's language statistics count, and every seeded
/// person reaches them through *Owners*, which holds every repository of the
/// organization. So the test makes its own: an organization repository with
/// one Rust file on `main`, added to one team of its own, and removes both
/// at the end. A user's repository (the seeded mirror) is the other arm:
/// Gitea answers 405 for its teams, which reads as none.
#[tokio::test]
#[ignore = "needs testenv's seeded Gitea container"]
async fn a_repository_reads_its_language_and_its_own_teams_not_owners() {
    let env = env();
    let http = reqwest::Client::builder()
        .timeout(live_env::REQUEST_BUDGET)
        .build()
        .expect("a client with a timeout");
    let api = format!("{}/api/v1", env.url);
    let (admin, password) = TESTENV_ADMIN;
    let owner = env.owner.clone();
    let scratch = "knobas-live-language";
    let team_name = "knobas-live-team";
    let full_name = format!("{owner}/{scratch}");

    let remove = async || {
        let _ = http
            .delete(format!("{api}/repos/{full_name}"))
            .basic_auth(admin, Some(password))
            .send()
            .await;
        let teams: serde_json::Value = http
            .get(format!("{api}/orgs/{owner}/teams"))
            .basic_auth(admin, Some(password))
            .send()
            .await
            .expect("GET the organization's teams")
            .json()
            .await
            .expect("a team list");
        for team in teams.as_array().into_iter().flatten() {
            if team["name"] == team_name {
                let _ = http
                    .delete(format!("{api}/teams/{}", team["id"]))
                    .basic_auth(admin, Some(password))
                    .send()
                    .await;
            }
        }
    };
    remove().await;

    let status = |what: &str, response: reqwest::Response| {
        assert!(
            response.status().is_success(),
            "{what}: {}",
            response.status()
        );
        response
    };
    status(
        "create the repository",
        http.post(format!("{api}/orgs/{owner}/repos"))
            .basic_auth(admin, Some(password))
            .json(&serde_json::json!({ "name": scratch, "auto_init": true, "default_branch": "main" }))
            .send()
            .await
            .expect("POST /orgs/{owner}/repos"),
    );
    let team: serde_json::Value = status(
        "create the team",
        http.post(format!("{api}/orgs/{owner}/teams"))
            .basic_auth(admin, Some(password))
            .json(&serde_json::json!({
                "name": team_name, "permission": "read", "units": ["repo.code"],
                "includes_all_repositories": false
            }))
            .send()
            .await
            .expect("POST /orgs/{owner}/teams"),
    )
    .json()
    .await
    .expect("the team record");
    status(
        "add the repository to the team",
        http.put(format!("{api}/teams/{}/repos/{full_name}", team["id"]))
            .basic_auth(admin, Some(password))
            .send()
            .await
            .expect("PUT /teams/{id}/repos"),
    );
    // `fn main() {}\n`, so Gitea's statistics find Rust on the default branch.
    status(
        "push a Rust file",
        http.post(format!("{api}/repos/{full_name}/contents/src/main.rs"))
            .basic_auth(admin, Some(password))
            .json(&serde_json::json!({
                "branch": "main", "content": "Zm4gbWFpbigpIHt9Cg==", "message": "knobas live: a Rust file"
            }))
            .send()
            .await
            .expect("POST contents"),
    );
    // The statistics are worked out after the push, on Gitea's own queue.
    let mut said = String::new();
    for _ in 0..60 {
        let record: serde_json::Value = http
            .get(format!("{api}/repos/{full_name}"))
            .basic_auth(admin, Some(password))
            .send()
            .await
            .expect("GET the repository")
            .json()
            .await
            .expect("a repository record");
        said = record["language"].as_str().unwrap_or_default().to_owned();
        if !said.is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
    println!("LIVE Gitea's language for {full_name}: {said:?}");

    let declared = knobas_source_gitea::descriptor_template()
        .payload_paths
        .into_iter()
        .find(|paths| paths.kind == "repo")
        .expect("a repository declares its paths");
    let read = |items: &[SyncItem], name: &str| {
        let repo = of_kind(items, "repo")
            .into_iter()
            .find(|item| item.title == name)
            .unwrap_or_else(|| panic!("{name} is synced"))
            .clone();
        (
            knobas_core::payload::resolve_string(&repo.payload, &declared.language),
            knobas_core::payload::resolve_list(&repo.payload, &declared.teams),
            repo.payload.get("teams").cloned(),
        )
    };
    let (items, _) = full(&*env.source(serde_json::json!({ "repos": [full_name] }))).await;
    let (language, teams, _) = read(&items, &full_name);
    let mirror = format!("{admin}/ops-runbooks-mirror");
    let (user_items, _) = full(&*env.source(serde_json::json!({ "repos": [mirror] }))).await;
    let (_, _, user_teams) = read(&user_items, &mirror);
    remove().await;

    println!(
        "LIVE declared language {language:?}, teams {teams:?}; the mirror's teams {user_teams:?}"
    );
    assert_eq!(language.as_deref(), Some("Rust"), "Gitea said {said:?}");
    assert_eq!(
        teams,
        vec![team_name.to_owned()],
        "Owners holds every repository and is left out"
    );
    assert_eq!(
        user_teams,
        Some(serde_json::json!([])),
        "a user's repository has no teams to have"
    );
}

// -- a source action: Add label… on a pull request (issue #1194) -------------

/// Delete every label named `knobas-label-…` of the mutated repository and
/// of its organisation (#1235): this test's own, and an earlier run's that
/// panicked before it cleaned up. Nothing `testenv/seed-gitea.sh` creates is
/// a label, let alone one named so.
async fn clear_test_labels(http: &reqwest::Client, env: &Env, auth: &str) {
    let repo = format!("{}/api/v1/repos/{}", env.url, env.full_name());
    let org = format!("{}/api/v1/orgs/{}", env.url, env.owner);
    for api in [repo, org] {
        let labels: Vec<serde_json::Value> = http
            .get(format!("{api}/labels?limit=50"))
            .header("Authorization", auth)
            .send()
            .await
            .expect("list the labels")
            .json()
            .await
            .expect("a label list");
        for label in labels {
            if label["name"]
                .as_str()
                .is_some_and(|n| n.starts_with("knobas-label-"))
            {
                http.delete(format!("{api}/labels/{}", label["id"]))
                    .header("Authorization", auth)
                    .send()
                    .await
                    .expect("delete a test label");
            }
        }
    }
}

/// Make a label named `name` at `api` (a repository's or an organisation's
/// `…/labels`), answering its id.
async fn make_label(http: &reqwest::Client, api: &str, auth: &str, name: &str) -> String {
    let response = http
        .post(api)
        .header("Authorization", auth)
        .json(&serde_json::json!({ "name": name, "color": "#00aabb" }))
        .send()
        .await
        .expect("make a label");
    assert!(
        response.status().is_success(),
        "{api}: {}",
        response.status()
    );
    let made: serde_json::Value = response.json().await.expect("the label made");
    made["id"].as_u64().expect("a label id").to_string()
}

/// ADR-0029's source action against the real server (ADR-0013): on a pull
/// request this test opens, the toolbar offers *Add label…* over a label this
/// test makes; the answer the dialog would give, handed to the adapter as the
/// queue's flush hands a `source_action` back, puts the label on the pull
/// request; Gitea reads it back, and the view no longer offers it. A label of
/// the owning organisation is offered and taken the same way (#1235), and
/// the organisation listing the adapter reads is 404 for a user.
///
/// Never a seeded pull request or label: all are made here, [`Litter`]
/// removes the pull request and its branch, and the labels are deleted at
/// the end and at the start of the next run.
#[tokio::test]
#[ignore = "needs testenv's seeded Gitea container"]
async fn a_label_is_added_to_a_pull_request_through_its_source_action() {
    use knobas_source::WriteOp;

    let env = env();
    let mut litter = Litter::new(&env).await;
    let source = env.one_repo();
    let http = reqwest::Client::new();
    let auth = format!("token {}", env.token);
    let api = format!("{}/api/v1/repos/{}", env.url, env.full_name());
    clear_test_labels(&http, &env, &auth).await;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let name = format!("knobas-label-{stamp}");
    let label_id = make_label(&http, &format!("{api}/labels"), &auth, &name).await;
    let org_name = format!("knobas-label-org-{stamp}");
    let org_labels = format!("{}/api/v1/orgs/{}/labels", env.url, env.owner);
    let org_label_id = make_label(&http, &org_labels, &auth, &org_name).await;
    // A user is no organisation: Gitea's 404, which the adapter reads as a
    // user owner with no organisation labels.
    let user_labels = http
        .get(format!(
            "{}/api/v1/orgs/{}/labels",
            env.url,
            env.login().await
        ))
        .header("Authorization", &auth)
        .send()
        .await
        .expect("a user's organisation labels");
    assert_eq!(user_labels.status().as_u16(), 404);

    let branch = format!("knobas-label-{stamp}");
    litter.branch_off_main(&branch).await;
    let number = litter
        .open_pull_as(
            &env.login().await,
            &branch,
            &format!("knobas label check {stamp}"),
        )
        .await;
    let id = format!("gitea:{}#{number}", env.full_name());
    let view = || async {
        source
            .view(&id, None)
            .await
            .expect("the view reads")
            .expect("Gitea draws a pull request")
    };

    // 1. The toolbar offers the label, as a choice, with the action's id fixed.
    let add = toolbar(&view().await, "Add label…");
    assert_eq!(add.write_op, "source_action", "{add:#?}");
    assert_eq!(add.disabled, None, "{add:#?}");
    let knobas_source::view::FieldKind::Choice { options } = &add.fields[0].kind else {
        panic!("Add label… asks for a choice: {add:#?}")
    };
    for (id, name) in [(&label_id, &name), (&org_label_id, &org_name)] {
        assert!(
            options.iter().any(|o| o.value == *id && o.label == *name),
            "{name}: {options:#?}"
        );
    }

    // 2. What the shell queues from that dialog -- the fixed values, the id
    //    among them, and the answer -- and the flush hands to the adapter.
    //    The organisation's label too: Gitea takes it on the repository's
    //    pull request.
    for picked in [&label_id, &org_label_id] {
        let mut fields = add.values.clone();
        let action_id = fields.remove("id").expect("the action's id is fixed");
        fields.insert(add.fields[0].name.clone(), picked.clone());
        source
            .write(WriteOp::SourceAction {
                entity: id.clone(),
                id: action_id,
                fields,
            })
            .await
            .expect("Gitea takes the label");
    }

    // 3. Read back from the server, and from the view.
    let pull: serde_json::Value = http
        .get(format!("{api}/pulls/{number}"))
        .header("Authorization", &auth)
        .send()
        .await
        .expect("read the pull request back")
        .json()
        .await
        .expect("a pull request record");
    for name in [&name, &org_name] {
        assert!(
            pull["labels"]
                .as_array()
                .is_some_and(|l| l.iter().any(|l| l["name"] == name.as_str())),
            "{name}: {}",
            pull["labels"]
        );
    }
    let again = toolbar(&view().await, "Add label…");
    let offered_again = match &again.fields.first().map(|f| &f.kind) {
        Some(knobas_source::view::FieldKind::Choice { options }) => options
            .iter()
            .any(|o| o.value == label_id || o.value == org_label_id),
        _ => false,
    };
    assert!(
        !offered_again,
        "a label it carries is offered again: {again:#?}"
    );

    clear_test_labels(&http, &env, &auth).await;
    println!("{name} added to #{number} through its source action");
}

/// The role data check (#1199, ADR-0028) on the seeded corpus: every role the
/// adapter claims is filled by the records a full sync emits, each kind it
/// needs present. What a source's first sync checks in the engine, held here
/// to the real container.
#[tokio::test]
#[ignore = "needs testenv's seeded Gitea container"]
async fn fills_every_role_it_claims_on_the_seeded_corpus() {
    let env = env();
    let source = env.source_with(&env.token, serde_json::json!({}));
    knobas_source::contract::fills_its_roles(&*source).await;
}

// -- issues as tickets (issue #1206) ------------------------------------------

/// The seeded issues of a quiet repository (`seed-gitea.sh` section 7a) come
/// back as tickets carrying what the ticket system reads, a closed one among
/// them in its state; and an idle run after the full one emits no ticket.
/// This is where the fake's issue shapes are certified: the listing's
/// `repository`, `assignee`, `labels` and `pull_request`, and `since=`.
#[tokio::test]
#[ignore = "needs testenv's seeded Gitea container"]
async fn the_seeded_issues_sync_as_tickets_with_their_priority_assignee_and_state() {
    let env = env();
    let ledger = format!("{}/ledger-api", env.owner);
    let source = env.source(serde_json::json!({ "repos": [ledger] }));
    let (items, cursor) = full(&*source).await;
    let tickets = of_kind(&items, "ticket");
    let titled = |title: &str| {
        *tickets
            .iter()
            .find(|t| t.title == title)
            .unwrap_or_else(|| panic!("no ticket {title:?}: did the seed run?"))
    };

    let high = titled("Ledger export rounds negative amounts");
    let id = high.entity.to_string();
    assert!(
        id.starts_with(&format!("gitea:{ledger}/issues/")),
        "the ticket key form: {id}"
    );
    assert_eq!(high.payload["state"], "open");
    assert_eq!(high.payload["knobas"]["priority"], "High");
    assert_eq!(high.payload["assignee"]["login"], "tomasz.wojcik");
    assert_eq!(high.payload["repository"]["full_name"], ledger.as_str());
    assert_eq!(high.payload["pull_request"], serde_json::Value::Null);
    let labels: Vec<&str> = high.payload["labels"]
        .as_array()
        .expect("labels are a list")
        .iter()
        .filter_map(|l| l["name"].as_str())
        .collect();
    assert!(labels.contains(&"Kind/Bug"), "{labels:?}");
    assert!(
        high.body_text.contains("Seen on the September close too."),
        "the discussion is searchable: {:?}",
        high.body_text
    );
    assert_eq!(high.author.as_deref(), Some("priya.nair"));

    let medium = titled("Reconcile settlement fees nightly");
    assert_eq!(medium.payload["knobas"]["priority"], "Medium");
    assert_eq!(medium.payload["assignee"], serde_json::Value::Null);
    let closed = titled("Drop the v1 balance endpoint");
    assert_eq!(closed.payload["state"], "closed");
    assert_eq!(
        closed.payload["knobas"]["priority"],
        serde_json::Value::Null
    );
    assert!(!closed.deleted);
    // No pull request of the repository is a ticket.
    for pull in of_kind(&items, "pr") {
        assert!(
            !tickets.iter().any(|t| t.title == pull.title),
            "{} is a ticket too",
            pull.title
        );
    }

    let mut sink = VecSink(Vec::new());
    source
        .sync(Some(cursor), &mut sink)
        .await
        .expect("an idle run");
    assert!(
        of_kind(&sink.0, "ticket").is_empty(),
        "an idle run re-delivered tickets: {:?}",
        of_kind(&sink.0, "ticket")
            .iter()
            .map(|t| t.entity.to_string())
            .collect::<Vec<_>>()
    );
    println!("{} tickets from {ledger}", tickets.len());
}

/// The prefix of every issue the next test files, which it and its next run
/// delete. Nothing `seed-gitea.sh` creates is titled so.
const TEST_ISSUE: &str = "knobas-ticket-";

/// Delete every issue of the mutated repository titled [`TEST_ISSUE`]…: this
/// test's own, and one an earlier run left when it panicked.
async fn clear_test_issues(http: &reqwest::Client, env: &Env, auth: &str) {
    let api = format!("{}/api/v1/repos/{}", env.url, env.full_name());
    // Every page, not `q=`: Gitea's keyword search reads an indexer that may
    // not hold an issue filed a moment ago.
    let mut ours = Vec::new();
    for page in 1.. {
        let found: Vec<serde_json::Value> = http
            .get(format!(
                "{api}/issues?type=issues&state=all&limit=50&page={page}"
            ))
            .header("Authorization", auth)
            .send()
            .await
            .expect("list the issues")
            .json()
            .await
            .expect("an issue list");
        if found.is_empty() {
            break;
        }
        ours.extend(found.into_iter().filter(|issue| {
            issue["title"]
                .as_str()
                .is_some_and(|t| t.starts_with(TEST_ISSUE))
        }));
    }
    for issue in ours {
        let gone = http
            .delete(format!("{api}/issues/{}", issue["number"]))
            .header("Authorization", auth)
            .send()
            .await
            .expect("delete a test issue");
        assert!(gone.status().is_success(), "{}", gone.status());
    }
}

/// Each ticket write the adapter declares, against the real server: an issue
/// filed with `create_ticket`, read back by the next incremental run; its
/// workflow read; `transition` closing it, which the next run reads as a
/// state and not a deletion, and reopening it; and a `comment` on it. The
/// issue is deleted at the end, and at the start of the next run.
#[tokio::test]
#[ignore = "needs testenv's seeded Gitea container"]
async fn a_ticket_is_filed_moved_and_commented_on_through_its_writes() {
    use knobas_source::WriteOp;

    let env = env();
    let http = reqwest::Client::new();
    let auth = format!("token {}", env.token);
    let api = format!("{}/api/v1/repos/{}", env.url, env.full_name());
    clear_test_issues(&http, &env, &auth).await;
    let source = env.one_repo();
    let (_, cursor) = full(&*source).await;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let title = format!("{TEST_ISSUE}{stamp}");

    // 1. Filed, and named by its key (#1754).
    let made = source
        .write(WriteOp::CreateTicket {
            entity: format!("gitea:{}", env.full_name()),
            title: title.clone(),
            body: "Filed by the live suite.".to_owned(),
            ticket_type: "Task".to_owned(),
        })
        .await
        .expect("Gitea takes the issue");
    let key = made.remote_id.expect("the receipt names the issue");
    let id = format!("gitea:{key}");
    let number = key
        .strip_prefix(&format!("{}/issues/", env.full_name()))
        .unwrap_or_else(|| panic!("the receipt is the issue's key, not its bare number: {key}"))
        .to_owned();

    let incremental = |cursor: String| {
        let source = &source;
        async move {
            let mut sink = VecSink(Vec::new());
            let next = source
                .sync(Some(cursor), &mut sink)
                .await
                .expect("an incremental run");
            (sink.0, next)
        }
    };
    let (items, cursor) = incremental(cursor).await;
    let ticket = of_kind(&items, "ticket")
        .into_iter()
        .find(|t| t.entity.to_string() == id)
        .unwrap_or_else(|| panic!("{id} did not arrive"));
    assert_eq!(ticket.title, title);
    assert_eq!(ticket.payload["state"], "open");

    // 2. Its workflow, then a move along it.
    assert_eq!(source.reachable_transitions(&id).await.unwrap(), ["closed"]);
    source
        .write(WriteOp::Transition {
            entity: id.clone(),
            status: "closed".to_owned(),
        })
        .await
        .expect("Gitea closes the issue");
    let (items, cursor) = incremental(cursor).await;
    let moved = of_kind(&items, "ticket")
        .into_iter()
        .find(|t| t.entity.to_string() == id)
        .unwrap_or_else(|| panic!("the closed {id} did not come back"));
    assert_eq!(moved.payload["state"], "closed");
    assert!(!moved.deleted, "a closed issue is not a deleted one");
    assert_eq!(source.reachable_transitions(&id).await.unwrap(), ["open"]);
    source
        .write(WriteOp::Transition {
            entity: id.clone(),
            status: "open".to_owned(),
        })
        .await
        .expect("Gitea reopens the issue");

    // 3. A comment, read back from the server and by the next run.
    source
        .write(WriteOp::Comment {
            entity: id.clone(),
            body: format!("comment {stamp}"),
        })
        .await
        .expect("Gitea takes the comment");
    let comments: Vec<serde_json::Value> = http
        .get(format!("{api}/issues/{number}/comments"))
        .header("Authorization", &auth)
        .send()
        .await
        .expect("read the discussion")
        .json()
        .await
        .expect("a comment list");
    assert!(
        comments
            .iter()
            .any(|c| c["body"] == format!("comment {stamp}")),
        "{comments:#?}"
    );
    let (items, _) = incremental(cursor).await;
    let back = of_kind(&items, "ticket")
        .into_iter()
        .find(|t| t.entity.to_string() == id)
        .unwrap_or_else(|| panic!("the commented {id} did not come back"));
    assert_eq!(back.payload["state"], "open");
    assert!(back.body_text.contains(&format!("comment {stamp}")));

    clear_test_issues(&http, &env, &auth).await;
    println!("{id} filed, closed, reopened and commented on");
}

/// The item a refresh answers for `id`, or a panic naming what it answered.
async fn refreshed(source: &dyn knobas_source::Source, id: &str) -> SyncItem {
    match source.refresh(id).await {
        Ok(knobas_source::RefreshAnswer::Item(item)) => item,
        other => panic!("{id} refreshed as {other:?}, not as the item"),
    }
}

/// **A refresh reads an issue and a pull request back as Gitea holds them**
/// (#2098, ADR-0031), on items this test makes itself: an issue filed, read
/// back as the next run emits it, commented on and read back with the
/// comment, then deleted at the server and answered *gone*; a pull request
/// opened from a scratch branch, read back as the run emits it, commented on
/// and read back with the comment, then deleted and answered *gone*. A
/// number Gitea never issued is *gone*, and
/// the repository, the branch and its head commit are *not offered*. The
/// battery's clause 13 holds the seeded items of the quiet repositories.
#[tokio::test]
#[ignore = "needs testenv's seeded Gitea container"]
async fn an_issue_and_a_pull_request_are_refreshed_as_gitea_holds_them_and_gone_once_deleted() {
    use knobas_source::{RefreshAnswer, WriteOp};

    let env = env();
    let http = reqwest::Client::new();
    let auth = format!("token {}", env.token);
    let api = format!("{}/api/v1/repos/{}", env.url, env.full_name());
    clear_test_issues(&http, &env, &auth).await;
    let mut litter = Litter::new(&env).await;
    let source = env.one_repo();
    let repo_id = format!("gitea:{}", env.full_name());
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let (_, cursor) = full(&*source).await;

    // The issue: filed, read back as the run emits it.
    let filed = source
        .write(WriteOp::CreateTicket {
            entity: repo_id.clone(),
            title: format!("{TEST_ISSUE}refresh-{stamp}"),
            body: "Filed by the live suite to be refreshed.".to_owned(),
            ticket_type: "Task".to_owned(),
        })
        .await
        .expect("Gitea takes the issue");
    let ticket = format!("gitea:{}", filed.remote_id.expect("the receipt names it"));
    let read = refreshed(&*source, &ticket).await;
    let mut sink = VecSink(Vec::new());
    source
        .sync(Some(cursor), &mut sink)
        .await
        .expect("an incremental run");
    let synced = sink
        .0
        .iter()
        .find(|item| item.entity.to_string() == ticket)
        .unwrap_or_else(|| panic!("the run did not emit {ticket}"));
    assert_eq!(
        (&read.kind, &read.title, &read.body_text, &read.payload),
        (&synced.kind, &synced.title, &synced.body_text, &synced.payload),
        "the refresh of {ticket} is the item the run emits"
    );

    // Commented on: the refresh brings the discussion.
    let words = format!("refreshed comment {stamp}");
    source
        .write(WriteOp::Comment {
            entity: ticket.clone(),
            body: words.clone(),
        })
        .await
        .expect("Gitea takes the comment");
    let read = refreshed(&*source, &ticket).await;
    assert!(read.body_text.contains(&words), "{:?}", read.body_text);
    assert_eq!(read.payload["comments"], 1, "{:#?}", read.payload);

    // Deleted at the server: gone.
    clear_test_issues(&http, &env, &auth).await;
    assert!(
        matches!(source.refresh(&ticket).await, Ok(RefreshAnswer::Gone)),
        "a deleted issue is gone"
    );

    // The pull request, from a scratch branch of this test's own.
    let branch = format!("{}refresh-{stamp}", live_env::LITTER);
    litter.branch_off_main(&branch).await;
    push_file(
        &http,
        &env,
        &branch,
        &format!("{branch}.txt"),
        &format!("{branch}: the work"),
    )
    .await;
    let opened = source
        .write(WriteOp::CreatePullRequest {
            entity: repo_id.clone(),
            title: format!("knobas refresh check {stamp}"),
            body: "Opened to be refreshed.".to_owned(),
            head: branch.clone(),
            base: "main".to_owned(),
        })
        .await
        .expect("Gitea opens the pull request");
    let pull = format!("gitea:{}", opened.remote_id.expect("the receipt names it"));
    let number: u64 = pull
        .rsplit_once('#')
        .and_then(|(_, n)| n.parse().ok())
        .expect("a pull request key");
    litter.will_open_pull(number);
    let read = refreshed(&*source, &pull).await;
    let (items, _) = full(&*source).await;
    let synced = items
        .iter()
        .find(|item| item.entity.to_string() == pull)
        .unwrap_or_else(|| panic!("the run did not emit {pull}"));
    assert_eq!(
        (&read.kind, &read.title, &read.body_text),
        (&synced.kind, &synced.title, &synced.body_text),
        "the refresh of {pull} is the item the run emits"
    );
    for at in [
        "/state",
        "/merged",
        "/head/ref",
        "/head/sha",
        "/base/ref",
        "/base/repo/full_name",
    ] {
        assert_eq!(
            read.payload.pointer(at),
            synced.payload.pointer(at),
            "{pull} at {at}"
        );
    }
    source
        .write(WriteOp::Comment {
            entity: pull.clone(),
            body: words.clone(),
        })
        .await
        .expect("Gitea takes the comment");
    let read = refreshed(&*source, &pull).await;
    assert!(read.body_text.contains(&words), "{:?}", read.body_text);
    let head = read.payload["head"]["sha"]
        .as_str()
        .expect("a head sha")
        .to_owned();

    // Deleted at the server, as an admin deletes one (its issue): gone. A
    // number never issued is gone too.
    let deleted = http
        .delete(format!("{api}/issues/{number}"))
        .header("Authorization", &auth)
        .send()
        .await
        .expect("delete the pull request");
    assert!(deleted.status().is_success(), "{}", deleted.status());
    let unissued = format!("{repo_id}#{}", number + 100_000);
    for id in [&pull, &unissued] {
        assert!(
            matches!(source.refresh(id).await, Ok(RefreshAnswer::Gone)),
            "{id}"
        );
    }

    // The kinds not read by id are not offered.
    for id in [
        repo_id.clone(),
        format!("{repo_id}@refs/heads/{branch}"),
        format!("{repo_id}@{head}"),
    ] {
        assert!(
            matches!(source.refresh(&id).await, Ok(RefreshAnswer::NotOffered)),
            "{id}"
        );
    }
    println!("refreshed {ticket} (then gone) and {pull}");
}

/// The walk reading against the real server (#1379): over the two quiet
/// repositories, a full run and then an unchanged one each report 0, 1, 2 of 2
/// `repositories`, and the unchanged one pushes no item while it does.
#[tokio::test]
#[ignore = "needs testenv's seeded Gitea container"]
async fn a_run_reports_its_walk_over_the_real_repositories_even_when_unchanged() {
    let env = env();
    let source = env.source(serde_json::json!({ "repos": env.quiet_repos() }));
    let of_two: Vec<_> = (0..=2)
        .map(|done| (done, Some(2), "repositories".to_owned()))
        .collect();

    let mut first = Walking::default();
    let cursor = source.sync(None, &mut first).await.expect("full sync");
    let mut unchanged = Walking::default();
    source
        .sync(Some(cursor), &mut unchanged)
        .await
        .expect("unchanged sync");
    println!(
        "walk: full run {} items {:?}; unchanged run {} items {:?}",
        first.items.len(),
        first.readings,
        unchanged.items.len(),
        unchanged.readings
    );
    assert!(!first.items.is_empty());
    assert_eq!(first.readings, of_two);
    assert_eq!(unchanged.items.len(), 0);
    assert_eq!(unchanged.readings, of_two);
}

/// GET `path` under `/api/v1` with the seeded token, as JSON.
async fn api_json(env: &Env, path: &str) -> serde_json::Value {
    let response = reqwest::Client::new()
        .get(format!("{}/api/v1/{path}", env.url))
        .header("Authorization", format!("token {}", env.token))
        .send()
        .await
        .unwrap_or_else(|error| panic!("{path}: {error}"));
    assert!(
        response.status().is_success(),
        "{path}: {}",
        response.status()
    );
    response.json().await.expect("a JSON body")
}

/// #1624: the repository walk pages over `sort=id&order=asc`, the one key a
/// push cannot move, and relies on the server answering in that order across
/// page boundaries. Two to a page, so the seeded corpus spans several.
#[tokio::test]
#[ignore = "needs testenv's seeded Gitea container"]
async fn the_repository_listing_pages_in_ascending_id_order() {
    let env = env();
    let mut ids: Vec<u64> = Vec::new();
    for page in 1.. {
        let body = api_json(
            &env,
            &format!("repos/search?sort=id&order=asc&limit=2&page={page}"),
        )
        .await;
        let data = body["data"].as_array().expect("the {ok,data} envelope");
        if data.is_empty() {
            break;
        }
        ids.extend(
            data.iter()
                .map(|r| r["id"].as_u64().expect("a repository id")),
        );
        assert!(page < 100, "the listing never ended: {ids:?}");
    }
    assert!(
        ids.len() >= 3,
        "the seed creates more than one page of two: {ids:?}"
    );
    assert!(
        ids.windows(2).all(|w| w[0] < w[1]),
        "repositories arrived out of id order: {ids:?}"
    );
}

/// #1624, #2034: a commit walk bounds its window with `since=` and, on a
/// catch-up, `until=`. Gitea 1.27.3 hands both to `git log --since/--until`,
/// on the committer date its `created` carries, so both are server-side and
/// inclusive: for a commit's own `created`, `until=` answers every commit at
/// or before it, it among them, and nothing after, and `since=` the mirror
/// image. Forgejo 17.0.0 declares neither parameter on this endpoint and lists
/// the branch whole whatever they say, which is why the adapter holds both
/// bounds client-side too (`sync::walk_commits`). Checked: which of the two the
/// server under test does, so neither forge's answer can drift unseen.
#[tokio::test]
#[ignore = "needs testenv's seeded Gitea container"]
async fn the_commit_listings_bounds_are_server_side_on_gitea_and_ignored_on_forgejo() {
    let env = env();
    // Forgejo names the Gitea it forked from in its version: measured
    // `17.0.0+gitea-1.22.0`, against Gitea's bare `1.27.3`.
    let version = api_json(&env, "version").await;
    let version = version["version"].as_str().expect("a version").to_owned();
    let forgejo = version.contains("+gitea-");
    let base = format!(
        "repos/{}/commits?sha=main&stat=false&verification=false&files=false&limit=50",
        env.full_name()
    );
    let created = |c: &serde_json::Value| {
        chrono::DateTime::parse_from_rfc3339(c["created"].as_str().expect("created"))
            .expect("RFC 3339")
            .with_timezone(&chrono::Utc)
    };
    let sha = |c: &serde_json::Value| c["sha"].as_str().expect("sha").to_owned();
    let all = api_json(&env, &base).await;
    let all = all.as_array().expect("a commit list");
    let times: std::collections::BTreeSet<_> = all.iter().map(created).collect();
    assert!(
        times.len() >= 2,
        "the seed's main has commits on two seconds: {times:?}"
    );
    // The older of the two newest seconds, so something lies on each side.
    let cut = *times.iter().rev().nth(1).expect("two seconds");
    let at = cut.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let listed = async |bound: &str| -> Vec<String> {
        let body = api_json(&env, &format!("{base}&{bound}={at}")).await;
        body.as_array()
            .expect("a commit list")
            .iter()
            .map(sha)
            .collect()
    };
    let kept = |keep: &dyn Fn(&serde_json::Value) -> bool| -> Vec<String> {
        all.iter().filter(|c| forgejo || keep(c)).map(sha).collect()
    };
    let (to_cut, from_cut) = (kept(&|c| created(c) <= cut), kept(&|c| created(c) >= cut));
    assert!(!to_cut.is_empty() && !from_cut.is_empty());
    assert_eq!(
        listed("until").await,
        to_cut,
        "until={at} on {version}: inclusive and server-side on Gitea, ignored on Forgejo"
    );
    assert_eq!(
        listed("since").await,
        from_cut,
        "since={at} on {version}: inclusive and server-side on Gitea, ignored on Forgejo"
    );
}

/// #2034, the read past the cap on the real server. 1,100 commits pushed at
/// once against a budget of 1,000: the first incremental run delivers the
/// newest 1,000 and is cut, and the next must deliver the other 100. Forgejo
/// ignores `until=`, so that run lists the branch from its head and passes the
/// 1,000 it already delivered first, a whole page cap of them; before #2034
/// it stopped there on every run. Gitea's `until=` keeps those pages from
/// being listed, so on Gitea this is the plain catch-up. Its own organisation
/// repository, removed before and after, so no other test's counts move.
#[tokio::test]
#[ignore = "needs testenv's seeded Gitea container"]
async fn a_commit_catch_up_reads_past_a_page_cap_of_commits_the_server_lists_above_it() {
    const PUSHED: i64 = 1_100;
    let env = env();
    let http = reqwest::Client::builder()
        .timeout(live_env::REQUEST_BUDGET)
        .build()
        .expect("a client with a timeout");
    let api = format!("{}/api/v1", env.url);
    let (admin, password) = TESTENV_ADMIN;
    let scratch = "knobas-live-deep-catch-up";
    let full_name = format!("{}/{scratch}", env.owner);
    let remove = async || {
        let _ = http
            .delete(format!("{api}/repos/{full_name}"))
            .basic_auth(admin, Some(password))
            .send()
            .await;
    };
    remove().await;
    let created = http
        .post(format!("{api}/orgs/{}/repos", env.owner))
        .basic_auth(admin, Some(password))
        .json(&serde_json::json!({ "name": scratch, "auto_init": true, "default_branch": "main" }))
        .send()
        .await
        .expect("POST /orgs/{owner}/repos");
    assert!(
        created.status().is_success(),
        "create the repository: {}",
        created.status()
    );
    let source = env.source(serde_json::json!({
        "repos": [full_name], "commits_per_repo": 1000, "prs_per_repo": 0
    }));
    let (_, cursor) = full(&*source).await;

    // Empty commits a second apart, all after the first one's second, made
    // by `git fast-import` and pushed in one go.
    let dir = std::env::temp_dir().join(format!("knobas-live-deep-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let remote = env
        .url
        .replacen("://", &format!("://{admin}:{password}@"), 1)
        + &format!("/{full_name}.git");
    let git = |args: &[&str], input: Option<&str>| -> String {
        use std::io::Write as _;
        let mut child = std::process::Command::new("git")
            .args(args)
            .current_dir(if args[0] == "clone" {
                std::env::temp_dir()
            } else {
                dir.clone()
            })
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("git runs");
        if let Some(input) = input {
            child
                .stdin
                .take()
                .expect("a stdin")
                .write_all(input.as_bytes())
                .expect("git reads the stream");
        }
        let out = child.wait_with_output().expect("git ends");
        assert!(out.status.success(), "git {args:?}: {}", out.status);
        String::from_utf8(out.stdout)
            .expect("UTF-8")
            .trim()
            .to_owned()
    };
    git(
        &["clone", "-q", &remote, dir.to_str().expect("a UTF-8 path")],
        None,
    );
    let head = git(&["rev-parse", "HEAD"], None);
    let start = chrono::Utc::now().timestamp() + 60;
    let mut stream = String::new();
    for k in 1..=PUSHED {
        let message = format!("knobas live: deep catch-up {k}\n");
        stream += &format!(
            "commit refs/heads/main\ncommitter knobas <knobas@tidewater.example> {} +0000\ndata {}\n{message}",
            start + k,
            message.len()
        );
        if k == 1 {
            stream += &format!("from {head}\n");
        }
        stream += "\n";
    }
    git(&["fast-import", "--quiet"], Some(&stream));
    git(&["push", "-q", "origin", "main"], None);
    let _ = std::fs::remove_dir_all(&dir);

    let mut delivered: Vec<String> = Vec::new();
    let mut cursor = cursor;
    for run in 1..=2 {
        let mut sink = VecSink(Vec::new());
        cursor = source
            .sync(Some(cursor), &mut sink)
            .await
            .expect("an incremental sync");
        let commits = of_kind(&sink.0, "commit");
        println!("LIVE deep catch-up run {run}: {} commits", commits.len());
        delivered.extend(commits.iter().map(|i| i.entity.key.clone()));
        if run == 1 {
            hint_premise(&env, &full_name, &cursor).await;
        }
    }
    let position: serde_json::Value = serde_json::from_str(&cursor).expect("the cursor is JSON");
    remove().await;

    let distinct: std::collections::BTreeSet<&String> = delivered.iter().collect();
    assert_eq!(
        (delivered.len(), distinct.len()),
        (1_100, 1_100),
        "every pushed commit, once each, in two runs"
    );
    assert!(
        position["repos"][&full_name]
            .get("commits_catching_up")
            .is_none(),
        "caught up: {}",
        position["repos"][&full_name]
    );
}

/// #2039, on the cursor the deep catch-up's first run left: the cut fell on
/// page 20 of `main` (the newest 1,000 at 50 a page), and what the next run
/// does with that page rests on what each forge answers under the catch-up's
/// `until=`. Forgejo lists from the head whatever `until=` says, so page 1 is
/// all above the window and page 20 still starts above it: the run goes
/// there. Gitea bounds the listing, so its page 1 starts at or below the
/// window's top and the run never goes to the page. Asserted here, before the
/// next run, while the window is where the cursor says. Panics on a failure
/// before the caller removes its repository, which its next run removes first.
async fn hint_premise(env: &Env, full_name: &str, cursor: &str) {
    let version = api_json(env, "version").await;
    let forgejo = version["version"]
        .as_str()
        .expect("a version")
        .contains("+gitea-");
    let position: serde_json::Value = serde_json::from_str(cursor).expect("the cursor is JSON");
    let catch_up = &position["repos"][full_name]["commits_catching_up"];
    assert_eq!(catch_up["page"], 20, "where the cut fell: {catch_up}");
    let below = catch_up["below"].as_str().expect("a resume point");
    let created = |page: &serde_json::Value| {
        let first = &page.as_array().expect("a commit list")[0];
        chrono::DateTime::parse_from_rfc3339(first["created"].as_str().expect("created"))
            .expect("RFC 3339")
    };
    let below_at = chrono::DateTime::parse_from_rfc3339(below).expect("RFC 3339");
    let listed = |page: u32| {
        format!(
            "repos/{full_name}/commits?sha=main&stat=false&verification=false&files=false&limit=50&until={below}&page={page}"
        )
    };
    let first = created(&api_json(env, &listed(1)).await);
    if forgejo {
        let twentieth = created(&api_json(env, &listed(20)).await);
        assert!(
            first > below_at && twentieth > below_at,
            "Forgejo: pages 1 and 20 start above {below}: {first}, {twentieth}"
        );
    } else {
        assert!(
            first <= below_at,
            "Gitea: page 1 under until= starts at or below {below}: {first}"
        );
    }
}

/// #1716: a branch walk of more than one page asks `git/refs/heads` for every
/// branch name at once, and relies on it being one unpaged answer naming
/// exactly what the paged listing names, at the same heads. Asked with
/// `limit=1`, so a paged endpoint would answer one.
#[tokio::test]
#[ignore = "needs testenv's seeded Gitea container"]
async fn the_branch_refs_name_every_branch_in_one_answer() {
    let env = env();
    let mut listed = std::collections::BTreeMap::new();
    for page in 1.. {
        let body = api_json(
            &env,
            &format!("repos/{}/branches?limit=1&page={page}", env.full_name()),
        )
        .await;
        let batch = body.as_array().expect("a branch list");
        if batch.is_empty() {
            break;
        }
        for b in batch {
            listed.insert(
                b["name"].as_str().expect("a name").to_owned(),
                b["commit"]["id"].as_str().expect("a head").to_owned(),
            );
        }
        assert!(page < 1000, "the listing never ended: {listed:?}");
    }
    assert!(
        listed.len() >= 2,
        "the seed gives {} more than one branch: {listed:?}",
        env.full_name()
    );
    let refs = api_json(
        &env,
        &format!("repos/{}/git/refs/heads?limit=1&page=1", env.full_name()),
    )
    .await;
    let refs: std::collections::BTreeMap<String, String> = refs
        .as_array()
        .expect("an array of refs")
        .iter()
        .map(|r| {
            (
                r["ref"]
                    .as_str()
                    .and_then(|name| name.strip_prefix("refs/heads/"))
                    .expect("a refs/heads/ name")
                    .to_owned(),
                r["object"]["sha"].as_str().expect("a sha").to_owned(),
            )
        })
        .collect();
    assert_eq!(refs, listed);

    // And page 1 counts the whole listing, which is how a walk whose page 2
    // a deletion emptied still knows it was more than one page.
    let first = reqwest::Client::new()
        .get(format!(
            "{}/api/v1/repos/{}/branches?limit=1&page=1",
            env.url,
            env.full_name()
        ))
        .header("Authorization", format!("token {}", env.token))
        .send()
        .await
        .expect("page 1 of the branch listing");
    let total = first
        .headers()
        .get("x-total-count")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<usize>().ok());
    assert_eq!(total, Some(listed.len()), "X-Total-Count on page 1");
}

// -- what the token may write (issue #2030) ----------------------------------

/// Every toolbar action of `view`, and every action in its blocks, with the
/// reason it is drawn disabled.
fn offered(view: &knobas_source::view::SourceView) -> Vec<(String, Option<String>)> {
    use knobas_source::view::Block;
    let blocks = view
        .sections
        .iter()
        .flat_map(|s| &s.blocks)
        .filter_map(|b| match b {
            Block::Actions { actions } => Some(actions),
            _ => None,
        })
        .flatten();
    view.actions
        .iter()
        .chain(blocks)
        .map(|a| (a.label.clone(), a.disabled.clone()))
        .collect()
}

/// What a view reads, every block but its writes: the read-only token must
/// read what a token that may write reads.
fn read_of(view: &knobas_source::view::SourceView) -> Vec<(String, Option<u64>, usize)> {
    view.sections
        .iter()
        .map(|s| (s.title.clone(), s.count, s.blocks.len()))
        .collect()
}

/// A seeded person, as `testenv/seed-gitea.sh` creates them (`PEOPLE_PASS`):
/// an owner of the organisation and no collaborator of the admin's mirror.
/// The fixed local development account, never a real credential.
const TESTENV_READER: (&str, &str) = ("jonas.becker", "tidewater-dev");

/// Issue #2030, spec #1192 story 10, against the real server: a token minted
/// with `read:repository` reads the repository, a branch, a commit, a pull
/// request and a file as a token that may write does, and is offered every
/// write there disabled, saying which scope it lacks; the token that may
/// write is offered none for its scope. An account that may not push to a
/// repository (a seeded person on the admin's mirror) is told so on
/// *New branch…*. The answers `view::grants` reads are measured here: the
/// empty-bodied branch and issue writes, refused for the scope or for the
/// body.
#[tokio::test]
#[ignore = "needs testenv's seeded Gitea container"]
async fn a_token_that_may_only_read_is_offered_its_writes_disabled_saying_why() {
    let env = env();
    let http = reqwest::Client::builder()
        .timeout(live_env::REQUEST_BUDGET)
        .build()
        .expect("a client with a timeout");
    let api = format!("{}/api/v1", env.url);
    let auth = format!("token {}", env.token);
    let (admin, password) = TESTENV_ADMIN;
    let (reader, reader_password) = TESTENV_READER;
    let names = [
        (admin, password, "knobas-live-2030-read"),
        (admin, password, "knobas-live-2030-write"),
        (reader, reader_password, "knobas-live-2030-reader"),
    ];
    let remove = async || {
        for (user, pass, name) in names {
            let _ = http
                .delete(format!("{api}/users/{user}/tokens/{name}"))
                .basic_auth(user, Some(pass))
                .send()
                .await;
        }
    };
    // What the probes could have made: a branch, an issue, a pull request.
    // Counted on both repositories before any token is minted and after the
    // last is removed, so a forge that took `{}` for a create fails here. A
    // listing the repository does not serve (a mirror's pull requests) is
    // compared by its status.
    let mirror = format!("{admin}/ops-runbooks-mirror");
    let footprint = async |repository: &str| -> Vec<(u16, Option<usize>)> {
        let mut counts = Vec::new();
        for listing in [
            "branches?",
            "issues?state=all&type=issues&",
            "pulls?state=all&",
        ] {
            let answer = http
                .get(format!("{api}/repos/{repository}/{listing}limit=1"))
                .header("Authorization", &auth)
                .send()
                .await
                .expect("GET a listing's count");
            let count = answer
                .headers()
                .get("x-total-count")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<usize>().ok());
            counts.push((answer.status().as_u16(), count));
        }
        counts
    };
    let before = (footprint(&env.full_name()).await, footprint(&mirror).await);
    assert!(
        before.0.iter().all(|(_, count)| count.is_some()),
        "the seeded repository counts its branches, issues and pull requests: {before:?}"
    );
    remove().await;
    let mint = async |(user, pass, name): (&str, &str, &str), scopes: &[&str]| -> String {
        let minted: serde_json::Value = http
            .post(format!("{api}/users/{user}/tokens"))
            .basic_auth(user, Some(pass))
            .json(&serde_json::json!({ "name": name, "scopes": scopes }))
            .send()
            .await
            .expect("mint a token")
            .json()
            .await
            .expect("the token record");
        // A failed mint panics here, naming it; the next run's `remove`
        // clears what this one minted.
        minted["sha1"]
            .as_str()
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| panic!("{user} minted no token {name}: {minted}"))
            .to_owned()
    };
    // Every read scope the adapter reads with: a pull request's view reads
    // its discussion off the issue (`read:issue`) and the organisation's
    // labels (`read:organization`).
    let read_only = mint(
        names[0],
        &[
            "read:repository",
            "read:issue",
            "read:organization",
            "read:user",
        ],
    )
    .await;
    let writes = mint(
        names[1],
        &[
            "write:repository",
            "write:issue",
            "read:organization",
            "read:user",
        ],
    )
    .await;
    let account = mint(
        names[2],
        &[
            "write:repository",
            "write:issue",
            "read:organization",
            "read:user",
        ],
    )
    .await;

    // The entities: the seeded open pull request, its head branch and commit,
    // the repository, and its README.
    let full = env.full_name();
    let pulls: serde_json::Value = http
        .get(format!("{api}/repos/{full}/pulls?state=open&limit=50"))
        .header("Authorization", &auth)
        .send()
        .await
        .expect("GET the open pull requests")
        .json()
        .await
        .expect("a pull request listing");
    let sepa = pulls
        .as_array()
        .and_then(|p| {
            p.iter()
                .find(|p| p["title"] == "SEPA retry with exponential backoff")
        })
        .expect("the seed opens the SEPA retry pull request");
    let number = sepa["number"].as_u64().expect("a number");
    let head = sepa["head"]["ref"].as_str().expect("a head").to_owned();
    let sha = sepa["head"]["sha"].as_str().expect("a head sha").to_owned();
    let (owner, repo) = (env.owner.as_str(), env.repo.as_str());
    let id = |key: String| format!("gitea:{key}");
    let entities = [
        id(knobas_source_gitea::keys::repo_key(owner, repo)),
        id(knobas_source_gitea::keys::branch_key(owner, repo, &head)),
        id(knobas_source_gitea::keys::commit_key(owner, repo, &sha)),
        id(knobas_source_gitea::keys::pr_key(owner, repo, number)),
    ];
    let config = serde_json::json!({ "repos": [full] });
    let narrow = env.source_with(&read_only, config.clone());
    let wide = env.source_with(&writes, config);
    let mut seen = Vec::new();
    for entity in &entities {
        let (a, b) = tokio::join!(narrow.view(entity, None), wide.view(entity, None));
        seen.push((
            entity.clone(),
            a.unwrap_or_else(|e| panic!("the read-only token reads {entity}: {e:?}"))
                .expect("drawn"),
            b.unwrap_or_else(|e| panic!("the token that may write reads {entity}: {e:?}"))
                .expect("drawn"),
        ));
    }
    // The README, where *Commit change…* is offered (the Files tab's file).
    let file = "file:README.md";
    let (a, b) = tokio::join!(
        narrow.view(&entities[0], Some(file)),
        wide.view(&entities[0], Some(file))
    );
    seen.push((
        format!("{} {file}", entities[0]),
        a.expect("the README reads").expect("drawn"),
        b.expect("the README reads").expect("drawn"),
    ));

    let tip: serde_json::Value = http
        .get(format!("{api}/repos/{mirror}/commits?limit=1"))
        .header("Authorization", &auth)
        .send()
        .await
        .expect("GET the mirror's commits")
        .json()
        .await
        .expect("a commit listing");
    let mirror_sha = tip[0]["sha"].as_str().expect("a commit").to_owned();
    let outsider = env.source_with(&account, serde_json::json!({ "repos": [mirror] }));
    let pushless = outsider
        .view(&format!("gitea:{mirror}@{mirror_sha}"), None)
        .await;

    remove().await;

    let after = (footprint(&full).await, footprint(&mirror).await);
    assert_eq!(
        after, before,
        "the empty-bodied probes created no branch, issue or pull request"
    );

    for (entity, narrow, wide) in &seen {
        println!("LIVE {entity}: read-only {:?}", offered(narrow));
        println!("LIVE {entity}: may write {:?}", offered(wide));
        assert_eq!(
            read_of(narrow),
            read_of(wide),
            "{entity}: the reads are unchanged"
        );
        let drawn = offered(narrow);
        assert!(!drawn.is_empty(), "{entity} offers writes to draw disabled");
        for (label, why) in &drawn {
            assert!(
                why.as_deref()
                    .is_some_and(|w| w.starts_with("This token cannot write: it lacks the write:")),
                "{entity}: {label} is drawn disabled, saying which scope: {why:?}"
            );
        }
        let labels = |v| offered(v).into_iter().map(|(l, _)| l).collect::<Vec<_>>();
        assert_eq!(
            labels(narrow),
            labels(wide),
            "{entity}: the same writes are drawn"
        );
        for (label, why) in offered(wide) {
            assert!(
                !why.as_deref()
                    .is_some_and(|w| w.starts_with("This token cannot write")),
                "{entity}: {label} is not refused a token that may write: {why:?}"
            );
        }
    }
    let pushless = pushless
        .expect("a reader reads the mirror's commit")
        .expect("drawn");
    assert_eq!(
        offered(&pushless),
        [(
            "New branch…".to_owned(),
            Some("This account cannot push to this repository".to_owned())
        )],
        "an account without push is told so"
    );
}

/// Issue #2054, spec #1192 story 10, against the real server: the writes a
/// shell surface draws outside a view -- a ticket's comment and move, *Create
/// ticket* in the repository, a pull request's comment, approval and merge
/// -- answered by `Source::refused_writes` from the same empty-bodied probes
/// a view sends. A token minted with read scopes only is refused each,
/// naming the scope it lacks; a token that may write is refused none; and
/// the probes leave the repository's branches, issues and pull requests as
/// they were.
#[tokio::test]
#[ignore = "needs testenv's seeded Gitea container"]
async fn a_read_only_token_is_refused_the_writes_drawn_outside_a_view_saying_why() {
    use knobas_source::grants::WriteAsk;

    let env = env();
    let http = reqwest::Client::builder()
        .timeout(live_env::REQUEST_BUDGET)
        .build()
        .expect("a client with a timeout");
    let api = format!("{}/api/v1", env.url);
    let auth = format!("token {}", env.token);
    let (admin, password) = TESTENV_ADMIN;
    let names = ["knobas-live-2054-read", "knobas-live-2054-write"];
    let remove = async || {
        for name in names {
            let _ = http
                .delete(format!("{api}/users/{admin}/tokens/{name}"))
                .basic_auth(admin, Some(password))
                .send()
                .await;
        }
    };
    let full = env.full_name();
    let footprint = async || -> Vec<Option<usize>> {
        let mut counts = Vec::new();
        for listing in [
            "branches?",
            "issues?state=all&type=issues&",
            "pulls?state=all&",
        ] {
            let answer = http
                .get(format!("{api}/repos/{full}/{listing}limit=1"))
                .header("Authorization", &auth)
                .send()
                .await
                .expect("GET a listing's count");
            counts.push(
                answer
                    .headers()
                    .get("x-total-count")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<usize>().ok()),
            );
        }
        counts
    };
    let before = footprint().await;
    assert!(
        before.iter().all(Option::is_some),
        "the seeded repository counts its branches, issues and pull requests: {before:?}"
    );
    remove().await;
    let mint = async |name: &str, scopes: &[&str]| -> String {
        let minted: serde_json::Value = http
            .post(format!("{api}/users/{admin}/tokens"))
            .basic_auth(admin, Some(password))
            .json(&serde_json::json!({ "name": name, "scopes": scopes }))
            .send()
            .await
            .expect("mint a token")
            .json()
            .await
            .expect("the token record");
        // A failed mint panics here, naming it; the next run's `remove`
        // clears what this one minted.
        minted["sha1"]
            .as_str()
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| panic!("{admin} minted no token {name}: {minted}"))
            .to_owned()
    };
    let read_only = mint(
        names[0],
        &[
            "read:repository",
            "read:issue",
            "read:organization",
            "read:user",
        ],
    )
    .await;
    let writes = mint(
        names[1],
        &[
            "write:repository",
            "write:issue",
            "read:organization",
            "read:user",
        ],
    )
    .await;

    // A seeded issue and the seeded open pull request.
    let issues: serde_json::Value = http
        .get(format!(
            "{api}/repos/{full}/issues?state=all&type=issues&limit=1"
        ))
        .header("Authorization", &auth)
        .send()
        .await
        .expect("GET an issue")
        .json()
        .await
        .expect("an issue listing");
    let issue = issues[0]["number"].as_u64().expect("the seed files issues");
    let pulls: serde_json::Value = http
        .get(format!("{api}/repos/{full}/pulls?state=open&limit=1"))
        .header("Authorization", &auth)
        .send()
        .await
        .expect("GET a pull request")
        .json()
        .await
        .expect("a pull request listing");
    let pull = pulls[0]["number"]
        .as_u64()
        .expect("the seed opens a pull request");
    let (owner, repo) = (env.owner.as_str(), env.repo.as_str());
    let id = |key: String| format!("gitea:{key}");
    let issue = id(knobas_source_gitea::keys::issue_key(owner, repo, issue));
    let pull = id(knobas_source_gitea::keys::pr_key(owner, repo, pull));
    let repository = id(knobas_source_gitea::keys::repo_key(owner, repo));
    let ask = |entity: &str, op: &str| WriteAsk {
        entity: entity.to_owned(),
        op: op.to_owned(),
        field: None,
    };
    let asks = [
        (ask(&issue, "comment"), "write:issue"),
        (ask(&issue, "transition"), "write:issue"),
        (ask(&repository, "create_ticket"), "write:issue"),
        (ask(&pull, "comment"), "write:issue"),
        (ask(&pull, "approve"), "write:repository"),
        (ask(&pull, "merge_pull_request"), "write:repository"),
    ];
    let of: Vec<WriteAsk> = asks.iter().map(|(a, _)| a.clone()).collect();
    let config = serde_json::json!({ "repos": [full] });
    let narrow = env.source_with(&read_only, config.clone());
    let wide = env.source_with(&writes, config);
    let (refused, allowed) = tokio::join!(narrow.refused_writes(&of), wide.refused_writes(&of));

    remove().await;
    assert_eq!(
        footprint().await,
        before,
        "the empty-bodied probes created no branch, issue or pull request"
    );

    let refused = refused.expect("the read-only token is answered");
    println!("LIVE refused_writes, read-only: {refused:?}");
    assert_eq!(
        refused
            .iter()
            .map(|r| (r.entity.as_str(), r.op.as_str(), r.reason.as_str()))
            .collect::<Vec<_>>(),
        asks.iter()
            .map(|(a, scope)| (
                a.entity.as_str(),
                a.op.as_str(),
                if *scope == "write:issue" {
                    "This token cannot write: it lacks the write:issue scope"
                } else {
                    "This token cannot write: it lacks the write:repository scope"
                }
            ))
            .collect::<Vec<_>>(),
        "each write is refused a token that may only read, naming the scope"
    );
    let allowed = allowed.expect("the token that may write is answered");
    assert_eq!(allowed, [], "a token that may write is refused nothing");
}

// -- what the token may not read (issue #2042) --------------------------------

/// Issue #2042, spec #1192 story 10, against the real server: a token minted
/// with `read:repository` and `read:user` alone reads the repository, a
/// branch, a commit, a file and a pull request's view, every tab drawn. What
/// it is refused is what Gitea keeps on the issue of the same index: the
/// pull request's *Conversation* opens on a line naming `read:issue` and
/// draws the reviews without the comments; *Add label…* says the token cannot
/// write there (#2030's reason, which takes the place of the labels read's).
/// A full sync on the same token completes and mirrors the pull request
/// without its discussion (`sync::discussion`'s 403). A second token, which
/// may write issues and not read the organisation, is told on *Add label…*
/// that it lacks `read:organization`.
#[tokio::test]
#[ignore = "needs testenv's seeded Gitea container"]
async fn a_token_that_may_not_read_issues_reads_every_view_but_the_discussion() {
    use knobas_source::view::{Block, Tone};
    let env = env();
    let http = reqwest::Client::builder()
        .timeout(live_env::REQUEST_BUDGET)
        .build()
        .expect("a client with a timeout");
    let api = format!("{}/api/v1", env.url);
    let auth = format!("token {}", env.token);
    let (admin, password) = TESTENV_ADMIN;
    let names = ["knobas-live-2042-read", "knobas-live-2042-org"];
    let remove = async || {
        for name in names {
            let _ = http
                .delete(format!("{api}/users/{admin}/tokens/{name}"))
                .basic_auth(admin, Some(password))
                .send()
                .await;
        }
    };
    remove().await;
    let mint = async |name: &str, scopes: &[&str]| -> serde_json::Value {
        http.post(format!("{api}/users/{admin}/tokens"))
            .basic_auth(admin, Some(password))
            .json(&serde_json::json!({ "name": name, "scopes": scopes }))
            .send()
            .await
            .expect("mint a token")
            .json()
            .await
            .expect("the token record")
    };
    let minted = mint(names[0], &["read:repository", "read:user"]).await;
    let token = minted["sha1"].as_str().unwrap_or_default().to_owned();
    let minted_org = mint(names[1], &["read:repository", "write:issue", "read:user"]).await;
    let org_token = minted_org["sha1"].as_str().unwrap_or_default().to_owned();

    let full = env.full_name();
    let pulls: serde_json::Value = http
        .get(format!("{api}/repos/{full}/pulls?state=open&limit=50"))
        .header("Authorization", &auth)
        .send()
        .await
        .expect("GET the open pull requests")
        .json()
        .await
        .expect("a pull request listing");
    let sepa = pulls
        .as_array()
        .and_then(|p| {
            p.iter()
                .find(|p| p["title"] == "SEPA retry with exponential backoff")
        })
        .expect("the seed opens the SEPA retry pull request")
        .clone();
    let number = sepa["number"].as_u64().expect("a number");
    // The comments the token may not read, as the seed's token reads them.
    let said: Vec<serde_json::Value> = http
        .get(format!("{api}/repos/{full}/issues/{number}/comments"))
        .header("Authorization", &auth)
        .send()
        .await
        .expect("GET the discussion")
        .json()
        .await
        .expect("a comment listing");
    // And what the token is answered there, before knobas reads it.
    let refused = http
        .get(format!("{api}/repos/{full}/issues/{number}/comments"))
        .header("Authorization", format!("token {token}"))
        .send()
        .await
        .expect("GET the discussion on the minted token");
    let refused = (
        refused.status().as_u16(),
        refused.text().await.unwrap_or_default(),
    );

    let head = sepa["head"]["ref"].as_str().expect("a head").to_owned();
    let sha = sepa["head"]["sha"].as_str().expect("a head sha").to_owned();
    let (owner, repo) = (env.owner.as_str(), env.repo.as_str());
    let id = |key: String| format!("gitea:{key}");
    let pr = id(knobas_source_gitea::keys::pr_key(owner, repo, number));
    let asked = [
        (id(knobas_source_gitea::keys::repo_key(owner, repo)), None),
        (
            id(knobas_source_gitea::keys::repo_key(owner, repo)),
            Some("file:README.md"),
        ),
        (
            id(knobas_source_gitea::keys::branch_key(owner, repo, &head)),
            None,
        ),
        (
            id(knobas_source_gitea::keys::commit_key(owner, repo, &sha)),
            None,
        ),
        (pr.clone(), None),
        (pr.clone(), Some("files")),
        (pr.clone(), Some("commits")),
        (pr.clone(), Some("conversation")),
    ];
    let config = serde_json::json!({ "repos": [full] });
    let narrow = env.source_with(&token, config.clone());
    let orgless = env.source_with(&org_token, config.clone());
    let wide = env.source(config);
    let mut seen = Vec::new();
    for (entity, section) in &asked {
        let (a, b) = tokio::join!(narrow.view(entity, *section), wide.view(entity, *section));
        seen.push((format!("{entity} {section:?}"), a, b));
    }
    let orgless = orgless.view(&pr, None).await;
    let mut sink = VecSink(Vec::new());
    let synced = narrow.sync(None, &mut sink).await;

    remove().await;

    assert!(!token.is_empty(), "a token was minted: {minted}");
    assert!(!org_token.is_empty(), "a token was minted: {minted_org}");
    assert_eq!(refused.0, 403, "the premise: {refused:?}");
    assert!(
        refused.1.contains("read:issue"),
        "the premise: refused for the issue scope: {refused:?}"
    );
    assert!(
        !said.is_empty(),
        "the seed comments on the SEPA pull request"
    );
    let note = "This token cannot read this pull request's comments: it lacks the read:issue scope";
    for (asked, narrow, wide) in seen {
        let narrow = narrow
            .unwrap_or_else(|e| panic!("the token reads {asked}: {e:?}"))
            .expect("drawn");
        let wide = wide
            .unwrap_or_else(|e| panic!("the seed's token reads {asked}: {e:?}"))
            .expect("drawn");
        let ids = |v: &knobas_source::view::SourceView| {
            v.sections.iter().map(|s| s.id.clone()).collect::<Vec<_>>()
        };
        assert_eq!(ids(&narrow), ids(&wide), "{asked}: every tab is drawn");
        for (n, w) in narrow.sections.iter().zip(&wide.sections) {
            if n.id != "conversation" || !asked.starts_with(&pr) {
                assert_eq!(
                    (n.count, n.blocks.len()),
                    (w.count, w.blocks.len()),
                    "{asked}: {} reads as the seed's token reads it",
                    n.id
                );
                continue;
            }
            println!("LIVE {asked}: conversation {:?}", n.blocks.first());
            let Some(Block::Entities { title: None, rows }) = n.blocks.first() else {
                panic!("{asked}: the tab opens on the line: {n:#?}");
            };
            let drawn: Vec<_> = rows.iter().map(|r| (r.title.as_str(), r.tone)).collect();
            assert_eq!(drawn, [(note, Some(Tone::Muted))], "{asked}");
            assert_eq!(
                n.count.map(|c| c + said.len() as u64),
                w.count,
                "{asked}: the reviews, and none of the {} comments",
                said.len()
            );
        }
        if asked == format!("{pr} None") {
            let label = narrow
                .actions
                .iter()
                .find(|a| a.label == "Add label…")
                .expect("Add label… is drawn");
            println!("LIVE {asked}: Add label… {:?}", label.disabled);
            assert_eq!(
                label.disabled.as_deref(),
                Some("This token cannot write: it lacks the write:issue scope"),
                "{asked}"
            );
        }
    }

    let orgless = orgless
        .expect("a token that may not read the organisation reads the pull request")
        .expect("drawn");
    let label = orgless
        .actions
        .iter()
        .find(|a| a.label == "Add label…")
        .expect("Add label… is drawn");
    println!(
        "LIVE {pr} without read:organization: Add label… {:?}",
        label.disabled
    );
    assert_eq!(
        label.disabled.as_deref(),
        Some(
            "This token cannot read its organisation's labels: it lacks the read:organization scope"
        ),
    );
    let conversation = orgless
        .sections
        .iter()
        .find(|s| s.id == "conversation")
        .expect("a conversation");
    assert!(
        !matches!(conversation.blocks.first(), Some(Block::Entities { .. })),
        "a token that may read issues reads the comments: {conversation:#?}"
    );

    let cursor = synced.expect("a full sync on a token that may not read issues completes");
    assert!(!cursor.is_empty(), "the run ends on a cursor");
    let prs = of_kind(&sink.0, "pr");
    let mirrored = prs
        .iter()
        .find(|i| i.title == "SEPA retry with exponential backoff")
        .expect("the SEPA pull request is mirrored");
    let first = said[0]["body"].as_str().expect("a comment's text");
    assert!(
        !mirrored.body_text.contains(first),
        "mirrored without its discussion: {:?}",
        mirrored.body_text
    );
    assert!(
        of_kind(&sink.0, "ticket").is_empty(),
        "the issue listing is refused too, and skipped"
    );
}

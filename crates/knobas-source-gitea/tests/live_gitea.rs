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
//! # What cannot be asserted here, and why
//!
//! Gitea numbers pull requests from a per-repository counter and git derives
//! object ids from content, so the fixture's `#142` and its `c90d11` **cannot**
//! be dictated by a seed script (testenv/README.md says so at length; the seed
//! burns issue indices to land the numbers, and records what it actually got in
//! `seed-state.json`). These tests therefore assert the key *forms* interfaces
//! §4.2 fixes and the seeded *titles*, never a literal id.

mod live_env;

use knobas_source::SourceError;
use knobas_source::contract::{Fault, VecSink, battery};
use live_env::{Env, env, full, of_kind};

/// A port nothing listens on: bound to learn the number, then dropped.
fn dead_url() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    format!("http://127.0.0.1:{port}")
}

/// A token that never existed. Salted with the process id so a run cannot
/// accidentally collide with a real one.
fn revoked() -> String {
    format!("revoked-{}", std::process::id())
}

#[tokio::test]
#[ignore = "needs testenv's seeded Gitea container"]
async fn passes_the_contract_battery_against_the_real_container() {
    let env = env();
    let scope = serde_json::json!({ "owners": [env.owner.clone()] });
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
#[tokio::test]
#[ignore = "needs testenv's seeded Gitea container"]
async fn a_pull_request_opened_through_the_api_appears_in_the_next_incremental_run() {
    let env = env();
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

    let created = http
        .post(format!("{api}/branches"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "new_branch_name": branch, "old_branch_name": "main" }))
        .send()
        .await
        .expect("create the branch");
    assert!(
        created.status().is_success(),
        "branch: {}",
        created.text().await.unwrap_or_default()
    );

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

    // And the run after it is silent again, byte-identically (battery clause 2
    // on a position this run wrote rather than on a fresh one).
    let mut idle = VecSink(Vec::new());
    let same = source
        .sync(Some(moved.clone()), &mut idle)
        .await
        .expect("idle sync");
    assert!(idle.0.is_empty(), "second run emitted {:?}", idle.0);
    assert_eq!(same, moved);
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
#[tokio::test]
#[ignore = "needs testenv's seeded Gitea container"]
async fn a_commit_pushed_through_the_api_arrives_once_and_only_once() {
    let env = env();
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

    let created = http
        .post(format!("{api}/branches"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "new_branch_name": branch, "old_branch_name": "main" }))
        .send()
        .await
        .expect("create the branch");
    assert!(
        created.status().is_success(),
        "branch: {}",
        created.text().await.unwrap_or_default()
    );

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

    // …and the run after it is silent, which is the inclusive `since=` boundary
    // being closed rather than merely narrow.
    let mut idle = VecSink(Vec::new());
    let same = source
        .sync(Some(moved.clone()), &mut idle)
        .await
        .expect("idle sync");
    assert!(idle.0.is_empty(), "second run emitted {:?}", idle.0);
    assert_eq!(same, moved);
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

/// The page size the adapter asks for, mirrored here because `client::PAGE_SIZE`
/// is crate-private and an integration test cannot see it. A discussion of
/// `PAGE + 1` is the smallest one that only a second request can reach.
const PAGE: usize = 50;

/// Issue #131, against the server that decides it.
///
/// `issue_comments` used to send **no `limit` and no `page`** and read the one
/// answer it got as the whole discussion. Two facts about this container make
/// that a live defect rather than a theoretical one, and only this container
/// can settle either:
///
/// * **A request naming no `limit` is answered `DEFAULT_PAGING_NUM` records** --
///   thirty, on a stock Gitea, with no admin configuration change of any kind.
///   Every discussion past thirty comments was mirrored down to thirty.
/// * **A request naming `limit=50` is answered fifty**, `MAX_RESPONSE_ITEMS`
///   being the ceiling rather than the count -- so the discussion below still
///   needs a second request, and the walk is what makes it.
///
/// So a discussion of `PAGE + 1` crosses both boundaries at once: the fix's
/// explicit `limit` is what carries comments 31 to 50, and its paging is what
/// carries the 51st. Reverting either loses a comment here, and the assertion
/// is on `body_text` -- where interfaces §4.1 puts them -- rather than on the
/// request, because a request carrying a `limit` and a walk that reads one page
/// look identical from the wire.
///
/// **This needs no capped server**, which is why it runs under the default
/// profile: the truncation is what a stock install does today. The *other*
/// half of the rule -- that a page shorter than the `limit` is not the end
/// either -- needs a server capping below what was asked for, and lives on the
/// wiremock fake (`sync::a_server_that_caps_its_pages_short_is_still_walked_to_the_end`)
/// exactly as issue #81's four walks do.
///
/// The discussion is written through Gitea's own API, like the pull request in
/// the test above: `PAGE + 1` comment POSTs is what this test costs, and it is
/// the only way to have a discussion this long in a fixture whose comments the
/// seed writes one at a time.
#[tokio::test]
#[ignore = "needs testenv's seeded Gitea container"]
async fn a_discussion_longer_than_one_page_comes_back_whole() {
    let env = env();
    let source = env.one_repo();
    let (_, cursor) = full(&*source).await;

    // A unique name, so a re-run does not collide with the last one -- and a
    // unique note text, so `contains` cannot be satisfied by an earlier run's
    // pull request.
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let branch = format!("knobas-live-paging-{stamp}");
    let note = |n: usize| format!("knobas live paging note #{n:03} of run {stamp}");
    let api = format!("{}/api/v1/repos/{}", env.url, env.full_name());
    let http = reqwest::Client::new();
    let auth = format!("token {}", env.token);

    let created = http
        .post(format!("{api}/branches"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "new_branch_name": branch, "old_branch_name": "main" }))
        .send()
        .await
        .expect("create the branch");
    assert!(
        created.status().is_success(),
        "branch: {}",
        created.text().await.unwrap_or_default()
    );

    let opened = http
        .post(format!("{api}/pulls"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "head": branch, "base": "main",
            "title": format!("knobas live paging check {stamp}"),
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

    // What the server itself says the discussion is, before the adapter is
    // asked: if this is not `PAGE + 1` the assertions below are about the
    // fixture rather than about the walk.
    let counted = http
        .get(format!("{api}/pulls/{number}"))
        .header("Authorization", &auth)
        .send()
        .await
        .expect("read the pull request back")
        .json::<serde_json::Value>()
        .await
        .expect("Gitea answers a pull request record");
    assert_eq!(
        counted["comments"].as_u64(),
        Some(PAGE as u64 + 1),
        "the seed above did not land {} comments",
        PAGE + 1
    );

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
                sink.0.iter().map(|i| i.entity.to_string()).collect::<Vec<_>>()
            )
        });

    let missing: Vec<usize> = (1..=PAGE + 1)
        .filter(|n| !pr.body_text.contains(&note(*n)))
        .collect();
    assert!(
        missing.is_empty(),
        "the discussion came back truncated: {} of {} comments are missing from body_text, \
         first {:?}. body_text was {} bytes",
        missing.len(),
        PAGE + 1,
        missing.first(),
        pr.body_text.len()
    );
}

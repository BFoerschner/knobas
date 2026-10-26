//! What one run emits, and what it leaves behind in the cursor.

use crate::support::{self, Fake, PAGE, State, TOKEN, branch, instance, source};
use knobas_source::contract::{VecSink, Walking};
use knobas_source::{Source, SourceError, SyncItem};

async fn full(source: &dyn Source) -> (Vec<SyncItem>, String) {
    let mut sink = VecSink(Vec::new());
    let cursor = source.sync(None, &mut sink).await.expect("full sync");
    (sink.0, cursor)
}

async fn again(source: &dyn Source, cursor: &str) -> (Vec<SyncItem>, String) {
    let mut sink = VecSink(Vec::new());
    let next = source
        .sync(Some(cursor.to_owned()), &mut sink)
        .await
        .expect("incremental sync");
    (sink.0, next)
}

fn ids(items: &[SyncItem], kind: &str) -> Vec<String> {
    let mut out: Vec<String> = items
        .iter()
        .filter(|i| i.kind == kind)
        .map(|i| i.entity.to_string())
        .collect();
    out.sort();
    out
}

#[tokio::test]
async fn a_full_sync_emits_the_repository_and_every_branch() {
    let fake = Fake::start(&State::tidewater()).await;
    let source = source(fake.base_url(), serde_json::json!({}));
    let (items, cursor) = full(&*source).await;

    assert_eq!(ids(&items, "repo"), vec!["gitea:tidewater/payout-service"]);
    assert_eq!(
        ids(&items, "branch"),
        vec![
            "gitea:tidewater/payout-service@refs/heads/feature/PAY-231-sepa-retry",
            "gitea:tidewater/payout-service@refs/heads/main",
        ]
    );
    assert!(items.iter().all(|i| !i.deleted));
    // The position records what was seen, so the next run has something to
    // compare against.
    let position: serde_json::Value = serde_json::from_str(&cursor).expect("the cursor is JSON");
    assert_eq!(position["v"], 3);
    let repo = &position["repos"]["tidewater/payout-service"];
    assert_eq!(repo["repo_updated_at"], "2026-08-22T11:42:00Z");
    assert_eq!(
        repo["branches"]["feature/PAY-231-sepa-retry"],
        "c90d11a3f5e2b7c4d9018e6a2b3c4d5e6f708192"
    );
    assert_eq!(
        repo["branches"]["main"],
        "1111111111111111111111111111111111111111"
    );
}

/// A repository's teams (#966): the ones it was added to, by name, written
/// beside Gitea's record where the descriptor declares them. *Owners* holds
/// every repository of its organization, so it says nothing about this one.
#[tokio::test]
async fn a_repository_carries_the_teams_it_was_added_to_and_not_owners() {
    let fake = Fake::start(&State::tidewater()).await;
    let source = source(fake.base_url(), serde_json::json!({}));
    let (items, _) = full(&*source).await;

    let repo = items
        .iter()
        .find(|i| i.kind == "repo")
        .expect("the repository is emitted");
    let declared = knobas_source_gitea::descriptor_template()
        .payload_paths
        .into_iter()
        .find(|p| p.kind == "repo")
        .expect("a repository declares its paths");
    assert_eq!(
        knobas_core::payload::resolve_list(&repo.payload, &declared.teams),
        vec!["Payments".to_owned()]
    );
    assert_eq!(
        knobas_core::payload::resolve_string(&repo.payload, &declared.language).as_deref(),
        Some("Rust")
    );
}

/// A repository whose teams Gitea will not list costs the teams and nothing
/// else (#966): one owned by a user (Gitea answers 405, so no team at all)
/// reads as none, one the token may not ask about (403) as a miss, and both
/// runs succeed.
#[tokio::test]
async fn teams_gitea_will_not_list_name_none_and_the_run_goes_on() {
    let mut state = State::tidewater().with_elsewhere();
    state.teams.insert(
        "elsewhere/unrelated".to_owned(),
        (
            405,
            serde_json::json!({ "message": "repo is not owned by an organization" }),
        ),
    );
    state
        .teams
        .insert("tidewater/payout-service".to_owned(), (403, serde_json::json!({ "message": "token does not have at least one of required scope(s): [read:organization]" })));
    let fake = Fake::start(&state).await;
    let source = source(fake.base_url(), serde_json::json!({}));
    let (items, _) = full(&*source).await;

    let teams_of = |entity: &str| {
        items
            .iter()
            .find(|i| i.entity.to_string() == entity)
            .unwrap_or_else(|| panic!("{entity} is emitted"))
            .payload
            .get("teams")
            .cloned()
    };
    assert_eq!(
        teams_of("gitea:elsewhere/unrelated"),
        Some(serde_json::json!([]))
    );
    assert_eq!(teams_of("gitea:tidewater/payout-service"), None);
}

/// The teams cost one request per repository whose record is written, never
/// one per repository per run (#966): an idle run does not ask for them.
#[tokio::test]
async fn an_idle_run_does_not_ask_for_the_teams() {
    let fake = Fake::start(&State::tidewater()).await;
    let source = source(fake.base_url(), serde_json::json!({}));
    let teams_asked = |paths: &[String]| paths.iter().filter(|p| p.ends_with("/teams")).count();
    let (_, cursor) = full(&*source).await;
    assert_eq!(
        teams_asked(&fake.paths().await),
        1,
        "the full sync reads them once"
    );

    let (items, _) = again(&*source, &cursor).await;
    assert!(items.is_empty(), "idle run emitted {items:?}");
    assert_eq!(
        teams_asked(&fake.paths().await),
        1,
        "the idle run asked for the teams again"
    );
}

/// Interfaces §4.1, battery clause 2: nothing changed, nothing emitted, and the
/// cursor comes back byte-identical -- otherwise a five-minute schedule writes
/// 288 "synced nothing" activity lines a day.
#[tokio::test]
async fn an_idle_run_emits_nothing_and_returns_the_same_bytes() {
    let fake = Fake::start(&State::tidewater()).await;
    let source = source(fake.base_url(), serde_json::json!({}));
    let (_, cursor) = full(&*source).await;

    let (items, next) = again(&*source, &cursor).await;
    assert!(items.is_empty(), "idle run emitted {items:?}");
    assert_eq!(next, cursor);
}

/// The idle case certified the way stream A learned to: **one new item, then
/// idle**. A full sync followed by an idle run is stable even for an adapter
/// with no change gate at all, because the full sync skips nothing -- so it
/// proves nothing. Emitting exactly one changed branch and *then* going quiet
/// is what shows the gate works on a position the run itself wrote.
#[tokio::test]
async fn the_position_after_one_change_is_itself_idle_stable() {
    let mut state = State::tidewater();
    let fake = Fake::start(&state).await;
    let source = source(fake.base_url(), serde_json::json!({}));
    let (_, first) = full(&*source).await;

    state.branches.get_mut("tidewater/payout-service").unwrap()[1] = branch(
        "feature/PAY-231-sepa-retry",
        "2222222222222222222222222222222222222222",
        "PAY-231: bound the jitter",
        "2026-08-22T14:05:00Z",
    );
    fake.remount(&state).await;

    let (changed, second) = again(&*source, &first).await;
    assert_eq!(changed.len(), 1, "{changed:?}");
    assert_ne!(second, first);

    let (quiet, third) = again(&*source, &second).await;
    assert!(quiet.is_empty(), "the run after a change emitted {quiet:?}");
    assert_eq!(third, second);
}

#[tokio::test]
async fn a_moved_branch_head_is_re_emitted_and_an_untouched_one_is_not() {
    let mut state = State::tidewater();
    let fake = Fake::start(&state).await;
    let source = source(fake.base_url(), serde_json::json!({}));
    let (_, cursor) = full(&*source).await;

    state.branches.get_mut("tidewater/payout-service").unwrap()[1] = branch(
        "feature/PAY-231-sepa-retry",
        "2222222222222222222222222222222222222222",
        "PAY-231: bound the jitter",
        "2026-08-22T14:05:00Z",
    );
    fake.remount(&state).await;

    let (items, moved) = again(&*source, &cursor).await;
    assert_eq!(
        ids(&items, "branch"),
        vec!["gitea:tidewater/payout-service@refs/heads/feature/PAY-231-sepa-retry"]
    );
    // `main` did not move, and neither did the repository record.
    assert!(ids(&items, "repo").is_empty(), "{items:?}");
    assert_ne!(
        moved, cursor,
        "the position has to move when something was emitted"
    );
    assert!(
        moved.contains("2222222222222222222222222222222222222222"),
        "{moved}"
    );
}

/// A branch deleted after a merge is a dead link target until something says
/// so. On an **incremental** run the adapter's own tombstone is the only thing
/// that will ever retire the row: the engine's sweep fires after a cursor-less
/// run and no other (`branch` declares `full_sync_exhaustive: true`, which is
/// what covers the cursor-less case).
#[tokio::test]
async fn a_vanished_branch_is_tombstoned() {
    let mut state = State::tidewater();
    let fake = Fake::start(&state).await;
    let source = source(fake.base_url(), serde_json::json!({}));
    let (_, cursor) = full(&*source).await;

    state
        .branches
        .get_mut("tidewater/payout-service")
        .unwrap()
        .retain(|b| b["name"] == "main");
    fake.remount(&state).await;

    let (items, _) = again(&*source, &cursor).await;
    let gone: Vec<&SyncItem> = items.iter().filter(|i| i.deleted).collect();
    assert_eq!(gone.len(), 1, "{items:?}");
    assert_eq!(
        gone[0].entity.to_string(),
        "gitea:tidewater/payout-service@refs/heads/feature/PAY-231-sepa-retry"
    );
    assert_eq!(gone[0].title, "feature/PAY-231-sepa-retry");
    // Nothing else was disturbed: `main` is unchanged and stays quiet.
    assert_eq!(items.len(), 1, "{items:?}");
}

/// An explicit allowlist fetches each repository directly and syncs nothing
/// else, even though the listing would have offered more.
#[tokio::test]
async fn an_explicit_repository_list_is_fetched_directly() {
    let fake = Fake::start(&State::tidewater().with_elsewhere()).await;
    let source = source(
        fake.base_url(),
        serde_json::json!({ "repos": ["tidewater/payout-service"] }),
    );
    let (items, _) = full(&*source).await;
    assert_eq!(ids(&items, "repo"), vec!["gitea:tidewater/payout-service"]);
    // …and the listing endpoint was never asked, which is the point of the
    // allowlist: one request per repository instead of a scan of the instance.
    let asked: Vec<String> = fake.paths().await;
    assert!(
        !asked.iter().any(|p| p.contains("/repos/search")),
        "{asked:?}"
    );
}

#[tokio::test]
async fn an_owner_filter_drops_everything_else() {
    let fake = Fake::start(&State::tidewater().with_elsewhere()).await;
    let filtered = source(
        fake.base_url(),
        serde_json::json!({ "owners": ["tidewater"] }),
    );
    let (items, _) = full(&*filtered).await;
    assert_eq!(ids(&items, "repo"), vec!["gitea:tidewater/payout-service"]);
    // Nothing from the other owner leaked in through the branch pass either.
    assert!(
        items.iter().all(|i| i.entity.key.starts_with("tidewater/")),
        "{items:?}"
    );
    // And without the filter the second repository *is* synced, so the
    // assertion above is about the filter rather than about the fixture.
    let unfiltered = source(fake.base_url(), serde_json::json!({}));
    let (items, _) = full(&*unfiltered).await;
    assert_eq!(
        ids(&items, "repo"),
        vec![
            "gitea:elsewhere/unrelated",
            "gitea:tidewater/payout-service"
        ]
    );
}

/// The token died between runs. The identity preflight is what makes this a
/// credential verdict rather than a silent sync of whatever is public.
#[tokio::test]
async fn a_revoked_token_fails_the_run_rather_than_syncing_a_subset() {
    let fake = Fake::start(&State::tidewater()).await;
    let source =
        knobas_source_gitea::build(instance(fake.base_url(), "revoked", serde_json::json!({})))
            .unwrap();
    let mut sink = VecSink(Vec::new());
    let error = source.sync(None, &mut sink).await.unwrap_err();
    assert!(
        matches!(error, SourceError::Unauthorized { .. }),
        "{error:?}"
    );
    assert!(sink.0.is_empty());
}

/// The preflight's actual job, on a fixture that can show it: an instance that
/// serves its public repositories to anyone. Without `GET /user` first, a run
/// with a dead token succeeds and quietly replaces the mirror with the public
/// subset -- a revoked token silently shrinking the corpus to whatever the
/// instance serves anonymously, reported as a healthy sync.
#[tokio::test]
async fn a_dead_token_against_a_publicly_readable_instance_still_fails() {
    let fake = Fake::start_public(&State::tidewater()).await;
    let revoked =
        knobas_source_gitea::build(instance(fake.base_url(), "revoked", serde_json::json!({})))
            .unwrap();
    let mut sink = VecSink(Vec::new());
    let error = revoked.sync(None, &mut sink).await.unwrap_err();
    assert!(
        matches!(error, SourceError::Unauthorized { .. }),
        "{error:?}"
    );
    assert!(sink.0.is_empty(), "{:?}", sink.0);

    // The fixture really would have served a sync: the same instance with a
    // good token mirrors the repository, so the refusal above is the preflight
    // and not an unreachable server.
    let good = source(fake.base_url(), serde_json::json!({}));
    let (items, _) = full(&*good).await;
    assert!(!items.is_empty());
}

/// Ruling B4, the incremental half: one repository refusing us costs that
/// repository this run, not the run. Its watermarks are kept, so the next run
/// does not refetch it from scratch.
#[tokio::test]
async fn a_forbidden_repository_is_skipped_on_an_incremental_run() {
    let mut state = State::tidewater().with_elsewhere();
    let fake = Fake::start(&state).await;
    let source = source(fake.base_url(), serde_json::json!({}));
    let (_, cursor) = full(&*source).await;

    state.forbidden.insert("elsewhere/unrelated".to_owned());
    state.branches.get_mut("tidewater/payout-service").unwrap()[1] = branch(
        "feature/PAY-231-sepa-retry",
        "2222222222222222222222222222222222222222",
        "PAY-231: bound the jitter",
        "2026-08-22T14:05:00Z",
    );
    fake.remount(&state).await;

    let (items, next) = again(&*source, &cursor).await;
    assert_eq!(
        ids(&items, "branch"),
        vec!["gitea:tidewater/payout-service@refs/heads/feature/PAY-231-sepa-retry"]
    );
    // Nothing was tombstoned for the repository we could not read.
    assert!(items.iter().all(|i| !i.deleted), "{items:?}");
    // Its position survives verbatim, so the next run resumes rather than
    // re-mirroring it.
    let before: serde_json::Value = serde_json::from_str(&cursor).unwrap();
    let after: serde_json::Value = serde_json::from_str(&next).unwrap();
    assert_eq!(
        after["repos"]["elsewhere/unrelated"],
        before["repos"]["elsewhere/unrelated"]
    );
}

/// Ruling B4, the other half: every repository refusing us is a fact about the
/// token's scope, and a green source over an empty mirror is worse than an
/// error.
///
/// The run raises the 403 it was refused with. Since #722 the body says which
/// fault it is: the fake's *user does not have permission* is not Gitea's
/// scope sentence, so it is a refusal (`Protocol`), not `Unauthorized`.
#[tokio::test]
async fn every_repository_refusing_us_raises() {
    let mut state = State::tidewater();
    let fake = Fake::start(&state).await;
    let source = source(fake.base_url(), serde_json::json!({}));
    let (_, cursor) = full(&*source).await;

    state
        .forbidden
        .insert("tidewater/payout-service".to_owned());
    fake.remount(&state).await;

    let mut sink = VecSink(Vec::new());
    let error = source.sync(Some(cursor), &mut sink).await.unwrap_err();
    assert!(
        matches!(
            error,
            SourceError::Protocol {
                status: Some(403),
                ..
            }
        ),
        "{error:?}"
    );
    assert!(sink.0.is_empty());
}

/// A cursor-less run may not report success over a hole. Ruling B4's
/// skip-with-warning is an *incremental* run's option, where the repository's
/// watermarks survive and the next run picks it back up; a cursor-less run has
/// no such state, so the skip would be the only record the repository was ever
/// in scope.
#[tokio::test]
async fn a_forbidden_repository_is_fatal_during_a_full_sync() {
    let mut state = State::tidewater().with_elsewhere();
    state.forbidden.insert("elsewhere/unrelated".to_owned());
    let fake = Fake::start(&state).await;
    let source = source(fake.base_url(), serde_json::json!({}));

    let mut sink = VecSink(Vec::new());
    let error = source.sync(None, &mut sink).await.unwrap_err();
    assert!(
        matches!(
            error,
            SourceError::Protocol {
                status: Some(403),
                ..
            }
        ),
        "{error:?}"
    );

    // The same fixture without the refusal syncs both repositories, so the
    // assertion above is about the refusal and not about the fixture.
    state.forbidden.clear();
    fake.remount(&state).await;
    let (items, _) = full(&*source).await;
    assert_eq!(ids(&items, "repo").len(), 2, "{items:?}");
}

/// A cursor the adapter cannot read is documented as meaning "sync in full"
/// (`GiteaCursor::parse`), so the run that follows one holds no position -- and
/// the ratified fatal-skip rule has to fire for it exactly as it does for
/// `cursor: None`. Deriving that from `cursor.is_none()` instead let an
/// unreadable cursor return `Ok` over a hole.
///
/// The three cases run side by side on one fixture: two ways of holding nothing
/// (both must raise) and one of holding a real position (must skip, per ruling
/// B4). Without the third the test would pass on an adapter that simply raises
/// on every refusal.
#[tokio::test]
async fn a_run_holding_no_position_is_a_full_sync_whatever_cursor_it_was_handed() {
    let mut state = State::tidewater().with_elsewhere();
    let fake = Fake::start(&state).await;
    let source = source(fake.base_url(), serde_json::json!({}));
    let (_, usable) = full(&*source).await;

    state.forbidden.insert("elsewhere/unrelated".to_owned());
    fake.remount(&state).await;

    for (label, cursor) in [
        ("no cursor at all", None),
        (
            "an unrecognised version",
            Some(r#"{"v":9,"repos":{}}"#.into()),
        ),
        ("an unreadable cursor", Some("not json".to_owned())),
        (
            "a cursor naming no repository",
            Some(r#"{"v":3,"repos":{}}"#.into()),
        ),
    ] {
        let mut sink = VecSink(Vec::new());
        let error = source
            .sync(cursor, &mut sink)
            .await
            .map(|c| format!("Ok({c})"))
            .unwrap_err();
        assert!(
            matches!(
                error,
                SourceError::Protocol {
                    status: Some(403),
                    ..
                }
            ),
            "{label}: {error:?}"
        );
    }

    // And the position the run actually wrote is still an incremental run, so
    // ruling B4's skip-with-warning is intact rather than traded away.
    let (items, _) = again(&*source, &usable).await;
    assert!(items.is_empty(), "{items:?}");
}

/// A branch deleted while no usable position was held is never tombstoned **by
/// this adapter**: the tombstone loop diffs against the previous cursor, and a
/// full sync has none.
///
/// That is a division of labour, not a hole, since ADR-0003: a cursor-less run
/// emits every branch of every walked repository and `branch` declares
/// `full_sync_exhaustive: true`, so the engine's sweep retires what this run
/// did not re-emit. Pinned here so the two halves cannot silently swap -- an
/// adapter that started emitting full-sync branch tombstones would be diffing
/// against a position it does not hold, and this test is what says so.
#[tokio::test]
async fn a_run_holding_no_position_cannot_tombstone_a_deleted_branch() {
    let mut state = State::tidewater();
    let fake = Fake::start(&state).await;
    let source = source(fake.base_url(), serde_json::json!({}));
    let (_, cursor) = full(&*source).await;

    state
        .branches
        .get_mut("tidewater/payout-service")
        .unwrap()
        .retain(|b| b["name"] != "feature/PAY-231-sepa-retry");
    fake.remount(&state).await;

    // Holding the position from the first run, the deletion is seen.
    let (incremental, _) = again(&*source, &cursor).await;
    assert_eq!(
        incremental.iter().filter(|i| i.deleted).count(),
        1,
        "the fixture really does delete a branch: {incremental:?}"
    );

    // Holding nothing, the very same deletion is invisible.
    let (cursor_less, _) = full(&*source).await;
    assert_eq!(
        cursor_less.iter().filter(|i| i.deleted).count(),
        0,
        "known limitation: a full sync has nothing to diff against"
    );
}

/// Ruling B4's skip must cost a repository *this run*, never its stored
/// position -- and the `repos[]` allowlist path skips during selection, where
/// the walk loop's careful `next.repos.insert(name, before)` never runs.
///
/// The same fixture is driven down both paths, because the listing path already
/// had a test and the allowlist path is the one that dropped state.
#[tokio::test]
async fn a_refused_repository_keeps_its_position_on_either_selection_path() {
    for config in [
        serde_json::json!({}),
        serde_json::json!({ "repos": ["tidewater/payout-service", "elsewhere/unrelated"] }),
    ] {
        let mut state = State::tidewater().with_elsewhere();
        let fake = Fake::start(&state).await;
        let source = source(fake.base_url(), config.clone());
        let (_, cursor) = full(&*source).await;

        let before: serde_json::Value = serde_json::from_str(&cursor).unwrap();
        assert!(
            before["repos"]["elsewhere/unrelated"]["branches"]["main"].is_string(),
            "{config}: the fixture must have a position to lose: {before}"
        );

        // Something else changes too, so the run emits and therefore writes a
        // *new* cursor rather than handing back the one it was given -- which
        // is the only shape in which the position can be dropped.
        state.forbidden.insert("elsewhere/unrelated".to_owned());
        state.branches.get_mut("tidewater/payout-service").unwrap()[1] = branch(
            "feature/PAY-231-sepa-retry",
            "2222222222222222222222222222222222222222",
            "PAY-231: bound the jitter",
            "2026-08-22T14:05:00Z",
        );
        fake.remount(&state).await;

        let (items, next) = again(&*source, &cursor).await;
        assert!(!items.is_empty(), "{config}: the run must have emitted");
        let after: serde_json::Value = serde_json::from_str(&next).unwrap();
        assert_eq!(
            after["repos"]["elsewhere/unrelated"], before["repos"]["elsewhere/unrelated"],
            "{config}: a refused repository lost its position"
        );
    }
}

/// The compound case the two guards left open: the token dies **after** the
/// first repository was walked. `walked == 0` cannot see it, so the run used to
/// return `Ok` with an advanced cursor over a credential that no longer exists
/// -- and the sources view never offered *Re-enter*.
///
/// The credential's death and one repository's refusal are two different
/// answers -- 401 and 403 -- and until ADR-0004 they arrived at this adapter as
/// one indistinguishable `SourceError::Unauthorized`, so telling them apart
/// cost a second `GET /user` per refusal (`sync::credential_still_good`). The
/// status is carried now and the classification is read straight off it.
///
/// Both selection paths, because they refuse in different places, and each with
/// its control: the identical fixture answering 403 must still *skip*, or this
/// would be a test of "any refusal raises" and would have traded ruling B4 away
/// rather than implemented it.
#[tokio::test]
async fn a_credential_revoked_after_the_first_repository_raises() {
    for config in [
        serde_json::json!({}),
        serde_json::json!({ "repos": ["tidewater/payout-service", "elsewhere/unrelated"] }),
    ] {
        let mut state = State::tidewater().with_elsewhere();
        let fake = Fake::start(&state).await;
        let source = source(fake.base_url(), config.clone());
        let (_, cursor) = full(&*source).await;

        // `elsewhere/unrelated` sorts first and walks fine; the refusal lands on
        // the *second* repository, so `walked == 1` and the run's other guard
        // cannot fire.
        state
            .forbidden
            .insert("tidewater/payout-service".to_owned());

        // Control: a 403. Ruling B4 -- skip, warn, return Ok.
        fake.remount(&state).await;
        let mut sink = VecSink(Vec::new());
        let outcome = source.sync(Some(cursor.clone()), &mut sink).await;
        assert!(
            outcome.is_ok(),
            "{config}: a forbidden repository must still skip, not raise: {outcome:?}"
        );

        // The one condition changed: the same repository answers 401 instead --
        // the token was revoked after the run's identity preflight.
        state.forbidden.clear();
        state.revoked.insert("tidewater/payout-service".to_owned());
        fake.remount(&state).await;
        let mut sink = VecSink(Vec::new());
        let error = source
            .sync(Some(cursor.clone()), &mut sink)
            .await
            .map(|c| format!("Ok({c})"))
            .unwrap_err();
        assert!(
            matches!(error, SourceError::Unauthorized { .. }),
            "{config}: a revoked credential must be the run's verdict: {error:?}"
        );
        assert_eq!(
            error.status(),
            Some(401),
            "{config}: and it must be the 401 that says so, not a 403 believed \
             without checking"
        );
    }
}

/// More than one page of branches: the walk must not stop at `PAGE_SIZE`.
#[tokio::test]
async fn branch_listings_are_paged() {
    let mut state = State::tidewater();
    let many: Vec<serde_json::Value> = (0..PAGE + 2)
        .map(|i| {
            branch(
                &format!("wip/{i:03}"),
                &format!("{i:040}"),
                "work",
                "2026-08-22T09:00:00Z",
            )
        })
        .collect();
    state
        .branches
        .insert("tidewater/payout-service".to_owned(), many);
    let fake = Fake::start_paged(&state).await;
    // `commits_per_repo: 0` scopes this to the branch *listing*. Every branch
    // here is a fixture stub with no commit list mounted behind it, so the
    // commit pass would otherwise spend one request per branch answering
    // nothing -- a property of the fake, not of the walk.
    let source = source(
        fake.base_url(),
        serde_json::json!({ "commits_per_repo": 0 }),
    );
    let (items, _) = full(&*source).await;
    assert_eq!(ids(&items, "branch").len(), PAGE + 2);
}

/// A run that hits a page cap must **fail**, not return `Ok` over a truncated
/// corpus: an `Ok` short of the listing reports a complete mirror of something
/// this run never finished walking.
#[tokio::test]
async fn a_branch_listing_that_would_exceed_the_cap_fails_the_run() {
    let mut state = State::tidewater();
    // One record past the 20 pages of 50 the cap's 21 requests can carry.
    let many: Vec<serde_json::Value> = (0..=20 * PAGE)
        .map(|i| {
            branch(
                &format!("wip/{i:05}"),
                &format!("{i:040}"),
                "work",
                "2026-08-22T09:00:00Z",
            )
        })
        .collect();
    state
        .branches
        .insert("tidewater/payout-service".to_owned(), many);
    let fake = Fake::start_paged(&state).await;
    let source = source(fake.base_url(), serde_json::json!({}));

    let mut sink = VecSink(Vec::new());
    let error = source.sync(None, &mut sink).await.unwrap_err();
    // Names the *branch* cap, so this cannot pass on the repository listing's
    // message instead.
    assert!(
        matches!(error, SourceError::Protocol { message: ref m, .. }
            if m.contains("1001 branches in one repository") && m.contains("never finished walking")),
        "{error:?}"
    );
}

/// The same rule on the repository listing, which is the other unbounded walk.
#[tokio::test]
async fn a_repository_listing_that_would_exceed_the_cap_fails_the_run() {
    let mut state = State::tidewater();
    // One record past the 20 pages of 50 the cap's 21 requests can carry.
    state.repos = (0..=20 * PAGE)
        .map(|i| {
            support::repo(
                "tidewater",
                &format!("service-{i:05}"),
                "2026-08-22T11:42:00Z",
            )
        })
        .collect();
    state.branches.clear();
    let fake = Fake::start_paged(&state).await;
    let source = source(fake.base_url(), serde_json::json!({}));

    let mut sink = VecSink(Vec::new());
    let error = source.sync(None, &mut sink).await.unwrap_err();
    // Names the *repository* cap. `owners[]` alone would not: both messages
    // come from the same `cap_reached` and both name that lever, so asserting
    // it would pass on the branch cap's message too.
    assert!(
        matches!(error, SourceError::Protocol { message: ref m, .. }
            if m.contains("1001 repositories") && m.contains("owners[]")),
        "{error:?}"
    );
}

/// One branch walk of `count` branches against a server that serves the 50 it
/// is asked for.
///
/// `commits_per_repo: 0` scopes it to the branch *listing*. Every branch here
/// is a fixture stub with no commit list mounted behind it, so the commit pass
/// would otherwise spend one request per branch answering nothing -- a property
/// of the fake, not of the walk.
async fn branch_walk_of(count: usize) -> Result<Vec<SyncItem>, SourceError> {
    let mut state = State::tidewater();
    state.branches.insert(
        "tidewater/payout-service".to_owned(),
        (0..count)
            .map(|i| {
                branch(
                    &format!("wip/{i:05}"),
                    &format!("{i:040}"),
                    "work",
                    "2026-08-22T09:00:00Z",
                )
            })
            .collect(),
    );
    let fake = Fake::start_paged(&state).await;
    let source = source(
        fake.base_url(),
        serde_json::json!({ "commits_per_repo": 0 }),
    );
    let mut sink = VecSink(Vec::new());
    source.sync(None, &mut sink).await?;
    Ok(sink.0)
}

/// The cap fires at **exactly** the boundary its message names, not past it:
/// the last request the cap affords came back with something on it, and a
/// non-empty page is not proof there is no page after it. The conservatism is
/// deliberate; what this pins is that the message says the same thing the code
/// does, at the values where a user is most likely to check the arithmetic.
///
/// **The cap's request budget now includes the empty page that proves the
/// end** (issue #81), so the record target costs one more request than it did
/// and `MAX_BRANCH_PAGES` is 21 for a target of 1,000. `20 * PAGE` walks
/// cleanly -- which the short-page rule never managed, because a full page 20
/// could not prove there was no page 21 -- and one record past it is fatal, as
/// is a corpus that fills every request the cap affords.
#[tokio::test]
async fn a_cap_fires_at_exactly_the_boundary_it_names() {
    let clean = branch_walk_of(20 * PAGE)
        .await
        .expect("20 pages of records, and the 21st request proves the end");
    assert_eq!(ids(&clean, "branch").len(), 20 * PAGE);

    // One record more, and the 21st request comes back non-empty instead. The
    // second case fills every request the cap affords, and is the value the
    // message's arithmetic is easiest to check against.
    for count in [20 * PAGE + 1, 21 * PAGE] {
        let error = branch_walk_of(count)
            .await
            .map(|items| format!("Ok({} branches)", ids(&items, "branch").len()))
            .unwrap_err();
        assert!(
            matches!(&error, SourceError::Protocol { message, .. }
                if message.contains(&format!("at least {count} branches"))),
            "{count} must fail, and say accurately what it walked: {error:?}"
        );
    }
}

/// Three of everything, so each of the four paged walks needs more than one
/// page against a server capping at two: three repositories in the listing, and
/// three branches, three pull requests and three commits on one branch inside
/// the first of them.
fn three_of_each() -> State {
    let full = "tidewater/payout-service";
    let mut state = State::tidewater();
    state
        .repos
        .push(support::repo("tidewater", "ledger", "2026-08-20T09:00:00Z"));
    state.repos.push(support::repo(
        "tidewater",
        "settlement",
        "2026-08-19T09:00:00Z",
    ));
    state.branches.get_mut(full).unwrap().push(branch(
        "release/2026-08",
        "5555555555555555555555555555555555555555",
        "cut the August release",
        "2026-08-20T08:00:00Z",
    ));
    state.pulls.get_mut(full).unwrap().insert(
        0,
        support::pull(146, "Retry the retry", "", "2026-08-22T15:00:00Z", 0),
    );
    state
        .commits
        .get_mut(format!("{full}@feature/PAY-231-sepa-retry").as_str())
        .unwrap()
        .insert(
            0,
            support::commit(
                "6666666666666666666666666666666666666666",
                "PAY-231: widen the retry window",
                "2026-08-22T12:10:00Z",
            ),
        );
    state
}

/// Issue #81: a page shorter than the `limit` the adapter asked for is **not**
/// proof the collection ran out. A self-hosted Gitea may answer fewer -- an
/// admin-lowered `MAX_RESPONSE_ITEMS`, a per-endpoint maximum, a partial page
/// under load -- and a walk that stops there reports success while its
/// watermark advances past everything it never saw, which is the silent,
/// watermark-advancing failure ADR-0003 and ruling B4 exist to refuse. An
/// **empty** page is the only unambiguous end.
///
/// Deliberately one test over all four walks rather than four tests: the
/// decision is one rule applied at four sites, and four separate tests would
/// let three of them drift back to `batch.len() < PAGE_SIZE` while the fourth
/// kept the suite green. Reverting any single walk fails exactly one of the
/// assertions below.
///
/// Four, not five: the pull-request discussion is not a paged listing and is
/// not walked. Issue #131 established that against the pinned container -- the
/// endpoint declares no `page` and ignores one -- and
/// `a_discussion_the_server_did_not_send_whole_ends_the_run` is what guards it
/// instead.
#[tokio::test]
async fn a_server_that_caps_its_pages_short_is_still_walked_to_the_end() {
    let state = three_of_each();
    let capped = Fake::start_capped(&state, 2).await;
    let capped_source = source(capped.base_url(), serde_json::json!({}));
    let (items, _) = full(&*capped_source).await;

    // The repository listing. Two per page, three to find.
    assert_eq!(
        ids(&items, "repo"),
        vec![
            "gitea:tidewater/ledger",
            "gitea:tidewater/payout-service",
            "gitea:tidewater/settlement",
        ],
        "the repository listing stopped on a capped page"
    );
    assert_eq!(
        ids(&items, "branch"),
        vec![
            "gitea:tidewater/payout-service@refs/heads/feature/PAY-231-sepa-retry",
            "gitea:tidewater/payout-service@refs/heads/main",
            "gitea:tidewater/payout-service@refs/heads/release/2026-08",
        ],
        "the branch listing stopped on a capped page"
    );
    assert_eq!(
        ids(&items, "pr"),
        vec![
            "gitea:tidewater/payout-service#142",
            "gitea:tidewater/payout-service#144",
            "gitea:tidewater/payout-service#146",
        ],
        "the pull-request walk stopped on a capped page"
    );
    assert_eq!(
        ids(&items, "commit"),
        vec![
            "gitea:tidewater/payout-service@1111111111111111111111111111111111111111",
            "gitea:tidewater/payout-service@6666666666666666666666666666666666666666",
            "gitea:tidewater/payout-service@a41f2c8b7d6e5f403192837465a0b1c2d3e4f506",
            "gitea:tidewater/payout-service@c90d11a3f5e2b7c4d9018e6a2b3c4d5e6f708192",
        ],
        "the commit walk stopped on a capped page"
    );

    // The control: the identical fixture served by a server that honours
    // `limit=50` mirrors exactly the same corpus. Without it these four
    // assertions would be about the fixture rather than about the cap.
    let honest = Fake::start(&state).await;
    let (same, _) = full(&*source(honest.base_url(), serde_json::json!({}))).await;
    for kind in ["repo", "branch", "pr", "commit"] {
        assert_eq!(
            ids(&items, kind),
            ids(&same, kind),
            "{kind}: a capped server must mirror what an uncapped one does"
        );
    }
}

/// One comment body. Zero-padded, so no note's text is a prefix of another's
/// and `contains` cannot report note 5 present because note 51 is.
fn note(n: usize) -> String {
    format!("review note #{n:03}")
}

/// A discussion of `count` comments on the fixture's `#142`, with the pull
/// request's own `comments` count kept honest -- `fetch_comments` skips the
/// request entirely when it reads zero.
fn discussion_of(count: usize) -> State {
    let full = "tidewater/payout-service";
    let mut state = State::tidewater();
    let notes: Vec<serde_json::Value> = (1..=count)
        .map(|n| support::comment(&note(n), "jonas", "2026-08-22T09:30:00Z"))
        .collect();
    state
        .comments
        .insert(format!("{full}#142"), notes)
        .expect("the fixture already has a discussion on #142");
    let pulls = state.pulls.get_mut(full).expect("the fixture has pulls");
    let sepa = pulls
        .iter_mut()
        .find(|p| p["number"] == serde_json::json!(142))
        .expect("the fixture has #142");
    sepa["comments"] = serde_json::json!(count);
    state
}

/// Every comment of `#142` missing from the indexed text.
fn notes_missing_from(items: &[SyncItem], count: usize) -> Vec<usize> {
    let sepa = items
        .iter()
        .find(|i| i.entity.key.ends_with("#142"))
        .expect("the fixture's discussion is on #142");
    (1..=count)
        .filter(|n| !sepa.body_text.contains(&note(*n)))
        .collect()
}

/// Issue #131's real question: a long discussion must reach `body_text` whole.
///
/// The issue expected the answer to be a paged walk. It is not -- Gitea's
/// `issueGetComments` declares no `page` and ignores one, so the discussion
/// arrives in a single response however long it is, and the fake models that
/// (`support::mount_as`). What this pins is the property either design owed:
/// far more comments than the 50 a listing request asks for, and every one of
/// them in the indexed text.
///
/// Asserted on `body_text`, where the comments land (interfaces §4.1: title +
/// description + comment texts), never on the request. A discussion long
/// enough to page on a server that paged is the shape a regression here would
/// take, whichever direction the regression came from.
#[tokio::test]
async fn a_long_discussion_reaches_the_indexed_text_whole() {
    let count = PAGE + 1;
    let state = discussion_of(count);
    let fake = Fake::start(&state).await;
    let source = source(fake.base_url(), serde_json::json!({}));
    let (items, _) = full(&*source).await;

    assert_eq!(
        notes_missing_from(&items, count),
        Vec::<usize>::new(),
        "a discussion of {count} came back truncated"
    );
    // And it cost exactly one request: the endpoint has no second page, and
    // asking for one would re-read the discussion this fake -- like the server
    // it stands for -- serves whole every time.
    let asked = fake.paths().await;
    assert_eq!(
        asked
            .iter()
            .filter(|p| p.ends_with("/issues/142/comments"))
            .count(),
        1,
        "{asked:?}"
    );
}

/// The completeness check, and the only thing standing between a Gitea that
/// pages this endpoint and a mirror that is quietly wrong on every long
/// discussion.
///
/// `issue_comments` reads the whole discussion in one request because the
/// pinned container has no second page to offer -- measured, and re-measured
/// live by `live_gitea::the_discussion_endpoint_does_not_page`. A server that
/// answered fewer records than its own `X-Total-Count` would break that premise
/// silently: `body_text` (interfaces §4.1) would be short on every discussion
/// past the page size, forever, with no error, no warning and a watermark that
/// advanced exactly as it would have. There is no second request to recover
/// with, so the run stops and says so.
///
/// **Fatal, deliberately, and the exception to everything else in
/// `fetch_comments`.** A *refused* discussion warns and carries on -- the test
/// above this one -- because it is one pull request the server said no to. A
/// short one is a fact about the endpoint, and therefore about every discussion
/// the source will ever read. Issue #114 gave TeamCity's unpaged
/// `/app/rest/buildTypes` the same treatment for the same reason.
#[tokio::test]
async fn a_discussion_the_server_did_not_send_whole_ends_the_run() {
    let mut state = discussion_of(4);
    // Four comments served, five claimed: a server that truncated without
    // saying so in the payload.
    state
        .discussion_total
        .insert("tidewater/payout-service#142".to_owned(), 5);
    let fake = Fake::start(&state).await;
    let truncating = source(fake.base_url(), serde_json::json!({}));

    let mut sink = VecSink(Vec::new());
    let error = truncating
        .sync(None, &mut sink)
        .await
        .expect_err("a discussion the server says it truncated cannot be mirrored quietly");
    let SourceError::Protocol { message, .. } = &error else {
        panic!("{error:?}");
    };
    assert!(
        message.contains("4 of its 5 comments")
            && message.contains("X-Total-Count")
            && message.contains("include_pr_comments"),
        "the message must name what was missed, the header it read it from, and the lever \
         that stops asking: {message}"
    );
    // No cursor came back over the gap, so the next run re-reads from where
    // this one stood rather than past it.
    assert!(
        !sink.0.iter().any(|i| i.entity.key.ends_with("#142")),
        "the pull request must not be mirrored with a discussion known to be short"
    );

    // The control: the identical fixture whose header agrees with its body
    // syncs, so the failure above is about the disagreement and not about the
    // discussion being four comments long.
    let honest = Fake::start(&discussion_of(4)).await;
    let (items, _) = full(&*source(honest.base_url(), serde_json::json!({}))).await;
    assert_eq!(notes_missing_from(&items, 4), Vec::<usize>::new());
}

/// The completeness check counts the records the **server sent**, not the
/// comments that parsed -- and the difference is a whole run.
///
/// A comment `model::Comment` cannot read is dropped on purpose: the rest of
/// the discussion is still worth indexing, and a field Gitea adds tomorrow is
/// not a reason to lose a pull request. Count the survivors against
/// `X-Total-Count` instead and that deliberate drop reads as truncation, so
/// the adapter ends the run over a comment it chose to skip -- on a server
/// that sent everything it had. Two lines apart in `fetch_comments`, and no
/// other test tells them apart: a fixture where every record parses passes
/// either way.
#[tokio::test]
async fn an_unreadable_comment_is_dropped_without_reading_as_a_truncation() {
    let mut state = discussion_of(3);
    // The middle record is one no `Comment` can be projected from -- a `body`
    // that is not a string. The header still says three, because three is what
    // the server sent.
    state
        .comments
        .get_mut("tidewater/payout-service#142")
        .expect("the fixture's discussion is on #142")[1] = serde_json::json!({ "body": 42 });
    let fake = Fake::start(&state).await;
    let source = source(fake.base_url(), serde_json::json!({}));

    let (items, _) = full(&*source).await;
    assert_eq!(
        notes_missing_from(&items, 3),
        vec![2],
        "the readable comments are indexed and only the unreadable one is gone"
    );
}

/// A sink that rejects an item aborts the run -- the remaining items are not
/// pushed at it, and no cursor is handed back over the gap (battery clause 6).
#[tokio::test]
async fn a_sink_failure_stops_the_run_where_it_happened() {
    struct FailsAfter(usize);
    #[async_trait::async_trait]
    impl knobas_source::Sink for FailsAfter {
        async fn item(&mut self, _item: SyncItem) -> Result<(), SourceError> {
            if self.0 == 0 {
                return Err(SourceError::Sink("pool closed".to_owned()));
            }
            self.0 -= 1;
            Ok(())
        }
    }

    let fake = Fake::start(&State::tidewater()).await;
    let source = source(fake.base_url(), serde_json::json!({}));
    let mut sink = FailsAfter(1);
    let error = source.sync(None, &mut sink).await.unwrap_err();
    assert!(matches!(error, SourceError::Sink(_)), "{error:?}");
}

/// The `owners` filter is matched case-insensitively, because Gitea owner names
/// are, and a user typing `Tidewater` should not get an empty mirror.
#[tokio::test]
async fn the_owner_filter_ignores_case() {
    let fake = Fake::start(&State::tidewater()).await;
    let source = source(
        fake.base_url(),
        serde_json::json!({ "owners": ["TideWater"] }),
    );
    let (items, _) = full(&*source).await;
    assert_eq!(ids(&items, "repo"), vec!["gitea:tidewater/payout-service"]);
}

/// Every item a run emits is namespaced to the *instance* id, not to the
/// adapter kind -- a second Gitea writes into its own namespace (P10).
#[tokio::test]
async fn a_second_instance_emits_into_its_own_namespace() {
    let fake = Fake::start(&State::tidewater()).await;
    let mut input = instance(fake.base_url(), TOKEN, serde_json::json!({}));
    input.id = "gitea-eu".to_owned();
    let source = knobas_source_gitea::build(input).unwrap();
    let (items, _) = full(&*source).await;
    assert!(!items.is_empty());
    assert!(
        items.iter().all(|i| i.entity.namespace == "gitea-eu"),
        "{items:?}"
    );
}

// ---------------------------------------------------------------- pulls ----

/// Björn's stated pain is bad source-system search; review discussion is
/// exactly the text Jira's and Gitea's own search will not find for him, so it
/// is folded into the indexed body rather than left in the payload.
#[tokio::test]
async fn a_full_sync_emits_pull_requests_with_their_discussion() {
    let fake = Fake::start(&State::tidewater()).await;
    let source = source(fake.base_url(), serde_json::json!({}));
    let (items, _) = full(&*source).await;

    assert_eq!(
        ids(&items, "pr"),
        vec![
            "gitea:tidewater/payout-service#142",
            "gitea:tidewater/payout-service#144",
        ]
    );
    let sepa = items
        .iter()
        .find(|i| i.entity.key.ends_with("#142"))
        .expect("the fixture has #142");
    assert_eq!(sepa.title, "SEPA retry with exponential backoff");
    assert!(
        sepa.body_text.contains("Bounded to +/-10 %"),
        "{}",
        sepa.body_text
    );
    assert_eq!(sepa.author.as_deref(), Some("mara"));
}

/// The exit criterion, in miniature: something changed upstream, and the next
/// incremental run returns exactly that and nothing else.
#[tokio::test]
async fn an_incremental_run_returns_only_the_pull_request_that_moved() {
    let mut state = State::tidewater();
    let fake = Fake::start(&state).await;
    let source = source(fake.base_url(), serde_json::json!({}));
    let (_, cursor) = full(&*source).await;

    state.touch_pull("tidewater/payout-service", 142, "2026-08-22T15:10:00Z");
    fake.remount(&state).await;

    let (items, moved) = again(&*source, &cursor).await;
    assert_eq!(
        ids(&items, "pr"),
        vec!["gitea:tidewater/payout-service#142"]
    );
    assert_eq!(items.len(), 1, "only the pull request moved: {items:?}");
    assert_ne!(moved, cursor);
}

/// Gitea timestamps have one-second resolution. Two pull requests updated in
/// the same second must both be delivered once -- and neither of them again on
/// the next poll, which is what `pulls_seen_at_watermark` is for.
#[tokio::test]
async fn two_pull_requests_on_the_same_second_are_delivered_once_each() {
    let mut state = State::tidewater();
    state.touch_pull("tidewater/payout-service", 142, "2026-08-22T13:50:00Z");
    let fake = Fake::start(&state).await;
    let source = source(fake.base_url(), serde_json::json!({}));

    let (items, cursor) = full(&*source).await;
    assert_eq!(ids(&items, "pr").len(), 2, "{items:?}");
    assert!(cursor.contains("pulls_seen_at_watermark"), "{cursor}");

    let (again_items, again_cursor) = again(&*source, &cursor).await;
    assert!(again_items.is_empty(), "re-delivered {again_items:?}");
    assert_eq!(again_cursor, cursor);
}

/// Issue #764: a pull request merged in the **same second** the previous run
/// read it keeps its `updated_at`, because Gitea stamps it to the second. The
/// next incremental run re-reads the watermark second and must deliver the
/// merge -- and the run after that, with nothing changed, must deliver nothing
/// and hand back the cursor it was given (battery clause 2).
#[tokio::test]
async fn a_pull_request_merged_in_the_watermark_second_is_picked_up() {
    let mut state = State::tidewater();
    let fake = Fake::start(&state).await;
    let source = source(fake.base_url(), serde_json::json!({}));
    let (_, cursor) = full(&*source).await;

    // #144 is the newest, so it stands on the watermark (13:50:00). Merged
    // within that second: state changes, `updated_at` does not.
    let pulls = state
        .pulls
        .get_mut("tidewater/payout-service")
        .expect("the fixture has pull requests");
    let pr = pulls
        .iter_mut()
        .find(|p| p["number"] == 144)
        .expect("the fixture has #144");
    assert_eq!(pr["updated_at"], "2026-08-22T13:50:00Z");
    pr["state"] = serde_json::json!("closed");
    pr["merged"] = serde_json::json!(true);
    fake.remount(&state).await;

    let (items, moved) = again(&*source, &cursor).await;
    assert_eq!(
        ids(&items, "pr"),
        vec!["gitea:tidewater/payout-service#144"],
        "the merge in the watermark second is delivered: {items:?}"
    );
    assert_eq!(items.len(), 1, "only the merged pull request: {items:?}");
    assert_eq!(items[0].payload["merged"], true);
    assert_ne!(moved, cursor, "the run delivered something");

    // Read again, unchanged: nothing written, the same bytes back.
    let (idle, idle_cursor) = again(&*source, &moved).await;
    assert!(idle.is_empty(), "an unchanged re-read emitted {idle:?}");
    assert_eq!(idle_cursor, moved);
}

/// The other direction of #764: what a pull request record **embeds** of its
/// repository (`base.repo`, `head.repo`: the whole repository record on a real
/// Gitea, with its `updated_at` and counters) moves on every push into the
/// repository, and is the repository item's to report. A pull request on the
/// watermark whose own fields stand still is not re-delivered for it. Seen
/// live with the whole record fingerprinted: after a push onto a fresh branch,
/// the five seeded pull requests on the watermark second came again.
#[tokio::test]
async fn a_pull_request_on_the_watermark_is_not_re_delivered_for_its_repository() {
    let mut state = State::tidewater();
    let fake = Fake::start(&state).await;
    let source = source(fake.base_url(), serde_json::json!({}));
    let (_, cursor) = full(&*source).await;

    let pr = state
        .pulls
        .get_mut("tidewater/payout-service")
        .and_then(|pulls| pulls.iter_mut().find(|p| p["number"] == 144))
        .expect("the fixture has #144");
    pr["base"]["repo"]["updated_at"] = serde_json::json!("2026-08-22T14:05:00Z");
    pr["head"]["repo"]["updated_at"] = serde_json::json!("2026-08-22T14:05:00Z");
    fake.remount(&state).await;

    let (items, next) = again(&*source, &cursor).await;
    assert!(ids(&items, "pr").is_empty(), "{items:?}");
    assert_eq!(next, cursor);
}

/// The same hole on the repository record: an edit inside the second the
/// previous run read it leaves `updated_at` where it was.
#[tokio::test]
async fn a_repository_edited_in_the_second_it_was_read_is_picked_up() {
    let mut state = State::tidewater();
    let fake = Fake::start(&state).await;
    let source = source(fake.base_url(), serde_json::json!({}));
    let (_, cursor) = full(&*source).await;

    state.repos[0]["description"] = serde_json::json!("Payouts, now with SEPA retry");
    fake.remount(&state).await;

    let (items, moved) = again(&*source, &cursor).await;
    assert_eq!(
        ids(&items, "repo"),
        vec!["gitea:tidewater/payout-service"],
        "{items:?}"
    );
    assert_eq!(items.len(), 1, "only the repository changed: {items:?}");
    assert_eq!(
        items[0].payload["description"],
        "Payouts, now with SEPA retry"
    );

    let (idle, idle_cursor) = again(&*source, &moved).await;
    assert!(idle.is_empty(), "an unchanged re-read emitted {idle:?}");
    assert_eq!(idle_cursor, moved);
}

/// #764 changed the cursor without a version bump. A cursor written before it
/// -- `pulls_at_watermark` as a list of numbers, no fingerprints -- must still
/// be a position: the next run re-delivers what sits on the watermarks once
/// (the repository and #144 here), not the whole corpus, and is idle after.
#[tokio::test]
async fn a_cursor_from_before_the_fingerprints_is_still_a_position() {
    let state = State::tidewater();
    let fake = Fake::start(&state).await;
    let source = source(fake.base_url(), serde_json::json!({}));
    let (_, cursor) = full(&*source).await;

    let mut legacy: serde_json::Value = serde_json::from_str(&cursor).expect("JSON");
    let repo = legacy["repos"]["tidewater/payout-service"]
        .as_object_mut()
        .expect("the repository has a position");
    assert!(repo.remove("repo_fingerprint").is_some());
    assert!(repo.remove("pulls_seen_at_watermark").is_some());
    repo.insert("pulls_at_watermark".to_owned(), serde_json::json!([144]));
    let legacy = legacy.to_string();

    let (items, upgraded) = again(&*source, &legacy).await;
    let mut kinds: Vec<String> = items.iter().map(|i| i.entity.to_string()).collect();
    kinds.sort();
    assert_eq!(
        kinds,
        vec![
            "gitea:tidewater/payout-service",
            "gitea:tidewater/payout-service#144",
        ],
        "only what sits on a watermark comes again"
    );
    assert!(!upgraded.contains("\"pulls_at_watermark\""), "{upgraded}");

    let (idle, idle_cursor) = again(&*source, &upgraded).await;
    assert!(idle.is_empty(), "{idle:?}");
    assert_eq!(idle_cursor, upgraded);
}

#[tokio::test]
async fn discussion_can_be_left_out_of_the_index() {
    let fake = Fake::start(&State::tidewater()).await;
    let source = source(
        fake.base_url(),
        serde_json::json!({ "include_pr_comments": false }),
    );
    let (items, _) = full(&*source).await;
    let sepa = items
        .iter()
        .find(|i| i.entity.key.ends_with("#142"))
        .expect("the fixture has #142");
    assert!(!sepa.body_text.contains("Bounded to"), "{}", sepa.body_text);
    assert!(sepa.body_text.contains("Retries transient PSP errors."));
    // …and the request was never made, which is what the setting is for.
    let asked = fake.paths().await;
    assert!(
        !asked.iter().any(|p| p.contains("/issues/142/comments")),
        "{asked:?}"
    );
}

/// A budget is a bound on the corpus, not a page size: the newest-updated
/// pull requests are the ones worth mirroring.
#[tokio::test]
async fn the_pull_request_budget_keeps_the_newest() {
    let fake = Fake::start(&State::tidewater()).await;
    let one = source(fake.base_url(), serde_json::json!({ "prs_per_repo": 1 }));
    let (items, _) = full(&*one).await;
    assert_eq!(
        ids(&items, "pr"),
        vec!["gitea:tidewater/payout-service#144"],
        "the budget keeps the most recently updated"
    );

    // Zero mirrors none at all, and costs no request: a fresh fake, so the
    // paths below are this run's alone.
    let quiet = Fake::start(&State::tidewater()).await;
    let none = source(quiet.base_url(), serde_json::json!({ "prs_per_repo": 0 }));
    let (items, _) = full(&*none).await;
    assert!(ids(&items, "pr").is_empty(), "{items:?}");
    let asked = quiet.paths().await;
    assert!(!asked.iter().any(|p| p.ends_with("/pulls")), "{asked:?}");
}

/// A repository with its issue unit switched off answers **404** for the
/// discussion; a token without issue scope answers **403**. Neither is a
/// credential fault, so the pull request is indexed without its comment text and
/// the run carries on.
///
/// The 403 is the case ADR-0004 unlocked: it used to arrive as the same value a
/// revoked token's 401 did, and could only be believed after a second `GET
/// /user` proved the credential alive.
#[tokio::test]
async fn a_pull_request_whose_discussion_is_refused_is_still_indexed() {
    for status in [404, 403] {
        let mut state = State::tidewater();
        state
            .discussion_status
            .insert("tidewater/payout-service#142".to_owned(), status);
        let fake = Fake::start(&state).await;
        let source = source(fake.base_url(), serde_json::json!({}));

        let (items, _) = full(&*source).await;
        let sepa = items
            .iter()
            .find(|i| i.entity.key.ends_with("#142"))
            .unwrap_or_else(|| panic!("{status}: the pull request itself is still emitted"));
        assert!(sepa.body_text.contains("Retries transient PSP errors."));
        assert!(
            !sepa.body_text.contains("Bounded to"),
            "{status}: {}",
            sepa.body_text
        );
        // …and the run reached the pull request that follows it.
        assert!(
            items.iter().any(|i| i.entity.key.ends_with("#144")),
            "{status}: {items:?}"
        );
        // Exactly one request for that discussion: the identity probe that
        // used to follow a refusal is gone (ADR-0004).
        let asked = fake.paths().await;
        assert_eq!(
            asked
                .iter()
                .filter(|p| p.ends_with("/issues/142/comments"))
                .count(),
            1,
            "{status}: {asked:?}"
        );
        assert_eq!(
            asked.iter().filter(|p| p.ends_with("/api/v1/user")).count(),
            1,
            "{status}: only the run's own identity preflight: {asked:?}"
        );
    }
}

/// The hole a plain swallow would leave: a discussion refused **401** is a
/// credential that has died mid-run, and swallowing it would report success
/// with an advanced cursor over a source nobody can read any more.
///
/// The control is the test above, which is the same shape with a 403 and a 404:
/// those cost the discussion and nothing else. Before ADR-0004 all three
/// arrived here as the same value and only a second `GET /user` per refusal
/// separated them.
#[tokio::test]
async fn a_discussion_refused_by_a_dead_credential_fails_the_run() {
    let mut state = State::tidewater();
    state
        .discussion_status
        .insert("tidewater/payout-service#142".to_owned(), 401);
    let fake = Fake::start(&state).await;
    let source = source(fake.base_url(), serde_json::json!({}));

    let mut sink = VecSink(Vec::new());
    let error = source.sync(None, &mut sink).await.unwrap_err();
    assert!(
        matches!(error, SourceError::Unauthorized { .. }),
        "{error:?}"
    );
    assert_eq!(error.status(), Some(401), "{error:?}");
}

/// A pull request deleted between the listing and its discussion read is gone,
/// not a failure (issue #724). Gitea 1.27.3 answers that read **500** `issue
/// does not exist`, so the adapter asks for the pull request itself, and its
/// 404 is what makes the 500 mean "deleted": the pull request is skipped and
/// the run -- a full one, where a hole would be fatal -- goes on and succeeds.
#[tokio::test]
async fn a_pull_request_deleted_mid_run_is_skipped_and_the_run_succeeds() {
    let mut state = State::tidewater();
    state
        .vanished_pulls
        .insert("tidewater/payout-service#142".to_owned());
    let fake = Fake::start(&state).await;
    let source = source(fake.base_url(), serde_json::json!({}));

    let (items, _) = full(&*source).await;
    assert!(
        !items.iter().any(|i| i.entity.key.ends_with("#142")),
        "a deleted pull request is not mirrored: {items:?}"
    );
    assert!(
        items.iter().any(|i| i.entity.key.ends_with("#144")),
        "{items:?}"
    );
    // The walk went on past it, to the commits.
    assert_eq!(ids(&items, "commit").len(), 3, "{items:?}");
}

/// The control: the same 500 on a pull request that is still there is a
/// server fault, and ends the run -- the re-read is what tells the two apart,
/// not the status.
#[tokio::test]
async fn a_failed_discussion_read_of_a_pull_request_still_there_ends_the_run() {
    let mut state = State::tidewater();
    state
        .discussion_status
        .insert("tidewater/payout-service#142".to_owned(), 500);
    let fake = Fake::start(&state).await;
    let source = source(fake.base_url(), serde_json::json!({}));

    let mut sink = VecSink(Vec::new());
    let error = source.sync(None, &mut sink).await.unwrap_err();
    assert_eq!(error.status(), Some(500), "{error:?}");
}

/// A transient fault on both reads -- the discussion and the pull request
/// that would confirm it gone -- is not "deleted": only the re-read's 404 may
/// skip a pull request, so a still-existing one is never dropped for a 5xx and
/// the run ends as it did before #724.
#[tokio::test]
async fn a_fault_on_the_confirming_read_never_skips_a_pull_request() {
    let mut state = State::tidewater();
    state
        .discussion_status
        .insert("tidewater/payout-service#142".to_owned(), 500);
    state
        .confirm_status
        .insert("tidewater/payout-service#142".to_owned(), 503);
    let fake = Fake::start(&state).await;
    let source = source(fake.base_url(), serde_json::json!({}));

    let mut sink = VecSink(Vec::new());
    let error = source.sync(None, &mut sink).await.unwrap_err();
    assert_eq!(error.status(), Some(500), "{error:?}");
}

// --------------------------------------------------------------- commits ----

/// A branch deleted between the listing and its commit read is gone, not a
/// hole (issue #724): merge with delete-branch on a busy repository does it
/// every day. Gitea answers the read 404; the run skips the branch's commits
/// and succeeds, even a full one. The branch row is still the listing's, and
/// the next run that does not list it tombstones it.
#[tokio::test]
async fn a_branch_deleted_mid_run_is_skipped_and_the_run_succeeds() {
    let mut state = State::tidewater();
    state
        .vanished_branches
        .insert("tidewater/payout-service@feature/PAY-231-sepa-retry".to_owned());
    let fake = Fake::start(&state).await;
    let source = source(fake.base_url(), serde_json::json!({}));

    let (items, cursor) = full(&*source).await;
    assert_eq!(
        ids(&items, "commit"),
        vec!["gitea:tidewater/payout-service@1111111111111111111111111111111111111111"],
        "main's history is still walked; the vanished branch's is not"
    );
    assert_eq!(ids(&items, "pr").len(), 2, "{items:?}");

    // The next run no longer sees it listed, and retires it.
    state
        .branches
        .get_mut("tidewater/payout-service")
        .unwrap()
        .retain(|b| b["name"] == "main");
    state.vanished_branches.clear();
    fake.remount(&state).await;
    let (items, _) = again(&*source, &cursor).await;
    let gone: Vec<String> = items
        .iter()
        .filter(|i| i.deleted)
        .map(|i| i.entity.to_string())
        .collect();
    assert_eq!(
        gone,
        vec!["gitea:tidewater/payout-service@refs/heads/feature/PAY-231-sepa-retry"]
    );
}

/// The control: a 404 on the commit read of a branch that still reads is not
/// "deleted" -- the branch's own read is what decides that -- so a full sync
/// ends on it, as it did before #724, rather than mirroring a hole.
#[tokio::test]
async fn a_missing_history_on_a_branch_still_there_ends_a_full_sync() {
    let mut state = State::tidewater();
    state
        .history_missing
        .insert("tidewater/payout-service@feature/PAY-231-sepa-retry".to_owned());
    let fake = Fake::start(&state).await;
    let source = source(fake.base_url(), serde_json::json!({}));

    let mut sink = VecSink(Vec::new());
    let error = source.sync(None, &mut sink).await.unwrap_err();
    assert_eq!(error.status(), Some(404), "{error:?}");
}

/// As for a pull request: a fault on the branch's own read is not "deleted",
/// and a full sync ends on the commit read's 404 rather than skip a branch
/// that may still be there.
#[tokio::test]
async fn a_fault_on_the_confirming_read_never_skips_a_branch() {
    let mut state = State::tidewater();
    state
        .history_missing
        .insert("tidewater/payout-service@feature/PAY-231-sepa-retry".to_owned());
    state.confirm_status.insert(
        "tidewater/payout-service@feature/PAY-231-sepa-retry".to_owned(),
        503,
    );
    let fake = Fake::start(&state).await;
    let source = source(fake.base_url(), serde_json::json!({}));

    let mut sink = VecSink(Vec::new());
    let error = source.sync(None, &mut sink).await.unwrap_err();
    assert_eq!(error.status(), Some(404), "{error:?}");
}

/// The same race on an incremental run: the branch moved, then vanished before
/// its new commits were read. The run succeeds and the rest of the repository
/// is not given up for it -- before #724 the 404 skipped the whole repository.
#[tokio::test]
async fn a_moved_branch_deleted_mid_run_does_not_cost_the_rest_of_the_repository() {
    let mut state = State::tidewater();
    let fake = Fake::start(&state).await;
    let source = source(fake.base_url(), serde_json::json!({}));
    let (_, cursor) = full(&*source).await;

    state.branches.get_mut("tidewater/payout-service").unwrap()[1] = branch(
        "feature/PAY-231-sepa-retry",
        "4444444444444444444444444444444444444444",
        "PAY-231: bound the jitter",
        "2026-08-22T16:20:00Z",
    );
    state
        .vanished_branches
        .insert("tidewater/payout-service@feature/PAY-231-sepa-retry".to_owned());
    state
        .pulls
        .get_mut("tidewater/payout-service")
        .unwrap()
        .insert(
            0,
            support::pull(146, "Fresh one", "", "2026-08-22T16:30:00Z", 0),
        );
    fake.remount(&state).await;

    let (items, next) = again(&*source, &cursor).await;
    assert!(
        items.iter().any(|i| i.entity.key.ends_with("#146")),
        "{items:?}"
    );

    // The position moved past #146: a skipped repository would have kept the
    // old one and delivered #146 again here.
    state
        .branches
        .get_mut("tidewater/payout-service")
        .unwrap()
        .retain(|b| b["name"] == "main");
    state.vanished_branches.clear();
    fake.remount(&state).await;
    let (items, _) = again(&*source, &next).await;
    assert!(
        !items.iter().any(|i| i.entity.key.ends_with("#146")),
        "delivered twice: {items:?}"
    );
    assert_eq!(items.len(), 1, "only the branch's tombstone: {items:?}");
}

#[tokio::test]
async fn a_full_sync_emits_commits_under_their_object_ids() {
    let fake = Fake::start(&State::tidewater()).await;
    let source = source(fake.base_url(), serde_json::json!({}));
    let (items, _) = full(&*source).await;

    assert_eq!(
        ids(&items, "commit"),
        vec![
            "gitea:tidewater/payout-service@1111111111111111111111111111111111111111",
            "gitea:tidewater/payout-service@a41f2c8b7d6e5f403192837465a0b1c2d3e4f506",
            "gitea:tidewater/payout-service@c90d11a3f5e2b7c4d9018e6a2b3c4d5e6f708192",
        ]
    );
    let jitter = items
        .iter()
        .find(|i| {
            i.entity
                .key
                .ends_with("c90d11a3f5e2b7c4d9018e6a2b3c4d5e6f708192")
        })
        .expect("the fixture has c90d11");
    assert_eq!(
        jitter.title,
        "PAY-231: jitter in backoff, cap at 5 attempts"
    );
}

/// The point of keeping branch heads in the cursor: a repository whose branches
/// all stand still costs one listing request and no commit walk at all.
#[tokio::test]
async fn commits_are_not_re_walked_while_the_branch_head_stands_still() {
    let mut state = State::tidewater();
    let fake = Fake::start(&state).await;
    let source = source(fake.base_url(), serde_json::json!({}));
    let (_, cursor) = full(&*source).await;

    // The commit list gains an entry, but no branch head moved -- an upstream
    // state the adapter deliberately does not go looking for.
    state
        .commits
        .get_mut("tidewater/payout-service@main")
        .unwrap()
        .insert(
            0,
            support::commit(
                "3333333333333333333333333333333333333333",
                "unreferenced",
                "2026-08-22T16:00:00Z",
            ),
        );
    fake.remount(&state).await;

    let (items, again_cursor) = again(&*source, &cursor).await;
    assert!(
        items.is_empty(),
        "walked a branch that did not move: {items:?}"
    );
    assert_eq!(again_cursor, cursor);
    // …and the proof it is the *head* that gates the walk, not luck: no commit
    // listing was requested at all on this run.
    let asked = fake.paths().await;
    assert!(!asked.iter().any(|p| p.ends_with("/commits")), "{asked:?}");
}

#[tokio::test]
async fn a_new_commit_on_a_moved_branch_is_emitted_once() {
    let mut state = State::tidewater();
    let fake = Fake::start(&state).await;
    let source = source(fake.base_url(), serde_json::json!({}));
    let (_, cursor) = full(&*source).await;

    let head = "4444444444444444444444444444444444444444";
    state.branches.get_mut("tidewater/payout-service").unwrap()[1] = branch(
        "feature/PAY-231-sepa-retry",
        head,
        "PAY-231: bound the jitter",
        "2026-08-22T16:20:00Z",
    );
    state
        .commits
        .get_mut("tidewater/payout-service@feature/PAY-231-sepa-retry")
        .unwrap()
        .insert(
            0,
            support::commit(head, "PAY-231: bound the jitter", "2026-08-22T16:20:00Z"),
        );
    fake.remount(&state).await;

    let (items, _) = again(&*source, &cursor).await;
    assert_eq!(
        ids(&items, "commit"),
        vec![format!("gitea:tidewater/payout-service@{head}")]
    );
    // The branch moved too, so both are in the run -- and nothing else is.
    assert_eq!(items.len(), 2, "{items:?}");
}

#[tokio::test]
async fn the_commit_budget_bounds_the_mirror() {
    let fake = Fake::start(&State::tidewater()).await;

    let one = source(
        fake.base_url(),
        serde_json::json!({ "commits_per_repo": 1 }),
    );
    let (items, _) = full(&*one).await;
    assert_eq!(ids(&items, "commit").len(), 1, "{items:?}");

    let quiet = Fake::start(&State::tidewater()).await;
    let none = source(
        quiet.base_url(),
        serde_json::json!({ "commits_per_repo": 0 }),
    );
    let (items, _) = full(&*none).await;
    assert!(ids(&items, "commit").is_empty(), "{items:?}");
    let asked = quiet.paths().await;
    assert!(!asked.iter().any(|p| p.ends_with("/commits")), "{asked:?}");
}

/// A commit reachable from two branches is one entity, and must cost one slot
/// of the budget, not two.
#[tokio::test]
async fn a_commit_on_two_branches_is_emitted_once() {
    let mut state = State::tidewater();
    let shared = state
        .commits
        .get("tidewater/payout-service@main")
        .unwrap()
        .clone();
    state
        .commits
        .get_mut("tidewater/payout-service@feature/PAY-231-sepa-retry")
        .unwrap()
        .extend(shared);
    let fake = Fake::start(&state).await;
    let source = source(fake.base_url(), serde_json::json!({}));
    let (items, _) = full(&*source).await;
    assert_eq!(ids(&items, "commit").len(), 3, "{items:?}");
}

/// The carry the budget makes necessary, end to end. `pulls_seen_at_watermark` holds
/// the pull requests already delivered on the boundary second; a run whose
/// budget stops before it re-observes them has to keep them anyway, or the run
/// after it delivers them a second time.
///
/// The unit case is `sync::tests::the_watermark_keeps_every_key_on_the_instant_it_stands_on`;
/// this is the walk that has to reach it with a truncated `examined`.
#[tokio::test]
async fn a_budget_that_stops_short_keeps_the_boundary_it_was_handed() {
    let mut state = State::tidewater();
    // Both fixture pull requests on one second, so the first run leaves two
    // numbers sitting on the watermark.
    state.touch_pull("tidewater/payout-service", 142, "2026-08-22T13:50:00Z");
    let fake = Fake::start(&state).await;
    let (_, cursor) = full(&*source(fake.base_url(), serde_json::json!({}))).await;

    // A third pull request updated in that same second, and a budget that can
    // afford exactly it -- so the walk stops without ever looking at 142/144.
    state
        .pulls
        .get_mut("tidewater/payout-service")
        .unwrap()
        .insert(
            0,
            support::pull(146, "Retry the retry", "", "2026-08-22T13:50:00Z", 0),
        );
    fake.remount(&state).await;

    let tight = source(fake.base_url(), serde_json::json!({ "prs_per_repo": 1 }));
    let (items, moved) = again(&*tight, &cursor).await;
    assert_eq!(
        ids(&items, "pr"),
        vec!["gitea:tidewater/payout-service#146"],
        "the budget affords one, and it is the one that is new"
    );

    // The run after it is silent: 142 and 144 are still on the boundary second
    // and still delivered, though this run's budget never got to them.
    let (idle, same) = again(&*tight, &moved).await;
    assert!(idle.is_empty(), "re-delivered {idle:?}");
    assert_eq!(same, moved);
}

/// Issue #610, the upgrade path for a payload change, last made by #718. An
/// install that synced before #718 holds a version-2 cursor whose branch
/// heads still match the server, so an incremental run under it would
/// re-emit nothing and every mirrored branch would keep a `repository`
/// object without the `default_branch` the Code tile reads (as #595's
/// version 1 kept none at all). The version bump makes that cursor unusable,
/// which turns the first run after the upgrade into a full sync.
///
/// Both halves on one fixture: the same position at version 2 re-emits, and at
/// version 3 it is idle (battery clause 2). Without the second half this would
/// pass on an adapter that ignored its cursor altogether.
#[tokio::test]
async fn a_cursor_from_before_the_repository_payload_re_emits_branches_and_budgeted_commits() {
    let fake = Fake::start(&State::tidewater()).await;
    // A budget below the fixture's three commits, so "inside the budget" is a
    // claim the run can get wrong.
    let source = source(
        fake.base_url(),
        serde_json::json!({ "commits_per_repo": 2 }),
    );
    let (first, current) = full(&*source).await;
    assert_eq!(ids(&first, "commit").len(), 2, "{first:?}");

    // The same position, as a knobas from before #718 stored it.
    let mut old: serde_json::Value = serde_json::from_str(&current).expect("the cursor is JSON");
    assert_eq!(old["v"], 3, "{current}");
    old["v"] = 2.into();
    let old = old.to_string();

    let (items, upgraded) = again(&*source, &old).await;
    assert_eq!(
        ids(&items, "branch"),
        vec![
            "gitea:tidewater/payout-service@refs/heads/feature/PAY-231-sepa-retry",
            "gitea:tidewater/payout-service@refs/heads/main",
        ]
    );
    assert_eq!(ids(&items, "commit"), ids(&first, "commit"));
    for item in items
        .iter()
        .filter(|i| i.kind == "branch" || i.kind == "commit")
    {
        assert_eq!(
            item.payload["repository"],
            serde_json::json!({
                "full_name": "tidewater/payout-service",
                "name": "payout-service",
                "default_branch": "main",
            }),
            "{}",
            item.entity
        );
        assert!(
            !item.body_text.contains("payout-service"),
            "{}: {}",
            item.entity,
            item.body_text
        );
    }
    assert!(upgraded.starts_with(r#"{"v":3"#), "{upgraded}");

    // The same heads at version 3: nothing moved, nothing is emitted.
    let (idle, same) = again(&*source, &current).await;
    assert!(idle.is_empty(), "idle run emitted {idle:?}");
    assert_eq!(same, current);
}

/// A repository whose default branch changes re-emits every branch (#718),
/// though no head moved: each branch's stored `repository` names the default,
/// which the Code tile reads to keep it out of *my branches*, so a branch
/// left holding the old default would be hidden from it. Nothing moved, so
/// no commit is walked; the run after it is idle.
#[tokio::test]
async fn a_changed_default_branch_re_emits_every_branch_and_walks_no_commit() {
    let mut state = State::tidewater();
    let fake = Fake::start(&state).await;
    let source = source(fake.base_url(), serde_json::json!({}));
    let (_, cursor) = full(&*source).await;

    // What Gitea answers once the default is switched in the settings: the
    // record names the new default and its `updated_at` moves.
    state.repos[0]["default_branch"] = "feature/PAY-231-sepa-retry".into();
    state.repos[0]["updated_at"] = "2026-08-23T08:00:00Z".into();
    fake.remount(&state).await;

    let (items, next) = again(&*source, &cursor).await;
    assert_eq!(
        ids(&items, "branch"),
        vec![
            "gitea:tidewater/payout-service@refs/heads/feature/PAY-231-sepa-retry",
            "gitea:tidewater/payout-service@refs/heads/main",
        ]
    );
    for branch in items.iter().filter(|i| i.kind == "branch") {
        assert_eq!(
            branch.payload["repository"]["default_branch"], "feature/PAY-231-sepa-retry",
            "{}",
            branch.entity
        );
    }
    assert!(ids(&items, "commit").is_empty(), "no head moved: {items:?}");

    let (idle, same) = again(&*source, &next).await;
    assert!(idle.is_empty(), "idle run emitted {idle:?}");
    assert_eq!(same, next);
}

/// Gitea answers `/pulls` with a bare `404 {"message":"not found"}` for a
/// repository that cannot have pull requests -- the unit switched off, or a
/// pull mirror -- and a cursor-less run reads a repository-scoped 404 as a
/// hole it may not report success over. So asking ended every first sync of an
/// instance holding one such repository, with `HTTP 404 Not Found: not found`
/// and no word of which. The record says so before anyone asks.
#[tokio::test]
async fn a_repository_that_cannot_have_pull_requests_is_walked_without_asking_for_them() {
    for (field, value) in [("has_pull_requests", false), ("mirror", true)] {
        let mut state = State::tidewater().with_elsewhere();
        state.repos[1][field] = serde_json::json!(value);
        let fake = Fake::start(&state).await;
        let source = source(fake.base_url(), serde_json::json!({}));

        let mut sink = VecSink(Vec::new());
        let outcome = source.sync(None, &mut sink).await;
        assert!(outcome.is_ok(), "{field}={value}: {outcome:?}");
        assert_eq!(ids(&sink.0, "repo").len(), 2, "{field}={value}");
        assert!(
            ids(&sink.0, "branch")
                .contains(&"gitea:elsewhere/unrelated@refs/heads/main".to_owned()),
            "{field}={value}: the rest of the repository is still walked"
        );
        // The repository beside it keeps its pull requests: the gate is per
        // repository, not a pass that switched off.
        assert_eq!(ids(&sink.0, "pr").len(), 2, "{field}={value}");
        assert!(
            !fake
                .paths()
                .await
                .iter()
                .any(|p| p == "/api/v1/repos/elsewhere/unrelated/pulls"),
            "{field}={value}: asked for pull requests the record says cannot exist"
        );
    }
}

/// A repository with no commits yet is the third one Gitea refuses `/pulls`
/// for, and its branch listing is `200 null` rather than `[]`. A first sync of
/// an instance with one freshly created repository must still succeed and
/// mirror it.
#[tokio::test]
async fn an_empty_repository_is_mirrored_on_a_full_sync() {
    let mut state = State::tidewater();
    let mut fresh = support::repo("tidewater", "fresh", "2026-08-23T08:00:00Z");
    fresh["empty"] = serde_json::json!(true);
    state.repos.push(fresh);
    let fake = Fake::start(&state).await;
    let source = source(fake.base_url(), serde_json::json!({}));

    let (items, cursor) = full(&*source).await;
    assert!(
        ids(&items, "repo").contains(&"gitea:tidewater/fresh".to_owned()),
        "{items:?}"
    );
    assert!(
        !fake
            .paths()
            .await
            .iter()
            .any(|p| p.starts_with("/api/v1/repos/tidewater/fresh/pulls")),
        "asked an empty repository for pull requests"
    );
    // And it is idle-stable like any other.
    let (idle, same) = again(&*source, &cursor).await;
    assert!(idle.is_empty(), "idle run emitted {idle:?}");
    assert_eq!(same, cursor);
}

/// A repository the listing offered and that was deleted before its own read
/// is gone, not a hole (issue #728, amending ruling B4 for listed repositories):
/// every read under it answers 404, the repository's own read confirms it, and
/// even a full sync skips it and succeeds, as #724 skips a deleted branch. The
/// rest of the corpus is walked.
#[tokio::test]
async fn a_listed_repository_deleted_mid_run_is_skipped_and_the_run_succeeds() {
    let mut state = State::tidewater().with_elsewhere();
    state.missing.insert("elsewhere/unrelated".to_owned());
    let fake = Fake::start(&state).await;
    let source = source(fake.base_url(), serde_json::json!({}));

    let (items, cursor) = full(&*source).await;
    assert!(
        !items
            .iter()
            .any(|i| i.kind == "branch" && i.entity.key.starts_with("elsewhere/")),
        "nothing under the deleted repository is mirrored: {items:?}"
    );
    assert_eq!(ids(&items, "commit").len(), 3, "{items:?}");
    assert!(
        !cursor.contains("elsewhere/unrelated"),
        "a repository confirmed gone holds no position: {cursor}"
    );
    // Its own row is step 1 of the walk and was emitted before the 404, so the
    // sweep after this run keeps it. The next cursor-less run, whose listing
    // no longer offers it, retires it: the same residual as a repository
    // deleted just after a full sync (module docs, "An incremental run
    // reconciles nothing").
}

/// The same race on an incremental run: before #728 the repository was skipped
/// with its position kept, and if it was the only one, the run raised as
/// "every repository refused". Confirmed gone, it is neither: the run
/// succeeds and the cursor drops it, as the next listing would.
#[tokio::test]
async fn a_listed_repository_deleted_mid_run_drops_out_of_an_incremental_cursor() {
    let mut state = State::tidewater().with_elsewhere();
    let fake = Fake::start(&state).await;
    let source = source(fake.base_url(), serde_json::json!({}));
    let (_, cursor) = full(&*source).await;
    assert!(cursor.contains("elsewhere/unrelated"), "{cursor}");

    state.missing.insert("elsewhere/unrelated".to_owned());
    // The repository still there moves, so the run emits something and writes
    // a new cursor instead of handing back the old one byte-identical
    // (interfaces §4.1). Not the deleted one: its own row would be the only
    // emission, and the test would lean on it.
    state
        .repos
        .iter_mut()
        .find(|r| r["full_name"] == "tidewater/payout-service")
        .expect("tidewater fixture")["updated_at"] = serde_json::json!("2026-08-23T09:00:00Z");
    fake.remount(&state).await;
    let (_, next) = again(&*source, &cursor).await;
    assert!(!next.contains("elsewhere/unrelated"), "{next}");
    assert!(next.contains("tidewater/payout-service"), "{next}");
}

/// The control: a 5xx on the repository's own read -- the one that decides
/// "deleted" -- is not "deleted", so the 404 under it stays a hole and a full
/// sync ends on it, naming the repository.
#[tokio::test]
async fn a_fault_on_the_confirming_read_never_skips_a_repository() {
    let mut state = State::tidewater().with_elsewhere();
    state.missing.insert("elsewhere/unrelated".to_owned());
    state
        .confirm_status
        .insert("elsewhere/unrelated".to_owned(), 503);
    let fake = Fake::start(&state).await;
    let source = source(fake.base_url(), serde_json::json!({}));

    let mut sink = VecSink(Vec::new());
    let error = source.sync(None, &mut sink).await.unwrap_err();
    assert!(
        matches!(error, SourceError::Protocol { status: Some(404), message: ref m }
            if m.contains("elsewhere/unrelated")),
        "{error:?}"
    );
}

/// Only a 404 under a listed repository is read as "maybe deleted". A 403 is
/// a refusal and stays ruling B4's, fatal on a full sync, even when the
/// repository's own read answers 404 by the time it would be asked.
#[tokio::test]
async fn a_refusal_under_a_listed_repository_is_not_read_as_deleted() {
    let mut state = State::tidewater().with_elsewhere();
    state.forbidden.insert("elsewhere/unrelated".to_owned());
    state
        .confirm_status
        .insert("elsewhere/unrelated".to_owned(), 404);
    let fake = Fake::start(&state).await;
    let source = source(fake.base_url(), serde_json::json!({}));

    let mut sink = VecSink(Vec::new());
    let error = source.sync(None, &mut sink).await.unwrap_err();
    assert_eq!(error.status(), Some(403), "{error:?}");
}

/// A refusal that ends a run has to say where it happened. Gitea's 404 body is
/// `not found` for a missing repository and for a missing endpoint alike, so
/// the bare server sentence is a message nobody can act on.
///
/// The repository is one the user **configured**, deleted after selection read
/// it: ruling B4 stands for those even as amended by #728 -- a 404 on a name
/// the user wrote down is a misconfiguration worth failing on, and its own
/// read answering 404 too does not make it a skip.
#[tokio::test]
async fn a_refusal_that_ends_a_full_sync_names_the_repository() {
    let mut state = State::tidewater().with_elsewhere();
    state.missing.insert("elsewhere/unrelated".to_owned());
    state.read_once.insert("elsewhere/unrelated".to_owned());
    let fake = Fake::start(&state).await;
    let source = source(
        fake.base_url(),
        serde_json::json!({ "repos": ["tidewater/payout-service", "elsewhere/unrelated"] }),
    );

    let mut sink = VecSink(Vec::new());
    let error = source.sync(None, &mut sink).await.unwrap_err();
    // The status survives the rewording: it is what classifies the failure.
    assert!(
        matches!(error, SourceError::Protocol { status: Some(404), message: ref m }
            if m.contains("elsewhere/unrelated") && m.contains("not found")),
        "{error:?}"
    );
    // Selection read it, and the walk is where it ended: its branch listing,
    // step 2 of that walk, was asked. (A failed walk's items never reach the
    // sink since #2159, so the sink cannot say this.)
    let asked = fake.paths().await;
    assert!(
        asked
            .iter()
            .any(|p| p == "/api/v1/repos/elsewhere/unrelated/branches"),
        "{asked:?}"
    );
}

/// The wiremock records the sync tests serve fingerprint as they did before
/// #1314 moved the algorithm from this crate into `knobas_source::fingerprint`.
/// The literals are what Gitea's own copy gave them, run on that copy. A value
/// that moves here would re-deliver every record on a watermark once after the
/// upgrade. The pull request's own framing is pinned beside it, in
/// `fingerprint::tests::a_pull_request_keeps_its_fingerprint`.
#[test]
fn fixture_records_keep_their_fingerprints() {
    use knobas_source::fingerprint::of;
    assert_eq!(
        of(&support::repo(
            "tidewater",
            "ledger",
            "2026-08-20T09:00:00Z"
        )),
        "3eb927492dea51d3"
    );
    assert_eq!(
        of(&support::issue(
            142,
            "SEPA retry",
            "open",
            &[],
            "2026-08-22T10:20:00Z"
        )),
        "2ee6d50c1dc29971"
    );
    assert_eq!(
        of(&support::pull(
            146,
            "Retry the retry",
            "",
            "2026-08-22T15:00:00Z",
            0
        )),
        "eaaed275a3f42593"
    );
}

/// The run reports its walk over the repositories it selected: before each
/// one and once after the last, so an unchanged run that pushes no item still
/// moves the bar from 0 to N (#1378, #1379).
#[tokio::test]
async fn a_run_reports_its_walk_over_the_selected_repositories_even_when_it_pushes_nothing() {
    let fake = Fake::start(&State::tidewater().with_elsewhere()).await;
    let source = source(fake.base_url(), serde_json::json!({}));
    let two = |done| (done, Some(2), "repositories".to_owned());

    let mut first = Walking::default();
    let cursor = source.sync(None, &mut first).await.expect("full sync");
    assert!(!first.items.is_empty());
    assert_eq!(first.readings, vec![two(0), two(1), two(2)]);

    let mut idle = Walking::default();
    source
        .sync(Some(cursor), &mut idle)
        .await
        .expect("idle sync");
    assert!(idle.items.is_empty(), "idle run emitted {:?}", idle.items);
    assert_eq!(idle.readings, vec![two(0), two(1), two(2)]);

    // The total is the selection, not the instance: the owner filter leaves one.
    let filtered = support::source(
        fake.base_url(),
        serde_json::json!({ "owners": ["tidewater"] }),
    );
    let mut one = Walking::default();
    filtered.sync(None, &mut one).await.expect("filtered sync");
    let of_one = |done| (done, Some(1), "repositories".to_owned());
    assert_eq!(one.readings, vec![of_one(0), of_one(1)]);
}

/// #2186: an owner taken out of `owners[]` leaves repositories in the cursor
/// the narrowed scope no longer holds. The run walks what is left in full and
/// says it walked the repositories and branches whole, so the engine retires
/// the removed owner's rows; it reads nothing of that owner. An idle run under
/// the same scope says nothing, and a full sync says it too.
#[tokio::test]
async fn an_owner_taken_out_of_the_scope_leaves_the_repositories_walked_whole() {
    let fake = Fake::start(&State::tidewater().with_elsewhere()).await;
    let both = source(fake.base_url(), serde_json::json!({}));
    let mut first = Walking::default();
    let cursor = both.sync(None, &mut first).await.expect("full sync");
    assert_eq!(first.whole, ["repo", "branch"], "a full sync walks both whole");

    let mut idle = Walking::default();
    both.sync(Some(cursor.clone()), &mut idle)
        .await
        .expect("idle sync");
    assert!(idle.items.is_empty() && idle.whole.is_empty(), "{:?}", idle.whole);

    let narrowed = source(
        fake.base_url(),
        serde_json::json!({ "owners": ["tidewater"] }),
    );
    let mut run = Walking::default();
    let after = narrowed
        .sync(Some(cursor), &mut run)
        .await
        .expect("narrowed sync");
    assert_eq!(ids(&run.items, "repo"), vec!["gitea:tidewater/payout-service"]);
    assert!(
        run.items
            .iter()
            .all(|i| i.entity.key.starts_with("tidewater/") && !i.deleted),
        "{:?}",
        run.items
    );
    assert_eq!(run.whole, ["repo", "branch"]);
    let position: serde_json::Value = serde_json::from_str(&after).unwrap();
    assert!(position["repos"].get("elsewhere/unrelated").is_none(), "{after}");

    let mut settled = Walking::default();
    narrowed
        .sync(Some(after), &mut settled)
        .await
        .expect("idle sync");
    assert!(settled.items.is_empty() && settled.whole.is_empty());
}

//! What one run emits, and what it leaves behind in the cursor.

mod support;

use knobas_source::contract::VecSink;
use knobas_source::{Source, SourceError, SyncItem};
use support::{Fake, PAGE, State, TOKEN, branch, instance, source};

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
    assert_eq!(position["v"], 1);
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
/// so -- and since this source declares `full_sync_exhaustive: false`, the
/// engine's sweep never runs for it, so the adapter's own tombstone is the only
/// thing that will ever retire the row.
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
    assert!(matches!(error, SourceError::Unauthorized), "{error:?}");
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
    assert!(matches!(error, SourceError::Unauthorized), "{error:?}");
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
    assert!(matches!(error, SourceError::Unauthorized), "{error:?}");
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
    assert!(matches!(error, SourceError::Unauthorized), "{error:?}");

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
            Some(r#"{"v":1,"repos":{}}"#.into()),
        ),
    ] {
        let mut sink = VecSink(Vec::new());
        let error = source
            .sync(cursor, &mut sink)
            .await
            .map(|c| format!("Ok({c})"))
            .unwrap_err();
        assert!(
            matches!(error, SourceError::Unauthorized),
            "{label}: {error:?}"
        );
    }

    // And the position the run actually wrote is still an incremental run, so
    // ruling B4's skip-with-warning is intact rather than traded away.
    let (items, _) = again(&*source, &usable).await;
    assert!(items.is_empty(), "{items:?}");
}

/// A branch deleted while no usable position was held is **never** tombstoned:
/// the tombstone loop diffs against the previous cursor, and a full sync has
/// none. Recorded in `sync`'s module docs as what `full_sync_exhaustive: false`
/// costs, and pinned here so the doc and the behaviour cannot drift apart --
/// close the gap and this test fails, which is the point.
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
/// first repository was walked. `walked == 0` cannot see it, and 401 arrives
/// indistinguishable from 403, so the run used to return `Ok` with an advanced
/// cursor over a credential that no longer exists -- and the sources view never
/// offered *Re-enter*.
///
/// Both selection paths, because they refuse in different places, and each with
/// its control: the identical fixture with the credential still alive must
/// still *skip*, or this would be a test of "any refusal raises" and would have
/// traded ruling B4 away rather than implemented it.
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

        // Control: credential alive. Ruling B4 -- skip, warn, return Ok.
        fake.remount(&state).await;
        let mut sink = VecSink(Vec::new());
        let outcome = source.sync(Some(cursor.clone()), &mut sink).await;
        assert!(
            outcome.is_ok(),
            "{config}: a live token must still skip, not raise: {outcome:?}"
        );

        // The one condition changed: the token stops working after the run's
        // identity preflight.
        fake.remount_revoked_after_preflight(&state).await;
        let mut sink = VecSink(Vec::new());
        let error = source
            .sync(Some(cursor.clone()), &mut sink)
            .await
            .map(|c| format!("Ok({c})"))
            .unwrap_err();
        assert!(
            matches!(error, SourceError::Unauthorized),
            "{config}: a revoked credential must be the run's verdict: {error:?}"
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
    let source = source(fake.base_url(), serde_json::json!({}));
    let (items, _) = full(&*source).await;
    assert_eq!(ids(&items, "branch").len(), PAGE + 2);
}

/// A run that hits a page cap must **fail**, not return `Ok` over a truncated
/// corpus: an `Ok` short of the listing reports a complete mirror of something
/// this run never finished walking.
#[tokio::test]
async fn a_branch_listing_that_would_exceed_the_cap_fails_the_run() {
    let mut state = State::tidewater();
    // One record past 20 pages of 50.
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
        matches!(error, SourceError::Protocol(ref m)
            if m.contains("1000 branches in one repository") && m.contains("never finished walking")),
        "{error:?}"
    );
}

/// The same rule on the repository listing, which is the other unbounded walk.
#[tokio::test]
async fn a_repository_listing_that_would_exceed_the_cap_fails_the_run() {
    let mut state = State::tidewater();
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
        matches!(error, SourceError::Protocol(ref m)
            if m.contains("1000 repositories") && m.contains("owners[]")),
        "{error:?}"
    );
}

/// The cap fires at **exactly** `20 * PAGE`, not past it: page 20 comes back
/// full and a full page is not proof there is no page 21. The conservatism is
/// deliberate; what this pins is that the message says the same thing the code
/// does, at the one value where "more than 1000" would have been a lie.
#[tokio::test]
async fn a_cap_fires_at_exactly_the_boundary_it_names() {
    let mut state = State::tidewater();
    let exactly: Vec<serde_json::Value> = (0..20 * PAGE)
        .map(|i| {
            branch(
                &format!("wip/{i:05}"),
                &format!("{i:040}"),
                "work",
                "2026-08-22T09:00:00Z",
            )
        })
        .collect();
    assert_eq!(exactly.len(), 1000, "the boundary this test is about");
    state
        .branches
        .insert("tidewater/payout-service".to_owned(), exactly);
    let fake = Fake::start_paged(&state).await;
    let at_boundary = source(fake.base_url(), serde_json::json!({}));

    let mut sink = VecSink(Vec::new());
    let error = at_boundary.sync(None, &mut sink).await.unwrap_err();
    assert!(
        matches!(error, SourceError::Protocol(ref m) if m.contains("at least 1000 branches")),
        "exactly 1000 must fail, and say so accurately: {error:?}"
    );

    // One under the boundary walks cleanly, so the assertion above is about the
    // boundary and not about a fixture that could never have succeeded.
    let mut under = State::tidewater();
    under.branches.insert(
        "tidewater/payout-service".to_owned(),
        (0..20 * PAGE - 1)
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
    let fake = Fake::start_paged(&under).await;
    let under_boundary = source(fake.base_url(), serde_json::json!({}));
    let (items, _) = full(&*under_boundary).await;
    assert_eq!(ids(&items, "branch").len(), 20 * PAGE - 1);
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

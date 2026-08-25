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
/// so. The engine's full-sync sweep would only catch it on the next full run.
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
/// subset -- and because this source declares its full sync exhaustive, the
/// engine then tombstones everything the token used to be able to see.
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

/// A cursor-less run may not report success over a hole. The descriptor claims
/// `full_sync_exhaustive`, so the engine reads a successful full sync as
/// permission to tombstone every row it did not re-emit -- which for a skipped
/// repository is that repository's whole corpus.
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
/// corpus: this source declares its full sync exhaustive, so an `Ok` short of
/// the corpus authorises the engine to tombstone everything past the cap.
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
    assert!(
        matches!(error, SourceError::Protocol(ref m) if m.contains("tombstone")),
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
    assert!(
        matches!(error, SourceError::Protocol(ref m) if m.contains("owners[]")),
        "{error:?}"
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

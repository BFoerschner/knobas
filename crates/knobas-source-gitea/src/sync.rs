//! One sync run.
//!
//! Order is fixed and deterministic -- repository, its branches, its pull
//! requests (Task 6), its commits (Task 7), repositories sorted by full name --
//! so a test can assert on what came out and a budget spends itself the same
//! way twice.
//!
//! # Why every kind has a change gate
//!
//! Battery clause 2 requires that a run which changed nothing emits nothing and
//! returns its cursor unchanged. Gitea offers no "modified since" filter for
//! repositories or branches, so the gate is on this side: a repository is
//! emitted when its `updated_at` differs from the one in the cursor, a branch
//! when its head object id does. Without those gates every poll would re-emit
//! the whole corpus and the activity log would fill with runs that changed
//! nothing.
//!
//! # Why a truncated run fails instead of succeeding
//!
//! The rule is the promise this adapter keeps to everything downstream:
//! **what one run set out to walk, it walked completely.** A budget the user
//! configured is a known bound; a page cap or a refusal hit mid-walk is a hole
//! the user never asked for, and a mirror that is silently missing 400
//! repositories is worse than a run that says so.
//!
//! Since ADR-0003 this is load-bearing rather than merely tidy. `repo` and
//! `branch` declare `full_sync_exhaustive: true`, so the engine's sweep
//! tombstones every repo and branch row a cursor-less run did not re-emit --
//! and the two guards below are exactly what make "did not re-emit" mean
//! "gone" instead of "we stopped early". A truncated walk that returned `Ok`
//! would hand the sweep a corpus it never saw.
//!
//! Two things follow, and both are the same rule:
//!
//! * **A page cap that is reached ends the run with an error.** Returning `Ok`
//!   after walking 1,000 of 1,400 repositories would report a complete mirror
//!   of a corpus that was never walked.
//! * **A repository skipped during a full sync is fatal too.** Ruling B4
//!   grants skip-with-warning for a 403 or 404 on one repository, and that is
//!   what an *incremental* run does -- the repository's watermarks are kept, so
//!   the next run picks up where this one left off. A run holding no position
//!   has no such state to fall back on: the skip would be the only record that
//!   the repository was ever in scope. A repository genuinely deleted upstream
//!   never takes this path -- it simply stops appearing in the listing.
//!
//!   **"Full sync" here is about the position, not the argument.** `cursor:
//!   None` is one way to arrive with nothing; an unreadable cursor, a cursor
//!   from another version, and a cursor naming no repository are the others,
//!   and `GiteaCursor::parse` documents the first two as meaning exactly "sync
//!   in full". `run` therefore derives the flag from what it recovered rather
//!   than from `cursor.is_none()`, or the ratified rule would not fire on three
//!   of its four cases.
//!
//! Which kind is bounded and which is not is declared in
//! `crate::entity_kinds`, and pinned against the config schema by
//! `crate::tests::exactly_the_unbudgeted_kinds_are_exhaustive` -- so a budget
//! added for repositories or branches fails there rather than quietly
//! licensing a sweep over a bounded walk.
//!
//! # Hard deletes: two holes closed, two left open
//!
//! Before ADR-0003 this adapter declared one per-source `false` and **nothing**
//! retired a row it stopped returning -- not the engine, and not this adapter.
//! The per-kind declaration closes the two that mattered:
//!
//! * **A repository** that stops appearing in the listing is now retired by the
//!   engine's sweep after any cursor-less run **that emitted at least one
//!   repository**. (Its cursor entry was already dropped the next time the run
//!   emitted anything; the live `repo` row is what had no way to go.) The
//!   qualifier is the engine's emptiness guard and it is load-bearing here: a
//!   listing that came back empty is exactly what a token which quietly lost
//!   its repo scope returns, so a source whose *last* repository is deleted --
//!   or whose `owners[]` narrows to nothing -- keeps that row, and the branch
//!   rows under it, indefinitely. See `knobas_sync::run_once`, *Limitations*.
//! * **A branch** is tombstoned *by this adapter* only when the run can see
//!   that it is gone, and the only thing that remembers a branch is the
//!   previous cursor (`before.branches`, in `branches` below). A run holding no
//!   position therefore still emits **zero** branch tombstones -- it has
//!   nothing to compare against -- but it no longer needs to: a cursor-less run
//!   emits every branch of every walked repository, and the sweep retires the
//!   rest. That was the recorded window (a re-added source, a cleared or
//!   unreadable cursor, a cursor-version bump), and `branch` is precisely the
//!   kind links hang off (spec §5a). The adapter's own diff still carries the
//!   *incremental* case, which the sweep never touches.
//!   `tests/sync.rs::a_run_holding_no_position_cannot_tombstone_a_deleted_branch`
//!   pins the adapter half, so the division of labour cannot drift silently.
//!
//! Two holes stay open, and both are ADR-0003's documented residual rather than
//! anything this file can fix:
//!
//! * **`commit` and `pr` are budgeted**, so for them "stopped being returned"
//!   and "deleted upstream" are the same observation and the sweep must not
//!   run. A stale row is the cheap side of that trade; sweeping would tombstone
//!   everything past the cap on every full sync.
//! * **An incremental run reconciles nothing**, for any kind: the engine sweeps
//!   only after a cursor-less run, and this adapter's own diff only sees a
//!   branch that a *walked* repository stopped listing.
//!
//! One consequence worth stating plainly, because it looks like a bug and is
//! not: narrowing `owners[]`/`repos[]` and then running a full sync retires the
//! repositories that fell out of scope. They are no longer part of this
//! source's corpus, and `deleted_at` on the entity is the mirror saying so --
//! the row, its links and its notes all survive. The other reading of an
//! incomplete walk -- a repository *skipped* rather than deselected -- cannot
//! reach the sweep at all, because a skip during a cursor-less run is fatal
//! (above).

use std::collections::BTreeMap;

use knobas_source::{Cursor, Sink, SourceError, SyncItem};
use serde_json::Value;

use crate::client::{PAGE_SIZE, is_repo_scoped};
use crate::cursor::{GiteaCursor, RepoCursor};
use crate::map::{self, RepoRef};
use crate::model;

/// At 50 per page: 1,000 repositories, and 1,000 branches per repository.
const MAX_LIST_PAGES: u32 = 20;
const MAX_BRANCH_PAGES: u32 = 20;

/// One repository this run will walk.
pub(crate) struct Selected {
    full_name: String,
    raw: Value,
    repo: model::Repo,
}

/// What `select_repos` decided: what to walk, and what was refused while
/// deciding.
struct Selection {
    walk: Vec<Selected>,
    /// Allowlist entries refused during selection. They never enter `walk`, so
    /// the walk loop cannot carry their stored positions forward and this is
    /// the only record that they were ever in scope.
    skipped: Vec<String>,
    /// The first refusal met, kept so a run that walked nothing can report it.
    first_skip: Option<SourceError>,
}

/// Whether a failure ends the run or just this repository.
enum RepoError {
    Fatal(SourceError),
    /// A 403 or 404 on one repository: gone, or not ours to read any more. The
    /// run's identity preflight already proved the token itself is good.
    Skip(SourceError),
}

impl From<SourceError> for RepoError {
    /// Sink failures and connectivity failures abort the run (battery clause
    /// 6); a refusal or an absence is about this repository alone.
    fn from(error: SourceError) -> Self {
        if is_repo_scoped(&error) {
            RepoError::Skip(error)
        } else {
            RepoError::Fatal(error)
        }
    }
}

/// The failure a reached page cap ends the run with.
///
/// **"at least", not "more than."** The cap fires when page `cap` came back
/// full, and a full last page is not proof there is another one -- so a corpus
/// of exactly `cap * PAGE_SIZE` fails too. Saying "more than" would be wrong at
/// the one value where a user is most likely to check the arithmetic;
/// `a_cap_fires_at_exactly_the_boundary_it_names` pins that boundary.
fn cap_reached(what: &str, cap: u32) -> SourceError {
    SourceError::Protocol(format!(
        "gitea: at least {} {what} to walk in one run; stopping at the cap would report a \
         complete mirror of a corpus this run never finished walking. \
         Narrow the source with owners[] or repos[].",
        cap * PAGE_SIZE
    ))
}

/// Whether a repository-scoped refusal is believable, or the credential itself
/// has gone.
///
/// `knobas-http` maps **401 and 403 alike** onto a bare
/// [`SourceError::Unauthorized`] (`crates/knobas-http/src/classify.rs`), so the
/// status that separates "this repository is not ours" from "this token is
/// dead" never reaches this crate. Task 5's brief classified 403/404 as a skip
/// and 401 as fatal; with the status gone, that classification is recovered by
/// **measuring** instead of guessing -- re-run the identity probe the run
/// already opened with.
///
/// **The probe has three outcomes, not two**, and each gets its own:
///
/// 1. **It answers** ⇒ the credential is alive, so the refusal really was about
///    that one repository (ruling B4: skip with a warning). `Ok(())`.
/// 2. **It answers `Unauthorized`** ⇒ the credential died mid-run. That is the
///    run's verdict, passed through unchanged so it stays the fault class the
///    sources view offers *Re-enter* on (interfaces §3).
/// 3. **It fails any other way** -- a 500 (not in `status_is_transient`, so not
///    retried), a timeout, a DNS blip. The credential is then *unknown*, which
///    is neither of the above. This **ends the run**: believing the refusal
///    would be guessing in the direction that loses data silently, which is the
///    thing this function exists to stop. But it ends it honestly --
///    * the probe's own fault class is kept, so a timeout is still
///      `Unreachable` and never gets relabelled `Unauthorized`, and nothing
///      puts *Re-enter* on screen over a credential nobody has disproved;
///    * the message names **the repository that was actually refused**, not
///      `/user`, because that is the event the user has to act on.
///
/// The cost is one extra request per refused repository, on a path that was
/// already losing a repository, and none at all on a healthy run. It is not
/// cached across refusals within a run on purpose: a cached "alive" is a
/// refusal believed without checking, which is exactly what this replaced.
async fn credential_still_good(
    source: &crate::GiteaSource,
    repository: &str,
    refusal: &SourceError,
) -> Result<(), SourceError> {
    match source.client.current_user().await {
        Ok(_) => Ok(()),
        // The credential is gone: the run's verdict, unchanged.
        Err(SourceError::Unauthorized) => Err(SourceError::Unauthorized),
        // Unknown. Keep the probe's fault class, name the real event.
        Err(probe) => {
            let why = format!(
                "gitea: {repository} refused this run ({refusal}), and the identity probe that \
                 would say whether the credential is still good could not be completed: {probe}"
            );
            Err(match probe {
                SourceError::Unreachable(_) => SourceError::Unreachable(why),
                // `current_user` cannot produce a `Sink` failure -- it never
                // touches one -- so everything left is a protocol fault.
                _ => SourceError::Protocol(why),
            })
        }
    }
}

/// Copy the stored position of every repository this run refused into the
/// cursor it is about to write.
///
/// **A function and not four inline lines** because two of its properties are
/// invisible to any end-to-end fixture: the allowlist is matched
/// case-insensitively, but the wiremock fake routes paths case-sensitively, so
/// a mixed-case entry never reaches the refusal path there at all. The seam is
/// what lets `a_refused_entry_is_carried_under_the_spelling_the_cursor_stores`
/// drive the case that a real user with `repos: ["Tidewater/Payout-Service"]`
/// hits on their first refusal.
fn carry_skipped_forward(previous: &GiteaCursor, skipped: &[String], next: &mut GiteaCursor) {
    for name in skipped {
        if let Some((stored, position)) = previous.entry_like(name) {
            // The *stored* key: inserting under the queried spelling would give
            // one repository two entries, and the walk looks up neither by the
            // name Gitea gives it.
            next.repos.insert(stored.clone(), position.clone());
        }
    }
}

pub(crate) async fn run(
    source: &crate::GiteaSource,
    cursor: Option<Cursor>,
    sink: &mut (dyn Sink + Send),
) -> Result<Cursor, SourceError> {
    // Identity first, one request. Three things depend on it: a 401 here is the
    // credential verdict the sources view offers *Re-enter* on (interfaces §3),
    // it stops a revoked token from quietly degrading the mirror to whatever
    // this instance serves anonymously, and it is what lets a later
    // per-repository 403 be read as a fact about that repository rather than
    // about the credential.
    source.client.current_user().await?;

    // **A full sync is a run holding no position, not a run handed no cursor.**
    // `cursor.is_none()` is only one of the ways to arrive with nothing: an
    // unreadable cursor and one from another version are documented on
    // `GiteaCursor::parse` as meaning "sync in full", and a cursor that parses
    // but names no repository knows nothing about any of them either. All three
    // are the case ruling B4's extension was ratified for -- there is no stored
    // state for a skipped repository to fall back on, so the skip would be the
    // only record it was ever in scope.
    let recovered = GiteaCursor::parse(cursor.as_deref());
    let full_sync = recovered
        .as_ref()
        .is_none_or(|position| position.repos.is_empty());
    let previous = recovered.unwrap_or_else(GiteaCursor::empty);
    let mut next = GiteaCursor::fresh();
    let mut emitted = 0u64;
    let mut walked = 0usize;

    let Selection {
        walk,
        skipped,
        mut first_skip,
    } = select_repos(source, full_sync).await?;

    // Carry a refused allowlist entry's stored position forward before the walk
    // writes anything, exactly as the walk loop's own skip arm does. Without
    // this the entry is simply absent from the cursor this run returns, and one
    // transient refusal costs that repository its `repo_updated_at` and every
    // per-branch head sha -- after which the next run re-emits it whole and,
    // having no branch memory, can tombstone nothing deleted in between.
    carry_skipped_forward(&previous, &skipped, &mut next);

    for selected in &walk {
        let before = previous.repo(&selected.full_name);
        let mut after = before.clone();
        match sync_repo(source, selected, &before, &mut after, sink, &mut emitted).await {
            Ok(()) => {
                walked += 1;
                next.repos.insert(selected.full_name.clone(), after);
            }
            Err(RepoError::Fatal(error)) => return Err(error),
            // A cursor-less run may not report success over a hole: see the
            // module docs.
            Err(RepoError::Skip(error)) if full_sync => return Err(error),
            Err(RepoError::Skip(error)) => {
                // Believe the refusal only while the credential is still good:
                // a 401 arrives here indistinguishable from a 403, and a token
                // revoked after the first repository was walked would otherwise
                // be reported as a healthy sync.
                credential_still_good(source, &selected.full_name, &error).await?;
                tracing::warn!(
                    repository = %selected.full_name,
                    %error,
                    "gitea: skipping this repository for this run"
                );
                if first_skip.is_none() {
                    first_skip = Some(error);
                }
                // Keep the watermark we came in with: a transient refusal must
                // not force a full refetch of that repository next time.
                next.repos.insert(selected.full_name.clone(), before);
            }
        }
    }

    // One repository refusing us is a fact about that repository. *Every*
    // repository refusing us is a fact about the token's scope, and reporting
    // success would leave the user staring at an empty mirror with a green
    // source.
    if walked == 0
        && let Some(error) = first_skip
    {
        return Err(error);
    }

    // Interfaces §4.1: a run that emitted nothing returns the cursor it was
    // handed, byte-identical. Re-serialising would be *equal* but not
    // *identical* -- `repos_listed_at` alone would move on every poll.
    if emitted == 0
        && let Some(unchanged) = cursor
    {
        return Ok(unchanged);
    }
    Ok(next.to_json())
}

/// Which repositories this run walks, and what was refused while deciding.
async fn select_repos(
    source: &crate::GiteaSource,
    full_sync: bool,
) -> Result<Selection, SourceError> {
    let mut out = Vec::new();
    let mut skipped = Vec::new();
    let mut skip = None;
    let config = &source.config;

    if config.repos.is_empty() {
        let mut page = 1;
        loop {
            let batch = source.client.search_repos(page).await?;
            let last = batch.len() < PAGE_SIZE as usize;
            for raw in batch {
                push_selected(&mut out, raw, config);
            }
            if last {
                break;
            }
            if page == MAX_LIST_PAGES {
                return Err(cap_reached("repositories", MAX_LIST_PAGES));
            }
            page += 1;
        }
    } else {
        for entry in &config.repos {
            // `GiteaConfig::validate` has already refused anything else.
            let Some((owner, name)) = entry.split_once('/') else {
                continue;
            };
            match source.client.get_repo(owner, name).await {
                Ok(raw) => push_selected(&mut out, raw, config),
                Err(error) if is_repo_scoped(&error) && !full_sync => {
                    // The same measurement the walk loop makes: 401 and 403 are
                    // one error here, so a dead credential would otherwise read
                    // as "the user configured a repository they cannot see".
                    credential_still_good(source, entry, &error).await?;
                    tracing::warn!(
                        repository = %entry,
                        %error,
                        "gitea: configured repository is unavailable"
                    );
                    skipped.push(entry.clone());
                    if skip.is_none() {
                        skip = Some(error);
                    }
                }
                Err(error) => return Err(error),
            }
        }
    }

    // Deterministic: the commit budget is spent in this order.
    out.sort_by(|a, b| a.full_name.cmp(&b.full_name));
    Ok(Selection {
        walk: out,
        skipped,
        first_skip: skip,
    })
}

fn push_selected(out: &mut Vec<Selected>, raw: Value, config: &crate::config::GiteaConfig) {
    let repo: model::Repo = match serde_json::from_value(raw.clone()) {
        Ok(repo) => repo,
        Err(error) => {
            tracing::warn!(
                %error,
                "gitea: skipping a repository record this adapter cannot read"
            );
            return;
        }
    };
    let Some((owner, _)) = repo.owner_repo() else {
        tracing::warn!(
            full_name = %repo.full_name,
            "gitea: skipping a repository with an unaddressable name"
        );
        return;
    };
    if !config.owners.is_empty()
        && !config
            .owners
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(owner))
    {
        return;
    }
    if !config.repos.is_empty()
        && !config
            .repos
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(&repo.full_name))
    {
        return;
    }
    out.push(Selected {
        full_name: repo.full_name.clone(),
        raw,
        repo,
    });
}

async fn sync_repo(
    source: &crate::GiteaSource,
    selected: &Selected,
    before: &RepoCursor,
    after: &mut RepoCursor,
    sink: &mut (dyn Sink + Send),
    emitted: &mut u64,
) -> Result<(), RepoError> {
    let (owner, name) = selected
        .repo
        .owner_repo()
        .expect("selection checked the name");
    let at = RepoRef {
        owner,
        name,
        full_name: &selected.full_name,
        html_url: selected.repo.html_url.as_deref(),
    };

    // 1. The repository itself.
    let updated_at = map::real_time(selected.repo.updated_at);
    if before.repo_updated_at != updated_at {
        push(
            sink,
            map::repo_item(&source.id, &selected.raw, &selected.repo, at),
            emitted,
        )
        .await?;
    }
    after.repo_updated_at = updated_at;

    // 2. Branches, and the ones that vanished. Task 7 walks commits for
    //    exactly the branches this reports as moved.
    let _moved = branches(source, at, before, after, sink, emitted).await?;
    Ok(())
}

/// Emit every branch whose head moved, tombstone every name that disappeared,
/// and report which branches moved so the commit pass knows where to look.
async fn branches(
    source: &crate::GiteaSource,
    at: RepoRef<'_>,
    before: &RepoCursor,
    after: &mut RepoCursor,
    sink: &mut (dyn Sink + Send),
    emitted: &mut u64,
) -> Result<Vec<String>, RepoError> {
    let mut heads: BTreeMap<String, String> = BTreeMap::new();
    let mut moved = Vec::new();

    let mut page = 1;
    loop {
        let batch = source.client.branches(at.owner, at.name, page).await?;
        let last = batch.len() < PAGE_SIZE as usize;
        for raw in batch {
            let branch: model::Branch = match serde_json::from_value(raw.clone()) {
                Ok(branch) => branch,
                Err(error) => {
                    tracing::warn!(
                        %error,
                        repository = %at.full_name,
                        "gitea: skipping an unreadable branch record"
                    );
                    continue;
                }
            };
            let head = branch
                .commit
                .as_ref()
                .and_then(|c| c.id.clone())
                .unwrap_or_default();
            if before.branches.get(&branch.name) != Some(&head) {
                push(
                    sink,
                    map::branch_item(&source.id, &raw, at, &branch),
                    emitted,
                )
                .await?;
                moved.push(branch.name.clone());
            }
            heads.insert(branch.name, head);
        }
        if last {
            break;
        }
        if page == MAX_BRANCH_PAGES {
            return Err(RepoError::Fatal(cap_reached(
                "branches in one repository",
                MAX_BRANCH_PAGES,
            )));
        }
        page += 1;
    }

    for gone in before
        .branches
        .keys()
        .filter(|name| !heads.contains_key(*name))
    {
        push(sink, map::branch_tombstone(&source.id, at, gone), emitted).await?;
    }

    after.branches = heads;
    Ok(moved)
}

/// Hand one item to the engine.
///
/// The `?` is the contract: a sink that rejected an item wants the sync
/// abandoned, not the remaining items pushed at it and a fresh cursor handed
/// back over the gap (SPI docs, battery clause 6).
async fn push(
    sink: &mut (dyn Sink + Send),
    item: SyncItem,
    emitted: &mut u64,
) -> Result<(), SourceError> {
    sink.item(item).await?;
    *emitted += 1;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use knobas_http::StatusCode;

    /// A sink failure must never be read as "skip this repository": that would
    /// swallow the one error battery clause 6 requires to abort the run.
    #[test]
    fn only_a_repository_scoped_failure_becomes_a_skip() {
        assert!(matches!(
            RepoError::from(SourceError::Unauthorized),
            RepoError::Skip(_)
        ));
        assert!(matches!(
            RepoError::from(knobas_http::status_error(StatusCode::NOT_FOUND, "")),
            RepoError::Skip(_)
        ));
        for fatal in [
            SourceError::Sink("pool closed".to_owned()),
            SourceError::Unreachable("connection refused".to_owned()),
            knobas_http::status_error(StatusCode::INTERNAL_SERVER_ERROR, ""),
        ] {
            assert!(
                matches!(RepoError::from(fatal), RepoError::Fatal(_)),
                "must abort the run"
            );
        }
    }

    /// A refused `repos[]` entry keeps its position **under the spelling the
    /// cursor stores**, whatever case the user typed it in.
    ///
    /// `cursor::…::a_refused_allowlist_entry_is_found_whatever_case_the_user_typed`
    /// pins what `entry_like` *returns*; this pins that the caller writes back
    /// the key it returned rather than the one it was asked with. Those fail
    /// differently -- the first loses the position outright, the second gives
    /// one repository two cursor entries and the walk finds neither -- and with
    /// an exact-case allowlist, which is every end-to-end fixture, the two
    /// spellings are the same string and neither failure is observable.
    #[test]
    fn a_refused_entry_is_carried_under_the_spelling_the_cursor_stores() {
        let stored = "tidewater/payout-service";
        let mut previous = GiteaCursor::empty();
        previous.repos.insert(
            stored.to_owned(),
            RepoCursor {
                repo_updated_at: Some(
                    "2026-08-20T09:00:00Z"
                        .parse::<chrono::DateTime<chrono::Utc>>()
                        .unwrap(),
                ),
                branches: BTreeMap::from([("main".to_owned(), "9".repeat(40))]),
                ..RepoCursor::default()
            },
        );

        let mut next = GiteaCursor::fresh();
        carry_skipped_forward(
            &previous,
            &[
                "Tidewater/Payout-Service".to_owned(),
                // Never seen before: nothing to carry, and no blank entry
                // invented for it either.
                "elsewhere/unrelated".to_owned(),
            ],
            &mut next,
        );

        assert_eq!(
            next.repos.keys().collect::<Vec<_>>(),
            vec![stored],
            "exactly one entry, under the stored spelling"
        );
        assert_eq!(next.repo(stored), previous.repo(stored));
    }

    /// The cap message has to name the limit that was hit and the lever that
    /// moves it, because it is what the user sees when a source is too big.
    #[test]
    fn a_reached_cap_names_the_limit_and_the_lever() {
        let SourceError::Protocol(message) = cap_reached("repositories", MAX_LIST_PAGES) else {
            panic!("a reached cap is a protocol failure");
        };
        assert!(message.contains("1000"), "{message}");
        assert!(message.contains("owners[]"), "{message}");
        // And says why stopping is not an option: an `Ok` here would claim a
        // corpus the run never finished walking.
        assert!(message.contains("never finished walking"), "{message}");
    }
}

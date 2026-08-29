//! One sync run.
//!
//! Order is fixed and deterministic -- repository, its branches, its pull
//! requests, its commits, repositories sorted by full name, branches by
//! `walk_order` -- so a test can assert on what came out and a budget spends
//! itself the same way twice.
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
//! Three things follow, and all three are the same rule:
//!
//! * **A page shorter than the one this run asked for is not the end of a
//!   listing.** Every walk here pages until a page comes back *empty*
//!   ([`last_page`]), never until one comes back short. `limit=50` is a
//!   request and Gitea's 50 is a *default* an admin of a self-hosted instance
//!   can lower, so a short page means "the collection ran out" or "the server
//!   capped us" and nothing in the answer says which. Reading it as the first
//!   is the same truncation as the two rules below, arriving through the
//!   transport instead of through a cap -- and the worse one, because it
//!   reports `Ok`. Issue #81, ruled 2026-08-29.
//! * **A page cap that is reached ends the run with an error.** Returning `Ok`
//!   after walking 950 of 1,400 repositories would report a complete mirror
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
//!   or whose `owners[]` is narrowed to owners that hold no repositories --
//!   keeps that row, and the branch rows under it, indefinitely. (`owners: []`
//!   is the opposite case: an empty list is no filter at all and syncs every
//!   repository the token can see, per `GiteaConfig::owners`.) See
//!   `knobas_sync::run_once`, *Limitations*.
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

use std::collections::{BTreeMap, HashSet};

use chrono::{DateTime, Utc};
use knobas_source::{Cursor, Sink, SourceError, SyncItem};
use serde_json::Value;

use crate::client::is_repo_scoped;
use crate::cursor::{GiteaCursor, RepoCursor};
use crate::map::{self, RepoRef};
use crate::model;

/// The runaway guard on the two exhaustive walks: at most this many requests
/// each, whatever the server chooses to put on a page.
///
/// A **request** budget rather than a record one, and since [`last_page`] the
/// difference is visible: a walk spends its last request on the empty page that
/// proves the listing ended, so 20 requests carry 19 pages of records. Against
/// a server that serves the 50 asked for, that is 950 repositories and 950
/// branches per repository, where terminating on a short page reached 999. Those
/// 50 records are not worth trading for a walk with no bound at all against a
/// server that ignores `page`, and a corpus past the guard fails loudly with the
/// lever that fixes it ([`cap_reached`]) rather than being silently truncated.
const MAX_LIST_PAGES: u32 = 20;
const MAX_BRANCH_PAGES: u32 = 20;
// The two budgeted walks' page caps. Both sit at 1,000 records, which is
// exactly the largest `prs_per_repo`/`commits_per_repo` the config schema
// allows (`config::config_schema`, `"maximum": 1000`). Sitting them *there* is
// what keeps them runaway guards rather than a second, hidden budget: any
// lower and a user who raised their budget to the top of the range the form
// offers would be truncated by a number no form ever showed them.
//
// Unlike `MAX_LIST_PAGES`/`MAX_BRANCH_PAGES`, reaching one of these ends the
// walk *silently* rather than with `cap_reached`. That is the
// exhaustive/budgeted split (ADR-0003): `repo` and `branch` promise a complete
// corpus, so stopping short of one has to fail the run rather than report a
// mirror it never finished; `pr` and `commit` promise only the newest
// `*_per_repo` of theirs and are never swept, so stopping is the normal case
// and cannot be read downstream as a deletion.
//
// `last_page` costs these two nothing, unlike the exhaustive pair: a budget
// of 1,000 is spent by the last record of page 20 and breaks the walk there,
// before any request for the empty page that would have confirmed the end.
/// Pull requests per repository, per run.
const MAX_PR_PAGES: u32 = 20;
/// Commits per *branch* per run. `commits_per_repo` is the whole-repository
/// budget the walk actually spends, and is what bites first.
const MAX_COMMIT_PAGES: u32 = 20;

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
/// **"at least", not "more than."** The cap fires when the cap'th page came
/// back with something on it, and a non-empty page is not proof there is
/// another one -- so a corpus that happens to end exactly on that page fails
/// too. Saying "more than" would be wrong at the one value where a user is most
/// likely to check the arithmetic; `a_cap_fires_at_exactly_the_boundary_it_names`
/// pins that boundary.
///
/// **`seen` is what the walk actually counted**, and it used to be
/// `cap * PAGE_SIZE`. That arithmetic assumed the server put the requested 50
/// records on every page -- the assumption [`last_page`] exists to remove.
/// Against an instance capping at 10, a reached cap means 200 records walked,
/// and a message naming 1,000 would send the user narrowing a source that was
/// never that big.
fn cap_reached(what: &str, seen: usize) -> SourceError {
    SourceError::protocol(format!(
        "gitea: at least {seen} {what} to walk in one run; stopping at the cap would report a \
         complete mirror of a corpus this run never finished walking. \
         Narrow the source with owners[] or repos[]."
    ))
}

/// Whether a page just answered is the end of its listing.
///
/// **Empty, not short.** The obvious spelling -- `batch.len() < PAGE_SIZE` --
/// reads a page shorter than the `limit` the request asked for as the end of
/// the collection, and that is sound only if the server honoured `limit`
/// exactly. It is not a promise it makes: `PAGE_SIZE` documents 50 as Gitea's
/// *default* cap, a self-hosted instance can lower `MAX_RESPONSE_ITEMS`, an
/// endpoint can carry its own maximum, and a loaded server can answer a partial
/// page. Under any of those the short-page rule stops the walk early and
/// returns `Ok`, with the watermark advancing past every record after the stop
/// -- silent truncation, which is the failure the whole module docs above are
/// about. An empty page is the one answer that cannot mean anything else.
///
/// **One spelling, in one place, for all four walks.** Issue #81 was filed
/// because the rule had been copied to four sites and asked for one deliberate
/// decision instead of four accidental ones; option 1 (page until empty) was
/// ruled on 2026-08-29. Splitting it back into four inline comparisons is how
/// three of them drift.
///
/// The price is one extra request per *exhausted* walk, under the 10 req/s
/// limiter of interfaces §4.1 -- and the `MAX_*_PAGES` caps above are what keep
/// "until empty" from becoming "until forever" against a server that ignores
/// the `page` parameter.
fn last_page(batch: &[Value]) -> bool {
    batch.is_empty()
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
                // A 403 or a 404, and nothing else: a revoked credential
                // answers 401, which `is_repo_scoped` reads as fatal and this
                // arm therefore never sees (ADR-0004).
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
        let mut seen = 0usize;
        loop {
            let batch = source.client.search_repos(page).await?;
            let last = last_page(&batch);
            seen += batch.len();
            for raw in batch {
                push_selected(&mut out, raw, config);
            }
            if last {
                break;
            }
            if page == MAX_LIST_PAGES {
                return Err(cap_reached("repositories", seen));
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
                    // Again a 403 or a 404 only. A dead credential answers 401
                    // and raises through the `Err(error) => return` arm below,
                    // rather than reading as "the user configured a repository
                    // they cannot see".
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

    // 2. Branches, and the ones that vanished. Step 4 walks commits for
    //    exactly the branches this reports as moved.
    let moved = branches(source, at, before, after, sink, emitted).await?;

    // 3. Pull requests, newest-updated first, down to the watermark.
    pulls(source, at, before, after, sink, emitted).await?;

    // 4. Commits, only where a head moved. On a first sync every branch counts
    //    as moved, so this is the full walk the budget bounds.
    commits(
        source,
        at,
        &selected.repo,
        before,
        after,
        &moved,
        sink,
        emitted,
    )
    .await?;
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
    let mut seen = 0usize;
    loop {
        let batch = source.client.branches(at.owner, at.name, page).await?;
        let last = last_page(&batch);
        seen += batch.len();
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
                seen,
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

/// Pull requests changed since the watermark, newest-updated first.
///
/// The walk relies on `sort=recentupdate` ordering the answer newest first, so
/// the first record strictly below the watermark ends it. That assumption is
/// checked against the real server in `tests/live_gitea.rs` rather than
/// trusted -- if a Gitea release ever changed it, this walk would silently
/// truncate every sync and no fixture would notice, because the fake is
/// written to the same assumption.
///
/// # Why a budget here is not the truncation the module docs forbid
///
/// `prs_per_repo` is a bound the **user configured** on the corpus knobas
/// mirrors; a reached page cap or a refusal is a hole nobody asked for. That is
/// the whole distinction, and it is why `pr` declares
/// `full_sync_exhaustive: false` (ADR-0003): a budgeted kind is never swept, so
/// stopping at the budget can never be read downstream as "the rest was
/// deleted".
async fn pulls(
    source: &crate::GiteaSource,
    at: RepoRef<'_>,
    before: &RepoCursor,
    after: &mut RepoCursor,
    sink: &mut (dyn Sink + Send),
    emitted: &mut u64,
) -> Result<(), RepoError> {
    // Carried first, so every early return below leaves the position it came
    // in with rather than a blank one.
    after.pulls_updated_to = before.pulls_updated_to;
    after.pulls_at_watermark = before.pulls_at_watermark.clone();
    if source.config.prs_per_repo == 0 {
        return Ok(());
    }

    let watermark = before.pulls_updated_to;
    let mut budget = source.config.prs_per_repo;
    // Every pull request this run looked at -- delivered, or recognised as
    // already delivered. The new watermark is computed from this, so a pull
    // request skipped as already-delivered still holds the boundary open.
    let mut examined: Vec<(u64, Option<DateTime<Utc>>)> = Vec::new();

    'paging: for page in 1..=MAX_PR_PAGES {
        let batch = source.client.pulls(at.owner, at.name, page).await?;
        let last = last_page(&batch);
        for raw in batch {
            let pr: model::PullRequest = match serde_json::from_value(raw.clone()) {
                Ok(pr) => pr,
                Err(error) => {
                    tracing::warn!(
                        %error,
                        repository = %at.full_name,
                        "gitea: skipping an unreadable pull request"
                    );
                    continue;
                }
            };
            let updated = map::real_time(pr.updated_at).or_else(|| map::real_time(pr.created_at));
            if let (Some(mark), Some(when)) = (watermark, updated) {
                if when < mark {
                    break 'paging;
                }
                if when == mark && before.pulls_at_watermark.contains(&pr.number) {
                    examined.push((pr.number, updated));
                    continue;
                }
            }
            let comments = fetch_comments(source, at, &pr).await?;
            examined.push((pr.number, updated));
            push(
                sink,
                map::pr_item(&source.id, &raw, at, &pr, &comments),
                emitted,
            )
            .await?;
            budget -= 1;
            if budget == 0 {
                break 'paging;
            }
        }
        if last {
            break;
        }
    }

    (after.pulls_updated_to, after.pulls_at_watermark) =
        close_watermark(&examined, watermark, &before.pulls_at_watermark);
    Ok(())
}

/// Where a budgeted walk's watermark now stands, and which keys sit exactly on
/// it.
///
/// Gitea's timestamps have one-second resolution, so a strict `>` boundary
/// would drop a record updated in the same second as the newest one. Both
/// budgeted walks close their position the same way, and it is subtle enough in
/// the same two places that it is written once here rather than twice.
///
/// `examined` is every record this run **looked at** -- delivered, or
/// recognised as already delivered -- because a record skipped as
/// already-delivered still holds the boundary open.
///
/// `carried` is the previous run's keys on the incoming mark, and it is the
/// half worth reading twice. When the position does not move, whatever the
/// previous run recorded at that instant is still delivered **even where this
/// run's budget stopped before re-observing it**; dropping those keys would
/// re-deliver them on the next poll, and battery clause 2 would fail on the
/// *second* idle run rather than the first, which is the hard version of this
/// bug to find. When the position does move, the carried keys are strictly
/// below the new mark and each walk's own `<` test already stops at them, so
/// carrying them would only grow the cursor.
fn close_watermark<K: Ord + Clone>(
    examined: &[(K, Option<DateTime<Utc>>)],
    incoming: Option<DateTime<Utc>>,
    carried: &[K],
) -> (Option<DateTime<Utc>>, Vec<K>) {
    let mark = examined
        .iter()
        .filter_map(|(_, when)| *when)
        .max()
        .or(incoming);
    let Some(at) = mark else {
        return (None, Vec::new());
    };
    let mut keys: Vec<K> = examined
        .iter()
        .filter(|(_, when)| *when == Some(at))
        .map(|(key, _)| key.clone())
        .collect();
    if mark == incoming {
        keys.extend(carried.iter().cloned());
    }
    keys.sort();
    keys.dedup();
    (mark, keys)
}

/// The discussion, when there is any and the source wants it indexed.
///
/// Gitea keeps a pull request's discussion on the **issue** of the same index,
/// which is the fifth read endpoint ruling B1 granted. `pr.comments == 0` is
/// what makes an idle-ish run cheap: no discussion, no request.
///
/// # What a refusal here costs, and what it is allowed to hide
///
/// A refusal costs searchable text, not the run -- the pull request itself was
/// readable a moment ago on the same credential. Which refusal it is decides
/// whether that reading is available, and until ADR-0004 it could not be known:
/// `knobas-http` collapsed 401 and 403 into one bare
/// [`SourceError::Unauthorized`], so "this token has no issue scope" and "this
/// token was just revoked" arrived identically, and an identity probe had to be
/// re-run per refusal to tell them apart. The status is carried now, and
/// [`crate::client::is_repo_scoped`] reads it:
///
/// * **403** -- the token really lacks issue scope. The pull request is indexed
///   without its discussion, and the warning names the setting that turns the
///   asking off.
/// * **404** -- Gitea's answer for a repository with its issue unit disabled,
///   and the common case. Never a credential fault; same treatment.
/// * **401** -- the credential is gone, which is not repository-scoped and
///   never was. It is the run's verdict, and it costs no extra request to say
///   so.
///
/// # Why this is not ruling B4's "fatal on a cursor-less run"
///
/// A repository refused during a full sync **is** fatal (the walk loop's
/// `Err(Skip) if full_sync` arm), because that run would otherwise report a
/// complete corpus it never read. A refused *discussion* is deliberately not
/// that, on either kind of run: B4's subject is a repository, and here the pull
/// request itself is still emitted, no entity is missing from the mirror, and
/// `pr` is a budgeted kind the sweep never touches -- so nothing downstream can
/// read the shorter `body_text` as a deletion. What is lost is search text on
/// one item, and it is restored the next time that pull request is updated, or
/// by the next full sync.
async fn fetch_comments(
    source: &crate::GiteaSource,
    at: RepoRef<'_>,
    pr: &model::PullRequest,
) -> Result<Vec<model::Comment>, RepoError> {
    if !source.config.include_pr_comments || pr.comments == 0 {
        return Ok(Vec::new());
    }
    let error = match source
        .client
        .issue_comments(at.owner, at.name, pr.number)
        .await
    {
        Ok(raw) => {
            // A single unreadable comment is dropped rather than failing the
            // pull request: the rest of the discussion is still worth indexing.
            return Ok(raw
                .into_iter()
                .filter_map(|c| serde_json::from_value(c).ok())
                .collect());
        }
        Err(error) => error,
    };
    if !is_repo_scoped(&error) {
        return Err(RepoError::from(error));
    }
    tracing::warn!(
        repository = %at.full_name,
        number = pr.number,
        %error,
        "gitea: indexing this pull request without its discussion; \
         set include_pr_comments to false to stop asking"
    );
    Ok(Vec::new())
}

/// New commits on the branches whose heads moved this run.
///
/// Gitea's `since=` is inclusive and server-side, so the boundary commits come
/// back on every run; `commits_at_watermark` is what keeps them from being
/// re-delivered. Commits are immutable, so a delivered object id is delivered
/// for good.
///
/// **A branch that did not move is not walked at all**, which is what the
/// per-branch head object ids in the cursor are for (`cursor::RepoCursor`): an
/// idle repository costs its two listings and no commit request. The cost of
/// that trade is stated plainly -- a commit reachable only from a branch whose
/// *head* did not change is never noticed, and nothing here goes looking for
/// one.
///
/// **Known limitation:** a branch that appears with history older than
/// `commits_since` -- a long-lived branch pushed for the first time -- has that
/// older history filtered out by `since=`. It arrives with the next full sync
/// (`cursor: None`). Fetching it eagerly would mean walking every new branch to
/// its root, which is exactly the cost `commits_per_repo` exists to bound.
#[expect(
    clippy::too_many_arguments,
    reason = "the whole per-repository walk state, threaded explicitly: a struct \
              here would be a bag of unrelated borrows with a different lifetime each"
)]
async fn commits(
    source: &crate::GiteaSource,
    at: RepoRef<'_>,
    repo: &model::Repo,
    before: &RepoCursor,
    after: &mut RepoCursor,
    moved: &[String],
    sink: &mut (dyn Sink + Send),
    emitted: &mut u64,
) -> Result<(), RepoError> {
    after.commits_since = before.commits_since;
    after.commits_at_watermark = before.commits_at_watermark.clone();
    // `repo.empty` is the one that saves a request rather than a mistake:
    // Gitea answers `/commits` on a repository with no commits at all with a
    // 409, which `client::commits` already reads as "none".
    if source.config.commits_per_repo == 0 || repo.empty || moved.is_empty() {
        return Ok(());
    }

    let since = before.commits_since;
    let mut budget = source.config.commits_per_repo;
    let mut examined: Vec<(String, Option<DateTime<Utc>>)> = Vec::new();
    // One object id can be reachable from several branches; it is one entity
    // and must cost one slot of the budget.
    let mut seen: HashSet<String> = HashSet::new();

    'branches: for branch in walk_order(repo.default_branch.as_deref(), moved) {
        for page in 1..=MAX_COMMIT_PAGES {
            let batch = source
                .client
                .commits(at.owner, at.name, &branch, since, page)
                .await?;
            let last = last_page(&batch);
            let mut fresh = 0usize;
            for raw in batch {
                let commit: model::Commit = match serde_json::from_value(raw.clone()) {
                    Ok(commit) => commit,
                    Err(error) => {
                        tracing::warn!(
                            %error,
                            repository = %at.full_name,
                            "gitea: skipping an unreadable commit"
                        );
                        continue;
                    }
                };
                let happened = map::real_time(commit.happened_at());
                if let (Some(mark), Some(when)) = (since, happened) {
                    // Not a `break`: `since=` is a server-side filter this walk
                    // does not control, and a server that ignored it would put
                    // the whole history in front of the new commits.
                    if when < mark {
                        continue;
                    }
                    if when == mark && before.commits_at_watermark.contains(&commit.sha) {
                        examined.push((commit.sha.clone(), happened));
                        continue;
                    }
                }
                if !seen.insert(commit.sha.clone()) {
                    continue;
                }
                examined.push((commit.sha.clone(), happened));
                push(
                    sink,
                    map::commit_item(&source.id, &raw, at, &commit),
                    emitted,
                )
                .await?;
                fresh += 1;
                budget -= 1;
                if budget == 0 {
                    break 'branches;
                }
            }
            // A page with nothing new on it is the end of this branch's new
            // history -- and if the server ignored `since=` altogether, it is
            // the point where paging stops being worth anything.
            if last || fresh == 0 {
                break;
            }
        }
    }

    (after.commits_since, after.commits_at_watermark) =
        close_watermark(&examined, since, &before.commits_at_watermark);
    Ok(())
}

/// Default branch first, then by name: on a first sync the budget should go to
/// `main` before it goes to `wip/spike`, and the order has to be the same twice
/// or two runs of one repository would mirror two different subsets of it.
fn walk_order(default_branch: Option<&str>, moved: &[String]) -> Vec<String> {
    let mut order = moved.to_vec();
    order.sort_by(|a, b| {
        let rank = |name: &String| usize::from(Some(name.as_str()) != default_branch);
        rank(a).cmp(&rank(b)).then_with(|| a.cmp(b))
    });
    order
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
    ///
    /// **A 401 is fatal here too, and that is ADR-0004's line.** It used to
    /// arrive as the same bare `Unauthorized` a 403 did and therefore became a
    /// `Skip`, which only `credential_still_good` -- one extra request per
    /// refusal -- kept from reporting a healthy sync over a revoked token.
    #[test]
    fn only_a_repository_scoped_failure_becomes_a_skip() {
        for skipped in [
            knobas_http::status_error(StatusCode::FORBIDDEN, "", None),
            knobas_http::status_error(StatusCode::NOT_FOUND, "", None),
        ] {
            assert!(
                matches!(RepoError::from(skipped), RepoError::Skip(_)),
                "ruling B4: a refusal or an absence is about one repository"
            );
        }
        for fatal in [
            // The credential itself, in both its shapes: rejected by the
            // server, and absent from the keychain.
            knobas_http::status_error(StatusCode::UNAUTHORIZED, "", None),
            SourceError::unauthorized(),
            SourceError::Sink("pool closed".to_owned()),
            SourceError::Unreachable("connection refused".to_owned()),
            knobas_http::status_error(StatusCode::INTERNAL_SERVER_ERROR, "", None),
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

    /// The budget is spent in a fixed order, and the default branch is where a
    /// first sync should spend it: `main` before `wip/spike`.
    ///
    /// Determinism is the half that is easy to lose and hard to see -- two runs
    /// of one repository that ordered its branches differently would mirror two
    /// different hundred-commit subsets of it and each would look correct.
    #[test]
    fn the_default_branch_gets_the_budget_first() {
        let moved = vec![
            "wip/spike".to_owned(),
            "main".to_owned(),
            "feature/PAY-231".to_owned(),
        ];
        assert_eq!(
            walk_order(Some("main"), &moved),
            vec!["main", "feature/PAY-231", "wip/spike"]
        );
        // No default branch reported: still a stable order, just alphabetical.
        assert_eq!(
            walk_order(None, &moved),
            vec!["feature/PAY-231", "main", "wip/spike"]
        );
        // A default branch that did not move does not get walked just for
        // being the default.
        assert_eq!(
            walk_order(Some("main"), &["wip/spike".to_owned()]),
            vec!["wip/spike"]
        );
        assert!(walk_order(Some("main"), &[]).is_empty());
    }

    fn at(t: &str) -> Option<DateTime<Utc>> {
        Some(t.parse().expect("a timestamp"))
    }

    /// The boundary second, closed. Each case is a run the budget ended
    /// somewhere different.
    #[test]
    fn the_watermark_keeps_every_key_on_the_instant_it_stands_on() {
        // Nothing known and nothing seen: no position to write.
        assert_eq!(
            close_watermark::<u64>(&[], None, &[]),
            (None, Vec::new()),
            "a walk that saw nothing invents no position"
        );

        // A first sync: the mark is the newest examined, and only the keys on
        // it are the boundary.
        assert_eq!(
            close_watermark(
                &[
                    (144, at("2026-08-22T13:50:00Z")),
                    (143, at("2026-08-22T13:50:00Z")),
                    (142, at("2026-08-22T10:20:00Z")),
                ],
                None,
                &[],
            ),
            (at("2026-08-22T13:50:00Z"), vec![143, 144])
        );

        // The position moved. The keys the previous run held are strictly
        // below the new mark, so carrying them would only grow the cursor --
        // each walk's own `<` test is what stops at them now.
        assert_eq!(
            close_watermark(
                &[(146, at("2026-08-22T14:00:00Z"))],
                at("2026-08-22T13:50:00Z"),
                &[142, 144],
            ),
            (at("2026-08-22T14:00:00Z"), vec![146])
        );

        // The position did **not** move and this run's budget stopped after
        // one new record on the same instant. The two the previous run
        // delivered are still delivered; dropping them re-delivers them on the
        // next poll, which is the bug this carry exists to stop.
        assert_eq!(
            close_watermark(
                &[(146, at("2026-08-22T13:50:00Z"))],
                at("2026-08-22T13:50:00Z"),
                &[142, 144],
            ),
            (at("2026-08-22T13:50:00Z"), vec![142, 144, 146])
        );

        // An idle run examines the boundary again; re-observing a carried key
        // must not record it twice.
        assert_eq!(
            close_watermark(
                &[(144, at("2026-08-22T13:50:00Z"))],
                at("2026-08-22T13:50:00Z"),
                &[142, 144],
            ),
            (at("2026-08-22T13:50:00Z"), vec![142, 144])
        );

        // A run that examined nothing at all keeps the position it was handed,
        // boundary and all.
        assert_eq!(
            close_watermark(
                &[],
                at("2026-08-22T13:50:00Z"),
                &["c90d11".to_owned(), "a41f2c".to_owned()],
            ),
            (
                at("2026-08-22T13:50:00Z"),
                vec!["a41f2c".to_owned(), "c90d11".to_owned()]
            )
        );
    }

    /// The cap message has to name what was walked and the lever that moves the
    /// limit, because it is what the user sees when a source is too big.
    ///
    /// The number is the walk's own count, hedged with "at least": since
    /// `last_page` there is no page size to multiply by, and inventing one
    /// would misreport every server that caps its pages lower than the 50 the
    /// request asks for.
    #[test]
    fn a_reached_cap_names_what_it_walked_and_the_lever() {
        let SourceError::Protocol { message, .. } = cap_reached("repositories", 950) else {
            panic!("a reached cap is a protocol failure");
        };
        assert!(message.contains("at least 950 repositories"), "{message}");
        assert!(message.contains("owners[]"), "{message}");
        // And says why stopping is not an option: an `Ok` here would claim a
        // corpus the run never finished walking.
        assert!(message.contains("never finished walking"), "{message}");
    }
}

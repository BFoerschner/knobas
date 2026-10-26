//! One sync run.
//!
//! Order is fixed **within one repository** -- the repository, its branches,
//! its pull requests, its issues, its commits, branches by `walk_order` -- so a
//! budget spends itself the same way twice and each walk's watermark sees its
//! own pages in order. **Across repositories it is not** (#2159): up to
//! `concurrent_repos` of them are walked side by side, started in full-name
//! order, and each one's items reach the sink when that repository finishes.
//! Nothing downstream reads that order: the engine writes a run in one
//! transaction and sorts each batch by entity id, every budget and page cap is
//! per repository, and the cursor is a map written at the end of the run.
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
//!   ([`PageWalk`]), never until one comes back short. `limit=50` is a
//!   request and Gitea's 50 is a *default* an admin of a self-hosted instance
//!   can lower, so a short page means "the collection ran out" or "the server
//!   capped us" and nothing in the answer says which. Reading it as the first
//!   is the same truncation as the two rules below, arriving through the
//!   transport instead of through a cap -- and the worse one, because it
//!   reports `Ok`. Issue #81, ruled 2026-08-29.
//! * **A page cap that is reached ends the run with an error.** Returning `Ok`
//!   after walking 1,050 of 1,400 repositories would report a complete mirror
//!   of a corpus that was never walked.
//! * **A repository skipped during a full sync is fatal too.** Ruling B4
//!   grants skip-with-warning for a 403 or 404 on one repository, and that is
//!   what an *incremental* run does -- the repository's watermarks are kept, so
//!   the next run picks up where this one left off. A run holding no position
//!   has no such state to fall back on: the skip would be the only record that
//!   the repository was ever in scope. A repository deleted upstream before
//!   the listing never takes this path -- it simply stops appearing in it.
//!
//!   **Amended by #728 (deputy for Björn, 2026-10-13): B4 stands for a
//!   repository the user configured.** A 404 on a name in `repos[]` is a
//!   misconfiguration worth failing on. A repository that came from *this
//!   run's listing* and then answers 404 under it is the one case above that
//!   does take this path: deleted between the listing and its read. When the
//!   repository's own read (`GET /repos/{owner}/{repo}`) answers 404 too, it is
//!   skipped on either kind of run, as #724 skips a deleted branch, and the next
//!   sweep retires it. Any other answer to that read, a 5xx included, leaves
//!   the 404 a hole.
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
//! `crate::tests::exactly_the_unbudgeted_code_host_kinds_are_exhaustive_and_the_ticket_is_not` -- so a budget
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
//!   *incremental* case, which the sweep never touches. "Every branch" holds
//!   while the listing moves too (#1716): a walk of more than one page reads
//!   each name git lists that the pages missed, so a push or a deletion
//!   between two pages hides no branch from the sweep.
//!   `tests/it/sync.rs::a_run_holding_no_position_cannot_tombstone_a_deleted_branch`
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
//!   only after a run that walked the kind whole, and this adapter's own diff
//!   only sees a branch that a *walked* repository stopped listing. A run this
//!   adapter makes a full sync although it was handed a cursor (an unreadable
//!   one, one naming no repository, a narrowed scope) is not incremental: it
//!   says it walked `repo` and `branch` whole (`Sink::walked_whole`, #2186),
//!   and the engine sweeps them as after a cursor-less run.
//!
//! One consequence worth stating plainly, because it looks like a bug and is
//! not: narrowing `owners[]`/`repos[]` retires the repositories that fell out
//! of scope. Since #2186 the next scheduled run does it: a position holding a
//! repository the scope no longer admits makes that run a full sync. They are
//! no longer part of this source's corpus, and `deleted_at` on the entity is
//! the mirror saying so --
//! the row, its links and its notes all survive. The other reading of an
//! incomplete walk -- a repository *skipped* rather than deselected -- cannot
//! reach the sweep at all, because a skip during a cursor-less run is fatal
//! (above).

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::OnceLock;

use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use futures_util::stream::FuturesUnordered;
use knobas_http::{PageNumber, PageWalk, Walked};
use knobas_source::{Cursor, Sink, SourceError, SyncItem};
use serde_json::Value;

use crate::client::is_repo_scoped;
use crate::cursor::{CommitsCatchUp, GiteaCursor, PullsCatchUp, RepoCursor};
use crate::map::{self, RepoRef};
use crate::model;

/// The runaway guard on the two exhaustive walks: at most this many requests
/// each, whatever the server chooses to put on a page.
///
/// **A request budget, and the arithmetic that sizes it.** The target has
/// always been a *record* count: this comment used to read "At 50 per page:
/// 1,000 repositories, and 1,000 branches per repository", and 20 was that
/// target divided by the 50 a request asks for. Since the empty-page rule a walk
/// spends one further request on the empty page that proves the listing ended,
/// so the same 1,000 costs 21 requests -- twenty pages of records, plus the one
/// that ends them. Keeping 20 would have kept the arithmetic and quietly shrunk
/// the target to 950; nobody ever chose 950. At 21 a corpus of exactly 1,000
/// walks cleanly, which the short-page rule never managed either: a full page
/// 20 was not proof there was no page 21, so 1,000 failed there too. Issue #81,
/// ruled 2026-08-29.
///
/// **A literal, not `1_000 / PAGE_SIZE + 1`.** Deriving it would put the
/// honoured-page-size assumption back into this file in executable form, which
/// is exactly what the empty-page rule took out of it. The guard is sized under an
/// assumption it does not depend on: against a server capping lower the real
/// capacity is lower, and a corpus past the guard still fails loudly with an
/// accurate count of what it walked ([`cap_reached`]) rather than being
/// silently truncated.
const MAX_LIST_PAGES: u32 = 21;
const MAX_BRANCH_PAGES: u32 = 21;
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
// and cannot be read downstream as a deletion. On an incremental run a
// reached cap cuts the walk short exactly as the budget does, and the next run
// continues it (#1624, [`pulls`]) -- except where a commit catch-up's capped
// pages all lay above its window, which only a server ignoring `until=` lists:
// that walk reads on from the next page (#2034, [`walk_commits`]).
//
// The empty page costs these two nothing, unlike the exhaustive pair: a budget
// of 1,000 is spent by the last record of page 20 and breaks the walk there,
// before any request for the empty page that would have confirmed the end.
// That is why these two stayed at 20 when `MAX_LIST_PAGES` and
// `MAX_BRANCH_PAGES` went to 21: a 21st request is one they never make.
/// Pull requests per repository, per run.
const MAX_PR_PAGES: u32 = 20;
/// Pages per walk of one branch. `commits_per_repo` is the whole-repository
/// budget the walk actually spends, and is what bites first. A catch-up over
/// a server that ignores `until=` opens a further walk past the pages above
/// its window, which spend no budget (#2034).
const MAX_COMMIT_PAGES: u32 = 20;
/// Issues per repository per run (#1206): 10,000 at 50 a page, plus the empty
/// page that ends them. Unbudgeted, so reaching it ends the run with
/// [`cap_reached`] rather than mirroring a part of the issues as if it were
/// all of them.
const MAX_ISSUE_PAGES: u32 = 201;

/// One repository this run will walk.
pub(crate) struct Selected {
    full_name: String,
    raw: Value,
    repo: model::Repo,
    /// From the listing, not named in `repos[]`: whether a 404 under it may be
    /// read as "deleted since the listing" (issue #728).
    listed: bool,
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
    /// Connectivity failures abort the run (battery clause 6); a refusal or an
    /// absence is about this repository alone. A walk's sink is its own
    /// buffer, which never refuses; a refusal from the run's sink arrives
    /// where `run` pushes the buffer, and aborts the run there.
    fn from(error: SourceError) -> Self {
        if is_repo_scoped(&error) {
            RepoError::Skip(error)
        } else {
            RepoError::Fatal(error)
        }
    }
}

/// A failure that ends the run inside one repository's walk, saying which.
///
/// Gitea's 404 body is `not found` for a missing repository and a missing
/// endpoint alike, so `HTTP 404 Not Found: not found` on its own names nothing
/// the user can go and look at. Only the message changes: the status is what
/// classifies the failure downstream, and the other variants carry either no
/// text worth prefixing (`Unauthorized`) or a fault that is not about the
/// repository (`Sink`, `Unreachable`).
fn in_repository(full_name: &str, error: SourceError) -> SourceError {
    match error {
        SourceError::Protocol { status, message } => SourceError::Protocol {
            status,
            message: format!("gitea: {full_name}: {message}"),
        },
        other => other,
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
/// records on every page -- the assumption the empty-page rule exists to remove.
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

// Where a listing ends is the transport's `PageWalk` over `PageNumber`
// (#1315): on an **empty** page, never a short one (#81, ruled 2026-08-29).
// `PAGE_SIZE` documents 50 as Gitea's *default* cap, a self-hosted instance
// can lower `MAX_RESPONSE_ITEMS`, an endpoint can carry its own maximum, and a
// loaded server can answer a partial page; under any of those a short-page
// rule stops the walk early and returns `Ok`, with the watermark advancing
// past every record after the stop. One walker for the five walks here, rather
// than five inline comparisons, is how none of them drifts.
//
// `fetch_comments` is deliberately not a sixth. Issue #131 read the
// discussion request as a paged listing that never paged, and the pinned
// container says it is not one: `issueGetComments` declares no `page` and no
// `limit` and ignores both, so "page until empty" there would re-read the same
// discussion until the budget ran out and fold it into `body_text` once per
// request. That read gets the server's own `X-Total-Count` instead -- see
// `GiteaClient::issue_comments` for the measurements.
//
// The price is one extra request per *exhausted* walk, under the 10 req/s
// limiter of interfaces §4.1 -- and the `MAX_*_PAGES` caps above are what keep
// "until empty" from becoming "until forever" against a server that ignores
// the `page` parameter.

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

/// The unit of the walk reading: the UI prints "120 of 300 repositories".
const REPOSITORIES: &str = "repositories";

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
    //
    // **A narrowed scope is a full sync too (#2186).** A position holding a
    // repository that `owners[]` or `repos[]` no longer admits was taken
    // under a wider scope, and only a run that walks the narrowed one whole
    // can tell the engine to retire what fell out of it. Its positions are
    // dropped, so every repository and branch left is emitted again.
    let recovered = GiteaCursor::parse(cursor.as_deref());
    let narrowed = recovered.as_ref().is_some_and(|position| {
        position
            .repos
            .keys()
            .any(|name| !in_scope(&source.config, name))
    });
    let recovered = recovered.filter(|_| !narrowed);
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

    // Up to `concurrent_repos` repositories walked side by side (#2159), all
    // in this one task: nothing is spawned, so returning drops every walk
    // still in flight, and the shared limiter stays the one bound on the
    // request rate. Each walk buffers its own items, and they reach the sink
    // when that repository finishes, in the order the walks finish: the
    // engine writes the run in one transaction and sorts each batch itself,
    // and every budget, page cap and watermark is per repository.
    //
    // The walk reading (#1379) counts finished repositories, over the
    // selection just made, so an unchanged run that pushes no item still moves
    // the bar. A `usize` always fits a `u64` here.
    let of = u64::try_from(walk.len()).unwrap_or(u64::MAX);
    // `GiteaConfig::validate` keeps the setting in 1..=32; the clamp only
    // keeps a zero, which would never start a walk, from reading as success.
    let at_once = usize::try_from(source.config.concurrent_repos)
        .unwrap_or(usize::MAX)
        .max(1);
    let mut queued = walk.iter();
    let mut in_flight = FuturesUnordered::new();
    let mut done = 0u64;
    sink.walked(done, Some(of), REPOSITORIES).await;
    loop {
        while in_flight.len() < at_once
            && let Some(selected) = queued.next()
        {
            in_flight.push(walk_repo(
                source,
                selected,
                previous.repo(&selected.full_name),
                full_sync,
            ));
        }
        let Some((selected, outcome)) = in_flight.next().await else {
            break;
        };
        match outcome {
            Outcome::Walked(after, items) => {
                for item in items {
                    push(sink, item, &mut emitted).await?;
                }
                walked += 1;
                next.repos.insert(selected.full_name.clone(), after);
            }
            Outcome::Fatal(error) => return Err(in_repository(&selected.full_name, error)),
            Outcome::Gone(error) => {
                tracing::info!(
                    repository = %selected.full_name,
                    %error,
                    "gitea: repository deleted since the listing; skipping it"
                );
            }
            Outcome::Skipped(error, before) => {
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
        done += 1;
        sink.walked(done, Some(of), REPOSITORIES).await;
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

    // A full sync emitted every repository and every branch of every walked
    // one (module docs), whatever the engine handed us: say so, so the
    // engine retires the rows this run did not return (#2186) -- a
    // repository deleted upstream, or one a narrowed scope left out. The
    // budgeted kinds were not walked whole and say nothing.
    if full_sync {
        sink.walked_whole(crate::KIND_REPO).await;
        sink.walked_whole(crate::KIND_BRANCH).await;
    }

    // Interfaces §4.1: a run that emitted nothing returns the cursor it was
    // handed, byte-identical. Re-serialising would be *equal* but not
    // *identical* -- `repos_listed_at` alone would move on every poll. That
    // holds for a narrowed run too: one that emitted nothing (a scope
    // narrowed to owners holding no repository) keeps the old position, and
    // the next run lists again and walks nothing, which costs the listing.
    if emitted == 0
        && let Some(unchanged) = cursor
    {
        return Ok(unchanged);
    }
    Ok(next.to_json())
}

/// How one repository's walk ended.
enum Outcome {
    /// Walked: its position for the next run and the items it emitted, in
    /// the order its walk emitted them.
    Walked(RepoCursor, Vec<SyncItem>),
    /// Ends the run.
    Fatal(SourceError),
    /// Listed a moment ago and deleted since: ruling B4 as amended by #728
    /// (module docs). It holds no position; its row is the next sweep's to
    /// retire.
    Gone(SourceError),
    /// A 403 or a 404 on an incremental run, and nothing else: a revoked
    /// credential answers 401, which `is_repo_scoped` reads as fatal and so
    /// never arrives here (ADR-0004). It carries the position the run came
    /// in with, which the run keeps.
    Skipped(SourceError, RepoCursor),
}

/// The items one repository's walk emits, held until that walk finishes and
/// the run hands them to its sink (#2159).
struct Buffered(Vec<SyncItem>);

#[async_trait::async_trait]
impl Sink for Buffered {
    async fn item(&mut self, item: SyncItem) -> Result<(), SourceError> {
        self.0.push(item);
        Ok(())
    }
}

/// Walk one repository into a buffer of its own and say how it ended, the
/// skip rules applied: what a 403 or 404 means depends on the repository
/// (`listed`) and on the run (`full_sync`), never on the other walks.
async fn walk_repo<'a>(
    source: &crate::GiteaSource,
    selected: &'a Selected,
    before: RepoCursor,
    full_sync: bool,
) -> (&'a Selected, Outcome) {
    let mut after = before.clone();
    let mut buffer = Buffered(Vec::new());
    // `run` counts what it pushes to its own sink; this count is the buffer's.
    let mut emitted = 0;
    let outcome = match sync_repo(
        source,
        selected,
        &before,
        &mut after,
        &mut buffer,
        &mut emitted,
    )
    .await
    {
        Ok(()) => Outcome::Walked(after, buffer.0),
        Err(RepoError::Fatal(error)) => Outcome::Fatal(error),
        Err(RepoError::Skip(error))
            if selected.listed
                && error.status() == Some(404)
                && repo_is_gone(source, selected).await =>
        {
            Outcome::Gone(error)
        }
        // A cursor-less run may not report success over a hole: see the
        // module docs.
        Err(RepoError::Skip(error)) if full_sync => Outcome::Fatal(error),
        Err(RepoError::Skip(error)) => Outcome::Skipped(error, before),
    };
    (selected, outcome)
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
        // **A full sync lists twice and walks both (#1979).** The listing
        // pages by number over `id`, which no push moves, but a repository
        // deleted (or unshared from the token) from a page already read
        // shifts the first row of the next page onto it, and no request
        // returns that row. `/repos/search` offers no keyset to page by
        // instead. A full sync's `Ok` licenses the sweep, which would retire
        // the repository and its branches, so it reads the listing again and
        // walks the union of the two: a repository missed by one listing is in the other
        // unless a second deletion lands on the same row in the second. One
        // that is gone by then answers 404 when walked, and is skipped as
        // deleted since the listing. An incremental run licenses no sweep;
        // a repository its listing missed keeps no position this run, and
        // the next run reads it whole.
        let listings = if full_sync { 2 } else { 1 };
        for _ in 0..listings {
            let mut walk = PageWalk::new(PageNumber, MAX_LIST_PAGES, |page| {
                source.client.search_repos(page)
            });
            loop {
                match walk.next().await? {
                    Walked::Page(batch) => {
                        for raw in batch {
                            push_selected(&mut out, raw, config, true);
                        }
                    }
                    Walked::End => break,
                    Walked::Capped => return Err(cap_reached("repositories", walk.rows())),
                }
            }
        }
    } else {
        for entry in &config.repos {
            // `GiteaConfig::validate` has already refused anything else.
            let Some((owner, name)) = entry.split_once('/') else {
                continue;
            };
            match source.client.get_repo(owner, name).await {
                Ok(raw) => push_selected(&mut out, raw, config, false),
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

    // Deterministic: the commit budget is spent in this order. A stable sort,
    // so of a row the listing repeated (`client::search_repos`, #1624) the
    // first read is the one kept, and it is walked once.
    out.sort_by(|a, b| a.full_name.cmp(&b.full_name));
    out.dedup_by(|later, first| later.full_name == first.full_name);
    Ok(Selection {
        walk: out,
        skipped,
        first_skip: skip,
    })
}

fn push_selected(
    out: &mut Vec<Selected>,
    raw: Value,
    config: &crate::config::GiteaConfig,
    listed: bool,
) {
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
    if !admits(config, owner, &repo.full_name) {
        return;
    }
    out.push(Selected {
        full_name: repo.full_name.clone(),
        raw,
        repo,
        listed,
    });
}

/// Whether `owners[]` and `repos[]` admit the repository `full_name` of
/// `owner`, case-insensitively as Gitea names them. An empty list admits all.
fn admits(config: &crate::config::GiteaConfig, owner: &str, full_name: &str) -> bool {
    let owner_ok = config.owners.is_empty()
        || config
            .owners
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(owner));
    let repo_ok = config.repos.is_empty()
        || config
            .repos
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(full_name));
    owner_ok && repo_ok
}

/// [`admits`] for a name the cursor keeps, `owner/name`. A key without an
/// owner part is not this adapter's to judge and counts as in scope.
fn in_scope(config: &crate::config::GiteaConfig, full_name: &str) -> bool {
    full_name
        .split_once('/')
        .is_none_or(|(owner, _)| admits(config, owner, full_name))
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
        default_branch: selected.repo.default_branch.as_deref(),
    };

    // 1. The repository itself. Its `updated_at` is to the second, so an edit
    //    inside the second the previous run read it would not move it (#764):
    //    the record's fingerprint is what decides, and the timestamp stays
    //    beside it for the cursor's reader.
    let updated_at = map::real_time(selected.repo.updated_at);
    let fingerprint = knobas_source::fingerprint::of(&selected.raw);
    if before.repo_updated_at != updated_at
        || before.repo_fingerprint.as_deref() != Some(fingerprint.as_str())
    {
        // Its teams (#966), read only when the record is written: a team
        // change shows with the repository's next change, or the next full
        // sync, at one request per changed repository rather than one per
        // repository per run.
        let teams = source.client.repo_teams(owner, name).await?;
        push(
            sink,
            map::repo_item(
                &source.id,
                &selected.raw,
                &selected.repo,
                at,
                teams.as_deref(),
            ),
            emitted,
        )
        .await?;
    }
    after.repo_updated_at = updated_at;
    after.repo_fingerprint = Some(fingerprint);

    // 2. Branches, and the ones that vanished. Step 4 walks commits for
    //    exactly the branches this reports as moved.
    let moved = branches(source, at, before, after, sink, emitted).await?;

    // 3. Pull requests, newest-updated first, down to the watermark.
    //    Not where the record says Gitea would answer 404 (a mirror, an empty
    //    repository, the unit switched off), which a cursor-less run would
    //    otherwise read as a hole and fail on.
    pulls(source, at, &selected.repo, before, after, sink, emitted).await?;

    // 3a. Issues, as tickets (#1206), down to their own watermark.
    issues(source, at, &selected.repo, before, after, sink, emitted).await?;

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
    // Every branch's stored `repository` names the default (#718), so a
    // switched default is a change to every branch, moved or not.
    let default_changed = before.default_branch.as_deref() != at.default_branch;
    after.default_branch = at.default_branch.map(str::to_owned);

    let mut records: Vec<Value> = Vec::new();
    let mut pages = 0u32;
    // A lock only because the walk's future must be `Send`; page 1 sets it.
    let first_count: OnceLock<Option<usize>> = OnceLock::new();
    let mut walk = PageWalk::new(PageNumber, MAX_BRANCH_PAGES, |page| {
        let first_count = &first_count;
        async move {
            let (batch, total) = source.client.branches(at.owner, at.name, page).await?;
            if page == 1 {
                let _ = first_count.set(total);
            }
            Ok::<_, SourceError>(batch)
        }
    });
    loop {
        match walk.next().await? {
            Walked::Page(batch) => {
                pages += 1;
                records.extend(batch);
            }
            Walked::End => break,
            Walked::Capped => {
                return Err(RepoError::Fatal(cap_reached(
                    "branches in one repository",
                    walk.rows(),
                )));
            }
        }
    }

    // **A name the listing did not return is not yet a deleted branch
    // (#1624).** Gitea orders its branch listing by commit time, so a push
    // while the walk is between two pages moves that branch onto a page
    // already read, and the walk never meets it. Its own read decides: a
    // 404 is a branch that is gone, a record is a branch that moved, and
    // anything else ends the repository's walk as any failed read does.
    //
    // **Nor is a run without a previous cursor safe from it (#1716).** It
    // has no names to re-read, and the sweep retires every branch it does
    // not emit. So a walk that took more than one page also asks git for
    // every branch name at once, a snapshot no push or deletion reorders,
    // and reads each one the pages missed on its own. (A deletion hides a
    // branch too: everything below it shifts up a place, and the first
    // branch of the next page slides onto the page already read.) One
    // page is one query, and nothing can move inside it -- but a walk that
    // read one page is not always a one-page listing: where a deletion
    // empties page 2, the branch page 2 held slid onto page 1 after it was
    // read. Page 1's own count of the listing tells the two apart.
    let listed: HashSet<String> = records
        .iter()
        .filter_map(|raw| raw.get("name").and_then(Value::as_str))
        .map(str::to_owned)
        .collect();
    let mut unlisted: BTreeSet<String> = before
        .branches
        .keys()
        .filter(|name| !listed.contains(name.as_str()))
        .cloned()
        .collect();
    let more_than_a_page = pages > 1
        || first_count
            .get()
            .copied()
            .flatten()
            .is_some_and(|total| total > records.len());
    if more_than_a_page {
        let names = source
            .client
            .branch_names(at.owner, at.name)
            .await
            .map_err(RepoError::from)?;
        unlisted.extend(names.into_iter().filter(|name| !listed.contains(name)));
    }
    let mut gone = Vec::new();
    for name in unlisted {
        match source
            .client
            .branch_as::<Value>(at.owner, at.name, &name)
            .await
        {
            Ok(raw) => records.push(raw),
            // Only a branch the cursor remembers has a row to retire; one
            // git named a moment ago and now 404s was never emitted.
            Err(error) if error.status() == Some(404) => {
                if before.branches.contains_key(&name) {
                    gone.push(name);
                }
            }
            Err(error) => return Err(RepoError::from(error)),
        }
    }

    for raw in records {
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
        // A branch the listing returned twice keeps its first record, so
        // the head emitted and the head remembered are the same; a head that
        // moved between the two reads is the next run's.
        if heads.contains_key(&branch.name) {
            continue;
        }
        let head = branch
            .commit
            .as_ref()
            .and_then(|c| c.id.clone())
            .unwrap_or_default();
        let head_moved = before.branches.get(&branch.name) != Some(&head);
        // An unmoved branch under a switched default is re-emitted for
        // its `repository` (#718), and no commit is walked for it.
        if head_moved || default_changed {
            push(
                sink,
                map::branch_item(&source.id, &raw, at, &branch),
                emitted,
            )
            .await?;
        }
        if head_moved {
            moved.push(branch.name.clone());
        }
        heads.insert(branch.name, head);
    }

    for name in &gone {
        push(sink, map::branch_tombstone(&source.id, at, name), emitted).await?;
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
///
/// # On an incremental run the budget bounds the run, never the delta (#1624)
///
/// ADR-0003's budget is a bound on what a **full** sync mirrors. Applied to
/// an incremental run it used to lose updates: a week away with 260 pull
/// requests changed, the walk read the newest 200, the watermark moved to the
/// newest, and the other 60 sat below it for good -- a merged pull request
/// stayed open in the mirror. Now a walk heading for a watermark that the
/// budget (or the page cap) stops first leaves a [`PullsCatchUp`]: the next
/// run reads the window between the oldest record examined and that
/// watermark, under the same budget, and when it reaches the watermark it
/// adopts the newest instant the cut-short walk examined and walks on from
/// the top with what is left of the budget. Spec #1618 ruling A6; the cursor
/// type says why the oldest examined is where the next run continues and the
/// watermark stays put.
async fn pulls(
    source: &crate::GiteaSource,
    at: RepoRef<'_>,
    repo: &model::Repo,
    before: &RepoCursor,
    after: &mut RepoCursor,
    sink: &mut (dyn Sink + Send),
    emitted: &mut u64,
) -> Result<(), RepoError> {
    // Carried first, so every early return below leaves the position it came
    // in with rather than a blank one.
    after.pulls_updated_to = before.pulls_updated_to;
    after.pulls_seen_at_watermark = before.pulls_seen_at_watermark.clone();
    after.pulls_catching_up = before.pulls_catching_up.clone();
    if source.config.prs_per_repo == 0 || !repo.can_have_pulls() {
        return Ok(());
    }
    let mut budget = source.config.prs_per_repo;
    let mut watermark = before.pulls_updated_to;
    let mut seen = before.pulls_seen_at_watermark.clone();

    if let Some(catch_up) = &before.pulls_catching_up {
        let window = Window {
            lower: before.pulls_updated_to,
            lower_seen: &before.pulls_seen_at_watermark,
            upper: catch_up.below,
            upper_seen: &catch_up.seen_at_below,
        };
        let walk = walk_pulls(
            source,
            at,
            &window,
            catch_up.page,
            &mut budget,
            sink,
            emitted,
        )
        .await?;
        if let Some(page) = walk.stopped_on {
            let carried: Vec<u64> = catch_up.seen_at_below.keys().copied().collect();
            let (below, on_below) = resume_below(&walk.examined, window.upper, &carried);
            after.pulls_catching_up = Some(PullsCatchUp {
                below,
                seen_at_below: fingerprints_of(
                    on_below,
                    &walk.fingerprints,
                    &catch_up.seen_at_below,
                ),
                page,
                ..catch_up.clone()
            });
            return Ok(());
        }
        // Reached the watermark: everything up to `newest` is delivered, and
        // the run walks on from the top as an ordinary incremental run.
        watermark = Some(catch_up.newest);
        seen.clone_from(&catch_up.seen_at_newest);
        for (number, when) in &walk.examined {
            if *when == watermark
                && let Some(fingerprint) = walk.fingerprints.get(number)
            {
                seen.insert(*number, fingerprint.clone());
            }
        }
        after.pulls_updated_to = watermark;
        after.pulls_seen_at_watermark.clone_from(&seen);
        after.pulls_catching_up = None;
    }

    let none = BTreeMap::new();
    let window = Window {
        lower: watermark,
        lower_seen: &seen,
        upper: None,
        upper_seen: &none,
    };
    let walk = walk_pulls(source, at, &window, 1, &mut budget, sink, emitted).await?;
    let carried: Vec<u64> = seen.keys().copied().collect();
    let (newest, on_newest) = close_watermark(&walk.examined, watermark, &carried);
    let on_newest = fingerprints_of(on_newest, &walk.fingerprints, &seen);
    match (walk.stopped_on, watermark, newest) {
        // An incremental walk cut short before its watermark: continue below
        // the oldest record it examined next time.
        // The watermark stays where it is.
        (Some(page), Some(_), Some(newest)) => {
            let (below, on_below) = resume_below(&walk.examined, None, &[]);
            after.pulls_catching_up = Some(PullsCatchUp {
                below,
                seen_at_below: fingerprints_of(on_below, &walk.fingerprints, &seen),
                newest,
                seen_at_newest: on_newest,
                page,
            });
        }
        // A full sync (the budget bounds the mirror, ADR-0003), or a walk
        // that reached its watermark.
        _ => {
            after.pulls_updated_to = newest;
            after.pulls_seen_at_watermark = on_newest;
        }
    }
    Ok(())
}

/// The span of a budgeted walk, newest first: from `upper` (inclusive) down to
/// `lower` (inclusive), each boundary second with the records already
/// delivered on it. `None` is no bound on that side.
struct Window<'a, S> {
    lower: Option<DateTime<Utc>>,
    lower_seen: &'a S,
    upper: Option<DateTime<Utc>>,
    upper_seen: &'a S,
}

/// What one pull-request walk read.
struct PullWalk {
    /// Every pull request looked at -- delivered, or recognised as already
    /// delivered on the lower boundary.
    examined: Vec<(u64, Option<DateTime<Utc>>)>,
    fingerprints: BTreeMap<u64, String>,
    /// The page the budget or the page cap stopped the walk on, before it
    /// reached `lower` or the end of the listing.
    stopped_on: Option<u32>,
}

/// One walk over `window`, starting at page `start`.
///
/// **`start` is a hint, checked before it is trusted.** A catch-up's window
/// lies below every record the cut-short walk delivered, and re-reading those
/// pages from page 1 would spend the page cap on records it then skips -- at
/// a budget of 1,000, the whole cap. So it starts on the page the cut fell
/// on. That is safe only if every record above that page is newer than
/// `upper`, which holds when the page's first record is: the listing is
/// ordered. An edit moves a record up, which shifts the rest down and only
/// repeats a row; a deletion, or a server answering more per page than it
/// did, can shift unread records above the page. Either shows as a first
/// record at or below `upper`, or an empty page, and the walk starts again
/// from page 1.
async fn walk_pulls(
    source: &crate::GiteaSource,
    at: RepoRef<'_>,
    window: &Window<'_, BTreeMap<u64, String>>,
    start: u32,
    budget: &mut u32,
    sink: &mut (dyn Sink + Send),
    emitted: &mut u64,
) -> Result<PullWalk, RepoError> {
    let mut out = PullWalk {
        examined: Vec::new(),
        fingerprints: BTreeMap::new(),
        stopped_on: None,
    };
    let client = &source.client;
    let mut start = start.max(1);
    'restart: loop {
        let offset = start - 1;
        // A reached cap ends this walk quietly: see the note on the caps.
        let mut walk = PageWalk::new(PageNumber, MAX_PR_PAGES, |page| {
            client.pulls(at.owner, at.name, page + offset)
        });
        let mut page = offset;
        loop {
            let batch = match walk.next().await? {
                Walked::Page(batch) => batch,
                Walked::End if page == offset && offset > 0 => {
                    start = 1;
                    continue 'restart;
                }
                Walked::End => return Ok(out),
                Walked::Capped => {
                    out.stopped_on = Some(page);
                    return Ok(out);
                }
            };
            if page == offset && offset > 0 {
                let first = batch
                    .first()
                    .and_then(|raw| serde_json::from_value::<model::PullRequest>(raw.clone()).ok())
                    .and_then(|pr| {
                        map::real_time(pr.updated_at).or_else(|| map::real_time(pr.created_at))
                    });
                if !matches!((first, window.upper), (Some(first), Some(upper)) if first > upper) {
                    start = 1;
                    continue 'restart;
                }
            }
            page += 1;
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
                let updated =
                    map::real_time(pr.updated_at).or_else(|| map::real_time(pr.created_at));
                // Taken from the listing record before the discussion is read, so
                // deciding to skip costs no request.
                let fingerprint = crate::fingerprint::of_pull(&raw);
                if let (Some(top), Some(when)) = (window.upper, updated) {
                    // Above the window: delivered by the walk that was cut
                    // short, or changed since and read once the window closes.
                    if when > top
                        || (when == top && window.upper_seen.get(&pr.number) == Some(&fingerprint))
                    {
                        continue;
                    }
                }
                if let (Some(mark), Some(when)) = (window.lower, updated) {
                    if when < mark {
                        return Ok(out);
                    }
                    // The watermark second is re-read on every run; a pull request
                    // on it is skipped only if it is the record already delivered.
                    // By number alone, a merge inside that second was skipped for
                    // good (#764).
                    if when == mark && window.lower_seen.get(&pr.number) == Some(&fingerprint) {
                        out.examined.push((pr.number, updated));
                        out.fingerprints.insert(pr.number, fingerprint);
                        continue;
                    }
                }
                let Some(comments) = fetch_comments(source, at, &pr).await? else {
                    // Deleted since the listing (#724): nothing to mirror, and
                    // nothing to hold the watermark open either.
                    continue;
                };
                out.examined.push((pr.number, updated));
                out.fingerprints.insert(pr.number, fingerprint);
                push(
                    sink,
                    map::pr_item(&source.id, &raw, at, &pr, &comments),
                    emitted,
                )
                .await?;
                *budget -= 1;
                if *budget == 0 {
                    out.stopped_on = Some(page);
                    return Ok(out);
                }
            }
        }
    }
}

/// This run's fingerprint for each of `numbers` where it read the pull
/// request, `carried`'s where it did not.
fn fingerprints_of(
    numbers: Vec<u64>,
    read: &BTreeMap<u64, String>,
    carried: &BTreeMap<u64, String>,
) -> BTreeMap<u64, String> {
    numbers
        .into_iter()
        .filter_map(|number| {
            read.get(&number)
                .or_else(|| carried.get(&number))
                .map(|fingerprint| (number, fingerprint.clone()))
        })
        .collect()
}

/// Issues changed since the watermark, as tickets (#1206).
///
/// **Every page `since` admits is read.** Gitea's issue listing ignores
/// `sort=recentupdate` (it answers newest *created* first, measured on
/// 1.27.3), so unlike [`pulls`] there is no order to stop early on. `since` is
/// server-side and inclusive, so the watermark second comes back on every run
/// and `issues_seen_at_watermark` keeps it from being re-delivered, by
/// fingerprint as the pull requests' does (#764). An idle repository costs one
/// request: the empty page.
///
/// **A refused listing skips this repository's issues, on either kind of
/// run**, with a warning, and keeps their watermark: a 403 is a token without
/// issue scope and a 404 a repository whose issue unit is off or points at an
/// external tracker. Neither is the code host's failure, and a sync that
/// worked before #1206 must not start failing on issues it never read. That
/// is why `ticket` is not exhaustive (`crate::entity_kinds`). A 401 is the
/// run's, as everywhere.
async fn issues(
    source: &crate::GiteaSource,
    at: RepoRef<'_>,
    repo: &model::Repo,
    before: &RepoCursor,
    after: &mut RepoCursor,
    sink: &mut (dyn Sink + Send),
    emitted: &mut u64,
) -> Result<(), RepoError> {
    after.issues_updated_to = before.issues_updated_to;
    after.issues_seen_at_watermark = before.issues_seen_at_watermark.clone();
    if !repo.has_issues {
        return Ok(());
    }
    let watermark = before.issues_updated_to;
    let mut examined: Vec<(u64, Option<DateTime<Utc>>)> = Vec::new();
    let mut fingerprints: BTreeMap<u64, String> = BTreeMap::new();
    let mut walk = PageWalk::new(PageNumber, MAX_ISSUE_PAGES, |page| {
        source.client.issues(at.owner, at.name, watermark, page)
    });
    loop {
        let batch: Vec<Value> = match walk.next().await {
            Ok(Walked::Page(batch)) => batch,
            Ok(Walked::End) => break,
            Ok(Walked::Capped) => {
                return Err(RepoError::Fatal(cap_reached(
                    "issues in one repository",
                    walk.rows(),
                )));
            }
            Err(error) if is_repo_scoped(&error) => {
                tracing::warn!(
                    repository = %at.full_name,
                    %error,
                    "gitea: not reading this repository's issues for this run"
                );
                return Ok(());
            }
            Err(error) => return Err(RepoError::Fatal(error)),
        };
        for raw in batch {
            let issue = match serde_json::from_value::<model::Issue>(raw.clone()) {
                Ok(issue) if issue.pull_request.is_none() => issue,
                Ok(_) => continue,
                Err(error) => {
                    tracing::warn!(
                        %error,
                        repository = %at.full_name,
                        "gitea: skipping an unreadable issue"
                    );
                    continue;
                }
            };
            let updated =
                map::real_time(issue.updated_at).or_else(|| map::real_time(issue.created_at));
            let fingerprint = knobas_source::fingerprint::of(&raw);
            if watermark.is_some()
                && updated == watermark
                && before.issues_seen_at_watermark.get(&issue.number) == Some(&fingerprint)
            {
                examined.push((issue.number, updated));
                fingerprints.insert(issue.number, fingerprint);
                continue;
            }
            let Some(comments) =
                discussion(source, at, issue.number, issue.comments, Thread::Issue).await?
            else {
                continue;
            };
            examined.push((issue.number, updated));
            fingerprints.insert(issue.number, fingerprint);
            push(
                sink,
                map::ticket_item(&source.id, &raw, at, &issue, &comments),
                emitted,
            )
            .await?;
        }
    }

    let carried: Vec<u64> = before.issues_seen_at_watermark.keys().copied().collect();
    let (mark, on_mark) = close_watermark(&examined, watermark, &carried);
    after.issues_updated_to = mark;
    after.issues_seen_at_watermark = on_mark
        .into_iter()
        .filter_map(|number| {
            fingerprints
                .get(&number)
                .or_else(|| before.issues_seen_at_watermark.get(&number))
                .map(|fingerprint| (number, fingerprint.clone()))
        })
        .collect();
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

/// Where a walk the budget cut short continues next time: the oldest instant
/// it examined, and the keys on it (#1624).
///
/// `upper` is the top of the window it walked, if it had one, and `carried`
/// the keys already delivered on that instant: when everything this walk
/// examined sat on `upper`, they still sit on the resume point. A walk that
/// examined nothing resumes where it began.
fn resume_below<K: Ord + Clone>(
    examined: &[(K, Option<DateTime<Utc>>)],
    upper: Option<DateTime<Utc>>,
    carried: &[K],
) -> (Option<DateTime<Utc>>, Vec<K>) {
    let point = examined
        .iter()
        .filter_map(|(_, when)| *when)
        .min()
        .or(upper);
    let Some(at) = point else {
        return (None, Vec::new());
    };
    let mut keys: Vec<K> = examined
        .iter()
        .filter(|(_, when)| *when == Some(at))
        .map(|(key, _)| key.clone())
        .collect();
    if point == upper {
        keys.extend(carried.iter().cloned());
    }
    keys.sort();
    keys.dedup();
    (point, keys)
}

/// The discussion, when there is any and the source wants it indexed.
///
/// Gitea keeps a pull request's discussion on the **issue** of the same index,
/// which is the fifth read endpoint ruling B1 granted. `pr.comments == 0` is
/// what makes an idle-ish run cheap: no discussion, no request.
///
/// # Why one request is enough here, and what proves it every time
///
/// Issue #131 read this as the fifth paged listing: no `limit`, no `page`, and
/// therefore `DEFAULT_PAGING_NUM` records -- thirty on a stock Gitea -- with a
/// discussion silently cut off there. Measured against the pinned container it
/// is not a paged endpoint at all, and paging it would have been the worse bug
/// of the two: `page` is ignored, so the walk would have re-read the same
/// discussion until its budget ran out and folded every comment into
/// `body_text` once per request. The three measurements are on
/// [`crate::client::GiteaClient::issue_comments`], with the endpoint's own
/// OpenAPI declaration and a same-minute control on the repository-wide
/// comments endpoint, which does page and does honour `limit`.
///
/// So the shape #131 asked for is refused and the risk it was filed about is
/// answered instead. **The completeness of a discussion is checked, not
/// assumed**: Gitea sends `X-Total-Count`, it equalled the record count of
/// every discussion in the fixture under every query, and a read where it does
/// **not** ends the run.
///
/// Ending the run is the deliberate part, and it is the one thing here that is
/// not "search text on one item". A short discussion means the measured
/// premise of this whole function is false on that server -- and it would be
/// false for *every* discussion it ever reads, so what looks like one blemish
/// is a `body_text` that is quietly wrong across the source, on the very field
/// interfaces §4.1 defines as title + description + comment texts and that a
/// `comment` write op's hold detection reads. There is no second request to
/// recover with, because there is no second page to ask for; the honest
/// answers are to stop, or to turn the discussion off. The message offers the
/// second (`include_pr_comments`), and any Gitea that needs it is a Gitea this
/// adapter has to be re-measured against -- exactly the treatment issue #114
/// gave TeamCity's unpaged `/app/rest/buildTypes`.
///
/// **The price is nothing.** One request per emitted pull request that has a
/// discussion, unchanged since M1, and none at all for one that has none.
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
///
/// # A pull request deleted since the listing (issue #724)
///
/// `None` is that answer: the pull request was listed, then deleted before its
/// discussion was read, and the caller skips it. Gitea 1.27.3 does not say so
/// directly -- the discussion read of a deleted index answers **500** `issue
/// does not exist`, the same status as any server fault (measured 2026-10-13).
/// So a 404 or a 5xx here is followed by one read of the pull request itself,
/// and only *its* 404 means "deleted". A pull request that is still there
/// keeps the treatment above: a 404 costs its discussion, a 5xx ends the run.
async fn fetch_comments(
    source: &crate::GiteaSource,
    at: RepoRef<'_>,
    pr: &model::PullRequest,
) -> Result<Option<Vec<model::Comment>>, RepoError> {
    discussion(source, at, pr.number, pr.comments, Thread::Pull).await
}

/// Whose discussion [`discussion`] reads: which read confirms it is gone.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Thread {
    Pull,
    /// An issue's (#1206), read under the same `include_pr_comments`: Gitea
    /// keeps both on the issue endpoint, and one switch turns that off.
    Issue,
}

/// [`discussion`] for a refresh (#2098, `crate::refresh`): the one item's
/// discussion read as a run reads it, `None` for an item deleted since its
/// own read, and a failure the [`SourceError`] it carries, a refused read
/// costing the discussion and nothing else as it does in a run.
pub(crate) async fn discussion_of(
    source: &crate::GiteaSource,
    at: RepoRef<'_>,
    number: u64,
    count: u64,
    of: Thread,
) -> Result<Option<Vec<model::Comment>>, SourceError> {
    discussion(source, at, number, count, of)
        .await
        .map_err(|(RepoError::Fatal(error) | RepoError::Skip(error))| error)
}

/// [`fetch_comments`]'s reading, for a pull request or an issue `number`
/// with `count` comments.
async fn discussion(
    source: &crate::GiteaSource,
    at: RepoRef<'_>,
    number: u64,
    count: u64,
    of: Thread,
) -> Result<Option<Vec<model::Comment>>, RepoError> {
    if !source.config.include_pr_comments || count == 0 {
        return Ok(Some(Vec::new()));
    }
    let discussion = match source
        .client
        .issue_comments(at.owner, at.name, number)
        .await
    {
        Ok(discussion) => discussion,
        Err(error)
            if matches!(error.status(), Some(404 | 500..=599))
                && match of {
                    Thread::Pull => pull_is_gone(source, at, number).await,
                    Thread::Issue => issue_is_gone(source, at, number).await,
                } =>
        {
            tracing::info!(
                repository = %at.full_name,
                number,
                %error,
                "gitea: deleted since the listing; skipping it"
            );
            return Ok(None);
        }
        Err(error) if is_repo_scoped(&error) => {
            tracing::warn!(
                repository = %at.full_name,
                number,
                %error,
                "gitea: indexing this item without its discussion; \
                 set include_pr_comments to false to stop asking"
            );
            return Ok(Some(Vec::new()));
        }
        Err(error) => return Err(RepoError::from(error)),
    };
    // Against the **records the server sent**, not against the comments that
    // parsed: an unreadable comment is dropped below on purpose, and counting
    // that as truncation would fail the run over a field Gitea added.
    // `pr.comments` is not the comparison either -- it read 3 on the fixture's
    // `#142` where the endpoint sent 2, so it counts something this endpoint
    // does not return.
    if let Some(total) = discussion.total
        && total > discussion.raw.len()
    {
        return Err(RepoError::Fatal(SourceError::protocol(format!(
            "gitea: {}#{} answered {} of its {total} comments (its own X-Total-Count header \
             says {total}) and this endpoint has no second page to ask for, so every long \
             discussion on this instance would be mirrored incomplete without saying so. Set \
             include_pr_comments to false to index pull requests and issues without their \
             discussion.",
            at.full_name,
            number,
            discussion.raw.len(),
        ))));
    }
    // A single unreadable comment is dropped rather than failing the
    // pull request: the rest of the discussion is still worth indexing.
    Ok(Some(
        discussion
            .raw
            .into_iter()
            .filter_map(|c| serde_json::from_value(c).ok())
            .collect(),
    ))
}

/// Whether the repository itself now answers 404. As [`pull_is_gone`]:
/// anything else is "not shown to be gone".
async fn repo_is_gone(source: &crate::GiteaSource, selected: &Selected) -> bool {
    let (owner, name) = selected
        .repo
        .owner_repo()
        .expect("selection checked the name");
    matches!(
        source.client.get_repo(owner, name).await,
        Err(error) if error.status() == Some(404)
    )
}

/// Whether the branch itself now answers 404. As [`pull_is_gone`]: anything
/// else is "not shown to be gone".
async fn branch_is_gone(source: &crate::GiteaSource, at: RepoRef<'_>, branch: &str) -> bool {
    matches!(
        source.client.branch_as::<Value>(at.owner, at.name, branch).await,
        Err(error) if error.status() == Some(404)
    )
}

/// Whether the pull request itself now answers 404. Any other answer, failures
/// included, is "not shown to be gone": the caller then treats the read that
/// failed as it would have without asking.
async fn pull_is_gone(source: &crate::GiteaSource, at: RepoRef<'_>, number: u64) -> bool {
    matches!(
        source.client.pull(at.owner, at.name, number).await,
        Err(error) if error.status() == Some(404)
    )
}

/// Whether the issue itself now answers 404. As [`pull_is_gone`].
async fn issue_is_gone(source: &crate::GiteaSource, at: RepoRef<'_>, number: u64) -> bool {
    matches!(
        source.client.issue(at.owner, at.name, number).await,
        Err(error) if error.status() == Some(404)
    )
}

/// New commits on the branches whose heads moved this run.
///
/// Gitea's `since=` is inclusive and server-side, so the boundary commits come
/// back on every run (Forgejo 17 ignores it and `until=`, #2034, and the walk
/// filters both bounds itself); `commits_at_watermark` is what keeps them from
/// being re-delivered. Commits are immutable, so a delivered object id is delivered
/// for good -- which is why this boundary, unlike the pull requests', needs no
/// fingerprint (#764): nothing about a commit can change inside its second,
/// and a new commit in that second has an object id the set does not hold.
/// Branches have no timestamp watermark at all; they compare head object ids.
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
///
/// **A walk the budget cuts short on an incremental run is continued (#1624).**
/// A push of 150 commits against a budget of 100 used to lose the oldest 50
/// for good: the watermark moved past them, and the branch head the cursor
/// remembered had already moved, so nothing walked that branch again. Now
/// the cut leaves a [`CommitsCatchUp`] naming the branches not finished; the
/// next run walks them first, the cut one from below the oldest commit
/// examined down to the watermark and the rest from their heads,
/// and then the branches that moved since, as [`pulls`] does. A full sync's
/// budget is unchanged (ADR-0003).
///
/// **On a server that ignores `until=` the cut branch starts on the page the
/// cut fell on (#2039).** Forgejo 17 lists a catch-up's branch from its head,
/// so without that every run re-read what the runs before it delivered: a
/// catch-up of N commits under a budget of B read about N²/2B records.
/// [`CommitsCatchUp`]'s `page` says where to go, and [`walk_commits`] goes
/// there only once the branch's first page shows the server listed above
/// the window.
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
    after.commits_catching_up = before.commits_catching_up.clone();
    // `repo.empty` is the one that saves a request rather than a mistake:
    // Gitea answers `/commits` on a repository with no commits at all with a
    // 409, which `client::commits` already reads as "none".
    if source.config.commits_per_repo == 0
        || repo.empty
        || (moved.is_empty() && before.commits_catching_up.is_none())
    {
        return Ok(());
    }

    let mut budget = source.config.commits_per_repo;
    // One object id can be reachable from several branches; it is one entity
    // and must cost one slot of the budget.
    let mut seen: HashSet<String> = HashSet::new();
    let mut since = before.commits_since;
    let mut at_mark = before.commits_at_watermark.clone();
    // Branches this run walked from their heads to the watermark already, and
    // what it examined doing so.
    let mut finished: Vec<String> = Vec::new();
    let mut examined: Vec<(String, Option<DateTime<Utc>>)> = Vec::new();

    if let Some(catch_up) = &before.commits_catching_up {
        let none = Vec::new();
        let legs: Vec<Leg<'_>> = catch_up
            .branches
            .iter()
            .enumerate()
            .map(|(index, branch)| {
                // The cut branch is read below where the cut fell, the rest
                // from their heads.
                let (upper, upper_seen, hint) = if index == 0 {
                    (catch_up.below, &catch_up.seen_at_below, catch_up.page)
                } else {
                    (None, &none, None)
                };
                Leg {
                    branch: branch.clone(),
                    window: Window {
                        lower: before.commits_since,
                        lower_seen: &before.commits_at_watermark,
                        upper,
                        upper_seen,
                    },
                    hint,
                }
            })
            .collect();
        let walk = walk_commits(source, at, &legs, &mut budget, &mut seen, sink, emitted).await?;
        if let Some(stop) = walk.stopped {
            let (newest, seen_at_newest) = close_watermark(
                &walk.examined,
                Some(catch_up.newest),
                &catch_up.seen_at_newest,
            );
            let leg = &legs[stop.leg];
            let (below, seen_at_below) =
                resume_below(&stop.examined, leg.window.upper, leg.window.upper_seen);
            let mut branches: Vec<String> =
                legs[stop.leg..].iter().map(|l| l.branch.clone()).collect();
            // What moved this run is walked from its head next time, unless
            // this run already walked it from its head to the watermark.
            for name in walk_order(repo.default_branch.as_deref(), moved) {
                let done = legs.iter().take(stop.leg).skip(1).any(|l| l.branch == name);
                if !done && !branches.contains(&name) {
                    branches.push(name);
                }
            }
            after.commits_catching_up = Some(CommitsCatchUp {
                below,
                seen_at_below,
                newest: newest.unwrap_or(catch_up.newest),
                seen_at_newest,
                branches,
                page: Some(stop.page),
            });
            return Ok(());
        }
        // Reached the watermark on every branch: everything up to `newest` is
        // delivered. What moved since is walked from `newest`, not from the
        // newest commit just read on a branch walked from its head, which
        // would pass over a moved branch's commits between the two. (A commit
        // pushed later with an older committer date is passed over all the
        // same: the known limitation every `since=` walk here has.)
        since = Some(catch_up.newest);
        at_mark.clone_from(&catch_up.seen_at_newest);
        finished.extend(legs.iter().skip(1).map(|l| l.branch.clone()));
        // Only what reaches `newest` can move the watermark; the window's own
        // commits sit below it.
        examined = walk
            .examined
            .into_iter()
            .filter(|(_, when)| *when >= since)
            .collect();
    }

    let none = Vec::new();
    let legs: Vec<Leg<'_>> = walk_order(repo.default_branch.as_deref(), moved)
        .into_iter()
        .filter(|name| !finished.contains(name))
        .map(|branch| Leg {
            branch,
            window: Window {
                lower: since,
                lower_seen: &at_mark,
                upper: None,
                upper_seen: &none,
            },
            hint: None,
        })
        .collect();
    let walk = walk_commits(source, at, &legs, &mut budget, &mut seen, sink, emitted).await?;
    examined.extend(walk.examined);
    let (newest, on_newest) = close_watermark(&examined, since, &at_mark);
    after.commits_catching_up = None;
    match (walk.stopped, since, newest) {
        // An incremental walk cut short before its watermark: continue below
        // the oldest commit it examined on that branch next time. The
        // watermark stays where it is.
        (Some(stop), Some(_), Some(newest)) => {
            let (below, seen_at_below) = resume_below(&stop.examined, None, &[]);
            after.commits_since = since;
            after.commits_at_watermark.clone_from(&at_mark);
            after.commits_catching_up = Some(CommitsCatchUp {
                below,
                seen_at_below,
                newest,
                seen_at_newest: on_newest,
                branches: legs[stop.leg..].iter().map(|l| l.branch.clone()).collect(),
                page: Some(stop.page),
            });
        }
        _ => {
            after.commits_since = newest;
            after.commits_at_watermark = on_newest;
        }
    }
    Ok(())
}

/// One branch of a commit walk, and the span of its history to read.
struct Leg<'a> {
    branch: String,
    window: Window<'a, Vec<String>>,
    /// The page a catch-up's cut fell on, on the branch it cut (#2039).
    hint: Option<u32>,
}

/// What one commit walk read.
struct CommitWalk {
    /// Every commit looked at -- delivered, or recognised as delivered on the
    /// lower boundary.
    examined: Vec<(String, Option<DateTime<Utc>>)>,
    /// Where an incremental walk was cut short, if it was.
    stopped: Option<Stop>,
}

/// The leg a budget or a page cap cut short, what was examined on it, and
/// the page of its listing the cut fell on.
struct Stop {
    leg: usize,
    examined: Vec<(String, Option<DateTime<Utc>>)>,
    page: u32,
}

async fn walk_commits(
    source: &crate::GiteaSource,
    at: RepoRef<'_>,
    legs: &[Leg<'_>],
    budget: &mut u32,
    seen: &mut HashSet<String>,
    sink: &mut (dyn Sink + Send),
    emitted: &mut u64,
) -> Result<CommitWalk, RepoError> {
    let mut out = CommitWalk {
        examined: Vec::new(),
        stopped: None,
    };
    'legs: for (index, leg) in legs.iter().enumerate() {
        let window = &leg.window;
        let branch = leg.branch.as_str();
        let mut on_leg: Vec<(String, Option<DateTime<Utc>>)> = Vec::new();
        // A reached cap ends a full sync's branch quietly (see the note on
        // the caps), and cuts an incremental walk short like the budget,
        // unless the walk is still passing commits above a catch-up's window
        // (below). `skip` is the pages before the walk's first.
        let walk_from = |skip: u32| {
            PageWalk::new(PageNumber, MAX_COMMIT_PAGES, move |page| {
                source.client.commits(
                    at.owner,
                    at.name,
                    branch,
                    (window.lower, window.upper),
                    page + skip,
                )
            })
        };
        // The page last read, counted from the branch head.
        let mut page = 0;
        let mut walk = walk_from(page);
        // The commits above the window met on this leg, and whether the last
        // page read held any this leg had not met before (#2034).
        let mut above: HashSet<String> = HashSet::new();
        let mut passing_above = false;
        // The walk has gone to a catch-up's page and not yet checked it (#2039).
        let mut unchecked_hint = false;
        loop {
            let batch = match walk.next().await {
                Ok(Walked::Page(batch)) => batch,
                Ok(Walked::End) if !unchecked_hint => break,
                // A catch-up's page past the end of the listing: not trusted,
                // as any page that does not start above the window (below).
                Ok(Walked::End) => Vec::new(),
                Ok(Walked::Capped) if window.lower.is_none() => break,
                // A server that ignores `until=` (Forgejo 17, #2034) lists a
                // catch-up's branch from its head, and what the catch-up
                // delivered before its window can fill every page one walk
                // may read. Stopping there would leave the same catch-up to
                // every run after, so the walk reads on from the next page.
                // Bounded by the branch's own history: a page that only
                // repeats what this leg already read is not `passing_above`.
                Ok(Walked::Capped) if passing_above => {
                    walk = walk_from(page);
                    passing_above = false;
                    continue;
                }
                Ok(Walked::Capped) => {
                    out.stopped = Some(Stop {
                        leg: index,
                        examined: on_leg,
                        page,
                    });
                    return Ok(out);
                }
                // Listed a moment ago and gone now (#724): a merge with
                // delete-branch on a busy repository. Gitea answers `404
                // object does not exist` for the ref, and the branch's own
                // read -- what decides it, as for a pull request -- answers
                // 404 too.
                // Its row came from the listing; the next run that does not
                // list it tombstones it. Not a hole, so not a skip of the
                // repository, and not fatal on a full sync.
                Err(error)
                    if error.status() == Some(404) && branch_is_gone(source, at, branch).await =>
                {
                    tracing::info!(
                        repository = %at.full_name,
                        %branch,
                        %error,
                        "gitea: branch deleted since the listing; skipping its commits"
                    );
                    continue 'legs;
                }
                Err(error) => return Err(RepoError::from(error)),
            };
            page += 1;
            if unchecked_hint {
                unchecked_hint = false;
                // Unread commits can sit above the page only where the
                // branch lost history since (a force-push): its first commit
                // is then at or below the window's top, and the walk reads on
                // from page 2.
                if !starts_above(&batch, window.upper) {
                    page = 1;
                    walk = walk_from(page);
                    continue;
                }
            }
            // Records on this page that were not already-delivered history:
            // new ones, and the ones above a catch-up's window.
            let mut fresh = 0usize;
            let mut new_above = 0usize;
            // The page's readable commits, those strictly above the window,
            // and whether the last one lies below the watermark.
            let mut readable = 0usize;
            let mut over_top = 0usize;
            let mut ends_below = false;
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
                readable += 1;
                ends_below =
                    matches!((window.lower, happened), (Some(mark), Some(when)) if when < mark);
                if let (Some(top), Some(when)) = (window.upper, happened)
                    && (when > top || (when == top && window.upper_seen.contains(&commit.sha)))
                {
                    // `until=` is server-side too, and the same reasoning
                    // as `since=` below applies: not trusted to have held.
                    // Forgejo 17 ignores it altogether (#2034).
                    if above.insert(commit.sha.clone()) {
                        new_above += 1;
                    }
                    over_top += usize::from(when > top);
                    fresh += 1;
                    continue;
                }
                if let (Some(mark), Some(when)) = (window.lower, happened) {
                    // Not a `break`: `since=` is a server-side filter this walk
                    // does not control, and a server that ignored it would put
                    // the whole history in front of the new commits.
                    if when < mark {
                        continue;
                    }
                    if when == mark && window.lower_seen.contains(&commit.sha) {
                        out.examined.push((commit.sha.clone(), happened));
                        on_leg.push((commit.sha.clone(), happened));
                        continue;
                    }
                }
                if !seen.insert(commit.sha.clone()) {
                    continue;
                }
                out.examined.push((commit.sha.clone(), happened));
                on_leg.push((commit.sha.clone(), happened));
                push(
                    sink,
                    map::commit_item(&source.id, &raw, at, &commit),
                    emitted,
                )
                .await?;
                fresh += 1;
                *budget -= 1;
                if *budget == 0 {
                    if window.lower.is_some() {
                        out.stopped = Some(Stop {
                            leg: index,
                            examined: on_leg,
                            page,
                        });
                    }
                    return Ok(out);
                }
            }
            // A page with nothing new on it is the end of this branch's new
            // history -- and if the server ignored `since=` altogether, it is
            // the point where paging stops being worth anything. So is a page
            // that ends below the watermark (#2039): the listing is newest
            // first, so nothing after it is new. Gitea's `since=` never lists
            // such a page; Forgejo's listing reaches one on every walk, and
            // stopping on it saves the page of old history after.
            if fresh == 0 || ends_below {
                break;
            }
            passing_above = new_above > 0;
            // A catch-up's first page that holds only commits above its
            // window is a server that ignored `until=` (Forgejo 17) listing
            // from the branch head, and the pages down to where the cut fell
            // are what the cut walk already delivered (#2039). The walk goes
            // to that page, and checks it before trusting it (above). Gitea
            // bounds the listing, so its first page never qualifies.
            if page == 1
                && let Some(hint) = leg.hint
                && hint > 1
                && over_top > 0
                && over_top == readable
            {
                page = hint - 1;
                walk = walk_from(page);
                unchecked_hint = true;
            }
        }
    }
    Ok(out)
}

/// Whether a page's first commit lies strictly above `upper`: what lets a
/// catch-up start on the page its cut fell on (#2039), as
/// [`walk_pulls`] checks its own.
fn starts_above(batch: &[Value], upper: Option<DateTime<Utc>>) -> bool {
    let first = batch
        .first()
        .and_then(|raw| serde_json::from_value::<model::Commit>(raw.clone()).ok())
        .and_then(|commit| map::real_time(commit.happened_at()));
    matches!((first, upper), (Some(first), Some(upper)) if first > upper)
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
    /// the empty-page rule there is no page size to multiply by, and inventing one
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

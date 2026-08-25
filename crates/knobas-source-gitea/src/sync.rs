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
//! The descriptor claims `full_sync_exhaustive: true` (interfaces §4.2), and
//! the engine reads that as permission to tombstone every row of this source a
//! cursor-less run did not re-emit. Two things follow, and both are the same
//! rule:
//!
//! * **A page cap that is reached ends the run with an error.** Returning `Ok`
//!   after walking 1,000 of 1,400 repositories would authorise the sweep to
//!   tombstone the other 400.
//! * **A repository skipped during a cursor-less run is fatal too.** Ruling B4
//!   grants skip-with-warning for a 403 or 404 on one repository, and that is
//!   what an *incremental* run does -- no sweep follows it, and the
//!   repository's watermarks are kept so the next run picks up where this one
//!   left off. During a full sync the same skip would hand the engine an
//!   incomplete corpus and a green light, so it raises instead. A repository
//!   genuinely deleted upstream never takes this path: it simply stops
//!   appearing in the listing, and the sweep tombstoning it is then correct.

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
fn cap_reached(what: &str, cap: u32) -> SourceError {
    SourceError::Protocol(format!(
        "gitea: more than {} {what} to walk in one run; this source declares its full sync \
         exhaustive, so stopping at the cap would let the engine tombstone everything past it. \
         Narrow the source with owners[] or repos[].",
        cap * PAGE_SIZE
    ))
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

    let full_sync = cursor.is_none();
    let previous = GiteaCursor::parse(cursor.as_deref());
    let mut next = GiteaCursor::fresh();
    let mut emitted = 0u64;
    let mut walked = 0usize;

    let (selection, mut first_skip) = select_repos(source, full_sync).await?;

    for selected in &selection {
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

/// Which repositories this run walks, and the first refusal met while deciding.
async fn select_repos(
    source: &crate::GiteaSource,
    full_sync: bool,
) -> Result<(Vec<Selected>, Option<SourceError>), SourceError> {
    let mut out = Vec::new();
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
                    tracing::warn!(
                        repository = %entry,
                        %error,
                        "gitea: configured repository is unavailable"
                    );
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
    Ok((out, skip))
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

    /// The cap message has to name the limit that was hit and the lever that
    /// moves it, because it is what the user sees when a source is too big.
    #[test]
    fn a_reached_cap_names_the_limit_and_the_lever() {
        let SourceError::Protocol(message) = cap_reached("repositories", MAX_LIST_PAGES) else {
            panic!("a reached cap is a protocol failure");
        };
        assert!(message.contains("1000"), "{message}");
        assert!(message.contains("owners[]"), "{message}");
        // Not swept away silently: the message says why stopping is not an
        // option.
        assert!(message.contains("tombstone"), "{message}");
    }
}

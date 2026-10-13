//! The four writes M2 ratified for Gitea -- branch, pull request, comment,
//! approval -- the merge the source views added (issue #667), the review a
//! pull request's toolbar writes (issue #705) and the branch delete (issue
//! #707), and a pull request's edit, close and reopen (issue #706).
//!
//! Deliberately free of [`WriteOp`], for the same reason the Jira adapter's is:
//! knobas has one outbound write path, and
//! `knobas-sync/tests/it/write_choke_point.rs` refuses any production file that
//! *names* the op enum without implementing the trait. [`crate::source`] --
//! where `impl Source for GiteaSource` lives -- unpacks; this performs.
//!
//! [`WriteOp`]: knobas_source::WriteOp
//!
//! # The endpoints
//!
//! | write | path | body |
//! | --- | --- | --- |
//! | branch | `POST /repos/{o}/{r}/branches` | `{"new_branch_name":…,"old_ref_name":…}` |
//! | pull request | `POST /repos/{o}/{r}/pulls` | `{"title":…,"body":…,"head":…,"base":…}` |
//! | comment | `POST /repos/{o}/{r}/issues/{index}/comments` | `{"body":…}` |
//! | approval | `POST /repos/{o}/{r}/pulls/{index}/reviews` | `{"event":"APPROVED","body":…}` |
//! | merge | `POST /repos/{o}/{r}/pulls/{index}/merge` | `{"Do":…,"delete_branch_after_merge":…}` |
//! | branch update | `POST /repos/{o}/{r}/pulls/{index}/update?style=…` | none |
//! | review | `POST /repos/{o}/{r}/pulls/{index}/reviews` | `{"event":…,"body":…,"comments":[…]}` |
//! | branch delete | `DELETE /repos/{o}/{r}/branches/{name}` | none |
//! | edit | `PATCH /repos/{o}/{r}/pulls/{index}` | only the given of `{"title":…,"body":…,"state":…}` |
//!
//! The comment goes on the **issue** with the pull request's number, which is
//! not a quirk of this adapter: Gitea keeps a pull request's discussion on the
//! issue of the same index, which is why `crate::client::issue_comments` reads
//! it from there too.

use std::time::Duration;

use serde_json::json;

use knobas_source::{
    MergeStyle, PullRequestState, ReviewComment, ReviewSide, ReviewVerdict, SourceError,
    UpdateStyle,
};

use crate::client::GiteaClient;

/// Create a branch in a repository.
///
/// Gitea answers **409** when the branch already exists and **404** when
/// `from_ref` names nothing -- both refusals, which the queue records with
/// Gitea's own sentence instead of retrying forever.
///
/// # Errors
///
/// [`SourceError::Protocol`] for an owner or repository name that is not one
/// Gitea could have issued; otherwise whatever the request maps to.
pub(crate) async fn create_branch(
    client: &GiteaClient,
    owner: &str,
    repo: &str,
    name: &str,
    from_ref: &str,
) -> Result<(), SourceError> {
    let path = format!(
        "/repos/{}/{}/branches",
        GiteaClient::path_segment(owner)?,
        GiteaClient::path_segment(repo)?
    );
    client
        .post(
            &path,
            &json!({ "new_branch_name": name, "old_ref_name": from_ref }),
        )
        .await
        .map(|_| ())
}

/// Open a pull request from `head` into `base`.
///
/// # Errors
///
/// As [`create_branch`]. Gitea's own refusals -- an open pull request already
/// exists for this pair, `head` and `base` are the same branch -- arrive as
/// `Protocol` carrying what it said.
pub(crate) async fn create_pull_request(
    client: &GiteaClient,
    owner: &str,
    repo: &str,
    title: &str,
    body: &str,
    head: &str,
    base: &str,
) -> Result<(), SourceError> {
    let path = format!(
        "/repos/{}/{}/pulls",
        GiteaClient::path_segment(owner)?,
        GiteaClient::path_segment(repo)?
    );
    client
        .post(
            &path,
            &json!({ "title": title, "body": body, "head": head, "base": base }),
        )
        .await
        .map(|_| ())
}

/// Reply on a pull request's discussion.
///
/// # Errors
///
/// As [`create_branch`].
pub(crate) async fn comment(
    client: &GiteaClient,
    owner: &str,
    repo: &str,
    index: u64,
    body: &str,
) -> Result<(), SourceError> {
    let path = format!(
        "/repos/{}/{}/issues/{index}/comments",
        GiteaClient::path_segment(owner)?,
        GiteaClient::path_segment(repo)?
    );
    client
        .post(&path, &json!({ "body": body }))
        .await
        .map(|_| ())
}

/// Approve a pull request.
///
/// `event: "APPROVED"` is the field that makes this an approval rather than a
/// plain review: Gitea's `POST .../reviews` with no `event` files a `PENDING`
/// review, which nobody sees and which does not unblock anyone. The body may
/// be empty -- an approval with no words is an approval -- and it is sent
/// either way, because the field is what Gitea's `CreatePullReviewOptions`
/// declares and an omitted one is not a shorter approval.
///
/// # Errors
///
/// As [`create_branch`].
pub(crate) async fn approve(
    client: &GiteaClient,
    owner: &str,
    repo: &str,
    index: u64,
    body: &str,
) -> Result<(), SourceError> {
    let path = format!(
        "/repos/{}/{}/pulls/{index}/reviews",
        GiteaClient::path_segment(owner)?,
        GiteaClient::path_segment(repo)?
    );
    client
        .post(&path, &json!({ "event": "APPROVED", "body": body }))
        .await
        .map(|_| ())
}

/// Merge a pull request, the way `style` names (`merge`, `rebase`, `squash`).
///
/// `Do` is the field Gitea's `MergePullRequestOption` requires, and it takes
/// the SPI's three words as they are. `delete_branch_after_merge` is always
/// sent, both ways (#708): Gitea 1.27 reads an absent field as the
/// repository's own default, so leaving it out when the reader unticked the
/// box could delete the branch anyway. Nothing else is sent: the merge
/// commit's title and message are Gitea's defaults, which is what its own
/// *Merge* button does without the reader typing anything.
///
/// Gitea answers **405** for every pull request it will not merge -- a
/// work-in-progress title, one already merged, an account that may not --
/// and that arrives as a `Protocol` refusal carrying its sentence (ADR-0004),
/// which the queue records rather than retries. The one exception is
/// [`still_checking`]: *"Please try again later"* is tried again here, beside
/// `knobas-http`'s own retry of a busy server, for up to [`MERGE_PATIENCE`]
/// of waiting (#721).
///
/// # Errors
///
/// As [`create_branch`].
pub(crate) async fn merge(
    client: &GiteaClient,
    owner: &str,
    repo: &str,
    index: u64,
    style: MergeStyle,
    delete_branch: bool,
) -> Result<(), SourceError> {
    let path = format!(
        "/repos/{}/{}/pulls/{index}/merge",
        GiteaClient::path_segment(owner)?,
        GiteaClient::path_segment(repo)?
    );
    let body = json!({ "Do": style.as_str(), "delete_branch_after_merge": delete_branch });
    patiently(MERGE_PATIENCE, || async {
        client.post(&path, &body).await.map(|_| ())
    })
    .await
}

/// How long a merge keeps asking while Gitea answers [`still_checking`].
/// Measured on 1.27.3 (#721): a pull request opened a moment ago reads
/// `mergeable: true`, then Gitea's check of the push behind it runs and the
/// merge is refused for 1--2.5 s. Several times that, so a busy server is
/// waited for too; past it, the answer is believed.
const MERGE_PATIENCE: Duration = Duration::from_secs(15);

/// The longest single wait between two merge attempts.
const MERGE_WAIT_CAP: Duration = Duration::from_secs(2);

/// Gitea's *"Please try again later"*: its 405 to a merge of a pull request
/// whose recorded state is not *mergeable* (`ErrNotMergeableState` in
/// `services/pull/check.go`). That state is *checking* for a moment after the
/// pull request or its head moved -- the case worth waiting out -- and is
/// also *conflict* or *error* for good, which the same sentence cannot tell
/// apart; [`MERGE_PATIENCE`] is what keeps those a refusal in the end
/// rather than a write that waits forever (story 19). Matched on the status
/// **and** the sentence: every other 405 is a refusal the first time.
fn still_checking(error: &SourceError) -> bool {
    matches!(
        error,
        SourceError::Protocol { status: Some(405), message } if message.contains("Please try again later")
    )
}

/// Run `attempt` until it does anything but [`still_checking`], waiting
/// `knobas-http`'s own exponential backoff (capped at [`MERGE_WAIT_CAP`])
/// between tries, for at most `patience` of waiting; then the last answer is
/// the answer.
async fn patiently<F, Fut>(patience: Duration, mut attempt: F) -> Result<(), SourceError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<(), SourceError>>,
{
    let mut waited = Duration::ZERO;
    let mut tries: u32 = 0;
    loop {
        let result = attempt().await;
        match &result {
            Err(error) if still_checking(error) && waited < patience => {
                tries += 1;
                let wait = knobas_http::retry::backoff(tries)
                    .min(MERGE_WAIT_CAP)
                    .min(patience - waited);
                tracing::debug!(tries, ?wait, "gitea is still checking the merge");
                tokio::time::sleep(wait).await;
                waited += wait;
            }
            _ => return result,
        }
    }
}

/// Bring a pull request's head up to date with its base (#708): Gitea's
/// `update` with `style` as its query, `merge` or `rebase`.
///
/// # Errors
///
/// As [`create_branch`]; a conflict is Gitea's **409**.
pub(crate) async fn update_branch(
    client: &GiteaClient,
    owner: &str,
    repo: &str,
    index: u64,
    style: UpdateStyle,
) -> Result<(), SourceError> {
    client.update_pull(owner, repo, index, style).await
}

// -- submit_review (issue #705) ----------------------------------------------

/// Review a pull request: approve it, request changes, or only comment, with
/// line comments (spec #701 workflow 6).
///
/// `event` is Gitea's spelling of the verdict -- `APPROVED`,
/// `REQUEST_CHANGES`, `COMMENT` -- and is always sent: a `POST .../reviews`
/// without one files a `PENDING` draft that nobody sees (see [`approve`]).
/// A line comment is Gitea's `CreatePullReviewComment`: its line number goes
/// in `new_position` or `old_position` by the side it is on, the other `0`.
///
/// Gitea refuses an approval or a request for changes from the pull
/// request's own author with a **422** (*approve your own pull is not
/// allowed*), and a request for changes with no words likewise; both arrive
/// as `Protocol` refusals carrying its sentence, never retried (ADR-0004).
///
/// # Errors
///
/// As [`create_branch`].
pub(crate) async fn submit_review(
    client: &GiteaClient,
    owner: &str,
    repo: &str,
    index: u64,
    verdict: ReviewVerdict,
    body: &str,
    comments: &[ReviewComment],
) -> Result<(), SourceError> {
    let path = format!(
        "/repos/{}/{}/pulls/{index}/reviews",
        GiteaClient::path_segment(owner)?,
        GiteaClient::path_segment(repo)?
    );
    let event = match verdict {
        ReviewVerdict::Approve => "APPROVED",
        ReviewVerdict::RequestChanges => "REQUEST_CHANGES",
        ReviewVerdict::Comment => "COMMENT",
    };
    let comments: Vec<serde_json::Value> = comments
        .iter()
        .map(|c| {
            let (new, old) = match c.side {
                ReviewSide::New => (c.line, 0),
                ReviewSide::Old => (0, c.line),
            };
            json!({ "path": c.path, "body": c.body, "new_position": new, "old_position": old })
        })
        .collect();
    client
        .post(
            &path,
            &json!({ "event": event, "body": body, "comments": comments }),
        )
        .await
        .map(|_| ())
}

/// What a delete reads of the repository before it acts: its default branch
/// and whether it is a mirror.
#[derive(serde::Deserialize)]
struct RepoHead {
    #[serde(default)]
    default_branch: Option<String>,
    #[serde(default)]
    mirror: bool,
}

/// What a delete reads of the branch before it acts: whether it is protected.
#[derive(serde::Deserialize)]
struct BranchGuard {
    #[serde(default)]
    protected: bool,
}

/// Delete branch `name` (issue #707).
///
/// The repository's default branch, a protected branch and a branch of a
/// mirror are **refused here, before the delete**, with a sentence naming
/// why and what to do. Gitea refuses all three itself with a 403, which since
/// #722 arrives as a refusal carrying Gitea's own sentence
/// (`client::forbidden_refusal`) -- but *branch protected* says less than
/// these do, so the reads stay. The view offers none of them, so they only
/// answer a payload that names one anyway, or a branch protected between the
/// view and the flush; a protection set in the milliseconds between these
/// reads and the delete comes back as Gitea's 403 refusal.
///
/// Gitea answers a delete with 204, and 404 for a branch that is already gone
/// -- a refusal carrying its sentence (ADR-0004).
///
/// # Errors
///
/// [`SourceError::Protocol`] for the default branch, a protected branch, a
/// branch of a mirror, a repository whose record names no default branch, or
/// a name that is not one Gitea could have issued; otherwise whatever the
/// request maps to.
pub(crate) async fn delete_branch(
    client: &GiteaClient,
    owner: &str,
    repo: &str,
    name: &str,
) -> Result<(), SourceError> {
    let (head, guard) = tokio::try_join!(
        client.repo_as::<RepoHead>(owner, repo),
        client.branch_as::<BranchGuard>(owner, repo, name),
    )?;
    // No guess where Gitea names none: the views' `main` fallback is a fine
    // label and a wrong guard, since on a `master` repository it would let the
    // real default go.
    let Some(default) = head.default_branch.as_deref().filter(|b| !b.is_empty()) else {
        return Err(SourceError::protocol(format!(
            "gitea: {owner}/{repo} names no default branch, so {name:?} is not deleted"
        )));
    };
    if head.mirror {
        return Err(SourceError::protocol(format!(
            "gitea: {owner}/{repo} is a mirror, whose branches come from upstream, so {name:?} is \
             not deleted"
        )));
    }
    if name == default {
        return Err(SourceError::protocol(format!(
            "gitea: {name:?} is the default branch of {owner}/{repo}, which is not deleted"
        )));
    }
    if guard.protected {
        return Err(SourceError::protocol(format!(
            "gitea: {name:?} is a protected branch of {owner}/{repo}; lift its protection on Gitea \
             to delete it"
        )));
    }
    client.delete_branch(owner, repo, name).await
}

// -- edit_pull_request (issue #706) ------------------------------------------

/// Edit a pull request's title or description, close it or reopen it (spec
/// #701 workflow 8): one `PATCH` carrying **only the fields given**, so a
/// changed title never sends back a description somebody else edited since
/// the view was read (the deputy's ruling on #706). An edit with none is
/// refused before a request: a `PATCH {}` would change nothing.
///
/// Gitea answers **412** *cannot change state of this pull request, it was
/// already merged* for a close or reopen of a merged pull request (measured on
/// 1.27.3; a title or description on one is still taken), which arrives as a
/// `Protocol` refusal carrying its sentence, never retried (ADR-0004).
///
/// # Errors
///
/// As [`create_branch`], and [`SourceError::Protocol`] for an empty edit.
pub(crate) async fn edit_pull_request(
    client: &GiteaClient,
    owner: &str,
    repo: &str,
    index: u64,
    title: Option<&str>,
    body: Option<&str>,
    state: Option<PullRequestState>,
) -> Result<(), SourceError> {
    let path = format!(
        "/repos/{}/{}/pulls/{index}",
        GiteaClient::path_segment(owner)?,
        GiteaClient::path_segment(repo)?
    );
    let mut fields = serde_json::Map::new();
    if let Some(title) = title {
        fields.insert("title".to_owned(), json!(title));
    }
    if let Some(body) = body {
        fields.insert("body".to_owned(), json!(body));
    }
    if let Some(state) = state {
        fields.insert("state".to_owned(), json!(state.as_str()));
    }
    if fields.is_empty() {
        return Err(SourceError::protocol(format!(
            "gitea: an edit of {owner}/{repo}#{index} with nothing to change"
        )));
    }
    client
        .patch(&path, &serde_json::Value::Object(fields))
        .await
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::time::{Duration, Instant};

    use knobas_source::SourceError;

    use super::{patiently, still_checking};

    fn refused(message: &str) -> SourceError {
        SourceError::Protocol {
            status: Some(405),
            message: format!("HTTP 405 Method Not Allowed: {message}"),
        }
    }

    /// The same sentence is also Gitea's answer for a pull request that
    /// conflicts, for good (#721): patience runs out, and then Gitea's 405 is
    /// the answer -- a refusal the queue records, not a write that waits
    /// forever.
    #[tokio::test]
    async fn a_merge_still_refused_when_patience_runs_out_is_a_refusal() {
        let tries = Cell::new(0);
        let started = Instant::now();
        let result = patiently(Duration::from_millis(600), || {
            tries.set(tries.get() + 1);
            async { Err(refused("Please try again later")) }
        })
        .await;
        let error = result.expect_err("Gitea never stopped refusing");
        assert!(still_checking(&error), "{error:?}");
        assert_eq!(error.status(), Some(405));
        // 250 ms, 350 ms (what was left): three asks in 600 ms of waiting.
        assert_eq!(tries.get(), 3);
        assert!(started.elapsed() >= Duration::from_millis(600));
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    /// Only Gitea's *"Please try again later"* 405 is waited out: the same
    /// sentence on another status, and another 405, are not.
    #[test]
    fn only_the_405_still_checking_is_waited_out() {
        assert!(still_checking(&refused("Please try again later")));
        assert!(!still_checking(&refused(
            "Work in progress PRs cannot be merged"
        )));
        assert!(!still_checking(&SourceError::Protocol {
            status: Some(409),
            message: "HTTP 409 Conflict: Please try again later".to_owned(),
        }));
        assert!(!still_checking(&SourceError::Unreachable(
            "Please try again later".to_owned()
        )));
    }
}

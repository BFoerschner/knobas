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
/// conflict, a work-in-progress title, one already merged, and *"Please try
/// again later"* while it is still computing mergeability -- and that arrives
/// as a `Protocol` refusal carrying its sentence, never retried (ADR-0004).
/// The view offers the action only where Gitea said mergeable, which is what
/// keeps the last of those rare.
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
    client.post(&path, &body).await.map(|_| ())
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
/// why. Gitea refuses all three itself, but with a 403, which the shared fault
/// mapping reads as a dead credential (`knobas_http`'s 401-and-403 rule): the
/// write would wait for a token that is fine instead of saying what is wrong.
/// The view offers none of them, so these reads only answer a payload that
/// names one anyway, or a branch protected between the view and the flush.
///
/// A protection set in the milliseconds between these reads and the delete
/// still comes back as that 403. The write then waits as unauthorized for one
/// scheduler tick, and the next flush makes these reads again and refuses it
/// with the sentence: a short wrong wait, never a lost or a wrong delete.
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

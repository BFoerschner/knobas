//! The four writes M2 ratified for Gitea -- branch, pull request, comment,
//! approval -- the merge the source views added (issue #667), the review a
//! pull request's toolbar writes (issue #705) and the branch delete (issue
//! #707), a pull request's edit, close and reopen (issue #706), and a file
//! commit through the contents API (issue #822).
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
//! | file commit | `PUT /repos/{o}/{r}/contents/{path}` | `{"content":<base64>,"message":…,"branch":…,"sha":…}`, plus `"new_branch"` when set |
//! | add label (source action) | `POST /repos/{o}/{r}/issues/{index}/labels` | `{"labels":[<id>]}` |
//! | transition (an issue, #1206) | `PATCH /repos/{o}/{r}/issues/{index}` | `{"state":"open"\|"closed"}` |
//! | create ticket (an issue, #1206) | `POST /repos/{o}/{r}/issues` | `{"title":…,"body":…}` |
//!
//! The comment goes on the **issue** with the pull request's number, which is
//! not a quirk of this adapter: Gitea keeps a pull request's discussion on the
//! issue of the same index, which is why `crate::client::issue_comments` reads
//! it from there too.

use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
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
/// A 409 where `name` now points at the commit `from_ref` names is this
/// write's own branch, met again by a repeat after the first attempt landed
/// and its answer was lost (#1129): the transport's retry, or the queue's
/// later resend. That is answered as delivered, as [`commit_file`] answers
/// its own commit. A branch at any other commit keeps Gitea's refusal.
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
    match client
        .post(
            &path,
            &json!({ "new_branch_name": name, "old_ref_name": from_ref }),
        )
        .await
    {
        Ok(_) => Ok(()),
        Err(error)
            if error.status() == Some(409)
                && holds_branch_from(client, owner, repo, name, from_ref).await =>
        {
            Ok(())
        }
        Err(error) => Err(error),
    }
}

/// A branch's tip, as `GET .../branches/{name}` answers it.
#[derive(serde::Deserialize)]
struct BranchTip {
    commit: TipCommit,
}

#[derive(serde::Deserialize)]
struct TipCommit {
    id: String,
}

/// Whether branch `name` points at the commit `from_ref` names: `from_ref`
/// itself where it is a commit id (or an abbreviation of the tip's), else the
/// tip of the branch it names. Any read that fails is "no", which keeps the
/// refusal: a tag, or a base that moved since, reads as somebody else's.
async fn holds_branch_from(
    client: &GiteaClient,
    owner: &str,
    repo: &str,
    name: &str,
    from_ref: &str,
) -> bool {
    let Ok(made) = client.branch_as::<BranchTip>(owner, repo, name).await else {
        return false;
    };
    let tip = made.commit.id.to_ascii_lowercase();
    let wanted = from_ref.to_ascii_lowercase();
    if wanted.len() >= 7
        && wanted.chars().all(|c| c.is_ascii_hexdigit())
        && tip.starts_with(&wanted)
    {
        return true;
    }
    client
        .branch_as::<BranchTip>(owner, repo, from_ref)
        .await
        .is_ok_and(|base| base.commit.id.eq_ignore_ascii_case(&tip))
}

/// Open a pull request from `head` into `base`.
///
/// # Errors
///
/// As [`create_branch`]. Gitea's own refusals -- an open pull request already
/// exists for this pair, `head` and `base` are the same branch -- arrive as
/// `Protocol` carrying what it said.
///
/// Gitea's 409 for the pair, where the open pull request for `head` into
/// `base` carries this write's `title` and was opened by this token's
/// account, is this write's own, met again after its answer was lost
/// (#1129): answered as delivered. One with another title, or opened by
/// somebody else, keeps the refusal.
///
/// Answers the number of the pull request this write opened (#1949): the one
/// Gitea's 201 carries, or, on that 409, the one found open for the pair.
/// `None` where Gitea's answer carried none; the pull request is open all the
/// same.
pub(crate) async fn create_pull_request(
    client: &GiteaClient,
    owner: &str,
    repo: &str,
    title: &str,
    body: &str,
    head: &str,
    base: &str,
) -> Result<Option<u64>, SourceError> {
    let path = format!(
        "/repos/{}/{}/pulls",
        GiteaClient::path_segment(owner)?,
        GiteaClient::path_segment(repo)?
    );
    match client
        .post(
            &path,
            &json!({ "title": title, "body": body, "head": head, "base": base }),
        )
        .await
    {
        Ok(made) => {
            let answer: serde_json::Value = made.json().await.unwrap_or_default();
            Ok(answer["number"].as_u64())
        }
        Err(error) if error.status() == Some(409) => {
            match this_writes_open_pull(client, owner, repo, title, head, base).await {
                Some(pull) => Ok(pull.number),
                None => Err(error),
            }
        }
        Err(error) => Err(error),
    }
}

/// One open pull request, as far as [`this_writes_open_pull`] reads it.
#[derive(serde::Deserialize)]
struct OpenPull {
    #[serde(default)]
    number: Option<u64>,
    #[serde(default)]
    title: String,
    head: PullEnd,
    base: PullEnd,
    #[serde(default)]
    user: Option<crate::model::UserRef>,
}

#[derive(serde::Deserialize)]
struct PullEnd {
    #[serde(default, rename = "ref")]
    branch: String,
    #[serde(default)]
    label: String,
}

impl PullEnd {
    /// Whether this end is branch `name`, by its ref or its `owner:branch`
    /// label.
    fn names(&self, name: &str) -> bool {
        self.branch == name || self.label == name
    }
}

/// How many pages of open pull requests [`this_writes_open_pull`] reads: Gitea refuses a
/// second one per pair, so the one it refused over is open, and recently
/// updated, which is the listing's order. The readback ends on an empty page
/// or here, whichever comes first.
const OPEN_PULL_PAGES: u32 = 3;

/// The open pull request from `head` into `base`, where it carries `title`
/// and was opened by the account this token is: this write's own. A
/// colleague's pull request for the same pair, even under the same title, is
/// `None` and keeps the refusal: their body, not this write's, is on it. Any
/// read that fails is `None`, which keeps the refusal.
async fn this_writes_open_pull(
    client: &GiteaClient,
    owner: &str,
    repo: &str,
    title: &str,
    head: &str,
    base: &str,
) -> Option<OpenPull> {
    for page in 1..=OPEN_PULL_PAGES {
        let pulls = client
            .open_pulls_as::<OpenPull>(owner, repo, page)
            .await
            .ok()?;
        // The end is an empty page, never a short one: a Gitea capped below
        // the 50 asked for answers short with more behind it (issue #81), and
        // reading that as the end turned a landed create into a refusal
        // (#1625).
        if pulls.is_empty() {
            return None;
        }
        if let Some(pull) = pulls
            .into_iter()
            .find(|p| p.head.names(head) && p.base.names(base))
        {
            let by = pull.user.as_ref().and_then(|u| u.login.as_deref())?;
            let ours = pull.title == title && is_this_account(client, by).await;
            return ours.then_some(pull);
        }
    }
    None
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
        .post_once(&path, &json!({ "body": body }))
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
        .post_once(&path, &json!({ "event": "APPROVED", "body": body }))
        .await
        .map(|_| ())
}

/// Merge a pull request, the way `style` names (`merge`, `rebase`, `squash`).
///
/// `Do` is the field Gitea's `MergePullRequestOption` requires, and it takes
/// the SPI's three words as they are. `delete_branch_after_merge` is always
/// sent, both ways (#708): Gitea 1.27 reads an absent field as the
/// repository's own default, so leaving it out when the reader unticked the
/// box could delete the branch anyway. The reader's message (#843), where
/// there is one, is `MergeTitleField` (its first line) and
/// `MergeMessageField` (the rest, trimmed; empty for a one-line message);
/// without one the merge commit's title and message are Gitea's defaults,
/// which is what its own *Merge* button does without the reader typing
/// anything.
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
    how: &Merge<'_>,
) -> Result<(), SourceError> {
    let path = format!(
        "/repos/{}/{}/pulls/{index}/merge",
        GiteaClient::path_segment(owner)?,
        GiteaClient::path_segment(repo)?
    );
    let mut body =
        json!({ "Do": how.style.as_str(), "delete_branch_after_merge": how.delete_branch });
    if let Some(message) = how.message {
        let (title, rest) = message.split_once('\n').unwrap_or((message, ""));
        body["MergeTitleField"] = json!(title.trim());
        body["MergeMessageField"] = json!(rest.trim());
    }
    match patiently(MERGE_PATIENCE, || async {
        client.post(&path, &body).await.map(|_| ())
    })
    .await
    {
        Err(error)
            if error.status() == Some(405)
                && !still_checking(&error)
                && merged_by_this_account(client, owner, repo, index).await =>
        {
            Ok(())
        }
        result => result,
    }
}

/// A pull request's merge, as far as [`merged_by_this_account`] reads it.
#[derive(serde::Deserialize)]
struct MergedPull {
    #[serde(default)]
    merged: bool,
    #[serde(default)]
    merged_by: Option<crate::model::UserRef>,
}

/// Whether pull request `index` is merged, and by the account this token is:
/// a 405 over that is this write's own merge, met again by a repeat after the
/// first attempt landed and its answer was lost (#1129). Merged by anyone
/// else, the refusal stands: the reader is told it was not theirs. Any read
/// that fails is "no".
async fn merged_by_this_account(client: &GiteaClient, owner: &str, repo: &str, index: u64) -> bool {
    let Ok(pull) = client.pull(owner, repo, index).await else {
        return false;
    };
    let Ok(pull) = serde_json::from_value::<MergedPull>(pull) else {
        return false;
    };
    let Some(by) = pull.merged_by.and_then(|u| u.login).filter(|_| pull.merged) else {
        return false;
    };
    is_this_account(client, &by).await
}

/// Whether `login` is the account this token is (`GET /user`). A failed read
/// is "no".
async fn is_this_account(client: &GiteaClient, login: &str) -> bool {
    client
        .current_user()
        .await
        .is_ok_and(|me| me.login.is_some_and(|me| me.eq_ignore_ascii_case(login)))
}

/// What [`merge`] is asked to do: the style, whether Gitea deletes the head
/// after, and the reader's commit message (`None` for Gitea's default).
pub(crate) struct Merge<'a> {
    pub(crate) style: MergeStyle,
    pub(crate) delete_branch: bool,
    pub(crate) message: Option<&'a str>,
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
/// No `commit_id` is sent, so Gitea files the review, and every line in it,
/// at the pull request's head. Gitea would take an older one (measured on
/// 1.27.3), but a review carries one commit, and the shell offers line
/// comments on the full diff only, never on a picked commit or range
/// (#2273, `TfPrFiles.svelte`): those line numbers are the head's.
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
        .post_once(
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

// -- an issue's workflow and creation (issue #1206) ---------------------------

/// Move an issue to `status`: `open` or `closed`. Any other status is refused
/// before a request, naming the two there are. Moving an issue to the state
/// it is in is Gitea's to take and changes nothing, so a resend is harmless.
///
/// # Errors
///
/// As [`create_branch`], and [`SourceError::Protocol`] for a status Gitea has
/// no state for.
pub(crate) async fn transition(
    client: &GiteaClient,
    owner: &str,
    repo: &str,
    index: u64,
    status: &str,
) -> Result<(), SourceError> {
    let state = crate::model::issue_state(status).ok_or_else(|| {
        SourceError::protocol(format!(
            "gitea: an issue is open or closed, so it cannot move to {status:?}"
        ))
    })?;
    let path = format!("{}/{index}", crate::client::issues_path(owner, repo)?);
    client
        .patch(&path, &json!({ "state": state }))
        .await
        .map(|_| ())
}

/// File an issue in a repository, sent **once**: a resend would file it twice
/// (#1122). Gitea has no issue type, so the one the dialog chose is not sent.
/// Answers the new issue's number where Gitea's answer names it; an answer
/// that does not is still a write that landed, and is not refused.
///
/// # Errors
///
/// As [`create_branch`].
pub(crate) async fn create_ticket(
    client: &GiteaClient,
    owner: &str,
    repo: &str,
    title: &str,
    body: &str,
) -> Result<Option<u64>, SourceError> {
    let path = crate::client::issues_path(owner, repo)?;
    let made = client
        .post_once(&path, &json!({ "title": title, "body": body }))
        .await?;
    let answer: serde_json::Value = made.json().await.unwrap_or_default();
    Ok(answer["number"].as_u64())
}

// -- the add_label source action (issue #1194) --------------------------------

/// Put label `label` (its id) on a pull request: Gitea keeps a pull request's
/// labels on the issue of the same index, as its discussion. Adding a label
/// it already carries changes nothing, so the request may be sent again.
///
/// # Errors
///
/// As [`create_branch`]: a label of another repository is Gitea's 422.
pub(crate) async fn add_label(
    client: &GiteaClient,
    owner: &str,
    repo: &str,
    index: u64,
    label: u64,
) -> Result<(), SourceError> {
    let path = format!(
        "/repos/{}/{}/issues/{index}/labels",
        GiteaClient::path_segment(owner)?,
        GiteaClient::path_segment(repo)?
    );
    client
        .post(&path, &json!({ "labels": [label] }))
        .await
        .map(|_| ())
}

// -- commit_file (issue #822) ------------------------------------------------

/// What a commit reads of the branch it would land on: whether it is
/// protected, and whether this token may push to it anyway.
#[derive(serde::Deserialize)]
pub(crate) struct PushGuard {
    #[serde(default)]
    protected: bool,
    #[serde(default = "pushable")]
    user_can_push: bool,
}

impl PushGuard {
    /// Why a commit straight onto `branch` is refused, in the reader's
    /// words, or `None` where it is taken: a protected branch this account
    /// may not push to (#822). The flush refuses with this sentence, and the
    /// file view declares its commit disabled with it (#985), so the two
    /// cannot say different things.
    pub(crate) fn refusal(&self, branch: &str) -> Option<String> {
        (self.protected && !self.user_can_push).then(|| {
            format!(
                "{branch} is a protected branch that this account may not push to: commit to a \
                 new branch and open a pull request instead"
            )
        })
    }
}

/// An answer that does not say is not read as a refusal: Gitea's own 403 is
/// the backstop.
fn pushable() -> bool {
    true
}

/// One file as the contents API answers it: its blob and its text.
#[derive(serde::Deserialize)]
struct FileAt {
    sha: String,
    #[serde(default)]
    content: Option<String>,
    /// The commit that last touched the file on the ref read.
    #[serde(default)]
    last_commit_sha: Option<String>,
}

impl FileAt {
    fn text(&self) -> String {
        decoded(self.content.as_deref().unwrap_or_default())
    }
}

/// What one file commit says (#822), as the source's match unpacks it.
pub(crate) struct Commit<'a> {
    pub(crate) branch: &'a str,
    pub(crate) new_branch: Option<&'a str>,
    pub(crate) path: &'a str,
    pub(crate) content: &'a str,
    pub(crate) message: &'a str,
    pub(crate) base_sha: &'a str,
}

/// Commit one file's new text to `branch`, or to `new_branch` made from it
/// (spec #808 D4, R-8.6): Gitea's `PUT /repos/{o}/{r}/contents/{path}`, the
/// text in base64 and `base_sha` as its `sha`, and the commit it made as the
/// receipt's id. The checkout is never written to.
///
/// **The race is Gitea's to see.** It compares `sha` with the blob `branch`
/// holds and answers a stale one 422 (*sha does not match*, measured on
/// 1.27.3; Forgejo 17 answers 409 with the same sentence, #1195). On a 422 the file is read again on `branch`, and a blob other than
/// `base_sha` comes back as [`SourceError::Moved`] with that blob and its
/// text: the write queue holds it with both versions. A 422 over a file that
/// did not move (a new branch that already exists) stays Gitea's refusal, so
/// the reading does not rest on the sentence. And a 422 where the file
/// already holds this commit's text -- on `new_branch` when one is named,
/// else on `branch` -- is a re-send of a commit that landed (ADR-0012's
/// at-least-once), answered as delivered with the commit that last touched
/// the file; without that, the reader would be shown their own text as
/// *theirs*.
///
/// A commit straight onto a **protected** branch this token may not push to is
/// refused here, before the request, saying what to do instead: Gitea's own
/// 403 says only *user cannot commit to repo*. A commit to a new branch reads
/// no protection, since nothing lands on `branch`. A file that is not on
/// `branch` (404) or a branch that is not (404) is Gitea's refusal, never a
/// create.
///
/// # Errors
///
/// [`SourceError::Moved`] as above; [`SourceError::Protocol`] for a protected
/// branch, an empty branch name or a path that is not one; otherwise whatever
/// the request maps to.
pub(crate) async fn commit_file(
    client: &GiteaClient,
    owner: &str,
    repo: &str,
    commit: &Commit<'_>,
) -> Result<Option<String>, SourceError> {
    if commit.branch.is_empty() || commit.new_branch.is_some_and(str::is_empty) {
        return Err(SourceError::protocol(format!(
            "gitea: a commit to {owner}/{repo} names no branch"
        )));
    }
    // Before any request, the read of the branch included: an empty path is
    // the repository's top directory to the contents API, and the request
    // path's own escaping refuses the rest only once it is built.
    let unusable = |segment: &str| {
        segment.is_empty()
            || segment == "."
            || segment == ".."
            || segment.contains(char::is_control)
    };
    if commit.path.split('/').any(unusable) {
        return Err(SourceError::protocol(format!(
            "gitea: {:?} is not a file path in {owner}/{repo}",
            commit.path
        )));
    }
    if commit.new_branch.is_none() {
        let guard = client
            .branch_as::<PushGuard>(owner, repo, commit.branch)
            .await?;
        if let Some(refusal) = guard.refusal(commit.branch) {
            return Err(SourceError::protocol(format!(
                "gitea: {owner}/{repo}: {refusal}"
            )));
        }
    }
    let mut body = json!({
        "content": BASE64.encode(commit.content.as_bytes()),
        "message": commit.message,
        "branch": commit.branch,
        "sha": commit.base_sha,
    });
    if let Some(new_branch) = commit.new_branch {
        body["new_branch"] = json!(new_branch);
    }
    match client.put_contents(owner, repo, commit.path, &body).await {
        Ok(response) => {
            let answer: serde_json::Value = response.json().await.unwrap_or_default();
            Ok(answer["commit"]["sha"].as_str().map(ToOwned::to_owned))
        }
        // 422 is Gitea's answer to a stale `sha`; Forgejo answers 409 (#1195).
        // Both go on to the re-read below, which is what decides: a file that
        // did not move stays the server's refusal whichever status carried it.
        Err(error) if matches!(error.status(), Some(422 | 409)) => {
            // A re-send of a commit that already landed -- its answer lost,
            // or knobas gone before the settle (ADR-0012) -- finds its own
            // text where it was going: that is the write delivered, not a
            // file somebody else moved, and not a refusal.
            if let Some(new_branch) = commit.new_branch
                && let Ok(there) = client
                    .contents_entry_as::<FileAt>(owner, repo, commit.path, new_branch)
                    .await
                && there.text() == commit.content
            {
                return Ok(there.last_commit_sha);
            }
            let now = client
                .contents_entry_as::<FileAt>(owner, repo, commit.path, commit.branch)
                .await?;
            if commit.new_branch.is_none() && now.text() == commit.content {
                return Ok(now.last_commit_sha);
            }
            if now.sha == commit.base_sha {
                return Err(error);
            }
            Err(SourceError::Moved {
                current: now.text(),
                base: now.sha,
            })
        }
        Err(error) => Err(error),
    }
}

/// The text of a contents answer's base64 `content`, which Gitea may wrap.
/// Bytes that are not UTF-8 are shown replaced rather than refused: the reader
/// is being asked to compare, and an approximate other side beats none.
fn decoded(content: &str) -> String {
    let packed: String = content.split_whitespace().collect();
    let bytes = BASE64.decode(packed).unwrap_or_default();
    String::from_utf8_lossy(&bytes).into_owned()
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::time::Duration;

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
    ///
    /// Patience is a duration, so the test runs on tokio's paused clock
    /// (#1925), where the waits are exactly as long as they say and nothing
    /// else is waited on: the count and the time are both exact.
    #[tokio::test(start_paused = true)]
    async fn a_merge_still_refused_when_patience_runs_out_is_a_refusal() {
        let tries = Cell::new(0);
        let started = tokio::time::Instant::now();
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
        assert_eq!(started.elapsed(), Duration::from_millis(600));
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

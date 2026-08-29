//! The four writes M2 ratified for Gitea: branch, pull request, comment,
//! approval.
//!
//! Deliberately free of [`WriteOp`], for the same reason the Jira adapter's is:
//! knobas has one outbound write path, and
//! `knobas-sync/tests/write_choke_point.rs` refuses any production file that
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
//!
//! The comment goes on the **issue** with the pull request's number, which is
//! not a quirk of this adapter: Gitea keeps a pull request's discussion on the
//! issue of the same index, which is why `crate::client::issue_comments` reads
//! it from there too.

use serde_json::json;

use knobas_source::SourceError;

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

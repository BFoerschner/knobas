//! The reads a pull request's source view makes (#662), beside the ones the
//! sync makes, each one request and none stored (ADR-0020 rule 1); and the
//! branch update a pull request's toolbar writes (#708).

use knobas_http::Method;
use knobas_source::{SourceError, UpdateStyle};
use serde_json::Value;

use super::{API_ROOT, GiteaClient, page_params, segment};

impl GiteaClient {
    /// `GET /repos/{o}/{r}/pulls/{n}` -- one pull request, whole.
    pub(crate) async fn pull(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
    ) -> Result<Value, SourceError> {
        self.get_json(&pulls_path(owner, repo, number)?, &[]).await
    }

    /// `GET /repos/{o}/{r}/reviewers` -- the users a review can be requested
    /// from in the repository (#2303), as Gitea and Forgejo both answer it.
    pub(crate) async fn repo_reviewers(
        &self,
        owner: &str,
        repo: &str,
    ) -> Result<Vec<crate::model::UserRef>, SourceError> {
        let users: Option<Vec<crate::model::UserRef>> = self
            .get_json(
                &format!("/repos/{}/{}/reviewers", segment(owner)?, segment(repo)?),
                &[],
            )
            .await?;
        Ok(users.unwrap_or_default())
    }

    /// `GET /repos/{o}/{r}/pulls/{n}/reviews` -- every review, oldest first,
    /// a review request among them (`REQUEST_REVIEW`).
    pub(crate) async fn pull_reviews(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
    ) -> Result<Vec<Value>, SourceError> {
        // `null` for none, as the branch listing answers an empty repository.
        let reviews: Option<Vec<Value>> = self
            .get_json(
                &format!("{}/reviews", pulls_path(owner, repo, number)?),
                &[],
            )
            .await?;
        Ok(reviews.unwrap_or_default())
    }

    /// `GET /repos/{o}/{r}/pulls/{n}/reviews/{id}/comments` -- one review's
    /// line comments (#709). Gitea 1.27 has no listing of a pull request's
    /// line comments across its reviews, so the *Files* tab asks each review
    /// that says it has some. Unpaged, as Gitea serves it.
    pub(crate) async fn pull_review_comments(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        review: u64,
    ) -> Result<Vec<Value>, SourceError> {
        let comments: Option<Vec<Value>> = self
            .get_json(
                &format!(
                    "{}/reviews/{review}/comments",
                    pulls_path(owner, repo, number)?
                ),
                &[],
            )
            .await?;
        Ok(comments.unwrap_or_default())
    }

    /// `GET /repos/{o}/{r}/pulls/{n}/commits`, one page, with the server's
    /// own count of them all. `verification` and `files` default to on, as on
    /// `/commits`, and are turned off for the same reason.
    pub(crate) async fn pull_commits(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        page: u32,
    ) -> Result<(Vec<Value>, Option<usize>), SourceError> {
        let (limit, page) = page_params(page);
        let (batch, total): (Option<Vec<Value>>, _) = self
            .get_json_counted(
                &format!("{}/commits", pulls_path(owner, repo, number)?),
                &[
                    ("verification", "false".to_owned()),
                    ("files", "false".to_owned()),
                    limit,
                    page,
                ],
            )
            .await?;
        Ok((batch.unwrap_or_default(), total))
    }

    /// `GET /repos/{o}/{r}/pulls/{n}.diff` -- the pull request's change as
    /// `git diff` prints it. A refusal is the error it maps to, never an
    /// empty text.
    pub(crate) async fn pull_diff(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
    ) -> Result<String, SourceError> {
        self.get_text(&format!("{}.diff", pulls_path(owner, repo, number)?))
            .await
    }
}

impl GiteaClient {
    /// `POST /repos/{o}/{r}/pulls/{n}/update?style=` (#708): the base merged
    /// into the head, or the head rebased onto it. No body; Gitea answers an
    /// empty 200.
    ///
    /// # Errors
    ///
    /// The [`SourceError`] the failure maps to, Gitea's sentence in it: a
    /// **409** for a conflict, a **403** where the account may not push to
    /// the head.
    pub(crate) async fn update_pull(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        style: UpdateStyle,
    ) -> Result<(), SourceError> {
        let path = format!("{API_ROOT}{}/update", pulls_path(owner, repo, number)?);
        let request = self
            .http
            .request(Method::POST, &path)
            .query(&[("style", style.as_str().to_owned())]);
        self.http.send(request).await.map(|_| ())
    }
}

fn pulls_path(owner: &str, repo: &str, number: u64) -> Result<String, SourceError> {
    Ok(format!(
        "/repos/{}/{}/pulls/{number}",
        segment(owner)?,
        segment(repo)?
    ))
}

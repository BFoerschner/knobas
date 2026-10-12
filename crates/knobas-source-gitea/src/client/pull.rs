//! The reads a pull request's source view makes (#662), beside the ones the
//! sync makes. Each is one request; none is stored (ADR-0020 rule 1).

use knobas_source::SourceError;
use serde_json::Value;

use super::{GiteaClient, page_params, segment};

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

fn pulls_path(owner: &str, repo: &str, number: u64) -> Result<String, SourceError> {
    Ok(format!(
        "/repos/{}/{}/pulls/{number}",
        segment(owner)?,
        segment(repo)?
    ))
}

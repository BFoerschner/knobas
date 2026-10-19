//! The reads of an issue (#1206): the listing the sync walks, and one
//! issue's record. Its writes are `crate::write`'s.

use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::Value;

use knobas_source::SourceError;

use super::{GiteaClient, page_params, segment};

impl GiteaClient {
    /// `GET /repos/{o}/{r}/issues?type=issues&state=all` -- one page of the
    /// repository's issues, pull requests left out, closed ones in.
    ///
    /// `since` is Gitea's own filter on `updated_at`, server-side and
    /// **inclusive** (measured on 1.27.3, 2026-10-19: an issue updated on the
    /// very second `since` names is answered). The listing has no `sort` this
    /// walk can rely on (`sort=recentupdate` is ignored, the answer is newest
    /// created first), which is why the walk reads every page `since` admits
    /// rather than stopping at a watermark.
    pub(crate) async fn issues(
        &self,
        owner: &str,
        repo: &str,
        since: Option<DateTime<Utc>>,
        page: u32,
    ) -> Result<Vec<Value>, SourceError> {
        let (limit, page) = page_params(page);
        let mut query = vec![
            ("type", "issues".to_owned()),
            ("state", "all".to_owned()),
            limit,
            page,
        ];
        if let Some(since) = since {
            query.push(("since", since.to_rfc3339_opts(SecondsFormat::Secs, true)));
        }
        self.get_json(&issues_path(owner, repo)?, &query).await
    }

    /// `GET /repos/{o}/{r}/issues/{n}` -- one issue, whole: its state for
    /// `reachable_transitions`, and whether it is still there.
    pub(crate) async fn issue(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
    ) -> Result<Value, SourceError> {
        self.get_json(&format!("{}/{number}", issues_path(owner, repo)?), &[])
            .await
    }
}

/// `/repos/{o}/{r}/issues`, each segment checked.
pub(crate) fn issues_path(owner: &str, repo: &str) -> Result<String, SourceError> {
    Ok(format!("/repos/{}/{}/issues", segment(owner)?, segment(repo)?))
}

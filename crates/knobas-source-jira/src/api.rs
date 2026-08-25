//! The five calls the sync run makes.
//!
//! A trait rather than a direct dependency on [`crate::http::JiraHttp`],
//! because the interesting part of this adapter is paging and cursor
//! discipline -- and neither of those needs a socket to be tested. The HTTP
//! implementation lands beside it (task 6); the tests in [`crate::sync`] use a
//! fake.
//!
//! # The endpoint set, and every parameter it is allowed to send
//!
//! Straight out of `testenv/specs/jira-dc-rest.wadl`; anything else is a
//! recorded violation in `knobas-mockd`, which is what turns "the adapter
//! invented a parameter" into a red test in this crate's own suite.
//!
//! | Call | WADL resource | Parameters the WADL declares | What this adapter sends |
//! |---|---|---|---|
//! | [`server_info`](JiraApi::server_info) | `api/2/serverInfo` | `doHealthCheck` | *(none)* |
//! | [`myself`](JiraApi::myself) | `api/2/myself` | *(none)* | *(none)* |
//! | [`search`](JiraApi::search) | `api/2/search` | `jql`, `startAt`, `maxResults`, `validateQuery`, `fields`, `expand` | `jql`, `startAt`, `maxResults`, `fields` |
//! | [`comments`](JiraApi::comments) | `api/2/issue/{key}/comment` | `startAt`, `maxResults`, `orderBy`, `expand` | `startAt`, `maxResults` |
//! | [`worklogs`](JiraApi::worklogs) | `api/2/issue/{key}/worklog` | **none at all** | *(none)* |
//! | — | `api/2/issue/{key}` | `fields`, `expand`, `properties`, `updateHistory` | **unused in M1** |
//!
//! Two consequences worth naming:
//!
//! * **`expand=renderedFields` is deliberately not sent.** It returns Jira's
//!   HTML rendering, and roadmap §4 gotcha 7 is exactly about HTML reaching
//!   the webview. `description` and comment bodies in REST v2 are wiki markup
//!   -- plain text, which is what FTS wants and what the §3a generic detail
//!   view can project safely.
//! * **`GET /rest/api/2/issue/{key}` is not called.** `/search` already
//!   returns `fields`, so a per-issue GET would be an N+1 for nothing. It
//!   stays in mockd's set for M2's write-back re-read.

use knobas_source::SourceError;

use crate::model::{CommentPage, Myself, SearchPage, ServerInfo, WorklogPage};

#[async_trait::async_trait]
pub(crate) trait JiraApi: Send + Sync {
    /// `GET /rest/api/2/serverInfo` -- the version for the sources view, and
    /// the server's UTC offset, which JQL date literals are read in.
    async fn server_info(&self) -> Result<ServerInfo, SourceError>;
    /// `GET /rest/api/2/myself` -- who the credential is.
    async fn myself(&self) -> Result<Myself, SourceError>;
    /// `GET /rest/api/2/search` -- the classic `startAt`/`total` page.
    async fn search(
        &self,
        jql: &str,
        start_at: u32,
        max_results: u32,
        fields: &str,
    ) -> Result<SearchPage, SourceError>;
    /// `GET /rest/api/2/issue/{key}/comment`.
    async fn comments(
        &self,
        key: &str,
        start_at: u32,
        max_results: u32,
    ) -> Result<CommentPage, SourceError>;
    /// `GET /rest/api/2/issue/{key}/worklog` -- no parameters exist on this
    /// resource, so there is no paging to do.
    async fn worklogs(&self, key: &str) -> Result<WorklogPage, SourceError>;
}

/// The real endpoints. Every path and every parameter here is declared in
/// `testenv/specs/jira-dc-rest.wadl`; `knobas-mockd` records anything else as a
/// violation, which is what turns "the adapter invented an endpoint" into a red
/// test in this crate's own suite rather than a surprise against a real Jira.
///
/// The issue key is interpolated into the path. Keys come from Jira's own
/// responses (`PAY-231`) and are `[A-Z0-9_]+-\d+`, so no escaping is required;
/// a key that is not one would 404 and be reported as
/// [`SourceError::Protocol`].
#[async_trait::async_trait]
impl JiraApi for crate::http::JiraHttp {
    async fn server_info(&self) -> Result<ServerInfo, SourceError> {
        self.get_json("rest/api/2/serverInfo", &[]).await
    }

    async fn myself(&self) -> Result<Myself, SourceError> {
        self.get_json("rest/api/2/myself", &[]).await
    }

    async fn search(
        &self,
        jql: &str,
        start_at: u32,
        max_results: u32,
        fields: &str,
    ) -> Result<SearchPage, SourceError> {
        // Data Center's classic search. NOT Cloud's /rest/api/3/search/jql --
        // roadmap §4 gotcha 4. `validateQuery` and `expand` are left at their
        // defaults: renderedFields would give us HTML (gotcha 7).
        self.get_json(
            "rest/api/2/search",
            &[
                ("jql", jql.to_owned()),
                ("startAt", start_at.to_string()),
                ("maxResults", max_results.to_string()),
                ("fields", fields.to_owned()),
            ],
        )
        .await
    }

    async fn comments(
        &self,
        key: &str,
        start_at: u32,
        max_results: u32,
    ) -> Result<CommentPage, SourceError> {
        self.get_json(
            &format!("rest/api/2/issue/{key}/comment"),
            &[
                ("startAt", start_at.to_string()),
                ("maxResults", max_results.to_string()),
            ],
        )
        .await
    }

    async fn worklogs(&self, key: &str) -> Result<WorklogPage, SourceError> {
        // No parameters: the WADL declares none on this resource.
        self.get_json(&format!("rest/api/2/issue/{key}/worklog"), &[])
            .await
    }
}

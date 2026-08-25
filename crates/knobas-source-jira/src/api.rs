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

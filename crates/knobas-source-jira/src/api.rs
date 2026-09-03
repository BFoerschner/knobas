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
//! | [`fields`](JiraApi::fields) | `api/2/field` | *(none)* | *(none)* |
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
//! * **`api/2/field` is read by `test_connection` and by nothing else** (#297).
//!   It is the whole field table of the instance, which is a big answer to a
//!   small question, and the question is asked once per *Test connection* --
//!   never per sync run, never per issue. `knobas-mockd` does not serve it
//!   (its deviation 13), so the endpoint's witness is
//!   `tests/live_jira_seeded.rs` against the real product.

use knobas_source::SourceError;

use crate::model::{CommentPage, FieldMeta, Myself, SearchPage, ServerInfo, WorklogPage};

#[async_trait::async_trait]
pub(crate) trait JiraApi: Send + Sync {
    /// `GET /rest/api/2/serverInfo` -- the version for the sources view, and
    /// the server's UTC offset, which JQL date literals are read in.
    async fn server_info(&self) -> Result<ServerInfo, SourceError>;
    /// `GET /rest/api/2/myself` -- who the credential is.
    async fn myself(&self) -> Result<Myself, SourceError>;
    /// `GET /rest/api/2/field` -- every field this instance has, so that
    /// [`crate::discover`] can name the Epic Link one (#297).
    ///
    /// Called by `test_connection` only; a sync run never asks.
    async fn fields(&self) -> Result<Vec<FieldMeta>, SourceError>;
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

    async fn fields(&self) -> Result<Vec<FieldMeta>, SourceError> {
        // No parameters: the WADL declares none on this resource, and the
        // answer is the whole table either way.
        self.get_json("rest/api/2/field", &[]).await
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::{JiraHttp, credential};
    use knobas_source::AuthMethod;

    /// Every call in this trait, once, against `knobas-mockd`.
    ///
    /// `tests/mockd.rs` drives the adapter and therefore only reaches the
    /// endpoints a *sync* happens to need -- and with `fields=comment,worklog`
    /// honoured, the fixture's containers arrive complete, so
    /// [`JiraApi::comments`] and [`JiraApi::worklogs`] are never called over the
    /// wire there at all. Their paths and their parameter sets would then be
    /// pinned by nothing but the fake, which answers whatever it is asked. This
    /// test calls all five directly, so an invented path or an undeclared query
    /// parameter on any of them is a recorded violation here.
    ///
    /// **[`JiraApi::fields`] is the sixth call and is deliberately not among
    /// them.** mockd serves no `api/2/field` handler (its deviation 13), so
    /// asking it over the wire records an `Unimplemented` violation about
    /// *mockd* and certifies nothing about the adapter. mockd is frozen
    /// (ADR-0013), so the route is not added. What is left is the question
    /// this test really asks -- is the path one the contract declares? -- and
    /// [`the_field_table_is_a_path_the_wadl_declares`] asks it of the WADL
    /// tables directly. The behaviour over a socket is certified against the
    /// real product in `tests/live_jira_seeded.rs`.
    #[tokio::test]
    async fn every_call_is_one_the_wadl_declares() {
        let jira = knobas_mockd::spawn_mock_jira().await;
        let http = JiraHttp::new(
            &jira.base_url(),
            &crate::JiraConfig::default(),
            credential(Some(AuthMethod::Pat), None, Some(knobas_mockd::JIRA_TOKEN))
                .expect("a PAT is a credential"),
        )
        .expect("mockd's base URL is a URL");

        let server = http.server_info().await.expect("serverInfo answers");
        assert_eq!(server.version.as_deref(), Some("9.17.0"));
        // The whole reason `serverInfo` is called at all: mockd is on +02:00,
        // and the JQL literal has to be rendered in that zone.
        assert_eq!(server.offset_secs(), 2 * 3600);

        let me = http.myself().await.expect("myself answers");
        assert_eq!(me.name.as_deref(), Some("mara.lindqvist"));

        let page = http
            .search(
                "ORDER BY updated ASC",
                0,
                2,
                "summary,updated,comment,worklog",
            )
            .await
            .expect("search answers");
        assert_eq!(page.total, 7);
        assert_eq!(page.issues.len(), 2);

        let comments = http
            .comments("PAY-231", 0, 1)
            .await
            .expect("the comment endpoint answers");
        assert_eq!(comments.total, 2);
        assert_eq!(
            comments.comments.len(),
            1,
            "startAt/maxResults are honoured"
        );

        let worklogs = http
            .worklogs("PAY-231")
            .await
            .expect("the worklog endpoint answers");
        assert_eq!(worklogs.worklogs.len(), 1);

        jira.assert_no_violations();
    }

    /// The sixth call's path and its (empty) parameter set, against the same
    /// document the test above validates the other five against.
    ///
    /// Read off `knobas_mockd::allowlist` -- which `build.rs` generates from
    /// the pinned `testenv/specs/jira-dc-rest.wadl` -- rather than over a
    /// socket, because mockd has no handler for this path and would answer a
    /// 501 that says nothing about whether the path is in the contract.
    #[test]
    fn the_field_table_is_a_path_the_wadl_declares() {
        let knobas_mockd::allowlist::Lookup::Allowed { query } =
            knobas_mockd::allowlist::lookup("GET", "api/2/field")
        else {
            panic!(
                "GET api/2/field is not in testenv/specs/jira-dc-rest.wadl; this adapter must not \
                 send a path the contract does not declare"
            )
        };
        assert!(
            query.is_empty(),
            "the WADL declares no query parameter on api/2/field, and the adapter sends \
             none: {query:?}"
        );
    }
}

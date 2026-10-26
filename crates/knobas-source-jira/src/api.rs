//! The five calls the sync run makes.
//!
//! A trait rather than a direct dependency on [`crate::http::HttpClient`],
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
//! | [`my_permissions`](JiraApi::my_permissions) | `api/2/mypermissions` | `projectKey`, `projectId`, `issueKey`, `issueId` | `projectKey` (#2052) |
//! | [`issue_stamp`](JiraApi::issue_stamp) | `api/2/issue/{key}` | `fields`, `expand`, `properties`, `updateHistory` | `fields=updated`, a refresh's probe only (#2097) |
//!
//! Consequences worth naming:
//!
//! * **`expand=changelog` is not sent** (#2165, retiring #929's adapter
//!   half). It put each issue's history in the stored record for the
//!   conversation's *Include changes*, which the conversation no longer
//!   draws (#2163). Data Center returns that history whole, with no paged or
//!   status-only endpoint
//!   (`/issue/{key}/changelog` answers 404), and a description edit's old and
//!   new text whole: 10 issues with 1000 edits each made one page 54 MB and
//!   2.5–3.1 s where the same page without it was 237 KB and under 50 ms,
//!   over the transport's 32 MiB cap, so the run failed. A ticket's current
//!   status is a field and still arrives as one
//!   (`live_jira_seeded::a_long_history_syncs_without_its_changelog`).
//! * **`expand=renderedFields` is deliberately not sent.** It returns Jira's
//!   HTML rendering, and roadmap §4 gotcha 7 is exactly about HTML reaching
//!   the webview. `description` and comment bodies in REST v2 are wiki markup
//!   -- plain text, which is what FTS wants and what the §3a generic detail
//!   view can project safely.
//! * **`GET /rest/api/2/issue/{key}` is never a sync's read.** `/search`
//!   already returns `fields`, so a per-issue GET would be an N+1 for
//!   nothing. A refresh (#2097) asks it for `updated` alone, to learn whether
//!   the issue exists and whether the search index has caught up with it;
//!   the item it answers is still the search's ([`crate::refresh`]).
//! * **`api/2/field` is read by `test_connection` and by nothing else** (#297).
//!   It is the whole field table of the instance, which is a big answer to a
//!   small question, and the question is asked once per *Test connection* --
//!   never per sync run, never per issue. `knobas-mockd` does not serve it
//!   (its deviation 13), so the endpoint's witness is
//!   `tests/live_jira_seeded.rs` against the real product.

use knobas_source::SourceError;

use crate::http::get_json;
use crate::model::{CommentPage, FieldMeta, Issue, Myself, SearchPage, ServerInfo, WorklogPage};

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
    /// `GET /rest/api/2/issue/{key}?fields=updated`: the issue's current key
    /// and `updated` as the **database** holds them, or `None` for a key Jira
    /// answers 404 (deleted, or not this account's to see). A refresh's probe
    /// ([`crate::refresh`], #2097), never its item: the item is the search's.
    /// A sync run's fakes model no probe, so the default is no answer.
    async fn issue_stamp(&self, _key: &str) -> Result<Option<Issue>, SourceError> {
        Err(SourceError::protocol("this Jira answers no issue read"))
    }

    // ---- Jira views (#877): Jira Software's Agile REST, read-only ----
    //
    // Each defaults to the 404 a Jira without Jira Software answers, which is
    // what the sync run's fakes stand for: they model no boards, and
    // `crate::views` reads a 404 on the board listing as "no boards here".

    /// `GET rest/agile/1.0/board?projectKeyOrId=..`, one page; with no
    /// project, every board this account can see (#878).
    async fn boards(
        &self,
        _project: Option<&str>,
        _start_at: u32,
        _max_results: u32,
    ) -> Result<crate::views::BoardPage, SourceError> {
        Err(no_agile())
    }
    /// `GET rest/agile/1.0/board/{id}/configuration`.
    async fn board_configuration(
        &self,
        _board: u64,
    ) -> Result<crate::views::BoardConfiguration, SourceError> {
        Err(no_agile())
    }
    /// `GET rest/agile/1.0/board/{id}/sprint?state=active`.
    async fn active_sprints(&self, _board: u64) -> Result<crate::views::SprintPage, SourceError> {
        Err(no_agile())
    }
    /// `GET rest/api/2/filter/{id}`.
    async fn filter(&self, _id: &str) -> Result<crate::views::Filter, SourceError> {
        Err(no_agile())
    }
    /// `GET rest/api/2/filter/favourite`: this account's favourite filters
    /// (#878). Core REST, not Agile: a fake that models none answers none.
    async fn favourite_filters(&self) -> Result<Vec<crate::views::FavouriteFilter>, SourceError> {
        Ok(Vec::new())
    }
    /// `GET rest/api/2/search?fields=key`: one page of keys. Its own shape,
    /// because Jira answers `"fields": null` on every issue of a keys-only
    /// search (measured against Jira DC 10.3, #877), which the issue walk's
    /// record does not accept. A fake answers through its [`search`](Self::search).
    async fn search_keys(
        &self,
        jql: &str,
        start_at: u32,
        max_results: u32,
    ) -> Result<crate::views::KeysPage, SourceError> {
        let page = self.search(jql, start_at, max_results, "key").await?;
        Ok(crate::views::KeysPage {
            total: page.total,
            issues: page
                .issues
                .into_iter()
                .map(|raw| crate::views::KeyOnly { key: raw.issue.key })
                .collect(),
        })
    }
    /// `GET rest/api/2/status`: every status, for the names a board's
    /// columns hold by id.
    async fn statuses(&self) -> Result<Vec<crate::views::Status>, SourceError> {
        Err(no_agile())
    }

    /// `GET rest/api/2/mypermissions?projectKey=..`: every permission, with
    /// whether this user holds it in the project (#2052, [`crate::grants`]).
    /// A sync run's fakes model no permissions, so the default is no answer.
    async fn my_permissions(
        &self,
        _project: &str,
    ) -> Result<crate::grants::MyPermissions, SourceError> {
        Err(SourceError::protocol("this Jira answers no permissions"))
    }
}

/// The answer of a Jira that has no Jira Software, so no Agile REST.
fn no_agile() -> SourceError {
    SourceError::Protocol {
        status: Some(404),
        message: "this Jira answers no Agile REST".to_owned(),
    }
}

/// What a sync run's `/search` sends.
///
/// A refresh asks the same query (`crate::refresh`). No `expand` (#2165):
/// `changelog` is a ticket's whole edit history, which no reader wants and
/// which outgrew the transport's response cap (module docs), and
/// `renderedFields` would give us HTML (gotcha 7). `validateQuery` stays at
/// its default.
fn search_query(
    jql: &str,
    start_at: u32,
    max_results: u32,
    fields: &str,
) -> [(&'static str, String); 4] {
    [
        ("jql", jql.to_owned()),
        ("startAt", start_at.to_string()),
        ("maxResults", max_results.to_string()),
        ("fields", fields.to_owned()),
    ]
}

/// The real endpoints. Every path and every parameter here is declared in
/// `testenv/specs/jira-dc-rest.wadl`; `knobas-mockd` records anything else as a
/// violation, which is what turns "the adapter invented an endpoint" into a red
/// test in this crate's own suite rather than a surprise against a real Jira.
///
/// The issue key is interpolated into the path, unescaped. A read's key comes
/// from Jira's own responses (`PAY-231`, `[A-Z][A-Z0-9_]*-\d+`); a write's
/// comes from a queue row, and `JiraSource::issue_key` refuses one outside
/// that grammar before it reaches here (#1644).
#[async_trait::async_trait]
impl JiraApi for crate::http::HttpClient {
    async fn server_info(&self) -> Result<ServerInfo, SourceError> {
        get_json(self, "rest/api/2/serverInfo", &[]).await
    }

    async fn myself(&self) -> Result<Myself, SourceError> {
        get_json(self, "rest/api/2/myself", &[]).await
    }

    async fn fields(&self) -> Result<Vec<FieldMeta>, SourceError> {
        // No parameters: the WADL declares none on this resource, and the
        // answer is the whole table either way.
        get_json(self, "rest/api/2/field", &[]).await
    }

    async fn search(
        &self,
        jql: &str,
        start_at: u32,
        max_results: u32,
        fields: &str,
    ) -> Result<SearchPage, SourceError> {
        // Data Center's classic search. NOT Cloud's /rest/api/3/search/jql --
        // roadmap §4 gotcha 4.
        get_json(
            self,
            "rest/api/2/search",
            &search_query(jql, start_at, max_results, fields),
        )
        .await
    }

    async fn comments(
        &self,
        key: &str,
        start_at: u32,
        max_results: u32,
    ) -> Result<CommentPage, SourceError> {
        get_json(
            self,
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
        get_json(self, &format!("rest/api/2/issue/{key}/worklog"), &[]).await
    }

    async fn issue_stamp(&self, key: &str) -> Result<Option<Issue>, SourceError> {
        match get_json(
            self,
            &format!("rest/api/2/issue/{key}"),
            &[("fields", "updated".to_owned())],
        )
        .await
        {
            Ok(issue) => Ok(Some(issue)),
            Err(error) if error.status() == Some(404) => Ok(None),
            Err(error) => Err(error),
        }
    }

    async fn boards(
        &self,
        project: Option<&str>,
        start_at: u32,
        max_results: u32,
    ) -> Result<crate::views::BoardPage, SourceError> {
        let mut query = vec![
            ("startAt", start_at.to_string()),
            ("maxResults", max_results.to_string()),
        ];
        if let Some(project) = project {
            query.insert(0, ("projectKeyOrId", project.to_owned()));
        }
        get_json(self, "rest/agile/1.0/board", &query).await
    }

    async fn favourite_filters(&self) -> Result<Vec<crate::views::FavouriteFilter>, SourceError> {
        get_json(self, "rest/api/2/filter/favourite", &[]).await
    }

    async fn board_configuration(
        &self,
        board: u64,
    ) -> Result<crate::views::BoardConfiguration, SourceError> {
        get_json(
            self,
            &format!("rest/agile/1.0/board/{board}/configuration"),
            &[],
        )
        .await
    }

    async fn active_sprints(&self, board: u64) -> Result<crate::views::SprintPage, SourceError> {
        get_json(
            self,
            &format!("rest/agile/1.0/board/{board}/sprint"),
            &[("state", "active".to_owned())],
        )
        .await
    }

    async fn filter(&self, id: &str) -> Result<crate::views::Filter, SourceError> {
        get_json(self, &format!("rest/api/2/filter/{id}"), &[]).await
    }

    async fn statuses(&self) -> Result<Vec<crate::views::Status>, SourceError> {
        get_json(self, "rest/api/2/status", &[]).await
    }

    async fn my_permissions(
        &self,
        project: &str,
    ) -> Result<crate::grants::MyPermissions, SourceError> {
        get_json(
            self,
            "rest/api/2/mypermissions",
            &[("projectKey", project.to_owned())],
        )
        .await
    }

    async fn search_keys(
        &self,
        jql: &str,
        start_at: u32,
        max_results: u32,
    ) -> Result<crate::views::KeysPage, SourceError> {
        get_json(
            self,
            "rest/api/2/search",
            &[
                ("jql", jql.to_owned()),
                ("startAt", start_at.to_string()),
                ("maxResults", max_results.to_string()),
                ("fields", "key".to_owned()),
            ],
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::{CREDENTIAL_RULES, client, credential};
    use knobas_source::AuthMethod;

    /// Every call in this trait, once, against `knobas-mockd`.
    ///
    /// `tests/it/mockd.rs` drives the adapter and therefore only reaches the
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
        let http = client(
            &jira.base_url(),
            &crate::JiraConfig::default(),
            credential(
                &CREDENTIAL_RULES,
                Some(AuthMethod::Pat),
                None,
                Some(knobas_mockd::JIRA_TOKEN),
            )
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

    /// A sync run's search, and so a refresh's, sends no `expand` at all
    /// (#2165): not `changelog`, whose whole history made one 10-issue page
    /// 54 MB (#2163), and not `renderedFields`' HTML (gotcha 7).
    #[test]
    fn the_search_sends_no_expand() {
        let query = search_query("ORDER BY updated ASC", 0, 50, "summary");
        let names: Vec<&str> = query.iter().map(|(name, _)| *name).collect();
        assert_eq!(names, ["jql", "startAt", "maxResults", "fields"]);
    }

    /// The sixth call's path and its (empty) parameter set, against the same
    /// document `every_call_is_one_the_wadl_declares` validates the other five
    /// against.
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

    /// The permission read's path and the one parameter it sends (#2052),
    /// against the WADL tables: mockd serves no handler for it either.
    #[test]
    fn the_permission_read_is_a_path_the_wadl_declares() {
        let knobas_mockd::allowlist::Lookup::Allowed { query } =
            knobas_mockd::allowlist::lookup("GET", "api/2/mypermissions")
        else {
            panic!("GET api/2/mypermissions is not in testenv/specs/jira-dc-rest.wadl")
        };
        assert!(
            query.contains(&"projectKey"),
            "the read sends projectKey, which the WADL must declare: {query:?}"
        );
    }
}

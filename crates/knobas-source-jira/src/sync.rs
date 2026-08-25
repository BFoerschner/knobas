//! One incremental (or full) sync run.
//!
//! The shape, and why each part is there:
//!
//! 1. `GET serverInfo` -- the server's UTC offset, because JQL date literals
//!    carry no zone.
//! 2. Read the cursor. Unreadable or absent ⇒ full sync (interfaces §4.1).
//! 3. Query `updated >= watermark − 2 minutes ORDER BY updated ASC`, paging
//!    with `startAt` until `total` is reached.
//! 4. Drop what the overlap re-delivered, complete truncated comment/worklog
//!    containers, push the rest into the sink.
//! 5. If nothing was pushed, hand back the cursor byte-identically. Otherwise
//!    advance the watermark to `max(previous, newest updated emitted)` -- never
//!    to `now()`, because Jira's search index lags its own writes by seconds
//!    and an issue updated in that gap would be skipped forever.
//!
//! # The two things the cursor layer is owed
//!
//! [`JiraCursor::since`] documents them and this is where they are honoured;
//! they fail in different places, so neither substitutes for the other.
//!
//! * **`max(previous, …)`** on the watermark, so a re-delivered *older* item
//!   cannot drag the position backwards.
//! * **`seen_in_window` is every pair the run saw**, skipped ones included --
//!   `seen` records what the *window* contained, not what crossed the SPI. A
//!   run that reports only what it emitted forgets, every run, whatever it
//!   recognised that run, and an idle poll never settles. The watermark holds
//!   perfectly while that happens, which is why the sequence that shows it is
//!   *one new issue and then an idle poll*, never a full sync and then one.

use chrono::{DateTime, Utc};
use knobas_source::{Sink, SourceError};

use crate::api::JiraApi;
use crate::cursor::JiraCursor;
use crate::jql::build_jql;
use crate::map;
use crate::model::{RawIssue, SearchPage, ServerInfo};
use crate::{JiraConfig, time::parse_jira_time};

/// The fields `/search` is asked for.
///
/// Explicit rather than Jira's `*navigable` default: a deterministic payload,
/// a smaller response, and no surprise when an instance changes its navigable
/// set. `comment` and `worklog` are in the list -- neither is navigable -- so
/// the common case costs one request per page instead of one per issue.
///
/// **Why exactly these twelve.** `fields=` accepts any field a Jira instance
/// has, and a wider list (`labels`, `resolution`, `parent`, `issuelinks`,
/// `components`, `fixVersions`, the time-tracking trio) would enrich `payload`
/// on a real instance. It would also be a request no test in this workspace
/// can certify: `knobas-mockd` validates `fields=` against the closed set its
/// WADL-derived fixture actually serves, and answers anything else with a 400
/// and a recorded `UnknownField` violation. Shipping an uncertified query
/// parameter is precisely what the mockd gate exists to prevent, so the list
/// stops where the certification stops. Widening it is one constant here plus
/// one constant in mockd, in that order.
const BASE_FIELDS: &str = "summary,description,issuetype,status,priority,assignee,reporter,\
project,created,updated,comment,worklog";

/// A stop so a server that keeps reporting "more" cannot spin a run forever.
/// At the default page size this is half a million issues.
const MAX_PAGES: u32 = 5_000;

pub(crate) struct SyncRun<'a> {
    pub api: &'a (dyn JiraApi + 'a),
    pub cfg: &'a JiraConfig,
    /// The instance id -- the `EntityRef` namespace (P10), not the adapter kind.
    pub source_id: &'a str,
    pub base_url: &'a str,
}

impl SyncRun<'_> {
    pub(crate) async fn run(
        &self,
        cursor: Option<String>,
        sink: &mut (dyn Sink + Send),
    ) -> Result<String, SourceError> {
        let server: ServerInfo = self.api.server_info().await?;
        let offset = server.offset_secs();
        let previous = cursor.as_deref().and_then(JiraCursor::parse);
        let since = previous.as_ref().and_then(|c| c.since(offset));
        let jql = build_jql(self.cfg, since, offset);
        let fields = self.fields();

        let mut start_at: u32 = 0;
        let mut pages: u32 = 0;
        let mut emitted: usize = 0;
        // Every `(key, updated)` this run *saw* in the window -- see the module
        // docs. Skipped pairs belong here as much as emitted ones.
        let mut seen_in_window: Vec<(String, DateTime<Utc>)> = Vec::new();
        // Seeded from the previous position, which is what makes the new one
        // `max(previous, newest emitted)` rather than "newest emitted".
        let mut watermark: Option<DateTime<Utc>> = previous.as_ref().and_then(|c| c.updated_to);

        loop {
            let page: SearchPage = self
                .api
                .search(&jql, start_at, self.cfg.page_size, &fields)
                .await?;
            let total = page.total;
            let returned = page.issues.len() as u32;
            if returned == 0 {
                break;
            }
            for mut raw in page.issues {
                let updated = raw
                    .issue
                    .fields
                    .updated
                    .as_deref()
                    .and_then(parse_jira_time);
                if let Some(u) = updated {
                    seen_in_window.push((raw.issue.key.clone(), u));
                }
                // The overlap exists so nothing is missed; this is what keeps
                // it from also meaning "everything arrives twice".
                if previous
                    .as_ref()
                    .is_some_and(|c| c.already_delivered(&raw.issue.key, updated))
                {
                    continue;
                }
                self.complete(&mut raw).await?;
                let item = map::to_sync_item(self.source_id, self.base_url, &raw);
                // Sink failures are never swallowed (SPI doc on `Source::sync`).
                sink.item(item).await?;
                emitted += 1;
                if let Some(u) = updated {
                    watermark = Some(watermark.map_or(u, |w| w.max(u)));
                }
            }
            start_at += returned;
            pages += 1;
            if start_at >= total || pages >= MAX_PAGES {
                break;
            }
        }

        match (emitted, watermark) {
            // Battery clause 2: nothing happened, so the position did not move.
            // Returning the *input string* rather than a re-encoded struct makes
            // that byte-identical by construction.
            (0, _) => Ok(cursor.unwrap_or_else(|| JiraCursor::empty(offset).encode())),
            // Items arrived, but neither they nor the previous cursor carry a
            // readable `updated`; advancing to anything would be a guess. An
            // issue with no `updated` at all is therefore re-delivered on every
            // run -- `already_delivered` cannot recognise a pair with no
            // timestamp. Real Jira always sets the field, and mockd synthesizes
            // it for the fixture's one null, so this is a tolerated shape and
            // not a supported one.
            (_, None) => Ok(cursor.unwrap_or_else(|| JiraCursor::empty(offset).encode())),
            (_, Some(w)) => Ok(JiraCursor::advanced(w, offset, &seen_in_window).encode()),
        }
    }

    /// Fill in what `/search` truncated.
    ///
    /// A container that is absent is treated as truncated: a server that
    /// ignored `fields=comment` must not be read as "this issue has no
    /// comments" -- that would silently drop the discussion out of the search
    /// corpus.
    async fn complete(&self, raw: &mut RawIssue) -> Result<(), SourceError> {
        let key = raw.issue.key.clone();

        let comments_truncated = match &raw.issue.fields.comment {
            None => true,
            Some(c) => c.total.unwrap_or(0) as usize > c.comments.len(),
        };
        if comments_truncated {
            let mut all = Vec::new();
            let mut start_at: u32 = 0;
            loop {
                let page = self
                    .api
                    .comments(&key, start_at, self.cfg.page_size)
                    .await?;
                let returned = page.comments.len() as u32;
                if returned == 0 {
                    break;
                }
                all.extend(page.comments);
                start_at += returned;
                if start_at >= page.total {
                    break;
                }
            }
            raw.set_comments(all);
        }

        let worklogs_truncated = match &raw.issue.fields.worklog {
            None => true,
            Some(w) => w.total.unwrap_or(0) as usize > w.worklogs.len(),
        };
        if worklogs_truncated {
            // No paging: the WADL declares no query parameters on this
            // resource. What the server returns is what M1 records;
            // `fields.worklog.total` keeps the truth visible. M3's time work
            // uses /rest/api/2/worklog/updated + /worklog/list instead.
            let page = self.api.worklogs(&key).await?;
            raw.set_worklogs(page.worklogs);
        }
        Ok(())
    }

    fn fields(&self) -> String {
        let mut fields = BASE_FIELDS.to_owned();
        if let Some(extra) = &self.cfg.epic_link_field {
            fields.push(',');
            fields.push_str(extra);
        }
        fields
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // `super::*` already brings in Sink, SourceError, JiraApi, SearchPage and
    // ServerInfo; the fake also has to answer the other three calls.
    use crate::model::{CommentPage, Myself, WorklogPage};
    use knobas_source::contract::VecSink;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// An issue as `/search` would return it, with a comment container that may
    /// claim more comments than it carries.
    fn issue(key: &str, updated: &str, inline: usize, total: usize) -> serde_json::Value {
        serde_json::json!({
            "key": key,
            "fields": {
                "summary": format!("{key} summary"),
                "updated": updated,
                "comment": {
                    "startAt": 0, "maxResults": inline, "total": total,
                    "comments": (0..inline).map(|i| serde_json::json!({
                        "id": format!("{key}-c{i}"), "body": format!("inline {i}")
                    })).collect::<Vec<_>>()
                },
                "worklog": { "startAt": 0, "maxResults": 0, "total": 0, "worklogs": [] }
            }
        })
    }

    #[derive(Default)]
    struct FakeApi {
        issues: Vec<serde_json::Value>,
        comments: HashMap<String, Vec<serde_json::Value>>,
        worklogs: HashMap<String, Vec<serde_json::Value>>,
        unauthorized: bool,
        calls: Mutex<Vec<String>>,
    }

    impl FakeApi {
        fn calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }
        fn count(&self, prefix: &str) -> usize {
            self.calls()
                .iter()
                .filter(|c| c.starts_with(prefix))
                .count()
        }
        fn searches(&self) -> Vec<String> {
            self.calls()
                .into_iter()
                .filter(|c| c.starts_with("search@"))
                .collect()
        }
        /// Rewrite one issue's `updated`, the way an edit upstream would.
        fn edit(&mut self, key: &str, updated: &str) {
            let hit = self
                .issues
                .iter_mut()
                .find(|i| i["key"] == key)
                .unwrap_or_else(|| panic!("no {key} in the fake"));
            hit["fields"]["updated"] = serde_json::json!(updated);
        }
    }

    /// The fake honours the one JQL clause the run depends on. Without it the
    /// incremental tests would prove nothing.
    fn since_of(jql: &str) -> Option<chrono::DateTime<chrono::Utc>> {
        let rest = jql.split("updated >= \"").nth(1)?;
        let literal = rest.split('"').next()?;
        chrono::NaiveDateTime::parse_from_str(literal, "%Y-%m-%d %H:%M")
            .ok()
            .map(|n| n.and_utc())
    }

    #[async_trait::async_trait]
    impl JiraApi for FakeApi {
        async fn server_info(&self) -> Result<ServerInfo, SourceError> {
            self.calls.lock().unwrap().push("serverInfo".to_owned());
            Ok(serde_json::from_value(serde_json::json!({
                "version": "9.17.0", "deploymentType": "Server",
                "serverTime": "2026-08-22T11:48:00.000+0000"
            }))
            .unwrap())
        }

        async fn myself(&self) -> Result<Myself, SourceError> {
            Ok(serde_json::from_value(serde_json::json!({ "name": "mara.lindqvist" })).unwrap())
        }

        async fn search(
            &self,
            jql: &str,
            start_at: u32,
            max_results: u32,
            _fields: &str,
        ) -> Result<SearchPage, SourceError> {
            if self.unauthorized {
                return Err(SourceError::Unauthorized);
            }
            self.calls
                .lock()
                .unwrap()
                .push(format!("search@{start_at}"));
            let since = since_of(jql);
            let matching: Vec<serde_json::Value> = self
                .issues
                .iter()
                .filter(|i| {
                    let updated = i["fields"]["updated"]
                        .as_str()
                        .and_then(crate::time::parse_jira_time);
                    match (since, updated) {
                        (Some(s), Some(u)) => u >= s,
                        (Some(_), None) => false,
                        (None, _) => true,
                    }
                })
                .cloned()
                .collect();
            let window: Vec<serde_json::Value> = matching
                .iter()
                .skip(start_at as usize)
                .take(max_results as usize)
                .cloned()
                .collect();
            Ok(serde_json::from_value(serde_json::json!({
                "startAt": start_at, "maxResults": max_results,
                "total": matching.len(), "issues": window
            }))
            .unwrap())
        }

        async fn comments(
            &self,
            key: &str,
            start_at: u32,
            max_results: u32,
        ) -> Result<CommentPage, SourceError> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("comments:{key}@{start_at}"));
            let all = self.comments.get(key).cloned().unwrap_or_default();
            let window: Vec<serde_json::Value> = all
                .iter()
                .skip(start_at as usize)
                .take(max_results as usize)
                .cloned()
                .collect();
            Ok(serde_json::from_value(serde_json::json!({
                "startAt": start_at, "maxResults": max_results,
                "total": all.len(), "comments": window
            }))
            .unwrap())
        }

        async fn worklogs(&self, key: &str) -> Result<WorklogPage, SourceError> {
            self.calls.lock().unwrap().push(format!("worklogs:{key}"));
            let all = self.worklogs.get(key).cloned().unwrap_or_default();
            Ok(serde_json::from_value(serde_json::json!({
                "startAt": 0, "maxResults": all.len(), "total": all.len(), "worklogs": all
            }))
            .unwrap())
        }
    }

    struct FailingSink;
    #[async_trait::async_trait]
    impl Sink for FailingSink {
        async fn item(&mut self, _item: knobas_source::SyncItem) -> Result<(), SourceError> {
            Err(SourceError::Sink("sink is down".to_owned()))
        }
    }

    fn cfg(value: serde_json::Value) -> crate::JiraConfig {
        crate::JiraConfig::from_json(&value).unwrap()
    }

    async fn run(
        api: &FakeApi,
        cfg: &crate::JiraConfig,
        cursor: Option<String>,
        sink: &mut (dyn Sink + Send),
    ) -> Result<String, SourceError> {
        SyncRun {
            api,
            cfg,
            source_id: "jira",
            base_url: "https://jira.example",
        }
        .run(cursor, sink)
        .await
    }

    /// One run, returning the keys it emitted and the cursor it handed back.
    async fn keys_of(
        api: &FakeApi,
        cfg: &crate::JiraConfig,
        cursor: Option<String>,
    ) -> (Vec<String>, String) {
        let mut sink = VecSink(Vec::new());
        let next = run(api, cfg, cursor, &mut sink)
            .await
            .expect("run succeeds");
        (sink.0.iter().map(|i| i.entity.key.clone()).collect(), next)
    }

    fn five_issues() -> Vec<serde_json::Value> {
        vec![
            issue("PAY-219", "2026-08-18T10:00:00.000+0000", 0, 0),
            issue("OPS-77", "2026-08-19T10:00:00.000+0000", 0, 0),
            issue("PAY-236", "2026-08-20T10:00:00.000+0000", 0, 0),
            issue("PAY-228", "2026-08-21T16:05:00.000+0000", 0, 0),
            issue("PAY-231", "2026-08-22T11:48:00.000+0000", 2, 2),
        ]
    }

    #[tokio::test]
    async fn pages_with_start_at_until_the_total_is_reached() {
        let api = FakeApi {
            issues: five_issues(),
            ..FakeApi::default()
        };
        let mut sink = VecSink(Vec::new());
        run(
            &api,
            &cfg(serde_json::json!({ "page_size": 2 })),
            None,
            &mut sink,
        )
        .await
        .unwrap();
        assert_eq!(sink.0.len(), 5);
        assert_eq!(api.searches(), vec!["search@0", "search@2", "search@4"]);
        // Same corpus, one page: identical set of keys.
        let mut one_page = VecSink(Vec::new());
        run(&api, &cfg(serde_json::json!({})), None, &mut one_page)
            .await
            .unwrap();
        let keys = |s: &VecSink| {
            let mut k: Vec<String> = s.0.iter().map(|i| i.entity.key.clone()).collect();
            k.sort();
            k
        };
        assert_eq!(keys(&sink), keys(&one_page));
    }

    /// `descriptor_template().full_sync_exhaustive` is `true`, and the engine
    /// tombstones every row a full sync did not return. A run that stopped
    /// after the first page would therefore not merely sync less -- it would
    /// delete the rest of the corpus from the mirror. So the claim's paging
    /// half is pinned separately from the "which requests went out" assertion
    /// above: *every* key arrives, whatever the page size.
    #[tokio::test]
    async fn a_full_sync_walks_the_whole_total_not_just_the_first_page() {
        let api = FakeApi {
            issues: five_issues(),
            ..FakeApi::default()
        };
        for page_size in [1, 2, 3, 4, 5, 100] {
            let (mut keys, _) = keys_of(
                &api,
                &cfg(serde_json::json!({ "page_size": page_size })),
                None,
            )
            .await;
            keys.sort();
            assert_eq!(
                keys,
                vec!["OPS-77", "PAY-219", "PAY-228", "PAY-231", "PAY-236"],
                "page_size {page_size} did not deliver the whole corpus"
            );
        }
    }

    /// `total` is a live number and can shrink under the run. An empty page
    /// must end the loop, or the adapter spins forever against a server that
    /// keeps saying "there are more".
    #[tokio::test]
    async fn an_empty_page_ends_the_run() {
        let api = FakeApi {
            issues: Vec::new(),
            ..FakeApi::default()
        };
        let mut sink = VecSink(Vec::new());
        let cursor = run(&api, &cfg(serde_json::json!({})), None, &mut sink)
            .await
            .unwrap();
        assert!(sink.0.is_empty());
        assert_eq!(api.count("search@"), 1);
        // A full sync that found nothing still hands back a readable cursor.
        let parsed = crate::cursor::JiraCursor::parse(&cursor).unwrap();
        assert_eq!(parsed.updated_to, None);
    }

    /// `/search` truncates the comment container; the container says so, and
    /// the dedicated endpoint has the rest.
    #[tokio::test]
    async fn completes_a_truncated_comment_container() {
        let api = FakeApi {
            issues: vec![issue("PAY-231", "2026-08-22T11:48:00.000+0000", 1, 3)],
            comments: HashMap::from([(
                "PAY-231".to_owned(),
                (0..3)
                    .map(|i| {
                        serde_json::json!({ "id": format!("c{i}"), "body": format!("full {i}") })
                    })
                    .collect(),
            )]),
            ..FakeApi::default()
        };
        let mut sink = VecSink(Vec::new());
        run(&api, &cfg(serde_json::json!({})), None, &mut sink)
            .await
            .unwrap();
        let item = &sink.0[0];
        assert_eq!(
            item.payload["fields"]["comment"]["comments"]
                .as_array()
                .unwrap()
                .len(),
            3
        );
        assert!(item.body_text.contains("full 2"), "{}", item.body_text);
        assert_eq!(api.count("comments:PAY-231"), 1);
    }

    /// The comment endpoint pages like `/search` does, and a discussion longer
    /// than one page is exactly the case the completion exists for. Stopping
    /// after the first page would put a *shorter* comment list in `payload`
    /// than the container's own `total` claims -- and the next run would
    /// re-fetch the issue forever, because the container would still look
    /// truncated.
    #[tokio::test]
    async fn completion_pages_the_comment_endpoint_too() {
        let api = FakeApi {
            issues: vec![issue("PAY-231", "2026-08-22T11:48:00.000+0000", 0, 5)],
            comments: HashMap::from([(
                "PAY-231".to_owned(),
                (0..5)
                    .map(|i| {
                        serde_json::json!({ "id": format!("c{i}"), "body": format!("full {i}") })
                    })
                    .collect(),
            )]),
            ..FakeApi::default()
        };
        let mut sink = VecSink(Vec::new());
        run(
            &api,
            &cfg(serde_json::json!({ "page_size": 2 })),
            None,
            &mut sink,
        )
        .await
        .unwrap();
        assert_eq!(
            api.calls()
                .iter()
                .filter(|c| c.starts_with("comments:"))
                .collect::<Vec<_>>(),
            vec![
                "comments:PAY-231@0",
                "comments:PAY-231@2",
                "comments:PAY-231@4"
            ]
        );
        let item = &sink.0[0];
        assert_eq!(
            item.payload["fields"]["comment"]["comments"]
                .as_array()
                .unwrap()
                .len(),
            5
        );
        assert!(item.body_text.contains("full 4"), "{}", item.body_text);
    }

    /// An issue whose comments all fit costs no extra request. This is the
    /// difference between one call per run and one call per issue.
    #[tokio::test]
    async fn leaves_a_complete_container_alone() {
        let api = FakeApi {
            issues: vec![issue("PAY-231", "2026-08-22T11:48:00.000+0000", 2, 2)],
            ..FakeApi::default()
        };
        let mut sink = VecSink(Vec::new());
        run(&api, &cfg(serde_json::json!({})), None, &mut sink)
            .await
            .unwrap();
        assert_eq!(api.count("comments:"), 0);
        assert!(sink.0[0].body_text.contains("inline 1"));
    }

    /// A server that ignored `fields=comment` returns no container at all --
    /// the adapter must not read that as "no comments".
    #[tokio::test]
    async fn fetches_comments_when_the_container_is_absent() {
        let api = FakeApi {
            issues: vec![serde_json::json!({
                "key": "PAY-231",
                "fields": { "summary": "s", "updated": "2026-08-22T11:48:00.000+0000" }
            })],
            comments: HashMap::from([(
                "PAY-231".to_owned(),
                vec![serde_json::json!({ "id": "c0", "body": "fetched" })],
            )]),
            worklogs: HashMap::from([(
                "PAY-231".to_owned(),
                vec![serde_json::json!({ "id": "w0", "timeSpentSeconds": 900 })],
            )]),
            ..FakeApi::default()
        };
        let mut sink = VecSink(Vec::new());
        run(&api, &cfg(serde_json::json!({})), None, &mut sink)
            .await
            .unwrap();
        assert_eq!(api.count("comments:PAY-231"), 1);
        assert_eq!(api.count("worklogs:PAY-231"), 1);
        assert!(sink.0[0].body_text.contains("fetched"));
        assert_eq!(
            sink.0[0].payload["fields"]["worklog"]["worklogs"][0]["id"],
            "w0"
        );
    }

    /// Battery clause 2, and the reason the cursor carries a `seen` set: the
    /// two-minute overlap re-fetches, and the run must still emit nothing and
    /// return the *same bytes*.
    ///
    /// Necessary and **not** sufficient on its own -- a full sync skips
    /// nothing, so this sequence is stable even with the `seen_in_window`
    /// defect. See
    /// [`one_new_issue_then_an_idle_poll_settles`](fn@one_new_issue_then_an_idle_poll_settles).
    #[tokio::test]
    async fn an_idle_incremental_emits_nothing_and_returns_the_same_cursor() {
        let api = FakeApi {
            issues: five_issues(),
            ..FakeApi::default()
        };
        let config = cfg(serde_json::json!({}));
        let mut first = VecSink(Vec::new());
        let cursor = run(&api, &config, None, &mut first).await.unwrap();
        assert_eq!(first.0.len(), 5);

        let mut second = VecSink(Vec::new());
        let again = run(&api, &config, Some(cursor.clone()), &mut second)
            .await
            .unwrap();
        assert!(second.0.is_empty(), "{:?}", second.0);
        assert_eq!(
            again, cursor,
            "an idle run must hand back the cursor it was given"
        );
    }

    /// The sequence that actually certifies cursor discipline: **one new issue
    /// and then an idle poll**, twice over.
    ///
    /// Full-sync-then-idle passes even when the run hands
    /// [`JiraCursor::advanced`](crate::cursor::JiraCursor::advanced) only what
    /// it *emitted*, because a full sync skips nothing and `seen` therefore
    /// comes out the same either way. This one does not: run 2 skips PAY-231
    /// as already delivered, and if that pair drops out of `seen`, run 3 --
    /// whose window still reaches back over it -- emits it again. The watermark
    /// holds perfectly the whole time, so nothing watching the cursor's
    /// timestamp can see the failure.
    ///
    /// The edits are one minute apart on purpose: the overlap is two minutes,
    /// so each run's window still contains the pair the previous run skipped.
    #[tokio::test]
    async fn one_new_issue_then_an_idle_poll_settles() {
        let mut api = FakeApi {
            issues: five_issues(),
            ..FakeApi::default()
        };
        let config = cfg(serde_json::json!({}));

        let (full, c1) = keys_of(&api, &config, None).await;
        assert_eq!(full.len(), 5);

        // One issue edited: it is the only thing that comes back, and PAY-231
        // -- still inside the two-minute window -- is recognised, not re-sent.
        api.edit("PAY-236", "2026-08-22T11:49:00.000+0000");
        let (second, c2) = keys_of(&api, &config, Some(c1)).await;
        assert_eq!(second, vec!["PAY-236".to_owned()]);

        // A second edit, one minute on. PAY-231 *and* PAY-236 are both still in
        // the window; both must be recognised. This is the assertion that goes
        // red when `advanced` is fed only the emitted pairs.
        api.edit("OPS-77", "2026-08-22T11:50:00.000+0000");
        let (third, c3) = keys_of(&api, &config, Some(c2)).await;
        assert_eq!(
            third,
            vec!["OPS-77".to_owned()],
            "a pair skipped as already-delivered must stay in `seen`, or it comes back \
             unrecognised on the next run"
        );

        // And now nothing at all changed: the run is idle and says so.
        let (idle, c4) = keys_of(&api, &config, Some(c3.clone())).await;
        assert!(idle.is_empty(), "{idle:?}");
        assert_eq!(
            c4, c3,
            "an idle poll must hand back the cursor it was given"
        );
    }

    /// The other half: something really changed, and exactly that comes back.
    #[tokio::test]
    async fn a_changed_issue_is_the_only_thing_the_next_run_emits() {
        let mut api = FakeApi {
            issues: five_issues(),
            ..FakeApi::default()
        };
        let config = cfg(serde_json::json!({}));
        let mut first = VecSink(Vec::new());
        let cursor = run(&api, &config, None, &mut first).await.unwrap();

        // PAY-228 edited after the watermark.
        api.edit("PAY-228", "2026-08-22T12:30:00.000+0000");
        let mut second = VecSink(Vec::new());
        let advanced = run(&api, &config, Some(cursor.clone()), &mut second)
            .await
            .unwrap();
        let keys: Vec<&str> = second.0.iter().map(|i| i.entity.key.as_str()).collect();
        assert_eq!(keys, vec!["PAY-228"]);
        assert_ne!(advanced, cursor);

        // …and the watermark is the newest `updated` delivered, not `now()`.
        let parsed = crate::cursor::JiraCursor::parse(&advanced).unwrap();
        assert_eq!(
            parsed.updated_to,
            crate::time::parse_jira_time("2026-08-22T12:30:00.000+0000")
        );
    }

    /// The watermark is `max(previous, newest emitted)`, never just "newest
    /// emitted".
    ///
    /// An item can enter the window carrying an `updated` *older* than the
    /// current watermark -- an issue whose project was added to the scope, or
    /// one the credential could not see until a permission changed. Taking the
    /// newest emitted alone drags the position backwards onto it, and the next
    /// query's window widens to everything since. That is recoverable exactly
    /// while `seen` can hold the whole widened window; past
    /// [`SEEN_CAP`](crate::cursor::SEEN_CAP) it is re-delivery on every poll,
    /// forever.
    #[tokio::test]
    async fn the_watermark_never_moves_backwards() {
        let mut api = FakeApi {
            issues: five_issues(),
            ..FakeApi::default()
        };
        let config = cfg(serde_json::json!({}));
        let (_, cursor) = keys_of(&api, &config, None).await;
        let before = crate::cursor::JiraCursor::parse(&cursor)
            .unwrap()
            .updated_to;
        assert_eq!(
            before,
            crate::time::parse_jira_time("2026-08-22T11:48:00.000+0000")
        );

        // Newly visible, and older than the watermark -- but inside the window,
        // so the query returns it and the run has never seen the pair.
        api.issues
            .push(issue("PAY-999", "2026-08-22T11:47:00.000+0000", 0, 0));
        let (keys, advanced) = keys_of(&api, &config, Some(cursor)).await;
        assert_eq!(keys, vec!["PAY-999".to_owned()]);
        assert_eq!(
            crate::cursor::JiraCursor::parse(&advanced)
                .unwrap()
                .updated_to,
            before,
            "emitting an item older than the watermark must not move the watermark back to it"
        );
    }

    /// Interfaces §4.1: "an unrecognised version means full sync".
    #[tokio::test]
    async fn an_unreadable_cursor_falls_back_to_a_full_sync() {
        let api = FakeApi {
            issues: five_issues(),
            ..FakeApi::default()
        };
        let mut sink = VecSink(Vec::new());
        run(
            &api,
            &cfg(serde_json::json!({})),
            Some("tidewater-v1".to_owned()),
            &mut sink,
        )
        .await
        .unwrap();
        assert_eq!(sink.0.len(), 5);
    }

    /// Battery clause 6: a sink error aborts the run rather than being
    /// swallowed, and it keeps its own variant.
    #[tokio::test]
    async fn a_sink_failure_aborts_the_run() {
        let api = FakeApi {
            issues: five_issues(),
            ..FakeApi::default()
        };
        let got = run(&api, &cfg(serde_json::json!({})), None, &mut FailingSink)
            .await
            .unwrap_err();
        assert!(matches!(got, SourceError::Sink(_)), "{got:?}");
    }

    /// Battery clause 4: the classification raised mid-sync is the one the user
    /// actually sees, because `test_connection` ran once when the source was
    /// added.
    #[tokio::test]
    async fn an_auth_failure_mid_sync_keeps_its_classification() {
        let api = FakeApi {
            unauthorized: true,
            issues: five_issues(),
            ..FakeApi::default()
        };
        let mut sink = VecSink(Vec::new());
        let got = run(&api, &cfg(serde_json::json!({})), None, &mut sink)
            .await
            .unwrap_err();
        assert!(matches!(got, SourceError::Unauthorized), "{got:?}");
    }
}

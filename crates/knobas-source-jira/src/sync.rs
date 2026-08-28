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
/// **Why these eighteen (issue #32).** This list used to stop after twelve,
/// and the reason it gave was the wrong way round: `knobas-mockd` validated
/// `fields=` against a closed set, so anything wider was a 400 plus a recorded
/// `UnknownField` violation, and the list "stopped where the certification
/// stopped". That makes a mock's coverage decide what production fetches. The
/// mock is there to certify the query, not to bound it, so mockd's set was
/// widened first (its deviation 5) and this follows.
///
/// The six that came back are `labels`, `parent`, `resolution`, `issuelinks`,
/// `timeoriginalestimate` and `timespent`. The reader is **`payload`**: spec
/// §3a keeps the raw record precisely so a later, smarter mapping can
/// re-project existing data without re-syncing, and the §3a generic detail
/// view renders out of it. So the TeamCity rule -- ask for nothing no reader
/// looks at -- lands differently here: a name left out of this list is not a
/// slightly larger response saved, it is data the mirror never holds, and
/// recovering it costs a full re-fetch of every issue in the source. `parent`
/// in particular is where epic membership lives, which is what the Contexts
/// work reads.
///
/// `components`, `fixVersions` and `timeestimate` are still out, and now for
/// their own reason rather than for mockd's: the Tidewater fixture records no
/// source for any of them, so mockd would have to invent one, and an invented
/// value reaches `payload` and the search index as if the dataset had said it
/// (the #28 ruling). They are one fixture field away, not one constant away.
///
/// Certified at the wire, not here: `tests/mockd.rs`'s
/// `a_synced_issue_carries_epic_membership_links_and_resolution` runs this
/// exact query against the mock and ends in `assert_no_violations()`, so a
/// name this instance would refuse fails there rather than silently thinning
/// the payload in production.
const BASE_FIELDS: &str = "summary,description,issuetype,status,priority,assignee,reporter,\
project,created,updated,labels,parent,resolution,issuelinks,timeoriginalestimate,timespent,\
comment,worklog";

/// A stop so a server that keeps reporting "more" cannot spin a run forever.
/// At the default page size this is half a million issues.
///
/// Reaching it is a **failure**, never a quiet end to the walk. The one kind
/// this adapter emits claims `full_sync_exhaustive: true`, which is the
/// engine's licence to tombstone every row a `cursor: None` run did not
/// return -- so a truncated walk reported as `Ok` authorises deleting whatever
/// fell off the end. The query is `ORDER BY updated ASC`, so what falls off
/// the end is the *most recently updated* work: the tickets someone is looking
/// at today. And it is not a first-sync-only hazard, because any later
/// `cursor: None` run re-enters the same path against a populated mirror.
///
/// An incremental run gets the same refusal, and unconditionally. Two reasons,
/// the second of which was found by mutating the first away:
///
/// 1. The stop exists for a server that mispages -- one that keeps reporting
///    "more" without delivering it -- and "progress" measured against such a
///    server is progress against a number it is getting wrong, so advancing a
///    watermark on that basis skips whatever sat in the gap.
/// 2. **It is the only thing that terminates the loop.** The other exit is
///    `start_at >= total`, and `total` is the very number the server is
///    inflating. Restricting the refusal to full syncs -- which looks like the
///    careful, narrower fix, since no sweep follows an incremental -- makes an
///    incremental against such a server run forever. Verified: the test below
///    does not terminate in 180 s with `&& cursor.is_none()` added here.
///
/// A source genuinely this large is narrowed with `projects` or `jql_filter`,
/// which is what the message says.
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
            if start_at >= total {
                break;
            }
            if pages >= MAX_PAGES {
                // Not a `break`: see the note on MAX_PAGES. Leaving the loop
                // here and returning `Ok` would hand the engine a partial walk
                // wearing a completed one's clothes.
                return Err(SourceError::protocol(format!(
                    "the Jira search did not reach its own reported total of {total} issues \
                     within {MAX_PAGES} pages of {}; refusing to report a partial walk as a \
                     completed sync. Narrow this source with `projects` or `jql_filter`.",
                    self.cfg.page_size
                )));
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

    struct FakeApi {
        issues: Vec<serde_json::Value>,
        comments: HashMap<String, Vec<serde_json::Value>>,
        worklogs: HashMap<String, Vec<serde_json::Value>>,
        /// The zone `serverInfo` reports and JQL literals are resolved in.
        /// Defaults to `+02:00` like `knobas-mockd`, and deliberately not to
        /// UTC: a fake on UTC cannot tell a watermark rendered in the server's
        /// zone from one rendered in UTC, which is the trap this adapter's
        /// whole `serverInfo` call exists to avoid.
        offset_secs: i32,
        /// Extra `total` the fake claims beyond what it will ever return -- a
        /// server whose index shrank under the run.
        phantom_total: u32,
        /// A server that never stops promising more: every page carries one
        /// fresh issue and a `total` the walk can never reach.
        endless: bool,
        unauthorized: bool,
        calls: Mutex<Vec<String>>,
    }

    impl Default for FakeApi {
        fn default() -> Self {
            Self {
                issues: Vec::new(),
                comments: HashMap::new(),
                worklogs: HashMap::new(),
                offset_secs: 2 * 3600,
                phantom_total: 0,
                endless: false,
                unauthorized: false,
                calls: Mutex::new(Vec::new()),
            }
        }
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
    ///
    /// The literal is resolved **in the server's zone**, never in UTC -- that
    /// is what a real Jira does, what mockd does, and the only reason the
    /// adapter reads `serverInfo.serverTime` at all.
    fn since_of(jql: &str, offset_secs: i32) -> Option<chrono::DateTime<chrono::Utc>> {
        use chrono::TimeZone;
        let rest = jql.split("updated >= \"").nth(1)?;
        let literal = rest.split('"').next()?;
        let naive = chrono::NaiveDateTime::parse_from_str(literal, "%Y-%m-%d %H:%M").ok()?;
        let zone = chrono::FixedOffset::east_opt(offset_secs)?;
        zone.from_local_datetime(&naive)
            .single()
            .map(|t| t.with_timezone(&chrono::Utc))
    }

    #[async_trait::async_trait]
    impl JiraApi for FakeApi {
        async fn server_info(&self) -> Result<ServerInfo, SourceError> {
            self.calls.lock().unwrap().push("serverInfo".to_owned());
            let (sign, mins) = if self.offset_secs < 0 {
                ('-', -self.offset_secs / 60)
            } else {
                ('+', self.offset_secs / 60)
            };
            Ok(serde_json::from_value(serde_json::json!({
                "version": "9.17.0", "deploymentType": "Server",
                "serverTime": format!(
                    "2026-08-22T11:48:00.000{sign}{:02}{:02}", mins / 60, mins % 60
                )
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
                return Err(SourceError::unauthorized());
            }
            self.calls
                .lock()
                .unwrap()
                .push(format!("search@{start_at}"));
            if self.endless {
                return Ok(serde_json::from_value(serde_json::json!({
                    "startAt": start_at, "maxResults": max_results, "total": u32::MAX,
                    "issues": [issue(
                        &format!("PAY-{start_at}"), "2026-08-22T11:48:00.000+0000", 0, 0
                    )]
                }))
                .unwrap());
            }
            let since = since_of(jql, self.offset_secs);
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
                "total": matching.len() as u32 + self.phantom_total, "issues": window
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

    /// The `ticket` kind claims `full_sync_exhaustive: true`, and the engine
    /// tombstones every row of such a kind that a full sync did not return. A
    /// run that stopped after the first page would therefore not merely sync
    /// less -- it would delete the rest of the corpus from the mirror. So the
    /// claim's paging
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

    /// The trap the `serverInfo` call exists for, at the layer that sends the
    /// query.
    ///
    /// JQL date literals carry no zone and are read in the **server's**. A
    /// watermark rendered in UTC and sent to a `+02:00` server therefore names
    /// an instant two hours earlier, and the query returns two extra hours of
    /// issues on every incremental run. That is invisible in the emitted items
    /// as long as everything in the extra band is still in `seen` -- so the
    /// issue here sits *outside* the two-minute overlap the cursor remembers
    /// and inside the two-hour band the mistake would open. Under the correct
    /// rendering the idle run does not see it at all; under a UTC rendering it
    /// comes back unrecognised and is emitted.
    ///
    /// `time.rs` pins the renderer itself. This pins that the run hands it the
    /// zone the server reported rather than a zero.
    #[tokio::test]
    async fn the_watermark_literal_is_read_in_the_servers_zone() {
        let mut issues = five_issues();
        // 11:46Z is the floor an idle run really queries from; 09:46Z is the
        // floor a UTC-rendered literal would open on a +02:00 server.
        issues.push(issue("PAY-500", "2026-08-22T10:30:00.000+0000", 0, 0));
        let api = FakeApi {
            issues,
            ..FakeApi::default()
        };
        let config = cfg(serde_json::json!({}));
        let (full, cursor) = keys_of(&api, &config, None).await;
        assert_eq!(full.len(), 6);
        // Outside the overlap window, so the cursor does not remember it -- the
        // only thing keeping it out of the next run is the query's lower bound.
        let seen: Vec<String> = crate::cursor::JiraCursor::parse(&cursor)
            .unwrap()
            .seen
            .into_iter()
            .map(|s| s.k)
            .collect();
        assert!(!seen.contains(&"PAY-500".to_owned()), "{seen:?}");

        let (idle, again) = keys_of(&api, &config, Some(cursor.clone())).await;
        assert!(
            idle.is_empty(),
            "the query floor reached back further than the server's zone puts it: {idle:?}"
        );
        assert_eq!(again, cursor);
    }

    /// `total` is a live number: an issue can leave the result set while the
    /// run is paging it, and a server can simply be wrong. The loop stops on
    /// the page that arrives empty rather than on the arithmetic, because
    /// `startAt` does not advance over rows that were not returned -- so a run
    /// trusting `total` alone re-asks for the same offset forever.
    #[tokio::test]
    async fn a_page_that_arrives_empty_while_total_still_claims_more_ends_the_run() {
        let api = FakeApi {
            issues: five_issues(),
            phantom_total: 40,
            ..FakeApi::default()
        };
        let (keys, _) = keys_of(&api, &cfg(serde_json::json!({})), None).await;
        assert_eq!(keys.len(), 5);
        // One page with the five issues, one that comes back empty and ends it.
        assert_eq!(api.searches(), vec!["search@0", "search@5"]);
    }

    /// Battery clause 2 says an idle run returns the cursor it was handed
    /// **byte-identically**, and the run keeps that promise by handing back the
    /// input string rather than re-encoding the struct. Re-encoding looks
    /// equivalent and is not: the envelope carries the zone its watermark was
    /// taken in, so the first idle poll after the server crossed a daylight
    /// saving boundary would come back with a different `tz_offset_secs` and a
    /// window widened to 65 minutes -- a changed cursor for a run in which
    /// nothing happened, which is exactly the activity line the clause exists
    /// to suppress.
    #[tokio::test]
    async fn an_idle_run_after_the_server_changed_zone_returns_the_same_bytes() {
        let mut api = FakeApi {
            issues: five_issues(),
            offset_secs: 2 * 3600,
            ..FakeApi::default()
        };
        let config = cfg(serde_json::json!({}));
        let (full, cursor) = keys_of(&api, &config, None).await;
        assert_eq!(full.len(), 5);

        // Winter time: the server is now an hour west of where the cursor was
        // written, so the next run widens its overlap. Nothing changed upstream.
        api.offset_secs = 3600;
        let (idle, again) = keys_of(&api, &config, Some(cursor.clone())).await;
        assert!(idle.is_empty(), "{idle:?}");
        assert_eq!(
            again, cursor,
            "a run that emitted nothing must return the bytes it was handed, zone change included"
        );
    }

    /// A walk that hits the page cap **fails**; it never comes back as a
    /// completed sync.
    ///
    /// This is the one that matters most, and it is the one mockd structurally
    /// cannot see: the fixture is seven issues, so the cap is a real-server-only
    /// failure mode. The `ticket` kind claims `full_sync_exhaustive: true`,
    /// which is the engine's licence to tombstone every row of that kind a
    /// `cursor: None` run did not return. An `Ok` here is therefore not "we
    /// synced a bit less" -- it is authorisation to delete everything past the
    /// cap. And because
    /// the query is `ORDER BY updated ASC`, what falls off the end is the
    /// newest work, so the rows offered up for deletion are exactly the ones
    /// someone is looking at today.
    ///
    /// The assertion is the *outcome*, not the emission: items do reach the
    /// sink before the cap, and that is fine. What must not happen is the run
    /// telling the engine it saw the whole corpus.
    #[tokio::test]
    async fn a_walk_that_hits_the_page_cap_fails_instead_of_reporting_a_completed_sync() {
        let api = FakeApi {
            endless: true,
            ..FakeApi::default()
        };
        let mut sink = VecSink(Vec::new());
        let got = run(&api, &cfg(serde_json::json!({})), None, &mut sink)
            .await
            .unwrap_err();
        assert!(
            matches!(&got, SourceError::Protocol { message: m, .. } if m.contains("partial walk")),
            "{got:?}"
        );
        // The cap fired where it is meant to, rather than the run ending for
        // some other reason that happens to look the same.
        assert_eq!(api.count("search@"), MAX_PAGES as usize);
        assert!(
            !sink.0.is_empty(),
            "the items before the cap are still delivered; it is the verdict that changes"
        );
    }

    /// An incremental run gets the same refusal.
    ///
    /// Tempting to let it through: no sweep follows an incremental, the items
    /// were delivered, and the watermark advanced honestly under
    /// `ORDER BY updated ASC`, so the next run would resume where this one
    /// stopped. That narrower fix is worse than it looks, for a reason the
    /// mutation check turned up rather than the argument: with
    /// `&& cursor.is_none()` on the cap, **this test does not terminate**. The
    /// loop's only other exit is `start_at >= total`, and `total` is the number
    /// the server is inflating -- so an incremental against a mispaging server
    /// has no stop at all. (Measured: no completion in 180 s, against 1.7 s for
    /// the whole lib suite.)
    ///
    /// The refusal is therefore unconditional, and this pins it rather than
    /// leaving it to a comment.
    #[tokio::test]
    async fn an_incremental_run_that_hits_the_page_cap_fails_too() {
        let settled = FakeApi {
            issues: five_issues(),
            ..FakeApi::default()
        };
        let (_, cursor) = keys_of(&settled, &cfg(serde_json::json!({})), None).await;

        let api = FakeApi {
            endless: true,
            ..FakeApi::default()
        };
        let mut sink = VecSink(Vec::new());
        let got = run(&api, &cfg(serde_json::json!({})), Some(cursor), &mut sink)
            .await
            .unwrap_err();
        assert!(
            matches!(&got, SourceError::Protocol { message: m, .. } if m.contains("partial walk")),
            "{got:?}"
        );
    }

    /// The widening is for the run *after* a zone change and no other. A run
    /// whose zone did not move reaches back exactly the two-minute overlap.
    ///
    /// `seen` is filtered to that same two minutes, so anything the window
    /// reaches beyond it comes back unrecognised and is emitted -- on every
    /// poll, forever. A permanently widened window is therefore not a harmless
    /// over-fetch: it is battery clause 2 failing in steady state, for any
    /// source with an edit between three and sixty-five minutes old.
    #[tokio::test]
    async fn an_idle_poll_does_not_reach_back_further_than_the_overlap() {
        let mut issues = five_issues();
        // Inside the daylight-saving widening, outside the overlap -- so the
        // cursor does not remember it and only an over-wide floor returns it.
        issues.push(issue("PAY-502", "2026-08-22T11:00:00.000+0000", 0, 0));
        let api = FakeApi {
            issues,
            ..FakeApi::default()
        };
        let config = cfg(serde_json::json!({}));
        let (full, cursor) = keys_of(&api, &config, None).await;
        assert_eq!(full.len(), 6);
        let (idle, again) = keys_of(&api, &config, Some(cursor.clone())).await;
        assert!(
            idle.is_empty(),
            "an idle poll reached past the two-minute overlap: {idle:?}"
        );
        assert_eq!(again, cursor);
    }

    /// The one run after the server's UTC offset changed reaches back an hour
    /// further than the overlap, and it is the **current** offset that decides
    /// that -- comparing the cursor's zone against itself would make the
    /// widening unreachable.
    ///
    /// `cursor.rs` pins that [`JiraCursor::since`] widens; this pins that the
    /// run gives it something to compare against. The issue in the seam is
    /// outside the two-minute overlap and therefore not in `seen`, so the only
    /// thing that can bring it back is the wider floor -- and the watermark
    /// must not follow it backwards.
    #[tokio::test]
    async fn the_run_after_a_zone_change_widens_its_window_once() {
        let mut issues = five_issues();
        // 48 minutes before the watermark: inside the 65-minute widening,
        // outside the 2-minute overlap.
        issues.push(issue("PAY-501", "2026-08-22T11:00:00.000+0000", 0, 0));
        let mut api = FakeApi {
            issues,
            offset_secs: 2 * 3600,
            ..FakeApi::default()
        };
        let config = cfg(serde_json::json!({}));
        let (_, cursor) = keys_of(&api, &config, None).await;

        // Same corpus, unchanged; only the server's zone moved.
        api.offset_secs = 3600;
        let (widened, advanced) = keys_of(&api, &config, Some(cursor)).await;
        assert_eq!(
            widened,
            vec!["PAY-501".to_owned()],
            "the run after a zone change must re-read the daylight-saving seam"
        );
        assert_eq!(
            crate::cursor::JiraCursor::parse(&advanced)
                .unwrap()
                .updated_to,
            crate::time::parse_jira_time("2026-08-22T11:48:00.000+0000"),
            "re-reading the seam must not drag the watermark back into it"
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

    /// `comment` and `worklog` are not in Jira's `*navigable` set, so a
    /// `/search` that does not name them returns no container -- which this
    /// adapter reads as "truncated" and completes with a request per issue.
    /// Asking for them up front is the difference between one request per page
    /// and one per issue, and nothing else in the suite would notice their
    /// removal: the completion path would quietly pick up the slack.
    #[test]
    fn the_field_list_asks_for_the_two_containers_that_would_otherwise_cost_a_request_each() {
        let config = cfg(serde_json::json!({}));
        let api = FakeApi::default();
        let fields = SyncRun {
            api: &api,
            cfg: &config,
            source_id: "jira",
            base_url: "https://jira.example",
        }
        .fields();
        let names: Vec<&str> = fields.split(',').collect();
        assert!(names.contains(&"comment"), "{fields}");
        assert!(names.contains(&"worklog"), "{fields}");
        // The fields the mapping itself reads (interfaces §4.1 normalization).
        for required in ["summary", "description", "updated", "assignee"] {
            assert!(
                names.contains(&required),
                "{required} missing from {fields}"
            );
        }
    }

    /// The other direction, and the half nothing else can see: a name that is
    /// **out** of the list, and the reason it is out.
    ///
    /// The positive half of #32's widening is certified at the wire --
    /// `tests/mockd.rs` runs this exact query against the mock and ends in
    /// `assert_no_violations()`, which is the only thing that can say the
    /// server would accept it. What no wire test can say is why a name is
    /// absent, and that is what went wrong the first time: the list stopped at
    /// twelve because mockd's closed set stopped there, so a mock's coverage
    /// was silently deciding what production fetched.
    ///
    /// Each row below is paired with its own reason, on the #33 precedent,
    /// because the reasons differ and a shared message would be false about
    /// at least one of them. **The fix for any of these is a fixture field,
    /// not this list**: adding the name alone makes mockd invent the value,
    /// and an invented value reaches `payload` and the search index as if the
    /// dataset had said it (the #28 ruling).
    #[test]
    fn the_field_list_asks_for_nothing_the_fixture_cannot_answer() {
        let config = cfg(serde_json::json!({}));
        let api = FakeApi::default();
        let run = SyncRun {
            api: &api,
            cfg: &config,
            source_id: "jira",
            base_url: "https://jira.example",
        };
        let fields = run.fields();
        let names: Vec<&str> = fields.split(',').collect();

        for (missing, why) in [
            (
                "components",
                "no ticket in fixtures/tidewater/work.json names a component, so mockd would \
                 have to invent one",
            ),
            (
                "fixVersions",
                "the fixture records no releases at all, so mockd would have to invent one",
            ),
            (
                "timeestimate",
                "the fixture records an estimate and worklogs but no *remaining* estimate, and \
                 deriving one would be arithmetic mockd made up",
            ),
        ] {
            assert!(
                !names.contains(&missing),
                "{missing} is in {fields}: {why}. mockd answers it with 400 + an UnknownField \
                 violation, so tests/mockd.rs fails too -- but fix it by giving the fixture the \
                 field, not by widening mockd's set around an invented value."
            );
        }

        // ...and the widened six, each named with what its absence costs.
        // Absence is not a smaller response here: `payload` is the reader
        // (spec §3a), so a name left out is data the mirror never holds and
        // a full re-fetch of the source is the only way back.
        for (needed, cost) in [
            ("parent", "epic membership, which the Contexts work reads"),
            ("issuelinks", "the blocks/blocked-by graph"),
            ("resolution", "whether a done issue was fixed or dropped"),
            ("labels", "every label-driven view and filter"),
            ("timeoriginalestimate", "the estimate side of time tracking"),
            ("timespent", "the logged side of time tracking"),
        ] {
            assert!(
                names.contains(&needed),
                "{needed} is missing from {fields}, so the mirror loses {cost} for every issue \
                 -- and gets it back only by re-syncing the whole source (#32)"
            );
        }

        // No whitespace and no duplicates: this goes into a query string
        // verbatim, and a repeated name is a widening that was applied twice.
        assert!(!fields.contains(' '), "{fields}");
        let mut sorted = names.clone();
        sorted.sort_unstable();
        let before = sorted.len();
        sorted.dedup();
        assert_eq!(before, sorted.len(), "a name appears twice in {fields}");
    }

    /// The configured Epic Link custom field rides along in `fields=`, which is
    /// the whole of what the option does -- it preserves epic membership in
    /// `payload` for a classic DC project (gap 3). Dropped, the option is a
    /// form input that silently does nothing.
    #[test]
    fn the_epic_link_field_is_appended_to_the_field_list() {
        let config = cfg(serde_json::json!({ "epic_link_field": "customfield_10008" }));
        let api = FakeApi::default();
        let fields = SyncRun {
            api: &api,
            cfg: &config,
            source_id: "jira",
            base_url: "https://jira.example",
        }
        .fields();
        assert!(
            fields.split(',').any(|n| n == "customfield_10008"),
            "{fields}"
        );
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
        assert!(matches!(got, SourceError::Unauthorized { .. }), "{got:?}");
    }
}

//! A **refresh** (`CONTEXT.md`, ADR-0031, #2097): one issue read by a search
//! on its key, so the item is the one a run's walk would emit for it -- the
//! same `/search` record, the same `expand=changelog`, the same completed
//! comment and worklog containers, the same `updated` to the second.
//!
//! # Why `/issue/{key}` is asked, and what it is not asked for
//!
//! `/search` reads Jira's Lucene index, and the index lags the database by
//! seconds after a write (`atlassian_live`'s `Env::indexed`, #325). A refresh
//! is what follows a landed write, so a search fired at once can answer the
//! issue as it was before the write, or not at all for a ticket just filed.
//! So the database is asked first, for `updated` alone
//! (`GET /rest/api/2/issue/{key}?fields=updated`), and the search is asked
//! again until its record is at least as new, to the second. The item is
//! never built from that GET: it keeps milliseconds where `/search` rounds
//! to the second, and the mirror would hold two precisions for one issue.
//!
//! The same GET is what says *gone*: a 404 is an issue deleted, or one this
//! account may no longer see, and an issue moved to another project answers
//! under its new key. Both are gone under the id asked, as a full sync would
//! leave them. An issue the index holds fresh outside this source's scope
//! (its filter, its projects) is gone too: a run would not read it.
//!
//! An index that does not catch up within [`PATIENCE`] is a failed read
//! ([`SourceError::Unreachable`]), never *gone*: the issue is there.

use std::time::Duration;

use chrono::{DateTime, SubsecRound, Utc};
use knobas_source::{RefreshAnswer, SourceError};

use crate::jql::{build_key_jql, is_scoped};
use crate::map;
use crate::model::RawIssue;
use crate::sync::SyncRun;
use crate::time::parse_jira_time;

/// How long a refresh waits for the search index: how often it asks, and how
/// many times. Ten seconds, a quarter of a second apart, the step
/// `atlassian_live`'s wait for the same index takes.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Patience {
    pub every: Duration,
    pub tries: u32,
}

/// The production wait (see [`Patience`]).
pub(crate) const PATIENCE: Patience = Patience {
    every: Duration::from_millis(250),
    tries: 40,
};

impl SyncRun<'_> {
    /// Read the issue `key` as a run would emit it (module docs).
    ///
    /// # Errors
    ///
    /// What a request answers, classified as a sync's are, and
    /// [`SourceError::Unreachable`] for an index that never caught up.
    pub(crate) async fn refresh(
        &self,
        key: &str,
        patience: Patience,
    ) -> Result<RefreshAnswer, SourceError> {
        let Some(stamp) = self.api.issue_stamp(key).await? else {
            return Ok(RefreshAnswer::Gone);
        };
        if stamp.key != key {
            return Ok(RefreshAnswer::Gone);
        }
        // To the second: what `/search` keeps of it.
        let held = stamp
            .fields
            .updated
            .as_deref()
            .and_then(parse_jira_time)
            .map(|at| at.trunc_subsecs(0));
        let fields = self.fields();
        for attempt in 0..patience.tries {
            if attempt > 0 {
                tokio::time::sleep(patience.every).await;
            }
            if let Some(raw) = self.indexed(key, true, held, &fields).await? {
                return self.item(raw).await;
            }
            // Not in the scope's answer: outside the scope, or not indexed
            // yet. Only the index can say which. Once it holds the issue
            // fresh, the scope is asked once more, since the index may have
            // caught up between the two searches.
            if is_scoped(self.cfg) && self.indexed(key, false, held, &fields).await?.is_some() {
                return match self.indexed(key, true, held, &fields).await? {
                    Some(raw) => self.item(raw).await,
                    None => Ok(RefreshAnswer::Gone),
                };
            }
        }
        Err(SourceError::Unreachable(format!(
            "Jira's search index has not caught up with {key} after {:?}; the next run reads it",
            patience.every * patience.tries
        )))
    }

    /// The record completed and mapped as the walk completes and maps it.
    async fn item(&self, mut raw: RawIssue) -> Result<RefreshAnswer, SourceError> {
        self.complete(&mut raw).await?;
        Ok(RefreshAnswer::Item(map::to_sync_item(
            self.source_id,
            self.base_url,
            self.cfg.epic_link_field.as_deref(),
            self.cfg.sprint_field.as_deref(),
            &raw,
        )))
    }

    /// The search's record of `key`, when it is at least as new as `held`.
    async fn indexed(
        &self,
        key: &str,
        scoped: bool,
        held: Option<DateTime<Utc>>,
        fields: &str,
    ) -> Result<Option<RawIssue>, SourceError> {
        let page = self
            .api
            .search(&build_key_jql(self.cfg, key, scoped), 0, 1, fields)
            .await?;
        Ok(page.issues.into_iter().find(|raw| {
            let indexed = raw
                .issue
                .fields
                .updated
                .as_deref()
                .and_then(parse_jira_time);
            raw.issue.key == key
                && match (held, indexed) {
                    (Some(held), Some(indexed)) => indexed >= held,
                    _ => true,
                }
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::JiraApi;
    use crate::model::{CommentPage, Issue, Myself, SearchPage, ServerInfo, WorklogPage};
    use serde_json::{Value, json};
    use std::collections::HashMap;
    use std::sync::Mutex;

    const QUICK: Patience = Patience {
        every: Duration::from_millis(1),
        tries: 5,
    };

    /// An issue as `/search` returns it: `updated` to the second, one comment
    /// inline of the two it has.
    fn record(key: &str, summary: &str, updated: &str) -> Value {
        json!({
            "id": "10001",
            "key": key,
            "fields": {
                "summary": summary,
                "updated": updated,
                "project": { "key": key.split('-').next().unwrap() },
                "comment": { "startAt": 0, "maxResults": 1, "total": 2,
                    "comments": [{ "id": "1", "body": "first" }] },
                "worklog": { "startAt": 0, "maxResults": 0, "total": 0, "worklogs": [] }
            }
        })
    }

    /// A Jira whose database and search index can disagree. `index` is what
    /// `/search` answers until `catch_up_after` searches have been made, the
    /// database's records from then on.
    #[derive(Default)]
    struct Jira {
        db: HashMap<String, Value>,
        /// Old key → the key Jira answers it under now.
        moved: HashMap<String, String>,
        index: Mutex<HashMap<String, Value>>,
        catch_up_after: Option<usize>,
        calls: Mutex<Vec<String>>,
    }

    impl Jira {
        fn holding(records: &[Value]) -> Self {
            let db: HashMap<String, Value> = records
                .iter()
                .map(|r| (r["key"].as_str().unwrap().to_owned(), r.clone()))
                .collect();
            Self {
                index: Mutex::new(db.clone()),
                db,
                ..Self::default()
            }
        }
        fn calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }
        fn searches(&self) -> usize {
            self.calls()
                .iter()
                .filter(|c| c.starts_with("search"))
                .count()
        }
    }

    #[async_trait::async_trait]
    impl JiraApi for Jira {
        async fn server_info(&self) -> Result<ServerInfo, SourceError> {
            unreachable!("a refresh reads no server info")
        }
        async fn myself(&self) -> Result<Myself, SourceError> {
            unreachable!()
        }
        async fn fields(&self) -> Result<Vec<crate::model::FieldMeta>, SourceError> {
            unreachable!()
        }
        async fn issue_stamp(&self, key: &str) -> Result<Option<Issue>, SourceError> {
            self.calls.lock().unwrap().push(format!("issue:{key}"));
            let now = self.moved.get(key).map_or(key, String::as_str);
            Ok(self.db.get(now).map(|r| {
                // The database keeps milliseconds.
                let mut stamp =
                    json!({ "key": now, "fields": { "updated": r["fields"]["updated"] } });
                if let Some(at) = r["fields"]["updated"].as_str() {
                    stamp["fields"]["updated"] = json!(at.replace(".000", ".700"));
                }
                serde_json::from_value(stamp).unwrap()
            }))
        }
        async fn search(
            &self,
            jql: &str,
            _start_at: u32,
            _max_results: u32,
            _fields: &str,
        ) -> Result<SearchPage, SourceError> {
            self.calls.lock().unwrap().push(format!("search:{jql}"));
            if self.catch_up_after.is_some_and(|n| self.searches() > n) {
                *self.index.lock().unwrap() = self.db.clone();
            }
            let key = jql.split("key = \"").nth(1).unwrap().trim_end_matches('"');
            let in_scope = |r: &&Value| {
                jql.split("project in (").nth(1).is_none_or(|list| {
                    list.contains(&format!(
                        "\"{}\"",
                        r["fields"]["project"]["key"].as_str().unwrap()
                    ))
                })
            };
            let hits: Vec<Value> = self
                .index
                .lock()
                .unwrap()
                .get(key)
                .filter(in_scope)
                .cloned()
                .into_iter()
                .collect();
            Ok(serde_json::from_value(json!({ "total": hits.len(), "issues": hits })).unwrap())
        }
        async fn comments(
            &self,
            key: &str,
            _start_at: u32,
            _max_results: u32,
        ) -> Result<CommentPage, SourceError> {
            self.calls.lock().unwrap().push(format!("comments:{key}"));
            Ok(
                serde_json::from_value(json!({ "startAt": 0, "maxResults": 2, "total": 2,
                "comments": [{ "id": "1", "body": "first" }, { "id": "2", "body": "second" }] }))
                .unwrap(),
            )
        }
        async fn worklogs(&self, _key: &str) -> Result<WorklogPage, SourceError> {
            unreachable!("the worklog container is complete")
        }
    }

    fn cfg(value: Value) -> crate::JiraConfig {
        crate::JiraConfig::from_json(&value).unwrap()
    }

    async fn refresh(
        jira: &Jira,
        cfg: &crate::JiraConfig,
        key: &str,
    ) -> Result<RefreshAnswer, SourceError> {
        SyncRun {
            api: jira,
            cfg,
            source_id: "jira",
            base_url: "https://jira.example",
        }
        .refresh(key, QUICK)
        .await
    }

    fn item(answer: RefreshAnswer) -> knobas_source::SyncItem {
        match answer {
            RefreshAnswer::Item(item) => item,
            other => panic!("expected the item, got {other:?}"),
        }
    }

    /// The item is the search's record completed as a walk completes it and
    /// mapped as a walk maps it: `updated` to the second, both comments.
    #[tokio::test]
    async fn an_issue_is_its_search_record_completed_and_mapped_as_a_run_emits_it() {
        let pay = record(
            "PAY-231",
            "Retry failed SEPA payouts",
            "2026-08-22T11:48:05.000+0000",
        );
        let jira = Jira::holding(&[pay]);
        let read = item(refresh(&jira, &cfg(json!({})), "PAY-231").await.unwrap());
        assert_eq!(read.entity.to_string(), "jira:PAY-231");
        assert_eq!(read.kind, crate::KIND_TICKET);
        assert_eq!(read.title, "Retry failed SEPA payouts");
        assert_eq!(
            read.payload["fields"]["updated"],
            "2026-08-22T11:48:05.000+0000"
        );
        assert_eq!(
            read.payload["fields"]["comment"]["comments"][1]["body"],
            "second"
        );
        assert_eq!(
            jira.calls(),
            [
                "issue:PAY-231",
                r#"search:key = "PAY-231""#,
                "comments:PAY-231"
            ]
        );
    }

    /// A key Jira answers 404 for is gone, and no search is made for it.
    #[tokio::test]
    async fn a_key_jira_does_not_hold_is_gone() {
        let jira = Jira::holding(&[]);
        assert!(matches!(
            refresh(&jira, &cfg(json!({})), "PAY-999").await.unwrap(),
            RefreshAnswer::Gone
        ));
        assert_eq!(jira.calls(), ["issue:PAY-999"]);
    }

    /// An issue moved to another project answers under its new key, so the
    /// id asked is gone, as the next full sync leaves it.
    #[tokio::test]
    async fn an_issue_moved_to_another_key_is_gone_under_the_old_one() {
        let mut jira = Jira::holding(&[record("OPS-5", "moved", "2026-08-22T11:48:05.000+0000")]);
        jira.moved.insert("PAY-231".to_owned(), "OPS-5".to_owned());
        assert!(matches!(
            refresh(&jira, &cfg(json!({})), "PAY-231").await.unwrap(),
            RefreshAnswer::Gone
        ));
    }

    /// An index still holding the issue as it was before the write is asked
    /// again until it holds the write, and the item is the new record.
    #[tokio::test]
    async fn a_stale_index_is_asked_again_until_it_holds_the_write() {
        let mut jira = Jira::holding(&[record("PAY-231", "after", "2026-08-22T11:48:05.000+0000")]);
        jira.index.lock().unwrap().insert(
            "PAY-231".to_owned(),
            record("PAY-231", "before", "2026-08-22T11:47:00.000+0000"),
        );
        jira.catch_up_after = Some(2);
        let read = item(refresh(&jira, &cfg(json!({})), "PAY-231").await.unwrap());
        assert_eq!(read.title, "after");
        assert_eq!(jira.searches(), 3);
    }

    /// A ticket just filed and not indexed yet is waited for, not gone.
    #[tokio::test]
    async fn a_ticket_not_yet_indexed_is_waited_for() {
        let mut jira = Jira::holding(&[record("PAY-300", "filed", "2026-08-22T11:48:05.000+0000")]);
        jira.index.lock().unwrap().clear();
        jira.catch_up_after = Some(1);
        let read = item(
            refresh(&jira, &cfg(json!({ "projects": ["PAY"] })), "PAY-300")
                .await
                .unwrap(),
        );
        assert_eq!(read.title, "filed");
    }

    /// An issue the index holds fresh outside the source's projects is gone:
    /// a run would not read it.
    #[tokio::test]
    async fn an_issue_outside_the_sources_scope_is_gone() {
        let jira = Jira::holding(&[record(
            "PAY-231",
            "elsewhere",
            "2026-08-22T11:48:05.000+0000",
        )]);
        assert!(matches!(
            refresh(&jira, &cfg(json!({ "projects": ["OPS"] })), "PAY-231")
                .await
                .unwrap(),
            RefreshAnswer::Gone
        ));
        assert_eq!(
            jira.calls(),
            [
                "issue:PAY-231",
                r#"search:project in ("OPS") AND key = "PAY-231""#,
                r#"search:key = "PAY-231""#,
                r#"search:project in ("OPS") AND key = "PAY-231""#
            ]
        );
    }

    /// An index that never catches up is a failed read, never gone: the
    /// issue is there.
    #[tokio::test]
    async fn an_index_that_never_catches_up_is_a_failed_read() {
        let jira = Jira::holding(&[record("PAY-231", "after", "2026-08-22T11:48:05.000+0000")]);
        jira.index.lock().unwrap().insert(
            "PAY-231".to_owned(),
            record("PAY-231", "before", "2026-08-22T11:47:00.000+0000"),
        );
        let error = refresh(&jira, &cfg(json!({})), "PAY-231")
            .await
            .unwrap_err();
        assert!(matches!(error, SourceError::Unreachable(_)), "{error:?}");
        assert_eq!(jira.searches(), QUICK.tries as usize);
    }
}

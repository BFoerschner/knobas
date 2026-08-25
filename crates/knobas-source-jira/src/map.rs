//! One Jira issue → one [`SyncItem`], per interfaces doc §4.1 "Normalization".

use knobas_core::entity::EntityRef;
use knobas_source::SyncItem;

use crate::model::RawIssue;

/// Map one issue, in the namespace of the instance that fetched it.
///
/// `source_id` is `SourceInstance::id` and never the adapter kind (P10): two
/// Jiras are `jira` and `jira-eu`, and their items must not collide.
pub(crate) fn to_sync_item(source_id: &str, base_url: &str, raw: &RawIssue) -> SyncItem {
    let fields = &raw.issue.fields;
    let title = fields.summary.clone().unwrap_or_default();
    let comments = fields
        .comment
        .as_ref()
        .map(|c| c.comments.as_slice())
        .unwrap_or_default();
    let body_text = body_text(
        [title.clone()]
            .into_iter()
            .chain(fields.description.clone())
            .chain(comments.iter().filter_map(|c| c.body.clone())),
    );
    SyncItem {
        entity: EntityRef::new(source_id, &raw.issue.key),
        kind: crate::KIND_TICKET.to_owned(),
        title,
        body_text,
        // "the source's username string" -- the assignee, which is what makes
        // the launcher's @me filter mean "my tickets". Display-name mapping is
        // M2's people work.
        author: fields.assignee.as_ref().and_then(|u| u.name.clone()),
        // The source's own timestamp, never now().
        updated_at: fields
            .updated
            .as_deref()
            .and_then(crate::time::parse_jira_time),
        payload: raw.raw.clone(),
        // P5: `/browse/<key>` is the URL a human opens. It is built from the
        // configured base URL rather than `serverInfo.baseUrl`, because the URL
        // the user typed is the one reachable from the user's machine.
        web_url: Some(format!(
            "{}/browse/{}",
            base_url.trim_end_matches('/'),
            raw.issue.key
        )),
        // Jira's search API cannot report deletions; the engine's full-sync
        // sweep tombstones what stopped coming back (interfaces §4.1).
        deleted: false,
    }
}

/// Join the non-empty parts into the blob FTS indexes -- the same rule
/// `knobas-source-mock` uses, so a Jira ticket and a mock ticket are indexed
/// alike.
fn body_text(parts: impl IntoIterator<Item = String>) -> String {
    parts
        .into_iter()
        .filter(|p| !p.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SearchPage;

    const SEARCH_PAGE: &str = include_str!("../tests/golden/search-page.json");
    const EPIC: &str = include_str!("../tests/golden/issue-epic.json");

    fn page() -> SearchPage {
        serde_json::from_str(SEARCH_PAGE).expect("the golden page is a valid search response")
    }

    fn epic() -> crate::model::RawIssue {
        serde_json::from_str(EPIC).expect("the golden epic is a valid issue")
    }

    /// `total` is what ends the paging loop; the golden page is deliberately a
    /// partial one (2 of 3), the shape task 5 pages over.
    #[test]
    fn reads_the_pagination_envelope_the_wadl_documents() {
        let p = page();
        assert_eq!(p.total, 3);
        assert_eq!(p.issues.len(), 2);
    }

    /// Interfaces §4.1 normalization, and §4.2's key form `jira:PAY-231`.
    #[test]
    fn maps_an_issue_the_way_the_conventions_say() {
        let p = page();
        let item = to_sync_item("jira", "https://jira.tidewater.example", &p.issues[0]);
        assert_eq!(item.entity.to_string(), "jira:PAY-231");
        assert_eq!(item.kind, crate::KIND_TICKET);
        assert_eq!(item.title, "Retry failed SEPA payouts");
        // author = the source's username string, not the display name.
        assert_eq!(item.author.as_deref(), Some("mara.lindqvist"));
        assert_eq!(
            item.updated_at.map(|t| t.to_rfc3339()),
            Some("2026-08-22T11:48:00+00:00".to_owned())
        );
        // P5: every detail view needs "Open in browser".
        assert_eq!(
            item.web_url.as_deref(),
            Some("https://jira.tidewater.example/browse/PAY-231")
        );
        // Jira's search cannot report deletions; the engine's full-sync sweep does.
        assert!(!item.deleted);
    }

    /// body_text = title + description + comment texts, blank-line joined --
    /// what FTS indexes, mirroring knobas-source-mock.
    #[test]
    fn body_text_is_what_search_will_match_on() {
        let p = page();
        let item = to_sync_item("jira", "https://jira.tidewater.example", &p.issues[0]);
        assert!(item.body_text.starts_with("Retry failed SEPA payouts\n\n"));
        assert!(item.body_text.contains("retryable SEPA error"));
        assert!(item.body_text.contains("AC04"));
        assert!(item.body_text.contains("retry budget still needs a cap"));
        // No HTML: the adapter never asks for renderedFields (gotcha 7).
        assert!(!item.body_text.contains('<'), "{}", item.body_text);
    }

    /// Spec §3a: `payload` is the raw record, so a later mapping can re-project
    /// it without re-syncing. Custom fields the adapter knows nothing about are
    /// part of that.
    #[test]
    fn payload_keeps_the_record_verbatim() {
        let p = page();
        let item = to_sync_item("jira", "https://jira.tidewater.example", &p.issues[0]);
        assert_eq!(item.payload["key"], "PAY-231");
        assert_eq!(item.payload["fields"]["status"]["name"], "In Progress");
        assert_eq!(item.payload["fields"]["priority"]["name"], "High");
        assert_eq!(item.payload["fields"]["customfield_10008"], "PAY-200");
        assert_eq!(
            item.payload["fields"]["worklog"]["worklogs"][0]["timeSpentSeconds"],
            5400
        );
    }

    /// Roadmap §2 row A reads "issues, epics": in Jira an epic *is* an issue, so
    /// it syncs as a ticket. The epic relation stays in the payload -- links are
    /// M2.
    #[test]
    fn an_epic_is_a_ticket_whose_payload_still_says_epic() {
        let item = to_sync_item("jira", "https://jira.tidewater.example", &epic());
        assert_eq!(item.kind, crate::KIND_TICKET);
        assert_eq!(item.entity.to_string(), "jira:PAY-200");
        assert_eq!(item.payload["fields"]["issuetype"]["name"], "Epic");
    }

    #[test]
    fn an_unassigned_issue_has_no_author() {
        let p = page();
        let item = to_sync_item("jira", "https://jira.tidewater.example", &p.issues[1]);
        assert_eq!(item.author, None);
        assert_eq!(item.body_text, "Add a retry budget metric");
    }

    /// The instance id is the namespace (P10), so a second Jira does not
    /// collide with the first.
    #[test]
    fn the_namespace_is_the_instance_id_not_the_adapter_kind() {
        let p = page();
        let item = to_sync_item("jira-eu", "https://jira.eu.example/jira", &p.issues[0]);
        assert_eq!(item.entity.namespace, "jira-eu");
        assert_eq!(
            item.web_url.as_deref(),
            Some("https://jira.eu.example/jira/browse/PAY-231")
        );
    }

    /// A base URL the user typed with a trailing slash must not produce
    /// `…example//browse/PAY-231`.
    #[test]
    fn a_trailing_slash_on_the_base_url_does_not_double() {
        let p = page();
        let item = to_sync_item("jira", "https://jira.tidewater.example/", &p.issues[0]);
        assert_eq!(
            item.web_url.as_deref(),
            Some("https://jira.tidewater.example/browse/PAY-231")
        );
    }

    /// A server that truncated a container says so in `total`; the sync run
    /// uses this to decide whether to fetch the dedicated endpoint (task 5).
    #[test]
    fn a_truncated_comment_container_is_visible() {
        let raw = epic();
        let c = raw.issue.fields.comment.as_ref().unwrap();
        assert_eq!(c.total, Some(3));
        assert_eq!(c.comments.len(), 1);
    }

    /// Completion rewrites both views, or `payload` and `body_text` disagree
    /// about the same issue.
    #[test]
    fn setting_comments_rewrites_the_payload_too() {
        let mut raw = epic();
        let extra: crate::model::RawComment = serde_json::from_value(serde_json::json!({
            "id": "20011",
            "author": { "name": "mara.lindqvist", "displayName": "Mara Lindqvist", "active": true },
            "body": "Retry budget capped at five attempts.",
            "created": "2026-08-02T09:00:00.000+0200",
            "updated": "2026-08-02T09:00:00.000+0200"
        }))
        .unwrap();
        let first = raw.issue.fields.comment.as_ref().unwrap().comments[0].clone();
        raw.set_comments(vec![first, extra]);
        let item = to_sync_item("jira", "https://jira.tidewater.example", &raw);
        assert!(item.body_text.contains("Retry budget capped"));
        assert_eq!(item.payload["fields"]["comment"]["total"], 2);
        assert_eq!(
            item.payload["fields"]["comment"]["comments"][1]["id"],
            "20011"
        );
        // The typed view is what the completion check reads next run; leaving
        // it at 3 would re-fetch this issue's comments forever.
        assert_eq!(raw.issue.fields.comment.as_ref().unwrap().total, Some(2));
    }

    #[test]
    fn setting_worklogs_rewrites_the_payload_too() {
        let mut raw = epic();
        raw.set_worklogs(vec![
            serde_json::json!({ "id": "30099", "timeSpentSeconds": 600 }),
        ]);
        let item = to_sync_item("jira", "https://jira.tidewater.example", &raw);
        assert_eq!(item.payload["fields"]["worklog"]["total"], 1);
        assert_eq!(
            item.payload["fields"]["worklog"]["worklogs"][0]["id"],
            "30099"
        );
        assert_eq!(raw.issue.fields.worklog.as_ref().unwrap().total, Some(1));
    }

    /// A Jira instance has hundreds of custom fields and a version bump adds
    /// more. Rejecting an unknown key would make the adapter fail on the next
    /// upgrade -- the opposite of the config struct, where an unknown key is a
    /// bug.
    #[test]
    fn unknown_fields_do_not_break_parsing() {
        let raw: crate::model::RawIssue = serde_json::from_value(serde_json::json!({
            "key": "PAY-1",
            "fields": { "summary": "s", "somethingNew": { "nested": true } },
            "brandNewTopLevel": 7
        }))
        .unwrap();
        let item = to_sync_item("jira", "https://x.example", &raw);
        assert_eq!(item.title, "s");
        assert_eq!(item.payload["brandNewTopLevel"], 7);
    }

    /// The three responses the sync run reads but does not map into items.
    #[test]
    fn the_remaining_responses_parse_into_what_reads_them() {
        let info: crate::model::ServerInfo = serde_json::from_value(serde_json::json!({
            "baseUrl": "https://jira.tidewater.example",
            "version": "9.17.0",
            "deploymentType": "Server",
            "serverTime": "2026-08-22T13:48:00.000+0200",
            "scmInfo": "unread"
        }))
        .unwrap();
        assert_eq!(info.version.as_deref(), Some("9.17.0"));
        assert_eq!(info.deployment_type.as_deref(), Some("Server"));
        // The zone every JQL literal is rendered in, read off the server's own
        // clock rather than guessed from a timezone database.
        assert_eq!(info.offset_secs(), 7_200);
        // A server that will not say is read as the *lowest* real offset, not
        // as UTC: guessing high moves the query's lower bound forward and
        // skips edits permanently (a UTC-05 server would lose five hours on
        // every run), guessing low only re-reads what was already delivered.
        assert_eq!(
            crate::model::ServerInfo::default().offset_secs(),
            crate::time::MIN_UTC_OFFSET_SECS
        );
        // An unparseable serverTime takes the same safe road as a missing one.
        let broken: crate::model::ServerInfo =
            serde_json::from_value(serde_json::json!({ "serverTime": "yesterday" })).unwrap();
        assert_eq!(broken.offset_secs(), crate::time::MIN_UTC_OFFSET_SECS);

        let me: crate::model::Myself = serde_json::from_value(serde_json::json!({
            "name": "mara.lindqvist",
            "displayName": "Mara Lindqvist",
            "emailAddress": "unread@tidewater.example"
        }))
        .unwrap();
        assert_eq!(me.name.as_deref(), Some("mara.lindqvist"));
        assert_eq!(me.display_name.as_deref(), Some("Mara Lindqvist"));

        let comments: crate::model::CommentPage = serde_json::from_value(serde_json::json!({
            "startAt": 0, "maxResults": 1, "total": 3,
            "comments": [{ "id": "20010", "body": "Scope is retries only." }]
        }))
        .unwrap();
        assert_eq!(comments.total, 3);
        assert_eq!(
            comments.comments[0].body.as_deref(),
            Some("Scope is retries only.")
        );

        // The worklog endpoint declares no query parameters, so its `total` is
        // not modelled: there is no second request it could drive.
        let worklogs: crate::model::WorklogPage = serde_json::from_value(serde_json::json!({
            "startAt": 0, "maxResults": 1, "total": 1,
            "worklogs": [{ "id": "30001", "timeSpentSeconds": 5400 }]
        }))
        .unwrap();
        assert_eq!(worklogs.worklogs.len(), 1);
        assert_eq!(worklogs.worklogs[0]["timeSpentSeconds"], 5400);
    }
}

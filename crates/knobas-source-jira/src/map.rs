//! One Jira issue → one [`SyncItem`], per interfaces doc §4.1 "Normalization".

use knobas_core::entity::EntityRef;
use knobas_source::SyncItem;

use crate::model::RawIssue;

/// Map one issue, in the namespace of the instance that fetched it.
///
/// `source_id` is `SourceInstance::id` and never the adapter kind (P10): two
/// Jiras are `jira` and `jira-eu`, and their items must not collide.
pub(crate) fn to_sync_item(
    source_id: &str,
    base_url: &str,
    epic_link_field: Option<&str>,
    sprint_field: Option<&str>,
    raw: &RawIssue,
) -> SyncItem {
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
        payload: with_sprints(with_epic(&raw.raw, epic_link_field), sprint_field),
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

/// The record verbatim, plus the epic its Epic Link field names, as
/// `epic: {"key": …}` beside `fields`, where the record does not already have
/// an `epic` key.
///
/// A classic Data Center project keeps epic membership only in the Epic Link
/// custom field: a bare key, under an id minted per instance, which no
/// adapter-wide declaration can name and which `at` cannot read (it must reach
/// an object). So when the source has the field configured, the adapter stores
/// it where the descriptor's second parent candidate reads it. This is the
/// second departure from §3a's verbatim payload, for Gitea's reason
/// (ADR-0018; the deputy's ruling of 2026-10-12 on #598). Never into
/// `fields.parent`, which is Jira's own sub-task relation; the custom field
/// itself stays as Jira sent it, and the key is stored as that string. A null,
/// absent, empty or non-string value adds nothing.
fn with_epic(raw: &serde_json::Value, epic_link_field: Option<&str>) -> serde_json::Value {
    let mut payload = raw.clone();
    let epic = epic_link_field
        .and_then(|field| raw.get("fields")?.get(field)?.as_str())
        .filter(|key| !key.is_empty());
    if let (Some(key), Some(record)) = (epic, payload.as_object_mut()) {
        record
            .entry("epic")
            .or_insert_with(|| serde_json::json!({ "key": key }));
    }
    payload
}

/// The record with the sprints its Sprint field names stored beside it as
/// `sprints: [{id, name, state}]` (#811), where the record does not already
/// have a `sprints` key.
///
/// The Sprint field is a custom field whose id is the instance's own, so no
/// adapter-wide declaration can name it, as with Epic Link above; the
/// descriptor's `sprint_state` reads `sprints` instead. A third departure
/// from §3a's verbatim payload, for #598's reason. **Always written**, empty
/// when the field is not configured, null, or holds nothing readable: an
/// empty list is a ticket in no sprint as far as knobas knows, and a key that
/// is there lets the contract battery tell that from a misspelt path.
///
/// Jira Data Center sends each sprint in one of two spellings: an object
/// (`{"id": 4, "name": …, "state": "active"}`) or, on older instances, the
/// sprint's `toString`, `…Sprint@6f2c[id=3,rapidViewId=1,state=CLOSED,name=…,
/// startDate=…]`. Both are read; the state is lowercased so the two agree. An
/// element that yields no state is dropped: a sprint whose state is unknown
/// says nothing `sprint:current` could use.
fn with_sprints(mut payload: serde_json::Value, sprint_field: Option<&str>) -> serde_json::Value {
    let sprints: Vec<serde_json::Value> = sprint_field
        .and_then(|field| payload.get("fields")?.get(field)?.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(sprint)
        .collect();
    if let Some(record) = payload.as_object_mut() {
        record
            .entry("sprints")
            .or_insert_with(|| serde_json::Value::Array(sprints));
    }
    payload
}

/// One sprint, in either spelling [`with_sprints`] reads, as
/// `{id, name, state}`; `None` when no state can be read.
fn sprint(value: &serde_json::Value) -> Option<serde_json::Value> {
    if let Some(object) = value.as_object() {
        let state = object.get("state")?.as_str()?.trim().to_lowercase();
        if state.is_empty() {
            return None;
        }
        return Some(serde_json::json!({
            "id": object.get("id").cloned().unwrap_or(serde_json::Value::Null),
            "name": object.get("name").cloned().unwrap_or(serde_json::Value::Null),
            "state": state,
        }));
    }
    let text = value.as_str()?;
    let body = text.split_once('[')?.1.strip_suffix(']')?;
    // `key=value` pairs separated by commas, where a name may itself hold a
    // comma: a piece that does not start with `word=` belongs to the value
    // before it.
    let mut pairs: Vec<(String, String)> = Vec::new();
    for piece in body.split(',') {
        match piece.split_once('=') {
            Some((key, rest)) if !key.is_empty() && key.chars().all(char::is_alphanumeric) => {
                pairs.push((key.to_owned(), rest.to_owned()));
            }
            _ => {
                if let Some((_, value)) = pairs.last_mut() {
                    value.push(',');
                    value.push_str(piece);
                }
            }
        }
    }
    let get = |key: &str| {
        pairs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
            .filter(|v| !v.is_empty() && *v != "<null>")
    };
    let state = get("state")?.trim().to_lowercase();
    let id = get("id").map_or(serde_json::Value::Null, |id| {
        id.parse::<i64>()
            .map_or_else(|_| serde_json::json!(id), |n| serde_json::json!(n))
    });
    Some(serde_json::json!({ "id": id, "name": get("name"), "state": state }))
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
        let item = to_sync_item(
            "jira",
            "https://jira.tidewater.example",
            None,
            None,
            &p.issues[0],
        );
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
        let item = to_sync_item(
            "jira",
            "https://jira.tidewater.example",
            None,
            None,
            &p.issues[0],
        );
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
        let item = to_sync_item(
            "jira",
            "https://jira.tidewater.example",
            None,
            None,
            &p.issues[0],
        );
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
        let item = to_sync_item(
            "jira",
            "https://jira.tidewater.example",
            None,
            None,
            &epic(),
        );
        assert_eq!(item.kind, crate::KIND_TICKET);
        assert_eq!(item.entity.to_string(), "jira:PAY-200");
        assert_eq!(item.payload["fields"]["issuetype"]["name"], "Epic");
    }

    /// The payload with the knobas-written `epic` removed: what the rest of
    /// the record must equal, byte for byte, in every case below.
    fn without_epic(payload: &serde_json::Value) -> serde_json::Value {
        let mut rest = without_sprints(payload);
        rest.as_object_mut().unwrap().remove("epic");
        rest
    }

    /// The payload without the `sprints` every ticket carries (#811), which
    /// must be the empty list where no Sprint field was configured.
    fn without_sprints(payload: &serde_json::Value) -> serde_json::Value {
        let mut rest = payload.clone();
        let sprints = rest.as_object_mut().unwrap().remove("sprints");
        assert_eq!(sprints, Some(serde_json::json!([])), "{payload}");
        rest
    }

    /// A classic Data Center project keeps epic membership in the Epic Link
    /// custom field, a bare key under an id minted per instance, which no
    /// adapter-wide declaration can read. So a configured source stores it as
    /// `epic: {"key": …}` beside the record, where the descriptor's second
    /// parent candidate reads it (ADR-0018, the deputy's ruling of 2026-10-12
    /// on #598). The rest of the record stays verbatim, the custom field too.
    #[test]
    fn a_configured_epic_link_is_stored_as_epic_beside_the_record() {
        let p = page();
        let raw = &p.issues[0];
        let item = to_sync_item(
            "jira",
            "https://jira.tidewater.example",
            Some("customfield_10008"),
            None,
            raw,
        );
        assert_eq!(
            item.payload["epic"],
            serde_json::json!({ "key": "PAY-200" })
        );
        assert_eq!(item.payload["fields"]["customfield_10008"], "PAY-200");
        assert!(item.payload["fields"].get("parent").is_none());
        assert_eq!(without_epic(&item.payload), raw.raw);
    }

    /// Nothing to say, nothing stored: an issue with no epic (the field comes
    /// back null), an issue whose record lacks the field, an empty value, a
    /// value that is not a string, and a source with no Epic Link configured. The
    /// payload is then the record verbatim.
    #[test]
    fn no_epic_is_stored_without_an_epic_link_value() {
        let p = page();
        let with = |fields: serde_json::Value| -> crate::model::RawIssue {
            serde_json::from_value(serde_json::json!({ "key": "PAY-1", "fields": fields })).unwrap()
        };
        let cases: Vec<(&str, Option<&str>, crate::model::RawIssue)> = vec![
            ("unconfigured", None, p.issues[0].clone()),
            ("missing", Some("customfield_10008"), p.issues[1].clone()),
            (
                "null",
                Some("customfield_10008"),
                with(serde_json::json!({ "summary": "s", "customfield_10008": null })),
            ),
            (
                "empty",
                Some("customfield_10008"),
                with(serde_json::json!({ "summary": "s", "customfield_10008": "" })),
            ),
            (
                "not a string",
                Some("customfield_10008"),
                with(
                    serde_json::json!({ "summary": "s", "customfield_10008": { "key": "PAY-2" } }),
                ),
            ),
        ];
        for (case, field, raw) in cases {
            let item = to_sync_item("jira", "https://jira.tidewater.example", field, None, &raw);
            assert!(
                item.payload.get("epic").is_none(),
                "{case}: {}",
                item.payload
            );
            assert_eq!(without_sprints(&item.payload), raw.raw, "{case}");
        }
    }

    /// A record that already has a top-level `epic` keeps it: knobas adds
    /// beside the record and never overwrites what the source sent, as
    /// Gitea's `with_repository` does.
    #[test]
    fn an_epic_key_the_record_already_has_is_kept() {
        let raw: crate::model::RawIssue = serde_json::from_value(serde_json::json!({
            "key": "PAY-1",
            "epic": "the source's own",
            "fields": { "summary": "s", "customfield_10008": "PAY-200" }
        }))
        .unwrap();
        let item = to_sync_item(
            "jira",
            "https://x.example",
            Some("customfield_10008"),
            None,
            &raw,
        );
        assert_eq!(without_sprints(&item.payload), raw.raw);
    }

    #[test]
    fn an_unassigned_issue_has_no_author() {
        let p = page();
        let item = to_sync_item(
            "jira",
            "https://jira.tidewater.example",
            None,
            None,
            &p.issues[1],
        );
        assert_eq!(item.author, None);
        assert_eq!(item.body_text, "Add a retry budget metric");
    }

    /// The instance id is the namespace (P10), so a second Jira does not
    /// collide with the first.
    #[test]
    fn the_namespace_is_the_instance_id_not_the_adapter_kind() {
        let p = page();
        let item = to_sync_item(
            "jira-eu",
            "https://jira.eu.example/jira",
            None,
            None,
            &p.issues[0],
        );
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
        let item = to_sync_item(
            "jira",
            "https://jira.tidewater.example/",
            None,
            None,
            &p.issues[0],
        );
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
        let item = to_sync_item("jira", "https://jira.tidewater.example", None, None, &raw);
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
        let item = to_sync_item("jira", "https://jira.tidewater.example", None, None, &raw);
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
        let item = to_sync_item("jira", "https://x.example", None, None, &raw);
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

    /// A configured Sprint field is stored as `sprints` beside the record
    /// (#811), in either spelling Jira Data Center serves it: the objects a
    /// recent instance sends, and the `Sprint@…[id=…,state=ACTIVE,name=…]`
    /// strings an older one sends. The state is lowercased, so `active` is
    /// one word whichever spelling arrived. The custom field itself stays as
    /// Jira sent it.
    #[test]
    fn a_configured_sprint_field_is_stored_as_sprints_beside_the_record() {
        let mut raw = page().issues[0].clone();
        raw.raw["fields"]["customfield_10104"] = serde_json::json!([
            "com.atlassian.greenhopper.service.sprint.Sprint@6f2c[id=3,rapidViewId=1,state=CLOSED,name=PAY Sprint 2, the long one,startDate=2026-09-01T09:00:00.000Z,endDate=2026-09-14T17:00:00.000Z,completeDate=<null>,activatedDate=<null>,sequence=3,goal=,autoStartStop=false,synced=false]",
            { "id": 4, "name": "PAY Sprint 3", "state": "active", "boardId": 1 }
        ]);
        let item = to_sync_item(
            "jira",
            "https://x.example",
            None,
            Some("customfield_10104"),
            &raw,
        );
        assert_eq!(
            item.payload["sprints"],
            serde_json::json!([
                { "id": 3, "name": "PAY Sprint 2, the long one", "state": "closed" },
                { "id": 4, "name": "PAY Sprint 3", "state": "active" }
            ])
        );
        assert_eq!(item.payload["fields"]["customfield_10104"][1]["id"], 4);
    }

    /// Every ticket carries `sprints`, empty where the field is not
    /// configured, is null, or holds nothing readable: a declared list that
    /// is there and empty is a ticket in no sprint, which the contract
    /// battery can tell from a misspelt path (clause 3).
    #[test]
    fn a_ticket_in_no_sprint_carries_an_empty_list() {
        let p = page();
        let unconfigured = to_sync_item("jira", "https://x.example", None, None, &p.issues[0]);
        assert_eq!(unconfigured.payload["sprints"], serde_json::json!([]));
        let mut raw = p.issues[0].clone();
        for value in [
            serde_json::Value::Null,
            serde_json::json!("not a list"),
            serde_json::json!([42, "no brackets here", { "name": "no state" }]),
        ] {
            raw.raw["fields"]["customfield_10104"] = value.clone();
            let item = to_sync_item(
                "jira",
                "https://x.example",
                None,
                Some("customfield_10104"),
                &raw,
            );
            assert_eq!(item.payload["sprints"], serde_json::json!([]), "{value}");
        }
    }
}

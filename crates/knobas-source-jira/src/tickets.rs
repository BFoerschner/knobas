//! A Jira ticket's fields (ADR-0054): what a person may change on an issue
//! besides its status, the people it can be assigned to, and the edit.
//!
//! | call | request |
//! | --- | --- |
//! | [`fields`] | `GET api/2/issue/{key}?expand=editmeta&fields=summary,assignee,priority,labels` |
//! | [`assignable`] | `GET api/2/user/assignable/search?issueKey={key}&username={typed}&maxResults=1000` |
//! | [`edit`]: summary, priority, labels | `PUT api/2/issue/{key}`, `{"fields": {…}, "update": {"labels": [{"add": …}, {"remove": …}]}}` |
//! | [`edit`]: assignee | `PUT api/2/issue/{key}/assignee`, `{"name": …}` or `{"name": null}` |
//!
//! Every resource is in `testenv/specs/jira-dc-rest.wadl` (`getIssue`'s
//! `expand`, `findAssignableUsers`, `editIssue`, `assign`). The assignee has
//! its own resource because it needs *Assign Issues* alone, not *Edit
//! Issues*.
//!
//! **Not witnessed on a real Jira** (no Jira ran where this was built;
//! section 33 of the 2026-11-21 ruling): whether the assignable search with
//! an empty `username` lists everyone, whether `expand=editmeta` on the
//! issue read equals the `editmeta` resource for a user without *Edit
//! Issues*, and the exact 400 body of an edit to a field off the screen. The
//! mapping and the bodies are held to recorded answers in this module's
//! tests and to `knobas-mockd`.

use knobas_source::ticket::{
    self, FieldOption, FieldOptions, TicketField, TicketFieldKind, TicketFields,
};
use knobas_source::{Assignee, LabelChange, SourceError};
use serde_json::{Value, json};

use crate::http::{self, HttpClient};

/// The most people one assignable search answers: Jira's own ceiling for
/// the resource. Fewer back means the list is whole.
pub(crate) const ASSIGNABLE_MAX: usize = 1_000;

/// Why a named field `editmeta` leaves out cannot be changed here.
pub(crate) const NOT_ON_SCREEN: &str =
    "Not on this ticket's edit screen in Jira, or this ticket cannot be edited in its status.";

/// The fields the issue read asks for: the four knobas names.
const NAMED_FIELDS: &str = "summary,assignee,priority,labels";

fn issue_path(key: &str) -> String {
    format!("rest/api/2/issue/{key}")
}

/// The issue's current values and its edit screen, in one request.
///
/// # Errors
///
/// As the request maps.
pub(crate) async fn fields(http: &HttpClient, key: &str) -> Result<TicketFields, SourceError> {
    let answer: Value = http::get_json(
        http,
        &issue_path(key),
        &[
            ("expand", "editmeta".to_owned()),
            ("fields", NAMED_FIELDS.to_owned()),
        ],
    )
    .await?;
    Ok(map(key, &answer))
}

/// A user as Jira names them for a write: `name`, or `key` where there is
/// no name, as the declared assignee path reads it.
fn account(user: &Value) -> Option<String> {
    user["name"]
        .as_str()
        .or_else(|| user["key"].as_str())
        .map(str::to_owned)
}

/// `editmeta` and the values, read into knobas' fields.
pub(crate) fn map(key: &str, answer: &Value) -> TicketFields {
    let project = key.rsplit_once('-').map_or(key, |(project, _)| project);
    let values = &answer["fields"];
    let meta = &answer["editmeta"]["fields"];
    let on_screen = |id: &str| meta.get(id).filter(|m| m.is_object());
    let required = |id: &str| on_screen(id).is_some_and(|m| m["required"].as_bool() == Some(true));
    let readonly = |id: &str| on_screen(id).is_none().then(|| NOT_ON_SCREEN.to_owned());

    let assignee = &values["assignee"];
    let current_priority = values["priority"]["name"].as_str().map(str::to_owned);
    let mut priorities: Vec<FieldOption> = on_screen("priority")
        .and_then(|m| m["allowedValues"].as_array())
        .map(|allowed| {
            allowed
                .iter()
                .filter_map(|v| v["name"].as_str())
                .map(|name| FieldOption {
                    value: name.to_owned(),
                    label: name.to_owned(),
                    hint: None,
                })
                .collect()
        })
        .unwrap_or_default();
    // The value must be one of the options; a priority the screen no longer
    // offers is still what the ticket holds.
    if let Some(now) = &current_priority
        && !priorities.iter().any(|o| &o.value == now)
    {
        priorities.insert(
            0,
            FieldOption {
                value: now.clone(),
                label: now.clone(),
                hint: None,
            },
        );
    }
    let labels: Vec<&str> = values["labels"]
        .as_array()
        .map(|l| l.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();

    TicketFields {
        fields: vec![
            TicketField {
                id: ticket::TITLE.into(),
                label: "Summary".into(),
                kind: TicketFieldKind::Text,
                value: values["summary"].as_str().map_or(Value::Null, |s| json!(s)),
                shown: None,
                required: required("summary"),
                readonly: readonly("summary"),
            },
            TicketField {
                id: ticket::ASSIGNEE.into(),
                label: "Assignee".into(),
                kind: TicketFieldKind::Search {
                    scope: project.to_owned(),
                    person: true,
                },
                value: account(assignee).map_or(Value::Null, Value::String),
                shown: assignee["displayName"].as_str().map(str::to_owned),
                required: required("assignee"),
                readonly: readonly("assignee"),
            },
            TicketField {
                id: ticket::PRIORITY.into(),
                label: "Priority".into(),
                kind: TicketFieldKind::Choice {
                    options: priorities,
                },
                value: current_priority.map_or(Value::Null, Value::String),
                shown: None,
                required: required("priority"),
                readonly: readonly("priority"),
            },
            TicketField {
                id: ticket::LABELS.into(),
                label: "Labels".into(),
                kind: TicketFieldKind::Labels {
                    scope: None,
                    create: true,
                },
                value: json!(labels),
                shown: None,
                required: required("labels"),
                readonly: readonly("labels"),
            },
        ],
    }
}

/// Who issue `key` can be assigned to, matching `typed` (Jira matches it
/// against username, name and e-mail).
///
/// # Errors
///
/// As the request maps.
pub(crate) async fn assignable(
    http: &HttpClient,
    key: &str,
    typed: &str,
) -> Result<FieldOptions, SourceError> {
    let users: Vec<Value> = http::get_json(
        http,
        "rest/api/2/user/assignable/search",
        &[
            ("issueKey", key.to_owned()),
            ("username", typed.trim().to_owned()),
            ("maxResults", ASSIGNABLE_MAX.to_string()),
        ],
    )
    .await?;
    Ok(people(&users))
}

/// Users read into options, `complete` when fewer than [`ASSIGNABLE_MAX`]
/// came back.
pub(crate) fn people(users: &[Value]) -> FieldOptions {
    let options = users
        .iter()
        .filter(|u| u["active"].as_bool() != Some(false))
        .filter_map(|u| {
            let account = account(u)?;
            let name = u["displayName"]
                .as_str()
                .filter(|n| !n.trim().is_empty())
                .map(str::to_owned);
            Some(match name {
                Some(name) => FieldOption {
                    value: account.clone(),
                    label: name,
                    hint: Some(account),
                },
                None => FieldOption {
                    value: account.clone(),
                    label: account,
                    hint: None,
                },
            })
        })
        .collect();
    FieldOptions {
        options,
        complete: users.len() < ASSIGNABLE_MAX,
    }
}

/// What an edit asks of an issue.
pub(crate) struct Edit<'a> {
    pub(crate) title: Option<&'a str>,
    pub(crate) assignee: Option<&'a Assignee>,
    pub(crate) priority: Option<&'a str>,
    pub(crate) labels: Option<&'a LabelChange>,
    pub(crate) fields: &'a std::collections::BTreeMap<String, Value>,
}

impl Edit<'_> {
    /// The issue's `PUT` body, or `None` when the edit sets nothing but the
    /// assignee. Labels go under `update` as names added and removed, never
    /// as the whole list (a label somebody added a second ago stays).
    pub(crate) fn body(&self) -> Option<Value> {
        let mut fields = serde_json::Map::new();
        if let Some(title) = self.title {
            fields.insert("summary".into(), json!(title));
        }
        if let Some(priority) = self.priority {
            fields.insert("priority".into(), json!({ "name": priority }));
        }
        let mut update = serde_json::Map::new();
        if let Some(change) = self.labels.filter(|l| !l.is_empty()) {
            let ops: Vec<Value> = change
                .add
                .iter()
                .map(|name| json!({ "add": name }))
                .chain(change.remove.iter().map(|name| json!({ "remove": name })))
                .collect();
            update.insert("labels".into(), Value::Array(ops));
        }
        if fields.is_empty() && update.is_empty() {
            return None;
        }
        let mut body = serde_json::Map::new();
        if !fields.is_empty() {
            body.insert("fields".into(), Value::Object(fields));
        }
        if !update.is_empty() {
            body.insert("update".into(), Value::Object(update));
        }
        Some(Value::Object(body))
    }

    /// Refuse an edit with nothing in it, or a field of the source's own
    /// this adapter does not edit, before anything is sent.
    fn check(&self, key: &str) -> Result<(), SourceError> {
        if self.title.is_none()
            && self.assignee.is_none()
            && self.priority.is_none()
            && self.labels.is_none_or(LabelChange::is_empty)
            && self.fields.is_empty()
        {
            return Err(SourceError::protocol(format!(
                "jira: an edit of {key} with nothing to change"
            )));
        }
        if let Some(id) = self.fields.keys().next() {
            return Err(SourceError::protocol(format!(
                "jira: {id:?} is not a field knobas edits on {key}"
            )));
        }
        Ok(())
    }
}

/// Edit issue `key`: the issue's `PUT` first, then the assignee's, each
/// answered 204. If the second fails the write fails with its sentence, and
/// a resend repeats both, which sets what is already set.
///
/// # Errors
///
/// [`SourceError::Protocol`] for what [`Edit::check`] refuses, and as each
/// request maps: Jira's own sentence (`errorMessages`, `errors`) for a field
/// it will not take.
pub(crate) async fn edit(http: &HttpClient, key: &str, edit: &Edit<'_>) -> Result<(), SourceError> {
    edit.check(key)?;
    if let Some(body) = edit.body() {
        put_json(http, &issue_path(key), &body).await?;
    }
    if let Some(assignee) = edit.assignee {
        put_json(
            http,
            &format!("{}/assignee", issue_path(key)),
            &json!({ "name": assignee.account() }),
        )
        .await?;
    }
    Ok(())
}

/// `PUT <base>/<path>` with `body` as JSON. Unlike a `POST`, the transport
/// may send it again: a repeat sets what is already set.
async fn put_json(http: &HttpClient, path: &str, body: &Value) -> Result<(), SourceError> {
    http.send(http.request(knobas_http::Method::PUT, path).json(body))
        .await
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `GET issue/PAY-231?expand=editmeta&fields=…` as Jira's documented
    /// shape answers it (the WADL's `getIssue` example and its `field-meta`
    /// schema): assignee and priority on the screen, labels too, summary
    /// required.
    fn recorded() -> Value {
        json!({
            "key": "PAY-231",
            "fields": {
                "summary": "SEPA payout fails on retry",
                "assignee": { "name": "mara", "key": "JIRAUSER10100", "displayName": "Mara Lindqvist" },
                "priority": { "name": "High" },
                "labels": ["sepa", "triage"]
            },
            "editmeta": { "fields": {
                "summary": { "required": true, "name": "Summary", "schema": { "type": "string", "system": "summary" }, "operations": ["set"] },
                "assignee": { "required": false, "name": "Assignee", "schema": { "type": "user", "system": "assignee" }, "operations": ["set"] },
                "priority": { "required": false, "name": "Priority", "schema": { "type": "priority", "system": "priority" }, "operations": ["set"],
                    "allowedValues": [{ "name": "Highest", "id": "1" }, { "name": "High", "id": "2" }, { "name": "Low", "id": "4" }] },
                "labels": { "required": false, "name": "Labels", "schema": { "type": "array", "items": "string", "system": "labels" }, "operations": ["add", "set", "remove"] }
            } }
        })
    }

    #[test]
    fn editmeta_reads_into_the_four_named_fields() {
        let fields = map("PAY-231", &recorded());
        assert_eq!(ticket::fault(&fields), None);
        let title = fields.get("title").unwrap();
        assert_eq!(title.value, json!("SEPA payout fails on retry"));
        assert!(title.required);
        assert_eq!(title.readonly, None);
        let who = fields.get("assignee").unwrap();
        assert_eq!(who.value, json!("mara"));
        assert_eq!(who.shown.as_deref(), Some("Mara Lindqvist"));
        assert_eq!(
            who.kind,
            TicketFieldKind::Search {
                scope: "PAY".into(),
                person: true
            }
        );
        let priority = fields.get("priority").unwrap();
        let TicketFieldKind::Choice { options } = &priority.kind else {
            panic!("{priority:?}")
        };
        assert_eq!(
            options.iter().map(|o| o.value.as_str()).collect::<Vec<_>>(),
            ["Highest", "High", "Low"]
        );
        assert_eq!(priority.value, json!("High"));
        assert_eq!(
            fields.get("labels").unwrap().value,
            json!(["sepa", "triage"])
        );
    }

    /// A named field the screen leaves out is listed read only, with the
    /// reason; an assignee without a name is named by its key.
    #[test]
    fn a_field_off_the_screen_is_read_only_and_a_nameless_user_reads_by_key() {
        let mut answer = recorded();
        answer["editmeta"]["fields"]
            .as_object_mut()
            .unwrap()
            .remove("priority");
        answer["fields"]["assignee"] = json!({ "key": "JIRAUSER10200", "displayName": "Ben" });
        answer["fields"]["priority"] = json!({ "name": "Medium" });
        let fields = map("PAY-231", &answer);
        assert_eq!(
            ticket::fault(&fields),
            None,
            "the held priority stays an option"
        );
        assert_eq!(
            fields.get("priority").unwrap().readonly.as_deref(),
            Some(NOT_ON_SCREEN)
        );
        assert_eq!(
            fields.get("assignee").unwrap().value,
            json!("JIRAUSER10200")
        );
        let none = map(
            "PAY-231",
            &json!({ "fields": {}, "editmeta": { "fields": {} } }),
        );
        assert_eq!(ticket::fault(&none), None);
        assert!(none.fields.iter().all(|f| f.readonly.is_some()));
    }

    #[test]
    fn assignable_people_read_by_name_with_their_account_and_complete_under_the_ceiling() {
        let found = people(&[
            json!({ "name": "mara", "displayName": "Mara Lindqvist", "active": true }),
            json!({ "name": "gone", "displayName": "Left", "active": false }),
            json!({ "key": "JIRAUSER1", "displayName": "" }),
        ]);
        assert!(found.complete);
        assert_eq!(found.options.len(), 2);
        assert_eq!(found.options[0].hint.as_deref(), Some("mara"));
        assert_eq!(found.options[1].label, "JIRAUSER1");
        let many: Vec<Value> = (0..ASSIGNABLE_MAX)
            .map(|n| json!({ "name": format!("u{n}") }))
            .collect();
        assert!(!people(&many).complete);
    }

    /// The labels go as names added and removed under `update`, never the
    /// whole list under `fields`; the assignee never rides in the issue's
    /// `PUT` (a user with *Assign Issues* alone would be refused).
    #[test]
    fn the_issue_body_carries_summary_priority_and_label_changes_and_never_the_assignee() {
        let none = std::collections::BTreeMap::new();
        let change = LabelChange {
            add: vec!["sepa".into()],
            remove: vec!["triage".into()],
        };
        let who = Assignee::Account("mara".into());
        let edit = Edit {
            title: Some("Retry three times"),
            assignee: Some(&who),
            priority: Some("High"),
            labels: Some(&change),
            fields: &none,
        };
        assert_eq!(
            edit.body(),
            Some(json!({
                "fields": { "summary": "Retry three times", "priority": { "name": "High" } },
                "update": { "labels": [{ "add": "sepa" }, { "remove": "triage" }] }
            }))
        );
        let only_assignee = Edit {
            title: None,
            priority: None,
            labels: None,
            ..edit
        };
        assert_eq!(only_assignee.body(), None);
        assert!(only_assignee.check("PAY-231").is_ok());
        let empty = Edit {
            assignee: None,
            ..only_assignee
        };
        assert!(
            empty
                .check("PAY-231")
                .unwrap_err()
                .to_string()
                .contains("nothing")
        );
        let own: std::collections::BTreeMap<String, Value> =
            [("customfield_10010".to_owned(), json!("2026-12-01"))].into();
        let unknown = Edit {
            fields: &own,
            ..empty
        };
        assert!(
            unknown
                .check("PAY-231")
                .unwrap_err()
                .to_string()
                .contains("customfield_10010")
        );
    }
}

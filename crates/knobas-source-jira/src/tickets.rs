//! A Jira ticket's fields (ADR-0054): what a person may change on an issue
//! besides its status, the people it can be assigned to, and the edit.
//!
//! | call | request |
//! | --- | --- |
//! | [`fields`] | `GET api/2/issue/{key}?expand=editmeta&fields=*navigable` |
//! | [`assignable`] | `GET api/2/user/assignable/search?issueKey={key}&username={typed}&maxResults=1000` |
//! | [`edit`]: summary, priority, labels, own fields | `PUT api/2/issue/{key}`, `{"fields": {…}, "update": {"labels": [{"add": …}, {"remove": …}]}}` |
//! | [`edit`] with own fields, first | `GET api/2/issue/{key}?expand=editmeta&fields=<their ids>`, to shape each value |
//! | [`edit`]: assignee | `PUT api/2/issue/{key}/assignee`, `{"name": …}` or `{"name": null}` |
//!
//! Every resource is in `testenv/specs/jira-dc-rest.wadl` (`getIssue`'s
//! `expand`, `findAssignableUsers`, `editIssue`, `assign`). The assignee has
//! its own resource because it needs *Assign Issues* alone, not *Edit
//! Issues*.
//!
//! **A source's own fields** (step 10 of the ruling): each other `editmeta`
//! entry this module maps ([`own_kind`]): the due date and a custom date
//! picker as `date`, a custom text field as `text`, a custom number as
//! `number`, a custom single select or radio buttons as `choice` (sent as
//! `{"value": …}`), a custom user picker as `search` with `person` (sent as
//! `{"name": …}`). Anything else (arrays, cascading selects, rich text, the
//! sprint and epic fields, time tracking) is left out, not listed read only.
//!
//! **Witnessed on a real Jira 10.3.24** (2026-11-22, `live_jira_seeded`'s
//! `a_tickets_fields_are_edited_and_read_back_on_a_real_jira`, its answers kept
//! in `tests/golden/ticket-fields-*.json`): the screen read and its mapping,
//! the assignable search with an empty `username` listing every assignable
//! user, each named field and the due date written and read back, Jira's 400
//! sentence for a field off the screen, and that `editmeta` covers only the
//! fields a read asks for. **Not witnessed:** what `expand=editmeta` answers
//! a user without *Edit Issues* (the seeded users all have it), and custom
//! fields (the seeded screen has none of the kinds this module maps).

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

/// The fields the issue read asks for: every navigable one, so a source's
/// own field's value comes with its screen. The four knobas names are among
/// them.
const READ_FIELDS: &str = "*navigable";

/// The ids `editmeta` names for the four fields knobas names, and the
/// system fields that are never a source's own here.
const NAMED_IDS: [&str; 4] = ["summary", "assignee", "priority", "labels"];

/// A custom field's type as `editmeta`'s schema names it (`schema.custom`).
const CUSTOM: &str = "com.atlassian.jira.plugin.system.customfieldtypes:";

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
            ("fields", READ_FIELDS.to_owned()),
        ],
    )
    .await?;
    Ok(map(key, &answer))
}

/// The kind of a source's own field from its `editmeta` entry, or `None` for
/// one this module does not edit. `project`: a user picker's search scope.
pub(crate) fn own_kind(id: &str, meta: &Value, project: &str) -> Option<TicketFieldKind> {
    if NAMED_IDS.contains(&id) {
        return None;
    }
    let schema = &meta["schema"];
    if schema["system"].as_str() == Some("duedate") {
        return Some(TicketFieldKind::Date);
    }
    let custom = schema["custom"].as_str()?.strip_prefix(CUSTOM)?;
    match (custom, schema["type"].as_str()?) {
        ("textfield", "string") => Some(TicketFieldKind::Text),
        ("datepicker", "date") => Some(TicketFieldKind::Date),
        ("float", "number") => Some(TicketFieldKind::Number),
        ("select" | "radiobuttons", "option") => {
            let options: Vec<FieldOption> = meta["allowedValues"]
                .as_array()?
                .iter()
                .filter(|o| o["disabled"].as_bool() != Some(true))
                .filter_map(|o| o["value"].as_str())
                .map(|v| FieldOption {
                    value: v.to_owned(),
                    label: v.to_owned(),
                    hint: None,
                })
                .collect();
            Some(TicketFieldKind::Choice { options })
        }
        ("userpicker", "user") => Some(TicketFieldKind::Search {
            scope: project.to_owned(),
            person: true,
        }),
        _ => None,
    }
}

/// A source's own field's value as the issue read holds it, in the form
/// its kind takes, and what it reads as where that differs.
fn own_value(kind: &TicketFieldKind, value: &Value) -> (Value, Option<String>) {
    match kind {
        TicketFieldKind::Text | TicketFieldKind::Number | TicketFieldKind::Date => {
            let fits = kind.fits(value);
            (if fits { value.clone() } else { Value::Null }, None)
        }
        TicketFieldKind::Choice { .. } => (
            value["value"].as_str().map_or(Value::Null, |v| json!(v)),
            None,
        ),
        TicketFieldKind::Search { .. } => (
            account(value).map_or(Value::Null, Value::String),
            value["displayName"].as_str().map(str::to_owned),
        ),
        _ => (Value::Null, None),
    }
}

/// The issue's own fields, by label, from its screen and its values.
fn own_fields(project: &str, values: &Value, meta: &Value) -> Vec<TicketField> {
    let Some(meta) = meta.as_object() else {
        return Vec::new();
    };
    let mut fields: Vec<TicketField> = meta
        .iter()
        .filter_map(|(id, m)| {
            let kind = own_kind(id, m, project)?;
            let (mut value, shown) = own_value(&kind, &values[id.as_str()]);
            // A held choice the screen no longer offers is read as none.
            if !kind.fits(&value) {
                value = Value::Null;
            }
            Some(TicketField {
                id: id.clone(),
                label: m["name"].as_str().unwrap_or(id).to_owned(),
                kind,
                value,
                shown,
                required: m["required"].as_bool() == Some(true),
                readonly: None,
            })
        })
        .collect();
    fields.sort_by_key(|f| f.label.to_lowercase());
    fields
}

/// Each own field of an edit, shaped as Jira takes it on the issue's
/// `PUT`, from the issue's screen: refused by name where the screen does
/// not offer it, or the value does not fit it.
pub(crate) fn shape_own(
    key: &str,
    own: &std::collections::BTreeMap<String, Value>,
    meta: &Value,
) -> Result<serde_json::Map<String, Value>, SourceError> {
    let project = key.rsplit_once('-').map_or(key, |(project, _)| project);
    let mut out = serde_json::Map::new();
    for (id, value) in own {
        let kind = meta
            .get(id.as_str())
            .and_then(|m| own_kind(id, m, project))
            .ok_or_else(|| {
                SourceError::protocol(format!(
                    "jira: {id:?} is not a field knobas edits on {key}: it is not on the \
                     ticket's edit screen, or not a kind knobas edits"
                ))
            })?;
        if !kind.fits(value) {
            return Err(SourceError::protocol(format!(
                "jira: {value} does not fit {id:?} on {key}"
            )));
        }
        let sent = match (&kind, value) {
            (_, Value::Null) => Value::Null,
            (TicketFieldKind::Choice { .. }, v) => json!({ "value": v }),
            (TicketFieldKind::Search { .. }, v) => json!({ "name": v }),
            (_, v) => v.clone(),
        };
        out.insert(id.clone(), sent);
    }
    Ok(out)
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

    let mut named = vec![
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
    ];
    named.extend(own_fields(project, values, meta));
    TicketFields { fields: named }
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
    /// as the whole list (a label somebody added a second ago stays). `own`:
    /// the source's own fields as [`shape_own`] shaped them.
    pub(crate) fn body(&self, own: serde_json::Map<String, Value>) -> Option<Value> {
        let mut fields = own;
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

    /// Refuse an edit with nothing in it, or an own field that cannot be one
    /// of Jira's ([`jira_field_id`]), before anything is sent. One that can is
    /// held to the issue's screen by [`shape_own`].
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
        if let Some(id) = self.fields.keys().find(|id| !jira_field_id(id)) {
            return Err(SourceError::protocol(format!(
                "jira: {id:?} is not a field knobas edits on {key}"
            )));
        }
        Ok(())
    }
}

/// Whether `id` can name a field of Jira's own this module edits: the due
/// date, or a custom field (`customfield_<n>`).
fn jira_field_id(id: &str) -> bool {
    id == "duedate"
        || id
            .strip_prefix("customfield_")
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// The `fields` a write's screen read asks for: the own fields it sets, by id.
fn screen_fields(own: &std::collections::BTreeMap<String, Value>) -> String {
    own.keys().map(String::as_str).collect::<Vec<_>>().join(",")
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
    let own = if edit.fields.is_empty() {
        serde_json::Map::new()
    } else {
        // `editmeta` covers only the fields the read asks for (witnessed on Jira 10.3.24,
        // 2026-11-22: asked with `fields=summary` it left the due date out), so the read
        // asks for exactly the fields this edit sets.
        let screen: Value = http::get_json(
            http,
            &issue_path(key),
            &[
                ("expand", "editmeta".to_owned()),
                ("fields", screen_fields(edit.fields)),
            ],
        )
        .await?;
        shape_own(key, edit.fields, &screen["editmeta"]["fields"])?
    };
    if let Some(body) = edit.body(own) {
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

    /// The answer a **real Jira 10.3.24** gave the fields read (recorded by
    /// the live case on 2026-11-22, `tests/golden/ticket-fields-*.json`, the
    /// host and e-mail addresses masked): the four named fields editable, the
    /// due date its own, everything else on the screen (components, the epic
    /// link, the description, versions, links, the reporter) left out.
    #[test]
    fn a_real_jiras_screen_reads_into_the_four_and_its_due_date() {
        let answer: Value = serde_json::from_str(include_str!(
            "../tests/golden/ticket-fields-issue-editmeta.json"
        ))
        .expect("the recorded answer");
        let key = answer["key"].as_str().expect("a key");
        let fields = map(key, &answer);
        assert_eq!(ticket::fault(&fields), None);
        let ids: Vec<&str> = fields.fields.iter().map(|f| f.id.as_str()).collect();
        assert_eq!(ids, ["title", "assignee", "priority", "labels", "duedate"]);
        assert!(
            fields.fields.iter().all(|f| f.readonly.is_none()),
            "{fields:?}"
        );
        let TicketFieldKind::Choice { options } = &fields.get("priority").unwrap().kind else {
            panic!("a choice")
        };
        assert!(options.iter().any(|o| o.value == "Highest"), "{options:?}");
        let users: Vec<Value> = serde_json::from_str(include_str!(
            "../tests/golden/ticket-fields-assignable-empty.json"
        ))
        .expect("the recorded people");
        let found = people(&users);
        assert!(found.complete && found.options.len() >= 5, "{found:?}");
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

    /// A priority the ticket holds that the screen no longer offers is still
    /// an option, first; one it offers is not listed twice.
    #[test]
    fn a_held_priority_the_screen_drops_leads_the_options_and_an_offered_one_is_not_doubled() {
        let options = |answer: &Value| {
            let fields = map("PAY-231", answer);
            let TicketFieldKind::Choice { options } = fields.get("priority").unwrap().kind.clone()
            else {
                panic!("a choice")
            };
            options.into_iter().map(|o| o.value).collect::<Vec<_>>()
        };
        assert_eq!(options(&recorded()), ["Highest", "High", "Low"]);
        let mut dropped = recorded();
        dropped["fields"]["priority"] = json!({ "name": "Medium" });
        assert_eq!(options(&dropped), ["Medium", "Highest", "High", "Low"]);
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
            edit.body(serde_json::Map::new()),
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
        assert_eq!(only_assignee.body(serde_json::Map::new()), None);
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
            unknown.check("PAY-231").is_ok(),
            "held to the screen at the write"
        );
        assert!(
            shape_own("PAY-231", &own, &recorded()["editmeta"]["fields"])
                .unwrap_err()
                .to_string()
                .contains("customfield_10010")
        );
    }

    /// A screen with a source's own fields as Jira Data Center answers them
    /// (the WADL's `field-meta` shape, the system and custom schema names of
    /// Jira 10): the due date, a text field, a number, a single select and a
    /// user picker are offered; the description, the sprint and a multi
    /// select are left out, not listed read only.
    fn with_own() -> Value {
        let mut answer = recorded();
        let meta = answer["editmeta"]["fields"].as_object_mut().unwrap();
        let custom = |name: &str, kind: &str, custom: &str, id: u64| {
            json!({ "required": false, "name": name, "operations": ["set"],
                "schema": { "type": kind, "custom": format!("{CUSTOM}{custom}"), "customId": id } })
        };
        meta.insert("duedate".into(), json!({ "required": false, "name": "Due date", "operations": ["set"], "schema": { "type": "date", "system": "duedate" } }));
        meta.insert("description".into(), json!({ "required": false, "name": "Description", "operations": ["set"], "schema": { "type": "string", "system": "description" } }));
        meta.insert(
            "customfield_10100".into(),
            custom("Customer ref", "string", "textfield", 10100),
        );
        meta.insert(
            "customfield_10106".into(),
            custom("Story Points", "number", "float", 10106),
        );
        let mut env = custom("Environment kind", "option", "select", 10200);
        env["allowedValues"] = json!([{ "value": "Staging", "id": "1" }, { "value": "Production", "id": "2" }, { "value": "Old", "id": "3", "disabled": true }]);
        meta.insert("customfield_10200".into(), env);
        meta.insert(
            "customfield_10300".into(),
            custom("Reviewer", "user", "userpicker", 10300),
        );
        meta.insert("customfield_10101".into(), json!({ "required": false, "name": "Sprint", "operations": ["set"], "schema": { "type": "array", "items": "string", "custom": "com.pyxis.greenhopper.jira:gh-sprint", "customId": 10101 } }));
        meta.insert(
            "customfield_10400".into(),
            custom("Regions", "array", "multiselect", 10400),
        );
        let values = answer["fields"].as_object_mut().unwrap();
        values.insert("duedate".into(), json!("2026-12-01"));
        values.insert("customfield_10100".into(), json!("ACME-7"));
        values.insert("customfield_10106".into(), json!(3.0));
        values.insert(
            "customfield_10200".into(),
            json!({ "value": "Staging", "id": "1" }),
        );
        values.insert(
            "customfield_10300".into(),
            json!({ "name": "ben", "displayName": "Ben Okafor" }),
        );
        answer
    }

    #[test]
    fn a_sources_own_fields_are_read_by_kind_and_label_and_the_rest_left_out() {
        let fields = map("PAY-231", &with_own());
        assert_eq!(ticket::fault(&fields), None);
        let own: Vec<(&str, &str)> = fields.fields[4..]
            .iter()
            .map(|f| (f.id.as_str(), f.kind.word()))
            .collect();
        assert_eq!(
            own,
            [
                ("customfield_10100", "text"),
                ("duedate", "date"),
                ("customfield_10200", "choice"),
                ("customfield_10300", "search"),
                ("customfield_10106", "number"),
            ]
        );
        assert_eq!(fields.get("duedate").unwrap().value, json!("2026-12-01"));
        assert_eq!(fields.get("customfield_10106").unwrap().value, json!(3.0));
        let env = fields.get("customfield_10200").unwrap();
        assert_eq!(env.value, json!("Staging"));
        let TicketFieldKind::Choice { options } = &env.kind else {
            panic!("{env:?}")
        };
        assert_eq!(options.len(), 2, "a disabled option is not offered");
        let reviewer = fields.get("customfield_10300").unwrap();
        assert_eq!(
            (&reviewer.value, reviewer.shown.as_deref()),
            (&json!("ben"), Some("Ben Okafor"))
        );
        for left in ["description", "customfield_10101", "customfield_10400"] {
            assert!(fields.get(left).is_none(), "{left} is left out");
        }
    }

    /// The write's screen read names exactly the fields it sets: Jira's
    /// `editmeta` leaves out every field the read does not ask for.
    #[test]
    fn the_writes_screen_read_asks_for_the_fields_it_sets() {
        let own: std::collections::BTreeMap<String, Value> = [
            ("duedate".to_owned(), json!("2026-12-01")),
            ("customfield_10106".to_owned(), json!(3)),
        ]
        .into();
        assert_eq!(screen_fields(&own), "customfield_10106,duedate");
        for (id, can) in [
            ("duedate", true),
            ("customfield_10106", true),
            ("customfield_", false),
            ("customfield_10x", false),
            ("contract_battery_probe", false),
            ("summary", false),
        ] {
            assert_eq!(jira_field_id(id), can, "{id}");
        }
    }

    /// Each own field is shaped as Jira takes it: a choice as `{"value"}`, a
    /// person as `{"name"}`, the rest as they are, `null` clears; a value
    /// that does not fit, or a field off the screen, is refused by name.
    #[test]
    fn own_fields_are_shaped_from_the_screen_and_refused_where_they_do_not_fit() {
        let meta = with_own()["editmeta"]["fields"].clone();
        let own: std::collections::BTreeMap<String, Value> = [
            ("duedate".to_owned(), json!("2026-12-24")),
            ("customfield_10106".to_owned(), json!(5)),
            ("customfield_10200".to_owned(), json!("Production")),
            ("customfield_10300".to_owned(), json!("mara")),
            ("customfield_10100".to_owned(), Value::Null),
        ]
        .into();
        let shaped = shape_own("PAY-231", &own, &meta).unwrap();
        assert_eq!(
            Value::Object(shaped.clone()),
            json!({
                "duedate": "2026-12-24",
                "customfield_10106": 5,
                "customfield_10200": { "value": "Production" },
                "customfield_10300": { "name": "mara" },
                "customfield_10100": null
            })
        );
        let none = std::collections::BTreeMap::new();
        let edit = Edit {
            title: Some("t"),
            assignee: None,
            priority: None,
            labels: None,
            fields: &none,
        };
        assert_eq!(
            edit.body(shaped).unwrap()["fields"]["customfield_10200"],
            json!({ "value": "Production" })
        );
        for (id, value, says) in [
            ("customfield_10200", json!("Old"), "does not fit"),
            ("duedate", json!("next week"), "does not fit"),
            ("customfield_10106", json!("five"), "does not fit"),
            (
                "customfield_10101",
                json!(["Sprint 4"]),
                "not a field knobas edits",
            ),
            ("description", json!("text"), "not a field knobas edits"),
        ] {
            let one: std::collections::BTreeMap<String, Value> = [(id.to_owned(), value)].into();
            let refused = shape_own("PAY-231", &one, &meta).unwrap_err().to_string();
            assert!(refused.contains(says) && refused.contains(id), "{refused}");
        }
    }
}

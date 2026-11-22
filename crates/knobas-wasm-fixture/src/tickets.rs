//! The fixture's tickets (ADR-0054): two berth tickets of a ticket system,
//! synced when the config says `"tickets": true`, so every other test's
//! corpus stays as it was.
//!
//! * **Built against 1.5.0** (`--features v1_5`) it is an adapter from before
//!   ticket fields: its tickets move (it declares `transition`, which every
//!   1.x world carries in `write`), and it exports no `tickets`, so knobas
//!   reads their other fields as text.
//! * **Built against 1.6.0** it also declares `edit_ticket` and answers
//!   `tickets.ticket-fields` and `tickets.field-options`: a summary, an
//!   assignee searched among the harbour's people, a priority, labels from
//!   a list, and one field of its own, a due date.
//! * **A move** is `PATCH <base_url>/tickets/<key>` with `{"status": …}`,
//!   and **an edit** the same with the op's fields, so a stand-in server
//!   sees what reached it. A status the workflow does not offer from where
//!   the ticket stands is refused by name before anything is sent, as an
//!   edit with nothing in it and a field the answer does not list are.

use chrono::{TimeZone, Utc};
use knobas_adapter_sdk::spi::role::RoleClaim;
use knobas_adapter_sdk::spi::{EntityRef, KindInfo, KindPaths, ListPath, ParentPath, PayloadPath};
use knobas_adapter_sdk::{SourceError, SyncItem, WriteReceipt};

use crate::fetch;

/// The ticket kind, the ticket-system role's.
pub const KIND: &str = "ticket";

/// `(key, summary, status, priority, assignee, labels)` per ticket.
pub const TICKETS: [(&str, &str, &str, &str, Option<&str>, &[&str]); 2] = [
    (
        "BERTH-1",
        "Fender panels cracked at berth 7",
        "To Do",
        "High",
        Some("harbourmaster"),
        &["fenders"],
    ),
    (
        "BERTH-2",
        "Reefer plug 12 trips under load",
        "In Progress",
        "Medium",
        None,
        &[],
    ),
];

/// The workflow: from each status, where a ticket can go.
pub fn reach(status: &str) -> &'static [&'static str] {
    match status {
        "To Do" => &["In Progress"],
        "In Progress" => &["Done", "To Do"],
        "Done" => &["In Progress"],
        _ => &[],
    }
}

/// The harbour's people, `(account, name)`: who a ticket can be assigned to.
#[cfg(not(feature = "v1_5"))]
pub const PEOPLE: [(&str, &str); 3] = [
    ("harbourmaster", "Ines Harbourmaster"),
    ("pilot-ana", "Ana Pilot"),
    ("rigger-tom", "Tom Rigger"),
];

/// The labels a ticket may carry.
#[cfg(not(feature = "v1_5"))]
pub const LABELS: [&str; 3] = ["fenders", "reefers", "cranes"];

/// The priorities, in the source's order.
#[cfg(not(feature = "v1_5"))]
pub const PRIORITIES: [&str; 3] = ["High", "Medium", "Low"];

/// The fixture's own field: a due date.
#[cfg(not(feature = "v1_5"))]
pub const DUE: &str = "due";

pub fn kind() -> KindInfo {
    KindInfo {
        id: KIND.into(),
        label: "Ticket".into(),
        plural: "Tickets".into(),
        monogram: "BT".into(),
        full_sync_exhaustive: true,
        refreshes: true,
        title_stays_in_mirror: false,
    }
}

/// Every path the ticket-system role needs, each landing on [`item`]'s
/// payload.
pub fn payload_paths() -> KindPaths {
    let at = |key: &str| vec![PayloadPath::of([key])];
    KindPaths {
        kind: KIND.into(),
        status_name: at("status"),
        priority: at("priority"),
        assignee: at("assignee"),
        issue_type: at("type"),
        labels: vec![ListPath {
            at: PayloadPath::of(["labels"]),
            entry: PayloadPath::default(),
        }],
        parent: vec![ParentPath {
            kind: KIND.into(),
            at: PayloadPath::of(["parent"]),
            names: vec![PayloadPath::of(["key"])],
        }],
        project_key: at("project"),
        ..KindPaths::default()
    }
}

/// The ticket-system claim: the workflow read, and in the 1.6.0 build the
/// fields read that goes with `edit_ticket`.
pub fn role() -> RoleClaim {
    let mut reads = vec!["reachable_transitions".to_owned()];
    if cfg!(not(feature = "v1_5")) {
        reads.push(knobas_adapter_sdk::spi::role::TICKET_FIELDS.to_owned());
    }
    RoleClaim {
        role: knobas_adapter_sdk::spi::role::TICKET_SYSTEM.into(),
        reads,
    }
}

/// The writes the tickets add.
pub fn write_ops() -> Vec<String> {
    let mut ops = vec!["transition".to_owned()];
    if cfg!(not(feature = "v1_5")) {
        ops.push("edit_ticket".to_owned());
    }
    ops
}

type Ticket = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    Option<&'static str>,
    &'static [&'static str],
);

pub fn item(source: &str, ticket: &Ticket) -> SyncItem {
    let (key, summary, status, priority, assignee, labels) = *ticket;
    SyncItem {
        entity: EntityRef::new(source, key),
        kind: KIND.into(),
        title: summary.into(),
        body_text: format!("quaywright ticket: {summary}"),
        author: Some("harbourmaster".into()),
        updated_at: Utc.with_ymd_and_hms(2026, 11, 21, 9, 0, 0).single(),
        payload: serde_json::json!({
            "key": key,
            "summary": summary,
            "status": status,
            "priority": priority,
            "assignee": assignee,
            "labels": labels,
            "type": "Task",
            "project": "BERTH",
            "parent": null,
        }),
        web_url: None,
        deleted: false,
    }
}

/// The ticket whose key `key` is.
pub fn find(key: &str) -> Option<&'static Ticket> {
    TICKETS.iter().find(|t| t.0 == key)
}

/// A move or an edit sent to the stand-in: `PATCH <base>/tickets/<key>`.
pub fn send(
    base_url: &str,
    key: &str,
    body: &serde_json::Value,
) -> Result<WriteReceipt, SourceError> {
    use wasip2::http::types::Method;
    let url = format!("{base_url}/tickets/{key}");
    let hop = fetch::send(&Method::Patch, &url, body.to_string().as_bytes())
        .map_err(SourceError::Unreachable)?;
    if !(200..300).contains(&hop.status) {
        return Err(SourceError::protocol(format!(
            "{url} answered {}: {}",
            hop.status, hop.body
        )));
    }
    Ok(WriteReceipt { remote_id: None })
}

/// A move of `key` to `status`, refused by name where the workflow does not
/// offer it from where the ticket stands.
pub fn transition(base_url: &str, key: &str, status: &str) -> Result<WriteReceipt, SourceError> {
    let ticket = find(key).ok_or_else(|| SourceError::protocol(format!("no ticket {key}")))?;
    let offered = reach(ticket.2);
    if !offered
        .iter()
        .any(|s| s.eq_ignore_ascii_case(status.trim()))
    {
        return Err(SourceError::protocol(format!(
            "{key} cannot move to {status:?} from {:?}: this workflow offers {offered:?}",
            ticket.2
        )));
    }
    send(base_url, key, &serde_json::json!({ "status": status }))
}

/// An edit of `key`, its `EditTicket` fields as JSON: refused before
/// anything is sent when it carries nothing, or a field of its own other
/// than [`DUE`]; otherwise sent whole.
#[cfg(not(feature = "v1_5"))]
pub fn edit(
    base_url: &str,
    key: &str,
    op: &serde_json::Value,
) -> Result<WriteReceipt, SourceError> {
    if find(key).is_none() {
        return Err(SourceError::protocol(format!("no ticket {key}")));
    }
    let set = |name: &str| !op[name].is_null();
    let labels = &op["labels"];
    let labelled = labels.is_object()
        && (labels["add"].as_array().is_some_and(|a| !a.is_empty())
            || labels["remove"].as_array().is_some_and(|a| !a.is_empty()));
    let own = op["fields"].as_object().cloned().unwrap_or_default();
    if !set("title") && !set("assignee") && !set("priority") && !labelled && own.is_empty() {
        return Err(SourceError::protocol(format!(
            "an edit of {key} with nothing to change"
        )));
    }
    if let Some(id) = own.keys().find(|id| id.as_str() != DUE) {
        return Err(SourceError::protocol(format!(
            "{id:?} is not a field of a berth ticket"
        )));
    }
    send(base_url, key, op)
}

/// What a person may change on `key`.
#[cfg(not(feature = "v1_5"))]
pub fn fields(key: &str) -> Option<serde_json::Value> {
    let (_, summary, _, priority, assignee, labels) = *find(key)?;
    let shown = assignee
        .and_then(|a| PEOPLE.iter().find(|p| p.0 == a))
        .map(|p| p.1);
    Some(serde_json::json!({ "fields": [
        { "id": "title", "label": "Summary", "kind": { "type": "text" }, "value": summary, "required": true },
        { "id": "assignee", "label": "Assignee", "kind": { "type": "search", "scope": "BERTH", "person": true },
          "value": assignee, "shown": shown },
        { "id": "priority", "label": "Priority",
          "kind": { "type": "choice", "options": PRIORITIES.iter().map(|p| serde_json::json!({ "value": p, "label": p })).collect::<Vec<_>>() },
          "value": priority },
        { "id": "labels", "label": "Labels", "kind": { "type": "labels", "scope": "BERTH", "create": false }, "value": labels },
        { "id": DUE, "label": "Due date", "kind": { "type": "date" }, "value": null },
    ] }))
}

/// The choices of `field` matching `typed`: the harbour's people, or its
/// labels. Each list is whole.
#[cfg(not(feature = "v1_5"))]
pub fn options(key: &str, field: &str, typed: &str) -> Option<serde_json::Value> {
    find(key)?;
    let typed = typed.trim().to_lowercase();
    let keep = |text: &str| typed.is_empty() || text.to_lowercase().contains(&typed);
    let options: Vec<serde_json::Value> = match field {
        "assignee" => PEOPLE
            .iter()
            .filter(|(account, name)| keep(account) || keep(name))
            .map(|(account, name)| serde_json::json!({ "value": account, "label": name, "hint": account }))
            .collect(),
        "labels" => LABELS
            .iter()
            .filter(|l| keep(l))
            .map(|l| serde_json::json!({ "value": l, "label": l }))
            .collect(),
        _ => return None,
    };
    Some(serde_json::json!({ "options": options, "complete": true }))
}

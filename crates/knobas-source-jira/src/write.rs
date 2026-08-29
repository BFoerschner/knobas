//! The three writes M2 ratified for Jira: transition, comment, create.
//!
//! Deliberately free of [`WriteOp`]: this module takes the values already
//! unpacked, and [`crate::source`] -- which is where `impl Source for
//! JiraSource` lives -- does the unpacking. That is not a style choice. The
//! write queue is knobas' only outbound write path, and
//! `knobas-sync/tests/write_choke_point.rs` enforces it by refusing any
//! production file that *names* the op enum without implementing the trait; a
//! dispatch module that named it would read as a second write path.
//!
//! [`WriteOp`]: knobas_source::WriteOp
//!
//! # The endpoints, and what the WADL declares on them
//!
//! | write | resource | body |
//! | --- | --- | --- |
//! | transition | `GET api/2/issue/{key}/transitions`, then `POST` the same | `{"transition":{"id":"<id>"}}` |
//! | comment | `POST api/2/issue/{key}/comment` | `{"body":"<text>"}` |
//! | create | `POST api/2/issue` | `{"fields":{"project":{"key":…},"summary":…,"description":…,"issuetype":{"name":…}}}` |
//!
//! All three are in `testenv/specs/jira-dc-rest.wadl`, so `knobas-mockd`
//! records a violation for anything else and this crate's own suite goes red.

use knobas_source::SourceError;

use crate::http::JiraHttp;

/// Move a ticket to `status`.
///
/// Two requests, and the first one is the point (interfaces §5, story 2):
/// **which transition reaches a status is workflow configuration**, per project
/// and per issue type, so the set is read from the source immediately before
/// the move rather than assumed. A ticket that has moved since the user chose
/// -- the queue's hold detection catches the common case, and a workflow that
/// simply does not offer the status catches the rest -- is refused by name,
/// with what *was* available, instead of being posted as a transition id that
/// means something else on this instance.
///
/// The match on the target status is case-insensitive and trimmed: the name
/// comes back to us from the source's own list in the first request, but it
/// reaches the queue as text a person picked and a payload a person may have
/// edited.
///
/// # Errors
///
/// [`SourceError::Protocol`] when the workflow does not offer `status` from
/// where the ticket stands, naming what it does offer; otherwise whatever the
/// two requests map to.
pub(crate) async fn transition(
    http: &JiraHttp,
    key: &str,
    status: &str,
) -> Result<(), SourceError> {
    #[derive(serde::Deserialize)]
    struct Available {
        #[serde(default)]
        transitions: Vec<Transition>,
    }
    #[derive(serde::Deserialize)]
    struct Transition {
        id: String,
        #[serde(default)]
        to: Option<To>,
    }
    #[derive(serde::Deserialize)]
    struct To {
        #[serde(default)]
        name: Option<String>,
    }

    let path = format!("rest/api/2/issue/{key}/transitions");
    let available: Available = http.get_json(&path, &[]).await?;
    let wanted = status.trim();
    let named = |t: &Transition| t.to.as_ref().and_then(|to| to.name.clone());

    let Some(id) = available
        .transitions
        .iter()
        .find(|t| named(t).is_some_and(|name| name.trim().eq_ignore_ascii_case(wanted)))
        .map(|t| t.id.clone())
    else {
        let offered: Vec<String> = available.transitions.iter().filter_map(named).collect();
        return Err(SourceError::protocol(format!(
            "{key} cannot move to {wanted:?} from where it stands: this workflow offers {offered:?}"
        )));
    };

    // 204, no body. Nothing is decoded, deliberately -- see `post_json`.
    http.post_json(&path, &serde_json::json!({ "transition": { "id": id } }))
        .await
        .map(|_| ())
}

/// Reply on a ticket.
///
/// # Errors
///
/// The [`SourceError`] the request maps to. An empty body is refused by Jira
/// itself with a 400, which reaches the queue as a refusal carrying Jira's own
/// sentence rather than as a retry.
pub(crate) async fn comment(http: &JiraHttp, key: &str, body: &str) -> Result<(), SourceError> {
    http.post_json(
        &format!("rest/api/2/issue/{key}/comment"),
        &serde_json::json!({ "body": body }),
    )
    .await
    .map(|_| ())
}

/// Create a ticket in `project`, and report the key Jira gave it.
///
/// `description` is omitted from the body entirely when it is empty rather than
/// sent as `""`: a Jira with a required-field validator on description treats
/// an empty string as a filled-in field, and a screen that does not have the
/// field at all rejects the whole create for naming it.
///
/// # Errors
///
/// [`SourceError::Protocol`] if the create succeeded but Jira did not name the
/// issue it made -- which would leave the write reported as done with nothing
/// to point at; otherwise whatever the request maps to.
pub(crate) async fn create_ticket(
    http: &JiraHttp,
    project: &str,
    title: &str,
    body: &str,
    ticket_type: &str,
) -> Result<String, SourceError> {
    let mut fields = serde_json::json!({
        "project": { "key": project },
        "summary": title,
        "issuetype": { "name": ticket_type },
    });
    if !body.trim().is_empty() {
        fields["description"] = serde_json::Value::String(body.to_owned());
    }

    let response = http
        .post_json("rest/api/2/issue", &serde_json::json!({ "fields": fields }))
        .await?;
    let status = response.status();
    let created: serde_json::Value = response.json().await.map_err(|error| {
        SourceError::protocol(format!(
            "Jira answered {status} to a create but not with the shape the REST v2 contract \
             documents: {error}"
        ))
    })?;
    created
        .get("key")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| {
            SourceError::protocol(
                "Jira accepted the new ticket but did not say what key it gave it".to_owned(),
            )
        })
}

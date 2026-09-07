//! The writes ratified for Jira: transition, comment, create (M2) and the
//! worklog (M3.1, issue #280) -- and, since #498, the one **read** a write of
//! this crate makes on its own account: [`offered_transitions`], which is the
//! request `transition` has always made to resolve a status and which
//! `Source::reachable_transitions` now also serves the select from. It lives
//! here rather than beside the sync's reads because it is that request and not
//! a second one; a copy of it in a read module would be the two-lists problem
//! the shared function exists to prevent.
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
//! | log work | `POST api/2/issue/{key}/worklog` | `{"started":"<yyyy-MM-dd'T'HH:mm:ss.SSSZ>","timeSpentSeconds":…,"comment":…}` |
//!
//! All four are in `testenv/specs/jira-dc-rest.wadl` -- the worklog resource as
//! `addWorklog` -- so `knobas-mockd` records a violation for anything else and
//! this crate's own suite goes red. mockd serves the first three and answers
//! `POST .../worklog` with its 501, which is a contract verb it has no handler
//! for rather than a violation: ADR-0013 freezes it, and the worklog's witness
//! is the real Jira (`knobas-app/tests/atlassian_live.rs`).

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
    let path = transitions_path(key);
    let offered = offered_transitions(http, key).await?;
    let wanted = status.trim();

    let Some(id) = offered
        .iter()
        .find(|t| t.status.trim().eq_ignore_ascii_case(wanted))
        .map(|t| t.id.clone())
    else {
        let named: Vec<&str> = offered.iter().map(|t| t.status.as_str()).collect();
        return Err(SourceError::protocol(format!(
            "{key} cannot move to {wanted:?} from where it stands: this workflow offers {named:?}"
        )));
    };

    // 204, no body. Nothing is decoded, deliberately -- see `post_json`.
    http.post_json(&path, &serde_json::json!({ "transition": { "id": id } }))
        .await
        .map(|_| ())
}

/// One move this issue's workflow offers from where it stands: the transition
/// knobas would post, and the status it lands on.
///
/// Both halves, because the two callers want different ones and asking twice
/// would be two requests where Jira answers both in one: [`transition`] posts
/// the `id`, and [`Source::reachable_transitions`] offers the `status`.
///
/// [`Source::reachable_transitions`]: knobas_source::Source::reachable_transitions
pub(crate) struct Offered {
    pub(crate) id: String,
    /// The status the move lands on, in Jira's own spelling (`"In Progress"`).
    pub(crate) status: String,
}

/// The one endpoint, spelled once: a `GET` reads the offer and a `POST` to the
/// same path takes it.
fn transitions_path(key: &str) -> String {
    format!("rest/api/2/issue/{key}/transitions")
}

/// What this issue's workflow offers **from where the issue stands right now**.
///
/// **This is the read the write already made** (`CONTEXT.md`: *reachable
/// transition*, issue #498). Before the SPI grew
/// [`Source::reachable_transitions`] this request had one caller,
/// [`transition`], which made it a step inside a write; the select upstream had
/// no read at all and offered whatever the source's corpus had been seen to
/// use. Nothing about the request changed when the second caller arrived --
/// same path, same decoding, same `to.name` -- which is what makes the offer
/// the reader is shown and the list the write resolves against **one answer
/// from one endpoint**, and not two lists that can disagree.
///
/// A transition Jira does not say the destination of is dropped rather than
/// reported under some placeholder: `to` is optional in the response and a move
/// with no landing status is one neither caller can do anything with -- the
/// write cannot match a name against it and the select cannot offer it.
///
/// Deliberately **not** deduplicated and not sorted. Two transitions may land
/// on one status (Jira's workflows routinely have several routes to *Done*),
/// and the order is the workflow's own, which is the order a Jira user sees in
/// the issue view. Collapsing either would be knobas editing the source's
/// answer.
///
/// # Errors
///
/// Whatever the request maps to: a dead credential is
/// [`SourceError::Unauthorized`] here as everywhere else, and an issue key this
/// instance does not have is the 404 the request answered.
///
/// [`Source::reachable_transitions`]: knobas_source::Source::reachable_transitions
pub(crate) async fn offered_transitions(
    http: &JiraHttp,
    key: &str,
) -> Result<Vec<Offered>, SourceError> {
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

    let available: Available = http.get_json(&transitions_path(key), &[]).await?;
    Ok(available
        .transitions
        .into_iter()
        .filter_map(|t| {
            t.to.and_then(|to| to.name)
                .map(|status| Offered { id: t.id, status })
        })
        .collect())
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

/// Jira's own spelling of an instant, and the only one its worklog resource
/// accepts: `yyyy-MM-dd'T'HH:mm:ss.SSSZ`.
///
/// **Three milliseconds and a numeric offset, both mandatory.** Jira parses
/// this field with a fixed `SimpleDateFormat` rather than an ISO-8601 reader,
/// so RFC 3339's `Z` (which is what `DateTime::to_rfc3339` writes for UTC) is
/// rejected outright with *"Date value 2026-09-03T09:30:00Z is invalid"*, and
/// so is a value with no sub-second part. `%z` is what writes `+0000`; `%:z`
/// would write `+00:00`, which the same parser refuses.
///
/// Sent in **UTC** rather than in the server's own offset. The value is an
/// instant either way -- Jira converts to the instance's timezone for display
/// -- and the alternative would be a second reason for this adapter to care
/// what `/serverInfo` said.
const JIRA_STARTED: &str = "%Y-%m-%dT%H:%M:%S%.3f%z";

/// Log time against a ticket, and report the id Jira gave the worklog.
///
/// `seconds` goes over as `timeSpentSeconds`, which is the field that means
/// what knobas measured: `timeSpent` ("2h 30m") is the same fact rounded to
/// whatever the instance's *time tracking* configuration calls a day, and a
/// worklog knobas wrote as a duration and Jira stored as a rounded string
/// would not read back as what was logged.
///
/// **No `adjustEstimate` parameter**, so Jira applies its own default of
/// `auto` -- the remaining estimate goes down by the time logged, which is
/// what logging work in the Jira UI does. knobas puts no such question to the
/// user, and a parameter sent here would be this adapter answering it on their
/// behalf (issue #280).
///
/// An empty `comment` is sent as an empty field rather than omitted: a worklog
/// with no words is a worklog, and Jira accepts one.
///
/// # Errors
///
/// [`SourceError::Protocol`] if Jira accepted the worklog but did not name it
/// -- the local copy would then have nothing to point at, which is the same
/// refusal [`create_ticket`] makes for the same reason; otherwise whatever the
/// request maps to.
pub(crate) async fn log_work(
    http: &JiraHttp,
    key: &str,
    started: chrono::DateTime<chrono::Utc>,
    seconds: i64,
    comment: &str,
) -> Result<String, SourceError> {
    let response = http
        .post_json(
            &format!("rest/api/2/issue/{key}/worklog"),
            &serde_json::json!({
                "started": started.format(JIRA_STARTED).to_string(),
                "timeSpentSeconds": seconds,
                "comment": comment,
            }),
        )
        .await?;
    let status = response.status();
    let logged: serde_json::Value = response.json().await.map_err(|error| {
        SourceError::protocol(format!(
            "Jira answered {status} to a worklog but not with the shape the REST v2 contract \
             documents: {error}"
        ))
    })?;
    logged
        .get("id")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| {
            SourceError::protocol(
                "Jira accepted the worklog but did not say what id it gave it".to_owned(),
            )
        })
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    /// The one thing about this module that is a *format* rather than a
    /// request, and the one Jira rejects outright when it is wrong.
    ///
    /// Milliseconds and a numeric offset, both present, and `+0000` rather
    /// than `Z` or `+00:00`. Asserted here rather than only in the live suite
    /// because the live suite needs a licensed container and this is a string.
    #[test]
    fn a_worklogs_started_carries_milliseconds_and_a_numeric_offset() {
        let at = chrono::Utc.with_ymd_and_hms(2026, 9, 3, 9, 30, 0).unwrap();
        assert_eq!(
            at.format(super::JIRA_STARTED).to_string(),
            "2026-09-03T09:30:00.000+0000",
            "Jira parses `started` with a fixed pattern: `Z` and `+00:00` are \
             both refused, and so is a value with no milliseconds"
        );
    }
}

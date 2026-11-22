//! Gitea's issues as tickets (#1206), against the docker-free fake: what a run
//! emits and keeps in its cursor, the workflow read, and the three writes the
//! ticket system offers. `tests/live_gitea.rs` holds each to the real
//! container (ADR-0013); where the two disagree, the fake is wrong.

use knobas_source::contract::VecSink;
use knobas_source::{Source, SourceError, SyncItem, WriteOp};
use serde_json::{Value, json};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::support::{Fake, State, TOKEN, issue, source};

const FULL: &str = "tidewater/payout-service";
const OPEN: &str = "gitea:tidewater/payout-service/issues/7";
const CLOSED: &str = "gitea:tidewater/payout-service/issues/3";

async fn run(source: &dyn Source, cursor: Option<&str>) -> (Vec<SyncItem>, String) {
    let mut sink = VecSink(Vec::new());
    let next = source
        .sync(cursor.map(str::to_owned), &mut sink)
        .await
        .expect("the sync runs");
    (sink.0, next)
}

fn tickets(items: &[SyncItem]) -> Vec<&SyncItem> {
    items.iter().filter(|i| i.kind == "ticket").collect()
}

fn ids(items: &[SyncItem]) -> Vec<String> {
    tickets(items)
        .iter()
        .map(|i| i.entity.to_string())
        .collect()
}

#[tokio::test]
async fn a_full_sync_emits_every_issue_open_or_closed_as_a_ticket() {
    let mut state = State::tidewater();
    // The listing serves a pull request too unless asked for `type=issues`,
    // which the fake requires; one that slips through is still no ticket.
    let mut pull = issue(142, "SEPA retry", "open", &[], "2026-08-22T10:20:00Z");
    pull["pull_request"] = json!({ "merged": false });
    state.issues.get_mut(FULL).unwrap().push(pull);
    let fake = Fake::start(&state).await;
    let (items, _) = run(&*source(fake.base_url(), json!({})), None).await;

    assert_eq!(ids(&items), [OPEN, CLOSED]);
    let open = tickets(&items)[0];
    assert_eq!(open.title, "Payout CSV drops the currency column");
    assert!(
        open.body_text.contains("Reproduced on the EUR batch."),
        "the discussion is searchable: {:?}",
        open.body_text
    );
    assert_eq!(open.payload["state"], "open");
    assert_eq!(open.payload["assignee"]["login"], "tom");
    assert_eq!(open.payload["knobas"]["priority"], "High");
    assert_eq!(open.payload["repository"]["full_name"], FULL);
    let closed = tickets(&items)[1];
    assert_eq!(closed.payload["state"], "closed");
    assert_eq!(closed.payload["knobas"]["priority"], Value::Null);
    assert!(!closed.deleted);
}

/// **Who filed an issue and when** (#2403): the ticket page card's
/// *Reporter*, by the creator's login as the assignee is read, and the
/// created date, both through the paths the adapter declares. Gitea keeps no
/// time spent and no worklog, so a synced issue reads neither.
#[tokio::test]
async fn a_synced_issue_reads_its_creator_and_created_and_no_time_spent() {
    use knobas_core::payload::{parse_instant, resolve_number, resolve_string, resolve_worklog};

    let fake = Fake::start(&State::tidewater()).await;
    let source = source(fake.base_url(), json!({}));
    let (items, _) = run(&*source, None).await;
    let descriptor = source.descriptor();
    let paths = descriptor
        .payload_paths
        .iter()
        .find(|p| p.kind == "ticket")
        .expect("the adapter declares its tickets");
    let open = tickets(&items)[0];

    assert_eq!(
        resolve_string(&open.payload, &paths.reporter).as_deref(),
        Some("mara")
    );
    assert_eq!(
        resolve_string(&open.payload, &paths.created)
            .as_deref()
            .and_then(parse_instant)
            .map(|at| at.to_rfc3339()),
        Some("2026-08-20T09:00:00+00:00".to_owned())
    );
    assert_eq!(resolve_number(&open.payload, &paths.time_spent), None);
    assert_eq!(resolve_worklog(&open.payload, &paths.worklog), None);
}

#[tokio::test]
async fn an_idle_run_emits_no_ticket_and_a_changed_issue_comes_back_alone() {
    let mut state = State::tidewater();
    let fake = Fake::start(&state).await;
    let source = source(fake.base_url(), json!({}));
    let (_, cursor) = run(&*source, None).await;

    let (idle, same) = run(&*source, Some(&cursor)).await;
    assert!(idle.is_empty(), "{idle:#?}");
    assert_eq!(same, cursor);

    // Closed upstream, on a later second: it moves state, it is not retired.
    state.edit_issue(FULL, 7, |i| {
        i["state"] = json!("closed");
        i["updated_at"] = json!("2026-08-23T08:00:00Z");
    });
    fake.remount(&state).await;
    let (changed, cursor) = run(&*source, Some(&cursor)).await;
    assert_eq!(ids(&changed), [OPEN]);
    assert_eq!(tickets(&changed)[0].payload["state"], "closed");
    assert!(!tickets(&changed)[0].deleted);

    let (idle, _) = run(&*source, Some(&cursor)).await;
    assert!(idle.is_empty(), "{idle:#?}");
}

/// Gitea's `updated_at` is to the second and `since` is inclusive: an issue
/// changed inside the second the previous run read is still delivered, by
/// its fingerprint, as a pull request is (#764).
#[tokio::test]
async fn an_issue_changed_on_the_watermark_second_is_delivered() {
    let mut state = State::tidewater();
    let fake = Fake::start(&state).await;
    let source = source(fake.base_url(), json!({}));
    let (_, cursor) = run(&*source, None).await;

    state.edit_issue(FULL, 7, |i| i["state"] = json!("closed"));
    fake.remount(&state).await;
    let (changed, _) = run(&*source, Some(&cursor)).await;
    assert_eq!(ids(&changed), [OPEN]);
    assert_eq!(tickets(&changed)[0].payload["state"], "closed");
}

#[tokio::test]
async fn a_new_issue_arrives_with_the_next_incremental_run() {
    let mut state = State::tidewater();
    let fake = Fake::start(&state).await;
    let source = source(fake.base_url(), json!({}));
    let (_, cursor) = run(&*source, None).await;

    state.issues.get_mut(FULL).unwrap().insert(
        0,
        issue(
            150,
            "Nightly payout report",
            "open",
            &["priority/low"],
            "2026-08-24T07:00:00Z",
        ),
    );
    fake.remount(&state).await;
    let (items, _) = run(&*source, Some(&cursor)).await;
    assert_eq!(ids(&items), ["gitea:tidewater/payout-service/issues/150"]);
    assert_eq!(tickets(&items)[0].payload["knobas"]["priority"], "low");
}

/// A token without issue scope (403) or a repository whose issue unit is off
/// (404) costs the tickets of that repository and nothing else, on a full
/// sync too: the code host's kinds still come.
#[tokio::test]
async fn a_refused_issue_listing_skips_the_tickets_and_keeps_the_run() {
    for status in [403, 404] {
        let mut state = State::tidewater();
        state.issues_status.insert(FULL.to_owned(), status);
        let fake = Fake::start(&state).await;
        let (items, _) = run(&*source(fake.base_url(), json!({})), None).await;
        assert!(ids(&items).is_empty(), "{status}");
        assert!(items.iter().any(|i| i.kind == "pr"), "{status}: {items:#?}");
    }
}

/// A revoked credential is the run's, as everywhere (ADR-0004).
#[tokio::test]
async fn a_401_on_the_issue_listing_ends_the_run() {
    let mut state = State::tidewater();
    state.issues_status.insert(FULL.to_owned(), 401);
    let fake = Fake::start(&state).await;
    let mut sink = VecSink(Vec::new());
    let failed = source(fake.base_url(), json!({}))
        .sync(None, &mut sink)
        .await;
    assert!(
        matches!(failed, Err(SourceError::Unauthorized { .. })),
        "{failed:?}"
    );
}

#[tokio::test]
async fn a_repository_with_its_issue_unit_off_is_not_asked() {
    let mut state = State::tidewater();
    state.repos[0]["has_issues"] = json!(false);
    let fake = Fake::start(&state).await;
    let (items, _) = run(&*source(fake.base_url(), json!({})), None).await;
    assert!(ids(&items).is_empty());
    assert!(
        !fake.paths().await.iter().any(|p| p.ends_with("/issues")),
        "{:?}",
        fake.paths().await
    );
}

/// The workflow is Gitea's two states: from either, the other.
#[tokio::test]
async fn an_issues_reachable_transition_is_the_state_it_is_not_in() {
    let fake = Fake::start(&State::tidewater()).await;
    let source = source(fake.base_url(), json!({}));
    assert_eq!(
        source.reachable_transitions(OPEN).await.unwrap(),
        ["closed"]
    );
    assert_eq!(
        source.reachable_transitions(CLOSED).await.unwrap(),
        ["open"]
    );
    let refused = source
        .reachable_transitions("gitea:tidewater/payout-service#142")
        .await;
    assert!(
        matches!(refused, Err(SourceError::Protocol { message: ref m, .. }) if m.contains("not an issue")),
        "{refused:?}"
    );
}

/// A Gitea that takes an issue's writes and records them.
async fn accepting() -> MockServer {
    let server = MockServer::builder().start().await;
    let token = format!("token {TOKEN}");
    let authed = |m: wiremock::MockBuilder| m.and(header("Authorization", token.as_str()));
    authed(Mock::given(method("PATCH")).and(path(format!("/api/v1/repos/{FULL}/issues/7"))))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({ "number": 7 })))
        .mount(&server)
        .await;
    authed(Mock::given(method("POST")).and(path(format!("/api/v1/repos/{FULL}/issues"))))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({ "number": 151 })))
        .mount(&server)
        .await;
    authed(
        Mock::given(method("POST")).and(path(format!("/api/v1/repos/{FULL}/issues/7/comments"))),
    )
    .respond_with(ResponseTemplate::new(201).set_body_json(json!({ "id": 9 })))
    .mount(&server)
    .await;
    server
}

/// The one request `server` received: its method, path and body.
async fn sent(server: &MockServer) -> (String, String, Value) {
    let received = server.received_requests().await.unwrap_or_default();
    let [request] = received.as_slice() else {
        panic!("one request, got {}", received.len())
    };
    (
        request.method.to_string(),
        request.url.path().to_owned(),
        serde_json::from_slice(&request.body).expect("a JSON body"),
    )
}

#[tokio::test]
async fn a_transition_patches_the_issues_state() {
    for (status, state) in [("Closed", "closed"), ("open", "open")] {
        let server = accepting().await;
        source(server.uri(), json!({}))
            .write(WriteOp::Transition {
                entity: OPEN.to_owned(),
                status: status.to_owned(),
            })
            .await
            .expect("Gitea takes the move");
        assert_eq!(
            sent(&server).await,
            (
                "PATCH".to_owned(),
                format!("/api/v1/repos/{FULL}/issues/7"),
                json!({ "state": state })
            )
        );
    }
}

#[tokio::test]
async fn a_ticket_is_created_in_the_repository_and_named_by_its_key() {
    let server = accepting().await;
    let receipt = source(server.uri(), json!({}))
        .write(WriteOp::CreateTicket {
            entity: format!("gitea:{FULL}"),
            title: "Nightly payout report".to_owned(),
            body: "From standup.".to_owned(),
            ticket_type: "Task".to_owned(),
        })
        .await
        .expect("Gitea takes the issue");
    // #1754: the issue's key, the local part of its entity id, so the app
    // takes `gitea:<key>` as the ticket this write made.
    assert_eq!(
        receipt.remote_id.as_deref(),
        Some(format!("{FULL}/issues/151").as_str())
    );
    assert_eq!(
        sent(&server).await,
        (
            "POST".to_owned(),
            format!("/api/v1/repos/{FULL}/issues"),
            json!({ "title": "Nightly payout report", "body": "From standup." })
        )
    );
}

#[tokio::test]
async fn a_comment_on_an_issue_lands_on_its_discussion() {
    let server = accepting().await;
    source(server.uri(), json!({}))
        .write(WriteOp::Comment {
            entity: OPEN.to_owned(),
            body: "Fixed in #142.".to_owned(),
        })
        .await
        .expect("Gitea takes the comment");
    assert_eq!(
        sent(&server).await,
        (
            "POST".to_owned(),
            format!("/api/v1/repos/{FULL}/issues/7/comments"),
            json!({ "body": "Fixed in #142." })
        )
    );
}

// -- a ticket's fields (ADR-0054) ---------------------------------------------

/// A Gitea whose issue 7 carries `assignees` and the labels `payments` (id
/// 11) and `triage` (id 12), whose repository has the labels `sepa` (id 21)
/// and `payments`, and which takes the issue's edits and records them.
async fn editable(assignees: Value) -> MockServer {
    let server = MockServer::builder().start().await;
    let token = format!("token {TOKEN}");
    let authed = |m: wiremock::MockBuilder| m.and(header("Authorization", token.as_str()));
    let mut record = issue(
        7,
        "Payout CSV drops the currency column",
        "open",
        &[],
        "2026-08-22T10:20:00Z",
    );
    record["assignees"] = assignees;
    record["labels"] = json!([{ "id": 11, "name": "payments" }, { "id": 12, "name": "triage" }]);
    authed(Mock::given(method("GET")).and(path(format!("/api/v1/repos/{FULL}/issues/7"))))
        .respond_with(ResponseTemplate::new(200).set_body_json(record))
        .mount(&server)
        .await;
    authed(Mock::given(method("GET")).and(path(format!("/api/v1/repos/{FULL}/labels"))))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(
                json!([{ "id": 21, "name": "sepa" }, { "id": 11, "name": "payments" }]),
            ),
        )
        .mount(&server)
        .await;
    authed(Mock::given(method("GET")).and(path("/api/v1/orgs/tidewater/labels")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&server)
        .await;
    authed(Mock::given(method("GET")).and(path(format!("/api/v1/repos/{FULL}/assignees"))))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            { "login": "tom", "full_name": "" },
            { "login": "mara", "full_name": "Mara Lindqvist" }
        ])))
        .mount(&server)
        .await;
    for verb in ["PATCH", "POST", "DELETE"] {
        authed(Mock::given(method(verb)))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
            .mount(&server)
            .await;
    }
    server
}

/// The writes `server` received, in order, without the reads.
async fn writes(server: &MockServer) -> Vec<(String, String, Value)> {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|r| r.method.as_str() != "GET")
        .map(|r| {
            (
                r.method.to_string(),
                r.url.path().to_owned(),
                serde_json::from_slice(&r.body).unwrap_or(Value::Null),
            )
        })
        .collect()
}

fn edit() -> WriteOp {
    WriteOp::EditTicket {
        entity: OPEN.to_owned(),
        title: None,
        assignee: None,
        priority: None,
        labels: None,
        fields: std::collections::BTreeMap::new(),
    }
}

fn with(change: impl FnOnce(&mut WriteOp)) -> WriteOp {
    let mut op = edit();
    change(&mut op);
    op
}

#[tokio::test]
async fn an_issue_answers_its_title_one_assignee_and_labels_and_a_pull_request_none() {
    let server = editable(json!([{ "login": "mara", "full_name": "Mara Lindqvist" }])).await;
    let source = source(server.uri(), json!({}));
    let fields = source
        .ticket_fields(OPEN)
        .await
        .unwrap()
        .expect("an issue edits");
    assert_eq!(knobas_source::ticket::fault(&fields), None);
    assert_eq!(
        fields.get("title").unwrap().value,
        json!("Payout CSV drops the currency column")
    );
    let who = fields.get("assignee").unwrap();
    assert_eq!(
        (who.value.clone(), who.shown.as_deref()),
        (json!("mara"), Some("Mara Lindqvist"))
    );
    assert_eq!(
        fields.get("labels").unwrap().value,
        json!(["payments", "triage"])
    );
    assert!(
        fields.get("priority").is_none(),
        "Gitea has no priority of its own"
    );
    assert!(
        source
            .ticket_fields(&format!("gitea:{FULL}#142"))
            .await
            .unwrap()
            .is_none(),
        "a pull request is not a ticket"
    );

    let people = source
        .field_options(OPEN, "assignee", "")
        .await
        .unwrap()
        .unwrap();
    assert!(people.complete);
    assert_eq!(people.options[0].label, "Mara Lindqvist");
    assert_eq!(people.options[0].hint.as_deref(), Some("mara"));
    let labels = source
        .field_options(OPEN, "labels", "se")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        labels
            .options
            .iter()
            .map(|o| o.value.as_str())
            .collect::<Vec<_>>(),
        ["sepa"]
    );
    assert!(
        source
            .field_options(OPEN, "due", "")
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn an_issue_with_several_assignees_reads_its_assignee_read_only_and_refuses_one() {
    let server =
        editable(json!([{ "login": "mara" }, { "login": "ben" }, { "login": "olu" }])).await;
    let source = source(server.uri(), json!({}));
    let fields = source.ticket_fields(OPEN).await.unwrap().unwrap();
    assert_eq!(
        fields.get("assignee").unwrap().readonly.as_deref(),
        Some("This issue has 3 assignees. Change them in Tidewater Git.")
    );
    let refused = source
        .write(with(|op| {
            if let WriteOp::EditTicket { assignee, .. } = op {
                *assignee = Some(knobas_source::Assignee::Account("mara".into()));
            }
        }))
        .await;
    assert!(
        matches!(refused, Err(SourceError::Protocol { .. })),
        "{refused:?}"
    );
    assert!(writes(&server).await.is_empty(), "nothing was sent");
}

/// The title and the assignee go in one `PATCH`, and the assignee
/// **replaces** the issue's: a mutant appending it would send the old one too.
#[tokio::test]
async fn a_retitle_and_an_assignee_are_one_patch_that_replaces_the_assignee() {
    let server = editable(json!([{ "login": "tom" }])).await;
    source(server.uri(), json!({}))
        .write(with(|op| {
            if let WriteOp::EditTicket {
                title, assignee, ..
            } = op
            {
                *title = Some("Keep the currency column".into());
                *assignee = Some(knobas_source::Assignee::Account("mara".into()));
            }
        }))
        .await
        .expect("Gitea takes the edit");
    assert_eq!(
        writes(&server).await,
        [(
            "PATCH".to_owned(),
            format!("/api/v1/repos/{FULL}/issues/7"),
            json!({ "title": "Keep the currency column", "assignees": ["mara"] })
        )]
    );
    let server = editable(json!([{ "login": "tom" }])).await;
    source(server.uri(), json!({}))
        .write(with(|op| {
            if let WriteOp::EditTicket { assignee, .. } = op {
                *assignee = Some(knobas_source::Assignee::Nobody);
            }
        }))
        .await
        .expect("Gitea takes the unassign");
    assert_eq!(writes(&server).await[0].2, json!({ "assignees": [] }));
}

/// The due date (a source's own field, ADR-0054 step 10) rides the issue's
/// one `PATCH`: a day at its start in UTC, or cleared by `unset_due_date`.
#[tokio::test]
async fn a_due_date_is_set_at_the_days_start_and_cleared_in_the_issues_patch() {
    let server = editable(json!([{ "login": "tom" }])).await;
    source(server.uri(), json!({}))
        .write(with(|op| {
            if let WriteOp::EditTicket { fields, .. } = op {
                fields.insert("due_date".into(), json!("2026-12-01"));
            }
        }))
        .await
        .expect("Gitea takes the due date");
    assert_eq!(
        writes(&server).await,
        [(
            "PATCH".to_owned(),
            format!("/api/v1/repos/{FULL}/issues/7"),
            json!({ "due_date": "2026-12-01T00:00:00Z" })
        )]
    );
    let server = editable(json!([{ "login": "tom" }])).await;
    source(server.uri(), json!({}))
        .write(with(|op| {
            if let WriteOp::EditTicket { fields, .. } = op {
                fields.insert("due_date".into(), Value::Null);
            }
        }))
        .await
        .expect("Gitea clears the due date");
    assert_eq!(
        writes(&server).await[0].2,
        json!({ "unset_due_date": true })
    );
}

/// A label is added by its id, never its name, and removed by the id the
/// issue carries it under; a name added that it carries, or removed that it
/// does not, sends nothing.
#[tokio::test]
async fn labels_are_added_and_removed_by_id_and_only_where_they_change() {
    let server = editable(Value::Null).await;
    source(server.uri(), json!({}))
        .write(with(|op| {
            if let WriteOp::EditTicket { labels, .. } = op {
                *labels = Some(knobas_source::LabelChange {
                    add: vec!["sepa".into(), "payments".into()],
                    remove: vec!["triage".into(), "nowhere".into()],
                });
            }
        }))
        .await
        .expect("Gitea takes the labels");
    assert_eq!(
        writes(&server).await,
        [
            (
                "POST".to_owned(),
                format!("/api/v1/repos/{FULL}/issues/7/labels"),
                json!({ "labels": [21] })
            ),
            (
                "DELETE".to_owned(),
                format!("/api/v1/repos/{FULL}/issues/7/labels/12"),
                Value::Null
            ),
        ]
    );
}

#[tokio::test]
async fn what_a_gitea_issue_cannot_take_is_refused_by_name_before_a_request() {
    let server = editable(Value::Null).await;
    let source = source(server.uri(), json!({}));
    for (op, words) in [
        (edit(), "nothing to change"),
        (
            with(|op| {
                if let WriteOp::EditTicket { priority, .. } = op {
                    *priority = Some("High".into());
                }
            }),
            "priority",
        ),
        (
            with(|op| {
                if let WriteOp::EditTicket { fields, .. } = op {
                    fields.insert("due".into(), json!("2026-12-01"));
                }
            }),
            "\"due\"",
        ),
    ] {
        let refused = source.write(op).await;
        assert!(
            matches!(&refused, Err(SourceError::Protocol { message, status: None }) if message.contains(words)),
            "{refused:?}"
        );
    }
    let unknown = source
        .write(with(|op| {
            if let WriteOp::EditTicket { labels, .. } = op {
                *labels = Some(knobas_source::LabelChange {
                    add: vec!["nonesuch".into()],
                    remove: vec![],
                });
            }
        }))
        .await;
    assert!(
        matches!(&unknown, Err(SourceError::Protocol { message, .. }) if message.contains("\"nonesuch\"")),
        "{unknown:?}"
    );
    assert!(writes(&server).await.is_empty(), "nothing was sent");
}

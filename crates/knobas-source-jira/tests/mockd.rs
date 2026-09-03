//! The Jira adapter against the HTTP-level mock (roadmap §3: `knobas-mockd`
//! in-process, no Docker, deterministic).
//!
//! Every test ends with `assert_no_violations()`. That is the point of the mock
//! being validated against the vendored WADL: an adapter that invents an
//! endpoint, a verb or a query parameter fails *its own* suite instead of
//! passing against something lenient and then failing against a real Jira.
//!
//! Two things about mockd shape the tests below and are worth stating once:
//!
//! * **mockd accepts any non-empty `Bearer`/`Basic` credential.** A "wrong
//!   token" is therefore not a 401 -- rejection is what
//!   [`MockFault::Unauthorized`] is for. A blank one is worse than useless: it
//!   is recorded as a `MissingHeader` violation, so it certifies nothing. The
//!   unauthorized cases here run against their own server with that fault set.
//! * **The server is on `+02:00`, not UTC**, and reads JQL date literals in
//!   that zone. Every incremental test below is therefore also a test that the
//!   watermark is rendered in the server's zone.

use knobas_mockd::{MockFault, spawn_mock_jira};
use knobas_source::contract::{Fault, VecSink, battery};
use knobas_source::instance::SourceInstance;
use knobas_source::{AuthMethod, Source, SourceError, WriteOp};

fn instance(base_url: &str, secret: &str, config: serde_json::Value) -> SourceInstance {
    SourceInstance {
        id: "jira".to_owned(),
        kind: "jira".to_owned(),
        display_name: "Tidewater Jira".to_owned(),
        base_url: base_url.to_owned(),
        auth: Some(AuthMethod::Pat),
        secret: Some(secret.to_owned()),
        config,
    }
}

fn source(base_url: &str, config: serde_json::Value) -> Box<dyn Source> {
    source_named("jira", base_url, config)
}

fn source_named(id: &str, base_url: &str, config: serde_json::Value) -> Box<dyn Source> {
    let mut i = instance(base_url, knobas_mockd::JIRA_TOKEN, config);
    i.id = id.to_owned();
    match knobas_source_jira::build(i) {
        Ok(s) => s,
        // `Box<dyn Source>` is not `Debug`, so `expect` is unavailable.
        Err(e) => panic!("the adapter must build against mockd: {e:?}"),
    }
}

async fn sync_all(source: &dyn Source, cursor: Option<String>) -> (Vec<String>, String) {
    let mut sink = VecSink(Vec::new());
    let next = source.sync(cursor, &mut sink).await.expect("sync succeeds");
    (sink.0.iter().map(|i| i.entity.key.clone()).collect(), next)
}

/// The shared suite every adapter must pass.
///
/// The unauthorized case gets **its own server** with the fault set, rather
/// than a wrong token against the shared one: mockd accepts any non-empty
/// credential, and the battery keeps the healthy adapter alive and calls it
/// again after building the faulted ones (clauses 5 and 6), so a fault on the
/// shared server would make those later calls fail with the wrong error.
#[tokio::test]
async fn passes_the_contract_battery() {
    let jira = spawn_mock_jira().await;
    let denied = spawn_mock_jira().await;
    denied.set_fault(MockFault::Unauthorized);
    let base = jira.base_url();
    let denied_base = denied.base_url();
    let dead = knobas_mockd::refused_url();
    battery(move |fault| match fault {
        Fault::None => source(&base, serde_json::json!({})),
        Fault::Unauthorized => source(&denied_base, serde_json::json!({})),
        Fault::Unreachable => source(&dead, serde_json::json!({})),
    })
    .await;
    jira.assert_no_violations();
    denied.assert_no_violations();
}

#[tokio::test]
async fn a_full_sync_lands_the_fixture_issues() {
    let jira = spawn_mock_jira().await;
    let source = source(&jira.base_url(), serde_json::json!({}));
    let mut sink = VecSink(Vec::new());
    source.sync(None, &mut sink).await.unwrap();

    // The Tidewater fixture's tickets: PAY-200/231/228/240/219/236 and OPS-77.
    assert_eq!(
        sink.0.len(),
        7,
        "expected the fixture's seven issues, got {}",
        sink.0.len()
    );
    let pay231 = sink
        .0
        .iter()
        .find(|i| i.entity.key == "PAY-231")
        .expect("PAY-231 is in the fixture");
    assert_eq!(pay231.entity.to_string(), "jira:PAY-231");
    assert_eq!(pay231.title, "Retry failed SEPA payouts");
    assert_eq!(pay231.kind, "ticket");
    assert!(
        pay231
            .web_url
            .as_deref()
            .expect("P5: the adapter can say where a human reads this")
            .ends_with("/browse/PAY-231")
    );
    assert!(pay231.updated_at.is_some());
    // §3a: the raw record is kept, so a later mapping can re-project it.
    assert_eq!(pay231.payload["key"], "PAY-231");
    // `fields=comment,worklog` is honoured, so the discussion arrives with the
    // page rather than costing a request per issue -- and it reaches `body_text`,
    // which is what FTS indexes.
    assert_eq!(
        pay231.payload["fields"]["comment"]["comments"]
            .as_array()
            .expect("the comment container came back")
            .len(),
        2
    );
    assert_eq!(
        pay231.payload["fields"]["worklog"]["worklogs"]
            .as_array()
            .expect("the worklog container came back")
            .len(),
        1
    );
    for item in &sink.0 {
        assert_eq!(item.entity.namespace, "jira");
        assert!(!item.deleted, "jira search cannot report deletions");
    }
    jira.assert_no_violations();
}

/// Exit criterion: pagination across >= 2 pages, i.e. `maxResults` < `total`.
///
/// The `ticket` kind claims `full_sync_exhaustive: true`, and the engine
/// tombstones every row of such a kind that a full sync did not return -- so a
/// run that stopped
/// after page one would delete live issues from the mirror, not merely sync
/// fewer of them.
#[tokio::test]
async fn pages_across_more_than_one_response() {
    let jira = spawn_mock_jira().await;
    let paged = source(&jira.base_url(), serde_json::json!({ "page_size": 2 }));
    let single = source(&jira.base_url(), serde_json::json!({}));
    let (mut a, _) = sync_all(paged.as_ref(), None).await;
    let (mut b, _) = sync_all(single.as_ref(), None).await;
    a.sort();
    b.sort();
    assert!(a.len() > 2, "the fixture must not fit in one 2-issue page");
    assert_eq!(a, b, "paging must not change which issues arrive");
    jira.assert_no_violations();
}

/// Exit criterion: `touch_issue` ⇒ the next incremental returns exactly that
/// issue and the watermark advances.
#[tokio::test]
async fn an_incremental_run_returns_exactly_what_changed() {
    let jira = spawn_mock_jira().await;
    let source = source(&jira.base_url(), serde_json::json!({}));
    let (_, cursor) = sync_all(source.as_ref(), None).await;

    jira.touch_issue("PAY-231");
    let (keys, advanced) = sync_all(source.as_ref(), Some(cursor.clone())).await;
    assert_eq!(keys, vec!["PAY-231".to_owned()]);
    assert_ne!(
        advanced, cursor,
        "the watermark must move when an item was delivered"
    );
    jira.assert_no_violations();
}

/// Exit criterion, and battery clause 2 against the real wire: an idle
/// incremental returns the same cursor and zero items.
///
/// Necessary, and on its own **not** sufficient -- see
/// [`one_new_issue_then_an_idle_poll_settles`](fn@one_new_issue_then_an_idle_poll_settles).
#[tokio::test]
async fn an_idle_incremental_returns_nothing_and_the_same_cursor() {
    let jira = spawn_mock_jira().await;
    let source = source(&jira.base_url(), serde_json::json!({}));
    let (_, cursor) = sync_all(source.as_ref(), None).await;
    let (keys, again) = sync_all(source.as_ref(), Some(cursor.clone())).await;
    assert!(keys.is_empty(), "{keys:?}");
    assert_eq!(again, cursor);
    jira.assert_no_violations();
}

/// Cursor discipline as the scheduler actually exercises it: **one new issue,
/// then an idle poll**, twice over.
///
/// Full-sync-then-idle is stable even when the run tells the cursor only what
/// it *emitted*, because a full sync skips nothing. This sequence is not: the
/// third run skips PAY-240 as already delivered, and if that pair drops out of
/// `seen`, the fourth run -- whose two-minute window still reaches over it --
/// emits it again, and every poll after that emits something forever. The
/// watermark holds perfectly throughout, so nothing watching the cursor's
/// timestamp can see it happening.
///
/// mockd's clock advances exactly one minute per `touch_issue`, which is what
/// keeps both touched issues inside the same overlap window.
#[tokio::test]
async fn one_new_issue_then_an_idle_poll_settles() {
    let jira = spawn_mock_jira().await;
    let source = source(&jira.base_url(), serde_json::json!({}));

    let (full, c1) = sync_all(source.as_ref(), None).await;
    assert_eq!(full.len(), 7);

    jira.touch_issue("PAY-240");
    let (second, c2) = sync_all(source.as_ref(), Some(c1)).await;
    assert_eq!(second, vec!["PAY-240".to_owned()]);

    jira.touch_issue("PAY-228");
    let (third, c3) = sync_all(source.as_ref(), Some(c2)).await;
    assert_eq!(third, vec!["PAY-228".to_owned()]);

    let (idle, c4) = sync_all(source.as_ref(), Some(c3.clone())).await;
    assert!(
        idle.is_empty(),
        "an issue skipped as already-delivered must stay in the cursor's `seen` set; \
         re-emitted: {idle:?}"
    );
    assert_eq!(
        c4, c3,
        "an idle poll must hand back the cursor it was given"
    );
    jira.assert_no_violations();
}

/// Exit criterion: 401 ⇒ `SourceError::Unauthorized`, from both entry points --
/// and the case stream F's credential-health path is built for, a credential
/// that worked when the source was added and stopped working later.
#[tokio::test]
async fn a_rejected_credential_is_unauthorized_from_both_entry_points() {
    let jira = spawn_mock_jira().await;
    let source = source(&jira.base_url(), serde_json::json!({}));
    source
        .test_connection()
        .await
        .expect("connects before the fault");

    jira.set_fault(MockFault::Unauthorized);
    assert!(matches!(
        source.test_connection().await,
        Err(SourceError::Unauthorized { .. })
    ));
    let synced = source.sync(None, &mut VecSink(Vec::new())).await;
    assert!(
        matches!(synced, Err(SourceError::Unauthorized { .. })),
        "{synced:?}"
    );
    jira.assert_no_violations();
}

/// Exit criterion: timeout ⇒ `Unreachable`. A one-second request timeout, so
/// the suite does not wait out the 30-second production default.
#[tokio::test]
async fn a_timeout_is_unreachable() {
    let jira = spawn_mock_jira().await;
    jira.set_fault(MockFault::Timeout { hang_ms: 2_000 });
    let source = source(
        &jira.base_url(),
        serde_json::json!({ "request_timeout_secs": 1 }),
    );
    let connected = source.test_connection().await;
    assert!(
        matches!(connected, Err(SourceError::Unreachable(_))),
        "{connected:?}"
    );
    jira.assert_no_violations();
}

// -- M2's write-back set (issue #43) ----------------------------------------
//
// Every one of these asserts what the *far end received* -- the fixture after
// the write -- rather than what the adapter built. `assert_no_violations()` is
// what proves the request was one the WADL declares: a path, verb or query
// parameter the contract does not have is recorded rather than answered.

/// Story 3: replying to a ticket puts the reply on the ticket.
///
/// The assertion is the comment's **text and author** in mockd's own state, not
/// a 201: a request that reached the right path with an empty body would also
/// be a 201 from a server less strict than this one.
#[tokio::test]
async fn a_comment_reaches_the_ticket() {
    let jira = spawn_mock_jira().await;
    let source = source(&jira.base_url(), serde_json::json!({}));
    let before = jira.state().issue("PAY-231").expect("in the fixture");

    source
        .write(WriteOp::Comment {
            entity: "jira:PAY-231".to_owned(),
            body: "picking this up now".to_owned(),
        })
        .await
        .expect("a declared op is performed");

    let after = jira.state().issue("PAY-231").expect("in the fixture");
    assert_eq!(after.comments.len(), before.comments.len() + 1);
    let posted = after.comments.last().expect("the comment just added");
    assert_eq!(posted.body, "picking this up now");
    assert_eq!(
        posted.author, "mara.lindqvist",
        "story 17: the source attributes the write to the credential's own account"
    );
    jira.assert_no_violations();
}

/// M3.1 (#280): logging work puts a worklog on the ticket, and the id Jira
/// gave it comes back on the receipt.
///
/// Three claims, and each of them is a different way the write could be
/// reported as done while being wrong:
///
/// * the worklog is **on the ticket**, with the seconds and the comment that
///   were asked for -- read out of mockd's own state, not out of a 201;
/// * `started` survives the hop, which is the field with a format rather than a
///   value: mockd parses it with Jira's own pattern and answers 400 for `Z` or
///   for a missing millisecond field, so this passing is the wire format being
///   right;
/// * the **receipt carries the id**. `WriteReceipt::none()` would leave a local
///   copy that can never name what it stands for, and nothing else in knobas
///   would notice.
#[tokio::test]
async fn logging_work_puts_a_worklog_on_the_ticket_and_names_it() {
    let jira = spawn_mock_jira().await;
    let source = source(&jira.base_url(), serde_json::json!({}));
    let before = jira.state().issue("PAY-231").expect("in the fixture");
    let started = chrono::DateTime::parse_from_rfc3339("2026-09-03T09:30:00Z")
        .expect("a fixed instant")
        .with_timezone(&chrono::Utc);

    let receipt = source
        .write(WriteOp::LogWork {
            entity: "jira:PAY-231".to_owned(),
            started,
            seconds: 2_700,
            comment: "- Retry SEPA payouts".to_owned(),
        })
        .await
        .expect("a declared op is performed");

    let after = jira.state().issue("PAY-231").expect("in the fixture");
    assert_eq!(after.worklogs.len(), before.worklogs.len() + 1);
    let logged = after.worklogs.last().expect("the worklog just added");
    assert_eq!(logged.time_spent_seconds, 2_700);
    assert_eq!(logged.comment, "- Retry SEPA payouts");
    assert_eq!(
        logged.started, started,
        "the instant the work began is the one field of a worklog that has a \
         format rather than a value, and Jira refuses every spelling but its own"
    );
    assert_eq!(
        logged.author, "mara.lindqvist",
        "story 17: the source attributes the write to the credential's own account"
    );
    assert_eq!(
        receipt.remote_id.as_deref(),
        Some(logged.id.to_string().as_str()),
        "the receipt is the only way the local copy can ever name the worklog \
         Jira just made"
    );
    jira.assert_no_violations();
}

/// Story 1: a ticket moves, and the mirror's source of truth says so.
///
/// PAY-231 is `In Progress`, whose workflow offers `In Review`.
#[tokio::test]
async fn a_transition_moves_the_ticket() {
    let jira = spawn_mock_jira().await;
    let source = source(&jira.base_url(), serde_json::json!({}));
    assert_eq!(
        jira.state()
            .issue("PAY-231")
            .expect("in the fixture")
            .status,
        "In Progress"
    );

    source
        .write(WriteOp::Transition {
            entity: "jira:PAY-231".to_owned(),
            status: "In Review".to_owned(),
        })
        .await
        .expect("a status the workflow offers");

    assert_eq!(
        jira.state()
            .issue("PAY-231")
            .expect("in the fixture")
            .status,
        "In Review"
    );
    jira.assert_no_violations();
}

/// Story 2, from the other side: the workflow is **read**, and a status it does
/// not offer is refused by name with what it does.
///
/// PAY-231 is `In Progress`, from which this workflow reaches `In Review` and
/// `To Do` but not `Done`. An adapter that guessed a transition id, or that
/// posted the status name as if it were one, would move the ticket somewhere
/// or fail with the server's words instead of its own -- so the assertion is
/// on the message *and* on the ticket not having moved.
#[tokio::test]
async fn a_status_the_workflow_does_not_offer_is_refused_by_name() {
    let jira = spawn_mock_jira().await;
    let source = source(&jira.base_url(), serde_json::json!({}));

    let refused = source
        .write(WriteOp::Transition {
            entity: "jira:PAY-231".to_owned(),
            status: "Done".to_owned(),
        })
        .await;

    let Err(SourceError::Protocol { message, .. }) = &refused else {
        panic!("an unreachable status must be refused, got {refused:?}");
    };
    assert!(message.contains("Done"), "{message}");
    assert!(
        message.contains("In Review"),
        "the refusal must say what the workflow does offer: {message}"
    );
    assert_eq!(
        jira.state()
            .issue("PAY-231")
            .expect("in the fixture")
            .status,
        "In Progress",
        "a refused transition must not have moved anything"
    );
    jira.assert_no_violations();
}

/// The status the user picked comes back through a payload a person may have
/// typed, so the match is trimmed and case-insensitive rather than byte-equal.
#[tokio::test]
async fn a_status_matches_however_the_user_spelled_it() {
    let jira = spawn_mock_jira().await;
    let source = source(&jira.base_url(), serde_json::json!({}));
    source
        .write(WriteOp::Transition {
            entity: "jira:PAY-231".to_owned(),
            status: "  in review ".to_owned(),
        })
        .await
        .expect("the same status, spelled by a human");
    assert_eq!(
        jira.state()
            .issue("PAY-231")
            .expect("in the fixture")
            .status,
        "In Review"
    );
    jira.assert_no_violations();
}

/// Story 4: capturing work is one action, and the ticket exists afterwards.
///
/// The target is the **project** (`jira:PAY`), a container knobas does not
/// mirror -- see `knobas_core::write_queue::project`.
#[tokio::test]
async fn a_create_files_a_new_ticket_in_the_project() {
    let jira = spawn_mock_jira().await;
    let source = source(&jira.base_url(), serde_json::json!({}));
    let before = jira.state().issues().len();

    source
        .write(WriteOp::CreateTicket {
            entity: "jira:PAY".to_owned(),
            title: "SEPA retries need a dead-letter queue".to_owned(),
            body: "the batch job times out and the payouts are lost".to_owned(),
            ticket_type: "Task".to_owned(),
        })
        .await
        .expect("a create in a project the fixture has");

    let issues = jira.state().issues();
    assert_eq!(issues.len(), before + 1);
    let created = issues.last().expect("the ticket just created");
    assert_eq!(created.project, "PAY");
    assert_eq!(created.summary, "SEPA retries need a dead-letter queue");
    assert_eq!(
        created.description.as_deref(),
        Some("the batch job times out and the payouts are lost"),
        "a create that dropped the description would be reported as a success"
    );
    assert_eq!(created.issue_type, "Task");
    assert_eq!(
        created.reporter, "mara.lindqvist",
        "story 17: attributed to me"
    );
    jira.assert_no_violations();
}

/// The three fields Jira requires all reach it: dropping any one is a 400,
/// which the queue reads as a **refusal** and does not retry. Asserted one at a
/// time against the server's own validation, because a test that only checked
/// the path would pass for a body that carried none of them.
#[tokio::test]
async fn a_create_missing_what_jira_requires_is_refused_by_the_server() {
    let jira = spawn_mock_jira().await;
    let source = source(&jira.base_url(), serde_json::json!({}));
    let before = jira.state().issues().len();

    for (entity, ticket_type, why) in [
        ("jira:NOPE", "Task", "a project the fixture does not have"),
        ("jira:PAY", "", "no issue type"),
    ] {
        let refused = source
            .write(WriteOp::CreateTicket {
                entity: entity.to_owned(),
                title: "a title".to_owned(),
                body: String::new(),
                ticket_type: ticket_type.to_owned(),
            })
            .await;
        assert!(
            matches!(
                refused,
                Err(SourceError::Protocol {
                    status: Some(400),
                    ..
                })
            ),
            "{why}: {refused:?}"
        );
    }
    assert_eq!(
        jira.state().issues().len(),
        before,
        "nothing was created by a refused create"
    );
    jira.assert_no_violations();
}

/// A create with no description omits the field rather than sending `""`: a
/// Jira whose create screen does not carry description rejects the whole
/// request for naming it.
#[tokio::test]
async fn a_create_with_no_body_files_a_ticket_with_no_description() {
    let jira = spawn_mock_jira().await;
    let source = source(&jira.base_url(), serde_json::json!({}));
    source
        .write(WriteOp::CreateTicket {
            entity: "jira:OPS".to_owned(),
            title: "rotate the staging PAT".to_owned(),
            body: "   ".to_owned(),
            ticket_type: "Task".to_owned(),
        })
        .await
        .expect("a create with nothing in the body");
    let created = jira
        .state()
        .issues()
        .into_iter()
        .next_back()
        .expect("the ticket just created");
    assert_eq!(created.project, "OPS");
    assert!(created.description.is_none(), "{:?}", created.description);
    jira.assert_no_violations();
}

/// An op this adapter does not declare must be refused **without reaching the
/// server**, which is battery clause 5's promise seen from the far end: the
/// fixture is unchanged and mockd recorded nothing.
#[tokio::test]
async fn an_undeclared_op_never_reaches_the_server() {
    let jira = spawn_mock_jira().await;
    let source = source(&jira.base_url(), serde_json::json!({}));
    let before = jira.state().issues();

    let refused = source
        .write(WriteOp::Approve {
            entity: "jira:PAY-231".to_owned(),
            body: String::new(),
        })
        .await;
    assert!(
        matches!(refused, Err(SourceError::Protocol { .. })),
        "{refused:?}"
    );
    let after = jira.state().issues();
    assert_eq!(before.len(), after.len());
    assert!(
        before
            .iter()
            .zip(&after)
            .all(|(b, a)| b.status == a.status && b.comments.len() == a.comments.len()),
        "an undeclared op must not have touched anything"
    );
    jira.assert_no_violations();
}

/// ADR-0004, on the write path: a source that is down is `Unreachable` -- which
/// the queue waits on -- and not a refusal, which it would not retry.
#[tokio::test]
async fn a_write_to_an_unreachable_jira_waits_rather_than_being_refused() {
    let jira = spawn_mock_jira().await;
    jira.set_fault(MockFault::Timeout { hang_ms: 2_000 });
    let source = source(
        &jira.base_url(),
        serde_json::json!({ "request_timeout_secs": 1 }),
    );
    let failed = source
        .write(WriteOp::Comment {
            entity: "jira:PAY-231".to_owned(),
            body: "into the void".to_owned(),
        })
        .await;
    assert!(
        matches!(failed, Err(SourceError::Unreachable(_))),
        "{failed:?}"
    );
}

/// The same for a credential the server rejects: `Unauthorized`, so the queue
/// waits for a human to re-enter one rather than discarding what they typed.
#[tokio::test]
async fn a_write_with_a_rejected_credential_waits_for_a_human() {
    let jira = spawn_mock_jira().await;
    jira.set_fault(MockFault::Unauthorized);
    let source = source(&jira.base_url(), serde_json::json!({}));
    let failed = source
        .write(WriteOp::Comment {
            entity: "jira:PAY-231".to_owned(),
            body: "not with this token".to_owned(),
        })
        .await;
    assert!(
        matches!(failed, Err(SourceError::Unauthorized { .. })),
        "{failed:?}"
    );
}

/// P4: the Add-source flow shows who it connected as and which server answered.
#[tokio::test]
async fn test_connection_reports_the_account_and_the_server() {
    let jira = spawn_mock_jira().await;
    let source = source(&jira.base_url(), serde_json::json!({}));
    let info = source.test_connection().await.unwrap();
    assert_eq!(info.account.as_deref(), Some("mara.lindqvist"), "{info:?}");
    assert_eq!(info.server_version.as_deref(), Some("9.17.0"), "{info:?}");
    assert_eq!(info.detail.as_deref(), Some("Server 9.17.0"), "{info:?}");
    // PAT expiry needs /rest/pat/latest/tokens, which is outside M1's endpoints.
    assert!(info.secret_expires_at.is_none());
    jira.assert_no_violations();
}

/// P10: the `EntityRef` namespace is the **instance** id, not the adapter kind.
///
/// Every other test here uses an instance called `jira`, which is also
/// `ADAPTER_KIND` -- so the two are indistinguishable and a `sync` wired to the
/// constant would pass all of them. Two Jiras are `jira` and `jira-eu`, and
/// their items must not collide in the mirror.
#[tokio::test]
async fn a_second_instance_of_the_same_jira_carries_its_own_namespace() {
    let jira = spawn_mock_jira().await;
    let eu = source_named("jira-eu", &jira.base_url(), serde_json::json!({}));
    assert_eq!(eu.descriptor().id, "jira-eu");
    let mut sink = VecSink(Vec::new());
    eu.sync(None, &mut sink).await.expect("sync succeeds");
    assert!(!sink.0.is_empty());
    for item in &sink.0 {
        assert_eq!(
            item.entity.namespace, "jira-eu",
            "{} is namespaced to the adapter kind, not to the instance",
            item.entity
        );
    }
    assert!(
        sink.0
            .iter()
            .any(|i| i.entity.to_string() == "jira-eu:PAY-231")
    );
    jira.assert_no_violations();
}

/// Scoping by project must reach the server as JQL, not as a client-side
/// filter: a 50,000-issue instance cannot be filtered locally.
///
/// The unscoped run is the control -- without it, an adapter that syncs nothing
/// at all would pass the "every key starts with PAY-" half.
#[tokio::test]
async fn a_project_scope_is_pushed_into_the_jql() {
    let jira = spawn_mock_jira().await;
    let scoped = source(&jira.base_url(), serde_json::json!({ "projects": ["PAY"] }));
    let all = source(&jira.base_url(), serde_json::json!({}));
    let (keys, _) = sync_all(scoped.as_ref(), None).await;
    let (every, _) = sync_all(all.as_ref(), None).await;
    assert!(!keys.is_empty());
    assert!(keys.iter().all(|k| k.starts_with("PAY-")), "{keys:?}");
    assert!(
        every.iter().any(|k| k.starts_with("OPS-")),
        "the unscoped run must see the project the scoped one filtered out: {every:?}"
    );
    assert_eq!(keys.len(), every.len() - 1, "{keys:?} vs {every:?}");
    jira.assert_no_violations();
}

// -- the widened payload (issue #32) ----------------------------------------

/// `payload` is the product, not a by-product.
///
/// Spec §3a keeps the raw record so a later mapping can re-project it without
/// re-syncing, and the §3a generic detail view renders straight out of it. So
/// a name missing from `BASE_FIELDS` is not merely a field the *mapping* does
/// not read today -- it is data the mirror will never hold, and the only way
/// to get it is a re-sync of every issue. Which is why this is asserted
/// against the wire rather than against the constant: `assert_no_violations`
/// is what certifies the widened query is one a real Jira would accept, and
/// the constant on its own certifies nothing.
///
/// Both branches of every field, from the fixture, so an adapter that hard-
/// coded any of them fails here.
#[tokio::test]
async fn a_synced_issue_carries_epic_membership_links_and_resolution() {
    let jira = spawn_mock_jira().await;
    let source = source(&jira.base_url(), serde_json::json!({}));
    let mut sink = VecSink(Vec::new());
    source.sync(None, &mut sink).await.unwrap();
    let payload = |key: &str| {
        sink.0
            .iter()
            .find(|i| i.entity.key == key)
            .unwrap_or_else(|| panic!("{key} is in the fixture"))
            .payload
            .clone()
    };

    // Epic membership -- what the Contexts work reads, and the whole reason
    // this widening is ordered before it.
    assert_eq!(payload("PAY-231")["fields"]["parent"]["key"], "PAY-200");
    assert_eq!(
        payload("PAY-231")["fields"]["parent"]["fields"]["issuetype"]["name"],
        "Epic"
    );
    assert!(
        payload("OPS-77")["fields"].get("parent").is_none(),
        "an issue with no epic must not acquire one"
    );

    // Links, at both ends.
    let blocked = payload("PAY-228");
    assert_eq!(blocked["fields"]["issuelinks"][0]["type"]["name"], "Blocks");
    assert_eq!(
        blocked["fields"]["issuelinks"][0]["inwardIssue"]["key"],
        "OPS-77"
    );
    assert_eq!(
        payload("OPS-77")["fields"]["issuelinks"][0]["outwardIssue"]["key"],
        "PAY-228"
    );
    assert_eq!(
        payload("PAY-240")["fields"]["issuelinks"]
            .as_array()
            .expect("the container is served even when empty")
            .len(),
        0
    );

    // Resolution, labels, and the two time fields.
    assert_eq!(payload("PAY-219")["fields"]["resolution"]["name"], "Done");
    assert!(payload("PAY-231")["fields"]["resolution"].is_null());
    assert!(payload("PAY-231")["fields"]["labels"].is_array());
    assert_eq!(payload("PAY-231")["fields"]["timeoriginalestimate"], 57_600);
    assert_eq!(payload("PAY-231")["fields"]["timespent"], 16_200);
    assert!(payload("PAY-240")["fields"]["timespent"].is_null());

    jira.assert_no_violations();
}

/// The same payload, through the *incremental* path.
///
/// `/search` is one query and every run sends the same `fields=`, so this
/// cannot realistically diverge -- but it is what the backfill's premise
/// rests on, and stating it here is what makes the premise checked rather
/// than assumed: an incremental run delivers the widened payload for the
/// issues it returns, and returns **only** those. Everything else keeps
/// whatever the mirror last stored, which is what the cursor-less backfill
/// exists to repair.
#[tokio::test]
async fn an_incremental_run_delivers_the_widened_payload_for_what_it_returns() {
    let jira = spawn_mock_jira().await;
    let source = source(&jira.base_url(), serde_json::json!({}));
    let (_, cursor) = sync_all(source.as_ref(), None).await;

    jira.touch_issue("PAY-228");
    let mut sink = VecSink(Vec::new());
    source.sync(Some(cursor), &mut sink).await.unwrap();

    assert_eq!(sink.0.len(), 1, "only the touched issue comes back");
    assert_eq!(sink.0[0].entity.key, "PAY-228");
    assert_eq!(
        sink.0[0].payload["fields"]["issuelinks"][0]["inwardIssue"]["key"], "OPS-77",
        "an incremental run sends the same fields= as a full one"
    );
    assert_eq!(sink.0[0].payload["fields"]["parent"]["key"], "PAY-200");
    jira.assert_no_violations();
}

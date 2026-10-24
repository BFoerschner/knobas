//! `refused_writes` against a stub that answers `mypermissions` (#2052).
//!
//! `knobas-mockd` serves no `GET /rest/api/2/mypermissions` and is frozen
//! (ADR-0013), so this is a transport test in the shape of
//! `field_discovery.rs`: canned bytes on a socket, plus a record of every
//! request line, so the tests can say what was sent and that nothing was
//! written to find out. The answers' shape is what the seeded Jira 10.3.24
//! answered (#2052); which permissions are lacking is the test's own. The
//! real product is asked in
//! `live_jira_seeded::a_user_the_permission_scheme_refuses_is_refused_those_writes_and_nothing_is_written`.

use std::sync::{Arc, Mutex};

use knobas_source::grants::{RefusedWrite, WriteAsk};
use knobas_source::instance::SourceInstance;
use knobas_source::{AuthMethod, Source};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// PAY's answer for a user who may move and file tickets but neither comment
/// nor log work there.
const PAY: &str = r#"{"permissions":{
  "ADD_COMMENTS":{"id":"15","key":"ADD_COMMENTS","name":"Add Comments","type":"PROJECT","description":"Ability to comment on issues.","havePermission":false},
  "TRANSITION_ISSUES":{"id":"46","key":"TRANSITION_ISSUES","name":"Transition Issues","type":"PROJECT","description":"Ability to transition issues.","havePermission":true},
  "WORK_ON_ISSUES":{"id":"20","key":"WORK_ON_ISSUES","name":"Work On Issues","type":"PROJECT","description":"Ability to log work done against an issue.","havePermission":false},
  "CREATE_ISSUES":{"id":"11","key":"CREATE_ISSUES","name":"Create Issues","type":"PROJECT","description":"Ability to create issues.","havePermission":true},
  "BROWSE_PROJECTS":{"id":"10","key":"BROWSE_PROJECTS","name":"Browse Projects","type":"PROJECT","description":"Ability to browse projects.","havePermission":true}}}"#;

/// LEDG's: nothing but browsing.
const LEDG: &str = r#"{"permissions":{
  "ADD_COMMENTS":{"key":"ADD_COMMENTS","havePermission":false},
  "TRANSITION_ISSUES":{"key":"TRANSITION_ISSUES","havePermission":false},
  "WORK_ON_ISSUES":{"key":"WORK_ON_ISSUES","havePermission":false},
  "CREATE_ISSUES":{"key":"CREATE_ISSUES","havePermission":false},
  "BROWSE_PROJECTS":{"key":"BROWSE_PROJECTS","havePermission":true}}}"#;

/// What the stub answers for a request target (`/path?query`), or a 404.
type Route = fn(&str) -> (u16, &'static str);

/// A Jira-shaped stub on `127.0.0.1:0`, answering from `route` and keeping
/// each request line (`GET /rest/api/2/mypermissions?projectKey=PAY`).
async fn stub(route: Route) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind 127.0.0.1:0");
    let addr = listener
        .local_addr()
        .expect("a bound listener has an address");
    let seen = Arc::new(Mutex::new(Vec::new()));
    let kept = seen.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let mut buf = vec![0_u8; 4096];
            let Ok(read) = socket.read(&mut buf).await else {
                continue;
            };
            let request = String::from_utf8_lossy(&buf[..read]).into_owned();
            let mut line = request.split_whitespace();
            let method = line.next().unwrap_or_default().to_owned();
            let target = line.next().unwrap_or_default().to_owned();
            kept.lock()
                .expect("the record")
                .push(format!("{method} {target}"));
            let (status, body) = route(&target);
            let response = format!(
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\n\
                 Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = socket.write_all(response.as_bytes()).await;
            let _ = socket.shutdown().await;
        }
    });
    (format!("http://{addr}"), seen)
}

fn source(base_url: &str) -> Box<dyn Source> {
    match knobas_source_jira::build(SourceInstance {
        id: "jira".to_owned(),
        kind: "jira".to_owned(),
        display_name: "Tidewater Jira".to_owned(),
        base_url: base_url.to_owned(),
        auth: Some(AuthMethod::Pat),
        secret: Some("a-personal-access-token".to_owned()),
        account: None,
        config: serde_json::json!({}),
    }) {
        Ok(s) => s,
        Err(e) => panic!("the adapter must build against the stub: {e:?}"),
    }
}

fn ask(entity: &str, op: &str) -> WriteAsk {
    WriteAsk {
        entity: entity.to_owned(),
        op: op.to_owned(),
    }
}

fn refused(entity: &str, op: &str, reason: &str) -> RefusedWrite {
    RefusedWrite {
        entity: entity.to_owned(),
        op: op.to_owned(),
        reason: reason.to_owned(),
    }
}

fn permissions(target: &str) -> (u16, &'static str) {
    match target {
        "/rest/api/2/mypermissions?projectKey=PAY" => (200, PAY),
        "/rest/api/2/mypermissions?projectKey=LEDG" => (200, LEDG),
        _ => (
            404,
            r#"{"errorMessages":["No project could be found with key 'X'."]}"#,
        ),
    }
}

/// The whole point: each write is answered by its project's permissions, a
/// ticket's in the project its key names and a create's in the project it
/// is filed in, with one read per project and no write sent.
#[tokio::test]
async fn each_write_is_answered_by_its_projects_permissions_with_one_read_each() {
    let (url, seen) = stub(permissions).await;
    let asks = [
        ask("jira:PAY-231", "comment"),
        ask("jira:PAY-231", "transition"),
        ask("jira:PAY-104", "log_work"),
        ask("jira:PAY", "create_ticket"),
        ask("jira:LEDG-12", "comment"),
        ask("jira:LEDG", "create_ticket"),
        // No verdict: another source's ticket, an op no permission gates, a
        // create aimed at a ticket, an id that is not one.
        ask("jira-eu:PAY-231", "comment"),
        ask("jira:PAY-231", "rerun_build"),
        ask("jira:PAY-231", "create_ticket"),
        ask("jira:PAY-1/transitions", "comment"),
    ];

    let answer = source(&url)
        .refused_writes(&asks)
        .await
        .expect("a read is an answer");

    assert_eq!(
        answer,
        [
            refused(
                "jira:PAY-231",
                "comment",
                "This Jira user cannot comment in PAY: it lacks the Add Comments permission"
            ),
            refused(
                "jira:PAY-104",
                "log_work",
                "This Jira user cannot log work in PAY: it lacks the Work On Issues permission"
            ),
            refused(
                "jira:LEDG-12",
                "comment",
                "This Jira user cannot comment in LEDG: it lacks the Add Comments permission"
            ),
            refused(
                "jira:LEDG",
                "create_ticket",
                "This Jira user cannot create tickets in LEDG: it lacks the Create Issues permission"
            ),
        ]
    );
    assert_eq!(
        *seen.lock().expect("the record"),
        [
            "GET /rest/api/2/mypermissions?projectKey=PAY",
            "GET /rest/api/2/mypermissions?projectKey=LEDG",
        ],
        "one read per project, and nothing written to find out"
    );
}

/// A project whose read fails is no verdict for its own asks, and the other
/// projects are still answered.
#[tokio::test]
async fn a_failed_read_refuses_nothing_in_that_project_alone() {
    let (url, _) = stub(|target| match target {
        "/rest/api/2/mypermissions?projectKey=LEDG" => (500, r#"{"errorMessages":["boom"]}"#),
        other => permissions(other),
    })
    .await;

    let answer = source(&url)
        .refused_writes(&[
            ask("jira:LEDG-12", "comment"),
            ask("jira:PAY-231", "comment"),
        ])
        .await
        .expect("a read is an answer");

    assert_eq!(
        answer,
        [refused(
            "jira:PAY-231",
            "comment",
            "This Jira user cannot comment in PAY: it lacks the Add Comments permission"
        )]
    );
}

/// No ask a permission gates sends nothing at all.
#[tokio::test]
async fn nothing_to_answer_reads_nothing() {
    let (url, seen) = stub(permissions).await;
    let answer = source(&url)
        .refused_writes(&[
            ask("jira:PAY-231", "rerun_build"),
            ask("gitea:a/b#1", "comment"),
        ])
        .await
        .expect("no ask is an answer");
    assert!(answer.is_empty());
    assert!(seen.lock().expect("the record").is_empty());
}

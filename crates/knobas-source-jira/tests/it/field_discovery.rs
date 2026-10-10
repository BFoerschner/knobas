//! *Test connection* against a stub that answers the field table (#297).
//!
//! `knobas-mockd` serves no `GET /rest/api/2/field` and is frozen (ADR-0013),
//! so the only server in this workspace that can answer it is the real Jira in
//! `testenv` -- which `tests/live_jira_seeded.rs` uses and which `just check`
//! never starts. That leaves the **success** direction of discovery with no
//! offline witness at all: `tests/it/mockd.rs` certifies the 501, `src/discover.rs`
//! certifies the picking, and nothing between them certifies that what was
//! picked reaches `ConnectionInfo::discovered` rather than being computed and
//! dropped.
//!
//! This file is that witness, and it is a *transport* test rather than a second
//! mock: forty lines of canned bytes on a socket, no fixture, no state, no
//! second reading of the Jira contract that could agree with the adapter's own.
//! The bodies below are copied from what Jira 10.3.24 answered during #276.

use std::collections::BTreeMap;

use knobas_source::instance::SourceInstance;
use knobas_source::{AuthMethod, Source};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// The field table, as a real Jira Software answers it: the four Greenhopper
/// epic fields in the id order the product mints them, and Epic Link is not
/// the first of them.
const FIELD_TABLE: &str = r#"[
  {"id":"summary","name":"Summary","custom":false,"schema":{"type":"string","system":"summary"}},
  {"id":"customfield_10100","name":"Epic Name","custom":true,
   "schema":{"type":"string","custom":"com.pyxis.greenhopper.jira:gh-epic-label","customId":10100}},
  {"id":"customfield_10101","name":"Epic Link","custom":true,
   "schema":{"type":"any","custom":"com.pyxis.greenhopper.jira:gh-epic-link","customId":10101}},
  {"id":"customfield_10102","name":"Epic Status","custom":true,
   "schema":{"type":"option","custom":"com.pyxis.greenhopper.jira:gh-epic-status","customId":10102}}
]"#;

/// What the stub answers for one request path, or a 404.
type Route = fn(&str) -> Option<(u16, &'static str)>;

/// A Jira-shaped stub on `127.0.0.1:0`, answering from `route`.
///
/// One request per connection, `Connection: close`, so there is no keep-alive
/// state machine to get wrong. The task ends when the test drops the runtime.
async fn stub(route: Route) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind 127.0.0.1:0");
    let addr = listener
        .local_addr()
        .expect("a bound listener has an address");
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
            // "GET /rest/api/2/field?x=y HTTP/1.1"
            let path = request
                .split_whitespace()
                .nth(1)
                .unwrap_or_default()
                .split('?')
                .next()
                .unwrap_or_default()
                .to_owned();
            let (status, body) = route(&path).unwrap_or((404, r#"{"errorMessages":["nope"]}"#));
            let response = format!(
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\n\
                 Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = socket.write_all(response.as_bytes()).await;
            let _ = socket.shutdown().await;
        }
    });
    format!("http://{addr}")
}

fn source(base_url: &str, config: serde_json::Value) -> Box<dyn Source> {
    match knobas_source_jira::build(SourceInstance {
        id: "jira".to_owned(),
        kind: "jira".to_owned(),
        display_name: "Tidewater Jira".to_owned(),
        base_url: base_url.to_owned(),
        auth: Some(AuthMethod::Pat),
        secret: Some("a-personal-access-token".to_owned()),
        account: None,
        config,
    }) {
        Ok(s) => s,
        // `Box<dyn Source>` is not `Debug`, so `expect` is unavailable.
        Err(e) => panic!("the adapter must build against the stub: {e:?}"),
    }
}

/// The two calls every `test_connection` makes, plus whatever the case adds.
fn base(path: &str) -> Option<(u16, &'static str)> {
    match path {
        "/rest/api/2/myself" => Some((200, r#"{"name":"mara.lindqvist"}"#)),
        "/rest/api/2/serverInfo" => Some((
            200,
            r#"{"version":"10.3.24","deploymentType":"Server",
                "serverTime":"2026-09-03T11:48:00.000+0200"}"#,
        )),
        _ => None,
    }
}

/// The whole point: an instance that answers the field table has its Epic Link
/// id **on the report**, keyed by the config property the dialog fills.
#[tokio::test]
async fn a_field_table_puts_the_epic_link_id_on_the_connection_report() {
    let url = stub(|path| match path {
        "/rest/api/2/field" => Some((200, FIELD_TABLE)),
        other => base(other),
    })
    .await;

    let info = source(&url, serde_json::json!({}))
        .test_connection()
        .await
        .expect("the stub answers every call test_connection makes");

    assert_eq!(
        info.discovered,
        BTreeMap::from([("epic_link_field".to_owned(), "customfield_10101".to_owned())]),
        "the discovered id must reach the report under the config property it fills: {info:?}"
    );
    // Not Epic Status, which sits one id along and would mirror a workflow
    // state as though it were the epic. And the connection note is honest
    // about what *finding* it does and does not do: nothing is mirrored until
    // the id is in the configuration, which for a saved source is a later
    // ticket. The note is that clause alone -- the deployment and version are
    // `server_version`'s to say, and the report shows them once (#326).
    assert_eq!(
        info.detail.as_deref(),
        Some(
            "Epic Link customfield_10101 found but not configured: \
             epic membership is not mirrored"
        ),
        "{info:?}"
    );
    // And the value is one the form will accept back, which is what makes the
    // fill a working configuration rather than a value that fails on save.
    let filled = serde_json::json!({ "epic_link_field": info.discovered["epic_link_field"] });
    knobas_source_jira::JiraConfig::from_json(&filled).expect("a discovered id is a valid config");
}

/// An id the reader typed is what the connection reports, and the probe does
/// not silently replace it -- the dialog's "only when empty" rule has a
/// backend half, which is that nothing here writes the config at all.
#[tokio::test]
async fn a_configured_id_is_reported_even_when_the_instance_names_another() {
    let url = stub(|path| match path {
        "/rest/api/2/field" => Some((200, FIELD_TABLE)),
        other => base(other),
    })
    .await;

    let info = source(
        &url,
        serde_json::json!({ "epic_link_field": "customfield_10008" }),
    )
    .test_connection()
    .await
    .expect("the stub answers");

    assert_eq!(
        info.discovered.get("epic_link_field").map(String::as_str),
        Some("customfield_10101"),
        "the report still says what the instance has: {info:?}"
    );
    assert_eq!(
        info.detail.as_deref(),
        Some("Epic Link customfield_10008"),
        "the note names the configured id, which is the one the sync will use: {info:?}"
    );
}

/// **A 404 on the field table is not a failed connection.** A Jira Core, or a
/// proxy exposing only the M1 paths, has no field table -- and losing the
/// account, the version and the credential's verdict over an optional
/// convenience would make a working source unsaveable.
#[tokio::test]
async fn a_404_on_the_field_table_is_a_connection_that_worked() {
    let url = stub(base).await;

    let info = source(&url, serde_json::json!({}))
        .test_connection()
        .await
        .expect("a 404 on the field table must not fail the connection");

    assert_eq!(info.account.as_deref(), Some("mara.lindqvist"));
    assert_eq!(info.server_version.as_deref(), Some("10.3.24"));
    assert!(info.discovered.is_empty(), "{info:?}");
    // Said out loud rather than left to be found weeks later in an empty
    // Contexts view: this source mirrors no epic membership.
    assert_eq!(
        info.detail.as_deref(),
        Some("no Epic Link field: a classic project's epic membership is not mirrored"),
        "{info:?}"
    );
}

/// …but a **401** on the field table is still a refused credential. The
/// tolerance is for the endpoint being absent, not for the connection being
/// bad: reporting a dead PAT as a healthy source missing one convenience is
/// the failure this arm exists to prevent.
#[tokio::test]
async fn a_401_on_the_field_table_is_still_unauthorized() {
    let url = stub(|path| match path {
        "/rest/api/2/field" => Some((401, r#"{"errorMessages":["refused"]}"#)),
        other => base(other),
    })
    .await;

    let refused = source(&url, serde_json::json!({})).test_connection().await;
    assert!(
        matches!(
            refused,
            Err(knobas_source::SourceError::Unauthorized { .. })
        ),
        "{refused:?}"
    );
}

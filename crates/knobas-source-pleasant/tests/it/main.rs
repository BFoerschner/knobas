//! The Pleasant adapter over a replayed server (ADR-0056): the contract
//! battery with clause 16, the mapping, the four code-key formats, and **the
//! request budget, which is the audit promise as a test**.

use knobas_source::contract::{Fault, KeySink, battery};
use knobas_source::instance::SourceInstance;
use knobas_source::passwords::{EntryRecord, OTP_UNREADABLE, SecretAnswer, SecretAsk, SecretRef};
use knobas_source::{AuthMethod, Source};
use wiremock::matchers::{body_string_contains, method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

const TREE: &str = include_str!("../fixtures/tree.json");
const DB: &str = "a1b2c3d4-0001-4b00-8000-000000000001";
const WIKI: &str = "a1b2c3d4-0002-4b00-8000-000000000002";

fn instance(base_url: &str, password: &str) -> SourceInstance {
    SourceInstance {
        id: "pw".into(),
        kind: knobas_source_pleasant::ADAPTER_KIND.into(),
        display_name: "Team passwords".into(),
        base_url: base_url.into(),
        auth: Some(AuthMethod::UserPassword),
        secret: Some(password.into()),
        account: None,
        config: serde_json::json!({ "username": "mara.lindqvist" }),
    }
}

/// A Pleasant server: sign-in, the tree, two passwords (one behind a usage
/// comment), and **a trap on `GET entries/{id}`** that fails the test on drop
/// if anything ever asks for it.
async fn server() -> MockServer {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/OAuth2/Token"))
        .and(body_string_contains("grant_type=password"))
        .and(body_string_contains("password=right"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "tok-1", "token_type": "bearer", "expires_in": 3600
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/OAuth2/Token"))
        .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
            "error": "invalid_grant", "error_description": "The user name or password is incorrect."
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v5/rest/folders"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(TREE, "application/json"))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v5/rest/GetServerInfo"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"Version": "7.11.6"})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/api/v5/rest/entries/{WIKI}/password")))
        .respond_with(ResponseTemplate::new(200).set_body_json("Ad0n!s-wiki"))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/api/v5/rest/entries/{DB}/password")))
        .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
            "Message": "A comment is required to view this password."
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("/api/v5/rest/entries/{DB}/password")))
        .and(body_string_contains("PAY-231"))
        .respond_with(ResponseTemplate::new(200).set_body_json("Tide-77!payout"))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex(r"^/api/v5/rest/entries/[^/]+$"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .named("GET entries/{id}, which answers the password inside the entry")
        .mount(&server)
        .await;
    server
}

fn source(base_url: &str) -> Box<dyn Source> {
    knobas_source_pleasant::build(instance(base_url, "right")).unwrap()
}

#[tokio::test]
async fn the_adapter_passes_the_contract_battery() {
    let server = server().await;
    let url = server.uri();
    battery(move |fault| {
        let (base, password) = match fault {
            Fault::None => (url.as_str(), "right"),
            Fault::Unauthorized => (url.as_str(), "wrong"),
            // Nothing listens there.
            Fault::Unreachable => ("http://127.0.0.1:9", "right"),
        };
        knobas_source_pleasant::build(instance(base, password)).unwrap()
    })
    .await;
}

#[tokio::test]
async fn a_sync_is_one_sign_in_and_one_tree_and_nothing_else() {
    let server = server().await;
    let s = source(&server.uri());
    let mut sink = KeySink::default();
    let (_, sent) = knobas_http::counted::counted(s.sync(None, &mut sink)).await;
    assert_eq!(sent, 2, "a sync is the token and the tree");
    // A second instance of the same source signs in zero times.
    let again = source(&server.uri());
    let (_, sent) = knobas_http::counted::counted(again.sync(None, &mut KeySink::default())).await;
    assert_eq!(sent, 1, "the token is shared across instances");
    let requests = server.received_requests().await.unwrap();
    assert!(
        requests.iter().all(|r| !r.url.path().contains("/entries/")),
        "a sync asked for an entry"
    );

    // The mapping: five entries in four folders, the root left out.
    let kinds: Vec<&str> = sink.items.iter().map(|i| i.kind.as_str()).collect();
    assert_eq!(kinds.iter().filter(|k| **k == "password_folder").count(), 4);
    assert_eq!(kinds.iter().filter(|k| **k == "password_entry").count(), 5);
    let record = |key: &str| -> EntryRecord {
        let item = sink.items.iter().find(|i| i.entity.key == key).unwrap();
        serde_json::from_value(item.payload.clone()).unwrap()
    };
    let db = record(DB);
    assert!(db.comment.read && db.comment.modify);
    assert_eq!(
        db.path.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
        ["Tidewater", "Payments", "Databases"]
    );
    assert_eq!(
        db.fields
            .iter()
            .map(|f| (f.name.as_str(), f.value.is_some()))
            .collect::<Vec<_>>(),
        [("Port", true), ("Recovery PIN", false)]
    );
    // One code key per readable format; the Steam one is unreadable and handed over as nothing.
    let formats: Vec<String> = [
        "a1b2c3d4-0001-4b00-8000-000000000001",
        WIKI,
        "a1b2c3d4-0003-4b00-8000-000000000003",
        "a1b2c3d4-0004-4b00-8000-000000000004",
        "a1b2c3d4-0005-4b00-8000-000000000005",
    ]
    .iter()
    .map(|k| record(k).otp.unwrap().format)
    .collect();
    assert_eq!(
        formats,
        ["keepass", "tray_totp", "keeotp", "otpauth", OTP_UNREADABLE]
    );
    assert_eq!(sink.keys.len(), 4);
    assert_eq!(
        sink.keys[0].1.secret.expose(),
        "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ"
    );
}

#[tokio::test]
async fn an_unchanged_tree_emits_nothing_and_keeps_its_cursor() {
    let server = server().await;
    let s = source(&server.uri());
    let cursor = s.sync(None, &mut KeySink::default()).await.unwrap();
    let mut sink = KeySink::default();
    let again = s.sync(Some(cursor.clone()), &mut sink).await.unwrap();
    assert_eq!(again, cursor);
    assert!(sink.items.is_empty() && sink.keys.is_empty());
}

#[tokio::test]
async fn a_password_is_one_request_and_a_comment_is_asked_for_by_the_server() {
    let server = server().await;
    let s = source(&server.uri());
    s.test_connection().await.unwrap();
    let ask = |entity: &str, comment: Option<&str>| SecretAsk {
        entity: format!("pw:{entity}"),
        which: SecretRef::Password,
        comment: comment.map(str::to_owned),
        folder: None,
    };
    let (answer, sent) = knobas_http::counted::counted(s.read_secret(&ask(WIKI, None))).await;
    assert_eq!(sent, 1, "a copy is one request");
    match answer.unwrap() {
        SecretAnswer::Value(v) => assert_eq!(v.expose(), "Ad0n!s-wiki"),
        other => panic!("{other:?}"),
    }
    let (answer, sent) = knobas_http::counted::counted(s.read_secret(&ask(DB, None))).await;
    assert_eq!(sent, 1, "a refused read is not sent again");
    assert_eq!(
        answer.unwrap(),
        SecretAnswer::CommentRequired { scope: vec![] }
    );
    match s
        .read_secret(&ask(DB, Some("PAY-231 Payout retries")))
        .await
        .unwrap()
    {
        SecretAnswer::Value(v) => assert_eq!(v.expose(), "Tide-77!payout"),
        other => panic!("{other:?}"),
    }
    // An entity of another source, and one that is no entry, are no reads.
    assert_eq!(
        s.read_secret(&SecretAsk {
            entity: format!("other:{WIKI}"),
            ..ask(WIKI, None)
        })
        .await
        .unwrap(),
        SecretAnswer::NotOffered
    );
}

#[tokio::test]
async fn test_connection_signs_in_and_reads_no_entry() {
    let server = server().await;
    let info = source(&server.uri()).test_connection().await.unwrap();
    assert_eq!(info.account.as_deref(), Some("mara.lindqvist"));
    assert_eq!(
        info.server_version.as_deref(),
        Some("Password Server 7.11.6")
    );
    assert_eq!(info.detail.as_deref(), Some("It read no entries."));
    let wrong = knobas_source_pleasant::build(instance(&server.uri(), "wrong")).unwrap();
    assert!(matches!(
        wrong.test_connection().await,
        Err(knobas_source::SourceError::Unauthorized { .. })
    ));
}

#[tokio::test]
async fn an_expired_token_signs_in_once_more_and_repeats_once() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/OAuth2/Token"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"access_token": "tok", "expires_in": 3600})),
        )
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v5/rest/folders"))
        .respond_with(ResponseTemplate::new(401))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v5/rest/folders"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(TREE, "application/json"))
        .mount(&server)
        .await;
    let s = source(&server.uri());
    let (result, sent) = knobas_http::counted::counted(s.sync(None, &mut KeySink::default())).await;
    result.unwrap();
    assert_eq!(sent, 4, "token, refused tree, token, tree");
}

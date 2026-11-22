//! The Pleasant adapter over a replayed server (ADR-0056): the contract
//! battery with clause 16, the mapping, the four code-key formats, and **the
//! request budget, which is the audit promise as a test**.

mod recorded;

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

async fn token_refusal(body: serde_json::Value) -> knobas_source::SourceError {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/OAuth2/Token"))
        .respond_with(ResponseTemplate::new(400).set_body_json(body))
        .mount(&server)
        .await;
    match source(&server.uri()).test_connection().await {
        Err(error) => error,
        Ok(info) => panic!("signed in: {info:?}"),
    }
}

#[tokio::test]
async fn a_refused_sign_in_is_the_credential_and_a_second_factor_is_said_in_words() {
    assert!(matches!(
        token_refusal(serde_json::json!({"error": "invalid_grant"})).await,
        knobas_source::SourceError::Unauthorized { .. }
    ));
    assert!(matches!(
        token_refusal(
            serde_json::json!({"error": "unsupported", "error_description": "Bad password."})
        )
        .await,
        knobas_source::SourceError::Unauthorized { .. }
    ));
    let otp = token_refusal(
        serde_json::json!({"error": "otp", "error_description": "two-factor code required"}),
    )
    .await;
    assert!(otp.to_string().contains("second factor"), "{otp}");
    let other = token_refusal(serde_json::json!({"error": "unsupported_grant_type"})).await;
    assert!(
        matches!(other, knobas_source::SourceError::Protocol { .. })
            && !other.to_string().contains("second factor"),
        "{other}"
    );
}

#[tokio::test]
async fn a_token_refused_twice_is_unauthorized_and_not_a_loop() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/OAuth2/Token"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"access_token": "tok"})),
        )
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v5/rest/folders"))
        .respond_with(ResponseTemplate::new(401))
        .expect(2)
        .mount(&server)
        .await;
    let refused = source(&server.uri())
        .sync(None, &mut KeySink::default())
        .await;
    assert!(matches!(
        refused,
        Err(knobas_source::SourceError::Unauthorized { .. })
    ));
}

#[tokio::test]
async fn a_server_older_than_7_9_7_is_refused_by_name() {
    for (version, ok) in [
        ("7.9.6", false),
        ("6.0.0", false),
        ("7.9.7", true),
        ("unknown", true),
    ] {
        let server = MockServer::builder().start().await;
        Mock::given(method("POST"))
            .and(path("/OAuth2/Token"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"access_token": "tok"})),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v5/rest/GetServerInfo"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"Version": version})),
            )
            .mount(&server)
            .await;
        let answer = source(&server.uri()).test_connection().await;
        assert_eq!(answer.is_ok(), ok, "{version}: {answer:?}");
        if let Err(error) = answer {
            assert!(error.to_string().contains("older than 7.9.7"), "{error}");
        }
    }
}

/// `walk = folders`: the root's id, then each folder one level down, eight
/// at a time, put back together into the tree a single request answers.
#[tokio::test]
async fn the_folder_walk_reads_the_same_tree_a_folder_at_a_time() {
    let tree: serde_json::Value = serde_json::from_str(TREE).unwrap();
    let server = server().await;
    Mock::given(method("GET"))
        .and(path("/api/v5/rest/folders/root"))
        .respond_with(ResponseTemplate::new(200).set_body_json(tree["Id"].clone()))
        .mount(&server)
        .await;
    // Each folder answers with its entries and its children, the children's
    // own contents left out (`recurseLevel=1`).
    fn mount_all(folder: &serde_json::Value, out: &mut Vec<(String, serde_json::Value)>) {
        let mut shallow = folder.clone();
        if let Some(children) = shallow["Children"].as_array_mut() {
            for child in children.iter_mut() {
                child["Children"] = serde_json::json!([]);
                child["Credentials"] = serde_json::json!([]);
            }
        }
        out.push((folder["Id"].as_str().unwrap().to_owned(), shallow));
        for child in folder["Children"].as_array().unwrap() {
            mount_all(child, out);
        }
    }
    let mut folders = Vec::new();
    mount_all(&tree, &mut folders);
    for (id, body) in folders {
        Mock::given(method("GET"))
            .and(path(format!("/api/v5/rest/folders/{id}")))
            .and(wiremock::matchers::query_param("recurseLevel", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;
    }
    let mut by_tree = KeySink::default();
    source(&server.uri())
        .sync(None, &mut by_tree)
        .await
        .unwrap();
    let mut config = instance(&server.uri(), "right");
    config.config = serde_json::json!({"username": "mara.lindqvist", "walk": "folders"});
    let mut by_folders = KeySink::default();
    knobas_source_pleasant::build(config)
        .unwrap()
        .sync(None, &mut by_folders)
        .await
        .unwrap();
    let ids = |s: &KeySink| {
        s.items
            .iter()
            .map(|i| (i.entity.to_string(), i.payload.clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(ids(&by_folders), ids(&by_tree));
    assert_eq!(by_folders.keys.len(), by_tree.keys.len());
}

#[tokio::test]
async fn a_field_the_listing_did_not_send_is_read_from_its_folder() {
    let server = server().await;
    let folder = "6f9b1c3e-0003-4a10-9000-000000000003";
    Mock::given(method("GET"))
        .and(path(format!("/api/v5/rest/folders/{folder}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "Id": folder, "Name": "Databases", "Children": [],
            "Credentials": [{"Id": DB, "CustomUserFields": {"Recovery PIN": "4711", "Empty": ""}}]
        })))
        .mount(&server)
        .await;
    let s = source(&server.uri());
    let ask = |name: &str| SecretAsk {
        entity: format!("pw:{DB}"),
        which: SecretRef::Field { name: name.into() },
        comment: None,
        folder: Some(format!("pw:{folder}")),
    };
    let (answer, sent) = knobas_http::counted::counted(s.read_secret(&ask("Recovery PIN"))).await;
    match answer.unwrap() {
        SecretAnswer::Value(v) => assert_eq!(v.expose(), "4711"),
        other => panic!("{other:?}"),
    }
    assert_eq!(sent, 2, "a sign-in and the folder");
    assert_eq!(
        s.read_secret(&ask("Empty")).await.unwrap(),
        SecretAnswer::NotOffered
    );
    assert_eq!(
        s.read_secret(&ask("Missing")).await.unwrap(),
        SecretAnswer::NotOffered
    );
}

/// The three entry writes (ADR-0056): each one request sent once, an edit
/// carrying only what changed, then the entry read back by listing its folder
/// (never `GET entries/{id}`). The bodies are what the stand-in received.
#[tokio::test]
async fn the_writes_send_what_changed_once_and_read_the_entry_back_from_its_folder() {
    use knobas_source::passwords::{SecretText, Write};
    let server = server().await;
    let folder = "6f9b1c3e-0004-4a10-9000-000000000004";
    Mock::given(method("POST"))
        .and(path("/api/v5/rest/entries"))
        .respond_with(ResponseTemplate::new(200).set_body_json(WIKI))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .and(path(format!("/api/v5/rest/entries/{WIKI}")))
        .respond_with(ResponseTemplate::new(204))
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/api/v5/rest/folders/{folder}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "Id": folder, "Name": "Platform", "ParentId": "x", "Children": [],
            "Credentials": [{"Id": WIKI, "Name": "wiki admin", "Username": "root", "GroupId": folder,
                             "HasViewEntryContentsAccess": true,
                             "CustomUserFields": {"TimeOtp-Secret-Base32": "GEZDGNBVGY3TQOJQ"}}]
        })))
        .mount(&server)
        .await;
    let s = source(&server.uri());
    let (written, sent) = knobas_http::counted::counted(s.passwords_write(Write::CreateEntry {
        source: "pw".into(),
        folder: Some(format!("pw:{folder}")),
        title: "wiki admin".into(),
        username: Some("root".into()),
        address: None,
        notes: None,
        expires: None,
        password: SecretText::new("N3w!pass"),
        otp_key: Some(SecretText::new("GEZDGNBVGY3TQOJQ")),
        comment: None,
    }))
    .await;
    let written = written.unwrap();
    assert_eq!(sent, 3, "a sign-in, the create and the folder");
    let item = written.item.expect("read back");
    assert_eq!(item.entity.to_string(), format!("pw:{WIKI}"));
    assert!(!item.payload.to_string().contains("GEZDGNBV"));
    // A folder under the root is the entry's folder.
    assert_eq!(item.payload["folder"], format!("pw:{folder}"));
    assert_eq!(written.otp_key.unwrap().secret.expose(), "GEZDGNBVGY3TQOJQ");

    let edit = s
        .passwords_write(Write::EditEntry {
            entity: format!("pw:{WIKI}"),
            title: None,
            username: Some(Some("admin2".into())),
            address: None,
            notes: None,
            expires: None,
            folder: None,
            otp_key: None,
            comment: None,
            listed_in: Some(format!("pw:{folder}")),
        })
        .await
        .unwrap();
    assert!(edit.item.is_some());
    s.passwords_write(Write::SetPassword {
        entity: format!("pw:{WIKI}"),
        password: SecretText::new("Ch4nged!"),
        comment: Some("OPS-77".into()),
        listed_in: None,
    })
    .await
    .unwrap();
    let bodies: Vec<serde_json::Value> = server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.method.as_str() != "GET" && r.url.path().contains("/entries"))
        .map(|r| serde_json::from_slice(&r.body).unwrap())
        .collect();
    assert_eq!(bodies.len(), 3);
    assert_eq!(bodies[0]["Name"], "wiki admin");
    assert_eq!(bodies[0]["Password"], "N3w!pass");
    assert_eq!(bodies[0]["GroupId"], folder);
    assert_eq!(
        bodies[0]["CustomUserFields"]["TimeOtp-Secret-Base32"],
        "GEZDGNBVGY3TQOJQ"
    );
    assert_eq!(
        bodies[1],
        serde_json::json!({"Username": "admin2"}),
        "an edit sends only what changed"
    );
    assert_eq!(
        bodies[2],
        serde_json::json!({"Password": "Ch4nged!", "Comment": "OPS-77"})
    );
    // Access requests are not offered: refused before anything is sent.
    let refused = s
        .passwords_write(Write::RequestAccess {
            entity: format!("pw:{WIKI}"),
            comment: "x".into(),
        })
        .await;
    assert!(matches!(
        refused,
        Err(knobas_source::SourceError::Protocol { status: None, .. })
    ));
}

/// An entry made in no folder goes to the root, whose id `folders/root`
/// answers, and is read back as in no folder.
#[tokio::test]
async fn an_entry_made_in_no_folder_goes_to_the_root_and_reads_back_in_none() {
    use knobas_source::passwords::{SecretText, Write};
    let server = server().await;
    let root = "6f9b1c3e-0000-4a10-9000-0000000000aa";
    Mock::given(method("GET"))
        .and(path("/api/v5/rest/folders/root"))
        .respond_with(ResponseTemplate::new(200).set_body_json(root))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v5/rest/entries"))
        .respond_with(ResponseTemplate::new(200).set_body_json(WIKI))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/api/v5/rest/folders/{root}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "Id": root, "Name": "Root", "ParentId": null, "Children": [],
            "Credentials": [{"Id": WIKI, "Name": "loose", "GroupId": root, "HasViewEntryContentsAccess": true}]
        })))
        .mount(&server)
        .await;
    let s = source(&server.uri());
    let written = s
        .passwords_write(Write::CreateEntry {
            source: "pw".into(),
            folder: None,
            title: "loose".into(),
            username: None,
            address: None,
            notes: None,
            expires: None,
            password: SecretText::new("x"),
            otp_key: None,
            comment: None,
        })
        .await
        .unwrap();
    let item = written.item.expect("read back");
    assert_eq!(item.payload["folder"], serde_json::Value::Null);
    assert!(written.otp_key.is_none());
    let posted: Vec<serde_json::Value> = server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.method.as_str() == "POST" && r.url.path().ends_with("/entries"))
        .map(|r| serde_json::from_slice(&r.body).unwrap())
        .collect();
    assert_eq!(posted.len(), 1);
    assert_eq!(posted[0]["GroupId"], root);
}

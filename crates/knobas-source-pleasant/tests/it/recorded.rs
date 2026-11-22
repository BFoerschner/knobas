//! The adapter over **the real server's answers** (Pleasant 9.5.4, recorded
//! by the server agent in `testenv/pleasant/recorded/`, `S/pleasant-server.md`
//! §6). Each recording is served as it was answered, its status, its
//! content type and its body, with the placeholders the recording masked made
//! concrete: an id becomes a fixed GUID, a time a fixed instant, a code seed
//! RFC 6238's.

use std::collections::BTreeMap;
use std::path::PathBuf;

use knobas_source::Source;
use knobas_source::contract::KeySink;
use knobas_source::passwords::{Attachments, EntryRecord, SecretAnswer, SecretAsk, SecretRef};
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// RFC 6238's SHA-1 seed in Base32: what a masked seed becomes.
const SEED: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testenv/pleasant/recorded")
}

/// A placeholder id (`{entry:pl-teamcity}`, `{folder:Tidewater/Payments}`)
/// as a fixed GUID: the same name, the same id, in a body and in a path.
pub fn guid(placeholder: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in placeholder.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    let hex = format!(
        "{hash:016x}{:016x}",
        hash.rotate_left(29) ^ 0x9e37_79b9_7f4a_7c15
    );
    format!(
        "{}-{}-4{}-8{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[13..16],
        &hex[17..20],
        &hex[20..32]
    )
}

/// `text` with every masked part made concrete.
fn concrete(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(['{', '<']) {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        let close = if tail.starts_with('{') { '}' } else { '>' };
        let Some(end) = tail.find(close) else {
            out.push_str(tail);
            return out;
        };
        let token = &tail[..=end];
        let made = if ["{entry:", "{folder:", "{attachment:"]
            .iter()
            .any(|p| token.starts_with(p))
        {
            Some(guid(token))
        } else {
            match token {
                "<time dddd-dd-ddTdd:dd:dd±dd:dd>" => Some("2026-08-30T10:22:00-06:00".to_owned()),
                "<base32 seed>" => Some(SEED.to_owned()),
                "<password>" => Some("Tide-77!payout".to_owned()),
                "<token>" => Some("tok-recorded".to_owned()),
                "<ServerVersion>" => Some("9.5.4.0".to_owned()),
                "<guid>" => Some("00000000-0000-4000-8000-0000000000aa".to_owned()),
                // `<protected>` stays as it is: a plain string, as the server sent it.
                _ if token.starts_with("<base64 ") => Some("QUFBQQ==".to_owned()),
                _ => None,
            }
        };
        match made {
            Some(made) => {
                out.push_str(&made);
                rest = &tail[end + close.len_utf8()..];
            }
            // Not a mask (JSON's own braces): keep the one character, read on.
            None => {
                out.push(tail.chars().next().unwrap_or_default());
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// One recording, concrete.
#[derive(Debug, Clone)]
pub struct Recorded {
    pub status: u16,
    pub content_type: Option<String>,
    pub body: String,
    pub method: String,
    pub path: String,
}

pub fn load(name: &str) -> Recorded {
    let file = dir().join(format!("{name}.json"));
    let text = std::fs::read_to_string(&file).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
    let body = &value["body"];
    Recorded {
        status: u16::try_from(value["status"].as_u64().unwrap()).unwrap(),
        content_type: value["headers"]["content-type"].as_str().map(str::to_owned),
        // A body the server sent empty (IIS's 404) stays empty.
        body: if body.is_null() {
            String::new()
        } else {
            concrete(&body.to_string())
        },
        method: value["request"]["method"].as_str().unwrap().to_owned(),
        // The matcher reads the path alone; a query (`?recurseLevel=2`) is left off.
        path: concrete(value["request"]["path"].as_str().unwrap())
            .split('?')
            .next()
            .unwrap_or_default()
            .to_owned(),
    }
}

fn answer(r: &Recorded) -> ResponseTemplate {
    let template = ResponseTemplate::new(r.status);
    match &r.content_type {
        Some(kind) => template.set_body_raw(r.body.clone().into_bytes(), kind),
        None => template,
    }
}

/// A server that answers each named recording at its own method and path,
/// signs anyone in with the recorded token, and fails on drop if anything
/// asks `GET entries/{id}` or the offline package (an audit line: §6.1).
async fn serve(names: &[&str]) -> MockServer {
    let server = MockServer::builder().start().await;
    let token = load("token-ok");
    Mock::given(method("POST"))
        .and(path("/OAuth2/Token"))
        .respond_with(answer(&token))
        .mount(&server)
        .await;
    for name in names {
        let r = load(name);
        Mock::given(method(r.method.as_str()))
            .and(path(r.path.as_str()))
            .respond_with(answer(&r))
            .named(*name)
            .mount(&server)
            .await;
    }
    Mock::given(method("GET"))
        .and(path_regex(r"^/api/v5/rest/entries/[^/]+$"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .named("GET entries/{id}")
        .mount(&server)
        .await;
    Mock::given(path_regex(r"(?i)offline"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .named("the offline package, which logs Offline Sync")
        .mount(&server)
        .await;
    server
}

fn source(base_url: &str) -> Box<dyn Source> {
    knobas_source_pleasant::build(super::instance(base_url, "right")).unwrap()
}

fn records(sink: &KeySink) -> BTreeMap<String, (String, EntryRecord)> {
    sink.items
        .iter()
        .filter(|i| i.kind == knobas_source::passwords::ENTRY_KIND)
        .map(|i| {
            (
                i.entity.key.clone(),
                (
                    i.title.clone(),
                    serde_json::from_value(i.payload.clone()).unwrap(),
                ),
            )
        })
        .collect()
}

#[tokio::test]
async fn a_sync_of_the_real_tree_reads_its_codes_attachments_rules_and_fields_in_two_requests() {
    let server = serve(&["tree-reader"]).await;
    let s = source(&server.uri());
    let mut sink = KeySink::default();
    let (cursor, sent) = knobas_http::counted::counted(s.sync(None, &mut sink)).await;
    cursor.unwrap();
    assert_eq!(sent, 2, "one sign-in and one tree: the audit promise");
    let entries = records(&sink);
    assert!(entries.len() >= 15, "{} entries", entries.len());
    // Pleasant's root folder is not emitted; Tidewater is.
    assert!(sink.items.iter().any(|i| i.title == "Tidewater"));
    assert!(!sink.items.iter().any(|i| i.title == "Root"));

    let by_title = |title: &str| {
        entries
            .values()
            .find(|(t, _)| t == title)
            .unwrap_or_else(|| panic!("no entry {title}"))
            .1
            .clone()
    };
    // The usage-comment rule comes from the entry, inherited below Prod.
    assert!(by_title("payout-service database user").comment.read);
    assert!(!by_title("TeamCity service account").comment.read);
    // The listing names attachments with no bytes.
    let keyed = entries
        .values()
        .filter(|(_, r)| matches!(r.attachments, Attachments::Listed(_)))
        .count();
    assert!(keyed >= 1, "no entry lists an attachment");
    // A field the listing sends is mirrored (Fable's ruling A), the masked
    // protected PIN among them, and no code-key field is.
    let fields: Vec<(String, Option<String>)> = entries
        .values()
        .flat_map(|(_, r)| r.fields.iter().map(|f| (f.name.clone(), f.value.clone())))
        .collect();
    assert!(fields.iter().all(|(_, v)| v.is_some()), "{fields:?}");
    assert!(
        fields
            .iter()
            .any(|(_, v)| v.as_deref() == Some("<protected>"))
    );
    assert!(!fields.iter().any(|(n, _)| n.eq_ignore_ascii_case("otp")
        || n.starts_with("TOTP ")
        || n.starts_with("TimeOtp-")));
    // Every entry with a key in the recording has its key beside its record,
    // never in it.
    let coded = entries.values().filter(|(_, r)| r.otp.is_some()).count();
    assert!(coded >= 3, "{coded} entries with codes");
    assert_eq!(sink.keys.len(), coded);
    for item in &sink.items {
        assert!(!item.payload.to_string().contains(SEED), "{}", item.title);
    }
}

#[tokio::test]
async fn a_password_read_is_answered_as_the_real_server_refuses_and_answers() {
    let server = serve(&[
        "password-get",
        "password-get-comment-required",
        "password-missing-entry",
        "password-post-comment",
    ])
    .await;
    let s = source(&server.uri());
    let ask = |name: &str, comment: Option<&str>| {
        let r = load(name);
        let key = r
            .path
            .trim_start_matches("/api/v5/rest/entries/")
            .trim_end_matches("/password")
            .to_owned();
        SecretAsk {
            entity: format!("pw:{key}"),
            which: SecretRef::Password,
            comment: comment.map(str::to_owned),
            folder: None,
        }
    };
    assert!(matches!(
        s.read_secret(&ask("password-get", None)).await.unwrap(),
        SecretAnswer::Value(_)
    ));
    assert!(matches!(
        s.read_secret(&ask("password-get-comment-required", None))
            .await
            .unwrap(),
        SecretAnswer::CommentRequired { .. }
    ));
    assert!(matches!(
        s.read_secret(&ask("password-post-comment", Some("OPS-77")))
            .await
            .unwrap(),
        SecretAnswer::Value(_)
    ));
    assert_eq!(
        s.read_secret(&ask("password-missing-entry", None))
            .await
            .unwrap(),
        SecretAnswer::NotOffered
    );
}

#[tokio::test]
async fn test_connection_reads_the_real_servers_version() {
    let server = serve(&["server-info"]).await;
    let info = source(&server.uri()).test_connection().await.unwrap();
    assert!(format!("{info:?}").contains("9.5.4"), "{info:?}");
}

#[tokio::test]
async fn the_real_servers_refused_sign_in_and_expired_token_are_the_credential() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/OAuth2/Token"))
        .respond_with(answer(&load("token-wrong-password")))
        .mount(&server)
        .await;
    let refused = source(&server.uri()).test_connection().await.unwrap_err();
    assert!(
        matches!(refused, knobas_source::SourceError::Unauthorized { .. }),
        "{refused:?}"
    );

    // An expired token is the same 401 as a made-up one: sign in once more.
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/OAuth2/Token"))
        .respond_with(answer(&load("token-ok")))
        .expect(2)
        .mount(&server)
        .await;
    let expired = load("expired-token");
    Mock::given(method("GET"))
        .and(path("/api/v5/rest/folders"))
        .respond_with(answer(&expired))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v5/rest/folders"))
        .respond_with(answer(&load("tree-reader")))
        .mount(&server)
        .await;
    let mut sink = KeySink::default();
    source(&server.uri()).sync(None, &mut sink).await.unwrap();
    assert!(!sink.items.is_empty());
}

#[tokio::test]
async fn the_folder_walk_takes_the_comment_rule_from_the_entry_where_folders_say_false() {
    // Under `GET folders/{id}` every folder object says false (§6.4); the
    // entries inside still say true, and the rule is taken from them. PSP
    // blocks access inheritance, which blocks the inherited rule too (§6.7):
    // its two entries say false, and so must the walk.
    let r = load("folder-comment-rule");
    let folder: knobas_source_pleasant::api::ApiFolder = serde_json::from_str(&r.body).unwrap();
    let walked = knobas_source_pleasant::map::walk("pw", &folder);
    let asks: Vec<(String, bool)> = walked
        .items
        .iter()
        .filter(|i| i.kind == knobas_source::passwords::ENTRY_KIND)
        .map(|i| {
            let read = serde_json::from_value::<EntryRecord>(i.payload.clone())
                .unwrap()
                .comment
                .read;
            (i.title.clone(), read)
        })
        .collect();
    let in_psp = |t: &str| ["PSP live secret key", "PSP dashboard, finance sign-in"].contains(&t);
    assert_eq!(
        asks.iter().filter(|(t, _)| !in_psp(t)).count(),
        5,
        "{asks:?}"
    );
    assert!(
        asks.iter().filter(|(t, _)| !in_psp(t)).all(|(_, a)| *a),
        "{asks:?}"
    );
    assert_eq!(
        asks.iter().filter(|(t, _)| in_psp(t)).count(),
        2,
        "{asks:?}"
    );
    assert!(
        asks.iter().filter(|(t, _)| in_psp(t)).all(|(_, a)| !*a),
        "{asks:?}"
    );
}

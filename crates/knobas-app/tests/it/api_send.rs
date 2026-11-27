//! The API explorer's send (ux31, ADR-0057) against servers of the test's own: one request a send, no retry, the
//! limits, *Stop*, the cap on the body, the same-origin redirect rule, the origin of a secret, and what is kept.

use std::time::Duration;

use knobas_app::api::held::{Held, held};
use knobas_app::api::kept::kept;
use knobas_app::api::send::{self, ApiAnswer, ApiAuth, ApiSend, AuthPlace};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn request(id: u64, verb: &str, url: String) -> ApiSend {
    ApiSend {
        id,
        method: verb.into(),
        url,
        file: format!("repo\napi-{id}.yaml"),
        operation: format!("{verb} /x"),
        ..ApiSend::default()
    }
}

#[tokio::test]
async fn each_method_arrives_once_with_its_headers_and_body() {
    let server = MockServer::builder().start().await;
    for verb in ["GET", "PUT", "POST", "DELETE", "OPTIONS", "HEAD", "PATCH"] {
        Mock::given(method(verb))
            .and(path("/things"))
            .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
            .expect(1)
            .mount(&server)
            .await;
    }
    for (n, verb) in ["GET", "PUT", "POST", "DELETE", "OPTIONS", "HEAD", "PATCH"]
        .into_iter()
        .enumerate()
    {
        let mut send = request(100 + n as u64, verb, format!("{}/things", server.uri()));
        send.headers = vec![("X-Trace".into(), "t1".into())];
        send.body = (verb == "POST").then(|| "{\"a\":1}".to_owned());
        let answer = send::send(send, None).await.expect("sent");
        assert!(
            matches!(answer, ApiAnswer::Answered { status: 200, .. }),
            "{verb}: {answer:?}"
        );
    }
    let got = server.received_requests().await.expect("recorded");
    let post = got
        .iter()
        .find(|r| r.method.as_str() == "POST")
        .expect("a POST");
    assert_eq!(post.body, b"{\"a\":1}");
    assert_eq!(
        post.headers.get("x-trace").map(|v| v.to_str().unwrap()),
        Some("t1")
    );
    assert!(
        post.headers
            .get("user-agent")
            .is_some_and(|v| v.to_str().unwrap().starts_with("knobas/"))
    );
}

#[tokio::test]
async fn a_500_is_answered_and_never_retried() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500))
        .expect(1)
        .mount(&server)
        .await;
    let answer = send::send(
        request(200, "POST", format!("{}/charge", server.uri())),
        None,
    )
    .await
    .expect("sent");
    assert!(matches!(answer, ApiAnswer::Answered { status: 500, .. }));
    // `expect(1)` is checked when the server drops: one request, not two.
}

#[tokio::test]
async fn a_send_past_its_time_fails_at_the_timeout_and_the_server_saw_one() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(8)))
        .expect(1)
        .mount(&server)
        .await;
    let mut send = request(300, "GET", format!("{}/slow", server.uri()));
    send.timeout_s = Some(5);
    let answer = send::send(send, None).await.expect("sent");
    assert!(
        matches!(answer, ApiAnswer::Failed { ref stage, .. } if stage == "timeout"),
        "{answer:?}"
    );
}

#[tokio::test]
async fn stop_ends_a_send_at_once() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(20)))
        .mount(&server)
        .await;
    let running = tokio::spawn(send::send(
        request(400, "GET", format!("{}/slow", server.uri())),
        None,
    ));
    tokio::time::sleep(Duration::from_millis(300)).await;
    let t0 = std::time::Instant::now();
    send::cancel(400);
    let answer = running.await.expect("joined").expect("sent");
    assert!(t0.elapsed() < Duration::from_secs(2));
    assert!(
        matches!(answer, ApiAnswer::Failed { ref stage, ref words, .. } if stage == "stopped" && words.contains("may still have received")),
        "{answer:?}"
    );
}

#[tokio::test]
async fn an_answer_longer_than_the_cap_is_cut_there_and_says_so() {
    let server = MockServer::builder().start().await;
    let big = vec![b'a'; send::MAX_BODY + 1024 * 1024];
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(big))
        .mount(&server)
        .await;
    let answer = send::send(request(500, "GET", format!("{}/big", server.uri())), None)
        .await
        .expect("sent");
    let ApiAnswer::Answered { bytes, cut, .. } = answer else {
        panic!("{answer:?}")
    };
    assert!(cut);
    assert_eq!(bytes, send::MAX_BODY as u64);
}

#[tokio::test]
async fn file_and_trace_are_refused_before_any_socket_opens() {
    assert!(
        send::send(request(600, "GET", "file:///etc/hosts".into()), None)
            .await
            .is_err()
    );
    let server = MockServer::builder().start().await;
    assert!(
        send::send(request(601, "TRACE", format!("{}/x", server.uri())), None)
            .await
            .is_err()
    );
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_same_origin_redirect_is_followed_for_a_get_and_listed() {
    let server = MockServer::builder().start().await;
    Mock::given(path("/old"))
        .respond_with(ResponseTemplate::new(301).insert_header("Location", "/new"))
        .mount(&server)
        .await;
    Mock::given(path("/new"))
        .respond_with(ResponseTemplate::new(200).set_body_string("here"))
        .mount(&server)
        .await;
    let answer = send::send(request(700, "GET", format!("{}/old", server.uri())), None)
        .await
        .expect("sent");
    let ApiAnswer::Answered {
        status,
        hops,
        follow,
        ..
    } = answer
    else {
        panic!()
    };
    assert_eq!(status, 200);
    assert_eq!(hops.len(), 1);
    assert_eq!(hops[0].status, 301);
    assert!(follow.is_none());
    // A POST's redirect is its answer, even on its own origin.
    let answer = send::send(request(701, "POST", format!("{}/old", server.uri())), None)
        .await
        .expect("sent");
    assert!(
        matches!(
            answer,
            ApiAnswer::Answered {
                status: 301,
                follow: Some(_),
                ..
            }
        ),
        "{answer:?}"
    );
}

#[tokio::test]
async fn five_redirects_are_followed_and_the_sixth_is_the_answer() {
    let server = MockServer::builder().start().await;
    // /r0 -> /r1 -> ... -> /r6 -> /end: six hops would reach /r6, five stop at /r5's redirect.
    for n in 0..7 {
        Mock::given(path(format!("/r{n}")))
            .respond_with(
                ResponseTemplate::new(302).insert_header("Location", format!("/r{}", n + 1)),
            )
            .mount(&server)
            .await;
    }
    let answer = send::send(request(710, "GET", format!("{}/r0", server.uri())), None)
        .await
        .expect("sent");
    let ApiAnswer::Answered {
        status,
        hops,
        follow,
        ..
    } = answer
    else {
        panic!()
    };
    assert_eq!(status, 302);
    assert_eq!(hops.len(), send::MAX_HOPS);
    assert_eq!(
        follow.as_deref(),
        Some(format!("{}/r6", server.uri()).as_str())
    );
    assert_eq!(
        server.received_requests().await.expect("recorded").len(),
        send::MAX_HOPS + 1
    );
}

#[tokio::test]
async fn a_redirect_to_another_origin_is_the_answer_and_carries_nothing_there() {
    let away = MockServer::builder().start().await;
    let home = MockServer::builder().start().await;
    Mock::given(path("/go"))
        .respond_with(
            ResponseTemplate::new(302).insert_header("Location", format!("{}/landed", away.uri())),
        )
        .mount(&home)
        .await;
    held().hold(&home.uri(), "bearer", Held::new("tok-home".into(), None));
    let mut send = request(800, "GET", format!("{}/go", home.uri()));
    send.auth = Some(ApiAuth {
        origin: home.uri(),
        scheme: "bearer".into(),
        place: AuthPlace::Bearer,
        name: String::new(),
    });
    let answer = send::send(send, None).await.expect("sent");
    let ApiAnswer::Answered { status, follow, .. } = answer else {
        panic!()
    };
    assert_eq!(status, 302);
    assert_eq!(
        follow.as_deref(),
        Some(format!("{}/landed", away.uri()).as_str())
    );
    assert!(away.received_requests().await.unwrap().is_empty());
    // The home server saw the secret; *Follow* is a plain GET with none.
    let got = home.received_requests().await.unwrap();
    assert_eq!(
        got[0]
            .headers
            .get("authorization")
            .map(|v| v.to_str().unwrap()),
        Some("Bearer tok-home")
    );
    held().forget(&home.uri());
}

#[tokio::test]
async fn a_held_secret_is_attached_for_its_origin_and_refused_for_another() {
    let mine = MockServer::builder().start().await;
    let other = MockServer::builder().start().await;
    Mock::given(header("x-api-key", "k-1"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&mine)
        .await;
    held().hold(&mine.uri(), "key", Held::new("k-1".into(), None));
    let auth = ApiAuth {
        origin: mine.uri(),
        scheme: "key".into(),
        place: AuthPlace::Header,
        name: "X-API-Key".into(),
    };
    let mut ok = request(900, "GET", format!("{}/x", mine.uri()));
    ok.auth = Some(auth.clone());
    assert!(matches!(
        send::send(ok, None).await.expect("sent"),
        ApiAnswer::Answered { status: 204, .. }
    ));
    let mut away = request(901, "GET", format!("{}/x", other.uri()));
    away.auth = Some(auth);
    let refused = send::send(away, None).await.unwrap_err();
    assert!(
        refused
            .message
            .starts_with("This secret is kept for 127.0.0.1."),
        "{}",
        refused.message
    );
    assert!(other.received_requests().await.unwrap().is_empty());
    held().forget(&mine.uri());
}

#[tokio::test]
async fn a_send_is_kept_with_its_answer_and_no_secret() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{\"b\":[1,2],\"a\":1.50}"))
        .mount(&server)
        .await;
    held().hold(
        &server.uri(),
        "bearer",
        Held::new("never-kept".into(), None),
    );
    let mut send = request(1000, "GET", format!("{}/k", server.uri()));
    send.auth = Some(ApiAuth {
        origin: server.uri(),
        scheme: "bearer".into(),
        place: AuthPlace::Bearer,
        name: String::new(),
    });
    send::send(send, None).await.expect("sent");
    let lines = kept().lines("repo\napi-1000.yaml");
    assert_eq!(lines.len(), 1);
    let printed = serde_json::to_string(&lines).unwrap();
    assert!(!printed.contains("never-kept"), "{printed}");
    // Indented (#2533), the keys in the server's order and the numbers as written.
    assert_eq!(
        send::body(1000, true).as_deref(),
        Some(&b"{\n  \"b\": [\n    1,\n    2\n  ],\n  \"a\": 1.50\n}"[..])
    );
    held().forget(&server.uri());
}

/// A CA file the person named is read and must hold a certificate: one that
/// holds none fails the send before anything goes out (the ux30 end gate's
/// mutants: a client built without the file's roots went unseen).
#[tokio::test]
async fn a_ca_file_without_a_certificate_fails_the_send_before_it_goes() {
    let server = MockServer::builder().start().await;
    let dir = tempfile::tempdir().expect("a folder");
    let ca = dir.path().join("not-a-ca.pem");
    std::fs::write(&ca, "no certificate here\n").expect("the file");
    let answer = send::send(
        request(900, "GET", format!("{}/x", server.uri())),
        Some(ca.to_string_lossy().into_owned()),
    )
    .await;
    let failed = match answer {
        Ok(ApiAnswer::Failed { words, .. }) => words,
        Err(error) => format!("{error:?}"),
        Ok(other) => panic!("the send went out: {other:?}"),
    };
    assert!(failed.contains("CA file"), "{failed}");
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn basic_auth_with_empty_password_sends_user_colon_encoded() {
    let server = MockServer::builder().start().await;
    // base64("tok_live_abc:") == "dG9rX2xpdmVfYWJjOg=="
    Mock::given(header("authorization", "Basic dG9rX2xpdmVfYWJjOg=="))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .expect(1)
        .mount(&server)
        .await;
    held().hold(
        &server.uri(),
        "__manual:basic",
        Held::new(String::new(), Some("tok_live_abc".into())),
    );
    let mut send = request(1100, "GET", format!("{}/token-as-user", server.uri()));
    send.auth = Some(ApiAuth {
        origin: server.uri(),
        scheme: "__manual:basic".into(),
        place: AuthPlace::Basic,
        name: String::new(),
    });
    let answer = send::send(send, None).await.expect("sent");
    assert!(matches!(answer, ApiAnswer::Answered { status: 200, .. }));
    let printed = serde_json::to_string(&kept().lines("repo\napi-1100.yaml")).unwrap();
    assert!(!printed.contains("tok_live_abc"), "{printed}");
    held().forget(&server.uri());
}

#[tokio::test]
async fn manual_header_and_query_api_keys_attach_by_name_and_refuse_blank_name() {
    let server = MockServer::builder().start().await;
    Mock::given(path("/hdr"))
        .and(header("x-custom-auth", "k-hdr"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path("/qry"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;
    held().hold(
        &server.uri(),
        "__manual:apiKey",
        Held::new("k-hdr".into(), None),
    );
    let mut hdr = request(1200, "GET", format!("{}/hdr", server.uri()));
    hdr.auth = Some(ApiAuth {
        origin: server.uri(),
        scheme: "__manual:apiKey".into(),
        place: AuthPlace::Header,
        name: "X-Custom-Auth".into(),
    });
    assert!(matches!(
        send::send(hdr, None).await.expect("sent"),
        ApiAnswer::Answered { status: 200, .. }
    ));

    let mut qry = request(1201, "GET", format!("{}/qry?existing=1", server.uri()));
    qry.auth = Some(ApiAuth {
        origin: server.uri(),
        scheme: "__manual:apiKey".into(),
        place: AuthPlace::Query,
        name: "api_token".into(),
    });
    assert!(matches!(
        send::send(qry, None).await.expect("sent"),
        ApiAnswer::Answered { status: 200, .. }
    ));
    let got = server.received_requests().await.expect("recorded");
    let qry_req = got.iter().find(|r| r.url.path() == "/qry").expect("qry");
    assert_eq!(qry_req.url.query(), Some("existing=1&api_token=k-hdr"));

    let mut blank_hdr = request(1202, "GET", format!("{}/hdr", server.uri()));
    blank_hdr.auth = Some(ApiAuth {
        origin: server.uri(),
        scheme: "__manual:apiKey".into(),
        place: AuthPlace::Header,
        name: "   ".into(),
    });
    assert!(
        send::send(blank_hdr, None)
            .await
            .unwrap_err()
            .message
            .contains("no name")
    );
    let mut blank_qry = request(1203, "GET", format!("{}/qry", server.uri()));
    blank_qry.auth = Some(ApiAuth {
        origin: server.uri(),
        scheme: "__manual:apiKey".into(),
        place: AuthPlace::Query,
        name: String::new(),
    });
    assert!(
        send::send(blank_qry, None)
            .await
            .unwrap_err()
            .message
            .contains("no name")
    );
    let mut crlf_hdr = request(1204, "GET", format!("{}/hdr", server.uri()));
    crlf_hdr.auth = Some(ApiAuth {
        origin: server.uri(),
        scheme: "__manual:apiKey".into(),
        place: AuthPlace::Header,
        name: "X-Key\r\nEvil: 1".into(),
    });
    assert!(
        send::send(crlf_hdr, None)
            .await
            .unwrap_err()
            .message
            .contains("line break")
    );
    held().forget(&server.uri());
}

#[tokio::test]
async fn forgetting_held_secrets_clears_origin_and_refuses_next_send() {
    let server = MockServer::builder().start().await;
    Mock::given(header("authorization", "Bearer tok-once"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;
    held().hold(
        &server.uri(),
        "__manual:bearer",
        Held::new("tok-once".into(), None),
    );
    let auth = ApiAuth {
        origin: server.uri(),
        scheme: "__manual:bearer".into(),
        place: AuthPlace::Bearer,
        name: String::new(),
    };
    let mut first = request(1300, "GET", format!("{}/once", server.uri()));
    first.auth = Some(auth.clone());
    assert!(matches!(
        send::send(first, None).await.expect("sent"),
        ApiAnswer::Answered { status: 200, .. }
    ));

    held().forget(&server.uri());
    assert!(held().schemes(&server.uri()).is_empty());

    let mut second = request(1301, "GET", format!("{}/once", server.uri()));
    second.auth = Some(auth);
    let refused = send::send(second, None).await.unwrap_err();
    assert!(
        refused.message.contains("No secret is held"),
        "{}",
        refused.message
    );
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

/// The jq step runs in a child: the app's own binary (#2314), pointed at the one Cargo built beside the test.
fn jq_helper() {
    knobas_app::file_pipeline::use_helper(env!("CARGO_BIN_EXE_knobas-app"));
}

/// #2533: *Pipeline* on an answer runs the kept body's bytes through the file pipeline, in memory, as a file's are:
/// a gzipped base64 body (a compressed answer some APIs wrap) through `base64 decode → gunzip → jq`, the answer kept
/// as it came; once the body is no longer kept there is nothing to run.
#[tokio::test]
async fn a_kept_answer_runs_through_a_file_pipeline_from_memory() {
    use knobas_app::file_pipeline::{PipeOutput, PipeRun, PipeStep};
    use std::io::Write as _;
    jq_helper();
    let json = br#"{"items":[{"id":7,"name":"Rex"},{"id":8,"name":"Fido"}]}"#;
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    gz.write_all(json).unwrap();
    let wrapped = {
        use base64::Engine as _;
        base64::engine::general_purpose::STANDARD.encode(gz.finish().unwrap())
    };
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(wrapped.clone(), "text/plain"))
        .mount(&server)
        .await;
    send::send(
        request(253_301, "GET", format!("{}/pets", server.uri())),
        None,
    )
    .await
    .expect("sent");
    let steps = [
        PipeStep::Base64Decode,
        PipeStep::Gunzip,
        PipeStep::Jq {
            filter: ".items[1].name".into(),
        },
    ];
    let run = send::piped(253_301, "answer-200.txt", &steps, &[])
        .await
        .expect("kept");
    match run {
        PipeRun::Done {
            output: PipeOutput::Text { text },
            ..
        } => assert_eq!(text.trim(), "\"Fido\""),
        other => panic!("not text: {other:?}"),
    }
    // The run took nothing from the kept body: it is still the answer as it came.
    assert_eq!(
        send::body(253_301, false).as_deref(),
        Some(wrapped.as_bytes())
    );
    // A failed step is an answer, naming the step.
    let bad = send::piped(253_301, "answer-200.txt", &[PipeStep::Gunzip], &[])
        .await
        .expect("kept");
    assert!(
        matches!(&bad, PipeRun::Failed { failure } if failure.index == 0 && failure.name == "gunzip"),
        "{bad:?}"
    );
    // An answer never kept (or dropped from the 64 MiB) has nothing to run.
    assert!(send::piped(253_399, "x.json", &steps, &[]).await.is_none());
}

/// #2533, ADR-0057: an answer's body and what a pipeline makes of it stay in memory. Nothing on the way (the kept
/// store, the pipeline and its steps, the answer's run) opens a file to write: the only write is *Save to Downloads*,
/// which the person presses (`commands/api.rs`, `save_run`). A guard on the code, since a write would leave no trace a
/// test of the answer could see.
#[test]
fn nothing_on_an_answers_pipeline_writes_a_file() {
    const WRITES: [&str; 9] = [
        "fs::write",
        "File::create",
        "OpenOptions",
        "create_dir",
        "tempfile",
        "NamedTempFile",
        "fs::rename",
        "fs::copy",
        "tokio::fs",
    ];
    let piped = include_str!("../../src/api/send.rs");
    let start = piped.find("pub async fn piped(").expect("send::piped");
    let end = start + piped[start..].find("\n}\n").expect("its end");
    let files: [(&str, &str); 9] = [
        ("api/kept.rs", include_str!("../../src/api/kept.rs")),
        ("api/send.rs piped", &piped[start..end]),
        (
            "file_pipeline/mod.rs",
            include_str!("../../src/file_pipeline/mod.rs"),
        ),
        (
            "file_pipeline/child.rs",
            include_str!("../../src/file_pipeline/child.rs"),
        ),
        (
            "file_pipeline/jq.rs",
            include_str!("../../src/file_pipeline/jq.rs"),
        ),
        (
            "file_pipeline/session.rs",
            include_str!("../../src/file_pipeline/session.rs"),
        ),
        (
            "file_pipeline/steps.rs",
            include_str!("../../src/file_pipeline/steps.rs"),
        ),
        (
            "file_pipeline/values.rs",
            include_str!("../../src/file_pipeline/values.rs"),
        ),
        (
            "file_pipeline/saved.rs",
            include_str!("../../src/file_pipeline/saved.rs"),
        ),
    ];
    for (name, code) in files {
        for w in WRITES {
            assert!(
                !code.contains(w),
                "{name} holds {w}: an answer's bytes must not reach a file (ADR-0057)"
            );
        }
    }
}

//! A person's request (the API explorer's client, ADR-0057), against a server
//! on the loopback that answers one exchange the way each test needs.
//!
//! Written for the ux30 end gate's mutants: the client's own rules (the two
//! headers it sets itself, the body cap, the times, the stage a failure is
//! told at) had no test in this crate.

use std::time::Duration;

use knobas_http::person::{Client, ClientOptions, Failure, Request};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// What the server does with the one connection it takes.
enum Then {
    /// Wait this long after the request's head, then write the answer.
    Answer(Duration, Vec<u8>),
    /// Hang up without a word once the head is in.
    HangUp,
}

/// A one-exchange server; its URL and the request head it read.
async fn serve(then: Then) -> (reqwest::Url, tokio::sync::oneshot::Receiver<String>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let (heard, head) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let Ok((mut socket, _)) = listener.accept().await else {
            return;
        };
        let mut read = Vec::new();
        let mut buffer = [0u8; 2048];
        while !read.windows(4).any(|w| w == b"\r\n\r\n") {
            match socket.read(&mut buffer).await {
                Ok(0) | Err(_) => return,
                Ok(n) => read.extend_from_slice(&buffer[..n]),
            }
        }
        let _ = heard.send(String::from_utf8_lossy(&read).into_owned());
        match then {
            Then::Answer(wait, answer) => {
                tokio::time::sleep(wait).await;
                let _ = socket.write_all(&answer).await;
                let _ = socket.shutdown().await;
            }
            Then::HangUp => drop(socket),
        }
    });
    let url = format!("http://127.0.0.1:{port}/probe")
        .parse()
        .expect("url");
    (url, head)
}

fn ok_with(body: &str) -> Vec<u8> {
    format!(
        "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

fn get(url: reqwest::Url, headers: &[(&str, &str)]) -> Request {
    Request {
        method: reqwest::Method::GET,
        url,
        headers: headers
            .iter()
            .map(|(n, v)| ((*n).to_owned(), (*v).to_owned()))
            .collect(),
        body: None,
    }
}

fn client() -> Client {
    Client::new(&ClientOptions {
        direct: true,
        ..ClientOptions::default()
    })
    .expect("client")
}

/// A URL nothing answers on: port 1 on the loopback, refused at once (as
/// `transport.rs`'s `dead_url`).
fn dead_url() -> reqwest::Url {
    "http://127.0.0.1:1/".parse().expect("url")
}

async fn send(request: Request, max_body: usize) -> Result<knobas_http::person::Answer, Failure> {
    client()
        .send(
            request,
            Duration::from_secs(20),
            max_body,
            std::future::pending(),
        )
        .await
}

#[test]
fn each_failure_names_its_stage_as_the_page_reads_it() {
    let w = String::new;
    let stages = [
        (Failure::Resolve(w()), "resolve"),
        (Failure::Connect(w()), "connect"),
        (Failure::Tls(w()), "tls"),
        (Failure::Send(w()), "send"),
        (Failure::Read(w()), "read"),
        (Failure::Timeout, "timeout"),
        (Failure::Stopped, "stopped"),
    ];
    for (failure, stage) in stages {
        assert_eq!(failure.stage(), stage, "{failure:?}");
    }
}

/// A header a person set goes out as set, and a body as long as the cap
/// arrives whole, not marked cut.
#[tokio::test(flavor = "multi_thread")]
async fn a_persons_header_goes_out_and_a_body_at_the_cap_is_whole() {
    let (url, head) = serve(Then::Answer(Duration::ZERO, ok_with("hello"))).await;
    let answer = send(get(url, &[("X-Probe", "tide")]), 5)
        .await
        .expect("an answer");
    let head = head.await.expect("the server read the head").to_lowercase();
    assert!(head.contains("x-probe: tide"), "{head}");
    assert_eq!(answer.status, 200);
    assert_eq!(answer.reason, "OK");
    assert_eq!(&answer.body[..], b"hello");
    assert!(!answer.cut, "a body exactly at the cap is not cut");
    assert_eq!(answer.wire_bytes, 5);
}

/// One byte past the cap: the body stops at the cap and says it was cut.
#[tokio::test(flavor = "multi_thread")]
async fn a_body_past_the_cap_is_cut_there_and_says_so() {
    let (url, _head) = serve(Then::Answer(Duration::ZERO, ok_with("hello!"))).await;
    let answer = send(get(url, &[]), 5).await.expect("an answer");
    assert_eq!(&answer.body[..], b"hello");
    assert!(answer.cut);
    assert_eq!(answer.wire_bytes, 6, "the size the server said it sent");
}

/// `Host` and `Content-Length` are the client's: either one, set by a
/// person, ends the request before anything goes out. Sent at a dead port,
/// so a request that did go out would fail at `connect` instead.
#[tokio::test(flavor = "multi_thread")]
async fn host_and_content_length_are_refused_before_anything_goes_out() {
    for name in ["Host", "Content-Length"] {
        let failure = send(get(dead_url(), &[(name, "1")]), 64)
            .await
            .expect_err("refused");
        let Failure::Send(words) = &failure else {
            panic!("{name}: {failure:?}");
        };
        assert!(
            words.contains(&name.to_lowercase()),
            "{name}: the refusal names the header: {words}"
        );
    }
}

/// The times are the wall clock's, from the start of the send.
#[tokio::test(flavor = "multi_thread")]
async fn the_times_count_from_the_send() {
    let (url, _head) = serve(Then::Answer(Duration::from_millis(120), ok_with("x"))).await;
    let answer = send(get(url, &[]), 64).await.expect("an answer");
    assert!(answer.to_headers_ms >= 100, "{}", answer.to_headers_ms);
    assert!(answer.total_ms >= answer.to_headers_ms);
}

/// A refused connection is the `connect` stage, and its words carry every
/// cause under reqwest's own line, down to the refusal itself.
#[tokio::test(flavor = "multi_thread")]
async fn a_refused_connection_is_the_connect_stage_with_every_causes_words() {
    let failure = send(get(dead_url(), &[]), 64).await.expect_err("refused");
    let Failure::Connect(words) = &failure else {
        panic!("{failure:?}");
    };
    let lower = words.to_lowercase();
    assert!(lower.contains("error sending request"), "{words}");
    assert!(lower.contains("refused"), "{words}");
    // Each cause is said once.
    assert_eq!(lower.matches("error sending request").count(), 1, "{words}");
}

/// A server that hangs up on the request before answering: the request did
/// not get through, so it is the `send` stage, not `read`.
#[tokio::test(flavor = "multi_thread")]
async fn a_hang_up_before_any_answer_is_the_send_stage() {
    let (url, _head) = serve(Then::HangUp).await;
    let failure = send(get(url, &[]), 64).await.expect_err("no answer");
    assert!(matches!(failure, Failure::Send(_)), "{failure:?}");
}

/// An address whose scheme HTTP does not take never goes out: the `send`
/// stage, as a header refused before sending is.
#[tokio::test(flavor = "multi_thread")]
async fn an_address_http_does_not_take_is_the_send_stage() {
    let url = "ftp://127.0.0.1:1/file".parse().expect("url");
    let failure = send(get(url, &[]), 64).await.expect_err("refused");
    assert!(matches!(failure, Failure::Send(_)), "{failure:?}");
}

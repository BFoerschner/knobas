//! A wiremock stand-in for Uptime Kuma's `/metrics`.
//!
//! This adapter's contract source is the **real** pinned container
//! (ADR-0013), and `tests/live_kuma.rs` is where the acceptance criteria are
//! measured. This fake exists for one reason: `just check` and CI must stay
//! docker-free (roadmap §3), and the contract battery has to run on every
//! commit.
//!
//! **It serves a recording, not a fiction.** `support/metrics.txt` is what the
//! real Kuma answered on 2026-09-06, trimmed only by whole families -- so the
//! shapes the battery exercises here are the server's own, and the live suite
//! re-asserts them against it. If the two ever disagree, the recording is what
//! is out of date.

// One test binary declares `mod support;` today (`contract.rs`), and it does not
// call every item here -- `Fake::start` and `Fake::serving` are both used, but a
// helper added for the next binary would be dead until that binary exists. The
// allow is what keeps `clippy --all-targets -- -D warnings` from deciding that
// for us, and it is the same one the Gitea suite's support module carries.
#![allow(dead_code)]

use knobas_source::instance::SourceInstance;
use knobas_source::{AuthMethod, Source};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// The one API key the fake accepts. Anything else gets Kuma's 401.
pub const KEY: &str = "uk1_recorded-for-the-contract-battery";

/// The recorded document, served verbatim.
pub const METRICS: &str = include_str!("metrics.txt");

/// `Authorization` for an API key, as Kuma takes it: HTTP Basic with an empty
/// username. Spelled out here rather than built with a base64 crate, because
/// what this asserts is that the adapter sends *this exact header* -- a value
/// computed the same way the adapter computes it would agree with the adapter
/// about a scheme neither of them got right.
///
/// `base64(":uk1_recorded-for-the-contract-battery")`.
pub const AUTHORIZATION: &str = "Basic OnVrMV9yZWNvcmRlZC1mb3ItdGhlLWNvbnRyYWN0LWJhdHRlcnk=";

/// A Kuma serving the recording to the right key and a 401 to everything else.
pub struct Fake {
    server: MockServer,
}

impl Fake {
    pub async fn start() -> Self {
        Self::serving(METRICS).await
    }

    /// The same fake over a body of the caller's choosing -- how a test makes
    /// the remote system change between two sync runs.
    pub async fn serving(body: &str) -> Self {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/metrics"))
            .and(header("authorization", AUTHORIZATION))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(body)
                    // What Kuma answers with; the adapter reads the body as
                    // text and must not care, which is worth serving honestly.
                    .insert_header("content-type", "text/plain; charset=utf-8"),
            )
            .mount(&server)
            .await;
        // The fall-through, and it is the shape a wrong key really arrives in:
        // measured on the pinned image, a refused key is a `401` with
        // `WWW-Authenticate: Basic` and an **empty body**.
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(401)
                    .insert_header("www-authenticate", "Basic")
                    .set_body_string(""),
            )
            .mount(&server)
            .await;
        Self { server }
    }

    pub fn base_url(&self) -> String {
        self.server.uri()
    }
}

/// A port nothing is listening on -- the `Unreachable` case.
pub fn dead_url() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    format!("http://127.0.0.1:{port}")
}

/// One configured source pointing at `base_url`.
pub fn instance(base_url: String, key: &str) -> SourceInstance {
    SourceInstance {
        id: "kuma".to_owned(),
        kind: "kuma".to_owned(),
        display_name: "Uptime Kuma".to_owned(),
        base_url,
        auth: Some(AuthMethod::ApiToken),
        secret: Some(key.to_owned()),
        account: None,
        config: serde_json::json!({}),
    }
}

/// The adapter under test, built against the fake.
pub fn adapter(base_url: String, key: &str) -> Box<dyn Source> {
    knobas_source_kuma::build(instance(base_url, key)).expect("the adapter builds")
}

/// The same source **with an account** beside its key (issue #452) -- the
/// configuration that declares the write ops.
///
/// The account is never used against this fake: `/metrics` is all the
/// recording serves, and the socket.io channel is witnessed against the real
/// container by `just kuma-live`. What it is here for is the *declaration*,
/// which is a property of the built instance and needs no server at all.
pub fn adapter_with_account(base_url: String, key: &str) -> Box<dyn Source> {
    let with_account = SourceInstance {
        account: Some(knobas_source::instance::Account {
            username: "knobas".to_owned(),
            password: "knobas-dev".to_owned(),
        }),
        ..instance(base_url, key)
    };
    knobas_source_kuma::build(with_account).expect("the adapter builds")
}

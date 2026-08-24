//! The transport, exercised without a network.
//!
//! No server is spun up here: `knobas-mockd` (stream T) is what adapters test
//! against, and this crate's own risk is the *classification* -- getting 403
//! wrong is a source that looks broken instead of one that needs a password.

use knobas_http::{Auth, HttpClient, HttpConfig};
use knobas_source::SourceError;

fn config(base_url: String) -> HttpConfig {
    HttpConfig {
        base_url,
        adapter_kind: "test".to_owned(),
        adapter_version: "0.1.0".to_owned(),
        auth: Auth::Bearer("token".to_owned()),
        ..HttpConfig::default()
    }
}

/// §4.1: connect/DNS/TLS/timeout are `Unreachable` -- the class the scheduler
/// backs off on, as opposed to the one that needs a human.
#[tokio::test]
async fn a_refused_connection_is_unreachable() {
    // Bind and drop: the port is real, closed, and nobody else's.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    drop(listener);

    let client = HttpClient::new(config(format!("http://127.0.0.1:{port}"))).expect("client");
    let error = client
        .get_json::<serde_json::Value>("/rest/api/2/myself", &[])
        .await
        .expect_err("nothing is listening");
    assert!(matches!(error, SourceError::Unreachable(_)), "{error:?}");
}

/// A base URL that is not a URL is a configuration mistake, and it must fail
/// at construction rather than on the first request of the first sync.
#[tokio::test]
async fn a_bad_base_url_is_refused_up_front() {
    let error = HttpClient::new(config("not a url".to_owned())).expect_err("bad base url");
    assert!(matches!(error, SourceError::Protocol(_)), "{error:?}");
}

/// The secret reaches the wire and nothing else. `Auth` and `HttpClient` both
/// carry it, both are `Debug`, and a `tracing` field or a panic message is one
/// `{:?}` away -- so the redaction is a test, not a convention.
#[test]
fn a_secret_never_reaches_a_debug_rendering() {
    const SECRET: &str = "s3cr3t-token-value";

    let renderings = [
        format!("{:?}", Auth::Bearer(SECRET.to_owned())),
        format!("{:?}", Auth::GiteaToken(SECRET.to_owned())),
        format!(
            "{:?}",
            Auth::Basic {
                username: "someone".to_owned(),
                password: SECRET.to_owned(),
            }
        ),
        format!(
            "{:?}",
            HttpConfig {
                auth: Auth::Bearer(SECRET.to_owned()),
                ..config("https://example.test".to_owned())
            }
        ),
        format!(
            "{:?}",
            HttpClient::new(HttpConfig {
                auth: Auth::Bearer(SECRET.to_owned()),
                ..config("https://example.test".to_owned())
            })
            .expect("client")
        ),
    ];

    for rendering in renderings {
        assert!(!rendering.contains(SECRET), "{rendering}");
    }
}

/// Paths are appended, not `Url::join`ed: joining `"/api/v1/user"` onto
/// `"https://gitea.example/prefix"` silently drops `prefix`, and a source
/// behind a path prefix is exactly the setup nobody tests until production.
#[test]
fn a_path_is_appended_to_the_whole_base_url() {
    let client =
        HttpClient::new(config("https://example.test/prefix/".to_owned())).expect("client");
    assert_eq!(
        client.url_for("/api/v1/user"),
        "https://example.test/prefix/api/v1/user"
    );
    assert_eq!(
        client.url_for("api/v1/user"),
        "https://example.test/prefix/api/v1/user"
    );
}

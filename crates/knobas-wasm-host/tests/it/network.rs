//! The sandbox's way out (#1203), tested at the SPI: a `dyn Source` over the
//! fixture component, whose `test_connection` fetches what its config names
//! and answers everything it was handed. Local HTTP servers stand in for the
//! instance and the other hosts, and record what reached them.

use std::sync::{Arc, Mutex};

use knobas_source::instance::SourceInstance;
use knobas_source::{AuthMethod, Source, SourceError};
use knobas_wasm_host::allow::HostPattern;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const TOKEN: &str = "tok-1203-quayside-7f3a";
const PASSWORD: &str = "pw-1203-harbour-c41d";

/// One request a server saw: its path and its headers, lower-cased names.
#[derive(Debug, Clone)]
struct Seen {
    path: String,
    headers: Vec<(String, String)>,
}

impl Seen {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }
}

/// A local HTTP/1.1 server. `/redirect` answers 302 to `redirect_to`; every
/// other path echoes the request's headers back, in the body and in one
/// `x-echo-authorization` header, as a careless server would.
struct Server {
    url: String,
    seen: Arc<Mutex<Vec<Seen>>>,
}

impl Server {
    async fn start(redirect_to: Option<String>) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&seen);
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                let log = Arc::clone(&log);
                let redirect_to = redirect_to.clone();
                tokio::spawn(async move {
                    let mut buf = Vec::new();
                    let mut chunk = [0u8; 4096];
                    while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
                        match socket.read(&mut chunk).await {
                            Ok(0) | Err(_) => return,
                            Ok(n) => buf.extend_from_slice(&chunk[..n]),
                        }
                    }
                    let text = String::from_utf8_lossy(&buf).into_owned();
                    let mut lines = text.split("\r\n");
                    let path = lines
                        .next()
                        .and_then(|l| l.split(' ').nth(1))
                        .unwrap_or("/")
                        .to_owned();
                    let headers: Vec<(String, String)> = lines
                        .take_while(|l| !l.is_empty())
                        .filter_map(|l| l.split_once(':'))
                        .map(|(n, v)| (n.trim().to_ascii_lowercase(), v.trim().to_owned()))
                        .collect();
                    let seen = Seen { path, headers };
                    log.lock().unwrap().push(seen.clone());
                    let response = match (&*seen.path, &redirect_to) {
                        ("/redirect", Some(to)) => format!(
                            "HTTP/1.1 302 Found\r\nlocation: {to}\r\ncontent-length: 0\r\n\
                             connection: close\r\n\r\n"
                        ),
                        _ => {
                            let body: String = seen
                                .headers
                                .iter()
                                .map(|(n, v)| format!("{n}: {v}\n"))
                                .collect();
                            let auth = seen.header("authorization").unwrap_or("none");
                            format!(
                                "HTTP/1.1 200 OK\r\ncontent-type: text/plain\r\n\
                                 x-echo-authorization: {auth}\r\ncontent-length: {}\r\n\
                                 connection: close\r\n\r\n{body}",
                                body.len()
                            )
                        }
                    };
                    let _ = socket.write_all(response.as_bytes()).await;
                    let _ = socket.shutdown().await;
                });
            }
        });
        Self { url, seen }
    }

    fn seen(&self) -> Vec<Seen> {
        self.seen.lock().unwrap().clone()
    }
}

fn hosts(entries: &[&str]) -> Vec<HostPattern> {
    entries
        .iter()
        .map(|e| HostPattern::parse(e).unwrap())
        .collect()
}

fn instance(base_url: &str, auth: AuthMethod, config: serde_json::Value) -> SourceInstance {
    SourceInstance {
        id: "surveys".to_owned(),
        kind: "wasm-fixture".to_owned(),
        display_name: "Harbour surveys".to_owned(),
        base_url: base_url.to_owned(),
        auth: Some(auth),
        secret: Some(
            if auth == AuthMethod::UserPassword {
                PASSWORD
            } else {
                TOKEN
            }
            .to_owned(),
        ),
        account: None,
        config,
    }
}

fn build(patterns: &[&str], instance: SourceInstance) -> Box<dyn Source> {
    match knobas_wasm_host::load_fixture()
        .with_hosts(hosts(patterns))
        .build(instance)
    {
        Ok(source) => source,
        Err(e) => panic!("the fixture must build: {e:?}"),
    }
}

/// Everything the component answered, `Ok` or `Err`, as one text.
async fn answer(source: &dyn Source) -> (bool, String) {
    match source.test_connection().await {
        Ok(info) => (true, format!("{info:?}")),
        Err(error) => (false, format!("{error:?}")),
    }
}

#[tokio::test]
async fn a_request_to_an_undeclared_host_is_refused_before_it_leaves() {
    let instance_server = Server::start(None).await;
    let other = Server::start(None).await;
    let source = build(
        &["{base_url}"],
        instance(
            &instance_server.url,
            AuthMethod::Pat,
            serde_json::json!({ "fetch": format!("{}/data", other.url) }),
        ),
    );
    let error = source.test_connection().await.unwrap_err();
    assert!(
        matches!(&error, SourceError::Unreachable(text) if text.contains("ConnectionRefused")),
        "{error:?}"
    );
    assert!(other.seen().is_empty(), "{:?}", other.seen());
}

#[tokio::test]
async fn a_redirect_to_an_undeclared_host_is_refused() {
    let elsewhere = Server::start(None).await;
    let instance_server = Server::start(Some(format!("{}/landing", elsewhere.url))).await;
    let source = build(
        &["{base_url}"],
        instance(
            &instance_server.url,
            AuthMethod::Pat,
            serde_json::json!({ "fetch": format!("{}/redirect", instance_server.url) }),
        ),
    );
    let (ok, text) = answer(&*source).await;
    assert!(!ok && text.contains("ConnectionRefused"), "{text}");
    assert_eq!(instance_server.seen().len(), 1);
    assert!(elsewhere.seen().is_empty(), "{:?}", elsewhere.seen());
}

#[tokio::test]
async fn a_declared_host_that_is_not_the_instance_gets_no_credential() {
    let mirror = Server::start(None).await;
    let instance_server = Server::start(Some(format!("{}/landing", mirror.url))).await;
    let source = build(
        &["{base_url}", "{mirror}"],
        instance(
            &instance_server.url,
            AuthMethod::Pat,
            serde_json::json!({
                "fetch": format!("{}/redirect", instance_server.url),
                "mirror": mirror.url,
            }),
        ),
    );
    let (ok, text) = answer(&*source).await;
    assert!(ok, "{text}");
    let at_instance = instance_server.seen();
    assert_eq!(
        at_instance[0].header("authorization"),
        Some(format!("Bearer {TOKEN}").as_str())
    );
    let at_mirror = mirror.seen();
    assert_eq!(at_mirror.len(), 1, "{at_mirror:?}");
    assert_eq!(at_mirror[0].path, "/landing");
    assert_eq!(at_mirror[0].header("authorization"), None);
}

#[tokio::test]
async fn a_header_token_reaches_the_instance_and_never_the_component() {
    let server = Server::start(None).await;
    let source = build(
        &["{base_url}"],
        instance(
            &server.url,
            AuthMethod::Pat,
            serde_json::json!({ "fetch": format!("{}/whoami", server.url) }),
        ),
    );
    let (ok, text) = answer(&*source).await;
    assert!(ok, "{text}");
    assert_eq!(
        server.seen()[0].header("authorization"),
        Some(format!("Bearer {TOKEN}").as_str())
    );
    // The server echoed it in its body and a header; the component saw neither.
    assert!(text.contains("[redacted]"), "{text}");
    assert!(!text.contains(TOKEN), "{text}");
    assert!(!text.contains("x-echo-authorization"), "{text}");
}

#[tokio::test]
async fn basic_auth_reaches_the_instance_and_never_the_component() {
    let server = Server::start(None).await;
    let source = build(
        &["{base_url}"],
        instance(
            &server.url,
            AuthMethod::UserPassword,
            serde_json::json!({
                "fetch": format!("{}/whoami", server.url),
                "username": "harbourmaster",
            }),
        ),
    );
    let (ok, text) = answer(&*source).await;
    assert!(ok, "{text}");
    // base64("harbourmaster:pw-1203-harbour-c41d"), from `base64(1)`.
    let basic = "aGFyYm91cm1hc3Rlcjpwdy0xMjAzLWhhcmJvdXItYzQxZA==";
    assert_eq!(
        server.seen()[0].header("authorization"),
        Some(format!("Basic {basic}").as_str())
    );
    assert!(!text.contains(PASSWORD) && !text.contains(basic), "{text}");
}

/// The battery-level witness: whatever the component is handed (its
/// instance record and config, a response from the instance that echoes the
/// request, a refusal), the secret is in none of it, though the instance
/// did receive it.
#[tokio::test]
async fn the_secret_is_in_nothing_the_component_receives() {
    let elsewhere = Server::start(None).await;
    let server = Server::start(None).await;
    for fetch in [
        format!("{}/whoami", server.url),
        format!("{}/data", elsewhere.url),
    ] {
        let source = build(
            &["{base_url}"],
            instance(
                &server.url,
                AuthMethod::Pat,
                serde_json::json!({ "fetch": fetch }),
            ),
        );
        let (_, text) = answer(&*source).await;
        assert!(
            text.contains("instance"),
            "the fixture answers its instance: {text}"
        );
        assert!(!text.contains(TOKEN), "{text}");
        let descriptor = serde_json::to_string(&source.descriptor()).unwrap();
        assert!(!descriptor.contains(TOKEN), "{descriptor}");
    }
    assert!(
        server
            .seen()
            .iter()
            .any(|s| s.header("authorization").is_some_and(|a| a.contains(TOKEN)))
    );
}

/// The port a local server listens on.
fn port(server: &Server) -> u16 {
    server.url.rsplit(':').next().unwrap().parse().unwrap()
}

/// A source whose only reach is `entries`, fetching `fetch`, its address
/// somewhere no test listens.
fn fetching(entries: &[&str], fetch: &str, config: serde_json::Value) -> Box<dyn Source> {
    let mut config = config;
    config["fetch"] = serde_json::json!(fetch);
    build(
        entries,
        instance("https://instance.invalid", AuthMethod::Pat, config),
    )
}

const LOOPBACK: std::net::IpAddr = std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST);
/// TEST-NET-3 (RFC 5737): public as far as the guard can tell, and routed
/// nowhere.
const PUBLIC: std::net::IpAddr = std::net::IpAddr::V4(std::net::Ipv4Addr::new(203, 0, 113, 7));

/// DNS rebinding (#1244): a name the manifest fixed, which its owner points
/// at the reader's own machine, is refused before a socket opens.
#[tokio::test]
async fn a_fixed_host_that_resolves_to_loopback_is_refused_and_nothing_reaches_it() {
    let local = Server::start(None).await;
    knobas_wasm_host::fake_dns("rebound-1244.test", &[LOOPBACK]);
    let source = fetching(
        &[&format!("rebound-1244.test:{}", port(&local))],
        &format!("http://rebound-1244.test:{}/data", port(&local)),
        serde_json::json!({}),
    );
    let (ok, text) = answer(&*source).await;
    assert!(!ok && text.contains("ConnectionRefused"), "{text}");
    assert!(local.seen().is_empty(), "{:?}", local.seen());
}

/// Every answer counts, not the first: a name that resolves to a public
/// address and to loopback is refused whole.
#[tokio::test]
async fn a_fixed_host_with_one_public_and_one_loopback_answer_is_refused() {
    let local = Server::start(None).await;
    knobas_wasm_host::fake_dns("split-1244.test", &[PUBLIC, LOOPBACK]);
    let source = fetching(
        &[&format!("split-1244.test:{}", port(&local))],
        &format!("http://split-1244.test:{}/data", port(&local)),
        serde_json::json!({}),
    );
    let (ok, text) = answer(&*source).await;
    assert!(!ok && text.contains("ConnectionRefused"), "{text}");
    assert!(local.seen().is_empty(), "{:?}", local.seen());
}

/// The same through the system's own lookup, not `fake_dns`: `localhost.`
/// is a name other than `localhost`, and resolves to loopback.
#[tokio::test]
async fn a_fixed_name_the_system_resolves_to_loopback_is_refused() {
    let local = Server::start(None).await;
    let entry = format!("localhost.:{}", port(&local));
    let source = fetching(
        &[&entry],
        &format!("http://{entry}/data"),
        serde_json::json!({}),
    );
    let (ok, text) = answer(&*source).await;
    assert!(!ok && text.contains("ConnectionRefused"), "{text}");
    assert!(local.seen().is_empty(), "{:?}", local.seen());
}

/// The control for the refusals above: the same name, resolving the same way,
/// reaches the server when the reader typed it (a template's value).
#[tokio::test]
async fn a_templated_host_that_resolves_to_loopback_still_works() {
    let local = Server::start(None).await;
    knobas_wasm_host::fake_dns("typed-1244.test", &[LOOPBACK]);
    let url = format!("http://typed-1244.test:{}", port(&local));
    let source = fetching(
        &["{mirror}"],
        &format!("{url}/data"),
        serde_json::json!({ "mirror": url }),
    );
    let (ok, text) = answer(&*source).await;
    assert!(ok, "{text}");
    assert_eq!(local.seen().len(), 1, "{:?}", local.seen());
}

/// The source's own address is one the reader typed, local or not.
#[tokio::test]
async fn a_base_url_on_loopback_still_works() {
    let local = Server::start(None).await;
    let source = build(
        &["{base_url}"],
        instance(
            &local.url,
            AuthMethod::Pat,
            serde_json::json!({ "fetch": format!("{}/data", local.url) }),
        ),
    );
    let (ok, text) = answer(&*source).await;
    assert!(ok, "{text}");
    assert_eq!(local.seen().len(), 1, "{:?}", local.seen());
}

/// The manifest's author wrote the local address itself: the install list
/// showed it to the reader.
#[tokio::test]
async fn a_fixed_loopback_address_or_localhost_still_works() {
    let local = Server::start(None).await;
    for host in ["127.0.0.1", "localhost"] {
        let entry = format!("{host}:{}", port(&local));
        let source = fetching(
            &[&entry],
            &format!("http://{entry}/data"),
            serde_json::json!({}),
        );
        let (ok, text) = answer(&*source).await;
        assert!(ok, "{host}: {text}");
    }
    assert_eq!(local.seen().len(), 2, "{:?}", local.seen());
}

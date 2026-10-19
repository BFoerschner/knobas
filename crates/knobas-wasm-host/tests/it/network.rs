//! The sandbox's way out (#1203), tested at the SPI: a `dyn Source` over the
//! fixture component, whose `test_connection` fetches what its config names
//! and answers everything it was handed. Local HTTP servers stand in for the
//! instance and the other hosts, and record what reached them.

use std::sync::{Arc, Mutex};
use std::time::Duration;

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

/// A system proxy on the reader's machine (#1283): it records the request
/// line of each request it carries, and forwards every one to `to`, as a
/// proxy that resolves the request's name to that address would. It reads
/// only up to the end of the request line; the rest is copied through.
struct Proxy {
    url: String,
    carried: Arc<Mutex<Vec<String>>>,
}

impl Proxy {
    async fn start(to: &Server) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let to = to.url.trim_start_matches("http://").to_owned();
        let carried = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&carried);
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                let log = Arc::clone(&log);
                let to = to.clone();
                tokio::spawn(async move {
                    let mut head = Vec::new();
                    let mut chunk = [0u8; 4096];
                    while !head.windows(2).any(|w| w == b"\r\n") {
                        match socket.read(&mut chunk).await {
                            Ok(0) | Err(_) => return,
                            Ok(n) => head.extend_from_slice(&chunk[..n]),
                        }
                    }
                    let text = String::from_utf8_lossy(&head);
                    log.lock()
                        .unwrap()
                        .push(text.split("\r\n").next().unwrap_or_default().to_owned());
                    let mut upstream = tokio::net::TcpStream::connect(&to).await.unwrap();
                    upstream.write_all(&head).await.unwrap();
                    let _ = tokio::io::copy_bidirectional(&mut socket, &mut upstream).await;
                });
            }
        });
        Self { url, carried }
    }

    fn carried(&self) -> Vec<String> {
        self.carried.lock().unwrap().clone()
    }
}

/// Through a system proxy the proxy resolves the name, so the transport's
/// guard never sees it: the name is checked before the request is handed
/// over, and the reader's own machine is not reached by way of the proxy.
#[tokio::test]
async fn a_fixed_host_that_resolves_to_loopback_is_refused_through_a_proxy() {
    let local = Server::start(None).await;
    let proxy = Proxy::start(&local).await;
    knobas_wasm_host::fake_dns("proxied-1283.test", &[LOOPBACK]);
    knobas_wasm_host::fake_proxy("proxied-1283.test", &proxy.url);
    let source = fetching(
        &[&format!("proxied-1283.test:{}", port(&local))],
        &format!("http://proxied-1283.test:{}/data", port(&local)),
        serde_json::json!({}),
    );
    let (ok, text) = answer(&*source).await;
    assert!(!ok && text.contains("ConnectionRefused"), "{text}");
    assert!(proxy.carried().is_empty(), "{:?}", proxy.carried());
    assert!(local.seen().is_empty(), "{:?}", local.seen());
}

/// A fixed host that resolves to a public address still goes through the
/// proxy, not around it: a reader whose only way out is the proxy keeps it.
#[tokio::test]
async fn a_fixed_public_host_still_goes_through_the_proxy() {
    let local = Server::start(None).await;
    let proxy = Proxy::start(&local).await;
    knobas_wasm_host::fake_dns("public-1283.test", &[PUBLIC]);
    knobas_wasm_host::fake_proxy("public-1283.test", &proxy.url);
    let source = fetching(
        &["public-1283.test"],
        "http://public-1283.test/data",
        serde_json::json!({}),
    );
    let (ok, text) = answer(&*source).await;
    assert!(ok, "{text}");
    assert_eq!(
        proxy.carried(),
        ["GET http://public-1283.test/data HTTP/1.1"]
    );
}

/// A network whose only way out is the proxy may answer no name to the
/// reader's machine: a fixed host knobas cannot resolve is handed to the
/// proxy, which can. (`.invalid` never resolves, RFC 6761.)
#[tokio::test]
async fn a_fixed_host_knobas_cannot_resolve_still_goes_through_the_proxy() {
    let local = Server::start(None).await;
    let proxy = Proxy::start(&local).await;
    knobas_wasm_host::fake_proxy("unresolved-1283.invalid", &proxy.url);
    let source = fetching(
        &["unresolved-1283.invalid"],
        "http://unresolved-1283.invalid/data",
        serde_json::json!({}),
    );
    let (ok, text) = answer(&*source).await;
    assert!(ok, "{text}");
    assert_eq!(
        proxy.carried(),
        ["GET http://unresolved-1283.invalid/data HTTP/1.1"]
    );
}

/// A template's value is the reader's to point home: it goes through the
/// proxy as before, unchecked.
#[tokio::test]
async fn a_templated_host_that_resolves_to_loopback_goes_through_the_proxy() {
    let local = Server::start(None).await;
    let proxy = Proxy::start(&local).await;
    knobas_wasm_host::fake_dns("typed-1283.test", &[LOOPBACK]);
    knobas_wasm_host::fake_proxy("typed-1283.test", &proxy.url);
    let url = format!("http://typed-1283.test:{}", port(&local));
    let source = fetching(
        &["{mirror}"],
        &format!("{url}/data"),
        serde_json::json!({ "mirror": url }),
    );
    let (ok, text) = answer(&*source).await;
    assert!(ok, "{text}");
    assert_eq!(proxy.carried(), [format!("GET {url}/data HTTP/1.1")]);
    assert_eq!(local.seen().len(), 1, "{:?}", local.seen());
}

/// A network whose DNS drops outside queries never answers the lookup: the
/// wait before the proxy is bounded, not the system's lookup timeout (#1290).
#[tokio::test]
async fn a_fixed_host_whose_lookup_never_answers_reaches_the_proxy_within_the_bound() {
    let local = Server::start(None).await;
    let proxy = Proxy::start(&local).await;
    knobas_wasm_host::fake_dns_hangs("silent-1290.test");
    knobas_wasm_host::fake_proxy("silent-1290.test", &proxy.url);
    let source = fetching(
        &["silent-1290.test"],
        "http://silent-1290.test/data",
        serde_json::json!({}),
    );
    let (ok, text) = tokio::time::timeout(Duration::from_secs(10), answer(&*source))
        .await
        .expect("the request waited on the lookup past the bound");
    assert!(ok, "{text}");
    assert_eq!(
        proxy.carried(),
        ["GET http://silent-1290.test/data HTTP/1.1"]
    );
}

/// The component's own timeouts do not shorten the lookup before hand-off: a
/// short first-byte timeout would otherwise time the lookup out and send a
/// name that points home on to the proxy (#1290).
#[tokio::test]
async fn a_short_component_timeout_does_not_cut_the_lookup_before_the_proxy() {
    let local = Server::start(None).await;
    let proxy = Proxy::start(&local).await;
    knobas_wasm_host::fake_dns("slow-1290.test", &[LOOPBACK]);
    knobas_wasm_host::fake_dns_slow("slow-1290.test", Duration::from_millis(300));
    knobas_wasm_host::fake_proxy("slow-1290.test", &proxy.url);
    let source = fetching(
        &["slow-1290.test"],
        "http://slow-1290.test/data",
        serde_json::json!({ "first_byte_ms": 20 }),
    );
    let (ok, text) = answer(&*source).await;
    assert!(!ok && text.contains("ConnectionRefused"), "{text}");
    assert!(proxy.carried().is_empty(), "{:?}", proxy.carried());
    assert!(local.seen().is_empty(), "{:?}", local.seen());
}

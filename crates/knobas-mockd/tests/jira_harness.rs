use knobas_mockd::{JIRA_TOKEN, MockFault, ViolationKind, spawn_mock_jira};

fn client() -> reqwest::Client {
    reqwest::Client::builder().build().unwrap()
}

async fn get(base: &str, path: &str) -> reqwest::Response {
    client()
        .get(format!("{base}{path}"))
        .header("Authorization", format!("Bearer {JIRA_TOKEN}"))
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn server_info_and_myself_answer_the_documented_shapes() {
    let s = spawn_mock_jira().await;
    let base = s.base_url();

    let si: serde_json::Value = get(&base, "/rest/api/2/serverInfo")
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(si["baseUrl"], base);
    assert_eq!(si["deploymentType"], "Server");
    assert!(si["version"].as_str().unwrap().starts_with("9.17"));
    // The DC datetime format, not RFC 3339 (see state.rs) -- and NOT on UTC.
    // This offset is the only place an adapter can learn the server's zone,
    // and JQL date literals are interpreted in it.
    let t = si["serverTime"].as_str().unwrap();
    assert!(t.ends_with("+0200"), "serverTime was {t}");
    assert!(si["buildDate"].as_str().unwrap().ends_with("+0200"));

    let me: serde_json::Value = get(&base, "/rest/api/2/myself").await.json().await.unwrap();
    assert_eq!(me["name"], "mara.lindqvist");
    assert_eq!(me["displayName"], "Mara Lindqvist");
    assert_eq!(me["active"], true);

    s.assert_no_violations();
}

#[tokio::test]
async fn a_request_without_credentials_is_a_seraph_401() {
    let s = spawn_mock_jira().await;
    let r = client()
        .get(format!("{}/rest/api/2/myself", s.base_url()))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 401);
    assert_eq!(r.headers()["X-Seraph-LoginReason"], "AUTHENTICATED_FAILED");
    let body: serde_json::Value = r.json().await.unwrap();
    assert!(!body["errorMessages"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn the_unauthorized_fault_401s_a_well_formed_request() {
    let s = spawn_mock_jira().await;
    s.set_fault(MockFault::Unauthorized);
    let r = get(&s.base_url(), "/rest/api/2/myself").await;
    assert_eq!(r.status(), 401);
    assert_eq!(r.headers()["X-Seraph-LoginReason"], "AUTHENTICATED_FAILED");
    // A well-formed request is not a contract violation, whatever it answers.
    s.assert_no_violations();

    s.set_fault(MockFault::None);
    assert_eq!(get(&s.base_url(), "/rest/api/2/myself").await.status(), 200);
}

#[tokio::test]
async fn the_rate_limited_fault_sets_retry_after() {
    let s = spawn_mock_jira().await;
    s.set_fault(MockFault::RateLimited {
        retry_after_secs: 7,
    });
    let r = get(&s.base_url(), "/rest/api/2/myself").await;
    assert_eq!(r.status(), 429);
    assert_eq!(r.headers()["Retry-After"], "7");
}

#[tokio::test]
async fn the_cloud_dialect_is_a_violation_not_a_404_shrug() {
    let s = spawn_mock_jira().await;
    let r = get(&s.base_url(), "/rest/api/3/search/jql?jql=order+by+updated").await;
    assert_eq!(r.status(), 404);
    let v = s.violations();
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].kind, ViolationKind::UnknownPath);
    assert!(v[0].path.contains("search/jql"));
}

#[tokio::test]
async fn an_undeclared_query_parameter_is_a_400_and_a_violation() {
    let s = spawn_mock_jira().await;
    let r = get(&s.base_url(), "/rest/api/2/search?jql=&nextPageToken=abc").await;
    assert_eq!(r.status(), 400);
    let v = s.violations();
    assert_eq!(v[0].kind, ViolationKind::UnknownQueryParam);
    assert!(v[0].detail.contains("nextPageToken"));
}

#[tokio::test]
async fn a_wrong_verb_on_a_known_path_is_405() {
    let s = spawn_mock_jira().await;
    let r = client()
        .delete(format!("{}/rest/api/2/search", s.base_url()))
        .header("Authorization", "Bearer t")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 405);
    assert!(r.headers()["Allow"].to_str().unwrap().contains("GET"));
    assert_eq!(s.violations()[0].kind, ViolationKind::UnknownMethod);
}

#[tokio::test]
async fn a_documented_but_unimplemented_endpoint_is_501_with_a_hint() {
    let s = spawn_mock_jira().await;
    let r = get(&s.base_url(), "/rest/api/2/dashboard").await;
    assert_eq!(r.status(), 501);
    assert!(r.headers().contains_key("X-Mockd-Hint"));
    assert_eq!(s.violations()[0].kind, ViolationKind::Unimplemented);
}

#[tokio::test]
async fn a_path_without_the_rest_prefix_is_an_unknown_path() {
    let s = spawn_mock_jira().await;
    let r = get(&s.base_url(), "/api/2/myself").await;
    assert_eq!(r.status(), 404);
    assert_eq!(s.violations()[0].kind, ViolationKind::UnknownPath);
}

#[tokio::test]
async fn dropping_the_guard_stops_the_server() {
    let s = spawn_mock_jira().await;
    let base = s.base_url();
    assert_eq!(get(&base, "/rest/api/2/myself").await.status(), 200);
    // Bounded: a server that ignored the shutdown signal must fail this test
    // by name, not hang the whole binary.
    tokio::time::timeout(std::time::Duration::from_secs(5), s.stop())
        .await
        .expect("stop() must return once the server has shut down");
    let err = client()
        .get(format!("{base}/rest/api/2/myself"))
        .timeout(std::time::Duration::from_secs(2))
        .send()
        .await;
    assert!(err.is_err(), "the listener outlived its guard");
}

#[tokio::test]
async fn refused_url_points_at_nothing() {
    let url = knobas_mockd::refused_url();
    let err = client()
        .get(format!("{url}/rest/api/2/myself"))
        .timeout(std::time::Duration::from_secs(2))
        .send()
        .await;
    assert!(err.is_err(), "{url} answered");
}

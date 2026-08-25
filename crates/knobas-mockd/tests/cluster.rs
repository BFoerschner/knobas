//! One state, two servers, and the `/__mock/*` admin API that drives it over
//! HTTP for the compose environment (where a test cannot reach the typed
//! [`MockState`](knobas_mockd::MockState) at all).

use knobas_mockd::spawn_all;

async fn json(r: reqwest::Response) -> serde_json::Value {
    r.json().await.unwrap()
}

#[tokio::test]
async fn one_state_backs_both_servers_and_the_admin_api_drives_it() {
    let c = spawn_all().await;
    let http = reqwest::Client::new();

    // The admin API is reachable on each server's own port.
    let h = json(
        http.get(format!("{}/__mock/health", c.jira.base_url()))
            .send()
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(h["ok"], true);
    assert_eq!(h["apis"], serde_json::json!(["jira", "teamcity"]));
    assert_eq!(h["fixture_today"], "2026-08-22T14:32:00Z");

    // Touch through HTTP, observe through the typed state -- same object.
    let before = c.state().issue("PAY-240").unwrap().updated;
    let r = http
        .post(format!(
            "{}/__mock/jira/issue/PAY-240/touch",
            c.jira.base_url()
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    assert!(c.state().issue("PAY-240").unwrap().updated > before);

    // A TeamCity build and a Jira issue agree about the branch.
    let b = json(
        http.get(format!(
            "{}/app/rest/builds/id:1187?fields=branchName",
            c.teamcity.base_url()
        ))
        .header("Accept", "application/json")
        .header("Authorization", "Bearer t")
        .send()
        .await
        .unwrap(),
    )
    .await;
    assert!(b["branchName"].as_str().unwrap().contains("PAY-231"));
    assert!(c.state().issue("PAY-231").is_some());

    c.assert_no_violations();
    c.stop().await;
}

#[tokio::test]
async fn each_server_owns_its_own_links_despite_the_shared_state() {
    // The trap of mounting two routers over one state: whichever bound last
    // would otherwise own every `self`/`webUrl` link in the other's bodies.
    let c = spawn_all().await;
    let http = reqwest::Client::new();

    let si = json(
        http.get(format!("{}/rest/api/2/serverInfo", c.jira.base_url()))
            .header("Authorization", "Bearer t")
            .send()
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(si["baseUrl"], c.jira.base_url());

    let srv = json(
        http.get(format!("{}/app/rest/server", c.teamcity.base_url()))
            .header("Accept", "application/json")
            .header("Authorization", "Bearer t")
            .send()
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(srv["webUrl"], c.teamcity.base_url());
    assert_ne!(c.jira.base_url(), c.teamcity.base_url());
    c.assert_no_violations();
}

#[tokio::test]
async fn violations_are_readable_and_clearable_over_http() {
    let c = spawn_all().await;
    let http = reqwest::Client::new();
    http.get(format!("{}/rest/api/3/search/jql", c.jira.base_url()))
        .header("Authorization", "Bearer t")
        .send()
        .await
        .unwrap();

    let v = json(
        http.get(format!("{}/__mock/violations", c.jira.base_url()))
            .send()
            .await
            .unwrap(),
    )
    .await;
    let rows = v.as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["kind"], "unknown_path");
    assert_eq!(rows[0]["path"], "/rest/api/3/search/jql");

    http.delete(format!("{}/__mock/violations", c.jira.base_url()))
        .send()
        .await
        .unwrap();
    c.assert_no_violations();
}

#[tokio::test]
async fn a_fault_and_a_config_change_arrive_over_http() {
    let c = spawn_all().await;
    let http = reqwest::Client::new();

    // The cap first, while the server still answers normally.
    let r = http
        .post(format!("{}/__mock/config", c.jira.base_url()))
        .json(&serde_json::json!({ "jira_max_results_cap": 2 }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let page = json(
        http.get(format!(
            "{}/rest/api/2/search?jql=&maxResults=100",
            c.jira.base_url()
        ))
        .header("Authorization", "Bearer t")
        .send()
        .await
        .unwrap(),
    )
    .await;
    assert_eq!(page["maxResults"], 2);
    assert_eq!(page["issues"].as_array().unwrap().len(), 2);
    assert_eq!(page["total"], 7, "the cap pages, it does not filter");

    // Then the fault, which the *other* server must feel too -- one state.
    let r = http
        .post(format!("{}/__mock/fault", c.jira.base_url()))
        .json(&serde_json::json!({ "kind": "rate_limited", "retry_after_secs": 5 }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let tc = http
        .get(format!("{}/app/rest/server", c.teamcity.base_url()))
        .header("Accept", "application/json")
        .header("Authorization", "Bearer t")
        .send()
        .await
        .unwrap();
    assert_eq!(tc.status(), 429);
    assert_eq!(tc.headers()["retry-after"], "5");

    // `none` clears it again.
    http.post(format!("{}/__mock/fault", c.teamcity.base_url()))
        .json(&serde_json::json!({ "kind": "none" }))
        .send()
        .await
        .unwrap();
    let ok = http
        .get(format!("{}/app/rest/server", c.teamcity.base_url()))
        .header("Accept", "application/json")
        .header("Authorization", "Bearer t")
        .send()
        .await
        .unwrap();
    assert_eq!(ok.status(), 200);
    c.assert_no_violations();
}

#[tokio::test]
async fn reset_restores_the_fixture_over_http() {
    let c = spawn_all().await;
    let http = reqwest::Client::new();
    let before = c.state().issue("PAY-240").unwrap().updated;
    c.state()
        .finish_build(1188, knobas_mockd::TcStatus::Failure);
    c.state().touch_issue("PAY-240");

    let r = http
        .post(format!("{}/__mock/reset", c.teamcity.base_url()))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(c.state().issue("PAY-240").unwrap().updated, before);
    assert_eq!(
        c.state().build(1188).unwrap().state,
        knobas_mockd::TcState::Running,
        "reset restores the TeamCity half too"
    );
    c.assert_no_violations();
}

#[tokio::test]
async fn the_admin_api_is_exempt_from_the_product_middlewares() {
    // No Authorization, no Accept: application/json -- and no violation for
    // either, because /__mock/* is not part of any vendored contract.
    let c = spawn_all().await;
    for base in [c.jira.base_url(), c.teamcity.base_url()] {
        let r = reqwest::Client::new()
            .get(format!("{base}/__mock/health"))
            .header("Accept", "*/*")
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200, "{base}");
    }
    c.assert_no_violations();
}

#[tokio::test]
async fn touching_an_issue_the_fixture_does_not_have_is_404() {
    let c = spawn_all().await;
    let r = reqwest::Client::new()
        .post(format!(
            "{}/__mock/jira/issue/NOPE-1/touch",
            c.jira.base_url()
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 404, "a silent 200 would hide a driver's typo");
    c.assert_no_violations();
}

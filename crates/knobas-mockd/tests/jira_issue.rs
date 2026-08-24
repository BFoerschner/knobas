use knobas_mockd::spawn_mock_jira;

async fn get(base: &str, path: &str) -> (reqwest::StatusCode, serde_json::Value) {
    let r = reqwest::Client::new()
        .get(format!("{base}{path}"))
        .header("Authorization", "Bearer t")
        .send()
        .await
        .unwrap();
    let st = r.status();
    (st, r.json().await.unwrap_or(serde_json::Value::Null))
}

#[tokio::test]
async fn an_issue_reads_by_key_with_comments_and_worklogs_on_demand() {
    let s = spawn_mock_jira().await;
    let (st, v) = get(&s.base_url(), "/rest/api/2/issue/PAY-231?fields=*all").await;
    assert_eq!(st, 200);
    assert_eq!(v["key"], "PAY-231");
    assert_eq!(v["id"], "10001", "Jira serialises ids as strings");
    assert_eq!(v["fields"]["summary"], "Retry failed SEPA payouts");
    assert_eq!(v["fields"]["comment"]["total"], 2);
    assert_eq!(v["fields"]["worklog"]["total"], 1);
    assert_eq!(
        v["fields"]["worklog"]["worklogs"][0]["timeSpentSeconds"],
        270 * 60
    );
    s.assert_no_violations();
}

#[tokio::test]
async fn an_issue_reads_by_numeric_id_too() {
    // `{issueIdOrKey}`: an adapter that kept the id from a search response and
    // fetched by it must work.
    let s = spawn_mock_jira().await;
    let (st, v) = get(&s.base_url(), "/rest/api/2/issue/10001").await;
    assert_eq!(st, 200);
    assert_eq!(v["key"], "PAY-231");
    s.assert_no_violations();
}

#[tokio::test]
async fn an_unknown_key_is_a_jira_404_not_a_violation() {
    let s = spawn_mock_jira().await;
    let (st, v) = get(&s.base_url(), "/rest/api/2/issue/PAY-999").await;
    assert_eq!(st, 404);
    assert!(v["errorMessages"][0].as_str().unwrap().contains("PAY-999"));
    // The path is in the contract; only the resource is missing.
    s.assert_no_violations();
}

#[tokio::test]
async fn comments_and_worklogs_page_on_their_own_endpoints() {
    let s = spawn_mock_jira().await;
    let (_, c) = get(
        &s.base_url(),
        "/rest/api/2/issue/PAY-231/comment?startAt=1&maxResults=1",
    )
    .await;
    assert_eq!(c["total"], 2);
    assert_eq!(c["startAt"], 1);
    assert_eq!(c["comments"].as_array().unwrap().len(), 1);
    assert_eq!(c["comments"][0]["author"]["name"], "mara.lindqvist");

    let (_, w) = get(&s.base_url(), "/rest/api/2/issue/PAY-231/worklog").await;
    assert_eq!(w["total"], 1);
    assert_eq!(w["startAt"], 0);
    assert_eq!(w["worklogs"][0]["comment"], "Retry loop implementation");
    s.assert_no_violations();
}

#[tokio::test]
async fn the_worklog_endpoint_takes_no_query_parameters() {
    // The WADL declares none. An allowlist that invented startAt/maxResults
    // here would let an adapter page an endpoint that does not page.
    let s = spawn_mock_jira().await;
    let (st, _) = get(&s.base_url(), "/rest/api/2/issue/PAY-231/worklog?startAt=0").await;
    assert_eq!(st, 400);
    assert_eq!(
        s.violations()[0].kind,
        knobas_mockd::ViolationKind::UnknownQueryParam
    );
}

#[tokio::test]
async fn a_posted_comment_shows_up_in_later_gets() {
    let s = spawn_mock_jira().await;
    let r = reqwest::Client::new()
        .post(format!("{}/rest/api/2/issue/PAY-231/comment", s.base_url()))
        .header("Authorization", "Bearer t")
        .json(&serde_json::json!({ "body": "retested on staging" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 201);
    let created: serde_json::Value = r.json().await.unwrap();
    assert_eq!(created["body"], "retested on staging");
    assert_eq!(created["author"]["name"], "mara.lindqvist");

    let (_, c) = get(&s.base_url(), "/rest/api/2/issue/PAY-231/comment").await;
    assert_eq!(c["total"], 3);
    assert_eq!(c["comments"][2]["id"], created["id"]);
    s.assert_no_violations();
}

#[tokio::test]
async fn a_comment_posted_to_an_unknown_issue_is_a_404() {
    let s = spawn_mock_jira().await;
    let r = reqwest::Client::new()
        .post(format!("{}/rest/api/2/issue/PAY-999/comment", s.base_url()))
        .header("Authorization", "Bearer t")
        .json(&serde_json::json!({ "body": "nobody home" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 404);
    s.assert_no_violations();
}

#[tokio::test]
async fn comment_order_by_is_honoured_and_an_unknown_ordering_is_refused() {
    let s = spawn_mock_jira().await;
    let (_, oldest) = get(
        &s.base_url(),
        "/rest/api/2/issue/PAY-231/comment?orderBy=created",
    )
    .await;
    assert_eq!(oldest["comments"][0]["author"]["name"], "priya.nair");

    let (_, newest) = get(
        &s.base_url(),
        "/rest/api/2/issue/PAY-231/comment?orderBy=-created",
    )
    .await;
    assert_eq!(newest["comments"][0]["author"]["name"], "mara.lindqvist");
    s.assert_no_violations();

    // The parameter is declared by the WADL, so the allowlist lets it through;
    // a value mockd does not implement must still not be silently ignored.
    let (st, _) = get(
        &s.base_url(),
        "/rest/api/2/issue/PAY-231/comment?orderBy=assignee",
    )
    .await;
    assert_eq!(st, 400);
    assert_eq!(
        s.violations()[0].kind,
        knobas_mockd::ViolationKind::UnsupportedQuery
    );
}

#[tokio::test]
async fn a_non_integer_paging_parameter_on_comments_is_a_400_and_a_violation() {
    let s = spawn_mock_jira().await;
    let (st, body) = get(
        &s.base_url(),
        "/rest/api/2/issue/PAY-231/comment?maxResults=lots",
    )
    .await;
    assert_eq!(st, 400);
    assert!(
        body["errorMessages"][0]
            .as_str()
            .unwrap()
            .contains("maxResults"),
        "body was {body}"
    );
    assert_eq!(
        s.violations()[0].kind,
        knobas_mockd::ViolationKind::UnsupportedQuery
    );
}

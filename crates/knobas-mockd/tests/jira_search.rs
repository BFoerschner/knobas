use knobas_mockd::{ViolationKind, spawn_mock_jira};

async fn search(base: &str, q: &str) -> (reqwest::StatusCode, serde_json::Value) {
    let r = reqwest::Client::new()
        .get(format!("{base}/rest/api/2/search?{q}"))
        .header(
            "Authorization",
            format!("Bearer {}", knobas_mockd::JIRA_TOKEN),
        )
        .send()
        .await
        .unwrap();
    let st = r.status();
    (st, r.json().await.unwrap_or(serde_json::Value::Null))
}

fn keys(v: &serde_json::Value) -> Vec<String> {
    v["issues"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["key"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn an_empty_jql_returns_every_issue_newest_first_by_default() {
    let s = spawn_mock_jira().await;
    let (st, v) = search(&s.base_url(), "jql=&maxResults=100").await;
    assert_eq!(st, 200);
    assert_eq!(v["total"], 7);
    assert_eq!(v["startAt"], 0);
    assert_eq!(keys(&v).len(), 7);
    assert_eq!(keys(&v).first().unwrap(), "PAY-231", "newest first");
    assert_eq!(keys(&v).last().unwrap(), "PAY-200", "oldest last");
    s.assert_no_violations();
}

#[tokio::test]
async fn a_bare_order_by_with_no_clause_parses_and_sorts() {
    // This is stream A's full-sync query: no filter, just an ordering.
    let s = spawn_mock_jira().await;
    let q = "jql=ORDER%20BY%20updated%20ASC&maxResults=100";
    let (st, v) = search(&s.base_url(), q).await;
    assert_eq!(st, 200);
    let ks = keys(&v);
    assert_eq!(
        ks.first().unwrap(),
        "PAY-200",
        "the synthesized-updated epic sorts oldest"
    );
    assert_eq!(
        ks.last().unwrap(),
        "PAY-231",
        "11:48 UTC on the fixture's today is newest"
    );

    let (_, desc) = search(
        &s.base_url(),
        "jql=order%20by%20updated%20desc&maxResults=100",
    )
    .await;
    let mut rev = ks.clone();
    rev.reverse();
    assert_eq!(
        keys(&desc),
        rev,
        "keywords are case-insensitive and DESC reverses"
    );
    s.assert_no_violations();
}

#[tokio::test]
async fn project_in_filters() {
    let s = spawn_mock_jira().await;
    let (_, v) = search(&s.base_url(), "jql=project%20in%20(PAY)&maxResults=100").await;
    assert_eq!(v["total"], 6);
    assert!(!keys(&v).contains(&"OPS-77".to_string()));

    // `project = OPS` is the other accepted spelling.
    let (_, v) = search(&s.base_url(), "jql=project%20%3D%20OPS&maxResults=100").await;
    assert_eq!(keys(&v), vec!["OPS-77"]);
    s.assert_no_violations();
}

#[tokio::test]
async fn updated_gte_reads_literals_in_the_server_zone() {
    let s = spawn_mock_jira().await;
    // PAY-231 is updated 11:48:00 UTC = 13:48 on this +02:00 server, and the
    // boundary literal must include it.
    let q = "jql=updated%20%3E%3D%20%222026-08-22%2013%3A48%22%20ORDER%20BY%20updated%20ASC";
    let (_, v) = search(&s.base_url(), &format!("{q}&maxResults=100")).await;
    assert_eq!(keys(&v), vec!["PAY-231"]);

    // One minute later: nothing.
    let q = "jql=updated%20%3E%3D%20%222026-08-22%2013%3A49%22";
    let (_, v) = search(&s.base_url(), &format!("{q}&maxResults=100")).await;
    assert_eq!(v["total"], 0);

    // A naive literal is resolved in the server's zone, never in UTC. `10:00`
    // here means 08:00 UTC and therefore sweeps up PAY-240 (08:12 UTC); read as
    // UTC it would mean 10:00 UTC and return PAY-231 alone. This is exactly the
    // two-hour error a UTC-on-UTC mock would hide.
    let q = "jql=updated%20%3E%3D%20%222026-08-22%2010%3A00%22%20ORDER%20BY%20updated%20ASC";
    let (_, v) = search(&s.base_url(), &format!("{q}&maxResults=100")).await;
    assert_eq!(keys(&v), vec!["PAY-240", "PAY-231"]);
    s.assert_no_violations();
}

#[tokio::test]
async fn the_slash_date_separator_and_the_date_only_literal_parse_too() {
    let s = spawn_mock_jira().await;
    let q = "jql=updated%20%3E%3D%20%222026%2F08%2F22%2013%3A48%22";
    let (st, v) = search(&s.base_url(), &format!("{q}&maxResults=100")).await;
    assert_eq!(st, 200);
    assert_eq!(keys(&v), vec!["PAY-231"]);

    // A date-only literal is midnight in the server's zone: 2026-08-21T22:00Z,
    // which takes PAY-228 (16:05 UTC on the 21st) out and leaves the rest.
    let q = "jql=updated%20%3E%3D%20%222026-08-22%22%20ORDER%20BY%20updated%20ASC";
    let (st, v) = search(&s.base_url(), &format!("{q}&maxResults=100")).await;
    assert_eq!(st, 200);
    assert_eq!(keys(&v), vec!["PAY-240", "PAY-231"]);
    s.assert_no_violations();
}

#[tokio::test]
async fn a_touched_issue_is_exactly_what_the_next_incremental_returns() {
    let s = spawn_mock_jira().await;
    // Watermark = the fixture's now (14:32 UTC = 16:32 server-zone); nothing is
    // newer, so an idle poll is empty.
    let q = "jql=updated%20%3E%3D%20%222026-08-22%2016%3A32%22%20ORDER%20BY%20updated%20ASC";
    let (_, v) = search(&s.base_url(), &format!("{q}&maxResults=100")).await;
    assert_eq!(v["total"], 0, "an idle incremental must return nothing");

    s.touch_issue("PAY-240");
    let (_, v) = search(&s.base_url(), &format!("{q}&maxResults=100")).await;
    assert_eq!(keys(&v), vec!["PAY-240"]);
    s.assert_no_violations();
}

#[tokio::test]
async fn max_results_is_capped_by_the_server_and_the_response_says_so() {
    let s = spawn_mock_jira().await;
    s.state().set_max_results_cap(2);
    let q = "jql=ORDER%20BY%20updated%20ASC";

    let (_, p1) = search(&s.base_url(), &format!("{q}&startAt=0&maxResults=100")).await;
    assert_eq!(p1["total"], 7);
    assert_eq!(
        p1["maxResults"], 2,
        "the server's cap, not what the client asked for"
    );
    assert_eq!(keys(&p1).len(), 2);

    let (_, p2) = search(&s.base_url(), &format!("{q}&startAt=2&maxResults=100")).await;
    assert_eq!(keys(&p2).len(), 2);
    assert_ne!(keys(&p1), keys(&p2));

    let (_, last) = search(&s.base_url(), &format!("{q}&startAt=6&maxResults=100")).await;
    assert_eq!(keys(&last).len(), 1, "the tail page");

    // Walking every page yields every issue exactly once, in order.
    let mut all = Vec::new();
    let mut start = 0;
    loop {
        let (_, page) = search(
            &s.base_url(),
            &format!("{q}&startAt={start}&maxResults=100"),
        )
        .await;
        let ks = keys(&page);
        start += ks.len();
        all.extend(ks);
        if start >= page["total"].as_u64().unwrap() as usize {
            break;
        }
    }
    assert_eq!(
        all,
        vec![
            "PAY-200", "PAY-219", "OPS-77", "PAY-236", "PAY-228", "PAY-240", "PAY-231"
        ]
    );
    s.assert_no_violations();
}

#[tokio::test]
async fn an_unsupported_jql_clause_is_a_400_and_a_violation() {
    let s = spawn_mock_jira().await;
    let (st, body) = search(&s.base_url(), "jql=assignee%20%3D%20currentUser()").await;
    assert_eq!(st, 400);
    assert!(
        body["errorMessages"][0]
            .as_str()
            .unwrap()
            .contains("assignee")
    );
    assert_eq!(s.violations()[0].kind, ViolationKind::UnsupportedQuery);
}

#[tokio::test]
async fn an_unparseable_date_literal_is_a_400() {
    let s = spawn_mock_jira().await;
    let (st, body) = search(&s.base_url(), "jql=updated%20%3E%3D%20%2222-08-2026%22").await;
    assert_eq!(st, 400);
    assert!(
        body["errorMessages"][0]
            .as_str()
            .unwrap()
            .contains("22-08-2026")
    );
    assert_eq!(s.violations()[0].kind, ViolationKind::UnsupportedQuery);
}

#[tokio::test]
async fn search_serves_comments_and_worklogs_when_fields_asks_for_them() {
    // Stream A completes comments and worklogs from the search response and
    // deliberately never calls /issue/{key}. If `fields=comment,worklog` did
    // not populate them here, that adapter would sync empty bodies.
    let s = spawn_mock_jira().await;
    let (st, v) = search(
        &s.base_url(),
        "jql=project%20in%20(PAY)&fields=summary,updated,comment,worklog&maxResults=100",
    )
    .await;
    assert_eq!(st, 200);
    let issue = v["issues"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["key"] == "PAY-231")
        .expect("PAY-231 in the page");
    let f = &issue["fields"];
    assert_eq!(f["comment"]["total"], 2);
    assert_eq!(f["comment"]["comments"].as_array().unwrap().len(), 2);
    assert_eq!(f["comment"]["comments"][0]["author"]["name"], "priya.nair");
    assert_eq!(f["worklog"]["total"], 1);
    assert_eq!(f["worklog"]["worklogs"][0]["timeSpentSeconds"], 270 * 60);

    // And they stay out when not asked for: the default is *navigable.
    let (_, v) = search(&s.base_url(), "jql=&maxResults=100").await;
    assert!(v["issues"][0]["fields"].get("comment").is_none());
    assert!(v["issues"][0]["fields"].get("worklog").is_none());
    s.assert_no_violations();
}

#[tokio::test]
async fn fields_and_expand_are_projected_and_typos_are_refused() {
    let s = spawn_mock_jira().await;
    let (_, v) = search(
        &s.base_url(),
        "jql=&fields=summary,status,updated&maxResults=100",
    )
    .await;
    let f = &v["issues"][0]["fields"];
    assert!(f.get("summary").is_some() && f.get("status").is_some());
    assert!(f.get("assignee").is_none(), "fields= must actually project");

    let (_, v) = search(&s.base_url(), "jql=&expand=renderedFields&maxResults=100").await;
    assert!(v["issues"][0]["renderedFields"].is_object());

    let (st, _) = search(&s.base_url(), "jql=&fields=sumary&maxResults=100").await;
    assert_eq!(st, 400, "a typo'd field name must not be silently ignored");
    assert_eq!(s.violations()[0].kind, ViolationKind::UnknownField);
}

#[tokio::test]
async fn the_issue_envelope_is_the_data_center_shape() {
    let s = spawn_mock_jira().await;
    let (_, v) = search(&s.base_url(), "jql=&fields=*all&maxResults=100").await;
    let i = &v["issues"][0];
    assert_eq!(i["key"], "PAY-231");
    assert_eq!(i["id"], "10001", "Jira serialises ids as strings");
    assert_eq!(
        i["self"],
        format!("{}/rest/api/2/issue/10001", s.base_url())
    );
    assert_eq!(i["fields"]["issuetype"]["name"], "Story");
    assert_eq!(i["fields"]["issuetype"]["subtask"], false);
    assert_eq!(i["fields"]["status"]["name"], "In Progress");
    assert_eq!(
        i["fields"]["status"]["statusCategory"]["key"],
        "indeterminate"
    );
    assert_eq!(i["fields"]["priority"]["name"], "High");
    assert_eq!(i["fields"]["assignee"]["name"], "mara.lindqvist");
    assert_eq!(i["fields"]["reporter"]["name"], "mara.lindqvist");
    assert_eq!(i["fields"]["updated"], "2026-08-22T13:48:00.000+0200");
    // PAY-200 has no priority and no assignee: both are null, not absent.
    let epic = v["issues"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["key"] == "PAY-200")
        .unwrap();
    assert!(epic["fields"]["priority"].is_null());
    assert!(epic["fields"]["assignee"].is_null());
    s.assert_no_violations();
}

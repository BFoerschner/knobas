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

// -- the widened `fields=` set (issue #32) ----------------------------------

/// Epic membership, as `fields.parent`.
///
/// The fixture's `epic` is the transcription source, so both branches are real:
/// five tickets name an epic and two (PAY-200, which *is* the epic, and OPS-77)
/// name none. A `parent` mockd invented for the second group would reach the
/// mirror and be read as membership the dataset never claimed (the #28 ruling),
/// so the key is **absent** there, which is what a real Jira serves for an
/// issue with no parent.
#[tokio::test]
async fn parent_carries_the_fixtures_epic_and_is_absent_where_the_fixture_names_none() {
    let s = spawn_mock_jira().await;
    let (st, child) = get(&s.base_url(), "/rest/api/2/issue/PAY-231?fields=parent").await;
    assert_eq!(st, 200);
    let parent = &child["fields"]["parent"];
    assert_eq!(parent["key"], "PAY-200");
    assert_eq!(parent["id"], "10000", "Jira serialises ids as strings");
    assert_eq!(parent["fields"]["summary"], "Payout reliability");
    assert_eq!(
        parent["fields"]["issuetype"]["name"], "Epic",
        "the parent's own type is what tells a reader this is epic membership"
    );

    for orphan in ["PAY-200", "OPS-77"] {
        let (_, v) = get(
            &s.base_url(),
            &format!("/rest/api/2/issue/{orphan}?fields=parent"),
        )
        .await;
        assert!(
            v["fields"].get("parent").is_none(),
            "{orphan} has no epic in the fixture, so no parent may be invented: {v}"
        );
    }
    s.assert_no_violations();
}

/// `issuelinks`, from the fixture's `blocked_by` -- and from **both** ends of
/// it, because a real Jira serves the link on the blocker as well as on the
/// blocked issue. Serving only the recorded direction would teach an adapter
/// that a link is one-sided.
#[tokio::test]
async fn issuelinks_come_from_blocked_by_and_are_served_at_both_ends() {
    let s = spawn_mock_jira().await;
    let (st, blocked) = get(&s.base_url(), "/rest/api/2/issue/PAY-228?fields=issuelinks").await;
    assert_eq!(st, 200);
    let links = blocked["fields"]["issuelinks"].as_array().unwrap();
    assert_eq!(links.len(), 1, "PAY-228 is blocked by OPS-77: {blocked}");
    assert_eq!(links[0]["type"]["name"], "Blocks");
    assert_eq!(links[0]["type"]["inward"], "is blocked by");
    assert_eq!(links[0]["inwardIssue"]["key"], "OPS-77");
    assert!(
        links[0].get("outwardIssue").is_none(),
        "one link object carries one direction, never both: {}",
        links[0]
    );

    let (_, blocker) = get(&s.base_url(), "/rest/api/2/issue/OPS-77?fields=issuelinks").await;
    let back = blocker["fields"]["issuelinks"].as_array().unwrap();
    assert_eq!(back.len(), 1, "OPS-77 blocks PAY-228: {blocker}");
    assert_eq!(back[0]["type"]["outward"], "blocks");
    assert_eq!(back[0]["outwardIssue"]["key"], "PAY-228");

    let (_, unlinked) = get(&s.base_url(), "/rest/api/2/issue/PAY-240?fields=issuelinks").await;
    assert_eq!(
        unlinked["fields"]["issuelinks"].as_array().unwrap().len(),
        0,
        "an issue the fixture links to nothing gets an empty array, not a null"
    );
    s.assert_no_violations();
}

/// `resolution` follows the status: Jira sets one exactly when the issue
/// reaches a `done` status category, and PAY-219 is the fixture's only such
/// issue.
#[tokio::test]
async fn resolution_is_set_for_a_done_issue_and_null_for_every_other() {
    let s = spawn_mock_jira().await;
    let (st, done) = get(&s.base_url(), "/rest/api/2/issue/PAY-219?fields=resolution").await;
    assert_eq!(st, 200);
    assert_eq!(done["fields"]["resolution"]["name"], "Done");
    assert_eq!(done["fields"]["resolution"]["id"], "10000");

    for open in ["PAY-231", "PAY-228", "PAY-240"] {
        let (_, v) = get(
            &s.base_url(),
            &format!("/rest/api/2/issue/{open}?fields=resolution"),
        )
        .await;
        assert!(
            v["fields"]["resolution"].is_null(),
            "{open} is not done, so its resolution is null (present, not absent): {v}"
        );
    }
    s.assert_no_violations();
}

/// The two time fields the dataset can answer: the estimate the fixture
/// records, and the sum of the worklogs it records. Neither is invented --
/// `timeestimate` (remaining) has no source in the dataset and is therefore
/// not served at all.
#[tokio::test]
async fn the_time_fields_are_the_fixtures_estimate_and_the_sum_of_its_worklogs() {
    let s = spawn_mock_jira().await;
    let (st, v) = get(
        &s.base_url(),
        "/rest/api/2/issue/PAY-231?fields=timeoriginalestimate,timespent",
    )
    .await;
    assert_eq!(st, 200);
    assert_eq!(v["fields"]["timeoriginalestimate"], 16 * 3600);
    assert_eq!(v["fields"]["timespent"], 270 * 60);

    // PAY-228 logs time against no estimate; PAY-240 has neither.
    let (_, no_estimate) = get(
        &s.base_url(),
        "/rest/api/2/issue/PAY-228?fields=timeoriginalestimate,timespent",
    )
    .await;
    assert!(no_estimate["fields"]["timeoriginalestimate"].is_null());
    assert_eq!(no_estimate["fields"]["timespent"], 160 * 60);

    let (_, neither) = get(
        &s.base_url(),
        "/rest/api/2/issue/PAY-240?fields=timeoriginalestimate,timespent",
    )
    .await;
    assert!(neither["fields"]["timeoriginalestimate"].is_null());
    assert!(
        neither["fields"]["timespent"].is_null(),
        "no worklogs is null, not zero -- zero is what Jira reports for an issue \
         whose logged time was deleted"
    );
    s.assert_no_violations();
}

/// `labels` is in the closed set so production may ask for it, and it is empty
/// on every issue because the dataset names no labels. mockd inventing some
/// was refused in #28: they would reach `payload`, be indexed, and read as
/// something the fixture said.
#[tokio::test]
async fn labels_are_served_and_empty_because_the_dataset_names_none() {
    let s = spawn_mock_jira().await;
    let (st, v) = get(&s.base_url(), "/rest/api/2/issue/PAY-231?fields=labels").await;
    assert_eq!(st, 200);
    assert_eq!(
        v["fields"]["labels"],
        serde_json::json!([]),
        "present and empty, so a `fields=labels` request is a 200 rather than a 400"
    );
    s.assert_no_violations();
}

/// `*all` is the whole closed set, so the widened names have to be in it --
/// otherwise `fields=*all` and `fields=<every name>` answer differently and the
/// contract suite's golden covers less than it looks like it does.
#[tokio::test]
async fn the_widened_names_are_part_of_all() {
    let s = spawn_mock_jira().await;
    let (_, v) = get(&s.base_url(), "/rest/api/2/issue/PAY-231?fields=*all").await;
    for name in [
        "labels",
        "parent",
        "resolution",
        "issuelinks",
        "timeoriginalestimate",
        "timespent",
    ] {
        assert!(
            v["fields"].get(name).is_some(),
            "fields=*all did not serve {name}: {v}"
        );
    }
    s.assert_no_violations();
}

/// The set stays *closed* -- deviation 5 is widened, not retired. A name mockd
/// does not serve is still a 400 plus a recorded violation, which is the whole
/// reason the adapter's `BASE_FIELDS` can be trusted to be a query real Jira
/// would accept.
#[tokio::test]
async fn widening_the_set_did_not_open_it() {
    let s = spawn_mock_jira().await;
    for unknown in ["parrent", "components", "fixVersions", "timeestimate"] {
        let (st, _) = get(
            &s.base_url(),
            &format!("/rest/api/2/issue/PAY-231?fields={unknown}"),
        )
        .await;
        assert_eq!(st, 400, "{unknown} must not be silently accepted");
    }
    assert_eq!(s.violations().len(), 4);
    assert!(
        s.violations()
            .iter()
            .all(|v| v.kind == knobas_mockd::ViolationKind::UnknownField)
    );
}

// -- the classic Data Center epic link (issue #125) ---------------------------

/// The *other* spelling of the same fixture relationship: a classic Data Center
/// project keeps epic membership in a custom field, and
/// `JiraConfig::epic_link_field` is the only way knobas can reach it. mockd
/// served no `customfield_*` at all, so that whole configuration path answered
/// 400 plus an `UnknownField` violation and could not be run against the mock
/// -- the request was asserted in a unit test and the round trip never was.
///
/// The value is the epic's **key as a bare string**, which is what Greenhopper's
/// Epic Link field carries; `parent` nests a whole abbreviated issue. Two
/// spellings, one fixture `epic`, and no invention on either side.
#[tokio::test]
async fn the_epic_link_custom_field_carries_the_fixtures_epic_as_a_key() {
    let s = spawn_mock_jira().await;
    let field = knobas_mockd::jira::EPIC_LINK_FIELD;
    let (st, child) = get(
        &s.base_url(),
        &format!("/rest/api/2/issue/PAY-231?fields={field}"),
    )
    .await;
    assert_eq!(st, 200);
    assert_eq!(
        child["fields"][field], "PAY-200",
        "the Epic Link custom field is the epic's key, not a nested issue: {child}"
    );

    // Where the fixture names no epic the key is present and `null`, which is
    // how Jira serves a custom field with no value -- and the deliberate
    // contrast with `parent`, which is omitted entirely.
    for orphan in ["PAY-200", "OPS-77"] {
        let (_, v) = get(
            &s.base_url(),
            &format!("/rest/api/2/issue/{orphan}?fields={field},parent"),
        )
        .await;
        assert_eq!(
            v["fields"].get(field),
            Some(&serde_json::Value::Null),
            "{orphan} has no epic, so the custom field is null rather than absent: {v}"
        );
        assert!(
            v["fields"].get("parent").is_none(),
            "{orphan}: `parent` is still omitted, which is the shape Jira serves: {v}"
        );
    }
    s.assert_no_violations();
}

/// Opening the set by exactly one id is not opening it to a pattern. A
/// `customfield_*` this instance does not have is still a 400 plus a recorded
/// violation -- which is the whole point of deviation 5, and the reason a
/// mistyped `epic_link_field` fails a test instead of silently syncing nothing.
#[tokio::test]
async fn another_instances_custom_field_is_still_refused() {
    let s = spawn_mock_jira().await;
    for unknown in ["customfield_10009", "customfield_99999", "customfield_"] {
        let (st, _) = get(
            &s.base_url(),
            &format!("/rest/api/2/issue/PAY-231?fields={unknown}"),
        )
        .await;
        assert_eq!(
            st, 400,
            "{unknown} is not this instance's Epic Link field and must not be served"
        );
    }
    assert_eq!(s.violations().len(), 3);
    assert!(
        s.violations()
            .iter()
            .all(|v| v.kind == knobas_mockd::ViolationKind::UnknownField)
    );
}

// -- ADR-0054: editing a ticket's fields, its assignee, and who it can go to --

async fn put(
    base: &str,
    path: &str,
    body: serde_json::Value,
) -> (reqwest::StatusCode, serde_json::Value) {
    let r = reqwest::Client::new()
        .put(format!("{base}{path}"))
        .header("Authorization", "Bearer t")
        .json(&body)
        .send()
        .await
        .unwrap();
    let st = r.status();
    (st, r.json().await.unwrap_or(serde_json::Value::Null))
}

#[tokio::test]
async fn an_edit_sets_the_summary_and_priority_and_moves_labels_in_later_gets() {
    let s = spawn_mock_jira().await;
    let base = s.base_url();
    let (st, _) = put(
        &base,
        "/rest/api/2/issue/PAY-231",
        serde_json::json!({
            "fields": { "summary": "Retry failed SEPA payouts twice", "priority": { "name": "Low" } },
            "update": { "labels": [ { "add": "sepa" }, { "add": "payouts" } ] },
        }),
    )
    .await;
    assert_eq!(st, 204, "Jira answers an edit with no content");
    let (st, _) = put(
        &base,
        "/rest/api/2/issue/PAY-231",
        serde_json::json!({ "update": { "labels": [ { "remove": "sepa" } ] } }),
    )
    .await;
    assert_eq!(st, 204);

    let (_, v) = get(
        &base,
        "/rest/api/2/issue/PAY-231?fields=summary,priority,labels",
    )
    .await;
    assert_eq!(v["fields"]["summary"], "Retry failed SEPA payouts twice");
    assert_eq!(v["fields"]["priority"]["name"], "Low");
    assert_eq!(v["fields"]["labels"], serde_json::json!(["payouts"]));
    s.assert_no_violations();
}

#[tokio::test]
async fn an_edit_of_the_summary_alone_leaves_the_priority() {
    let s = spawn_mock_jira().await;
    let base = s.base_url();
    let (st, v) = put(
        &base,
        "/rest/api/2/issue/PAY-231",
        serde_json::json!({ "fields": { "summary": "Only the words" } }),
    )
    .await;
    assert_eq!(st, 204, "{v}");
    let (_, v) = get(&base, "/rest/api/2/issue/PAY-231?fields=summary,priority").await;
    assert_eq!(v["fields"]["summary"], "Only the words");
    assert_eq!(v["fields"]["priority"]["name"], "High");
}

/// What Jira refuses in an edit, mockd refuses too: a field off the edit
/// screen (under `fields` or `update`), a label operation it does not take,
/// an empty summary, a priority without a name.
#[tokio::test]
async fn an_edit_mockd_does_not_take_is_a_400_that_changes_nothing() {
    let s = spawn_mock_jira().await;
    let base = s.base_url();
    for body in [
        serde_json::json!({ "fields": { "duedate": "2026-12-01" } }),
        serde_json::json!({ "update": { "components": [ { "add": { "name": "api" } } ] } }),
        serde_json::json!({ "update": { "labels": [ { "set": ["sepa"] } ] } }),
        serde_json::json!({ "fields": { "summary": "  " } }),
        serde_json::json!({ "fields": { "priority": { "id": "2" } } }),
        serde_json::json!({ "fields": { "priority": { "name": "Urgent" } } }),
    ] {
        let (st, v) = put(&base, "/rest/api/2/issue/PAY-231", body.clone()).await;
        assert_eq!(st, 400, "{body}: {v}");
        assert!(!v["errorMessages"].as_array().unwrap().is_empty(), "{v}");
    }
    let (_, v) = get(
        &base,
        "/rest/api/2/issue/PAY-231?fields=summary,priority,labels",
    )
    .await;
    assert_eq!(v["fields"]["summary"], "Retry failed SEPA payouts");
    assert_eq!(v["fields"]["priority"]["name"], "High");
    assert_eq!(v["fields"]["labels"], serde_json::json!([]));

    let (st, _) = put(
        &base,
        "/rest/api/2/issue/PAY-999",
        serde_json::json!({ "fields": { "summary": "x" } }),
    )
    .await;
    assert_eq!(st, 404);
}

#[tokio::test]
async fn an_assignee_is_set_and_cleared_on_its_own_resource() {
    let s = spawn_mock_jira().await;
    let base = s.base_url();
    let (st, _) = put(
        &base,
        "/rest/api/2/issue/PAY-231/assignee",
        serde_json::json!({ "name": "priya.nair" }),
    )
    .await;
    assert_eq!(st, 204);
    let (_, v) = get(&base, "/rest/api/2/issue/PAY-231?fields=assignee").await;
    assert_eq!(v["fields"]["assignee"]["name"], "priya.nair");

    let (st, _) = put(
        &base,
        "/rest/api/2/issue/PAY-231/assignee",
        serde_json::json!({ "name": null }),
    )
    .await;
    assert_eq!(st, 204);
    let (_, v) = get(&base, "/rest/api/2/issue/PAY-231?fields=assignee").await;
    assert!(v["fields"]["assignee"].is_null(), "{v}");

    let (st, _) = put(
        &base,
        "/rest/api/2/issue/PAY-231/assignee",
        serde_json::json!({ "name": "nobody.here" }),
    )
    .await;
    assert_eq!(st, 400);
    s.assert_no_violations();
}

/// Who an issue can go to: everyone without `username`; with it, those whose
/// username or whose name holds it, either one.
#[tokio::test]
async fn the_assignable_search_matches_the_username_or_the_name() {
    let s = spawn_mock_jira().await;
    let base = s.base_url();
    let names = |v: &serde_json::Value| -> Vec<String> {
        v.as_array()
            .unwrap()
            .iter()
            .map(|u| u["name"].as_str().unwrap().to_owned())
            .collect()
    };
    let path = "/rest/api/2/user/assignable/search?issueKey=PAY-231";
    let (st, all) = get(&base, path).await;
    assert_eq!(st, 200);
    assert_eq!(
        names(&all).len(),
        knobas_source_mock::fixture().people.len()
    );

    // `.` is in every username and in no name.
    let (_, v) = get(&base, &format!("{path}&username=.")).await;
    assert_eq!(names(&v), names(&all));
    // `a l` is in Mara Lindqvist's name and in no username.
    let (_, v) = get(&base, &format!("{path}&username=a%20l")).await;
    assert_eq!(names(&v), ["mara.lindqvist"]);

    let (st, _) = get(&base, "/rest/api/2/user/assignable/search").await;
    assert_eq!(st, 400, "one of issueKey and project is required");
    s.assert_no_violations();
}

/// `expand=editmeta`: the four fields mockd's one user may change.
#[tokio::test]
async fn editmeta_names_the_four_fields_an_edit_takes() {
    let s = spawn_mock_jira().await;
    let (_, v) = get(&s.base_url(), "/rest/api/2/issue/PAY-231?expand=editmeta").await;
    let fields = v["editmeta"]["fields"]
        .as_object()
        .expect("editmeta fields");
    let mut ids: Vec<&str> = fields.keys().map(String::as_str).collect();
    ids.sort_unstable();
    assert_eq!(ids, ["assignee", "labels", "priority", "summary"]);
    assert_eq!(fields["summary"]["required"], true);
    assert_eq!(
        fields["priority"]["allowedValues"][0],
        serde_json::json!({ "id": "1", "name": "Highest" })
    );
    assert_eq!(
        fields["labels"]["operations"],
        serde_json::json!(["add", "set", "remove"])
    );
    s.assert_no_violations();
}

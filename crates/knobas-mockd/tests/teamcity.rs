//! The TeamCity REST subset, from stream C's point of view.
//!
//! Every test here is one of stream C's exit criteria written down as a wire
//! request, so a mockd change that would break the TeamCity adapter breaks
//! here first.

use knobas_mockd::{ViolationKind, spawn_mock_teamcity};

async fn tc(base: &str, path: &str) -> (reqwest::StatusCode, serde_json::Value) {
    let r = reqwest::Client::new()
        .get(format!("{base}{path}"))
        .header("Accept", "application/json")
        .header(
            "Authorization",
            format!("Bearer {}", knobas_mockd::TEAMCITY_TOKEN),
        )
        .send()
        .await
        .unwrap();
    let st = r.status();
    (st, r.json().await.unwrap_or(serde_json::Value::Null))
}

fn ids(v: &serde_json::Value) -> Vec<u64> {
    v["build"]
        .as_array()
        .unwrap_or(&Vec::new())
        .iter()
        .map(|b| b["id"].as_u64().unwrap())
        .collect()
}

#[tokio::test]
async fn a_request_without_a_json_accept_header_is_406_with_a_hint() {
    let s = spawn_mock_teamcity().await;
    for accept in ["*/*", "application/xml"] {
        let r = reqwest::Client::new()
            .get(format!("{}/app/rest/server", s.base_url()))
            .header("Accept", accept)
            .header("Authorization", "Bearer t")
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 406, "Accept: {accept}");
        assert!(r.headers().contains_key("X-Mockd-Hint"));
    }
    assert_eq!(s.violations().len(), 2);
    assert!(
        s.violations()
            .iter()
            .all(|v| v.kind == ViolationKind::MissingHeader)
    );
}

#[tokio::test]
async fn a_request_without_a_credential_is_401() {
    let s = spawn_mock_teamcity().await;
    let r = reqwest::Client::new()
        .get(format!("{}/app/rest/server", s.base_url()))
        .header("Accept", "application/json")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 401);
    assert_eq!(s.violations()[0].kind, ViolationKind::MissingHeader);
}

#[tokio::test]
async fn the_current_user_is_readable_for_connection_info() {
    // P4's ConnectionInfo.account comes from here. Without this endpoint the
    // adapter's test_connection would be a recorded violation, not a report.
    let s = spawn_mock_teamcity().await;
    let (st, me) = tc(
        &s.base_url(),
        "/app/rest/users/current?fields=id,username,name,email",
    )
    .await;
    assert_eq!(st, 200);
    assert_eq!(me["username"], "mara.lindqvist");
    assert_eq!(me["name"], "Mara Lindqvist");
    s.assert_no_violations();
}

#[tokio::test]
async fn server_and_build_types_read() {
    let s = spawn_mock_teamcity().await;
    let (st, srv) = tc(&s.base_url(), "/app/rest/server").await;
    assert_eq!(st, 200);
    assert!(srv["version"].is_string());
    assert_eq!(srv["webUrl"], s.base_url());

    // Cross-stream contract: buildType.id is the fixture's `cfg`, verbatim.
    let (_, bt) = tc(
        &s.base_url(),
        "/app/rest/buildTypes?fields=count,buildType(id,name,projectId,projectName,webUrl)",
    )
    .await;
    assert_eq!(bt["count"], 3);
    let first = &bt["buildType"][0];
    assert_eq!(first["id"], "Ledger_Deploy_Staging");
    assert_eq!(first["projectId"], "Ledger");
    assert_eq!(first["name"], "Deploy Staging");
    assert!(first.get("href").is_none(), "fields= must actually project");
    s.assert_no_violations();
}

#[tokio::test]
async fn the_build_type_ids_are_the_fixture_cfg_values_verbatim() {
    // Stream C derives its expectations from fixtures/tidewater/work.json and
    // fails loudly on a mismatch, so this mapping is a cross-stream contract:
    // no case folding, no separator normalisation, no prettification.
    let s = spawn_mock_teamcity().await;
    let (_, bt) = tc(
        &s.base_url(),
        "/app/rest/buildTypes?fields=count,buildType(id)",
    )
    .await;
    let got: Vec<&str> = bt["buildType"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        got,
        [
            "Ledger_Deploy_Staging",
            "Payout_Build",
            "Payout_IntegrationTests"
        ]
    );
    s.assert_no_violations();
}

#[tokio::test]
async fn the_build_ids_and_numbers_are_the_fixture_nums() {
    // The other half of the cross-stream identity contract: `build.id` is an
    // integer and `build.number` is the same value as a string.
    let s = spawn_mock_teamcity().await;
    let (_, v) = tc(
        &s.base_url(),
        "/app/rest/builds?locator=state:any,count:100&fields=count,build(id,number,buildTypeId)",
    )
    .await;
    let got: Vec<(u64, &str, &str)> = v["build"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| {
            (
                b["id"].as_u64().unwrap(),
                b["number"].as_str().unwrap(),
                b["buildTypeId"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        got,
        [
            (412, "412", "Ledger_Deploy_Staging"),
            (1187, "1187", "Payout_IntegrationTests"),
            (1188, "1188", "Payout_Build"),
        ]
    );
    s.assert_no_violations();
}

#[tokio::test]
async fn the_default_locator_hides_the_running_build() {
    let s = spawn_mock_teamcity().await;
    let (_, v) = tc(
        &s.base_url(),
        "/app/rest/builds?locator=count:100&fields=count,build(id,state)",
    )
    .await;
    assert_eq!(
        ids(&v),
        vec![412, 1187],
        "1188 is running and must be filtered by default"
    );
    s.assert_no_violations();
}

#[tokio::test]
async fn default_filter_false_shows_everything() {
    let s = spawn_mock_teamcity().await;
    let (_, v) = tc(
        &s.base_url(),
        "/app/rest/builds?locator=defaultFilter:false,count:100&fields=count,build(id)",
    )
    .await;
    assert_eq!(ids(&v), vec![412, 1187, 1188]);
    s.assert_no_violations();
}

#[tokio::test]
async fn the_nested_state_locator_returns_running_and_queued_builds() {
    let s = spawn_mock_teamcity().await;
    let q = "/app/rest/builds?locator=state:(queued:true,running:true)&fields=count,build(id,state,running-info(percentageComplete,currentStageText))";
    let (_, v) = tc(&s.base_url(), q).await;
    assert_eq!(ids(&v), vec![1188]);
    assert_eq!(v["build"][0]["state"], "running");
    assert!(
        v["build"][0]["running-info"]["currentStageText"]
            .as_str()
            .unwrap()
            .contains("step 3/5")
    );
    assert_eq!(v["build"][0]["running-info"]["percentageComplete"], 60);
    s.assert_no_violations();
}

#[tokio::test]
async fn the_repeated_state_spelling_is_refused() {
    let s = spawn_mock_teamcity().await;
    let (st, _) = tc(
        &s.base_url(),
        "/app/rest/builds?locator=state:running,state:queued&fields=count",
    )
    .await;
    assert_eq!(
        st, 400,
        "real TeamCity wants state:(queued:true,running:true)"
    );
    assert_eq!(s.violations()[0].kind, ViolationKind::UnsupportedQuery);
    assert!(
        s.violations()[0].detail.contains("state:(queued:true"),
        "the 400 must name the spelling that works, got: {}",
        s.violations()[0].detail
    );
}

#[tokio::test]
async fn an_unsupported_locator_dimension_is_refused() {
    let s = spawn_mock_teamcity().await;
    for locator in ["personal:true", "branch:main", "sinceBuild:1187"] {
        let (st, _) = tc(
            &s.base_url(),
            &format!("/app/rest/builds?locator={locator}&fields=count"),
        )
        .await;
        assert_eq!(st, 400, "locator={locator}");
    }
    assert_eq!(s.violations().len(), 3);
    assert!(
        s.violations()
            .iter()
            .all(|v| v.kind == ViolationKind::UnsupportedQuery)
    );
}

#[tokio::test]
async fn the_build_type_locator_filters() {
    let s = spawn_mock_teamcity().await;
    for spelling in [
        "buildType:Payout_IntegrationTests",
        "buildType:(id:Payout_IntegrationTests)",
    ] {
        let (_, v) = tc(
            &s.base_url(),
            &format!(
                "/app/rest/builds?locator={spelling},state:any,count:100&fields=count,build(id)"
            ),
        )
        .await;
        assert_eq!(ids(&v), vec![1187], "{spelling}");
    }
    s.assert_no_violations();
}

#[tokio::test]
async fn count_and_start_page_the_result() {
    let s = spawn_mock_teamcity().await;
    let (_, v) = tc(
        &s.base_url(),
        "/app/rest/builds?locator=state:any,count:2&fields=count,build(id)",
    )
    .await;
    assert_eq!(ids(&v), vec![412, 1187]);
    assert_eq!(v["count"], 2, "count is the size of this page");
    let (_, v) = tc(
        &s.base_url(),
        "/app/rest/builds?locator=state:any,start:2,count:100&fields=count,build(id)",
    )
    .await;
    assert_eq!(ids(&v), vec![1188]);
    s.assert_no_violations();
}

#[tokio::test]
async fn since_build_advances_only_past_finished_builds() {
    let s = spawn_mock_teamcity().await;
    let q = |n: u64| {
        format!("/app/rest/builds?locator=sinceBuild:(id:{n}),count:100&fields=count,build(id)")
    };
    let (_, v) = tc(&s.base_url(), &q(0)).await;
    assert_eq!(ids(&v), vec![412, 1187]);
    let (_, v) = tc(&s.base_url(), &q(412)).await;
    assert_eq!(ids(&v), vec![1187]);
    let (_, v) = tc(&s.base_url(), &q(1187)).await;
    assert!(
        ids(&v).is_empty(),
        "1188 is still running -- the cursor must not move past it"
    );

    s.state()
        .finish_build(1188, knobas_mockd::TcStatus::Failure);
    let (_, v) = tc(&s.base_url(), &q(1187)).await;
    assert_eq!(ids(&v), vec![1188]);
    s.assert_no_violations();
}

#[tokio::test]
async fn a_build_queued_after_the_cursor_arrives_once_it_finishes() {
    // The TeamCity counterpart of Jira's touch_issue: the one thing a frozen
    // fixture cannot express is a build that appears *after* the cursor was
    // taken. Stream C's incremental test is exactly this shape.
    let s = spawn_mock_teamcity().await;
    let q = |n: u64| {
        format!(
            "/app/rest/builds?locator=sinceBuild:(id:{n}),count:100&fields=count,build(id,state)"
        )
    };

    s.state()
        .finish_build(1188, knobas_mockd::TcStatus::Success);
    let (_, v) = tc(&s.base_url(), &q(1187)).await;
    assert_eq!(ids(&v), vec![1188], "the cursor is now at 1188");

    let new_id = s
        .state()
        .queue_build("Payout_Build", "feature/PAY-231-sepa-retry");
    assert!(
        new_id > 1188,
        "ids must stay monotonic or sinceBuild is meaningless"
    );

    // Queued builds are hidden by the default filter, so the finished-build
    // poll must not see it yet -- which is why §4.2 mandates the second poll.
    let (_, v) = tc(&s.base_url(), &q(1188)).await;
    assert!(ids(&v).is_empty());
    let (_, v) = tc(
        &s.base_url(),
        "/app/rest/builds?locator=state:(queued:true,running:true)&fields=count,build(id,state)",
    )
    .await;
    assert_eq!(ids(&v), vec![new_id]);
    assert_eq!(v["build"][0]["state"], "queued");

    s.state()
        .finish_build(new_id, knobas_mockd::TcStatus::Success);
    let (_, v) = tc(&s.base_url(), &q(1188)).await;
    assert_eq!(
        ids(&v),
        vec![new_id],
        "and now the incremental returns exactly it"
    );
    s.assert_no_violations();
}

#[tokio::test]
async fn a_build_reads_by_its_path_locator() {
    let s = spawn_mock_teamcity().await;
    let (st, b) = tc(
        &s.base_url(),
        "/app/rest/builds/id:1187?fields=id,number,status,state,branchName,statusText,webUrl,startDate,finishDate",
    )
    .await;
    assert_eq!(st, 200);
    assert_eq!(b["status"], "FAILURE");
    assert_eq!(b["state"], "finished");
    assert_eq!(b["branchName"], "feature/PAY-231-sepa-retry");
    assert!(b["webUrl"].as_str().unwrap().contains("buildId=1187"));
    // Compact TeamCity timestamps, and `4 m 12 s` after the start.
    assert_eq!(b["startDate"], "20260822T101000+0000");
    assert_eq!(b["finishDate"], "20260822T101412+0000");
    assert!(
        b["statusText"]
            .as_str()
            .unwrap()
            .starts_with("test sepa::retry"),
        "statusText is the first line of the fixture log"
    );
    s.assert_no_violations();
}

#[tokio::test]
async fn an_unknown_build_id_is_404() {
    let s = spawn_mock_teamcity().await;
    let (st, _) = tc(&s.base_url(), "/app/rest/builds/id:999999?fields=id").await;
    assert_eq!(st, 404);
    // A well-formed request for something that does not exist is not a
    // contract violation -- the adapter did nothing wrong.
    s.assert_no_violations();
}

#[tokio::test]
async fn missing_or_typoed_fields_are_refused() {
    let s = spawn_mock_teamcity().await;
    let (st, _) = tc(&s.base_url(), "/app/rest/builds?locator=count:1").await;
    assert_eq!(st, 400, "fields= is mandatory");

    let (st, _) = tc(
        &s.base_url(),
        "/app/rest/builds?locator=count:1&fields=count,build(idd)",
    )
    .await;
    assert_eq!(st, 400);
    assert!(
        s.violations()
            .iter()
            .any(|v| v.kind == ViolationKind::UnknownField)
    );
}

#[tokio::test]
async fn the_long_preset_is_everything_and_the_others_are_refused() {
    let s = spawn_mock_teamcity().await;
    let (st, v) = tc(&s.base_url(), "/app/rest/builds/id:412?fields=$long").await;
    assert_eq!(st, 200);
    assert_eq!(v["id"], 412);
    assert!(v.get("webUrl").is_some() && v.get("statusText").is_some());

    for preset in ["$short", "$locator"] {
        let (st, _) = tc(
            &s.base_url(),
            &format!("/app/rest/builds/id:412?fields={preset}"),
        )
        .await;
        assert_eq!(st, 400, "{preset} is deviation 6, not a silent drop");
    }
    assert_eq!(
        s.violations()
            .iter()
            .filter(|v| v.kind == ViolationKind::UnknownField)
            .count(),
        2
    );
}

#[tokio::test]
async fn a_path_the_route_table_does_not_have_is_501_and_a_violation() {
    let s = spawn_mock_teamcity().await;
    let (st, _) = tc(&s.base_url(), "/app/rest/agents?fields=count").await;
    assert_eq!(st, 501);
    assert_eq!(s.violations()[0].kind, ViolationKind::Unimplemented);
}

#[tokio::test]
#[should_panic(expected = "no build 4242")]
async fn finishing_a_build_that_does_not_exist_panics() {
    // A silent no-op here would make a stream C test pass for the wrong reason.
    let s = spawn_mock_teamcity().await;
    s.state()
        .finish_build(4242, knobas_mockd::TcStatus::Success);
}

#[tokio::test]
#[should_panic(expected = "no build type \"Nope\"")]
async fn queueing_against_an_unknown_build_type_panics() {
    let s = spawn_mock_teamcity().await;
    s.state().queue_build("Nope", "main");
}

#[tokio::test]
async fn an_injected_fault_reaches_teamcity_too() {
    let s = spawn_mock_teamcity().await;
    s.set_fault(knobas_mockd::MockFault::RateLimited {
        retry_after_secs: 7,
    });
    let r = reqwest::Client::new()
        .get(format!("{}/app/rest/server", s.base_url()))
        .header("Accept", "application/json")
        .header("Authorization", "Bearer t")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 429);
    assert_eq!(r.headers()["retry-after"], "7");
    s.assert_no_violations();
}

#[tokio::test]
async fn only_a_running_build_carries_running_information() {
    // The negative half matters as much as the positive one: an adapter that
    // reads `running-info` to drive a progress bar would show a finished build
    // as perpetually in flight if mockd emitted the object unconditionally.
    let s = spawn_mock_teamcity().await;
    let (_, done) = tc(&s.base_url(), "/app/rest/builds/id:412?fields=$long").await;
    assert!(done.get("running-info").is_none(), "412 is finished");
    assert!(done.get("running").is_none());
    assert!(done.get("percentageComplete").is_none());

    let (_, live) = tc(&s.base_url(), "/app/rest/builds/id:1188?fields=$long").await;
    assert_eq!(live["running"], true);
    assert_eq!(live["percentageComplete"], 60);
    assert!(live["running-info"]["elapsedSeconds"].as_i64().unwrap() > 0);

    // ... and they go away again when it finishes.
    s.state()
        .finish_build(1188, knobas_mockd::TcStatus::Success);
    let (_, after) = tc(&s.base_url(), "/app/rest/builds/id:1188?fields=$long").await;
    assert!(after.get("running-info").is_none());
    assert!(after.get("running").is_none());
    assert!(after.get("percentageComplete").is_none());
    s.assert_no_violations();
}

#[tokio::test]
async fn a_queued_build_has_a_queued_date_and_no_start_date() {
    let s = spawn_mock_teamcity().await;
    let id = s.state().queue_build("Payout_Build", "main");
    let (_, b) = tc(
        &s.base_url(),
        &format!("/app/rest/builds/id:{id}?fields=$long"),
    )
    .await;
    assert_eq!(b["state"], "queued");
    assert!(b["queuedDate"].is_string());
    assert!(
        b.get("startDate").is_none(),
        "a build that has not started has no startDate"
    );
    assert!(b.get("finishDate").is_none());
    s.assert_no_violations();
}

#[tokio::test]
async fn the_mutators_advance_the_clock_and_stamp_the_build() {
    // One minute per mutation, the same rule as Jira's touch_issue: the clock
    // is what `server.currentTime` reports, and a mutator that did not move it
    // would make two consecutive finishes indistinguishable.
    let s = spawn_mock_teamcity().await;
    let before = s.state().now();
    s.state()
        .finish_build(1188, knobas_mockd::TcStatus::Success);
    let after = s.state().now();
    assert_eq!((after - before).num_seconds(), 60);
    assert_eq!(
        s.state().build(1188).unwrap().finish_date,
        Some(after),
        "finish_date is the ticked clock"
    );

    let id = s.state().queue_build("Payout_Build", "main");
    let queued_at = s.state().now();
    assert_eq!((queued_at - after).num_seconds(), 60);
    assert_eq!(s.state().build(id).unwrap().start_date, queued_at);
}

// -- the authorship fields the M1 adapter cannot ask for yet -----------------
//
// `knobas-source-teamcity`'s `rest` module docs record the gap these two tests
// close: `triggered(user(username))` is the only place TeamCity names the
// person who started a build, and mockd's serialiser did not carry the name,
// so asking for it was a 400 + `UnknownField` violation rather than a field.
// The adapter therefore hard-codes `SyncItem::author = None` for every build.
//
// These tests assert the *names are servable*, which is the half stream T owns.
// They deliberately do **not** assert an author: `fixtures/tidewater/work.json`
// records no triggerer for any of its three builds, and inventing one here
// would put a fabricated person into `SyncItem::author`.

#[tokio::test]
async fn a_build_serves_the_triggered_subtree_the_adapter_will_widen_to() {
    let s = spawn_mock_teamcity().await;
    let (st, v) = tc(
        &s.base_url(),
        "/app/rest/builds?locator=state:any,count:100\
         &fields=count,build(id,queuedDate,triggered(type,date,user(username,name)))",
    )
    .await;
    assert_eq!(st, 200, "{v}");

    let builds = v["build"].as_array().expect("a build array");
    assert_eq!(builds.len(), 3, "the fixture's three builds: {v}");
    for b in builds {
        let t = &b["triggered"];
        assert_eq!(
            t["type"], "vcs",
            "the fixture records no person pressing Run, and a VCS trigger is \
             what a branch build is: {b}"
        );
        assert_eq!(
            t["date"], b["queuedDate"],
            "the trigger fires when the build is queued: {b}"
        );
        assert!(
            t.get("user").is_none(),
            "a VCS trigger has no user, and the fixture names none: {b}"
        );
    }
    // The whole point: asking for these names is no longer a violation.
    s.assert_no_violations();
}

#[tokio::test]
async fn a_build_type_serves_description_and_paused() {
    let s = spawn_mock_teamcity().await;
    let (st, v) = tc(
        &s.base_url(),
        "/app/rest/buildTypes?fields=count,buildType(id,description,paused)",
    )
    .await;
    assert_eq!(st, 200, "{v}");

    let types = v["buildType"].as_array().expect("a buildType array");
    assert!(!types.is_empty(), "{v}");
    for t in types {
        assert_eq!(
            t["paused"], false,
            "no fixture configuration is paused, and `false` is a fact rather \
             than an omission: {t}"
        );
        assert!(
            t.get("description").is_none(),
            "the fixture gives no configuration a description: {t}"
        );
    }
    s.assert_no_violations();
}

#[tokio::test]
async fn the_new_names_are_still_a_closed_set() {
    // The additions must widen the known set by exactly these names -- a
    // serialiser that started answering anything would make every test above
    // vacuous.
    let s = spawn_mock_teamcity().await;
    for bad in [
        "count,build(id,triggeredBy)",
        "count,build(id,triggered(who))",
    ] {
        let (st, v) = tc(
            &s.base_url(),
            &format!("/app/rest/builds?locator=state:any,count:100&fields={bad}"),
        )
        .await;
        assert_eq!(st, 400, "fields={bad} must be refused: {v}");
    }
    assert_eq!(
        s.violations()
            .iter()
            .filter(|v| v.kind == ViolationKind::UnknownField)
            .count(),
        2,
        "each refusal is also a recorded violation"
    );

    // The limit of that closed set, asserted rather than left as a surprise:
    // `user` is `null` on every fixture build, and a null carries no key set,
    // so a typo *inside* an absent object cannot be caught. Stream C widening
    // to `triggered(user(username))` is safe; a widening to a misspelled
    // sub-name would pass here and return nothing.
    let (st, _) = tc(
        &s.base_url(),
        "/app/rest/builds?locator=state:any,count:100\
         &fields=count,build(id,triggered(user(nosuchfield)))",
    )
    .await;
    assert_eq!(
        st, 200,
        "a sub-name of an absent object is accepted -- see tc_fields::check_names"
    );
}

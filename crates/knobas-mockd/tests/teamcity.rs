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
            (1188, "1188", "Payout_Build"),
            (1187, "1187", "Payout_IntegrationTests"),
            (412, "412", "Ledger_Deploy_Staging"),
        ],
        "newest first"
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
        vec![1187, 412],
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
    assert_eq!(ids(&v), vec![1188, 1187, 412]);
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
    assert_eq!(
        ids(&v),
        vec![1188, 1187],
        "a page is taken off the newest end"
    );
    assert_eq!(v["count"], 2, "count is the size of this page");
    let (_, v) = tc(
        &s.base_url(),
        "/app/rest/builds?locator=state:any,start:2,count:100&fields=count,build(id)",
    )
    .await;
    assert_eq!(ids(&v), vec![412], "...and `start:` skips from that end");
    s.assert_no_violations();
}

#[tokio::test]
async fn since_build_advances_only_past_finished_builds() {
    let s = spawn_mock_teamcity().await;
    let q = |n: u64| {
        format!("/app/rest/builds?locator=sinceBuild:(id:{n}),count:100&fields=count,build(id)")
    };
    let (_, v) = tc(&s.base_url(), &q(0)).await;
    assert_eq!(ids(&v), vec![1187, 412]);
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

// -- the authorship fields --------------------------------------------------
//
// `triggered(user(username))` is the only place TeamCity names the person who
// started a build, and `knobas-source-teamcity` asks for it. These tests are
// mockd's half of that: the names are servable, and what they carry is what
// the fixture says.
//
// `triggered` is served from the fixture and from nowhere else. A person
// invented here would flow through the adapter into `SyncItem::author` and be
// indexed and searched as if `fixtures/tidewater/work.json` had said it, which
// is why #28 refused to add one and the fixture gained `triggered_by` first.

/// The fixture's triggerer, as TeamCity spells it: the person's `username` and
/// `name` under `triggered.user`, and `type: "user"` rather than `"vcs"`.
///
/// The fixture names a person for one build and nobody for the other two, and
/// both halves are asserted -- a serialiser that hard-coded either answer
/// would fail one of them.
#[tokio::test]
async fn a_build_names_the_person_the_fixture_says_triggered_it() {
    let f = knobas_source_mock::fixture();
    let s = spawn_mock_teamcity().await;
    let (st, v) = tc(
        &s.base_url(),
        "/app/rest/builds?locator=state:any,count:100\
         &fields=count,build(id,queuedDate,triggered(type,date,user(username,name)))",
    )
    .await;
    assert_eq!(st, 200, "{v}");

    let builds = v["build"].as_array().expect("a build array");
    assert_eq!(builds.len(), f.builds.len(), "the fixture's builds: {v}");
    let mut named = 0;
    for b in builds {
        let num = b["id"].as_u64().expect("an id") as u32;
        let fixture_build = f
            .builds
            .iter()
            .find(|x| x.num == num)
            .unwrap_or_else(|| panic!("mockd served a build the fixture has not: {num}"));
        let t = &b["triggered"];
        assert_eq!(
            t["date"], b["queuedDate"],
            "the trigger fires when the build is queued: {b}"
        );
        match fixture_build.triggered_by.as_deref() {
            Some(id) => {
                named += 1;
                let p = f
                    .person(id)
                    .expect("the triggerer is a person in the fixture");
                assert_eq!(t["type"], "user", "a person pressed Run: {b}");
                assert_eq!(t["user"]["username"], p.username, "{b}");
                assert_eq!(t["user"]["name"], p.name, "{b}");
            }
            None => {
                assert_eq!(
                    t["type"], "vcs",
                    "the fixture records no person for this build, and a VCS trigger is what a \
                     branch build is: {b}"
                );
                assert!(
                    t.get("user").is_none(),
                    "a VCS trigger has no user, and inventing one would be indexed as \
                     authorship: {b}"
                );
            }
        }
    }
    assert!(
        named > 0 && named < builds.len(),
        "the fixture must name a triggerer for some builds and not others, or neither branch \
         above can fail"
    );
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

    // Sub-names of `triggered.user` are checked too, now that the fixture
    // names a person: a null carries no key set, so before `triggered_by` this
    // could only be accepted.
    let (st, v) = tc(
        &s.base_url(),
        "/app/rest/builds?locator=state:any,count:100\
         &fields=count,build(id,triggered(user(nosuchfield)))",
    )
    .await;
    assert_eq!(
        st, 400,
        "a build with a real triggerer has a key set to validate against: {v}"
    );

    // ...and the limit that remains, asserted rather than left as a surprise:
    // scope the answer to builds the fixture names nobody for and the check
    // has nothing to work from again (deviation 11, one level down).
    let quiet = knobas_source_mock::fixture()
        .builds
        .iter()
        .find(|b| b.triggered_by.is_none())
        .map(|b| b.cfg.clone())
        .expect("the fixture has a build with no triggerer");
    let (st, _) = tc(
        &s.base_url(),
        &format!(
            "/app/rest/builds?locator=buildType:(id:{quiet}),state:any,count:100\
             &fields=count,build(id,triggered(user(nosuchfield)))"
        ),
    )
    .await;
    assert_eq!(
        st, 200,
        "a sub-name of an absent object is still accepted -- see tc_fields::check_names"
    );
}

/// Deviation 12, closed: `/app/rest/builds` answers **newest first**, so
/// `count:1` is the newest build rather than the oldest.
///
/// This is the one thing the vendored swagger cannot catch — it validates the
/// shape of a response, never the order of a collection — so a mock that
/// answered ascending taught every adapter written against it that `count:1`
/// means "the oldest build", which is the opposite of what the same request
/// does in production.
///
/// The expectation is computed from the fixture rather than written out, and
/// the fixture's three ids are distinct, so an implementation that ignored the
/// order entirely could not satisfy both halves.
#[tokio::test]
async fn builds_come_back_newest_first_so_count_1_is_the_newest_build() {
    let mut newest_first: Vec<u64> = knobas_source_mock::fixture()
        .builds
        .iter()
        .map(|b| u64::from(b.num))
        .collect();
    newest_first.sort_unstable_by(|a, b| b.cmp(a));

    let s = spawn_mock_teamcity().await;
    let (_, all) = tc(
        &s.base_url(),
        "/app/rest/builds?locator=defaultFilter:false,count:100&fields=count,build(id)",
    )
    .await;
    assert_eq!(ids(&all), newest_first, "newest first, as real TeamCity");

    let (_, one) = tc(
        &s.base_url(),
        "/app/rest/builds?locator=defaultFilter:false,count:1&fields=count,build(id)",
    )
    .await;
    assert_eq!(
        ids(&one),
        newest_first[..1].to_vec(),
        "count:1 is the newest build, not the oldest"
    );
    s.assert_no_violations();
}

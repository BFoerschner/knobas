use knobas_mockd::state::{MockState, jira_date};

#[test]
fn issue_ids_and_projects_are_stable() {
    let s = MockState::from_fixture();
    let i = s.issue("PAY-231").expect("PAY-231 is in the fixture");
    assert_eq!(i.id, 10001);
    assert_eq!(i.project, "PAY");
    assert_eq!(i.summary, "Retry failed SEPA payouts");
    assert_eq!(i.issue_type, "Story");
    assert_eq!(i.status, "In Progress");
    assert_eq!(i.assignee.as_deref(), Some("mara.lindqvist"));
    assert_eq!(i.comments.len(), 2);
    assert_eq!(i.worklogs.len(), 1);
    assert_eq!(i.worklogs[0].time_spent_seconds, 270 * 60);
    assert_eq!(s.issue("OPS-77").unwrap().project, "OPS");
    assert_eq!(s.issues().len(), 7);
}

#[test]
fn ids_are_assigned_by_fixture_position() {
    let s = MockState::from_fixture();
    assert_eq!(s.issue("PAY-200").unwrap().id, 10000);
    assert_eq!(s.issue("OPS-77").unwrap().id, 10006);
    // Comments and worklogs number across the whole fixture, not per issue.
    assert_eq!(s.issue("PAY-231").unwrap().comments[0].id, 20000);
    assert_eq!(s.issue("PAY-231").unwrap().comments[1].id, 20001);
    assert_eq!(s.issue("PAY-231").unwrap().worklogs[0].id, 30000);
    assert_eq!(s.issue("PAY-228").unwrap().worklogs[0].id, 30001);
    // `reporter` falls back through assigned_by -> assignee -> priya.
    assert_eq!(s.issue("PAY-240").unwrap().reporter, "priya.nair");
    assert_eq!(s.issue("PAY-231").unwrap().reporter, "mara.lindqvist");
    assert_eq!(s.issue("PAY-200").unwrap().reporter, "priya.nair");
}

#[test]
fn the_epic_without_an_updated_timestamp_gets_the_documented_one() {
    // PAY-200 is the fixture's only null `updated`. Stream A asserts against
    // this same literal, so it is a contract, not an implementation detail.
    let s = MockState::from_fixture();
    let epic = s.issue("PAY-200").unwrap();
    assert_eq!(epic.updated.to_rfc3339(), "2026-07-23T14:32:00+00:00");
    assert_eq!(
        jira_date(epic.updated, s.server_offset()),
        "2026-07-23T16:32:00.000+0200"
    );
    assert!(epic.created < epic.updated);
}

#[test]
fn timestamps_use_the_data_center_format_in_the_server_zone_not_rfc3339() {
    let s = MockState::from_fixture();
    // PAY-231 is updated 11:48 UTC; the server is on +02:00, so it renders 13:48.
    let rendered = jira_date(s.issue("PAY-231").unwrap().updated, s.server_offset());
    assert_eq!(rendered, "2026-08-22T13:48:00.000+0200");
    assert!(
        chrono::DateTime::parse_from_rfc3339(&rendered).is_err(),
        "if this ever parses as RFC 3339 the mock stopped reproducing the DC format \
         adapters must handle (the offset carries no colon)"
    );
}

#[test]
fn the_server_zone_defaults_to_something_other_than_utc() {
    let s = MockState::from_fixture();
    assert_ne!(
        s.server_offset().local_minus_utc(),
        0,
        "a UTC mock would let a timezone-blind watermark pass; see the module docs"
    );
    assert_eq!(s.server_offset().local_minus_utc(), 2 * 3600);
}

#[test]
fn touch_advances_the_clock_by_one_minute_and_bumps_only_that_issue() {
    let s = MockState::from_fixture();
    let before_231 = s.issue("PAY-231").unwrap().updated;
    let before_228 = s.issue("PAY-228").unwrap().updated;

    s.touch_issue("PAY-231");
    let after_231 = s.issue("PAY-231").unwrap().updated;
    assert!(after_231 > before_231);
    assert_eq!(s.issue("PAY-228").unwrap().updated, before_228);

    s.touch_issue("PAY-228");
    let after_228 = s.issue("PAY-228").unwrap().updated;
    assert_eq!(after_228 - after_231, chrono::Duration::minutes(1));
}

#[test]
fn a_posted_comment_is_visible_afterwards_and_bumps_updated() {
    let s = MockState::from_fixture();
    let before = s.issue("PAY-231").unwrap().updated;
    let id = s
        .add_comment("PAY-231", "jonas.becker", "looks good")
        .unwrap();
    let after = s.issue("PAY-231").unwrap();
    assert_eq!(after.comments.len(), 3);
    assert_eq!(after.comments[2].id, id);
    assert_eq!(after.comments[2].body, "looks good");
    assert!(after.updated > before);
}

#[test]
fn reset_restores_the_fixture() {
    let s = MockState::from_fixture();
    s.touch_issue("PAY-231");
    s.add_comment("PAY-231", "jonas.becker", "x");
    s.reset();
    let i = s.issue("PAY-231").unwrap();
    assert_eq!(i.comments.len(), 2);
    assert_eq!(
        jira_date(i.updated, s.server_offset()),
        "2026-08-22T13:48:00.000+0200"
    );
}

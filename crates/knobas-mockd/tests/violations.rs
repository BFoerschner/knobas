use knobas_mockd::{Violation, ViolationKind, ViolationLog};

fn v(kind: ViolationKind, path: &str) -> Violation {
    Violation {
        kind,
        method: "GET".into(),
        path: path.into(),
        query: String::new(),
        detail: "test".into(),
        at: chrono::Utc::now(),
    }
}

#[test]
fn log_records_snapshots_and_clears() {
    let log = ViolationLog::default();
    assert!(log.snapshot().is_empty());
    log.assert_empty("fresh log");

    log.record(v(ViolationKind::UnknownPath, "/rest/api/3/search/jql"));
    log.record(v(ViolationKind::UnknownQueryParam, "/rest/api/2/search"));
    let snap = log.snapshot();
    assert_eq!(snap.len(), 2);
    assert_eq!(snap[0].kind, ViolationKind::UnknownPath);
    assert_eq!(snap[1].kind, ViolationKind::UnknownQueryParam);

    log.clear();
    assert!(log.snapshot().is_empty());
}

#[test]
#[should_panic(expected = "/rest/api/3/search/jql")]
fn assert_empty_names_every_violation() {
    let log = ViolationLog::default();
    log.record(v(ViolationKind::UnknownPath, "/rest/api/3/search/jql"));
    log.assert_empty("jira");
}

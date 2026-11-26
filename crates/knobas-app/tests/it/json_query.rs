//! **jq kept running for an open document** (the XML view's design,
//! 2026-11-23, section 26): a session reads the document once and answers a
//! filter a key; a filter that loops is stopped at its deadline and the next
//! one answers; a value too deep for the stack ends the helper, not the app;
//! closed, the helper is gone.

use knobas_app::file_pipeline::session::{JqSessions, JsonPathStep, JsonQueryAnswer};

const ORDERS: &str = r#"{"orders": [{"id": "a", "total": 2400}, {"id": "b", "total": 90}, {"id": "c", "total": 3100}]}"#;

fn sessions() -> JqSessions {
    knobas_app::file_pipeline::use_helper(env!("CARGO_BIN_EXE_knobas-app"));
    JqSessions::default()
}

#[tokio::test]
async fn a_picking_filter_answers_its_values_and_paths_and_a_making_one_no_paths() {
    let jq = sessions();
    let id = jq.open(ORDERS.as_bytes().to_vec());
    let picked = jq
        .query(id, ".orders[] | select(.total > 2000) | .id")
        .await
        .expect("answered");
    let JsonQueryAnswer::Answered {
        values,
        count,
        paths,
        ..
    } = picked
    else {
        panic!("not answered: {picked:?}");
    };
    assert_eq!(
        (values, count),
        (vec!["\"a\"".to_owned(), "\"c\"".to_owned()], 2)
    );
    let key = |k: &str| JsonPathStep::Key(k.to_owned());
    assert_eq!(
        paths,
        Some(vec![
            vec![key("orders"), JsonPathStep::Index(0), key("id")],
            vec![key("orders"), JsonPathStep::Index(2), key("id")],
        ])
    );
    // The same helper answers a second filter: it makes a value, so nothing is picked.
    let made = jq
        .query(id, ".orders | map(.total) | add")
        .await
        .expect("answered");
    assert!(
        matches!(&made, JsonQueryAnswer::Answered { values, paths: None, .. } if values == &vec!["5590".to_owned()]),
        "{made:?}"
    );
    // A filter that does not parse says so in jq's words.
    let bad = jq.query(id, ".orders[").await.expect("answered");
    assert!(
        matches!(&bad, JsonQueryAnswer::Failed { why, .. } if why.contains("does not parse")),
        "{bad:?}"
    );
    assert_eq!(jq.helper_running(id).await, Some(true));
}

#[tokio::test]
async fn a_filter_that_loops_is_stopped_at_its_deadline_and_the_next_one_answers() {
    let jq = sessions();
    let id = jq.open(ORDERS.as_bytes().to_vec());
    // The first filter starts the helper; the loop then has the deadline.
    jq.query(id, ".").await.expect("answered");
    let looped = jq.query(id, "def f: f; f").await.expect("answered");
    assert!(
        matches!(&looped, JsonQueryAnswer::Stopped { why } if why.contains("1.5 s")),
        "{looped:?}"
    );
    let next = jq.query(id, ".orders | length").await.expect("answered");
    assert!(
        matches!(&next, JsonQueryAnswer::Answered { values, .. } if values == &vec!["3".to_owned()]),
        "{next:?}"
    );
}

#[tokio::test]
async fn a_value_too_deep_ends_the_helper_not_the_app_and_the_next_filter_answers() {
    let jq = sessions();
    let id = jq.open(ORDERS.as_bytes().to_vec());
    let deep = jq
        .query(id, "reduce range(1000000) as $_ (0; [.])")
        .await
        .expect("answered");
    assert!(
        !matches!(deep, JsonQueryAnswer::Answered { .. }),
        "{deep:?}"
    );
    let next = jq.query(id, ".orders[0].id").await.expect("answered");
    assert!(
        matches!(&next, JsonQueryAnswer::Answered { values, .. } if values == &vec!["\"a\"".to_owned()]),
        "{next:?}"
    );
}

#[tokio::test]
async fn closed_the_session_is_gone_and_asks_nothing() {
    let jq = sessions();
    let id = jq.open(ORDERS.as_bytes().to_vec());
    jq.query(id, ".").await.expect("answered");
    jq.close(id);
    assert!(!jq.is_open(id));
    assert_eq!(jq.helper_running(id).await, None);
    assert!(jq.query(id, ".").await.is_err());
}

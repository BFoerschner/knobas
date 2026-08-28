//! The Jira adapter against the HTTP-level mock (roadmap §3: `knobas-mockd`
//! in-process, no Docker, deterministic).
//!
//! Every test ends with `assert_no_violations()`. That is the point of the mock
//! being validated against the vendored WADL: an adapter that invents an
//! endpoint, a verb or a query parameter fails *its own* suite instead of
//! passing against something lenient and then failing against a real Jira.
//!
//! Two things about mockd shape the tests below and are worth stating once:
//!
//! * **mockd accepts any non-empty `Bearer`/`Basic` credential.** A "wrong
//!   token" is therefore not a 401 -- rejection is what
//!   [`MockFault::Unauthorized`] is for. A blank one is worse than useless: it
//!   is recorded as a `MissingHeader` violation, so it certifies nothing. The
//!   unauthorized cases here run against their own server with that fault set.
//! * **The server is on `+02:00`, not UTC**, and reads JQL date literals in
//!   that zone. Every incremental test below is therefore also a test that the
//!   watermark is rendered in the server's zone.

use knobas_mockd::{MockFault, spawn_mock_jira};
use knobas_source::contract::{Fault, VecSink, battery};
use knobas_source::instance::SourceInstance;
use knobas_source::{AuthMethod, Source, SourceError, WriteOp};

fn instance(base_url: &str, secret: &str, config: serde_json::Value) -> SourceInstance {
    SourceInstance {
        id: "jira".to_owned(),
        kind: "jira".to_owned(),
        display_name: "Tidewater Jira".to_owned(),
        base_url: base_url.to_owned(),
        auth: Some(AuthMethod::Pat),
        secret: Some(secret.to_owned()),
        config,
    }
}

fn source(base_url: &str, config: serde_json::Value) -> Box<dyn Source> {
    source_named("jira", base_url, config)
}

fn source_named(id: &str, base_url: &str, config: serde_json::Value) -> Box<dyn Source> {
    let mut i = instance(base_url, knobas_mockd::JIRA_TOKEN, config);
    i.id = id.to_owned();
    match knobas_source_jira::build(i) {
        Ok(s) => s,
        // `Box<dyn Source>` is not `Debug`, so `expect` is unavailable.
        Err(e) => panic!("the adapter must build against mockd: {e:?}"),
    }
}

async fn sync_all(source: &dyn Source, cursor: Option<String>) -> (Vec<String>, String) {
    let mut sink = VecSink(Vec::new());
    let next = source.sync(cursor, &mut sink).await.expect("sync succeeds");
    (sink.0.iter().map(|i| i.entity.key.clone()).collect(), next)
}

/// The shared suite every adapter must pass.
///
/// The unauthorized case gets **its own server** with the fault set, rather
/// than a wrong token against the shared one: mockd accepts any non-empty
/// credential, and the battery keeps the healthy adapter alive and calls it
/// again after building the faulted ones (clauses 5 and 6), so a fault on the
/// shared server would make those later calls fail with the wrong error.
#[tokio::test]
async fn passes_the_contract_battery() {
    let jira = spawn_mock_jira().await;
    let denied = spawn_mock_jira().await;
    denied.set_fault(MockFault::Unauthorized);
    let base = jira.base_url();
    let denied_base = denied.base_url();
    let dead = knobas_mockd::refused_url();
    battery(move |fault| match fault {
        Fault::None => source(&base, serde_json::json!({})),
        Fault::Unauthorized => source(&denied_base, serde_json::json!({})),
        Fault::Unreachable => source(&dead, serde_json::json!({})),
    })
    .await;
    jira.assert_no_violations();
    denied.assert_no_violations();
}

#[tokio::test]
async fn a_full_sync_lands_the_fixture_issues() {
    let jira = spawn_mock_jira().await;
    let source = source(&jira.base_url(), serde_json::json!({}));
    let mut sink = VecSink(Vec::new());
    source.sync(None, &mut sink).await.unwrap();

    // The Tidewater fixture's tickets: PAY-200/231/228/240/219/236 and OPS-77.
    assert_eq!(
        sink.0.len(),
        7,
        "expected the fixture's seven issues, got {}",
        sink.0.len()
    );
    let pay231 = sink
        .0
        .iter()
        .find(|i| i.entity.key == "PAY-231")
        .expect("PAY-231 is in the fixture");
    assert_eq!(pay231.entity.to_string(), "jira:PAY-231");
    assert_eq!(pay231.title, "Retry failed SEPA payouts");
    assert_eq!(pay231.kind, "ticket");
    assert!(
        pay231
            .web_url
            .as_deref()
            .expect("P5: the adapter can say where a human reads this")
            .ends_with("/browse/PAY-231")
    );
    assert!(pay231.updated_at.is_some());
    // §3a: the raw record is kept, so a later mapping can re-project it.
    assert_eq!(pay231.payload["key"], "PAY-231");
    // `fields=comment,worklog` is honoured, so the discussion arrives with the
    // page rather than costing a request per issue -- and it reaches `body_text`,
    // which is what FTS indexes.
    assert_eq!(
        pay231.payload["fields"]["comment"]["comments"]
            .as_array()
            .expect("the comment container came back")
            .len(),
        2
    );
    assert_eq!(
        pay231.payload["fields"]["worklog"]["worklogs"]
            .as_array()
            .expect("the worklog container came back")
            .len(),
        1
    );
    for item in &sink.0 {
        assert_eq!(item.entity.namespace, "jira");
        assert!(!item.deleted, "jira search cannot report deletions");
    }
    jira.assert_no_violations();
}

/// Exit criterion: pagination across >= 2 pages, i.e. `maxResults` < `total`.
///
/// The `ticket` kind claims `full_sync_exhaustive: true`, and the engine
/// tombstones every row of such a kind that a full sync did not return -- so a
/// run that stopped
/// after page one would delete live issues from the mirror, not merely sync
/// fewer of them.
#[tokio::test]
async fn pages_across_more_than_one_response() {
    let jira = spawn_mock_jira().await;
    let paged = source(&jira.base_url(), serde_json::json!({ "page_size": 2 }));
    let single = source(&jira.base_url(), serde_json::json!({}));
    let (mut a, _) = sync_all(paged.as_ref(), None).await;
    let (mut b, _) = sync_all(single.as_ref(), None).await;
    a.sort();
    b.sort();
    assert!(a.len() > 2, "the fixture must not fit in one 2-issue page");
    assert_eq!(a, b, "paging must not change which issues arrive");
    jira.assert_no_violations();
}

/// Exit criterion: `touch_issue` ⇒ the next incremental returns exactly that
/// issue and the watermark advances.
#[tokio::test]
async fn an_incremental_run_returns_exactly_what_changed() {
    let jira = spawn_mock_jira().await;
    let source = source(&jira.base_url(), serde_json::json!({}));
    let (_, cursor) = sync_all(source.as_ref(), None).await;

    jira.touch_issue("PAY-231");
    let (keys, advanced) = sync_all(source.as_ref(), Some(cursor.clone())).await;
    assert_eq!(keys, vec!["PAY-231".to_owned()]);
    assert_ne!(
        advanced, cursor,
        "the watermark must move when an item was delivered"
    );
    jira.assert_no_violations();
}

/// Exit criterion, and battery clause 2 against the real wire: an idle
/// incremental returns the same cursor and zero items.
///
/// Necessary, and on its own **not** sufficient -- see
/// [`one_new_issue_then_an_idle_poll_settles`](fn@one_new_issue_then_an_idle_poll_settles).
#[tokio::test]
async fn an_idle_incremental_returns_nothing_and_the_same_cursor() {
    let jira = spawn_mock_jira().await;
    let source = source(&jira.base_url(), serde_json::json!({}));
    let (_, cursor) = sync_all(source.as_ref(), None).await;
    let (keys, again) = sync_all(source.as_ref(), Some(cursor.clone())).await;
    assert!(keys.is_empty(), "{keys:?}");
    assert_eq!(again, cursor);
    jira.assert_no_violations();
}

/// Cursor discipline as the scheduler actually exercises it: **one new issue,
/// then an idle poll**, twice over.
///
/// Full-sync-then-idle is stable even when the run tells the cursor only what
/// it *emitted*, because a full sync skips nothing. This sequence is not: the
/// third run skips PAY-240 as already delivered, and if that pair drops out of
/// `seen`, the fourth run -- whose two-minute window still reaches over it --
/// emits it again, and every poll after that emits something forever. The
/// watermark holds perfectly throughout, so nothing watching the cursor's
/// timestamp can see it happening.
///
/// mockd's clock advances exactly one minute per `touch_issue`, which is what
/// keeps both touched issues inside the same overlap window.
#[tokio::test]
async fn one_new_issue_then_an_idle_poll_settles() {
    let jira = spawn_mock_jira().await;
    let source = source(&jira.base_url(), serde_json::json!({}));

    let (full, c1) = sync_all(source.as_ref(), None).await;
    assert_eq!(full.len(), 7);

    jira.touch_issue("PAY-240");
    let (second, c2) = sync_all(source.as_ref(), Some(c1)).await;
    assert_eq!(second, vec!["PAY-240".to_owned()]);

    jira.touch_issue("PAY-228");
    let (third, c3) = sync_all(source.as_ref(), Some(c2)).await;
    assert_eq!(third, vec!["PAY-228".to_owned()]);

    let (idle, c4) = sync_all(source.as_ref(), Some(c3.clone())).await;
    assert!(
        idle.is_empty(),
        "an issue skipped as already-delivered must stay in the cursor's `seen` set; \
         re-emitted: {idle:?}"
    );
    assert_eq!(
        c4, c3,
        "an idle poll must hand back the cursor it was given"
    );
    jira.assert_no_violations();
}

/// Exit criterion: 401 ⇒ `SourceError::Unauthorized`, from both entry points --
/// and the case stream F's credential-health path is built for, a credential
/// that worked when the source was added and stopped working later.
#[tokio::test]
async fn a_rejected_credential_is_unauthorized_from_both_entry_points() {
    let jira = spawn_mock_jira().await;
    let source = source(&jira.base_url(), serde_json::json!({}));
    source
        .test_connection()
        .await
        .expect("connects before the fault");

    jira.set_fault(MockFault::Unauthorized);
    assert!(matches!(
        source.test_connection().await,
        Err(SourceError::Unauthorized)
    ));
    let synced = source.sync(None, &mut VecSink(Vec::new())).await;
    assert!(
        matches!(synced, Err(SourceError::Unauthorized)),
        "{synced:?}"
    );
    jira.assert_no_violations();
}

/// Exit criterion: timeout ⇒ `Unreachable`. A one-second request timeout, so
/// the suite does not wait out the 30-second production default.
#[tokio::test]
async fn a_timeout_is_unreachable() {
    let jira = spawn_mock_jira().await;
    jira.set_fault(MockFault::Timeout { hang_ms: 2_000 });
    let source = source(
        &jira.base_url(),
        serde_json::json!({ "request_timeout_secs": 1 }),
    );
    let connected = source.test_connection().await;
    assert!(
        matches!(connected, Err(SourceError::Unreachable(_))),
        "{connected:?}"
    );
    jira.assert_no_violations();
}

/// Exit criterion: `write_ops` empty, and every write refused.
///
/// mockd *implements* `POST /rest/api/2/issue/{key}/comment` for M2, so a write
/// that leaked out would be accepted, not recorded as a violation. The check
/// that the refusal is real is therefore the fixture itself: PAY-231 must still
/// carry exactly the two comments it started with.
#[tokio::test]
async fn the_adapter_is_read_only() {
    let jira = spawn_mock_jira().await;
    let source = source(&jira.base_url(), serde_json::json!({}));
    let d = source.descriptor();
    assert!(d.write_ops.is_empty());
    assert!(d.capabilities.is_empty());

    let before = jira
        .state()
        .issue("PAY-231")
        .expect("PAY-231 is in the fixture")
        .comments
        .len();
    let refused = source
        .write(WriteOp::Comment {
            entity: "jira:PAY-231".to_owned(),
            body: "M2, not M1".to_owned(),
        })
        .await;
    assert!(
        matches!(refused, Err(SourceError::Protocol(_))),
        "{refused:?}"
    );
    let after = jira
        .state()
        .issue("PAY-231")
        .expect("PAY-231 is in the fixture")
        .comments
        .len();
    assert_eq!(
        after, before,
        "a refused write must not have reached the server"
    );
    jira.assert_no_violations();
}

/// P4: the Add-source flow shows who it connected as and which server answered.
#[tokio::test]
async fn test_connection_reports_the_account_and_the_server() {
    let jira = spawn_mock_jira().await;
    let source = source(&jira.base_url(), serde_json::json!({}));
    let info = source.test_connection().await.unwrap();
    assert_eq!(info.account.as_deref(), Some("mara.lindqvist"), "{info:?}");
    assert_eq!(info.server_version.as_deref(), Some("9.17.0"), "{info:?}");
    assert_eq!(info.detail.as_deref(), Some("Server 9.17.0"), "{info:?}");
    // PAT expiry needs /rest/pat/latest/tokens, which is outside M1's endpoints.
    assert!(info.secret_expires_at.is_none());
    jira.assert_no_violations();
}

/// P10: the `EntityRef` namespace is the **instance** id, not the adapter kind.
///
/// Every other test here uses an instance called `jira`, which is also
/// `ADAPTER_KIND` -- so the two are indistinguishable and a `sync` wired to the
/// constant would pass all of them. Two Jiras are `jira` and `jira-eu`, and
/// their items must not collide in the mirror.
#[tokio::test]
async fn a_second_instance_of_the_same_jira_carries_its_own_namespace() {
    let jira = spawn_mock_jira().await;
    let eu = source_named("jira-eu", &jira.base_url(), serde_json::json!({}));
    assert_eq!(eu.descriptor().id, "jira-eu");
    let mut sink = VecSink(Vec::new());
    eu.sync(None, &mut sink).await.expect("sync succeeds");
    assert!(!sink.0.is_empty());
    for item in &sink.0 {
        assert_eq!(
            item.entity.namespace, "jira-eu",
            "{} is namespaced to the adapter kind, not to the instance",
            item.entity
        );
    }
    assert!(
        sink.0
            .iter()
            .any(|i| i.entity.to_string() == "jira-eu:PAY-231")
    );
    jira.assert_no_violations();
}

/// Scoping by project must reach the server as JQL, not as a client-side
/// filter: a 50,000-issue instance cannot be filtered locally.
///
/// The unscoped run is the control -- without it, an adapter that syncs nothing
/// at all would pass the "every key starts with PAY-" half.
#[tokio::test]
async fn a_project_scope_is_pushed_into_the_jql() {
    let jira = spawn_mock_jira().await;
    let scoped = source(&jira.base_url(), serde_json::json!({ "projects": ["PAY"] }));
    let all = source(&jira.base_url(), serde_json::json!({}));
    let (keys, _) = sync_all(scoped.as_ref(), None).await;
    let (every, _) = sync_all(all.as_ref(), None).await;
    assert!(!keys.is_empty());
    assert!(keys.iter().all(|k| k.starts_with("PAY-")), "{keys:?}");
    assert!(
        every.iter().any(|k| k.starts_with("OPS-")),
        "the unscoped run must see the project the scoped one filtered out: {every:?}"
    );
    assert_eq!(keys.len(), every.len() - 1, "{keys:?} vs {every:?}");
    jira.assert_no_violations();
}

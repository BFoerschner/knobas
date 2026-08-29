//! Stream C's certification: the adapter against the HTTP-level mock.
//!
//! The unit tests inside the crate pin the *algorithm*; these pin the *wire
//! contract* -- that every request this adapter sends is one `knobas-mockd`'s
//! TeamCity route table, locator grammar and `fields=` validator recognise,
//! and that the Tidewater fixture comes back as knobas entities.
//!
//! Three things about mockd shape the tests below and are worth stating once:
//!
//! * **mockd accepts any non-empty `Bearer`/`Basic` credential.** A "wrong
//!   token" is therefore not a 401 -- rejection is what
//!   [`MockFault::Unauthorized`] is for, and the unauthorized cases run
//!   against their own server with that fault set.
//! * **`fields=` is validated against a closed set of names** (mockd deviation
//!   6) and **the route table is the allowlist**. Both answer a mistake with a
//!   real error *and* a recorded [`Violation`](knobas_mockd::Violation), which
//!   is why every test here ends with `assert_no_violations()`.
//! * **Builds come back newest-first**, as they do on a real server, and
//!   nothing below asserts an order -- because the run does not depend on one.
//!   The opening probe's page is read for its **maximum** id, which is the
//!   same number however the rows are arranged. It used to be read for row 0,
//!   and a real TeamCity answering an unordered page wedged the source
//!   permanently (issue #91); `sync`'s own unit tests hold that, against a
//!   fake that can serve a page whose row 0 is not its maximum, which this one
//!   cannot.
//!
//! Expectations are **computed from `knobas_source_mock::fixture()`**, not
//! hard-coded: mockd transcribes `build.num` to `build.id` and `build.cfg` to
//! `buildType.id`, and if that ever changes these fail naming the ids they
//! actually saw rather than a bare assertion.

use knobas_mockd::{MockFault, MockServer, TcStatus, spawn_mock_teamcity};
use knobas_source::contract::{Fault, VecSink, battery};
use knobas_source::instance::SourceInstance;
use knobas_source::{AuthMethod, Source, SourceError, SyncItem};

fn instance(base_url: &str, config: serde_json::Value) -> SourceInstance {
    SourceInstance {
        id: "teamcity".to_owned(),
        kind: knobas_source_teamcity::ADAPTER_KIND.to_owned(),
        display_name: "Tidewater CI".to_owned(),
        base_url: base_url.to_owned(),
        auth: Some(AuthMethod::Pat),
        secret: Some(knobas_mockd::TEAMCITY_TOKEN.to_owned()),
        config,
    }
}

fn adapter(base_url: &str, config: serde_json::Value) -> Box<dyn Source> {
    match knobas_source_teamcity::build(instance(base_url, config)) {
        Ok(s) => s,
        // `Box<dyn Source>` is not `Debug`, so `expect` is unavailable.
        Err(e) => panic!("the adapter must build against mockd: {e:?}"),
    }
}

/// A build configuration in the fixture that has **no** build in flight.
///
/// The contract battery requires an incremental sync after no changes to emit
/// nothing -- and a running build is, by definition, something to re-emit on
/// every poll. Scoping the certified instance to a quiet configuration is what
/// makes clause 2 meaningful here rather than vacuous or impossible.
fn quiet_build_type() -> String {
    let f = knobas_source_mock::fixture();
    let mut cfgs: Vec<&str> = f.builds.iter().map(|b| b.cfg.as_str()).collect();
    cfgs.sort_unstable();
    cfgs.dedup();
    cfgs.into_iter()
        .find(|cfg| {
            f.builds
                .iter()
                .filter(|b| &b.cfg == cfg)
                .all(|b| b.status != "running")
        })
        .expect("the fixture has a configuration with no running build")
        .to_owned()
}

/// The newest finished build id in that configuration -- the watermark a full
/// sync scoped to it must land on.
fn quiet_newest_build() -> i64 {
    let cfg = quiet_build_type();
    knobas_source_mock::fixture()
        .builds
        .iter()
        .filter(|b| b.cfg == cfg && b.status != "running")
        .map(|b| i64::from(b.num))
        .max()
        .expect("that configuration has a finished build")
}

/// The fixture's running build, and the configuration it belongs to.
fn running_build() -> (i64, String) {
    let b = knobas_source_mock::fixture()
        .builds
        .iter()
        .find(|b| b.status == "running")
        .expect("the fixture has a running build");
    (i64::from(b.num), b.cfg.clone())
}

async fn sync(source: &dyn Source, cursor: Option<String>) -> (Vec<SyncItem>, String) {
    let mut sink = VecSink(Vec::new());
    let next = source.sync(cursor, &mut sink).await.expect("sync succeeds");
    (sink.0, next)
}

fn keys(items: &[SyncItem]) -> Vec<String> {
    items.iter().map(|i| i.entity.key.clone()).collect()
}

/// The watermark inside an opaque cursor. Only the tests that assert an
/// *inequality* look inside it; the ones that assert a fixed position compare
/// the whole string, because byte-stability is itself the contract.
fn since_build_id(cursor: &str) -> i64 {
    serde_json::from_str::<serde_json::Value>(cursor)
        .ok()
        .and_then(|v| v["since_build_id"].as_i64())
        .unwrap_or_else(|| panic!("not a cursor this adapter wrote: {cursor}"))
}

/// The shared suite every adapter must pass.
///
/// The unauthorized case gets **its own server** with the fault set, rather
/// than a wrong token against the shared one: mockd accepts any non-empty
/// credential, and the battery keeps the healthy adapter alive and calls it
/// again after building the faulted ones, so a fault on the shared server
/// would make those later calls fail with the wrong error.
#[tokio::test]
async fn passes_the_contract_battery() {
    let healthy = spawn_mock_teamcity().await;
    let denied = spawn_mock_teamcity().await;
    denied.set_fault(MockFault::Unauthorized);
    let healthy_url = healthy.base_url();
    let denied_url = denied.base_url();
    let dead_url = knobas_mockd::refused_url();
    let config = serde_json::json!({ "build_type_ids": [quiet_build_type()] });

    battery(move |fault| {
        let url = match fault {
            Fault::None => healthy_url.clone(),
            Fault::Unauthorized => denied_url.clone(),
            Fault::Unreachable => dead_url.clone(),
        };
        adapter(&url, config.clone())
    })
    .await;

    // An endpoint, verb, locator dimension or field name this adapter invented
    // would be recorded here rather than silently accepted.
    healthy.assert_no_violations();
    denied.assert_no_violations();
}

#[tokio::test]
async fn a_full_sync_mirrors_the_build_configurations_and_builds() {
    let server = spawn_mock_teamcity().await;
    let source = adapter(&server.base_url(), serde_json::json!({}));
    let (items, _) = sync(source.as_ref(), None).await;

    let declared: Vec<String> = source
        .descriptor()
        .entity_kinds
        .iter()
        .map(|k| k.id.clone())
        .collect();
    assert!(!items.is_empty(), "the mock's fixture is not empty");
    for it in &items {
        assert_eq!(it.entity.namespace, "teamcity");
        assert!(declared.contains(&it.kind), "undeclared kind {:?}", it.kind);
        assert!(
            it.entity.key.starts_with("build:") || it.entity.key.starts_with("buildType:"),
            "unexpected key {:?}",
            it.entity.key
        );
        assert!(!it.title.trim().is_empty());
        assert!(!it.payload.is_null(), "the raw record is kept (spec §3a)");
        assert!(!it.deleted);
    }
    let f = knobas_source_mock::fixture();
    for cfg in f.builds.iter().map(|b| b.cfg.as_str()) {
        assert!(
            items
                .iter()
                .any(|i| i.entity.key == format!("buildType:{cfg}")),
            "configuration {cfg} missing; got {:?}",
            keys(&items)
        );
    }
    for num in f.builds.iter().map(|b| b.num) {
        assert!(
            items.iter().any(|i| i.entity.key == format!("build:{num}")),
            "build {num} missing; got {:?}",
            keys(&items)
        );
    }
    server.assert_no_violations();
}

/// The mapping, end to end against the mock's own transcription of the
/// fixture: what the launcher renders and what FTS indexes.
#[tokio::test]
async fn a_mirrored_build_carries_what_the_ui_and_the_index_read() {
    let server = spawn_mock_teamcity().await;
    let (items, _) = sync(
        adapter(&server.base_url(), serde_json::json!({})).as_ref(),
        None,
    )
    .await;

    let failed = knobas_source_mock::fixture()
        .builds
        .iter()
        .find(|b| b.status == "failed")
        .expect("the fixture has a failed build")
        .clone();
    let it = items
        .iter()
        .find(|i| i.entity.key == format!("build:{}", failed.num))
        .unwrap_or_else(|| panic!("build {} missing; got {:?}", failed.num, keys(&items)));
    assert_eq!(it.kind, "build");
    assert!(
        it.title.contains(&failed.num.to_string()),
        "the build number names it to a human: {:?}",
        it.title
    );
    assert!(
        it.body_text.contains("FAILURE"),
        "a failed build is searchable as one: {:?}",
        it.body_text
    );
    assert!(
        it.body_text.contains(&failed.branch),
        "the branch is what a build is looked up by: {:?}",
        it.body_text
    );
    // P5: *Open in browser* renders from this alone.
    assert!(
        it.web_url
            .as_deref()
            .expect("the adapter can say where a human reads this")
            .contains(&format!("buildId={}", failed.num)),
        "{:?}",
        it.web_url
    );
    // Interfaces §4.1: the source's own timestamp, never `now()`.
    assert!(
        it.updated_at.expect("a finished build is dated") >= failed.when,
        "a finished build is dated at or after it started"
    );

    // ...and the configuration it belongs to, which is a different kind with
    // different rules.
    let cfg = items
        .iter()
        .find(|i| i.entity.key == format!("buildType:{}", failed.cfg))
        .expect("its configuration");
    assert_eq!(cfg.kind, "build_config");
    assert!(
        cfg.body_text.contains(&failed.cfg),
        "the configuration id is searchable: {:?}",
        cfg.body_text
    );
    assert_eq!(
        cfg.updated_at, None,
        "TeamCity dates no configuration, and `now()` is forbidden"
    );
    server.assert_no_violations();
}

/// Authorship, at the wire: `triggered(user(username))` is the only place
/// TeamCity names the person who started a build, so the selector has to ask
/// for it and the mock has to recognise the name.
///
/// The fixture names a person for one build and nobody for the others, and
/// both are asserted here. A run that reported the same author for every build
/// -- or `None` for every build, which is what the narrow selector produced --
/// would fail one of the two.
#[tokio::test]
async fn a_build_names_the_person_who_triggered_it() {
    let server = spawn_mock_teamcity().await;
    let (items, _) = sync(
        adapter(&server.base_url(), serde_json::json!({})).as_ref(),
        None,
    )
    .await;

    let f = knobas_source_mock::fixture();
    let mut named = 0;
    for b in &f.builds {
        let it = items
            .iter()
            .find(|i| i.entity.key == format!("build:{}", b.num))
            .unwrap_or_else(|| panic!("build {} missing; got {:?}", b.num, keys(&items)));
        match b.triggered_by.as_deref() {
            Some(id) => {
                named += 1;
                let p = f
                    .person(id)
                    .expect("the triggerer is a person in the fixture");
                assert_eq!(
                    it.author.as_deref(),
                    Some(p.username.as_str()),
                    "build {} was triggered by {id}",
                    b.num
                );
                assert!(
                    it.body_text.contains(&p.username),
                    "...and is searchable by that person: {:?}",
                    it.body_text
                );
            }
            None => assert_eq!(
                it.author, None,
                "the fixture names nobody for build {}, and an invented author would be indexed \
                 as if it had",
                b.num
            ),
        }
    }
    assert!(
        named > 0 && named < f.builds.len(),
        "the fixture must name a triggerer for some builds and not others, or neither branch \
         above can fail"
    );
    server.assert_no_violations();
}

/// A build configuration's prose reaches the search blob -- selector asks,
/// mockd serves, `map` reads, `body_text` carries it.
///
/// The description is set here rather than read from the fixture because no
/// fixture configuration has one, and with the value `null` everywhere a
/// selector that asks for `description` and one that does not produce
/// byte-identical output. Without this the widening that added it to
/// `BUILD_TYPE_FIELDS` would be pinned by a string comparison in `rest`'s unit
/// tests and by nothing on the wire: dropping the name again would leave all
/// seventeen tests in this file green. Same shape as
/// `a_full_sync_is_a_window_...`, which creates the second build the fixture
/// does not have for the same reason.
#[tokio::test]
async fn a_configuration_description_reaches_the_search_blob() {
    let server = spawn_mock_teamcity().await;
    let cfg = quiet_build_type();
    let prose = "Deploys the ledger to staging after every merge to main";
    server.state().describe_build_type(&cfg, prose);

    let (items, _) = sync(
        adapter(&server.base_url(), serde_json::json!({})).as_ref(),
        None,
    )
    .await;
    let it = items
        .iter()
        .find(|i| i.entity.key == format!("buildType:{cfg}"))
        .unwrap_or_else(|| panic!("{cfg} missing; got {:?}", keys(&items)));
    assert!(
        it.body_text.contains(prose),
        "the configuration's description is what a human searches for when they cannot \
         remember its id: {:?}",
        it.body_text
    );
    // ...and it is in the payload verbatim, so the selector really asked for
    // it rather than the blob having been built from something else.
    assert_eq!(it.payload["description"], prose);

    // The configurations nobody described still carry none, so this cannot
    // pass by the adapter inventing prose for every configuration.
    for other in items
        .iter()
        .filter(|i| i.kind == "build_config" && i.entity.key != format!("buildType:{cfg}"))
    {
        assert!(
            !other.body_text.contains(prose),
            "only the described configuration carries it: {:?}",
            other.body_text
        );
        assert!(
            other.payload.get("description").is_none(),
            "TeamCity omits an absent description rather than sending null: {:?}",
            other.payload
        );
    }
    server.assert_no_violations();
}

/// Exit criterion: `sinceBuild` advances only when a finished build was
/// emitted, and an idle poll changes nothing.
#[tokio::test]
async fn the_watermark_advances_on_a_finished_build_and_then_stands_still() {
    let server = spawn_mock_teamcity().await;
    let source = adapter(
        &server.base_url(),
        serde_json::json!({ "build_type_ids": [quiet_build_type()] }),
    );
    let (items, cursor) = sync(source.as_ref(), None).await;
    assert!(
        items
            .iter()
            .any(|i| i.entity.key == format!("build:{}", quiet_newest_build()))
    );
    assert_eq!(
        cursor,
        format!(r#"{{"v":1,"since_build_id":{}}}"#, quiet_newest_build())
    );

    let (idle_items, idle) = sync(source.as_ref(), Some(cursor.clone())).await;
    assert!(idle_items.is_empty(), "nothing changed upstream");
    assert_eq!(idle, cursor, "byte-identical");
    server.assert_no_violations();
}

/// Exit criterion: a running build is re-polled every run -- there is no
/// watermark that could find it, because its id was handed out when it was
/// queued -- and re-polling it never moves the cursor.
#[tokio::test]
async fn a_running_build_is_re_polled_without_moving_the_cursor() {
    let server = spawn_mock_teamcity().await;
    let (running, _) = running_build();
    let source = adapter(&server.base_url(), serde_json::json!({}));
    let (_, cursor) = sync(source.as_ref(), None).await;
    // A *numeric* comparison, via the helper documented for exactly this. The
    // string form this used to compare is a lexicographic ordering of two JSON
    // documents standing in for an ordering of two build ids: it agrees only
    // while the ids have equal digit counts, and a watermark of 11880 against
    // a running build of 1188 would compare '0' against '}' and pass while
    // being numerically far above the build it claims to sit below.
    assert!(
        since_build_id(&cursor) < running,
        "the watermark must sit below the running build {running}: {cursor}"
    );

    let (items, next) = sync(source.as_ref(), Some(cursor.clone())).await;
    assert!(
        items
            .iter()
            .any(|i| i.entity.key == format!("build:{running}")),
        "the running build is re-fetched without a new id to find it by; got {:?}",
        keys(&items)
    );
    assert_eq!(
        next, cursor,
        "a running build alone does not move the watermark"
    );
    server.assert_no_violations();
}

/// The other exit criterion, at the wire level: a build that **finishes**
/// arrives on the next incremental run, moves the watermark, and the run after
/// that is idle.
///
/// Idle-stability is certified as *one new event, then idle* rather than
/// *full sync, then idle*: a full sync skips nothing, so the full-sync path is
/// stable even when the incremental path is broken. Only a run that had to
/// decide what to skip proves the decision is right.
#[tokio::test]
async fn a_newly_finished_build_arrives_incrementally_and_then_the_source_is_idle() {
    let server = spawn_mock_teamcity().await;
    let (running, cfg) = running_build();
    // Scoped to the running build's own configuration, so the event under test
    // is the only thing that can move.
    let source = adapter(
        &server.base_url(),
        serde_json::json!({ "build_type_ids": [cfg] }),
    );
    let (_, cursor) = sync(source.as_ref(), None).await;

    server
        .state()
        .finish_build(running as u64, TcStatus::Success);

    let (items, after) = sync(source.as_ref(), Some(cursor.clone())).await;
    assert!(
        items
            .iter()
            .any(|i| i.entity.key == format!("build:{running}")),
        "the build that just finished; got {:?}",
        keys(&items)
    );
    assert_eq!(
        after,
        format!(r#"{{"v":1,"since_build_id":{running}}}"#),
        "a finished build is exactly what moves the watermark"
    );
    assert_ne!(after, cursor, "and it did move");

    let (idle_items, idle) = sync(source.as_ref(), Some(after.clone())).await;
    assert!(
        idle_items.is_empty(),
        "the run after the event skips it; got {:?}",
        keys(&idle_items)
    );
    assert_eq!(idle, after, "byte-identical");
    server.assert_no_violations();
}

/// A build queued after the last run is in flight, so it arrives through the
/// unconditional poll -- and holds the watermark below itself, because its id
/// was assigned now and its finish is still to come.
#[tokio::test]
async fn a_newly_queued_build_arrives_and_holds_the_watermark_below_itself() {
    let server = spawn_mock_teamcity().await;
    let cfg = quiet_build_type();
    let source = adapter(
        &server.base_url(),
        serde_json::json!({ "build_type_ids": [cfg.clone()] }),
    );
    let (_, cursor) = sync(source.as_ref(), None).await;
    assert_eq!(
        cursor,
        format!(r#"{{"v":1,"since_build_id":{}}}"#, quiet_newest_build())
    );

    let queued = server.state().queue_build(&cfg, "main");

    let (items, after) = sync(source.as_ref(), Some(cursor)).await;
    assert!(
        items
            .iter()
            .any(|i| i.entity.key == format!("build:{queued}")),
        "the queued build; got {:?}",
        keys(&items)
    );
    assert!(
        since_build_id(&after) < i64::try_from(queued).expect("a build id fits"),
        "the watermark must stay below a build still in flight -- `sinceBuild` is exclusive, so \
         a watermark at {queued} would put this build permanently out of reach once it finishes \
         (cursor {after})"
    );
    // ...which is not an abstract claim: the very next run still finds it.
    let (again, _) = sync(source.as_ref(), Some(after.clone())).await;
    assert!(
        again
            .iter()
            .any(|i| i.entity.key == format!("build:{queued}")),
        "still reachable while in flight; got {:?}",
        keys(&again)
    );

    // ...and once it finishes, the watermark reaches it.
    server.state().finish_build(queued, TcStatus::Success);
    let (items, done) = sync(source.as_ref(), Some(after)).await;
    assert!(
        items
            .iter()
            .any(|i| i.entity.key == format!("build:{queued}")),
        "the same build, now finished; got {:?}",
        keys(&items)
    );
    assert_eq!(done, format!(r#"{{"v":1,"since_build_id":{queued}}}"#));
    server.assert_no_violations();
}

/// The descriptor's `full_sync_exhaustive: false` is a claim about this run,
/// and this is the run that makes it true: a configuration with two finished
/// builds and `builds_per_config: 1` mirrors **one** of them.
///
/// The fixture cannot witness this on its own -- every configuration in it has
/// exactly one build -- so the second build is created here. Without it the
/// window and the corpus are the same thing and the assertion could not fail.
#[tokio::test]
async fn a_full_sync_is_a_window_which_is_why_the_sweep_must_not_run() {
    let server = spawn_mock_teamcity().await;
    let cfg = quiet_build_type();
    let extra = server.state().queue_build(&cfg, "main");
    server.state().finish_build(extra, TcStatus::Success);

    let unwindowed = adapter(
        &server.base_url(),
        serde_json::json!({ "build_type_ids": [cfg.clone()] }),
    );
    let (all, _) = sync(unwindowed.as_ref(), None).await;
    assert_eq!(
        all.iter().filter(|i| i.kind == "build").count(),
        2,
        "the configuration really does have two finished builds now: {:?}",
        keys(&all)
    );

    let windowed = adapter(
        &server.base_url(),
        serde_json::json!({ "build_type_ids": [cfg], "builds_per_config": 1 }),
    );
    let (window, _) = sync(windowed.as_ref(), None).await;
    assert_eq!(
        window.iter().filter(|i| i.kind == "build").count(),
        1,
        "a full sync mirrors a window, not the corpus: {:?}",
        keys(&window)
    );
    // Which is precisely why the engine must not tombstone what this run did
    // not re-emit.
    assert!(
        !windowed
            .descriptor()
            .entity_kinds
            .iter()
            .any(|k| k.full_sync_exhaustive)
    );
    server.assert_no_violations();
}

#[tokio::test]
async fn a_401_is_unauthorized_from_both_entry_points() {
    let server = spawn_mock_teamcity().await;
    server.set_fault(MockFault::Unauthorized);
    let source = adapter(&server.base_url(), serde_json::json!({}));
    let connected = source.test_connection().await;
    assert!(
        matches!(connected, Err(SourceError::Unauthorized { .. })),
        "{connected:?}"
    );
    let synced = source.sync(None, &mut VecSink(Vec::new())).await;
    assert!(
        matches!(synced, Err(SourceError::Unauthorized { .. })),
        "{synced:?}"
    );
    // A faulted server still gets well-formed requests.
    server.assert_no_violations();
}

/// A 500 is a protocol failure, not a credential one: the sources view must
/// not offer *Re-enter* for someone else's outage.
#[tokio::test]
async fn a_server_error_is_a_protocol_failure() {
    let server = spawn_mock_teamcity().await;
    server.set_fault(MockFault::ServerError);
    let source = adapter(&server.base_url(), serde_json::json!({}));
    let connected = source.test_connection().await;
    assert!(
        matches!(connected, Err(SourceError::Protocol { .. })),
        "{connected:?}"
    );
    let synced = source.sync(None, &mut VecSink(Vec::new())).await;
    assert!(
        matches!(synced, Err(SourceError::Protocol { .. })),
        "{synced:?}"
    );
    server.assert_no_violations();
}

#[tokio::test]
async fn a_server_that_does_not_answer_is_unreachable() {
    let source = adapter(&knobas_mockd::refused_url(), serde_json::json!({}));
    let connected = source.test_connection().await;
    assert!(
        matches!(connected, Err(SourceError::Unreachable(_))),
        "{connected:?}"
    );
    let synced = source.sync(None, &mut VecSink(Vec::new())).await;
    assert!(
        matches!(synced, Err(SourceError::Unreachable(_))),
        "{synced:?}"
    );
}

#[tokio::test]
async fn test_connection_reports_the_server_version_and_the_account() {
    let server = spawn_mock_teamcity().await;
    let info = adapter(&server.base_url(), serde_json::json!({}))
        .test_connection()
        .await
        .expect("connected");
    assert!(
        info.server_version.is_some(),
        "the Add-source flow shows the version it reached"
    );
    // P4: "Connected as …" is what tells a wrong-account token from a working
    // one. The fixture's one seat.
    assert_eq!(
        info.account.as_deref(),
        Some(
            knobas_source_mock::fixture()
                .person("mara")
                .expect("the fixture has Mara")
                .username
                .as_str()
        )
    );
    assert_eq!(
        info.secret_expires_at, None,
        "TeamCity publishes no token expiry over REST"
    );
    server.assert_no_violations();
}

/// `assert_no_violations()` is worth nothing unless the mock records
/// violations at all -- so the guard is mutated here rather than trusted: one
/// deliberately malformed request must both be refused **and** make
/// `assert_no_violations()` panic.
///
/// It also pins the documented 406 + `X-Mockd-Hint` deviation (P11), which
/// nobody is to "fix".
#[tokio::test]
async fn the_violation_log_catches_a_request_this_adapter_would_never_send() {
    let server = spawn_mock_teamcity().await;
    // Before: the guard is quiet, so the panic below is caused by this request
    // and not by leftover noise.
    server.assert_no_violations();

    let response = reqwest::Client::new()
        .get(format!("{}/app/rest/server", server.base_url()))
        .header("Authorization", "Bearer x")
        .send()
        .await
        .expect("reached the mock");
    assert_eq!(
        response.status().as_u16(),
        406,
        "mockd deviation 1: no Accept: application/json, no answer"
    );
    assert!(response.headers().contains_key("x-mockd-hint"));

    assert!(
        !server.violations().is_empty(),
        "the mock records what its contract forbids"
    );
    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        server.assert_no_violations();
    }));
    assert!(
        panicked.is_err(),
        "assert_no_violations() must fail once a violation is on the log -- otherwise every \
         other test in this file certifies nothing"
    );
}

/// The same, one level deeper: a locator dimension outside the grammar is a
/// 400 **and** a violation. This is the guard that would catch the superseded
/// `state:running,state:queued` spelling, so it is worth knowing it is live.
#[tokio::test]
async fn the_superseded_in_flight_spelling_is_still_refused() {
    let server = spawn_mock_teamcity().await;
    let response = reqwest::Client::new()
        .get(format!(
            "{}/app/rest/builds?locator=state:running,state:queued&fields=count",
            server.base_url()
        ))
        .header("Accept", "application/json")
        .header("Authorization", "Bearer x")
        .send()
        .await
        .expect("reached the mock");
    assert_eq!(response.status().as_u16(), 400);
    let body = response.text().await.unwrap_or_default();
    assert!(
        body.contains("state:(queued:true,running:true)"),
        "the refusal names the spelling this adapter does send: {body}"
    );
    assert!(!server.violations().is_empty());
}

/// The replaced-server refusal, over the wire.
///
/// The unit tests hold the *decision*; this holds the request it rests on.
/// `GET /app/rest/builds/id:{id}` is a route mockd's allowlist knows and
/// `fields=id` a name its validator accepts, so an adapter that invented
/// either would be a recorded violation here rather than a 404 — which is
/// exactly the failure mode this check must not have, because it reads a 404
/// as "the server was replaced" and would then say so about a healthy one.
///
/// A watermark far above anything in the fixture is the trigger: nothing the
/// run witnesses reaches it, so the run asks the server about that one build
/// and gets the only answer that refuses.
#[tokio::test]
async fn a_watermark_no_build_answers_to_is_refused_over_the_wire() {
    let server = spawn_mock_teamcity().await;
    let source = adapter(&server.base_url(), serde_json::json!({}));
    let mut sink = VecSink(Vec::new());
    let err = source
        .sync(
            Some(r#"{"v":1,"since_build_id":999999}"#.to_owned()),
            &mut sink,
        )
        .await
        .expect_err("a watermark no build on this server answers to must not be synced past");
    assert!(
        matches!(&err, SourceError::Protocol { message: m, .. }
            if m.contains("`/app/rest/builds/id:999999` answers 404")
                && m.contains("reset the source's cursor")),
        "{err:?}"
    );
    assert!(sink.0.is_empty(), "nothing is emitted from a refused run");
    server.assert_no_violations();
}

/// Two TeamCitys must not overwrite each other's rows: the instance id is the
/// namespace of everything a source emits, and it is chosen at add time (P10).
#[tokio::test]
async fn a_second_instance_namespaces_its_items_to_itself() {
    let server = spawn_mock_teamcity().await;
    let mut i = instance(&server.base_url(), serde_json::json!({}));
    i.id = "teamcity-eu".to_owned();
    let source = match knobas_source_teamcity::build(i) {
        Ok(s) => s,
        Err(e) => panic!("the adapter must build: {e:?}"),
    };
    let (items, _) = sync(source.as_ref(), None).await;
    assert!(!items.is_empty());
    for it in &items {
        assert_eq!(it.entity.namespace, "teamcity-eu");
        assert!(it.entity.to_string().starts_with("teamcity-eu:"));
    }
    server.assert_no_violations();
}

/// Basic authentication reaches the same server the token does -- the mock
/// accepts either scheme, so this certifies the *spelling*, which is the half
/// no adapter gets right by inspection.
#[tokio::test]
async fn user_password_authentication_reaches_the_server() {
    let server: MockServer = spawn_mock_teamcity().await;
    let mut i = instance(
        &server.base_url(),
        serde_json::json!({ "username": "mara" }),
    );
    i.auth = Some(AuthMethod::UserPassword);
    i.secret = Some("a-password".to_owned());
    let source = match knobas_source_teamcity::build(i) {
        Ok(s) => s,
        Err(e) => panic!("the adapter must build: {e:?}"),
    };
    source.test_connection().await.expect("connected as basic");
    server.assert_no_violations();
}

/// The wire half of issue #114: `/app/rest/builds` says whether it truncated,
/// and it says so in `nextHref`.
///
/// The adapter ends a walk on that field (`sync::last_page`) rather than on a
/// page's length, so mockd not serving it was not a cosmetic gap -- it was the
/// fake teaching the reading the fix removes. Every page mockd answered
/// carried `nextHref: null`, so *every* short page looked like an exhausted
/// query and no fixture here could tell the two apart.
///
/// Asserted over raw HTTP rather than through a sync, because what is being
/// pinned is the shape of the response and not what the run does with it; the
/// run's side is `sync`'s own unit tests, against a fake that can cap.
///
/// The three cases are the ones the live server was measured on (2026-08-29):
/// a `count:` under the number of matches carries the field, a `count:` over
/// it does not, and -- the one that catches an off-by-one -- a `count:`
/// *equal* to it still carries it, because a page filled to its limit is not
/// proof there is nothing after it.
#[tokio::test]
async fn a_truncated_build_page_reports_the_next_one_and_an_exhausted_one_does_not() {
    let server: MockServer = spawn_mock_teamcity().await;
    let http = reqwest::Client::new();
    let rows = |page: &serde_json::Value| -> usize { page["build"].as_array().map_or(0, Vec::len) };
    // `defaultFilter:false`: the run's own opening-probe locator, and the one
    // that puts every build in the fixture on the page rather than the two
    // finished ones.
    let ask = async |count: usize| -> serde_json::Value {
        let url = format!(
            "{}/app/rest/builds?locator=defaultFilter:false,count:{count}&fields=count,nextHref,build(id)",
            server.base_url()
        );
        http.get(url)
            .header("Accept", "application/json")
            .bearer_auth(knobas_mockd::TEAMCITY_TOKEN)
            .send()
            .await
            .expect("mockd answers")
            .json()
            .await
            .expect("a JSON envelope")
    };

    // How many finished builds there are, from a page nothing could truncate.
    let all = ask(1_000).await;
    let total = rows(&all);
    assert!(total >= 2, "the fixture needs more than one build: {total}");
    assert!(
        all["nextHref"].is_null(),
        "a page wider than the query has nothing after it: {all}"
    );

    let short = ask(total - 1).await;
    assert_eq!(rows(&short), total - 1);
    assert!(
        short["nextHref"]
            .as_str()
            .is_some_and(|h| h.contains("start:")),
        "a truncated page has to say so, and say where the rest starts: {short}"
    );

    // Exactly the number of matches: still truncated as far as the server can
    // tell, because a full page is not proof there is no page after it.
    let exact = ask(total).await;
    assert_eq!(rows(&exact), total);
    assert!(
        !exact["nextHref"].is_null(),
        "a page filled to its limit is not an exhausted query: {exact}"
    );

    server.assert_no_violations();
}

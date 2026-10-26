//! Build configurations asked about side by side (#2176): `concurrent_configs`
//! of them have a build query in flight at once, 1 is the walk one after
//! another, and a fatal error in one ends the run with nothing left asking.
//!
//! The witness is [`Held`], a proxy in front of a wiremock TeamCity that holds
//! each configuration's build query (`/app/rest/builds` with
//! `buildType:(id:…)` in its locator) open until the test lets it go, so "in
//! flight together" is counted, not timed. On a full sync each arrival is
//! stamped with how many configurations the run had finished by then (its
//! walk reading, #1382), so a query that began before a slot was free is
//! caught whatever the machine's speed: at N at once, the i-th configuration
//! to be asked about may only be asked once i - N + 1 have finished.
//!
//! The fake answers every other build query (the opening probe, the in-flight
//! poll) with an empty page, so the run's only builds are the one each
//! configuration's query returns. Whether the real server answers these
//! shapes is `tests/live_teamcity.rs`'s to say; on disagreement this fake is
//! what is wrong.

use std::sync::Arc;
use std::time::Duration;

use crate::support::held::Held;
use knobas_source::instance::SourceInstance;
use knobas_source::{AuthMethod, Sink, Source, SourceError, SyncItem};
use serde_json::json;
use wiremock::matchers::{method, path, query_param_contains};
use wiremock::{Mock, MockServer, ResponseTemplate};

const TOKEN: &str = "tidewater-tc-token";

/// A deadline on waits that should end at once, so a regression fails instead
/// of hanging. Nothing is measured against it.
const DEADLINE: Duration = Duration::from_secs(20);

/// The build each configuration's query returns: `Cfg<n>` has build `100 + n`.
fn build_of(n: usize) -> i64 {
    100 + i64::try_from(n).expect("fits")
}

/// A wiremock TeamCity with configurations `Cfg1` to `Cfg<count>` in project
/// `P`, each with one finished build; `revoked` answers its build query 401.
async fn teamcity(count: usize, revoked: Option<&str>) -> MockServer {
    let server = MockServer::start().await;
    let ids: Vec<String> = (1..=count).map(|n| format!("Cfg{n}")).collect();
    Mock::given(method("GET"))
        .and(path("/app/rest/projects"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "count": 2,
            "project": [
                { "id": "_Root", "name": "<Root project>" },
                { "id": "P", "name": "P", "parentProjectId": "_Root" }
            ]
        })))
        .mount(&server)
        .await;
    let types: Vec<_> = ids
        .iter()
        .map(|id| json!({ "id": id, "name": id, "projectId": "P", "projectName": "P" }))
        .collect();
    Mock::given(method("GET"))
        .and(path("/app/rest/buildTypes"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "count": types.len(), "buildType": types })),
        )
        .mount(&server)
        .await;
    for (n, id) in (1..).zip(&ids) {
        let answer = if revoked == Some(id.as_str()) {
            ResponseTemplate::new(401)
        } else {
            ResponseTemplate::new(200).set_body_json(json!({
                "count": 1,
                "build": [{
                    "id": build_of(n), "number": build_of(n).to_string(),
                    "buildTypeId": id, "state": "finished", "status": "SUCCESS",
                    "buildType": { "id": id, "name": id, "projectId": "P", "projectName": "P" }
                }]
            }))
        };
        Mock::given(method("GET"))
            .and(path("/app/rest/builds"))
            .and(query_param_contains("locator", format!("buildType:(id:{id})")))
            .respond_with(answer)
            .with_priority(1)
            .mount(&server)
            .await;
    }
    // The opening probe and the in-flight poll: nothing anywhere else.
    Mock::given(method("GET"))
        .and(path("/app/rest/builds"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "count": 0, "build": [] })))
        .with_priority(10)
        .mount(&server)
        .await;
    server
}

fn source(base_url: String, config: serde_json::Value) -> Box<dyn Source> {
    let instance = SourceInstance {
        id: "teamcity".to_owned(),
        kind: knobas_source_teamcity::ADAPTER_KIND.to_owned(),
        display_name: "Tidewater CI".to_owned(),
        base_url,
        auth: Some(AuthMethod::Pat),
        secret: Some(TOKEN.to_owned()),
        account: None,
        config,
    };
    match knobas_source_teamcity::build(instance) {
        Ok(source) => source,
        Err(e) => panic!("the adapter builds against wiremock: {e:?}"),
    }
}

/// Hold every configuration's build query, under the configuration's id.
fn per_config(path: &str, query: &str) -> Option<String> {
    if path != "/app/rest/builds" {
        return None;
    }
    let locator = query
        .split('&')
        .find_map(|pair| pair.strip_prefix("locator="))?;
    let locator = decoded(locator);
    let rest = locator.split("buildType:(id:").nth(1)?;
    Some(rest.split(')').next()?.to_owned())
}

/// A query value as reqwest percent-encodes it, decoded.
fn decoded(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).expect("ascii");
                out.push(u8::from_str_radix(hex, 16).expect("a percent escape"));
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8(out).expect("utf-8")
}

async fn within<T>(what: &str, work: impl Future<Output = T>) -> T {
    tokio::time::timeout(DEADLINE, work)
        .await
        .unwrap_or_else(|_| panic!("{what} did not happen"))
}

/// Collects what a run emits and tells [`Held`] each walk reading.
struct Stamping {
    held: Arc<Held>,
    items: Vec<SyncItem>,
}

#[async_trait::async_trait]
impl Sink for Stamping {
    async fn item(&mut self, item: SyncItem) -> Result<(), SourceError> {
        self.items.push(item);
        Ok(())
    }

    async fn walked(&mut self, done: u64, _of: Option<u64>, unit: &str) {
        // Only a full sync's walk is over configurations; an incremental
        // run's is over the builds it listed, after every query.
        if unit == "build configurations" {
            self.held.walked(done);
        }
    }
}

/// Start a sync of `source` from `cursor` on a task of its own, returning the
/// keys it emitted, sorted, or its error.
fn spawn_sync(
    source: Box<dyn Source>,
    held: &Arc<Held>,
    cursor: Option<String>,
) -> tokio::task::JoinHandle<Result<Vec<String>, SourceError>> {
    let held = Arc::clone(held);
    tokio::spawn(async move {
        let mut sink = Stamping {
            held,
            items: Vec::new(),
        };
        source.sync(cursor, &mut sink).await?;
        let mut keys: Vec<String> = sink
            .items
            .iter()
            .map(|item| item.entity.key.clone())
            .collect();
        keys.sort();
        Ok(keys)
    })
}

/// Every configuration and its one build, sorted as [`spawn_sync`] sorts.
fn everything(count: usize, configs: bool) -> Vec<String> {
    let mut keys: Vec<String> = (1..=count)
        .map(|n| format!("build:{}", build_of(n)))
        .collect();
    if configs {
        keys.extend((1..=count).map(|n| format!("buildType:Cfg{n}")));
    }
    keys.sort();
    keys
}

/// No configuration was asked about before enough others had finished to
/// free a slot for it.
fn assert_never_more_than(held: &Held, at_once: usize) {
    for (i, (id, finished)) in held.arrivals().into_iter().enumerate() {
        let needed = u64::try_from((i + 1).saturating_sub(at_once)).expect("fits");
        assert!(
            finished >= needed,
            "{id} was query {} with {finished} finished; at {at_once} at once that needs {needed}",
            i + 1
        );
    }
}

/// Release the held configurations one at a time, each time waiting for the
/// run to ask about the next one, until every configuration has been let
/// through.
async fn release_all(held: &Held, total: usize, at_once: usize) {
    let mut released = 0;
    while released < total {
        let open = held.holding();
        let next = open.first().expect("a configuration is held").clone();
        held.release(&next);
        released += 1;
        let arrived = (released + at_once).min(total);
        let holding = arrived - released;
        within(
            "the next configuration's query",
            held.until(|h| h.arrived().len() == arrived && h.holding().len() == holding),
        )
        .await;
    }
}

#[tokio::test]
async fn a_full_sync_asks_about_up_to_concurrent_configs_configurations_at_once() {
    let server = teamcity(7, None).await;
    let held = Held::start(server.uri(), per_config).await;
    let run = spawn_sync(
        source(held.base_url(), json!({ "concurrent_configs": 3 })),
        &held,
        None,
    );

    within(
        "three configurations in flight",
        held.until(|h| h.holding().len() == 3),
    )
    .await;
    release_all(&held, 7, 3).await;

    let keys = within("the run", run).await.expect("joins").expect("syncs");
    assert_eq!(keys, everything(7, true), "every configuration and build once");
    assert_eq!(held.most(), 3, "three at once");
    assert_never_more_than(&held, 3);
}

/// At 1 the run is today's: one configuration at a time, in id order.
#[tokio::test]
async fn at_one_the_configurations_are_asked_about_one_after_another_in_id_order() {
    let server = teamcity(4, None).await;
    let held = Held::start(server.uri(), per_config).await;
    let run = spawn_sync(
        source(held.base_url(), json!({ "concurrent_configs": 1 })),
        &held,
        None,
    );

    within(
        "the first configuration",
        held.until(|h| h.holding().len() == 1),
    )
    .await;
    release_all(&held, 4, 1).await;

    let keys = within("the run", run).await.expect("joins").expect("syncs");
    assert_eq!(keys, everything(4, true));
    assert_eq!(held.most(), 1);
    assert_never_more_than(&held, 1);
    assert_eq!(held.arrived(), ["Cfg1", "Cfg2", "Cfg3", "Cfg4"]);
}

/// A source scoped to a configuration list asks each configuration for its
/// builds since the watermark (#1626), and those queries overlap too.
#[tokio::test]
async fn a_configuration_lists_incremental_run_asks_up_to_concurrent_configs_at_once() {
    let server = teamcity(5, None).await;
    let held = Held::start(server.uri(), per_config).await;
    let ids: Vec<String> = (1..=5).map(|n| format!("Cfg{n}")).collect();
    // A watermark below every build, which the opening probe's empty page
    // does not reach, so the run first asks the server whether it still has
    // that build.
    Mock::given(method("GET"))
        .and(path("/app/rest/builds/id:50"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "id": 50 })))
        .mount(&server)
        .await;
    let run = spawn_sync(
        source(
            held.base_url(),
            json!({ "concurrent_configs": 2, "build_type_ids": ids }),
        ),
        &held,
        Some(r#"{"v":2,"since_build_id":50}"#.to_owned()),
    );

    within(
        "two configurations in flight",
        held.until(|h| h.holding().len() == 2),
    )
    .await;
    release_all(&held, 5, 2).await;

    let keys = within("the run", run).await.expect("joins").expect("syncs");
    assert_eq!(keys, everything(5, true), "every build, and its configuration");
    assert_eq!(held.most(), 2, "two at once");
}

/// A fatal error in one configuration ends the run with that error, and the
/// queries still in flight are dropped: their held requests are hung up on,
/// rather than left to run on behind a run that already failed.
#[tokio::test]
async fn a_fatal_error_in_one_configuration_ends_the_run_and_every_other_query() {
    let server = teamcity(3, Some("Cfg1")).await;
    let held = Held::start(server.uri(), per_config).await;
    let run = spawn_sync(
        source(held.base_url(), json!({ "concurrent_configs": 3 })),
        &held,
        None,
    );

    within(
        "all three in flight",
        held.until(|h| h.holding().len() == 3),
    )
    .await;
    held.release("Cfg1");

    let error = within("the run", run)
        .await
        .expect("joins")
        .expect_err("a revoked credential ends the run");
    assert!(
        matches!(error, SourceError::Unauthorized { .. }),
        "{error:?}"
    );
    within(
        "the other queries hung up",
        held.until(|h| h.abandoned().len() == 2),
    )
    .await;
    let mut abandoned = held.abandoned();
    abandoned.sort();
    assert_eq!(abandoned, ["Cfg2", "Cfg3"]);
}

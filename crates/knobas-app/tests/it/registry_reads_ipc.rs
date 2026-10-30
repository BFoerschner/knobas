//! Every read takes the registry it is given (#1322, spec #1305).
//!
//! Each read below is asked twice over one database: through its command on
//! a lifecycle carrying the process's registry, and on one carrying
//! [`Probe`], an adapter kind nothing compiled in declares. The same mirror
//! answers differently, and only the registry differs, so what is witnessed
//! is that the command handed its read the lifecycle's registry and the read
//! used it. Before #1322 these reads called the process singleton, and no
//! test could give them an adapter of its own.
//!
//! The write queue's post-write read is not a command: it runs when a run of
//! the source ends, through the events `sources::start` wraps
//! ([`ArrivalLinks`]), with the sources state's registry.
//!
//! Every test has a database of its own: the inbox counts the whole mirror,
//! and a search command writes its registry's declarations to the database
//! (#627), which a shared one would hand to every other test in it.

use std::sync::Arc;

use knobas_app::Lifecycle;
use knobas_app::sources::write_queue::{ArrivalLinks, hold_arrival_link};
use knobas_source::docs::{DocContent, DocProjection};
use knobas_source::{
    KindInfo, KindPaths, ListPath, PayloadPath, SourceDescriptor, StatePath, StateValue, Tone,
};
use knobas_sync::scheduler::{AdapterRegistry, SyncEvents};
use serde_json::{Value, json};
use sqlx::PgPool;
use tauri::Manager;
use tauri::ipc::CallbackFn;

/// The probe's adapter kind.
const PROBE: &str = "probe_reads";
/// The one configured source of it.
const SRC: &str = "probe";
/// The reader, as the source knows them.
const ME: &str = "mara.lindqvist";

/// One adapter kind, declaring what each read below takes from a registry:
/// a ticket's state chip and assignee, its kind's words, a pull request's
/// reviewers and merged flag, and a page projection.
struct Probe;

impl AdapterRegistry for Probe {
    fn descriptors(&self) -> Vec<SourceDescriptor> {
        vec![SourceDescriptor {
            id: PROBE.to_owned(),
            adapter_kind: PROBE.to_owned(),
            name: "Probe".to_owned(),
            capabilities: Vec::new(),
            adapter_version: "0".to_owned(),
            auth_methods: Vec::new(),
            accepts_account: false,
            min_sync_interval_secs: 60,
            roles: Vec::new(),
            source_actions: Vec::new(),
            clone_from: Vec::new(),
            url_forms: Vec::new(),
            code_pages: None,
            write_ops: Vec::new(),
            entity_kinds: vec![KindInfo {
                id: "ticket".to_owned(),
                label: "Probe ticket".to_owned(),
                plural: "Probe tickets".to_owned(),
                monogram: "PT".to_owned(),
                full_sync_exhaustive: true,
                refreshes: true,
            }],
            config_schema: json!({}),
            payload_paths: vec![
                KindPaths {
                    kind: "ticket".to_owned(),
                    state: vec![StatePath {
                        at: PayloadPath::of(["state"]),
                        values: vec![StateValue::of("open", Tone::Info)],
                    }],
                    assignee: vec![PayloadPath::of(["assignee"])],
                    ..KindPaths::default()
                },
                KindPaths {
                    kind: "pr".to_owned(),
                    reviewers: vec![ListPath {
                        at: PayloadPath::of(["requested_reviewers"]),
                        entry: PayloadPath::of(["login"]),
                    }],
                    merged: vec![PayloadPath::of(["merged"])],
                    ..KindPaths::default()
                },
            ],
        }]
    }

    fn doc_projection(&self, kind: &str) -> Option<DocProjection> {
        (kind == PROBE).then_some(project as DocProjection)
    }

    fn build(
        &self,
        _: knobas_source::instance::SourceInstance,
    ) -> Result<Box<dyn knobas_source::Source>, knobas_source::SourceError> {
        Err(knobas_source::SourceError::protocol(
            "the probe builds nothing",
        ))
    }
}

/// The probe's page projection: every page, with one label.
fn project(kind: &str, _payload: &Value) -> Option<DocContent> {
    (kind == "page").then(|| DocContent {
        version: Some(3),
        editor_name: None,
        labels: vec!["probe-label".to_owned()],
        blocks: Vec::new(),
        comments: Vec::new(),
        sections: Vec::new(),
        history: Vec::new(),
    })
}

/// A database of this test's own, with [`SRC`] configured as a [`PROBE`].
async fn database(label: &str) -> PgPool {
    let pool = knobas_db::test_util::scratch_database(label)
        .await
        .pool(4)
        .await
        .expect("a pool onto this test's own database");
    crate::support::source_config(
        &pool,
        SRC,
        PROBE,
        SRC,
        &serde_json::json!({ "username": ME }),
    )
    .await;
    pool
}

/// One live item of [`SRC`]'s, updated a day ago.
async fn item(pool: &PgPool, id: &str, kind: &str, title: &str, payload: Value) {
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, $2, $3)")
        .bind(id)
        .bind(kind)
        .bind(title)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item
             (entity_id, source_id, kind, title, body_text, author, item_updated_at, payload)
         values ($1, $2, $3, $4, '', 'jonas.becker', now() - interval '1 day', $5)",
    )
    .bind(id)
    .bind(SRC)
    .bind(kind)
    .bind(title)
    .bind(payload)
    .execute(pool)
    .await
    .unwrap();
}

/// A lifecycle over `pool`, up, as bring-up leaves it.
fn up(lifecycle: Lifecycle, pool: &PgPool) -> Lifecycle {
    lifecycle.install(knobas_app::AppState::over_pool(pool.clone()));
    lifecycle.set(knobas_app::DbState::Ready);
    lifecycle
}

#[cfg(any(windows, target_os = "android"))]
const LOCAL_ORIGIN: &str = "http://tauri.localhost";
#[cfg(not(any(windows, target_os = "android")))]
const LOCAL_ORIGIN: &str = "tauri://localhost";

/// Invoke `cmd` with `body` on a mock app managing `lifecycle`, through
/// Tauri's own argument resolution, and answer what it returned.
fn invoke_blocking(lifecycle: Lifecycle, cmd: &str, body: Value) -> Result<Value, String> {
    let app = tauri::test::mock_builder()
        .invoke_handler(tauri::generate_handler![
            knobas_app::commands::entity::list_entities,
            knobas_app::commands::entity::get_entity,
            knobas_app::commands::entity::docs_page,
            knobas_app::commands::entity::inbox_count,
            knobas_app::commands::entity::preview_link_set_rule,
        ])
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("mock app");
    app.manage(lifecycle);
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", tauri::WebviewUrl::default())
        .build()
        .expect("mock webview");
    tauri::test::get_ipc_response(
        &webview,
        tauri::webview::InvokeRequest {
            cmd: cmd.to_owned(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: LOCAL_ORIGIN.parse().expect("url"),
            body: body.into(),
            headers: Default::default(),
            invoke_key: tauri::test::INVOKE_KEY.to_string(),
        },
    )
    .map_err(|error| format!("{error:?}"))?
    .deserialize::<Value>()
    .map_err(|error| error.to_string())
}

/// `cmd` asked of the process's registry and of [`Probe`], over `pool`:
/// `(process, probe)`.
async fn both(pool: &PgPool, cmd: &'static str, body: Value) -> (Value, Value) {
    let mut answers = Vec::new();
    for lifecycle in [Lifecycle::new(), Lifecycle::with_registry(Arc::new(Probe))] {
        let lifecycle = up(lifecycle, pool);
        let body = body.clone();
        // Off the test's runtime: the command runs on Tauri's, and this
        // thread waits for it.
        let answer = tokio::task::spawn_blocking(move || invoke_blocking(lifecycle, cmd, body))
            .await
            .unwrap()
            .unwrap_or_else(|error| panic!("{cmd} answers: {error}"));
        answers.push(answer);
    }
    let probe = answers.pop().unwrap();
    (answers.pop().unwrap(), probe)
}

/// The entity list draws a row's state chip from the lifecycle's registry's
/// declarations: the probe's chip, where the process's registry, which does
/// not know the kind, draws none.
#[tokio::test(flavor = "multi_thread")]
async fn the_entity_list_reads_the_lifecycles_declarations() {
    let pool = database("registry_reads_list").await;
    let ticket = format!("{SRC}:PAY-1");
    item(
        &pool,
        &ticket,
        "ticket",
        "Retry payouts",
        json!({"state": "open"}),
    )
    .await;

    let filter = json!({
        "sources": [SRC], "kinds": ["ticket"], "updated_within_days": null,
        "context": null, "project": null, "order": "updated_desc",
        "include_deleted": false, "reach": null, "role": null,
    });
    let (process, probe) = both(
        &pool,
        "list_entities",
        json!({ "filter": filter, "limit": 50, "offset": 0 }),
    )
    .await;
    assert_eq!(probe["rows"][0]["entity_id"], json!(ticket));
    assert_eq!(
        probe["rows"][0]["state"],
        json!({"text": "open", "tone": "info"})
    );
    assert_eq!(process["rows"][0]["entity_id"], json!(ticket));
    assert_eq!(process["rows"][0]["state"], Value::Null);
}

/// The detail's kind info is the lifecycle's registry's words for the kind.
#[tokio::test(flavor = "multi_thread")]
async fn the_detail_reads_its_kind_info_from_the_lifecycles_registry() {
    let pool = database("registry_reads_detail").await;
    let ticket = format!("{SRC}:PAY-2");
    item(
        &pool,
        &ticket,
        "ticket",
        "Retry payouts",
        json!({"state": "open"}),
    )
    .await;

    let (process, probe) = both(&pool, "get_entity", json!({ "entityId": ticket })).await;
    assert_eq!(probe["kind_info"]["label"], json!("Probe ticket"));
    assert_eq!(probe["kind_info"]["monogram"], json!("PT"));
    assert_eq!(process["kind_info"], Value::Null);
}

/// The Docs reader projects a page through the lifecycle's registry.
#[tokio::test(flavor = "multi_thread")]
async fn a_docs_page_reads_through_the_lifecycles_projection() {
    let pool = database("registry_reads_docs").await;
    let page = format!("{SRC}:900");
    item(&pool, &page, "page", "Runbook", json!({})).await;

    let (process, probe) = both(&pool, "docs_page", json!({ "entityId": page })).await;
    assert_eq!(probe["readable"], json!(true));
    assert_eq!(probe["labels"], json!(["probe-label"]));
    assert_eq!(process["readable"], json!(false));
    assert_eq!(process["labels"], json!([]));
}

/// The inbox counts a review request where the lifecycle's registry
/// declares where a pull request keeps its reviewers.
#[tokio::test(flavor = "multi_thread")]
async fn the_inbox_reads_the_lifecycles_declared_paths() {
    let pool = database("registry_reads_inbox").await;
    item(
        &pool,
        &format!("{SRC}:acme/payouts#7"),
        "pr",
        "Back off",
        json!({"state": "open", "requested_reviewers": [{"login": ME}]}),
    )
    .await;

    let (process, probe) = both(&pool, "inbox_count", json!({})).await;
    assert_eq!(probe, json!(1), "the review request");
    assert_eq!(process, json!(0));
}

/// A link set's rule marks the reader's own tickets by the assignee the
/// lifecycle's registry declares.
#[tokio::test(flavor = "multi_thread")]
async fn a_link_set_rule_reads_the_lifecycles_declarations() {
    let pool = database("registry_reads_link_set").await;
    let ticket = format!("{SRC}:PAY-3");
    item(
        &pool,
        &ticket,
        "ticket",
        "Retry payouts",
        json!({"state": "open", "assignee": ME}),
    )
    .await;

    let rule = json!({ "rule": { "kind": "manual", "value": ticket } });
    let (process, probe) = both(&pool, "preview_link_set_rule", rule).await;
    assert_eq!(
        probe,
        json!([{"entity_id": ticket, "title": "Retry payouts", "mine": true}])
    );
    assert_eq!(
        process,
        json!([{"entity_id": ticket, "title": "Retry payouts", "mine": false}])
    );
}

/// Passes nothing on: the wrapped events are not under test here.
struct Quiet;

impl SyncEvents for Quiet {
    fn sync_state(&self, _status: knobas_sync::SourceSyncStatus) {}
    fn source_health(&self, _health: knobas_sync::CredentialHealth) {}
    fn activity_new(&self, _row: knobas_core::activity::ActivityRow) {}
}

/// The entities `ticket` reaches by a confirmed link.
async fn reached(pool: &PgPool, ticket: &str) -> Vec<String> {
    let mut ids: Vec<String> = knobas_core::reach::reach(pool, ticket)
        .await
        .unwrap()
        .into_iter()
        .map(|r| r.entity_id)
        .collect();
    ids.sort();
    ids
}

/// A sent write to `repo`, owing a link to `ticket`.
async fn sent(pool: &PgPool, repo: &str, op: &str, payload: Value, ticket: &str) {
    let id = knobas_core::write_queue::queue(
        pool,
        SRC,
        &knobas_core::entity::EntityRef::parse(repo).unwrap(),
        op,
        payload,
        json!({}),
    )
    .await
    .unwrap()
    .id;
    hold_arrival_link(pool, id, ticket).await.unwrap();
    knobas_core::write_queue::sent(pool, id, None)
        .await
        .unwrap();
}

/// The end of a run resolves each link `registry` reads as owed, and
/// answers once the branch, queued last, is linked: the pass that links it
/// has looked at the pull request already.
async fn run_ends(pool: &PgPool, registry: Arc<dyn AdapterRegistry>, ticket: &str, until: &str) {
    let events = ArrivalLinks::new(
        Quiet,
        pool.clone(),
        registry,
        knobas_sync::background::Background::new(),
    );
    let started = knobas_sync::SourceSyncStatus::started(SRC, 1);
    events.sync_state(knobas_sync::SourceSyncStatus {
        running: false,
        ..started
    });
    tokio::time::timeout(std::time::Duration::from_secs(20), async {
        while !reached(pool, ticket).await.iter().any(|id| id == until) {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("the run's end links {until}"));
}

/// The write queue's post-write read takes the registry it is given: a pull
/// request open from the write's head into its base, which the probe declares
/// merged, is not the write's and is not linked; the process's registry, which
/// declares no merged flag for the kind, links it.
#[tokio::test(flavor = "multi_thread")]
async fn the_post_write_read_takes_the_registry_it_is_given() {
    let pool = database("registry_reads_write_queue").await;
    let ticket = format!("{SRC}:PAY-4");
    let repo = format!("{SRC}:acme/payouts");
    let pr = format!("{SRC}:acme/payouts#150");
    let branch = format!("{SRC}:acme/payouts@refs/heads/feature/PAY-4");
    item(&pool, &ticket, "ticket", "Retry payouts", json!({})).await;
    item(&pool, &repo, "repo", "acme/payouts", json!({})).await;

    sent(
        &pool,
        &repo,
        "create_pull_request",
        json!({ "CreatePullRequest": {
            "entity": repo, "title": "PAY-4", "body": "",
            "head": "feature/PAY-4-backoff", "base": "main"
        }}),
        &ticket,
    )
    .await;
    sent(
        &pool,
        &repo,
        "create_branch",
        json!({ "CreateBranch": {
            "entity": repo, "name": "feature/PAY-4", "from_ref": "main"
        }}),
        &ticket,
    )
    .await;
    item(
        &pool,
        &pr,
        "pr",
        "PAY-4",
        json!({
            "state": "open", "merged": true,
            "head": {"ref": "feature/PAY-4-backoff"}, "base": {"ref": "main"}
        }),
    )
    .await;
    item(&pool, &branch, "branch", "feature/PAY-4", json!({})).await;

    run_ends(&pool, Arc::new(Probe), &ticket, &branch).await;
    assert_eq!(
        reached(&pool, &ticket).await,
        vec![branch.clone()],
        "the probe declares the pull request merged"
    );

    run_ends(
        &pool,
        Arc::new(knobas_app::sources::Registry::for_process()),
        &ticket,
        &pr,
    )
    .await;
    let mut both = vec![branch, pr];
    both.sort();
    assert_eq!(reached(&pool, &ticket).await, both);
}

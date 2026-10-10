//! **The seam: a real adapter, through the sync engine, into a real database.**
//!
//! Issue #93. Every other test in this workspace stops one side short of this
//! join, and the two halves it leaves are each honest on their own:
//!
//! * `knobas-sync/tests/it/backfill.rs` runs the engine against a real embedded
//!   PostgreSQL and asserts on stored rows -- with a *stand-in* adapter whose
//!   payload is `{"key": …, "payload_version": 1}`. It proves the engine's
//!   backfill semantics and nothing about any adapter's record.
//! * `knobas-source-jira/tests/it/mockd.rs` drives the real adapter over real
//!   HTTP against `knobas-mockd` and asserts on the parsed [`SyncItem`] --
//!   with no database anywhere. It proves the query and the parsing and
//!   nothing about what is stored.
//!
//! Both can be green while the join between them is broken. A mapping bug
//! between "the adapter parsed eighteen fields" and "eighteen fields are in
//! the `sync.item.payload` column" is invisible to each of them, and that
//! column is what #32's acceptance criterion -- M2 exit criterion 6, *an
//! untouched issue carries the widened payload after the re-sync* -- is
//! actually about. So every assertion in this file is on a **stored row**,
//! never on a parsed struct.
//!
//! # Where this lives, and why it runs in `just check`
//!
//! Decided by Fable under delegation on 2026-08-29 (issue #93's ruling), and
//! written down here because this is a new kind of test for the repo and this
//! file is where the next reader of it looks.
//!
//! **Home: `crates/knobas-app/tests/`.** It needs an adapter *and* the
//! database, and this is the one crate that already has both: `knobas-app`
//! depends on all four adapter crates as plain `[dependencies]` (its
//! `sources/registry.rs` is "the only place in knobas that names them"), and
//! this directory already runs an embedded PostgreSQL for `ipc`, `entity`,
//! `sources_crud`, `backup` and `demo`. So no new crate and no new production
//! dependency edge -- only `knobas-mockd` joins the dev-dependencies. It also
//! puts the test where the joining code is: the run below goes through
//! [`Registry`], which is this crate's own "config + secret ⇒ `Box<dyn
//! Source>`" landing site and the seam under test.
//!
//! **`just check`: yes, and not `#[ignore]`d.** mockd is in-process and needs
//! no Docker, and an embedded PostgreSQL per test binary is already the norm
//! here. The `#[ignore]` treatment belongs to the suites that need a network
//! or a container (the live-Gitea one); this needs neither, and an ignored
//! seam test is a seam test nobody runs -- which is exactly how a gap between
//! two green halves stays open. The cost is one PostgreSQL and one mockd for
//! this binary, paid once and shared by every test in it.
//!
//! **Jira first, TeamCity as one more test.** Jira is the adapter whose
//! acceptance criterion exposed the gap. TeamCity joined with #232, for the
//! join that ticket's guard depends on (the last test in this file), as one
//! more test rather than a framework. Gitea is deliberately *not* here: mockd
//! serves no Gitea by standing decision, and Gitea's equivalent join belongs
//! to the docker-gated container layer.

use async_trait::async_trait;
use knobas_app::assets::ESTATE_FILE_PRODUCER;
use knobas_app::sources::Registry;
use knobas_mockd::{spawn_mock_jira, spawn_mock_teamcity};
use knobas_source::contract::VecSink;
use knobas_source::instance::SourceInstance;
use knobas_source::{
    AuthMethod, ConnectionInfo, Cursor, Sink, Source, SourceDescriptor, SourceError, SyncItem,
    WriteOp,
};
use knobas_sync::scheduler::AdapterRegistry;
use serde_json::Value;
use sqlx::PgPool;

/// The six names issue #32 added to `knobas-source-jira`'s `BASE_FIELDS`.
///
/// Spelled out here rather than imported: the constant is private to the
/// adapter, and a test that read it would agree with the code by construction.
/// These are the names the mirror was missing on every issue knobas had ever
/// synced, which is the defect the backfill exists to repair.
const WIDENED_BY_32: [&str; 6] = [
    "labels",
    "parent",
    "resolution",
    "issuelinks",
    "timeoriginalestimate",
    "timespent",
];

/// The issue nobody touches. Its stored payload is the whole point: an
/// incremental run will never bring it back, so whatever the first run wrote
/// is what the mirror holds until a backfill re-reads it.
const UNTOUCHED: &str = "PAY-231";

/// The one issue that *is* edited upstream between the runs -- the control
/// that shows the scheduled path still works and still cannot repair the rest.
const TOUCHED: &str = "PAY-228";

/// The fixture's seven Tidewater issues.
///
/// Named for the fixture rather than `CORPUS`, because the glossary reserves
/// that word: "a count of the mirror is a corpus, never a run's Upserted"
/// (`CONTEXT.md`), and this number is compared against both.
const FIXTURE_ISSUES: usize = 7;

// -- the mirror as the pre-#32 adapter left it -------------------------------

/// A sink that removes [`WIDENED_BY_32`] from every item on its way to the
/// engine, reproducing the record the adapter emitted **before** #32 widened
/// its `fields=` list from twelve names to eighteen.
///
/// This is scaffolding for the *starting state* only. It is the honest way to
/// get a narrow mirror without a second copy of the adapter: the fetch, the
/// parse and the write are all real, and only the six names are taken back
/// out. Nothing this file asserts is about what the narrowed run stored except
/// that it is narrow.
struct NarrowSink<'s> {
    inner: &'s mut (dyn Sink + Send),
}

#[async_trait]
impl Sink for NarrowSink<'_> {
    async fn item(&mut self, mut item: SyncItem) -> Result<(), SourceError> {
        if let Some(fields) = item
            .payload
            .get_mut("fields")
            .and_then(Value::as_object_mut)
        {
            for name in WIDENED_BY_32 {
                fields.remove(name);
            }
        }
        self.inner.item(item).await
    }
}

/// The real Jira adapter with its pre-#32 payload. Everything else -- the
/// descriptor, the cursor, the HTTP -- is the adapter's own.
struct PreWidening(Box<dyn Source>);

#[async_trait]
impl Source for PreWidening {
    fn descriptor(&self) -> SourceDescriptor {
        self.0.descriptor()
    }
    async fn test_connection(&self) -> Result<ConnectionInfo, SourceError> {
        self.0.test_connection().await
    }
    async fn sync(
        &self,
        cursor: Option<Cursor>,
        sink: &mut (dyn Sink + Send),
    ) -> Result<Cursor, SourceError> {
        let mut narrowed = NarrowSink { inner: sink };
        self.0.sync(cursor, &mut narrowed).await
    }
    async fn write(&self, op: WriteOp) -> Result<knobas_source::WriteReceipt, SourceError> {
        self.0.write(op).await
    }

    /// No workflow: this fake declares no `transition` write op, which is the
    /// contract battery's rule for when the read is refused (#498).
    async fn reachable_transitions(
        &self,
        entity: &str,
    ) -> Result<Vec<String>, knobas_source::SourceError> {
        Err(knobas_source::SourceError::protocol(format!(
            "this fake has no workflow, so there are no reachable transitions for {entity:?}"
        )))
    }
}

// -- harness -----------------------------------------------------------------

async fn pool() -> PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    pool
}

/// A connection of this run's own -- interfaces §10.6(c), which
/// [`knobas_sync::run_from_stored_cursor`] and [`knobas_sync::run_backfill`]
/// both require.
async fn dedicated() -> sqlx::PgConnection {
    knobas_db::test_util::test_connector()
        .await
        .connect()
        .await
        .expect("a connection outside every pool")
}

/// A per-test instance id. The embedded server is shared across the binary, so
/// a hardcoded namespace would make two tests each other's fixture.
fn unique_id() -> String {
    format!("seam-{}", uuid::Uuid::new_v4().simple())
}

/// The row the sources view would have written, so the engine has somewhere to
/// store the position a run comes back with. `kind` is the registry's word
/// for the adapter (`jira`, `teamcity`).
async fn configure(pool: &PgPool, id: &str, kind: &str, base_url: &str) {
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
         values ($1, $2, $3, $4, 'pat')",
    )
    .bind(id)
    .bind(kind)
    .bind(format!("Tidewater {kind}"))
    .bind(base_url)
    .execute(pool)
    .await
    .unwrap();
}

/// Build the **real** Jira adapter the way the scheduler does: through
/// `knobas-app`'s registry, from a configured instance and a credential.
fn adapter(id: &str, base_url: &str) -> Box<dyn Source> {
    configured(id, base_url, serde_json::json!({}))
}

/// The same, with an instance `config` of the caller's choosing: the shape
/// `source_config.config` holds -- the column the Add-source dialog writes and
/// the scheduler reads -- handed to the registry to parse into `JiraConfig`.
/// The **parse** is the part of an operator's path this covers, and a test that
/// hand-built a `JiraConfig` would skip it. The column *read* is not in this
/// path; said plainly because this file's whole premise is not letting a reader
/// assume the half that is missing.
fn configured(id: &str, base_url: &str, config: Value) -> Box<dyn Source> {
    let instance = SourceInstance {
        id: id.to_owned(),
        kind: "jira".to_owned(),
        display_name: "Tidewater Jira".to_owned(),
        base_url: base_url.to_owned(),
        auth: Some(AuthMethod::Pat),
        secret: Some(knobas_mockd::JIRA_TOKEN.to_owned()),
        account: None,
        config,
    };
    match Registry::builtin().build(instance) {
        Ok(source) => source,
        // `Box<dyn Source>` is not `Debug`, so `expect` is unavailable.
        Err(e) => panic!("the registry must build a jira instance: {e:?}"),
    }
}

/// The **real** TeamCity adapter, the same way: through the registry, with
/// the default instance `config` -- every project and every configuration
/// mockd serves.
fn teamcity_adapter(id: &str, base_url: &str) -> Box<dyn Source> {
    let instance = SourceInstance {
        id: id.to_owned(),
        kind: "teamcity".to_owned(),
        display_name: "Tidewater CI".to_owned(),
        base_url: base_url.to_owned(),
        auth: Some(AuthMethod::Pat),
        secret: Some(knobas_mockd::TEAMCITY_TOKEN.to_owned()),
        account: None,
        config: serde_json::json!({}),
    };
    match Registry::builtin().build(instance) {
        Ok(source) => source,
        Err(e) => panic!("the registry must build a teamcity instance: {e:?}"),
    }
}

/// One mirrored row, every column the adapter decides.
#[derive(Debug, sqlx::FromRow, PartialEq)]
struct StoredItem {
    kind: String,
    title: String,
    body_text: String,
    author: Option<String>,
    item_updated_at: Option<chrono::DateTime<chrono::Utc>>,
    payload: Value,
    web_url: Option<String>,
}

async fn stored(pool: &PgPool, source_id: &str, key: &str) -> StoredItem {
    sqlx::query_as(
        "select kind, title, body_text, author, item_updated_at, payload, web_url
           from sync.item where entity_id = $1",
    )
    .bind(format!("{source_id}:{key}"))
    .fetch_one(pool)
    .await
    .unwrap_or_else(|e| panic!("{source_id}:{key} should be in the mirror: {e}"))
}

/// Every name under `payload.fields`, sorted -- the comparable summary of how
/// wide a record is.
fn field_names(payload: &Value) -> Vec<String> {
    let mut names: Vec<String> = payload["fields"]
        .as_object()
        .map(|f| f.keys().cloned().collect())
        .unwrap_or_default();
    names.sort();
    names
}

/// Which of the six #32 names the stored `payload` actually carries.
///
/// Presence, not truthiness: `resolution` is `null` on an unresolved issue and
/// that null **is** the widened record. A check that treated it as missing
/// would report the mirror as narrow for every open ticket.
fn widened_names(payload: &Value) -> Vec<&'static str> {
    WIDENED_BY_32
        .into_iter()
        .filter(|name| payload["fields"].get(name).is_some())
        .collect()
}

// -- the test ----------------------------------------------------------------

/// **M2 exit criterion 6, end to end.** A real Jira adapter, over real HTTP
/// against mockd, through `knobas-sync`, into `sync.item.payload`.
#[tokio::test]
async fn a_backfill_widens_the_stored_payload_of_an_issue_nobody_touched() {
    let jira = spawn_mock_jira().await;
    let pool = pool().await;
    let id = unique_id();
    configure(&pool, &id, "jira", &jira.base_url()).await;
    let real = adapter(&id, &jira.base_url());
    let mut conn = dedicated().await;

    // 1. The mirror as the pre-#32 adapter left it: real issues, narrow records.
    let filled = knobas_sync::run_from_stored_cursor(
        &mut conn,
        &pool,
        &PreWidening(adapter(&id, &jira.base_url())),
    )
    .await
    .unwrap();
    assert_eq!(filled.upserted as usize, FIXTURE_ISSUES);
    let narrow = stored(&pool, &id, UNTOUCHED).await.payload;
    assert_eq!(
        widened_names(&narrow),
        Vec::<&str>::new(),
        "the starting state must be a narrow mirror, or nothing below is a widening"
    );
    assert!(
        !field_names(&narrow).is_empty(),
        "and it is still a real Jira record: an absent `fields` object would satisfy the \
         assertion above without anything having been narrowed"
    );

    // 2. The query widens and one issue is edited upstream. The scheduled run
    //    brings back that issue and nothing else.
    jira.touch_issue(TOUCHED);
    let incremental = knobas_sync::run_from_stored_cursor(&mut conn, &pool, real.as_ref())
        .await
        .unwrap();
    assert_eq!(
        incremental.upserted, 1,
        "only the touched issue changed upstream"
    );
    assert_eq!(
        widened_names(&stored(&pool, &id, TOUCHED).await.payload),
        WIDENED_BY_32.to_vec(),
        "the touched issue was re-read, so it carries the widened record"
    );
    assert_eq!(
        widened_names(&stored(&pool, &id, UNTOUCHED).await.payload),
        Vec::<&str>::new(),
        "an untouched issue keeps the narrow payload -- this is the defect #32 exists to fix, \
         and waiting does not repair it"
    );

    // 3. The backfill: every issue re-read, none swept.
    let backfill = knobas_sync::run_backfill(&mut conn, &pool, real.as_ref())
        .await
        .unwrap();
    assert_eq!(
        backfill.upserted as usize, FIXTURE_ISSUES,
        "the whole corpus is re-fetched"
    );
    assert_eq!(
        backfill.swept, 0,
        "a backfill re-fetches payloads; it does not reconcile deletions"
    );

    // The stored row, field by field, for the issue nobody touched.
    let row = stored(&pool, &id, UNTOUCHED).await;
    assert_eq!(
        widened_names(&row.payload),
        WIDENED_BY_32.to_vec(),
        "#32's criterion, on the column rather than on a parsed struct"
    );
    // Epic membership -- the thing the Contexts work reads out of `payload`.
    assert_eq!(row.payload["fields"]["parent"]["key"], "PAY-200");
    assert_eq!(
        row.payload["fields"]["parent"]["fields"]["issuetype"]["name"],
        "Epic"
    );
    assert!(row.payload["fields"]["issuelinks"].is_array());
    assert!(row.payload["fields"]["labels"].is_array());
    assert!(
        row.payload["fields"]["resolution"].is_null(),
        "{UNTOUCHED} is open, and the null is stored rather than the key being absent"
    );
    assert_eq!(row.payload["fields"]["timeoriginalestimate"], 57_600);
    assert_eq!(row.payload["fields"]["timespent"], 16_200);
    // A resolved issue, so the assertion above is about this issue's state and
    // not about a column that is null for everything.
    assert_eq!(
        stored(&pool, &id, "PAY-219").await.payload["fields"]["resolution"]["name"],
        "Done"
    );

    // **The load-bearing property.** Everything the adapter parsed is in the
    // row, for every issue: the reference is a fresh sync of the *same* real
    // adapter into a `VecSink`, which is the other half's assertion target.
    // Comparing the whole row against it is what fails when the path from
    // `SyncItem` to the column drops anything -- a payload key, a title, the
    // author, the URL -- rather than only when a name somebody thought to list
    // here goes missing.
    let mut parsed = VecSink(Vec::new());
    real.sync(None, &mut parsed).await.unwrap();
    assert_eq!(parsed.0.len(), FIXTURE_ISSUES);
    for item in &parsed.0 {
        let row = stored(&pool, &id, &item.entity.key).await;
        // The key sets first, and separately: a whole-row `assert_eq!` over two
        // Jira issues prints two screens of JSON and leaves the reader to spot
        // the one name that differs. This says which names went missing.
        assert_eq!(
            field_names(&row.payload),
            field_names(&item.payload),
            "{}: the `fields` the mirror holds are not the `fields` the adapter parsed",
            item.entity
        );
        assert_eq!(
            row,
            StoredItem {
                kind: item.kind.clone(),
                title: item.title.clone(),
                body_text: item.body_text.clone(),
                author: item.author.clone(),
                item_updated_at: item.updated_at,
                payload: item.payload.clone(),
                web_url: item.web_url.clone(),
            },
            "{} reached the mirror thinner than the adapter parsed it",
            item.entity
        );
    }

    // Every request this test made is one the vendored WADL declares, so the
    // widened `fields=` is a query a real Jira DC would accept.
    jira.assert_no_violations();
}

/// **The classic Data Center epic path, end to end (issue #125).**
///
/// `JiraConfig::epic_link_field` is the *only* way knobas can read epic
/// membership out of a classic DC project — `fields.parent` is the next-gen
/// spelling and a classic instance leaves it empty — and it is therefore the
/// setting most likely to be in use against the self-hosted Jira this app is
/// aimed at. Until #125 it was proven in halves that never met, exactly as #93
/// found for the widened payload: `sync::tests` asserted the id reaches the
/// `fields=` query string, and nothing anywhere ran that query. It could not
/// be run — mockd served no `customfield_*`, so the round trip was a 400.
///
/// So this asserts on the **stored row**. Between the option and that column
/// sit the config parse, the registry, the query, the response, the raw
/// payload and the engine's write; a break in any of them is a mirror with no
/// epic membership on a classic instance, and it would look exactly like a
/// working sync.
#[tokio::test]
async fn a_classic_projects_epic_link_reaches_the_stored_payload() {
    let field = knobas_mockd::jira::EPIC_LINK_FIELD;
    let jira = spawn_mock_jira().await;
    let pool = pool().await;
    let mut conn = dedicated().await;

    // A source configured the way an operator with a classic DC project
    // configures one: the instance's Epic Link field id, through
    // `source_config.config`.
    let classic = unique_id();
    configure(&pool, &classic, "jira", &jira.base_url()).await;
    let run = knobas_sync::run_from_stored_cursor(
        &mut conn,
        &pool,
        configured(
            &classic,
            &jira.base_url(),
            serde_json::json!({ "epic_link_field": field }),
        )
        .as_ref(),
    )
    .await
    .unwrap();
    assert_eq!(run.upserted as usize, FIXTURE_ISSUES);

    let row = stored(&pool, &classic, UNTOUCHED).await;
    assert_eq!(
        row.payload["fields"][field], "PAY-200",
        "{UNTOUCHED}'s epic membership must be in the column, not only in the query string"
    );
    assert_eq!(
        row.payload["fields"]["parent"]["key"], "PAY-200",
        "and the two spellings agree, which is what makes one a fallback for the other"
    );
    // The null branch is stored too, so a reader can tell "this issue has no
    // epic" from "this mirror was synced without the option".
    assert_eq!(
        stored(&pool, &classic, "OPS-77").await.payload["fields"].get(field),
        Some(&Value::Null),
        "OPS-77 belongs to no epic, and that answer is part of the record"
    );

    // The control, and the reason the assertions above are not vacuous: the
    // same adapter against the same mock, with the option left unset, stores
    // no such key at all. Without this, a mock that leaked the field into every
    // projection would satisfy the test while the option did nothing -- which
    // is the failure mode #125 is a repeat of.
    let default = unique_id();
    configure(&pool, &default, "jira", &jira.base_url()).await;
    knobas_sync::run_from_stored_cursor(
        &mut conn,
        &pool,
        adapter(&default, &jira.base_url()).as_ref(),
    )
    .await
    .unwrap();
    assert_eq!(
        stored(&pool, &default, UNTOUCHED).await.payload["fields"].get(field),
        None,
        "unconfigured, the field is never asked for and never stored -- so the option is what \
         put it in the row above"
    );

    // Both queries are ones the vendored WADL declares: the configured
    // `fields=` is not a 400 and not an `UnknownField` violation, which is the
    // half that could not even be attempted before.
    jira.assert_no_violations();
}

/// **The wire between the adapter's kind name and the census's guard (#232).**
///
/// `knobas_core::project_key_read!`'s third arm reads a TeamCity build
/// configuration's top-level `projectId` only where `i.kind = 'build_config'`
/// -- a literal in `knobas-core`, which cannot import the adapter's
/// `KIND_BUILD_CONFIG`. `knobas-core/tests/it/projects.rs` pins that arm with
/// hand-written rows that spell the kind the same way, so those tests and the
/// macro agree by construction; the adapter's own tests pin the kind on a
/// parsed `SyncItem` and reach no database. Between them the literal and the
/// constant could drift apart with every test green. So this is the join: the
/// real adapter, over mockd, through the engine, into the mirror, and then the
/// census -- with every *build* tombstoned first, so the configuration rows
/// are the only ones left to spell the project. Rename the kind on either
/// side, or move where the adapter stores the project, and the census below
/// loses the source.
///
/// The reference is the census *before* the tombstoning, which the builds'
/// `buildType.projectId` arm carries; the two arms spell one fact, and
/// `a_build_and_its_configuration_are_one_project` says so on hand-written
/// rows. Nothing here rederives how mockd names a project from a fixture
/// configuration id.
#[tokio::test]
async fn a_teamcity_project_survives_its_builds_through_its_configurations() {
    let teamcity = spawn_mock_teamcity().await;
    let pool = pool().await;
    let id = unique_id();
    configure(&pool, &id, "teamcity", &teamcity.base_url()).await;
    let real = teamcity_adapter(&id, &teamcity.base_url());
    let mut conn = dedicated().await;

    let run = knobas_sync::run_from_stored_cursor(&mut conn, &pool, real.as_ref())
        .await
        .unwrap();
    assert!(run.upserted > 0, "the fixture reached the mirror");

    // The kinds the adapter emits, from the adapter rather than from this
    // test's memory of it: the tombstoning below names one of them, and a
    // renamed kind must fail here by name instead of leaving the builds live
    // and the assertion at the end vacuous.
    let mut parsed = VecSink(Vec::new());
    real.sync(None, &mut parsed).await.unwrap();
    let kinds: std::collections::BTreeSet<&str> =
        parsed.0.iter().map(|item| item.kind.as_str()).collect();
    assert_eq!(
        kinds,
        std::collections::BTreeSet::from(["build", "build_config"]),
        "the adapter's two kinds -- a rename must reach the census's guard too"
    );

    let census = || async {
        let declarations = knobas_app::sources::paths::declared_paths(
            &pool,
            &knobas_app::sources::Registry::builtin(),
        )
        .await
        .expect("what the configured sources declare");
        knobas_core::project::list(&pool, &declarations)
            .await
            .unwrap()
            .into_iter()
            .filter(|project| project.source_id == id)
            .map(|project| project.key)
            .collect::<std::collections::BTreeSet<String>>()
    };
    let through_builds = census().await;
    assert!(
        !through_builds.is_empty(),
        "the fixture's builds name their projects, so the census has something to lose"
    );

    // Every build of this source is tombstoned; the configurations stay, and
    // now they are the only live rows that spell a project for it.
    sqlx::query(
        "update knobas.entity e set deleted_at = now()
           from sync.item i
          where i.entity_id = e.id and i.source_id = $1 and i.kind = 'build'",
    )
    .bind(&id)
    .execute(&pool)
    .await
    .unwrap();
    let live: Vec<String> =
        sqlx::query_scalar("select distinct kind from sync.live_item where source_id = $1")
            .bind(&id)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(
        live,
        vec!["build_config"],
        "only the configurations are left to vouch for a project"
    );

    assert_eq!(
        census().await,
        through_builds,
        "a project whose builds are gone is still reported, through its configurations: the \
         kind the adapter stores is the kind the census's third arm reads"
    );
    teamcity.assert_no_violations();
}

// -- Uptime Kuma (#442) ------------------------------------------------------

/// The recording `knobas-source-kuma`'s own contract suite serves, read from
/// **its** file rather than transcribed into this one.
///
/// One recording with two readers cannot disagree about what the server said;
/// two transcriptions of it can, and the way that goes wrong is invisible --
/// each suite is green about its own copy. It is `/metrics` as the pinned image
/// (2.5.3) answered on 2026-09-06, and `crates/knobas-source-kuma/tests/live_kuma.rs`
/// is what keeps it current.
const KUMA_METRICS: &str = include_str!("../../knobas-source-kuma/tests/it/support/metrics.txt");

/// The API key the fake below accepts, and the `Authorization` it arrives as:
/// HTTP Basic with an empty username, which is how Kuma authenticates
/// `/metrics`.
const KUMA_KEY: &str = "uk1_recorded-for-the-contract-battery";
const KUMA_AUTHORIZATION: &str = "Basic OnVrMV9yZWNvcmRlZC1mb3ItdGhlLWNvbnRyYWN0LWJhdHRlcnk=";

/// A Kuma answering the recording, in process.
async fn spawn_mock_kuma() -> wiremock::MockServer {
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, ResponseTemplate};

    let server = wiremock::MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/metrics"))
        .and(header("authorization", KUMA_AUTHORIZATION))
        .respond_with(ResponseTemplate::new(200).set_body_string(KUMA_METRICS))
        .mount(&server)
        .await;
    server
}

/// **A monitor, from `/metrics` to the timeseries** (issue #443).
///
/// The other join this recording can answer, and the one no seam answers
/// alone: `knobas-sync`'s own battery drives a *fake* source whose payload
/// this file's author chose, so it cannot say that the engine's declared
/// `status_name` read resolves against the shape the real adapter actually
/// writes. A `payload_paths` entry pointing one key wide would leave every
/// sample stateless, and every test in both crates green.
///
/// Two runs, and the second one is the point. The recording never changes, so
/// the real cursor -- a digest of the last corpus (contract §4.2 E) -- makes
/// the second run emit **nothing at all**, which is precisely the run a
/// timeseries built from emitted items would have no row for. Under a
/// threshold moved between the runs, it is also where *warn* is witnessed on
/// numbers Uptime Kuma really published rather than on a literal.
///
/// A database of its own: the threshold is one `knobas.setting` row for the
/// whole profile, so a test that moved it on the shared database would be
/// every other test's fixture.
#[tokio::test]
async fn a_kuma_poll_leaves_one_sample_per_monitor_and_derives_warn_from_the_threshold() {
    let kuma = spawn_mock_kuma().await;
    let pool = knobas_db::test_util::scratch_database("kuma-samples")
        .await
        .pool(2)
        .await
        .expect("a pool on the scratch database");
    let id = unique_id();

    let instance = SourceInstance {
        id: id.clone(),
        kind: knobas_source_kuma::ADAPTER_KIND.to_owned(),
        display_name: "Uptime Kuma".to_owned(),
        base_url: kuma.uri(),
        auth: Some(AuthMethod::ApiToken),
        secret: Some(KUMA_KEY.to_owned()),
        account: None,
        config: serde_json::json!({}),
    };
    let real = Registry::builtin()
        .build(instance)
        .expect("the registry must build a kuma instance");

    let first = knobas_sync::run_once(&pool, real.as_ref(), None)
        .await
        .unwrap();
    assert_eq!(first.upserted, 8, "the recording holds eight monitors");

    // The recording's own numbers: five monitors up with readings of 16 to
    // 32.8 ms, and the three tunnel checks down with Kuma's `-1` sentinel,
    // which the adapter carries as an absence and the sample keeps as one.
    let after_first = samples_of(&pool, &id).await;
    assert_eq!(
        after_first.len(),
        8,
        "one sample per monitor: {after_first:?}"
    );
    assert_eq!(
        after_first.get("8").unwrap(),
        &(Some("up".to_owned()), Some(27)),
        "the canary, at the ratified 1500 ms threshold"
    );
    assert_eq!(
        after_first.get("5").unwrap(),
        &(Some("down".to_owned()), None),
        "a check that did not answer has a state and no reading"
    );

    // Move the threshold under four of the five readings, and poll again. The
    // corpus is byte-identical, so the adapter emits nothing and hands its
    // cursor straight back -- and every live monitor is sampled all the same.
    knobas_sync::samples::set_threshold_ms(&pool, 20)
        .await
        .unwrap();
    let second = knobas_sync::run_once(&pool, real.as_ref(), Some(first.cursor.clone()))
        .await
        .unwrap();
    assert_eq!(second.upserted, 0, "an unchanged Kuma emits nothing");
    assert_eq!(second.cursor, first.cursor, "and hands its cursor back");

    let newest = samples_of(&pool, &id).await;
    assert_eq!(
        newest.get("7").unwrap(),
        &(Some("up".to_owned()), Some(16)),
        "gitea answered in 16 ms and is still up under a 20 ms threshold"
    );
    assert_eq!(
        newest.get("8").unwrap(),
        &(Some("warn".to_owned()), Some(27)),
        "the canary answered in 27 ms and is now warn"
    );
    assert_eq!(
        newest.get("1").unwrap(),
        &(Some("warn".to_owned()), Some(33)),
        "32.8 ms rounds to 33, and the threshold reads the rounded number"
    );
    assert_eq!(
        newest.get("5").unwrap(),
        &(Some("down".to_owned()), None),
        "a monitor that is down is not softened to warn by a threshold"
    );

    let (rows,): (i64,) = sqlx::query_as(
        "select count(*) from knobas.monitor_sample s
           join sync.item i on i.entity_id = s.entity_id where i.source_id = $1",
    )
    .bind(&id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(rows, 16, "two polls, eight monitors, sixteen rows");
}

/// The newest sample of each of one source's monitors, by the monitor's key in
/// Kuma (`8` is the canary).
async fn samples_of(
    pool: &PgPool,
    source_id: &str,
) -> std::collections::HashMap<String, (Option<String>, Option<i32>)> {
    let rows: Vec<(String, Option<String>, Option<i32>)> = sqlx::query_as(
        "select distinct on (s.entity_id) s.entity_id, s.state, s.response_time_ms
           from knobas.monitor_sample s
           join sync.item i on i.entity_id = s.entity_id
          where i.source_id = $1
          order by s.entity_id, s.taken_at desc, s.id desc",
    )
    .bind(source_id)
    .fetch_all(pool)
    .await
    .unwrap();
    rows.into_iter()
        .map(|(entity_id, state, ms)| {
            let key = entity_id
                .split_once(':')
                .expect("an entity id is namespace:key")
                .1
                .to_owned();
            (key, (state, ms))
        })
        .collect()
}

/// **A monitor, from `/metrics` to the launcher's own read.**
///
/// The join this file exists for, on the one criterion of issue #442 that no
/// seam can answer alone: *"monitors are found in the launcher by name with
/// their state"*. `knobas-source-kuma`'s own tests pin what the adapter puts
/// **into** a `SyncItem`; `knobas-app`'s `search_ipc.rs` pins what the launcher
/// gets **out of** a row. Both are green while the join between them is broken,
/// and the join is the whole of the criterion.
///
/// So: the real adapter, over real HTTP against the recording, through
/// `knobas-sync` and the registry, into `sync.item` -- and then out again
/// through `search_inner`, which is the function the launcher calls.
#[tokio::test]
async fn a_kuma_monitor_reaches_the_mirror_and_the_launcher_finds_it_by_name() {
    let kuma = spawn_mock_kuma().await;
    let pool = pool().await;
    let id = unique_id();
    configure(&pool, &id, "kuma", &kuma.uri()).await;

    let instance = SourceInstance {
        id: id.clone(),
        kind: knobas_source_kuma::ADAPTER_KIND.to_owned(),
        display_name: "Uptime Kuma".to_owned(),
        base_url: kuma.uri(),
        auth: Some(AuthMethod::ApiToken),
        secret: Some(KUMA_KEY.to_owned()),
        account: None,
        config: serde_json::json!({}),
    };
    let real = match Registry::builtin().build(instance) {
        Ok(source) => source,
        Err(e) => panic!("the registry must build a kuma instance: {e:?}"),
    };

    let mut conn = dedicated().await;
    let run = knobas_sync::run_from_stored_cursor(&mut conn, &pool, real.as_ref())
        .await
        .unwrap();
    assert_eq!(run.upserted, 8, "the recording holds eight monitors");

    // The stored row, every column the adapter decided. `8` is the canary's own
    // monitor id in Kuma, which is the key half of its entity id.
    let row = stored(&pool, &id, "8").await;
    assert_eq!(row.kind, "monitor");
    assert_eq!(row.title, "canary");
    assert_eq!(
        row.body_text, "canary up http http://host.docker.internal:8299/",
        "the indexed text leads with the name and the state"
    );
    assert_eq!(row.payload["state"], "up");
    assert_eq!(row.payload["id"], "8");
    // `/metrics` carries no timestamp of any kind, so a monitor is undated and
    // the mirror keeps the hole rather than filling it with `now()`.
    assert_eq!(row.item_updated_at, None);
    assert_eq!(row.author, None);
    assert_eq!(
        row.web_url.as_deref(),
        Some(format!("{}/dashboard/8", kuma.uri()).as_str())
    );

    // ...and out again through the launcher's own read. Scoped by the source id
    // this test minted, because the database is shared with every other test in
    // this binary.
    let found = knobas_app::commands::search::search_inner(
        &pool,
        knobas_search::SearchQuery {
            raw: "canary".to_owned(),
            limit: 20,
            filters: knobas_search::SearchFilters {
                sources: vec![id.clone()],
                ..knobas_search::SearchFilters::default()
            },
        },
    )
    .await
    .unwrap();
    assert_eq!(found.total, 1, "{found:?}");
    let group = &found.groups[0];
    assert_eq!(group.kind, "monitor");
    assert_eq!(group.plural, "Monitors");
    assert_eq!(group.hits[0].row.title, "canary");
    let excerpt: String = group.hits[0]
        .snippet
        .iter()
        .map(|segment| segment.text.as_str())
        .collect();
    assert!(
        excerpt.contains("up"),
        "the launcher row shows the monitor's state under its title: {excerpt:?}"
    );

    // **A state word is not a search term, and this is where that is written
    // down.** `websearch_to_tsquery('english', …)` drops `up` and `down` as
    // stopwords, so a query that is only a state lexes to the empty tsquery and
    // finds nothing -- measured here rather than assumed, because the obvious
    // reading of "found by name with their state" is that `kuma down` is a
    // query, and it is not. What the criterion asks for is delivered by the row
    // above: the state is in the indexed text, so it is in the excerpt the
    // launcher draws under the title.
    let by_state = |raw: &str| {
        knobas_app::commands::search::search_inner(
            &pool,
            knobas_search::SearchQuery {
                raw: raw.to_owned(),
                limit: 20,
                filters: knobas_search::SearchFilters {
                    sources: vec![id.clone()],
                    ..knobas_search::SearchFilters::default()
                },
            },
        )
    };
    for stopword in ["up", "down"] {
        let answer = by_state(stopword).await.unwrap();
        assert_eq!(
            answer.total, 0,
            "{stopword:?} is an English stopword, so it is no query at all: {answer:?}"
        );
    }
}

/// **After the Kuma source syncs, the estate file's monitor names become
/// links** (issue #445, spec #427's import sentence).
///
/// The one criterion of #445 that no seam can answer alone. `assets_ipc.rs`
/// pins the import against a mirror **seeded by hand**, which proves the
/// resolution rule and takes the monitor names on trust; `map.rs` pins what
/// the adapter calls a monitor and takes the estate file on trust. Both are
/// green while the two vocabularies disagree -- and the whole join is a string
/// comparison between a name somebody typed into `testenv/hetzner/estate.json`
/// and a name somebody typed into Uptime Kuma, which is the single most
/// likely thing in this milestone to be off by a space or a case.
///
/// So: the real adapter, over real HTTP against the recording, into the
/// mirror, and then the **real estate file** imported over it -- with the
/// count asserted as a literal seven rather than counted out of the file,
/// which would agree with the file by construction however wrong the names
/// were.
///
/// A scratch database of its own, unlike the rest of this file: the import
/// resolves a name against *every* monitor the mirror holds, and this binary's
/// shared database has another test's Kuma in it.
#[tokio::test]
async fn after_kuma_syncs_the_estate_files_monitor_names_become_links() {
    /// The file the demo profile and `estate_exit.rs` load -- the real estate.
    const ESTATE_FILE: &str = include_str!("../../../testenv/hetzner/estate.json");

    let kuma = spawn_mock_kuma().await;
    let scratch = knobas_db::test_util::scratch_database("kuma-estate-import").await;
    let pool = scratch
        .pool(2)
        .await
        .expect("a pool on the scratch database");
    let id = unique_id();
    configure(&pool, &id, "kuma", &kuma.uri()).await;

    let real = Registry::builtin()
        .build(SourceInstance {
            id: id.clone(),
            kind: knobas_source_kuma::ADAPTER_KIND.to_owned(),
            display_name: "Uptime Kuma".to_owned(),
            base_url: kuma.uri(),
            auth: Some(AuthMethod::ApiToken),
            secret: Some(KUMA_KEY.to_owned()),
            account: None,
            config: serde_json::json!({}),
        })
        .expect("the registry must build a kuma instance");

    let mut conn = scratch
        .connect()
        .await
        .expect("a connection outside every pool");
    let run = knobas_sync::run_from_stored_cursor(&mut conn, &pool, real.as_ref())
        .await
        .unwrap();
    assert_eq!(run.upserted, 8, "the recording holds eight monitors");

    let preview = knobas_app::assets::preview_import(&pool, ESTATE_FILE, ESTATE_FILE_PRODUCER)
        .await
        .expect("the preview");
    assert_eq!(
        preview
            .monitor_links
            .iter()
            .map(|link| (link.asset_id.as_str(), link.monitor_name.as_str()))
            .collect::<Vec<_>>(),
        [
            ("asset:hetzner-confluence", "knobas-confluence"),
            ("asset:hetzner-jira", "knobas-jira"),
            ("asset:hetzner-teamcity", "knobas-teamcity"),
            ("asset:knobas-confluence", "confluence (tunnel)"),
            ("asset:knobas-gitea", "gitea"),
            ("asset:knobas-jira", "jira (tunnel)"),
            ("asset:knobas-teamcity", "teamcity (tunnel)"),
        ],
        "every one of the file's seven monitor names answers to a monitor \
         Kuma published"
    );
    assert!(
        preview.unresolved.is_empty(),
        "nothing is left waiting: {:?}",
        preview.unresolved
    );

    let outcome = knobas_app::assets::apply_import(&pool, ESTATE_FILE, ESTATE_FILE_PRODUCER)
        .await
        .expect("the import")
        .value;
    assert_eq!(outcome.monitors_linked, 7);

    // ...and out again through the pane the reader looks at, which is the far
    // end of story 33's *monitoring* row: the state Kuma published and the
    // page in Kuma to open, on a monitor nobody attached by hand.
    let pane = knobas_app::assets::get(&pool, "asset:knobas-gitea")
        .await
        .expect("the container's pane");
    assert_eq!(
        pane.monitoring
            .iter()
            .map(|watch| (
                watch.name.as_str(),
                watch.state.as_deref(),
                watch.tombstoned
            ))
            .collect::<Vec<_>>(),
        [("gitea", Some("up"), false)],
        "the monitor the file named, with the state the adapter parsed"
    );
    assert_eq!(
        pane.monitoring[0].web_url.as_deref(),
        Some(format!("{}/dashboard/7", kuma.uri()).as_str()),
        "the deep link is the adapter's own, not one this read built"
    );
}

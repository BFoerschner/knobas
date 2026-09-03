//! **The seam: a real adapter, through the sync engine, into a real database.**
//!
//! Issue #93. Every other test in this workspace stops one side short of this
//! join, and the two halves it leaves are each honest on their own:
//!
//! * `knobas-sync/tests/backfill.rs` runs the engine against a real embedded
//!   PostgreSQL and asserts on stored rows -- with a *stand-in* adapter whose
//!   payload is `{"key": …, "payload_version": 1}`. It proves the engine's
//!   backfill semantics and nothing about any adapter's record.
//! * `knobas-source-jira/tests/mockd.rs` drives the real adapter over real
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
/// `KIND_BUILD_CONFIG`. `knobas-core/tests/projects.rs` pins that arm with
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

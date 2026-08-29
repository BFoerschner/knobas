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
//! **One adapter: Jira.** It is the adapter whose acceptance criterion exposed
//! the gap. TeamCity is a follow-up, and a second adapter here should be one
//! more test rather than a framework. Gitea is deliberately *not* here: mockd
//! serves no Gitea by standing decision, and Gitea's equivalent join belongs
//! to the docker-gated container layer.

use async_trait::async_trait;
use knobas_app::sources::Registry;
use knobas_mockd::spawn_mock_jira;
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
const CORPUS: usize = 7;

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
    async fn write(&self, op: WriteOp) -> Result<(), SourceError> {
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
/// store the position a run comes back with.
async fn configure(pool: &PgPool, id: &str, base_url: &str) {
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
         values ($1, 'jira', 'Tidewater Jira', $2, 'pat')",
    )
    .bind(id)
    .bind(base_url)
    .execute(pool)
    .await
    .unwrap();
}

/// Build the **real** Jira adapter the way the scheduler does: through
/// `knobas-app`'s registry, from a configured instance and a credential.
fn adapter(id: &str, base_url: &str) -> Box<dyn Source> {
    let instance = SourceInstance {
        id: id.to_owned(),
        kind: "jira".to_owned(),
        display_name: "Tidewater Jira".to_owned(),
        base_url: base_url.to_owned(),
        auth: Some(AuthMethod::Pat),
        secret: Some(knobas_mockd::JIRA_TOKEN.to_owned()),
        config: serde_json::json!({}),
    };
    match Registry::builtin().build(instance) {
        Ok(source) => source,
        // `Box<dyn Source>` is not `Debug`, so `expect` is unavailable.
        Err(e) => panic!("the registry must build a jira instance: {e:?}"),
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
    configure(&pool, &id, &jira.base_url()).await;
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
    assert_eq!(filled.upserted as usize, CORPUS);
    assert_eq!(
        widened_names(&stored(&pool, &id, UNTOUCHED).await.payload),
        Vec::<&str>::new(),
        "the starting state must be a narrow mirror, or nothing below is a widening"
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
        backfill.upserted as usize, CORPUS,
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
    assert_eq!(parsed.0.len(), CORPUS);
    let mut compared = 0_usize;
    for item in &parsed.0 {
        let row = stored(&pool, &id, &item.entity.key).await;
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
        compared += 1;
    }
    assert_eq!(
        compared, CORPUS,
        "no issue was compared, so this proves nothing"
    );

    // Every request this test made is one the vendored WADL declares, so the
    // widened `fields=` is a query a real Jira DC would accept.
    jira.assert_no_violations();
}

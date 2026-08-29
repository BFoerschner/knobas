//! The inbox as the interface sees it: what it offers, what it records, and
//! **an action completing through the queue to a source** (issue #45).
//!
//! # Why the end-to-end test is here and not in a store battery
//!
//! M2 exit criterion 2 is *"inbox populated and actionable end-to-end"*, and
//! #45 is explicit that a test showing items appearing does not discharge it
//! without an action completing through the queue to a source. That join --
//! derivation, the offered op, `WriteOp`, the write queue, `Source::write` --
//! exists in no other file: `knobas-core/tests/inbox.rs` stops at the
//! derivation and `knobas-sync/tests/write_queue.rs` starts at a submitted
//! write. Both can be green while the join between them is broken, which is
//! the argument `tests/adapter_to_mirror.rs` makes for its own seam.
//!
//! **The dispatcher is a fake `Source`** that records what it was asked to do,
//! so the assertion is on the op the adapter received rather than on a row
//! this test wrote.
//!
//! # Why every test gets a database of its own
//!
//! The derivation is a pass over the whole mirror, so the binary's shared
//! database would put every other test's fixtures in this one's stream. The
//! same reasoning `knobas-core/tests/inbox.rs` records. `scratch_database`
//! hands back a `Connector`, which is also what the flush loop's
//! `RunConnections` needs, so the end-to-end test runs entirely inside its own
//! database too.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Duration, TimeZone, Utc};
use knobas_app::commands::entity::{
    complete_inbox_item_inner, inbox_count_inner, inbox_items_inner, snooze_inbox_item_inner,
};
use knobas_app::inbox::InboxEntry;
use knobas_core::inbox::Shelf;
use knobas_secrets::{MemoryStore, Secret, SecretStore};
use knobas_source::instance::SourceInstance;
use knobas_source::{
    AuthMethod, ConnectionInfo, Cursor, KindInfo, Sink, Source, SourceDescriptor, SourceError,
    WriteOp,
};
use knobas_sync::config::{self, AuthKind, InsertConfig};
use knobas_sync::scheduler::{AdapterRegistry, RunConnections, SchedulerDeps, SyncEvents};
use sqlx::PgPool;

const ME: &str = "mara.lindqvist";
const THEM: &str = "jonas.becker";

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 29, 12, 0, 0).unwrap()
}

// -- a source that records what it was asked to write -----------------------

/// A `Source` whose only job is to say what it received.
struct Recording {
    id: String,
    kind: String,
    ops: Arc<Mutex<Vec<WriteOp>>>,
    write_ops: Vec<String>,
}

#[async_trait]
impl Source for Recording {
    fn descriptor(&self) -> SourceDescriptor {
        descriptor(&self.id, &self.kind, &self.write_ops)
    }

    async fn test_connection(&self) -> Result<ConnectionInfo, SourceError> {
        Ok(ConnectionInfo {
            account: None,
            server_version: None,
            secret_expires_at: None,
            detail: None,
        })
    }

    async fn sync(
        &self,
        cursor: Option<Cursor>,
        _sink: &mut (dyn Sink + Send),
    ) -> Result<Cursor, SourceError> {
        Ok(cursor.unwrap_or_default())
    }

    async fn write(&self, op: WriteOp) -> Result<(), SourceError> {
        self.ops.lock().unwrap().push(op);
        Ok(())
    }
}

fn descriptor(id: &str, kind: &str, write_ops: &[String]) -> SourceDescriptor {
    SourceDescriptor {
        id: id.to_owned(),
        adapter_kind: kind.to_owned(),
        name: kind.to_owned(),
        capabilities: Vec::new(),
        adapter_version: "0".to_owned(),
        auth_methods: vec![AuthMethod::Pat],
        write_ops: write_ops.to_vec(),
        entity_kinds: vec![KindInfo {
            id: "pr".to_owned(),
            label: "Pull request".to_owned(),
            plural: "Pull requests".to_owned(),
            monogram: "PR".to_owned(),
            full_sync_exhaustive: true,
        }],
        config_schema: serde_json::json!({"type": "object", "properties": {}}),
    }
}

/// Two adapter kinds with different write ops, so "an op the source does not
/// declare is not offered" is a statement about two real descriptors rather
/// than about one flag.
struct Registry {
    ops: Arc<Mutex<Vec<WriteOp>>>,
}

const FORGE_OPS: &[&str] = &["approve", "comment", "create_pull_request"];
const TRACKER_OPS: &[&str] = &["comment", "transition"];

fn ops_of(kind: &str) -> Vec<String> {
    match kind {
        "forge" => FORGE_OPS,
        _ => TRACKER_OPS,
    }
    .iter()
    .map(|op| (*op).to_owned())
    .collect()
}

impl AdapterRegistry for Registry {
    fn descriptors(&self) -> Vec<SourceDescriptor> {
        ["forge", "tracker"]
            .into_iter()
            .map(|kind| descriptor(kind, kind, &ops_of(kind)))
            .collect()
    }

    fn build(&self, instance: SourceInstance) -> Result<Box<dyn Source>, SourceError> {
        let kind = instance.config["adapter_kind"]
            .as_str()
            .unwrap_or("forge")
            .to_owned();
        Ok(Box::new(Recording {
            write_ops: ops_of(&kind),
            id: instance.id,
            kind,
            ops: Arc::clone(&self.ops),
        }))
    }
}

/// A newtype, because the orphan rule forbids implementing `knobas-sync`'s
/// trait for `knobas-db`'s connector from a test binary that owns neither.
struct Connections(knobas_db::embedded::Connector);

#[async_trait]
impl RunConnections for Connections {
    async fn open(&self) -> Result<sqlx::PgConnection, sqlx::Error> {
        self.0.connect().await
    }
}

#[derive(Default)]
struct Silent;

impl SyncEvents for Silent {
    fn sync_state(&self, _status: knobas_sync::SourceSyncStatus) {}
    fn source_health(&self, _health: knobas_sync::CredentialHealth) {}
    fn activity_new(&self, _row: knobas_core::activity::ActivityRow) {}
}

struct Harness {
    deps: SchedulerDeps,
    registry: Arc<Registry>,
    ops: Arc<Mutex<Vec<WriteOp>>>,
}

impl Harness {
    fn pool(&self) -> &PgPool {
        &self.deps.pool
    }

    /// The stream, with each entry's offered actions.
    async fn stream(&self) -> Vec<InboxEntry> {
        inbox_items_inner(self.pool(), self.registry.as_ref(), now(), Shelf::Stream)
            .await
            .expect("the inbox reads")
    }

    async fn entry(&self, key: &str) -> InboxEntry {
        self.stream()
            .await
            .into_iter()
            .find(|entry| entry.item.key == key)
            .unwrap_or_else(|| panic!("no inbox item {key:?}"))
    }

    /// Every activity line, oldest first.
    async fn activity(&self) -> Vec<knobas_core::activity::ActivityRow> {
        let mut rows = knobas_core::activity::recent(self.pool(), 100, None)
            .await
            .unwrap();
        rows.reverse();
        rows
    }

    /// A configured source whose adapter is the recorder.
    async fn source(&self, id: &str, kind: &str, username: &str) {
        config::insert(
            self.pool(),
            &InsertConfig {
                id: id.to_owned(),
                adapter_kind: kind.to_owned(),
                display_name: id.to_owned(),
                base_url: String::new(),
                auth_kind: AuthKind::Method(AuthMethod::Pat),
                config: serde_json::json!({ "username": username, "adapter_kind": kind }),
                sync_interval_secs: 300,
                enabled: true,
            },
        )
        .await
        .expect("the source row is written");
    }

    /// One live mirror item.
    async fn item(
        &self,
        source: &str,
        kind: &str,
        key: &str,
        author: &str,
        body: &str,
        payload: serde_json::Value,
    ) -> String {
        let id = format!("{source}:{key}");
        sqlx::query("insert into knobas.entity (id, kind, title) values ($1,$2,$3)")
            .bind(&id)
            .bind(kind)
            .bind(key)
            .execute(self.pool())
            .await
            .unwrap();
        sqlx::query(
            "insert into sync.item
                 (entity_id, source_id, kind, title, body_text, author, item_updated_at, payload)
             values ($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(&id)
        .bind(source)
        .bind(kind)
        .bind(key)
        .bind(body)
        .bind(author)
        .bind(now() - Duration::days(1))
        .bind(payload)
        .execute(self.pool())
        .await
        .unwrap();
        id
    }

    /// A pull request asking `ME` for a review.
    async fn review_request(&self, source: &str, key: &str) -> String {
        self.item(
            source,
            "pr",
            key,
            THEM,
            "",
            serde_json::json!({
                "state": "open",
                "requested_reviewers": [{ "login": ME }],
            }),
        )
        .await
    }
}

async fn harness() -> Harness {
    let connector = knobas_db::test_util::scratch_database("inbox-ipc").await;
    let pool = connector.pool(4).await.expect("a pool onto the scratch db");

    let secrets = MemoryStore::new();
    for id in ["forge", "tracker"] {
        secrets
            .put(
                id,
                &Secret {
                    kind: AuthMethod::Pat,
                    value: "tok".to_owned(),
                },
            )
            .unwrap();
    }

    let ops = Arc::new(Mutex::new(Vec::new()));
    let registry = Arc::new(Registry {
        ops: Arc::clone(&ops),
    });
    Harness {
        deps: SchedulerDeps {
            pool,
            connections: Arc::new(Connections(connector)),
            registry: Arc::clone(&registry) as Arc<dyn AdapterRegistry>,
            secrets: Arc::new(secrets),
            events: Arc::new(Silent) as Arc<dyn SyncEvents>,
        },
        registry,
        ops,
    }
}

// -- what the inbox offers --------------------------------------------------

/// Stories 9, 12 and 23. The forge declares `approve`, so a review request on
/// it offers *approve* first and *comment* as the alternative; the tracker does
/// not, so the same category on it offers only what it can do.
///
/// The negative half is the whole point: an interface that offered *approve*
/// on a source that cannot approve would be one that lies, and the failure
/// would surface as a rejected write long after the button was pressed.
#[tokio::test]
async fn an_item_offers_only_the_ops_its_own_source_declares() {
    let h = harness().await;
    h.source("forge", "forge", ME).await;
    h.source("tracker", "tracker", ME).await;
    let can = h.review_request("forge", "acme/payouts#144").await;
    let cannot = h.review_request("tracker", "acme/payouts#145").await;

    assert_eq!(
        h.entry(&format!("review_request:{can}")).await.actions,
        vec!["approve".to_owned(), "comment".to_owned()],
        "the answer first, the alternative second"
    );
    assert_eq!(
        h.entry(&format!("review_request:{cannot}")).await.actions,
        vec!["comment".to_owned()],
        "a source that cannot approve must not be shown an approve button"
    );
}

/// A category with no write op offers none however capable the source is --
/// and the item is still in the stream, because *open*, *snooze* and *done*
/// are always available and are not write ops.
#[tokio::test]
async fn an_item_with_no_write_op_is_still_an_item() {
    let h = harness().await;
    h.source("tracker", "tracker", ME).await;
    sqlx::query("update knobas.source_config set secret_expires_at = $1 where id = 'tracker'")
        .bind(now() + Duration::days(4))
        .execute(h.pool())
        .await
        .unwrap();

    let entry = h.entry("credential_expiry:tracker").await;
    assert!(entry.actions.is_empty(), "nothing at a source can fix this");
    assert!(
        entry.item.reason.contains("expires"),
        "{}",
        entry.item.reason
    );
}

// -- an action, all the way to a source -------------------------------------

/// **M2 exit criterion 2.** The action an inbox item offers, submitted through
/// the write queue, reaches `Source::write` -- and the activity stream records
/// it.
///
/// The op is taken from the *entry* rather than written out, so a change that
/// made the inbox offer something else would be caught here as well as in the
/// unit test: this asserts that what the interface offers is what the adapter
/// receives.
#[tokio::test]
async fn the_action_an_item_offers_reaches_the_source_and_is_recorded() {
    let h = harness().await;
    h.source("forge", "forge", ME).await;
    let pr = h.review_request("forge", "acme/payouts#144").await;

    let entry = h.entry(&format!("review_request:{pr}")).await;
    let offered = entry.actions.first().expect("an offered action");
    assert_eq!(offered, "approve");

    // The inbox composes the op from what it offered and the item it is on;
    // `knobas_sync::write_queue` is the only thing that calls `Source::write`.
    knobas_sync::write_queue::submit(
        &h.deps,
        &entry.item.source_id,
        WriteOp::Approve {
            entity: entry.item.entity_id.clone().expect("a pull request"),
            body: String::new(),
        },
    )
    .await
    .expect("the write queues and flushes");

    let received = h.ops.lock().unwrap().clone();
    assert!(
        matches!(received.as_slice(), [WriteOp::Approve { entity, .. }] if entity == &pr),
        "the source received exactly the approval the inbox offered: {received:?}"
    );

    let verbs: Vec<String> = h.activity().await.into_iter().map(|r| r.verb).collect();
    assert!(
        verbs.contains(&"sent".to_owned()),
        "the queue records what it delivered: {verbs:?}"
    );
}

// -- what an answer records -------------------------------------------------

/// Story 21: *every* inbox action is recorded. Snooze is one of the two the
/// queue knows nothing about, so the inbox records it -- once, naming the item
/// and the date it comes back.
#[tokio::test]
async fn snoozing_records_one_line_naming_the_item_and_its_date() {
    let h = harness().await;
    h.source("forge", "forge", ME).await;
    let pr = h.review_request("forge", "acme/payouts#144").await;
    let key = format!("review_request:{pr}");
    let until = now() + Duration::days(2);

    snooze_inbox_item_inner(h.pool(), now(), &key, until)
        .await
        .expect("the item is on the stream");

    let rows = h.activity().await;
    assert_eq!(rows.len(), 1, "one line, not none and not two: {rows:?}");
    assert_eq!(rows[0].verb, "snoozed");
    assert_eq!(rows[0].actor, "user");
    assert_eq!(rows[0].entity_id.as_deref(), Some(pr.as_str()));
    assert_eq!(rows[0].detail["item_key"], serde_json::json!(key));
    assert_eq!(
        rows[0].detail["category"],
        serde_json::json!("review_request")
    );
    assert!(
        rows[0].detail["until"].is_string(),
        "the line says when it comes back: {}",
        rows[0].detail
    );

    assert_eq!(
        inbox_count_inner(h.pool(), now()).await.unwrap(),
        0,
        "and the number the top strip shows is what needs me now"
    );
}

/// The other half of story 21, and story 16.
#[tokio::test]
async fn marking_an_item_done_records_one_line_and_clears_the_count() {
    let h = harness().await;
    h.source("forge", "forge", ME).await;
    let pr = h.review_request("forge", "acme/payouts#144").await;
    let key = format!("review_request:{pr}");
    assert_eq!(inbox_count_inner(h.pool(), now()).await.unwrap(), 1);

    complete_inbox_item_inner(h.pool(), now(), &key)
        .await
        .expect("the item is on the stream");

    let rows = h.activity().await;
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].verb, "completed");
    assert!(
        rows[0].detail["until"].is_null(),
        "done has no return date: {}",
        rows[0].detail
    );
    assert_eq!(inbox_count_inner(h.pool(), now()).await.unwrap(), 0);
}

/// An answer to an item that is no longer derived is refused, and **nothing is
/// written**: the key arrives from a webview holding a list, and a durable row
/// plus an activity line about work that has since been resolved at the source
/// would be a record of something that never happened.
#[tokio::test]
async fn answering_an_item_that_is_no_longer_there_is_refused_and_records_nothing() {
    let h = harness().await;
    h.source("forge", "forge", ME).await;

    let error = complete_inbox_item_inner(h.pool(), now(), "review_request:forge:acme/gone#1")
        .await
        .expect_err("no such item");
    assert_eq!(error.code, knobas_app::IpcErrorCode::NotFound);
    assert!(error.message.contains("acme/gone#1"), "{}", error.message);
    assert!(
        h.activity().await.is_empty(),
        "nothing happened, so nothing is logged"
    );
}

/// A snoozed item can be snoozed again -- that is changing your mind about a
/// date, not answering something that is not there -- so the refusal above
/// must search both shelves.
#[tokio::test]
async fn an_item_on_the_snoozed_shelf_can_still_be_answered() {
    let h = harness().await;
    h.source("forge", "forge", ME).await;
    let pr = h.review_request("forge", "acme/payouts#144").await;
    let key = format!("review_request:{pr}");

    snooze_inbox_item_inner(h.pool(), now(), &key, now() + Duration::days(2))
        .await
        .unwrap();
    complete_inbox_item_inner(h.pool(), now(), &key)
        .await
        .expect("a snoozed item is still an item I can finish");

    assert_eq!(
        h.activity()
            .await
            .into_iter()
            .map(|r| r.verb)
            .collect::<Vec<_>>(),
        vec!["snoozed".to_owned(), "completed".to_owned()],
        "every inbox action, in order"
    );
}

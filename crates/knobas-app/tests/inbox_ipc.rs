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
            discovered: std::collections::BTreeMap::new(),
        })
    }

    async fn sync(
        &self,
        cursor: Option<Cursor>,
        _sink: &mut (dyn Sink + Send),
    ) -> Result<Cursor, SourceError> {
        Ok(cursor.unwrap_or_default())
    }

    async fn write(&self, op: WriteOp) -> Result<knobas_source::WriteReceipt, SourceError> {
        self.ops.lock().unwrap().push(op);
        Ok(knobas_source::WriteReceipt::none())
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
        // Where this stand-in keeps what the inbox reads (#277). Gitea's
        // spellings, because the fixtures below are Gitea-shaped pull
        // requests: an inbox rule reads through the declaration now, so a
        // source that declares nothing produces no review requests and no
        // assignments -- which is what
        // `a_source_that_declares_no_paths_produces_no_assignments_and_no_review_requests`
        // asserts in `knobas-core/tests/inbox.rs`.
        payload_paths: vec![knobas_source::KindPaths {
            kind: "pr".to_owned(),
            reviewers: vec![knobas_source::ListPath {
                at: knobas_source::PayloadPath::of(["requested_reviewers"]),
                entry: knobas_source::PayloadPath::of(["login"]),
            }],
            ..knobas_source::KindPaths::default()
        }],
    }
}

/// Three adapter kinds with different write ops, so "an op the source does not
/// declare is not offered" is a statement about real descriptors rather than
/// about one flag.
///
/// `wiki` declares **none**, which is the shape `knobas-source-confluence` is
/// at (#287: its `Comment` op arrives with #286). It is here so the
/// read-only-source case is witnessed by a descriptor rather than assumed.
struct Registry {
    ops: Arc<Mutex<Vec<WriteOp>>>,
}

const FORGE_OPS: &[&str] = &["approve", "comment", "create_pull_request"];
const TRACKER_OPS: &[&str] = &["comment", "transition"];
/// A read-only source: the Confluence adapter's shape until #286.
const WIKI_OPS: &[&str] = &[];

fn ops_of(kind: &str) -> Vec<String> {
    match kind {
        "forge" => FORGE_OPS,
        "wiki" => WIKI_OPS,
        _ => TRACKER_OPS,
    }
    .iter()
    .map(|op| (*op).to_owned())
    .collect()
}

impl AdapterRegistry for Registry {
    fn descriptors(&self) -> Vec<SourceDescriptor> {
        ["forge", "tracker", "wiki"]
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
    for id in ["forge", "tracker", "wiki"] {
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
            timing: knobas_sync::scheduler::SchedulerTiming::default(),
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

    snooze_inbox_item_inner(h.pool(), h.registry.as_ref(), now(), &key, until)
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
        inbox_count_inner(h.pool(), h.registry.as_ref(), now())
            .await
            .unwrap(),
        0,
        "and the number the top strip shows is what needs me now"
    );
}

/// **The Confluence mention's actions, in full** (#287, criterion 2).
///
/// A wiki page whose discussion names the reader is a mention like any other,
/// and everything the inbox promises on it is here in one test:
///
/// * it is **on the stream**, with the page as its entity;
/// * *open* is knobas' own -- it is `web_url`, not a write op, so it is never
///   in `actions`;
/// * **no source-side action is offered**, because this adapter declares no
///   write ops yet. The ticket's *comment (through the comment op)* is the
///   half #286 lands -- `Category::Mention` already asks for `comment`, so the
///   day the Confluence descriptor declares it the button appears with nothing
///   in this crate changing, which
///   `knobas_app::inbox`'s `a_mention_offers_comment_from_a_source_that_declares_it`
///   pins from the other side;
/// * *snooze* and *complete* each record **one** activity line naming the
///   page, which is what story 21 asks for and the only writer that can (the
///   queue records the ops, and there is no op here).
#[tokio::test]
async fn a_wiki_mention_offers_no_source_action_and_still_records_both_answers() {
    let h = harness().await;
    h.source("wiki", "wiki", ME).await;
    let page = h
        .item(
            "wiki",
            "page",
            "98307",
            THEM,
            // A transcription of what `knobas-source-confluence`'s storage
            // renderer produces for `<ac:link><ri:user ri:userkey="…"/></ac:link>`
            // in a comment -- this crate cannot witness that agreement, and
            // `atlassian_live.rs` is where it is witnessed. What is asserted
            // here is what the *inbox* does with such a body.
            &format!("SEPA payout retry design\n\n@{ME} can you add the SLA?"),
            serde_json::json!({ "space": { "key": "ENG" } }),
        )
        .await;
    let key = format!("mention:{page}");

    let entry = h.entry(&key).await;
    assert_eq!(entry.item.entity_id.as_deref(), Some(page.as_str()));
    assert_eq!(entry.item.kind.as_deref(), Some("page"));
    assert!(
        entry.actions.is_empty(),
        "a source that declares no write op must offer no button: {:?}",
        entry.actions
    );

    let until = now() + Duration::days(2);
    snooze_inbox_item_inner(h.pool(), h.registry.as_ref(), now(), &key, until)
        .await
        .expect("the mention is on the stream");
    complete_inbox_item_inner(h.pool(), h.registry.as_ref(), now(), &key)
        .await
        .expect("a snoozed item can still be answered");

    let verbs: Vec<(String, Option<String>)> = h
        .activity()
        .await
        .into_iter()
        .map(|r| (r.verb, r.entity_id))
        .collect();
    assert_eq!(
        verbs,
        vec![
            ("snoozed".to_owned(), Some(page.clone())),
            ("completed".to_owned(), Some(page.clone())),
        ],
        "one line each, and each naming the page"
    );
}

/// The other half of story 21, and story 16.
#[tokio::test]
async fn marking_an_item_done_records_one_line_and_clears_the_count() {
    let h = harness().await;
    h.source("forge", "forge", ME).await;
    let pr = h.review_request("forge", "acme/payouts#144").await;
    let key = format!("review_request:{pr}");
    assert_eq!(
        inbox_count_inner(h.pool(), h.registry.as_ref(), now())
            .await
            .unwrap(),
        1
    );

    complete_inbox_item_inner(h.pool(), h.registry.as_ref(), now(), &key)
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
    assert_eq!(
        inbox_count_inner(h.pool(), h.registry.as_ref(), now())
            .await
            .unwrap(),
        0
    );
}

/// An answer to an item that is no longer derived is refused, and **nothing is
/// written**: the key arrives from a webview holding a list, and a durable row
/// plus an activity line about work that has since been resolved at the source
/// would be a record of something that never happened.
#[tokio::test]
async fn answering_an_item_that_is_no_longer_there_is_refused_and_records_nothing() {
    let h = harness().await;
    h.source("forge", "forge", ME).await;

    let error = complete_inbox_item_inner(
        h.pool(),
        h.registry.as_ref(),
        now(),
        "review_request:forge:acme/gone#1",
    )
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

    snooze_inbox_item_inner(
        h.pool(),
        h.registry.as_ref(),
        now(),
        &key,
        now() + Duration::days(2),
    )
    .await
    .unwrap();
    complete_inbox_item_inner(h.pool(), h.registry.as_ref(), now(), &key)
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

/// Every candidate op a category names is one the SPI actually defines.
///
/// A typo would be an action silently never offered by any source, which
/// nothing else in the tree would catch: `Category::candidate_ops` returns
/// plain strings — deliberately, because `knobas-source` depends on
/// `knobas-core` and not the reverse — so no compiler checks them.
///
/// **In this file rather than beside the table**, and that is the choke-point
/// guard's doing rather than a preference: `knobas_sync`'s
/// `write_choke_point.rs` fails any production file under `crates/*/src/` that
/// so much as *names* `WriteOp` without implementing the trait, and it counts
/// a `#[cfg(test)]` module. `knobas_app::inbox` names ops as identifiers and
/// nothing else, which is exactly the property that guard exists to keep true,
/// so the pin belongs in `tests/` where the scan does not reach.
#[test]
fn every_candidate_op_is_one_the_spi_defines() {
    let known = [
        WriteOp::Comment {
            entity: String::new(),
            body: String::new(),
        },
        WriteOp::Transition {
            entity: String::new(),
            status: String::new(),
        },
        WriteOp::Approve {
            entity: String::new(),
            body: String::new(),
        },
        WriteOp::RerunBuild {
            entity: String::new(),
        },
        WriteOp::TriggerBuild {
            entity: String::new(),
        },
        WriteOp::CreateBranch {
            entity: String::new(),
            name: String::new(),
            from_ref: String::new(),
        },
        WriteOp::CreatePullRequest {
            entity: String::new(),
            title: String::new(),
            body: String::new(),
            head: String::new(),
            base: String::new(),
        },
        WriteOp::CreateTicket {
            entity: String::new(),
            title: String::new(),
            body: String::new(),
            ticket_type: String::new(),
        },
    ];
    let identifiers: Vec<&str> = known.iter().map(WriteOp::identifier).collect();
    for category in knobas_core::inbox::Category::ALL {
        for op in category.candidate_ops() {
            assert!(
                identifiers.contains(op),
                "{category} asks for {op:?}, which no WriteOp identifies as"
            );
        }
    }
}

// -- which kinds may notify -------------------------------------------------

/// **Every kind is off until somebody says otherwise** (spec #272, story 71).
///
/// The direction that matters: a fresh profile must read as *empty*, not as
/// *all*. The whole reason the setting exists is that a noisy Jira must not be
/// able to make the feature unusable on the day it is installed, and a default
/// that came back full would notify for every item in the first sync.
#[tokio::test]
async fn a_profile_nobody_has_opted_in_on_notifies_for_nothing() {
    let harness = harness().await;
    let stored = knobas_app::inbox::notification_kinds(&harness.deps.pool)
        .await
        .expect("the setting reads");
    assert!(
        stored.is_empty(),
        "a fresh profile notifies for {stored:?} -- every kind is off by default"
    );
}

/// The round trip, and the two things the write normalises.
///
/// The settings section draws a checkbox per category and sends the whole set
/// back on every click, so the order is the order the boxes happen to sit in
/// and a double-click can send a word twice. What comes back is one spelling
/// for one set: deduplicated, and in `Category::ALL`'s order rather than the
/// caller's.
#[tokio::test]
async fn what_comes_back_is_the_stored_set_deduplicated_and_in_one_order() {
    let harness = harness().await;
    let answered = knobas_app::inbox::set_notification_kinds(
        &harness.deps.pool,
        &[
            "mention".to_owned(),
            "review_request".to_owned(),
            "mention".to_owned(),
        ],
    )
    .await
    .expect("two known categories are stored");
    assert_eq!(
        answered,
        vec![
            knobas_core::inbox::Category::ReviewRequest,
            knobas_core::inbox::Category::Mention
        ]
    );
    assert_eq!(
        knobas_app::inbox::notification_kinds(&harness.deps.pool)
            .await
            .expect("the setting reads back"),
        answered,
        "the read answers what the write said it stored"
    );
}

/// Switching every kind off again stores the empty set rather than leaving the
/// last one on.
///
/// The failure this is about is a write that treats "nothing chosen" as
/// "nothing to do": the reader unticks the last box, the row is left where it
/// was, and knobas goes on interrupting somebody who has just asked it to
/// stop.
#[tokio::test]
async fn unticking_the_last_box_stores_the_empty_set() {
    let harness = harness().await;
    knobas_app::inbox::set_notification_kinds(&harness.deps.pool, &["failed_build".to_owned()])
        .await
        .expect("one category is stored");
    let answered = knobas_app::inbox::set_notification_kinds(&harness.deps.pool, &[])
        .await
        .expect("the empty set is stored");
    assert!(answered.is_empty());
    assert!(
        knobas_app::inbox::notification_kinds(&harness.deps.pool)
            .await
            .expect("the setting reads back")
            .is_empty(),
        "a kind switched off is still switched on after a restart"
    );
}

/// A word this build has no category for is **refused, with a sentence**, and
/// nothing is stored.
///
/// `invalid` and not a Tauri decode failure: the command takes the words rather
/// than the enum precisely so that this arrives at the settings section as an
/// `IpcError` it can draw, instead of as a bare string with no code on it.
#[tokio::test]
async fn a_word_that_is_not_a_category_is_refused_and_stores_nothing() {
    let harness = harness().await;
    knobas_app::inbox::set_notification_kinds(&harness.deps.pool, &["mention".to_owned()])
        .await
        .expect("one category is stored");

    let refused = knobas_app::inbox::set_notification_kinds(
        &harness.deps.pool,
        &["mention".to_owned(), "everything".to_owned()],
    )
    .await
    .expect_err("an unknown category is refused");
    assert_eq!(refused.code, knobas_app::IpcErrorCode::Invalid);
    assert!(
        refused.message.contains("everything") && refused.message.contains("review_request"),
        "the refusal names the word and what the inbox does notify for: {}",
        refused.message
    );

    assert_eq!(
        knobas_app::inbox::notification_kinds(&harness.deps.pool)
            .await
            .expect("the setting reads back"),
        vec![knobas_core::inbox::Category::Mention],
        "the refused write left the stored set where it was"
    );
}

/// A stored value that is not a list of category words reads as **silence**,
/// not as a failure.
///
/// The setting is a permission to interrupt somebody, so the safe direction for
/// one knobas cannot read is off. Two shapes, both reachable: a value of the
/// wrong type (an older build, a hand-edited row, a restored backup) and a list
/// carrying a category some future build has and this one does not.
#[tokio::test]
async fn an_unreadable_setting_reads_as_off_rather_than_as_on() {
    let harness = harness().await;
    for value in [
        serde_json::json!(true),
        serde_json::json!("mention"),
        serde_json::json!(["not_a_category"]),
    ] {
        sqlx::query(
            "insert into knobas.setting (key, value) values ($1, $2)
             on conflict (key) do update set value = excluded.value",
        )
        .bind(knobas_app::inbox::NOTIFICATION_KINDS_KEY)
        .bind(&value)
        .execute(&harness.deps.pool)
        .await
        .expect("the row is written");

        assert!(
            knobas_app::inbox::notification_kinds(&harness.deps.pool)
                .await
                .expect("an unreadable setting is not an error")
                .is_empty(),
            "{value} read as something other than silence"
        );
    }
}

/// A list that is *partly* readable keeps the half this build knows.
///
/// The other direction of the test above, and the one that says the forgiving
/// read is forgiving rather than blunt: a profile that used a build with a
/// sixth category must not lose the five it still has.
#[tokio::test]
async fn an_unknown_word_beside_known_ones_drops_only_itself() {
    let harness = harness().await;
    sqlx::query(
        "insert into knobas.setting (key, value) values ($1, $2)
         on conflict (key) do update set value = excluded.value",
    )
    .bind(knobas_app::inbox::NOTIFICATION_KINDS_KEY)
    .bind(serde_json::json!([
        "from_the_future",
        "mention",
        "failed_build"
    ]))
    .execute(&harness.deps.pool)
    .await
    .expect("the row is written");

    assert_eq!(
        knobas_app::inbox::notification_kinds(&harness.deps.pool)
            .await
            .expect("the setting reads"),
        vec![
            knobas_core::inbox::Category::Mention,
            knobas_core::inbox::Category::FailedBuild
        ]
    );
}

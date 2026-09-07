//! The standup protocol as the interface sees it (issue #289, spec #272
//! stories 64-69).
//!
//! # Why this seam
//!
//! Spec #272's Testing Decisions name it: *"the protocol's get-or-create and
//! the note-to-page link on settle"*, at the command seam, against a scratch
//! database and the trait-level mock source. Everything this feature promises
//! is a join between things that are each already tested -- notes, the write
//! queue, the settle, the link store, the mirror -- so a battery of unit tests
//! over the pieces could be green while the feature was wrong in all four of
//! the ways that matter:
//!
//! * a second open makes a second note with the same title;
//! * a second publish makes a second page;
//! * the link is drawn one way, so only one detail shows the other;
//! * the page id is recorded before the write has settled, or is lost when the
//!   process that held the receipt is gone.
//!
//! # The source is a real adapter as far as everything below the trait knows
//!
//! [`Wiki`] is a `Source`: it declares `create_page` and `create_ticket`, its
//! `write` answers a [`WriteReceipt`] carrying an id the way the real
//! Confluence adapter does, and its `sync` pushes whatever it has been asked
//! to hold. Nothing is stubbed between the command and the database -- the op
//! is composed, `submit` queues it, the scheduler flushes it, the settle
//! stamps `remote_id`, the re-sync mirrors the page and the link is drawn
//! against real rows.
//!
//! **The write and the sync are deliberately separate acts on the fake.** A
//! `write` that also put the page in the mirror would hide the ordering this
//! feature actually depends on, and the ordering is the interesting part: the
//! id exists on the queue row before the page exists in the mirror, so a link
//! drawn from the receipt alone would fail and a link drawn only at publish
//! time would never happen for a write that settled later. [`Wiki::offline`]
//! is what makes that second case reachable.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::NaiveDate;
use knobas_app::commands::entity::{
    create_action_item_ticket_inner, publish_standup_protocol_inner, standup_protocol_inner,
};
use knobas_app::protocol::{self, PublishTarget};
use knobas_app::sources::SourcesState;
use knobas_core::write_queue::WriteState;
use knobas_secrets::{MemoryStore, SecretStore};
use knobas_source::instance::SourceInstance;
use knobas_source::{
    AuthMethod, Capability, ConnectionInfo, Cursor, KindInfo, Sink, Source, SourceDescriptor,
    SourceError, SyncItem, WriteOp, WriteReceipt,
};
use knobas_sync::config::{self, AuthKind, InsertConfig};
use knobas_sync::scheduler::{AdapterRegistry, Scheduler, SchedulerDeps};
use sqlx::PgPool;

/// The wiki this battery publishes into, and the namespace its pages are in.
const WIKI: &str = "wiki";
/// A second Confluence, configured so that "which one" is a real question.
const OTHER_WIKI: &str = "wiki2";
/// The tracker action items become tickets in.
const TRACKER: &str = "tracker";

/// The parent every publish goes under -- the *Standup protocols* page.
const PARENT_KEY: &str = "98400";
/// The content id Confluence gives the page this battery creates.
const CREATED_KEY: &str = "98411";
/// The space the parent is in, in Confluence's own spelling.
const SPACE: &str = "ENG";

fn day() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 9, 3).expect("a date")
}

fn parent_id() -> String {
    format!("{WIKI}:{PARENT_KEY}")
}

fn created_page_id() -> String {
    format!("{WIKI}:{CREATED_KEY}")
}

fn target() -> PublishTarget {
    PublishTarget {
        source_id: WIKI.to_owned(),
        parent: parent_id(),
    }
}

// -- the fake wiki ----------------------------------------------------------

/// What the fake source has been told to do and what it was asked to do.
#[derive(Default)]
struct Log {
    /// Every op identifier written, in order -- the assertion surface for
    /// "knobas queued one create, not two".
    written: Vec<String>,
    /// Every `CreatePage` body sent, so the storage-format translation is
    /// witnessed on the wire rather than only in its own unit test.
    pages: Vec<(String, String, String)>,
    /// Items the next sync will push.
    pending: Vec<SyncItem>,
    /// While true, `write` refuses the way an unreachable server does.
    offline: bool,
}

/// A source that behaves like the Confluence adapter at the trait.
struct Wiki {
    id: String,
    log: Arc<Mutex<Log>>,
}

impl Wiki {
    fn log(&self) -> std::sync::MutexGuard<'_, Log> {
        self.log.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[async_trait]
impl Source for Wiki {
    fn descriptor(&self) -> SourceDescriptor {
        descriptor_for(&self.id)
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
        sink: &mut (dyn Sink + Send),
    ) -> Result<Cursor, SourceError> {
        let pending = std::mem::take(&mut self.log().pending);
        for item in pending {
            sink.item(item).await?;
        }
        Ok(cursor.unwrap_or_default())
    }

    async fn write(&self, op: WriteOp) -> Result<WriteReceipt, SourceError> {
        {
            let mut log = self.log();
            log.written.push(op.identifier().to_owned());
            if log.offline {
                return Err(SourceError::Unreachable("the wiki is down".to_owned()));
            }
        }
        match op {
            WriteOp::CreatePage {
                space, title, body, ..
            } => {
                self.log().pages.push((space, title.clone(), body.clone()));
                // The page exists at the source now, and will reach the mirror
                // at the next sync -- which is the ordering the feature has to
                // survive, so the fake keeps the two apart.
                self.log().pending.push(page_item(CREATED_KEY, &title, ""));
                Ok(WriteReceipt::id(CREATED_KEY))
            }
            // The real Jira drops the key it was given, on `WriteReceipt`'s own
            // reasoning: a created ticket is a mirrored entity and the mirror is
            // the answer that survives a re-send. So does this.
            WriteOp::CreateTicket { title, .. } => {
                self.log().pending.push(ticket_item("PAY-999", &title));
                Ok(WriteReceipt::none())
            }
            other => Err(SourceError::protocol(format!(
                "this source does not do {}",
                other.identifier()
            ))),
        }
    }
}

/// One mirrored page, shaped like a Confluence record: the space key is where
/// `protocol::space_of` reads it.
fn page_item(key: &str, title: &str, ancestors: &str) -> SyncItem {
    SyncItem {
        entity: knobas_core::entity::EntityRef::new(WIKI, key),
        kind: "page".to_owned(),
        title: title.to_owned(),
        body_text: ancestors.to_owned(),
        author: None,
        updated_at: Some(chrono::Utc::now()),
        payload: serde_json::json!({ "id": key, "space": { "key": SPACE } }),
        web_url: None,
        deleted: false,
    }
}

fn ticket_item(key: &str, title: &str) -> SyncItem {
    SyncItem {
        entity: knobas_core::entity::EntityRef::new(TRACKER, key),
        kind: "ticket".to_owned(),
        title: title.to_owned(),
        body_text: String::new(),
        author: None,
        updated_at: Some(chrono::Utc::now()),
        payload: serde_json::json!({ "key": key }),
        web_url: None,
        deleted: false,
    }
}

fn kind(id: &str) -> KindInfo {
    KindInfo {
        id: id.to_owned(),
        label: id.to_owned(),
        plural: id.to_owned(),
        monogram: "P".to_owned(),
        full_sync_exhaustive: true,
    }
}

/// The descriptor an instance answers with -- the tracker's for the tracker,
/// a wiki's otherwise. It has to match what the registry lists: the sink
/// checks an item's kind against the source's own declaration, so a tracker
/// answering a wiki's descriptor would drop every ticket it synced.
fn descriptor_for(id: &str) -> SourceDescriptor {
    if id == TRACKER {
        tracker_descriptor()
    } else {
        wiki_descriptor(id)
    }
}

fn wiki_descriptor(id: &str) -> SourceDescriptor {
    SourceDescriptor {
        id: id.to_owned(),
        adapter_kind: "confluence".to_owned(),
        name: id.to_owned(),
        capabilities: vec![Capability::Write],
        adapter_version: "0".to_owned(),
        auth_methods: vec![AuthMethod::Pat],
        accepts_account: false,
        write_ops: vec!["create_page".to_owned()],
        entity_kinds: vec![kind("page")],
        config_schema: serde_json::json!({"type": "object", "properties": {}}),
        payload_paths: Vec::new(),
    }
}

fn tracker_descriptor() -> SourceDescriptor {
    SourceDescriptor {
        id: TRACKER.to_owned(),
        adapter_kind: "jira".to_owned(),
        name: TRACKER.to_owned(),
        capabilities: vec![Capability::Write],
        adapter_version: "0".to_owned(),
        auth_methods: vec![AuthMethod::Pat],
        accepts_account: false,
        write_ops: vec!["create_ticket".to_owned()],
        entity_kinds: vec![kind("ticket")],
        config_schema: serde_json::json!({"type": "object", "properties": {}}),
        payload_paths: Vec::new(),
    }
}

/// The registry: three configured sources, all built over one shared log.
struct Registry(Arc<Mutex<Log>>);

impl AdapterRegistry for Registry {
    fn descriptors(&self) -> Vec<SourceDescriptor> {
        vec![
            wiki_descriptor(WIKI),
            wiki_descriptor(OTHER_WIKI),
            tracker_descriptor(),
        ]
    }

    fn build(&self, instance: SourceInstance) -> Result<Box<dyn Source>, SourceError> {
        Ok(Box::new(Wiki {
            id: instance.id,
            log: Arc::clone(&self.0),
        }))
    }
}

// -- the harness ------------------------------------------------------------

struct Quiet;

impl knobas_sync::scheduler::SyncEvents for Quiet {
    fn sync_state(&self, _status: knobas_sync::SourceSyncStatus) {}
    fn source_health(&self, _health: knobas_sync::CredentialHealth) {}
    fn activity_new(&self, _row: knobas_core::activity::ActivityRow) {}
}

struct Connections(knobas_db::embedded::Connector);

#[async_trait]
impl knobas_sync::scheduler::RunConnections for Connections {
    async fn open(&self) -> Result<sqlx::PgConnection, sqlx::Error> {
        self.0.connect().await
    }
}

/// A sink that resolves when the run it is watching ends -- the shape
/// `status_move.rs` records: a run that has not finished is a mirror that has
/// not moved, and a sleep would be flaky in the direction that gets a test
/// deleted.
struct Ending {
    done: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
}

impl knobas_sync::progress::ProgressSink for Ending {
    fn report(&self, progress: knobas_sync::progress::SyncProgress) {
        use knobas_sync::progress::SyncPhase;
        if !matches!(progress.phase, SyncPhase::Finished | SyncPhase::Failed) {
            return;
        }
        if let Some(sender) = self.done.lock().unwrap_or_else(|e| e.into_inner()).take() {
            let _ = sender.send(());
        }
    }
}

struct Harness {
    state: SourcesState,
    log: Arc<Mutex<Log>>,
}

impl Harness {
    /// A scratch database with the three sources configured and the parent
    /// page already in the mirror -- which is where a reader picks it.
    async fn new(name: &str) -> Self {
        let connector = knobas_db::test_util::scratch_database(name).await;
        let pool = connector.pool(4).await.expect("a pool");
        let log = Arc::new(Mutex::new(Log::default()));
        let secrets = Arc::new(MemoryStore::new());

        for id in [WIKI, OTHER_WIKI, TRACKER] {
            config::insert(
                &pool,
                &InsertConfig {
                    id: id.to_owned(),
                    adapter_kind: if id == TRACKER { "jira" } else { "confluence" }.to_owned(),
                    display_name: id.to_owned(),
                    base_url: "https://wiki.invalid".to_owned(),
                    auth_kind: AuthKind::Method(AuthMethod::Pat),
                    config: serde_json::json!({}),
                    sync_interval_secs: 86_400,
                    enabled: true,
                },
            )
            .await
            .expect("the source row is written");
            secrets
                .put(id, &knobas_secrets::Secret::just(AuthMethod::Pat, "token"))
                .expect("a credential");
        }

        let registry = Arc::new(Registry(Arc::clone(&log)));
        let scheduler = Scheduler::start(SchedulerDeps {
            pool: pool.clone(),
            connections: Arc::new(Connections(connector)),
            registry: registry.clone(),
            secrets: secrets.clone(),
            events: Arc::new(Quiet),
            timing: knobas_sync::scheduler::SchedulerTiming::default(),
        })
        .await
        .expect("a scheduler");

        let harness = Self {
            state: SourcesState {
                pool,
                scheduler,
                secrets,
                registry,
            },
            log,
        };
        harness.mirror(page_item(PARENT_KEY, "Standup protocols", ""));
        harness.sync(WIKI).await;
        harness
    }

    fn pool(&self) -> &PgPool {
        &self.state.pool
    }

    fn log(&self) -> std::sync::MutexGuard<'_, Log> {
        self.log.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Hand an item to the fake source, to be pushed at the next sync.
    fn mirror(&self, item: SyncItem) {
        self.log().pending.push(item);
    }

    async fn sync(&self, source: &str) {
        let (done, wait) = tokio::sync::oneshot::channel();
        let sink = Arc::new(Ending {
            done: Mutex::new(Some(done)),
        });
        self.state
            .scheduler
            .trigger(source, knobas_sync::SyncTrigger::Manual, Some(sink))
            .await
            .expect("the run starts");
        wait.await.expect("the run reports its ending");
    }

    async fn open(&self) -> knobas_app::protocol::Protocol {
        standup_protocol_inner(self.pool(), day())
            .await
            .expect("the protocol opens")
    }

    async fn publish(
        &self,
        target: Option<PublishTarget>,
    ) -> Result<knobas_app::protocol::Protocol, knobas_app::IpcError> {
        publish_standup_protocol_inner(&self.state, day(), target).await
    }

    /// Every link the note takes part in, as `(from, to, relation)`.
    async fn links(&self, note_id: &str) -> Vec<(String, String, String)> {
        sqlx::query_as::<_, (String, String, String)>(
            "select from_id, to_id, relation from knobas.confirmed_link
              where from_id = $1 or to_id = $1
              order by id",
        )
        .bind(note_id)
        .fetch_all(self.pool())
        .await
        .expect("the links read")
    }

    /// How many notes carry this date's title -- the get-or-create's whole
    /// question, asked of the database rather than of the return value.
    async fn notes_titled(&self) -> i64 {
        sqlx::query_scalar::<_, i64>("select count(*) from knobas.note where title = $1")
            .bind(protocol::title_of(day()))
            .fetch_one(self.pool())
            .await
            .expect("the count reads")
    }

    async fn create_pages_queued(&self) -> i64 {
        sqlx::query_scalar::<_, i64>(
            "select count(*) from knobas.write_queue where op = 'create_page'",
        )
        .fetch_one(self.pool())
        .await
        .expect("the count reads")
    }

    async fn save_note(&self, note_id: &str, body: &str) {
        knobas_app::commands::entity::save_note_inner(
            self.pool(),
            note_id,
            &protocol::title_of(day()),
            body,
        )
        .await
        .expect("the note saves");
    }
}

// -- get-or-create ----------------------------------------------------------

/// Story 64, and the acceptance criterion's own words: *the second open
/// returns the same note rather than a new one with the same title.*
///
/// The count is asked of the **database**, not of the two answers: two calls
/// that each made a note would still return a note each, with the right title
/// and the right body, and every assertion about the return value would pass
/// while the reader's week filled up with duplicates.
#[tokio::test(flavor = "multi_thread")]
async fn opening_a_date_twice_lands_in_one_note() {
    let harness = Harness::new("protocol_get_or_create").await;

    let first = harness.open().await;
    assert_eq!(harness.notes_titled().await, 1, "the first open makes one");
    assert!(first.note_id.starts_with("note:"), "{}", first.note_id);
    assert!(first.publication.is_none(), "nothing published yet");

    // What the reader typed into it. If the second open made a new note, this
    // is the sentence that would vanish.
    harness
        .save_note(&first.note_id, "## Attendees\n\n- Mara\n")
        .await;

    let second = harness.open().await;
    assert_eq!(second.note_id, first.note_id, "the same note, not a twin");
    assert_eq!(harness.notes_titled().await, 1, "and still only one");

    let note = knobas_core::note::get(
        harness.pool(),
        &knobas_core::entity::EntityRef::parse(&second.note_id).unwrap(),
    )
    .await
    .expect("the note reads")
    .expect("the note is there");
    assert!(note.body_md.contains("Mara"), "{}", note.body_md);
}

/// The template is what a protocol starts as, so the view has the three
/// sections to draw before anybody types.
#[tokio::test(flavor = "multi_thread")]
async fn a_new_protocol_starts_from_the_template() {
    let harness = Harness::new("protocol_template").await;
    let opened = harness.open().await;
    let note = knobas_core::note::get(
        harness.pool(),
        &knobas_core::entity::EntityRef::parse(&opened.note_id).unwrap(),
    )
    .await
    .expect("the note reads")
    .expect("the note is there");
    assert_eq!(note.body_md, protocol::template_of(day()));
    assert_eq!(note.title, "Standup 2026-09-03");
    assert_eq!(opened.page_title, "2026-09-03");
}

// -- publishing -------------------------------------------------------------

/// The whole of story 65 and 66 in one pass: the op goes out carrying the
/// note's body **as storage format**, the settle names the page, and the link
/// exists.
///
/// The link is asserted **from both ends**, which is what story 66 -- *either
/// detail shows the other* -- actually asks for. A `from_id = note` filter
/// alone would pass against a link drawn one way, and the two details read
/// this table from opposite sides.
#[tokio::test(flavor = "multi_thread")]
async fn publishing_creates_the_page_and_links_it_both_ways() {
    let harness = Harness::new("protocol_publish").await;
    let opened = harness.open().await;
    harness
        .save_note(
            &opened.note_id,
            "## Attendees\n\n- Mara\n\n## Action items\n\n- [ ] Ask Ines\n",
        )
        .await;

    let published = harness.publish(Some(target())).await.expect("it publishes");
    let publication = published.publication.expect("there is a publication");
    assert_eq!(publication.state, WriteState::Sent, "the write went");
    assert_eq!(
        publication.page_entity_id.as_deref(),
        Some(created_page_id().as_str()),
        "the page id is the source id and the id the source gave it"
    );
    assert!(publication.linked, "and the two are linked");

    // What crossed the wire: the space key off the parent's own record, the
    // date as the title, and the body as storage format rather than markdown.
    let (space, title, body) = harness.log().pages.first().cloned().expect("one page");
    assert_eq!(space, SPACE);
    assert_eq!(title, "2026-09-03");
    assert_eq!(
        body,
        "<h2>Attendees</h2><ul><li>Mara</li></ul><h2>Action items</h2>\
         <ul><li>\u{2610} Ask Ines</li></ul>",
        "the page is stored in the dialect Confluence stores pages in"
    );

    // Both directions, from the one row. `entries_of` is undirected and both
    // details read it, so a link that exists at all shows on both -- and this
    // asserts it as the two details ask it, from each end in turn.
    let links = harness.links(&opened.note_id).await;
    assert_eq!(
        links,
        [(
            opened.note_id.clone(),
            created_page_id(),
            protocol::PUBLISHED_RELATION.to_owned()
        )],
        "one link, note to page"
    );
    let from_the_page = sqlx::query_as::<_, (String, String)>(
        "select from_id, to_id from knobas.confirmed_link where to_id = $1 or from_id = $1",
    )
    .bind(created_page_id())
    .fetch_all(harness.pool())
    .await
    .expect("the page's links read");
    assert_eq!(
        from_the_page,
        [(opened.note_id.clone(), created_page_id())],
        "the page's own detail finds the note"
    );
}

/// The ruling this ticket was asked to make: **a second publish for one date
/// queues nothing**.
///
/// Delivery is at-least-once and a re-sent `CreatePage` has no version check,
/// so the layer knobas owns is refusing to ask twice. The assertion is on the
/// queue's depth and on what the source was asked to do -- a publish that
/// answered with the first publication while quietly queuing a second write
/// would pass every assertion about the return value.
#[tokio::test(flavor = "multi_thread")]
async fn publishing_a_date_twice_queues_one_page() {
    let harness = Harness::new("protocol_publish_twice").await;
    harness.open().await;

    let first = harness.publish(Some(target())).await.expect("it publishes");
    let again = harness.publish(None).await.expect("it answers");

    assert_eq!(
        harness.create_pages_queued().await,
        1,
        "one write, however many times the button is pressed"
    );
    assert_eq!(
        harness.log().written,
        ["create_page"],
        "the source was asked once"
    );
    assert_eq!(
        again.publication.expect("a publication").write_id,
        first.publication.expect("a publication").write_id,
        "the second press points at the first publication"
    );
}

/// A page of the same title in **another Confluence** is not this date's
/// publication.
///
/// The publication is scoped to the source the target names, because the
/// collision the ruling is about is Confluence's own -- a title is unique
/// within a space, and a space lives in one instance. A match on the title
/// alone would let a `2026-09-03` in a second wiki suppress publishing here,
/// which is a *refusal to publish at all* rather than a duplicate avoided.
#[tokio::test(flavor = "multi_thread")]
async fn a_page_of_this_title_in_another_wiki_is_not_this_publication() {
    let harness = Harness::new("protocol_other_wiki").await;
    harness.open().await;

    // The other wiki's parent, and a create_page under it carrying this date
    // as its title -- the row the old title-only match would have found.
    harness.mirror(SyncItem {
        entity: knobas_core::entity::EntityRef::new(OTHER_WIKI, PARENT_KEY),
        kind: "page".to_owned(),
        title: "Standup protocols".to_owned(),
        body_text: String::new(),
        author: None,
        updated_at: Some(chrono::Utc::now()),
        payload: serde_json::json!({ "id": PARENT_KEY, "space": { "key": SPACE } }),
        web_url: None,
        deleted: false,
    });
    harness.sync(OTHER_WIKI).await;
    knobas_app::commands::entity::publish_standup_protocol_inner(
        &harness.state,
        day(),
        Some(PublishTarget {
            source_id: OTHER_WIKI.to_owned(),
            parent: format!("{OTHER_WIKI}:{PARENT_KEY}"),
        }),
    )
    .await
    .expect("the other wiki publishes");
    assert_eq!(harness.create_pages_queued().await, 1);

    // Now the reader points the target back at this wiki and publishes. The
    // other wiki's page is not this one's publication.
    harness.publish(Some(target())).await.expect("it publishes");
    assert_eq!(
        harness.create_pages_queued().await,
        2,
        "two Confluences, two pages -- neither suppresses the other"
    );
    let sources: Vec<String> = sqlx::query_scalar(
        "select source_id from knobas.write_queue where op = 'create_page' order by id",
    )
    .fetch_all(harness.pool())
    .await
    .expect("the sources read");
    assert_eq!(sources, [OTHER_WIKI.to_owned(), WIKI.to_owned()]);
}

/// The same refusal while the write is still **waiting**, which is the case
/// that would actually bite: an offline Confluence, a reader who presses
/// *Publish* again because nothing seems to have happened.
#[tokio::test(flavor = "multi_thread")]
async fn a_publication_still_waiting_is_not_published_again() {
    let harness = Harness::new("protocol_publish_waiting").await;
    harness.log().offline = true;
    harness.open().await;

    let first = harness.publish(Some(target())).await.expect("it queues");
    assert_eq!(
        first.publication.expect("a publication").state,
        WriteState::Pending,
        "the wiki is down, so the write waits"
    );

    harness.publish(None).await.expect("it answers");
    assert_eq!(
        harness.create_pages_queued().await,
        1,
        "a write the queue still owes is a publication, not a gap"
    );
}

/// The other half of the ruling: a write that was **refused** left nothing
/// behind, so a fresh publish is right rather than blocked forever.
#[tokio::test(flavor = "multi_thread")]
async fn a_refused_publication_can_be_published_again() {
    let harness = Harness::new("protocol_publish_after_refusal").await;
    let opened = harness.open().await;
    harness.publish(Some(target())).await.expect("it publishes");

    // The queue's own terminal refusal, as a source's "no" leaves it.
    // `write_queue_settled_chk`: only a sent or discarded row carries a
    // `settled_at`, so a refusal leaves it null.
    sqlx::query(
        "update knobas.write_queue
            set state = 'refused', detail = 'no', settled_at = null, remote_id = null",
    )
    .execute(harness.pool())
    .await
    .expect("the refusal is recorded");

    let again = harness.publish(None).await.expect("it publishes again");
    assert_eq!(
        harness.create_pages_queued().await,
        2,
        "nothing landed, so asking again is the right thing"
    );
    assert_eq!(again.note_id, opened.note_id, "and it is the same protocol");
}

/// Story 67, the direction a dialog-only test cannot witness: the setting is
/// what a **later** publish reads.
///
/// The first publish is given the target and stores it; the settings command
/// then moves it; and the next publish -- given nothing -- lands under the new
/// parent. Without the second half, "the dialog writes the setting" and "the
/// publish reads the setting" are two claims with a gap between them that no
/// test crosses.
#[tokio::test(flavor = "multi_thread")]
async fn the_stored_target_is_what_the_next_publish_uses() {
    let harness = Harness::new("protocol_target").await;
    harness.open().await;
    harness.publish(Some(target())).await.expect("it publishes");
    assert_eq!(
        protocol::publish_target(harness.pool())
            .await
            .expect("the setting reads")
            .expect("the first publish stored it"),
        target(),
        "the dialog's answer became the setting"
    );

    // Settings moves it to another parent in the same wiki.
    harness.mirror(page_item("77000", "Team space", ""));
    harness.sync(WIKI).await;
    let moved = PublishTarget {
        source_id: WIKI.to_owned(),
        parent: format!("{WIKI}:77000"),
    };
    protocol::set_publish_target(harness.pool(), &moved)
        .await
        .expect("the setting is changed");

    // Tomorrow's protocol -- a fresh date, so the duplicate rule is not what
    // is being measured.
    let tomorrow = day().succ_opt().expect("the next day");
    publish_standup_protocol_inner(&harness.state, tomorrow, None)
        .await
        .expect("it publishes");

    let parents: Vec<String> = sqlx::query_scalar(
        "select payload->'CreatePage'->>'parent' from knobas.write_queue
          where op = 'create_page' order by id",
    )
    .fetch_all(harness.pool())
    .await
    .expect("the parents read");
    assert_eq!(
        parents,
        [parent_id(), format!("{WIKI}:77000")],
        "the second publish went under the parent settings named"
    );
}

/// Publishing before anybody has said where is a refusal with a sentence, not
/// a guess at whichever Confluence happens to be first.
#[tokio::test(flavor = "multi_thread")]
async fn publishing_with_no_target_refuses_rather_than_guessing() {
    let harness = Harness::new("protocol_no_target").await;
    let refused = harness.publish(None).await.expect_err("it refuses");
    assert_eq!(refused.code, knobas_app::IpcErrorCode::Invalid);
    assert!(
        refused.message.contains("where to publish"),
        "{}",
        refused.message
    );
    assert_eq!(harness.create_pages_queued().await, 0, "and queued nothing");
}

/// A target whose parent is in another source's address space would route the
/// write at a wiki that cannot see the page. §4.1 makes the two one string, so
/// there is one answer and it is checked.
#[tokio::test(flavor = "multi_thread")]
async fn a_parent_in_another_wikis_address_space_is_refused() {
    let harness = Harness::new("protocol_wrong_namespace").await;
    let crossed = PublishTarget {
        source_id: OTHER_WIKI.to_owned(),
        parent: parent_id(),
    };
    let refused = harness
        .publish(Some(crossed))
        .await
        .expect_err("it refuses");
    assert_eq!(refused.code, knobas_app::IpcErrorCode::Invalid);
    assert!(
        refused.message.contains("address space"),
        "{}",
        refused.message
    );
}

/// ADR-0007 requirement 3 for `protocol::space_of`, direction one: **a parent
/// the mirror does not hold misses to a refusal, not to a guess.**
///
/// `space_of` is a payload read outside an adapter, so the ADR binds it: it
/// must miss rather than guess, sit in one named statement, and have its
/// failure direction *pinned by a test*. The doc states both directions; these
/// two tests are what turn that from an author's habit into an obligation.
///
/// The direction that would bite is a page the reader configured as the parent
/// and the source has since removed: a read that fell back to a default space
/// would publish the team's standup into a space nobody chose.
#[tokio::test(flavor = "multi_thread")]
async fn a_parent_the_mirror_does_not_hold_refuses_rather_than_guessing_a_space() {
    let harness = Harness::new("protocol_parent_gone").await;
    let refused = harness
        .publish(Some(PublishTarget {
            source_id: WIKI.to_owned(),
            parent: format!("{WIKI}:99999"),
        }))
        .await
        .expect_err("it refuses");
    assert_eq!(refused.code, knobas_app::IpcErrorCode::NotFound);
    assert!(
        refused.message.contains("not in the mirror"),
        "{}",
        refused.message
    );
    assert_eq!(harness.create_pages_queued().await, 0, "and queued nothing");
}

/// The same requirement, direction two: **a parent record that names no space
/// is refused**, rather than published into a space read off something else.
///
/// A Confluence page record always carries its space, so this is the shape the
/// ADR's "unrecognized shape" clause is about -- a second wiki spelling it
/// differently, or a record widened later. The read contributes nothing and
/// the publish stops; nothing reaches the queue.
#[tokio::test(flavor = "multi_thread")]
async fn a_parent_record_that_names_no_space_is_refused() {
    let harness = Harness::new("protocol_parent_spaceless").await;
    harness.mirror(SyncItem {
        entity: knobas_core::entity::EntityRef::new(WIKI, "98401"),
        kind: "page".to_owned(),
        title: "Standup protocols".to_owned(),
        body_text: String::new(),
        author: None,
        updated_at: Some(chrono::Utc::now()),
        // The one field this read wants, absent.
        payload: serde_json::json!({ "id": "98401" }),
        web_url: None,
        deleted: false,
    });
    harness.sync(WIKI).await;

    let refused = harness
        .publish(Some(PublishTarget {
            source_id: WIKI.to_owned(),
            parent: format!("{WIKI}:98401"),
        }))
        .await
        .expect_err("it refuses");
    assert_eq!(refused.code, knobas_app::IpcErrorCode::Invalid);
    assert!(
        refused.message.contains("which space"),
        "{}",
        refused.message
    );
    assert_eq!(harness.create_pages_queued().await, 0, "and queued nothing");
}

// -- the settle after a restart ---------------------------------------------

/// The acceptance criterion's hardest question: *does the settle path record
/// the page id when the write settles **after a restart**?*
///
/// The receipt is gone by then -- it lives for the length of one flush -- so
/// the id can only come from `knobas.write_queue.remote_id`, which the settle
/// wrote. This drives exactly that: the publish happens while the wiki is
/// down, so nothing settles and nothing links; the wiki comes back and the
/// **queue** flushes it, with no protocol code anywhere near the flush, which
/// is what a scheduler tick or a relaunch looks like from here; and then the
/// ordinary read -- the one the standup view makes on open -- draws the link.
///
/// If the id were taken from the receipt, or the link drawn only by the call
/// that pressed *Publish*, this note would still be unlinked at the end.
#[tokio::test(flavor = "multi_thread")]
async fn a_write_that_settles_later_is_linked_at_the_next_read() {
    let harness = Harness::new("protocol_settles_later").await;
    harness.log().offline = true;
    let opened = harness.open().await;

    let queued = harness.publish(Some(target())).await.expect("it queues");
    let queued = queued.publication.expect("a publication");
    assert_eq!(queued.state, WriteState::Pending);
    assert!(queued.page_entity_id.is_none(), "nothing has settled");
    assert!(!queued.linked, "so nothing is linked");

    // The wiki comes back and the *queue* sends it. Nothing in this block is
    // the protocol's code: this is the scheduler's own flush, which is what
    // happens while the standup view is not on screen -- or while the app is
    // not running at all.
    harness.log().offline = false;
    knobas_sync::write_queue::flush_source(harness.state.scheduler.deps(), WIKI)
        .await
        .expect("the queue flushes");
    harness.sync(WIKI).await;

    let reopened = harness.open().await;
    let publication = reopened.publication.expect("a publication");
    assert_eq!(publication.state, WriteState::Sent);
    assert_eq!(
        publication.page_entity_id.as_deref(),
        Some(created_page_id().as_str()),
        "the id came off the queue row, which is the only place it still exists"
    );
    assert!(publication.linked, "and the read drew the link");
    assert_eq!(
        harness.links(&opened.note_id).await,
        [(
            opened.note_id.clone(),
            created_page_id(),
            protocol::PUBLISHED_RELATION.to_owned()
        )]
    );
}

/// The step before that one, isolated: a write that settled while the page is
/// **not in the mirror yet** records the id and draws no link -- rather than
/// failing, or claiming a link that has no rows behind it.
///
/// `knobas.link`'s endpoints are `knobas.entity` rows, so this is not a
/// nicety: a link drawn the moment the receipt arrived would be a foreign-key
/// violation on the reader's screen.
#[tokio::test(flavor = "multi_thread")]
async fn a_page_the_mirror_has_not_seen_yet_is_named_but_not_linked() {
    let harness = Harness::new("protocol_before_the_sync").await;
    harness.open().await;
    harness.publish(Some(target())).await.expect("it publishes");

    // Undo the mirroring the publish's own re-sync did, leaving the queue row
    // exactly as it was: settled, with the id, and no page to point at.
    sqlx::query("delete from sync.item where entity_id = $1")
        .bind(created_page_id())
        .execute(harness.pool())
        .await
        .expect("the mirror row goes");
    sqlx::query("delete from knobas.link")
        .execute(harness.pool())
        .await
        .expect("the link goes");
    sqlx::query("delete from knobas.entity where id = $1")
        .bind(created_page_id())
        .execute(harness.pool())
        .await
        .expect("the entity row goes");

    let reopened = harness.open().await;
    let publication = reopened.publication.expect("a publication");
    assert_eq!(
        publication.page_entity_id.as_deref(),
        Some(created_page_id().as_str()),
        "the source named the page, and that is recorded"
    );
    assert!(
        !publication.linked,
        "but there is nothing to link to yet, and saying otherwise would be a lie"
    );
}

// -- action items -----------------------------------------------------------

/// Story 69: the existing op files the ticket, and the ticket is linked to the
/// protocol.
#[tokio::test(flavor = "multi_thread")]
async fn an_action_item_becomes_a_ticket_linked_to_the_protocol() {
    let harness = Harness::new("protocol_action_item").await;
    let opened = harness.open().await;

    let filed = create_action_item_ticket_inner(
        &harness.state,
        &opened.note_id,
        &format!("{TRACKER}:PAY"),
        "Task",
        "Ask Ines about the SEPA retry",
        "From the standup on 2026-09-03.",
    )
    .await
    .expect("the ticket is filed");

    assert_eq!(
        harness.log().written,
        ["create_ticket"],
        "the existing op, and one of it"
    );
    assert_eq!(
        filed.ticket_entity_id.as_deref(),
        Some("tracker:PAY-999"),
        "found by reading the mirror, which is the only answer a create gives"
    );
    assert!(filed.linked);
    assert_eq!(
        harness.links(&opened.note_id).await,
        [(
            opened.note_id.clone(),
            "tracker:PAY-999".to_owned(),
            protocol::ACTION_ITEM_RELATION.to_owned()
        )]
    );
}

/// An action item with no words is not a ticket, and the refusal comes before
/// anything is queued -- a ticket called nothing is not undoable from knobas.
#[tokio::test(flavor = "multi_thread")]
async fn an_empty_action_item_files_nothing() {
    let harness = Harness::new("protocol_empty_action_item").await;
    let opened = harness.open().await;
    let refused = create_action_item_ticket_inner(
        &harness.state,
        &opened.note_id,
        &format!("{TRACKER}:PAY"),
        "Task",
        "   ",
        "",
    )
    .await
    .expect_err("it refuses");
    assert_eq!(refused.code, knobas_app::IpcErrorCode::Invalid);
    assert!(harness.log().written.is_empty(), "and sent nothing");
}

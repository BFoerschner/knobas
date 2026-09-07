//! The attachment battery (issue #453, M4.1): a monitor name an asset carries
//! becomes a `monitored-by` link on the poll that mirrors the monitor.
//!
//! Spec #427, *Import*: a monitor name *"the mirror does not hold yet is kept
//! on the asset and resolved by the next import or the M4.1 sync"*. The import
//! half is `knobas_app::assets`' `monitor_plan` and has its own battery; this
//! is the sync half.
//!
//! **Driven through the engine, never through the resolver.** Every test here
//! runs `knobas_sync::run_once` and then reads `knobas.link`, because the
//! claim is about *a poll*: a test that called `attach::resolve` directly
//! would stay green with the call site deleted from `run_locked`, which is the
//! one failure this file exists to catch. `tests/alerts.rs` records the same
//! reasoning for the same reason.
//!
//! **Every test gets a database of its own**, that file's arrangement.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use knobas_source::{
    Capability, ConnectionInfo, Cursor, KindInfo, Sink, Source, SourceDescriptor, SourceError,
    SyncItem, WriteOp,
};
use knobas_sync::samples;
use sqlx::{PgPool, Row};

/// An adapter publishing whichever monitors it is currently told to, by name.
///
/// `tests/alerts.rs`' `Monitors` narrowed to what this file asserts on: what
/// matters here is a monitor's **title**, since the title is what a name
/// resolves against, and its state is never read.
struct Monitors {
    id: String,
    /// `(key, title)`. Two fields, because the whole point of the resolution
    /// is that it matches on the *title* -- a fixture whose key and title were
    /// the same string could not tell a rule that matched the wrong one.
    corpus: Arc<Mutex<Vec<(&'static str, String)>>>,
    kind: &'static str,
}

#[async_trait]
impl Source for Monitors {
    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: self.id.clone(),
            adapter_kind: "monitors".into(),
            name: "Monitors".into(),
            capabilities: Vec::<Capability>::new(),
            adapter_version: "0.1.0".into(),
            auth_methods: Vec::new(),
            accepts_account: false,
            write_ops: Vec::new(),
            entity_kinds: vec![KindInfo {
                id: self.kind.to_owned(),
                label: self.kind.to_owned(),
                plural: self.kind.to_owned(),
                monogram: "MO".into(),
                full_sync_exhaustive: true,
            }],
            config_schema: serde_json::json!({ "type": "object", "properties": {} }),
            payload_paths: vec![knobas_source::KindPaths {
                kind: self.kind.to_owned(),
                status_name: vec![knobas_source::PayloadPath::of(["state"])],
                ..knobas_source::KindPaths::default()
            }],
        }
    }

    async fn test_connection(&self) -> Result<ConnectionInfo, SourceError> {
        Ok(ConnectionInfo::default())
    }

    async fn sync(
        &self,
        _cursor: Option<Cursor>,
        sink: &mut (dyn Sink + Send),
    ) -> Result<Cursor, SourceError> {
        let corpus = self.corpus.lock().unwrap().clone();
        for (key, title) in &corpus {
            sink.item(SyncItem {
                entity: knobas_core::entity::EntityRef::new(&self.id, key),
                kind: self.kind.to_owned(),
                title: title.clone(),
                body_text: String::new(),
                author: None,
                updated_at: None,
                payload: serde_json::json!({ "id": key, "state": "up" }),
                web_url: None,
                deleted: false,
            })
            .await?;
        }
        Ok(format!(r#"{{"n":{}}}"#, corpus.len()))
    }

    async fn write(&self, _op: WriteOp) -> Result<knobas_source::WriteReceipt, SourceError> {
        Err(SourceError::protocol("read-only"))
    }
}

type Corpus = Arc<Mutex<Vec<(&'static str, String)>>>;

fn source_of(
    id: &str,
    kind: &'static str,
    monitors: &[(&'static str, &str)],
) -> (Monitors, Corpus) {
    let corpus: Corpus = Arc::new(Mutex::new(
        monitors
            .iter()
            .map(|(key, title)| (*key, (*title).to_owned()))
            .collect(),
    ));
    (
        Monitors {
            id: id.to_owned(),
            corpus: Arc::clone(&corpus),
            kind,
        },
        corpus,
    )
}

fn source(id: &str, monitors: &[(&'static str, &str)]) -> (Monitors, Corpus) {
    source_of(id, samples::KIND, monitors)
}

async fn pool(label: &str) -> PgPool {
    knobas_db::test_util::scratch_database(label)
        .await
        .pool(2)
        .await
        .expect("a pool on the scratch database")
}

/// One asset, with the monitor names an import or a create left on it.
///
/// Written with SQL rather than through `knobas_app::assets::create`, because
/// this crate does not see the app crate at all -- and because what the rule
/// reads is two columns, which is what a fixture should be about.
async fn asset(pool: &PgPool, id: &str, name: &str, monitors: &[&str]) {
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'asset', $2)")
        .bind(id)
        .bind(name)
        .execute(pool)
        .await
        .expect("the entity row");
    sqlx::query(
        "insert into knobas.asset (id, type_id, name, monitors) values ($1, 'container', $2, $3)",
    )
    .bind(id)
    .bind(name)
    .bind(monitors)
    .execute(pool)
    .await
    .expect("the asset row");
}

/// Every active `monitored-by` link, as `(asset, monitor, origin, confirmed)`,
/// in a fixed order.
async fn attachments(pool: &PgPool) -> Vec<(String, String, String, bool)> {
    sqlx::query(
        "select from_id, to_id, origin, confirmed_at is not null as confirmed
           from knobas.link
          where deleted_at is null and relation = 'monitored-by'
          order by from_id, to_id",
    )
    .fetch_all(pool)
    .await
    .expect("the links are readable")
    .into_iter()
    .map(|row| {
        (
            row.get::<String, _>("from_id"),
            row.get::<String, _>("to_id"),
            row.get::<String, _>("origin"),
            row.get::<bool, _>("confirmed"),
        )
    })
    .collect()
}

/// Put monitor names on an asset that already exists -- what an import or a
/// create does *after* the estate is in place.
async fn names(pool: &PgPool, id: &str, monitors: &[&str]) {
    sqlx::query("update knobas.asset set monitors = $2 where id = $1")
        .bind(id)
        .bind(monitors)
        .execute(pool)
        .await
        .expect("the names are recorded");
}

/// Whether the mirror holds this entity as a tombstone.
///
/// Read as the raw join and not through `sync.live_item`, because what this
/// asserts is the difference between the two: the `sync.item` row is still
/// there and it is `knobas.entity.deleted_at` that has moved.
async fn tombstoned(pool: &PgPool, entity_id: &str) -> bool {
    sqlx::query(
        "select entity.deleted_at is not null as gone
           from sync.item item
           join knobas.entity entity on entity.id = item.entity_id
          where item.entity_id = $1",
    )
    .bind(entity_id)
    .fetch_one(pool)
    .await
    .expect("the mirror still holds the row")
    .get::<bool, _>("gone")
}

/// Stop publishing one monitor -- a Kuma pause, from `/metrics`.
fn pauses(corpus: &Corpus, key: &str) {
    corpus.lock().unwrap().retain(|(k, _)| *k != key);
}

/// Publish one more monitor, which is what a landed `create_monitor` looks
/// like from the next poll.
fn publishes(corpus: &Corpus, key: &'static str, title: &str) {
    corpus.lock().unwrap().push((key, title.to_owned()));
}

// --- the sequences a reader would recognise ---------------------------------

/// **The headline, and the ticket's own shape**: a name on an asset that the
/// mirror cannot answer yet, then a poll that mirrors a monitor called that,
/// and the link is drawn.
///
/// The first run is what makes the second one a fact about the *arrival*: a
/// test that only ran the second would pass with a rule that drew the link
/// from nothing but the name.
#[tokio::test]
async fn a_name_the_mirror_answers_to_becomes_a_link_on_that_poll() {
    let pool = pool("a_name_the_mirror_answer").await;
    let (src, corpus) = source("kuma", &[("1", "canary")]);
    asset(&pool, "asset:gitea", "knobas-gitea", &["gitea"]).await;

    knobas_sync::run_once(&pool, &src, None).await.unwrap();
    assert!(
        attachments(&pool).await.is_empty(),
        "a name no monitor answers to is not an attachment"
    );

    publishes(&corpus, "2", "gitea");
    knobas_sync::run_once(&pool, &src, None).await.unwrap();
    assert_eq!(
        attachments(&pool).await,
        vec![(
            "asset:gitea".to_owned(),
            "kuma:2".to_owned(),
            "imported".to_owned(),
            true
        )],
        "the poll that mirrored the monitor drew the link, asset to monitor, confirmed"
    );
}

/// The rule matches on the monitor's **title**, not on its key.
///
/// The direction that matters: an asset naming `2` -- which is the monitor's
/// *key* and a string a person could plausibly type -- gets nothing, and the
/// asset naming the title gets the link. A rule that matched the key would
/// pass every other test in this file, because a fixture whose key and title
/// agree cannot tell them apart.
#[tokio::test]
async fn the_name_is_matched_against_the_monitors_title() {
    let pool = pool("the_name_is_matched_agai").await;
    let (src, _corpus) = source("kuma", &[("2", "gitea")]);
    asset(&pool, "asset:by-title", "by title", &["gitea"]).await;
    asset(&pool, "asset:by-key", "by key", &["2"]).await;

    knobas_sync::run_once(&pool, &src, None).await.unwrap();

    assert_eq!(
        attachments(&pool).await,
        vec![(
            "asset:by-title".to_owned(),
            "kuma:2".to_owned(),
            "imported".to_owned(),
            true
        )],
        "only the asset naming the monitor's title is attached"
    );
}

/// Two polls draw one link, and the second poll is not an error.
///
/// The insert is over `0011`'s unordered unique index, so a resolution that
/// forgot its own guard would not draw a duplicate -- it would **fail the
/// run**, every minute, for as long as the name and the monitor both existed.
#[tokio::test]
async fn a_second_poll_draws_nothing_and_fails_nothing() {
    let pool = pool("a_second_poll_draws_noth").await;
    let (src, _corpus) = source("kuma", &[("2", "gitea")]);
    asset(&pool, "asset:gitea", "knobas-gitea", &["gitea"]).await;

    knobas_sync::run_once(&pool, &src, None).await.unwrap();
    knobas_sync::run_once(&pool, &src, None).await.unwrap();
    knobas_sync::run_once(&pool, &src, None).await.unwrap();

    assert_eq!(attachments(&pool).await.len(), 1, "one name, one link");
}

/// A link somebody drew **the other way round** is the same link, and the
/// resolution leaves it alone.
///
/// `0011` made the pair unordered, so a monitor-to-asset row is what a *Link
/// to…* drawn from the monitor's own detail leaves behind. A guard that
/// compared `from_id` and `to_id` in order would insert over it and fail the
/// run.
#[tokio::test]
async fn a_link_drawn_from_the_monitors_end_is_already_the_attachment() {
    let pool = pool("a_link_drawn_from_the_mo").await;
    let (src, _corpus) = source("kuma", &[("2", "gitea")]);
    asset(&pool, "asset:gitea", "knobas-gitea", &["gitea"]).await;
    // The monitor has to be in the mirror before a link may point at it.
    knobas_sync::run_once(&pool, &src, None).await.unwrap();
    sqlx::query("delete from knobas.link")
        .execute(&pool)
        .await
        .unwrap();
    knobas_core::link::create(
        &pool,
        &knobas_core::entity::EntityRef::parse("kuma:2").unwrap(),
        &knobas_core::entity::EntityRef::parse("asset:gitea").unwrap(),
        knobas_core::link::MONITORED_BY,
        knobas_core::link::Origin::Manual,
        None,
        "user",
    )
    .await
    .expect("a hand-drawn attachment, monitor first");

    knobas_sync::run_once(&pool, &src, None).await.unwrap();

    assert_eq!(
        attachments(&pool).await,
        vec![(
            "kuma:2".to_owned(),
            "asset:gitea".to_owned(),
            "manual".to_owned(),
            true
        )],
        "the hand-drawn link is untouched and no second one was drawn"
    );
}

/// An **unconfirmed proposal** over the same pair is left for the reader.
///
/// `monitor_url_host` (#478) proposes exactly this attachment from the URL's
/// host, and a proposal is an active `knobas.link` row: inserting over it
/// fails `link_pair_active_idx` and takes the whole poll down with it. So the
/// guard reads active rows rather than confirmed ones, and the consequence is
/// deliberate -- the suggestion stays in the tray, and accepting it is the
/// reader's own act (ADR: a suggestion is confirmed by a person, never by a
/// poll).
#[tokio::test]
async fn a_proposal_over_the_same_pair_is_left_alone() {
    let pool = pool("a_proposal_over_the_same").await;
    let (src, _corpus) = source("kuma", &[("2", "gitea")]);
    asset(&pool, "asset:gitea", "knobas-gitea", &["gitea"]).await;
    knobas_sync::run_once(&pool, &src, None).await.unwrap();
    sqlx::query("delete from knobas.link")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into knobas.link
             (from_id, to_id, relation, origin, created_by, confirmed_at, rule, rule_class, reason)
         values ($1, $2, 'monitored-by', 'suggested', 'knobas', null,
                 'monitor_url_host', 'source_relation', 'the monitor watches this host')",
    )
    .bind("asset:gitea")
    .bind("kuma:2")
    .execute(&pool)
    .await
    .expect("a proposal over the pair");

    knobas_sync::run_once(&pool, &src, None)
        .await
        .expect("a poll over a proposed pair is not a failed poll");

    assert_eq!(
        attachments(&pool).await,
        vec![(
            "asset:gitea".to_owned(),
            "kuma:2".to_owned(),
            "suggested".to_owned(),
            false
        )],
        "the proposal is still a proposal, and nothing was drawn over it"
    );
}

/// A monitor that has left the mirror attaches nothing, and the resume that
/// brings it back is what draws the link.
///
/// **The name arrives after the monitor is already tombstoned**, and that
/// sequence is the whole test rather than a convenience. A pause takes a
/// monitor out of `/metrics` entirely, so the sweep tombstones it while its
/// `sync.item` row stays -- which means a resolution reading `sync.item`
/// instead of `sync.live_item` would attach an asset to something knobas has
/// just been told is gone, and would do it in silence. Adding the name *before*
/// the pause cannot witness that: the poll before the pause would have drawn
/// the link already, and the two readings would agree.
#[tokio::test]
async fn a_tombstoned_monitor_is_not_attached_and_a_resumed_one_is() {
    let pool = pool("a_tombstoned_monitor_is_").await;
    let (src, corpus) = source("kuma", &[("1", "canary"), ("2", "gitea")]);
    asset(&pool, "asset:gitea", "knobas-gitea", &[]).await;

    // Mirrored, so the row exists -- and unnamed, so nothing is attached to it.
    knobas_sync::run_once(&pool, &src, None).await.unwrap();
    assert!(attachments(&pool).await.is_empty());

    // Now the name, and now the pause: the sweep tombstones the entity and
    // leaves the `sync.item` row where it is.
    names(&pool, "asset:gitea", &["gitea"]).await;
    pauses(&corpus, "2");
    knobas_sync::run_once(&pool, &src, None).await.unwrap();
    assert!(
        tombstoned(&pool, "kuma:2").await,
        "the fixture must actually tombstone the monitor, or this proves nothing"
    );
    assert!(
        attachments(&pool).await.is_empty(),
        "a monitor knobas has just been told is gone is not an attachment"
    );

    publishes(&corpus, "2", "gitea");
    knobas_sync::run_once(&pool, &src, None).await.unwrap();
    assert_eq!(
        attachments(&pool).await.len(),
        1,
        "the poll that found it again drew the link"
    );
}

/// Two monitors called the same thing attach the asset to both.
///
/// Uptime Kuma holds any number of monitors under one name, so this is a state
/// a real estate can reach -- and both are true attachments: the asset really
/// is watched by two checks called `gitea`. The alternative, picking one,
/// would be knobas choosing which of two live monitors counts.
#[tokio::test]
async fn two_monitors_of_one_name_are_two_attachments() {
    let pool = pool("two_monitors_of_one_name").await;
    let (src, _corpus) = source("kuma", &[("2", "gitea"), ("3", "gitea")]);
    asset(&pool, "asset:gitea", "knobas-gitea", &["gitea"]).await;

    knobas_sync::run_once(&pool, &src, None).await.unwrap();

    assert_eq!(
        attachments(&pool)
            .await
            .into_iter()
            .map(|(_, monitor, _, _)| monitor)
            .collect::<Vec<_>>(),
        vec!["kuma:2".to_owned(), "kuma:3".to_owned()]
    );
}

/// Another source's monitor of the same name is not this source's to attach.
///
/// The read is scoped to the source the run belongs to, and the reason is the
/// transaction: a run holds one source's advisory lock, so drawing a link off
/// another source's roster would be one run writing on another's behalf --
/// and that other source's own next poll draws it anyway.
#[tokio::test]
async fn a_run_attaches_only_its_own_sources_monitors() {
    let pool = pool("a_run_attaches_only_its_").await;
    let (theirs, _t) = source("kuma-eu", &[("2", "gitea")]);
    let (ours, _o) = source("kuma", &[("9", "canary")]);
    asset(&pool, "asset:gitea", "knobas-gitea", &["gitea"]).await;

    knobas_sync::run_once(&pool, &theirs, None).await.unwrap();
    sqlx::query("delete from knobas.link")
        .execute(&pool)
        .await
        .unwrap();

    knobas_sync::run_once(&pool, &ours, None).await.unwrap();
    assert!(
        attachments(&pool).await.is_empty(),
        "kuma's poll must not draw a link off kuma-eu's roster"
    );

    knobas_sync::run_once(&pool, &theirs, None).await.unwrap();
    assert_eq!(
        attachments(&pool).await,
        vec![(
            "asset:gitea".to_owned(),
            "kuma-eu:2".to_owned(),
            "imported".to_owned(),
            true
        )],
        "and kuma-eu's own next poll draws it"
    );
}

/// The negative control: a source that emits **no monitors** resolves nothing,
/// however well the names would have matched.
///
/// What this witnesses is the read's own `kind = 'monitor'` filter, and it is
/// worth being exact about which of the two guards that is. The resolution
/// also sits inside `run_locked`'s descriptor gate -- the one `samples` and
/// `alerts` share -- and **that gate is a second, independent guard**, so
/// moving the call outside it would leave every assertion in *this* test
/// standing: here the mirror holds no monitor at all, and the filter below is
/// the whole of why nothing is drawn.
/// `a_poll_ignores_monitor_rows_its_descriptor_does_not_declare` is the gate's
/// own control, over a mirror that does hold one, and it says why the gate is
/// a safety check rather than `!sweep_kinds.is_empty()`'s saved round trip.
#[tokio::test]
async fn a_source_that_emits_no_monitors_attaches_nothing() {
    let pool = pool("a_source_that_emits_no_m").await;
    let (src, _corpus) = source_of("gitea", "issue", &[("2", "gitea")]);
    asset(&pool, "asset:gitea", "knobas-gitea", &["gitea"]).await;

    knobas_sync::run_once(&pool, &src, None).await.unwrap();

    assert!(
        attachments(&pool).await.is_empty(),
        "an issue called `gitea` is not a monitor watching anything"
    );
}

/// The **other** guard, and this one is a safety check: a source whose
/// descriptor declares no monitor kind attaches nothing even when the mirror
/// still holds monitor rows under its id.
///
/// The control above witnesses `attach`'s own `kind = 'monitor'` filter, over
/// a mirror that never held a monitor. This one witnesses `run_locked`'s
/// descriptor gate, over a mirror that does -- and that is a state the app can
/// really be in, reached with no exotic history at all:
///
/// * `sync.item.source_id` has **no foreign key** to `knobas.source_config`
///   (`0002`, restated in `0012`), and `delete_source`'s `purge_items` is the
///   reader's own choice, so a source deleted without a purge leaves its rows
///   in `sync.item`;
/// * `0012` keeps them in `sync.live_item` -- its join is `left` and its
///   filter is `coalesce(s.enabled, true)`, so *"no config row at all leaves
///   them visible"*;
/// * a source id is a string the reader supplies, and `add_source` says in as
///   many words that *"something exists under this id again"* is a state it
///   plans for (#127).
///
/// A Kuma deleted without a purge and another adapter added under the same id
/// is therefore the fixture below, and without the gate that source's poll
/// would draw `monitored-by` links to the monitors of a source that is gone.
/// The sweep does not rescue it either: `sweep_kinds` is `emitted ∩
/// exhaustive`, and a kind this source never emits is in neither set -- which
/// the mid-test assertion checks rather than assumes, since an empty mirror
/// would pass this test while witnessing nothing.
#[tokio::test]
async fn a_poll_ignores_monitor_rows_its_descriptor_does_not_declare() {
    let pool = pool("a_poll_ignores_monitor_r").await;

    // The holder that put them there, polled before the asset exists so that
    // its own run draws no link and the rows are all it leaves behind.
    let (gone, _g) = source("kuma", &[("9", "gitea")]);
    knobas_sync::run_once(&pool, &gone, None).await.unwrap();
    asset(&pool, "asset:gitea", "knobas-gitea", &["gitea"]).await;

    // The id, reused by an adapter that mirrors issues. Its own sweep is over
    // `issue` alone, so the monitor rows stay live -- asserted after the run,
    // because it is what makes the assertion below about the gate.
    let (reused, _r) = source_of("kuma", "issue", &[("2", "payout")]);
    knobas_sync::run_once(&pool, &reused, None).await.unwrap();

    let live: i64 = sqlx::query_scalar(
        "select count(*) from sync.live_item where source_id = 'kuma' and kind = 'monitor'",
    )
    .fetch_one(&pool)
    .await
    .expect("the mirror is readable");
    assert_eq!(
        live, 1,
        "the fixture must still hold a live monitor row under the reused id, \
         or this test witnesses nothing"
    );

    assert!(
        attachments(&pool).await.is_empty(),
        "a poll whose descriptor declares no monitor kind must not attach the \
         monitor rows a previous holder of its id left in the mirror"
    );
}

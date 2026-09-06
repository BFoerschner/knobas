//! The sample battery (issue #443, M4.1): one row per poll per monitor.
//!
//! Spec #427: "At the end of every run of a source that emits `monitor`, the
//! engine appends one sample per live monitor (state, response time, taken
//! at) … Warn is derived from the response-time threshold setting at sample
//! time. Samples live in a knobas-owned table swept by the retention setting."
//!
//! **Why the engine and not the adapter.** A sample is knobas' own record of
//! what it saw, derived under a knobas setting, and the sentence above is
//! about *a source that emits `monitor`* rather than about Uptime Kuma. The
//! fake here declares that kind and the same `status_name` path the Kuma
//! descriptor declares (`state`), so what these tests pin is the engine's
//! rule and not one adapter's payload shape.
//!
//! **Why a poll and not a change.** The Kuma adapter's cursor is a digest of
//! the last corpus (contract §4.2 E), so an unchanged Kuma emits nothing at
//! all -- and `a_poll_that_emits_nothing_still_samples_every_live_monitor` is
//! the test that says a timeseries built from emitted items would go silent on
//! exactly the monitors that are steadily up.
//!
//! **Every test here gets a database of its own**, which is not this crate's
//! usual arrangement (`tests/run.rs` and `tests/sweep.rs` share one and give
//! each test a unique source id). Two things in this file are **global** and
//! cannot be namespaced by a source id: `knobas.setting`, which one test
//! writes and another reads a default from, and `samples::prune`, which
//! deletes every sample past the horizon and would take a concurrent test's
//! back-dated fixture out from under it. `scratch_database` costs one `create
//! database` on the same server, not a second postmaster.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use knobas_source::{
    Capability, ConnectionInfo, Cursor, KindInfo, Sink, Source, SourceDescriptor, SourceError,
    SyncItem, WriteOp,
};
use knobas_sync::samples;
use sqlx::PgPool;

/// One monitor as a source currently reports it: a key, a state word, and a
/// reading in milliseconds. `None` for either is what Kuma's own absences
/// look like -- a state gauge that has not landed, and the `-1` sentinel the
/// adapter already carries as "no response time".
#[derive(Clone)]
struct Monitor {
    key: &'static str,
    state: Option<&'static str>,
    response_time_ms: Option<f64>,
}

impl Monitor {
    fn up(key: &'static str, response_time_ms: f64) -> Self {
        Self {
            key,
            state: Some("up"),
            response_time_ms: Some(response_time_ms),
        }
    }
}

/// An adapter emitting whichever monitors it is currently told to, over the
/// kinds it is currently told to declare.
struct Monitors {
    id: String,
    corpus: Arc<Mutex<Vec<Monitor>>>,
    /// The kind these items are emitted under, and the kind the descriptor
    /// declares. `"monitor"` in every test but the negative control.
    kind: &'static str,
    /// Whether the descriptor declares where a monitor's state lives. Kuma
    /// declares `state`; an adapter that declares nothing is a miss (#277).
    declares_state: bool,
    /// Whether this run should emit anything at all. The digest-cursor shape:
    /// battery clause 2 says an idle incremental run emits nothing and hands
    /// its cursor straight back.
    idle: Arc<Mutex<bool>>,
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
            write_ops: Vec::new(),
            entity_kinds: vec![KindInfo {
                id: self.kind.to_owned(),
                label: self.kind.to_owned(),
                plural: self.kind.to_owned(),
                monogram: "MO".into(),
                full_sync_exhaustive: true,
            }],
            config_schema: serde_json::json!({ "type": "object", "properties": {} }),
            payload_paths: if self.declares_state {
                vec![knobas_source::KindPaths {
                    kind: self.kind.to_owned(),
                    status_name: vec![knobas_source::PayloadPath::of(["state"])],
                    ..knobas_source::KindPaths::default()
                }]
            } else {
                Vec::new()
            },
        }
    }

    async fn test_connection(&self) -> Result<ConnectionInfo, SourceError> {
        Ok(ConnectionInfo::default())
    }

    async fn sync(
        &self,
        cursor: Option<Cursor>,
        sink: &mut (dyn Sink + Send),
    ) -> Result<Cursor, SourceError> {
        if *self.idle.lock().unwrap() {
            // Battery clause 2: an idle incremental run emits nothing and
            // hands back the cursor it was given, byte for byte.
            return Ok(cursor.unwrap_or_else(|| r#"{"v":1}"#.to_owned()));
        }
        let corpus = self.corpus.lock().unwrap().clone();
        for monitor in &corpus {
            sink.item(SyncItem {
                entity: knobas_core::entity::EntityRef::new(&self.id, monitor.key),
                kind: self.kind.to_owned(),
                title: monitor.key.to_owned(),
                body_text: String::new(),
                author: None,
                updated_at: None,
                payload: serde_json::json!({
                    "id": monitor.key,
                    "state": monitor.state,
                    "response_time_ms": monitor.response_time_ms,
                }),
                web_url: None,
                deleted: false,
            })
            .await?;
        }
        Ok(r#"{"v":1}"#.to_owned())
    }

    async fn write(&self, _op: WriteOp) -> Result<knobas_source::WriteReceipt, SourceError> {
        Err(SourceError::protocol("read-only"))
    }
}

type Corpus = Arc<Mutex<Vec<Monitor>>>;
type Idle = Arc<Mutex<bool>>;

fn source_of(id: &str, kind: &'static str, monitors: &[Monitor]) -> (Monitors, Corpus, Idle) {
    let corpus: Corpus = Arc::new(Mutex::new(monitors.to_vec()));
    let idle: Idle = Arc::new(Mutex::new(false));
    (
        Monitors {
            id: id.to_owned(),
            corpus: Arc::clone(&corpus),
            kind,
            declares_state: true,
            idle: Arc::clone(&idle),
        },
        corpus,
        idle,
    )
}

fn source(id: &str, monitors: &[Monitor]) -> (Monitors, Corpus, Idle) {
    source_of(id, samples::KIND, monitors)
}

/// A migrated database this test shares with nobody. See the module header.
async fn pool(label: &str) -> PgPool {
    knobas_db::test_util::scratch_database(label)
        .await
        .pool(2)
        .await
        .expect("a pool on the scratch database")
}

/// Every sample of one monitor, oldest first: its state and its reading.
async fn taken(pool: &PgPool, entity_id: &str) -> Vec<(Option<String>, Option<i32>)> {
    sqlx::query_as(
        "select state, response_time_ms from knobas.monitor_sample
          where entity_id = $1 order by taken_at, id",
    )
    .bind(entity_id)
    .fetch_all(pool)
    .await
    .unwrap()
}

/// How many samples one source's monitors have between them.
async fn count(pool: &PgPool, source_id: &str) -> i64 {
    let (n,): (i64,) = sqlx::query_as(
        "select count(*) from knobas.monitor_sample s
           join sync.item i on i.entity_id = s.entity_id
          where i.source_id = $1",
    )
    .bind(source_id)
    .fetch_one(pool)
    .await
    .unwrap();
    n
}

/// Put one sample in the past, so retention has something to reach.
async fn back_date(pool: &PgPool, entity_id: &str, at: DateTime<Utc>) {
    sqlx::query("update knobas.monitor_sample set taken_at = $2 where entity_id = $1")
        .bind(entity_id)
        .bind(at)
        .execute(pool)
        .await
        .unwrap();
}

async fn set_setting(pool: &PgPool, key: &str, value: i64) {
    sqlx::query(
        "insert into knobas.setting (key, value) values ($1, $2)
         on conflict (key) do update set value = excluded.value",
    )
    .bind(key)
    .bind(serde_json::json!(value))
    .execute(pool)
    .await
    .unwrap();
}

// --- one row per poll per monitor -------------------------------------------

/// The headline: two runs, two samples for every monitor, and the reading of
/// the run they were taken in.
#[tokio::test]
async fn two_runs_append_one_sample_per_monitor_each() {
    let pool = pool("two_runs_append_one_samp").await;
    let id = "kuma".to_owned();
    let (src, corpus, _idle) = source(
        &id,
        &[Monitor::up("gitea", 35.0), Monitor::up("jira", 120.0)],
    );

    knobas_sync::run_once(&pool, &src, None).await.unwrap();
    corpus.lock().unwrap()[0].response_time_ms = Some(41.0);
    knobas_sync::run_once(&pool, &src, None).await.unwrap();

    assert_eq!(
        taken(&pool, &format!("{id}:gitea")).await,
        vec![
            (Some("up".to_owned()), Some(35)),
            (Some("up".to_owned()), Some(41)),
        ],
        "each poll leaves its own reading behind, in the order they were taken"
    );
    assert_eq!(
        taken(&pool, &format!("{id}:jira")).await,
        vec![
            (Some("up".to_owned()), Some(120)),
            (Some("up".to_owned()), Some(120)),
        ],
        "a monitor nothing changed about is sampled twice all the same"
    );
}

/// The digest-cursor case, and the reason samples are read off the mirror
/// rather than off the items a run emitted: an unchanged Uptime Kuma emits
/// **nothing** (contract §4.2 E), and a timeseries built from emissions would
/// go silent on exactly the monitors that are steadily up.
#[tokio::test]
async fn a_poll_that_emits_nothing_still_samples_every_live_monitor() {
    let pool = pool("a_poll_that_emits_nothin").await;
    let id = "kuma".to_owned();
    let (src, _corpus, idle) = source(&id, &[Monitor::up("gitea", 35.0)]);

    let first = knobas_sync::run_once(&pool, &src, None).await.unwrap();
    assert_eq!(first.upserted, 1);

    *idle.lock().unwrap() = true;
    let second = knobas_sync::run_once(&pool, &src, Some(r#"{"v":1}"#.to_owned()))
        .await
        .unwrap();
    assert_eq!(second.upserted, 0, "the poll emitted nothing");

    assert_eq!(
        taken(&pool, &format!("{id}:gitea")).await.len(),
        2,
        "an idle poll is still a poll, and the monitor was still up for it"
    );
}

/// The acceptance criterion's negative: a monitor the second run does not
/// publish gets no second sample. The adapter tombstones it (the Kuma shape --
/// a paused or deleted monitor simply leaves `/metrics`), and a tombstoned
/// monitor is not a live one.
#[tokio::test]
async fn a_monitor_the_second_run_does_not_publish_gets_no_second_sample() {
    let pool = pool("a_monitor_the_second_run").await;
    let id = "kuma".to_owned();
    let (src, corpus, _idle) = source(
        &id,
        &[Monitor::up("gitea", 35.0), Monitor::up("canary", 12.0)],
    );

    knobas_sync::run_once(&pool, &src, None).await.unwrap();
    corpus
        .lock()
        .unwrap()
        .retain(|monitor| monitor.key != "canary");
    knobas_sync::run_once(&pool, &src, None).await.unwrap();

    assert_eq!(
        taken(&pool, &format!("{id}:canary")).await.len(),
        1,
        "the vanished monitor keeps the history it had and gains nothing"
    );
    assert_eq!(
        taken(&pool, &format!("{id}:gitea")).await.len(),
        2,
        "and the one still there was sampled by both runs"
    );
}

/// The negative control: a source that emits no `monitor` kind writes no
/// samples, however much it syncs.
#[tokio::test]
async fn a_source_that_emits_no_monitor_kind_writes_no_samples() {
    let pool = pool("a_source_that_emits_no_m").await;
    let id = "kuma".to_owned();
    let (src, _corpus, _idle) = source_of(&id, "ticket", &[Monitor::up("PAY-231", 35.0)]);

    let report = knobas_sync::run_once(&pool, &src, None).await.unwrap();

    assert_eq!(report.upserted, 1, "the run itself mirrored its item");
    assert_eq!(
        count(&pool, &id).await,
        0,
        "a ticket is not a monitor and has no timeseries"
    );
}

// --- warn, and where it begins ----------------------------------------------

/// Warn is knobas', not Kuma's: a monitor answering slower than the threshold
/// is *up* to the source and *warn* to knobas, decided at sample time.
#[tokio::test]
async fn a_monitor_over_the_threshold_samples_as_warn() {
    let pool = pool("a_monitor_over_the_thres").await;
    let id = "kuma".to_owned();
    let (src, _corpus, _idle) = source(
        &id,
        &[
            Monitor::up("slow", samples::DEFAULT_THRESHOLD_MS as f64 + 1.0),
            Monitor::up("edge", samples::DEFAULT_THRESHOLD_MS as f64),
            Monitor::up("quick", 35.0),
        ],
    );

    knobas_sync::run_once(&pool, &src, None).await.unwrap();

    assert_eq!(
        taken(&pool, &format!("{id}:slow")).await,
        vec![(Some("warn".to_owned()), Some(1501))],
        "over the threshold is warn"
    );
    assert_eq!(
        taken(&pool, &format!("{id}:edge")).await,
        vec![(Some("up".to_owned()), Some(1500))],
        "*at* the threshold is not over it"
    );
    assert_eq!(
        taken(&pool, &format!("{id}:quick")).await,
        vec![(Some("up".to_owned()), Some(35))],
        "and a quick answer is up"
    );
}

/// The threshold is a setting and the samples follow it, which is the half of
/// "derived at sample time" a default cannot witness.
#[tokio::test]
async fn the_stored_threshold_is_what_decides_warn() {
    let pool = pool("the_stored_threshold_is_").await;
    let id = "kuma".to_owned();
    set_setting(&pool, samples::THRESHOLD_KEY, 100).await;
    let (src, _corpus, _idle) = source(&id, &[Monitor::up("gitea", 120.0)]);

    knobas_sync::run_once(&pool, &src, None).await.unwrap();

    assert_eq!(
        taken(&pool, &format!("{id}:gitea")).await,
        vec![(Some("warn".to_owned()), Some(120))],
        "120 ms is under the default and over the stored threshold"
    );
}

/// A monitor that is down stays down however fast it answered: warn is a
/// *worse* reading of an otherwise healthy monitor, never a milder one of a
/// broken monitor (`CONTEXT.md`'s rollup order is down > warn > up).
#[tokio::test]
async fn a_down_monitor_is_never_softened_to_warn() {
    let pool = pool("a_down_monitor_is_never_").await;
    let id = "kuma".to_owned();
    let (src, _corpus, _idle) = source(
        &id,
        &[Monitor {
            key: "gitea",
            state: Some("down"),
            response_time_ms: Some(9000.0),
        }],
    );

    knobas_sync::run_once(&pool, &src, None).await.unwrap();

    assert_eq!(
        taken(&pool, &format!("{id}:gitea")).await,
        vec![(Some("down".to_owned()), Some(9000))]
    );
}

/// ADR-0007's miss, both halves of it: a state the declaration does not
/// resolve is null rather than a guess, and the row is written anyway --
/// because "knobas polled and could not tell" is a fact about that minute and
/// a hole in the bar would say nothing happened.
#[tokio::test]
async fn a_monitor_whose_state_does_not_resolve_is_sampled_as_a_miss() {
    let pool = pool("a_monitor_whose_state_do").await;
    let id = "kuma".to_owned();
    let (src, _corpus, _idle) = source(
        &id,
        &[
            Monitor {
                key: "unlit",
                state: None,
                response_time_ms: None,
            },
            Monitor {
                key: "unknown",
                state: Some("bewildered"),
                response_time_ms: Some(35.0),
            },
        ],
    );

    knobas_sync::run_once(&pool, &src, None).await.unwrap();

    assert_eq!(
        taken(&pool, &format!("{id}:unlit")).await,
        vec![(None, None)],
        "a monitor whose first beat has not landed is a row with nothing in it"
    );
    assert_eq!(
        taken(&pool, &format!("{id}:unknown")).await,
        vec![(None, Some(35))],
        "a word knobas has no meaning for is a miss, not a new state"
    );
}

/// An adapter that declares no `status_name` path misses on every monitor: the
/// engine reads the declaration and never the key (#277, ADR-0007), so a
/// source that says nothing about where its state lives gets stateless
/// samples rather than a lucky guess.
#[tokio::test]
async fn a_source_declaring_no_state_path_samples_no_state() {
    let pool = pool("a_source_declaring_no_st").await;
    let id = "kuma".to_owned();
    let corpus: Corpus = Arc::new(Mutex::new(vec![Monitor::up("gitea", 35.0)]));
    let src = Monitors {
        id: id.clone(),
        corpus,
        kind: samples::KIND,
        declares_state: false,
        idle: Arc::new(Mutex::new(false)),
    };

    knobas_sync::run_once(&pool, &src, None).await.unwrap();

    assert_eq!(
        taken(&pool, &format!("{id}:gitea")).await,
        vec![(None, Some(35))],
        "the payload says `state`, the descriptor does not, and the engine reads the descriptor"
    );
}

// --- retention --------------------------------------------------------------

/// The sweep takes what is older than the setting and nothing younger -- the
/// observation-sweep shape (`knobas_app::time::passive::prune`), with the
/// horizon handed in so a fixture can place it.
#[tokio::test]
async fn the_sweep_takes_samples_older_than_retention_and_nothing_younger() {
    let pool = pool("the_sweep_takes_samples_").await;
    let id = "kuma".to_owned();
    let (src, _corpus, _idle) = source(&id, &[Monitor::up("gitea", 35.0)]);
    let entity = format!("{id}:gitea");

    let now = Utc::now();
    knobas_sync::run_once(&pool, &src, None).await.unwrap();
    back_date(
        &pool,
        &entity,
        now - Duration::days(samples::DEFAULT_RETENTION_DAYS) - Duration::hours(1),
    )
    .await;
    knobas_sync::run_once(&pool, &src, None).await.unwrap();

    let taken_rows = samples::prune(&pool, now).await.unwrap();

    assert_eq!(taken_rows, 1, "one sample was past the horizon");
    assert_eq!(
        taken(&pool, &entity).await.len(),
        1,
        "and the one inside it is still there"
    );
}

/// Retention is a setting, and the sweep reads it rather than a constant.
#[tokio::test]
async fn the_stored_retention_is_what_the_sweep_reaches_back_to() {
    let pool = pool("the_stored_retention_is_").await;
    let id = "kuma".to_owned();
    set_setting(&pool, samples::RETENTION_KEY, 1).await;
    let (src, _corpus, _idle) = source(&id, &[Monitor::up("gitea", 35.0)]);
    let entity = format!("{id}:gitea");

    let now = Utc::now();
    knobas_sync::run_once(&pool, &src, None).await.unwrap();
    back_date(&pool, &entity, now - Duration::days(2)).await;

    assert_eq!(samples::prune(&pool, now).await.unwrap(), 1);
    assert!(
        taken(&pool, &entity).await.is_empty(),
        "two days old is past a one-day retention"
    );
}

/// A sweep on a database with nothing old enough deletes nothing, which is
/// the direction that would otherwise go unnoticed: a cutoff computed the
/// wrong way round takes the whole table and every other test here would
/// still pass.
#[tokio::test]
async fn a_sweep_with_nothing_past_the_horizon_takes_nothing() {
    let pool = pool("a_sweep_with_nothing_pas").await;
    let id = "kuma".to_owned();
    let (src, _corpus, _idle) = source(&id, &[Monitor::up("gitea", 35.0)]);

    knobas_sync::run_once(&pool, &src, None).await.unwrap();
    let before = taken(&pool, &format!("{id}:gitea")).await.len();

    samples::prune(&pool, Utc::now()).await.unwrap();

    assert_eq!(
        taken(&pool, &format!("{id}:gitea")).await.len(),
        before,
        "today's samples are not what ninety days of retention is about"
    );
}

// --- the settings, read back ------------------------------------------------

/// The two keys, their defaults, and the round trip a settings surface makes.
#[tokio::test]
async fn the_settings_default_and_survive_a_round_trip() {
    let pool = pool("the_settings_default_and").await;

    // Read on a database nobody has written a setting to. No migration
    // inserts either key -- the convention `knobas.setting` has had since
    // `0002` -- so this is the whole of "the two settings keys exist with
    // their defaults": a fresh profile and a profile whose row was deleted
    // give the same answer, and the answer is the one spec #427 ratified.
    assert_eq!(samples::threshold_ms(&pool).await.unwrap(), 1500);
    assert_eq!(samples::retention_days(&pool).await.unwrap(), 90);
    assert_eq!(samples::DEFAULT_THRESHOLD_MS, 1500);
    assert_eq!(samples::DEFAULT_RETENTION_DAYS, 90);

    assert_eq!(samples::set_threshold_ms(&pool, 2500).await.unwrap(), 2500);
    assert_eq!(samples::threshold_ms(&pool).await.unwrap(), 2500);
    assert_eq!(samples::set_retention_days(&pool, 7).await.unwrap(), 7);
    assert_eq!(samples::retention_days(&pool).await.unwrap(), 7);

    // Out of range on the way in, so a hand-edited row or an older knobas
    // cannot turn retention into "delete everything".
    assert_eq!(samples::set_retention_days(&pool, 0).await.unwrap(), 1);
    assert_eq!(samples::set_threshold_ms(&pool, -5).await.unwrap(), 0);
}

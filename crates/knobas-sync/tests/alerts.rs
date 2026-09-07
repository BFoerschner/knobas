//! The alert battery (issue #444, M4.1): a crossing into down or warn opens
//! one, a return to up closes it, and never more than one is open at a time.
//!
//! Spec #427: *"the engine … reconciles alerts from the newest two samples per
//! monitor: a crossing into down or warn opens an alert if none is open, a
//! return to up closes the open one."* `CONTEXT.md`, **Alert**: *"a monitor's
//! transition to down or warn, open until the monitor recovers; at most one
//! open per monitor."*
//!
//! **Driven through the engine, never through the reconciler.** Every test
//! here runs `knobas_sync::run_once` and then reads `knobas.monitor_alert`,
//! because the claim being made is about *a poll* -- "at the end of every run
//! of a source that emits `monitor`" -- and a test that called
//! `alerts::reconcile` directly would be green with the call site deleted from
//! `run_locked`. The rule itself, as a table of every state word in both
//! directions, is a unit test in `src/alerts.rs`; what is here is the wiring
//! and the sequences a reader would recognise.
//!
//! **Every test gets a database of its own**, `tests/samples.rs`' arrangement
//! and for its reason: `knobas.setting` is global (the threshold one test
//! moves is the one another reads a default from) and so is the alert table's
//! `monitor_alert_one_open_idx`.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use knobas_source::{
    Capability, ConnectionInfo, Cursor, KindInfo, Sink, Source, SourceDescriptor, SourceError,
    SyncItem, WriteOp,
};
use knobas_sync::samples;
use sqlx::PgPool;

/// One monitor as a source currently reports it.
///
/// `state` is the word the adapter publishes -- Uptime Kuma's own vocabulary,
/// which has no *warn* in it: a warn sample is an `up` monitor answering
/// slower than the threshold, derived by the engine (#443).
#[derive(Clone)]
struct Monitor {
    key: &'static str,
    state: Option<&'static str>,
    response_time_ms: Option<f64>,
}

impl Monitor {
    fn at(key: &'static str, state: &'static str, response_time_ms: f64) -> Self {
        Self {
            key,
            state: Some(state),
            response_time_ms: Some(response_time_ms),
        }
    }

    fn up(key: &'static str) -> Self {
        Self::at(key, "up", 35.0)
    }

    fn down(key: &'static str) -> Self {
        Self::at(key, "down", 0.0)
    }
}

/// An adapter emitting whichever monitors it is currently told to.
struct Monitors {
    id: String,
    corpus: Arc<Mutex<Vec<Monitor>>>,
    /// The kinds the descriptor declares -- `["monitor"]` everywhere but the
    /// negative control.
    kinds: Vec<&'static str>,
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
            entity_kinds: self
                .kinds
                .iter()
                .map(|kind| KindInfo {
                    id: (*kind).to_owned(),
                    label: (*kind).to_owned(),
                    plural: (*kind).to_owned(),
                    monogram: "MO".into(),
                    full_sync_exhaustive: true,
                })
                .collect(),
            config_schema: serde_json::json!({ "type": "object", "properties": {} }),
            payload_paths: self
                .kinds
                .iter()
                .map(|kind| knobas_source::KindPaths {
                    kind: (*kind).to_owned(),
                    status_name: vec![knobas_source::PayloadPath::of(["state"])],
                    ..knobas_source::KindPaths::default()
                })
                .collect(),
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
        let kind = self.kinds[0].to_owned();
        for monitor in &corpus {
            sink.item(SyncItem {
                entity: knobas_core::entity::EntityRef::new(&self.id, monitor.key),
                kind: kind.clone(),
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

type Corpus = Arc<Mutex<Vec<Monitor>>>;

fn source_of(id: &str, kinds: &[&'static str], monitors: &[Monitor]) -> (Monitors, Corpus) {
    let corpus: Corpus = Arc::new(Mutex::new(monitors.to_vec()));
    (
        Monitors {
            id: id.to_owned(),
            corpus: Arc::clone(&corpus),
            kinds: kinds.to_vec(),
        },
        corpus,
    )
}

fn source(id: &str, monitors: &[Monitor]) -> (Monitors, Corpus) {
    source_of(id, &[samples::KIND], monitors)
}

/// A migrated database this test shares with nobody. See the module header.
async fn pool(label: &str) -> PgPool {
    knobas_db::test_util::scratch_database(label)
        .await
        .pool(2)
        .await
        .expect("a pool on the scratch database")
}

/// Replace what the source publishes for one monitor, so the next run is a
/// different poll of the same estate.
fn publishes(corpus: &Corpus, monitor: Monitor) {
    let mut held = corpus.lock().unwrap();
    for existing in held.iter_mut() {
        if existing.key == monitor.key {
            *existing = monitor;
            return;
        }
    }
    panic!("no monitor called {} in the corpus", monitor.key);
}

/// Stop publishing one monitor at all -- what a Kuma pause looks like from
/// `/metrics`, and what the engine's exhaustive sweep tombstones.
fn pauses(corpus: &Corpus, key: &str) {
    corpus.lock().unwrap().retain(|monitor| monitor.key != key);
}

/// One alert row, as every assertion here reads it.
#[derive(Debug, PartialEq, Eq)]
struct Alert {
    state: String,
    closed: bool,
}

/// Every alert of one monitor, oldest first.
async fn alerts(pool: &PgPool, entity_id: &str) -> Vec<Alert> {
    sqlx::query_as::<_, (String, Option<DateTime<Utc>>)>(
        "select state, closed_at from knobas.monitor_alert
          where entity_id = $1 order by opened_at, id",
    )
    .bind(entity_id)
    .fetch_all(pool)
    .await
    .unwrap()
    .into_iter()
    .map(|(state, closed_at)| Alert {
        state,
        closed: closed_at.is_some(),
    })
    .collect()
}

/// The open alerts of one monitor -- what the top strip counts and the Assets
/// view lists.
async fn open(pool: &PgPool, entity_id: &str) -> Vec<Alert> {
    alerts(pool, entity_id)
        .await
        .into_iter()
        .filter(|alert| !alert.closed)
        .collect()
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

// --- the sequences a reader would recognise ---------------------------------

/// The headline, and the ticket's first criterion: **down then up across three
/// runs opens and closes one alert**.
#[tokio::test]
async fn down_then_up_across_three_runs_opens_and_closes_one_alert() {
    let pool = pool("down_then_up_across_thre").await;
    let id = "kuma".to_owned();
    let (src, corpus) = source(&id, &[Monitor::up("gitea")]);
    let gitea = format!("{id}:gitea");

    knobas_sync::run_once(&pool, &src, None).await.unwrap();
    assert!(
        alerts(&pool, &gitea).await.is_empty(),
        "an up monitor is not news"
    );

    publishes(&corpus, Monitor::down("gitea"));
    knobas_sync::run_once(&pool, &src, None).await.unwrap();
    assert_eq!(
        alerts(&pool, &gitea).await,
        vec![Alert {
            state: "down".to_owned(),
            closed: false
        }],
        "the crossing into down opened one, and it is open"
    );

    publishes(&corpus, Monitor::up("gitea"));
    knobas_sync::run_once(&pool, &src, None).await.unwrap();
    assert_eq!(
        alerts(&pool, &gitea).await,
        vec![Alert {
            state: "down".to_owned(),
            closed: true
        }],
        "the return to up closed the one that was open, and opened nothing"
    );
}

/// The second criterion: **down, down opens one**, not two.
///
/// The anti-flood rule of spec #427 story 57, and the reason it is *one open
/// per monitor* rather than one alert per crossing.
#[tokio::test]
async fn two_down_polls_open_one_alert() {
    let pool = pool("two_down_polls_open_one_").await;
    let id = "kuma".to_owned();
    let (src, _corpus) = source(&id, &[Monitor::down("gitea")]);
    let gitea = format!("{id}:gitea");

    knobas_sync::run_once(&pool, &src, None).await.unwrap();
    knobas_sync::run_once(&pool, &src, None).await.unwrap();

    assert_eq!(
        alerts(&pool, &gitea).await,
        vec![Alert {
            state: "down".to_owned(),
            closed: false
        }],
        "two polls of a broken monitor are one piece of news"
    );
}

/// The third criterion: **a flap inside one poll is invisible.**
///
/// The monitor falls and recovers entirely between two polls, so no sample
/// ever carries `down` -- and an alert is reconciled from samples, which is
/// what makes "knobas does not report what it did not see" structural rather
/// than a threshold somebody tuned. The `down` is published and withdrawn
/// without a run in between, which is exactly what a flap between two
/// one-minute polls is.
#[tokio::test]
async fn a_flap_between_two_polls_is_invisible() {
    let pool = pool("a_flap_between_two_polls").await;
    let id = "kuma".to_owned();
    let (src, corpus) = source(&id, &[Monitor::up("gitea")]);
    let gitea = format!("{id}:gitea");

    knobas_sync::run_once(&pool, &src, None).await.unwrap();

    // Down and back up, with no poll in between.
    publishes(&corpus, Monitor::down("gitea"));
    publishes(&corpus, Monitor::up("gitea"));

    knobas_sync::run_once(&pool, &src, None).await.unwrap();

    assert!(
        alerts(&pool, &gitea).await.is_empty(),
        "nothing sampled the fall, so there is nothing to alert about"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "select count(*) from knobas.monitor_sample where entity_id = $1 and state = 'down'"
        )
        .bind(&gitea)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0,
        "and the reason is that no sample carries the fall"
    );
}

/// The fourth criterion: **warn opens one.**
///
/// Warn is knobas' own state, derived at sample time from the response-time
/// threshold (#443) -- the source calls this monitor `up`. So this is also the
/// test that the alert is reconciled from the *sample's* state and not from
/// the payload the adapter published: nothing Uptime Kuma emits ever says
/// `warn`.
#[tokio::test]
async fn a_monitor_over_the_threshold_opens_a_warn_alert() {
    let pool = pool("a_monitor_over_the_thres").await;
    let id = "kuma".to_owned();
    let slow = f64::from(u32::try_from(samples::DEFAULT_THRESHOLD_MS).unwrap()) + 1.0;
    let (src, corpus) = source(&id, &[Monitor::at("jira", "up", slow)]);
    let jira = format!("{id}:jira");

    knobas_sync::run_once(&pool, &src, None).await.unwrap();
    assert_eq!(
        alerts(&pool, &jira).await,
        vec![Alert {
            state: "warn".to_owned(),
            closed: false
        }],
        "a slow monitor is a warn alert, although the source called it up"
    );

    // And answering quickly again closes it, because the sample says `up`.
    publishes(&corpus, Monitor::up("jira"));
    knobas_sync::run_once(&pool, &src, None).await.unwrap();
    assert!(
        open(&pool, &jira).await.is_empty(),
        "answering inside the threshold is a recovery"
    );
}

/// Where *warn* begins is the setting's, so moving it moves what opens an
/// alert -- and it moves it for the **next** poll, never for what is already
/// recorded (#443's rule about the threshold, carried through to alerts).
#[tokio::test]
async fn the_threshold_decides_which_polls_open_a_warn_alert() {
    let pool = pool("the_threshold_decides_wh").await;
    let id = "kuma".to_owned();
    let (src, _corpus) = source(&id, &[Monitor::at("jira", "up", 900.0)]);
    let jira = format!("{id}:jira");

    knobas_sync::run_once(&pool, &src, None).await.unwrap();
    assert!(
        alerts(&pool, &jira).await.is_empty(),
        "900 ms is inside the 1500 ms default"
    );

    set_setting(&pool, samples::THRESHOLD_KEY, 500).await;
    knobas_sync::run_once(&pool, &src, None).await.unwrap();
    assert_eq!(
        open(&pool, &jira).await,
        vec![Alert {
            state: "warn".to_owned(),
            closed: false
        }],
        "the same monitor, a lower threshold, and the next poll is warn"
    );
}

// --- what does not open, and what does not close ----------------------------

/// **A paused monitor contributes nothing**: it gets no sample (#443) and its
/// alert is left exactly as it was.
///
/// The conservative half is the second assertion. Pausing a monitor in Uptime
/// Kuma is not it recovering, and an alert that closed itself because
/// somebody silenced the thing watching it would be knobas reporting a fix
/// nobody made. It closes when the monitor comes back and is *up*.
#[tokio::test]
async fn a_paused_monitor_keeps_the_alert_it_had() {
    let pool = pool("a_paused_monitor_keeps_t").await;
    let id = "kuma".to_owned();
    let (src, corpus) = source(&id, &[Monitor::down("canary"), Monitor::up("gitea")]);
    let canary = format!("{id}:canary");

    knobas_sync::run_once(&pool, &src, None).await.unwrap();
    assert_eq!(open(&pool, &canary).await.len(), 1);
    let samples_before = sqlx::query_scalar::<_, i64>(
        "select count(*) from knobas.monitor_sample where entity_id = $1",
    )
    .bind(&canary)
    .fetch_one(&pool)
    .await
    .unwrap();

    pauses(&corpus, "canary");
    knobas_sync::run_once(&pool, &src, None).await.unwrap();
    knobas_sync::run_once(&pool, &src, None).await.unwrap();

    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "select count(*) from knobas.monitor_sample where entity_id = $1"
        )
        .bind(&canary)
        .fetch_one(&pool)
        .await
        .unwrap(),
        samples_before,
        "a paused monitor is not live and is sampled no more"
    );
    assert_eq!(
        open(&pool, &canary).await,
        vec![Alert {
            state: "down".to_owned(),
            closed: false
        }],
        "and its alert is neither closed nor doubled"
    );
}

/// `pending` and `maintenance` are the two words Uptime Kuma has that knobas'
/// rollup has no meaning for, and they are neither trouble nor recovery.
///
/// The merge review of #443 left this open: `monitor_sample`'s CHECK holds
/// five words and `CONTEXT.md`'s vocabulary holds three. The reading taken
/// here is the conservative one -- neither word opens an alert, and neither
/// closes one -- because the other reading is the one that can lie: a monitor
/// put into maintenance while it is down would have its alert closed and
/// knobas would be reporting a recovery nobody made.
#[tokio::test]
async fn pending_and_maintenance_neither_open_an_alert_nor_close_one() {
    let pool = pool("pending_and_maintenance_").await;
    let id = "kuma".to_owned();
    let (src, corpus) = source(
        &id,
        &[Monitor::at("fresh", "pending", 0.0), Monitor::down("held")],
    );
    let fresh = format!("{id}:fresh");
    let held = format!("{id}:held");

    knobas_sync::run_once(&pool, &src, None).await.unwrap();
    assert!(
        alerts(&pool, &fresh).await.is_empty(),
        "a monitor whose first beat has not landed is not down"
    );
    assert_eq!(open(&pool, &held).await.len(), 1);

    publishes(&corpus, Monitor::at("fresh", "maintenance", 0.0));
    publishes(&corpus, Monitor::at("held", "maintenance", 0.0));
    knobas_sync::run_once(&pool, &src, None).await.unwrap();

    assert!(
        alerts(&pool, &fresh).await.is_empty(),
        "a monitor in a maintenance window is not down either"
    );
    assert_eq!(
        open(&pool, &held).await,
        vec![Alert {
            state: "down".to_owned(),
            closed: false
        }],
        "and a maintenance window over a broken monitor is not a recovery"
    );
}

/// A state the engine could not resolve is a gap, and a gap decides nothing.
///
/// #443's failure direction, carried through: a drifted read yields a sample
/// with no state, which draws as a hole in the Monitors tab's bar, opens no
/// alert -- and does not close one either, because "I could not read the
/// monitor" is not "the monitor is well".
#[tokio::test]
async fn a_sample_that_missed_neither_opens_an_alert_nor_closes_one() {
    let pool = pool("a_sample_that_missed_nei").await;
    let id = "kuma".to_owned();
    let (src, corpus) = source(&id, &[Monitor::down("canary")]);
    let canary = format!("{id}:canary");

    knobas_sync::run_once(&pool, &src, None).await.unwrap();
    assert_eq!(open(&pool, &canary).await.len(), 1);

    publishes(
        &corpus,
        Monitor {
            key: "canary",
            state: None,
            response_time_ms: None,
        },
    );
    knobas_sync::run_once(&pool, &src, None).await.unwrap();

    assert_eq!(
        sqlx::query_scalar::<_, Option<String>>(
            "select state from knobas.monitor_sample where entity_id = $1
              order by taken_at desc, id desc limit 1"
        )
        .bind(&canary)
        .fetch_one(&pool)
        .await
        .unwrap(),
        None,
        "the newest sample is a miss"
    );
    assert_eq!(
        open(&pool, &canary).await,
        vec![Alert {
            state: "down".to_owned(),
            closed: false
        }],
        "and the alert it could not read about still stands"
    );
}

/// The negative control: a source that emits no `monitor` kind reconciles
/// nothing, however much it syncs.
#[tokio::test]
async fn a_source_that_emits_no_monitor_kind_opens_no_alerts() {
    let pool = pool("a_source_that_emits_no_m").await;
    let id = "jira".to_owned();
    let (src, _corpus) = source_of(&id, &["ticket"], &[Monitor::down("PAY-231")]);

    let report = knobas_sync::run_once(&pool, &src, None).await.unwrap();

    assert_eq!(report.upserted, 1, "the run itself mirrored its item");
    assert_eq!(
        sqlx::query_scalar::<_, i64>("select count(*) from knobas.monitor_alert")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0,
        "a ticket that says `down` is not a monitor and opens nothing"
    );
}

// --- the two structural claims ----------------------------------------------

/// **A monitor that is already down the first time knobas looks opens an
/// alert.**
///
/// This is the case that decides between the two readings of "reconciled from
/// the newest two samples": the samples read `down, down`, which is not a
/// crossing, and a rule that needed one would stay silent about a broken
/// monitor until it recovered and fell again. `src/alerts.rs`' header argues
/// it in full; this is the witness.
///
/// The fixture deletes the alert rather than the samples, because that is
/// exactly the state every installed profile is in the first time it runs
/// after migration `0022`: a timeseries from `0021`, and no alert table
/// content at all.
#[tokio::test]
async fn a_monitor_already_down_when_the_alert_table_is_empty_opens_one() {
    let pool = pool("a_monitor_already_down_w").await;
    let id = "kuma".to_owned();
    let (src, _corpus) = source(&id, &[Monitor::down("canary")]);
    let canary = format!("{id}:canary");

    knobas_sync::run_once(&pool, &src, None).await.unwrap();
    knobas_sync::run_once(&pool, &src, None).await.unwrap();
    sqlx::query("delete from knobas.monitor_alert")
        .execute(&pool)
        .await
        .unwrap();

    knobas_sync::run_once(&pool, &src, None).await.unwrap();

    assert!(
        sqlx::query_scalar::<_, i64>(
            "select count(*) from knobas.monitor_sample where entity_id = $1 and state = 'down'"
        )
        .bind(&canary)
        .fetch_one(&pool)
        .await
        .unwrap()
            >= 3,
        "the two newest samples are both `down`, so this is not a crossing"
    );
    assert_eq!(
        open(&pool, &canary).await,
        vec![Alert {
            state: "down".to_owned(),
            closed: false
        }],
        "and knobas says so all the same"
    );
}

/// **One open per monitor is the schema's rule, not the reconciler's.**
///
/// `monitor_alert_one_open_idx` is a partial unique index, so a second opener
/// -- a future write path, a hand-run statement, two engines against one
/// database -- fails at the statement rather than quietly doubling every count
/// the top strip draws. The reconciler never reaches it, which is why the only
/// way to witness it is to try the insert by hand.
#[tokio::test]
async fn the_database_refuses_a_second_open_alert() {
    let pool = pool("the_database_refuses_a_s").await;
    let id = "kuma".to_owned();
    let (src, corpus) = source(&id, &[Monitor::down("canary")]);
    let canary = format!("{id}:canary");

    knobas_sync::run_once(&pool, &src, None).await.unwrap();

    let refused =
        sqlx::query("insert into knobas.monitor_alert (entity_id, state) values ($1, $2)")
            .bind(&canary)
            .bind("warn")
            .execute(&pool)
            .await;
    assert!(
        refused.is_err(),
        "a second open alert for one monitor was accepted"
    );

    // And once the first is closed, the monitor may open another -- a monitor
    // that has fallen twice has two rows, which is its history.
    publishes(&corpus, Monitor::up("canary"));
    knobas_sync::run_once(&pool, &src, None).await.unwrap();
    publishes(&corpus, Monitor::down("canary"));
    knobas_sync::run_once(&pool, &src, None).await.unwrap();

    assert_eq!(
        alerts(&pool, &canary).await,
        vec![
            Alert {
                state: "down".to_owned(),
                closed: true
            },
            Alert {
                state: "down".to_owned(),
                closed: false
            },
        ],
        "the closed one is history and the new one is the news"
    );
}

/// A run of one source decides about that source's monitors and no others.
///
/// Two sources on one database, one broken monitor each: the run that
/// reconciles `kuma` must leave `watchtower`'s alone. The roster read is
/// filtered on `source_id`, and a filter nobody exercises is a filter that can
/// go missing.
#[tokio::test]
async fn a_run_reconciles_its_own_sources_monitors_only() {
    let pool = pool("a_run_reconciles_its_own").await;
    let (first, first_corpus) = source("kuma", &[Monitor::down("canary")]);
    let (second, _second_corpus) = source("watchtower", &[Monitor::down("canary")]);

    knobas_sync::run_once(&pool, &first, None).await.unwrap();
    knobas_sync::run_once(&pool, &second, None).await.unwrap();
    assert_eq!(open(&pool, "kuma:canary").await.len(), 1);
    assert_eq!(open(&pool, "watchtower:canary").await.len(), 1);

    // `kuma`'s recovers and `watchtower`'s does not; only one alert closes.
    publishes(&first_corpus, Monitor::up("canary"));
    knobas_sync::run_once(&pool, &first, None).await.unwrap();

    assert!(
        open(&pool, "kuma:canary").await.is_empty(),
        "the source that ran had its recovery seen"
    );
    assert_eq!(
        open(&pool, "watchtower:canary").await.len(),
        1,
        "and the source that did not run was not touched"
    );
}

// --- what recovery leaves on the asset (#446) --------------------------------

/// One asset in the estate, and a confirmed `monitored-by` link to a monitor.
///
/// Written by hand rather than through `knobas_app::assets`, which this crate
/// cannot depend on -- `tests/contexts.rs` in `knobas-core` makes the same
/// trade for the same reason. The relation is the shared constant and not a
/// literal, so a rename reaches this fixture.
async fn watched_asset(pool: &PgPool, monitor_id: &str, name: &str, confirmed: bool) -> String {
    let id = format!("asset:{name}");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,'asset',$2)")
        .bind(&id)
        .bind(name)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("insert into knobas.asset (id, type_id, name) values ($1,'vm',$2)")
        .bind(&id)
        .bind(name)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into knobas.link
             (from_id, to_id, relation, origin, created_by, confirmed_at,
              rule, rule_class, reason)
         values ($1,$2,$3,'manual','user',
                 case when $4 then now() end,
                 case when $4 then null else 'branch_name_key' end,
                 case when $4 then null else 'exact_key' end,
                 case when $4 then null else 'a reason' end)",
    )
    .bind(monitor_id)
    .bind(&id)
    .bind(knobas_core::link::MONITORED_BY)
    .bind(confirmed)
    .execute(pool)
    .await
    .unwrap();
    id
}

/// One asset's history, newest first, as `(actor, verb)`.
async fn history(pool: &PgPool, asset_id: &str) -> Vec<(String, String)> {
    sqlx::query_as::<_, (String, String)>(
        "select actor, verb from knobas.activity
          where entity_id = $1 order by at desc, id desc",
    )
    .bind(asset_id)
    .fetch_all(pool)
    .await
    .unwrap()
}

/// Story 64: *"recovery to close the alert and remove an un-acked inbox item
/// **with a history line**"* -- and the negative that makes it a rule rather
/// than a habit: **opening** one writes nothing.
///
/// Three runs, and the history is read after each: up (nothing), down (an
/// alert, and still nothing in the history), up again (one `recovered` line,
/// by `sync:<source>`). Without the third assertion this would pass on an
/// implementation that wrote a line on every reconcile.
#[tokio::test]
async fn recovery_writes_a_history_line_on_the_asset_and_opening_writes_none() {
    let pool = pool("recovery_writes_a_histor").await;
    let id = "kuma".to_owned();
    let (src, corpus) = source(&id, &[Monitor::up("gitea")]);
    let gitea = format!("{id}:gitea");

    knobas_sync::run_once(&pool, &src, None).await.unwrap();
    let asset = watched_asset(&pool, &gitea, "hel1", true).await;
    assert!(history(&pool, &asset).await.is_empty());

    publishes(&corpus, Monitor::down("gitea"));
    knobas_sync::run_once(&pool, &src, None).await.unwrap();
    assert_eq!(open(&pool, &gitea).await.len(), 1, "the alert is open");
    assert!(
        history(&pool, &asset).await.is_empty(),
        "an alert opening is not somebody's act, and the strip already says so"
    );

    publishes(&corpus, Monitor::up("gitea"));
    knobas_sync::run_once(&pool, &src, None).await.unwrap();
    assert!(open(&pool, &gitea).await.is_empty(), "and it closed");
    assert_eq!(
        history(&pool, &asset).await,
        vec![("sync:kuma".to_owned(), knobas_sync::alerts::VERB.to_owned())],
        "what the alert row can no longer say, the asset's history does"
    );
}

/// The line goes on the asset the monitor **is confirmed to watch**, and
/// nowhere else.
///
/// Three assets and one recovery: one confirmed (a line), one whose link is a
/// bare proposal (nothing), and one nothing links to at all (nothing). The
/// proposal is the case a rule reading `knobas.link` instead of
/// `knobas.confirmed_link` would get wrong, and it would be knobas writing a
/// fact into somebody's history out of a guess (#41, #161).
#[tokio::test]
async fn the_recovery_line_lands_only_on_the_assets_the_monitor_is_confirmed_to_watch() {
    let pool = pool("the_recovery_line_lands_").await;
    let id = "kuma".to_owned();
    let (src, corpus) = source(&id, &[Monitor::down("gitea"), Monitor::up("jira")]);
    let gitea = format!("{id}:gitea");

    knobas_sync::run_once(&pool, &src, None).await.unwrap();
    let confirmed = watched_asset(&pool, &gitea, "hel1", true).await;
    let guessed = watched_asset(&pool, &gitea, "hel2", false).await;
    // Watched by a monitor that is not the one recovering.
    let elsewhere = watched_asset(&pool, &format!("{id}:jira"), "fsn1", true).await;

    publishes(&corpus, Monitor::up("gitea"));
    knobas_sync::run_once(&pool, &src, None).await.unwrap();

    assert_eq!(history(&pool, &confirmed).await.len(), 1);
    assert!(
        history(&pool, &guessed).await.is_empty(),
        "a proposed monitored-by link is a guess, not a fact to write down"
    );
    assert!(
        history(&pool, &elsewhere).await.is_empty(),
        "and an asset this monitor does not watch hears nothing"
    );
}

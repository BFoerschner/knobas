//! The per-run log the diagnostics view reads (`knobas.sync_run`).
//!
//! Deliberately not the activity stream: §2a is the user-facing record of what
//! happened to their *work*, carries no durations, and `run_once` writes no
//! line at all for a run that changed nothing. Diagnostics needs exactly the
//! runs §2a drops -- the failures and the no-ops -- and the scheduler needs the
//! last outcome to compute backoff (interfaces §1, §8 P7).

use crate::{SyncError, SyncReport};

closed_vocabulary! {
    /// Why a run happened.
    ///
    /// Stored in `knobas.sync_run.trigger`, whose `sync_run_trigger_chk`
    /// allows exactly these spellings.
    pub enum SyncTrigger {
        /// The scheduler's interval elapsed.
        Schedule => "schedule",
        /// *Sync now*.
        Manual => "manual",
        /// The first sync after a source was added (the first-run wizard's).
        FirstRun => "first_run",
    }
}

closed_vocabulary! {
    /// How a run ended.
    ///
    /// Stored in `knobas.sync_run.outcome`, whose `sync_run_outcome_chk`
    /// allows exactly these spellings. Stream F's backoff branches on it, so a
    /// variant the constraint does not know about is not a display bug: the
    /// `UPDATE` that closes the run is refused, `runner`'s failure path logs
    /// and swallows that, and the row never closes.
    pub enum SyncOutcome {
        Ok => "ok",
        Unauthorized => "unauthorized",
        Unreachable => "unreachable",
        Error => "error",
    }
}

impl SyncOutcome {
    /// Classify a failed run.
    ///
    /// The same three-way split the scheduler's backoff reads: `Unreachable`
    /// and `Error` are retried with an increasing delay, `Unauthorized` never
    /// is -- it needs a human (interfaces §8 P7). Defined here, once, so the
    /// log and the backoff cannot disagree about what a failure was.
    /// Whether this outcome advances the backoff ladder.
    ///
    /// `Unauthorized` does not: P7 gives it **no automatic retry**, because
    /// only a human can fix a rejected credential, and a source that retries
    /// one on a timer is a source that gets an account locked out. `Ok` does
    /// not either, obviously -- it clears the ladder instead.
    #[must_use]
    pub fn backs_off(self) -> bool {
        matches!(self, SyncOutcome::Unreachable | SyncOutcome::Error)
    }

    #[must_use]
    pub fn of(error: &SyncError) -> Self {
        match error {
            SyncError::Source(knobas_source::SourceError::Unauthorized) => {
                SyncOutcome::Unauthorized
            }
            SyncError::Source(knobas_source::SourceError::Unreachable(_)) => {
                SyncOutcome::Unreachable
            }
            _ => SyncOutcome::Error,
        }
    }
}

/// The coarse state one source's syncing is in -- the `sync:state` payload
/// (interfaces §2.3).
///
/// **Coarse by rule.** Roadmap §4: events are not for throughput. At most a
/// handful of these per run -- a transition each -- and per-item progress goes
/// on a [`Channel`](crate::progress::ProgressSink) and nowhere else.
///
/// Seeded here rather than by stream F because the contract PR emits the first
/// `sync:state` and an event needs a payload type; F extends it (and fills the
/// two scheduler fields) rather than defining a second one.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SourceSyncStatus {
    pub source_id: String,
    /// Whether a run is in flight right now.
    pub running: bool,
    /// The run this status is about, when there is one.
    pub run_id: Option<i64>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_finished_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_outcome: Option<SyncOutcome>,
    /// Derived from the interval and the previous run's `finished_at`
    /// (ruling P7). **Always `None` until stream F's scheduler exists** --
    /// there is no schedule to derive it from yet.
    pub next_run_at: Option<chrono::DateTime<chrono::Utc>>,
    /// `knobas.source_config.backoff_until`. **Always `None` until stream F**,
    /// which is what writes and honours it.
    pub backoff_until: Option<chrono::DateTime<chrono::Utc>>,
}

impl SourceSyncStatus {
    /// A run has just been recorded and is about to execute.
    #[must_use]
    pub fn started(source_id: &str, run_id: i64) -> Self {
        Self {
            source_id: source_id.to_owned(),
            running: true,
            run_id: Some(run_id),
            started_at: Some(chrono::Utc::now()),
            last_finished_at: None,
            last_outcome: None,
            next_run_at: None,
            backoff_until: None,
        }
    }

    /// A run has ended, one way or the other.
    #[must_use]
    pub fn finished(source_id: &str, run_id: i64, outcome: SyncOutcome) -> Self {
        Self {
            source_id: source_id.to_owned(),
            running: false,
            run_id: Some(run_id),
            started_at: None,
            last_finished_at: Some(chrono::Utc::now()),
            last_outcome: Some(outcome),
            next_run_at: None,
            backoff_until: None,
        }
    }
}

/// What a finished run wrote, in the log's own units.
#[derive(Debug, Clone, Default)]
pub struct RunCounts {
    pub upserted: i64,
    pub deleted: i64,
    /// Rows the full-sync sweep tombstoned -- `SyncReport::swept`. Zero for an
    /// incremental run and for an adapter whose full sync is not exhaustive.
    pub swept: i64,
    pub cursor_after: Option<String>,
}

impl RunCounts {
    /// The counts a successful [`SyncReport`] carries.
    #[must_use]
    pub fn of(report: &SyncReport) -> Self {
        Self {
            upserted: i64::try_from(report.upserted).unwrap_or(i64::MAX),
            deleted: i64::try_from(report.deleted).unwrap_or(i64::MAX),
            swept: i64::try_from(report.swept).unwrap_or(i64::MAX),
            cursor_after: Some(report.cursor.clone()),
        }
    }
}

/// Open a run and return its id.
///
/// Written before the adapter is touched, so a run that hangs or crashes is
/// still visible -- `finished_at is null` is what "running" means, and
/// `sync_run_running_idx` is the index for it.
///
/// # Errors
///
/// [`sqlx::Error`] if the insert fails.
pub async fn start(
    pool: &sqlx::PgPool,
    source_id: &str,
    trigger: SyncTrigger,
) -> Result<i64, sqlx::Error> {
    let (id,): (i64,) = sqlx::query_as(
        "insert into knobas.sync_run (source_id, trigger) values ($1, $2) returning id",
    )
    .bind(source_id)
    .bind(trigger.as_str())
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// Close a run.
///
/// # Errors
///
/// [`sqlx::Error`] if the update fails. Callers on a *failure* path must not
/// let this mask the failure they are reporting -- log it and return the
/// original error.
pub async fn finish(
    pool: &sqlx::PgPool,
    run_id: i64,
    outcome: SyncOutcome,
    counts: &RunCounts,
    error: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "update knobas.sync_run
            set finished_at = now(), outcome = $2, upserted = $3, deleted = $4,
                swept = $5, error = $6, cursor_after = $7
          where id = $1",
    )
    .bind(run_id)
    .bind(outcome.as_str())
    .bind(counts.upserted)
    .bind(counts.deleted)
    .bind(counts.swept)
    .bind(error)
    .bind(counts.cursor_after.as_deref())
    .execute(pool)
    .await?;
    Ok(())
}

/// How many runs per source the log keeps.
///
/// Older ones are pruned after a finished run: the log is a diagnostic, not an
/// archive, and a five-minute schedule would otherwise reach 288 rows per
/// source per day for ever.
pub const KEEP_RUNS: i64 = 200;

/// One row of the sync log, as the diagnostics view reads it.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SyncRunRow {
    pub id: i64,
    pub source_id: String,
    pub trigger: SyncTrigger,
    pub started_at: chrono::DateTime<chrono::Utc>,
    /// `None` while the run is in flight -- which is what "running" means.
    pub finished_at: Option<chrono::DateTime<chrono::Utc>>,
    pub outcome: Option<SyncOutcome>,
    pub upserted: i64,
    pub deleted: i64,
    pub swept: i64,
    pub error: Option<String>,
    pub cursor_after: Option<String>,
}

/// Parse a stored `trigger`.
///
/// Anything unrecognised reads as [`SyncTrigger::Schedule`]: a row written by
/// a newer knobas must not break the diagnostics list. Driven by `ALL`, so a
/// variant is readable the moment it is writable.
fn trigger_from_db(raw: &str) -> SyncTrigger {
    SyncTrigger::ALL
        .iter()
        .copied()
        .find(|t| t.as_str() == raw)
        .unwrap_or(SyncTrigger::Schedule)
}

/// Parse a stored `outcome`. As [`trigger_from_db`], defaulting to
/// [`SyncOutcome::Error`] -- an outcome this version cannot name is not a
/// success, and must not clear a backoff ladder.
fn outcome_from_db(raw: &str) -> SyncOutcome {
    SyncOutcome::ALL
        .iter()
        .copied()
        .find(|o| o.as_str() == raw)
        .unwrap_or(SyncOutcome::Error)
}

#[derive(sqlx::FromRow)]
struct RawRun {
    id: i64,
    source_id: String,
    trigger: String,
    started_at: chrono::DateTime<chrono::Utc>,
    finished_at: Option<chrono::DateTime<chrono::Utc>>,
    outcome: Option<String>,
    upserted: i64,
    deleted: i64,
    swept: i64,
    error: Option<String>,
    cursor_after: Option<String>,
}

impl From<RawRun> for SyncRunRow {
    fn from(r: RawRun) -> Self {
        SyncRunRow {
            id: r.id,
            source_id: r.source_id,
            trigger: trigger_from_db(&r.trigger),
            started_at: r.started_at,
            finished_at: r.finished_at,
            outcome: r.outcome.as_deref().map(outcome_from_db),
            upserted: r.upserted,
            deleted: r.deleted,
            swept: r.swept,
            error: r.error,
            cursor_after: r.cursor_after,
        }
    }
}

/// Every column the readers below select, named. Never `select *` -- the
/// project rule, and here it also keeps the column list in one place instead of
/// in three statements that drift.
const RUN_COLUMNS: &str = "id, source_id, trigger, started_at, finished_at, outcome,
                           upserted, deleted, swept, error, cursor_after";

/// The `limit` newest runs, newest first -- every source, or one.
///
/// One statement for both cases: `$1 is null` makes the filter optional without
/// building SQL. Ordered by `(started_at desc, id desc)`, which is
/// `sync_run_source_idx`'s order for the filtered read; `id` breaks ties so two
/// runs started in the same microsecond still have a stable order.
///
/// # Errors
/// [`sqlx::Error`] if the query fails.
pub async fn list(
    pool: &sqlx::PgPool,
    source_id: Option<&str>,
    limit: i64,
) -> Result<Vec<SyncRunRow>, sqlx::Error> {
    let sql = format!(
        "select {RUN_COLUMNS} from knobas.sync_run
          where ($1::text is null or source_id = $1)
          order by started_at desc, id desc
          limit $2"
    );
    let rows: Vec<RawRun> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(source_id)
        .bind(limit)
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// The newest **finished** run for a source -- what
/// [`SourceSyncStatus::last_outcome`] reads.
///
/// `finished_at is not null` is load-bearing here, unlike in the schedule's
/// lateral: this is a `limit 1` over an ordering, not a `max()`, so an open run
/// really would shadow the finished one underneath it and the status strip
/// would go blank for the duration of every sync.
///
/// # Errors
/// [`sqlx::Error`] if the query fails.
pub async fn last_finished(
    pool: &sqlx::PgPool,
    source_id: &str,
) -> Result<Option<SyncRunRow>, sqlx::Error> {
    let sql = format!(
        "select {RUN_COLUMNS} from knobas.sync_run
          where source_id = $1 and finished_at is not null
          order by started_at desc, id desc limit 1"
    );
    let row: Option<RawRun> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(source_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(Into::into))
}

/// How many **finished** runs there have been since the last successful one --
/// the rung of P7's backoff ladder, derived rather than stored.
///
/// A counter column would be a second truth that a crashed process, a manual
/// sync or a pruned log immediately falsifies. A run still in flight is
/// excluded: a slow source is not a failing one, and counting it would double
/// the backoff of a source that is merely taking its time.
///
/// # Errors
/// [`sqlx::Error`] if the query fails.
pub async fn failures_since_last_ok(
    pool: &sqlx::PgPool,
    source_id: &str,
) -> Result<i64, sqlx::Error> {
    let (n,): (i64,) = sqlx::query_as(
        "select count(*)
           from knobas.sync_run
          where source_id = $1
            and finished_at is not null
            and id > coalesce((select max(id) from knobas.sync_run
                                where source_id = $1 and outcome = 'ok'), 0)",
    )
    .bind(source_id)
    .fetch_one(pool)
    .await?;
    Ok(n)
}

/// Keep the newest `keep` runs for one source; return how many went.
///
/// A `not in` over a bounded subquery rather than an id-arithmetic trick: the
/// keep set is at most a few hundred ids, `sync_run_source_idx` serves the
/// ordering, and "keep these, delete the rest" is what the sentence says.
///
/// # Errors
/// [`sqlx::Error`] if the delete fails.
pub async fn prune(pool: &sqlx::PgPool, source_id: &str, keep: i64) -> Result<u64, sqlx::Error> {
    let done = sqlx::query(
        "delete from knobas.sync_run
          where source_id = $1
            and id not in (
                  select id from knobas.sync_run
                   where source_id = $1
                   order by started_at desc, id desc
                   limit $2)",
    )
    .bind(source_id)
    .bind(keep)
    .execute(pool)
    .await?;
    Ok(done.rows_affected())
}

/// Close every run left open by a process that is no longer running.
///
/// Called once at scheduler start, before the first tick. Nothing in a killed
/// process can close its own row, so an open row outlives it and makes
/// `sync_status` report a source as running for ever, and the diagnostics view
/// carry a run that never ended. Uses `sync_run_running_idx`.
///
/// **Whole-database, deliberately**: knobas runs one scheduler per profile, and
/// at the moment this is called no run of this process exists yet, so every
/// open row is by definition abandoned. That is also why it must run *before*
/// the first tick -- afterwards it would close live runs.
///
/// `coalesce(error, …)` rather than an overwrite: a run that recorded why it
/// was struggling before the process died keeps that, which is the more useful
/// half of the diagnosis.
///
/// # Errors
/// [`sqlx::Error`] if the update fails.
pub async fn reconcile_abandoned(pool: &sqlx::PgPool) -> Result<u64, sqlx::Error> {
    let done = sqlx::query(
        "update knobas.sync_run
            set finished_at = now(),
                outcome = $1,
                error = coalesce(error, 'interrupted: knobas exited while this run was in flight')
          where finished_at is null",
    )
    .bind(SyncOutcome::Error.as_str())
    .execute(pool)
    .await?;
    Ok(done.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The wire spelling is the stored spelling, for both enums: the columns
    /// are plain `text` with the vocabulary in a comment (migration 0002), so
    /// nothing but this keeps a serialized outcome and a written one the same
    /// string.
    #[test]
    fn the_wire_spellings_are_the_stored_spellings() {
        for trigger in SyncTrigger::ALL {
            assert_eq!(
                serde_json::to_value(trigger).unwrap(),
                serde_json::json!(trigger.as_str()),
                "{trigger:?}"
            );
        }
        for outcome in SyncOutcome::ALL {
            assert_eq!(
                serde_json::to_value(outcome).unwrap(),
                serde_json::json!(outcome.as_str()),
                "{outcome:?}"
            );
        }
    }

    /// **Every variant is a spelling migration 0002's CHECK allows, and every
    /// spelling it allows is a variant.**
    ///
    /// Driven by `ALL`, which the `closed_vocabulary!` macro generates from
    /// the same list as the variants -- so a variant cannot be added without
    /// reaching this test. Adding `SyncOutcome::Cancelled` fails here rather
    /// than at the `UPDATE` that closes a run, which is the failure that
    /// matters: `runner` logs and swallows a failed `finish`, so the row would
    /// simply never close and the diagnostics view would show a run that is
    /// still going, for ever.
    ///
    /// Both directions, because "the constraint allows a spelling nothing can
    /// produce" is dead vocabulary that the next reader has to reason about.
    #[test]
    fn the_vocabularies_are_exactly_what_the_migration_allows() {
        let migration = include_str!("../../knobas-db/migrations/0002_m1_cockpit.sql");

        for (constraint, spellings) in [
            (
                "sync_run_trigger_chk",
                SyncTrigger::ALL
                    .iter()
                    .map(|t| t.as_str())
                    .collect::<Vec<_>>(),
            ),
            (
                "sync_run_outcome_chk",
                SyncOutcome::ALL
                    .iter()
                    .map(|o| o.as_str())
                    .collect::<Vec<_>>(),
            ),
        ] {
            let line = migration
                .lines()
                .find(|line| line.contains(constraint))
                .unwrap_or_else(|| panic!("{constraint} is missing from 0002"));

            for spelling in &spellings {
                assert!(
                    line.contains(&format!("'{spelling}'")),
                    "{spelling:?} is a variant but {constraint} does not allow it: {line}"
                );
            }
            // And nothing the enum cannot produce.
            let allowed = line.matches('\'').count() / 2;
            assert_eq!(
                allowed,
                spellings.len(),
                "{constraint} lists {allowed} spellings but the enum has {}: {line}",
                spellings.len()
            );
        }
    }

    /// The classification stream F's backoff reads. `Unauthorized` is the one
    /// that must never be retried, so it is the one that must never be
    /// swallowed into `Error`.
    #[test]
    fn a_failure_is_classified_by_what_the_adapter_said() {
        use knobas_source::SourceError;

        let cases = [
            (SourceError::Unauthorized, SyncOutcome::Unauthorized),
            (
                SourceError::Unreachable("dns".to_owned()),
                SyncOutcome::Unreachable,
            ),
            (
                SourceError::Protocol("bad json".to_owned()),
                SyncOutcome::Error,
            ),
            (SourceError::Sink("rejected".to_owned()), SyncOutcome::Error),
        ];
        for (error, expected) in cases {
            let described = format!("{error:?}");
            assert_eq!(
                SyncOutcome::of(&SyncError::Source(error)),
                expected,
                "{described}"
            );
        }

        // Not the adapter's failure at all: a statement the engine issued.
        assert_eq!(
            SyncOutcome::of(&SyncError::BadSourceId {
                id: "jira:eu".to_owned(),
                reason: "contains ':'",
            }),
            SyncOutcome::Error
        );
    }

    /// The stored-value readers, both directions and both defaults.
    ///
    /// The defaults are the point. `outcome_from_db` falling back to
    /// [`SyncOutcome::Ok`] instead of [`SyncOutcome::Error`] would make a row
    /// this version cannot name read as a *success* -- which clears the
    /// backoff ladder, so a source failing with an outcome written by a newer
    /// knobas would be retried at full rate for ever. That mutant survived the
    /// first round of this PR because nothing asserted the fallback; it is the
    /// same shape `config::auth_state_from_db` already had a test for.
    #[test]
    fn a_stored_value_round_trips_and_an_unrecognised_one_degrades_safely() {
        for outcome in SyncOutcome::ALL {
            assert_eq!(outcome_from_db(outcome.as_str()), *outcome);
        }
        for trigger in SyncTrigger::ALL {
            assert_eq!(trigger_from_db(trigger.as_str()), *trigger);
        }

        // An outcome this version cannot name is **not** a success.
        for unknown in ["cancelled", "written_by_a_newer_knobas", ""] {
            assert_eq!(
                outcome_from_db(unknown),
                SyncOutcome::Error,
                "{unknown:?} must not read as a success: it would clear the \
                 backoff ladder"
            );
            assert!(
                outcome_from_db(unknown).backs_off()
                    || outcome_from_db(unknown) == SyncOutcome::Unauthorized,
                "an unnameable outcome must still be a failure"
            );
        }

        // A trigger this version cannot name is cosmetic -- it only labels a
        // row in the diagnostics list -- so it degrades to the commonest one
        // rather than refusing to list the run at all.
        for unknown in ["webhook", "written_by_a_newer_knobas", ""] {
            assert_eq!(trigger_from_db(unknown), SyncTrigger::Schedule);
        }
    }

    /// **The enum half of the `SourceSyncStatus` mirror pin** (M0 carry-over,
    /// stream F). The twin of `health::the_states_match_their_typescript_mirror`
    /// and, like it, driven by `::ALL` -- so a variant added to either
    /// vocabulary reaches this test rather than reaching a frontend `switch`
    /// that can never take the new branch.
    ///
    /// Both enums, because both are `SourceSyncStatus`/`SyncRunRow` fields on
    /// the wire: `last_outcome` is what puts *Re-enter password* on a row, and
    /// `trigger` is what the diagnostics list labels a run with.
    #[test]
    fn the_run_vocabularies_match_their_typescript_mirror() {
        let mirror = include_str!("../../../app/src/lib/ipc/sources.ts");
        for outcome in SyncOutcome::ALL {
            let wire = serde_json::to_string(&outcome).unwrap();
            assert!(
                mirror.contains(&wire),
                "SyncOutcome {wire} is missing from app/src/lib/ipc/sources.ts"
            );
        }
        for trigger in SyncTrigger::ALL {
            let wire = serde_json::to_string(&trigger).unwrap();
            assert!(
                mirror.contains(&wire),
                "SyncTrigger {wire} is missing from app/src/lib/ipc/sources.ts"
            );
        }
    }

    /// **The struct half** (M0 carry-over, stream F): the *exact* key set
    /// `SourceSyncStatus` puts on the wire, then each key in the mirror.
    ///
    /// Modelled on `knobas_search::types::the_hit_shape_matches_its_typescript_mirror`
    /// rather than on a `contains` walk over a hardcoded list, and the
    /// difference is the whole reason this exists: a list-walking test sees
    /// only the fields someone remembered to list, so a Rust field added with
    /// no TS counterpart is invisible to it. Serializing an instance and
    /// asserting the key set makes the *addition* the failure, which is the
    /// direction stream F breaks first -- it fills `next_run_at` and
    /// `backoff_until` and will add fields.
    #[test]
    fn the_sync_status_shape_matches_its_typescript_mirror() {
        let mirror = include_str!("../../../app/src/lib/ipc/sources.ts");
        let status = SourceSyncStatus {
            source_id: "mock".to_owned(),
            running: false,
            run_id: Some(7),
            started_at: Some(chrono::Utc::now()),
            last_finished_at: Some(chrono::Utc::now()),
            last_outcome: Some(SyncOutcome::Ok),
            next_run_at: Some(chrono::Utc::now()),
            backoff_until: Some(chrono::Utc::now()),
        };

        let wire = serde_json::to_value(&status).expect("a status serializes");
        let object = wire.as_object().expect("a status is a JSON object");
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "backoff_until",
                "last_finished_at",
                "last_outcome",
                "next_run_at",
                "run_id",
                "running",
                "source_id",
                "started_at",
            ],
            "SourceSyncStatus grew or lost a field; app/src/lib/ipc/sources.ts \
             has to grow or lose it too"
        );

        for key in &keys {
            assert!(
                mirror.contains(&format!("{key}:")),
                "SourceSyncStatus.{key} is missing from app/src/lib/ipc/sources.ts"
            );
        }
    }

    /// A run that opened and never closed is what `finished_at is null` means,
    /// and the diagnostics view renders it as *running*. Both halves in one
    /// test, because the interesting thing is the transition between them.
    #[tokio::test]
    async fn a_run_is_open_until_it_is_finished() {
        let pool = knobas_db::test_util::test_pool().await;
        knobas_db::migrate::run(&pool).await.unwrap();
        let source_id = format!("run-log-{}", uuid::Uuid::new_v4());

        let run_id = start(&pool, &source_id, SyncTrigger::FirstRun)
            .await
            .unwrap();
        let open: (String, Option<String>, Option<String>) = sqlx::query_as(
            "select trigger, outcome, error from knobas.sync_run
              where id = $1 and finished_at is null",
        )
        .bind(run_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(open, ("first_run".to_owned(), None, None));

        finish(
            &pool,
            run_id,
            SyncOutcome::Unauthorized,
            &RunCounts {
                upserted: 3,
                deleted: 1,
                swept: 2,
                cursor_after: Some("c-9".to_owned()),
            },
            Some("401"),
        )
        .await
        .unwrap();

        let closed: (String, i64, i64, i64, Option<String>, Option<String>) = sqlx::query_as(
            "select outcome, upserted, deleted, swept, error, cursor_after
               from knobas.sync_run where id = $1 and finished_at is not null",
        )
        .bind(run_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            closed,
            (
                "unauthorized".to_owned(),
                3,
                1,
                2,
                Some("401".to_owned()),
                Some("c-9".to_owned())
            )
        );
    }
}

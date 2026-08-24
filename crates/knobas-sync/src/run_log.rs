//! The per-run log the diagnostics view reads (`knobas.sync_run`).
//!
//! Deliberately not the activity stream: §2a is the user-facing record of what
//! happened to their *work*, carries no durations, and `run_once` writes no
//! line at all for a run that changed nothing. Diagnostics needs exactly the
//! runs §2a drops -- the failures and the no-ops -- and the scheduler needs the
//! last outcome to compute backoff (interfaces §1, §8 P7).

use crate::{SyncError, SyncReport};

/// Why a run happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncTrigger {
    /// The scheduler's interval elapsed.
    Schedule,
    /// *Sync now*.
    Manual,
    /// The first sync after a source was added (the first-run wizard's).
    FirstRun,
}

impl SyncTrigger {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            SyncTrigger::Schedule => "schedule",
            SyncTrigger::Manual => "manual",
            SyncTrigger::FirstRun => "first_run",
        }
    }
}

/// How a run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncOutcome {
    Ok,
    Unauthorized,
    Unreachable,
    Error,
}

impl SyncOutcome {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            SyncOutcome::Ok => "ok",
            SyncOutcome::Unauthorized => "unauthorized",
            SyncOutcome::Unreachable => "unreachable",
            SyncOutcome::Error => "error",
        }
    }

    /// Classify a failed run.
    ///
    /// The same three-way split the scheduler's backoff reads: `Unreachable`
    /// and `Error` are retried with an increasing delay, `Unauthorized` never
    /// is -- it needs a human (interfaces §8 P7). Defined here, once, so the
    /// log and the backoff cannot disagree about what a failure was.
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

/// What a finished run wrote, in the log's own units.
#[derive(Debug, Clone, Default)]
pub struct RunCounts {
    pub upserted: i64,
    pub deleted: i64,
    /// Rows the full-sync sweep tombstoned. Stream F's; zero until then.
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
            swept: 0,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The wire spelling is the stored spelling, for both enums: the columns
    /// are plain `text` with the vocabulary in a comment (migration 0002), so
    /// nothing but this keeps a serialized outcome and a written one the same
    /// string.
    #[test]
    fn the_wire_spellings_are_the_stored_spellings() {
        for trigger in [
            SyncTrigger::Schedule,
            SyncTrigger::Manual,
            SyncTrigger::FirstRun,
        ] {
            assert_eq!(
                serde_json::to_value(trigger).unwrap(),
                serde_json::json!(trigger.as_str()),
                "{trigger:?}"
            );
        }
        for outcome in [
            SyncOutcome::Ok,
            SyncOutcome::Unauthorized,
            SyncOutcome::Unreachable,
            SyncOutcome::Error,
        ] {
            assert_eq!(
                serde_json::to_value(outcome).unwrap(),
                serde_json::json!(outcome.as_str()),
                "{outcome:?}"
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

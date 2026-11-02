//! The latest run log of each pipeline, kept in the index (#2431, spec
//! #2419): what lets the launcher's *Everything* find a pipeline by a word
//! its newest finished run printed.
//!
//! A run's log is read from the source and never mirrored (`Source::run_log`,
//! #854), so the sync fetches the tail of each pipeline's newest finished run
//! after a run of its source committed and stores it in `sync.pipeline_log`
//! (migration `0067`), where the database lexes it once.
//!
//! **Best-effort and bounded.** A sync run is the source's own clock: it ends
//! when its items are in, and the logs follow it. At most [`PER_RUN`] logs a
//! run, inside [`BUDGET`], so the first sync of a TeamCity with four hundred
//! pipelines spreads their logs over the runs after it instead of holding one
//! run open for minutes. A failed read leaves the stored row; the next run
//! tries again. A source whose adapter keeps no runs answers `None` and is
//! left at once.
//!
//! **Only a finished run.** A running build's log grows; storing it would
//! store words the next minute takes back. Until a newer run finishes, the
//! row holds the one before.

use std::collections::HashMap;
use std::time::Duration;

use knobas_source::Source;
use sqlx::PgPool;

/// How many of a log's last lines are kept: where a build says why it ended.
pub const TAIL_LINES: u32 = 400;

/// The most logs one sync run fetches.
pub const PER_RUN: usize = 12;

/// How long one sync run spends fetching logs, at most.
pub const BUDGET: Duration = Duration::from_secs(20);

/// Fetch and store the newest finished run's log of every pipeline of
/// `source_id` whose stored log is of an older run, at most [`PER_RUN`] of
/// them inside [`BUDGET`]. Answers how many it stored.
///
/// # Errors
///
/// A database failure reading the pipelines or storing a log. A source that
/// fails one read is skipped for this run, not an error.
pub async fn refresh(
    pool: &PgPool,
    source_id: &str,
    source: &dyn Source,
) -> Result<usize, sqlx::Error> {
    let owed = owed(pool, source_id).await?;
    if owed.is_empty() {
        return Ok(0);
    }
    let deadline = tokio::time::Instant::now() + BUDGET;
    let mut stored = 0;
    for (pipeline, run) in owed.into_iter().take(PER_RUN) {
        let read = tokio::time::timeout_at(deadline, source.run_log(&run, None, TAIL_LINES)).await;
        let log = match read {
            Err(_) => {
                tracing::debug!(source_id, "the run logs' budget ran out; the rest wait");
                break;
            }
            // This adapter keeps no runs: nothing of it will answer.
            Ok(Ok(None)) => break,
            Ok(Err(error)) => {
                tracing::debug!(source_id, run, %error, "a run's log was not read");
                continue;
            }
            Ok(Ok(Some(log))) => log,
        };
        let first_line = log.lines.first().map_or(1, |line| {
            i64::try_from(line.number.max(1)).unwrap_or(i64::MAX)
        });
        let body = log
            .lines
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
            // A text column refuses NUL, and a build may print one.
            .replace('\0', "");
        store(pool, &pipeline, &run, first_line, &body).await?;
        stored += 1;
    }
    Ok(stored)
}

/// `(pipeline, run)` for every live pipeline of the source whose newest run
/// has finished and is not the run its stored log is of, in the Builds view's
/// order.
///
/// The declarations are the stored ones (`sync.parent_declaration`, written
/// from the registry at start), not the adapter's `descriptor()`: a run reads
/// that once (#1701). A source with no run under any pipeline
/// (`sync.item_pipeline`) costs one index probe and nothing else.
async fn owed(pool: &PgPool, source_id: &str) -> Result<Vec<(String, String)>, sqlx::Error> {
    let stored: Option<sqlx::types::Json<Vec<knobas_core::payload::KindPaths>>> =
        sqlx::query_scalar(
            "select pd.payload_paths
           from knobas.source_config sc
           join sync.parent_declaration pd on pd.adapter_kind = sc.kind
          where sc.id = $1
            and exists (select 1 from sync.item_pipeline ip where ip.source_id = $1)",
        )
        .bind(source_id)
        .fetch_optional(pool)
        .await?;
    let Some(sqlx::types::Json(paths)) = stored else {
        return Ok(Vec::new());
    };
    let declarations = knobas_core::payload::Declarations::empty().with(source_id, paths);
    let pipelines =
        match knobas_core::builds::pipelines(pool, &declarations, &[], &[source_id.to_owned()])
            .await
        {
            Ok(pipelines) => pipelines,
            Err(knobas_core::CoreError::Db(error)) => return Err(error),
            Err(error) => {
                tracing::debug!(source_id, %error, "the pipelines were not read for their logs");
                return Ok(Vec::new());
            }
        };
    let newest: Vec<(String, String)> = pipelines
        .into_iter()
        .filter_map(|pipeline| {
            let run = pipeline.recent.into_iter().next()?;
            run.finished?;
            Some((pipeline.entity_id, run.entity_id))
        })
        .collect();
    if newest.is_empty() {
        return Ok(newest);
    }
    let ids: Vec<&str> = newest
        .iter()
        .map(|(pipeline, _)| pipeline.as_str())
        .collect();
    let stored: HashMap<String, String> = sqlx::query_as::<_, (String, String)>(
        "select pipeline_id, run_id from sync.pipeline_log where pipeline_id = any($1)",
    )
    .bind(&ids)
    .fetch_all(pool)
    .await?
    .into_iter()
    .collect();
    Ok(newest
        .into_iter()
        .filter(|(pipeline, run)| stored.get(pipeline) != Some(run))
        .collect())
}

/// Keep `body` as `pipeline`'s latest log, replacing whatever run's it held.
async fn store(
    pool: &PgPool,
    pipeline: &str,
    run: &str,
    first_line: i64,
    body: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "insert into sync.pipeline_log (pipeline_id, run_id, first_line, body)
         select $1, $2, $3, $4 where exists (select 1 from sync.item where entity_id = $1)
         on conflict (pipeline_id) do update
           set run_id = excluded.run_id, first_line = excluded.first_line,
               body = excluded.body, fetched_at = now()",
    )
    .bind(pipeline)
    .bind(run)
    .bind(first_line)
    .bind(body)
    .execute(pool)
    .await?;
    Ok(())
}

//! The failed runs the person marked as read (ux18, migration `0098`).
//!
//! Björn on PR #2479: *"I want to be able to mark builds as read. Single ones
//! as well as all. Since I might not care about old failed runs."*
//!
//! **Who can be unread.** A finished run whose declared state reads `err`, with
//! no row in `knobas.run_read`. Every other run is read by its nature. A new
//! run is a new entity with no row, so a new failure is unread by
//! construction; a run stays unread until it is marked, whatever its pipeline
//! did since (the pipeline's alarm is its newest run, which the shell reads).
//!
//! **One notion with the Inbox.** *Read* is the row and nothing else. The
//! Inbox's failed build rule leaves out a run with a row
//! (`crate::inbox`'s `failed_build!`), and its *Done* on a failed build writes
//! the row in the same transaction ([`mark_with`]). *Mark as unread* deletes
//! the row and the line's *Done*, so the line comes back; a snooze stays.
//!
//! Knobas-local, never the source's, and kept across syncs and restarts.

use sqlx::PgPool;

use crate::CoreError;

/// The ids of the live runs that have a row: what the shell holds as its set.
/// A row whose run left the mirror is not answered.
const READ_RUNS: &str = "select r.entity_id
   from knobas.run_read r
  where exists (select 1 from knobas.entity e
                 where e.id = r.entity_id and e.deleted_at is null)";

/// The finished failed runs of the pipelines in the relation `$pipelines`
/// (aliased `t`, with `source_id`, `adapter_kind` and `pkey`), the
/// declarations `$1` bound: a `from … where …` that selects over `t`, `ip`
/// (the run's `sync.item_pipeline` row), `i` (its `sync.item`) and `ru`.
/// Each run by its stored pipeline range (`sync.item_pipeline`, as
/// `crate::newest_runs!` reads it), and its state and finish as
/// [`crate::builds`] reads a run: the stored row where the bound declarations
/// wrote one, the walk where not. Live runs only.
///
/// One definition for what a pipeline's mark writes ([`MARK_READ`]) and for
/// the unread failures the Builds list names (`crate::builds::PIPELINES`,
/// #2543), so the rail's count is what its press marks.
#[macro_export]
macro_rules! failed_runs_of {
    ($pipelines:literal) => {
        concat!(
            "
    from ",
            $pipelines,
            " t
    join sync.item_pipeline ip
      on ip.source_id = t.source_id and ip.pkey = t.pkey and ip.adapter_kind = t.adapter_kind
    join sync.item i on i.entity_id = ip.entity_id",
            $crate::run_join!("ru", "i"),
            "
   where exists (select 1 from knobas.entity e
                  where e.id = ip.entity_id and e.deleted_at is null)
     and ",
            $crate::row_or!("ru", "state", $crate::declared_state!("$1")),
            "->>'tone' = 'err'
     and ",
            $crate::row_or!("ru", "finished", $crate::declared_string!("$1", "finished")),
            " is not null"
        )
    };
}

/// Mark read: `$3`, runs by id, and every finished failed run of the live
/// pipelines `$2` (`$1` the declarations), as [`failed_runs_of!`] finds
/// them. Answers the ids it wrote: a run
/// already read is not among them, so *Undo* puts back exactly what this did.
const MARK_READ: &str = concat!(
    "with ",
    crate::frame_kinds_cte!(),
    ", ",
    crate::run_keys!("$1"),
    ",
target as (
  select i.source_id, s.kind as adapter_kind, ",
    crate::declared_string!("$1", "pipeline"),
    " as pkey
    from frames f
    join sync.live_item i on i.source_id = f.source_id and i.kind = f.kind
    left join knobas.source_config s on s.id = i.source_id
   where f.frame = 'pipeline' and i.entity_id = any($2::text[])
),
failed as (
  select i.entity_id",
    crate::failed_runs_of!("target"),
    "
),
wanted as (
  select f.entity_id from failed f
  union
  select w.id from unnest($3::text[]) as w(id) where nullif(w.id, '') is not null
)
insert into knobas.run_read (entity_id)
select w.entity_id from wanted w
on conflict (entity_id) do nothing
returning entity_id"
);

/// Mark unread: the rows of `$1` go, and so does each one's Inbox *Done*
/// (`failed_build:<id>`), so its line comes back. Answers the ids that had a
/// row.
const MARK_UNREAD: &str = "with gone as (
  delete from knobas.run_read r where r.entity_id = any($1::text[]) returning r.entity_id
), undone as (
  delete from knobas.inbox_state s
   where s.item_key = any(array(select 'failed_build:' || id from unnest($1::text[]) as u(id)))
     and s.done_at is not null
)
select g.entity_id from gone g";

/// The ids of the live runs marked read.
///
/// # Errors
///
/// [`CoreError::Db`] if the read fails.
pub async fn read_runs(pool: &PgPool) -> Result<Vec<String>, CoreError> {
    Ok(sqlx::query_scalar(READ_RUNS).fetch_all(pool).await?)
}

/// Mark `runs` read, and every finished failed run of `pipelines`. Answers the
/// ids it wrote, which a run already read is not among.
///
/// # Errors
///
/// [`CoreError::Db`] if the write fails.
pub async fn mark_read(
    pool: &PgPool,
    declarations: &crate::payload::Declarations,
    runs: &[String],
    pipelines: &[String],
) -> Result<Vec<String>, CoreError> {
    Ok(sqlx::query_scalar(MARK_READ)
        .bind(declarations.as_param())
        .bind(pipelines)
        .bind(runs)
        .fetch_all(pool)
        .await?)
}

/// How many of `runs` are marked read: what the top bar's count takes away
/// from the failing pipelines' newest runs.
///
/// # Errors
///
/// [`CoreError::Db`] if the read fails.
pub async fn count_read(pool: &PgPool, runs: &[String]) -> Result<i64, CoreError> {
    Ok(
        sqlx::query_scalar(
            "select count(*) from knobas.run_read where entity_id = any($1::text[])",
        )
        .bind(runs)
        .fetch_one(pool)
        .await?,
    )
}

/// The run `run` read, against an executor the caller chooses: the Inbox's
/// *Done* on a failed build writes it in the transaction of its own answer
/// (`crate::inbox::complete_with`'s shape).
///
/// # Errors
///
/// [`CoreError::Db`] if the write fails.
pub async fn mark_with<'e, E>(executor: E, run: &str) -> Result<(), CoreError>
where
    E: sqlx::PgExecutor<'e>,
{
    sqlx::query(
        "insert into knobas.run_read (entity_id) values ($1) on conflict (entity_id) do nothing",
    )
    .bind(run)
    .execute(executor)
    .await?;
    Ok(())
}

/// Mark `runs` unread, and take back each one's Inbox *Done*. Answers the ids
/// that were read.
///
/// # Errors
///
/// [`CoreError::Db`] if the write fails.
pub async fn mark_unread(pool: &PgPool, runs: &[String]) -> Result<Vec<String>, CoreError> {
    Ok(sqlx::query_scalar(MARK_UNREAD)
        .bind(runs)
        .fetch_all(pool)
        .await?)
}

/// The run a failed build's Inbox key names (`failed_build:<run id>`), `None`
/// for any other key.
#[must_use]
pub fn failed_build_run(item_key: &str) -> Option<&str> {
    item_key
        .strip_prefix("failed_build:")
        .filter(|run| !run.is_empty())
}

#[cfg(test)]
mod tests {
    use super::failed_build_run;

    #[test]
    fn a_failed_builds_key_names_its_run_and_no_other_key_does() {
        assert_eq!(
            failed_build_run("failed_build:teamcity:build:1187"),
            Some("teamcity:build:1187")
        );
        assert_eq!(failed_build_run("failed_build:"), None);
        assert_eq!(failed_build_run("mention:jira:PAY-1"), None);
        assert_eq!(failed_build_run("alert:kuma:monitor:3"), None);
    }
}

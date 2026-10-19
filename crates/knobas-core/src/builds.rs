//! The Builds frame's mirror reads (ADR-0022, #848; moved here by #1318,
//! spec #1303): the pipelines, the runs of a pipeline, the commits a run
//! names, the frame-kinds CTE and the run columns, beside the entity list
//! read ([`crate::entity_list`]).
//!
//! What needs an adapter stays in the app crate: a run's log, tests,
//! artifacts and chain, a pipeline's settings and older runs, and the live
//! read that tells two mirrored commits of one sha apart (#1144). So do the
//! declarations and the configured identity, which every read here takes as
//! a parameter.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};

use crate::CoreError;
use crate::entity::EntityRef;

/// One run of a pipeline, as the Builds view draws it (#848): every field
/// read through the run kind's declared paths (ADR-0007), and nothing else.
///
/// `started` and `finished` are the declared ISO 8601 instants, basic or
/// extended (`crate::payload::parse_instant`); a run in flight has no
/// `finished`, a queued one neither. `duration_seconds` is the one from the
/// other, only where both are read.
///
/// On the wire `started` and `finished` are RFC 3339, `None` while queued or in
/// flight.
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[cfg_attr(feature = "test-util", derive(serde::Deserialize))]
#[ts(export_to = "entity.ts")]
pub struct RunRow {
    pub entity_id: String,
    /// The run's number in the source's own spelling; a queued TeamCity
    /// build has none yet.
    pub number: Option<String>,
    /// The declared state chip (#666): its text in the source's word and the
    /// tone the shell draws it in.
    pub state: Option<crate::payload::StateChip>,
    pub branch: Option<String>,
    pub started: Option<DateTime<Utc>>,
    pub finished: Option<DateTime<Utc>>,
    /// Whole seconds, only where both instants were read.
    pub duration_seconds: Option<i64>,
    /// The source's own timestamp for the row, what runs are ordered by.
    pub updated_at: Option<DateTime<Utc>>,
    pub web_url: Option<String>,
    /// How the run was started (#961): the declared `trigger`, in the
    /// shell's words.
    ///
    /// `None` where the declaration misses.
    pub trigger: Option<crate::payload::Trigger>,
    /// Who started it, as the source spells the account: the declared
    /// `started_by`. A run a change or a schedule started has none.
    pub started_by: Option<String>,
    /// Whether `started_by` is one of the configured usernames, the identity
    /// behind `@me` (*Started by me*, *you*).
    pub started_by_me: bool,
    /// The commit it built: the first of the declared `revision`s (#710), a
    /// full sha.
    pub revision: Option<String>,
    /// The mirrored commit `revision` names (#1004): the live item of kind
    /// `commit` whose declared `commit` is that sha, what the run page's
    /// *Commit* opens. `None` where the mirror holds no such commit.
    pub commit_id: Option<String>,
    /// The machine it ran on: the declared `agent` (#1004). A queued run has
    /// none.
    pub agent: Option<String>,
    /// What it is doing now, in the source's words: the declared
    /// `current_step` (#1004). Only a run in flight has one.
    pub current_step: Option<String>,
}

/// One pipeline in the Builds list (R-9.4a, #848).
///
/// `source_name` is the configured source's display name: the one place the
/// source's word appears, in the list's header line (ADR-0022). `recent` is
/// its last runs newest first, at most [`PIPELINE_RECENT`]: the strip, the
/// latest time, and whether the latest run failed.
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[cfg_attr(feature = "test-util", derive(serde::Deserialize))]
#[ts(export_to = "entity.ts")]
pub struct PipelineRow {
    pub entity_id: String,
    pub source_id: String,
    pub source_name: String,
    pub name: String,
    pub group: Option<String>,
    /// The chain of groups it sits in, outermost first (#849): the declared
    /// `group_path`. Empty where the source declares none or the record
    /// carries none; the list then draws it under its `group`.
    pub path: Vec<String>,
    /// Whether the reader pinned it (#849): knobas-local, never the
    /// source's ([`PINS_SETTING`]).
    pub pinned: bool,
    pub web_url: Option<String>,
    pub recent: Vec<RunRow>,
}

/// The `knobas.setting` key holding the pinned pipelines (#849, R-9.3): a
/// JSON array of entity ids. In `knobas.setting` rather than a table of its
/// own, for the reason the clones root is (`0002`, comment 6): a short list
/// of app-level state with no other home. [`PIPELINES`] spells it as a
/// literal, and `the_pins_are_read_by_their_constant` holds the two together.
pub const PINS_SETTING: &str = "builds.pins";

/// How many runs a pipeline row carries: the strip of the last twelve.
pub const PIPELINE_RECENT: i64 = 12;

/// How many runs `pipeline_runs` answers when the caller names no cap.
const PIPELINE_RUNS: i64 = 100;

/// The (source, kind) pairs the declarations put in the Builds frame, with
/// their part: `pipeline` or `run`. `$1` is the declarations. Read once a
/// statement, so the live corpus is joined on its `(source_id, kind)` rather
/// than every row's declaration being looked up.
///
/// Exported for the app's *This job* read (`job_pipelines`), which joins the
/// frame's kinds too and stays beside the job's other reads.
#[macro_export]
macro_rules! frame_kinds_cte {
    () => {
        "frames as (
  select s.key as source_id, k.key as kind, k.value->>'frame' as frame
    from jsonb_each($1::jsonb) s
    cross join lateral jsonb_each(case when jsonb_typeof(s.value) = 'object' then s.value else '{}'::jsonb end) k
   where jsonb_typeof(k.value) = 'object' and k.value->>'frame' in ('pipeline', 'run')
)"
    };
}

/// A run's columns, read through its kind's declarations; the row alias is
/// `i` and the declarations `$1`.
macro_rules! run_columns {
    () => {
        concat!(
            "i.entity_id, i.source_id, i.web_url, i.item_updated_at, ",
            crate::declared_string!("$1", "pipeline"),
            " as pkey, ",
            crate::declared_string!("$1", "number"),
            " as number, ",
            crate::declared_state!("$1"),
            " as state, ",
            crate::declared_string!("$1", "branch"),
            " as branch, ",
            crate::declared_string!("$1", "started"),
            " as started, ",
            crate::declared_string!("$1", "finished"),
            " as finished, ",
            crate::declared_trigger!("$1"),
            " as trig, ",
            crate::declared_string!("$1", "started_by"),
            " as started_by, ",
            crate::declared_string!("$1", "agent"),
            " as agent, ",
            crate::declared_string!("$1", "current_step"),
            " as current_step, (select l.value from ",
            crate::declared_list!("$1", "revision"),
            " l limit 1) as revision"
        )
    };
}

/// Every live pipeline, by name, each with its last [`PIPELINE_RECENT`] runs
/// as a jsonb array newest first. A run sits under the pipeline of its own
/// source whose declared `pipeline` equals its own; a pipeline whose name or
/// identity misses is not drawn. Runs are ordered by the source's own
/// timestamp, newest first. `$3` is the CI role's fillers (#1295): a
/// pipeline of any other source is not drawn, and a run is drawn only under
/// a drawn pipeline.
const PIPELINES: &str = concat!(
    "with ",
    frame_kinds_cte!(),
    ",
p as (
  select i.entity_id, i.source_id, i.web_url, coalesce(c.display_name, i.source_id) as source_name, ",
    crate::declared_string!("$1", "name"),
    " as name, ",
    crate::declared_string!("$1", "group"),
    " as grp, ",
    crate::declared_string!("$1", "pipeline"),
    // The declared chain of groups, in order (#849). One candidate is what a
    // source declares; a second's names would follow the first's.
    " as pkey,
         (select coalesce(jsonb_agg(btrim(e.elem #>> c.entry_segs) order by e.ord), '[]'::jsonb) from ",
    crate::declared_list_candidates!("$1", "group_path"),
    " c cross join lateral jsonb_array_elements(
             case when jsonb_typeof(i.payload #> c.at_segs) = 'array'
                  then i.payload #> c.at_segs else '[]'::jsonb end) with ordinality as e(elem, ord)
           where jsonb_typeof(e.elem #> c.entry_segs) = 'string'
             and nullif(btrim(e.elem #>> c.entry_segs), '') is not null) as path
    from frames f
    join sync.live_item i on i.source_id = f.source_id and i.kind = f.kind
    left join knobas.source_config c on c.id = i.source_id
   where f.frame = 'pipeline' and i.source_id = any($3::text[])
),
pins as (
  select jsonb_array_elements_text(value) as id
    from knobas.setting
   where key = 'builds.pins' and jsonb_typeof(value) = 'array'
),
r as (
  select x.*, row_number() over (partition by x.source_id, x.pkey
                                 order by x.item_updated_at desc nulls last, x.entity_id desc) as rn
    from (select ",
    run_columns!(),
    "
            from frames f
            join sync.live_item i on i.source_id = f.source_id and i.kind = f.kind
           where f.frame = 'run') x
   where x.pkey is not null
)
select p.entity_id, p.source_id, p.source_name, p.name, p.grp, p.web_url, p.path,
       exists (select 1 from pins where pins.id = p.entity_id) as pinned,
       coalesce((select jsonb_agg(jsonb_build_object(
                   'entity_id', r.entity_id, 'number', r.number, 'state', r.state,
                   'branch', r.branch, 'started', r.started, 'finished', r.finished,
                   'updated_at', r.item_updated_at, 'web_url', r.web_url,
                   'trigger', r.trig, 'started_by', r.started_by,
                   'agent', r.agent, 'current_step', r.current_step,
                   'revision', r.revision) order by r.rn)
                   from r
                  where r.source_id = p.source_id and r.pkey = p.pkey and r.rn <= $2),
                '[]'::jsonb) as recent
  from p
 where p.pkey is not null and p.name is not null
 order by lower(p.name), p.entity_id
"
);

/// The runs of the pipeline `$2`, newest first, at most `$3`.
const PIPELINE_RUNS_OF: &str = concat!(
    "with ",
    frame_kinds_cte!(),
    ",
target as (
  select i.source_id, ",
    crate::declared_string!("$1", "pipeline"),
    " as pkey
    from frames f
    join sync.live_item i on i.source_id = f.source_id and i.kind = f.kind
   where f.frame = 'pipeline' and i.entity_id = $2
)
select t.pkey is not null as found, x.* from target t
  left join lateral (
    select ",
    run_columns!(),
    "
      from frames f
      join sync.live_item i on i.source_id = f.source_id and i.kind = f.kind
     where f.frame = 'run' and f.source_id = t.source_id
       and ",
    crate::declared_string!("$1", "pipeline"),
    " = t.pkey
     order by i.item_updated_at desc nulls last, i.entity_id desc
     limit $3) x on true
"
);

/// A run as it leaves a statement or a `recent` array: the declared times
/// still in the source's spelling.
///
/// Public for the app's live reads, which read a run the source answered
/// into one ([`RunRow::read`]) as a mirrored run is.
#[derive(serde::Deserialize)]
pub struct RawRun {
    pub entity_id: String,
    pub number: Option<String>,
    pub state: Option<crate::payload::StateChip>,
    pub branch: Option<String>,
    pub started: Option<String>,
    pub finished: Option<String>,
    pub updated_at: Option<DateTime<Utc>>,
    pub web_url: Option<String>,
    pub trigger: Option<crate::payload::Trigger>,
    pub started_by: Option<String>,
    pub agent: Option<String>,
    pub current_step: Option<String>,
    pub revision: Option<String>,
}

impl RunRow {
    /// A run as a statement or a source answered it, `identity` being the
    /// configured usernames (the app's `identity_of`).
    #[must_use]
    pub fn read(raw: RawRun, identity: &[String]) -> Self {
        let instant = |t: Option<String>| t.as_deref().and_then(crate::payload::parse_instant);
        let started = instant(raw.started);
        let finished = instant(raw.finished);
        RunRow {
            started_by_me: raw
                .started_by
                .as_ref()
                .is_some_and(|who| identity.contains(who)),
            trigger: raw.trigger,
            started_by: raw.started_by,
            revision: raw.revision,
            commit_id: None,
            agent: raw.agent,
            current_step: raw.current_step,
            entity_id: raw.entity_id,
            number: raw.number,
            state: raw.state,
            branch: raw.branch,
            duration_seconds: started
                .zip(finished)
                .map(|(from, to)| (to - from).num_seconds().max(0)),
            started,
            finished,
            updated_at: raw.updated_at,
            web_url: raw.web_url,
        }
    }
}

/// Every mirrored commit of every sha in `$2`, by the declarations `$1`: the
/// live items of kind `commit` whose declared `commit` is that sha. Two rows
/// for one sha are two mirrored repositories holding it, a fork or a mirror
/// on a second forge (#1144).
const COMMITS_OF: &str = concat!(
    "select c.sha, c.entity_id from (
  select i.entity_id, ",
    crate::declared_string!("$1", "commit"),
    " as sha
    from sync.live_item i
   where i.kind = 'commit') c
 where c.sha = any($2::text[])
 order by c.sha, c.entity_id"
);

/// Whether `$2` is a live pipeline of the declarations `$1`.
const IS_PIPELINE: &str = concat!(
    "with ",
    frame_kinds_cte!(),
    "
select exists (
  select 1 from frames f
    join sync.live_item i on i.source_id = f.source_id and i.kind = f.kind
   where f.frame = 'pipeline' and i.entity_id = $2)"
);

/// Every live pipeline of the sources in `fillers`, by name, each with its
/// last [`PIPELINE_RECENT`] runs ([`PIPELINES`]). `identity` is the configured
/// usernames, what *Started by me* matches; no run's `commit_id` is filled
/// here (the app links them, [`commits_of`] and a live read).
///
/// # Errors
///
/// [`CoreError::Db`] if the query fails.
pub async fn pipelines(
    pool: &PgPool,
    declarations: &crate::payload::Declarations,
    identity: &[String],
    fillers: &[String],
) -> Result<Vec<PipelineRow>, CoreError> {
    let rows = sqlx::query(PIPELINES)
        .bind(declarations.as_param())
        .bind(PIPELINE_RECENT)
        .bind(fillers)
        .fetch_all(pool)
        .await?;
    rows.iter()
        .map(|row| {
            let recent: sqlx::types::Json<Vec<RawRun>> = row.get("recent");
            let path: sqlx::types::Json<Vec<String>> = row.get("path");
            Ok(PipelineRow {
                entity_id: row.get("entity_id"),
                source_id: row.get("source_id"),
                source_name: row.get("source_name"),
                name: row.get("name"),
                group: row.get("grp"),
                path: path.0,
                pinned: row.get("pinned"),
                web_url: row.get("web_url"),
                recent: recent
                    .0
                    .into_iter()
                    .map(|raw| RunRow::read(raw, identity))
                    .collect(),
            })
        })
        .collect()
}

/// Every run of `pipelines`, each with the id of its pipeline: what the app
/// links to its commit.
pub fn recent_runs(pipelines: &mut [PipelineRow]) -> Vec<(&str, &mut RunRow)> {
    pipelines
        .iter_mut()
        .flat_map(|p| {
            let PipelineRow {
                entity_id, recent, ..
            } = p;
            let entity_id: &str = entity_id;
            recent.iter_mut().map(move |r| (entity_id, r))
        })
        .collect()
}

/// The runs of the live pipeline `pipeline`, newest first, at most `limit`
/// (100 when `None`), read through the run kind's declarations
/// ([`PIPELINE_RUNS_OF`]). `None` when no live pipeline carries the id. No
/// run's `commit_id` is filled here, as in [`pipelines`].
///
/// # Errors
///
/// [`CoreError::Db`] if the query fails.
pub async fn pipeline_runs(
    pool: &PgPool,
    declarations: &crate::payload::Declarations,
    identity: &[String],
    pipeline: &EntityRef,
    limit: Option<u32>,
) -> Result<Option<Vec<RunRow>>, CoreError> {
    let rows = sqlx::query(PIPELINE_RUNS_OF)
        .bind(declarations.as_param())
        .bind(pipeline.to_string())
        .bind(limit.map_or(PIPELINE_RUNS, i64::from))
        .fetch_all(pool)
        .await?;
    if !rows.first().is_some_and(|row| row.get::<bool, _>("found")) {
        return Ok(None);
    }
    Ok(Some(
        rows.iter()
            .filter_map(|row| {
                let entity_id: Option<String> = row.get("entity_id");
                Some(RunRow::read(
                    RawRun {
                        entity_id: entity_id?,
                        number: row.get("number"),
                        state: row
                            .get::<Option<sqlx::types::Json<crate::payload::StateChip>>, _>("state")
                            .map(|chip| chip.0),
                        branch: row.get("branch"),
                        started: row.get("started"),
                        finished: row.get("finished"),
                        updated_at: row.get("item_updated_at"),
                        web_url: row.get("web_url"),
                        trigger: row.get::<Option<String>, _>("trig").and_then(|word| {
                            serde_json::from_value(serde_json::Value::String(word)).ok()
                        }),
                        started_by: row.get("started_by"),
                        agent: row.get("agent"),
                        current_step: row.get("current_step"),
                        revision: row.get("revision"),
                    },
                    identity,
                ))
            })
            .collect(),
    ))
}

/// Every mirrored commit of every sha in `shas`, by sha ([`COMMITS_OF`]): one
/// id for a sha one repository holds, two or more for a sha two hold (#1144),
/// none for a sha the mirror lacks.
///
/// # Errors
///
/// [`CoreError::Db`] if the query fails.
pub async fn commits_of(
    pool: &PgPool,
    declarations: &crate::payload::Declarations,
    shas: &[&str],
) -> Result<HashMap<String, Vec<String>>, CoreError> {
    let mut held: HashMap<String, Vec<String>> = HashMap::new();
    for (sha, commit) in sqlx::query_as::<_, (String, String)>(COMMITS_OF)
        .bind(declarations.as_param())
        .bind(shas)
        .fetch_all(pool)
        .await?
    {
        held.entry(sha).or_default().push(commit);
    }
    Ok(held)
}

/// Whether `entity` is a live pipeline of `declarations` ([`IS_PIPELINE`]).
///
/// # Errors
///
/// [`CoreError::Db`] if the query fails.
pub async fn is_pipeline(
    pool: &PgPool,
    declarations: &crate::payload::Declarations,
    entity: &EntityRef,
) -> Result<bool, CoreError> {
    Ok(sqlx::query_scalar(IS_PIPELINE)
        .bind(declarations.as_param())
        .bind(entity.to_string())
        .fetch_one(pool)
        .await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// [`PIPELINES`] spells the pins' key as a literal (a `concat!` takes no
    /// constant), so this holds it to [`PINS_SETTING`] (#849).
    #[test]
    fn the_pins_are_read_by_their_constant() {
        assert!(PIPELINES.contains(&format!("key = '{PINS_SETTING}'")));
    }
}

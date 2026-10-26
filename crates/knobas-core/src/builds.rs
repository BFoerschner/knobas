//! The Builds frame's mirror reads (ADR-0022, #848; moved here by #1318,
//! spec #1303): the pipelines, the runs of a pipeline, the commits a run
//! names, the frame-kinds CTE and the run columns, beside the entity list
//! read ([`crate::entity_list`]). Since #1870 also the pins' write, *This
//! job*'s pipelines and the rule that links a run to its commit
//! ([`link_commits`]).
//!
//! What needs an adapter stays in the app crate: a run's log, tests,
//! artifacts and chain, a pipeline's settings and older runs, and the live
//! read that tells two mirrored commits of one sha apart (#1144), whose
//! answer [`link_commits`] takes. So do the declarations and the configured
//! identity, which every read here takes as a parameter.

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
    /// Its test summary (#1448, R-9.6): the declared `tests_failed`,
    /// `tests_passed` and `tests_ignored`. `None` for a run that ran no
    /// tests, which declares none of the three.
    pub tests: Option<TestSummary>,
}

/// A run's test summary (#1448): how many tests failed, passed and were
/// ignored. A count the run's record leaves out while another reads is
/// zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[cfg_attr(feature = "test-util", derive(serde::Deserialize))]
#[ts(export_to = "entity.ts")]
pub struct TestSummary {
    pub failed: i64,
    pub passed: i64,
    pub ignored: i64,
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
/// Exported at the crate root so a statement spells it `crate::frame_kinds_cte!()`
/// inside a `concat!`; [`JOB_PIPELINES`] joins the frame's kinds too.
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
            " as current_step, ",
            crate::declared_integer!("$1", "tests_failed"),
            " as tests_failed, ",
            crate::declared_integer!("$1", "tests_passed"),
            " as tests_passed, ",
            crate::declared_integer!("$1", "tests_ignored"),
            " as tests_ignored, (select l.value from ",
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
///
/// Each CTE is `materialized` and read once: inlined, the runs were
/// re-derived for every pipeline, pipelines × runs (2.5 s at 130 × 3,000),
/// and a pipeline's declarations for every pair of pipelines the final join
/// compares (1.35 s against 0.59 s at 400 × 30,000). The runs are ranked on
/// their `pipeline` alone and only the last [`PIPELINE_RECENT`] of each are
/// read through every declaration.
///
/// Public so the app's `tests/it/builds_list_plan.rs` can read its plan.
pub const PIPELINES: &str = concat!(
    "with ",
    frame_kinds_cte!(),
    ",
p as materialized (
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
ranked as materialized (
  select x.entity_id, x.rn
    from (select i.entity_id, ",
    crate::declared_string!("$1", "pipeline"),
    " as pkey, row_number() over (partition by i.source_id, ",
    crate::declared_string!("$1", "pipeline"),
    "
                                 order by i.item_updated_at desc nulls last, i.entity_id desc) as rn
            from frames f
            join sync.live_item i on i.source_id = f.source_id and i.kind = f.kind
           where f.frame = 'run') x
   where x.pkey is not null and x.rn <= $2
),
r as materialized (
  select k.rn, ",
    run_columns!(),
    "
    from ranked k
    join sync.live_item i on i.entity_id = k.entity_id
),
recent as materialized (
  select r.source_id, r.pkey, jsonb_agg(jsonb_build_object(
           'entity_id', r.entity_id, 'number', r.number, 'state', r.state,
           'branch', r.branch, 'started', r.started, 'finished', r.finished,
           'updated_at', r.item_updated_at, 'web_url', r.web_url,
           'trigger', r.trig, 'started_by', r.started_by,
           'agent', r.agent, 'current_step', r.current_step,
           'tests_failed', r.tests_failed, 'tests_passed', r.tests_passed,
           'tests_ignored', r.tests_ignored,
           'revision', r.revision) order by r.rn) as runs
    from r
   group by r.source_id, r.pkey
)
select p.entity_id, p.source_id, p.source_name, p.name, p.grp, p.web_url, p.path,
       exists (select 1 from pins where pins.id = p.entity_id) as pinned,
       coalesce(rc.runs, '[]'::jsonb) as recent
  from p
  left join recent rc on rc.source_id = p.source_id and rc.pkey = p.pkey
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
    pub tests_failed: Option<i64>,
    pub tests_passed: Option<i64>,
    pub tests_ignored: Option<i64>,
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
            tests: (raw.tests_failed.is_some()
                || raw.tests_passed.is_some()
                || raw.tests_ignored.is_some())
            .then(|| TestSummary {
                failed: raw.tests_failed.unwrap_or(0),
                passed: raw.tests_passed.unwrap_or(0),
                ignored: raw.tests_ignored.unwrap_or(0),
            }),
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
                        tests_failed: row.get("tests_failed"),
                        tests_passed: row.get("tests_passed"),
                        tests_ignored: row.get("tests_ignored"),
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

/// The shas of `runs` that two or more mirrored commits hold (`held`, from
/// [`commits_of`]), grouped by the pipeline whose run names them, each once
/// (#1144). Only these need the pipeline's settings to be told apart: a sha
/// one commit holds links that commit, and one the mirror lacks links
/// nothing.
#[must_use]
pub fn shared_shas<'a>(
    runs: &[(&'a str, &mut RunRow)],
    held: &HashMap<String, Vec<String>>,
) -> std::collections::BTreeMap<&'a str, Vec<String>> {
    let mut shared: std::collections::BTreeMap<&'a str, Vec<String>> =
        std::collections::BTreeMap::new();
    for (pipeline, run) in runs {
        if let Some(sha) = run.revision.as_ref()
            && held.get(sha).is_some_and(|commits| commits.len() > 1)
        {
            let wanted = shared.entry(*pipeline).or_default();
            if !wanted.contains(sha) {
                wanted.push(sha.clone());
            }
        }
    }
    shared
}

/// What a pipeline's settings said each of its shared shas is (#1144), by
/// pipeline id, then sha, then the commit's entity id: what
/// [`link_commits`] links a shared sha by.
pub type ResolvedCommits<'a> = HashMap<&'a str, HashMap<String, String>>;

/// Fills each run's [`RunRow::commit_id`] from its `revision` (#1004): the
/// one commit `held` names for that sha, or, for a sha two hold (#1144), the
/// one `resolved` names for the run's pipeline -- what its settings say it
/// builds -- and only if `held` names that one too. Anything else links
/// nothing: a commit in another repository is never offered as the one the
/// run built. A run that names no revision is left as it is.
pub fn link_commits(
    runs: &mut [(&str, &mut RunRow)],
    held: &HashMap<String, Vec<String>>,
    resolved: &ResolvedCommits<'_>,
) {
    for (pipeline, run) in runs {
        let Some(sha) = run.revision.as_ref() else {
            continue;
        };
        run.commit_id = match held.get(sha).map(Vec::as_slice) {
            Some([only]) => Some(only.clone()),
            Some(commits) => resolved
                .get(*pipeline)
                .and_then(|found| found.get(sha))
                .filter(|commit| commits.contains(commit))
                .cloned(),
            None => None,
        };
    }
}

/// Pin or unpin `pipeline` (#849, R-9.3) in [`PINS_SETTING`], and answer
/// whether it is pinned now.
///
/// The pins are a set: pinning twice keeps one, unpinning what is not pinned
/// changes nothing. They are kept by entity id and never checked against the
/// mirror, so a pin outlives a sync that withdrew its pipeline and is drawn
/// again when it comes back. A stored value that is not an array (an older or
/// a broken knobas) reads as no pins and is replaced by the first write.
///
/// # Errors
///
/// [`CoreError::Db`] if the write fails.
pub async fn set_pinned(
    pool: &PgPool,
    pipeline: &EntityRef,
    pinned: bool,
) -> Result<bool, CoreError> {
    let id = pipeline.to_string();
    let stored: serde_json::Value = sqlx::query_scalar(SET_PINNED)
        .bind(PINS_SETTING)
        .bind(&id)
        .bind(pinned)
        .fetch_one(pool)
        .await?;
    Ok(stored
        .as_array()
        .is_some_and(|ids| ids.iter().any(|v| v.as_str() == Some(id.as_str()))))
}

/// [`set_pinned`]'s upsert: `$1` the key, `$2` the pipeline, `$3` whether it
/// is pinned.
const SET_PINNED: &str = "insert into knobas.setting as s (key, value)
         values ($1, case when $3 then jsonb_build_array($2::text) else '[]'::jsonb end)
         on conflict (key) do update set
           value = (select coalesce(jsonb_agg(distinct x order by x), '[]'::jsonb)
                      from (select jsonb_array_elements_text(
                                     case when jsonb_typeof(s.value) = 'array'
                                          then s.value else '[]'::jsonb end) as x
                            union select $2::text where $3) kept
                     where $3 or x <> $2),
           updated_at = now()
         returning value";

/// Why a pipeline is on a job (#850, R-9.1, R-9.2). Tagged on `kind`, so a
/// link set (#866) joins as one more arm without changing these.
///
/// - `branch`: One of its runs built a branch the job reaches: a reached
///   branch, by its title, or the branch a reached pull request is opened from
///   (its declared `head`). *your branch*.
/// - `pr_check`: One of its runs checked a pull request the job reaches: it ran
///   on the pull request's declared `commit` (its head), or the job reaches the
///   run through the pull request. *PR check*.
/// - `linked`: The job reaches the pipeline, or one of its runs, some other
///   way: a direct link, the parent's links (R-7.3), or a link set (#866).
///   `step` and `origin` are the reach's own ([`knobas_core::reach::Reached`]).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export_to = "entity.ts")]
pub enum PipelineReason {
    Branch {
        branch: String,
    },
    PrCheck {
        pr: String,
    },
    Linked {
        step: String,
        origin: crate::reach::Origin,
    },
}

/// One pipeline on a job, with every reason it is there (#850): branches
/// first, then pull requests, then links, as prototype-6 lists them.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct JobPipeline {
    pub pipeline_id: String,
    pub reasons: Vec<PipelineReason>,
}

/// The pipelines a job's reach puts on it, with their reasons (#850).
///
/// `$1` is the declarations; `$2`, `$3` and `$4` are the reach's rows as
/// three arrays: what is reached, the first step of its path, and its
/// origin. Every field is a declared one (ADR-0007): a run's `branch` and
/// `pipeline`, a pull request's `head` and `commit`, a run's revisions as
/// `sync.item_revision` keeps them (#732). A branch's name is its title,
/// which is what Gitea titles a branch with and what a pull request's `head`
/// names (#712). A run built on `refs/heads/<name>` counts as `<name>`.
const JOB_PIPELINES: &str = concat!(
    "with ",
    crate::frame_kinds_cte!(),
    ",
reached as (
  select * from unnest($2::text[], $3::text[], $4::text[]) as r(id, step, origin)
),
names as (
  select i.title as name
    from reached r join sync.live_item i on i.entity_id = r.id
   where i.kind = 'branch'
  union
  select ",
    crate::declared_string!("$1", "head"),
    "
    from reached r join sync.live_item i on i.entity_id = r.id
),
heads as (
  select r.id as pr, ",
    crate::declared_string!("$1", "commit"),
    " as sha
    from reached r join sync.live_item i on i.entity_id = r.id
   where i.kind = 'pr'
),
p as (
  select i.entity_id, i.source_id, ",
    crate::declared_string!("$1", "pipeline"),
    " as pkey
    from frames f
    join sync.live_item i on i.source_id = f.source_id and i.kind = f.kind
   where f.frame = 'pipeline'
),
x as (
  select i.entity_id, i.source_id, ",
    crate::declared_string!("$1", "pipeline"),
    " as pkey, ",
    crate::declared_string!("$1", "branch"),
    " as branch
    from frames f
    join sync.live_item i on i.source_id = f.source_id and i.kind = f.kind
   where f.frame = 'run'
),
reasons as (
  select p.entity_id as pipeline, 1 as ord, 'branch' as kind, n.name as value, null::text as origin
    from x
    join p on p.source_id = x.source_id and p.pkey = x.pkey
    join names n on n.name is not null
                and (x.branch = n.name or x.branch = 'refs/heads/' || n.name)
  union
  select p.entity_id, 2, 'pr_check', h.pr, null
    from heads h
    join sync.item_revision rv on rv.revision = h.sha
    join x on x.entity_id = rv.entity_id
    join knobas.source_config s on s.id = x.source_id and s.kind = rv.adapter_kind
    join p on p.source_id = x.source_id and p.pkey = x.pkey
  union
  select p.entity_id,
         case when se.kind = 'pr' and r.origin = 'own' then 2 else 3 end,
         case when se.kind = 'pr' and r.origin = 'own' then 'pr_check' else 'linked' end,
         r.step,
         case when se.kind = 'pr' and r.origin = 'own' then null else r.origin end
    from reached r
    left join x on x.entity_id = r.id
    join p on p.entity_id = r.id or (p.source_id = x.source_id and p.pkey = x.pkey)
    left join knobas.entity se on se.id = r.step
)
select pipeline, kind, value, origin from reasons order by pipeline, ord, value
"
);

/// The pipelines on the job whose reach is `reached` ([`crate::reach::reach`],
/// #850), each once with its reasons, by entity id. A job that reaches
/// nothing has none.
///
/// # Errors
///
/// [`CoreError::Db`] if the query fails.
pub async fn job_pipelines(
    pool: &PgPool,
    declarations: &crate::payload::Declarations,
    reached: &[crate::reach::Reached],
) -> Result<Vec<JobPipeline>, CoreError> {
    use crate::reach::Origin;
    let ids: Vec<&str> = reached.iter().map(|r| r.entity_id.as_str()).collect();
    let steps: Vec<&str> = reached.iter().map(|r| r.step.as_str()).collect();
    let origins: Vec<&str> = reached
        .iter()
        .map(|r| match r.origin {
            Origin::Own => "own",
            Origin::Parent { .. } => "parent",
            Origin::LinkSet { .. } => "link_set",
        })
        .collect();
    let rows: Vec<(String, String, String, Option<String>)> = sqlx::query_as(JOB_PIPELINES)
        .bind(declarations.as_param())
        .bind(&ids)
        .bind(&steps)
        .bind(&origins)
        .fetch_all(pool)
        .await?;
    let mut out: Vec<JobPipeline> = Vec::new();
    for (pipeline, kind, value, origin) in rows {
        let reason = match kind.as_str() {
            "branch" => PipelineReason::Branch { branch: value },
            "pr_check" => PipelineReason::PrCheck { pr: value },
            _ => PipelineReason::Linked {
                origin: match origin.as_deref() {
                    Some("parent") => Origin::Parent {
                        ticket: value.clone(),
                    },
                    // A set's member is its own first step (#866), so the
                    // reach row it came from names the set.
                    Some("link_set") => reached
                        .iter()
                        .find(|r| r.step == value && matches!(r.origin, Origin::LinkSet { .. }))
                        .map_or(Origin::Own, |r| r.origin.clone()),
                    _ => Origin::Own,
                },
                step: value,
            },
        };
        match out.last_mut() {
            Some(last) if last.pipeline_id == pipeline => last.reasons.push(reason),
            _ => out.push(JobPipeline {
                pipeline_id: pipeline,
                reasons: vec![reason],
            }),
        }
    }
    Ok(out)
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

    fn run(revision: Option<&str>) -> RunRow {
        RunRow::read(
            RawRun {
                entity_id: "tc:run".to_owned(),
                number: None,
                state: None,
                branch: None,
                started: None,
                finished: None,
                updated_at: None,
                web_url: None,
                trigger: None,
                started_by: None,
                agent: None,
                current_step: None,
                tests_failed: None,
                tests_passed: None,
                tests_ignored: None,
                revision: revision.map(str::to_owned),
            },
            &[],
        )
    }

    fn held() -> HashMap<String, Vec<String>> {
        HashMap::from([
            ("aaa".to_owned(), vec!["gitea:one@aaa".to_owned()]),
            (
                "bbb".to_owned(),
                vec!["gitea:one@bbb".to_owned(), "gitea:two@bbb".to_owned()],
            ),
        ])
    }

    /// Only a sha two mirrored commits hold needs the pipeline's settings,
    /// asked once per pipeline however many of its runs name it (#1144).
    #[test]
    fn only_a_shared_sha_is_asked_of_its_pipeline() {
        let (mut a, mut b, mut c, mut d) = (
            run(Some("aaa")),
            run(Some("bbb")),
            run(Some("bbb")),
            run(None),
        );
        let runs = vec![
            ("p1", &mut a),
            ("p1", &mut b),
            ("p1", &mut c),
            ("p2", &mut d),
        ];
        let shared = shared_shas(&runs, &held());
        assert_eq!(
            shared,
            std::collections::BTreeMap::from([("p1", vec!["bbb".to_owned()])])
        );
    }

    /// One holder links it; two link the one the pipeline's repository names,
    /// and only when the mirror holds that one too; none links nothing.
    #[test]
    fn a_shared_sha_links_only_the_commit_its_pipeline_builds() {
        let (mut only, mut told, mut stranger, mut untold, mut missing) = (
            run(Some("aaa")),
            run(Some("bbb")),
            run(Some("bbb")),
            run(Some("bbb")),
            run(Some("ccc")),
        );
        let mut runs = vec![
            ("p1", &mut only),
            ("p1", &mut told),
            ("p2", &mut stranger),
            ("p3", &mut untold),
            ("p1", &mut missing),
        ];
        let resolved = HashMap::from([
            (
                "p1",
                HashMap::from([("bbb".to_owned(), "gitea:two@bbb".to_owned())]),
            ),
            // A commit the mirror does not hold for that sha is never offered.
            (
                "p2",
                HashMap::from([("bbb".to_owned(), "gitea:three@bbb".to_owned())]),
            ),
        ]);
        link_commits(&mut runs, &held(), &resolved);
        assert_eq!(only.commit_id.as_deref(), Some("gitea:one@aaa"));
        assert_eq!(told.commit_id.as_deref(), Some("gitea:two@bbb"));
        assert_eq!(stranger.commit_id, None);
        assert_eq!(untold.commit_id, None);
        assert_eq!(missing.commit_id, None);
    }
}

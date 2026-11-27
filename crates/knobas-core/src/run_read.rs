//! The marks a person's runs carry (ux18, migrations `0098` and `0105`,
//! ADR-0063).
//!
//! Björn on PR #2479: *"I want to be able to mark builds as read. Single ones
//! as well as all. Since I might not care about old failed runs."* And on
//! #2555 (2026-11-27): read means opening the run, or a *Mark … as read*.
//!
//! **Any run can carry a mark**, one row in `knobas.run_read`, with how it got
//! it (`opened`: its page was shown; `marked`: a press, or the Inbox's *Done*)
//! and whether the run had finished, as the mirror held it, when the row was
//! written.
//!
//! **What reads a run** ([`reads!`], the one predicate): a mark that is
//! `opened`, or one written after the run finished. A running run carrying
//! any mark is out of the blue; a mark taken while it ran takes only that, so
//! a run marked then that fails later is unread, and one opened then is not.
//! A new run is a new entity with no row, so a new failure is unread by
//! construction.
//!
//! **One notion with the Inbox.** The Inbox's failed build rule leaves out a
//! run [`reads!`] reads (`crate::inbox`'s `failed_build!`), and its *Done* on
//! a failed build writes a mark in the same transaction ([`mark_with`]).
//! *Mark as unread* deletes the row and the line's *Done*, so the line comes
//! back; a snooze stays.
//!
//! Knobas-local, never the source's, and kept across syncs and restarts.

use sqlx::PgPool;

use crate::CoreError;

/// Whether the mark `$rr` (a `knobas.run_read` row) reads its run: it was
/// got by opening the run, or written after the run finished (ADR-0063). The
/// one predicate every reader of "read" asks: the Builds list's unread
/// failures (`crate::builds::PIPELINES`), the Inbox's failed builds
/// (`crate::inbox`'s `failed_build!`), the window's set ([`read_runs`]) and
/// the counts ([`RUN_COUNTS`]). A mark that does not read its run
/// was taken while it ran, and takes only its blue.
#[macro_export]
macro_rules! reads {
    ($rr:literal) => {
        concat!("(", $rr, ".how = 'opened' or ", $rr, ".finished)")
    };
}

/// The ids of the runs a mark reads ([`reads!`]), for a `not in (…)`: what
/// the Inbox leaves out, hashed once a statement.
#[macro_export]
macro_rules! read_ids {
    () => {
        concat!(
            "select rr.entity_id from knobas.run_read rr where ",
            $crate::reads!("rr")
        )
    };
}

/// The live runs that carry a mark, each with whether the mark reads it.
const READ_RUNS: &str = concat!(
    "select r.entity_id, ",
    crate::reads!("r"),
    " as reads
   from knobas.run_read r
  where exists (select 1 from knobas.entity e
                 where e.id = r.entity_id and e.deleted_at is null)"
);

/// The marks a person's runs carry, split by what each does (ADR-0063): what
/// the window holds as its set, and what a mark answers it wrote.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct RunMarks {
    /// The runs a mark reads: opened, or marked after they finished. A failed
    /// one is not red, a passed one not green, a running one not blue.
    pub read: Vec<String>,
    /// The runs marked while they ran: out of the blue, and red again if
    /// they fail.
    pub while_running: Vec<String>,
}

impl RunMarks {
    fn of(rows: Vec<(String, bool)>) -> Self {
        let (read, while_running): (Vec<_>, Vec<_>) =
            rows.into_iter().partition(|(_, reads)| *reads);
        Self {
            read: read.into_iter().map(|(id, _)| id).collect(),
            while_running: while_running.into_iter().map(|(id, _)| id).collect(),
        }
    }
}

/// What a *Mark … as read* covers (ADR-0063): every run, the runs of a set of
/// pipelines, runs by id, or any union of them. *Everything* needs no list
/// from the page, so a press before the pipeline list has loaded marks what
/// it says (#2555).
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct MarkScope {
    /// Every run of every live pipeline.
    #[serde(default)]
    pub everything: bool,
    /// The live pipelines whose runs it covers, by entity id.
    #[serde(default)]
    pub pipelines: Vec<String>,
    /// Runs by entity id, whatever their state: a live run named here is
    /// marked.
    #[serde(default)]
    pub runs: Vec<String>,
}

impl MarkScope {
    /// Whether the scope reaches a pipeline's runs, so its read needs the
    /// stored pipeline rows (`sync.item_pipeline`).
    #[must_use]
    pub fn reaches_pipelines(&self) -> bool {
        self.everything || !self.pipelines.is_empty()
    }
}

/// The live runs of the pipelines in the relation `$pipelines`, the
/// declarations `$1` bound: a `from … where …` that selects over `t` (the
/// pipeline, with `source_id`, `adapter_kind` and `pkey`), `ip` (its run's
/// `entity_id`, and `at`, the order every reader draws runs in), `i` (the
/// run's `sync.item`) and `ru`. Each by its stored pipeline range
/// (`sync.item_pipeline`, as `crate::newest_runs!` reads it), with its state
/// and finish as [`crate::builds`] reads a run: the stored row where the
/// bound declarations wrote one, the walk where not.
///
/// **Its plan** (`builds_list_plan.rs`, `read_plans.rs`): one range of
/// `item_pipeline_runs_idx` per pipeline, kept a lateral by `offset 0`;
/// joined, the planner read every pipeline's runs whole, against
/// `read_plans.rs`'s rule (#2185). A run is live by a scalar probe of
/// `knobas.entity`: an `exists` became a hash of every entity rebuilt per
/// pipeline. Moving the run's tests into the lateral too re-read `run_keys`
/// per run, twice as slow.
#[macro_export]
macro_rules! runs_of {
    ($pipelines:literal) => {
        concat!(
            "
    from ",
            $pipelines,
            " t
    cross join lateral (select ip.entity_id, ip.at from sync.item_pipeline ip
                         where ip.source_id = t.source_id and ip.pkey = t.pkey
                           and ip.adapter_kind = t.adapter_kind
                        offset 0) ip
    join sync.item i on i.entity_id = ip.entity_id",
            $crate::run_join!("ru", "i"),
            "
   where (select e.deleted_at is null from knobas.entity e where e.id = ip.entity_id)"
        )
    };
}

/// [`runs_of!`] kept to the red ones ([`run_bubble!`]): the finished runs
/// whose state reads `err`. One definition for what a scope's mark reads ([`MARK_READ`]) and
/// for the unread failures the Builds list names
/// (`crate::builds::PIPELINES`, #2543), so the rail's count is what its press
/// marks.
#[macro_export]
macro_rules! failed_runs_of {
    ($pipelines:literal) => {
        concat!(
            $crate::runs_of!($pipelines),
            "
     and ",
            $crate::run_bubble!(),
            " = 'red'"
        )
    };
}

/// Which bubble a run stands in (ADR-0063), over `i` and `ru` as
/// [`runs_of!`] joins them and the declarations `$1`: the one place a run's
/// state becomes one of five, as `'blue'`, `'green'`, `'yellow'`, `'red'` or
/// `'grey'`, or null for a run not started (queued). Each adapter maps its
/// source's statuses to tones in its declarations; this maps the tones:
///
/// * finished: `ok` green (passed), `warn` yellow (passed with warnings),
///   `err` red (failed), `muted` grey (cancelled), and any other (`info`, no
///   tone: a status the declaration does not name) grey;
/// * started and not finished: blue (running), whatever its tone says;
/// * neither: none. A queued run is not news.
///
/// Whether the run is unread is the marks' business ([`reads!`]); this says
/// only which colour it would count in.
#[macro_export]
macro_rules! run_bubble {
    () => {
        concat!(
            "(case when ",
            $crate::row_or!("ru", "finished", $crate::declared_string!("$1", "finished")),
            " is not null then case ",
            $crate::row_or!("ru", "state", $crate::declared_state!("$1")),
            "->>'tone' when 'ok' then 'green' when 'warn' then 'yellow' when 'err' then 'red' \
              else 'grey' end when ",
            $crate::row_or!("ru", "started", $crate::declared_string!("$1", "started")),
            " is not null then 'blue' end)"
        )
    };
}

/// Whether the run over `i` and `ru` had finished, under the declarations
/// `$1`: what a mark records.
macro_rules! finished {
    () => {
        concat!(
            $crate::row_or!("ru", "finished", $crate::declared_string!("$1", "finished")),
            " is not null"
        )
    };
}

/// Mark read (ADR-0063): the runs of the scope, `$4` every live pipeline or
/// `$2` some, and `$3` runs by id; `$1` the declarations. Of a pipeline's
/// runs it takes every blue one out of the blue and reads every red one
/// ([`run_bubble!`]); the green, yellow and grey of a scope are cleared by
/// moving its instant, not by a mark on each (`docs/adr/0063-*.md`). A run named by id is marked whatever its
/// state. Each row records whether the run had finished; a run marked while
/// it ran and marked again after is updated in place. Answers the rows it
/// wrote and whether each reads its run: a run the mark changed nothing for
/// is not among them, so *Undo* takes back exactly what this did.
///
/// Public so the app's `tests/it/builds_list_plan.rs` can read its plan: a
/// mark of everything reads each pipeline's runs as [`runs_of!`] does.
pub const MARK_READ: &str = concat!(
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
   where f.frame = 'pipeline' and ($4::bool or i.entity_id = any($2::text[]))
),
scoped as (
  select i.entity_id, ",
    crate::run_bubble!(),
    " as bubble, ",
    finished!(),
    " as finished",
    crate::runs_of!("target"),
    "
),
named as (
  select i.entity_id, ",
    finished!(),
    " as finished
    from sync.item i",
    crate::run_join!("ru", "i"),
    "
   where i.entity_id = any($3::text[])
     and (select e.deleted_at is null from knobas.entity e where e.id = i.entity_id)
),
wanted as (
  select distinct on (u.entity_id) u.entity_id, u.finished
    from (select s.entity_id, s.finished from scoped s
           where s.bubble in ('red', 'blue')
          union all
          select n.entity_id, n.finished from named n) u
   order by u.entity_id
)
insert into knobas.run_read as r (entity_id, how, finished)
select w.entity_id, 'marked', w.finished from wanted w
on conflict (entity_id) do update set finished = true, read_at = now()
 where not ",
    crate::reads!("r"),
    " and excluded.finished
returning r.entity_id, ",
    crate::reads!("r")
);

/// Open `$2` (ADR-0063): its mark becomes `opened`, which reads it whatever it
/// does after, unless a mark already reads it. `$1` the declarations, for
/// whether it had finished. Answers the id when it wrote.
const MARK_OPENED: &str = concat!(
    "with ",
    crate::run_keys!("$1"),
    ",
run as (
  select i.entity_id, ",
    finished!(),
    " as finished
    from sync.item i",
    crate::run_join!("ru", "i"),
    "
   where i.entity_id = $2
     and (select e.deleted_at is null from knobas.entity e where e.id = i.entity_id)
)
insert into knobas.run_read as r (entity_id, how, finished)
select run.entity_id, 'opened', run.finished from run
on conflict (entity_id) do update set how = 'opened', finished = excluded.finished, read_at = now()
 where not ",
    crate::reads!("r"),
    "
returning r.entity_id"
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

/// The live runs that carry a mark, split by whether the mark reads them.
///
/// # Errors
///
/// [`CoreError::Db`] if the read fails.
pub async fn read_runs(pool: &PgPool) -> Result<RunMarks, CoreError> {
    Ok(RunMarks::of(
        sqlx::query_as(READ_RUNS).fetch_all(pool).await?,
    ))
}

/// Mark the runs of `scope` read (ADR-0063), against an executor the caller
/// chooses. Answers the marks it wrote, which a run already read is not
/// among.
///
/// # Errors
///
/// [`CoreError::Db`] if the write fails.
pub async fn mark_read<'e, E>(
    executor: E,
    declarations: &crate::payload::Declarations,
    scope: &MarkScope,
) -> Result<RunMarks, CoreError>
where
    E: sqlx::PgExecutor<'e>,
{
    Ok(RunMarks::of(
        sqlx::query_as(MARK_READ)
            .bind(declarations.as_param())
            .bind(&scope.pipelines)
            .bind(&scope.runs)
            .bind(scope.everything)
            .fetch_all(executor)
            .await?,
    ))
}

/// The run `run` opened: its mark becomes `opened` (ADR-0063). Answers
/// whether it wrote, which it does not when a mark already read the run, or
/// the run is not in the mirror.
///
/// # Errors
///
/// [`CoreError::Db`] if the write fails.
pub async fn mark_opened(
    pool: &PgPool,
    declarations: &crate::payload::Declarations,
    run: &str,
) -> Result<bool, CoreError> {
    Ok(sqlx::query_scalar::<_, String>(MARK_OPENED)
        .bind(declarations.as_param())
        .bind(run)
        .fetch_optional(pool)
        .await?
        .is_some())
}

/// The run `run` marked, against an executor the caller chooses: the Inbox's
/// *Done* on a failed build writes it in the transaction of its own answer
/// (`crate::inbox::complete_with`'s shape). The mark records whether the run
/// had finished, as any mark does.
///
/// # Errors
///
/// [`CoreError::Db`] if the write fails.
pub async fn mark_with<'e, E>(
    executor: E,
    declarations: &crate::payload::Declarations,
    run: &str,
) -> Result<(), CoreError>
where
    E: sqlx::PgExecutor<'e>,
{
    let scope = MarkScope {
        runs: vec![run.to_owned()],
        ..MarkScope::default()
    };
    mark_read(executor, declarations, &scope).await?;
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

// -- the counts and the instants (#2568, ADR-0063) ---------------------------

/// The `knobas.setting` key holding the instants green, yellow and grey
/// count from (ADR-0063, *The instant*): a JSON object with `at`, when Builds
/// was last left, `pipelines`, each pipeline's own a scope's mark moved, and
/// `visit`, when Builds came in front, while it is. Each an instant as
/// Postgres reads a `timestamptz`. [`RUN_COUNTS`] spells it as a literal, and
/// `the_instants_are_read_by_their_constant` holds the two together.
pub const SEEN_SETTING: &str = "builds.seen";

/// How many runs stand in each of the five bubbles (ADR-0063), unread.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS,
)]
#[ts(export_to = "entity.ts")]
pub struct BubbleCounts {
    /// Running, no mark of any kind.
    pub blue: u32,
    /// Passed, ended after the instant, no mark reads it.
    pub green: u32,
    /// Passed with warnings, as green.
    pub yellow: u32,
    /// Failed, no mark reads it, however long ago.
    pub red: u32,
    /// Cancelled, or an end no tone names, as green.
    pub grey: u32,
}

impl BubbleCounts {
    fn add(&mut self, other: &Self) {
        self.blue += other.blue;
        self.green += other.green;
        self.yellow += other.yellow;
        self.red += other.red;
        self.grey += other.grey;
    }
}

/// One pipeline's unread runs by bubble. A pipeline with none is not
/// answered.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct PipelineCounts {
    /// The pipeline's entity id.
    pub pipeline: String,
    /// Its runs in each bubble.
    pub counts: BubbleCounts,
}

/// The unread runs of every pipeline the Builds list draws, by bubble
/// ([`run_counts`]): what the top bar, the rail and the rows draw. A folder
/// is the sum of its pipelines; everything is [`RunCounts::total`].
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct RunCounts {
    /// Each pipeline with an unread run, by entity id.
    pub pipelines: Vec<PipelineCounts>,
    /// Every pipeline's summed.
    pub total: BubbleCounts,
    /// Whether Builds is in front ([`builds_in_front`]): then green, yellow
    /// and grey count only the runs that ended before it came in front, the
    /// visit's (question 2 on #2554), and [`RunCounts::fresh`] names them.
    pub visiting: bool,
    /// While [`RunCounts::visiting`], the runs green, yellow and grey count,
    /// so a row can draw its dot; empty otherwise.
    pub fresh: Vec<String>,
}

/// The unread runs of each pipeline by bubble (ADR-0063), `$1` the
/// declarations and `$2` the sources filling the CI role: the pipelines the
/// Builds list draws (`crate::builds::PIPELINES`), each over its runs as
/// [`runs_of!`] reads them, once.
///
/// * **Blue**: running, and no mark at all.
/// * **Red**: failed, and no mark reads it ([`reads!`]), the list's `unread`.
/// * **Green, yellow, grey**: ended, no mark reads it, and stamped
///   (`sync.run_finished`, migration `0106`) after the pipeline's instant:
///   the later of [`SEEN_SETTING`]'s `at` and the pipeline's own. While
///   Builds is in front, stamped no later than `visit` too: a run that ends
///   while you look counts in none.
///
/// **Its plan** (`read_plans.rs`): the runs are [`runs_of!`]'s range of
/// `item_pipeline_runs_idx` per pipeline; the marks and the stamps are probed
/// by key per run that stands in a bubble. About 3 µs a run, the cost of the
/// list's unread failures, which read the same range.
///
/// Public so the app's `tests/it/read_plans.rs` can read its plan.
pub const RUN_COUNTS: &str = concat!(
    "with ",
    crate::frame_kinds_cte!(),
    ", ",
    crate::run_keys!("$1"),
    ",
p as materialized (
  select i.entity_id, i.source_id, c.kind as adapter_kind, ",
    crate::declared_string!("$1", "pipeline"),
    " as pkey, ",
    crate::declared_string!("$1", "name"),
    " as name
    from frames f
    join sync.live_item i on i.source_id = f.source_id and i.kind = f.kind
    left join knobas.source_config c on c.id = i.source_id
   where f.frame = 'pipeline' and i.source_id = any($2::text[])
),
seen as materialized (
  select coalesce((s.value->>'at')::timestamptz, '-infinity'::timestamptz) as at,
         case when jsonb_typeof(s.value->'pipelines') = 'object'
              then s.value->'pipelines' else '{}'::jsonb end as own,
         (s.value->>'visit')::timestamptz as visit
    from (values (1)) as one (x)
    left join knobas.setting s on s.key = 'builds.seen' and jsonb_typeof(s.value) = 'object'
),
runs as (
  select t.entity_id as pipeline, i.entity_id as run, ",
    crate::run_bubble!(),
    " as bubble",
    crate::runs_of!("p"),
    "
     and t.pkey is not null and t.name is not null
),
counted as (
  select r.pipeline, r.run, r.bubble, seen.visit is not null as visiting
    from runs r cross join seen
   where r.bubble is not null
     and case r.bubble
           when 'blue' then not exists (select 1 from knobas.run_read rr where rr.entity_id = r.run)
           when 'red' then not exists (select 1 from knobas.run_read rr
                                        where rr.entity_id = r.run and ",
    crate::reads!("rr"),
    ")
           else exists (select 1 from sync.run_finished rf
                         where rf.entity_id = r.run
                           and rf.seen > greatest(seen.at, coalesce((seen.own->>r.pipeline)::timestamptz,
                                                                     '-infinity'::timestamptz))
                           and (seen.visit is null or rf.seen <= seen.visit))
                and not exists (select 1 from knobas.run_read rr
                                 where rr.entity_id = r.run and ",
    crate::reads!("rr"),
    ")
         end
)
select c.pipeline,
       (count(*) filter (where c.bubble = 'blue'))::bigint as blue,
       (count(*) filter (where c.bubble = 'green'))::bigint as green,
       (count(*) filter (where c.bubble = 'yellow'))::bigint as yellow,
       (count(*) filter (where c.bubble = 'red'))::bigint as red,
       (count(*) filter (where c.bubble = 'grey'))::bigint as grey,
       coalesce(array_agg(c.run order by c.run)
                  filter (where c.visiting and c.bubble in ('green', 'yellow', 'grey')),
                '{}'::text[]) as fresh
  from counted c
 group by c.pipeline
 order by c.pipeline
"
);

/// Whether Builds is in front, as [`SEEN_SETTING`] holds it.
const VISITING: &str = "select coalesce((select s.value ? 'visit' from knobas.setting s
                          where s.key = 'builds.seen' and jsonb_typeof(s.value) = 'object'), false)";

/// One row of [`RUN_COUNTS`].
#[derive(sqlx::FromRow)]
struct CountsRow {
    pipeline: String,
    blue: i64,
    green: i64,
    yellow: i64,
    red: i64,
    grey: i64,
    fresh: Vec<String>,
}

/// The unread runs of every pipeline of the sources in `fillers`, by bubble
/// ([`RUN_COUNTS`]).
///
/// # Errors
///
/// [`CoreError::Db`] if the read fails.
pub async fn run_counts(
    pool: &PgPool,
    declarations: &crate::payload::Declarations,
    fillers: &[String],
) -> Result<RunCounts, CoreError> {
    let rows: Vec<CountsRow> = sqlx::query_as(RUN_COUNTS)
        .bind(declarations.as_param())
        .bind(fillers)
        .fetch_all(pool)
        .await?;
    let visiting: bool = sqlx::query_scalar(VISITING).fetch_one(pool).await?;
    let n = |v: i64| u32::try_from(v).unwrap_or(u32::MAX);
    let mut answer = RunCounts {
        visiting,
        ..RunCounts::default()
    };
    for CountsRow {
        pipeline,
        blue,
        green,
        yellow,
        red,
        grey,
        fresh,
    } in rows
    {
        let counts = BubbleCounts {
            blue: n(blue),
            green: n(green),
            yellow: n(yellow),
            red: n(red),
            grey: n(grey),
        };
        answer.total.add(&counts);
        answer.fresh.extend(fresh);
        answer.pipelines.push(PipelineCounts { pipeline, counts });
    }
    Ok(answer)
}

/// [`SEEN_SETTING`]'s value. A value that is not this shape (an older or a
/// broken knobas) reads as no instant at all, and the next write replaces it.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct Seen {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    at: Option<String>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pipelines: std::collections::BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    visit: Option<String>,
}

/// A pipeline's own instant before a mark moved it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct PipelineSeen {
    /// The pipeline's entity id.
    pub pipeline: String,
    /// Its own instant before, `None` where it had none.
    pub before: Option<String>,
}

/// What a scope's mark did to the instants ([`move_seen`]), which its *Undo*
/// hands back to [`put_back_seen`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct SeenMove {
    /// The instant it moved them to.
    pub to: String,
    /// Whether it moved the one instant, `at` ([`MarkScope::everything`]).
    pub everything: bool,
    /// `at` before, where `everything`.
    pub at: Option<String>,
    /// Each pipeline's own before, for [`MarkScope::pipelines`].
    pub pipelines: Vec<PipelineSeen>,
}

/// [`SEEN_SETTING`] in `tx`, locked until it commits, and the transaction's
/// instant as an RFC 3339 string to the microsecond, as Postgres holds one.
async fn seen_for_update(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
) -> Result<(Seen, String), CoreError> {
    let (now, value): (chrono::DateTime<chrono::Utc>, Option<serde_json::Value>) = sqlx::query_as(
        "select now(), (select s.value from knobas.setting s where s.key = $1 for update)",
    )
    .bind(SEEN_SETTING)
    .fetch_one(&mut **tx)
    .await?;
    let seen = value
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();
    Ok((
        seen,
        now.to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
    ))
}

async fn write_seen(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    seen: &Seen,
) -> Result<(), CoreError> {
    let value = serde_json::to_value(seen).unwrap_or_else(|_| serde_json::json!({}));
    sqlx::query(
        "insert into knobas.setting as s (key, value) values ($1, $2)
         on conflict (key) do update set value = excluded.value, updated_at = now()",
    )
    .bind(SEEN_SETTING)
    .bind(value)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// A scope's *Mark all as read* on green, yellow and grey (ADR-0063): it
/// moves the scope's instant to now, the one instant for *everything*, each
/// pipeline's own for a set of pipelines. Runs by id move nothing. Answers
/// what it moved, `None` when the scope reaches no pipeline.
///
/// # Errors
///
/// [`CoreError::Db`] if the write fails.
pub async fn move_seen(pool: &PgPool, scope: &MarkScope) -> Result<Option<SeenMove>, CoreError> {
    if !scope.reaches_pipelines() {
        return Ok(None);
    }
    let mut tx = pool.begin().await?;
    let (mut seen, now) = seen_for_update(&mut tx).await?;
    let at = if scope.everything {
        seen.at.replace(now.clone())
    } else {
        None
    };
    let mut pipelines: Vec<PipelineSeen> = Vec::new();
    if !scope.everything {
        for pipeline in &scope.pipelines {
            if pipelines.iter().any(|p| &p.pipeline == pipeline) {
                continue;
            }
            let before = seen.pipelines.insert(pipeline.clone(), now.clone());
            pipelines.push(PipelineSeen {
                pipeline: pipeline.clone(),
                before,
            });
        }
    }
    write_seen(&mut tx, &seen).await?;
    tx.commit().await?;
    Ok(Some(SeenMove {
        to: now,
        everything: scope.everything,
        at,
        pipelines,
    }))
}

/// *Undo* of [`move_seen`]: each instant it moved goes back, unless it has
/// moved since (leaving Builds moves `at` and drops every pipeline's own).
/// Answers whether any went back.
///
/// # Errors
///
/// [`CoreError::Db`] if the write fails.
pub async fn put_back_seen(pool: &PgPool, moved: &SeenMove) -> Result<bool, CoreError> {
    let mut tx = pool.begin().await?;
    let (mut seen, _) = seen_for_update(&mut tx).await?;
    let mut back = false;
    if moved.everything && seen.at.as_ref() == Some(&moved.to) {
        seen.at.clone_from(&moved.at);
        back = true;
    }
    for p in &moved.pipelines {
        if seen.pipelines.get(&p.pipeline) == Some(&moved.to) {
            match &p.before {
                Some(before) => seen.pipelines.insert(p.pipeline.clone(), before.clone()),
                None => seen.pipelines.remove(&p.pipeline),
            };
            back = true;
        }
    }
    if back {
        write_seen(&mut tx, &seen).await?;
    }
    tx.commit().await?;
    Ok(back)
}

/// Builds came in front (`front`), or stopped being in front: the shell's
/// section change, and the quit while on it (ADR-0063, *The instant*). Coming
/// in front holds the visit's start, so green counts what ended before it for
/// the whole visit; leaving moves the one instant to now and drops every
/// pipeline's own, which are all older. Leaving when Builds was not in front
/// writes nothing. Answers whether it wrote.
///
/// # Errors
///
/// [`CoreError::Db`] if the write fails.
pub async fn builds_in_front(pool: &PgPool, front: bool) -> Result<bool, CoreError> {
    let mut tx = pool.begin().await?;
    let (mut seen, now) = seen_for_update(&mut tx).await?;
    if front {
        seen.visit.get_or_insert(now);
    } else if seen.visit.take().is_some() {
        seen.at = Some(now);
        seen.pipelines.clear();
    } else {
        return Ok(false);
    }
    write_seen(&mut tx, &seen).await?;
    tx.commit().await?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::{RUN_COUNTS, SEEN_SETTING, VISITING, failed_build_run};

    /// [`RUN_COUNTS`] and [`VISITING`] spell the setting's key as a literal
    /// inside a `concat!`: this holds them to [`SEEN_SETTING`].
    #[test]
    fn the_instants_are_read_by_their_constant() {
        let key = format!("s.key = '{SEEN_SETTING}'");
        assert!(RUN_COUNTS.contains(&key));
        assert!(VISITING.contains(&key));
    }

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

//! **What a source does after a branch moved is looked for on a short,
//! bounded schedule** (ADR-0064 rule 2, #2585).
//!
//! Björn, 2026-11-27: after a branch changes, knobas looks for its code
//! changes and its queued builds for a while, "both code and builds". A
//! landed write's effects are read once, at once ([`super::effects`]); CI
//! queues a build seconds later, and Gitea finishes a merge's bookkeeping
//! after it answered. So the place the change lands, its **scope** (a
//! repository here), is read again about [`CADENCE`] after the trigger: 5 s,
//! 15 s, 30 s, 60 s and 120 s.
//!
//! - **Two triggers.** A landed write whose op declares [`Effect::Parent`]
//!   ([`after_write`], the trigger set read off `WriteOp::effects`, no second
//!   list), and a run of a source that finds a branch's tip moved
//!   ([`after_run`]: a push made outside knobas, or by a command in the
//!   checkout).
//! - **Sides, each stopped once it moved.** The *code side* walks the
//!   repository (its branches, pull requests and commits, ADR-0047) and reads
//!   the pull request a write was made on by id (its state, head, checks and
//!   mergeability). It has moved once a read found anything new
//!   (`RefreshOutcome::Refreshed` or `Gone`). The *builds side* walks each of
//!   the repository's pipelines for its newest runs (#2564) and has moved
//!   once the repository's Builds tab would draw a run it did not draw when
//!   the schedule began. A side that cannot be read (`NotOffered`, its source
//!   turned off or gone) ends.
//! - **One schedule per scope.** A new trigger on a scope stops its schedule
//!   and starts it again from the first read. At most [`AT_ONCE`] scopes are
//!   looked at at a time; a newer one stops the oldest.
//! - **Respects the source's lock.** Every read is a refresh under
//!   [`Lock::Try`] in the background lane (`knobas_http::behind`): a run of
//!   the source holding its lock skips the read, and reads what it would
//!   have anyway. A read that finds nothing new tells the window nothing
//!   (ADR-0031's 2026-11-25 amendment), so a quiet schedule costs it nothing.
//! - **Said, quietly.** Each start, each side that stops and each end is told
//!   on `looking:again` ([`LookingAgain`]); [`looking`] answers the schedules
//!   now. The repository's Builds tab says *Looking for new builds…* while
//!   its builds side runs.
//!
//! **Reused by #2595** (the written item and its linked items, for bots,
//! automation and automatic merges): a scope is any entity id, and a side is
//! anything that implements [`Side`]: a read and the test of whether it
//! moved. [`begin`] takes the scope's key and its sides; the cadence, the
//! restart, the cap, the lock and the telling come with it.
//!
//! Not taken up again after a restart: the next run of the source reads what
//! the schedule would have.

use std::collections::{HashMap, HashSet, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use knobas_core::delivery::Effect;
use knobas_core::write_queue::QueuedWrite;
use knobas_sync::refresh::{Lock, RefreshError, RefreshOutcome};
use knobas_sync::scheduler::{LookingAgain, Scheduler};
use tokio::sync::watch;

/// When a scope is read again, after its trigger: about 5 s, 15 s, 30 s,
/// 60 s and 120 s (ADR-0064 rule 2).
pub const CADENCE: [Duration; 5] = [
    Duration::from_secs(5),
    Duration::from_secs(15),
    Duration::from_secs(30),
    Duration::from_secs(60),
    Duration::from_secs(120),
];

/// How many scopes are looked at at once; a newer one stops the oldest.
pub const AT_ONCE: usize = 8;

/// How often a wait looks up to see the quit.
const NAP: Duration = Duration::from_millis(250);

/// The kind a branch is mirrored as, as `knobas_core::builds` names it: what
/// [`after_run`] compares the tips of.
const BRANCH_KIND: &str = "branch";

/// Where the schedule stands in time: [`RealClock`] in the app, a hand-moved
/// one in a test ([`use_clock`]).
pub trait Clock: Send + Sync {
    /// How long since the clock's own start.
    fn now(&self) -> Duration;
    /// Answers once [`now`](Self::now) reaches `at`.
    fn at(&self, at: Duration) -> Pin<Box<dyn Future<Output = ()> + Send>>;
}

/// The wall clock, from its making.
pub struct RealClock(Instant);

impl Default for RealClock {
    fn default() -> Self {
        Self(Instant::now())
    }
}

impl Clock for RealClock {
    fn now(&self) -> Duration {
        self.0.elapsed()
    }
    fn at(&self, at: Duration) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        let left = at.saturating_sub(self.now());
        Box::pin(tokio::time::sleep(left))
    }
}

/// Which side of a scope a [`Side`] is: what [`LookingAgain`] tells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SideKind {
    /// Branches, pull requests and commits.
    Code,
    /// The pipelines' runs.
    Builds,
}

/// What one read of a side found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Look {
    /// It moved: this side stops.
    Moved,
    /// Nothing new, or the read was skipped (a run holds the lock) or
    /// failed: it is read again at the next step.
    Quiet,
    /// It cannot be read here (not offered, turned off, gone): it stops.
    Over,
}

/// One side of a scope: a read, and whether it moved.
#[async_trait]
pub trait Side: Send {
    /// Which side it is.
    fn kind(&self) -> SideKind;
    /// Read it once.
    async fn look(&mut self) -> Look;
}

/// One scope's schedule: `sides` read in their order at each of [`CADENCE`]
/// after `began` on `clock`, each dropped once it moved or is over. `tell`
/// hears the sides left each time one stops (empty at the end). Answers
/// `false` when it was stopped (`stop`, or `closed` once the quit began)
/// before it ended.
pub(crate) async fn run(
    clock: &dyn Clock,
    began: Duration,
    mut sides: Vec<Box<dyn Side>>,
    mut stop: watch::Receiver<bool>,
    closed: &(dyn Fn() -> bool + Sync),
    tell: &(dyn Fn(&[SideKind]) + Sync),
) -> bool {
    for offset in CADENCE {
        if !wait(clock, began + offset, &mut stop, closed).await {
            return false;
        }
        let before = sides.len();
        let mut kept = Vec::with_capacity(before);
        for mut side in sides {
            if *stop.borrow() {
                return false;
            }
            if side.look().await == Look::Quiet {
                kept.push(side);
            }
        }
        sides = kept;
        if sides.len() != before {
            tell(&kinds(&sides));
        }
        if sides.is_empty() {
            return true;
        }
    }
    tell(&[]);
    true
}

fn kinds(sides: &[Box<dyn Side>]) -> Vec<SideKind> {
    sides.iter().map(|side| side.kind()).collect()
}

/// Wait until `at`: `false` once stopped or the quit has begun.
async fn wait(
    clock: &dyn Clock,
    at: Duration,
    stop: &mut watch::Receiver<bool>,
    closed: &(dyn Fn() -> bool + Sync),
) -> bool {
    let until = clock.at(at);
    tokio::pin!(until);
    loop {
        if *stop.borrow() || closed() {
            return false;
        }
        tokio::select! {
            () = &mut until => return !*stop.borrow() && !closed(),
            changed = stop.changed() => if changed.is_err() { return false },
            () = tokio::time::sleep(NAP) => {}
        }
    }
}

/// The schedules running, oldest first, the clock they keep, and the branch
/// tips each source's last run left ([`after_run`]).
#[derive(Clone)]
pub(crate) struct Lookouts(Arc<Inner>);

struct Inner {
    running: Mutex<VecDeque<Running>>,
    clock: Mutex<Arc<dyn Clock>>,
    tips: Mutex<HashMap<String, HashMap<String, String>>>,
    next: AtomicU64,
}

struct Running {
    id: u64,
    looking: LookingAgain,
    stop: watch::Sender<bool>,
}

impl Default for Lookouts {
    fn default() -> Self {
        Self(Arc::new(Inner {
            running: Mutex::default(),
            clock: Mutex::new(Arc::new(RealClock::default())),
            tips: Mutex::default(),
            next: AtomicU64::new(1),
        }))
    }
}

fn looking_of(scope: &str, sides: &[SideKind]) -> LookingAgain {
    LookingAgain {
        scope: scope.to_owned(),
        code: sides.contains(&SideKind::Code),
        builds: sides.contains(&SideKind::Builds),
    }
}

impl Lookouts {
    /// The schedules of the scheduler's `FollowLandings`, or `None` for a
    /// scheduler not started by [`super::start_scheduler`], which looks for
    /// nothing.
    fn of(scheduler: &Scheduler) -> Option<Lookouts> {
        scheduler
            .deps()
            .events
            .as_any()?
            .downcast_ref::<super::write_queue::FollowLandings>()
            .map(super::write_queue::FollowLandings::lookouts)
    }

    fn clock(&self) -> Arc<dyn Clock> {
        Arc::clone(&self.0.clock.lock().unwrap_or_else(PoisonError::into_inner))
    }

    /// Note `scope` as looked at with `sides`: stop its schedule (a restart,
    /// told nothing, as the new one tells its start) or, past [`AT_ONCE`], the
    /// oldest (told ended). Answers its id and stop, and what to tell.
    fn start(
        &self,
        scope: &str,
        sides: &[SideKind],
    ) -> (u64, watch::Receiver<bool>, Vec<LookingAgain>) {
        let id = self.0.next.fetch_add(1, Ordering::SeqCst);
        let (stop, stopped) = watch::channel(false);
        let mut running = self
            .0
            .running
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let mut told = Vec::new();
        if let Some(at) = running.iter().position(|each| each.looking.scope == scope)
            && let Some(old) = running.remove(at)
        {
            let _ = old.stop.send(true);
        }
        while running.len() >= AT_ONCE {
            if let Some(oldest) = running.pop_front() {
                let _ = oldest.stop.send(true);
                told.push(looking_of(&oldest.looking.scope, &[]));
            }
        }
        let looking = looking_of(scope, sides);
        told.push(looking.clone());
        running.push_back(Running { id, looking, stop });
        (id, stopped, told)
    }

    /// The schedule `id` of `scope` has `sides` left: what to tell, or
    /// nothing when it was stopped since. Ends it when none is left.
    fn left(&self, scope: &str, id: u64, sides: &[SideKind]) -> Option<LookingAgain> {
        let mut running = self
            .0
            .running
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let at = running.iter().position(|each| each.id == id)?;
        let looking = looking_of(scope, sides);
        if sides.is_empty() {
            running.remove(at);
        } else if let Some(each) = running.get_mut(at) {
            each.looking = looking.clone();
        }
        Some(looking)
    }

    /// The schedule `id` ended without telling (stopped at the quit).
    fn gone(&self, id: u64) {
        self.0
            .running
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|each| each.id != id);
    }

    fn looking(&self) -> Vec<LookingAgain> {
        self.0
            .running
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .map(|each| each.looking.clone())
            .collect()
    }
}

/// The schedules running now, oldest first: empty for a scheduler that
/// looks for nothing. What the `looking_again` command answers, so a page
/// opened during a schedule says so.
#[must_use]
pub fn looking(scheduler: &Scheduler) -> Vec<LookingAgain> {
    Lookouts::of(scheduler).map_or_else(Vec::new, |lookouts| lookouts.looking())
}

/// Whether `source`'s branch tips are noted, so its next run that moves one
/// starts a schedule ([`after_run`]). For a test.
#[must_use]
pub fn noted(scheduler: &Scheduler, source: &str) -> bool {
    Lookouts::of(scheduler).is_some_and(|lookouts| {
        lookouts
            .0
            .tips
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains_key(source)
    })
}

/// Keep the schedules on `clock` from now on: a test's, moved by hand.
pub fn use_clock(scheduler: &Scheduler, clock: Arc<dyn Clock>) {
    if let Some(lookouts) = Lookouts::of(scheduler) {
        *lookouts
            .0
            .clock
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = clock;
    }
}

/// Look at `scope` with `sides` on the schedule, in the scheduler's
/// background set, stopping any schedule of it first. Answers whether it
/// began: not for a scheduler that looks for nothing, with no side, or once
/// the quit has begun.
pub(crate) fn begin(scheduler: &Scheduler, scope: &str, sides: Vec<Box<dyn Side>>) -> bool {
    let Some(lookouts) = Lookouts::of(scheduler) else {
        return false;
    };
    if sides.is_empty() || scheduler.background().is_closed() {
        return false;
    }
    let clock = lookouts.clock();
    let began = clock.now();
    let (id, stop, told) = lookouts.start(scope, &kinds(&sides));
    let events = Arc::clone(&scheduler.deps().events);
    for looking in &told {
        events.looking_again(looking);
    }
    let (task, scope) = (scheduler.clone(), scope.to_owned());
    let spawned = scheduler.background().spawn(async move {
        let background = task.background().clone();
        let closed = move || background.is_closed();
        let tell = |left: &[SideKind]| {
            if let Some(looking) = lookouts.left(&scope, id, left) {
                task.deps().events.looking_again(&looking);
            }
        };
        if !run(clock.as_ref(), began, sides, stop, &closed, &tell).await {
            lookouts.gone(id);
        }
    });
    if !spawned && let Some(lookouts) = Lookouts::of(scheduler) {
        lookouts.gone(id);
    }
    spawned
}

// -- the sides of a repository ---------------------------------------------

/// A refresh under the lock only if free, in the background lane.
async fn read(scheduler: &Scheduler, entity: &str) -> Result<RefreshOutcome, RefreshError> {
    knobas_http::behind(super::refresh::refresh(scheduler, entity, None, Lock::Try)).await
}

/// Whether a read's answer can never move: the side is over.
fn over(outcome: &Result<RefreshOutcome, RefreshError>) -> bool {
    matches!(
        outcome,
        Ok(RefreshOutcome::NotOffered)
            | Err(RefreshError::TurnedOff(_)
                | RefreshError::NotConfigured(_)
                | RefreshError::NotAnEntity(_))
    )
}

/// The code side: the repository walked, and the pull request a write was
/// made on read by id.
struct CodeSide {
    scheduler: Scheduler,
    repository: String,
    items: Vec<String>,
}

#[async_trait]
impl Side for CodeSide {
    fn kind(&self) -> SideKind {
        SideKind::Code
    }

    async fn look(&mut self) -> Look {
        let mut moved = false;
        let mut readable = false;
        for item in &self.items {
            let outcome = read(&self.scheduler, item).await;
            if let Err(error @ (RefreshError::Build(_) | RefreshError::Sync(_))) = &outcome {
                tracing::info!(%item, %error, "a branch's code could not be read again");
            }
            readable |= !over(&outcome);
            moved |= matches!(
                outcome,
                Ok(RefreshOutcome::Refreshed | RefreshOutcome::Gone)
            );
        }
        // What this side read is knobas' own: the next run does not find it
        // moved (`after_run`).
        note_tips(&self.scheduler, &source_of(&self.repository), Noter::Own).await;
        if moved {
            Look::Moved
        } else if readable {
            Look::Quiet
        } else {
            Look::Over
        }
    }
}

/// The builds side: the repository's pipelines walked for their newest runs
/// (#2564), moved once the Builds tab's answer holds a run it did not when
/// the schedule began.
struct BuildsSide {
    scheduler: Scheduler,
    repository: String,
    seen: HashSet<String>,
}

impl BuildsSide {
    /// The side of `repository`, from what its Builds tab draws now: `None`
    /// when no pipeline builds it, or its answer cannot be read.
    async fn of(scheduler: &Scheduler, repository: &str) -> Option<Self> {
        let builds = repo_builds(scheduler, repository).await?;
        if builds.pipelines.is_empty() {
            return None;
        }
        Some(Self {
            scheduler: scheduler.clone(),
            repository: repository.to_owned(),
            seen: runs_of(&builds),
        })
    }
}

async fn repo_builds(
    scheduler: &Scheduler,
    repository: &str,
) -> Option<knobas_core::builds::RepoBuilds> {
    let deps = scheduler.deps();
    match crate::commands::entity::repo_pipelines_inner(
        &deps.pool,
        deps.registry.as_ref(),
        repository,
    )
    .await
    {
        Ok(builds) => Some(builds),
        Err(error) => {
            tracing::info!(%repository, ?error, "a repository's builds could not be read");
            None
        }
    }
}

fn runs_of(builds: &knobas_core::builds::RepoBuilds) -> HashSet<String> {
    builds
        .pipelines
        .iter()
        .flat_map(|pipeline| pipeline.latest.iter().map(|run| run.entity_id.clone()))
        .collect()
}

#[async_trait]
impl Side for BuildsSide {
    fn kind(&self) -> SideKind {
        SideKind::Builds
    }

    async fn look(&mut self) -> Look {
        let Some(builds) = repo_builds(&self.scheduler, &self.repository).await else {
            return Look::Quiet;
        };
        // One source's pipelines one at a time, since each read takes its
        // source's lock only when free; sources side by side (as the tab's
        // ⌘R, #2564).
        let mut by_source: HashMap<&str, Vec<&str>> = HashMap::new();
        for pipeline in &builds.pipelines {
            by_source
                .entry(pipeline.source_id.as_str())
                .or_default()
                .push(pipeline.entity_id.as_str());
        }
        let scheduler = &self.scheduler;
        futures_util::future::join_all(by_source.into_values().map(|pipelines| async move {
            for pipeline in pipelines {
                let _ = read(scheduler, pipeline).await;
            }
        }))
        .await;
        let Some(after) = repo_builds(&self.scheduler, &self.repository).await else {
            return Look::Quiet;
        };
        if runs_of(&after).iter().any(|run| !self.seen.contains(run)) {
            Look::Moved
        } else {
            Look::Quiet
        }
    }
}

fn source_of(entity: &str) -> String {
    entity
        .split_once(':')
        .map_or_else(String::new, |(source, _)| source.to_owned())
}

/// Look at `repository` on the schedule: its code side (the repository and
/// `items` besides) and its builds side where a pipeline builds it.
async fn look_at_repository(scheduler: &Scheduler, repository: &str, items: Vec<String>) -> bool {
    let mut read = vec![repository.to_owned()];
    read.extend(items.into_iter().filter(|item| item != repository));
    let mut sides: Vec<Box<dyn Side>> = vec![Box::new(CodeSide {
        scheduler: scheduler.clone(),
        repository: repository.to_owned(),
        items: read,
    })];
    if let Some(builds) = BuildsSide::of(scheduler, repository).await {
        sides.push(Box::new(builds));
    }
    begin(scheduler, repository, sides)
}

// -- the triggers -----------------------------------------------------------

/// After a landed write's effects were read ([`super::effects::read`]): where
/// its op declares [`Effect::Parent`] (a write that moves a branch), look at
/// the repository on the schedule. The repository is the target's declared
/// parent, or the target itself where it is one (a branch made or a file
/// committed on a repository). A pull request the write was made on is read
/// by id besides. Answers the repositories looked at.
pub(crate) async fn after_write(
    scheduler: &Scheduler,
    write: &QueuedWrite,
    effects: &[Effect],
) -> Vec<String> {
    if !effects.contains(&Effect::Parent) || Lookouts::of(scheduler).is_none() {
        return Vec::new();
    }
    let pool = &scheduler.deps().pool;
    let target = write.entity_id.as_str();
    let mut repositories = match super::effects::parents(pool, target).await {
        Ok(parents) => parents,
        Err(error) => {
            tracing::warn!(write_id = write.id, %error, "a landed write's repository could not be found");
            return Vec::new();
        }
    };
    if repositories.is_empty() && answers_as_a_parent(pool, target).await {
        repositories.push(target.to_owned());
    }
    let items = if is_a_proposal(scheduler, target).await {
        vec![target.to_owned()]
    } else {
        Vec::new()
    };
    note_tips(scheduler, &write.source_id, Noter::Own).await;
    let mut began = Vec::new();
    for repository in repositories {
        if look_at_repository(scheduler, &repository, items.clone()).await {
            began.push(repository);
        }
    }
    began
}

/// Whether `entity` is a live parent of something (`sync.item_parent`).
async fn answers_as_a_parent(pool: &sqlx::PgPool, entity: &str) -> bool {
    sqlx::query_scalar::<_, bool>(
        "select exists (select 1 from sync.item_parent p
                         join sync.live_item l on l.entity_id = p.entity_id
                        where p.arm = 'parent' and p.entity_id = $1)",
    )
    .bind(entity)
    .fetch_one(pool)
    .await
    .unwrap_or(false)
}

/// Whether `entity` is a proposal (a pull request): its kind declares the
/// branch it is opened from (`KindPaths::head`).
async fn is_a_proposal(scheduler: &Scheduler, entity: &str) -> bool {
    let deps = scheduler.deps();
    let kind: Option<String> =
        sqlx::query_scalar("select kind from sync.live_item where entity_id = $1")
            .bind(entity)
            .fetch_optional(&deps.pool)
            .await
            .ok()
            .flatten();
    let Some(kind) = kind else {
        return false;
    };
    let Ok(declarations) = super::paths::declared_paths(&deps.pool, deps.registry.as_ref()).await
    else {
        return false;
    };
    declarations
        .get(&source_of(entity), &kind)
        .is_some_and(|paths| !paths.head.is_empty())
}

/// The tips of `source`'s live branches, by entity id, from the mirror.
async fn tips(scheduler: &Scheduler, source: &str) -> Option<HashMap<String, String>> {
    let deps = scheduler.deps();
    let declarations = super::paths::declared_paths(&deps.pool, deps.registry.as_ref())
        .await
        .ok()?;
    let paths = declarations.get(source, BRANCH_KIND)?;
    if paths.commit.is_empty() {
        return None;
    }
    let rows: Vec<(String, serde_json::Value)> = sqlx::query_as(
        "select entity_id, payload from sync.live_item where source_id = $1 and kind = $2",
    )
    .bind(source)
    .bind(BRANCH_KIND)
    .fetch_all(&deps.pool)
    .await
    .ok()?;
    Some(
        rows.into_iter()
            .filter_map(|(entity, payload)| {
                knobas_core::payload::resolve_string(&payload, &paths.commit)
                    .map(|tip| (entity, tip))
            })
            .collect(),
    )
}

/// Who notes a source's branch tips.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Noter {
    /// A run of the source ended `ok`, having written something or not.
    Run { changed: bool },
    /// A read of knobas' own (a write's effects, a schedule's code side): what
    /// it brought is not a push somebody else made.
    Own,
}

/// Note `source`'s branch tips as the mirror holds them now, answering the
/// branches whose tip moved (or that are new) since they were last noted,
/// for a run that wrote something. The first run after the start, even one
/// that wrote nothing, takes the first note; a run that wrote nothing after
/// it notes nothing; a read of knobas' own notes only where a note was taken.
async fn note_tips(scheduler: &Scheduler, source: &str, by: Noter) -> Vec<String> {
    let Some(lookouts) = Lookouts::of(scheduler) else {
        return Vec::new();
    };
    let noted = lookouts
        .0
        .tips
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .contains_key(source);
    let take = match by {
        Noter::Run { changed } => changed || !noted,
        Noter::Own => noted,
    };
    if !take {
        return Vec::new();
    }
    let Some(now) = tips(scheduler, source).await else {
        return Vec::new();
    };
    let before = lookouts
        .0
        .tips
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(source.to_owned(), now.clone());
    let (Noter::Run { changed: true }, Some(before)) = (by, before) else {
        return Vec::new();
    };
    let mut moved: Vec<String> = now
        .into_iter()
        .filter(|(entity, tip)| before.get(entity) != Some(tip))
        .map(|(entity, _)| entity)
        .collect();
    moved.sort();
    moved
}

/// After a run of `source` ended `ok`: where it found a branch's tip moved
/// since the last run (a push knobas did not make), look at each moved
/// branch's repository on the schedule. The first run after the start only
/// notes the tips; a later run that wrote nothing (`changed` false) is not
/// compared. Answers the repositories looked at.
pub(crate) async fn after_run(scheduler: &Scheduler, source: &str, changed: bool) -> Vec<String> {
    let moved = note_tips(scheduler, source, Noter::Run { changed }).await;
    if moved.is_empty() {
        return Vec::new();
    }
    let deps = scheduler.deps();
    // The declarations `sync.item_parent` is kept from, as for an effect.
    if let Err(error) =
        knobas_search::Searcher::declaring(deps.pool.clone(), deps.registry.descriptors()).await
    {
        tracing::warn!(%source, %error, "the declared parents could not be stated");
    }
    let pool = &deps.pool;
    let mut repositories = Vec::new();
    for branch in &moved {
        match super::effects::parents(pool, branch).await {
            Ok(parents) => {
                for parent in parents {
                    if !repositories.contains(&parent) {
                        repositories.push(parent);
                    }
                }
            }
            Err(error) => {
                tracing::info!(%branch, %error, "a moved branch's repository could not be found");
            }
        }
    }
    let mut began = Vec::new();
    for repository in repositories {
        if look_at_repository(scheduler, &repository, Vec::new()).await {
            began.push(repository);
        }
    }
    began
}

#[cfg(test)]
mod tests;

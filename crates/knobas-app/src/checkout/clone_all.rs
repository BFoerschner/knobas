//! *Clone all* (the smoothness pass of PR #2479): every repository the mirror
//! holds that has no checkout on this machine, cloned into the clones root
//! without opening each one.
//!
//! Each clone is *Clone*'s own ([`super::clone`], ADR-0023): the same URL,
//! the same new folder, the same refusals. This only picks the
//! repositories, runs [`AT_ONCE`] at a time, and keeps one run's tally.
//!
//! - **Said first:** [`plan`] answers how many, from which sources, and
//!   which cannot be cloned and why, reading the mirror and the disk only.
//! - **One run per app:** a second [`start`] while one runs answers that
//!   run's state and starts nothing, so two presses never start two runs.
//! - **In the background,** told on `clone:all` at each change ([`State`]);
//!   [`state`] answers the last one, so a page left and come back to draws
//!   where it is.
//! - **A failure is one row,** never the end: the rest go on, and the last
//!   state lists every failure with git's or knobas's reason.

use std::sync::{Arc, LazyLock, Mutex, PoisonError};

use serde::Serialize;
use sqlx::{PgPool, Row};
use tokio::sync::oneshot;

use knobas_sync::scheduler::AdapterRegistry;

use super::clone::{CloneAnswer, CloneEvent, CloneEvents, CloneState, Clones};
use crate::IpcError;

/// How many clones run side by side.
pub const AT_ONCE: usize = 2;

/// One source's share of a plan.
///
/// The smoothness pass (PR #2479).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts", rename = "CloneAllSource")]
pub struct PlannedSource {
    pub source_id: String,
    pub name: String,
    /// Its repositories this run would clone.
    pub repositories: u32,
}

/// A repository *Clone all* would not clone, and why.
///
/// The smoothness pass (PR #2479).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts", rename = "CloneAllSkipped")]
pub struct Skipped {
    pub entity_id: String,
    pub name: String,
    pub reason: String,
}

/// What *Clone all* would do, said before it starts.
///
/// The smoothness pass (PR #2479).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts", rename = "CloneAllPlan")]
pub struct Plan {
    /// Where the clones go; `None` when no clones root is set, and nothing
    /// can be cloned.
    pub root: Option<String>,
    /// The repositories it would clone, by name, in order.
    pub names: Vec<String>,
    pub sources: Vec<PlannedSource>,
    /// Repositories already on this machine, which it leaves alone.
    pub already: u32,
    /// Repositories it cannot clone, each with its reason.
    pub skipped: Vec<Skipped>,
}

/// One clone of a run that failed.
///
/// The smoothness pass (PR #2479).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts", rename = "CloneAllFailure")]
pub struct Failed {
    pub entity_id: String,
    pub name: String,
    pub reason: String,
}

/// Where a run of *Clone all* is: the `clone:all` payload and
/// [`state`]'s answer.
///
/// The smoothness pass (PR #2479).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts", rename = "CloneAllState")]
pub struct State {
    /// Each run's own number, so the shell tells a new run from the last.
    pub run: u64,
    /// How many it set out to clone.
    pub total: u32,
    /// Cloned so far.
    pub cloned: u32,
    /// Found already cloned in the clones root when its turn came.
    pub adopted: u32,
    pub failed: Vec<Failed>,
    /// The repositories cloning right now, by name.
    pub current: Vec<String>,
    /// Every repository has had its turn.
    pub finished: bool,
}

/// Who hears a run's states: the bridge in the app, a list in a test.
pub type Listener = Arc<dyn Fn(State) + Send + Sync>;

/// The run, one per app.
#[derive(Default)]
pub struct CloneAll {
    state: Mutex<Option<State>>,
    runs: std::sync::atomic::AtomicU64,
}

impl CloneAll {
    /// The one the app's commands use.
    #[must_use]
    pub fn global() -> Arc<CloneAll> {
        static GLOBAL: LazyLock<Arc<CloneAll>> = LazyLock::new(Arc::default);
        Arc::clone(&GLOBAL)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<State>> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// One repository the mirror holds.
struct Repo {
    entity_id: String,
    name: String,
    source_id: String,
    source_name: String,
}

async fn repositories(pool: &PgPool) -> Result<Vec<Repo>, IpcError> {
    let rows = sqlx::query(
        "select i.entity_id, i.title, i.source_id, coalesce(c.display_name, i.source_id) as source_name
           from sync.live_item i
           join knobas.source_config c on c.id = i.source_id
          where i.kind = 'repo' and coalesce(c.enabled, true)
          order by c.display_name, i.title, i.entity_id",
    )
    .fetch_all(pool)
    .await
    .map_err(IpcError::internal)?;
    Ok(rows
        .into_iter()
        .map(|row| Repo {
            entity_id: row.get("entity_id"),
            name: row.get("title"),
            source_id: row.get("source_id"),
            source_name: row.get("source_name"),
        })
        .collect())
}

/// What a run would clone now: every repository the mirror holds with no
/// checkout here and an address to clone from. Reads the mirror, the
/// settings and the disk; asks no source and runs no git.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
pub async fn plan(
    pool: &PgPool,
    registry: &dyn AdapterRegistry,
    protocols: &[&str],
) -> Result<(Plan, Vec<String>), IpcError> {
    let root = super::clones_root(pool).await?;
    let mut plan = Plan {
        root: root.clone(),
        names: Vec::new(),
        sources: Vec::new(),
        already: 0,
        skipped: Vec::new(),
    };
    let mut ids = Vec::new();
    for repo in repositories(pool).await? {
        if super::resolve(pool, &repo.entity_id)
            .await
            .ok()
            .and_then(|c| c.path)
            .is_some()
        {
            plan.already += 1;
            continue;
        }
        if root.is_none() {
            continue;
        }
        if let Err(error) = super::clone::url_of(pool, registry, &repo.entity_id, protocols).await {
            plan.skipped.push(Skipped {
                entity_id: repo.entity_id,
                name: repo.name,
                reason: error.message,
            });
            continue;
        }
        match plan
            .sources
            .iter_mut()
            .find(|s| s.source_id == repo.source_id)
        {
            Some(source) => source.repositories += 1,
            None => plan.sources.push(PlannedSource {
                source_id: repo.source_id.clone(),
                name: repo.source_name.clone(),
                repositories: 1,
            }),
        }
        plan.names.push(repo.name);
        ids.push(repo.entity_id);
    }
    Ok((plan, ids))
}

/// Start a run over [`plan`]'s repositories, or answer the one running.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) with no clones root;
/// [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
pub async fn start(
    pool: &PgPool,
    registry: Arc<dyn AdapterRegistry>,
    clones: &Arc<Clones>,
    all: &Arc<CloneAll>,
    events: Arc<dyn CloneEvents>,
    listener: Listener,
) -> Result<State, IpcError> {
    start_admitting(
        pool,
        registry,
        clones,
        all,
        events,
        listener,
        &knobas_core::checkout::clone::PROTOCOLS,
    )
    .await
}

/// [`start`], admitting `protocols`: [`super::clone::start_admitting`]'s door,
/// for the testenv Gitea's http.
///
/// # Errors
///
/// As [`start`].
pub async fn start_admitting(
    pool: &PgPool,
    registry: Arc<dyn AdapterRegistry>,
    clones: &Arc<Clones>,
    all: &Arc<CloneAll>,
    events: Arc<dyn CloneEvents>,
    listener: Listener,
    protocols: &[&'static str],
) -> Result<State, IpcError> {
    // Claimed before anything is read, so two presses at once start one run.
    let run = {
        let mut state = all.lock();
        if let Some(running) = state.as_ref().filter(|s| !s.finished) {
            return Ok(running.clone());
        }
        let run = all.runs.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        *state = Some(State {
            run,
            total: 0,
            cloned: 0,
            adopted: 0,
            failed: Vec::new(),
            current: Vec::new(),
            finished: false,
        });
        run
    };
    let planned = plan(pool, registry.as_ref(), protocols).await;
    let (plan, ids) = match planned {
        Ok(planned) if planned.0.root.is_some() => planned,
        Ok(_) => {
            *all.lock() = None;
            return Err(IpcError::invalid(
                "no clones root is set: choose where clones go in Settings › Checkouts",
            ));
        }
        Err(error) => {
            *all.lock() = None;
            return Err(error);
        }
    };
    let first = {
        let mut state = all.lock();
        let s = state.as_mut().expect("claimed above");
        s.total = u32::try_from(ids.len()).unwrap_or(u32::MAX);
        s.finished = ids.is_empty();
        s.clone()
    };
    listener(first.clone());
    if ids.is_empty() {
        return Ok(first);
    }
    let names: Vec<String> = plan.names;
    let work: Arc<Mutex<Vec<(String, String)>>> =
        Arc::new(Mutex::new(ids.into_iter().zip(names).rev().collect()));
    let pool = pool.clone();
    let clones = Arc::clone(clones);
    let all = Arc::clone(all);
    let protocols: Vec<&'static str> = protocols.to_vec();
    tokio::spawn(async move {
        let mut workers = tokio::task::JoinSet::new();
        for _ in 0..AT_ONCE {
            let (pool, registry, clones, all, events, listener, work, protocols) = (
                pool.clone(),
                Arc::clone(&registry),
                Arc::clone(&clones),
                Arc::clone(&all),
                Arc::clone(&events),
                Arc::clone(&listener),
                Arc::clone(&work),
                protocols.clone(),
            );
            workers.spawn(async move {
                loop {
                    let next = work.lock().unwrap_or_else(PoisonError::into_inner).pop();
                    let Some((id, name)) = next else { break };
                    tell(&all, &listener, run, |s| s.current.push(name.clone()));
                    let ended =
                        one(&pool, registry.as_ref(), &clones, &events, &id, &protocols).await;
                    tell(&all, &listener, run, |s| {
                        s.current.retain(|n| *n != name);
                        match ended {
                            Ok(Ended::Cloned) => s.cloned += 1,
                            Ok(Ended::Adopted) => s.adopted += 1,
                            Err(reason) => s.failed.push(Failed {
                                entity_id: id.clone(),
                                name: name.clone(),
                                reason,
                            }),
                        }
                    });
                }
            });
        }
        while workers.join_next().await.is_some() {}
        tell(&all, &listener, run, |s| s.finished = true);
    });
    Ok(first)
}

/// Change the run's state, if it is still run `run`, and tell the listener.
fn tell(all: &CloneAll, listener: &Listener, run: u64, change: impl FnOnce(&mut State)) {
    let told = {
        let mut state = all.lock();
        match state.as_mut().filter(|s| s.run == run) {
            Some(s) => {
                change(s);
                s.clone()
            }
            None => return,
        }
    };
    listener(told);
}

/// The run's state, or the last run's; `None` before the first.
#[must_use]
pub fn state(all: &CloneAll) -> Option<State> {
    all.lock().clone()
}

enum Ended {
    Cloned,
    Adopted,
}

/// One clone, to its end: *Clone*'s start, then its ending event.
async fn one(
    pool: &PgPool,
    registry: &dyn AdapterRegistry,
    clones: &Arc<Clones>,
    events: &Arc<dyn CloneEvents>,
    entity_id: &str,
    protocols: &[&'static str],
) -> Result<Ended, String> {
    let (sender, ended) = oneshot::channel();
    let relay = Arc::new(Relay {
        inner: Arc::clone(events),
        ended: Mutex::new(Some(sender)),
    });
    match super::clone::start_with(pool, registry, clones, relay, entity_id, protocols).await {
        Ok(CloneAnswer::Adopted { .. }) => Ok(Ended::Adopted),
        Ok(CloneAnswer::Cloning { .. }) => match ended.await {
            Ok(CloneState::Done) => Ok(Ended::Cloned),
            Ok(CloneState::Failed { reason, .. }) => Err(reason),
            Ok(CloneState::Stopped { .. }) => Err("stopped".to_owned()),
            Ok(CloneState::Progress { .. }) | Err(_) => {
                Err("the clone ended without saying how".to_owned())
            }
        },
        Ok(other) => Err(format!("{other:?}")),
        Err(error) => Err(error.message),
    }
}

/// Passes a clone's events on, and its ending to the run.
struct Relay {
    inner: Arc<dyn CloneEvents>,
    ended: Mutex<Option<oneshot::Sender<CloneState>>>,
}

impl CloneEvents for Relay {
    fn clone_progress(&self, event: CloneEvent) {
        if !matches!(event.state, CloneState::Progress { .. })
            && let Some(sender) = self
                .ended
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take()
        {
            let _ = sender.send(event.state.clone());
        }
        self.inner.clone_progress(event);
    }
}

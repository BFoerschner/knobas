//! **A run started in knobas is read to its end** (the TeamCity run fix,
//! 2026-11-25, `docs/decisions/2026-11-25-a-started-run-moves-on-at-once.md`,
//! Fable's ruling 1).
//!
//! Björn: "Team city isn't syncing a started run immediately after it's
//! started." A landed *Run* is read back by id (ADR-0031): the mirror holds the
//! build as TeamCity queued it. Then nothing read it again until the source's
//! next scheduled run, 300 s on the live sources: TeamCity's agent started the
//! build within seconds and every page said *queued* for minutes, then
//! *running* for minutes after it had finished.
//!
//! So the read-back goes on. Where the item a landed write was read back by is
//! a run (its kind's `frame` is `run`) that has not finished, it is refreshed
//! by id again ([`super::refresh::refresh`], `Lock::Try`) on a falling cadence
//! until it has:
//!
//! - every 2 s for the first 30 s after the read-back, every 5 s up to 5 min,
//!   every 15 s after that;
//! - as a person's read for the first 30 s, then in the background lane
//!   (`knobas_http::behind`, ADR-0042);
//! - a run of the source holding its lock skips the tick (`RunInFlight`): its
//!   own in-flight poll reads the build;
//! - a tick whose item arrived as the mirror holds it tells nothing
//!   (`RefreshOutcome::Unchanged`), so a build waiting for an agent costs the
//!   window nothing.
//!
//! It ends when the run has finished or is gone, after 2 h, after 5 failed
//! reads in a row, when the source is turned off or removed, or at the quit.
//! It is not taken up again after a restart: the pages read the runs they draw
//! in flight when shown, and the next run of the source reads every build in
//! flight. At most [`PER_SOURCE`] runs of one source are followed; a newer one
//! stops the oldest.
//!
//! Not polling for somebody else's change: the press started a process, and
//! reading it to its end is that write's read-back.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use knobas_sync::refresh::{Lock, RefreshError, RefreshOutcome};
use knobas_sync::scheduler::Scheduler;

/// How many runs of one source are followed at once.
pub const PER_SOURCE: usize = 5;

/// How long a run is followed at most.
pub const AT_MOST: Duration = Duration::from_secs(2 * 60 * 60);

/// How long a followed run's reads go as a person's, before the background lane.
const AS_A_PERSON: Duration = Duration::from_secs(30);

/// How many failed reads in a row end a follow.
const FAILURES: u32 = 5;

/// How long the wait between two reads looks up to see the quit or a stop.
const NAP: Duration = Duration::from_millis(250);

/// The wait before the next read, `elapsed` after the read-back: 2 s for the
/// first 30 s, 5 s up to 5 min, 15 s after that, `None` past [`AT_MOST`].
#[must_use]
pub fn next_wait(elapsed: Duration) -> Option<Duration> {
    if elapsed >= AT_MOST {
        None
    } else if elapsed < Duration::from_secs(30) {
        Some(Duration::from_secs(2))
    } else if elapsed < Duration::from_secs(5 * 60) {
        Some(Duration::from_secs(5))
    } else {
        Some(Duration::from_secs(15))
    }
}

/// The runs being followed, by source, oldest first, each with its stop.
#[derive(Clone, Default)]
pub(crate) struct RunFollows(Arc<Mutex<HashMap<String, VecDeque<Followed>>>>);

#[derive(Clone)]
struct Followed {
    entity: String,
    stop: Arc<AtomicBool>,
}

impl RunFollows {
    /// The follows of the scheduler's `FollowLandings`, or `None` for a
    /// scheduler not started by [`super::start_scheduler`], which follows
    /// nothing.
    fn of(scheduler: &Scheduler) -> Option<RunFollows> {
        scheduler
            .deps()
            .events
            .as_any()?
            .downcast_ref::<super::write_queue::FollowLandings>()
            .map(super::write_queue::FollowLandings::runs)
    }

    /// Note `entity` of `source` as followed, stopping the oldest past
    /// [`PER_SOURCE`]. `None` when it is followed already.
    fn begin(&self, source: &str, entity: &str) -> Option<Arc<AtomicBool>> {
        let mut all = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let runs = all.entry(source.to_owned()).or_default();
        if runs.iter().any(|run| run.entity == entity) {
            return None;
        }
        while runs.len() >= PER_SOURCE {
            if let Some(oldest) = runs.pop_front() {
                oldest.stop.store(true, Ordering::SeqCst);
            }
        }
        let stop = Arc::new(AtomicBool::new(false));
        runs.push_back(Followed {
            entity: entity.to_owned(),
            stop: Arc::clone(&stop),
        });
        Some(stop)
    }

    /// The follow `stop` belongs to has ended.
    fn end(&self, source: &str, stop: &Arc<AtomicBool>) {
        let mut all = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(runs) = all.get_mut(source) {
            runs.retain(|run| !Arc::ptr_eq(&run.stop, stop));
            if runs.is_empty() {
                all.remove(source);
            }
        }
    }

    /// The runs of `source` being followed now, oldest first.
    #[must_use]
    pub(crate) fn of_source(&self, source: &str) -> Vec<String> {
        let all = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        all.get(source)
            .map(|runs| runs.iter().map(|run| run.entity.clone()).collect())
            .unwrap_or_default()
    }
}

/// The runs of `source` being followed now, oldest first: empty for a
/// scheduler that follows nothing. For a test.
#[must_use]
pub fn followed(scheduler: &Scheduler, source: &str) -> Vec<String> {
    RunFollows::of(scheduler).map_or_else(Vec::new, |runs| runs.of_source(source))
}

/// Whether `entity` is a run in the mirror, live and not finished: its kind
/// declares the run frame and its payload no `finished` instant.
pub(crate) async fn in_flight(scheduler: &Scheduler, entity: &str) -> bool {
    use knobas_core::payload::{Frame, resolve_string};
    let deps = scheduler.deps();
    let Ok(parsed) = knobas_core::entity::EntityRef::parse(entity) else {
        return false;
    };
    let row: Option<(String, serde_json::Value)> =
        sqlx::query_as("select kind, payload from sync.live_item where entity_id = $1")
            .bind(entity)
            .fetch_optional(&deps.pool)
            .await
            .ok()
            .flatten();
    let Some((kind, payload)) = row else {
        return false;
    };
    let Ok(declarations) = super::paths::declared_paths(&deps.pool, deps.registry.as_ref()).await
    else {
        return false;
    };
    declarations
        .get(&parsed.namespace, &kind)
        .is_some_and(|paths| {
            paths.frame == Some(Frame::Run) && resolve_string(&payload, &paths.finished).is_none()
        })
}

/// After a landed write's read-back of `entity`: follow it, in the
/// scheduler's background set, when it is a run in flight. Answers whether a
/// follow began.
pub(crate) async fn follow_if_in_flight(scheduler: &Scheduler, source: &str, entity: &str) -> bool {
    let Some(follows) = RunFollows::of(scheduler) else {
        return false;
    };
    if !in_flight(scheduler, entity).await {
        return false;
    }
    let Some(stop) = follows.begin(source, entity) else {
        return false;
    };
    let (task, source_id, entity_id, stopped) = (
        scheduler.clone(),
        source.to_owned(),
        entity.to_owned(),
        Arc::clone(&stop),
    );
    let spawned = scheduler.background().spawn(async move {
        follow(&task, &entity_id, &stopped).await;
        follows.end(&source_id, &stopped);
    });
    if !spawned {
        // The quit has begun: the next start's scheduled run reads it.
        if let Some(follows) = RunFollows::of(scheduler) {
            follows.end(source, &stop);
        }
    }
    spawned
}

/// Read `entity` again on [`next_wait`]'s cadence until it is no run in
/// flight, or the follow ends otherwise (the module's list).
async fn follow(scheduler: &Scheduler, entity: &str, stop: &AtomicBool) {
    let began = Instant::now();
    let mut failed = 0;
    while let Some(wait) = next_wait(began.elapsed()) {
        if !nap(scheduler, wait, stop).await {
            return;
        }
        let read = super::refresh::refresh(scheduler, entity, None, Lock::Try);
        let outcome = if began.elapsed() < AS_A_PERSON {
            read.await
        } else {
            knobas_http::behind(read).await
        };
        match outcome {
            Ok(RefreshOutcome::Refreshed | RefreshOutcome::Unchanged) => failed = 0,
            // A run holds the lock: its in-flight poll reads the build.
            Ok(RefreshOutcome::RunInFlight) => continue,
            Ok(RefreshOutcome::Gone | RefreshOutcome::NotOffered)
            | Err(
                RefreshError::TurnedOff(_)
                | RefreshError::NotConfigured(_)
                | RefreshError::NotAnEntity(_),
            ) => return,
            Err(error @ (RefreshError::Build(_) | RefreshError::Sync(_))) => {
                failed += 1;
                tracing::info!(%entity, failed, %error, "a followed run could not be read");
                if failed >= FAILURES {
                    return;
                }
                continue;
            }
        }
        if !in_flight(scheduler, entity).await {
            return;
        }
    }
    tracing::info!(%entity, "a followed run is still in flight after 2 h; the source's runs read it from here");
}

/// Wait `wait`, looking up every [`NAP`]: `false` once the quit has begun or
/// the follow was stopped.
async fn nap(scheduler: &Scheduler, wait: Duration, stop: &AtomicBool) -> bool {
    let until = Instant::now() + wait;
    loop {
        if stop.load(Ordering::SeqCst) || scheduler.background().is_closed() {
            return false;
        }
        let left = until.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return true;
        }
        tokio::time::sleep(left.min(NAP)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cadence_falls_from_two_to_five_to_fifteen_seconds_and_ends_at_two_hours() {
        let s = Duration::from_secs;
        assert_eq!(next_wait(s(0)), Some(s(2)));
        assert_eq!(next_wait(s(29)), Some(s(2)));
        assert_eq!(next_wait(s(30)), Some(s(5)));
        assert_eq!(next_wait(s(299)), Some(s(5)));
        assert_eq!(next_wait(s(300)), Some(s(15)));
        assert_eq!(next_wait(s(7_199)), Some(s(15)));
        assert_eq!(next_wait(s(7_200)), None);
    }

    #[test]
    fn a_sixth_run_of_a_source_stops_the_oldest_and_a_run_is_followed_once() {
        let follows = RunFollows::default();
        let stops: Vec<_> = (0..PER_SOURCE)
            .map(|n| {
                follows
                    .begin("tc", &format!("tc:build:{n}"))
                    .expect("a new run")
            })
            .collect();
        assert!(
            follows.begin("tc", "tc:build:3").is_none(),
            "followed already"
        );
        let sixth = follows.begin("tc", "tc:build:9").expect("a new run");
        assert!(stops[0].load(Ordering::SeqCst), "the oldest is stopped");
        assert!(stops[1..].iter().all(|stop| !stop.load(Ordering::SeqCst)));
        assert_eq!(follows.of_source("tc").len(), PER_SOURCE);
        assert!(
            follows.begin("other", "other:build:1").is_some(),
            "per source"
        );
        follows.end("tc", &sixth);
        assert!(!follows.of_source("tc").contains(&"tc:build:9".to_owned()));
    }
}

//! The tasks that use the scheduler's pool without being sync runs: a write's
//! send behind an answer that has already gone back (#752), the owed links
//! resolved after a sync run (#1050), and a start-work flow's steps behind
//! the stepper's answer (#1751).
//!
//! Held so the quit can wait for them (#1633, spec #1619 ruling B1). Cancel
//! first: once [`Background::close`] has begun, nothing new is spawned, and a
//! write queued after that stays pending for the next start's tick. Then the
//! ones running are awaited for a grace period, and only then is the pool
//! closed. A send that outlives the grace is aborted: that is ADR-0012's
//! window, a write the source may have received while its row still says
//! pending.

use std::future::Future;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use tokio::task::JoinSet;
use tokio::time::Instant;

/// The background tasks, shared: every clone is a handle onto one set.
#[derive(Clone)]
pub struct Background {
    /// `None` once [`close`](Self::close) has taken the set: the closed state
    /// and the set are one value under one lock, so a spawn either lands in
    /// the set the quit awaits or is refused.
    tasks: Arc<Mutex<Option<JoinSet<()>>>>,
    /// How many spawned tasks have not finished, been aborted or been
    /// dropped ([`running`](Self::running)). Kept apart from the set because
    /// [`close`](Self::close) takes the set away while it waits for them.
    running: Arc<AtomicUsize>,
}

/// One task's place in [`Background::running`], given back when the task's
/// future is dropped: at its end, or at an abort.
struct Running(Arc<AtomicUsize>);

impl Drop for Running {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

impl Default for Background {
    fn default() -> Self {
        Self::new()
    }
}

impl Background {
    /// An open set with nothing in it.
    #[must_use]
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(Mutex::new(Some(JoinSet::new()))),
            running: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Run `task` in the background, unless the quit has begun. Answers
    /// whether it was spawned; a caller whose work is on a row has nothing to
    /// undo when it was not, since the row is the record and the next start
    /// picks it up.
    ///
    /// Must be called inside a Tokio runtime, as `tokio::spawn` must.
    pub fn spawn<F>(&self, task: F) -> bool
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.try_spawn(task).is_ok()
    }

    /// [`spawn`](Self::spawn), handing `task` back unrun once the quit has
    /// begun: for a caller that waits for the task's answer and has to say
    /// something when there is none (#1932).
    ///
    /// # Errors
    ///
    /// `task` itself, untouched, if [`close`](Self::close) has begun.
    pub fn try_spawn<F>(&self, task: F) -> Result<(), F>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let mut tasks = self.tasks.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(tasks) = tasks.as_mut() else {
            return Err(task);
        };
        // Reaped on every spawn: a finished task's slot is kept until it is
        // joined, and a week-long session spawns one per write.
        while tasks.try_join_next().is_some() {}
        self.running.fetch_add(1, Ordering::SeqCst);
        let place = Running(Arc::clone(&self.running));
        tasks.spawn(async move {
            let _place = place;
            task.await;
        });
        Ok(())
    }

    /// How many tasks are in the set and not yet over: neither finished nor
    /// aborted. The quit waits for exactly these, so a test reads here that a
    /// task is one the quit will wait for.
    #[must_use]
    pub fn running(&self) -> usize {
        self.running.load(Ordering::SeqCst)
    }

    /// Whether [`close`](Self::close) has begun: from then on nothing is
    /// spawned. For a task in the set that does its work in steps -- a
    /// start-work flow (#1751) -- to stop before the next one rather than
    /// start a send the quit would abort.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.tasks
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_none()
    }

    /// Refuse every later spawn **now**, and answer the wait: until
    /// `deadline` for the tasks running, then abort what is left. The refusal
    /// is not deferred to the first poll, so a caller can close the set at
    /// the start of a quit and await it later. Idempotent: a second call finds
    /// the set already taken and its wait is empty.
    pub fn close(&self, deadline: Instant) -> impl Future<Output = ()> + Send + 'static {
        let taken = self
            .tasks
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        async move {
            let Some(mut tasks) = taken else {
                return;
            };
            let drained = async { while tasks.join_next().await.is_some() {} };
            if tokio::time::timeout_at(deadline, drained).await.is_err() {
                tracing::warn!(
                    left = tasks.len(),
                    "a background task did not finish within the grace period: aborting it"
                );
                tasks.abort_all();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[tokio::test]
    async fn a_closed_set_spawns_nothing_and_close_waits_for_what_runs() {
        let background = Background::new();
        assert!(!background.is_closed());
        let (tell, mut heard) = tokio::sync::mpsc::unbounded_channel();
        let early = tell.clone();
        assert!(background.spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let _ = early.send("ran to its end");
        }));

        background
            .close(Instant::now() + Duration::from_secs(5))
            .await;
        assert_eq!(heard.try_recv(), Ok("ran to its end"), "close awaited it");

        assert!(background.is_closed());
        assert!(!background.spawn(async move {
            let _ = tell.send("spawned after the close");
        }));
        tokio::task::yield_now().await;
        assert!(heard.try_recv().is_err(), "nothing runs after the close");
    }

    #[tokio::test]
    async fn what_outlives_the_grace_is_aborted() {
        let background = Background::new();
        let (tell, mut heard) = tokio::sync::mpsc::unbounded_channel::<()>();
        assert!(background.spawn(async move {
            tokio::time::sleep(Duration::from_secs(3600)).await;
            let _ = tell.send(());
        }));
        background
            .close(Instant::now() + Duration::from_millis(20))
            .await;
        assert!(
            heard.recv().await.is_none(),
            "the aborted task dropped its sender without sending"
        );
    }
}

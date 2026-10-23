//! The one cleanup runner the live suites' `Drop` guards share (#1913).
//!
//! A live test that writes to a real product undoes the write from a guard's
//! `Drop`, so a panicking test cleans up the same way a passing one does. Each
//! guard keeps its own state and its own async cleanup; what they all need
//! from `Drop` is the same and easy to get wrong, so it lives here once:
//!
//! 1. **A thread with a current-thread runtime of its own.** `Drop` cannot
//!    await, and it runs on the test's tokio worker, so it cannot block on that
//!    runtime either. The cleanup closure is called *inside* the new runtime,
//!    so a `reqwest::Client` it builds is registered with that runtime's
//!    reactor: a client built on the test's runtime and driven from this one
//!    hangs rather than failing (measured in the Gitea suite, #143).
//! 2. **A bound on the whole sequence**, [`DEFAULT_BUDGET`] unless the guard
//!    names its own. Each request is bounded by its client's timeout, but a
//!    walk of slow-but-answering requests is not, and only this bound can say
//!    what was abandoned (#162). The wait is a channel rather than a `join`
//!    because `JoinHandle` has no timed wait; a thread still running at the
//!    bound is left running, since a blocked socket read cannot be cancelled
//!    from outside, and a leaked thread in a failing test process costs
//!    nothing.
//! 3. **A report that cannot abort the process.** A cleanup that did not undo
//!    everything panics, so the test is red. If the test is already unwinding,
//!    a second panic would abort the whole binary and replace a legible
//!    failure with a crash, so the report goes to stderr instead.
//!
//! Waiting for the thread also keeps the cleanup ordered before the next test
//! starts, which the next guard's leftover clearing counts on.

use std::future::Future;
use std::sync::mpsc::RecvTimeoutError;
use std::time::Duration;

/// How long [`Cleanup::run`] waits for a cleanup that names no budget of its
/// own before it reports the cleanup abandoned.
///
/// **Five minutes, and generous on purpose.** Its job is to turn "never" into a
/// sentence, not to judge speed. The slowest healthy cleanups wait on a
/// product's search index catching up after a delete (Jira's and Confluence's
/// index budgets are 60 to 120 s per item), and a bound below that would
/// report a healthy run as a failed one. A guard whose sequence is known to be
/// short names a tighter bound with [`Cleanup::within`], as Gitea's `Litter`
/// does.
pub const DEFAULT_BUDGET: Duration = Duration::from_secs(300);

/// One guard's cleanup, as its `Drop` hands it over: what failing would mean,
/// what may be left standing, and how long to wait.
///
/// ```text
/// impl Drop for SubTask {
///     fn drop(&mut self) {
///         let Some(key) = self.key.take() else { return };
///         let (url, filed) = (self.url.clone(), key.clone());
///         Cleanup::new(
///             "the live suite did not delete the sub-task it filed",
///             format!("{key} may still exist; the next run's leftover clearing deletes it"),
///         )
///         .run(move || async move { delete_issue(&client(), &url, &filed).await });
///     }
/// }
/// ```
pub struct Cleanup {
    undone: String,
    left: String,
    budget: Duration,
}

impl Cleanup {
    /// `undone` opens every report: what the suite failed to undo and what
    /// that means for the server. `left` names what may still be standing when
    /// the cleanup could not say so itself, because it panicked or ran out of
    /// budget; it reads after "so", as in "so PAY-9 may still exist".
    pub fn new(undone: impl Into<String>, left: impl Into<String>) -> Cleanup {
        Cleanup {
            undone: undone.into(),
            left: left.into(),
            budget: DEFAULT_BUDGET,
        }
    }

    /// Wait at most `budget` instead of [`DEFAULT_BUDGET`].
    pub fn within(mut self, budget: Duration) -> Cleanup {
        self.budget = budget;
        self
    }

    /// Run `cleanup` to its end on a thread and runtime of its own, and fail
    /// the test if it did not undo everything.
    ///
    /// `cleanup` answers `Err` with what is still standing, after checking:
    /// a cleanup that sends its deletes and does not read the result back
    /// cannot tell a server that stopped answering from one that obeyed. The
    /// report is a panic, or a line on stderr when the test is already
    /// panicking.
    pub fn run<F, Fut>(self, cleanup: F)
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), String>>,
    {
        let failure = match self.wait(cleanup) {
            Ok(()) => return,
            Err(failure) => failure,
        };
        let report = format!("{}: {failure}", self.undone);
        if std::thread::panicking() {
            eprintln!("live suite cleanup: {report}");
        } else {
            panic!("{report}");
        }
    }

    fn wait<F, Fut>(&self, cleanup: F) -> Result<(), String>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), String>>,
    {
        let (done, waiting) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let outcome = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("a runtime for the cleanup")
                .block_on(async move { cleanup().await });
            // The receiver is gone only if the waiter already gave up.
            let _ = done.send(outcome);
        });
        let budget = self.budget;
        let left = &self.left;
        match waiting.recv_timeout(budget) {
            Ok(outcome) => outcome,
            // The sender is dropped unsent only by an unwind: a request that
            // outlived its client's timeout, say, which names itself on stderr.
            Err(RecvTimeoutError::Disconnected) => Err(format!(
                "the cleanup thread panicked (its own message is on stderr), so {left}"
            )),
            Err(RecvTimeoutError::Timeout) => Err(format!(
                "the cleanup did not finish within {budget:?} and was abandoned, so {left}. \
                 Check the server is healthy, then re-run; if a healthy server really needs \
                 longer, raise the guard's budget"
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    const UNDONE: &str = "the live suite did not delete the issue it filed";
    const LEFT: &str = "PAY-9 may still exist";

    fn panic_message(panicked: &(dyn std::any::Any + Send)) -> String {
        panicked
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| panicked.downcast_ref::<&str>().map(|s| (*s).to_owned()))
            .unwrap_or_default()
    }

    /// The panic `run` ended with, for a cleanup that must not pass quietly.
    fn report_of(cleanup: Cleanup, run: impl FnOnce(Cleanup) + std::panic::UnwindSafe) -> String {
        let panicked = std::panic::catch_unwind(move || run(cleanup))
            .expect_err("a cleanup that did not undo everything must fail the test");
        panic_message(panicked.as_ref())
    }

    #[test]
    fn a_cleanup_that_fails_fails_the_test_with_what_it_could_not_undo() {
        let report = report_of(Cleanup::new(UNDONE, LEFT), |c| {
            c.run(|| async { Err("DELETE issue PAY-9 -> 500".to_owned()) })
        });
        assert_eq!(report, format!("{UNDONE}: DELETE issue PAY-9 -> 500"));
    }

    /// `run` returns only once the cleanup has: the next test's guard clears
    /// leftovers by prefix, and would take a cleanup still in flight for one.
    #[test]
    fn a_cleanup_that_succeeds_has_finished_when_run_returns() {
        let finished = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&finished);
        Cleanup::new(UNDONE, LEFT).run(move || async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            flag.store(true, Ordering::SeqCst);
            Ok(())
        });
        assert!(
            finished.load(Ordering::SeqCst),
            "run returned before its cleanup finished"
        );
    }

    /// The case every guard is for: `Drop` runs on the test's own runtime,
    /// which cannot be blocked on, and the closure itself (not only its
    /// future) runs inside the cleanup's runtime, where a client it builds
    /// belongs.
    #[tokio::test]
    async fn a_guard_dropped_inside_a_tokio_test_runs_its_cleanup_on_a_runtime_of_its_own() {
        let finished = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&finished);
        Cleanup::new(UNDONE, LEFT).run(move || {
            // Panics outside a runtime, which `run` reports as a failure.
            let _inside = tokio::runtime::Handle::current();
            async move {
                tokio::time::sleep(Duration::from_millis(10)).await;
                flag.store(true, Ordering::SeqCst);
                Ok(())
            }
        });
        assert!(finished.load(Ordering::SeqCst));
    }

    #[test]
    fn a_cleanup_that_panics_reports_what_may_be_left() {
        let report = report_of(Cleanup::new(UNDONE, LEFT), |c| {
            c.run(|| async { panic!("GET issue PAY-9: operation timed out") })
        });
        assert_eq!(
            report,
            format!(
                "{UNDONE}: the cleanup thread panicked (its own message is on stderr), so {LEFT}"
            )
        );
    }

    /// A server that accepted the connection and went quiet must end the run
    /// with a sentence, not stall it (#162). The report names the budget, so a
    /// reader knows what to raise, and what was abandoned, since nothing else
    /// will.
    #[test]
    fn a_cleanup_that_never_finishes_is_abandoned_at_its_budget() {
        let started = std::time::Instant::now();
        let report = report_of(
            Cleanup::new(UNDONE, LEFT).within(Duration::from_millis(100)),
            |c| c.run(std::future::pending),
        );
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the runner took {:?} to give up on a cleanup that never ends",
            started.elapsed()
        );
        assert!(
            report.starts_with(&format!(
                "{UNDONE}: the cleanup did not finish within 100ms and was abandoned, so {LEFT}."
            )),
            "{report}"
        );
    }

    /// A guard dropped while its test is already panicking reports on stderr:
    /// a second panic would abort the binary and hide the test's own failure.
    /// Under the mutant that always panics this test does not fail, the whole
    /// test binary aborts.
    #[test]
    fn a_guard_dropped_while_its_test_panics_keeps_the_tests_own_failure() {
        struct Guard;
        impl Drop for Guard {
            fn drop(&mut self) {
                Cleanup::new(UNDONE, LEFT).run(|| async { Err("still there".to_owned()) });
            }
        }
        let panicked = std::panic::catch_unwind(|| {
            let _guard = Guard;
            panic!("the assertion the test failed on");
        })
        .expect_err("the test body panicked");
        assert_eq!(
            panic_message(panicked.as_ref()),
            "the assertion the test failed on"
        );
    }
}

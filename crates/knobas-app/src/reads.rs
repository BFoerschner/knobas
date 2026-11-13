//! Reads a page can call off (the compare pass of PR #2479, 2026-11-13).
//!
//! A command the page may no longer want the answer to -- a branch's file
//! list when the reader picks another branch, a file's lines when its card
//! scrolls away before they came -- takes a `read` name the page made up,
//! and runs as a task of its own under it. `cancel_read` with that name
//! aborts the task: its futures are dropped where they stand, so a request
//! to the source is abandoned and a `git` it started is killed
//! (`kill_on_drop`, `checkout::working_tree::git_at`). The page drops a late
//! answer on its own as well; this is the backend's half, so a pick that is
//! superseded stops costing the machine and the forge.
//!
//! A read with no name runs in place, as every command did before.

use std::collections::HashMap;
use std::future::Future;
use std::sync::{Mutex, OnceLock};

use crate::IpcError;

fn running() -> &'static Mutex<HashMap<String, tokio::task::AbortHandle>> {
    static RUNNING: OnceLock<Mutex<HashMap<String, tokio::task::AbortHandle>>> = OnceLock::new();
    RUNNING.get_or_init(|| Mutex::new(HashMap::new()))
}

/// `work` under the name `read`, which [`cancel`] can call it off by; in
/// place where `read` is `None`.
///
/// # Errors
///
/// `work`'s own, and [`Conflict`](crate::IpcErrorCode::Conflict) for a read
/// called off before it answered.
pub async fn run<T, F>(read: Option<String>, work: F) -> Result<T, IpcError>
where
    T: Send + 'static,
    F: Future<Output = Result<T, IpcError>> + Send + 'static,
{
    let Some(name) = read else {
        return work.await;
    };
    let task = tokio::spawn(work);
    running()
        .lock()
        .expect("the running reads")
        .insert(name.clone(), task.abort_handle());
    let answer = task.await;
    running().lock().expect("the running reads").remove(&name);
    match answer {
        Ok(answer) => answer,
        Err(error) if error.is_cancelled() => Err(IpcError::new(
            crate::IpcErrorCode::Conflict,
            format!("the read {name} was called off"),
        )),
        Err(error) => Err(IpcError::internal(format!(
            "the read {name} failed: {error}"
        ))),
    }
}

/// Call off the read named `read`, if it is still running: whether it was.
pub fn cancel(read: &str) -> bool {
    let handle = running().lock().expect("the running reads").remove(read);
    handle.is_some_and(|handle| {
        handle.abort();
        true
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_called_off_read_stops_and_says_so() {
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let read = tokio::spawn(run(Some("r1".to_owned()), async move {
            // Dropped when the read is called off, which closes `rx`'s end.
            let _held = tx;
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            Ok::<_, IpcError>(1)
        }));
        while !running().lock().unwrap().contains_key("r1") {
            tokio::task::yield_now().await;
        }
        assert!(cancel("r1"));
        let answer = read.await.unwrap();
        assert_eq!(answer.unwrap_err().code, crate::IpcErrorCode::Conflict);
        assert!(rx.await.is_err(), "the work was dropped, not left running");
        assert!(!cancel("r1"), "nothing left to call off");
    }

    #[tokio::test]
    async fn a_read_with_no_name_runs_in_place() {
        assert_eq!(run(None, async { Ok::<_, IpcError>(7) }).await.unwrap(), 7);
        assert_eq!(
            run(Some("r2".to_owned()), async { Ok::<_, IpcError>(8) })
                .await
                .unwrap(),
            8
        );
        assert!(!cancel("r2"));
    }
}

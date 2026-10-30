//! The request counter (#2239): how many requests one piece of work put on
//! the wire, for the tests that hold a read to a round-trip budget (spec
//! #2234, *Step 1*). Behind `test-util`, so no shipped build carries it.
//!
//! The count is the work's own: a task-local the attempt loop of
//! [`HttpClient::send`](crate::HttpClient::send) adds to, one per attempt
//! that is sent, so a retry counts as the second request it is. Two tests
//! running at once against the same server do not see each other's requests.
//!
//! **What it cannot see**: a request sent from a task the work spawned
//! (`tokio::spawn`, a `JoinSet`), which does not inherit the task-local. A
//! budget asserted through it is therefore an upper bound only for work that
//! sends from its own task; pair a "no requests" assertion with a read that
//! counts more than none, so the counter is shown to see that path.

use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

tokio::task_local! {
    static SENT: Arc<AtomicU64>;
}

/// Run `work` and answer what it answered beside the number of requests it
/// sent through any [`HttpClient`](crate::HttpClient) from its own task.
pub async fn counted<F: Future>(work: F) -> (F::Output, u64) {
    let sent = Arc::new(AtomicU64::new(0));
    let answer = SENT.scope(Arc::clone(&sent), work).await;
    (answer, sent.load(Ordering::SeqCst))
}

/// One request is going on the wire; counted where [`counted`] is running.
pub(crate) fn sent() {
    let _ = SENT.try_with(|sent| sent.fetch_add(1, Ordering::SeqCst));
}

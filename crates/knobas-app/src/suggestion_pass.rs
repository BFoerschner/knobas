//! The suggestion detection pass: one at a time, on a connection of its own
//! (#745).
//!
//! Until #745 every finished sync made the tray call `detect_suggestions`,
//! which ran [`knobas_core::suggest::detect`] on the **UI pool**, with nothing
//! stopping the next call while earlier ones ran and no statement timeout.
//! With five sources syncing, the UI pool's five connections were all busy
//! detecting for minutes at a time, and `list_sources` and `search` failed
//! with a pool timeout: Björn's 2026-10-13 hang on Windows ("searches didn't
//! find anything and it said there were no sources available anymore").
//!
//! Three things changed, and this module is two of them:
//!
//! * **One pass at a time.** At most one pass runs, and at most one more is
//!   owed behind it: every request that arrives while a pass runs is answered
//!   by the *next* pass, which starts when the running one ends. N requests
//!   during a pass cost two passes, not N. A request is answered by a pass
//!   that started after it, so what the caller reads next includes what it
//!   asked about.
//! * **Off the UI pool.** A pass opens a connection of its own, the way a
//!   sync run does (§10.6(c)), and gives it a `statement_timeout`
//!   ([`PASS_STATEMENT_TIMEOUT`]). Not the scheduler's bookkeeping pool: its
//!   four connections are what the scheduler records runs with, and a pass
//!   holding one for its length is the same starvation one pool over. A
//!   statement that runs past the limit is cancelled by the server, the pass
//!   logs it and gives up, and the next pass carries on from the batches the
//!   last one finished (`similar_text` settles them one by one).
//!
//! The third is in `knobas_core::suggest`: `similar_text` compares only what
//! changed since it last compared it, so a pass with nothing new costs a read.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use knobas_sync::scheduler::RunConnections;
use tokio::sync::watch;

use crate::IpcError;

/// How long one statement of a pass may run before the server cancels it.
///
/// Per statement, and a pass is several: six whole-mirror rules that take
/// milliseconds, then `similar_text` in batches of
/// [`knobas_core::suggest::SIMILAR_TEXT_BATCH`]. The longest of those measured
/// for #745 is a batch against 5,000 settled items of the probe's 108-word
/// corpus (every title matching nearly every document): ~15 s on a machine
/// at load 25. Two minutes keeps that an order of magnitude inside the limit
/// and still bounds a pathological corpus.
pub const PASS_STATEMENT_TIMEOUT: Duration = Duration::from_secs(120);

/// The detection pass, shared by every caller in the process.
///
/// Cheap to clone: a handle onto one pass and one queue.
#[derive(Clone)]
pub struct SuggestionPass {
    inner: Arc<Inner>,
}

struct Inner {
    connections: Arc<dyn RunConnections>,
    statement_timeout: Duration,
    queue: Mutex<Queue>,
    /// The last pass that finished, and which requests it answered.
    finished: watch::Sender<Finished>,
}

/// Requests are numbered; a pass answers every request numbered up to the
/// number current when it started.
#[derive(Default)]
struct Queue {
    requested: u64,
    answered: u64,
    running: bool,
}

#[derive(Clone)]
struct Finished {
    answers_through: u64,
    outcome: Result<u64, IpcError>,
}

/// Connections opened with a pool's options and owned by nobody but the pass.
struct Beside(sqlx::PgPool);

#[async_trait::async_trait]
impl RunConnections for Beside {
    async fn open(&self) -> Result<sqlx::PgConnection, sqlx::Error> {
        use sqlx::Connection as _;
        sqlx::PgConnection::connect_with(&self.0.connect_options()).await
    }
}

impl SuggestionPass {
    /// A pass opening its connections from `connections`, its statements
    /// cancelled after `statement_timeout`.
    #[must_use]
    pub fn new(connections: Arc<dyn RunConnections>, statement_timeout: Duration) -> Self {
        let (finished, _) = watch::channel(Finished {
            answers_through: 0,
            outcome: Ok(0),
        });
        Self {
            inner: Arc::new(Inner {
                connections,
                statement_timeout,
                queue: Mutex::new(Queue::default()),
                finished,
            }),
        }
    }

    /// A pass whose connections are opened with `pool`'s options but are not
    /// the pool's: the pass never waits for, or holds, one of its connections.
    #[must_use]
    pub fn beside(pool: &sqlx::PgPool) -> Self {
        Self::new(Arc::new(Beside(pool.clone())), PASS_STATEMENT_TIMEOUT)
    }

    /// Ask for a pass, and wait for the one that answers.
    ///
    /// Starts a pass if none is running; otherwise joins the one owed behind
    /// the running pass. Returns how many proposals the answering pass wrote.
    /// Dropping the future does not stop the pass.
    ///
    /// # Errors
    ///
    /// [`Internal`](crate::IpcErrorCode::Internal) if the answering pass
    /// failed, a statement timeout included.
    pub async fn run(&self) -> Result<u64, IpcError> {
        let mut finished = self.inner.finished.subscribe();
        let ticket = {
            let mut queue = self.lock();
            queue.requested += 1;
            if !queue.running {
                queue.running = true;
                tokio::spawn(Arc::clone(&self.inner).drive());
            }
            queue.requested
        };
        let answer = finished
            .wait_for(|done| done.answers_through >= ticket)
            .await
            .map_err(|_| IpcError::internal("the suggestion pass stopped"))?;
        answer.outcome.clone()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Queue> {
        self.inner
            .queue
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

impl Inner {
    /// Run passes until every request is answered, then stop.
    async fn drive(self: Arc<Self>) {
        loop {
            let answers_through = {
                let mut queue = self.queue.lock().unwrap_or_else(PoisonError::into_inner);
                if queue.answered >= queue.requested {
                    queue.running = false;
                    return;
                }
                queue.requested
            };
            let outcome = self.pass().await;
            self.queue
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .answered = answers_through;
            self.finished.send_replace(Finished {
                answers_through,
                outcome,
            });
        }
    }

    async fn pass(&self) -> Result<u64, IpcError> {
        use sqlx::Connection as _;
        let started = std::time::Instant::now();
        let mut conn = self.connections.open().await.map_err(|error| {
            tracing::warn!(%error, "the suggestion pass could not open its connection");
            IpcError::internal(format!("the suggestion pass could not connect: {error}"))
        })?;
        sqlx::query("select set_config('statement_timeout', $1, false)")
            .bind(format!("{}ms", self.statement_timeout.as_millis()))
            .execute(&mut conn)
            .await?;
        let outcome = knobas_core::suggest::detect_on(&mut conn).await;
        // Closed rather than dropped, so the server sees a goodbye and not a
        // reset; a failure to say it changes nothing.
        let _ = conn.close().await;
        match outcome {
            Ok(written) => {
                tracing::debug!(
                    written,
                    ms = started.elapsed().as_millis(),
                    "suggestion pass finished"
                );
                Ok(written)
            }
            Err(knobas_core::CoreError::Db(error)) if is_timeout(&error) => {
                tracing::warn!(
                    limit = ?self.statement_timeout,
                    ms = started.elapsed().as_millis(),
                    "a suggestion pass statement ran past its limit and was cancelled; \
                     the next pass goes on from the batches this one finished"
                );
                Err(IpcError::internal(format!(
                    "the suggestion pass ran past its {:?} limit and stopped",
                    self.statement_timeout
                )))
            }
            Err(error) => {
                tracing::warn!(%error, "the suggestion pass failed");
                Err(error.into())
            }
        }
    }
}

/// `query_canceled`: what the server answers when `statement_timeout` fires.
fn is_timeout(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(sqlx::error::DatabaseError::code)
        .is_some_and(|code| code == "57014")
}

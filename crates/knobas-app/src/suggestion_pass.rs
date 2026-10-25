//! The suggestion detection pass: one at a time, on a connection of its own
//! (#745).
//!
//! **Who asks for one.** The sync scheduler, whenever a run ends OK
//! ([`SuggestOnSync`], #1485), without waiting for it, through the process's
//! one [`SuggestionPass`], built once at start-up and handed to the
//! scheduler: a second one would run a second pass beside the first, the
//! thing this module exists to stop. Until #881 the room's tray was the
//! caller, on every finished sync; with the tray gone nothing asked, and no
//! suggestion was made until #1485 gave the request to the scheduler. The
//! `detect_suggestions` command, the manual *run a pass now* that waited on
//! [`run`](SuggestionPass::run), had no caller after that and retired (#1611);
//! `run` stays as the waiting half of the queue, which the tests drive.
//!
//! Until #745 every finished sync made the tray call `detect_suggestions`,
//! which ran [`knobas_core::suggest::detect`] on the **UI pool**, with nothing
//! stopping the next call while earlier ones ran and no statement timeout.
//! With five sources syncing, the UI pool's five connections were all busy
//! detecting for minutes at a time, and `list_sources` and `search` failed
//! with a pool timeout: Björn's 2026-10-13 hang on Windows ("searches didn't
//! find anything and it said there were no sources available anymore").
//!
//! Five things changed; this module is two of them:
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
//! The others: `similar_text` (`knobas_core::suggest`) compares only what
//! changed since it last compared it, so a pass with nothing new costs a read;
//! the app's pool carries a statement timeout of its own
//! (`knobas_db::embedded::UI_STATEMENT_TIMEOUT`); and a pass is asked for
//! only when a sync run finishes, never on a room switch (the scheduler asks
//! now, the tray did then).

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use knobas_sync::scheduler::{RunConnections, SyncEvents};
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
    /// The similar_text batch size, kept between passes (#755).
    pacing: knobas_core::suggest::Pacing,
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
    /// How many requests found no pass running and started one.
    started: u64,
}

#[derive(Clone)]
struct Finished {
    answers_through: u64,
    outcome: Result<u64, IpcError>,
    /// How many passes have finished, failed ones included; one that
    /// stopped part-way ([`Stopped`]) is not counted.
    passes: u64,
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
            passes: 0,
        });
        Self {
            inner: Arc::new(Inner {
                connections,
                statement_timeout,
                pacing: knobas_core::suggest::Pacing::default(),
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
        let ticket = self.enqueue();
        let answer = finished
            .wait_for(|done| done.answers_through >= ticket)
            .await
            .map_err(|_| IpcError::internal("the suggestion pass stopped"))?;
        answer.outcome.clone()
    }

    /// Ask for a pass, and do not wait for it (#1485): what the sync
    /// scheduler does when a run ends OK ([`SuggestOnSync`]).
    ///
    /// The same queue as [`run`](Self::run): starts a pass if none is
    /// running, otherwise joins the one owed behind it. What the pass
    /// proposes, or why it failed, goes to the log and nowhere else.
    pub fn request(&self) {
        self.enqueue();
    }

    /// How many passes have finished since this pass was made, failed ones
    /// included. One that stopped part-way (a panic) is not counted.
    #[must_use]
    pub fn passes(&self) -> u64 {
        self.inner.finished.borrow().passes
    }

    /// How many requests have been made since this pass was made, by
    /// [`run`](Self::run) and [`request`](Self::request) alike: numbered as
    /// they are made, so a test reads here that a request has arrived before
    /// it looks at what the request did (#1927).
    #[must_use]
    pub fn requested(&self) -> u64 {
        self.lock().requested
    }

    /// How many requests found no pass running and started one, since this
    /// pass was made. A request made while a pass runs starts none: it joins
    /// the pass owed behind the running one. Counted as the request is made,
    /// under the queue's lock, so a test reads here that a request started
    /// nothing, where a count of opened connections cannot tell a pass not
    /// started from one not yet arrived (#1927).
    #[must_use]
    pub fn started(&self) -> u64 {
        self.lock().started
    }

    /// Wait until every request made before this call is answered.
    pub async fn idle(&self) {
        let ticket = self.lock().requested;
        let mut finished = self.inner.finished.subscribe();
        // The sender lives in `inner`, which `self` holds: it cannot close.
        let _ = finished
            .wait_for(|done| done.answers_through >= ticket)
            .await;
    }

    /// Number a request, start a pass if none is running, and answer the
    /// request's number: the pass that answers it is the first to finish
    /// with `answers_through` at or past it.
    fn enqueue(&self) -> u64 {
        let mut queue = self.lock();
        queue.requested += 1;
        if !queue.running {
            queue.running = true;
            queue.started += 1;
            tokio::spawn(Arc::clone(&self.inner).drive());
        }
        queue.requested
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Queue> {
        self.inner
            .queue
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

/// The scheduler's events, and after every run that ends OK a suggestion
/// pass requested (#1485), the way the scheduler requests an `ANALYZE` pass
/// (`knobas_sync::analyze`). Wraps the events `sources::bring_up` hands the
/// scheduler, as `ArrivalLinks` does; every event is passed on unchanged.
///
/// Keyed on the `sync:state` that ends a run, which `settle` emits once the
/// run's transaction has committed, and on its own outcome: a failed or
/// cancelled run changed nothing a rule could match. The emit that *starts*
/// a run carries the previous run's outcome and requests nothing.
///
/// The request does not wait, so the run's ending never waits on a pass.
pub struct SuggestOnSync<E> {
    inner: E,
    pass: SuggestionPass,
}

impl<E> SuggestOnSync<E> {
    /// `inner`'s events, and passes requested on `pass`: the process's one
    /// pass, the one bring-up builds.
    pub fn new(inner: E, pass: SuggestionPass) -> SuggestOnSync<E> {
        SuggestOnSync { inner, pass }
    }
}

impl<E: SyncEvents> SyncEvents for SuggestOnSync<E> {
    fn sync_state(&self, status: knobas_sync::SourceSyncStatus) {
        if !status.running && status.last_outcome == Some(knobas_sync::SyncOutcome::Ok) {
            self.pass.request();
        }
        self.inner.sync_state(status);
    }
    fn source_health(&self, health: knobas_sync::CredentialHealth) {
        self.inner.source_health(health);
    }
    fn activity_new(&self, row: knobas_core::activity::ActivityRow) {
        self.inner.activity_new(row);
    }
    fn write_landed(&self, write: &knobas_core::write_queue::QueuedWrite) {
        self.inner.write_landed(write);
    }
}

impl Inner {
    /// Run passes until every request is answered, then stop.
    async fn drive(self: Arc<Self>) {
        let mut stopped = Stopped {
            inner: Arc::clone(&self),
            armed: true,
        };
        loop {
            let answers_through = {
                let mut queue = self.queue.lock().unwrap_or_else(PoisonError::into_inner);
                if queue.answered >= queue.requested {
                    queue.running = false;
                    stopped.armed = false;
                    return;
                }
                queue.requested
            };
            let outcome = self.pass().await;
            self.queue
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .answered = answers_through;
            self.finished.send_modify(|finished| {
                *finished = Finished {
                    answers_through,
                    outcome,
                    passes: finished.passes + 1,
                };
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
        let outcome = knobas_core::suggest::detect_paced(&mut conn, &self.pacing).await;
        // Closed rather than dropped, so the server sees a goodbye and not a
        // reset; a failure to say it changes nothing.
        let _ = conn.close().await;
        match outcome {
            Ok(written) => {
                // `info`, not `debug` (#1485): at most one pass per sync run
                // that ends OK, as loud as that run's own `sync started`, and
                // the line that says passes happen at all.
                tracing::info!(
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

/// What clears the queue when [`Inner::drive`] ends without saying so: a pass
/// that panicked, or a task dropped at runtime shutdown. Without it `running`
/// stayed `true`, no later request started a pass, and every caller waited
/// for an answer nobody would send.
///
/// Every request owed so far is answered with an error, and the next request
/// starts a pass of its own.
struct Stopped {
    inner: Arc<Inner>,
    /// `false` once `drive` returns by its own path, having cleared the queue.
    armed: bool,
}

impl Drop for Stopped {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let answers_through = {
            let mut queue = self
                .inner
                .queue
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            queue.running = false;
            queue.answered = queue.requested;
            queue.requested
        };
        tracing::error!("the suggestion pass stopped part-way; the next request starts a new one");
        self.inner.finished.send_modify(|finished| {
            *finished = Finished {
                answers_through,
                outcome: Err(IpcError::internal(
                    "the suggestion pass stopped part-way; the next sync starts a new one",
                )),
                passes: finished.passes,
            };
        });
    }
}

/// `query_canceled`: what the server answers when `statement_timeout` fires.
fn is_timeout(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(sqlx::error::DatabaseError::code)
        .is_some_and(|code| code == "57014")
}

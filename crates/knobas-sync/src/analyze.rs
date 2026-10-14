//! knobas analyzes its own mirror (#778).
//!
//! A fresh install's tables have no planner statistics until autovacuum
//! reaches them, about a minute after the first sync or longer, and in that
//! window the planner reads `knobas.entity`'s `deleted_at is null` as 0.5 % of
//! the table and plans every read of `sync.live_item` from there (#767, #768,
//! #770): the *All* room's builds tile took 6.8 s on the probe's 22,000 items,
//! against 23 ms analyzed. So after a sync commits, the scheduler asks for a
//! pass ([`Analyzer::request`]), and a pass runs `ANALYZE` on every mirror
//! table that has grown or shrunk by more than [`GROWTH`] since it was last
//! analyzed, or was never analyzed at all.
//!
//! **Which tables.** Every table in the `sync` schema, and `knobas.entity`:
//! the ruling on #778 names `sync.*`, and `knobas.entity` is the other table
//! a sync writes, one row per item -- the one whose missing statistics are the
//! misestimate above. Nothing else is a sync's to analyze.
//!
//! **What "changed" is read from.** Nothing is stored: `pg_class.reltuples` is
//! the row count the last `ANALYZE` (or `VACUUM`) saw, `-1` before either, and
//! `pg_stat_user_tables.n_live_tup` is the count now. A vacuum moves
//! `reltuples` too, so "since it was last analyzed" is read as "since the
//! planner last learnt its size", which is the question that matters to a
//! plan. `n_live_tup` comes from the cumulative statistics, which a backend
//! flushes lazily; the sync's transaction forces its flush before it commits
//! (`run_inner`), so a pass that starts after the commit sees what it wrote.
//!
//! **Off the UI pool, one at a time.** A pass opens a connection of its own
//! through [`RunConnections`], as a sync run and the suggestion pass do
//! (#745), so it never holds one of the UI pool's five. At most one pass runs;
//! a request that arrives while one runs is owed one more pass after it, and
//! any number of such requests are that one pass.

use std::sync::{Arc, Mutex, PoisonError};

use tokio::sync::watch;

use crate::scheduler::RunConnections;

/// How far a table's row count may move before knobas analyzes it again: a
/// fifth of the count the planner last learnt.
pub const GROWTH: f64 = 0.2;

/// The mirror tables a pass would analyze now: never analyzed, or more than
/// [`GROWTH`] away from the count the last analyze saw. Quoted, ready to name
/// in a statement (`format('%I.%I')`).
const GROWN: &str = "
select format('%I.%I', s.schemaname, s.relname)
  from pg_stat_user_tables s
  join pg_class c on c.oid = s.relid
 where (s.schemaname = 'sync' or (s.schemaname = 'knobas' and s.relname = 'entity'))
   and (c.reltuples < 0
        or (s.last_analyze is null and s.last_autoanalyze is null)
        or abs(s.n_live_tup - c.reltuples) > $1 * c.reltuples)
 order by 1";

/// What a pass found to analyze, read on `conn`. See [`GROWN`].
///
/// # Errors
///
/// [`sqlx::Error`] if the catalog read fails.
pub async fn grown(conn: &mut sqlx::PgConnection) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar(GROWN).bind(GROWTH).fetch_all(conn).await
}

/// The analyze pass, shared by every caller in the process. Cheap to clone: a
/// handle onto one queue.
#[derive(Clone)]
pub struct Analyzer {
    inner: Arc<Inner>,
}

struct Inner {
    connections: Arc<dyn RunConnections>,
    queue: Mutex<Queue>,
    /// How many passes have finished, and whether one is running or owed.
    progress: watch::Sender<Progress>,
}

#[derive(Default)]
struct Queue {
    running: bool,
    owed: bool,
}

#[derive(Clone, Copy, Default)]
struct Progress {
    passes: u64,
    busy: bool,
}

impl Analyzer {
    /// An analyzer whose passes open their connections from `connections`.
    #[must_use]
    pub fn new(connections: Arc<dyn RunConnections>) -> Self {
        Self {
            inner: Arc::new(Inner {
                connections,
                queue: Mutex::new(Queue::default()),
                progress: watch::Sender::new(Progress::default()),
            }),
        }
    }

    /// Ask for a pass, and do not wait for it.
    ///
    /// Starts one if none is running; otherwise the running pass owes one
    /// more after it, which every request until then shares.
    pub fn request(&self) {
        let mut queue = self.inner.lock();
        if queue.running {
            queue.owed = true;
            return;
        }
        queue.running = true;
        self.inner.progress.send_modify(|p| p.busy = true);
        tokio::spawn(Arc::clone(&self.inner).drive());
    }

    /// How many passes have finished since this analyzer was made.
    #[must_use]
    pub fn passes(&self) -> u64 {
        self.inner.progress.borrow().passes
    }

    /// Wait until no pass is running and none is owed.
    pub async fn idle(&self) {
        let mut progress = self.inner.progress.subscribe();
        // The sender lives in `inner`, which `self` holds: it cannot close.
        let _ = progress.wait_for(|p| !p.busy).await;
    }
}

impl Inner {
    fn lock(&self) -> std::sync::MutexGuard<'_, Queue> {
        self.queue.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Run passes until none is owed, then stop.
    async fn drive(self: Arc<Self>) {
        loop {
            match self.pass().await {
                Ok(tables) if !tables.is_empty() => {
                    tracing::info!(?tables, "analyzed the mirror tables a sync grew");
                }
                Ok(_) => {}
                Err(error) => tracing::warn!(%error, "analyzing the mirror failed"),
            }
            // Under the queue's lock, so a request cannot slip in between
            // deciding to stop and saying so.
            let mut queue = self.lock();
            let again = std::mem::take(&mut queue.owed);
            queue.running = again;
            self.progress.send_modify(|p| {
                p.passes += 1;
                p.busy = again;
            });
            if !again {
                return;
            }
        }
    }

    async fn pass(&self) -> Result<Vec<String>, sqlx::Error> {
        use sqlx::Connection as _;
        let mut conn = self.connections.open().await?;
        let tables = grown(&mut conn).await?;
        for table in &tables {
            // A name out of the catalog, quoted by `format('%I.%I')`.
            sqlx::query(sqlx::AssertSqlSafe(format!("analyze {table}")))
                .execute(&mut conn)
                .await?;
        }
        if let Err(error) = conn.close().await {
            tracing::debug!(%error, "closing the analyze pass's connection failed");
        }
        Ok(tables)
    }
}

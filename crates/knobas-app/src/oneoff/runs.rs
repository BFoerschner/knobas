//! The runs of this session (sections 7, 18, 19): who runs, what each kept,
//! the four-at-once rule, the batches to the window, the memory cap and the
//! kill at quit.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex, OnceLock, PoisonError};
use std::time::Instant;

use super::history::{History, RunRow};
use super::kept::{Kept, Limits};
use super::run::{BATCH_BYTES, BATCH_LINES, Batch, Ended, Target};

/// The most runs at once.
pub const AT_ONCE: usize = 4;
/// The most runs that keep their lines in memory.
pub const KEPT_RUNS: usize = 50;
/// The most bytes the kept runs' lines weigh together.
pub const KEPT_BYTES: usize = 128 * 1024 * 1024;

/// Where a run's batches go: the window's channel, or a test's list.
pub trait Sink: Send + Sync {
    /// Sends `batch`; `false` once the other end is gone.
    fn send(&self, batch: &Batch) -> bool;
}

/// Who hears that a run started or ended: the `oneoff:moved` event.
pub trait Events: Send + Sync {
    fn moved(&self, row: &RunRow);
}

/// One run.
pub struct RunHandle {
    pub id: String,
    key: String,
    row: Mutex<RunRow>,
    kept: Mutex<Kept>,
    bytes: AtomicU64,
    findings: AtomicU32,
    stop: AtomicBool,
    pid: Mutex<Option<u32>>,
    sinks: Mutex<Vec<Arc<dyn Sink>>>,
    started: Instant,
    shown: Mutex<Instant>,
    /// The run's own folder, for `{out}` and `{outdir}`.
    pub scratch: PathBuf,
    result: Mutex<Option<super::result::RunResult>>,
    /// The generated files the run changed, once it ended (a run on a
    /// generated folder only).
    made_changed: Mutex<Option<Vec<String>>>,
    /// The job holding what the run started (Windows).
    #[cfg(windows)]
    job: Mutex<Option<super::win::Job>>,
    /// The result's bytes: the stdout, the file under `{out}`, or the kept
    /// `{outdir}`.
    held: Mutex<Held>,
    /// A try's throwaway folder (#2562), removed when the run ends.
    throwaway: Mutex<Option<PathBuf>>,
}

/// Where a run's result's bytes are.
#[derive(Default)]
enum Held {
    #[default]
    Nothing,
    Bytes(Vec<u8>),
    File(PathBuf),
    Made(crate::folder_render::keep::Made),
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl RunHandle {
    /// The run's row as it stands.
    #[must_use]
    pub fn row(&self) -> RunRow {
        let mut row = lock(&self.row).clone();
        row.findings = self.findings.load(Ordering::Relaxed);
        let kept = lock(&self.kept);
        row.lines = kept.total();
        row.kept = kept.kept() > 0 || kept.total() == 0;
        row
    }

    /// The job its program was put in (Windows).
    #[cfg(windows)]
    pub fn set_job(&self, job: Option<super::win::Job>) {
        *lock(&self.job) = job;
    }

    /// Kills what the run started: its job on Windows, then its tree or its
    /// process group.
    pub fn kill(&self, pid: Option<u32>) {
        #[cfg(windows)]
        if let Some(job) = lock(&self.job).as_ref() {
            job.kill();
        }
        super::run::kill_group(pid);
    }

    pub fn set_result(&self, result: Option<super::result::RunResult>) {
        *lock(&self.result) = result;
    }

    /// The generated files the run changed (ADR-0060, amended 2026-11-25).
    pub fn set_made_changed(&self, changed: Vec<String>) {
        *lock(&self.made_changed) = Some(changed);
    }

    pub fn keep_bytes(&self, bytes: Vec<u8>) {
        *lock(&self.held) = Held::Bytes(bytes);
    }

    pub fn keep_file(&self, path: Option<PathBuf>) {
        if let Some(path) = path {
            *lock(&self.held) = Held::File(path);
        }
    }

    pub fn keep_made(&self, made: Option<crate::folder_render::keep::Made>) {
        if let Some(made) = made {
            *lock(&self.held) = Held::Made(made);
        }
    }

    /// The bytes the result holds in memory.
    fn held_bytes(&self) -> usize {
        match &*lock(&self.held) {
            Held::Bytes(bytes) => bytes.len(),
            _ => 0,
        }
    }

    /// The result's bytes, or one kept file's of `{outdir}`.
    pub fn result_bytes(&self, path: Option<&str>) -> Result<Vec<u8>, String> {
        match (&*lock(&self.held), path) {
            (Held::Bytes(bytes), _) => Ok(bytes.clone()),
            (Held::File(file), _) => std::fs::read(file).map_err(|e| e.to_string()),
            (Held::Made(made), Some(path)) => {
                crate::folder_render::keep::read(made, path, super::result::RESULT_CAP)
                    .map_err(|e| format!("{e:?}"))
            }
            (Held::Made(_), None) => Err("name one of the files it wrote".to_owned()),
            (Held::Nothing, _) => Err("it made no result that is kept".to_owned()),
        }
    }

    /// The words it starts with and the folder it starts in.
    pub fn set_words(&self, argv: &[String], cwd: &str) {
        let mut row = lock(&self.row);
        row.argv = argv.to_vec();
        cwd.clone_into(&mut row.cwd);
    }

    /// Whether it still runs.
    #[must_use]
    pub fn running(&self) -> bool {
        lock(&self.row).ended.is_none()
    }

    pub fn with_kept<T>(&self, f: impl FnOnce(&mut Kept) -> T) -> T {
        f(&mut lock(&self.kept))
    }

    pub fn add_bytes(&self, n: usize) {
        self.bytes
            .fetch_add(u64::try_from(n).unwrap_or(u64::MAX), Ordering::Relaxed);
    }

    pub fn add_findings(&self, n: usize) {
        self.findings
            .fetch_add(u32::try_from(n).unwrap_or(u32::MAX), Ordering::Relaxed);
    }

    #[must_use]
    pub fn stop_asked(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    /// The program started as `pid`.
    pub fn started(&self, pid: u32) {
        *lock(&self.pid) = Some(pid);
    }

    fn batch(&self, kept: &mut Kept, ended: Option<Ended>) -> Batch {
        let (lines, changed) = kept.take(BATCH_LINES, BATCH_BYTES);
        Batch {
            run: self.id.clone(),
            lines,
            changed,
            total: kept.total(),
            dropped: kept.dropped(),
            tail_from: kept.tail_from(),
            bytes: self.bytes.load(Ordering::Relaxed),
            findings: self.findings.load(Ordering::Relaxed),
            elapsed_ms: u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX),
            result: if ended.is_some() {
                lock(&self.result).clone()
            } else {
                None
            },
            made_changed: if ended.is_some() {
                lock(&self.made_changed).clone()
            } else {
                None
            },
            ended,
        }
    }

    /// Sends what the window has not seen; with `ended`, everything, the last
    /// batch carrying it.
    pub fn flush(&self, _started: Instant, ended: Option<Ended>) {
        let mut batches = Vec::new();
        {
            let mut kept = lock(&self.kept);
            if ended.is_none() {
                if !kept.behind() {
                    return;
                }
                batches.push(self.batch(&mut kept, None));
            } else {
                while kept.behind() {
                    batches.push(self.batch(&mut kept, None));
                }
                batches.push(self.batch(&mut kept, ended));
            }
        }
        let mut sinks = lock(&self.sinks);
        for batch in &batches {
            sinks.retain(|sink| sink.send(batch));
        }
    }
}

/// The registry.
pub struct Runs {
    runs: Mutex<Vec<Arc<RunHandle>>>,
    dir: OnceLock<PathBuf>,
    history: OnceLock<History>,
    events: Mutex<Option<Arc<dyn Events>>>,
    next: AtomicU64,
}

static GLOBAL: LazyLock<Arc<Runs>> = LazyLock::new(|| Arc::new(Runs::new()));

impl Default for Runs {
    fn default() -> Self {
        Self::new()
    }
}

/// Why a run did not start.
#[derive(Debug, PartialEq, Eq)]
pub enum NotReserved {
    /// Four run already: their names.
    Full(Vec<String>),
}

impl std::fmt::Display for NotReserved {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Full(names) => write!(
                f,
                "four commands are running ({}): stop one or wait for it to end",
                names.join(", ")
            ),
        }
    }
}

impl Runs {
    #[must_use]
    pub fn new() -> Self {
        Self {
            runs: Mutex::new(Vec::new()),
            dir: OnceLock::new(),
            history: OnceLock::new(),
            events: Mutex::new(None),
            next: AtomicU64::new(0),
        }
    }

    #[must_use]
    pub fn global() -> Arc<Self> {
        Arc::clone(&GLOBAL)
    }

    /// Where runs keep their files and their list: `<profile>/command-runs`.
    /// What a crash left there is removed, the list kept.
    pub fn place(&self, dir: PathBuf) {
        if self.dir.set(dir.clone()).is_ok() {
            if let Ok(listed) = std::fs::read_dir(&dir) {
                for entry in listed.flatten() {
                    if entry.path().is_dir() {
                        let _ = std::fs::remove_dir_all(entry.path());
                    }
                }
            }
            let _ = self.history.set(History::new(&dir));
        }
    }

    /// The folder runs keep their files in, once placed.
    #[must_use]
    pub fn dir(&self) -> Option<PathBuf> {
        self.dir.get().cloned()
    }

    pub fn hear(&self, events: Arc<dyn Events>) {
        *lock(&self.events) = Some(events);
    }

    fn emit(&self, row: &RunRow) {
        let events = lock(&self.events).clone();
        if let Some(events) = events {
            events.moved(row);
        }
    }

    fn new_id(&self) -> String {
        let n = self.next.fetch_add(1, Ordering::Relaxed);
        format!("r{}-{n}", chrono::Utc::now().timestamp_millis())
    }

    /// The running run of `command` on `target` of `entity_id`, if any.
    #[must_use]
    pub fn running_of(
        &self,
        command: &str,
        entity_id: &str,
        target: &Target,
    ) -> Option<Arc<RunHandle>> {
        let key = key_of(command, entity_id, target);
        lock(&self.runs)
            .iter()
            .find(|r| r.key == key && r.running())
            .cloned()
    }

    /// Holds a place for a new run: refused past four at once.
    ///
    /// # Errors
    ///
    /// [`NotReserved::Full`] with the names of the four running.
    pub fn reserve(&self, mut row: RunRow, limits: Limits) -> Result<Arc<RunHandle>, NotReserved> {
        let mut runs = lock(&self.runs);
        let running: Vec<String> = runs
            .iter()
            .filter(|r| r.running())
            .map(|r| lock(&r.row).name.clone())
            .collect();
        if running.len() >= AT_ONCE {
            return Err(NotReserved::Full(running));
        }
        row.id = self.new_id();
        let scratch = self
            .dir
            .get()
            .cloned()
            .unwrap_or_else(std::env::temp_dir)
            .join(&row.id);
        let handle = Arc::new(RunHandle {
            id: row.id.clone(),
            key: key_of(&row.command, &row.entity_id, &row.target),
            row: Mutex::new(row),
            kept: Mutex::new(Kept::new(limits)),
            bytes: AtomicU64::new(0),
            findings: AtomicU32::new(0),
            stop: AtomicBool::new(false),
            pid: Mutex::new(None),
            sinks: Mutex::new(Vec::new()),
            started: Instant::now(),
            shown: Mutex::new(Instant::now()),
            scratch,
            result: Mutex::new(None),
            made_changed: Mutex::new(None),
            #[cfg(windows)]
            job: Mutex::new(None),
            held: Mutex::new(Held::Nothing),
            throwaway: Mutex::new(None),
        });
        runs.push(Arc::clone(&handle));
        drop(runs);
        self.emit(&handle.row());
        Ok(handle)
    }

    /// Hold `dir`, a try's throwaway folder, until the run ends; at once when it already has.
    pub fn throwaway(&self, handle: &RunHandle, dir: PathBuf) {
        if lock(&handle.row).ended.is_some() {
            let _ = std::fs::remove_dir_all(dir);
        } else {
            *lock(&handle.throwaway) = Some(dir);
        }
    }

    /// The run with this id.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<Arc<RunHandle>> {
        lock(&self.runs).iter().find(|r| r.id == id).cloned()
    }

    /// Sends this run's batches to `sink` too: what it printed so far is read
    /// with [`Self::lines`].
    pub fn watch(&self, id: &str, sink: Arc<dyn Sink>) -> bool {
        let Some(handle) = self.get(id) else {
            return false;
        };
        *lock(&handle.shown) = Instant::now();
        if handle.running() {
            lock(&handle.sinks).push(sink);
        }
        true
    }

    /// The kept lines numbered `from` to `from + count`.
    #[must_use]
    pub fn lines(&self, id: &str, from: u32, count: u32) -> Option<Batch> {
        let handle = self.get(id)?;
        *lock(&handle.shown) = Instant::now();
        let kept = lock(&handle.kept);
        let row = lock(&handle.row);
        Some(Batch {
            run: handle.id.clone(),
            lines: kept.range(from, count.min(20_000)),
            changed: Vec::new(),
            total: kept.total(),
            dropped: kept.dropped(),
            tail_from: kept.tail_from(),
            bytes: handle.bytes.load(Ordering::Relaxed),
            findings: handle.findings.load(Ordering::Relaxed),
            elapsed_ms: row.duration_ms.unwrap_or_else(|| {
                u64::try_from(handle.started.elapsed().as_millis()).unwrap_or(u64::MAX)
            }),
            ended: row.ended.clone(),
            result: lock(&handle.result).clone(),
            made_changed: lock(&handle.made_changed).clone(),
        })
    }

    /// What a run printed and keeps, as text a line each: the gap said where
    /// lines were not kept, and with `colours` its colour runs written back as
    /// SGR codes.
    #[must_use]
    pub fn text(&self, id: &str, colours: bool) -> Option<String> {
        let handle = self.get(id)?;
        let kept = lock(&handle.kept);
        let mut out = String::new();
        let mut last: Option<u32> = None;
        for line in kept.all() {
            if let Some(last) = last
                && line.n > last + 1
            {
                use std::fmt::Write as _;
                let _ = writeln!(out, "[{} lines here were not kept]", line.n - last - 1);
            }
            last = Some(line.n);
            if colours {
                out.push_str(&super::lines::with_codes(line));
            } else {
                out.push_str(&line.text);
            }
            out.push('\n');
        }
        Some(out)
    }

    /// Adds a sink to a run that was just reserved.
    pub fn attach(&self, handle: &RunHandle, sink: Arc<dyn Sink>) {
        lock(&handle.sinks).push(sink);
    }

    /// Asks a run to stop: its thread kills it with everything it started.
    pub fn stop(&self, id: &str) {
        if let Some(handle) = self.get(id) {
            handle.stop.store(true, Ordering::Relaxed);
        }
    }

    /// A run ended: the last batches, the list, the event, the memory cap.
    pub fn end(&self, handle: &RunHandle, ended: Ended) {
        {
            let mut row = lock(&handle.row);
            if row.ended.is_some() {
                return;
            }
            row.ended = Some(ended.clone());
            row.duration_ms =
                Some(u64::try_from(handle.started.elapsed().as_millis()).unwrap_or(u64::MAX));
        }
        *lock(&handle.pid) = None;
        handle.flush(handle.started, Some(ended));
        lock(&handle.sinks).clear();
        let row = handle.row();
        // A try is no row in the list of runs (#2562), and its throwaway file goes with it.
        if let Some(history) = self.history.get()
            && row.entity_id != crate::try_on::TRY_ENTITY
        {
            history.ended(row.clone());
        }
        if let Some(dir) = lock(&handle.throwaway).take() {
            let _ = std::fs::remove_dir_all(dir);
        }
        self.emit(&row);
        self.cap(KEPT_RUNS, KEPT_BYTES);
    }

    /// Drops the lines of the runs shown longest ago past `runs` runs or
    /// `bytes` bytes; each keeps its row.
    pub fn cap(&self, runs: usize, bytes: usize) {
        let mut ended: Vec<Arc<RunHandle>> = lock(&self.runs)
            .iter()
            .filter(|r| !r.running() && (lock(&r.kept).kept() > 0 || r.held_bytes() > 0))
            .cloned()
            .collect();
        ended.sort_by_key(|r| *lock(&r.shown));
        let mut total: usize = ended
            .iter()
            .map(|r| lock(&r.kept).bytes() + r.held_bytes())
            .sum();
        let mut count = ended.len();
        for run in ended {
            if count <= runs && total <= bytes {
                break;
            }
            let mut kept = lock(&run.kept);
            total = total.saturating_sub(kept.bytes() + run.held_bytes());
            kept.forget();
            *lock(&run.held) = Held::Nothing;
            *lock(&run.result) = None;
            count -= 1;
            let _ = std::fs::remove_dir_all(&run.scratch);
        }
        // The rows of runs that kept nothing live on in the list on disk.
        let mut all = lock(&self.runs);
        let over = all.len().saturating_sub(runs * 4);
        if over > 0 {
            let mut gone = 0;
            all.retain(|r| {
                let drop = gone < over && !r.running() && lock(&r.kept).kept() == 0;
                if drop {
                    gone += 1;
                }
                !drop
            });
        }
    }

    /// Every run of this session and the list's, newest first, the running
    /// ones first; of one repository where `entity_id` names it.
    #[must_use]
    pub fn rows(&self, entity_id: Option<&str>) -> Vec<RunRow> {
        let live: Vec<RunRow> = lock(&self.runs).iter().rev().map(|r| r.row()).collect();
        let mut rows: Vec<RunRow> = live.iter().filter(|r| r.ended.is_none()).cloned().collect();
        rows.extend(live.iter().filter(|r| r.ended.is_some()).cloned());
        if let Some(history) = self.history.get() {
            for row in history.rows() {
                if !rows.iter().any(|r| r.id == row.id) {
                    rows.push(row);
                }
            }
        }
        if let Some(entity) = entity_id {
            rows.retain(|r| r.entity_id == entity);
        }
        rows
    }

    /// Forgets one run (its lines and its row) or every run that ended.
    pub fn forget(&self, id: Option<&str>) {
        let mut runs = lock(&self.runs);
        runs.retain(|r| {
            let gone = !r.running() && id.is_none_or(|id| r.id == id);
            if gone {
                let _ = std::fs::remove_dir_all(&r.scratch);
            }
            !gone
        });
        drop(runs);
        if let Some(history) = self.history.get() {
            history.forget(id);
        }
    }

    /// The app is quitting: every run is killed with what it started, and
    /// their folders go.
    pub fn kill_all(&self) {
        let runs: Vec<Arc<RunHandle>> = lock(&self.runs).clone();
        for run in &runs {
            run.stop.store(true, Ordering::Relaxed);
            let pid = *lock(&run.pid);
            run.kill(pid);
            let _ = std::fs::remove_dir_all(&run.scratch);
        }
    }

    /// How many run now.
    #[must_use]
    pub fn running(&self) -> usize {
        lock(&self.runs).iter().filter(|r| r.running()).count()
    }
}

fn key_of(command: &str, entity_id: &str, target: &Target) -> String {
    format!(
        "{command}\u{0}{entity_id}\u{0}{}",
        serde_json::to_string(target).unwrap_or_default()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(name: &str) -> RunRow {
        RunRow {
            id: String::new(),
            command: name.to_lowercase(),
            name: name.to_owned(),
            entity_id: "gitea:a/b".to_owned(),
            target: Target::Repository,
            argv: vec![],
            cwd: String::new(),
            branch: None,
            head: None,
            started_at: 0,
            duration_ms: None,
            ended: None,
            lines: 0,
            findings: 0,
            kept: true,
        }
    }

    #[test]
    fn four_at_once_and_never_five() {
        let runs = Runs::new();
        let held: Vec<_> = ["A", "B", "C", "D"]
            .iter()
            .map(|n| runs.reserve(row(n), Limits::default()).unwrap())
            .collect();
        let refused = runs.reserve(row("E"), Limits::default()).err().unwrap();
        assert_eq!(
            refused,
            NotReserved::Full(vec!["A".into(), "B".into(), "C".into(), "D".into()])
        );
        assert!(refused.to_string().contains("four commands are running"));
        runs.end(&held[0], Ended::Exit { code: 0 });
        assert!(runs.reserve(row("E"), Limits::default()).is_ok());
    }

    #[test]
    fn the_running_run_of_a_command_on_a_thing_is_found_again() {
        let runs = Runs::new();
        let a = runs.reserve(row("A"), Limits::default()).unwrap();
        assert_eq!(
            runs.running_of("a", "gitea:a/b", &Target::Repository)
                .unwrap()
                .id,
            a.id
        );
        assert!(
            runs.running_of("a", "gitea:a/c", &Target::Repository)
                .is_none()
        );
        runs.end(&a, Ended::Stopped);
        assert!(
            runs.running_of("a", "gitea:a/b", &Target::Repository)
                .is_none()
        );
    }

    #[test]
    fn past_the_memory_cap_the_run_shown_longest_ago_loses_its_lines_first() {
        let runs = Runs::new();
        let mut held = Vec::new();
        for n in ["A", "B", "C"] {
            let h = runs.reserve(row(n), Limits::default()).unwrap();
            h.with_kept(|kept| {
                let mut reader = super::super::lines::Reader::default();
                reader.write(kept, super::super::lines::Stream::Out, b"one\ntwo\n");
            });
            runs.end(&h, Ended::Exit { code: 0 });
            held.push(h);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        // B is looked at again: A is the one shown longest ago.
        let _ = runs.lines(&held[1].id, 0, 10);
        runs.cap(2, usize::MAX);
        assert!(!held[0].row().kept);
        assert!(held[1].row().kept && held[2].row().kept);
        assert_eq!(held[0].row().lines, 2);
    }

    struct Collect(Mutex<Vec<Batch>>);

    impl Sink for Collect {
        fn send(&self, batch: &Batch) -> bool {
            lock(&self.0).push(batch.clone());
            true
        }
    }

    #[test]
    fn the_last_batch_carries_the_ending_and_every_line_before_it() {
        let runs = Runs::new();
        let h = runs.reserve(row("A"), Limits::default()).unwrap();
        let sink = Arc::new(Collect(Mutex::new(Vec::new())));
        runs.attach(&h, sink.clone());
        h.with_kept(|kept| {
            let mut reader = super::super::lines::Reader::default();
            for i in 0..5000 {
                reader.write(
                    kept,
                    super::super::lines::Stream::Out,
                    format!("line {i}\n").as_bytes(),
                );
            }
        });
        runs.end(&h, Ended::Exit { code: 2 });
        let batches = lock(&sink.0);
        let lines: usize = batches.iter().map(|b| b.lines.len()).sum();
        assert_eq!(lines, 5000);
        assert!(batches.iter().all(|b| b.lines.len() <= BATCH_LINES));
        assert_eq!(batches.last().unwrap().ended, Some(Ended::Exit { code: 2 }));
        assert!(
            batches[..batches.len() - 1]
                .iter()
                .all(|b| b.ended.is_none())
        );
    }
}

//! Folder renders: which run is going, what was drawn, and what is kept
//! (ADR-0044).
//!
//! # A run
//!
//! A page asks for a renderer on a folder ([`Runs::view`]) and is answered at
//! once: what is kept for the folder as it is now, else the last drawing of
//! that folder by that renderer (shown under a line saying it is from an
//! earlier state), and the run's state. Only an ask with `start` begins a
//! run: the Code page asks so when a person opens the folder's render, never
//! for a kept or prebuilt page, a hover or a sync. Progress and the end are
//! told on [`crate::events::FOLDER_RENDER`], and the page asks again.
//!
//! - One run per folder and renderer (a *slot*). An ask for the folder as it
//!   already runs joins it; an ask for it at another state (its content
//!   changed) stops the older run first.
//! - At most [`AT_ONCE`] tools run at the same time; the rest wait in order.
//!   A waiting run that the page left is dropped ([`Runs::leave`]); a run
//!   that has started finishes into the cache, bounded by its timeout.
//! - [`Runs::stop`] stops a slot's run, waiting or going.
//!
//! # What is kept
//!
//! Keyed by the renderer's fingerprint (its command and output), the copy's
//! name and the folder's content: from the clone, every path, mode and blob
//! under it, so the same folder on another branch or commit is the same
//! drawing; from the forge, which names no blob, the commit and the folder.
//!
//! - In memory, [`MEMORY_BYTES`] at most, the least recently used dropped
//!   first. A failed run is kept for the session, so coming back does not
//!   start a failing 30 s tool again.
//! - On disk, successes only, under the profile's `folder-renders/`, one
//!   folder per source, [`DISK_BYTES`] at most, the oldest dropped first.
//!   Removing a source or changing its credential removes its folder
//!   ([`forget_source`]). A backup dumps the database and never carries it.
//! - A folder kind's generated folder (ADR-0046) is kept as files in the
//!   [`store`](super::store), a folder per run, its manifest in memory for
//!   the last [`MEMORY_FOLDERS`]; its file bytes are never held here.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, PoisonError};
use std::time::Instant;

use serde::{Deserialize, Serialize};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

use super::copy::{self, Bytes, Item, Scratch};
use super::keep::{self, KeepError, Made};
use super::run::{DrawnKind, Ran};
use super::store;
use super::{FolderOutput, FolderRenderer, OUTPUT_CAP, hex};

/// How many folder renderers run at the same time.
pub const AT_ONCE: usize = 2;
/// The most output kept in memory.
pub const MEMORY_BYTES: usize = 128 * 1024 * 1024;
/// The most output kept on disk.
pub const DISK_BYTES: u64 = 512 * 1024 * 1024;
/// The most generated folders whose manifests are kept in memory.
pub const MEMORY_FOLDERS: usize = 64;
/// How often a shown generated folder's `.json` is touched, for the bound's
/// least recently shown.
const TOUCH_EVERY_SECS: u64 = 60;
/// The fewest milliseconds between two progress events of one run.
const PROGRESS_EVERY_MS: u128 = 150;

/// Where the copy was written from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "app.ts")]
pub enum ReadFrom {
    Clone,
    Forge,
}

/// What a run drew.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "app.ts")]
pub struct FolderDrawn {
    /// What `folder_render_output` reads the bytes by.
    pub token: String,
    pub kind: DrawnKind,
    pub size: u64,
    /// The commit the folder was copied at, where known.
    pub commit: Option<String>,
    pub from: ReadFrom,
    /// How long the copy and the run took, in milliseconds.
    pub took_ms: u64,
    /// When it was drawn (RFC 3339).
    pub drawn_at: String,
}

/// Where a run is.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(tag = "state", rename_all = "snake_case")]
#[ts(export_to = "app.ts")]
pub enum FolderRunState {
    /// Waiting for one of the [`AT_ONCE`] places.
    Queued,
    /// Writing the copy: files written of the total.
    Copying { done: u32, total: u32 },
    /// The tool is running, for this many milliseconds so far.
    Running { elapsed_ms: u64 },
    /// Keeping what a folder kind wrote: files kept of the total.
    Keeping { done: u32, total: u32 },
}

/// Why nothing was drawn. An answer the page draws, not an error.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(tag = "why", rename_all = "snake_case")]
#[ts(export_to = "app.ts")]
pub enum FolderFailure {
    /// The tool exited non-zero (`status` `None` for a signal).
    Failed {
        status: Option<i32>,
        stderr: String,
    },
    TimedOut {
        seconds: u32,
        stderr: String,
    },
    /// The output passed the cap.
    TooLarge {
        cap_bytes: u64,
    },
    NotStarted {
        reason: String,
    },
    /// Exit 0, `{out}` in the command, and not one file written there.
    NoOutputFile {
        found: u32,
        stderr: String,
    },
    /// The copy could not be written: a refused name, a read that failed.
    Copy {
        reason: String,
    },
    /// The folder is past what is copied from where it is read.
    TooBig {
        files: u64,
        bytes: u64,
        cap_files: u64,
        cap_bytes: u64,
        from: ReadFrom,
    },
    /// Nothing at this path is a folder at the commit.
    NoFolder,
    /// Stopped by the person.
    Stopped,
    /// Exit 0 and not one regular file kept from `{outdir}`.
    NoOutput {
        stderr: String,
    },
    /// `{outdir}` held more than is kept: what it held, and the caps.
    OutputTooBig {
        files: u64,
        bytes: u64,
        cap_files: u64,
        cap_bytes: u64,
    },
    /// The store, or a kept file, could not be written.
    Keep {
        reason: String,
    },
}

/// The answer to a page's ask.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "app.ts")]
pub struct FolderRenderView {
    pub renderer: String,
    pub folder: String,
    /// What to show: the drawing of the folder as it is now, or the last one
    /// of an earlier state (`current` false).
    pub drawn: Option<FolderDrawn>,
    pub current: bool,
    pub running: Option<FolderRunState>,
    /// The last run of the folder as it is now failed (or the copy cannot be
    /// made); `None` while one runs.
    pub failure: Option<FolderFailure>,
    /// Where the copy is, or would be, written from.
    pub from: Option<ReadFrom>,
    /// The commit the folder is read at now.
    pub commit: Option<String>,
}

/// The event's payload: a slot's run moved; the page showing it asks again.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "app.ts")]
pub struct FolderRenderMoved {
    pub entity_id: String,
    pub renderer: String,
    pub folder: String,
    /// The run's state now; `None` once it ended.
    pub running: Option<FolderRunState>,
}

/// What an ask may start.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "app.ts")]
pub enum Ask {
    /// Start nothing: a kept or prebuilt page, a return to a hidden one.
    Look,
    /// A person opened the render: run when nothing is kept for the folder as
    /// it is now and no run of it failed this session.
    Open,
    /// *Run again*: run whatever is kept.
    Again,
}

/// The folder could not be resolved: why, and what is known of it.
#[derive(Debug)]
pub struct Unresolved {
    pub failure: FolderFailure,
    pub from: Option<ReadFrom>,
    pub commit: Option<String>,
    pub source_id: Option<String>,
}

/// A folder and renderer on one repository or branch.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Slot {
    pub entity_id: String,
    pub renderer: String,
    pub folder: String,
}

/// The folder resolved at its state now: what to copy and where from.
pub struct Resolved {
    pub key: String,
    pub copy_name: String,
    pub items: Vec<Item>,
    pub bytes: Arc<dyn Bytes>,
    pub from: ReadFrom,
    pub commit: Option<String>,
    /// The source the folder is read from, which the disk cache is kept
    /// under; `None` keeps nothing on disk.
    pub source_id: Option<String>,
}

/// The cache key of a folder read from the clone: every path, kind, mode and
/// blob under it, with the renderer and the copy's name.
#[must_use]
pub fn clone_key(renderer: &FolderRenderer, copy_name: &str, items: &[Item]) -> String {
    use sha2::Digest as _;
    let mut hash = sha2::Sha256::new();
    hash.update(renderer.fingerprint().as_bytes());
    hash.update([0]);
    hash.update(copy_name.as_bytes());
    for item in items {
        hash.update([0]);
        hash.update(item.rel.as_bytes());
        hash.update([0, item.kind as u8, u8::from(item.executable)]);
        hash.update(item.blob.as_deref().unwrap_or("").as_bytes());
    }
    hex(&hash.finalize()[..16])
}

/// The cache key of a folder read from the forge, which names no blob: the
/// commit and the folder.
#[must_use]
pub fn forge_key(renderer: &FolderRenderer, copy_name: &str, commit: &str, folder: &str) -> String {
    use sha2::Digest as _;
    let mut hash = sha2::Sha256::new();
    for part in [
        renderer.fingerprint().as_str(),
        copy_name,
        "forge",
        commit,
        folder,
    ] {
        hash.update(part.as_bytes());
        hash.update([0]);
    }
    hex(&hash.finalize()[..16])
}

/// One drawing kept: what was drawn, and its bytes or its generated folder.
struct Kept {
    drawn: FolderDrawn,
    body: Body,
}

enum Body {
    Bytes(Arc<Vec<u8>>),
    Folder(Arc<Made>),
}

impl Kept {
    /// The bytes it holds in memory: a folder's are on disk.
    fn bytes_held(&self) -> usize {
        match &self.body {
            Body::Bytes(bytes) => bytes.len(),
            Body::Folder(_) => 0,
        }
    }
}

/// A slot's run.
struct Going {
    key: String,
    cancel: CancellationToken,
    state: FolderRunState,
    started: Instant,
}

#[derive(Default)]
struct State {
    /// Drawings by key, and the order they were last used in.
    memory: HashMap<String, Arc<Kept>>,
    used: VecDeque<String>,
    memory_bytes: usize,
    memory_folders: usize,
    /// When a generated folder's `.json` was last touched, by key.
    touched: HashMap<String, Instant>,
    /// The key each token names.
    tokens: HashMap<String, String>,
    failures: HashMap<String, FolderFailure>,
    /// The last key drawn for each slot.
    latest: HashMap<Slot, String>,
    going: HashMap<Slot, Going>,
    /// Sanitized HTML documents the frame loads, by their own token.
    frames: VecDeque<(String, Arc<Vec<u8>>)>,
}

type Listener = Arc<dyn Fn(FolderRenderMoved) + Send + Sync>;

/// The folder renders of one app.
pub struct Runs {
    state: Mutex<State>,
    places: Arc<Semaphore>,
    listener: Mutex<Option<Listener>>,
    disk: Mutex<Option<PathBuf>>,
    /// The store generated folders are kept in (ADR-0046).
    store: Mutex<Option<PathBuf>>,
    root: PathBuf,
    swept: std::sync::Once,
    staging_swept: std::sync::Once,
}

impl Runs {
    /// Renders whose copies go under `root`.
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self {
            state: Mutex::default(),
            places: Arc::new(Semaphore::new(AT_ONCE)),
            listener: Mutex::default(),
            disk: Mutex::default(),
            store: Mutex::default(),
            root,
            swept: std::sync::Once::new(),
            staging_swept: std::sync::Once::new(),
        }
    }

    /// The one the app's commands use.
    #[must_use]
    pub fn global() -> Arc<Runs> {
        static GLOBAL: LazyLock<Arc<Runs>> = LazyLock::new(|| Arc::new(Runs::new(copy::root())));
        Arc::clone(&GLOBAL)
    }

    /// Who is told a slot moved: the bridge, in the app.
    pub fn on_moved(&self, listener: impl Fn(FolderRenderMoved) + Send + Sync + 'static) {
        *self.listener.lock().unwrap_or_else(PoisonError::into_inner) = Some(Arc::new(listener));
    }

    /// Keep drawings on disk under `dir` too.
    pub fn keep_on_disk(&self, dir: PathBuf) {
        *self.disk.lock().unwrap_or_else(PoisonError::into_inner) = Some(dir);
    }

    /// Keep generated folders under `store` (`…/knobas-rendered`). A store
    /// that moved is removed whole at its old place, and what memory knows
    /// of it forgotten; folders are made again when opened.
    pub fn set_store(&self, store: PathBuf) {
        let old = self
            .store
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .replace(store.clone());
        if let Some(old) = old
            && old != store
        {
            self.forget_folders();
            store::remove(&old);
        }
    }

    /// The store, when set.
    #[must_use]
    pub fn store(&self) -> Option<PathBuf> {
        self.store
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// The store's folder for `source`, `unknown` hashed where the entity
    /// names none.
    fn store_dir(&self, source: Option<&str>) -> Option<PathBuf> {
        Some(self.store()?.join(source_dir(source.unwrap_or("unknown"))))
    }

    /// Forget every generated folder in memory (the store moved or was
    /// cleared); they are read from disk, or made again, when asked.
    pub fn forget_folders(&self) {
        let mut state = self.lock();
        let keys: Vec<String> = state
            .memory
            .iter()
            .filter(|(_, k)| matches!(k.body, Body::Folder(_)))
            .map(|(key, _)| key.clone())
            .collect();
        for key in keys {
            if let Some(gone) = state.memory.remove(&key) {
                state.tokens.remove(&gone.drawn.token);
            }
            state.used.retain(|k| k != &key);
            state.latest.retain(|_, k| k != &key);
        }
        state.memory_folders = 0;
        state.touched.clear();
    }

    /// Remove every generated folder, on disk and in memory: Settings'
    /// *Clear*.
    pub fn clear_store(&self) {
        self.forget_folders();
        if let Some(root) = self.store() {
            store::clear(&root);
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn tell(&self, slot: &Slot, running: Option<FolderRunState>) {
        let listener = self
            .listener
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        if let Some(listener) = listener {
            listener(FolderRenderMoved {
                entity_id: slot.entity_id.clone(),
                renderer: slot.renderer.clone(),
                folder: slot.folder.clone(),
                running,
            });
        }
    }

    fn disk_dir(&self, source: Option<&str>) -> Option<PathBuf> {
        let root = self
            .disk
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()?;
        Some(root.join(source_dir(source?)))
    }

    /// The kept drawing `key`, from memory or from disk (put back in
    /// memory): a generated folder from the store when `folder`.
    fn kept(&self, key: &str, source: Option<&str>, folder: bool) -> Option<Arc<Kept>> {
        {
            let mut state = self.lock();
            if let Some(kept) = state.memory.get(key).cloned() {
                touch(&mut state.used, key);
                return Some(kept);
            }
        }
        if folder {
            let dir = self.store_dir(source)?;
            let (meta, made) = store::read_meta(&dir, key)?;
            let kept = Arc::new(Kept {
                drawn: meta.drawn,
                body: Body::Folder(Arc::new(made)),
            });
            self.remember(key, kept.clone());
            return Some(kept);
        }
        let dir = self.disk_dir(source)?;
        let meta = std::fs::read(dir.join(format!("{key}.json"))).ok()?;
        let drawn: FolderDrawn = serde_json::from_slice(&meta).ok()?;
        let bytes = std::fs::read(dir.join(format!("{key}.out"))).ok()?;
        let kept = Arc::new(Kept {
            drawn,
            body: Body::Bytes(Arc::new(bytes)),
        });
        self.remember(key, kept.clone());
        Some(kept)
    }

    fn remember(&self, key: &str, kept: Arc<Kept>) {
        let mut state = self.lock();
        state
            .tokens
            .insert(kept.drawn.token.clone(), key.to_owned());
        state.memory_bytes += kept.bytes_held();
        state.memory_folders += usize::from(matches!(kept.body, Body::Folder(_)));
        if let Some(old) = state.memory.insert(key.to_owned(), kept) {
            state.memory_bytes -= old.bytes_held();
            state.memory_folders -= usize::from(matches!(old.body, Body::Folder(_)));
            // A re-run of one key: the old token names nothing now.
            let new_token = state.memory.get(key).map(|k| k.drawn.token.clone());
            if new_token.as_deref() != Some(old.drawn.token.as_str()) {
                state.tokens.remove(&old.drawn.token);
            }
        }
        touch(&mut state.used, key);
        // The oldest of whichever kind is past its bound goes first; the one
        // just remembered stays.
        loop {
            let bytes_over = state.memory_bytes > MEMORY_BYTES;
            let folders_over = state.memory_folders > MEMORY_FOLDERS;
            if !bytes_over && !folders_over {
                break;
            }
            let oldest = state.used.iter().position(|k| {
                k != key
                    && state.memory.get(k).is_some_and(|kept| {
                        if folders_over {
                            matches!(kept.body, Body::Folder(_))
                        } else {
                            matches!(kept.body, Body::Bytes(_))
                        }
                    })
            });
            let Some(at) = oldest else { break };
            let Some(gone_key) = state.used.remove(at) else {
                break;
            };
            if let Some(gone) = state.memory.remove(&gone_key) {
                state.memory_bytes -= gone.bytes_held();
                state.memory_folders -= usize::from(matches!(gone.body, Body::Folder(_)));
                state.tokens.remove(&gone.drawn.token);
            }
        }
    }

    /// Touch a shown generated folder's `.json`, at most once a minute: the
    /// store's bound takes the least recently shown first.
    fn touch_shown(&self, key: &str, source: Option<&str>, kept: &Kept) {
        let Body::Folder(made) = &kept.body else {
            return;
        };
        {
            let mut state = self.lock();
            if state
                .touched
                .get(key)
                .is_some_and(|at| at.elapsed().as_secs() < TOUCH_EVERY_SECS)
            {
                return;
            }
            state.touched.insert(key.to_owned(), Instant::now());
        }
        let (Some(dir), Some(leaf)) = (self.store_dir(source), made.dir.file_name()) else {
            return;
        };
        let json = dir.join(format!("{}.json", leaf.to_string_lossy()));
        if let Ok(file) = std::fs::File::options().write(true).open(json) {
            let _ = file.set_modified(std::time::SystemTime::now());
        }
    }

    /// The last key drawn for `slot`: in memory, else on disk (the store's
    /// slots for a folder kind).
    fn latest(&self, slot: &Slot, source: Option<&str>, folder: bool) -> Option<String> {
        if let Some(key) = self.lock().latest.get(slot).cloned() {
            return Some(key);
        }
        let dir = if folder {
            self.store_dir(source)?
        } else {
            self.disk_dir(source)?
        };
        let key = std::fs::read_to_string(dir.join("slots").join(slot_file(slot))).ok()?;
        let key = key.trim().to_owned();
        (!key.is_empty()).then_some(key)
    }

    /// The answer for `slot` at `resolved`, starting a run as `ask` says. A
    /// folder that cannot be resolved answers its failure beside the last
    /// drawing.
    pub fn view(
        self: &Arc<Self>,
        slot: &Slot,
        renderer: &FolderRenderer,
        resolved: Result<Resolved, Unresolved>,
        ask: Ask,
    ) -> FolderRenderView {
        let (key, from, commit, source) = match &resolved {
            Ok(r) => (
                Some(r.key.clone()),
                Some(r.from),
                r.commit.clone(),
                r.source_id.clone(),
            ),
            Err(u) => (None, u.from, u.commit.clone(), u.source_id.clone()),
        };
        let mut view = FolderRenderView {
            renderer: slot.renderer.clone(),
            folder: slot.folder.clone(),
            drawn: None,
            current: false,
            running: None,
            failure: None,
            from,
            commit,
        };
        let folder = renderer.output == FolderOutput::Folder;
        let kept_now = key
            .as_deref()
            .and_then(|key| self.kept(key, source.as_deref(), folder));
        if let (Some(key), Some(kept)) = (&key, &kept_now) {
            self.lock().latest.insert(slot.clone(), key.clone());
            view.drawn = Some(kept.drawn.clone());
            view.current = true;
            self.touch_shown(key, source.as_deref(), kept);
        } else if let Some(last) = self.latest(slot, source.as_deref(), folder)
            && let Some(kept) = self.kept(&last, source.as_deref(), folder)
        {
            view.drawn = Some(kept.drawn.clone());
            self.touch_shown(&last, source.as_deref(), &kept);
        }
        let resolved = match resolved {
            Ok(resolved) => resolved,
            Err(unresolved) => {
                view.failure = Some(unresolved.failure);
                return view;
            }
        };
        let key = resolved.key.clone();
        let mut state = self.lock();
        match state.going.get(slot) {
            // The folder as it runs already: join it.
            Some(going) if going.key == key => {
                view.running = Some(running_now(going));
                return view;
            }
            // Another state of the folder runs: an ask that starts stops it.
            Some(going) if ask != Ask::Look && (kept_now.is_none() || ask == Ask::Again) => {
                going.cancel.cancel()
            }
            Some(going) => {
                view.running = Some(running_now(going));
                return view;
            }
            None => {}
        }
        let failed = state.failures.get(&key).cloned();
        let starts = match ask {
            Ask::Look => false,
            // A return draws what is kept, and a failure kept for the session
            // is shown, not run again (a stop is run again).
            Ask::Open => {
                kept_now.is_none() && failed.as_ref().is_none_or(|f| *f == FolderFailure::Stopped)
            }
            Ask::Again => true,
        };
        if !starts {
            if kept_now.is_none() {
                view.failure = failed;
            }
            return view;
        }
        state.failures.remove(&key);
        let cancel = CancellationToken::new();
        state.going.insert(
            slot.clone(),
            Going {
                key,
                cancel: cancel.clone(),
                state: FolderRunState::Queued,
                started: Instant::now(),
            },
        );
        drop(state);
        view.running = Some(FolderRunState::Queued);
        view.failure = None;
        let runs = Arc::clone(self);
        let slot = slot.clone();
        let renderer = renderer.clone();
        tokio::spawn(async move { runs.go(slot, renderer, resolved, cancel).await });
        view
    }

    /// Drop `slot`'s run if it is still waiting for a place: the page that
    /// asked for it left.
    pub fn leave(&self, slot: &Slot) {
        let mut state = self.lock();
        if let Some(going) = state.going.get(slot)
            && going.state == FolderRunState::Queued
        {
            going.cancel.cancel();
            state.going.remove(slot);
        }
    }

    /// Stop `slot`'s run, waiting or going.
    pub fn stop(&self, slot: &Slot) {
        let going = self.lock().going.remove(slot);
        if let Some(going) = going {
            going.cancel.cancel();
            self.tell(slot, None);
        }
    }

    /// The run of `key` on `slot` is over, if it is still the one going.
    fn end_going(&self, slot: &Slot, key: &str) {
        let mut state = self.lock();
        if state.going.get(slot).is_some_and(|going| going.key == key) {
            state.going.remove(slot);
        }
    }

    fn set_state(&self, slot: &Slot, key: &str, next: FolderRunState) -> bool {
        let mut state = self.lock();
        match state.going.get_mut(slot) {
            Some(going) if going.key == key => {
                if matches!(next, FolderRunState::Running { .. }) {
                    going.started = Instant::now();
                }
                going.state = next;
                true
            }
            _ => false,
        }
    }

    async fn go(
        self: Arc<Self>,
        slot: Slot,
        renderer: FolderRenderer,
        resolved: Resolved,
        cancel: CancellationToken,
    ) {
        let key = resolved.key.clone();
        let place = tokio::select! {
            () = cancel.cancelled() => return,
            place = Arc::clone(&self.places).acquire_owned() => place,
        };
        let Ok(_place) = place else { return };
        if cancel.is_cancelled() {
            return;
        }
        let began = Instant::now();
        let total = u32::try_from(
            resolved
                .items
                .iter()
                .filter(|i| matches!(i.kind, copy::ItemKind::File | copy::ItemKind::Symlink))
                .count(),
        )
        .unwrap_or(u32::MAX);
        if self.set_state(&slot, &key, FolderRunState::Copying { done: 0, total }) {
            self.tell(&slot, Some(FolderRunState::Copying { done: 0, total }));
        }
        let token = uuid::Uuid::new_v4().simple().to_string();
        let ended = self
            .copy_and_run(
                &slot,
                &renderer,
                resolved_parts(&resolved),
                (
                    resolved.items,
                    resolved.bytes,
                    resolved.source_id.as_deref(),
                ),
                &token,
                cancel.clone(),
            )
            .await;
        let took_ms = u64::try_from(began.elapsed().as_millis()).unwrap_or(u64::MAX);
        // A run stopped or replaced leaves nothing behind it. A generated
        // folder ends its run only once it is placed and remembered: an ask
        // between would see neither a run nor a drawing.
        let placing = matches!(ended, Ok(Output::Folder(_)));
        let still_ours = {
            let mut state = self.lock();
            match state.going.get(&slot) {
                Some(going) if going.key == key => {
                    if !placing {
                        state.going.remove(&slot);
                    }
                    true
                }
                _ => false,
            }
        };
        if cancel.is_cancelled() || !still_ours {
            if still_ours {
                self.end_going(&slot, &key);
            }
            if let Ok(Output::Folder(made)) = ended {
                let dir = made.dir.clone();
                let _ = tokio::task::spawn_blocking(move || std::fs::remove_dir_all(dir)).await;
            }
            return;
        }
        match ended {
            Ok(Output::Folder(made)) => {
                let manifest = &made.manifest;
                let drawn = FolderDrawn {
                    token,
                    kind: DrawnKind::Folder {
                        files: u32::try_from(manifest.files.len()).unwrap_or(u32::MAX),
                        bytes: manifest.bytes,
                        left_out: manifest.left_out_total,
                    },
                    size: manifest.bytes,
                    commit: resolved.commit.clone(),
                    from: resolved.from,
                    took_ms,
                    drawn_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                };
                let source = resolved.source_id.clone();
                let placed = self
                    .place_folder(&slot, &key, source.as_deref(), drawn, made)
                    .await;
                match placed {
                    Ok(kept) => {
                        self.remember(&key, kept);
                        self.lock().latest.insert(slot.clone(), key.clone());
                    }
                    Err(reason) => {
                        self.lock()
                            .failures
                            .insert(key.clone(), FolderFailure::Keep { reason });
                    }
                }
                self.end_going(&slot, &key);
            }
            Ok(Output::Bytes(bytes, kind)) => {
                let drawn = FolderDrawn {
                    token,
                    kind,
                    size: bytes.len() as u64,
                    commit: resolved.commit.clone(),
                    from: resolved.from,
                    took_ms,
                    drawn_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                };
                let kept = Arc::new(Kept {
                    drawn,
                    body: Body::Bytes(Arc::new(bytes)),
                });
                self.remember(&key, kept.clone());
                self.lock().latest.insert(slot.clone(), key.clone());
                if let Some(dir) = self.disk_dir(resolved.source_id.as_deref()) {
                    let (slot_name, key) = (slot_file(&slot), key.clone());
                    let disk_root = self
                        .disk
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .clone();
                    let _ = tokio::task::spawn_blocking(move || {
                        write_disk(&dir, &key, &slot_name, &kept);
                        if let Some(root) = disk_root {
                            bound_disk(&root, DISK_BYTES);
                        }
                    })
                    .await;
                }
            }
            Err(failure) => {
                self.lock().failures.insert(key.clone(), failure);
            }
        }
        self.tell(&slot, None);
    }

    /// Move a kept folder from its `.tmp-` name to its leaf and write its
    /// description, key and slot; the old leaf of the same key is removed,
    /// and the store bounded. Answers what to remember.
    async fn place_folder(
        &self,
        slot: &Slot,
        key: &str,
        source: Option<&str>,
        drawn: FolderDrawn,
        made: Made,
    ) -> Result<Arc<Kept>, String> {
        let (Some(root), Some(dir)) = (self.store(), self.store_dir(source)) else {
            let _ = std::fs::remove_dir_all(&made.dir);
            return Err("there is no folder to keep generated files in".to_owned());
        };
        let slot_name = slot_file(slot);
        let key = key.to_owned();
        let placed = tokio::task::spawn_blocking(move || -> Result<Kept, String> {
            let leaf = made
                .dir
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .and_then(|n| n.split_once(".tmp-").map(|(leaf, _)| leaf.to_owned()))
                .ok_or_else(|| "the kept folder has no name".to_owned())?;
            let at = dir.join(&leaf);
            let tmp = made.dir.clone();
            if let Err(error) = std::fs::rename(&tmp, &at) {
                let _ = std::fs::remove_dir_all(&tmp);
                return Err(format!("{} could not be written: {error}", at.display()));
            }
            let made = made.moved_to(at);
            let before = std::fs::read_to_string(dir.join("keys").join(&key)).ok();
            let meta = store::Meta {
                bytes: made.manifest.bytes,
                drawn,
                key: key.clone(),
            };
            if let Err(error) = store::write_meta(&dir, &leaf, &meta, &made.manifest, &slot_name) {
                store::remove_leaf(&dir, &leaf);
                return Err(format!("{} could not be written: {error}", dir.display()));
            }
            // The old leaf of this key goes, best effort: a file open on
            // Windows keeps it until the next bound.
            if let Some(old) = before.map(|o| o.trim().to_owned())
                && !old.is_empty()
                && old != leaf
                && !old.contains(['/', '\\'])
            {
                store::remove_leaf(&dir, &old);
            }
            store::bound(&root, store::STORE_BYTES, Some(&leaf));
            Ok(Kept {
                drawn: meta.drawn,
                body: Body::Folder(Arc::new(made)),
            })
        })
        .await
        .map_err(|e| e.to_string())??;
        Ok(Arc::new(placed))
    }

    async fn copy_and_run(
        self: &Arc<Self>,
        slot: &Slot,
        renderer: &FolderRenderer,
        (key, copy_name): (String, String),
        (items, bytes, source): (Vec<Item>, Arc<dyn Bytes>, Option<&str>),
        token: &str,
        cancel: CancellationToken,
    ) -> Result<Output, FolderFailure> {
        let root = self.root.clone();
        self.swept.call_once(|| copy::sweep(&root));
        // A folder kind writes under the store's staging, so keeping is a
        // rename per file; a store that cannot be made fails the run before
        // the copy is written (ADR-0046).
        let folder = renderer.output == FolderOutput::Folder;
        let staging = if folder {
            let Some(store_root) = self.store() else {
                return Err(FolderFailure::Keep {
                    reason: "there is no folder to keep generated files in".to_owned(),
                });
            };
            let staging = store_root.join(store::STAGING);
            if let Err(error) = std::fs::create_dir_all(&staging) {
                return Err(FolderFailure::Keep {
                    reason: format!("{} could not be made: {error}", staging.display()),
                });
            }
            self.staging_swept.call_once(|| copy::sweep(&staging));
            Some(staging)
        } else {
            None
        };
        let made = {
            let (root, name, staging) = (root.clone(), copy_name.clone(), staging.clone());
            tokio::task::spawn_blocking(move || Scratch::make(&root, &name, staging.as_deref()))
                .await
        };
        let scratch = match made {
            Ok(Ok(scratch)) => scratch,
            Ok(Err(error)) => {
                return Err(FolderFailure::Copy {
                    reason: format!("the copy's folder could not be made: {error}"),
                });
            }
            Err(error) => {
                return Err(FolderFailure::Copy {
                    reason: error.to_string(),
                });
            }
        };
        let last = Arc::new(Mutex::new(
            Instant::now() - std::time::Duration::from_secs(1),
        ));
        let (runs, told, k) = (Arc::clone(self), slot.clone(), key.clone());
        let progress: copy::Progress = Arc::new(move |done, total| {
            let state = FolderRunState::Copying { done, total };
            if !runs.set_state(&told, &k, state.clone()) {
                return;
            }
            let mut at = last.lock().unwrap_or_else(PoisonError::into_inner);
            if done == total || at.elapsed().as_millis() >= PROGRESS_EVERY_MS {
                *at = Instant::now();
                runs.tell(&told, Some(state));
            }
        });
        if let Err(reason) = copy::write(&scratch, items, bytes, progress, cancel.clone()).await {
            return Err(if cancel.is_cancelled() {
                FolderFailure::Stopped
            } else {
                FolderFailure::Copy { reason }
            });
        }
        if self.set_state(slot, &key, FolderRunState::Running { elapsed_ms: 0 }) {
            self.tell(slot, Some(FolderRunState::Running { elapsed_ms: 0 }));
        }
        let ran = super::run::run(
            &renderer.command,
            renderer.output,
            renderer.timeout_secs,
            (&scratch.copy, &scratch.out, &scratch.cwd),
            std::env::vars_os().collect(),
            cancel.clone(),
        )
        .await;
        if let Ran::Wrote { stderr } = ran {
            let kept = self
                .keep_output(
                    slot,
                    &key,
                    (&scratch.out, source),
                    (&copy_name, &renderer.id, token),
                    cancel,
                    stderr,
                )
                .await;
            // The staging goes with the scratch, whatever was left in it.
            let _ = tokio::task::spawn_blocking(move || drop(scratch)).await;
            return kept;
        }
        // The folders go on the blocking pool: a big copy takes a while.
        let _ = tokio::task::spawn_blocking(move || drop(scratch)).await;
        match ran {
            Ran::Wrote { .. } => unreachable!("kept above"),
            Ran::Made { bytes, kind } => Ok(Output::Bytes(bytes, kind)),
            Ran::Failed { status, stderr } => Err(FolderFailure::Failed { status, stderr }),
            Ran::TimedOut { seconds, stderr } => Err(FolderFailure::TimedOut { seconds, stderr }),
            Ran::TooLarge => Err(FolderFailure::TooLarge {
                cap_bytes: OUTPUT_CAP as u64,
            }),
            Ran::NotStarted(reason) => Err(FolderFailure::NotStarted { reason }),
            Ran::NoOutputFile { found, stderr } => Err(FolderFailure::NoOutputFile {
                found: u32::try_from(found).unwrap_or(u32::MAX),
                stderr,
            }),
            Ran::Stopped => Err(FolderFailure::Stopped),
        }
    }

    /// Keep what a folder kind wrote into `out`, into a `.tmp-` folder of
    /// its leaf in the store, telling its progress as [`FolderRunState::Keeping`].
    async fn keep_output(
        self: &Arc<Self>,
        slot: &Slot,
        key: &str,
        (out, source): (&Path, Option<&str>),
        (copy_name, renderer, token): (&str, &str, &str),
        cancel: CancellationToken,
        stderr: String,
    ) -> Result<Output, FolderFailure> {
        let Some(dir) = self.store_dir(source) else {
            return Err(FolderFailure::Keep {
                reason: "there is no folder to keep generated files in".to_owned(),
            });
        };
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let leaf = store::leaf_name(copy_name, renderer, token);
        let into = dir.join(format!("{leaf}.tmp-{}-{n}", std::process::id()));
        if self.set_state(slot, key, FolderRunState::Keeping { done: 0, total: 0 }) {
            self.tell(slot, Some(FolderRunState::Keeping { done: 0, total: 0 }));
        }
        let last = Arc::new(Mutex::new(
            Instant::now() - std::time::Duration::from_secs(1),
        ));
        let (runs, told, k) = (Arc::clone(self), slot.clone(), key.to_owned());
        let progress: keep::Progress = Arc::new(move |done, total| {
            let state = FolderRunState::Keeping { done, total };
            if !runs.set_state(&told, &k, state.clone()) {
                return;
            }
            let mut at = last.lock().unwrap_or_else(PoisonError::into_inner);
            if done == total || at.elapsed().as_millis() >= PROGRESS_EVERY_MS {
                *at = Instant::now();
                runs.tell(&told, Some(state));
            }
        });
        let out = out.to_path_buf();
        let kept = tokio::task::spawn_blocking(move || keep::keep(&out, &into, &progress, &cancel))
            .await
            .map_err(|e| FolderFailure::Keep {
                reason: e.to_string(),
            })?;
        match kept {
            Ok(made) => Ok(Output::Folder(made)),
            Err(KeepError::Nothing) => Err(FolderFailure::NoOutput { stderr }),
            Err(KeepError::TooBig { files, bytes }) => Err(FolderFailure::OutputTooBig {
                files,
                bytes,
                cap_files: keep::OUT_FILES as u64,
                cap_bytes: keep::OUT_BYTES,
            }),
            Err(KeepError::Stopped) => Err(FolderFailure::Stopped),
            Err(KeepError::Io(reason)) => Err(FolderFailure::Keep { reason }),
        }
    }

    /// The bytes a token names: a single-output drawing's only.
    #[must_use]
    pub fn output(&self, token: &str) -> Option<Arc<Vec<u8>>> {
        let state = self.lock();
        let key = state.tokens.get(token)?;
        match &state.memory.get(key)?.body {
            Body::Bytes(bytes) => Some(bytes.clone()),
            Body::Folder(_) => None,
        }
    }

    /// The generated folder a token names, while memory holds it. A token
    /// it no longer holds is read back by a `look` (ADR-0046).
    #[must_use]
    pub fn made(&self, token: &str) -> Option<Arc<Made>> {
        let state = self.lock();
        let key = state.tokens.get(token)?;
        match &state.memory.get(key)?.body {
            Body::Folder(made) => Some(made.clone()),
            Body::Bytes(_) => None,
        }
    }

    /// Keep a sanitized HTML document for the frame and answer the token it
    /// is served by (`knobas-render://localhost/folder.<token>`). The last
    /// eight are kept.
    pub fn frame(&self, document: Vec<u8>) -> String {
        let token = uuid::Uuid::new_v4().simple().to_string();
        let mut state = self.lock();
        state.frames.push_back((token.clone(), Arc::new(document)));
        while state.frames.len() > 8 {
            state.frames.pop_front();
        }
        token
    }

    /// The document a frame token names.
    #[must_use]
    pub fn framed(&self, token: &str) -> Option<Arc<Vec<u8>>> {
        self.lock()
            .frames
            .iter()
            .find(|(t, _)| t == token)
            .map(|(_, doc)| doc.clone())
    }

    /// Forget every drawing of `source` in memory: its folder on disk is
    /// [`forget_source`]'s.
    pub fn forget_memory(&self) {
        let mut state = self.lock();
        state.memory.clear();
        state.used.clear();
        state.memory_bytes = 0;
        state.memory_folders = 0;
        state.touched.clear();
        state.tokens.clear();
        state.latest.clear();
        state.failures.clear();
    }

    /// Remove `source`'s drawings: its folder on disk, and memory (which is
    /// not kept by source, so all of it: a rare event, and memory refills).
    pub fn forget_source(&self, source_id: &str) {
        self.forget_memory();
        if let Some(dir) = self.disk_dir(Some(source_id)) {
            let _ = std::fs::remove_dir_all(dir);
        }
        if let Some(dir) = self.store_dir(Some(source_id)) {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

/// What a run made: a single output's bytes, or a generated folder kept in
/// its `.tmp-` place.
enum Output {
    Bytes(Vec<u8>, DrawnKind),
    Folder(Made),
}

fn resolved_parts(resolved: &Resolved) -> (String, String) {
    (resolved.key.clone(), resolved.copy_name.clone())
}

fn running_now(going: &Going) -> FolderRunState {
    match &going.state {
        FolderRunState::Running { .. } => FolderRunState::Running {
            elapsed_ms: u64::try_from(going.started.elapsed().as_millis()).unwrap_or(u64::MAX),
        },
        other => other.clone(),
    }
}

fn touch(used: &mut VecDeque<String>, key: &str) {
    if let Some(at) = used.iter().position(|k| k == key) {
        used.remove(at);
    }
    used.push_back(key.to_owned());
}

/// A source's folder on disk: its id hashed, so any id is a plain name.
fn source_dir(source_id: &str) -> String {
    use sha2::Digest as _;
    hex(&sha2::Sha256::digest(source_id.as_bytes())[..12])
}

fn slot_file(slot: &Slot) -> String {
    use sha2::Digest as _;
    let mut hash = sha2::Sha256::new();
    for part in [&slot.entity_id, &slot.renderer, &slot.folder] {
        hash.update(part.as_bytes());
        hash.update([0]);
    }
    hex(&hash.finalize()[..12])
}

fn write_disk(dir: &Path, key: &str, slot_name: &str, kept: &Kept) {
    let Body::Bytes(bytes) = &kept.body else {
        return;
    };
    let Ok(meta) = serde_json::to_vec(&kept.drawn) else {
        return;
    };
    let slots = dir.join("slots");
    if std::fs::create_dir_all(&slots).is_err() {
        return;
    }
    // The bytes first and the description last, so a half-written pair is
    // never read: a key is read by its `.json`.
    let _ = std::fs::write(dir.join(format!("{key}.out")), bytes.as_slice())
        .and_then(|()| std::fs::write(dir.join(format!("{key}.json")), meta))
        .and_then(|()| std::fs::write(slots.join(slot_name), key));
}

/// Drop the oldest drawings under `root` until it holds at most `bound`
/// bytes.
pub fn bound_disk(root: &Path, bound: u64) {
    let mut kept: Vec<(std::time::SystemTime, u64, PathBuf)> = Vec::new();
    for source in std::fs::read_dir(root).into_iter().flatten().flatten() {
        for file in std::fs::read_dir(source.path())
            .into_iter()
            .flatten()
            .flatten()
        {
            let path = file.path();
            if path.extension().is_some_and(|e| e == "out")
                && let Ok(meta) = file.metadata()
            {
                kept.push((
                    meta.modified().unwrap_or(std::time::UNIX_EPOCH),
                    meta.len(),
                    path,
                ));
            }
        }
    }
    let mut total: u64 = kept.iter().map(|(_, len, _)| len).sum();
    kept.sort();
    for (_, len, path) in kept {
        if total <= bound {
            break;
        }
        let _ = std::fs::remove_file(path.with_extension("json"));
        let _ = std::fs::remove_file(&path);
        total = total.saturating_sub(len);
    }
}

// -- the processes running, killed at quit -----------------------------------

static RUNNING: LazyLock<Mutex<HashSet<u32>>> = LazyLock::new(Mutex::default);

/// A tool started: its pid is held until it ends, so a quit can kill it.
pub fn started(pid: u32) {
    RUNNING
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(pid);
}

/// A tool ended.
pub fn ended(pid: u32) {
    RUNNING
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .remove(&pid);
}

/// Kill every tool still running, with what it started: the app is quitting,
/// and a dropped child is only killed itself (`kill_on_drop`).
pub fn kill_all() {
    let pids: Vec<u32> = RUNNING
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .drain()
        .collect();
    for pid in pids {
        #[cfg(unix)]
        if let Some(pid) = i32::try_from(pid)
            .ok()
            .and_then(rustix::process::Pid::from_raw)
        {
            let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
        }
        #[cfg(windows)]
        {
            let _ = knobas_db::console::command("taskkill")
                .args(["/T", "/F", "/PID", &pid.to_string()])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::folder_render::{FolderOutput, FolderRenderer};
    use std::collections::HashMap;
    use std::time::Duration;

    struct Fixed(HashMap<String, Vec<u8>>);

    #[async_trait::async_trait]
    impl Bytes for Fixed {
        async fn read(&self, item: &Item) -> Result<Vec<u8>, String> {
            self.0
                .get(&item.rel)
                .cloned()
                .ok_or_else(|| "not there".to_owned())
        }
    }

    /// A scratch place of its own per test: the runs' root, a script folder
    /// and a disk cache.
    struct Place {
        dir: PathBuf,
    }

    impl Place {
        fn new(name: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("knobas-fr-runs-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(dir.join("bin")).unwrap();
            Self { dir }
        }

        /// A script at `bin/<name>`, run by `sh`.
        fn script(&self, name: &str, body: &str) -> String {
            let path = self.dir.join("bin").join(name);
            std::fs::write(&path, body).unwrap();
            path.to_string_lossy().into_owned()
        }

        fn runs(&self) -> Arc<Runs> {
            let runs = Arc::new(Runs::new(self.dir.join("root")));
            runs.keep_on_disk(self.dir.join("disk"));
            runs.set_store(self.store());
            runs
        }

        fn store(&self) -> PathBuf {
            self.dir.join("clones").join(store::STORE_NAME)
        }

        /// The leaves kept under the store, by name.
        fn leaves(&self) -> Vec<String> {
            let mut found = Vec::new();
            for source in std::fs::read_dir(self.store())
                .into_iter()
                .flatten()
                .flatten()
            {
                if source.file_name().to_string_lossy().starts_with('.') {
                    continue;
                }
                for entry in std::fs::read_dir(source.path())
                    .into_iter()
                    .flatten()
                    .flatten()
                {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if entry.path().is_dir() && name != "keys" && name != "slots" {
                        found.push(name);
                    }
                }
            }
            found.sort();
            found
        }
    }

    impl Drop for Place {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn renderer(command: String, output: FolderOutput, timeout_secs: u32) -> FolderRenderer {
        FolderRenderer {
            id: "lister".to_owned(),
            name: "Lister".to_owned(),
            command,
            holds: vec![],
            folders: vec![],
            output,
            timeout_secs,
        }
    }

    fn slot() -> Slot {
        Slot {
            entity_id: "gitea:repo:o/r".to_owned(),
            renderer: "lister".to_owned(),
            folder: "flows/app".to_owned(),
        }
    }

    fn file(rel: &str, blob: &str) -> Item {
        Item {
            rel: rel.to_owned(),
            kind: copy::ItemKind::File,
            size: 1,
            executable: false,
            blob: Some(blob.to_owned()),
        }
    }

    fn resolved(r: &FolderRenderer, files: &[(&str, &str)]) -> Resolved {
        let items: Vec<Item> = files
            .iter()
            .map(|(rel, text)| file(rel, &format!("b-{text}")))
            .collect();
        Resolved {
            key: clone_key(r, "app", &items),
            copy_name: "app".to_owned(),
            items,
            bytes: Arc::new(Fixed(
                files
                    .iter()
                    .map(|(rel, text)| ((*rel).to_owned(), text.as_bytes().to_vec()))
                    .collect(),
            )),
            from: ReadFrom::Clone,
            commit: Some("a".repeat(40)),
            source_id: Some("gitea-main".to_owned()),
        }
    }

    /// Ask until the slot runs no more, as the page does on each event.
    async fn settled(
        runs: &Arc<Runs>,
        r: &FolderRenderer,
        files: &[(&str, &str)],
    ) -> FolderRenderView {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let view = runs.view(&slot(), r, Ok(resolved(r, files)), Ask::Look);
            if view.running.is_none() || Instant::now() > deadline {
                return view;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    fn text_of(runs: &Runs, view: &FolderRenderView) -> String {
        let drawn = view.drawn.as_ref().expect("a drawing");
        String::from_utf8(runs.output(&drawn.token).expect("its bytes").to_vec()).unwrap()
    }

    /// A tool that prints: the copy holds the folder under its own name, the
    /// tool starts in an empty folder of its own, stdout is the drawing as
    /// text; a second open draws it at once and starts nothing.
    #[tokio::test]
    async fn a_printing_tool_draws_and_is_kept() {
        let place = Place::new("print");
        let script = place.script(
            "list.sh",
            "empty=$(ls -A | wc -l | tr -d ' '); cd \"$1\" && echo \"name=$(basename \"$1\") cwd-empty=$empty\" && find . -type f | sort && cat a.txt\n",
        );
        let r = renderer(format!("sh {script} {{path}} ."), FolderOutput::Detect, 10);
        let runs = place.runs();
        let files = [("a.txt", "A"), ("sub/b.txt", "B")];
        let first = runs.view(&slot(), &r, Ok(resolved(&r, &files)), Ask::Open);
        assert_eq!(first.running, Some(FolderRunState::Queued));
        let view = settled(&runs, &r, &files).await;
        assert!(view.current, "{view:?}");
        let drawn = view.drawn.clone().unwrap();
        assert_eq!(drawn.kind, DrawnKind::Text);
        assert_eq!(drawn.from, ReadFrom::Clone);
        assert_eq!(
            text_of(&runs, &view),
            "name=app cwd-empty=0\n./a.txt\n./sub/b.txt\nA"
        );
        // The run's folders are gone.
        assert!(
            std::fs::read_dir(place.dir.join("root"))
                .unwrap()
                .next()
                .is_none()
        );
        // Open again: kept, nothing runs.
        let again = runs.view(&slot(), &r, Ok(resolved(&r, &files)), Ask::Open);
        assert_eq!(again.running, None);
        assert_eq!(again.drawn.unwrap().token, drawn.token);
    }

    /// A tool that writes `{out}.png`: the drawing is that file, an image
    /// told by its bytes, whatever the tool printed.
    #[tokio::test]
    async fn a_tool_that_writes_its_file_draws_the_file() {
        let place = Place::new("out");
        let script = place.script(
            "png.sh",
            "printf 'chatter'; printf '\\211PNG\\r\\n\\032\\nrest' > \"$2\"\n",
        );
        let r = renderer(
            format!("sh {script} {{path}} {{out}}.png"),
            FolderOutput::Detect,
            10,
        );
        let runs = place.runs();
        let files = [("a.txt", "A")];
        runs.view(&slot(), &r, Ok(resolved(&r, &files)), Ask::Open);
        let view = settled(&runs, &r, &files).await;
        let drawn = view.drawn.clone().expect("drawn");
        assert_eq!(
            drawn.kind,
            DrawnKind::Image {
                mime: "image/png".to_owned()
            }
        );
        let bytes = runs.output(&drawn.token).unwrap();
        assert!(bytes.starts_with(b"\x89PNG"), "{bytes:?}");

        // Exit 0 and no file under `{out}`: said so, with what it printed on stderr.
        let none = place.script("none.sh", "echo forgot >&2\n");
        let r = renderer(
            format!("sh {none} {{path}} {{out}}"),
            FolderOutput::Detect,
            10,
        );
        runs.view(&slot(), &r, Ok(resolved(&r, &files)), Ask::Open);
        let view = settled(&runs, &r, &files).await;
        assert_eq!(
            view.failure,
            Some(FolderFailure::NoOutputFile {
                found: 0,
                stderr: "forgot\n".to_owned()
            })
        );
    }

    /// A failing tool: its status and stderr are the answer, kept for the
    /// session (a return shows it and runs nothing), and *Run again* runs.
    #[tokio::test]
    async fn a_failure_is_shown_and_kept_until_run_again() {
        let place = Place::new("fail");
        let counter = place.dir.join("count");
        let script = place.script(
            "fail.sh",
            "echo x >> \"$2\"; echo 'no project file' >&2; exit 3\n",
        );
        let r = renderer(
            format!("sh {script} {{path}} {}", counter.display()),
            FolderOutput::Detect,
            10,
        );
        let runs = place.runs();
        let files = [("a.txt", "A")];
        runs.view(&slot(), &r, Ok(resolved(&r, &files)), Ask::Open);
        let view = settled(&runs, &r, &files).await;
        assert_eq!(
            view.failure,
            Some(FolderFailure::Failed {
                status: Some(3),
                stderr: "no project file\n".to_owned()
            })
        );
        let back = runs.view(&slot(), &r, Ok(resolved(&r, &files)), Ask::Open);
        assert!(back.running.is_none() && back.failure.is_some(), "{back:?}");
        assert_eq!(
            std::fs::read_to_string(&counter).unwrap().lines().count(),
            1
        );
        let again = runs.view(&slot(), &r, Ok(resolved(&r, &files)), Ask::Again);
        assert_eq!(again.running, Some(FolderRunState::Queued));
        settled(&runs, &r, &files).await;
        assert_eq!(
            std::fs::read_to_string(&counter).unwrap().lines().count(),
            2
        );
    }

    /// A tool past its timeout is stopped and says so; a stop from the page
    /// ends a run at once and leaves no failure that keeps it from running.
    #[tokio::test]
    async fn a_timeout_and_a_stop_end_the_run() {
        let place = Place::new("slow");
        let script = place.script("slow.sh", "echo started >&2; sleep 30\n");
        let r = renderer(format!("sh {script} {{path}}"), FolderOutput::Detect, 1);
        let runs = place.runs();
        let files = [("a.txt", "A")];
        let started = Instant::now();
        runs.view(&slot(), &r, Ok(resolved(&r, &files)), Ask::Open);
        let view = settled(&runs, &r, &files).await;
        assert!(started.elapsed() < Duration::from_secs(10));
        assert_eq!(
            view.failure,
            Some(FolderFailure::TimedOut {
                seconds: 1,
                stderr: "started\n".to_owned()
            })
        );

        let r = renderer(
            format!("sh {script} {{path}} again"),
            FolderOutput::Detect,
            60,
        );
        runs.view(&slot(), &r, Ok(resolved(&r, &files)), Ask::Open);
        // Running, then stopped.
        let deadline = Instant::now() + Duration::from_secs(10);
        while !matches!(
            runs.view(&slot(), &r, Ok(resolved(&r, &files)), Ask::Look)
                .running,
            Some(FolderRunState::Running { .. })
        ) {
            assert!(Instant::now() < deadline, "the tool never ran");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let stopping = Instant::now();
        runs.stop(&slot());
        let view = runs.view(&slot(), &r, Ok(resolved(&r, &files)), Ask::Look);
        assert!(view.running.is_none() && view.failure.is_none(), "{view:?}");
        assert!(stopping.elapsed() < Duration::from_secs(2));
        // Open again: it runs again.
        let reopened = runs.view(&slot(), &r, Ok(resolved(&r, &files)), Ask::Open);
        assert_eq!(reopened.running, Some(FolderRunState::Queued));
        runs.stop(&slot());
    }

    /// Two asks for the folder as it runs join one run; the folder changed
    /// meanwhile replaces it, and the last drawing is shown, not current,
    /// while the new state runs.
    #[tokio::test]
    async fn one_run_per_folder_and_the_last_drawing_while_it_changes() {
        let place = Place::new("join");
        let counter = place.dir.join("count");
        let script = place.script(
            "count.sh",
            "echo x >> \"$2\"; sleep 0.3; cat \"$1/a.txt\"\n",
        );
        let r = renderer(
            format!("sh {script} {{path}} {}", counter.display()),
            FolderOutput::Text,
            10,
        );
        let runs = place.runs();
        let v1 = [("a.txt", "one")];
        runs.view(&slot(), &r, Ok(resolved(&r, &v1)), Ask::Open);
        let joined = runs.view(&slot(), &r, Ok(resolved(&r, &v1)), Ask::Open);
        assert!(joined.running.is_some());
        let first = settled(&runs, &r, &v1).await;
        assert_eq!(text_of(&runs, &first), "one");
        assert_eq!(
            std::fs::read_to_string(&counter).unwrap().lines().count(),
            1
        );

        let v2 = [("a.txt", "two")];
        let changed = runs.view(&slot(), &r, Ok(resolved(&r, &v2)), Ask::Open);
        assert!(!changed.current);
        assert_eq!(
            changed.drawn.as_ref().map(|d| d.token.clone()),
            first.drawn.as_ref().map(|d| d.token.clone())
        );
        assert!(changed.running.is_some());
        let second = settled(&runs, &r, &v2).await;
        assert!(second.current);
        assert_eq!(text_of(&runs, &second), "two");
        // A look at the earlier state is still kept, and is now the last one
        // looked at: a third state shows it while it waits.
        assert!(
            runs.view(&slot(), &r, Ok(resolved(&r, &v1)), Ask::Look)
                .current
        );
        let v3 = [("a.txt", "three")];
        let third = runs.view(&slot(), &r, Ok(resolved(&r, &v3)), Ask::Look);
        assert!(!third.current);
        assert_eq!(
            third.drawn.map(|d| d.token),
            first.drawn.map(|d| d.token),
            "the drawing last looked at"
        );
    }

    /// What was drawn is kept on disk: a fresh app draws it at once, and the
    /// source's removal takes it away.
    #[tokio::test]
    async fn a_drawing_outlives_a_restart_and_goes_with_its_source() {
        let place = Place::new("disk");
        let script = place.script("p.sh", "cat \"$1/a.txt\"\n");
        let r = renderer(format!("sh {script} {{path}}"), FolderOutput::Text, 10);
        let files = [("a.txt", "kept")];
        {
            let runs = place.runs();
            runs.view(&slot(), &r, Ok(resolved(&r, &files)), Ask::Open);
            settled(&runs, &r, &files).await;
        }
        let fresh = place.runs();
        let view = fresh.view(&slot(), &r, Ok(resolved(&r, &files)), Ask::Look);
        assert!(view.current, "{view:?}");
        assert_eq!(text_of(&fresh, &view), "kept");
        fresh.forget_source("gitea-main");
        let after = place.runs();
        let gone = after.view(&slot(), &r, Ok(resolved(&r, &files)), Ask::Look);
        assert!(gone.drawn.is_none(), "{gone:?}");
    }

    /// The disk is held to its bound, the oldest drawing first.
    #[test]
    fn the_disk_is_bounded_oldest_first() {
        let place = Place::new("bound");
        let source = place.dir.join("disk").join("s");
        std::fs::create_dir_all(&source).unwrap();
        for (n, name) in ["old", "mid", "new"].iter().enumerate() {
            std::fs::write(source.join(format!("{name}.out")), vec![0_u8; 100]).unwrap();
            std::fs::write(source.join(format!("{name}.json")), b"{}").unwrap();
            let at = std::time::SystemTime::now() - Duration::from_secs(100 - n as u64 * 10);
            std::fs::File::options()
                .write(true)
                .open(source.join(format!("{name}.out")))
                .unwrap()
                .set_modified(at)
                .unwrap();
        }
        bound_disk(&place.dir.join("disk"), 200);
        assert!(!source.join("old.out").exists() && !source.join("old.json").exists());
        assert!(source.join("mid.out").exists() && source.join("new.out").exists());
    }

    fn folder_kind(command: String) -> FolderRenderer {
        FolderRenderer {
            id: "site".to_owned(),
            ..renderer(command, FolderOutput::Folder, 10)
        }
    }

    fn made_of(runs: &Runs, view: &FolderRenderView) -> Arc<Made> {
        runs.made(&view.drawn.as_ref().expect("a drawing").token)
            .expect("a generated folder")
    }

    /// A tool that writes a folder of files: they are kept under the store,
    /// read back by the manifest, the staging is gone, and the bytes are not
    /// `folder_render_output`'s.
    #[tokio::test]
    async fn a_folder_tool_writes_under_the_store_and_its_staging_goes() {
        let place = Place::new("folder");
        let script = place.script(
            "site.sh",
            "case \"$2\" in *.staging/*) ;; *) echo \"not staged: $2\" >&2; exit 9;; esac\n\
             mkdir -p \"$2/img\" && cat \"$1/a.txt\" > \"$2/index.md\" && printf png > \"$2/img/a.png\"\n\
             ln -s /etc/hosts \"$2/hosts\"\n",
        );
        let r = folder_kind(format!("sh {script} {{path}} {{outdir}}"));
        let runs = place.runs();
        let files = [("a.txt", "# made")];
        runs.view(&slot(), &r, Ok(resolved(&r, &files)), Ask::Open);
        let view = settled(&runs, &r, &files).await;
        assert!(view.current, "{view:?}");
        let drawn = view.drawn.clone().unwrap();
        assert_eq!(
            drawn.kind,
            DrawnKind::Folder {
                files: 2,
                bytes: 9,
                left_out: 1
            }
        );
        assert!(runs.output(&drawn.token).is_none());
        let made = made_of(&runs, &view);
        assert_eq!(keep::read(&made, "index.md", 100).unwrap(), b"# made");
        assert!(made.dir.starts_with(place.store()));
        assert_eq!(place.leaves(), [format!("app.site.{}", &drawn.token[..8])]);
        let staging = place.store().join(store::STAGING);
        assert!(
            std::fs::read_dir(&staging).unwrap().next().is_none(),
            "the staging is empty"
        );
    }

    /// A tool that writes and then fails keeps nothing, and the folder kept
    /// before is still the answer.
    #[tokio::test]
    async fn a_tool_that_fails_keeps_nothing_of_its_folder() {
        let place = Place::new("folder-fail");
        let script = place.script(
            "site.sh",
            "printf a > \"$2/a.md\"; printf b > \"$2/b.md\"; [ \"$(cat \"$1/a.txt\")\" = good ] || exit 3\n",
        );
        let r = folder_kind(format!("sh {script} {{path}} {{outdir}}"));
        let runs = place.runs();
        let good = [("a.txt", "good")];
        runs.view(&slot(), &r, Ok(resolved(&r, &good)), Ask::Open);
        let first = settled(&runs, &r, &good).await;
        let kept = place.leaves();
        assert_eq!(kept.len(), 1);
        let bad = [("a.txt", "bad")];
        runs.view(&slot(), &r, Ok(resolved(&r, &bad)), Ask::Open);
        let failed = settled(&runs, &r, &bad).await;
        assert_eq!(
            failed.failure,
            Some(FolderFailure::Failed {
                status: Some(3),
                stderr: String::new()
            })
        );
        assert_eq!(place.leaves(), kept, "no leaf of the failed run");
        assert_eq!(
            failed.drawn.map(|d| d.token),
            first.drawn.map(|d| d.token),
            "the last kept folder is still answered"
        );
        assert!(!failed.current);
    }

    /// Exit 0 and nothing kept is no output, with what the tool said.
    #[tokio::test]
    async fn a_tool_that_writes_nothing_is_no_output() {
        let place = Place::new("folder-none");
        let script = place.script("none.sh", "echo forgot >&2; ln -s /etc \"$2/etc\"\n");
        let r = folder_kind(format!("sh {script} {{path}} {{outdir}}"));
        let runs = place.runs();
        let files = [("a.txt", "A")];
        runs.view(&slot(), &r, Ok(resolved(&r, &files)), Ask::Open);
        let view = settled(&runs, &r, &files).await;
        assert_eq!(
            view.failure,
            Some(FolderFailure::NoOutput {
                stderr: "forgot\n".to_owned()
            })
        );
        assert!(place.leaves().is_empty());
    }

    /// *Run again* of one key writes a folder of its own, and the old one goes.
    #[tokio::test]
    async fn a_second_run_of_one_key_gets_a_folder_of_its_own() {
        let place = Place::new("folder-again");
        let script = place.script("site.sh", "date +%s%N > \"$2/at.txt\"\n");
        let r = folder_kind(format!("sh {script} {{path}} {{outdir}}"));
        let runs = place.runs();
        let files = [("a.txt", "A")];
        runs.view(&slot(), &r, Ok(resolved(&r, &files)), Ask::Open);
        let first = settled(&runs, &r, &files).await;
        let first_dir = made_of(&runs, &first).dir.clone();
        runs.view(&slot(), &r, Ok(resolved(&r, &files)), Ask::Again);
        let second = settled(&runs, &r, &files).await;
        let second_dir = made_of(&runs, &second).dir.clone();
        assert_ne!(first_dir, second_dir);
        assert!(!first_dir.exists(), "the old leaf of the key goes");
        assert_eq!(place.leaves().len(), 1);
        let old_token = first.drawn.unwrap().token;
        assert!(runs.made(&old_token).is_none());
    }

    /// A generated folder is read back from the store by a fresh app, under
    /// the same token; the same content draws at once with no run; the
    /// source's removal takes it away.
    #[tokio::test]
    async fn a_generated_folder_outlives_a_restart_and_goes_with_its_source() {
        let place = Place::new("folder-disk");
        let counter = place.dir.join("count");
        let script = place.script(
            "site.sh",
            "echo x >> \"$3\"; cp \"$1/a.txt\" \"$2/index.md\"\n",
        );
        let r = folder_kind(format!(
            "sh {script} {{path}} {{outdir}} {}",
            counter.display()
        ));
        let files = [("a.txt", "kept")];
        let token = {
            let runs = place.runs();
            runs.view(&slot(), &r, Ok(resolved(&r, &files)), Ask::Open);
            settled(&runs, &r, &files).await.drawn.unwrap().token
        };
        let fresh = place.runs();
        assert!(fresh.made(&token).is_none(), "not in memory yet");
        let view = fresh.view(&slot(), &r, Ok(resolved(&r, &files)), Ask::Open);
        assert!(view.current && view.running.is_none(), "{view:?}");
        assert_eq!(view.drawn.as_ref().unwrap().token, token);
        assert_eq!(
            keep::read(&made_of(&fresh, &view), "index.md", 100).unwrap(),
            b"kept"
        );
        assert_eq!(
            std::fs::read_to_string(&counter).unwrap().lines().count(),
            1,
            "drawn at once, no second run"
        );
        fresh.forget_source("gitea-main");
        assert!(place.leaves().is_empty());
        let after = place.runs();
        let gone = after.view(&slot(), &r, Ok(resolved(&r, &files)), Ask::Look);
        assert!(gone.drawn.is_none(), "{gone:?}");
    }

    /// The store moved: the old one is removed whole, and memory forgets.
    #[tokio::test]
    async fn a_moved_store_forgets_its_folders() {
        let place = Place::new("folder-move");
        let script = place.script("site.sh", "printf x > \"$2/a.md\"\n");
        let r = folder_kind(format!("sh {script} {{path}} {{outdir}}"));
        let runs = place.runs();
        let files = [("a.txt", "A")];
        runs.view(&slot(), &r, Ok(resolved(&r, &files)), Ask::Open);
        let view = settled(&runs, &r, &files).await;
        let token = view.drawn.unwrap().token;
        runs.set_store(place.dir.join("elsewhere").join(store::STORE_NAME));
        assert!(runs.made(&token).is_none());
        assert!(!place.store().exists());
        let again = runs.view(&slot(), &r, Ok(resolved(&r, &files)), Ask::Look);
        assert!(again.drawn.is_none());
    }

    /// A run waiting for a place is dropped when its page leaves; at most
    /// [`AT_ONCE`] tools run at once.
    #[tokio::test]
    async fn a_waiting_run_is_dropped_when_its_page_leaves() {
        let place = Place::new("queue");
        let script = place.script("hold.sh", "sleep 30\n");
        let r = renderer(format!("sh {script} {{path}}"), FolderOutput::Text, 60);
        let runs = place.runs();
        let files = [("a.txt", "A")];
        let slot_n = |n: usize| Slot {
            folder: format!("f{n}"),
            ..slot()
        };
        for n in 0..3 {
            runs.view(&slot_n(n), &r, Ok(resolved(&r, &files)), Ask::Open);
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
        let third = runs.view(&slot_n(2), &r, Ok(resolved(&r, &files)), Ask::Look);
        assert_eq!(third.running, Some(FolderRunState::Queued));
        runs.leave(&slot_n(2));
        assert!(
            runs.view(&slot_n(2), &r, Ok(resolved(&r, &files)), Ask::Look)
                .running
                .is_none()
        );
        // A going run is not dropped by a leave.
        runs.leave(&slot_n(0));
        assert!(
            runs.view(&slot_n(0), &r, Ok(resolved(&r, &files)), Ask::Look)
                .running
                .is_some()
        );
        runs.stop(&slot_n(0));
        runs.stop(&slot_n(1));
    }
}

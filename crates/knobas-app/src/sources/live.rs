//! **Live reads, kept cheap** (spec #2234, *Step 1*; ADR-0032): the built
//! source each live read is asked of, and the read cache in front of the
//! source view.
//!
//! # One source per source (#2240)
//!
//! A live read used to build its adapter afresh and ask the keychain for its
//! credential before it sent a single request. [`LiveSources::source`] builds
//! each configured source once and lends the same instance to every read
//! after. It is rebuilt when what it was built from changes: its
//! configuration row, read per call as before (kind, name, address,
//! authentication method, adapter config), or its credential, which the app
//! changes in one place each (*Re-enter*, *Remove*) and which calls
//! [`LiveSources::forget`]. A source whose row is gone is dropped and refused
//! as before.
//!
//! # The read cache (#2239)
//!
//! In memory only, never in the database or on disk. A view the adapter says
//! is [`Standing::Fixed`] before it is read (`Source::view_standing`: a file
//! at a whole commit sha) is kept for the session, keyed by the built
//! instance, the entity and the section, and the same ask again is answered
//! from here with no request to the source. A [`Standing::Live`] view is
//! read every time and kept nowhere. The cache is bounded by the bytes its
//! views take as JSON, and drops the least recently used first; a dropped
//! view is read again on demand. A rebuilt source keys its views anew, and
//! the old instance's are dropped with it: a new address may be another
//! server, a new credential may read less.
//!
//! # Moving reads (#2242) and grants (#2241)
//!
//! A view the adapter says is [`Standing::Moving`] -- keyed by a branch name,
//! a branch list, a tree by branch -- is kept the same way, and the same ask
//! again is answered from here at once while it is read again behind the
//! answer: one read per view at a time, whose answer replaces the kept one
//! for the next ask. A read behind that fails leaves the kept answer and
//! says nothing; one whose answer differs tells [`LiveSources::on_fresh`]'s
//! listener, which tells the shell (`view:fresh`), so a page showing it
//! draws it again. [`LiveSources::refused`] keeps a write's grants the same
//! way where the adapter says they move (`Source::refusal_standing`): Gitea's
//! probe is asked once per repository, not once per file opened.
//!
//! What is kept is the adapter's answer. The app's own work on it -- the
//! rule-7 check against the mirror, a row's commit hint -- is done on every
//! read, because the mirror moves under a fixed view.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use knobas_secrets::SecretStore;
use knobas_source::grants::{RefusedWrite, WriteAsk};
use knobas_source::view::{SourceView, Standing};
use knobas_source::{AuthMethod, Source, SourceError};
use knobas_sync::scheduler::AdapterRegistry;
use sqlx::PgPool;

use super::crud::{Credential, instance_from};
use super::{SecretInput, SourcesError};

/// How many bytes of views the cache holds by default: a few hundred files
/// at the view's own one-megabyte cap, far more of ordinary ones.
pub const DEFAULT_BOUND: usize = 64 * 1024 * 1024;

/// The built sources and the read cache, one per app (held by
/// [`SourcesState`](super::SourcesState)). Cheap to clone: clones share both.
#[derive(Clone)]
pub struct LiveSources(Arc<Inner>);

struct Inner {
    /// One slot per source id. The slot's own lock is held across a build,
    /// so two reads of one source do not both ask the keychain, and a slow
    /// keychain for one source holds up no other.
    built: Mutex<HashMap<String, Arc<tokio::sync::Mutex<Option<Built>>>>>,
    reads: Mutex<Reads>,
    /// Each build's number, which the views read through it are keyed by.
    builds: AtomicU64,
    /// Advanced by every [`LiveSources::forget`]. A build that saw it move
    /// since it read the configuration row is lent to its own read only.
    forgets: AtomicU64,
    /// The keys a read behind is under way for: one at a time each.
    behind: Mutex<HashSet<Key>>,
    /// How many reads behind are under way, and their end's signal.
    pending: AtomicUsize,
    idle: tokio::sync::Notify,
    /// Told of a read behind whose answer moved.
    fresh: Mutex<Option<Listener>>,
}

type Listener = Arc<dyn Fn(ViewFresh) + Send + Sync>;

/// The payload of the shell's `view:fresh` event (#2242): a view the shell
/// was answered from memory was read again behind, and its answer moved, so
/// a page showing it reads it again (from memory, now the fresh answer).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct ViewFresh {
    pub entity_id: String,
    pub section: Option<String>,
}

/// A source as built, and what it was built from.
struct Built {
    recipe: Recipe,
    generation: u64,
    source: Arc<dyn Source>,
}

/// What a built source was built from, short of its credential: the
/// configuration row's fields the build reads.
#[derive(Debug, PartialEq)]
struct Recipe {
    adapter_kind: String,
    display_name: String,
    base_url: String,
    method: Option<AuthMethod>,
    config: serde_json::Value,
}

/// A built source, lent to one read.
pub(crate) struct Lent {
    pub(crate) source: Arc<dyn Source>,
    generation: u64,
}

impl Default for LiveSources {
    fn default() -> Self {
        Self::with_bound(DEFAULT_BOUND)
    }
}

impl LiveSources {
    /// An empty set whose cache holds at most `bound` bytes of views.
    #[must_use]
    pub fn with_bound(bound: usize) -> Self {
        Self(Arc::new(Inner {
            built: Mutex::new(HashMap::new()),
            reads: Mutex::new(Reads::new(bound)),
            builds: AtomicU64::new(0),
            forgets: AtomicU64::new(0),
            behind: Mutex::new(HashSet::new()),
            pending: AtomicUsize::new(0),
            idle: tokio::sync::Notify::new(),
            fresh: Mutex::new(None),
        }))
    }

    /// Tell `listener` of every read behind whose answer moved: the shell's
    /// `view:fresh`, where the app wires it.
    pub fn on_fresh(&self, listener: impl Fn(ViewFresh) + Send + Sync + 'static) {
        *self.0.fresh.lock().expect("the listener") = Some(Arc::new(listener));
    }

    /// Once every read behind under way has ended: for a test that counts
    /// them, and asks again after.
    pub async fn settled(&self) {
        loop {
            let idle = self.0.idle.notified();
            if self.0.pending.load(Ordering::SeqCst) == 0 {
                return;
            }
            idle.await;
        }
    }

    /// The built source that owns `entity_id`, built the way a scheduled run
    /// builds it the first time it is asked for, and lent again after.
    ///
    /// The **source is the entity's namespace** (interfaces §4.1). The
    /// keychain is asked only where the configuration says a credential is
    /// needed at all, which is `build_source`'s rule: a source that
    /// authenticates nothing -- the mock, `--demo` against an empty keychain
    /// -- must not be turned into a keychain prompt by a detail opening.
    ///
    /// # Errors
    ///
    /// [`SourcesError::Invalid`] for an id that is not one,
    /// [`SourcesError::NotFound`] for a namespace no source is configured
    /// under, the keychain's and the build's failures.
    pub(crate) async fn source(
        &self,
        pool: &PgPool,
        secrets: &Arc<dyn SecretStore>,
        registry: &Arc<dyn AdapterRegistry>,
        entity_id: &str,
    ) -> Result<Lent, SourcesError> {
        let parsed = knobas_core::entity::EntityRef::parse(entity_id)
            .map_err(|error| SourcesError::Invalid(error.to_string()))?;
        let id = parsed.namespace;
        // Read before the row: a *Remove* that lands between the row read and
        // the slot below would otherwise let this read put an instance built
        // from the removed source (its row, and its credential or none) into
        // a fresh slot, where a source added again under the id with the same
        // row would be lent it.
        let forgets = self.0.forgets.load(Ordering::SeqCst);
        let Some(cfg) = knobas_sync::config::get(pool, &id).await? else {
            self.forget(&id);
            return Err(SourcesError::NotFound(id));
        };
        let recipe = Recipe {
            adapter_kind: cfg.adapter_kind,
            display_name: cfg.display_name,
            base_url: cfg.base_url,
            method: cfg.auth_kind.method(),
            config: cfg.config,
        };
        let slot = {
            let mut built = self.0.built.lock().expect("the built sources");
            Arc::clone(built.entry(id.clone()).or_default())
        };
        let mut slot = slot.lock().await;
        if let Some(built) = slot.as_ref()
            && built.recipe == recipe
        {
            return Ok(Lent {
                source: Arc::clone(&built.source),
                generation: built.generation,
            });
        }
        if let Some(stale) = slot.take() {
            self.drop_reads(stale.generation);
        }
        let stored = match recipe.method {
            None => None,
            Some(_) => {
                knobas_secrets::spawn::get(secrets, &knobas_secrets::KeychainAccount::source(&id))
                    .await?
            }
        };
        let source: Arc<dyn Source> = knobas_sync::scheduler::build_off_runtime(
            registry,
            instance_from(
                &id,
                &recipe.adapter_kind,
                &recipe.display_name,
                &recipe.base_url,
                recipe.method,
                Credential::keeping(&SecretInput::default(), stored),
                recipe.config.clone(),
            ),
        )
        .await?
        .into();
        let generation = self.0.builds.fetch_add(1, Ordering::Relaxed);
        if self.0.forgets.load(Ordering::SeqCst) != forgets {
            return Ok(Lent { source, generation });
        }
        *slot = Some(Built {
            recipe,
            generation,
            source: Arc::clone(&source),
        });
        Ok(Lent { source, generation })
    }

    /// Drop source `id`'s built instance and the views read through it: its
    /// credential changed or it was removed, so the next read builds it
    /// again and asks the keychain again.
    pub fn forget(&self, id: &str) {
        self.0.forgets.fetch_add(1, Ordering::SeqCst);
        let slot = self.0.built.lock().expect("the built sources").remove(id);
        // A slot a build holds right now is let go: whatever it builds is
        // lent to that read alone, and the next read starts a slot anew.
        if let Some(built) = slot.and_then(|slot| slot.try_lock().ok().and_then(|mut s| s.take())) {
            self.drop_reads(built.generation);
        }
    }

    /// The view of `entity` at `section`: from the cache where the adapter
    /// says the answer is fixed or moving and the cache holds it -- a moving
    /// one read again behind the answer -- and read live otherwise.
    ///
    /// # Errors
    ///
    /// The adapter's; a failed read is never kept.
    pub(crate) async fn view(
        &self,
        lent: &Lent,
        entity: &str,
        section: Option<&str>,
    ) -> Result<Option<SourceView>, SourceError> {
        let standing = lent.source.view_standing(entity, section);
        let behind = match standing {
            Standing::Live => return lent.source.view(entity, section).await,
            Standing::Fixed => false,
            Standing::Moving => true,
        };
        let key = Key {
            generation: lent.generation,
            entity: entity.to_owned(),
            ask: Ask::View(section.map(str::to_owned)),
        };
        if let Some(Answer::View(kept)) = self.reads().get(&key) {
            if behind {
                self.read_behind(lent, vec![key]);
            }
            return Ok(kept);
        }
        let view = lent.source.view(entity, section).await?;
        self.reads().put(key, Answer::View(view.clone()));
        Ok(view)
    }

    /// The asks among `asks` the source's credential cannot make, as
    /// `Source::refused_writes` answers: each ask the adapter says moves is
    /// answered from the cache where it holds it, and asked again behind;
    /// the rest are asked now, together.
    ///
    /// # Errors
    ///
    /// The adapter's; a failed ask is never kept.
    pub(crate) async fn refused(
        &self,
        lent: &Lent,
        asks: &[WriteAsk],
    ) -> Result<Vec<RefusedWrite>, SourceError> {
        let mut answered: Vec<RefusedWrite> = Vec::new();
        let mut kept_keys = Vec::new();
        let mut ask_now: Vec<WriteAsk> = Vec::new();
        for ask in asks {
            let key = match lent.source.refusal_standing(ask) {
                Standing::Live => {
                    ask_now.push(ask.clone());
                    continue;
                }
                Standing::Fixed | Standing::Moving => Key::of_ask(lent.generation, ask),
            };
            match self.reads().get(&key) {
                Some(Answer::Refused(kept)) => {
                    answered.extend(kept);
                    if lent.source.refusal_standing(ask) == Standing::Moving {
                        kept_keys.push(key);
                    }
                }
                _ => ask_now.push(ask.clone()),
            }
        }
        if !kept_keys.is_empty() {
            self.read_behind(lent, kept_keys);
        }
        if !ask_now.is_empty() {
            let fresh = lent.source.refused_writes(&ask_now).await?;
            let mut reads = self.reads();
            for ask in &ask_now {
                let refusal = fresh.iter().find(|r| r.refuses(ask)).cloned();
                if lent.source.refusal_standing(ask) != Standing::Live {
                    reads.put(
                        Key::of_ask(lent.generation, ask),
                        Answer::Refused(refusal.clone()),
                    );
                }
                answered.extend(refusal);
            }
        }
        Ok(asks
            .iter()
            .filter_map(|ask| answered.iter().find(|r| r.refuses(ask)).cloned())
            .collect())
    }

    /// Read `keys` again behind the answer just given from the cache, each
    /// unless a read of it is under way: a view's own read, or the writes'
    /// asks together, as they were asked. An answer replaces the kept one
    /// where the cache still holds it, and a moved view is told.
    fn read_behind(&self, lent: &Lent, keys: Vec<Key>) {
        let keys: Vec<Key> = {
            let mut behind = self.0.behind.lock().expect("the reads behind");
            keys.into_iter()
                .filter(|key| behind.insert(key.clone()))
                .collect()
        };
        if keys.is_empty() {
            return;
        }
        self.0.pending.fetch_add(1, Ordering::SeqCst);
        let this = self.clone();
        let source = Arc::clone(&lent.source);
        let work = async move {
            let mut views = Vec::new();
            let mut asks = Vec::new();
            for key in &keys {
                match &key.ask {
                    Ask::View(section) => views.push((key, section.clone())),
                    Ask::Write { op, field } => asks.push((
                        key,
                        WriteAsk {
                            entity: key.entity.clone(),
                            op: op.clone(),
                            field: field.clone(),
                        },
                    )),
                }
            }
            for (key, section) in views {
                match source.view(&key.entity, section.as_deref()).await {
                    Ok(view) => {
                        if this.reads().replace(key, Answer::View(view)) {
                            this.tell(ViewFresh {
                                entity_id: key.entity.clone(),
                                section,
                            });
                        }
                    }
                    Err(error) => {
                        tracing::debug!(entity = %key.entity, %error, "a read behind failed; the kept answer stands");
                    }
                }
            }
            if !asks.is_empty() {
                let of: Vec<WriteAsk> = asks.iter().map(|(_, ask)| ask.clone()).collect();
                match source.refused_writes(&of).await {
                    Ok(fresh) => {
                        let mut reads = this.reads();
                        for (key, ask) in &asks {
                            let refusal = fresh.iter().find(|r| r.refuses(ask)).cloned();
                            reads.replace(key, Answer::Refused(refusal));
                        }
                    }
                    Err(error) => {
                        tracing::debug!(%error, "grants asked behind failed; the kept answer stands");
                    }
                }
            }
            {
                let mut behind = this.0.behind.lock().expect("the reads behind");
                for key in &keys {
                    behind.remove(key);
                }
            }
            if this.0.pending.fetch_sub(1, Ordering::SeqCst) == 1 {
                this.0.idle.notify_waiters();
            }
        };
        // A test's request counter counts the read behind as the ask's own.
        #[cfg(feature = "test-util")]
        let work = knobas_http::counted::inherit(work);
        tokio::spawn(work);
    }

    fn tell(&self, fresh: ViewFresh) {
        let listener = self.0.fresh.lock().expect("the listener").clone();
        if let Some(listener) = listener {
            listener(fresh);
        }
    }

    fn reads(&self) -> std::sync::MutexGuard<'_, Reads> {
        self.0.reads.lock().expect("the read cache")
    }

    fn drop_reads(&self, generation: u64) {
        self.0
            .reads
            .lock()
            .expect("the read cache")
            .drop_generation(generation);
    }
}

/// What is kept, by the build it was read through and the entity.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Key {
    generation: u64,
    entity: String,
    ask: Ask,
}

/// A view's section, or a write's grants.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Ask {
    View(Option<String>),
    Write { op: String, field: Option<String> },
}

impl Key {
    fn of_ask(generation: u64, ask: &WriteAsk) -> Self {
        Self {
            generation,
            entity: ask.entity.clone(),
            ask: Ask::Write {
                op: ask.op.clone(),
                field: ask.field.clone(),
            },
        }
    }
}

/// A kept answer: a view, or whether one write is refused and why.
#[derive(Clone)]
enum Answer {
    View(Option<SourceView>),
    Refused(Option<RefusedWrite>),
}

impl Answer {
    /// Its JSON, the size it is measured by and what a moved one is told by.
    fn json(&self) -> Option<Vec<u8>> {
        match self {
            Self::View(view) => serde_json::to_vec(view).ok(),
            Self::Refused(refused) => Some(
                refused
                    .as_ref()
                    .map_or_else(Vec::new, |r| r.reason.clone().into_bytes()),
            ),
        }
    }
}

/// The kept views, least recently used first by `order`.
struct Reads {
    bound: usize,
    held: usize,
    /// Advanced on every use; an entry's last use is its place in `order`.
    clock: u64,
    entries: HashMap<Key, Kept>,
    order: BTreeMap<u64, Key>,
}

struct Kept {
    answer: Answer,
    bytes: usize,
    /// Its JSON's hash, against which a read behind is compared.
    digest: u64,
    used: u64,
}

fn digest(json: &[u8]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    json.hash(&mut hasher);
    hasher.finish()
}

impl Reads {
    fn new(bound: usize) -> Self {
        Self {
            bound,
            held: 0,
            clock: 0,
            entries: HashMap::new(),
            order: BTreeMap::new(),
        }
    }

    fn tick(&mut self) -> u64 {
        self.clock += 1;
        self.clock
    }

    fn get(&mut self, key: &Key) -> Option<Answer> {
        let now = self.tick();
        let kept = self.entries.get_mut(key)?;
        self.order.remove(&kept.used);
        kept.used = now;
        self.order.insert(now, key.clone());
        Some(kept.answer.clone())
    }

    /// Replace `key`'s answer where it is still kept (a forget or the bound
    /// may have dropped it since), and say whether the answer moved.
    fn replace(&mut self, key: &Key, answer: Answer) -> bool {
        let Some(before) = self.entries.get(key).map(|kept| kept.digest) else {
            return false;
        };
        let moved = answer.json().map(|json| digest(&json)) != Some(before);
        if moved {
            self.put(key.clone(), answer);
        }
        moved
    }

    fn put(&mut self, key: Key, answer: Answer) {
        // What the answer takes as the JSON the shell is sent: the measure of
        // its size the app already has, and close to what it holds.
        let Some(json) = answer.json() else {
            return;
        };
        let bytes = json.len();
        if bytes > self.bound {
            self.remove(&key);
            return;
        }
        self.remove(&key);
        let used = self.tick();
        self.held += bytes;
        self.order.insert(used, key.clone());
        self.entries.insert(
            key,
            Kept {
                answer,
                bytes,
                digest: digest(&json),
                used,
            },
        );
        while self.held > self.bound {
            let Some((_, oldest)) = self.order.pop_first() else {
                break;
            };
            if let Some(dropped) = self.entries.remove(&oldest) {
                self.held -= dropped.bytes;
            }
        }
    }

    fn remove(&mut self, key: &Key) {
        if let Some(kept) = self.entries.remove(key) {
            self.order.remove(&kept.used);
            self.held -= kept.bytes;
        }
    }

    fn drop_generation(&mut self, generation: u64) {
        let gone: Vec<Key> = self
            .entries
            .keys()
            .filter(|key| key.generation == generation)
            .cloned()
            .collect();
        for key in gone {
            self.remove(&key);
        }
    }
}

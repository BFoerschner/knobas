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
//! What is kept is the adapter's answer. The app's own work on it -- the
//! rule-7 check against the mirror, a row's commit hint -- is done on every
//! read, because the mirror moves under a fixed view.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use knobas_secrets::SecretStore;
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
        }))
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
        let slot = self.0.built.lock().expect("the built sources").remove(id);
        // A slot a build holds right now is let go: whatever it builds is
        // lent to that read alone, and the next read starts a slot anew.
        if let Some(built) = slot.and_then(|slot| slot.try_lock().ok().and_then(|mut s| s.take()))
        {
            self.drop_reads(built.generation);
        }
    }

    /// The view of `entity` at `section`, from the cache where the adapter
    /// says the answer is fixed and the cache holds it, read live otherwise.
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
        if lent.source.view_standing(entity, section) == Standing::Live {
            return lent.source.view(entity, section).await;
        }
        let key = (
            lent.generation,
            entity.to_owned(),
            section.map(str::to_owned),
        );
        if let Some(kept) = self.0.reads.lock().expect("the read cache").get(&key) {
            return Ok(kept);
        }
        let view = lent.source.view(entity, section).await?;
        self.0
            .reads
            .lock()
            .expect("the read cache")
            .put(key, view.clone());
        Ok(view)
    }

    fn drop_reads(&self, generation: u64) {
        self.0
            .reads
            .lock()
            .expect("the read cache")
            .drop_generation(generation);
    }
}

/// `(build, entity, section)`.
type Key = (u64, String, Option<String>);

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
    view: Option<SourceView>,
    bytes: usize,
    used: u64,
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

    fn get(&mut self, key: &Key) -> Option<Option<SourceView>> {
        let now = self.tick();
        let kept = self.entries.get_mut(key)?;
        self.order.remove(&kept.used);
        kept.used = now;
        self.order.insert(now, key.clone());
        Some(kept.view.clone())
    }

    fn put(&mut self, key: Key, view: Option<SourceView>) {
        // What the view takes as the JSON the shell is sent: the measure of
        // its size the app already has, and close to what it holds.
        let bytes = serde_json::to_vec(&view).map_or(usize::MAX, |json| json.len());
        if bytes > self.bound {
            return;
        }
        self.remove(&key);
        let used = self.tick();
        self.held += bytes;
        self.order.insert(used, key.clone());
        self.entries.insert(key, Kept { view, bytes, used });
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
            .filter(|(of, _, _)| *of == generation)
            .cloned()
            .collect();
        for key in gone {
            self.remove(&key);
        }
    }
}

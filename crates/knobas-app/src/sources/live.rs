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
//! In memory, never in the mirror's tables; a page's reads are also held
//! between sessions (below, ADR-0042). A view the adapter says
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
//! # Pages show what knobas last read (ADR-0042)
//!
//! A page's view ([`LiveSources::shown`]) that is not fixed -- moving, or
//! live -- is answered from what knobas last read whenever it holds one: the
//! cache above first, then the held reads kept between sessions
//! (`sync.held_read`, [`super::held`]), each with when it was read; and read
//! again behind the answer, as a moving view is. Only a view knobas has never
//! read waits for the source. A read behind that lands is held for the next
//! session too. The shell is told (`view:fresh`) when the answer moved, and
//! also when an answer older than [`HELD_AFTER`] was confirmed or could not
//! be, with the source's failure, so the page can say from when it is.
//!
//! # Bytes (#2256)
//!
//! A file's bytes ([`LiveSources::bytes`]: a README's image, a PDF) are kept
//! the same way where the adapter says the read is fixed
//! (`Source::bytes_standing`: at a whole sha), under the same bound,
//! measured by their length; a refusal over the cap is kept too, as the
//! nothing it holds. A byte read at a moving name is read every time.
//!
//! What is kept is the adapter's answer. The app's own work on it -- the
//! rule-7 check against the mirror, a row's commit hint -- is done on every
//! read, because the mirror moves under a fixed view.

use std::collections::{BTreeMap, HashMap};
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};

use knobas_secrets::SecretStore;
use knobas_source::grants::{RefusedWrite, WriteAsk};
use knobas_source::view::{FileBytes, SourceView, Standing};
use knobas_source::{AuthMethod, Source, SourceError};
use knobas_sync::scheduler::AdapterRegistry;
use sqlx::PgPool;

use super::crud::{Credential, instance_from};
use super::{SecretInput, SourcesError};

/// How many bytes of answers the cache holds by default: a few hundred files
/// at the view's own one-megabyte cap, far more of ordinary ones, beside one
/// byte read at its 50 MiB cap ([`knobas_source::view::BYTES_CAP`], #2256)
/// -- which a 64 MiB bound would have kept by dropping nearly every view.
pub const DEFAULT_BOUND: usize = 128 * 1024 * 1024;

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
    /// The keys a read behind is under way for: one at a time each, and
    /// whether it was asked for again meanwhile, so one more runs after it.
    behind: Mutex<HashMap<Key, bool>>,
    /// How many reads behind are under way, and their end's signal.
    pending: AtomicUsize,
    idle: tokio::sync::Notify,
    /// Told of a read behind whose answer moved.
    fresh: Mutex<Option<Listener>>,
    /// Where reads are held between sessions, once the app says
    /// ([`LiveSources::hold_in`]); none in a test that does not.
    store: Mutex<Option<PgPool>>,
    /// By build: the source's id and what it was built from short of its
    /// credential, which a held read is keyed by.
    recipes: Mutex<HashMap<u64, (String, serde_json::Value)>>,
}

/// How old an answer a page was given from what knobas held must be for the
/// shell to be told how its read behind went (ADR-0042): a younger one is the
/// read a moment ago, and says nothing.
pub const HELD_AFTER: chrono::TimeDelta = chrono::TimeDelta::seconds(30);

type Listener = Arc<dyn Fn(ViewFresh) + Send + Sync>;

/// A moving read, made again behind its answer: the source asked once more,
/// its answer as JSON ([`LiveSources::moving_read`]).
pub(crate) type ReadAgain = Arc<
    dyn Fn() -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<serde_json::Value, SourceError>> + Send>,
        > + Send
        + Sync,
>;

/// The payload of the shell's `view:fresh` event (#2242): a view the shell
/// was answered from memory was read again behind, and its answer moved, so
/// a page showing it reads it again (from memory, now the fresh answer).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct ViewFresh {
    pub entity_id: String,
    pub section: Option<String>,
    /// The source's failure, where the read behind an answer older than
    /// [`HELD_AFTER`] failed (ADR-0042): the page keeps what it drew, says
    /// from when it is and why it was not read now, and reads nothing again.
    /// Absent on every other telling.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub failed: Option<String>,
}

/// A page's view as [`LiveSources::shown`] answers it (ADR-0042): the view,
/// and when it was read where it is what knobas held rather than a read made
/// now. On the wire it is the view's own object with `held_at` beside its
/// fields, absent for a read made now.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[cfg_attr(feature = "test-util", derive(serde::Deserialize))]
#[ts(export_to = "entity.ts")]
pub struct ShownView {
    #[serde(flatten)]
    pub view: SourceView,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub held_at: Option<DateTime<Utc>>,
    /// The rows ADR-0020 rule 7 drew as plain text because the mirror has not
    /// synced their entity yet, each with the id its adapter named for it
    /// (the third smoothness pass of PR #2479): a page may *read* through it,
    /// as the repository page reads a branch made on the forge since the last
    /// sync, but never link to it, since no item page holds it. Absent on the
    /// wire when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(as = "Option<Vec<Unsynced>>", optional)]
    pub unsynced: Vec<Unsynced>,
}

/// One row of a [`ShownView`] whose entity the mirror does not hold yet: its
/// section, its title as drawn, and the id the adapter named for it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct Unsynced {
    pub section: String,
    pub title: String,
    pub entity_id: String,
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
            behind: Mutex::new(HashMap::new()),
            pending: AtomicUsize::new(0),
            idle: tokio::sync::Notify::new(),
            fresh: Mutex::new(None),
            store: Mutex::new(None),
            recipes: Mutex::new(HashMap::new()),
        }))
    }

    /// Tell `listener` of every read behind whose answer moved: the shell's
    /// `view:fresh`, where the app wires it.
    pub fn on_fresh(&self, listener: impl Fn(ViewFresh) + Send + Sync + 'static) {
        *self.0.fresh.lock().expect("the listener") = Some(Arc::new(listener));
    }

    /// Hold pages' reads between sessions in `pool`'s `sync.held_read`
    /// (ADR-0042): what the app does at bring-up.
    pub fn hold_in(&self, pool: PgPool) {
        *self.0.store.lock().expect("the store") = Some(pool);
    }

    fn store(&self) -> Option<PgPool> {
        self.0.store.lock().expect("the store").clone()
    }

    /// The source id and recipe build `generation` was made from.
    fn recipe_of(&self, generation: u64) -> Option<(String, serde_json::Value)> {
        self.0
            .recipes
            .lock()
            .expect("the recipes")
            .get(&generation)
            .cloned()
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
        self.0.recipes.lock().expect("the recipes").insert(
            generation,
            (
                id.clone(),
                serde_json::json!({
                    "adapter": recipe.adapter_kind,
                    "base_url": recipe.base_url,
                    "method": recipe.method,
                    "config": recipe.config,
                }),
            ),
        );
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
                self.read_behind(&lent.source, vec![key]);
            }
            return Ok(kept);
        }
        let view = lent.source.view(entity, section).await?;
        self.reads().put(key, Answer::View(view.clone()));
        Ok(view)
    }

    /// The view of `entity` at `section` a page draws (ADR-0042): from what
    /// knobas last read when it holds one -- this cache first, then the reads
    /// held between sessions -- with when it was read, and read again behind
    /// the answer; a fixed one is never read behind nor said to be from
    /// earlier. A view never read is drawn from the mirror's record where the
    /// adapter draws anything from it (`Source::mirror_view`), as of the sync
    /// that wrote it, and read behind. Only a view knobas holds nothing of is
    /// read now, and waited for; its answer is kept, and held.
    ///
    /// # Errors
    ///
    /// The adapter's, on a read made now; a failed read is never kept.
    pub(crate) async fn shown(
        &self,
        lent: &Lent,
        entity: &str,
        section: Option<&str>,
    ) -> Result<(Option<SourceView>, Option<DateTime<Utc>>), SourceError> {
        // A fixed view cannot move: kept and held like any other, never read
        // behind, and never said to be from earlier.
        let fixed = lent.source.view_standing(entity, section) == Standing::Fixed;
        let key = Key {
            generation: lent.generation,
            entity: entity.to_owned(),
            ask: Ask::View(section.map(str::to_owned)),
        };
        let kept = self.reads().get_at(&key);
        if let Some((Answer::View(kept), at)) = kept {
            if fixed {
                return Ok((kept, None));
            }
            self.read_behind(&lent.source, vec![key]);
            return Ok((kept, Some(at)));
        }
        let recipe = self.recipe_of(lent.generation);
        let store = self.store();
        if let (Some(pool), Some((id, recipe))) = (store.as_ref(), recipe.as_ref())
            && let Some((at, answer)) =
                super::held::get(pool, id, entity, &super::held::view_ask(section), recipe).await
            && let Ok(view) = serde_json::from_value::<Option<SourceView>>(answer)
        {
            self.reads()
                .put_at(key.clone(), Answer::View(view.clone()), at);
            if fixed {
                return Ok((view, None));
            }
            self.read_behind(&lent.source, vec![key]);
            return Ok((view, Some(at)));
        }
        // Never read: what the mirror's own record draws, where the adapter
        // draws anything from it, as of the sync that wrote it; never held.
        if !fixed
            && let Some(pool) = store.as_ref()
            && let Ok(Some((payload, synced_at))) =
                sqlx::query_as::<_, (serde_json::Value, DateTime<Utc>)>(
                    "select payload, synced_at from sync.item where entity_id = $1",
                )
                .bind(entity)
                .fetch_optional(pool)
                .await
            && let Some(view) = lent.source.mirror_view(entity, section, &payload)
        {
            self.reads()
                .put_at(key.clone(), Answer::View(Some(view.clone())), synced_at);
            self.read_behind(&lent.source, vec![key]);
            return Ok((Some(view), Some(synced_at)));
        }
        let view = lent.source.view(entity, section).await?;
        let read_at = Utc::now();
        self.reads().put(key.clone(), Answer::View(view.clone()));
        self.hold(&key, &view, read_at).await;
        Ok((view, None))
    }

    /// Hold `view`, read at `read_at` under `key`, for the next session,
    /// where the app holds reads at all.
    async fn hold(&self, key: &Key, view: &Option<SourceView>, read_at: DateTime<Utc>) {
        let Ask::View(section) = &key.ask else {
            return;
        };
        let (Some(pool), Some((id, recipe))) = (self.store(), self.recipe_of(key.generation))
        else {
            return;
        };
        let Ok(answer) = serde_json::to_value(view) else {
            return;
        };
        super::held::put(
            &pool,
            &id,
            &key.entity,
            &super::held::view_ask(section.as_deref()),
            &recipe,
            read_at,
            &answer,
        )
        .await;
    }

    /// A source read other than a view, named `ask` (`run:log`, an
    /// attachment's id), that cannot change once `fixed` says so -- a finished
    /// run's log -- answered from what knobas kept or held when it has it, with
    /// no request, and read with `read` otherwise, then kept and held
    /// (ADR-0042). Not fixed, it is read every time and kept nowhere.
    ///
    /// # Errors
    ///
    /// `read`'s; a failed read is never kept.
    pub(crate) async fn fixed_read<T, F>(
        &self,
        lent: &Lent,
        entity: &str,
        ask: &str,
        fixed: bool,
        read: F,
    ) -> Result<T, SourceError>
    where
        T: serde::Serialize + serde::de::DeserializeOwned,
        F: std::future::Future<Output = Result<T, SourceError>>,
    {
        if !fixed {
            return read.await;
        }
        let key = Key {
            generation: lent.generation,
            entity: entity.to_owned(),
            ask: Ask::Read(ask.to_owned()),
        };
        if let Some(Answer::Json(kept)) = self.reads().get(&key)
            && let Ok(answer) = serde_json::from_value::<T>((*kept).clone())
        {
            return Ok(answer);
        }
        let recipe = self.recipe_of(lent.generation);
        if let (Some(pool), Some((id, recipe))) = (self.store(), recipe.as_ref())
            && let Some((at, json)) = super::held::get(&pool, id, entity, ask, recipe).await
            && let Ok(answer) = serde_json::from_value::<T>(json.clone())
        {
            self.reads().put_at(key, Answer::Json(Arc::new(json)), at);
            return Ok(answer);
        }
        let answer = read.await?;
        if let Ok(json) = serde_json::to_value(&answer) {
            self.reads()
                .put(key.clone(), Answer::Json(Arc::new(json.clone())));
            self.hold_json(&key, ask, &json, Utc::now()).await;
        }
        Ok(answer)
    }

    /// A source read other than a view, named `ask`, that moves (a run's
    /// chain, a pipeline's settings): answered from what knobas kept or held
    /// when it has it, with when it was read, and read again behind with
    /// `read`, as a moving view is (ADR-0042). The shell is told on
    /// `view:fresh`, with `ask` as the section, as for a view. Held nothing of,
    /// it is read now and waited for.
    ///
    /// # Errors
    ///
    /// `read`'s, on a read made now; a failed read is never kept.
    pub(crate) async fn moving_read<T>(
        &self,
        lent: &Lent,
        entity: &str,
        ask: &str,
        read: ReadAgain,
    ) -> Result<(T, Option<DateTime<Utc>>), SourceError>
    where
        T: serde::de::DeserializeOwned,
    {
        let key = Key {
            generation: lent.generation,
            entity: entity.to_owned(),
            ask: Ask::Read(ask.to_owned()),
        };
        let kept = self.reads().get_at(&key);
        if let Some((Answer::Json(kept), at)) = kept
            && let Ok(answer) = serde_json::from_value::<T>((*kept).clone())
        {
            self.read_again_behind(key, read);
            return Ok((answer, Some(at)));
        }
        let recipe = self.recipe_of(lent.generation);
        if let (Some(pool), Some((id, recipe))) = (self.store(), recipe.as_ref())
            && let Some((at, json)) = super::held::get(&pool, id, entity, ask, recipe).await
            && let Ok(answer) = serde_json::from_value::<T>(json.clone())
        {
            self.reads()
                .put_at(key.clone(), Answer::Json(Arc::new(json)), at);
            self.read_again_behind(key, read);
            return Ok((answer, Some(at)));
        }
        let json = read().await?;
        let answer = serde_json::from_value::<T>(json.clone())
            .map_err(|error| SourceError::protocol(format!("{ask}: {error}")))?;
        self.reads()
            .put(key.clone(), Answer::Json(Arc::new(json.clone())));
        self.hold_json(&key, ask, &json, Utc::now()).await;
        Ok((answer, None))
    }

    /// [`Self::moving_read`]'s read behind: one per key at a time, as a
    /// view's; a moved answer, or one older than [`HELD_AFTER`] confirmed or
    /// failed, is told.
    fn read_again_behind(&self, key: Key, read: ReadAgain) {
        {
            let mut behind = self.0.behind.lock().expect("the reads behind");
            if let Some(again) = behind.get_mut(&key) {
                *again = true;
                return;
            }
            behind.insert(key.clone(), false);
        }
        self.0.pending.fetch_add(1, Ordering::SeqCst);
        let this = self.clone();
        let work = async move {
            while let Ask::Read(ask) = &key.ask {
                let held = this
                    .reads()
                    .at_of(&key)
                    .is_some_and(|at| Utc::now() - at > HELD_AFTER);
                let told = |failed| ViewFresh {
                    entity_id: key.entity.clone(),
                    section: Some(ask.clone()),
                    failed,
                };
                match read().await {
                    Ok(json) => {
                        let moved = this
                            .reads()
                            .replace(&key, Answer::Json(Arc::new(json.clone())));
                        this.hold_json(&key, ask, &json, Utc::now()).await;
                        if moved || held {
                            this.tell(told(None));
                        }
                    }
                    Err(error) => {
                        tracing::debug!(entity = %key.entity, %error, "a read behind failed; the kept answer stands");
                        if held {
                            this.tell(told(Some(error.to_string())));
                        }
                    }
                }
                let again = {
                    let mut behind = this.0.behind.lock().expect("the reads behind");
                    match behind.get_mut(&key) {
                        Some(again) if *again => {
                            *again = false;
                            true
                        }
                        _ => {
                            behind.remove(&key);
                            false
                        }
                    }
                };
                if !again {
                    break;
                }
            }
            if this.0.pending.fetch_sub(1, Ordering::SeqCst) == 1 {
                this.0.idle.notify_waiters();
            }
        };
        #[cfg(feature = "test-util")]
        let work = knobas_http::counted::inherit(work);
        tokio::spawn(work);
    }

    /// Hold a read's JSON `answer` to `ask`, read at `read_at` under `key`.
    async fn hold_json(
        &self,
        key: &Key,
        ask: &str,
        answer: &serde_json::Value,
        read_at: DateTime<Utc>,
    ) {
        let (Some(pool), Some((id, recipe))) = (self.store(), self.recipe_of(key.generation))
        else {
            return;
        };
        super::held::put(&pool, &id, &key.entity, ask, &recipe, read_at, answer).await;
    }

    /// The bytes of the file at `path` in `entity`'s tree, at `commit`, as
    /// `Source::file_bytes` answers them under `cap`: from the cache where
    /// the adapter says the read is fixed and the cache holds it, read live
    /// otherwise (#2256). `None` is an adapter that reads no bytes.
    ///
    /// # Errors
    ///
    /// The adapter's; a failed read is never kept.
    pub(crate) async fn bytes(
        &self,
        lent: &Lent,
        entity: &str,
        path: &str,
        commit: Option<&str>,
        cap: u64,
    ) -> Result<Option<Arc<FileBytes>>, SourceError> {
        let read = || async {
            Ok(lent
                .source
                .file_bytes(entity, path, commit, cap)
                .await?
                .map(Arc::new))
        };
        if lent.source.bytes_standing(entity, path, commit) != Standing::Fixed {
            return read().await;
        }
        let key = Key {
            generation: lent.generation,
            entity: entity.to_owned(),
            ask: Ask::Bytes {
                path: path.to_owned(),
                commit: commit.map(str::to_owned),
                cap,
            },
        };
        if let Some(Answer::Bytes(kept)) = self.reads().get(&key) {
            return Ok(kept);
        }
        let answer = read().await?;
        self.reads().put(key, Answer::Bytes(answer.clone()));
        Ok(answer)
    }

    /// A pipeline's kept run over the file at `path`, where the bytes are
    /// read at a whole sha (`bytes_standing` *Fixed*; #2264). `pipeline`
    /// names the steps and their limits.
    pub(crate) fn kept_run(
        &self,
        lent: &Lent,
        entity: &str,
        path: &str,
        commit: Option<&str>,
        pipeline: &str,
    ) -> Option<Arc<crate::file_pipeline::PipeRun>> {
        let key = Self::piped_key(lent, entity, path, commit, pipeline)?;
        match self.reads().get(&key) {
            Some(Answer::Piped(kept)) => Some(kept),
            _ => None,
        }
    }

    /// Keep `run` as [`Self::kept_run`] finds it; over moving bytes, nothing.
    pub(crate) fn keep_run(
        &self,
        lent: &Lent,
        entity: &str,
        path: &str,
        commit: Option<&str>,
        pipeline: &str,
        run: Arc<crate::file_pipeline::PipeRun>,
    ) {
        if let Some(key) = Self::piped_key(lent, entity, path, commit, pipeline) {
            self.reads().put(key, Answer::Piped(run));
        }
    }

    fn piped_key(
        lent: &Lent,
        entity: &str,
        path: &str,
        commit: Option<&str>,
        pipeline: &str,
    ) -> Option<Key> {
        (lent.source.bytes_standing(entity, path, commit) == Standing::Fixed).then(|| Key {
            generation: lent.generation,
            entity: entity.to_owned(),
            ask: Ask::Piped {
                path: path.to_owned(),
                commit: commit.map(str::to_owned),
                pipeline: pipeline.to_owned(),
            },
        })
    }

    /// The asks among `asks` the source's credential cannot make, as
    /// `Source::refused_writes` answers: each ask the adapter says moves is
    /// answered from the cache where it holds it, and asked again behind;
    /// the rest are asked now, together.
    ///
    /// **A read that fails is never drawn as allowed** (#2306). Asked now,
    /// each of its asks is refused with [`unasked`]'s reason and nothing is
    /// kept, so the next ask asks again; read behind, the kept answer stands
    /// and nothing is said, as for a view.
    pub(crate) async fn refused(&self, lent: &Lent, asks: &[WriteAsk]) -> Vec<RefusedWrite> {
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
            self.read_behind(&lent.source, kept_keys);
        }
        if !ask_now.is_empty() {
            let fresh = match lent.source.refused_writes(&ask_now).await {
                Ok(fresh) => fresh,
                Err(error) => {
                    tracing::debug!(%error, "grants asked now failed; refused, kept nowhere");
                    let why = unasked(&error);
                    answered.extend(ask_now.iter().map(|ask| RefusedWrite::of(ask, why.clone())));
                    ask_now.clear();
                    Vec::new()
                }
            };
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
        asks.iter()
            .filter_map(|ask| answered.iter().find(|r| r.refuses(ask)).cloned())
            .collect()
    }

    /// Read `keys` again behind the answer just given from the cache: a
    /// view's own read, or the writes' asks together, as they were asked. An
    /// answer replaces the kept one where the cache still holds it, and a
    /// moved view is told.
    ///
    /// One read per key at a time. A key asked for while its read is under
    /// way is read once more after it, however often it was asked: the read
    /// under way may have been sent before what the later ask comes after (a
    /// write's echo), and must not stand for it.
    fn read_behind(&self, source: &Arc<dyn Source>, keys: Vec<Key>) {
        let keys: Vec<Key> = {
            let mut behind = self.0.behind.lock().expect("the reads behind");
            keys.into_iter()
                .filter(|key| match behind.get_mut(key) {
                    Some(again) => {
                        *again = true;
                        false
                    }
                    None => {
                        behind.insert(key.clone(), false);
                        true
                    }
                })
                .collect()
        };
        if keys.is_empty() {
            return;
        }
        self.0.pending.fetch_add(1, Ordering::SeqCst);
        let this = self.clone();
        let source = Arc::clone(source);
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
                    // Kept only where fixed, so never read behind; a read
                    // other than a view is read behind by its own closure.
                    Ask::Bytes { .. } | Ask::Piped { .. } | Ask::Read(_) => {}
                }
            }
            for (key, section) in views {
                // An answer older than `HELD_AFTER` was drawn with when it was
                // read: its page is told how this read went, either way.
                let held = this
                    .reads()
                    .at_of(key)
                    .is_some_and(|at| Utc::now() - at > HELD_AFTER);
                match source.view(&key.entity, section.as_deref()).await {
                    Ok(view) => {
                        let read_at = Utc::now();
                        let moved = this.reads().replace(key, Answer::View(view.clone()));
                        this.hold(key, &view, read_at).await;
                        if moved || held {
                            this.tell(ViewFresh {
                                entity_id: key.entity.clone(),
                                section,
                                failed: None,
                            });
                        }
                    }
                    Err(error) => {
                        tracing::debug!(entity = %key.entity, %error, "a read behind failed; the kept answer stands");
                        if held {
                            this.tell(ViewFresh {
                                entity_id: key.entity.clone(),
                                section,
                                failed: Some(error.to_string()),
                            });
                        }
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
            let again: Vec<Key> = {
                let mut behind = this.0.behind.lock().expect("the reads behind");
                keys.into_iter()
                    .filter(|key| behind.remove(key) == Some(true))
                    .collect()
            };
            // Started before this one counts as ended, so `settled` waits
            // for it too.
            if !again.is_empty() {
                this.read_behind(&source, again);
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
            .recipes
            .lock()
            .expect("the recipes")
            .remove(&generation);
        self.0
            .reads
            .lock()
            .expect("the read cache")
            .drop_generation(generation);
    }
}

/// Why a write is drawn disabled when its grants could not be read (#2306):
/// a write is never drawn as allowed on a failed read, and the reader is told
/// it is knobas that does not know, not the source that refused.
fn unasked(error: &SourceError) -> String {
    format!(
        "knobas could not ask whether this credential may make this write ({error}); \
         it asks again when the write is drawn again"
    )
}

/// What is kept, by the build it was read through and the entity.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Key {
    generation: u64,
    entity: String,
    ask: Ask,
}

/// A view's section, a write's grants, or a file's bytes under a cap.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Ask {
    View(Option<String>),
    Write {
        op: String,
        field: Option<String>,
    },
    Bytes {
        path: String,
        commit: Option<String>,
        cap: u64,
    },
    /// A pipeline's run over a file (#2264): `pipeline` is its steps and
    /// limits, written out.
    Piped {
        path: String,
        commit: Option<String>,
        pipeline: String,
    },
    /// A source read other than a view, by its name (ADR-0042).
    Read(String),
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

/// A kept answer: a view, whether one write is refused and why, or a file's
/// bytes.
#[derive(Clone)]
enum Answer {
    View(Option<SourceView>),
    Refused(Option<RefusedWrite>),
    Bytes(Option<Arc<FileBytes>>),
    Piped(Arc<crate::file_pipeline::PipeRun>),
    /// A read other than a view, as JSON (ADR-0042).
    Json(Arc<serde_json::Value>),
}

impl Answer {
    /// The bytes it takes and the digest a moved one is told by: a view's and
    /// a grant's from their JSON, the measure of their size the app already
    /// has; a file's by its length, since its JSON would be four times it,
    /// with no digest, since a byte read is kept only where it cannot move.
    fn measure(&self) -> Option<(usize, u64)> {
        let json = match self {
            Self::View(view) => serde_json::to_vec(view).ok()?,
            Self::Json(json) => serde_json::to_vec(json.as_ref()).ok()?,
            // Refused with no words is not the same answer as taken.
            Self::Refused(refused) => refused.as_ref().map_or_else(
                || b"taken".to_vec(),
                |r| [b"refused:", r.reason.as_bytes()].concat(),
            ),
            Self::Bytes(read) => {
                let bytes = match read.as_deref() {
                    Some(FileBytes::Read(bytes)) => bytes.len(),
                    Some(FileBytes::TooLarge { .. }) | None => 0,
                };
                return Some((bytes, 0));
            }
            // Kept only over fixed bytes, so never compared either.
            Self::Piped(run) => return Some((run.weight(), 0)),
        };
        Some((json.len(), digest(&json)))
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
    /// When the answer was read: now for a read made now, a held read's own
    /// time for one taken from `sync.held_read`.
    at: DateTime<Utc>,
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
        self.get_at(key).map(|(answer, _)| answer)
    }

    /// [`Self::get`], and when the answer was read.
    fn get_at(&mut self, key: &Key) -> Option<(Answer, DateTime<Utc>)> {
        let now = self.tick();
        let kept = self.entries.get_mut(key)?;
        self.order.remove(&kept.used);
        kept.used = now;
        self.order.insert(now, key.clone());
        Some((kept.answer.clone(), kept.at))
    }

    /// When `key`'s answer was read, where it is kept; its use unchanged.
    fn at_of(&self, key: &Key) -> Option<DateTime<Utc>> {
        self.entries.get(key).map(|kept| kept.at)
    }

    /// Replace `key`'s answer where it is still kept (a forget or the bound
    /// may have dropped it since), and say whether the answer moved. An
    /// answer that did not move is read now all the same.
    fn replace(&mut self, key: &Key, answer: Answer) -> bool {
        let Some(before) = self.entries.get(key).map(|kept| kept.digest) else {
            return false;
        };
        let moved = answer.measure().map(|(_, digest)| digest) != Some(before);
        if moved {
            self.put(key.clone(), answer);
        } else if let Some(kept) = self.entries.get_mut(key) {
            kept.at = Utc::now();
        }
        moved
    }

    fn put(&mut self, key: Key, answer: Answer) {
        self.put_at(key, answer, Utc::now());
    }

    /// [`Self::put`], read at `at`.
    fn put_at(&mut self, key: Key, answer: Answer, at: DateTime<Utc>) {
        let Some((bytes, digest)) = answer.measure() else {
            return;
        };
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
                at,
                bytes,
                digest,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A refusal with no words and no refusal are two answers: a read behind
    /// that moves from one to the other replaces the kept one.
    #[test]
    fn a_wordless_refusal_is_not_taken() {
        let ask = WriteAsk {
            entity: "s:o/r".to_owned(),
            op: "commit_file".to_owned(),
            field: None,
        };
        let key = Key::of_ask(0, &ask);
        let mut reads = Reads::new(1 << 20);
        reads.put(key.clone(), Answer::Refused(None));
        assert!(reads.replace(
            &key,
            Answer::Refused(Some(RefusedWrite::of(&ask, String::new())))
        ));
        assert!(!reads.replace(
            &key,
            Answer::Refused(Some(RefusedWrite::of(&ask, String::new())))
        ));
    }
}

//! *State colours* (spec #2463): what the three commands read and write.
//!
//! The overrides themselves, their table and the merge are
//! `knobas_core::state_colours`'. This is where the registry is, so this is
//! where the words a kind offers are listed: the declared words of every
//! candidate of every configured adapter, then the words the mirror has seen
//! at the kind's open candidates, each with the tone its items fall back to
//! (Jira's status category, #2468).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, PoisonError};

use knobas_core::activity::ActivityRow;
use knobas_core::state_colours::{self, ToneChoice, ToneOverride};
use knobas_core::tone::Tone;
use knobas_sync::scheduler::AdapterRegistry;
use sqlx::PgPool;
use tokio::sync::watch;

use crate::IpcError;

/// The activity verb of a set (`detail`: `kind`, `word`, `tone`).
pub const SET_VERB: &str = "state_tone_set";
/// The activity verb of a clear (`detail`: `kind`, `word` or null, `cleared`).
pub const CLEARED_VERB: &str = "state_tone_cleared";

/// Where a word's resulting tone comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
#[ts(export_to = "sources.ts")]
pub enum ToneOrigin {
    /// The reader's override.
    Yours,
    /// The adapter's declared map.
    Adapter,
    /// Where the adapter reads on for a word its map leaves out: Jira's
    /// status category (#2468), for every seen status no name maps,
    /// whatever the category gives it (a to-do status: no colour).
    Category,
    /// Nothing: a seen word no adapter declares and nobody coloured, of a
    /// kind no adapter reads on past the open candidate for. No shipped
    /// adapter has such a kind today.
    Unset,
}

/// One state word a kind offers, and how it is drawn.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "sources.ts")]
pub struct StateWordColour {
    pub word: String,
    /// An adapter's map names the word. `false` for a word the mirror has
    /// seen at an open candidate.
    pub declared: bool,
    /// The adapter's tone, `None` for a word it declares with none or does
    /// not declare.
    pub declared_tone: Option<Tone>,
    /// The tone the word's items draw where the adapter reads on past the
    /// word (Jira's status category, #2468), for a seen word it does not
    /// declare. `None` for a declared word, and for one whose items resolve
    /// to nothing there (a to-do status).
    pub category_tone: Option<Tone>,
    /// The reader's override, if any.
    pub yours: Option<ToneChoice>,
    /// What the word draws: the override, else the adapter's tone, else the
    /// category's.
    pub tone: Option<Tone>,
    pub origin: ToneOrigin,
}

/// One entity kind that declares a state, and its offered words: declared
/// words in declared order, then the others alphabetically.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "sources.ts")]
pub struct KindStateColours {
    pub kind: String,
    pub words: Vec<StateWordColour>,
    /// Whether a configured adapter reads on past the kind's open candidate
    /// for a word its names leave out (Jira's status category, #2468), so
    /// that every such word, seen yet or not, takes a default there. The
    /// legend ends the kind with its category line by this.
    pub has_category: bool,
}

/// Every kind a configured source declares a state for, by kind id.
///
/// The adapters are read in adapter-kind order, so where two adapters declare
/// one word of one kind with different tones, the first adapter kind
/// alphabetically gives `declared_tone`. A source whose adapter is not
/// compiled in offers nothing.
///
/// # Errors
///
/// `internal` if a read fails.
pub async fn read(
    pool: &PgPool,
    registry: &dyn AdapterRegistry,
    counter: &SeenCounter,
) -> Result<Vec<KindStateColours>, IpcError> {
    let sources = knobas_sync::config::list(pool)
        .await
        .map_err(IpcError::internal)?;
    let overrides = state_colours::overrides(pool)
        .await
        .map_err(IpcError::internal)?;
    let templates = templates_of(&sources, registry);
    let counted = counted(&sources, &templates);
    // A source never counted, or counted under other declarations, waits for
    // the one count that is filling it (bring-up's, or the one this asks for),
    // never a second scan beside it. A count that failed is made here, so its
    // error is this read's.
    if !state_colours::uncounted(pool, &counted)
        .await
        .map_err(IpcError::internal)?
        .is_empty()
    {
        counter.wait(counter.request(pool, Owed::Stale)).await;
        for source in state_colours::uncounted(pool, &counted)
            .await
            .map_err(IpcError::internal)?
        {
            state_colours::count_seen_words(pool, source)
                .await
                .map_err(IpcError::internal)?;
        }
    }
    let seen = state_colours::stored_seen_words(pool, &counted)
        .await
        .map_err(IpcError::internal)?;
    Ok(offered(&templates, &seen, &overrides))
}

/// Each configured adapter kind's own declarations, by adapter kind.
fn templates_of(
    sources: &[knobas_sync::config::SourceConfigRow],
    registry: &dyn AdapterRegistry,
) -> BTreeMap<String, Vec<knobas_source::KindPaths>> {
    registry
        .descriptors()
        .into_iter()
        .filter(|d| sources.iter().any(|s| s.adapter_kind == d.adapter_kind))
        .map(|d| (d.adapter_kind, d.payload_paths))
        .collect()
}

/// The enabled sources whose words are seen, with what they are counted
/// under: a turned-off source's items are not live, as the walk over the
/// mirror left them out.
fn counted(
    sources: &[knobas_sync::config::SourceConfigRow],
    templates: &BTreeMap<String, Vec<knobas_source::KindPaths>>,
) -> Vec<state_colours::SourceWords> {
    sources
        .iter()
        .filter(|s| s.enabled)
        .filter_map(|s| state_colours::SourceWords::of(&s.id, templates.get(&s.adapter_kind)?))
        .collect()
}

/// What a count is asked for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Owed {
    /// Every counted source that has no count under its declarations now:
    /// bring-up's, and a read's that met one.
    Stale,
    /// This source again, whatever its count says: a run of it wrote to the
    /// mirror, or a write to one of its items landed.
    Source(String),
}

/// The process's one counter of seen words (`knobas_core::state_colours::
/// count_seen_words`), as [`crate::suggestion_pass::SuggestionPass`] is its
/// one pass: at most one count runs, every request that arrives meanwhile is
/// answered by the next, and a request is answered by a count that started
/// after it. Held by [`crate::commands::app::Lifecycle`], so the read, bring-up
/// and the scheduler's events share it.
#[derive(Clone)]
pub struct SeenCounter {
    inner: Arc<CounterInner>,
}

struct CounterInner {
    registry: Arc<dyn AdapterRegistry>,
    queue: Mutex<CountQueue>,
    answered: watch::Sender<u64>,
}

#[derive(Default)]
struct CountQueue {
    requested: u64,
    running: bool,
    stale: bool,
    owed: BTreeSet<String>,
}

impl SeenCounter {
    /// A counter over `registry`'s declarations.
    #[must_use]
    pub fn new(registry: Arc<dyn AdapterRegistry>) -> SeenCounter {
        SeenCounter {
            inner: Arc::new(CounterInner {
                registry,
                queue: Mutex::new(CountQueue::default()),
                answered: watch::channel(0).0,
            }),
        }
    }

    /// Ask for a count over `pool`; answers the request's number, which
    /// [`Self::wait`] takes. Starts a count if none runs, and never waits.
    pub fn request(&self, pool: &PgPool, owed: Owed) -> u64 {
        let mut queue = self.lock();
        queue.requested += 1;
        match owed {
            Owed::Stale => queue.stale = true,
            Owed::Source(id) => {
                queue.owed.insert(id);
            }
        }
        if !queue.running {
            queue.running = true;
            tokio::spawn(Arc::clone(&self.inner).drive(pool.clone()));
        }
        queue.requested
    }

    /// Until a count that started after request `asked` has ended, whatever
    /// it found.
    pub async fn wait(&self, asked: u64) {
        let mut answered = self.inner.answered.subscribe();
        let _ = answered.wait_for(|done| *done >= asked).await;
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, CountQueue> {
        self.inner
            .queue
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

impl CounterInner {
    /// Count until every request is answered, then stop.
    async fn drive(self: Arc<Self>, pool: PgPool) {
        loop {
            let (through, stale, owed) = {
                let mut queue = self.queue.lock().unwrap_or_else(PoisonError::into_inner);
                if *self.answered.borrow() >= queue.requested {
                    queue.running = false;
                    return;
                }
                (
                    queue.requested,
                    std::mem::take(&mut queue.stale),
                    std::mem::take(&mut queue.owed),
                )
            };
            if let Err(error) = self.count(&pool, stale, &owed).await {
                tracing::warn!(%error, "the seen state words could not be counted");
            }
            self.answered.send_replace(through);
        }
    }

    async fn count(
        &self,
        pool: &PgPool,
        stale: bool,
        owed: &BTreeSet<String>,
    ) -> Result<(), knobas_core::CoreError> {
        let sources = knobas_sync::config::list(pool).await?;
        let templates = templates_of(&sources, self.registry.as_ref());
        let counted = counted(&sources, &templates);
        let mut due: Vec<&state_colours::SourceWords> = counted
            .iter()
            .filter(|s| owed.contains(&s.source_id))
            .collect();
        if stale {
            for source in state_colours::uncounted(pool, &counted).await? {
                if !due.iter().any(|d| d.source_id == source.source_id) {
                    due.push(source);
                }
            }
        }
        for source in due {
            let started = std::time::Instant::now();
            state_colours::count_seen_words(pool, source).await?;
            tracing::debug!(source = %source.source_id, ms = started.elapsed().as_millis(), "seen state words counted");
        }
        Ok(())
    }
}

/// The scheduler's events, and a count of a source's seen words asked for
/// when a run of it ends OK having written to the mirror (or not saying), as
/// [`crate::suggestion_pass::SuggestOnSync`] asks for its pass; and when a
/// write to one of its items lands. Every event is passed on unchanged.
pub struct CountOnSync<E> {
    inner: E,
    counter: SeenCounter,
    pool: PgPool,
}

impl<E> CountOnSync<E> {
    /// `inner`'s events, and counts asked of `counter` over `pool`.
    pub fn new(inner: E, counter: SeenCounter, pool: PgPool) -> CountOnSync<E> {
        CountOnSync {
            inner,
            counter,
            pool,
        }
    }
}

impl<E: knobas_sync::scheduler::SyncEvents> knobas_sync::scheduler::SyncEvents for CountOnSync<E> {
    fn sync_state(&self, status: knobas_sync::SourceSyncStatus) {
        if !status.running
            && status.last_outcome == Some(knobas_sync::SyncOutcome::Ok)
            && status.last_changed != Some(false)
        {
            self.counter
                .request(&self.pool, Owed::Source(status.source_id.clone()));
        }
        self.inner.sync_state(status);
    }
    fn source_health(&self, health: knobas_sync::CredentialHealth) {
        self.inner.source_health(health);
    }
    fn activity_new(&self, row: ActivityRow) {
        self.inner.activity_new(row);
    }
    fn write_landed(&self, write: &knobas_core::write_queue::QueuedWrite) {
        self.counter
            .request(&self.pool, Owed::Source(write.source_id.clone()));
        self.inner.write_landed(write);
    }
    fn refresh_ended(&self, ended: &knobas_sync::scheduler::RefreshEnded) {
        self.inner.refresh_ended(ended);
    }
}

/// A kind's declared words in declared order, each with its adapter's tone.
type Declared = Vec<(String, Option<Tone>)>;

/// [`read`]'s answer from what it read.
fn offered(
    templates: &BTreeMap<String, Vec<knobas_source::KindPaths>>,
    seen: &[state_colours::SeenWord],
    overrides: &[ToneOverride],
) -> Vec<KindStateColours> {
    // Per kind: its declared words in order with their tone, whether it has
    // an open candidate, and whether an adapter reads on past that one.
    let mut kinds: BTreeMap<String, (Declared, bool, bool)> = BTreeMap::new();
    for paths in templates.values().flatten() {
        if paths.state.is_empty() {
            continue;
        }
        let (declared, open, category) = kinds.entry(paths.kind.clone()).or_default();
        *category |= !state_colours::fallback(std::slice::from_ref(paths)).is_empty();
        for candidate in &paths.state {
            *open |= candidate.open;
            // A candidate drawn as another path's string (Jira's category)
            // has keys, not words: the chip never says them.
            if candidate.text_at.is_some() {
                continue;
            }
            for value in &candidate.values {
                if let Some(word) = state_colours::word_of(value)
                    && !declared.iter().any(|(w, _)| *w == word)
                {
                    declared.push((word, value.tone));
                }
            }
        }
    }
    kinds
        .into_iter()
        .map(|(kind, (declared, open, has_category))| {
            let yours = |word: &str| {
                overrides
                    .iter()
                    .find(|o| o.kind == kind && o.word == word)
                    .map(|o| o.choice)
            };
            let mut others: Vec<String> = Vec::new();
            if open {
                let more = seen
                    .iter()
                    .filter(|w| w.kind == kind)
                    .map(|w| w.word.clone())
                    .chain(
                        overrides
                            .iter()
                            .filter(|o| o.kind == kind)
                            .map(|o| o.word.clone()),
                    );
                for word in more {
                    if !declared.iter().any(|(w, _)| *w == word) && !others.contains(&word) {
                        others.push(word);
                    }
                }
                others.sort();
            }
            let words = declared
                .iter()
                .map(|(word, tone)| (word.clone(), true, *tone))
                .chain(others.into_iter().map(|word| (word, false, None)))
                .map(|(word, declared, declared_tone)| {
                    let mine = yours(&word);
                    let category_tone = if declared {
                        None
                    } else {
                        seen.iter()
                            .find(|w| w.kind == kind && w.word == word)
                            .and_then(|w| w.fallback)
                    };
                    let (tone, origin) = match (mine, category_tone) {
                        (Some(choice), _) => (choice.tone(), ToneOrigin::Yours),
                        (None, _) if declared => (declared_tone, ToneOrigin::Adapter),
                        (None, Some(tone)) => (Some(tone), ToneOrigin::Category),
                        // A to-do status draws no colour, and that is its
                        // category's answer as much as `info` is.
                        (None, None) if has_category => (None, ToneOrigin::Category),
                        (None, None) => (None, ToneOrigin::Unset),
                    };
                    StateWordColour {
                        word,
                        declared,
                        declared_tone,
                        category_tone,
                        yours: mine,
                        tone,
                        origin,
                    }
                })
                .collect();
            KindStateColours {
                kind,
                words,
                has_category,
            }
        })
        .collect()
}

fn refuse_blank(what: &str, value: &str) -> Result<(), IpcError> {
    if value.trim().is_empty() {
        return Err(IpcError::invalid(format!("a tone override needs a {what}")));
    }
    Ok(())
}

/// Store `choice` for `kind`'s `word`, and write the activity line that says
/// so. The line is what the command announces.
///
/// # Errors
///
/// `invalid` for a blank kind or word, `internal` if a write fails.
pub async fn set(
    pool: &PgPool,
    kind: &str,
    word: &str,
    choice: ToneChoice,
) -> Result<ActivityRow, IpcError> {
    refuse_blank("kind", kind)?;
    refuse_blank("word", word)?;
    state_colours::set(pool, kind, word, choice).await?;
    Ok(knobas_core::activity::record(
        pool,
        "user",
        SET_VERB,
        None,
        serde_json::json!({ "kind": kind.trim(), "word": word.trim(), "tone": choice }),
    )
    .await?)
}

/// Forget `kind`'s override for `word`, or all of `kind`'s for `None`, and
/// write the activity line that says so. A clear that forgot nothing is no
/// change and writes no line.
///
/// # Errors
///
/// `invalid` for a blank kind, `internal` if a write fails.
pub async fn clear(
    pool: &PgPool,
    kind: &str,
    word: Option<&str>,
) -> Result<Option<ActivityRow>, IpcError> {
    refuse_blank("kind", kind)?;
    let cleared = state_colours::clear(pool, kind, word).await?;
    if cleared == 0 {
        return Ok(None);
    }
    Ok(Some(
        knobas_core::activity::record(
            pool,
            "user",
            CLEARED_VERB,
            None,
            serde_json::json!({
                "kind": kind.trim(), "word": word.map(str::trim), "cleared": cleared,
            }),
        )
        .await?,
    ))
}

#[cfg(test)]
mod tests {
    use knobas_core::payload::{KindPaths, PayloadPath, StatePath, StateValue};
    use knobas_core::state_colours::SeenWord;

    use super::*;

    /// A ticket's open status name, and, with `category`, the candidate a
    /// status no name maps is read on at (Jira's status category).
    fn ticket(category: bool) -> KindPaths {
        let mut state = vec![StatePath {
            at: PayloadPath::of(["status", "name"]),
            open: true,
            text_at: None,
            values: vec![StateValue::of("Done", Tone::Ok)],
        }];
        if category {
            state.push(StatePath {
                at: PayloadPath::of(["status", "category"]),
                open: false,
                text_at: Some(PayloadPath::of(["status", "name"])),
                values: vec![StateValue::of("indeterminate", Tone::Info)],
            });
        }
        KindPaths {
            kind: "ticket".to_owned(),
            state,
            ..KindPaths::default()
        }
    }

    fn seen(word: &str, fallback: Option<Tone>) -> SeenWord {
        SeenWord {
            kind: "ticket".to_owned(),
            word: word.to_owned(),
            fallback,
        }
    }

    fn origins(category: bool, seen: &[SeenWord]) -> Vec<(String, Option<Tone>, ToneOrigin)> {
        let templates = BTreeMap::from([("jira".to_owned(), vec![ticket(category)])]);
        offered(&templates, seen, &[])[0]
            .words
            .iter()
            .map(|w| (w.word.clone(), w.tone, w.origin))
            .collect()
    }

    /// Where an adapter reads on to a category, every seen word's default is
    /// the category's, a to-do status's no colour too (spec #2463: a chip
    /// shows *Jira category* for a ticket's default). Only a kind with
    /// nowhere to read on leaves a seen word unset.
    #[test]
    fn a_seen_word_takes_the_category_origin_wherever_its_kind_reads_one() {
        let words = [seen("Backlog", None), seen("Testing", Some(Tone::Info))];
        assert_eq!(
            origins(true, &words),
            [
                ("Done".to_owned(), Some(Tone::Ok), ToneOrigin::Adapter),
                ("Backlog".to_owned(), None, ToneOrigin::Category),
                ("Testing".to_owned(), Some(Tone::Info), ToneOrigin::Category),
            ]
        );
        assert_eq!(
            origins(false, &[seen("Backlog", None)]),
            [
                ("Done".to_owned(), Some(Tone::Ok), ToneOrigin::Adapter),
                ("Backlog".to_owned(), None, ToneOrigin::Unset),
            ]
        );
    }
}

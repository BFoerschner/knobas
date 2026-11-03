//! *State colours* (spec #2463): what the three commands read and write.
//!
//! The overrides themselves, their table and the merge are
//! `knobas_core::state_colours`'. This is where the registry is, so this is
//! where the words a kind offers are listed: the declared words of every
//! candidate of every configured adapter, then the words the mirror has seen
//! at the kind's open candidates, each with the tone its items fall back to
//! (Jira's status category, #2468).

use std::collections::BTreeMap;

use knobas_core::activity::ActivityRow;
use knobas_core::state_colours::{self, ToneChoice, ToneOverride};
use knobas_core::tone::Tone;
use knobas_sync::scheduler::AdapterRegistry;
use sqlx::PgPool;

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
    /// status category (#2468), for a seen status no name maps.
    Category,
    /// Nothing: a seen word no adapter declares and nobody coloured.
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
) -> Result<Vec<KindStateColours>, IpcError> {
    let sources = knobas_sync::config::list(pool)
        .await
        .map_err(IpcError::internal)?;
    let overrides = state_colours::overrides(pool)
        .await
        .map_err(IpcError::internal)?;
    let templates: BTreeMap<String, Vec<knobas_source::KindPaths>> = registry
        .descriptors()
        .into_iter()
        .filter(|d| sources.iter().any(|s| s.adapter_kind == d.adapter_kind))
        .map(|d| (d.adapter_kind, d.payload_paths))
        .collect();
    let open: Vec<state_colours::OpenPath> = sources
        .iter()
        .filter_map(|s| Some((s.id.as_str(), templates.get(&s.adapter_kind)?)))
        .flat_map(|(id, kinds)| state_colours::open_paths(id, kinds).collect::<Vec<_>>())
        .collect();
    let fallback = sources
        .iter()
        .filter_map(|s| Some((s.id.as_str(), templates.get(&s.adapter_kind)?)))
        .fold(
            knobas_core::payload::Declarations::empty(),
            |d, (id, kinds)| d.with(id, state_colours::fallback(kinds)),
        );
    let seen = state_colours::seen_words(pool, &open, &fallback)
        .await
        .map_err(IpcError::internal)?;
    Ok(offered(&templates, &seen, &overrides))
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

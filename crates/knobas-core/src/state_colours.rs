//! **State colours** (spec #2463): the reader's tone overrides, one per state
//! word of one entity kind, and the one merge that applies them to what the
//! adapters declare.
//!
//! A *tone override* is stored in `knobas.state_tone` (migration `0071`),
//! keyed by kind and word, holding a tone or no colour. A missing row is no
//! override. It is set **per entity kind**, never per source or adapter: a
//! `closed` set on `pr` colours a Gitea pull request and a mock one alike.
//!
//! [`merge`] is the one place an override reaches a declaration. Every read
//! route that builds [`Declarations`](crate::payload::Declarations) calls it
//! on the adapters' templates before folding them in (`knobas_app::sources::
//! paths`, and the searcher's copy in `knobas_search`), so the SQL rules,
//! which read only the declarations, need no change. It is **never** applied
//! to what is written into `sync.parent_declaration`: that table keeps the
//! adapters' own paths, so an override never recomputes the stored parents.

use sqlx::PgPool;

use crate::CoreError;
use crate::payload::{KindPaths, PayloadPath, StateValue};
use crate::tone::Tone;

/// What a reader picks for one state word: one of the five tones, or *No
/// colour*. On the wire the tone's own lowercase word, or `"none"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
#[ts(export_to = "sources.ts")]
pub enum ToneChoice {
    Ok,
    Warn,
    Err,
    Info,
    Muted,
    /// The word still matches and draws its text, with no tone: the walk over
    /// candidates stops there (a toneless value, #2464).
    None,
}

impl ToneChoice {
    /// The tone this choice draws, `None` for *No colour*.
    #[must_use]
    pub fn tone(self) -> Option<Tone> {
        match self {
            ToneChoice::Ok => Some(Tone::Ok),
            ToneChoice::Warn => Some(Tone::Warn),
            ToneChoice::Err => Some(Tone::Err),
            ToneChoice::Info => Some(Tone::Info),
            ToneChoice::Muted => Some(Tone::Muted),
            ToneChoice::None => None,
        }
    }

    /// The choice that draws `tone`, *No colour* for `None`.
    #[must_use]
    pub fn of(tone: Option<Tone>) -> ToneChoice {
        match tone {
            Some(Tone::Ok) => ToneChoice::Ok,
            Some(Tone::Warn) => ToneChoice::Warn,
            Some(Tone::Err) => ToneChoice::Err,
            Some(Tone::Info) => ToneChoice::Info,
            Some(Tone::Muted) => ToneChoice::Muted,
            None => ToneChoice::None,
        }
    }

    /// The stored spelling: the tone's word, or SQL `null` for *No colour*.
    fn stored(self) -> Option<&'static str> {
        match self {
            ToneChoice::Ok => Some("ok"),
            ToneChoice::Warn => Some("warn"),
            ToneChoice::Err => Some("err"),
            ToneChoice::Info => Some("info"),
            ToneChoice::Muted => Some("muted"),
            ToneChoice::None => None,
        }
    }

    fn from_stored(stored: Option<&str>) -> Option<ToneChoice> {
        Some(match stored {
            Some("ok") => ToneChoice::Ok,
            Some("warn") => ToneChoice::Warn,
            Some("err") => ToneChoice::Err,
            Some("info") => ToneChoice::Info,
            Some("muted") => ToneChoice::Muted,
            None => ToneChoice::None,
            Some(_) => return None,
        })
    }
}

/// One stored tone override: this kind's `word` draws `choice`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToneOverride {
    pub kind: String,
    pub word: String,
    pub choice: ToneChoice,
}

/// Every stored override, by kind and word.
///
/// A bare `sqlx::Error`, so the searcher, which has no `CoreError`, reads
/// it as it reads everything else.
///
/// # Errors
///
/// The database's, if the read fails.
pub async fn overrides(pool: &PgPool) -> Result<Vec<ToneOverride>, sqlx::Error> {
    let rows: Vec<(String, String, Option<String>)> =
        sqlx::query_as("select kind, word, tone from knobas.state_tone order by kind, word")
            .fetch_all(pool)
            .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(kind, word, tone)| {
            Some(ToneOverride {
                kind,
                word,
                choice: ToneChoice::from_stored(tone.as_deref())?,
            })
        })
        .collect())
}

/// Store `choice` for `kind`'s `word`, over any earlier one. `word` is
/// trimmed, the way a state value is matched.
///
/// # Errors
///
/// [`CoreError::Db`] if the write fails, the table's check included (an empty
/// kind or word).
pub async fn set(
    pool: &PgPool,
    kind: &str,
    word: &str,
    choice: ToneChoice,
) -> Result<(), CoreError> {
    sqlx::query(
        "insert into knobas.state_tone (kind, word, tone) values ($1, $2, $3)
         on conflict (kind, word) do update set tone = excluded.tone, set_at = now()",
    )
    .bind(kind.trim())
    .bind(word.trim())
    .bind(choice.stored())
    .execute(pool)
    .await?;
    Ok(())
}

/// Forget `kind`'s override for `word`, or every override of `kind` for
/// `None`. Answers how many there were.
///
/// # Errors
///
/// [`CoreError::Db`] if the delete fails.
pub async fn clear(pool: &PgPool, kind: &str, word: Option<&str>) -> Result<u64, CoreError> {
    let done = sqlx::query(
        "delete from knobas.state_tone where kind = $1 and ($2::text is null or word = $2)",
    )
    .bind(kind.trim())
    .bind(word.map(str::trim))
    .execute(pool)
    .await?;
    Ok(done.rows_affected())
}

/// The state word a declared value is offered and overridden as: what its
/// chip says (`merged` for Gitea's merged flag), else the string it matches.
/// A flag with no text is no word.
#[must_use]
pub fn word_of(value: &StateValue) -> Option<String> {
    match (&value.text, &value.value) {
        (Some(text), _) => Some(text.clone()),
        (None, serde_json::Value::String(word)) => Some(word.trim().to_owned()),
        _ => None,
    }
}

/// Apply `overrides` to one adapter's declared kinds, per word.
///
/// * An override for a word a candidate declares replaces that value's tone,
///   in every candidate that declares it.
/// * An override for a word no candidate of the kind declares is added to the
///   kind's first **open** candidate (`StatePath::open`), as a value matching
///   that string. A kind with no open candidate ignores it.
/// * *No colour* is a toneless value: it matches and stops the walk.
///
/// Words with no override keep the adapter's tone. Nothing but
/// `KindPaths::state` changes.
pub fn merge(kinds: &mut [KindPaths], overrides: &[ToneOverride]) {
    for paths in kinds.iter_mut() {
        for o in overrides.iter().filter(|o| o.kind == paths.kind) {
            let tone = o.choice.tone();
            let mut declared = false;
            for candidate in &mut paths.state {
                for value in &mut candidate.values {
                    if word_of(value).as_deref() == Some(o.word.as_str()) {
                        value.tone = tone;
                        declared = true;
                    }
                }
            }
            if !declared && let Some(open) = paths.state.iter_mut().find(|c| c.open) {
                open.values.push(StateValue {
                    value: serde_json::Value::String(o.word.clone()),
                    tone,
                    text: None,
                });
            }
        }
    }
}

/// [`merge`] over an owned list, for a caller folding templates.
#[must_use]
pub fn merged(mut kinds: Vec<KindPaths>, overrides: &[ToneOverride]) -> Vec<KindPaths> {
    merge(&mut kinds, overrides);
    kinds
}

/// One open candidate of one source: where [`seen_words`] looks.
#[derive(Clone, Debug, serde::Serialize)]
pub struct OpenPath {
    pub source_id: String,
    pub kind: String,
    pub at: PayloadPath,
}

/// Every open candidate `kinds` declares, for `source_id`.
pub fn open_paths<'a>(
    source_id: &'a str,
    kinds: &'a [KindPaths],
) -> impl Iterator<Item = OpenPath> + 'a {
    kinds.iter().flat_map(move |paths| {
        paths
            .state
            .iter()
            .filter(|c| c.open)
            .map(move |c| OpenPath {
                source_id: source_id.to_owned(),
                kind: paths.kind.clone(),
                at: c.at.clone(),
            })
    })
}

/// The words the mirror has seen at `paths`: per kind, the distinct trimmed
/// non-empty strings its live items hold there, across the sources. The
/// generalisation of the mini board's `OBSERVED_STATUSES`, which knows only
/// `ticket` and `status_name`.
const SEEN_WORDS: &str = concat!(
    "with open_path as (
       select p->>'source_id' as source_id, p->>'kind' as kind,
              array(select jsonb_array_elements_text(p->'at')) as segs
         from jsonb_array_elements($1::jsonb) p)
     select distinct o.kind, btrim(i.payload #>> o.segs) as word
       from open_path o
       join sync.item i on i.source_id = o.source_id and i.kind = o.kind
      where jsonb_typeof(i.payload #> o.segs) = 'string'
        and btrim(i.payload #>> o.segs) <> ''
        and ",
    crate::live_by_exclusion!("i"),
    "
      order by 1, 2"
);

/// [`SEEN_WORDS`] over `paths`: `(kind, word)`, by kind then word.
///
/// # Errors
///
/// [`CoreError::Db`] if the read fails.
pub async fn seen_words(
    pool: &PgPool,
    paths: &[OpenPath],
) -> Result<Vec<(String, String)>, CoreError> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }
    Ok(sqlx::query_as(SEEN_WORDS)
        .bind(sqlx::types::Json(paths))
        .fetch_all(pool)
        .await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::payload::StatePath;

    fn build() -> KindPaths {
        KindPaths {
            kind: "build".to_owned(),
            state: vec![
                StatePath {
                    at: PayloadPath::of(["state"]),
                    open: false,
                    values: vec![StateValue::of("running", Tone::Info)],
                },
                StatePath {
                    at: PayloadPath::of(["status"]),
                    open: false,
                    values: vec![StateValue::of("FAILURE", Tone::Err)],
                },
            ],
            ..KindPaths::default()
        }
    }

    fn over(kind: &str, word: &str, choice: ToneChoice) -> ToneOverride {
        ToneOverride {
            kind: kind.to_owned(),
            word: word.to_owned(),
            choice,
        }
    }

    /// An undeclared word on a kind with no open candidate changes nothing:
    /// a word the adapter never writes and nobody can see is not invented.
    #[test]
    fn an_undeclared_word_without_an_open_candidate_is_ignored() {
        let kinds = merged(vec![build()], &[over("build", "finished", ToneChoice::Err)]);
        assert_eq!(kinds, vec![build()]);
    }

    /// Only an open candidate is where seen words are looked for: a kind
    /// with an open and a closed candidate offers the words seen at the open
    /// one alone, so TeamCity's pass-through `finished`, on a closed one, is
    /// never offered.
    #[test]
    fn only_open_candidates_are_looked_at() {
        let mut kind = build();
        kind.state[1].open = true;
        let open: Vec<(String, String, PayloadPath)> =
            open_paths("tc", std::slice::from_ref(&kind))
                .map(|p| (p.source_id, p.kind, p.at))
                .collect();
        assert_eq!(
            open,
            [(
                "tc".to_owned(),
                "build".to_owned(),
                PayloadPath::of(["status"])
            )]
        );
    }

    /// The stored spelling round-trips, *No colour* as SQL null.
    #[test]
    fn every_choice_is_stored_and_read_back_as_itself() {
        for choice in [
            ToneChoice::Ok,
            ToneChoice::Warn,
            ToneChoice::Err,
            ToneChoice::Info,
            ToneChoice::Muted,
            ToneChoice::None,
        ] {
            assert_eq!(ToneChoice::from_stored(choice.stored()), Some(choice));
            assert_eq!(ToneChoice::of(choice.tone()), choice);
        }
        assert_eq!(ToneChoice::from_stored(Some("loud")), None);
    }
}

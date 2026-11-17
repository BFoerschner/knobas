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
///   in every candidate that declares it. A candidate drawn as another
///   path's string (`StatePath::text_at`, Jira's status category) declares
///   no words: its values are keys the chip never says, and keep their tone.
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
            for candidate in paths.state.iter_mut().filter(|c| c.text_at.is_none()) {
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

/// What a word seen at an open candidate falls back to (#2468): per kind
/// with an open candidate, the candidates after the first open one, which is
/// where a ticket whose status no name maps is read on (Jira's status
/// category). A kind with no open candidate, or none after it, has none.
#[must_use]
pub fn fallback(kinds: &[KindPaths]) -> Vec<KindPaths> {
    kinds
        .iter()
        .filter_map(|paths| {
            let open = paths.state.iter().position(|c| c.open)?;
            let rest = &paths.state[open + 1..];
            (!rest.is_empty()).then(|| KindPaths {
                kind: paths.kind.clone(),
                state: rest.to_vec(),
                ..KindPaths::default()
            })
        })
        .collect()
}

/// The words the mirror has seen at `paths`: per kind, the distinct trimmed
/// non-empty strings its live items hold there, across the sources, each
/// with the tone its items resolve to through `$2`, the [`fallback`]
/// declarations: the most common outcome among its items, where resolving
/// to nothing counts as one (`mode()` skips nulls, so they are spelled `''`
/// and turned back): a word most of whose items draw no tone has none, even
/// where another source files a few under a toned category. A tie goes to
/// no tone, then to the tone's word. The generalisation of the mini board's
/// `OBSERVED_STATUSES`, which knows only `ticket` and `status_name`.
const SEEN_WORDS: &str = concat!(
    "with open_path as (
       select p->>'source_id' as source_id, p->>'kind' as kind,
              array(select jsonb_array_elements_text(p->'at')) as segs
         from jsonb_array_elements($1::jsonb) p)
     select o.kind, btrim(i.payload #>> o.segs) as word,
            nullif(mode() within group (order by coalesce(",
    crate::declared_state!("$2"),
    "->>'tone', '')), '') as tone
       from open_path o
       join sync.item i on i.source_id = o.source_id and i.kind = o.kind
      where jsonb_typeof(i.payload #> o.segs) = 'string'
        and btrim(i.payload #>> o.segs) <> ''
        and ",
    crate::live_by_exclusion!("i"),
    "
      group by 1, 2
      order by 1, 2"
);

/// One word the mirror has seen at an open candidate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeenWord {
    pub kind: String,
    pub word: String,
    /// The tone its items draw through the [`fallback`] declarations: Jira's
    /// status category for a status no name maps.
    pub fallback: Option<Tone>,
}

/// [`SEEN_WORDS`] over `paths`, by kind then word. `fallback` holds each
/// source's [`fallback`] declarations.
///
/// # Errors
///
/// [`CoreError::Db`] if the read fails.
pub async fn seen_words(
    pool: &PgPool,
    paths: &[OpenPath],
    fallback: &crate::payload::Declarations,
) -> Result<Vec<SeenWord>, CoreError> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }
    let rows: Vec<(String, String, Option<String>)> = sqlx::query_as(SEEN_WORDS)
        .bind(sqlx::types::Json(paths))
        .bind(fallback.as_param())
        .fetch_all(pool)
        .await?;
    Ok(rows
        .into_iter()
        .map(|(kind, word, tone)| SeenWord {
            kind,
            word,
            fallback: tone.and_then(|t| serde_json::from_value(serde_json::Value::String(t)).ok()),
        })
        .collect())
}

/// One open candidate of one kind, and the candidates its words fall back
/// to ([`fallback`]): what a source's stored seen words are counted under.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct OpenKind {
    pub kind: String,
    pub at: PayloadPath,
    pub fallback: Vec<crate::payload::StatePath>,
}

/// What one source's seen words are counted under: each open candidate its
/// declarations name, with that kind's fallback. Stored beside the counts
/// (`sync.state_word_source.paths`), so a source whose declarations changed
/// since is counted again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceWords {
    pub source_id: String,
    pub kinds: Vec<OpenKind>,
}

impl SourceWords {
    /// `kinds`, one source's declarations, as [`open_paths`] and [`fallback`]
    /// read them. `None` for a source with no open candidate: it has no seen
    /// words to count.
    #[must_use]
    pub fn of(source_id: &str, kinds: &[KindPaths]) -> Option<SourceWords> {
        let fallback = fallback(kinds);
        let open: Vec<OpenKind> = open_paths(source_id, kinds)
            .map(|path| OpenKind {
                fallback: fallback
                    .iter()
                    .find(|f| f.kind == path.kind)
                    .map(|f| f.state.clone())
                    .unwrap_or_default(),
                kind: path.kind,
                at: path.at,
            })
            .collect();
        (!open.is_empty()).then(|| SourceWords {
            source_id: source_id.to_owned(),
            kinds: open,
        })
    }

    fn paths(&self) -> serde_json::Value {
        serde_json::to_value(&self.kinds).unwrap_or(serde_json::Value::Null)
    }
}

/// One source's words, grouped by what decides their fallback tone: per
/// open candidate's kind, each trimmed non-empty string its live items hold
/// there, with the values (`vals`, a JSON array, null for a miss) its
/// [`fallback`] candidates' paths lead to, and how many items hold that
/// pair. The tones are resolved per group in Rust ([`resolve_state`]), so a
/// payload is read once, not walked through the declarations per item:
/// 270 ms over 20,007 Jira tickets where [`SEEN_WORDS`] took 2.5 s quiet and
/// 11 to 14 s on a loaded Mac (2026-11-17).
///
/// [`resolve_state`]: crate::payload::resolve_state
const SEEN_GROUPS: &str = "with o as materialized (
       select p->>'kind' as kind,
              array(select jsonb_array_elements_text(p->'at')) as segs,
              array(select array_to_string(array(select jsonb_array_elements_text(f.at)), chr(31))
                      from jsonb_array_elements(p->'fallback') with ordinality as f(at, n)
                     order by f.n) as fb
         from jsonb_array_elements($2::jsonb) p)
     select o.kind, btrim(x.p #>> o.segs) as word,
            (select coalesce(jsonb_agg(x.p #> string_to_array(f.s, chr(31)) order by f.n), '[]'::jsonb)
               from unnest(o.fb) with ordinality as f(s, n)) as vals,
            count(*) as n
       from o
       join sync.item i on i.source_id = $1 and i.kind = o.kind
      cross join lateral (select i.payload #> '{}'::text[] as p offset 0) x
      where jsonb_typeof(x.p #> o.segs) = 'string'
        and btrim(x.p #>> o.segs) <> ''
        and i.entity_id not in (select e.id from knobas.entity e where e.deleted_at is not null)
      group by 1, 2, 3";

/// The tone a group of [`SEEN_GROUPS`] falls back to: its values put where
/// the fallback candidates look, read by the one Rust rule.
fn fallback_tone(fallback: &[crate::payload::StatePath], vals: &serde_json::Value) -> Option<Tone> {
    let vals = vals.as_array()?;
    let mut payload = serde_json::Map::new();
    let candidates: Vec<crate::payload::StatePath> = fallback
        .iter()
        .enumerate()
        .map(|(n, candidate)| {
            if let Some(value) = vals.get(n) {
                payload.insert(n.to_string(), value.clone());
            }
            crate::payload::StatePath {
                at: PayloadPath::of([n.to_string()]),
                text_at: None,
                ..candidate.clone()
            }
        })
        .collect();
    crate::payload::resolve_state(&serde_json::Value::Object(payload), &candidates)?.tone
}

/// The stored spelling of a fallback tone: its word, `''` for none.
fn tone_word(tone: Option<Tone>) -> String {
    tone.and_then(|t| serde_json::to_value(t).ok())
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// Count `source`'s seen words again and store them in place of the last
/// count (`sync.state_word_seen`), with what they were counted under
/// (`sync.state_word_source`): after a run of the source wrote to the
/// mirror, and on a read that finds the source never counted or counted
/// under other declarations. Two counts of one source take turns.
///
/// # Errors
///
/// [`CoreError::Db`] if the read or the write fails.
pub async fn count_seen_words(pool: &PgPool, source: &SourceWords) -> Result<(), CoreError> {
    let param: Vec<serde_json::Value> = source
        .kinds
        .iter()
        .map(|k| {
            serde_json::json!({
                "kind": k.kind,
                "at": k.at,
                "fallback": k.fallback.iter().map(|c| &c.at).collect::<Vec<_>>(),
            })
        })
        .collect();
    let mut tx = pool.begin().await?;
    sqlx::query("select pg_advisory_xact_lock(hashtext('sync.state_word_seen'), hashtext($1))")
        .bind(&source.source_id)
        .execute(&mut *tx)
        .await?;
    let groups: Vec<(String, String, serde_json::Value, i64)> = sqlx::query_as(SEEN_GROUPS)
        .bind(&source.source_id)
        .bind(serde_json::Value::Array(param))
        .fetch_all(&mut *tx)
        .await?;
    let mut counted: std::collections::BTreeMap<(String, String, String), i64> =
        std::collections::BTreeMap::new();
    for (kind, word, vals, n) in groups {
        let fallback = source
            .kinds
            .iter()
            .find(|k| k.kind == kind)
            .map_or(&[][..], |k| k.fallback.as_slice());
        let tone = tone_word(fallback_tone(fallback, &vals));
        *counted.entry((kind, word, tone)).or_default() += n;
    }
    let (mut kinds, mut words, mut tones, mut counts) = (vec![], vec![], vec![], vec![]);
    for ((kind, word, tone), n) in counted {
        kinds.push(kind);
        words.push(word);
        tones.push(tone);
        counts.push(n);
    }
    sqlx::query("delete from sync.state_word_seen where source_id = $1")
        .bind(&source.source_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "insert into sync.state_word_seen (source_id, kind, word, tone, items)
         select $1, k, w, t, n from unnest($2::text[], $3::text[], $4::text[], $5::bigint[]) as u(k, w, t, n)",
    )
    .bind(&source.source_id)
    .bind(&kinds)
    .bind(&words)
    .bind(&tones)
    .bind(&counts)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "insert into sync.state_word_source (source_id, paths) values ($1, $2)
         on conflict (source_id) do update set paths = excluded.paths, counted_at = now()",
    )
    .bind(&source.source_id)
    .bind(source.paths())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

/// The sources of `sources` with no stored count under the declarations
/// they have now: never counted (the first read after the migration), or
/// counted under others (an adapter changed its declarations).
///
/// # Errors
///
/// [`CoreError::Db`] if the read fails.
pub async fn uncounted<'a>(
    pool: &PgPool,
    sources: &'a [SourceWords],
) -> Result<Vec<&'a SourceWords>, CoreError> {
    if sources.is_empty() {
        return Ok(Vec::new());
    }
    let ids: Vec<&str> = sources.iter().map(|s| s.source_id.as_str()).collect();
    let marks: Vec<(String, serde_json::Value)> = sqlx::query_as(
        "select source_id, paths from sync.state_word_source where source_id = any($1)",
    )
    .bind(&ids)
    .fetch_all(pool)
    .await?;
    Ok(sources
        .iter()
        .filter(|source| {
            let paths = source.paths();
            !marks
                .iter()
                .any(|(id, at)| *id == source.source_id && *at == paths)
        })
        .collect())
}

/// The words the mirror has seen at `sources`' open candidates, as
/// [`seen_words`] answers, read from the stored counts ([`count_seen_words`]):
/// a read of a few rows a source, never a walk of the mirror. A source with
/// no count ([`uncounted`]) offers no word; the caller counts it first. A
/// word's fallback is the most common among its items across the sources, a
/// tie going to no tone, then to the tone's word, as [`SEEN_WORDS`]'
/// `mode()` has it.
///
/// # Errors
///
/// [`CoreError::Db`] if the read fails.
pub async fn stored_seen_words(
    pool: &PgPool,
    sources: &[SourceWords],
) -> Result<Vec<SeenWord>, CoreError> {
    if sources.is_empty() {
        return Ok(Vec::new());
    }
    let ids: Vec<&str> = sources.iter().map(|s| s.source_id.as_str()).collect();
    let rows: Vec<(String, String, String, i64)> = sqlx::query_as(
        "select kind, word, tone, items from sync.state_word_seen where source_id = any($1)",
    )
    .bind(&ids)
    .fetch_all(pool)
    .await?;
    let mut tally: std::collections::BTreeMap<(String, String), Vec<(String, i64)>> =
        std::collections::BTreeMap::new();
    for (kind, word, tone, n) in rows {
        let tones = tally.entry((kind, word)).or_default();
        match tones.iter_mut().find(|(t, _)| *t == tone) {
            Some((_, total)) => *total += n,
            None => tones.push((tone, n)),
        }
    }
    Ok(tally
        .into_iter()
        .map(|((kind, word), mut tones)| {
            tones.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            let fallback = tones
                .first()
                .filter(|(t, _)| !t.is_empty())
                .and_then(|(t, _)| {
                    serde_json::from_value(serde_json::Value::String(t.clone())).ok()
                });
            SeenWord {
                kind,
                word,
                fallback,
            }
        })
        .collect())
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
                    text_at: None,
                    values: vec![StateValue::of("running", Tone::Info)],
                },
                StatePath {
                    at: PayloadPath::of(["status"]),
                    open: false,
                    text_at: None,
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

    /// A ticket as Jira declares it (#2468): its open status name, then its
    /// category, drawn as the name.
    fn ticket() -> KindPaths {
        KindPaths {
            kind: "ticket".to_owned(),
            state: vec![
                StatePath {
                    at: PayloadPath::of(["status", "name"]),
                    open: true,
                    text_at: None,
                    values: vec![StateValue::of("Done", Tone::Ok)],
                },
                StatePath {
                    at: PayloadPath::of(["status", "category"]),
                    open: false,
                    text_at: Some(PayloadPath::of(["status", "name"])),
                    values: vec![StateValue::of("done", Tone::Ok)],
                },
            ],
            ..KindPaths::default()
        }
    }

    /// A candidate whose chip says another path's string has no words of its
    /// own: an override for `done` is a status named `done`, added to the
    /// open candidate, and never retones the category's `done`.
    #[test]
    fn an_override_never_retones_a_candidate_drawn_as_another_path() {
        let kinds = merged(vec![ticket()], &[over("ticket", "done", ToneChoice::Err)]);
        assert_eq!(kinds[0].state[1], ticket().state[1]);
        assert_eq!(
            kinds[0].state[0].values,
            [
                StateValue::of("Done", Tone::Ok),
                StateValue::of("done", Tone::Err)
            ]
        );
    }

    /// What a seen word falls back to is what the candidates after the open
    /// one draw: the category, never the name it was seen at.
    #[test]
    fn the_fallback_is_the_candidates_after_the_open_one() {
        let fallback = fallback(&[ticket(), build()]);
        assert_eq!(fallback.len(), 1, "a kind with no open candidate has none");
        assert_eq!(fallback[0].kind, "ticket");
        assert_eq!(fallback[0].state, ticket().state[1..]);
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

//! The values the query box suggests for `kind:`, `source:` and `under:`
//! (#601, spec #591 stories 27-30).
//!
//! **Only values that exist right now**, which is a different list from "every
//! value the grammar would accept": `kind:` takes any word at face value and
//! answers a kind nothing emits with an empty result, so the catalog alone
//! would offer kinds that lead nowhere. So each list is read off what is there:
//!
//! * **kinds** -- every kind with a row in one of the launcher's corpora
//!   ([`crate::corpus::ALL`]), labelled from the [`KindCatalog`];
//! * **sources** -- the vocabulary's enabled sources, the ones `source:`
//!   resolves (a disabled one is nameable by nothing, #200), that have at
//!   least one live row: one configured but not synced yet would lead to an
//!   empty result (the deputy's ruling of 2026-10-12 on #614);
//! * **parents** -- every name a parent answers to, with its kind: the names
//!   live items declare for their parents and the names mirrored records
//!   answer to as one, read through the **same two `knobas_core::payload`
//!   reads** `under:` resolves through (the deputy's ruling of 2026-10-11 on
//!   #594), so a suggestion is a value `crate::under` knows; and every asset
//!   by its name, which is `crate::under`'s asset arm (#600: an asset answers
//!   to its name, and `under:` returns the asset itself, so even a leaf leads
//!   somewhere -- as a mirrored repo nothing sits in yet does).
//!
//! One read per launcher opening, never per keystroke: the box narrows the
//! lists itself as the reader types, so the hot path pays nothing for them.
//!
//! [`KindCatalog`]: crate::KindCatalog

use sqlx::PgPool;

use crate::types::{FilterSuggestions, KindSuggestion, ParentSuggestion, SourceSuggestion};
use crate::{SearchError, Vocabulary};

/// Every kind with at least one row, over the four corpora the launcher
/// unions.
///
/// The mirror is `sync.live_item`, so a withdrawn item or a disabled source
/// does not vouch for a kind the corpus no longer has. The three corpora
/// knobas owns are asked whether they hold anything at all -- their kind is a
/// constant, so one row is the whole answer. The route probe joins its asset
/// as [`crate::corpus::ROUTE`]'s relation does, so it says what the launcher
/// can actually return.
const KINDS_SQL: &str = "\
select k.kind from (
  select distinct i.kind from sync.live_item i
  union select 'note' where exists (select 1 from knobas.note)
  union select 'asset' where exists (select 1 from knobas.asset)
  union select 'route' where exists (
    select 1 from knobas.route r join knobas.asset ra on ra.id = r.asset_id)
) k
 order by k.kind";

/// The source ids bound at `$1` that have at least one row in the mirror.
///
/// One `exists` per id rather than a `distinct` over the corpus:
/// `item_source_updated_idx` leads with `source_id`, so each probe stops at
/// the source's first live row instead of reading every row there is.
const SOURCES_SQL: &str = "\
select s.id from unnest($1::text[]) s(id)
 where exists (select 1 from sync.live_item i where i.source_id = s.id)";

/// Every `(name, kind)` a parent answers to, through the declarations bound
/// at `$1`.
///
/// Three arms. The first two are `crate::under`'s `KNOWN_SQL` with the kind
/// kept: a parent the children declare -- mirrored or not, which is why the
/// kind is the declaration's (#594) -- and a mirrored record answering to its
/// sibling kinds' declared names. The third is the estate (ADR-0014): every
/// asset, answering to its name -- `KNOWN_SQL`'s asset arm, so here too a
/// suggested name is one `under:` knows.
const PARENTS_SQL: &str = concat!(
    "select p.name, p.kind from (",
    "select pn.name, pn.kind from sync.live_item i cross join lateral ",
    knobas_core::declared_parent_names!("$1"),
    " pn union select sn.name, i.kind from sync.live_item i cross join lateral ",
    knobas_core::declared_names_as_parent!("$1"),
    " sn union select a.name, 'asset' from knobas.asset a",
    ") p where nullif(btrim(p.name), '') is not null and nullif(p.kind, '') is not null \
     order by p.name, p.kind"
);

/// Read the three lists against `vocab`.
///
/// # Errors
///
/// [`SearchError::Db`] if any read fails.
pub(crate) async fn read(
    pool: &PgPool,
    vocab: &Vocabulary,
) -> Result<FilterSuggestions, SearchError> {
    let kinds: Vec<String> = sqlx::query_scalar(KINDS_SQL).fetch_all(pool).await?;
    let parents: Vec<(String, String)> = sqlx::query_as(PARENTS_SQL)
        .bind(vocab.declarations.as_param())
        .fetch_all(pool)
        .await?;
    let enabled: Vec<&str> = vocab
        .sources
        .iter()
        .filter(|source| source.enabled)
        .map(|source| source.id.as_str())
        .collect();
    let synced: Vec<String> = sqlx::query_scalar(SOURCES_SQL)
        .bind(&enabled)
        .fetch_all(pool)
        .await?;
    Ok(FilterSuggestions {
        kinds: kinds
            .into_iter()
            .map(|kind| KindSuggestion {
                label: vocab.kinds.info(&kind).label,
                kind,
            })
            .collect(),
        sources: vocab
            .sources
            .iter()
            .filter(|source| source.enabled && synced.contains(&source.id))
            .map(|source| SourceSuggestion {
                id: source.id.clone(),
                display_name: source.display_name.clone(),
            })
            .collect(),
        parents: parents
            .into_iter()
            .map(|(name, kind)| ParentSuggestion { name, kind })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A kind knobas owns that the probe does not ask about would never be
    /// suggested, however many rows it had -- the same reminder
    /// `corpus::tests::every_kind_knobas_owns_has_a_corpus_to_search` is.
    #[test]
    fn every_kind_knobas_owns_is_probed() {
        for owned in knobas_core::entity::OWNED_KINDS {
            assert!(
                KINDS_SQL.contains(&format!("select '{}'", owned.id)),
                "{:?} is a kind knobas owns and KINDS_SQL never asks for it",
                owned.id
            );
        }
    }
}

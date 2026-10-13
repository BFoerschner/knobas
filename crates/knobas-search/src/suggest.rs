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
//! * **parents** -- the newest names a parent answers to that start with the
//!   prefix being typed (#761), with their kind: the names
//!   live items declare for their parents and the names mirrored records
//!   answer to as one, read through the **same two `knobas_core::payload`
//!   reads** `under:` resolves through (the deputy's ruling of 2026-10-11 on
//!   #594), so a suggestion is a value `crate::under` knows; and every asset
//!   by its name, which is `crate::under`'s asset arm (#600: an asset answers
//!   to its name, and `under:` returns the asset itself, so even a leaf leads
//!   somewhere -- as a mirrored repo nothing sits in yet does).
//!
//! Read once per launcher opening for the kinds and sources, which are small,
//! and again per keystroke of an `under:` value for the parents, which are
//! not: the mirror holds a parent name per ticket key, so the engine answers a
//! prefix rather than shipping every name to be narrowed in the webview.
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
/// One probe per id, each **ordered as `item_source_updated_idx` is**
/// (`source_id, item_updated_at desc nulls last`), so the planner walks that
/// index and stops at the source's first live row. A plain `exists` does
/// not: measured on the 100k fixture (#622's review), the planner hashed every
/// row of each source and took 30 ms at p50, the same as a `distinct` over the
/// corpus, against 0.3 ms for this shape. An unordered `limit 1` seq-scanned
/// to the same 30 ms once an enabled source with no rows was in the list.
const SOURCES_SQL: &str = "\
select s.id from unnest($1::text[]) s(id)
 cross join lateral (
  select 1 from sync.live_item i
   where i.source_id = s.id
   order by i.item_updated_at desc nulls last
   limit 1) l";

/// How many parents one prefix is answered with (#761): the newest this many
/// whose name starts with what was typed.
///
/// The box draws at most eight of them (`SUGGESTION_LIMIT` in
/// `app/src/lib/launcher/suggest.ts`) and narrows the last answer itself while
/// the next one is in flight, so twenty leave it something to narrow without
/// shipping the mirror.
pub const PARENT_SUGGESTIONS: i64 = 20;

/// The newest [`PARENT_SUGGESTIONS`] `(name, kind)`s a parent answers to whose
/// name starts with `$1`, ignoring case (#761, the deputy's ruling of
/// 2026-10-14). Before it, every parent name the mirror had was shipped once
/// per opening and narrowed in the webview -- 274-430 ms and 680 KB on the
/// 22,000-item responsiveness probe (#753), most of it ticket keys nobody
/// typed.
///
/// Two steps, so the cost does not hang on the planner's statistics:
///
/// 1. **Rank** every `(name, kind)` with the prefix by the newest mirrored
///    item that names it -- `sync.item_parent` joined to `sync.item` alone,
///    and every asset by its own time -- sorted. The `offset 0` keeps step 2
///    from being pushed beneath the sort.
/// 2. **Offer** them in that order while [`crate::under::parent_names!`] --
///    the fragment `under:`'s own lookup reads -- has the same name and kind,
///    up to the limit. So an offered name is one `under:` knows (#615); and
///    since step 1's rows are a superset of that fragment's, every name it
///    knows is ranked, and is offered for a prefix specific enough.
///
/// Joining every row to `sync.live_item` before ranking was 58 ms at the
/// probe's size with fresh statistics and 306 ms without (the join to
/// `knobas.source_config` on two columns is underestimated a hundredfold and
/// the planner picks nested loops); this shape is 40-51 ms either way.
///
/// **What the ranking concedes**: a name is ranked by its newest item, live or
/// not. A name whose newest item was withdrawn, or sits in a turned-off
/// source, while an older live one still answers to it, is offered a little
/// higher than its live items alone would put it. It is still offered only
/// when a live item answers to it.
///
/// The range on `~>=~` / `~<~` is the one `item_parent_name_prefix_idx`
/// (migration `0032`, `text_pattern_ops`) serves, in byte order whatever the
/// database's collation, and it is a parameter range, so a generic plan keeps
/// it -- a `like $1 || '%'` would not. The upper bound appends the highest code
/// point, so a name that continues the prefix with U+10FFFF itself falls
/// outside it; that is a private-use character no source spells a parent
/// with. `starts_with` is the exact test. `lower` on both sides, in SQL: the
/// lowering `under::KNOWN_SQL` compares with. A name the trim leaves empty, or
/// a row without a kind, is not offered.
const PARENTS_SQL: &str = concat!(
    "select g.name, g.kind from ( \
       select p.name, p.kind, max(p.at) as top from ( \
         select ip.name, ip.kind, coalesce(i.item_updated_at, i.synced_at) as at \
           from sync.item_parent ip join sync.item i on i.entity_id = ip.entity_id \
         union all select a.name, 'asset', a.updated_at from knobas.asset a) p \
        where lower(p.name) ~>=~ lower($1) \
          and lower(p.name) ~<~ (lower($1) || chr(1114111)) \
          and starts_with(lower(p.name), lower($1)) \
          and nullif(btrim(p.name), '') is not null and nullif(p.kind, '') is not null \
        group by p.name, p.kind \
        order by top desc nulls last, p.name, p.kind \
       offset 0) g \
     where exists (select 1 from ",
    crate::under::parent_names!(),
    " k where lower(k.name) = lower(g.name) and k.name = g.name and k.kind = g.kind) \
     limit $2"
);

/// Read the three lists against `vocab`, the parents for the prefix `under`
/// (empty for none).
///
/// # Errors
///
/// [`SearchError::Db`] if any read fails.
pub(crate) async fn read(
    pool: &PgPool,
    vocab: &Vocabulary,
    under: &str,
) -> Result<FilterSuggestions, SearchError> {
    let kinds: Vec<String> = sqlx::query_scalar(KINDS_SQL).fetch_all(pool).await?;
    let parents: Vec<(String, String)> = sqlx::query_as(PARENTS_SQL)
        .bind(under)
        .bind(PARENT_SUGGESTIONS)
        // The unnamed statement, planned with the prefix in hand every time.
        // A named one is planned generically from its sixth execution in a
        // session, and a generic plan cannot see how narrow the prefix range
        // is: `under:j` went from 50 ms cold to 313 ms warm on the
        // responsiveness probe. The launcher's search has the same remedy
        // for the same reason (`sql::query_as_with`). Pinned by
        // `the_parents_statement_is_never_a_named_prepared_statement`.
        .persistent(false)
        .fetch_all(pool)
        .await?;
    let enabled: Vec<&str> = vocab
        .sources
        .iter()
        .filter(|source| source.enabled)
        .map(|source| source.id.as_str())
        .collect();
    // Only the enabled ids are probed, so `live` holds enabled sources only.
    let live: Vec<String> = sqlx::query_scalar(SOURCES_SQL)
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
            .filter(|source| live.contains(&source.id))
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

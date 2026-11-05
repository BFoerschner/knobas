//! Which `under:` values name a parent at all (ADR-0018, #595), and which
//! typed tokens explain an empty answer by what the launcher left out (#2488).
//!
//! The grammar cannot say: a parent lives in its children's records -- the
//! repo a commit sits in is in the commit -- and need not be mirrored itself.
//! So "a value no declared name matches is reported as an unknown token" is a
//! question for the mirror, asked after the parse and only for a query that
//! used `under:`, so an ordinary keystroke pays nothing for it.
//!
//! **Through the two `knobas_core::payload` reads and nothing else**, the ones
//! the statement's predicate uses (the deputy's ruling of 2026-10-11 on #594):
//! a value is known if some live item declares a parent answering to it, or is
//! a mirrored parent answering to it. Since #627 the database runs those reads
//! when an item or a declaration changes and keeps their answer in
//! `sync.item_parent` (migration `0028`, ADR-0019), so a query reads an index
//! instead of walking the mirror. Every source's rows are asked, whatever
//! the query's `source:` or kind scope: a name that exists outside the scope is
//! not a typo, and the empty result such a query gets is a true one.
//!
//! **And the estate's names, read off the column** (#600): an asset answers to
//! its name, and a parent that is an asset declares nothing through a payload
//! -- its children point at it by `parent_id` -- so `knobas.asset.name` is the
//! whole of what the estate answers to. A route is no one's parent.

use sqlx::PgPool;

use crate::corpus::Corpus;
use crate::query::{EffectiveFilters, Parsed};
use crate::{RawHit, SearchError};

/// Every `(name, kind)` a parent answers to, as a table expression -- the one
/// place the parent arms are put together, read by [`crate::sql::known_parent_positions`] and by
/// `crate::suggest`'s `PARENTS_SQL`, so what `under:` knows and what the box
/// offers for it cannot drift apart (#615).
///
/// Two arms. **What the mirror's items answer to**, read off
/// `sync.item_parent` (migration `0028`, #627): the parents a live item
/// declares -- mirrored or not, which is why the kind is the declaration's
/// (#594) -- and the names a live item answers to as a parent of its own
/// kind. The database keeps that table to what the declarations say, through
/// `knobas_core`'s `declared_parent_names!` and `declared_names_as_parent!`,
/// so these rows are exactly what those two macros yielded per item when this
/// fragment walked them itself -- which it did on every keystroke, at 335 ms
/// for 100,000 items. And **the estate**: every asset, answering to its name
/// as `asset`.
///
/// Joined to `sync.live_item`, so a withdrawn item or a disabled source does
/// not vouch for a name the corpus no longer has -- the same reason the
/// coverage probe reads it; the table holds every item, live or not. And
/// joined to `knobas.source_config` on the row's `adapter_kind`: a stored row
/// says what one adapter's declarations make of the item, and it counts only
/// while the item's source is configured with that adapter --
/// `Vocabulary::declaring`'s rule, applied at read time so that no stored row
/// depends on a config row (the deputy's second ruling on #627). A
/// deleted asset's row is gone, so `knobas.asset` needs no such view. The
/// asset arm is the name `corpus::estate_subtree!` starts its walk from, and a
/// reader's `lower(name)` filter reaches it as `lower(a.name)`, the same
/// spelling: change one and the other must follow.
macro_rules! parent_names {
    () => {
        "(select ip.name, ip.kind from sync.item_parent ip \
           join sync.live_item i on i.entity_id = ip.entity_id \
           join knobas.source_config s on s.id = i.source_id and s.kind = ip.adapter_kind \
         union all select a.name, 'asset' from knobas.asset a)"
    };
}
pub(crate) use parent_names;

/// Report every `under:` value no parent answers to as an unknown token, in
/// the spelling the reader typed it, and hand back the values one does answer
/// to, for [`left_out_tokens`].
///
/// The value stays in the filter: `under:a under:typo` still scopes to `a`,
/// and `under:typo` alone answers with nothing -- which is true -- while the
/// greyed-out token says why.
///
/// # Errors
///
/// [`SearchError::Db`] if the lookup fails.
pub(crate) async fn report_unknown(
    pool: &PgPool,
    parsed: &mut Parsed,
) -> Result<Vec<(String, String)>, SearchError> {
    if parsed.under.is_empty() {
        return Ok(Vec::new());
    }
    let values: Vec<String> = parsed
        .under
        .iter()
        .map(|(_, value)| value.clone())
        .collect();
    let positions = crate::sql::known_parent_positions(pool, &values).await?;
    let (known, unknown): (Vec<_>, Vec<_>) = parsed
        .under
        .iter()
        .cloned()
        .zip(1_i64..)
        .partition(|(_, position)| positions.contains(position));
    for ((token, _), _) in unknown {
        parsed.unknown(&token);
    }
    Ok(known.into_iter().map(|(under, _)| under).collect())
}

/// The typed tokens that explain an empty launcher answer by what it leaves
/// out (#2488, [`crate::frames`]), in the spelling typed: each `kind:` naming
/// a left-out kind, then each name in `known_under` the launcher's own
/// statement admits no row beneath.
///
/// Read only after an empty answer, so an answer that holds rows pays nothing.
/// The launcher's statement runs once more per value in `known_under`, one at
/// a time, and a query's length bounds how many there are. A name no parent
/// answers to is not in it, which is what a name still being typed is at every
/// keystroke, so typing one pays nothing here. A known name beneath which the
/// statement admits nothing has only left-out rows there, since the known name
/// came from a live row. The statement is asked with nothing beside the name,
/// so what `under:` admits and what this says cannot drift: the words and the
/// `source:` and `kind:` scope are not asked, as [`report_unknown`] does not
/// ask them, because a word matching nothing beneath the name is not what the
/// launcher left out.
///
/// # Errors
///
/// [`SearchError::Db`] if a statement fails.
pub(crate) async fn left_out_tokens(
    pool: &PgPool,
    corpora: &[&Corpus],
    parsed: &Parsed,
    known_under: &[(String, String)],
    filters: &EffectiveFilters,
) -> Result<Vec<String>, SearchError> {
    let mut tokens: Vec<String> = parsed
        .typed_kinds
        .iter()
        .filter(|(_, kind)| filters.hidden_kinds.contains(kind))
        .map(|(token, _)| token.clone())
        .collect();
    for (token, value) in known_under {
        let answered = EffectiveFilters {
            under: vec![value.clone()],
            declarations: filters.declarations.clone(),
            hidden_kinds: filters.hidden_kinds.clone(),
            ..EffectiveFilters::default()
        };
        if !admits_a_row(pool, corpora, &answered).await? {
            tokens.push(token.clone());
        }
    }
    Ok(tokens)
}

/// Whether the launcher's statement admits any row under `filters`.
async fn admits_a_row(
    pool: &PgPool,
    corpora: &[&Corpus],
    filters: &EffectiveFilters,
) -> Result<bool, SearchError> {
    let built = crate::sql::search_sql(corpora, None, false, filters, 1, 1);
    let rows: Vec<RawHit> = crate::sql::query_as_with(built).fetch_all(pool).await?;
    Ok(!rows.is_empty())
}

//! Which `under:` values name a parent at all (ADR-0018, #595), which of
//! those have nothing the launcher admits beneath them (#2495), and which
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

use crate::query::{EffectiveFilters, Parsed};
use crate::SearchError;

/// Every `(name, kind)` a parent answers to, as a table expression -- the one
/// place the parent arms are put together, read by [`crate::sql::parent_positions`] and by
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

/// What the typed `under:` values came to: the ones a parent answers to, and
/// among those the ones with nothing the launcher admits beneath them.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct KnownUnder {
    /// Each `(token, value)` a parent answers to, in the order typed.
    pub known: Vec<(String, String)>,
    /// The tokens among `known` beneath which the launcher admits no row: a
    /// name only left-out kinds sit under (#2488), in the order typed.
    pub left_out: Vec<String>,
}

impl KnownUnder {
    /// Whether the launcher admits nothing beneath any of the typed values:
    /// every known one is left out, and an unknown one has nothing beneath
    /// it by definition. Such a query answers with nothing before the
    /// statement is asked (#2495).
    pub fn admits_nothing(&self) -> bool {
        self.left_out.len() == self.known.len()
    }
}

/// Report every `under:` value no parent answers to as an unknown token, in
/// the spelling the reader typed it, and hand back the values one does answer
/// to, with the ones the launcher admits nothing beneath (its `hidden` kinds
/// left out) named for [`left_out_tokens`].
///
/// One bounded read for both questions ([`crate::sql::parent_positions`]),
/// so an `under:` keystroke pays an index probe per value and no more: a
/// name still being typed answers to nothing and the probe stops at that.
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
    hidden: &[String],
) -> Result<KnownUnder, SearchError> {
    if parsed.under.is_empty() {
        return Ok(KnownUnder::default());
    }
    let values: Vec<String> = parsed
        .under
        .iter()
        .map(|(_, value)| value.clone())
        .collect();
    let positions = crate::sql::parent_positions(pool, &values, hidden).await?;
    let mut known_under = KnownUnder::default();
    let typed: Vec<(String, String)> = parsed.under.clone();
    for ((token, value), position) in typed.into_iter().zip(1_i64..) {
        if !positions.known.contains(&position) {
            parsed.unknown(&token);
            continue;
        }
        if !positions.admitting.contains(&position) {
            known_under.left_out.push(token.clone());
        }
        known_under.known.push((token, value));
    }
    Ok(known_under)
}

/// The typed tokens that explain an empty launcher answer by what it leaves
/// out (#2488, [`crate::frames`]), in the spelling typed: each `kind:` naming
/// a left-out kind, then each `under:` name the launcher admits no row
/// beneath, as [`report_unknown`] found them.
///
/// A known name beneath which nothing is admitted has only left-out rows
/// there, since the known name came from a live row. It was asked with
/// nothing beside the name: the words and the `source:` and `kind:` scope are
/// not asked, as [`report_unknown`] does not ask them, because a word
/// matching nothing beneath the name is not what the launcher left out. And
/// it was asked of the launcher's own `under:` predicate, arm for arm
/// ([`crate::sql::parent_positions`]), so what `under:` admits and what this
/// says cannot drift.
pub(crate) fn left_out_tokens(
    parsed: &Parsed,
    known_under: &KnownUnder,
    filters: &EffectiveFilters,
) -> Vec<String> {
    let mut tokens: Vec<String> = parsed
        .typed_kinds
        .iter()
        .filter(|(_, kind)| filters.hidden_kinds.contains(kind))
        .map(|(token, _)| token.clone())
        .collect();
    tokens.extend(known_under.left_out.iter().cloned());
    tokens
}

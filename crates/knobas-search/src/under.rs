//! Which `under:` values name a parent at all (ADR-0018, #595).
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

use crate::SearchError;
use crate::query::Parsed;

/// Every `(name, kind)` a parent answers to, as a table expression -- the one
/// place the parent arms are put together, read by [`KNOWN_SQL`] here and by
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
/// coverage probe reads it; the table holds every item, live or not. A
/// deleted asset's row is gone, so `knobas.asset` needs no such view. The
/// asset arm is the name `corpus::estate_subtree!` starts its walk from, and a
/// reader's `lower(name)` filter reaches it as `lower(a.name)`, the same
/// spelling: change one and the other must follow.
macro_rules! parent_names {
    () => {
        "(select ip.name, ip.kind from sync.item_parent ip \
           join sync.live_item i on i.entity_id = ip.entity_id \
         union all select a.name, 'asset' from knobas.asset a)"
    };
}
pub(crate) use parent_names;

/// The distinct lowercased names among `$1` that a parent answers to:
/// [`parent_names!`] without the kind. `lower(ip.name)` is
/// `item_parent_name_idx`'s expression, so the mirror's arm is an index read.
const KNOWN_SQL: &str = concat!(
    "select distinct lower(p.name) from ",
    parent_names!(),
    " p where lower(p.name) = any($1)"
);

/// Report every `under:` value no parent answers to as an unknown token, in
/// the spelling the reader typed it.
///
/// The value stays in the filter: `under:a under:typo` still scopes to `a`,
/// and `under:typo` alone answers with nothing -- which is true -- while the
/// greyed-out token says why.
///
/// # Errors
///
/// [`SearchError::Db`] if the lookup fails.
pub(crate) async fn report_unknown(pool: &PgPool, parsed: &mut Parsed) -> Result<(), SearchError> {
    if parsed.under.is_empty() {
        return Ok(());
    }
    let values: Vec<String> = parsed
        .under
        .iter()
        .map(|(_, value)| value.clone())
        .collect();
    let known: Vec<String> = sqlx::query_scalar(KNOWN_SQL)
        .bind(&values)
        .fetch_all(pool)
        .await?;
    let unknown: Vec<String> = parsed
        .under
        .iter()
        .filter(|(_, value)| !known.contains(value))
        .map(|(token, _)| token.clone())
        .collect();
    for token in unknown {
        parsed.unknown(&token);
    }
    Ok(())
}

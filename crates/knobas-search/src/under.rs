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
//! a mirrored parent answering to it. Every source's rows are asked, whatever
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

/// Every `(name, kind)` a parent answers to, through the declarations bound
/// at `$decl`, as a table expression -- the one place the parent shapes are
/// written, read by [`KNOWN_SQL`] here and by `crate::suggest`'s
/// `PARENTS_SQL`, so what `under:` knows and what the box offers for it
/// cannot drift apart (#615).
///
/// Three arms. A parent the live children declare -- mirrored or not, which
/// is why the kind is the declaration's (#594). A live item answering to its
/// sibling kinds' declared names, as its own kind. And the estate: every
/// asset, answering to its name as `asset`.
///
/// `sync.live_item`, so a withdrawn item or a disabled source does not vouch
/// for a name the corpus no longer has -- the same reason the coverage probe
/// reads it. A deleted asset's row is gone, so `knobas.asset` needs no such
/// view. The asset arm is the name `corpus::estate_subtree!` starts its walk
/// from, and a reader's `lower(name)` filter reaches it as `lower(a.name)`,
/// the same spelling: change one and the other must follow.
///
/// The two payload arms share one scan of `sync.live_item`, through one
/// lateral: the query path pays for one pass over the mirror, not two.
macro_rules! parent_names {
    ($decl:literal) => {
        concat!(
            "(select n.name, n.kind from sync.live_item i cross join lateral (select pn.name, pn.kind from ",
            knobas_core::declared_parent_names!($decl),
            " pn union all select sn.name, i.kind from ",
            knobas_core::declared_names_as_parent!($decl),
            " sn) n union all select a.name, 'asset' from knobas.asset a)"
        )
    };
}
pub(crate) use parent_names;

/// The distinct lowercased names among `$2` that a parent answers to, through
/// the declarations bound at `$1`: [`parent_names!`] without the kind.
const KNOWN_SQL: &str = concat!(
    "select distinct lower(p.name) from ",
    parent_names!("$1"),
    " p where lower(p.name) = any($2)"
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
        .bind(parsed.declarations.as_param())
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

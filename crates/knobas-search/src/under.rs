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

use sqlx::PgPool;

use crate::SearchError;
use crate::query::Parsed;

/// The distinct lowercased names among `$2` that any live item answers to as
/// a parent or names as one, read through the declarations bound at `$1`.
///
/// `sync.live_item`, so a withdrawn item or a disabled source does not vouch
/// for a name the corpus no longer has -- the same reason the coverage probe
/// reads it.
const KNOWN_SQL: &str = concat!(
    "select distinct lower(n.name) from sync.live_item i cross join lateral (select pn.name from ",
    knobas_core::declared_parent_names!("$1"),
    " pn union all select sn.name from ",
    knobas_core::declared_names_as_parent!("$1"),
    " sn) n where lower(n.name) = any($2)"
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

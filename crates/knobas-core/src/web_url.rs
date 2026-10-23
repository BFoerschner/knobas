//! The one spelling of "these two URLs name the same thing".
//!
//! A reader pastes a link from chat into the launcher and expects the entity
//! it names to open (spec #491 stories 9-11, 15-17). The mirror already holds
//! the address every adapter reported, in `sync.item.web_url`; what it did not
//! hold until issue #496 was a way to *find* a row by it. This module is that
//! rule, as SQL, in one place.
//!
//! # Why the rule is SQL and not Rust
//!
//! Matching happens on **both** sides -- the stored URL and the pasted one --
//! and it is backed by an expression index, so the rule exists in the database
//! whatever else happens. A Rust twin normalising the pasted side would be a
//! second implementation of one rule, in a different language, that nothing
//! forces to agree; #284's `ancestor_path_read!` was written against the
//! database for the same reason, and `knobas-core/tests/it/ancestor_path.rs`
//! still pins the path line that replaced it (#646) the same way. So the
//! pasted URL is bound as a parameter and put through
//! [`web_url_normalized!`] too, and the two sides cannot disagree because
//! they are the same characters.
//!
//! The migration that creates the index carries a **third** copy, because an
//! index definition cannot expand a Rust macro. That copy is not load-bearing
//! for correctness: if it drifted, the query would still answer correctly and
//! would merely stop using the index. It is load-bearing for *speed*, which is
//! why `crates/knobas-app/tests/it/url_resolve.rs` asks the planner whether the
//! shipped statement still reaches the index -- the same check
//! `the_view_still_reaches_the_fts_index` makes for the launcher's GIN index.
//!
//! # The rule
//!
//! Four normalisations, and no fifth (spec story 11):
//!
//! * **The fragment goes.** `#comment-42` addresses a place inside a page, not
//!   a different page.
//! * **A trailing slash goes**, from the *path* -- `…/PAY-231/` and
//!   `…/PAY-231` are one ticket.
//! * **The host is lower-cased**, with the scheme, because a host is
//!   case-insensitive and `JIRA.example` is typed by autocomplete more often
//!   than by hand.
//! * **The query stays, verbatim.** A Confluence
//!   `viewpage.action?pageId=42` link's identity *is* its query, so a
//!   normalisation that dropped it would resolve every page of an instance to
//!   whichever one was indexed first. This is the one place where doing less
//!   is the requirement rather than the shortcut.
//!
//! Nothing else: no percent-decoding, no default-port removal, no query
//! reordering, no `www.` folding. Each of those is a claim about what two URLs
//! mean, and the ticket asks for exact matching after the four rules above.
//!
//! # The failure direction is absence
//!
//! **The same direction ADR-0007 fixes for payload reads, borrowed rather
//! than owed.** That ADR governs reads *into a payload* outside an adapter,
//! and `web_url` is a plain column, so nothing here is bound by it; what is
//! worth keeping is its first requirement, because the hazard is the same
//! shape. So this one **misses rather than guesses**. A value that is not an
//! absolute URL --
//! anything without a `scheme://` -- normalises to SQL `null`, because both
//! `substring`s fail to match and `null || null` is `null`. A `null` index
//! entry equals nothing, so such a row is unreachable by paste rather than
//! reachable by accident, and `crates/knobas-core/tests/it/web_url.rs` pins that
//! direction across the shapes a mirror can hold.

/// SQL for the *trailing slash and fragment* half of the rule, over a URL
/// expression.
///
/// One `regexp_replace` does both, and keeps the query, because all three
/// live at the tail of a URL in a fixed order: optional slashes, an optional
/// `?query`, an optional `#fragment`, end of string. The pattern is anchored
/// on `$` and the engine takes the leftmost match, so on
/// `https://h/x?a=b/` the match starts at the `?` and the whole query --
/// trailing slash and all -- is put back by `\1`.
///
/// Not part of the public rule on its own; [`web_url_normalized!`] is what a
/// caller wants. It is a macro of its own only so that the expression appears
/// once rather than three times inside its sibling.
#[macro_export]
macro_rules! web_url_tail_trimmed {
    ($url:literal) => {
        concat!("regexp_replace(", $url, r", '/*(\?[^#]*)?(#.*)?$', '\1')")
    };
}

/// SQL for "this URL, as the resolver compares it" -- see the module docs for
/// the rule and for why it is not written in Rust.
///
/// `$url` is the URL expression as a **literal** (`"i.web_url"`, `"$1::text"`),
/// because that is what `concat!` folds: every statement built with this is a
/// `&'static str`, so nothing here can concatenate a value into SQL. The same
/// convention every declared payload read in `payload.rs` follows.
///
/// The expression is `IMMUTABLE` throughout -- `regexp_replace`, `substring`,
/// `lower` and `||` all are -- which is what lets an index be built on it.
///
/// The two `substring`s split the URL at the end of its authority: the first
/// takes `scheme://host[:port]` and lower-cases it, the second takes
/// everything after it verbatim. Splitting rather than lower-casing the whole
/// string is the point -- a Jira key is upper-case (`/browse/PAY-231`), a
/// Gitea path is case-sensitive, and folding either would make two different
/// pages one.
#[macro_export]
macro_rules! web_url_normalized {
    ($url:literal) => {
        concat!(
            "(lower(substring(",
            $crate::web_url_tail_trimmed!($url),
            " from '^[^:/?#]+://[^/?#]*')) || substring(",
            $crate::web_url_tail_trimmed!($url),
            " from '^[^:/?#]+://[^/?#]*(.*)$'))"
        )
    };
}

// -- the URL a paste names (moved from the app's command module, #1870) -----

/// What a pasted URL resolved to: the entity, and the kind that says where it
/// opens (issue #496).
///
/// Deliberately not an [`EntityRow`] and not an [`EntityDetail`]. A paste
/// answers one question -- *which entity is this link* -- and the frontend
/// then navigates to that entity's own address, at which point the detail is
/// fetched by the read that already exists. Answering with a row here would
/// make every paste pay for a title and a path nothing on the way draws, and
/// would put a second statement in the tree that has to keep in step with
/// `sync.live_item`'s columns.
///
/// `kind` rather than only the id because an entity id names a *namespace*
/// and a key (`gitea:acme/svc#142`), not a kind, and the address an entity
/// opens at is `#/<kind>/<id>` -- or, for an asset or a route, the Tree's own
/// encoder. The frontend has one function that turns the pair into an
/// address; it has nothing that turns an id alone into one.
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[cfg_attr(feature = "test-util", derive(serde::Deserialize))]
#[ts(export_to = "entity.ts")]
pub struct UrlMatch {
    pub entity_id: String,
    pub kind: String,
}

/// The one statement that reads `sync.item.web_url` (#496), and the forms
/// an adapter declares beside it (#1475).
///
/// **`sync.item`, not `sync.live_item`**, and for the two reasons
/// [`crate::detail`]'s statement reaches past that view as well: a withdrawn entity still opens
/// and says it is gone (spec #491 story 15 -- *"a stale link explains
/// itself"*), and a disabled source's items still open. It is the **third**
/// reader to do so, and `CONTEXT.md`'s **Live item** entry is the list of
/// them: a fourth owes a reason of its own and a line there. Resolving to a
/// tombstone and letting the detail's banner do the explaining is a better
/// answer than *Not in the mirror*, which would send the reader to the
/// browser to discover the same thing.
///
/// **Two arms, one read.** The first is the stored address: both sides of
/// the comparison go through `web_url_normalized!` -- the stored column and
/// the pasted parameter -- so there is exactly one rule and it cannot be
/// applied to one side only. `crates/knobas-core/src/web_url.rs` carries the
/// rule and the reasoning; migration `0023` carries the index.
/// `where i.web_url is not null` is the index's own predicate, repeated so the
/// planner may use the partial index.
///
/// The second is `$2`, the pasted address read as each declared
/// `knobas_source::url_forms::UrlForm` (the app's `form_readings`): a base
/// URL, an adapter kind, an entity kind and the payload values the item must
/// hold. The base is held to a configured source's `base_url` under the same
/// rule as the first arm, so a form never needs a second spelling of it; the
/// values are compared with `#>>` on that source's items of that kind. This
/// is a payload read outside an adapter (ADR-0007), and its failure direction
/// is the **miss**: a path the item does not hold is `null`, which is
/// distinct from every value, so the item is not named. A reading with no
/// values is skipped rather than allowed to name every item; `read` never
/// makes one, and `a_reading_with_no_values_names_nothing` pins the guard.
///
/// `order by via_form, entity_id limit 1`: the stored address wins over a form,
/// and nothing stops two mirrored items reporting one address, so an
/// ambiguous paste must answer the same entity on every run rather than
/// whichever row the plan reached first.
///
/// `$2` is `[]` for a caller that wants the stored address alone.
pub const RESOLVE_URL: &str = concat!(
    "select found.entity_id, found.kind
       from (
         select i.entity_id, i.kind, 0 as via_form
           from sync.item i
          where i.web_url is not null
            and ",
    crate::web_url_normalized!("i.web_url"),
    " = ",
    crate::web_url_normalized!("$1::text"),
    "
         union all
         select i.entity_id, i.kind, 1 as via_form
           from jsonb_to_recordset($2::jsonb)
                  as r(adapter_kind text, base text, kind text, fields jsonb)
           join knobas.source_config s
             on s.kind = r.adapter_kind
            and ",
    crate::web_url_normalized!("s.base_url"),
    " = ",
    crate::web_url_normalized!("r.base"),
    "
           join sync.item i
             on i.source_id = s.id
            and i.kind = r.kind
          where jsonb_array_length(r.fields) > 0
            and not exists (
                  select 1
                    from jsonb_to_recordset(r.fields) as f(path text[], value text)
                   where i.payload #>> f.path is distinct from f.value)
       ) found
      order by found.via_form, found.entity_id
      limit 1"
);

/// The entity `url` names, by its stored address or by one of `forms` (the
/// readings of `url` under each declared form, [`RESOLVE_URL`]'s `$2`), or
/// `None` -- *"a URL in, an entity reference or a miss out"* (spec #491).
/// One read on the local mirror and nothing else.
///
/// A `sqlx::Error` rather than a [`crate::CoreError`]: a read, whose one
/// failure is the statement's, and whose callers (the paste command and the
/// sources' commit-page look-ups) each classify one already.
///
/// # Errors
///
/// The statement's.
pub async fn resolve<F: serde::Serialize + Sync>(
    pool: &sqlx::PgPool,
    url: &str,
    forms: &[F],
) -> Result<Option<UrlMatch>, sqlx::Error> {
    let row: Option<(String, String)> = sqlx::query_as(RESOLVE_URL)
        .bind(url)
        .bind(sqlx::types::Json(forms))
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|(entity_id, kind)| UrlMatch { entity_id, kind }))
}

/// [`resolve`] by the stored address alone, with no form read.
///
/// # Errors
///
/// The statement's.
pub async fn resolve_stored(
    pool: &sqlx::PgPool,
    url: &str,
) -> Result<Option<UrlMatch>, sqlx::Error> {
    resolve::<serde_json::Value>(pool, url, &[]).await
}

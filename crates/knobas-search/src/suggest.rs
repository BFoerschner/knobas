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
//! Read once per launcher opening for the kinds and sources, which are small
//! ([`crate::Searcher::filter_suggestions`]), and again per keystroke of an
//! `under:` value for the parents alone, which are not
//! ([`crate::Searcher::filter_parent_suggestions`], #768): the mirror holds a parent name per ticket key, so the engine answers a
//! prefix rather than shipping every name to be narrowed in the webview.
//!
//! [`KindCatalog`]: crate::KindCatalog

use sqlx::PgPool;

use crate::types::{
    FieldSuggestion, FilterSuggestions, KindSuggestion, ParentSuggestion, SourceSuggestion,
};
use crate::{FieldKey, SearchError, Vocabulary};

/// Every kind with at least one row, over the four corpora the launcher
/// unions.
///
/// The mirror's kinds are `sync.live_item`'s -- a withdrawn item or a disabled
/// source does not vouch for a kind the corpus no longer has -- but read
/// **without that view**, in two steps whose cost does not hang on the
/// planner's statistics (#768):
///
/// 1. The distinct kinds of `sync.item`, by a recursive skip scan of
///    `item_kind_updated_idx`: the smallest kind, then the smallest above the
///    last.
/// 2. Per kind, one probe **ordered as that index is** (`kind,
///    item_updated_at desc nulls last`) that stops at the kind's first live
///    row. Liveness is two correlated subqueries in the `where`, which the
///    planner cannot turn into joins, so it cannot drive the plan from them.
///
/// The view's join did exactly that. On tables never analyzed `entity` is
/// estimated at 84 rows for its `deleted_at is null` (22,000 on the probe), the
/// planner drove from it, and the read took 90 ms; `SOURCES_SQL` took 520 ms
/// the same way. This shape measured 0.3 to 0.6 ms never analyzed and
/// analyzed alike, under a custom plan and a generic one. A `distinct` over
/// the view took 8 ms even with statistics.
///
/// The three corpora knobas owns are asked whether they hold anything at all
/// -- their kind is a constant, so one row is the whole answer. The route
/// probe joins its asset as [`crate::corpus::ROUTE`]'s relation does, so it
/// says what the launcher can actually return.
const KINDS_SQL: &str = "\
with recursive mirror_kinds(kind) as (
  (select i.kind from sync.item i order by i.kind limit 1)
  union all
  select (select i.kind from sync.item i
           where i.kind > m.kind order by i.kind limit 1)
    from mirror_kinds m where m.kind is not null
)
select k.kind from (
  select m.kind from mirror_kinds m
   cross join lateral (
    select 1 from sync.item i
     where i.kind = m.kind
       and (select e.deleted_at is null from knobas.entity e where e.id = i.entity_id)
       and coalesce((select c.enabled from knobas.source_config c where c.id = i.source_id), true)
     order by i.item_updated_at desc nulls last
     limit 1) l
   where m.kind is not null
  union select 'note' where exists (select 1 from knobas.note)
  union select 'asset' where exists (select 1 from knobas.asset)
  union select 'route' where exists (
    select 1 from knobas.route r join knobas.asset ra on ra.id = r.asset_id)
) k
 order by k.kind";

/// The source ids bound at `$1` that have at least one live row in the mirror.
///
/// One probe per id, each **ordered as `item_source_updated_idx` is**
/// (`source_id, item_updated_at desc nulls last`), so the planner walks that
/// index and stops at the source's first live row. A plain `exists` does
/// not: measured on the 100k fixture (#622's review), the planner hashed every
/// row of each source and took 30 ms at p50, the same as a `distinct` over the
/// corpus, against 0.3 ms for this shape. An unordered `limit 1` seq-scanned
/// to the same 30 ms once an enabled source with no rows was in the list.
///
/// Liveness is `sync.live_item`'s -- the entity not withdrawn, the source not
/// turned off -- as two correlated subqueries in the `where`, and not that
/// view's joins (#768): on tables never analyzed the planner drove from
/// `entity` and the read took 520 ms (437 ms of it here), where this shape
/// takes 0.4 ms never analyzed and analyzed alike, custom plan or generic. See
/// [`KINDS_SQL`].
const SOURCES_SQL: &str = "\
select s.id from unnest($1::text[]) s(id)
 cross join lateral (
  select 1 from sync.item i
   where i.source_id = s.id
     and (select e.deleted_at is null from knobas.entity e where e.id = i.entity_id)
     and coalesce((select c.enabled from knobas.source_config c where c.id = i.source_id), true)
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
///    item that names it -- `sync.item_parent` alone, which carries its
///    item's time (`at`, migration `0032`), and every asset by its own time --
///    sorted. Grouped in byte order (`collate "C"`): the grouping is only an
///    identity, and a broad prefix groups tens of thousands of rows, which
///    the database's collation compares several times slower. The `offset 0`
///    keeps step 2 from being pushed beneath the sort.
/// 2. **Offer** them in that order while [`crate::under::parent_names!`] --
///    the fragment `under:`'s own lookup reads -- has the same name and kind,
///    up to the limit. So an offered name is one `under:` knows (#615); and
///    since step 1's rows are a superset of that fragment's, every name it
///    knows is ranked, and is offered for a prefix specific enough. A
///    `lateral ... limit 1` and not an `exists`: PostgreSQL turns an `exists`
///    into a semi join that builds the whole fragment (30,890 rows on the
///    probe, 28-175 ms), while a lateral is a few index probes per candidate,
///    in rank order, stopped by the outer limit. The outer `order by` is the
///    subquery's own, so it adds no sort above the loop. The probe compares
///    in the database's collation again (`collate "default"`), which is what
///    `item_parent_name_idx` is built in.
///
/// Joining every row to `sync.live_item` before ranking was 58 ms at the
/// probe's size with fresh statistics and 306 ms without (the join to
/// `knobas.source_config` on two columns is underestimated a hundredfold and
/// the planner picks nested loops). Joining each row to `sync.item` for its
/// time instead was 33 ms for an empty prefix with statistics and 260 ms on
/// tables never analyzed, for the same reason. This shape measured 27 ms for
/// an empty prefix, 20 ms for `j`, 3 ms for `jira0-1` and 1 ms for `pay` on
/// the probe, never analyzed and analyzed alike, and under a generic plan.
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
       select p.name collate \"C\" as name, p.kind, max(p.at) as top from ( \
         select ip.name, ip.kind, ip.at from sync.item_parent ip \
         union all select a.name, 'asset', a.updated_at from knobas.asset a) p \
        where lower(p.name) ~>=~ lower($1) \
          and lower(p.name) ~<~ (lower($1) || chr(1114111)) \
          and starts_with(lower(p.name), lower($1)) \
          and nullif(btrim(p.name), '') is not null and nullif(p.kind, '') is not null \
        group by p.name collate \"C\", p.kind \
        order by top desc nulls last, p.name collate \"C\", p.kind \
       offset 0) g \
     cross join lateral (select 1 from ",
    crate::under::parent_names!(),
    " k where lower(k.name) = lower(g.name collate \"default\") \
        and k.name = g.name collate \"default\" and k.kind = g.kind limit 1) known \
     order by g.top desc nulls last, g.name, g.kind \
     limit $2"
);

/// How many values one ticket key is offered with (#811): the most used
/// this many. The box narrows them as the value is typed.
pub const FIELD_SUGGESTIONS: i64 = 50;

/// How many live rows of each declaring source and kind the ticket keys'
/// values are read from (#811): the newest this many.
///
/// A sample, and that is the trade: a value that only older tickets carry is
/// not offered, and typed out it still filters. Measured with `just
/// responsiveness-probe` (22,000 items, five declaring sources), warm median
/// of the opening read (kinds, sources, parents and these): the whole mirror
/// 4,274 ms (load 11 to 29); the newest 1,000 149 ms (load 8 to 10); the
/// newest 200 62 ms (load 5 to 6), against 28 ms without the ticket keys
/// (#768's entry).
pub(crate) const FIELD_SAMPLE: i64 = 200;

/// The values the live mirror holds for each ticket key (#811), the most
/// used [`FIELD_SUGGESTIONS`] per key, most used first: `(key, value,
/// detail)`.
///
/// Read through the **same** declared reads the keys filter by (ADR-0007):
/// `crate::corpus::FIELD_PATHS` over the declarations at `$1`, and the
/// corpus's `field_of!` and `list_of!`, so an offered value is one the key
/// matches and the two statements cannot read a field two ways. The
/// declaration is resolved once a source and kind, as the mini board does
/// (#753), rather than walked per row; and only the newest
/// [`FIELD_SAMPLE`] (`$3`) live rows of each source and kind that declares
/// any of these fields are read, each probe ordered as
/// `item_source_updated_idx` is so it stops early. A row whose record misses
/// a path contributes nothing for that key: the miss direction, offering
/// fewer values, never a wrong one.
///
/// The value keeps the source's spelling, the way a reader knows it; the key
/// matches it ignoring case. A project is offered by its key, its name beside
/// it. `sprint` is offered as `current`, once, when a sampled row is in an
/// active sprint.
const FIELDS_SQL: &str = concat!(
    "with fdecl as (select $1::jsonb as d), ",
    crate::corpus::field_paths_ctes!(),
    ", sampled as materialized ( \
       select r.source_id, r.kind, r.payload \
         from (select source_id, kind from fp union select source_id, kind from lp) p \
         cross join lateral (select i.source_id, i.kind, i.payload from sync.item i \
                              where i.source_id = p.source_id and i.kind = p.kind and ",
    knobas_core::live_by_exclusion!("i"),
    " order by i.item_updated_at desc nulls last limit $3) r), \
     v as ( \
       select 'project' as key, ",
    crate::corpus::field_of!("project_key"),
    " as value, ",
    crate::corpus::field_of!("project_name"),
    " as detail from sampled i \
       union all select 'status', ",
    crate::corpus::field_of!("status_name"),
    ", null from sampled i \
       union all select 'issuetype', ",
    crate::corpus::field_of!("issue_type"),
    ", null from sampled i \
       union all select 'priority', ",
    crate::corpus::field_of!("priority"),
    ", null from sampled i \
       union all select 'assignee', ",
    crate::corpus::field_of!("assignee"),
    ", null from sampled i \
       union all select 'label', l.value, null from sampled i cross join lateral ",
    crate::corpus::list_of!("labels"),
    " l \
       union all select 'sprint', 'current', null from sampled i where exists (select 1 from ",
    crate::corpus::list_of!("sprint_state"),
    " s where lower(s.value) = 'active')) \
     select f.key, f.value, f.detail from ( \
       select v.key, v.value, max(v.detail) as detail, count(*) as uses, \
              row_number() over (partition by v.key order by count(*) desc, v.value) as rn \
         from v where v.value is not null group by v.key, v.value) f \
      where f.rn <= $2 \
      order by f.key, f.uses desc, f.value"
);

/// The ticket keys' values (#811), `@me` first among the assignees while
/// knobas knows who the user is, and `assigned:`'s windows, which every
/// installation has, and `has:`'s four signals (#815).
async fn fields(pool: &PgPool, vocab: &Vocabulary) -> Result<Vec<FieldSuggestion>, SearchError> {
    let rows: Vec<(String, String, Option<String>)> = sqlx::query_as(FIELDS_SQL)
        .bind(vocab.declarations.as_param())
        .bind(FIELD_SUGGESTIONS)
        .bind(FIELD_SAMPLE)
        // The unnamed statement, planned with the values in hand, for
        // `parents`' reason below.
        .persistent(false)
        .fetch_all(pool)
        .await?;
    let mut out: Vec<FieldSuggestion> = Vec::new();
    let suggestion = |key: &str, value: &str, detail: &str| FieldSuggestion {
        key: key.to_owned(),
        value: value.to_owned(),
        detail: detail.to_owned(),
    };
    for field in FieldKey::ALL {
        let key = field.spelling();
        match field {
            FieldKey::Assignee if !vocab.identity.is_empty() => {
                out.push(suggestion(key, crate::query::ME, "you"));
            }
            FieldKey::Assigned => {
                out.push(suggestion(
                    key,
                    "today",
                    "assigned to you, changed in the last day",
                ));
                out.push(suggestion(
                    key,
                    "week",
                    "assigned to you, changed in the last 7 days",
                ));
            }
            // The signals (#815): the four every installation has.
            FieldKey::Has => {
                for (value, detail) in [
                    ("failing-build", "a build around it failed"),
                    ("blocker", "something blocks it"),
                    ("open-pr", "a pull request of it is open"),
                    ("review-for-me", "a review is asked of you"),
                ] {
                    out.push(suggestion(key, value, detail));
                }
            }
            _ => {}
        }
        for (_, value, detail) in rows.iter().filter(|(k, _, _)| k == key) {
            let detail = match field {
                FieldKey::Sprint => "the active sprint",
                _ => detail.as_deref().unwrap_or(""),
            };
            out.push(suggestion(key, value, detail));
        }
    }
    Ok(out)
}

/// The parents for the prefix `under` (empty for none), and nothing else: the
/// read the box makes on every keystroke of an `under:` value (#761, #768).
///
/// # Errors
///
/// [`SearchError::Db`] if the read fails.
pub(crate) async fn parents(
    pool: &PgPool,
    under: &str,
) -> Result<Vec<ParentSuggestion>, SearchError> {
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
    Ok(parents
        .into_iter()
        .map(|(name, kind)| ParentSuggestion { name, kind })
        .collect())
}

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
    let parents = parents(pool, under).await?;
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
        parents,
        fields: fields(pool, vocab).await?,
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

//! What a searchable relation looks like, in fragments the query builder can
//! splice.
//!
//! # Why this exists at all
//!
//! Two reasons, and the first one is safety. Every field of a [`Corpus`] is
//! `&'static str`, so the SQL [`crate::sql`] produces is a concatenation of
//! compile-time text and `$n` placeholders and nothing else. Nothing a user
//! typed, and nothing read out of the database, is ever spliced into it. That
//! invariant is what the runtime-SQL audit in [`crate::sql`] rests on; if a
//! field here ever becomes a `String`, that audit is void.
//!
//! The second reason is spec §4's *"asset search matches ancestor path names"*.
//! M1 has exactly one corpus, [`LIVE_ITEM`]. M4 adds a second whose
//! `headline_text` includes the asset's ancestor path, and the builder unions
//! them -- the parser, the grouping, the commands and the UI do not change. The
//! seam is here rather than in a `union` written by hand later, because a
//! second corpus bolted on afterwards is a second query, and a second query is
//! a second set of filters to keep in step.
//!
//! # The one rule for a new corpus
//!
//! Name its columns. The mirror's view carries a `tsvector` (`fts`), and
//! `select *` into a `FromRow` struct panics at runtime (interfaces §1). [`Corpus`]
//! makes that structural: the builder only ever emits the fragments listed
//! below, and `fts` is only ever *matched against*, never selected.
//!
//! # M4: the asset corpus (designed here, built in #428)
//!
//! **Built.** [`ASSET`] is below and is in [`ALL`]; migration `0017` carries
//! the column and the index this section sketched. The sketch is left as
//! written -- it is the argument for the shape, and two of its details did not
//! survive contact, both recorded on [`ASSET`] itself: there is no
//! `props_text`, and `source_id` is the constant `'asset'` rather than an
//! `imported_from` column the import (#439) has not yet asked for.
//!
//! Assets are a tree, and spec §4 requires "pve-02" to find the containers
//! *under* pve-02. The path therefore has to be part of the indexed text, and
//! the only place it can be maintained cheaply is the database:
//!
//! ```sql
//! -- migration 00NN (orchestrator), M4:
//! alter table knobas.asset
//!   add column path_text text not null default '',   -- ancestor names, root first
//!   add column fts tsvector generated always as (
//!     setweight(to_tsvector('english', coalesce(name, '')),      'A') ||
//!     setweight(to_tsvector('english', coalesce(path_text, '')), 'B') ||
//!     setweight(to_tsvector('english', coalesce(props_text, '')),'C')
//!   ) stored;                                        -- STORED: roadmap §4 gotcha 1
//! create index asset_fts_idx on knobas.asset using gin (fts);
//! ```
//!
//! `STORED` is not decoration. Roadmap §4 gotcha 1, verbatim: *"PG 18:
//! `GENERATED ALWAYS AS (...)` without `STORED` silently creates an
//! unindexable virtual column -- always write `STORED`."* A virtual `fts`
//! would take the `create index` above without complaint and then recompute
//! every row's tsvector on every keystroke.
//!
//! `path_text` is maintained by the asset store on move/rename (a subtree
//! update), not computed per query: a recursive CTE per keystroke is what makes
//! a 100 ms budget impossible. Weight B keeps an ancestor match below a name
//! match, so "pve-02" still ranks the hypervisor itself first and its
//! containers under it.
//!
//! Then M4's whole search change is:
//!
//! ```ignore
//! pub(crate) const ASSET: Corpus = Corpus {
//!     relation: "knobas.asset a", entity_id: "a.id", kind: "'asset'",
//!     source_id: "coalesce(a.imported_from, 'asset')", title: "a.name",
//!     fts: "a.fts", headline_text: "a.name || ' — ' || a.path_text || ' — ' || a.props_text",
//!     author: None, updated_at: "a.updated_at", synced_at: "a.updated_at", scope: None,
//! };
//! ```
//!
//! plus adding it to the corpus list when the parse asked for assets. The
//! parser already emits `Prefix::Asset` and `kinds = ["asset"]` (M1), the
//! grouping already has `"asset"` in `GROUP_ORDER`, and the launcher already
//! renders a group it has never seen. Nothing else moves.
//!
//! That last sentence is a claim, and a claim of that shape is worth nothing
//! unasserted -- so [`NOTE`] is a **second corpus that actually runs**:
//! `knobas.note` is a different relation, a different id space, a different
//! text composition, and (the point) a corpus whose `kind` and `source_id` are
//! not columns at all. `tests/corpus_seam.rs` pushes it through the same
//! builder, the same grouping and the same snippet splitter as `sync.live_item`
//! and asserts that ranking interleaves across the two. If M4's asset corpus
//! needs a branch anywhere above [`crate::sql`], that test is where it will
//! show.

/// One searchable relation, described in fragments that are all `&'static str`.
///
/// The fields are the expressions the builder splices, not column names: each
/// one is qualified with the alias [`Self::relation`] introduces, so a corpus
/// can be a view, a join, or a table with a computed title.
pub struct Corpus {
    /// The `from` item, **including its alias** -- `"sync.live_item i"`.
    pub(crate) relation: &'static str,
    /// The stable entity id (`"i.entity_id"`), which is also what the detail
    /// stage joins back on.
    pub(crate) entity_id: &'static str,
    /// The entity kind results are grouped by (`"i.kind"`).
    pub(crate) kind: &'static str,
    /// Which source instance the row came from (`"i.source_id"`).
    pub(crate) source_id: &'static str,
    /// The row's title (`"i.title"`).
    pub(crate) title: &'static str,
    /// The indexed `tsvector`. **Matched against, never selected** -- reading
    /// one into a `FromRow` struct panics at runtime (interfaces §1).
    pub(crate) fts: &'static str,
    /// The text `ts_headline` quotes the excerpt from.
    ///
    /// Title *and* body: `fts` weights the title into the match, so a query
    /// that hits the title alone is a hit with nothing to quote from the body,
    /// and a headline over the body alone would then be an excerpt with no
    /// visible relation to what was searched for.
    pub(crate) headline_text: &'static str,
    /// The author column, if the corpus has one. `None` means the corpus can
    /// hold nothing of anybody's, so an author filter excludes it entirely
    /// rather than silently ignoring the filter.
    pub(crate) author: Option<&'static str>,
    /// When the *source* last changed the row (`"i.item_updated_at"`), which is
    /// what `updated:` filters and what browse mode orders by.
    pub(crate) updated_at: &'static str,
    /// When knobas last saw the row -- the per-row provenance §4 requires
    /// ("synced 4 min ago").
    pub(crate) synced_at: &'static str,
    /// Where a row sits **inside its source**, as one line, or `null::text`
    /// for a corpus whose rows sit nowhere (#284).
    ///
    /// One SQL expression, and which one is settled by where the answer lives
    /// -- there are two cases and no third (amended by #428; it read "the only
    /// one allowed here is `ancestor_path_read!`" while every corpus was a
    /// mirror corpus):
    ///
    /// 1. **A row mirrored from a source**: the path is a *payload* read, and
    ///    the only spelling allowed is
    ///    [`ancestor_path_read!`](knobas_core::ancestor_path_read) -- a second
    ///    spelling would be a second answer to "where is this", and the
    ///    launcher row and the detail panel both draw it. ADR-0007: it misses
    ///    to `null` for every kind whose records carry no `ancestors`, which
    ///    is every kind but a Confluence page today.
    /// 2. **A row in a tree knobas owns**: the path is a *column* the store
    ///    maintains on create, rename and move, and the expression reads it.
    ///    [`ASSET`] is the first, over `knobas.asset.path_text`. There is no
    ///    payload to read and no `ancestors` to miss to `null`, so the macro
    ///    has nothing to expand here; what the macro's rule is protecting --
    ///    one answer to "where is this" -- is protected instead by the column
    ///    having one writer.
    pub(crate) path: &'static str,
    /// An extra `where` fragment scoping the corpus, for a relation that holds
    /// more than one kind of thing. `None` for [`LIVE_ITEM`].
    pub(crate) scope: Option<&'static str>,
}

/// The synced mirror: M1's only corpus.
///
/// **`sync.live_item`, never `sync.item`.** Migration `0002` made the tombstone
/// filter structural precisely so that no query has to remember the join to
/// `knobas.entity`; reading the base table would put items a source deleted
/// back in the launcher. Migration `0012` put the *disabled-source* filter in
/// the same place for the same reason (issue #202), so reading the base table
/// now also puts back items from a source the user switched off.
pub const LIVE_ITEM: Corpus = Corpus {
    relation: "sync.live_item i",
    entity_id: "i.entity_id",
    kind: "i.kind",
    source_id: "i.source_id",
    title: "i.title",
    fts: "i.fts",
    headline_text: "i.title || ' — ' || i.body_text",
    author: Some("i.author"),
    updated_at: "i.item_updated_at",
    synced_at: "i.synced_at",
    path: knobas_core::ancestor_path_read!("i.payload"),
    scope: None,
};

/// Notes: the corpus of the first kind knobas **owns** rather than mirrors.
///
/// Written for M1 as the M4 asset corpus in miniature and shipped in M2 (#46)
/// once there was a write path behind it. It stands in for M4's assets
/// precisely because of what it is *not*: its `kind` and `source_id` are not
/// columns but constants, its ids live in their own namespace, and its
/// `headline_text` composes two columns that are not the row's title. A corpus
/// that needed the builder to grow a branch would need it for one of exactly
/// those reasons -- and this one needed none, which is what
/// `tests/corpus_seam.rs` runs rather than asserts.
///
/// **The excerpt is markdown source.** `body_md` is what the user typed,
/// `[[refs]]` and `#` headings included, and [`crate::snippet`] hands it on as
/// [`Segment`](crate::types::Segment) text with a `hit` flag. That is the
/// standing rule, not a note-specific one: the renderer prints `segment.text`
/// as text and never as markup (roadmap §4 gotcha 7). Notes make it harder to
/// forget, because markdown in a search row *looks* like something to render.
///
/// No `scope`. A deleted note's `knobas.note` row is gone
/// (`knobas_core::note::delete` removes the body and tombstones the entity), so
/// there is nothing to filter out -- unlike the mirror, where the row survives
/// its tombstone and `sync.live_item` is what hides it.
pub const NOTE: Corpus = Corpus {
    relation: "knobas.note n",
    entity_id: "n.id",
    kind: "'note'",
    source_id: "'note'",
    title: "n.title",
    fts: "n.fts",
    headline_text: "n.title || ' — ' || n.body_md",
    // A note is the user's own; there is no author column and no author to
    // filter by, so an author filter must exclude this corpus entirely rather
    // than be silently dropped for it.
    author: None,
    updated_at: "n.updated_at",
    // A local table is never behind itself: what knobas holds *is* the source.
    synced_at: "n.updated_at",
    // A note sits nowhere: it is knobas' own, it has no source to be nested
    // inside, and there is no payload to read ancestors out of. `null::text`
    // and not an empty string -- absence is the ADR-0007 miss, and an empty
    // string would draw an empty path line on every note in the launcher.
    path: "null::text",
    scope: None,
};

/// Assets: the estate, and the corpus this module's own docs designed (#428).
///
/// The design above is built here almost verbatim, and the two places it is
/// *not* are worth naming.
///
/// **`path` is a column, not a payload read.** [`Corpus::path`] says the only
/// expression allowed there is `ancestor_path_read!`, and that rule is about
/// records whose ancestry is buried in an adapter's payload (ADR-0007). An
/// asset's ancestry is `knobas.asset.parent_id` -- knobas' own tree, kept by
/// knobas' own store -- so `path_text` *is* the answer that read approximates
/// elsewhere, maintained on create, rename and move by
/// `knobas_app::assets`. `nullif` because a root asset sits nowhere, and
/// absence is the miss ADR-0007 asks for: an empty string would draw an empty
/// path line under every site in the launcher.
///
/// **No `props_text`.** Migration `0017` records why: what a search for
/// "8080" should mean is a decision, and it is not this ticket's.
///
/// No `scope`, for [`NOTE`]'s reason: a deleted asset's row is gone, so there
/// is nothing to filter out. No `author` either -- an asset's `owner` is who is
/// responsible for a machine, not who wrote a sentence, and folding it into
/// `author:` would make `author:me` answer with somebody's servers.
pub const ASSET: Corpus = Corpus {
    relation: "knobas.asset a",
    entity_id: "a.id",
    kind: "'asset'",
    source_id: "'asset'",
    title: "a.name",
    fts: "a.fts",
    headline_text: "a.name || ' — ' || a.path_text",
    author: None,
    updated_at: "a.updated_at",
    // A local table is never behind itself: what knobas holds *is* the source.
    synced_at: "a.updated_at",
    path: "nullif(a.path_text, '')",
    scope: None,
};

/// Routes: what an asset is reachable at, and the second corpus of the estate
/// (#432).
///
/// **The relation is a join, and that is the whole design.** A route sits
/// where the asset exposing it sits, and that path is already maintained on
/// `knobas.asset.path_text` by one writer (`knobas_app::assets`). Reading it
/// through this join rather than keeping a `path_text` of its own on
/// `knobas.route` is what keeps the number of writers of "where is this" at
/// one: a second copy would have to be rewritten by every asset rename and
/// every move, and the day it was not, a route would report a host that no
/// longer exists. [`Corpus`]' own docs allow it in as many words -- *"a corpus
/// can be a view, a join, or a table with a computed title"*.
///
/// The consequence is deliberate rather than an oversight, and it is the one
/// place this corpus differs from [`ASSET`]: a route is **shown** with its
/// path and **matched** on its own name and URL. `0018`'s `fts` carries those
/// two at weight A and nothing else, so "kuma" and "8111" find the routes that
/// carry them, while "hel1" finds the *assets* under `hel1` and not every
/// route exposed anywhere beneath it. That is the right answer for a launcher
/// row: a query that named a host and came back with thirty routes it holds
/// would have buried the host.
///
/// `path` is the exposing asset's own path **plus its name**, which is where
/// the route sits -- one level deeper than the asset's own answer, for the
/// reason `knobas_app::assets`' `path_below` composes the same two halves for
/// a child. No `nullif`: a route exposed by a root asset still sits *on that
/// asset*, so the path is never empty.
///
/// No `author` and no `scope`, for [`NOTE`]'s reasons: a route is knobas' own
/// and holds nothing of anybody's, and a deleted route's row is gone rather
/// than tombstoned in place.
pub const ROUTE: Corpus = Corpus {
    relation: "knobas.route r join knobas.asset ra on ra.id = r.asset_id",
    entity_id: "r.id",
    kind: "'route'",
    source_id: "'route'",
    title: "r.name",
    fts: "r.fts",
    headline_text: "r.name || ' — ' || r.url",
    author: None,
    updated_at: "r.updated_at",
    // A local table is never behind itself: what knobas holds *is* the source.
    synced_at: "r.updated_at",
    path: "case when ra.path_text = '' then ra.name else ra.path_text || ' / ' || ra.name end",
    scope: None,
};

/// Every corpus the launcher searches.
///
/// One list, so a corpus cannot be added to the crate and forgotten by the
/// query. Which rows each one contributes is decided by the *filters*, not by
/// membership here: `note:` sets `kinds = ["note"]`, which the mirror's branch
/// cannot match; a `source:` chip names configured sources, which
/// [`NOTE`]'s constant `'note'` is not; and an author filter excludes a corpus
/// with no author column outright. So the union is always both branches and the
/// answer is always the right one.
pub const ALL: &[&Corpus] = &[&LIVE_ITEM, &NOTE, &ASSET, &ROUTE];

#[cfg(test)]
mod tests {
    use super::*;

    /// A kind knobas owns is searchable, or "notes are a first-class
    /// searchable kind" is a claim with nothing behind it.
    ///
    /// This is also the half of the catalog seam that M4 will trip: adding
    /// `asset` to `OWNED_KINDS` fails here until an asset corpus joins [`ALL`],
    /// which is the reminder that a kind knobas owns and cannot search is a
    /// kind the launcher lies about.
    #[test]
    fn every_kind_knobas_owns_has_a_corpus_to_search() {
        for owned in knobas_core::entity::OWNED_KINDS {
            let quoted = format!("'{}'", owned.id);
            assert!(
                ALL.iter().any(|corpus| corpus.kind == quoted),
                "{:?} is a kind knobas owns with no corpus in ALL",
                owned.id
            );
        }
    }

    /// The `fts` of a corpus is matched against and never selected: reading a
    /// `tsvector` into a `FromRow` struct panics at run time (interfaces §1).
    /// The builder is what enforces it; this is the reminder at the point a
    /// corpus is written.
    #[test]
    fn no_corpus_selects_its_tsvector() {
        for corpus in ALL {
            assert!(!corpus.title.contains("fts"));
            assert!(!corpus.headline_text.contains("fts"));
        }
    }
}

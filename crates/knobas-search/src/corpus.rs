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
//! # M4: the asset corpus (designed here, built there)
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
    /// An extra `where` fragment scoping the corpus, for a relation that holds
    /// more than one kind of thing. `None` for [`LIVE_ITEM`].
    pub(crate) scope: Option<&'static str>,
}

/// The synced mirror: M1's only corpus.
///
/// **`sync.live_item`, never `sync.item`.** Migration `0002` made the tombstone
/// filter structural precisely so that no query has to remember the join to
/// `knobas.entity`; reading the base table would put items a source deleted
/// back in the launcher.
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
    scope: None,
};

/// The M4 asset corpus in miniature: a second relation, wired for tests only.
///
/// `knobas.note` is `0001`'s notes table -- empty in M1 by construction (notes
/// are M2) and carrying its own generated `fts`. It stands in for M4's assets
/// precisely because of what it is *not*: its `kind` and `source_id` are not
/// columns but constants, its ids live in their own namespace, and its
/// `headline_text` composes two columns that are not the row's title. A corpus
/// that needed the builder to grow a branch would need it for one of exactly
/// those reasons.
///
/// Test-only, and deliberately so: M1's corpus is `sync.live_item` and nothing
/// else (interfaces §2.4), so shipping this in the product's corpus list would
/// make `note:` answer with rows M1 does not have a write path for.
#[cfg(any(test, feature = "test-util"))]
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
    scope: None,
};

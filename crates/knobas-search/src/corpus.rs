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

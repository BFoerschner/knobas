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
//! written -- it is the argument for the shape, and one of its details did not
//! survive contact, recorded on [`ASSET`] itself: `source_id` is the constant
//! `'asset'` rather than an `imported_from` column the import (#439) has not
//! yet asked for. The sketch's `props_text` was deferred by `0017` and landed
//! with `0019` (#436), values-only and at the weight sketched here -- but not
//! in the two lines below: PostgreSQL forbids a generated column from
//! referencing another, so the real `fts` cannot read `props_text` and
//! recomputes its expression instead. `0019` carries that and the mitigation.
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
//! (That is about *ranking* by ancestry on every keystroke. `under:` is the
//! other question -- which rows sit beneath a named asset -- and it does walk
//! `parent_id` per query, once per statement and only for a query that typed
//! `under:`: [`estate_subtree!`] says why that is the right trade there.)
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
//! not columns at all. `tests/it/corpus_seam.rs` pushes it through the same
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
    /// Whether a match is *named* what was typed (`q.words`), read from
    /// lexemes the relation stores, or `None` to lex [`Self::title`] per
    /// match through `crate::sql::NAMED`.
    ///
    /// The mirror stores them (migration `0056`, #1650): it is the corpus a
    /// common word matches most of, and lexing every matching title per
    /// keystroke was a fifth of the statement there. The stored
    /// comparison must be true exactly where `NAMED` over the same title is,
    /// which the migration's columns are written to be and
    /// `knobas-db`'s `zero_fifty_six_stores_what_the_launcher_lexed_per_match`
    /// pins. Read once per match, so it is a comparison and nothing heavier.
    pub(crate) named: Option<&'static str>,
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
    ///    [`path_line_read!`](knobas_core::path_line_read) -- a second
    ///    spelling would be a second answer to "where is this", and the
    ///    launcher row and the detail panel both draw it. Since #596 that is
    ///    the parent chain the row's kind declares (ADR-0018), `owner/repo` for
    ///    a Gitea commit, a page's ancestors for Confluence (#599); since #646
    ///    nothing else, #284's undeclared ancestor read being retired.
    ///    ADR-0007: it misses to `null` for every kind that declares no parent.
    ///    It reads the declarations from `pd.d`, a relation [`crate::sql`]
    ///    joins into the detail stage for every corpus, as `ud` is for
    ///    `under:`.
    /// 2. **A row in a tree knobas owns**: the path is a *column* the store
    ///    maintains on create, rename and move, and the expression reads it.
    ///    [`ASSET`] is the first, over `knobas.asset.path_text`. There is no
    ///    payload to read and no `ancestors` to miss to `null`, so the macro
    ///    has nothing to expand here; what the macro's rule is protecting --
    ///    one answer to "where is this" -- is protected instead by the column
    ///    having one writer.
    pub(crate) path: &'static str,
    /// The row's state chip as `jsonb`, or `null::jsonb` for a corpus whose
    /// rows have none (#666). For the mirror it is
    /// [`declared_state!`](knobas_core::declared_state) over `pd.d`, the
    /// relation `path` reads, and nothing else: one answer to "what state is
    /// this in", as `path_line_read!` is one to "where is this".
    pub(crate) state: &'static str,
    /// An extra `where` fragment scoping the corpus, for a relation that holds
    /// more than one kind of thing. `None` for [`LIVE_ITEM`].
    pub(crate) scope: Option<&'static str>,
    /// What `under:` matches, as a predicate over a row and a relation `ud`
    /// carrying the bound declarations (`ud.d`) and the lowercased values
    /// (`ud.v`), which [`crate::sql`] supplies -- or `None` for a corpus whose
    /// rows have no declared parent, which an `under:` query then excludes
    /// outright, as an author filter excludes a corpus with no author.
    ///
    /// For the mirror it is the two `knobas_core::payload` reads and nothing
    /// else (the deputy's ruling of 2026-10-11 on #594): the row sits beneath
    /// a parent answering to a value, or is that parent, mirrored. Since #627
    /// the database runs those reads when a row or a declaration changes and
    /// stores their answer in `sync.item_parent` (migration `0028`,
    /// ADR-0019), so the predicate is an index read on `lower(name)` and
    /// `ud.d` goes unread here. A stored row counts only while its item's
    /// source is configured with the adapter the row was resolved for --
    /// the join on `adapter_kind`, which `under::parent_names!` makes too.
    ///
    /// For the estate it is no payload read at all (#600): an asset's parent
    /// is its `parent_id` column and a route's is the asset exposing it
    /// (ADR-0014), so both read [`estate_subtree!`]'s walk over those columns.
    pub(crate) under: Option<&'static str>,
    /// What each ticket key matches (#811), as a predicate over a row, the
    /// statement's [`FIELD_PATHS`] CTEs (`fp`, `lp`) and a relation `fd`
    /// carrying the values (`fd.v`, folded by the server, #1843) and `assigned:`'s window in
    /// days (`fd.n`). A key with no entry here is a field this corpus's rows
    /// do not have: the condition fails on every row, and its negation holds
    /// on every row.
    ///
    /// For the mirror each is a declared payload read (ADR-0007, through
    /// `knobas_core::payload`'s macros, never a payload key named here), and
    /// a read that misses matches no value: see [`LIVE_ITEM_FIELDS`].
    pub(crate) fields: &'static [(crate::query::FieldKey, &'static str)],
}

/// One declared scalar field's candidates for every source and kind the
/// declarations name, as rows of `fp`.
macro_rules! field_paths {
    ($field:literal) => {
        concat!(
            "select i.source_id, i.kind, '",
            $field,
            "'::text as field, c.segs, c.ord from fk i cross join lateral ",
            knobas_core::declared_candidates!("i.d", $field),
            " c"
        )
    };
}
pub(crate) use field_paths;

/// The same for a declared list field, as rows of `lp`.
macro_rules! list_paths {
    ($field:literal) => {
        concat!(
            "select i.source_id, i.kind, '",
            $field,
            "'::text as field, c.at_segs, c.entry_segs from fk i cross join lateral ",
            knobas_core::declared_list_candidates!("i.d", $field),
            " c"
        )
    };
}
pub(crate) use list_paths;

/// The CTEs a statement with a ticket key carries (#811), after a CTE
/// `fdecl(d)` holding the bound declarations, which `crate::sql` writes:
/// every declared candidate of the fields the keys read, resolved **once a
/// statement** rather than walked per row -- the mini board's remedy (#753).
/// `fp (source_id, kind, field, segs, ord)` for the strings,
/// `lp (source_id, kind, field, at_segs, entry_segs)` for the lists.
///
/// Measured on 22,000 Jira-shaped tickets (#811), six runs each on a machine
/// at load 10 to 18: `status:Done` through `declared_string!` per row took
/// 171 to 202 ms; through these CTEs 122 to 146 ms, beside 152 to 155 ms for
/// the text query `payout`, which every row matched. A ticket key over the
/// whole mirror costs what a text query matching all of it does.
///
/// Which fixture that was is not in the tree (#1849): the figures above are
/// #811's own, on a Jira-only corpus whose generator was not kept, and no
/// fixture in the tree has a `payout` that every row holds. Read the text
/// query on the 22k responsiveness probe instead: `common` matches 17,625 of
/// its 23,500 items, 78 to 83 ms warm at machine load 6 to 12 (#1849).
macro_rules! field_paths_ctes {
    () => {
        concat!(
            "fk as materialized (select f.d, s.source_id, k.kind from fdecl f \
               cross join jsonb_each(case when jsonb_typeof(f.d) = 'object' then f.d \
                                          else '{}'::jsonb end) as s(source_id, kinds) \
               cross join lateral jsonb_object_keys(case when jsonb_typeof(s.kinds) = 'object' \
                                          then s.kinds else '{}'::jsonb end) as k(kind)),\n",
            "fp as materialized (",
            $crate::corpus::field_paths!("project_key"),
            " union all ",
            $crate::corpus::field_paths!("project_name"),
            " union all ",
            $crate::corpus::field_paths!("status_name"),
            " union all ",
            $crate::corpus::field_paths!("issue_type"),
            " union all ",
            $crate::corpus::field_paths!("priority"),
            " union all ",
            $crate::corpus::field_paths!("assignee"),
            "),\nlp as materialized (",
            $crate::corpus::list_paths!("labels"),
            " union all ",
            $crate::corpus::list_paths!("sprint_state"),
            ")"
        )
    };
}
pub(crate) use field_paths_ctes;

/// The same CTEs, as text.
pub(crate) const FIELD_PATHS: &str = field_paths_ctes!();

/// A row's declared string for one field, through `fp`.
macro_rules! field_of {
    ($field:literal) => {
        knobas_core::first_string_of!(concat!(
            "(select fp.segs, fp.ord from fp where fp.source_id = i.source_id \
              and fp.kind = i.kind and fp.field = '",
            $field,
            "')"
        ))
    };
}
pub(crate) use field_of;

/// [`field_of!`], read off `sync.item_field` where the database stored the
/// field for the declarations the statement binds (#2184,
/// `knobas_core::stored_string!`): a probe by the row's id rather than a
/// walk of its payload, with the same answer.
macro_rules! stored_field_of {
    ($field:literal) => {
        knobas_core::stored_string!(
            "(select fdecl.d from fdecl)",
            $field,
            $crate::corpus::field_of!($field)
        )
    };
}

/// A row's declared list values for one field, through `lp`.
macro_rules! list_of {
    ($field:literal) => {
        knobas_core::list_values_of!(concat!(
            "(select lp.at_segs, lp.entry_segs from lp where lp.source_id = i.source_id \
              and lp.kind = i.kind and lp.field = '",
            $field,
            "')"
        ))
    };
}
pub(crate) use list_of;

/// The mirror's ticket keys (#811), each the field its source declares.
///
/// **Which way each fails, stated (ADR-0007 requirement 3)**: a row whose
/// source declares no such path, or whose record misses it, yields `null`,
/// which `crate::sql` reads as *does not match*. So a miss is left out of
/// `status:Done` and kept by `-status:Done`: absence, never a wrong value.
/// Pinned by `knobas-search`'s `tests/it/ticket_keys.rs`.
///
/// `project:` also matches any project of the chain stored beside the item
/// (`sync.item_project.path`, #1019), with `under:`'s adapter check: a parent
/// project takes in its subprojects, as a link-set pattern's does.
///
/// Every comparison is on `lower(...)` against values the statement
/// folds with the same `lower()` (#1843): Jira's status, type, priority and project names are
/// case-insensitive in JQL, and so are its usernames.
pub(crate) const LIVE_ITEM_FIELDS: &[(crate::query::FieldKey, &str)] = {
    use crate::query::FieldKey;
    &[
        (
            FieldKey::Project,
            concat!(
                "(lower(",
                stored_field_of!("project_key"),
                ") = any(fd.v) or lower(",
                stored_field_of!("project_name"),
                ") = any(fd.v) or exists (select 1 from sync.item_project ip \
                   join knobas.source_config s on s.id = ip.source_id and s.kind = ip.adapter_kind \
                   where ip.entity_id = i.entity_id and ",
                knobas_core::project_chain_has!("ip.path", "fd.v"),
                "))"
            ),
        ),
        (
            FieldKey::Status,
            concat!("lower(", stored_field_of!("status_name"), ") = any(fd.v)"),
        ),
        (
            FieldKey::IssueType,
            concat!("lower(", field_of!("issue_type"), ") = any(fd.v)"),
        ),
        (
            FieldKey::Priority,
            concat!("lower(", field_of!("priority"), ") = any(fd.v)"),
        ),
        (
            FieldKey::Assignee,
            concat!("lower(", stored_field_of!("assignee"), ") = any(fd.v)"),
        ),
        (
            FieldKey::Label,
            concat!(
                "exists (select 1 from ",
                list_of!("labels"),
                " l where lower(l.value) = any(fd.v))"
            ),
        ),
        (
            FieldKey::Sprint,
            concat!(
                "exists (select 1 from ",
                list_of!("sprint_state"),
                " l where lower(l.value) = 'active')"
            ),
        ),
        (
            FieldKey::Assigned,
            concat!(
                "(lower(",
                stored_field_of!("assignee"),
                ") = any(fd.v) and i.item_updated_at >= now() - make_interval(days => fd.n))"
            ),
        ),
        // The cards' own reader (#815), so `has:` admits exactly the tickets
        // whose cards show the signal: `fd.u` is the user, lowercased, and
        // `fd.v` the kinds, which also pick the links walked (#939); the user
        // narrows `review-for-me`'s pull requests before the walk (#1148).
        (
            FieldKey::Has,
            concat!(
                "i.entity_id in (select sg.tid from ",
                knobas_core::ticket_signals!(
                    "(select fdecl.d from fdecl)",
                    "fd.u",
                    knobas_core::signal_ends!("(select fdecl.d from fdecl)", "fd.u", "fd.v"),
                    "fd.v"
                ),
                " sg where sg.kind = any(fd.v))"
            ),
        ),
    ]
};

/// The ids of every asset `under:` names and of every asset beneath one, at
/// any depth: a parenthesised subquery over `ud.v`, the lowercased values.
///
/// **Walked down from the named assets, per query, over `parent_id`** -- the
/// column ADR-0014 makes an asset's parent, and the one a move writes. Not
/// over `path_text`: that is a line of *names* for a reader, two siblings may
/// share a name, and a name in it says nothing about which asset it was. And
/// not kept anywhere: a move changes what this returns on the next keystroke
/// because there is nothing to go stale.
///
/// An asset answers to its **name**, exactly and ignoring case (spec #591),
/// and the walk starts *at* it, so the asset itself is in the set -- as the
/// mirror returns a parent it holds. `union`, not `union all`: the store
/// refuses a move that makes a cycle, and this terminates even if a row ever
/// got one some other way. It reads `ud.v` and nothing of the outer row, so
/// it is one walk per statement and not one per asset. Measured on
/// `testing::seed_estate`'s upper bound (3,303 assets, 1,500 routes) with a
/// site named by `under:`: all 1,601 rows of its subtree in 13 ms at p90, and
/// 18 ms with a word beside the filter (#600).
///
/// What an asset answers to is spelled twice, here and in
/// the asset arm of `crate::under`'s `parent_names!`, through which
/// `sql::known_parent_positions` reports a value no asset is called as an unknown token. Change
/// one and the other must follow.
macro_rules! estate_subtree {
    () => {
        concat!(
            "(with recursive sub(id) as (",
            "select x.id from knobas.asset x where lower(x.name) = any(ud.v)",
            " union select c.id from knobas.asset c join sub on c.parent_id = sub.id",
            ") select sub.id from sub)"
        )
    };
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
    // Three names, stored by `0056`: the title, the text after its last `/`
    // (#621, null without one), and a ticket's key (#1174), the id past its
    // namespace as `0048` indexes it and `sql::ids_sql` reads it --
    // `jira:PAY-231` is `PAY-231`, null for every other kind. The key is why
    // `PAY-231` ranks the ticket PAY-231 above PAY-2310, which matches the
    // same words with its last one as a prefix. A null half is "not named by
    // it", which `coalesce` makes false before `named desc` can sort it first.
    named: Some(
        "coalesce(i.title_words = q.words or i.title_tail_words = q.words \
         or i.key_words = q.words, false)",
    ),
    fts: "i.fts",
    // The words alone: a page's Docs-search marks never reach an excerpt (#860).
    headline_text: concat!(
        "i.title || ' — ' || ",
        knobas_core::body_text_shown!("i.body_text")
    ),
    author: Some("i.author"),
    updated_at: "i.item_updated_at",
    synced_at: "i.synced_at",
    path: knobas_core::path_line_read!("pd.d"),
    state: knobas_core::declared_state!("pd.d"),
    scope: None,
    under: Some(
        "i.entity_id in (select ip.entity_id from sync.item_parent ip \
          join sync.item x on x.entity_id = ip.entity_id \
          join knobas.source_config s on s.id = x.source_id and s.kind = ip.adapter_kind \
          where lower(ip.name) = any(ud.v))",
    ),
    fields: LIVE_ITEM_FIELDS,
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
/// `tests/it/corpus_seam.rs` runs rather than asserts.
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
    named: None,
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
    state: "null::jsonb",
    scope: None,
    // A note is knobas' own and sits inside nothing (see `path`), so it is
    // beneath no parent `under:` could name.
    under: None,
    // Knobas' own rows carry none of a ticket's fields.
    fields: &[],
};

/// Assets: the estate, and the corpus this module's own docs designed (#428).
///
/// The design above is built here almost verbatim, and the two places it is
/// *not* are worth naming.
///
/// **`path` is a column, not a payload read.** [`Corpus::path`] says the only
/// expression allowed there is `path_line_read!`, and that rule is about
/// records whose ancestry is buried in an adapter's payload (ADR-0007). An
/// asset's ancestry is `knobas.asset.parent_id` -- knobas' own tree, kept by
/// knobas' own store -- so `path_text` *is* the answer that read approximates
/// elsewhere, maintained on create, rename and move by
/// `knobas_app::assets`. `nullif` because a root asset sits nowhere, and
/// absence is the miss ADR-0007 asks for: an empty string would draw an empty
/// path line under every site in the launcher.
///
/// **`props_text` is the decision `0017` deferred, made by `0019` (#436).**
/// `0017` left the column out because *"what a search for '8080' should mean is
/// a decision"*, and #436's *"searching a hostname property finds the VM"* is
/// what asks it. The answer is in the migration in full: the property
/// **values** are indexed and the **keys** are not, at weight C -- below the
/// name and below the ancestor path -- so `vm-db-01` and `8080` find the one
/// row that carries them while `hostname` and `ports`, which are schema and
/// stand on every asset of a type, find nothing. The column is *generated*
/// rather than store-maintained, which `path_text` cannot be: an ancestor path
/// needs a recursive CTE, a property bag is a column of the same row.
///
/// It joins `headline_text` as well as the index, and that pairing is the rule
/// [`Corpus::headline_text`] states rather than a second decision: a hit
/// matched on a property with nothing quotable in the excerpt is a row whose
/// reason for being there is invisible.
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
    named: None,
    fts: "a.fts",
    headline_text: "a.name || ' — ' || a.path_text || ' — ' || a.props_text",
    author: None,
    updated_at: "a.updated_at",
    // A local table is never behind itself: what knobas holds *is* the source.
    synced_at: "a.updated_at",
    path: "nullif(a.path_text, '')",
    state: "null::jsonb",
    scope: None,
    // Knobas' own rows declare no parent through a payload: an asset's parent
    // is its `parent_id` column (ADR-0014), read directly (#600).
    under: Some(concat!("a.id in ", estate_subtree!())),
    // Knobas' own rows carry none of a ticket's fields.
    fields: &[],
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
/// path and **matched** on its own name and URL, while "hel1" finds the
/// *assets* under `hel1` and not every route exposed anywhere beneath it. That
/// is the right answer for a launcher row: a query that named a host and came
/// back with thirty routes it holds would have buried the host.
///
/// **How much of a URL is actually reachable, measured (#436).** `0018` says
/// *"`to_tsvector` lexes a URL into its host and path, so 'kuma' and '8111'
/// find the routes that carry them"*. That is true of the estate's own URLs
/// and not true in general, and the line between the two is worth having
/// written down, because it is not where anybody would guess:
///
/// | URL | lexemes |
/// |---|---|
/// | `http://127.0.0.1:8111/` | `127.0.0.1`, `8111` |
/// | `http://gitea:3000/` | `gitea`, `3000` |
/// | `http://127.0.0.1:33001/dashboard` | `127.0.0.1`, `33001/dashboard` |
/// | `https://kuma.example.test:3001/dashboard` | `kuma.example.test:3001/dashboard`, `kuma.example.test:3001`, `/dashboard` |
///
/// So it is the **path** that fuses things together, not the port: a URL that
/// ends at `/` gives the port as a lexeme of its own, which is why every one
/// of the nine routes in `testenv/estate.json` is findable by the port
/// that tells it apart (ADR-0013 -- the real container is the witness, and it
/// was asked). A URL with a path folds the port into `33001/dashboard`, and a
/// *named* host with a port is one lexeme whole. Those two are reachable only
/// while the last word is still being typed, when
/// [`Parsed::prefix_last_term`](crate::Parsed) makes the query a prefix --
/// which means a hit that vanishes on the space bar -- and a path segment
/// (`dashboard`) is not reachable at any stage.
///
/// **Not fixed here, deliberately.** #436 asks that a route be found by its
/// URL, and on the estate that exists it is. Widening what `0018`'s `fts`
/// indexes is a change to a surface ratified the day before and it belongs to
/// a ticket that says so; `tests/it/corpus_seam.rs` runs both the positive and
/// the two negatives, so the boundary is a fact the next reader inherits
/// rather than one they have to measure again.
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
    named: None,
    fts: "r.fts",
    headline_text: "r.name || ' — ' || r.url",
    author: None,
    updated_at: "r.updated_at",
    // A local table is never behind itself: what knobas holds *is* the source.
    synced_at: "r.updated_at",
    path: "case when ra.path_text = '' then ra.name else ra.path_text || ' / ' || ra.name end",
    state: "null::jsonb",
    scope: None,
    // A route's parent is the asset exposing it (ADR-0014): it is beneath
    // whatever that asset is beneath, and beneath the asset itself (#600).
    under: Some(concat!("r.asset_id in ", estate_subtree!())),
    // Knobas' own rows carry none of a ticket's fields.
    fields: &[],
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

/// [`ALL`] without [`NOTE`]: what the launcher searches while *Show notes and
/// quick capture* is off (#940, R-13a.6). The note data and commands stay
/// (D10); only the search leaves them out.
pub const WITHOUT_NOTES: &[&Corpus] = &[&LIVE_ITEM, &ASSET, &ROUTE];

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

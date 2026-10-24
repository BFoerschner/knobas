//! **Signals** on tickets (#815, R-3.8): short, tone-coloured facts about
//! the work around a ticket -- *build failed*, *PR #142 open*, *blocked by
//! OPS-77*, *review asked of you* -- read over the confirmed link graph and
//! the state each item's own source declares.
//!
//! # One reader, two consumers
//!
//! [`ticket_signals!`] is the reader, as SQL: a table expression of the
//! signals a set of tickets carries. The Tickets view's cards read it through
//! the mini board ([`crate::mini_board`]), and the search grammar's `has:`
//! asks whether a ticket carries one of a given kind through the same macro
//! (`knobas_search::corpus`). So a card that shows *blocked by OPS-77* is
//! exactly a card `has:blocker` admits, which is what the command tests in
//! `knobas-app/tests/it/signals_ipc.rs` hold the two to.
//!
//! # What each signal means, stated
//!
//! The ticket's **reach** is ADR-0008's walk from the ticket as a seed: what
//! it is confirm-linked to (its pull requests), and one hop further (their
//! builds). Every step reads `knobas.confirmed_link`, never a proposal, and
//! never walks through a context, for the ADR's reasons.
//!
//! * **`failing-build`**, `err`, *build failed*: of the builds in the
//!   reach, the newest of each configuration (its declared parent, through
//!   `sync.item_parent`; a build with none is its own) has a declared state
//!   drawn in the `err` tone. A newer green run of the same configuration
//!   puts the signal out, as the inbox's failed-build rule resolves.
//! * **`blocker`**, `err`, *blocked by KEY*: the ticket is the `to` end of a
//!   confirmed `blocks` link (the standup's reading: the stored direction is
//!   what tells `blocks` from `blocked by`). One signal per blocker, named by
//!   its key when it is a ticket and by its title otherwise.
//! * **`open-pr`**, `info`, *PR #N open*: a pull request the ticket links to
//!   directly whose declared state is drawn in the `info` tone, the tone of
//!   an open, in-flight item (`crate::tone::Tone`). One per pull request;
//!   the number is the `#N` its key ends in, and *PR open* when it has none.
//! * **`review-for-me`**, `warn`, *review asked of you*: one of those open
//!   pull requests declares a requested reviewer who is one of the
//!   configured usernames, ignoring case.
//!
//! Two words come from the entity id rather than a declaration: a ticket
//! blocker's key (the id past its namespace, as every card's key is) and a
//! pull request's number (the `#N` its key ends in, the grammar interfaces
//! §4.2 gives proposal keys; a key without one says *PR open*).
//!
//! The read names no adapter. It names two kinds (`build`, `pr`) and the
//! shell's tones, and reads states and reviewers only through the paths a
//! source declares (ADR-0007): a source that declares no state contributes
//! no build or pull-request signal, the miss direction, never a guess.
//!
//! # Keys compared bytewise
//!
//! The walk is a few thousand key probes when `has:` asks at 22,000 items,
//! and each one compared ids through the database's collation: `en_US.UTF-8`
//! on every cluster measured then, the app's own database included, so
//! `strcoll` at every step down a btree. About 4.7 µs a probe, against
//! 0.75 µs on an index ordered `collate "C"` (#1163). Since migration `0050` (#1172) every column
//! that holds an entity id is `collate "C"` itself, so each probe below is a
//! plain `=` on the key's own index, and the sets the walk deduplicates sort
//! bytewise without saying so. Only the tickets it is handed, whose collation
//! is the caller's, are cast to `C` before they are deduplicated.

use serde::{Deserialize, Serialize};

use crate::tone::Tone;

/// Which fact a [`Signal`] states: the four values `has:` takes.
///
/// - `failing-build`: A build in the ticket's reach is red.
/// - `blocker`: Something is in the ticket's way.
/// - `open-pr`: A pull request of the ticket's is open.
/// - `review-for-me`: One of those asks the user for a review.
///
/// Issue #815.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub enum SignalKind {
    #[serde(rename = "failing-build")]
    FailingBuild,
    #[serde(rename = "blocker")]
    Blocker,
    #[serde(rename = "open-pr")]
    OpenPr,
    #[serde(rename = "review-for-me")]
    ReviewForMe,
}

impl SignalKind {
    /// Every kind, in the order a card draws them.
    pub const ALL: [SignalKind; 4] = [
        SignalKind::FailingBuild,
        SignalKind::Blocker,
        SignalKind::OpenPr,
        SignalKind::ReviewForMe,
    ];

    /// The spelling `has:` takes and the wire carries.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            SignalKind::FailingBuild => "failing-build",
            SignalKind::Blocker => "blocker",
            SignalKind::OpenPr => "open-pr",
            SignalKind::ReviewForMe => "review-for-me",
        }
    }

    /// What the search box says beside the value when it offers it.
    #[must_use]
    pub fn detail(self) -> &'static str {
        match self {
            SignalKind::FailingBuild => "a build around it failed",
            SignalKind::Blocker => "something blocks it",
            SignalKind::OpenPr => "a pull request of it is open",
            SignalKind::ReviewForMe => "a review is asked of you",
        }
    }

    /// The kind `has:` names by `spelling`, ignoring case.
    #[must_use]
    pub fn from_spelling(spelling: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|kind| kind.as_str().eq_ignore_ascii_case(spelling))
    }
}

/// One signal on one ticket: what the card and the list row draw.
///
/// Issue #815.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct Signal {
    pub kind: SignalKind,
    /// The words drawn beside the dot (*blocked by OPS-77*).
    ///
    /// Other examples: *build failed*, *PR #142 open*.
    pub text: String,
    pub tone: Tone,
}

/// The signals of the tickets `$tickets` names, as a table expression of
/// `(tid, ord, kind, text, tone)`: see the module notes for each rule.
///
/// `$tickets` is a relation with one column, `tid`, of entity ids; an id
/// that is not a ticket carries no signal. `$decl` is where the
/// declarations are bound, as for [`crate::declared_string!`], and `$me` a
/// `text[]` of the configured usernames, lowercased by the server's `lower()`
/// (#1975: Rust's `to_lowercase` differs from it on `İ`).
///
/// **Set-based, never once per ticket** (#815): the walk joins the tickets
/// to the confirmed links once, so its cost is the links the tickets have,
/// not a walk per ticket. The board names the cards it keeps; `has:` names
/// the link ends that can carry the kinds it wants ([`signal_ends!`]) and
/// asks which of them carry one. The per-row parts are
/// index probes and the declaration's own reads. `builds` probes `sync.item`
/// once a reach row, in a `lateral ... offset 0` ([`item_by_key!`]), which
/// is also the reach row's kind: joined on the id alone, the planner
/// underestimated the reach and compared every reach row with every build
/// (1.8 million comparisons for 1,800 tickets and 500 builds, 230 ms; PR
/// #933's review), and until #1163 the reach probed each row's entity for
/// its kind first, a second probe of every row.
///
/// `$kinds` is a `text[]` of the kinds wanted, or `null` for all four
/// (#939). Each kind's branch is guarded by it, so `has:blocker` never
/// walks the reach for builds: a guard that is false is a one-time filter,
/// and the walk it guards is never read.
///
/// `prs` probes `sync.item` once a neighbour, in a `lateral ... offset 0`:
/// with the board's few tickets on analyzed tables, the planner otherwise
/// started from the pull requests and scanned `n1` once each (#1148).
///
/// Whether a `$tickets` id is a ticket is a lookup by its key, once an id:
/// joined, the planner merged the ids with every entity sorted by id, 58 ms
/// for the 1,200 tickets `has:failing-build` names at 22,000 items (#1148).
///
/// A blocker is found from the ticket's own links (`to_id`), the relation
/// tested after: in a `lateral ... offset 0`, so the planner cannot read
/// `relation = 'blocks'` against the pair index, whose third column it is.
/// On tables never analyzed it did, and scanned that whole index once a
/// ticket (567 tickets, 91,000 buffers, 130 ms; #939). The blocker's entity
/// is the same kind of lateral probe, so it cannot become a merge with every
/// entity sorted.
///
/// Every probe compares bytewise (the module notes), and so does each set the
/// walk deduplicates (`t`, `n1`, `reach`, the configurations in `builds`): in
/// the database's collation, sorting `has:blocker`'s 5,000 ends took 17 ms of
/// its plan (#1163).
#[macro_export]
macro_rules! ticket_signals {
    ($decl:literal, $me:literal, $tickets:expr, $kinds:literal) => {
        concat!(
            "(with state_paths as materialized (",
            $crate::state_paths!($decl),
            "),
              t as (select d.tid
                      from (select distinct tk.tid collate \"C\" as tid from ",
            $tickets,
            " tk) d
                     where ",
            $crate::kind_of!("d.tid"),
            " = 'ticket'),
              n1 as (select x.tid, x.id from (
                        select t.tid, l.to_id as id
                          from t join knobas.confirmed_link l on l.from_id = t.tid
                        union
                        select t.tid, l.from_id
                          from t join knobas.confirmed_link l on l.to_id = t.tid
                        ) x
                     where ",
            $crate::kind_of!("x.id"),
            " <> 'ctx'),
              reach as materialized (select x.tid, x.id from (
                        select n1.tid, n1.id from n1
                        union
                        select n1.tid, l.to_id
                          from n1 join knobas.confirmed_link l on l.from_id = n1.id
                        union
                        select n1.tid, l.from_id
                          from n1 join knobas.confirmed_link l on l.to_id = n1.id
                        ) x
                     where x.id <> x.tid),
              builds as (select distinct on (b.tid, b.cfg) b.tid, b.tone from (
                        select r.tid,
                               coalesce((select min(p.name) from sync.item_parent p
                                          where p.entity_id = i.entity_id), i.entity_id) as cfg,
                               coalesce(i.item_updated_at, i.synced_at) as at,
                               ",
            $crate::resolved_state!(),
            "->>'tone' as tone
                          from reach r cross join lateral (",
            $crate::item_by_key!("r.id"),
            ") i
                         where i.kind = 'build' and ",
            $crate::live_by_key!("i"),
            ") b order by b.tid, b.cfg, b.at desc, b.tone),
              prs as (select n1.tid, substring(i.entity_id from '#([0-9]+)$') as num,
                             ",
            $crate::resolved_state!(),
            "->>'tone' as tone,
                             ",
            $crate::asked_of!($decl, $me),
            " as asked
                        from n1 cross join lateral (",
            $crate::item_by_key!("n1.id"),
            ") i
                       where i.kind = 'pr' and ",
            $crate::live_by_key!("i"),
            ")
             select b.tid, 1 as ord, 'failing-build'::text as kind,
                    'build failed'::text as text, 'err'::text as tone
               from builds b where b.tone = 'err' and ",
            $crate::wants_signal!($kinds, "'failing-build'"),
            " group by b.tid
             union all
             select t.tid, 2, 'blocker', 'blocked by ' || case when o.kind = 'ticket'
                      then substr(o.id, strpos(o.id, ':') + 1) else o.title end, 'err'
               from t cross join lateral (select l.from_id, l.relation
                                            from knobas.confirmed_link l
                                           where l.to_id = t.tid offset 0) l
                      cross join lateral (select o.id, o.kind, o.title, o.deleted_at
                                            from knobas.entity o
                                           where o.id = l.from_id offset 0) o
              where l.relation = 'blocks' and o.kind <> 'ctx' and o.deleted_at is null and ",
            $crate::wants_signal!($kinds, "'blocker'"),
            "
             union all
             select p.tid, 3, 'open-pr', coalesce('PR #' || p.num || ' open', 'PR open'), 'info'
               from prs p where p.tone = 'info' and ",
            $crate::wants_signal!($kinds, "'open-pr'"),
            "
             union all
             select p.tid, 4, 'review-for-me', 'review asked of you', 'warn'
               from prs p where p.tone = 'info' and p.asked and ",
            $crate::wants_signal!($kinds, "'review-for-me'"),
            " group by p.tid)"
        )
    };
}

/// The other ends of `$id`'s confirmed links where it is the `$at` end, as
/// `(id)`: one probe of `link_from_idx` or `link_to_idx`, kept a probe per
/// row by `offset 0`. Joined instead, the planner hashed every
/// confirmed link, a scan of the whole table with its proposals (40,000 rows
/// on the probe) for each step of the walk from its starts.
#[doc(hidden)]
#[macro_export]
macro_rules! link_ends {
    ($end:literal, $at:literal, $id:literal) => {
        concat!(
            "select l.",
            $end,
            " as id from knobas.confirmed_link l where l.",
            $at,
            " = ",
            $id,
            " offset 0"
        )
    };
}

/// The kind of the entity `$id`, one probe of `entity_pkey`.
#[doc(hidden)]
#[macro_export]
macro_rules! kind_of {
    ($id:literal) => {
        concat!(
            "(select e.kind from knobas.entity e where e.id = ",
            $id,
            ")"
        )
    };
}

/// The `sync.item` row of the entity `$id` as a lateral subquery, one probe
/// of `item_pkey`: `offset 0` keeps it a probe per row, whatever the planner
/// estimates of the rows it is probed for (#1148).
#[doc(hidden)]
#[macro_export]
macro_rules! item_by_key {
    ($id:literal) => {
        concat!(
            "select i.entity_id, i.source_id, i.kind, i.payload, i.item_updated_at, i.synced_at
               from sync.item i where i.entity_id = ",
            $id,
            " offset 0"
        )
    };
}

/// Whether `$kinds` (a `text[]`, `null` for all) asks for `$kind`.
#[doc(hidden)]
#[macro_export]
macro_rules! wants_signal {
    ($kinds:literal, $kind:literal) => {
        concat!(
            "(",
            $kinds,
            "::text[] is null or ",
            $kind,
            " = any(",
            $kinds,
            "::text[]))"
        )
    };
}

/// Every ticket that could carry a signal of one of `$kinds` (#939): what
/// `has:` hands [`ticket_signals!`], which keeps the tickets among them.
///
/// Only the links that can give the kinds asked for: the `to` end of a
/// `blocks` link for `blocker`; the other end of an open pull request's link
/// for `open-pr`, and of one asking `$me` for a review for `review-for-me`
/// alone ([`crate::open_prs!`]); and for `failing-build`, the ends within
/// two hops of a linked build whose declared state is in the `err` tone
/// ([`crate::err_builds!`]), walked back the way the reach walks out. Before #939
/// every kind walked from every end: `has:blocker` read the builds of
/// 12,000 links to find the 5,000 blocked tickets. Until #1148 a pull
/// request's ends were found by probing the kind of every link's end, 24,000
/// lookups at 22,000 items, and every ticket with any pull request was read.
///
/// A superset is safe and a subset is not: [`ticket_signals!`] decides
/// over these, so an end here that carries no signal is dropped there.
///
/// Each step from a start to its link ends is a probe per row
/// ([`link_ends!`]), and a hop is a context or not by a probe of its entity
/// ([`kind_of!`]). Until #1163 each step was a join, which the planner hashed
/// over every confirmed link, and the context test a hash of every entity.
#[macro_export]
macro_rules! signal_ends {
    ($decl:literal, $me:literal, $kinds:literal) => {
        concat!(
            "(with state_paths as materialized (",
            $crate::state_paths!($decl),
            "),
              eb as materialized (",
            $crate::err_builds!(),
            " and ",
            $crate::wants_signal!($kinds, "'failing-build'"),
            "),
              hop as materialized (
                select l.id from eb cross join lateral (",
            $crate::link_ends!("from_id", "to_id", "eb.id"),
            ") l
                union select l.id from eb cross join lateral (",
            $crate::link_ends!("to_id", "from_id", "eb.id"),
            ") l),
              op as materialized (",
            $crate::open_prs!($decl, $me, $kinds),
            ")
             select hop.id as tid from hop
             union select l.id from hop cross join lateral (",
            $crate::link_ends!("from_id", "to_id", "hop.id"),
            ") l where ",
            $crate::kind_of!("hop.id"),
            " <> 'ctx'
             union select l.id from hop cross join lateral (",
            $crate::link_ends!("to_id", "from_id", "hop.id"),
            ") l where ",
            $crate::kind_of!("hop.id"),
            " <> 'ctx'
             union select l.to_id from knobas.confirmed_link l
               where l.relation = 'blocks' and ",
            $crate::wants_signal!($kinds, "'blocker'"),
            "
             union select l.id from op cross join lateral (",
            $crate::link_ends!("from_id", "to_id", "op.id"),
            ") l
             union select l.id from op cross join lateral (",
            $crate::link_ends!("to_id", "from_id", "op.id"),
            ") l)"
        )
    };
}

/// The live builds whose declared state is in the `err` tone and that are an
/// end of a confirmed link, as `(id)`: [`signal_ends!`]' start for
/// `failing-build`, reading `state_paths` ([`state_paths!`]). The `case` reads
/// the state first, so only a red build is probed for being live and for its
/// links: beside the `case`, the planner put the two liveness lookups first,
/// two probes for each of the 5,000 builds where 600 are red (#1148). The
/// link tests are [`link_ends!`]' probes: as plain `exists`, the planner
/// hashed every confirmed link twice to test the 600.
#[doc(hidden)]
#[macro_export]
macro_rules! err_builds {
    () => {
        concat!(
            "select i.entity_id as id from sync.item i
              where i.kind = 'build' and case when ",
            $crate::resolved_state!(),
            "->>'tone' = 'err'
                then ",
            $crate::live_by_key!("i"),
            "
                 and (exists (",
            $crate::link_ends!("to_id", "from_id", "i.entity_id"),
            ") or exists (",
            $crate::link_ends!("from_id", "to_id", "i.entity_id"),
            "))
                else false end"
        )
    };
}

/// The live pull requests whose declared state is in the `info` tone, the
/// open ones, as `(id)`: [`signal_ends!`]' start for `open-pr` and
/// `review-for-me`, reading `state_paths` and `$decl`, `$me` and `$kinds` as
/// [`ticket_signals!`] does. Asked for `review-for-me` alone, only those that
/// ask one of `$me` for a review ([`asked_of!`]). The state is read first, as
/// in [`err_builds!`].
#[doc(hidden)]
#[macro_export]
macro_rules! open_prs {
    ($decl:literal, $me:literal, $kinds:literal) => {
        concat!(
            "select i.entity_id as id from sync.item i
              where i.kind = 'pr' and (",
            $crate::wants_signal!($kinds, "'open-pr'"),
            " or ",
            $crate::wants_signal!($kinds, "'review-for-me'"),
            ") and case when ",
            $crate::resolved_state!(),
            "->>'tone' = 'info'
                then ",
            $crate::live_by_key!("i"),
            " and (",
            $crate::wants_signal!($kinds, "'open-pr'"),
            " or ",
            $crate::asked_of!($decl, $me),
            ")
                else false end"
        )
    };
}

/// Whether the pull request `i` declares a requested reviewer among `$me`
/// (a `text[]`, lowercased by the server), ignoring case: `review-for-me`'s test, in
/// [`ticket_signals!`] and [`open_prs!`].
#[doc(hidden)]
#[macro_export]
macro_rules! asked_of {
    ($decl:literal, $me:literal) => {
        concat!(
            "exists (select 1 from ",
            $crate::declared_list!($decl, "reviewers"),
            " rv where lower(rv.value) = any(",
            $me,
            "::text[]))"
        )
    };
}

/// Every declared source's `state` candidates for builds and pull requests,
/// read once a statement: `(source_id, kind, segs, ord, m)`, one row per
/// candidate, `m` its map as a jsonb object from each value's text (the value
/// as jsonb, `"open"` or `true`) to `{text, tone}`, the first entry kept where
/// a map names a value twice and only the values that have a tone. Which
/// [`resolved_state!`] reads per row as the CTE `state_paths`. Through
/// [`crate::declared_state!`] each row walked the whole declaration, about
/// 19 µs a build on the probe (#939); through [`crate::state_of!`] over the
/// candidates it still unnested each candidate's values once a row, about
/// 7 µs a build at 22,000 items, and 3.5 µs over one row per value (#1148).
/// One row per candidate, a row reads its value at each path once and looks
/// it up, about 2.3 µs (#1163).
#[doc(hidden)]
#[macro_export]
macro_rules! state_paths {
    ($decl:literal) => {
        concat!(
            "select i.source_id, i.kind, c.segs, c.ord, m.m
               from (select d.source_id, k.kind
                       from jsonb_object_keys(case when jsonb_typeof(",
            $decl,
            "::jsonb) = 'object' then ",
            $decl,
            "::jsonb else '{}'::jsonb end) as d(source_id)
                      cross join (values ('build'), ('pr')) as k(kind)) i
              cross join lateral ",
            $crate::declared_state_candidates!($decl),
            " c
              cross join lateral (
                select jsonb_object_agg((v.e->'value')::text,
                         jsonb_build_object('text', v.e->>'text', 'tone', v.e->>'tone')
                         order by v.vord desc) as m
                  from jsonb_array_elements(
                         case when jsonb_typeof(c.vals) = 'array' then c.vals else '[]'::jsonb end)
                       with ordinality as v(e, vord)
                 where jsonb_typeof(v.e->'tone') = 'string' and v.e->'value' is not null) m
             where m.m is not null"
        )
    };
}

/// The row `i`'s declared state through `state_paths` ([`state_paths!`]):
/// [`crate::declared_state!`]'s answer, by [`crate::state_of!`]'s rule over
/// the maps -- the first candidate whose map holds the string (trimmed, blank
/// as none) or boolean at its path, named by its `text` or else the value.
/// Two jsonb values are equal exactly when their texts are, for a string or
/// a boolean. `payload_paths.rs`'s state-chip cases hold the two spellings to
/// one answer.
#[doc(hidden)]
#[macro_export]
macro_rules! resolved_state {
    () => {
        concat!(
            "(select jsonb_build_object('text', coalesce(s.e->>'text', s.v #>> '{}'), 'tone', s.e->>'tone')
               from (select p.ord, x.v, p.m -> (x.v::text) as e
                       from state_paths p cross join lateral (select ",
            $crate::state_value_at!(),
            " as v) x
                      where p.source_id = i.source_id and p.kind = i.kind) s
              where s.e is not null order by s.ord limit 1)"
        )
    };
}

/// The value at `p.segs` in the row `i`'s payload as [`crate::state_of!`]
/// compares it: a string trimmed (blank as none), a boolean, else none.
#[doc(hidden)]
#[macro_export]
macro_rules! state_value_at {
    () => {
        "case jsonb_typeof(i.payload #> p.segs)
                when 'string' then to_jsonb(nullif(btrim(i.payload #>> p.segs), ''))
                when 'boolean' then i.payload #> p.segs end"
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The four spellings `has:` takes are the four the wire carries.
    #[test]
    fn a_kind_is_spelled_on_the_wire_as_has_takes_it() {
        for kind in SignalKind::ALL {
            assert_eq!(
                serde_json::to_value(kind).unwrap(),
                serde_json::json!(kind.as_str())
            );
            assert_eq!(
                SignalKind::from_spelling(&kind.as_str().to_uppercase()),
                Some(kind)
            );
        }
        assert_eq!(SignalKind::from_spelling("blocked"), None);
    }
}

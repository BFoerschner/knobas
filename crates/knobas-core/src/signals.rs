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

use serde::{Deserialize, Serialize};

use crate::tone::Tone;

/// Which fact a [`Signal`] states: the four values `has:` takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SignalKind {
    /// A build in the ticket's reach is red.
    #[serde(rename = "failing-build")]
    FailingBuild,
    /// Something is in the ticket's way.
    #[serde(rename = "blocker")]
    Blocker,
    /// A pull request of the ticket's is open.
    #[serde(rename = "open-pr")]
    OpenPr,
    /// One of those asks the user for a review.
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signal {
    pub kind: SignalKind,
    /// The words drawn beside the dot (*blocked by OPS-77*).
    pub text: String,
    pub tone: Tone,
}

/// The signals of the tickets `$tickets` names, as a table expression of
/// `(tid, ord, kind, text, tone)`: see the module notes for each rule.
///
/// `$tickets` is a relation with one column, `tid`, of entity ids; an id
/// that is not a ticket carries no signal. `$decl` is where the
/// declarations are bound, as for [`crate::declared_string!`], and `$me` a
/// `text[]` of the configured usernames, lowercased.
///
/// **Set-based, never once per ticket** (#815): the walk joins the tickets
/// to the confirmed links once, so its cost is the links the tickets have,
/// not a walk per ticket. The board names the cards it keeps; `has:` names
/// the link ends that can carry the kinds it wants ([`signal_ends!`]) and
/// asks which of them carry one. The per-row parts are
/// index probes and the declaration's own reads. `builds` keeps the reach's
/// builds by the entity's kind before it joins `sync.item`: joined on the
/// id alone, the planner underestimated the reach and compared every reach
/// row with every build (1.8 million comparisons for 1,800 tickets and 500
/// builds, 230 ms, where this reads 70 ms; PR #933's review).
///
/// `$kinds` is a `text[]` of the kinds wanted, or `null` for all four
/// (#939). Each kind's branch is guarded by it, so `has:blocker` never
/// walks the reach for builds: a guard that is false is a one-time filter,
/// and the walk it guards is never read.
///
/// A blocker is found from the ticket's own links (`to_id`), the relation
/// tested after: in a `lateral ... offset 0`, so the planner cannot read
/// `relation = 'blocks'` against the pair index, whose third column it is.
/// On tables never analyzed it did, and scanned that whole index once a
/// ticket (567 tickets, 91,000 buffers, 130 ms; #939).
#[macro_export]
macro_rules! ticket_signals {
    ($decl:literal, $me:literal, $tickets:expr, $kinds:literal) => {
        concat!(
            "(with state_paths as materialized (",
            $crate::state_paths!($decl),
            "),
              t as (select distinct tk.tid from ",
            $tickets,
            " tk join knobas.entity te on te.id = tk.tid where te.kind = 'ticket'),
              n1 as (select x.tid, x.id from (
                        select t.tid, l.to_id as id
                          from t join knobas.confirmed_link l on l.from_id = t.tid
                        union
                        select t.tid, l.from_id
                          from t join knobas.confirmed_link l on l.to_id = t.tid) x
                      join knobas.entity e on e.id = x.id where e.kind <> 'ctx'),
              reach as materialized (select x.tid, x.id, e.kind from (
                        select n1.tid, n1.id from n1
                        union
                        select n1.tid, l.to_id
                          from n1 join knobas.confirmed_link l on l.from_id = n1.id
                        union
                        select n1.tid, l.from_id
                          from n1 join knobas.confirmed_link l on l.to_id = n1.id) x
                      join knobas.entity e on e.id = x.id
                     where e.kind <> 'ctx' and x.id <> x.tid),
              builds as (select distinct on (b.tid, b.cfg) b.tid, b.tone from (
                        select r.tid,
                               coalesce((select min(p.name) from sync.item_parent p
                                          where p.entity_id = i.entity_id), i.entity_id) as cfg,
                               coalesce(i.item_updated_at, i.synced_at) as at,
                               ",
            $crate::resolved_state!(),
            "->>'tone' as tone
                          from reach r join sync.item i on i.entity_id = r.id and r.kind = 'build'
                         where i.kind = 'build' and ",
            $crate::live_by_key!("i"),
            ") b order by b.tid, b.cfg, b.at desc, b.tone),
              prs as (select n1.tid, substring(i.entity_id from '#([0-9]+)$') as num,
                             ",
            $crate::resolved_state!(),
            "->>'tone' as tone,
                             exists (select 1 from ",
            $crate::declared_list!($decl, "reviewers"),
            " rv where lower(rv.value) = any(",
            $me,
            "::text[])) as asked
                        from n1 join sync.item i on i.entity_id = n1.id
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
               join knobas.entity o on o.id = l.from_id
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
/// `blocks` link for `blocker`; an end whose other end is a pull request for
/// `open-pr` and `review-for-me`; and for `failing-build`, the ends within
/// two hops of a linked build whose declared state is in the `err` tone
/// ([`err_builds!`]), walked back the way the reach walks out. Before #939
/// every kind walked from every end: `has:blocker` read the builds of
/// 12,000 links to find the 5,000 blocked tickets.
///
/// A superset is safe and a subset is not: [`ticket_signals!`] decides
/// over these, so an end here that carries no signal is dropped there.
#[macro_export]
macro_rules! signal_ends {
    ($decl:literal, $kinds:literal) => {
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
                select l.from_id as id from eb join knobas.confirmed_link l on l.to_id = eb.id
                union select l.to_id from eb join knobas.confirmed_link l on l.from_id = eb.id)
             select hop.id as tid from hop
             union select l.from_id from hop join knobas.entity e on e.id = hop.id
               join knobas.confirmed_link l on l.to_id = hop.id where e.kind <> 'ctx'
             union select l.to_id from hop join knobas.entity e on e.id = hop.id
               join knobas.confirmed_link l on l.from_id = hop.id where e.kind <> 'ctx'
             union select l.to_id from knobas.confirmed_link l
               where l.relation = 'blocks' and ",
            $crate::wants_signal!($kinds, "'blocker'"),
            "
             union select l.from_id from knobas.confirmed_link l
               join knobas.entity e on e.id = l.to_id
              where e.kind = 'pr' and (",
            $crate::wants_signal!($kinds, "'open-pr'"),
            " or ",
            $crate::wants_signal!($kinds, "'review-for-me'"),
            ")
             union select l.to_id from knobas.confirmed_link l
               join knobas.entity e on e.id = l.from_id
              where e.kind = 'pr' and (",
            $crate::wants_signal!($kinds, "'open-pr'"),
            " or ",
            $crate::wants_signal!($kinds, "'review-for-me'"),
            "))"
        )
    };
}

/// The live builds whose declared state is in the `err` tone and that are an
/// end of a confirmed link, as `(id)`: [`signal_ends!`]' start for
/// `failing-build`, reading `state_paths` ([`state_paths!`]). The `case` reads the
/// state before the links, so only a red build's links are probed.
#[doc(hidden)]
#[macro_export]
macro_rules! err_builds {
    () => {
        concat!(
            "select i.entity_id as id from sync.item i
              where i.kind = 'build' and ",
            $crate::live_by_key!("i"),
            " and case when ",
            $crate::resolved_state!(),
            "->>'tone' = 'err'
                then exists (select 1 from knobas.confirmed_link l where l.from_id = i.entity_id)
                  or exists (select 1 from knobas.confirmed_link l where l.to_id = i.entity_id)
                else false end"
        )
    };
}

/// Every declared source's `state` candidates for builds and pull requests,
/// resolved once a statement: `(source_id, kind, segs, vals, ord)`, which
/// [`resolved_state!`] reads per row as the CTE `state_paths`. Through
/// [`crate::declared_state!`] each row walked the whole declaration, about
/// 19 µs a build on the probe (#939).
#[doc(hidden)]
#[macro_export]
macro_rules! state_paths {
    ($decl:literal) => {
        concat!(
            "select i.source_id, i.kind, c.segs, c.vals, c.ord
               from (select d.source_id, k.kind
                       from jsonb_object_keys(case when jsonb_typeof(",
            $decl,
            "::jsonb) = 'object' then ",
            $decl,
            "::jsonb else '{}'::jsonb end) as d(source_id)
                      cross join (values ('build'), ('pr')) as k(kind)) i
              cross join lateral ",
            $crate::declared_state_candidates!($decl),
            " c"
        )
    };
}

/// The row `i`'s declared state through `state_paths` ([`state_paths!`]):
/// [`crate::declared_state!`]'s answer, by the one rule ([`crate::state_of!`]).
#[doc(hidden)]
#[macro_export]
macro_rules! resolved_state {
    () => {
        $crate::state_of!(
            "(select p.segs, p.vals, p.ord from state_paths p \
              where p.source_id = i.source_id and p.kind = i.kind)"
        )
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

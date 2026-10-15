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
/// every end of a confirmed link, the only tickets that can carry a signal,
/// and asks which of them carry the kind it wants. The per-row parts are
/// index probes and the declaration's own reads.
#[macro_export]
macro_rules! ticket_signals {
    ($decl:literal, $me:literal, $tickets:expr) => {
        concat!(
            "(with t as (select distinct tk.tid from ",
            $tickets,
            " tk join knobas.entity te on te.id = tk.tid where te.kind = 'ticket'),
              n1 as (select x.tid, x.id from (
                        select t.tid, l.to_id as id
                          from t join knobas.confirmed_link l on l.from_id = t.tid
                        union
                        select t.tid, l.from_id
                          from t join knobas.confirmed_link l on l.to_id = t.tid) x
                      join knobas.entity e on e.id = x.id where e.kind <> 'ctx'),
              reach as (select x.tid, x.id from (
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
            $crate::declared_state!($decl),
            "->>'tone' as tone
                          from reach r join sync.item i on i.entity_id = r.id
                         where i.kind = 'build' and ",
            $crate::live_by_key!("i"),
            ") b order by b.tid, b.cfg, b.at desc, b.tone),
              prs as (select n1.tid, substring(i.entity_id from '#([0-9]+)$') as num,
                             ",
            $crate::declared_state!($decl),
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
               from builds b where b.tone = 'err' group by b.tid
             union all
             select t.tid, 2, 'blocker', 'blocked by ' || case when o.kind = 'ticket'
                      then substr(o.id, strpos(o.id, ':') + 1) else o.title end, 'err'
               from t join knobas.confirmed_link l on l.to_id = t.tid and l.relation = 'blocks'
               join knobas.entity o on o.id = l.from_id
              where o.kind <> 'ctx' and o.deleted_at is null
             union all
             select p.tid, 3, 'open-pr', coalesce('PR #' || p.num || ' open', 'PR open'), 'info'
               from prs p where p.tone = 'info'
             union all
             select p.tid, 4, 'review-for-me', 'review asked of you', 'warn'
               from prs p where p.tone = 'info' and p.asked group by p.tid)"
        )
    };
}

/// Every ticket that could carry a signal: an end of a confirmed link. What
/// `has:` hands [`ticket_signals!`], which keeps the tickets among them.
#[macro_export]
macro_rules! linked_ends {
    () => {
        "(select l.from_id as tid from knobas.confirmed_link l \
          union select l.to_id from knobas.confirmed_link l)"
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

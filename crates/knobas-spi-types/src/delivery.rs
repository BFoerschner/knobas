//! How a write-back op is delivered (ADR-0006, amended 2026-10-19).
//!
//! Every `WriteOp` variant answers one [`Delivery`] through
//! `WriteOp::delivery`, beside its identifier and with the same
//! no-wildcard match, so a new variant does not compile until it says how
//! the queue treats it. The value is Rust-only: an op crosses the WIT as
//! JSON, and nothing here is on that wire.
//!
//! It sits in this crate rather than in `knobas-core` because `knobas-source`
//! depends on this crate and on no other knobas crate (#1234);
//! `knobas-core` re-exports it as `knobas_core::delivery`, as it does
//! [`crate::entity`] and [`crate::payload`].

/// How the queue treats one write-back op: what a hold compares, what a
/// resend and a withdrawal mean, what the write targets and what its arrival
/// links.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Delivery {
    /// Which projection of the target a hold compares.
    pub projection: Projection,
    /// Whether a second arrival changes nothing: the source refuses the
    /// repeat or the write sets what is already set (#1122). `false` holds
    /// a write that went out and got no answer (#1125).
    pub resend_harmless: bool,
    /// Whether withdrawing the write in flight can leave an artefact at the
    /// source that no knobas row names (#336), so the activity log has to
    /// say so.
    pub can_leave_unclaimed: bool,
    /// Which of the op's fields names the entity the write targets: what the
    /// queue orders against and holds on leaving the mirror.
    pub target: TargetField,
    /// The link an arrival draws from what the write made to the job it was
    /// made for, if any.
    pub link: Option<ArrivalLink>,
    /// What else the write moves, read back at once after it lands, each at
    /// its own source and in its own place (ADR-0064). Empty for an op that
    /// moves nothing beyond the item it is read back by.
    pub effects: &'static [Effect],
}

/// What a held write compares. Which one an op gets is a statement about
/// what the op could overwrite (`knobas_core::write_queue::project` argues
/// each).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Projection {
    /// Only whether the target is in the mirror: the op adds beside what the
    /// container holds and overwrites nothing.
    Liveness,
    /// The target's state as its source declares it for the kind (#752):
    /// the op adds to a discussion, and only the item's state moving is a
    /// reason to look again.
    DeclaredState,
    /// The target's status as its kind declares it (`KindPaths::status_name`,
    /// ADR-0054): the op moves the item through its workflow, and only its
    /// status moving is a reason to look again.
    Status,
    /// The whole mirrored record: any change to the target holds.
    ///
    /// `per_field` narrows it to the fields the queued payload carries
    /// (#706): the op overwrites only those, and a hold compares only them.
    Whole { per_field: bool },
}

impl Projection {
    /// The whole record, not narrowed.
    pub const WHOLE: Self = Self::Whole { per_field: false };

    /// Whether this projection reads the target kind's declared state paths,
    /// so the queue knows when to look them up: [`Projection::DeclaredState`],
    /// and a per-field record, whose state change compares the declared
    /// state (#706).
    #[must_use]
    pub fn reads_declared_state(self) -> bool {
        matches!(self, Self::DeclaredState | Self::Whole { per_field: true })
    }

    /// Whether this projection reads the target kind's declared status path
    /// ([`Projection::Status`]), so the queue knows to look the kind's paths
    /// up for it too.
    #[must_use]
    pub fn reads_declared_status(self) -> bool {
        matches!(self, Self::Status)
    }
}

/// Which of an op's fields names its target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetField {
    /// The op's `entity`.
    Entity,
    /// The op's `parent`: a page is created under a parent page.
    Parent,
    /// The op's `repo`: a commit goes into a repository (#822).
    Repository,
}

/// The link an arrival draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArrivalLink {
    /// `related`, the relation a key in a branch's name proposes (#984).
    Related,
    /// `documents`, a page drawn against its job (#1121).
    Documents,
}

impl ArrivalLink {
    /// The relation as `knobas.link` stores it.
    #[must_use]
    pub fn relation(self) -> &'static str {
        match self {
            Self::Related => "related",
            Self::Documents => "documents",
        }
    }
}

/// Something a landed write moves beyond the item it is read back by
/// (`CONTEXT.md` *Effect*, ADR-0064), named in knobas' terms and resolved
/// from the mirror when the write lands, never from one adapter's keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    /// The target's declared parent (`sync.item_parent`): a pull request's
    /// repository, read by the walk under it (ADR-0047), which brings its
    /// branches' heads, a deleted branch's tombstone and the issues a merge
    /// closed.
    Parent,
    /// The tickets the target is linked to (`knobas.confirmed_link`, either
    /// end), each refreshed at whichever source holds it.
    LinkedTickets,
}

#[cfg(test)]
mod tests {
    use super::Projection;

    /// Only a status move looks the kind's status path up; a declared state,
    /// and a record whole or per field, look up the state paths instead (the
    /// final suite's mutants: either constant answer went unseen).
    #[test]
    fn only_a_status_projection_reads_the_declared_status() {
        assert!(Projection::Status.reads_declared_status());
        for other in [
            Projection::Liveness,
            Projection::DeclaredState,
            Projection::WHOLE,
            Projection::Whole { per_field: true },
        ] {
            assert!(!other.reads_declared_status(), "{other:?}");
        }
        assert!(!Projection::Status.reads_declared_state());
    }
}

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

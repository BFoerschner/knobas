//! What a source's **credential** may not write (issue #2040, spec #1192
//! story 10).
//!
//! [`SourceDescriptor::write_ops`](crate::SourceDescriptor::write_ops) says
//! what an adapter can write. It never says what the credential it was given
//! may write: a TeamCity user who only views a project, a Jira token whose
//! user may not comment, a Gitea token with read scopes. A surface that draws
//! a write from `write_ops` alone draws one the credential cannot make, and
//! the source refuses it only after it was queued.
//!
//! So a surface asks first: [`Source::refused_writes`](crate::Source::refused_writes)
//! takes the writes it would offer, as [`WriteAsk`]s, and answers the ones the
//! credential cannot make, each with its reason. The surface draws those
//! disabled with the reason (ADR-0020 rule 4: a credential can be replaced or
//! given the right, so the reader can still reach the state where the write
//! is enabled).
//!
//! **An adapter never finds out with a write that could change anything.**
//! TeamCity has no write that is safe as a probe (a `POST
//! /app/rest/buildQueue` with a build type queues a build), so it reads the
//! user's permissions. Gitea has no read of a token's scopes, and checks the
//! scope before the body, so it sends its creates with an empty body, which
//! are refused for the scope or for the body and make nothing (#2030,
//! #2054). An adapter that cannot tell answers nothing, which is no verdict
//! and draws every write as before.
//!
//! Plain serde data, like everything else crossing the SPI.

/// One write a surface would offer: `op` on `entity`.
///
/// `entity` is an [`EntityRef`](knobas_spi_types::entity::EntityRef) in
/// string form, the address every [`WriteOp`](crate::WriteOp) carries. For a
/// create it is the container the write acts in (a project, a space), the
/// entity the create's own payload names.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct WriteAsk {
    pub entity: String,
    /// A [`WriteOp`](crate::WriteOp) identifier, as `write_ops` spells it.
    pub op: String,
    /// One field of the op's payload, by the name the payload carries it
    /// under (`parameters`, `first_in_queue` on a `run_build`): may the
    /// write carry it set? `None` asks about the write itself (#2055).
    ///
    /// A field can need more than its write: a TeamCity user may run a
    /// build and not override its parameters for that run, or not put it
    /// first in the queue. Such an ask is answered by what the field needs
    /// beyond the write. An adapter that tells no field apart answers it as
    /// it answers the write, which is still true: a write the credential
    /// cannot make carries no field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub field: Option<String>,
}

impl WriteAsk {
    /// The ask about `op` on `entity` itself.
    #[must_use]
    pub fn new(entity: impl Into<String>, op: impl Into<String>) -> Self {
        Self {
            entity: entity.into(),
            op: op.into(),
            field: None,
        }
    }

    /// The ask about `op` on `entity` carrying `field` set.
    #[must_use]
    pub fn with_field(
        entity: impl Into<String>,
        op: impl Into<String>,
        field: impl Into<String>,
    ) -> Self {
        Self {
            field: Some(field.into()),
            ..Self::new(entity, op)
        }
    }
}

/// A write the source's credential cannot make, and why, in words a person
/// can act on: *This TeamCity user cannot run builds in Payout: it lacks the
/// Run build permission*.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct RefusedWrite {
    pub entity: String,
    pub op: String,
    /// The [`WriteAsk::field`] refused, or `None` for the write itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub field: Option<String>,
    pub reason: String,
}

impl RefusedWrite {
    /// The refusal of `ask`, saying `reason`.
    #[must_use]
    pub fn of(ask: &WriteAsk, reason: impl Into<String>) -> Self {
        Self {
            entity: ask.entity.clone(),
            op: ask.op.clone(),
            field: ask.field.clone(),
            reason: reason.into(),
        }
    }

    /// Whether this refuses `ask`.
    #[must_use]
    pub fn refuses(&self, ask: &WriteAsk) -> bool {
        self.entity == ask.entity && self.op == ask.op && self.field == ask.field
    }
}

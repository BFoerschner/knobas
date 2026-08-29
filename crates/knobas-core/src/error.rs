//! The error type every knobas-core store returns.

use uuid::Uuid;

/// What can go wrong in a store call.
///
/// Constraint violations the domain has an opinion about are lifted out of
/// [`sqlx::Error`] into their own variants, so callers match on meaning rather
/// than on SQLSTATE strings. Everything else stays wrapped.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// A unique constraint rejected the write: an identical row already
    /// exists. For links that is an active link with the same
    /// `(from, to, relation)`; the classifier is crate-wide, so the message
    /// stays about rows rather than claiming links.
    #[error("that row already exists")]
    Duplicate,

    /// `unlink` was given an id no link row carries.
    #[error("no link with id {0}")]
    LinkNotFound(Uuid),

    /// `context::promote` was asked to promote a context.
    ///
    /// Refused before any statement runs: a context anchored on a context
    /// would put a `ctx` node at the membership walk's root and union the two
    /// working sets -- the traversal ADR-0008's kind filter exists to refuse.
    #[error("a context cannot be promoted to a context")]
    AnchorIsAContext,

    /// A link was written with an endpoint that has no `knobas.entity` row.
    ///
    /// Produced by [`CoreError::from_link_write`] and by nothing else, so the
    /// message can name links. Linking to an entity that has not synced yet is
    /// an ordinary event rather than a fault, and this variant is what keeps it
    /// from crossing the IPC boundary as `internal`.
    #[error("one of the link's endpoints has no entity")]
    EndpointMissing,

    /// Any other database failure.
    ///
    /// `#[source]`, not `#[from]`: the conversion is hand-written below so it
    /// can classify, and a derived `From` would collide with it.
    #[error("database: {0}")]
    Db(#[source] sqlx::Error),
}

/// SQLSTATE of a unique-violation, which is how the partial unique index
/// `link_active_idx` reports a duplicate active link.
const UNIQUE_VIOLATION: &str = "23505";

/// SQLSTATE of a foreign-key violation: how `link_from_id_fkey` and
/// `link_to_id_fkey` report an endpoint with no `knobas.entity` row.
///
/// The code and not the constraint name, so both ends of a link classify the
/// same way and neither depends on how the migration happened to spell its
/// keys.
const FOREIGN_KEY_VIOLATION: &str = "23503";

impl CoreError {
    /// Classify a failed **link** write, where a foreign-key violation means an
    /// endpoint with no `knobas.entity` row.
    ///
    /// Deliberately not folded into the crate-wide [`From<sqlx::Error>`] below.
    /// `0001` puts foreign keys on `knobas.context.anchor_id` and
    /// `sync.item.entity_id` as well, and violating one of those is knobas' own
    /// bug -- `internal`, not the ordinary "no such thing" a user gets for
    /// naming an entity that has not synced. A crate-wide rule would have to
    /// claim that every foreign key this crate will ever touch is a link
    /// endpoint; keeping the classification next to the statement that makes it
    /// true means the claim cannot rot as this crate grows writes.
    ///
    /// `stores.rs` pins both halves: the link write produces
    /// [`CoreError::EndpointMissing`], and a violation outside one stays
    /// [`CoreError::Db`].
    #[must_use]
    pub(crate) fn from_link_write(error: sqlx::Error) -> Self {
        match error.as_database_error().and_then(|db| db.code()) {
            Some(code) if code == FOREIGN_KEY_VIOLATION => CoreError::EndpointMissing,
            _ => CoreError::from(error),
        }
    }
}

impl From<sqlx::Error> for CoreError {
    /// Classifies as it converts, so a plain `?` on any store query already
    /// yields [`CoreError::Duplicate`] rather than an opaque database error.
    fn from(err: sqlx::Error) -> Self {
        match err.as_database_error().and_then(|db| db.code()) {
            Some(code) if code == UNIQUE_VIOLATION => CoreError::Duplicate,
            _ => CoreError::Db(err),
        }
    }
}

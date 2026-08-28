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

    /// A foreign key rejected the write: the row it points at is not there.
    ///
    /// Every foreign key this crate can violate is a link endpoint --
    /// `knobas.link`'s `from_id` and `to_id` are the only ones its writes
    /// touch -- so the message names one. Linking to an entity that has not
    /// synced yet is an ordinary event rather than a fault, and this variant
    /// is what keeps it from crossing the IPC boundary as `internal`.
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
/// The code and not the constraint name, so both ends classify the same way
/// and neither depends on how the migration happened to spell its keys.
const FOREIGN_KEY_VIOLATION: &str = "23503";

impl From<sqlx::Error> for CoreError {
    /// Classifies as it converts, so a plain `?` on any store query already
    /// yields [`CoreError::Duplicate`] rather than an opaque database error.
    fn from(err: sqlx::Error) -> Self {
        match err.as_database_error().and_then(|db| db.code()) {
            Some(code) if code == UNIQUE_VIOLATION => CoreError::Duplicate,
            Some(code) if code == FOREIGN_KEY_VIOLATION => CoreError::EndpointMissing,
            _ => CoreError::Db(err),
        }
    }
}

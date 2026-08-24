//! The error type every knobas-core store returns.

use uuid::Uuid;

/// What can go wrong in a store call.
///
/// Constraint violations the domain has an opinion about are lifted out of
/// [`sqlx::Error`] into their own variants, so callers match on meaning rather
/// than on SQLSTATE strings. Everything else stays wrapped.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// An identical row already exists -- for links, an active link with the
    /// same `(from, to, relation)`.
    #[error("that link already exists")]
    Duplicate,

    /// `unlink` was given an id no link row carries.
    #[error("no link with id {0}")]
    LinkNotFound(Uuid),

    /// Any other database failure.
    #[error("database: {0}")]
    Db(sqlx::Error),
}

/// SQLSTATE of a unique-violation, which is how the partial unique index
/// `link_active_idx` reports a duplicate active link.
const UNIQUE_VIOLATION: &str = "23505";

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

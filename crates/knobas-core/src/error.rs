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

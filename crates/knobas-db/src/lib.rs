//! Database layer for knobas: owns the embedded PostgreSQL instance and the
//! connection pool every other crate borrows.

pub mod embedded;

#[cfg(feature = "test-util")]
pub mod test_util;

pub use embedded::{DATABASE_NAME, DbConfig, DbError, EmbeddedDb, PG_VERSION_REQ};

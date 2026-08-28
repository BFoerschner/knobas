//! Database layer for knobas: owns the embedded PostgreSQL instance and the
//! connection pool every other crate borrows.
//!
//! Full-text search used to live here as `knobas_db::search`. Ruling P9 moved
//! it -- moved, not copied -- into `knobas-search`, where it answers the frozen
//! `SearchQuery`/`SearchResponse` shape of interfaces §2.4. This crate is the
//! server, the pool and the schema; it has no opinion about queries over them.

pub mod backup;
pub mod embedded;
pub mod migrate;

#[cfg(feature = "test-util")]
pub mod test_util;

pub use embedded::{DATABASE_NAME, DbConfig, DbError, EmbeddedDb, PG_VERSION_REQ};

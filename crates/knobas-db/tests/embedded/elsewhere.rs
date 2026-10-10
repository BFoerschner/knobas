//! A second source file compiled into `embedded`'s binary: the caller of
//! `test_util` that is not `embedded.rs`, for the tests showing that the
//! shared database follows the calling file rather than the process.

/// The database `test_pool` hands this file.
pub async fn database_of_test_pool() -> String {
    super::database_of(&knobas_db::test_util::test_pool().await).await
}

/// The database `test_connector` hands this file.
pub async fn database_of_test_connector() -> String {
    super::database_of_connector(&knobas_db::test_util::test_connector().await).await
}

//! One embedded PostgreSQL for a whole `just test`.
//!
//! Prints the maintenance-database URL on stdout, serves until stdin closes
//! (or a `SIGINT`/`SIGTERM` arrives), then stops the server and releases its
//! scratch root. The recipe exports that URL as `KNOBAS_TEST_DB_URL`, which is
//! how every test binary's shared connector finds this server instead of
//! starting one of its own -- see `knobas_db::test_util`.
//!
//! Test support behind the `test-util` feature (`required-features` in
//! `Cargo.toml`); no shipped build contains it.

#[tokio::main]
async fn main() -> Result<(), knobas_db::DbError> {
    knobas_db::test_util::serve_until_closed(std::io::stdin(), std::io::stdout()).await
}

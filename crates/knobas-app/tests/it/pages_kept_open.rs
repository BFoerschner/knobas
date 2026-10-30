//! *Pages kept open* (#2247) against a real `knobas.setting`: 5 until set,
//! what was stored after a write, and a count past the ceiling stored as 15.

use knobas_app::settings;
use sqlx::PgPool;

async fn scratch(name: &str) -> PgPool {
    knobas_db::test_util::scratch_database(name)
        .await
        .pool(4)
        .await
        .expect("a pool onto this test's own database")
}

#[tokio::test]
async fn pages_kept_open_is_five_until_set_and_then_what_was_stored() {
    let pool = scratch("pages_kept_open_round_trip").await;
    assert_eq!(settings::pages_kept_open(&pool).await.unwrap(), 5);

    assert_eq!(settings::set_pages_kept_open(&pool, 0).await.unwrap(), 0);
    assert_eq!(settings::pages_kept_open(&pool).await.unwrap(), 0, "0 is a value, not unset");

    assert_eq!(settings::set_pages_kept_open(&pool, 40).await.unwrap(), 15);
    assert_eq!(settings::pages_kept_open(&pool).await.unwrap(), 15);
}

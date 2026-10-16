//! Run… presets (#852, R-9.7) through the two commands the dialog calls,
//! `run_presets` and `save_run_preset`, over the embedded PostgreSQL with
//! migration `0037`'s table.
//!
//! Each test names a pipeline of its own, so the presets it reads back are
//! told apart from anything else in this database.

use knobas_app::commands::entity::{run_presets_inner, save_run_preset_inner};
use knobas_app::run_presets::RunPreset;
use knobas_source::BuildParameter;

async fn pool() -> sqlx::PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    pool
}

/// A pipeline id nothing else in this database uses.
fn pipeline() -> String {
    format!(
        "teamcity:buildType:P{}",
        &uuid::Uuid::new_v4().simple().to_string()[..10]
    )
}

fn param(name: &str, value: &str) -> BuildParameter {
    BuildParameter {
        name: name.to_owned(),
        value: value.to_owned(),
    }
}

fn preset(pipeline_id: &str, name: &str) -> RunPreset {
    RunPreset {
        pipeline_id: pipeline_id.to_owned(),
        name: name.to_owned(),
        branch: Some("feature/PAY-231-sepa-retry".to_owned()),
        parameters: vec![
            param("test.filter", "sepa::retry"),
            param("log.level", "debug"),
        ],
        clean_checkout: true,
        first_in_queue: false,
    }
}

/// A saved preset reads back whole, with its overrides in the order they
/// were saved, and only on its own pipeline.
#[tokio::test]
async fn a_saved_preset_reads_back_on_its_pipeline_alone() {
    let pool = pool().await;
    let (tests, other) = (pipeline(), pipeline());
    let saved = save_run_preset_inner(&pool, preset(&tests, "Only the retry tests"))
        .await
        .unwrap();
    assert_eq!(saved, preset(&tests, "Only the retry tests"));

    assert_eq!(
        run_presets_inner(&pool, &tests).await.unwrap(),
        [preset(&tests, "Only the retry tests")]
    );
    assert!(run_presets_inner(&pool, &other).await.unwrap().is_empty());
}

/// Saving a name the pipeline already has replaces that preset, ignoring case
/// and spaces at either end; the name is kept as the last save typed it,
/// trimmed. Presets come back in the order they were first saved.
#[tokio::test]
async fn saving_a_name_again_replaces_that_preset() {
    let pool = pool().await;
    let tests = pipeline();
    save_run_preset_inner(&pool, preset(&tests, "Nightly"))
        .await
        .unwrap();
    save_run_preset_inner(&pool, preset(&tests, "My run"))
        .await
        .unwrap();
    let again = RunPreset {
        branch: None,
        parameters: vec![param("env", "prod")],
        clean_checkout: false,
        first_in_queue: true,
        ..preset(&tests, "  nightly ")
    };
    let saved = save_run_preset_inner(&pool, again).await.unwrap();
    assert_eq!(saved.name, "nightly");

    let all = run_presets_inner(&pool, &tests).await.unwrap();
    let names: Vec<&str> = all.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["nightly", "My run"], "one Nightly, still first");
    assert_eq!(all[0].branch, None);
    assert_eq!(all[0].parameters, [param("env", "prod")]);
    assert!(!all[0].clean_checkout && all[0].first_in_queue);
}

/// What could never run is refused before anything is stored: a pipeline
/// that is no entity id, a blank name, a parameter with no name. A blank
/// branch is the default branch, stored as none.
#[tokio::test]
async fn a_preset_that_could_never_run_is_refused() {
    let pool = pool().await;
    let tests = pipeline();
    for (why, bad) in [
        ("not an id", preset("Payout_IntegrationTests", "x")),
        ("blank name", preset(&tests, "   ")),
        (
            "unnamed parameter",
            RunPreset {
                parameters: vec![param(" ", "9")],
                ..preset(&tests, "x")
            },
        ),
    ] {
        let error = save_run_preset_inner(&pool, bad).await.expect_err(why);
        assert_eq!(
            error.code,
            knobas_app::IpcErrorCode::Invalid,
            "{why}: {error:?}"
        );
    }
    assert!(run_presets_inner(&pool, &tests).await.unwrap().is_empty());
    assert!(
        run_presets_inner(&pool, "not an id").await.is_err(),
        "the read refuses a non-id too"
    );

    let blank = RunPreset {
        branch: Some("  ".to_owned()),
        ..preset(&tests, "Default branch")
    };
    assert_eq!(
        save_run_preset_inner(&pool, blank).await.unwrap().branch,
        None
    );
}

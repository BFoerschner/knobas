//! A build's run-page reads against a wiremock TeamCity (#854, #855, #856,
//! ADR-0022): the whole log, the tests and a test's history, the artifact
//! folders, a preview and the downloads.
//!
//! The endpoint shapes are the ones `src/run_reads.rs` names; whether the real
//! server answers them is `tests/live_teamcity_seeded.rs`'s to say
//! (`a_seeded_runs_log_tests_and_artifacts_read_whole`). On disagreement this
//! fake is what is wrong.

use knobas_source::instance::SourceInstance;
use knobas_source::run::{DownloadSink, DownloadTarget, TestStatus};
use knobas_source::{AuthMethod, Source, SourceError, Tone};
use serde_json::json;
use wiremock::matchers::{header, method, path, query_param, query_param_contains};
use wiremock::{Mock, MockServer, ResponseTemplate};

const TOKEN: &str = "tidewater-tc-token";
const BUILD: &str = "teamcity:build:2401";

fn instance(base_url: String) -> SourceInstance {
    SourceInstance {
        id: "teamcity".to_owned(),
        kind: "teamcity".to_owned(),
        display_name: "Tidewater CI".to_owned(),
        base_url,
        auth: Some(AuthMethod::Pat),
        secret: Some(TOKEN.to_owned()),
        account: None,
        config: json!({}),
    }
}

fn source(server: &MockServer) -> Box<dyn Source> {
    match knobas_source_teamcity::build(instance(server.uri())) {
        Ok(source) => source,
        Err(e) => panic!("the adapter builds: {e:?}"),
    }
}

fn get(p: &str) -> wiremock::MockBuilder {
    Mock::given(method("GET"))
        .and(path(p.to_owned()))
        .and(header("Authorization", format!("Bearer {TOKEN}").as_str()))
}

/// 30,000 lines: one step's heading and then its lines, the last an error.
fn long_log() -> String {
    let mut log = String::from("[10:00:00] : Step 1/1: cargo test\n");
    for i in 2..30_000 {
        log.push_str(&format!("[10:00:01] :\t [Step 1/1] line {i}\n"));
    }
    log.push_str("[10:09:00]E:\t [Step 1/1] Process exited with code 101\n");
    log
}

#[tokio::test]
async fn the_whole_log_is_streamed_and_only_the_range_asked_for_is_answered() {
    let server = MockServer::builder().start().await;
    get("/downloadBuildLog.html")
        .and(query_param("buildId", "2401"))
        .respond_with(ResponseTemplate::new(200).set_body_string(long_log()))
        .mount(&server)
        .await;
    let source = source(&server);

    let tail = source.run_log(BUILD, None, 20_000).await.unwrap().unwrap();
    assert_eq!(tail.total, 30_000);
    assert_eq!(tail.lines.len(), 20_000);
    assert_eq!(tail.lines[0].number, 10_001);
    let last = tail.lines.last().unwrap();
    assert_eq!(
        (last.number, last.text.as_str(), last.tone),
        (30_000, "Process exited with code 101", Some(Tone::Err))
    );
    assert_eq!(tail.steps.len(), 1);
    assert_eq!(
        (
            tail.steps[0].first,
            tail.steps[0].last,
            tail.steps[0].seconds
        ),
        (1, 30_000, Some(540))
    );
    assert_eq!(tail.steps[0].tone, Some(Tone::Err));

    // More than the cap is asked for: the cap is what comes back.
    let capped = source
        .run_log(BUILD, Some(1), 50_000)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(capped.lines.len(), 20_000);
    assert_eq!(capped.lines[0].text, "Step 1/1: cargo test");
}

#[tokio::test]
async fn a_log_of_something_that_is_not_a_build_is_refused_before_any_request() {
    let server = MockServer::builder().start().await;
    let refused = source(&server)
        .run_log("teamcity:buildType:Payout_Build", None, 10)
        .await;
    assert!(
        matches!(refused, Err(SourceError::Protocol { .. })),
        "{refused:?}"
    );
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn tests_come_back_failed_first_with_their_module_details_and_id() {
    let server = MockServer::builder().start().await;
    get("/app/rest/testOccurrences")
        .and(query_param("locator", "build:(id:2401),count:5001"))
        .and(query_param_contains("fields", "test(id)"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "testOccurrence": [
            { "name": "sepa::retry::backs_off_exponentially", "status": "SUCCESS", "duration": 31, "test": { "id": "-11" } },
            { "name": "sepa::retry::gives_up_after_max_attempts", "status": "FAILURE", "duration": 42,
              "newFailure": true, "details": "assertion failed: attempts == 5", "test": { "id": -12 } },
            { "name": "psp.ClientTest.times_out", "status": "UNKNOWN", "ignored": true, "test": { "id": "13" } }
        ]})))
        .mount(&server)
        .await;
    let tests = source(&server).run_tests(BUILD).await.unwrap().unwrap();
    assert!(!tests.truncated);
    let first = &tests.tests[0];
    assert_eq!(first.name, "sepa::retry::gives_up_after_max_attempts");
    assert_eq!(
        (
            first.status,
            first.module.as_deref(),
            first.id.as_deref(),
            first.new_failure,
            first.duration_ms
        ),
        (
            TestStatus::Failed,
            Some("sepa::retry"),
            Some("-12"),
            true,
            Some(42)
        )
    );
    assert_eq!(
        first.details.as_deref(),
        Some("assertion failed: attempts == 5")
    );
    assert_eq!(tests.tests[1].status, TestStatus::Passed);
    assert_eq!(
        (tests.tests[2].status, tests.tests[2].module.as_deref()),
        (TestStatus::Ignored, Some("psp.ClientTest"))
    );
}

#[tokio::test]
async fn past_the_cap_the_failed_tests_are_read_on_their_own_so_none_is_left_out() {
    let server = MockServer::builder().start().await;
    let passed: Vec<_> = (0..5_001)
        .map(|i| json!({ "name": format!("t{i}"), "status": "SUCCESS" }))
        .collect();
    get("/app/rest/testOccurrences")
        .and(query_param("locator", "build:(id:2401),count:5001"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "testOccurrence": passed, "nextHref": "/more" })),
        )
        .mount(&server)
        .await;
    get("/app/rest/testOccurrences")
        .and(query_param(
            "locator",
            "build:(id:2401),status:FAILURE,count:5000",
        ))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "testOccurrence": [
                { "name": "late::failure", "status": "FAILURE" }
            ]})),
        )
        .mount(&server)
        .await;
    let tests = source(&server).run_tests(BUILD).await.unwrap().unwrap();
    assert!(tests.truncated);
    assert_eq!(tests.tests.len(), 5_000);
    assert_eq!(tests.tests[0].name, "late::failure");
}

#[tokio::test]
async fn a_tests_history_is_its_configurations_last_ten_runs_newest_first() {
    let server = MockServer::builder().start().await;
    get("/app/rest/builds/id:2401")
        .and(query_param("fields", "number,buildType(id)"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "buildType": { "id": "Payout_IntegrationTests" } })),
        )
        .mount(&server)
        .await;
    get("/app/rest/testOccurrences")
        .and(query_param("locator", "test:(id:-12),buildType:(id:Payout_IntegrationTests),count:10"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "testOccurrence": [
            { "status": "SUCCESS", "build": { "id": 2399, "number": "1186", "branchName": "main" } },
            { "status": "FAILURE", "build": { "id": 2401, "number": "1187", "branchName": "feature/PAY-231-sepa-retry" } },
            { "status": "FAILURE", "build": { "id": 2398, "number": "1185", "branchName": "feature/PAY-231-sepa-retry" } }
        ]})))
        .mount(&server)
        .await;
    let source = source(&server);
    let history = source.test_history(BUILD, "-12").await.unwrap().unwrap();
    let runs: Vec<_> = history.iter().map(|o| (o.run.as_str(), o.status)).collect();
    assert_eq!(
        runs,
        [
            ("teamcity:build:2401", TestStatus::Failed),
            ("teamcity:build:2399", TestStatus::Passed),
            ("teamcity:build:2398", TestStatus::Failed),
        ]
    );
    assert_eq!(history[1].branch.as_deref(), Some("main"));
    let refused = source.test_history(BUILD, "1),count:(9999").await;
    assert!(
        matches!(refused, Err(SourceError::Protocol { .. })),
        "{refused:?}"
    );
}

#[tokio::test]
async fn a_folder_is_read_by_its_path_and_its_entries_carry_their_whole_path() {
    let server = MockServer::builder().start().await;
    get("/app/rest/builds/id:2401/artifacts")
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "file": [
            { "name": "test report", "children": { "count": 2 } },
            { "name": "SHA256SUMS", "size": 120 }
        ]})))
        .mount(&server)
        .await;
    get("/app/rest/builds/id:2401/artifacts/children/test%20report")
        .and(query_param_contains("fields", "file(name,size"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "file": [
            { "name": "junit.xml", "size": 14_200 }
        ]})))
        .mount(&server)
        .await;
    let source = source(&server);
    let root = source.run_artifacts(BUILD, "").await.unwrap().unwrap();
    assert_eq!(
        (
            root[0].path.as_str(),
            root[0].folder,
            root[0].children,
            root[0].size
        ),
        ("test report", true, Some(2), None)
    );
    assert_eq!((root[1].folder, root[1].size), (false, Some(120)));
    let inside = source
        .run_artifacts(BUILD, "test report")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(inside[0].path, "test report/junit.xml");
    let refused = source.run_artifacts(BUILD, "../../app/rest/users").await;
    assert!(
        matches!(refused, Err(SourceError::Protocol { .. })),
        "{refused:?}"
    );
}

#[tokio::test]
async fn a_preview_is_the_files_text() {
    let server = MockServer::builder().start().await;
    get("/app/rest/builds/id:2401/artifacts/files/logs/psp-sandbox.log")
        .respond_with(ResponseTemplate::new(200).set_body_string("10:08:41 POST /batches 503\n"))
        .mount(&server)
        .await;
    let preview = source(&server)
        .artifact_preview(BUILD, "logs/psp-sandbox.log")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        preview.text.as_deref(),
        Some("10:08:41 POST /batches 503\n")
    );
    assert!(!preview.truncated);
}

#[derive(Default)]
struct Kept {
    name: Option<String>,
    bytes: Vec<u8>,
}

#[async_trait::async_trait]
impl DownloadSink for Kept {
    async fn start(&mut self, file_name: &str) -> Result<(), SourceError> {
        self.name = Some(file_name.to_owned());
        Ok(())
    }
    async fn write(&mut self, bytes: &[u8]) -> Result<(), SourceError> {
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }
}

#[tokio::test]
async fn each_download_streams_its_url_under_the_name_it_suggests() {
    let server = MockServer::builder().start().await;
    get("/app/rest/builds/id:2401")
        .and(query_param("fields", "number,buildType(id)"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({ "number": "1187", "buildType": { "id": "Payout_IntegrationTests" } }),
        ))
        .mount(&server)
        .await;
    get("/downloadBuildLog.html")
        .and(query_param("buildId", "2401"))
        .respond_with(ResponseTemplate::new(200).set_body_string("the log"))
        .mount(&server)
        .await;
    get("/downloadArtifacts.html")
        .and(query_param("buildId", "2401"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"PK all".to_vec()))
        .mount(&server)
        .await;
    get("/app/rest/builds/id:2401/artifacts/archived/test-report")
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"PK folder".to_vec()))
        .mount(&server)
        .await;
    get("/app/rest/builds/id:2401/artifacts/files/test-report/junit.xml")
        .respond_with(ResponseTemplate::new(200).set_body_string("<testsuites/>"))
        .mount(&server)
        .await;
    let source = source(&server);
    for (target, name, bytes) in [
        (
            DownloadTarget::Log,
            "Payout_IntegrationTests-1187.log",
            &b"the log"[..],
        ),
        (
            DownloadTarget::All,
            "Payout_IntegrationTests-1187-artifacts.zip",
            b"PK all",
        ),
        (
            DownloadTarget::Folder {
                path: "test-report".into(),
            },
            "test-report.zip",
            b"PK folder",
        ),
        (
            DownloadTarget::File {
                path: "test-report/junit.xml".into(),
            },
            "junit.xml",
            b"<testsuites/>",
        ),
    ] {
        let mut kept = Kept::default();
        assert!(
            source.download(BUILD, &target, &mut kept).await.unwrap(),
            "{target:?}"
        );
        assert_eq!(kept.name.as_deref(), Some(name), "{target:?}");
        assert_eq!(kept.bytes, bytes, "{target:?}");
    }
}

#[tokio::test]
async fn a_failed_download_never_starts_the_file() {
    let server = MockServer::builder().start().await;
    get("/app/rest/builds/id:2401/artifacts/files/gone.txt")
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    let mut kept = Kept::default();
    let failed = source(&server)
        .download(
            BUILD,
            &DownloadTarget::File {
                path: "gone.txt".into(),
            },
            &mut kept,
        )
        .await;
    assert!(failed.is_err());
    assert_eq!(kept.name, None);
}

//! A build's source view against a wiremock TeamCity (#665, ADR-0020).
//!
//! The mapping and the error classification live here. Whether the real
//! server answers these shapes is `tests/live_teamcity_seeded.rs`'s to say
//! (`the_seeded_failed_builds_view_shows_its_failed_test_and_log_tail`); on
//! disagreement this fake is what is wrong.
//!
//! The records are TeamCity's own shapes, as the seeded 2026.1.3 server
//! answered them on 2026-10-12 -- with two deliberate lies, both leaks a real
//! server does not commit and the view must not depend on: the password
//! parameter **carries its value**, and a VCS root's URL carries the
//! credential it fetches with.

use knobas_source::instance::SourceInstance;
use knobas_source::view::{Block, Fact, SourceView, Tone, ViewRow, ViewSection};
use knobas_source::{AuthMethod, Source, SourceError};
use serde_json::json;
use wiremock::matchers::{header, method, path, query_param, query_param_contains};
use wiremock::{Mock, MockServer, ResponseTemplate};

const TOKEN: &str = "tidewater-tc-token";
const BUILD: &str = "teamcity:build:2401";
const BUILD_PATH: &str = "/app/rest/builds/id:2401";
/// What the fake's password parameter holds, and what no view may contain.
const SECRET: &str = "s3pa-sandbox-key-never-drawn";
/// The credential in a fake VCS root URL, which no view may contain either.
const ROOT_TOKEN: &str = "root-token-never-drawn";
const SHA: &str = "179eb1fb0144c3b61ed482890f1a5f4e162736eb";

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

fn build_record() -> serde_json::Value {
    json!({
        "id": 2401, "buildTypeId": "Payout_IntegrationTests", "number": "1187",
        "status": "FAILURE", "state": "finished",
        "branchName": "feature/PAY-231-sepa-retry",
        "statusText": "Tests failed: 1 (1 new), passed: 0",
        "buildType": { "id": "Payout_IntegrationTests", "name": "IntegrationTests",
                       "projectName": "Payout",
                       "parameters": { "property": [
                           { "name": "env.SEPA_RETRIES" },
                           { "name": "env.SEPA_SANDBOX_KEY",
                             "type": { "rawValue": "password display='hidden'" } },
                           { "name": "tidewater.region" }
                       ]}},
        "queuedDate": "20261012T195139+0000",
        "startDate": "20261012T195140+0000",
        "finishDate": "20261012T195151+0000",
        "triggered": { "type": "user", "user": { "username": "knobas" } },
        "agent": { "name": "knobas-agent" },
        "testOccurrences": { "failed": 1 },
        "revisions": { "count": 1, "revision": [
            { "version": SHA,
              "vcsBranchName": "refs/heads/feature/PAY-231-sepa-retry",
              "vcs-root-instance": {
                  "name": "Payout / payout-service",
                  "properties": root(&format!(
                      "http://knobas:{ROOT_TOKEN}@gitea:3000/tidewater/payout-service.git"
                  ))["properties"].clone() } }
        ]},
        "properties": { "property": [
            { "name": "env.SEPA_RETRIES", "value": "5" },
            { "name": "env.SEPA_SANDBOX_KEY", "value": SECRET },
            { "name": "tidewater.region", "value": "eu-central-1" },
            { "name": "vcsroot.url",
              "value": format!("http://knobas:{ROOT_TOKEN}@gitea:3000/tidewater/payout-service.git") }
        ]}
    })
}

fn root(url: &str) -> serde_json::Value {
    json!({ "properties": { "property": [
        { "name": "authMethod", "value": "PASSWORD" },
        { "name": "secure:password" },
        { "name": "url", "value": url }
    ]}})
}

fn changes() -> serde_json::Value {
    json!({ "count": 2, "change": [
        { "id": 60, "version": SHA, "username": "mara", "date": "20261012T193000+0000",
          "comment": "PAY-231: cap retries at 5\n\nThe PSP counts the first try.\n",
          "vcsRootInstance": root(&format!(
              "http://knobas:{ROOT_TOKEN}@gitea:3000/tidewater/payout-service.git")) },
        { "id": 59, "version": "f653a80f67e415892fddb85bec9efd1c96c5b888", "username": "jonas",
          "date": "20261012T190000+0000", "comment": "",
          "vcsRootInstance": root("git@gitea:tidewater/payout-service.git") }
    ]})
}

fn failed_tests() -> serde_json::Value {
    json!({ "count": 2, "testOccurrence": [
        { "name": "sepa::retry::gives_up_after_max_attempts", "status": "FAILURE",
          "newFailure": true, "muted": false,
          "details": "assertion failed: attempts == 5 (left: 6, right: 5)\n" },
        { "name": "sepa::flaky_clock", "status": "FAILURE",
          "newFailure": false, "muted": true, "details": "" }
    ]})
}

/// A log of `n` numbered lines.
fn log(n: usize) -> String {
    (1..=n)
        .map(|i| format!("[19:51:4{}] line {i}\n", i % 10))
        .collect()
}

/// A TeamCity that answers the build, its changes, its failed tests, its
/// artifacts and a 450-line log.
async fn teamcity() -> MockServer {
    teamcity_with_log(log(450)).await
}

async fn teamcity_with_log(log: String) -> MockServer {
    let server = MockServer::builder().start().await;
    let bearer = format!("Bearer {TOKEN}");
    let get = |p: &str| {
        Mock::given(method("GET"))
            .and(path(p.to_owned()))
            .and(header("Authorization", bearer.as_str()))
    };
    get(BUILD_PATH)
        .and(query_param_contains("fields", "testOccurrences(failed)"))
        .and(query_param_contains(
            "fields",
            "buildType(id,name,projectName,parameters(property(name,type(rawValue))))",
        ))
        .and(query_param_contains(
            "fields",
            "properties(property(name,value))",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(build_record()))
        .mount(&server)
        .await;
    get("/app/rest/changes")
        .and(query_param("locator", "build:(id:2401)"))
        .and(query_param_contains(
            "fields",
            "vcsRootInstance(properties(",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(changes()))
        .mount(&server)
        .await;
    get("/app/rest/testOccurrences")
        .and(query_param(
            "locator",
            "build:(id:2401),status:FAILURE,count:100",
        ))
        .and(query_param_contains(
            "fields",
            "testOccurrence(name,details",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(failed_tests()))
        .mount(&server)
        .await;
    get("/app/rest/builds/id:2401/artifacts")
        .and(query_param_contains("fields", "file(name,size"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "file": [
            { "name": "sepa-report.txt", "size": 150 },
            { "name": "coverage", "children": { "count": 3 } },
            { "name": "bundle.tar.gz", "size": 5_400_000 }
        ]})))
        .mount(&server)
        .await;
    get("/downloadBuildLog.html")
        .and(query_param("buildId", "2401"))
        .respond_with(ResponseTemplate::new(200).set_body_string(log))
        .mount(&server)
        .await;
    server
}

fn source(server: &MockServer) -> Box<dyn Source> {
    match knobas_source_teamcity::build(instance(server.uri())) {
        Ok(source) => source,
        Err(e) => panic!("the adapter builds: {e:?}"),
    }
}

async fn read(server: &MockServer, section: Option<&str>) -> SourceView {
    source(server)
        .view(BUILD, section)
        .await
        .expect("the read succeeds")
        .expect("TeamCity draws a build")
}

fn section<'v>(view: &'v SourceView, id: &str) -> &'v ViewSection {
    view.sections
        .iter()
        .find(|s| s.id == id)
        .unwrap_or_else(|| panic!("no section {id:?} in {view:#?}"))
}

fn fact<'f>(facts: &'f [Fact], label: &str) -> &'f Fact {
    facts
        .iter()
        .find(|f| f.label == label)
        .unwrap_or_else(|| panic!("no fact {label:?} in {facts:#?}"))
}

fn rows(block: &Block) -> &[ViewRow] {
    match block {
        Block::Entities { rows, .. } => rows,
        other => panic!("expected an entity list, got {other:#?}"),
    }
}

async fn paths_asked(server: &MockServer) -> Vec<String> {
    server
        .received_requests()
        .await
        .expect("recorded")
        .iter()
        .map(|r| r.url.path().to_owned())
        .collect()
}

#[tokio::test]
async fn a_build_opens_on_its_overview_and_reads_neither_its_log_nor_its_tests_until_asked() {
    let server = teamcity().await;
    let view = read(&server, None).await;
    let tabs: Vec<_> = view
        .sections
        .iter()
        .map(|s| (s.id.as_str(), s.title.as_str(), s.count, s.deferred))
        .collect();
    assert_eq!(
        tabs,
        [
            ("overview", "Overview", None, false),
            ("changes", "Changes", Some(2), false),
            ("tests", "Tests", Some(1), true),
            ("log", "Log", None, true),
            ("artifacts", "Artifacts", Some(3), false),
            ("parameters", "Parameters", Some(4), false),
        ]
    );
    for deferred in ["tests", "log"] {
        assert!(section(&view, deferred).blocks.is_empty());
    }
    let mut asked = paths_asked(&server).await;
    asked.sort();
    assert_eq!(
        asked,
        [
            "/app/rest/builds/id:2401",
            "/app/rest/builds/id:2401/artifacts",
            "/app/rest/changes",
        ],
        "no log and no test read on the first call"
    );
}

#[tokio::test]
async fn the_overview_says_what_happened_where_and_for_how_long() {
    let server = teamcity().await;
    let view = read(&server, None).await;
    let overview = section(&view, "overview");
    let Block::Facts { facts } = &overview.blocks[0] else {
        panic!("{overview:#?}");
    };
    let outcome = fact(facts, "Outcome");
    assert_eq!(
        (outcome.value.as_str(), outcome.tone),
        ("failed", Some(Tone::Err))
    );
    assert_eq!(
        fact(facts, "Status").value,
        "Tests failed: 1 (1 new), passed: 0"
    );
    assert_eq!(fact(facts, "Number").value, "#1187");
    assert_eq!(fact(facts, "Branch").value, "feature/PAY-231-sepa-retry");
    assert_eq!(fact(facts, "Triggered by").value, "knobas");
    assert_eq!(fact(facts, "Queued").value, "2026-10-12 19:51 +00:00");
    assert_eq!(fact(facts, "Started").value, "2026-10-12 19:51 +00:00");
    assert_eq!(fact(facts, "Finished").value, "2026-10-12 19:51 +00:00");
    assert_eq!(fact(facts, "Duration").value, "11s");
    assert_eq!(fact(facts, "Agent").value, "knobas-agent");

    // The configuration is an address: its own detail opens from here.
    let config = rows(&overview.blocks[1]);
    assert_eq!(
        config,
        [ViewRow {
            entity_id: Some("teamcity:buildType:Payout_IntegrationTests".into()),
            title: "Payout / IntegrationTests".into(),
            meta: None,
            tone: None,
            chip: None,
            url: Some(format!(
                "{}/buildConfiguration/Payout_IntegrationTests?mode=builds",
                server.uri()
            )),
            commit: None,
            author: None,
        }]
    );
}

/// A change names its commit by repository and sha -- the app resolves that
/// against the mirror (#665) -- and the repository's URL keeps no credential.
#[tokio::test]
async fn a_change_names_its_commit_by_repository_and_sha() {
    let server = teamcity().await;
    let view = read(&server, None).await;
    let changes = rows(&section(&view, "changes").blocks[0]);
    assert_eq!(changes.len(), 2);

    let first = &changes[0];
    assert_eq!(first.title, "PAY-231: cap retries at 5");
    assert_eq!(
        first.entity_id, None,
        "the adapter cannot see Gitea's mirror"
    );
    assert_eq!(first.chip.as_deref(), Some("179eb1fb0144"));
    let meta = first.meta.as_deref().expect("a meta line");
    assert!(meta.starts_with("mara · "), "{meta}");
    assert_eq!(
        first.url.as_deref(),
        Some(format!("{}/change/60", server.uri()).as_str())
    );
    let hint = first.commit.as_ref().expect("a commit hint");
    assert_eq!(
        hint.repo_url,
        "http://gitea:3000/tidewater/payout-service.git"
    );
    assert_eq!(hint.sha, SHA);

    let second = &changes[1];
    assert_eq!(second.title, "f653a80f67e4", "no message: the sha names it");
    assert_eq!(
        second.commit.as_ref().map(|h| h.repo_url.as_str()),
        Some("gitea:tidewater/payout-service.git"),
        "an scp-like remote loses its user too; the app finds no web page for it"
    );

    let wire = serde_json::to_string(&view).expect("serializes");
    assert!(!wire.contains(ROOT_TOKEN), "a root's credential: {wire}");
}

/// The revision a build ran on is on its overview, named by repository and
/// sha like a change (#683): a first build or a re-run lists no change, and
/// still opens the commit it built.
#[tokio::test]
async fn the_overview_names_the_revision_it_built_by_repository_and_sha() {
    let server = teamcity().await;
    let view = read(&server, None).await;
    let overview = section(&view, "overview");
    let revision = overview
        .blocks
        .iter()
        .find(|b| matches!(b, Block::Entities { title: Some(t), .. } if t == "Revision"))
        .unwrap_or_else(|| panic!("no revision block: {overview:#?}"));
    assert_eq!(
        rows(revision),
        [ViewRow {
            entity_id: None,
            title: "Payout / payout-service".into(),
            meta: Some("feature/PAY-231-sepa-retry".into()),
            tone: None,
            chip: Some("179eb1fb0144".into()),
            url: None,
            commit: Some(knobas_source::view::CommitHint {
                repo_url: "http://gitea:3000/tidewater/payout-service.git".into(),
                sha: SHA.into(),
            }),
            author: None,
        }]
    );
    let wire = serde_json::to_string(&view).expect("serializes");
    assert!(!wire.contains(ROOT_TOKEN), "a root's credential: {wire}");
}

/// More failed tests than one read lists: the tab keeps the build's true
/// count and ends on a muted row that sends the reader to TeamCity (#683).
/// A server answering one full read of failed tests (100 rows) for a build
/// whose record counts `failed` of them.
async fn a_full_page_of_failed_tests(failed: u64) -> MockServer {
    let server = MockServer::builder().start().await;
    let page: Vec<_> = (0..100)
        .map(|i| json!({ "name": format!("t{i}"), "newFailure": false, "muted": false }))
        .collect();
    Mock::given(method("GET"))
        .and(path("/app/rest/testOccurrences"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "count": 100, "testOccurrence": page })),
        )
        .mount(&server)
        .await;
    let mut record = build_record();
    record["testOccurrences"] = json!({ "failed": failed });
    Mock::given(method("GET"))
        .and(path(BUILD_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(record))
        .mount(&server)
        .await;
    server
}

/// Exactly one read's worth of failures is the whole list: the count is that
/// many and no cut row follows the last test.
#[tokio::test]
async fn exactly_one_reads_worth_of_failed_tests_ends_on_no_cut_row() {
    let server = a_full_page_of_failed_tests(100).await;
    let view = read(&server, Some("tests")).await;
    let tests = section(&view, "tests");
    assert_eq!(tests.count, Some(100));
    let muted = tests
        .blocks
        .iter()
        .filter(|b| matches!(b, Block::Entities { rows, .. } if rows.iter().any(|r| r.tone == Some(Tone::Muted))))
        .count();
    assert_eq!(muted, 0, "a whole list is not cut: {tests:#?}");
}

#[tokio::test]
async fn more_failed_tests_than_one_read_keep_their_total_and_end_on_a_cut_row() {
    let server = a_full_page_of_failed_tests(142).await;

    let view = read(&server, Some("tests")).await;
    let tests = section(&view, "tests");
    assert_eq!(
        tests.count,
        Some(142),
        "the opened tab keeps the true total"
    );
    let last = rows(tests.blocks.last().expect("blocks"));
    assert_eq!(
        last,
        [ViewRow {
            entity_id: None,
            title: "The first 100 of 142 failed tests; the rest are on TeamCity.".into(),
            meta: None,
            tone: Some(Tone::Muted),
            chip: None,
            url: Some(format!(
                "{}/buildConfiguration/Payout_IntegrationTests/2401?buildTab=tests",
                server.uri()
            )),
            commit: None,
            author: None,
        }]
    );
    let listed = tests
        .blocks
        .iter()
        .filter(|b| matches!(b, Block::Entities { rows, .. } if rows[0].tone == Some(Tone::Err)))
        .count();
    assert_eq!(listed, 100);
}

#[tokio::test]
async fn the_tests_tab_reads_the_failed_tests_with_their_details() {
    let server = teamcity().await;
    let view = read(&server, Some("tests")).await;
    let ids: Vec<_> = view.sections.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["tests"]);
    let tests = section(&view, "tests");
    assert!(!tests.deferred, "a section read on its own is not deferred");

    let first = rows(&tests.blocks[0]);
    assert_eq!(
        first
            .iter()
            .map(|r| (r.title.as_str(), r.chip.as_deref(), r.tone))
            .collect::<Vec<_>>(),
        [(
            "sepa::retry::gives_up_after_max_attempts",
            Some("new"),
            Some(Tone::Err)
        )]
    );
    assert_eq!(
        tests.blocks[1],
        Block::Text {
            text: "assertion failed: attempts == 5 (left: 6, right: 5)".into(),
            markdown: false
        }
    );
    // A muted failure says so, and one without details draws no empty text.
    let second = rows(&tests.blocks[2]);
    assert_eq!(
        (second[0].chip.as_deref(), second[0].tone),
        (Some("muted"), Some(Tone::Muted))
    );
    assert_eq!(tests.blocks.len(), 3, "{tests:#?}");
    assert_eq!(
        paths_asked(&server).await,
        ["/app/rest/testOccurrences"],
        "the tests tab reads the tests and nothing else"
    );
}

#[tokio::test]
async fn the_log_tab_is_the_last_four_hundred_lines_and_says_it_was_cut() {
    let server = teamcity().await;
    let view = read(&server, Some("log")).await;
    let [Block::Log { text, truncated }] = section(&view, "log").blocks.as_slice() else {
        panic!("{view:#?}");
    };
    assert!(*truncated, "450 lines were cut to 400");
    let lines: Vec<_> = text.lines().collect();
    assert_eq!(lines.len(), 400);
    assert_eq!(lines[0], "[19:51:41] line 51");
    assert_eq!(lines[399], "[19:51:40] line 450");
    assert_eq!(
        paths_asked(&server).await,
        ["/downloadBuildLog.html"],
        "the log tab reads the log and nothing else"
    );

    let short = teamcity_with_log(log(12)).await;
    let view = read(&short, Some("log")).await;
    let [Block::Log { text, truncated }] = section(&view, "log").blocks.as_slice() else {
        panic!("{view:#?}");
    };
    assert!(!*truncated);
    assert_eq!(text.lines().count(), 12);
}

/// A log far larger than what is kept while it streams still ends in its
/// own last lines, and is marked cut.
#[tokio::test]
async fn a_huge_log_is_streamed_and_only_its_end_is_kept() {
    let server = teamcity_with_log(log(60_000)).await;
    let view = read(&server, Some("log")).await;
    let [Block::Log { text, truncated }] = section(&view, "log").blocks.as_slice() else {
        panic!("{view:#?}");
    };
    assert!(*truncated);
    let lines: Vec<_> = text.lines().collect();
    assert_eq!(lines.len(), 400);
    assert_eq!(lines[399], "[19:51:40] line 60000");
    assert_eq!(lines[0], "[19:51:41] line 59601");
}

#[tokio::test]
async fn artifacts_are_listed_with_their_sizes_and_their_download() {
    let server = teamcity().await;
    let view = read(&server, None).await;
    let drawn: Vec<_> = rows(&section(&view, "artifacts").blocks[0])
        .iter()
        .map(|r| (r.title.clone(), r.meta.clone(), r.url.clone()))
        .collect();
    let download = |name: &str| {
        Some(format!(
            "{}/repository/download/Payout_IntegrationTests/2401:id/{name}",
            server.uri()
        ))
    };
    assert_eq!(
        drawn,
        [
            (
                "sepa-report.txt".to_owned(),
                Some("150 B".to_owned()),
                download("sepa-report.txt")
            ),
            ("coverage/".to_owned(), Some("3 files".to_owned()), None),
            (
                "bundle.tar.gz".to_owned(),
                Some("5.1 MB".to_owned()),
                download("bundle.tar.gz")
            ),
        ]
    );
}

#[tokio::test]
async fn parameters_are_what_the_build_ran_with_and_never_carry_a_secret() {
    let server = teamcity().await;
    let view = read(&server, None).await;
    let parameters = section(&view, "parameters");
    let [Block::Settings { entity, values, .. }] = parameters.blocks.as_slice() else {
        panic!("{parameters:#?}");
    };
    assert_eq!(entity, BUILD);
    let drawn: Vec<_> = values
        .iter()
        .map(|s| (s.name.as_str(), s.value.as_str(), s.secret, s.editable))
        .collect();
    assert_eq!(
        drawn,
        [
            ("env.SEPA_RETRIES", "5", false, false),
            ("env.SEPA_SANDBOX_KEY", "", true, false),
            ("tidewater.region", "eu-central-1", false, false),
            (
                "vcsroot.url",
                "http://gitea:3000/tidewater/payout-service.git",
                false,
                false
            ),
        ]
    );
    let wire = serde_json::to_string(&view).expect("serializes");
    assert!(!wire.contains(SECRET), "a password's value: {wire}");
    assert!(!wire.contains(ROOT_TOKEN), "a URL's credential: {wire}");
}

/// TeamCity's own mask (`*******`) is a secret too, whatever the spec says:
/// the agent's `system.teamcity.auth.password` has no spec on any
/// configuration.
#[tokio::test]
async fn a_value_teamcity_masked_is_drawn_as_a_secret() {
    let server = MockServer::builder().start().await;
    let mut record = build_record();
    record["properties"] = json!({ "property": [
        { "name": "system.teamcity.auth.password", "value": "*******" }
    ]});
    Mock::given(method("GET"))
        .and(path(BUILD_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(record))
        .mount(&server)
        .await;
    let view = read(&server, Some("parameters")).await;
    let [Block::Settings { values, .. }] = section(&view, "parameters").blocks.as_slice() else {
        panic!("{view:#?}");
    };
    assert_eq!((values[0].value.as_str(), values[0].secret), ("", true));
}

/// A refusal is the error it maps to, status and all (ADR-0020 rule 2).
#[tokio::test]
async fn a_build_the_server_does_not_know_is_its_404() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/app/rest/builds/id:99"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({
            "errors": [{ "message": "No build found by locator 'id:99'." }]
        })))
        .mount(&server)
        .await;
    let failed = source(&server).view("teamcity:build:99", None).await;
    let Err(error) = failed else {
        panic!("a 404 must be an error, got {failed:?}");
    };
    assert_eq!(error.status(), Some(404), "{error:?}");
}

#[tokio::test]
async fn a_build_id_that_is_not_one_or_a_section_it_lacks_is_refused_unsent() {
    let server = teamcity().await;
    for bad in [
        "teamcity:build:abc",
        "teamcity:build:",
        "teamcity:build:1,2",
    ] {
        let refused = source(&server).view(bad, None).await;
        assert!(
            matches!(refused, Err(SourceError::Protocol { .. })),
            "{bad}: {refused:?}"
        );
    }
    let refused = source(&server).view(BUILD, Some("steps")).await;
    assert!(
        matches!(&refused, Err(SourceError::Protocol { message, .. }) if message.contains("steps")),
        "{refused:?}"
    );
    assert!(paths_asked(&server).await.is_empty());
}

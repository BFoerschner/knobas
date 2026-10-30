//! The diff between any two commits (#2271, spec #2234 step 6b): the
//! repository's section `compare:<base>...<head>`, read from Gitea's compare
//! as a diff. `base` may be `<sha>^`, the commit before it, which is how a
//! range picked in *Commits* names the parent of its oldest commit (Gitea
//! 1.27.3 resolves `^` in a compare, measured 2026-10-30).

use knobas_source::Source;
use knobas_source::view::Block;
use knobas_source::view::Standing::{Fixed, Live};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::support;

const REPO: &str = "/api/v1/repos/tidewater/payout-service";
const ENTITY: &str = "gitea:tidewater/payout-service";
const BASE: &str = "97cb5a8667f8aac8e6cdaeff83ffec9574f56a4b";
const HEAD: &str = "1262a5b610480702cfed4a1f3f76c87bf68fae55";

const DIFF: &str = "diff --git a/src/retry.rs b/src/retry.rs\n\
index 1111111..2222222 100644\n\
--- a/src/retry.rs\n\
+++ b/src/retry.rs\n\
@@ -1,2 +1,2 @@\n\
 fn retry() {\n\
-    once();\n\
+    twice();\n";

fn source(server: &MockServer) -> Box<dyn Source> {
    support::source(server.uri(), serde_json::json!({}))
}

async fn served(base_wire: &str, head_wire: &str) -> MockServer {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path(format!("{REPO}/compare/{base_wire}...{head_wire}")))
        .and(query_param("output", "diff"))
        .respond_with(ResponseTemplate::new(200).set_body_string(DIFF))
        .mount(&server)
        .await;
    // The same compare's commits (#2310): HEAD, whose parent is BASE.
    Mock::given(method("GET"))
        .and(path(format!("{REPO}/compare/{base_wire}...{head_wire}")))
        .and(query_param("files", "false"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "total_commits": 1,
            "commits": [{ "sha": HEAD, "parents": [{ "sha": BASE }] }]
        })))
        .mount(&server)
        .await;
    server
}

fn paths_of(files: &[knobas_source::view::DiffFile]) -> Vec<&str> {
    files.iter().map(|file| file.path.as_str()).collect()
}

#[tokio::test]
async fn a_repositorys_compare_section_is_the_diff_between_its_two_commits() {
    let server = served(BASE, HEAD).await;
    let section = format!("compare:{BASE}...{HEAD}");
    let view = source(&server)
        .view(ENTITY, Some(&section))
        .await
        .unwrap()
        .expect("drawn");
    assert_eq!(view.sections.len(), 1);
    let drawn = &view.sections[0];
    assert_eq!(
        drawn.id, section,
        "the section answers under the id it was asked by"
    );
    assert_eq!(drawn.count, Some(1));
    let Block::Diff { files, range, .. } = &drawn.blocks[0] else {
        panic!("a diff: {drawn:#?}");
    };
    assert_eq!(paths_of(files), ["src/retry.rs"]);
    // What it is between, read in the repository it was asked of (#2310).
    let range = range.as_ref().expect("a listing that settles both ends");
    assert_eq!(
        (
            range.repository.as_str(),
            range.base.as_str(),
            range.head.as_str()
        ),
        (ENTITY, BASE, HEAD)
    );
    assert!(view.actions.is_empty(), "a diff offers no write");
    // Only the compare, as a diff and as its commits: no repository record.
    let asked: Vec<String> = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .map(|r| r.url.path().to_owned())
        .collect();
    assert_eq!(
        asked,
        [
            format!("{REPO}/compare/{BASE}...{HEAD}"),
            format!("{REPO}/compare/{BASE}...{HEAD}")
        ]
    );
}

#[tokio::test]
async fn a_base_before_a_commit_and_branch_names_reach_gitea_escaped() {
    let server = served(&format!("{BASE}%5E"), "feature%2Fsepa").await;
    let view = source(&server)
        .view(ENTITY, Some(&format!("compare:{BASE}^...feature/sepa")))
        .await
        .unwrap()
        .expect("drawn");
    let Block::Diff { files, .. } = &view.sections[0].blocks[0] else {
        panic!("a diff");
    };
    assert_eq!(paths_of(files), ["src/retry.rs"]);
}

#[tokio::test]
async fn a_compare_section_without_two_ends_is_refused() {
    let server = MockServer::builder().start().await;
    for bad in [
        "compare:",
        "compare:main",
        "compare:...main",
        "compare:main...",
    ] {
        let error = source(&server).view(ENTITY, Some(bad)).await.unwrap_err();
        assert!(error.to_string().contains("compare"), "{bad}: {error}");
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

/// Kept by the app (ADR-0032) only where both ends are whole shas, the
/// commit before one included: a branch name moves.
#[tokio::test]
async fn a_compare_between_whole_shas_is_fixed_and_one_naming_a_branch_is_live() {
    let server = MockServer::builder().start().await;
    let source = source(&server);
    for (section, standing) in [
        (format!("compare:{BASE}...{HEAD}"), Fixed),
        (format!("compare:{BASE}^...{HEAD}"), Fixed),
        (format!("compare:main...{HEAD}"), Live),
        (format!("compare:{BASE}...main"), Live),
        (format!("compare:{}...{HEAD}", &BASE[..12]), Live),
        (format!("compare:{BASE}^^...{HEAD}"), Live),
    ] {
        assert_eq!(
            source.view_standing(ENTITY, Some(&section)),
            standing,
            "{section}"
        );
    }
    assert_eq!(
        source.view_standing(
            &format!("{ENTITY}@{HEAD}"),
            Some(&format!("compare:{BASE}...{HEAD}"))
        ),
        Live,
        "a commit's view has no compare section"
    );
}

/// The repository's first commit has no commit before it: Gitea answers
/// `<root>^` with 404 (1.27.3, measured 2026-10-30) and the commit with no
/// parents.
async fn root_served(parents: serde_json::Value) -> MockServer {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path(format!("{REPO}/compare/{BASE}%5E...{HEAD}")))
        .respond_with(ResponseTemplate::new(404).set_body_json(
            serde_json::json!({ "message": format!("ref \"{BASE}^\" does not exist") }),
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("{REPO}/compare/{BASE}%5E...{BASE}")))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("{REPO}/git/commits/{BASE}")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({ "sha": BASE, "parents": parents })),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("{REPO}/git/commits/{BASE}.diff")))
        .respond_with(ResponseTemplate::new(200).set_body_string(DIFF))
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn the_first_commit_alone_as_a_range_is_its_own_diff() {
    let server = root_served(serde_json::json!([])).await;
    let view = source(&server)
        .view(ENTITY, Some(&format!("compare:{BASE}^...{BASE}")))
        .await
        .unwrap()
        .expect("drawn");
    let Block::Diff { files, .. } = &view.sections[0].blocks[0] else {
        panic!("a diff");
    };
    assert_eq!(paths_of(files), ["src/retry.rs"]);
}

#[tokio::test]
async fn a_longer_range_from_the_first_commit_is_refused_saying_why() {
    let server = root_served(serde_json::json!([])).await;
    let error = source(&server)
        .view(ENTITY, Some(&format!("compare:{BASE}^...{HEAD}")))
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("first commit"),
        "the refusal names the first commit: {error}"
    );
}

#[tokio::test]
async fn a_compare_gitea_does_not_find_past_a_commit_with_a_parent_is_its_404() {
    let server = root_served(serde_json::json!([{ "sha": HEAD }])).await;
    let error = source(&server)
        .view(ENTITY, Some(&format!("compare:{BASE}^...{HEAD}")))
        .await
        .unwrap_err();
    assert_eq!(error.status(), Some(404), "{error}");
    assert!(!error.to_string().contains("first commit"), "{error}");
}

//! The fixture repositories' files (the everything-search decisions,
//! 2026-11-20, D16): what the mock answers the stored-trees pass and the
//! files pass, with git's own shas.

use std::collections::HashMap;

use knobas_source::Source;
use knobas_source::tree::{HeldTrees, TreeRead};
use knobas_source_mock::{MockSource, files};

/// A store holding the levels named.
struct Held(HashMap<String, u64>);

impl HeldTrees for Held {
    fn held(&self, sha: &str) -> Option<u64> {
        self.0.get(sha).copied()
    }
    fn estimate(&self) -> Option<u64> {
        None
    }
}

fn nothing() -> Held {
    Held(HashMap::new())
}

/// Every path of a read, from its levels down from the root.
fn paths(root: &str, levels: &[knobas_source::tree::TreeLevel]) -> Vec<String> {
    let by: HashMap<&str, &knobas_source::tree::TreeLevel> =
        levels.iter().map(|l| (l.sha.as_str(), l)).collect();
    let mut out = Vec::new();
    let mut stack = vec![(String::new(), root.to_owned())];
    while let Some((dir, sha)) = stack.pop() {
        for item in &by[sha.as_str()].entries {
            let path = if dir.is_empty() {
                item.name.clone()
            } else {
                format!("{dir}/{}", item.name)
            };
            if item.is_tree() {
                stack.push((path, item.sha.clone()));
            } else {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

#[test]
fn a_repositorys_shas_are_the_ones_git_gives_its_files() {
    // `git add -A && git write-tree` over the fixture's folder, and
    // `git ls-files -s`.
    let gateway = files::of("ledger-gateway").expect("ledger-gateway has files");
    assert_eq!(gateway.root, "5e99e8131462ae89a51259539ccad90689ab3b40");
    let payout = files::of("payout-service").expect("payout-service has files");
    assert_eq!(payout.root, "9206fe8ac7866d159ff615a6b2256c858167ae44");
    let top = &payout.levels[0];
    assert_eq!(top.sha, payout.root, "the root's level comes first");
    let lock = top.entries.iter().find(|e| e.name == "Cargo.lock").unwrap();
    assert_eq!(lock.sha, "12fc2a2ed258e352a28b0b952ed32940f99dd885");
    let names: Vec<&str> = top.entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "Cargo.lock",
            "assets",
            "data",
            "docs",
            "node_modules",
            "src"
        ],
        "git's order"
    );
    assert_eq!(gateway.files.len(), 6);
    assert_ne!(gateway.commit, payout.commit);
    assert!(files::of("ADProxy").is_none(), "a repository with no files");
}

#[tokio::test]
async fn a_tree_is_read_whole_with_what_the_store_holds_left_out() {
    let mock = MockSource::new();
    let gateway = files::of("ledger-gateway").unwrap();
    let Some(TreeRead::Read {
        root,
        levels,
        requests,
    }) = mock
        .read_tree("mock:ledger-gateway", &gateway.commit, &nothing(), 100)
        .await
        .unwrap()
    else {
        panic!("a read");
    };
    assert_eq!((root.as_str(), requests), (gateway.root.as_str(), 1));
    assert_eq!(
        paths(&root, &levels),
        [
            "README.md",
            "deploy/k8s/deployment.yaml",
            "pom.xml",
            "src/main/java/com/tidewater/settlebridge/SettleBridgeServer.java",
            "src/main/java/com/tidewater/settlebridge/config/SettleBridgeConfig.java",
            "src/test/java/com/tidewater/settlebridge/SettleBridgeServerTest.java",
        ]
    );
    // Every level held: only what is not held comes back.
    let all = Held(levels.iter().map(|l| (l.sha.clone(), 1)).collect());
    let Some(TreeRead::Read { levels, .. }) = mock
        .read_tree("mock:ledger-gateway", "any", &all, 100)
        .await
        .unwrap()
    else {
        panic!("a read");
    };
    assert!(levels.is_empty());
    // Past the cap: too large, with its entries (6 files, 14 folders).
    assert_eq!(
        mock.read_tree("mock:ledger-gateway", "any", &nothing(), 5)
            .await
            .unwrap(),
        Some(TreeRead::TooLarge {
            entries: 20,
            requests: 1
        })
    );
    // A repository with no files, and one of another instance: no tree.
    assert!(
        mock.read_tree("mock:ADProxy", "any", &nothing(), 100)
            .await
            .is_err()
    );
    assert!(
        mock.read_tree("other:ledger-gateway", "any", &nothing(), 100)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn a_blob_and_an_archive_hand_over_the_fixtures_bytes() {
    let mock = MockSource::new();
    let path = "src/main/java/com/tidewater/settlebridge/SettleBridgeServer.java";
    let bytes = mock
        .read_blob("mock:ledger-gateway", "any", path, 1 << 20)
        .await
        .unwrap()
        .expect("the file");
    assert!(
        String::from_utf8(bytes.clone())
            .unwrap()
            .contains("public final class SettleBridgeServer {")
    );
    assert_eq!(
        mock.read_blob("mock:ledger-gateway", "any", path, 10)
            .await
            .unwrap(),
        None,
        "past the cap"
    );
    let mut got = Vec::new();
    let requests = mock
        .read_archive(
            "mock:payout-service",
            "any",
            &|p: &str| !p.starts_with("node_modules/"),
            &mut |p, b| got.push((p, b.len())),
        )
        .await
        .unwrap();
    assert_eq!(requests, Some(1));
    let got: Vec<&str> = got.iter().map(|(p, _)| p.as_str()).collect();
    assert_eq!(got.len(), 11, "every file but the one refused: {got:?}");
    assert!(got.contains(&"src/payout/legacy/Charge.java"));
    // A latin-1 file comes as its bytes, not decoded.
    let charge = mock
        .read_blob(
            "mock:payout-service",
            "any",
            "src/payout/legacy/Charge.java",
            1 << 20,
        )
        .await
        .unwrap()
        .unwrap();
    assert!(
        charge.windows(2).any(|w| w == [0xf6, 0xdf]),
        "Größe in latin-1"
    );
}

#[tokio::test]
async fn a_failing_mock_reads_no_file() {
    let mock = MockSource::with_fault(knobas_source::contract::Fault::Unreachable);
    assert!(
        mock.read_blob("mock:ledger-gateway", "any", "README.md", 1 << 20)
            .await
            .is_err()
    );
    assert!(
        mock.read_tree("mock:ledger-gateway", "any", &nothing(), 100)
            .await
            .is_err()
    );
    assert!(
        mock.read_archive(
            "mock:ledger-gateway",
            "any",
            &|_: &str| true,
            &mut |_, _| {}
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn a_default_branch_with_files_stands_at_their_commit() {
    let mut sink = knobas_source::contract::VecSink(Vec::new());
    MockSource::new().sync(None, &mut sink).await.unwrap();
    let sink = sink.0;
    let head = |key: &str| {
        sink.iter()
            .find(|i| i.entity.key == key)
            .unwrap_or_else(|| panic!("{key} emitted"))
            .payload["head_sha"]
            .as_str()
            .map(str::to_owned)
    };
    for repo in ["ledger-gateway", "ledger-api", "payout-service"] {
        assert_eq!(
            head(&format!("{repo}@refs/heads/main")).as_deref(),
            Some(files::of(repo).unwrap().commit.as_str()),
            "{repo}'s main"
        );
    }
    // A branch the fixture records commits on stands at its newest.
    assert_eq!(
        head("payout-service@refs/heads/feature/PAY-231-sepa-retry").as_deref(),
        Some("c90d11")
    );
    // One with neither stands at nothing, as before.
    assert_eq!(
        head("payout-service@refs/heads/fix/PAY-228-partial-refund-drift"),
        None
    );
    assert!(MockSource::new().tree_shape().is_some());
}

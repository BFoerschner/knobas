//! D8 and D9's order within the files: each rule of the read with a file it
//! matches and one it does not.

use knobas_files::{FileIndex, Reads};

use super::{index, put, repos};

const SERVER: &str = "package com.tidewater.settlebridge;\n\
                      \n\
                      import com.tidewater.settlebridge.config.SettleBridgeConfig;\n\
                      \n\
                      // Forwards settlement batches to the ledger.\n\
                      public final class SettleBridgeServer {\n\
                      \x20   private final SettleBridgeConfig config;\n\
                      }\n";

fn nos(index: &FileIndex, typed: &str) -> Vec<u64> {
    index
        .ask(typed, Reads::BOTH, 30)
        .into_iter()
        .map(|hit| hit.no)
        .collect()
}

#[test]
fn a_repository_is_found_by_a_files_name_and_by_its_text_and_counted_by_each() {
    let (_dir, index) = index();
    put(
        &index,
        &[
            (
                1,
                "src/main/java/com/tidewater/settlebridge/SettleBridgeServer.java",
                Some(SERVER),
            ),
            (
                1,
                "src/main/java/com/tidewater/settlebridge/config/SettleBridgeConfig.java",
                Some("public record SettleBridgeConfig() {}"),
            ),
            (
                1,
                "deploy/k8s/deployment.yaml",
                Some("image: settle-bridge:4.2.0"),
            ),
            (1, "README.md", Some("The ledger gateway.")),
            (
                2,
                "src/LedgerApi.java",
                Some("import com.tidewater.settlebridge.client.Client;"),
            ),
        ],
    );
    let found = index.ask("settlebridge", Reads::BOTH, 30);
    assert_eq!(found.len(), 2);
    let first = &found[0];
    assert_eq!((first.no, first.by_text, first.by_name), (1, 3, 2));
    // A name match before a text match, the shortest path first.
    assert_eq!(
        first.best.path,
        "src/main/java/com/tidewater/settlebridge/SettleBridgeServer.java"
    );
    assert!(first.best.in_name);
    // The definition, not the package or the import.
    let line = first.best.line.as_ref().expect("a line");
    assert_eq!(
        (line.number, line.text.as_str()),
        (6, "public final class SettleBridgeServer {")
    );
    assert_eq!(line.before, "// Forwards settlement batches to the ledger.");
    // Its text alone: the import is its only line.
    let second = &found[1];
    assert_eq!(
        (
            second.no,
            second.by_text,
            second.by_name,
            second.best.in_name
        ),
        (2, 1, 0, false)
    );
    assert_eq!(
        second.best.line.as_ref().unwrap().text,
        "import com.tidewater.settlebridge.client.Client;"
    );
}

#[test]
fn a_file_matched_by_its_name_alone_has_no_line() {
    let (_dir, index) = index();
    put(
        &index,
        &[
            (1, "charts/ad-proxy/values.yaml", Some("replicas: 2")),
            (2, "x/AdProxy.bin", None),
        ],
    );
    let found = index.ask("adproxy", Reads::BOTH, 30);
    assert_eq!(found.len(), 2);
    assert!(
        found
            .iter()
            .all(|hit| hit.best.in_name && hit.best.line.is_none())
    );
}

#[test]
fn four_letters_are_a_words_start_and_three_are_whole() {
    let (_dir, index) = index();
    put(
        &index,
        &[
            (1, "a.java", Some("class JwtFilter {}")),
            (2, "b.java", Some("String parseJwt() {}")),
            (3, "c.java", Some("class ADProxyServer {}")),
        ],
    );
    assert_eq!(nos(&index, "jwt"), [2], "three letters: the word jwt alone");
    assert_eq!(nos(&index, "jwtf"), [1]);
    assert_eq!(nos(&index, "proxy"), [3], "a part's start");
    assert!(nos(&index, "dprox").is_empty(), "not inside a part");
    assert!(nos(&index, "jw").is_empty(), "two letters ask nothing");
}

#[test]
fn every_term_is_required_and_in_one_field() {
    let (_dir, index) = index();
    put(
        &index,
        &[
            (1, "ledger/Client.java", Some("retry queue")),
            (2, "Retry.java", Some("ledger client")),
            (3, "x.java", Some("ledger client retry")),
        ],
    );
    // Repository 1 holds `ledger` in its path and `retry` in its text: no match.
    assert_eq!(nos(&index, "ledger retry"), [3]);
    assert_eq!(nos(&index, "ledger client"), [1, 3, 2]);
}

#[test]
fn a_phrase_must_stand_side_by_side_in_a_shown_line() {
    let (_dir, index) = index();
    put(
        &index,
        &[
            (1, "a.log", Some("the connection refused us")),
            (2, "b.log", Some("connection was refused")),
            (3, "c/a.txt", Some("nothing")),
        ],
    );
    assert_eq!(nos(&index, "\"connection refused\""), [1]);
    assert_eq!(nos(&index, "connection refused"), [2, 1]);
}

#[test]
fn or_asks_nothing_and_a_negated_word_is_dropped() {
    let (_dir, index) = index();
    put(
        &index,
        &[
            (1, "payout.java", Some("payout retry")),
            (2, "x", Some("payout")),
        ],
    );
    assert!(nos(&index, "payout or retry").is_empty());
    assert_eq!(nos(&index, "payout -retry"), [1, 2]);
}

#[test]
fn past_the_bound_a_word_is_read_whole() {
    let (_dir, index) = index();
    let words = [
        "conf", "confa", "confb", "confc", "confd", "confe", "conff", "confg", "confh",
    ];
    let files: Vec<(u64, String, String)> = words
        .iter()
        .enumerate()
        .map(|(n, word)| {
            (
                1 + u64::try_from(n).unwrap() % 9,
                format!("f{n}.txt"),
                format!("x {word}"),
            )
        })
        .collect();
    let mut writer = index.writer().unwrap();
    for (no, path, text) in &files {
        writer.put(*no, path, Some(text)).unwrap();
    }
    writer.commit().unwrap();
    // Nine word lists begin `conf`: within 4,096 every one is read.
    assert_eq!(
        index
            .ask(
                "conf",
                Reads {
                    names: false,
                    text: true
                },
                30
            )
            .len(),
        9
    );
    // Within 9 still, past 8 the word is read whole.
    let bounded = index.clone().with_bound(9);
    assert_eq!(
        bounded
            .ask(
                "conf",
                Reads {
                    names: false,
                    text: true
                },
                30
            )
            .len(),
        9
    );
    let bounded = index.clone().with_bound(8);
    let whole: Vec<u64> = bounded
        .ask(
            "conf",
            Reads {
                names: false,
                text: true,
            },
            30,
        )
        .into_iter()
        .map(|hit| hit.no)
        .collect();
    assert_eq!(whole, [1]);
}

#[test]
fn reads_say_which_field_is_asked() {
    let (_dir, index) = index();
    put(
        &index,
        &[
            (1, "settle-bridge.yaml", Some("x")),
            (2, "y.java", Some("SettleBridge")),
        ],
    );
    let names = Reads {
        names: true,
        text: false,
    };
    let text = Reads {
        names: false,
        text: true,
    };
    let none = Reads {
        names: false,
        text: false,
    };
    assert_eq!(
        index
            .ask("settlebridge", names, 30)
            .iter()
            .map(|h| h.no)
            .collect::<Vec<_>>(),
        [1]
    );
    assert_eq!(
        index
            .ask("settlebridge", text, 30)
            .iter()
            .map(|h| h.no)
            .collect::<Vec<_>>(),
        [2]
    );
    assert!(index.ask("settlebridge", none, 30).is_empty());
    // Text not read: a file found by its name shows no line.
    put(
        &index,
        &[(3, "SettleBridge.java", Some("class SettleBridge {}"))],
    );
    let shown = index.ask("settlebridge", names, 30);
    assert!(shown.iter().all(|hit| hit.best.line.is_none()));
}

#[test]
fn named_repositories_come_first_then_more_files_then_a_favourite_then_the_newer() {
    let (_dir, index) = index();
    put(
        &index,
        &[
            // 1: named by two files.
            (1, "retryqueue/A.java", Some("x")),
            (1, "retryqueue/B.java", Some("x")),
            // 2: named by one file, its text by three.
            (2, "RetryQueue.java", Some("RetryQueue")),
            (2, "a.java", Some("RetryQueue")),
            (2, "b.java", Some("RetryQueue")),
            // 3 and 4: two files' text each; 4 is newer.
            (3, "a.java", Some("RetryQueue")),
            (3, "b.java", Some("RetryQueue")),
            (4, "a.java", Some("RetryQueue")),
            (4, "b.java", Some("RetryQueue")),
            // 5: five files' text.
            (5, "a", Some("retryQueue")),
            (5, "b", Some("retryQueue")),
            (5, "c", Some("retryQueue")),
            (5, "d", Some("retryQueue")),
            (5, "e", Some("retryQueue")),
        ],
    );
    assert_eq!(nos(&index, "retryqueue"), [1, 2, 5, 4, 3]);
    index.set_repos(repos(&[3]));
    assert_eq!(
        nos(&index, "retryqueue"),
        [1, 2, 5, 3, 4],
        "the favourite before the newer"
    );
    assert_eq!(
        index.ask("retryqueue", Reads::BOTH, 2).len(),
        2,
        "cut to most"
    );
}

#[test]
fn a_repository_it_knows_nothing_of_is_left_out() {
    let (_dir, index) = index();
    put(
        &index,
        &[
            (42, "Payout.java", Some("payout")),
            (1, "x", Some("payout")),
        ],
    );
    assert_eq!(nos(&index, "payout"), [1]);
}

#[test]
fn in_repo_lists_name_matches_first_then_the_shortest_path_and_counts_them_all() {
    let (_dir, index) = index();
    put(
        &index,
        &[
            (
                1,
                "src/main/java/com/tidewater/settlebridge/SettleBridgeServer.java",
                Some(SERVER),
            ),
            (
                1,
                "deploy/settle-bridge.yaml",
                Some("image: settle-bridge:4.2.0"),
            ),
            (1, "docs/notes.md", Some("SettleBridge is the old name.")),
            (1, "a.md", Some("settlebridge")),
            (2, "x.md", Some("settlebridge")),
        ],
    );
    let (files, total) = index.in_repo(1, "settlebridge", Reads::BOTH, 3);
    assert_eq!(total, 4);
    let paths: Vec<(&str, bool)> = files.iter().map(|f| (f.path.as_str(), f.in_name)).collect();
    assert_eq!(
        paths,
        [
            ("deploy/settle-bridge.yaml", true),
            (
                "src/main/java/com/tidewater/settlebridge/SettleBridgeServer.java",
                true
            ),
            ("a.md", false),
        ]
    );
    assert_eq!(
        files[0].line.as_ref().unwrap().text,
        "image: settle-bridge:4.2.0"
    );
    assert_eq!(index.in_repo(1, "nothing", Reads::BOTH, 8), (Vec::new(), 0));
}

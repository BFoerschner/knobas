//! D6: put, replace, remove, remove a repository, commit and reopen; a
//! folder holding garbage opens empty; one writer at a time.

use knobas_files::{FileIndex, FilesError, Reads};

use super::{index, put, repos};

fn found(index: &FileIndex, typed: &str) -> Vec<(u64, u32, u32, String)> {
    index
        .ask(typed, Reads::BOTH, 30)
        .into_iter()
        .map(|hit| (hit.no, hit.by_text, hit.by_name, hit.best.path))
        .collect()
}

#[test]
fn a_file_written_is_found_and_a_replaced_one_once() {
    let (_dir, index) = index();
    assert!(!index.holds_files());
    put(
        &index,
        &[(1, "src/Gateway.java", Some("class SettleBridgeServer {}"))],
    );
    assert!(index.holds_files());
    assert_eq!(
        found(&index, "settlebridge"),
        [(1, 1, 0, "src/Gateway.java".to_owned())]
    );
    put(
        &index,
        &[(
            1,
            "src/Gateway.java",
            Some("class SettleBridgeServer { int retry; }"),
        )],
    );
    assert_eq!(
        found(&index, "settlebridge"),
        [(1, 1, 0, "src/Gateway.java".to_owned())]
    );
    assert_eq!(index.held().unwrap()[&1], 1);
    assert_eq!(
        found(&index, "retry"),
        [(1, 1, 0, "src/Gateway.java".to_owned())]
    );
}

#[test]
fn a_removed_file_and_a_removed_repository_are_found_no_more() {
    let (_dir, index) = index();
    put(
        &index,
        &[
            (1, "a/Ledger.java", Some("ledger")),
            (1, "b/Ledger.java", Some("ledger")),
            (2, "Ledger.kt", Some("ledger")),
        ],
    );
    assert_eq!(
        index.held().unwrap(),
        [(1, 2), (2, 1)].into_iter().collect()
    );
    let mut writer = index.writer().unwrap();
    writer.remove(1, "a/Ledger.java");
    writer.commit().unwrap();
    assert_eq!(
        found(&index, "ledger")[1],
        (1, 1, 1, "b/Ledger.java".to_owned())
    );
    writer.remove_repo(2);
    writer.commit().unwrap();
    assert_eq!(
        found(&index, "ledger"),
        [(1, 1, 1, "b/Ledger.java".to_owned())]
    );
    assert_eq!(index.held().unwrap(), [(1, 1)].into_iter().collect());
}

#[test]
fn what_is_written_is_read_only_once_committed() {
    let (_dir, index) = index();
    let mut writer = index.writer().unwrap();
    writer.put(1, "Payout.java", Some("payout")).unwrap();
    assert!(found(&index, "payout").is_empty());
    writer.commit().unwrap();
    assert_eq!(found(&index, "payout").len(), 1);
}

#[test]
fn a_committed_index_is_read_again_after_a_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("files-1");
    {
        let index = FileIndex::open(&folder).unwrap();
        index.set_repos(repos(&[]));
        put(&index, &[(3, "deploy/settle-bridge.yaml", None)]);
        // The readers are dropped before the folder is opened again: on
        // Windows a mapped segment cannot go while one holds it.
    }
    let index = FileIndex::open(&folder).unwrap();
    index.set_repos(repos(&[]));
    assert_eq!(
        found(&index, "settlebridge"),
        [(3, 0, 1, "deploy/settle-bridge.yaml".to_owned())]
    );
}

#[test]
fn a_folder_holding_garbage_opens_empty() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("files-1");
    std::fs::create_dir_all(folder.join("junk")).unwrap();
    std::fs::write(folder.join("meta.json"), b"{ not tantivy").unwrap();
    std::fs::write(folder.join("x.idx"), b"\0\0\0").unwrap();
    let index = FileIndex::open(&folder).unwrap();
    assert!(index.held().unwrap().is_empty());
    index.set_repos(repos(&[]));
    put(&index, &[(1, "Payout.java", Some("payout"))]);
    assert_eq!(found(&index, "payout").len(), 1);
}

#[test]
fn two_writers_refuse() {
    let (_dir, index) = index();
    let first = index.writer().unwrap();
    assert!(matches!(index.writer(), Err(FilesError::Busy)));
    drop(first);
    assert!(index.writer().is_ok());
}

#[test]
fn an_index_of_none_holds_and_answers_nothing() {
    let none = FileIndex::none();
    assert!(!none.holds_files());
    assert!(none.ask("payout", Reads::BOTH, 30).is_empty());
    assert_eq!(none.in_repo(1, "payout", Reads::BOTH, 8), (Vec::new(), 0));
    assert!(none.held().unwrap().is_empty());
    assert!(none.writer().is_err());
}

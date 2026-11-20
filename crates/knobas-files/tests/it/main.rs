//! The files index (the everything-search decisions, D6 and D8): its writes
//! and its reads, over a temporary folder.

mod ask;
mod read_alone;
mod writes;

use knobas_files::{FileIndex, Repo, Repos};

/// An index in a folder of its own, knowing repositories 1 to 9.
pub fn index() -> (tempfile::TempDir, FileIndex) {
    let dir = tempfile::tempdir().expect("a scratch folder");
    let index = FileIndex::open(&dir.path().join("files-1")).expect("an index");
    index.set_repos(repos(&[]));
    (dir, index)
}

/// Repositories 1 to 9, `favourites` among them, the higher number the newer.
pub fn repos(favourites: &[u64]) -> Repos {
    (1..=9)
        .map(|no| {
            (
                no,
                Repo {
                    entity_id: format!("gitea:acme/r{no}"),
                    source_id: "gitea".to_owned(),
                    favourite: favourites.contains(&no),
                    updated: i64::try_from(no).unwrap(),
                },
            )
        })
        .collect()
}

/// Write `files` (repository, path, text) and commit.
pub fn put(index: &FileIndex, files: &[(u64, &str, Option<&str>)]) {
    let mut writer = index.writer().expect("the writer");
    for (no, path, text) in files {
        writer.put(*no, path, *text).unwrap();
    }
    writer.commit().unwrap();
}

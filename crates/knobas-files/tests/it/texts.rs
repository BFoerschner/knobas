//! Every file of one repository with the text the index keeps
//! (`FileIndex::each_text`): what *Text search* scans where no clone holds
//! the default branch (ADR-0062).

use super::{index, put};

fn texts(index: &knobas_files::FileIndex, no: u64) -> Vec<(String, String)> {
    let mut seen = Vec::new();
    index.each_text(no, &mut |path, text| {
        seen.push((path.to_owned(), text.to_owned()));
        true
    });
    seen.sort();
    seen
}

#[test]
fn each_file_of_one_repository_is_given_with_its_text_and_no_other_repositorys() {
    let (_dir, index) = index();
    put(
        &index,
        &[
            (1, "src/a.rs", Some("fn a() {}\n")),
            (1, "src/b.rs", Some("fn b() {}\n")),
            (1, "logo.png", None),
            (2, "src/a.rs", Some("other\n")),
        ],
    );
    assert_eq!(
        texts(&index, 1),
        vec![
            ("src/a.rs".to_owned(), "fn a() {}\n".to_owned()),
            ("src/b.rs".to_owned(), "fn b() {}\n".to_owned()),
        ]
    );
    // A removed file is given no more.
    let mut writer = index.writer().unwrap();
    writer.remove(1, "src/a.rs");
    writer.commit().unwrap();
    assert_eq!(texts(&index, 1).len(), 1);
    assert!(texts(&index, 3).is_empty());
}

#[test]
fn the_walk_stops_when_told() {
    let (_dir, index) = index();
    put(
        &index,
        &[
            (1, "a", Some("x")),
            (1, "b", Some("x")),
            (1, "c", Some("x")),
        ],
    );
    let mut given = 0;
    index.each_text(1, &mut |_, _| {
        given += 1;
        given < 2
    });
    assert_eq!(given, 2);
}

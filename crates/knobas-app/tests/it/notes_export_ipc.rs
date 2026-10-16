//! *Export notes as Markdown* at the IPC seam (#832, D10): notes written
//! through the note commands, exported into a temporary folder, read back as
//! files.

use knobas_app::commands::entity::{create_note_inner, delete_note_inner};
use knobas_app::notes_export::export;

async fn pool() -> sqlx::PgPool {
    knobas_db::test_util::scratch_database("notes-export")
        .await
        .pool(4)
        .await
        .expect("a pool onto the scratch db")
}

/// One `.md` per note, its title a heading and its body as typed, `[[refs]]`
/// kept as text whether they resolve or not.
#[tokio::test]
async fn every_note_is_one_markdown_file_with_its_refs_kept_as_text() {
    let pool = pool().await;
    create_note_inner(
        &pool,
        Some("SEPA retry: questions"),
        Some("Ask about [[mock:PAY-231]] and [[nowhere:404]].\n\n- one\n- two\n"),
        &[],
    )
    .await
    .expect("a note");
    create_note_inner(&pool, None, Some("first"), &[])
        .await
        .expect("an untitled note");
    create_note_inner(&pool, None, None, &[])
        .await
        .expect("a second untitled note");
    let gone = create_note_inner(&pool, Some("Deleted"), Some("x"), &[])
        .await
        .expect("a note to delete");
    delete_note_inner(&pool, &gone.note.id)
        .await
        .expect("delete");

    let folder = tempfile::tempdir().expect("a temp dir");
    let target = folder.path().join("knobas-notes");
    let written = export(&pool, &target).await.expect("the export");

    assert_eq!(written.count, 3, "a deleted note is not exported");
    assert_eq!(written.folder, target.display().to_string());

    let mut names: Vec<String> = std::fs::read_dir(&target)
        .expect("the folder")
        .map(|entry| {
            entry
                .expect("an entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    assert_eq!(
        names,
        [
            "sepa-retry-questions.md",
            "untitled-note-2.md",
            "untitled-note.md"
        ]
    );

    assert_eq!(
        std::fs::read_to_string(target.join("sepa-retry-questions.md")).expect("the file"),
        "# SEPA retry: questions\n\nAsk about [[mock:PAY-231]] and [[nowhere:404]].\n\n- one\n- two\n"
    );
    assert_eq!(
        std::fs::read_to_string(target.join("untitled-note.md")).expect("the file"),
        "# Untitled note\n\nfirst\n"
    );
    assert_eq!(
        std::fs::read_to_string(target.join("untitled-note-2.md")).expect("the file"),
        "# Untitled note\n",
        "an empty body is the heading alone"
    );
}

/// No notes is a folder with nothing in it, not a failure.
#[tokio::test]
async fn no_notes_writes_an_empty_folder() {
    let pool = pool().await;
    let folder = tempfile::tempdir().expect("a temp dir");
    let target = folder.path().join("knobas-notes");
    let written = export(&pool, &target).await.expect("the export");
    assert_eq!(written.count, 0);
    assert!(target.is_dir());
}

//! *Export notes as Markdown* (#832, D10, R-13a.6).
//!
//! Notes and quick capture hide behind a setting for one release (D10), and
//! before anything about them is ever deleted the reader can take them out:
//! one `.md` file per note in a folder, written through the one block-to-
//! Markdown converter (`knobas_source::docs::markdown`, R-10.17, #952).
//!
//! A note is a page whose body is already Markdown: its title as the
//! heading, then the body passed through. So the body is written as the reader
//! typed it, and a `[[ref]]` stays the text it was. Files are named by
//! [`page_file_name`], the name a page link points at, with `-2`, `-3` after
//! a name an earlier note already took (two notes are often both *Untitled
//! note*). A file of that name from an earlier export is replaced.
//!
//! *Move to Confluence…* (#941) is the other way out: [`note_pages`] is the
//! page each note becomes. Neither deletes a note.

use std::collections::HashSet;
use std::path::Path;

use knobas_core::note::NoteRow;
use knobas_source::docs::markdown::{page_file_name, page_from_markdown};
use serde::Serialize;
use sqlx::PgPool;

use crate::IpcError;

/// The folder under Downloads an export writes into, as prototype-6 names it.
pub const FOLDER: &str = "knobas-notes";

/// What an export wrote.
///
/// *Export notes as Markdown* (#832).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct NotesExport {
    /// How many notes were written, one file each.
    pub count: u64,
    /// The folder they are in, absolute.
    pub folder: String,
}

/// The Markdown one note exports as.
#[must_use]
pub fn note_markdown(note: &NoteRow) -> String {
    page_from_markdown(&note.title, &note.body_md)
}

/// Write every note into `folder`, creating it if it is not there.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) if the read fails or a file
/// cannot be written.
pub async fn export(pool: &PgPool, folder: &Path) -> Result<NotesExport, IpcError> {
    let notes = knobas_core::note::all(pool).await?;
    std::fs::create_dir_all(folder).map_err(IpcError::internal)?;
    let mut taken = HashSet::new();
    for note in &notes {
        let name = unique_name(&page_file_name(&note.title), &mut taken);
        std::fs::write(folder.join(name), note_markdown(note)).map_err(IpcError::internal)?;
    }
    Ok(NotesExport {
        count: notes.len() as u64,
        folder: folder.display().to_string(),
    })
}

/// *Move to Confluence…* (#941, D10): the page each note becomes under
/// `parent`, in the notes' order. The body is the note's Markdown as typed,
/// which the source's `body_from_text` reads (`##` a heading, `-` a list
/// item); a `[[ref]]` stays text. A wiki holds one page of a title per space,
/// so a title an earlier note took, in any case, gets ` (2)`, ` (3)`.
#[must_use]
pub fn note_pages(notes: &[NoteRow], parent: &str) -> Vec<crate::docs::NewPage> {
    let mut taken = HashSet::new();
    notes
        .iter()
        .map(|note| {
            let title = note.title.trim();
            let mut candidate = title.to_owned();
            let mut n = 1;
            while !taken.insert(candidate.to_lowercase()) {
                n += 1;
                candidate = format!("{title} ({n})");
            }
            crate::docs::NewPage {
                parent: parent.to_owned(),
                title: candidate,
                text: note.body_md.clone(),
            }
        })
        .collect()
}

/// `name`, or `name` with `-2`, `-3` before `.md` when it is taken.
fn unique_name(name: &str, taken: &mut HashSet<String>) -> String {
    let stem = name.strip_suffix(".md").unwrap_or(name);
    let mut candidate = name.to_owned();
    let mut n = 1;
    while !taken.insert(candidate.to_lowercase()) {
        n += 1;
        candidate = format!("{stem}-{n}.md");
    }
    candidate
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(title: &str, body_md: &str) -> NoteRow {
        NoteRow {
            id: "note:1".to_owned(),
            title: title.to_owned(),
            body_md: body_md.to_owned(),
            created_at: chrono::DateTime::UNIX_EPOCH,
            updated_at: chrono::DateTime::UNIX_EPOCH,
        }
    }

    /// Captured before #952 made the converter read the Docs frame's blocks:
    /// the title escaped onto one line, the body as typed, trailing space cut.
    #[test]
    fn a_note_exports_as_before() {
        assert_eq!(
            note_markdown(&note(
                "SEPA *retry*:\n[v2]",
                "Ask about [[mock:PAY-231]]\n\n- one\n\n\n"
            )),
            "# SEPA \\*retry\\*: \\[v2\\]\n\nAsk about [[mock:PAY-231]]\n\n- one\n"
        );
        assert_eq!(
            note_markdown(&note("Untitled note", " \n\t")),
            "# Untitled note\n"
        );
        assert_eq!(
            note_markdown(&note("# Hash", "  indented")),
            "# # Hash\n\n  indented\n"
        );
    }

    #[test]
    fn a_taken_name_gets_a_number() {
        let mut taken = HashSet::new();
        assert_eq!(
            unique_name("untitled-note.md", &mut taken),
            "untitled-note.md"
        );
        assert_eq!(
            unique_name("untitled-note.md", &mut taken),
            "untitled-note-2.md"
        );
        assert_eq!(
            unique_name("untitled-note.md", &mut taken),
            "untitled-note-3.md"
        );
    }
}

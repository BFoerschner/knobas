/**
 * The commands Settings › Notes and capture reads and writes (#832),
 * injectable so a component test needs no Tauri bridge. `capture_shortcut`
 * already exists (and answers the note count since #832); `set_notes_shown`
 * and `export_notes_markdown` are this ticket's.
 */
import {
  captureShortcut,
  exportNotesMarkdown,
  setNotesShown,
  type CaptureShortcut,
  type NotesExport,
} from "../../ipc/entity";

export interface NotesPorts {
  captureShortcut: () => Promise<CaptureShortcut>;
  setNotesShown: (shown: boolean) => Promise<CaptureShortcut>;
  exportNotes: () => Promise<NotesExport>;
}

export const realNotesPorts: NotesPorts = {
  captureShortcut: () => captureShortcut(),
  setNotesShown: (shown) => setNotesShown(shown),
  exportNotes: () => exportNotesMarkdown(),
};

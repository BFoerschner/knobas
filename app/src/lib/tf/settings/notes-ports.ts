/**
 * The commands Settings › Notes and capture reads and writes (#832),
 * injectable so a component test needs no Tauri bridge. `capture_shortcut`
 * already exists (and answers the note count since #832); `set_notes_shown`
 * and `export_notes_markdown` are #832's, `move_notes_to_confluence` #941's.
 * *Move to Confluence…* picks its page with the standup's picker, over the
 * sources, their adapters and the launcher's search.
 */
import {
  captureShortcut,
  exportNotesMarkdown,
  moveNotesToConfluence,
  setNotesShown,
  type CaptureShortcut,
  type NotesExport,
} from "../../ipc/entity";
import { search } from "../../ipc/search";
import { listAdapters, listSources, type QueuedWrite, type SourceDescriptor, type SourceSummary } from "../../ipc/sources";

export interface NotesPorts {
  captureShortcut: () => Promise<CaptureShortcut>;
  setNotesShown: (shown: boolean) => Promise<CaptureShortcut>;
  exportNotes: () => Promise<NotesExport>;
  moveNotes: (parent: string) => Promise<QueuedWrite[]>;
  listSources: () => Promise<SourceSummary[]>;
  listAdapters: () => Promise<SourceDescriptor[]>;
  search: typeof search;
}

export const realNotesPorts: NotesPorts = {
  captureShortcut: () => captureShortcut(),
  setNotesShown: (shown) => setNotesShown(shown),
  exportNotes: () => exportNotesMarkdown(),
  moveNotes: (parent) => moveNotesToConfluence(parent),
  listSources: () => listSources(),
  listAdapters: () => listAdapters(),
  search: (query) => search(query),
};

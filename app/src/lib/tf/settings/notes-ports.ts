/**
 * The commands Settings › Notes and capture reads and writes (#832),
 * injectable so a component test needs no Tauri bridge. `capture_shortcut`
 * already exists (and answers the note count since #832), and so does
 * `set_capture_shortcut` (#503, called from here since #1086); `set_notes_shown`
 * and `export_notes_markdown` are #832's, `move_notes_to_confluence` #941's.
 * *Move to Confluence…* picks its page with the standup's picker, over the
 * sources, their adapters and the launcher's search.
 */
import {
  captureShortcut,
  exportNotesMarkdown,
  moveNotesToConfluence,
  setCaptureShortcut,
  setNotesShown,
  type CaptureShortcut,
  type NotesExport,
} from "../../ipc/entity";
import { search } from "../../ipc/search";
import { listAdapters, listSources, type QueuedWrite, type SourceDescriptor, type SourceSummary } from "../../ipc/sources";
import type { RefusedWritesPort } from "../../shell/refused-writes";

export interface NotesPorts {
  captureShortcut: () => Promise<CaptureShortcut>;
  setNotesShown: (shown: boolean) => Promise<CaptureShortcut>;
  /** Set the shortcut, or clear it with `null` (#503, #1086). */
  setCaptureShortcut: (accelerator: string | null) => Promise<CaptureShortcut>;
  exportNotes: () => Promise<NotesExport>;
  moveNotes: (parent: string) => Promise<QueuedWrite[]>;
  listSources: () => Promise<SourceSummary[]>;
  listAdapters: () => Promise<SourceDescriptor[]>;
  search: typeof search;
  /** Which picked pages the source's credential cannot add pages under (#2053); the IPC read when omitted. */
  refusedWrites?: RefusedWritesPort;
}

export const realNotesPorts: NotesPorts = {
  captureShortcut: () => captureShortcut(),
  setNotesShown: (shown) => setNotesShown(shown),
  setCaptureShortcut: (accelerator) => setCaptureShortcut(accelerator),
  exportNotes: () => exportNotesMarkdown(),
  moveNotes: (parent) => moveNotesToConfluence(parent),
  listSources: () => listSources(),
  listAdapters: () => listAdapters(),
  search: (query) => search(query),
};

/**
 * `?fake-ipc`: The capture window (#503): its shortcut and the context it attaches.
 * `seed` reads the stored shortcut from the query.
 */
import { keyCaps } from "../../../tf/settings/notes";
import type { Handlers } from "../fake-tauri";
import { onFakeReset, seedOf } from "../fake-seed";

/**
 * The pair the capture window reads its two born links from (#503).
 *
 * A **stored** room and an open ticket, so the walk exercises the case with
 * both links rather than the degenerate one: the SEPA context this fixture's
 * `list_contexts` answers with, and the ticket its rooms open on.
 */
const FAKE_CAPTURE_CONTEXT = { context: "ctx:sepa", foreground: "mock:PAY-231" };

/**
 * `?fake-shortcut=<accelerator>` seeds a stored shortcut, and
 * `?fake-shortcut-refused` makes it read as one the operating system would not
 * hand over -- the branch a browser can otherwise never reach, since no browser
 * can ask a window server for a key.
 */
function seedCaptureShortcut(params: URLSearchParams): void {
  const stored = params.get("fake-shortcut");
  if (stored !== null) fakeShortcutStored = stored.trim() === "" ? null : stored.trim();
  fakeShortcutRefused = params.has("fake-shortcut-refused");
  fakeNotesShown = params.has("fake-notes-shown");
}

/**
 * The capture shortcut this session has stored, and why it is not holding.
 *
 * Seeded by {@link seedCaptureShortcut}.
 */
let fakeShortcutStored: string | null = null;
let fakeShortcutRefused = false;
/** *Show notes and quick capture* (#832): off, unless `?fake-notes-shown`. */
let fakeNotesShown = false;

/** Whether the fake has *Show notes and quick capture* on; `launcher_home` answers it too (#1036). */
export function fakeNotesAreShown(): boolean {
  return fakeNotesShown;
}

/**
 * What the registrar would say: a string its parser refuses (the same grammar
 * `keyCaps` draws, #1467), or `?fake-shortcut-refused`'s taken combination.
 */
function fakeRefusal(): string | null {
  if (fakeShortcutStored === null || !fakeNotesShown) return null;
  if (keyCaps(fakeShortcutStored).length === 0) return `Couldn't recognize "${fakeShortcutStored}" as a valid hotkey`;
  return fakeShortcutRefused ? `${fakeShortcutStored} is registered by another application` : null;
}

function fakeShortcut() {
  return {
    accelerator: fakeShortcutStored,
    refusal: fakeRefusal(),
    shown: fakeNotesShown,
    // Prototype-6's count (#832): 37 notes, the last from 12 August.
    note_count: 37,
    last_note_at: "2026-08-12T09:00:00Z",
  };
}

function fakeSetShortcut(accelerator: unknown) {
  const typed = typeof accelerator === "string" ? accelerator.trim() : "";
  fakeShortcutStored = typed === "" ? null : typed;
  return fakeShortcut();
}

/** This feature's commands. */
export function handlers(): Handlers {
  return {
    // The capture window (#503). Its own document (`capture.html?fake-ipc`),
    // so these five are what that page's walk answers from: the shortcut the
    // settings section draws, the pair the capture reads its links from, and
    // the button that would bring the note into the main window.
    capture_shortcut: () => fakeShortcut(),
    set_capture_shortcut: (args) => fakeSetShortcut(args["accelerator"]),
    record_capture_context: () => null,
    capture_context: () => FAKE_CAPTURE_CONTEXT,
    reveal_note: () => null,
    // Settings › Notes and capture (#832): the setting, and the export, which
    // answers the 37 notes `capture_shortcut` counts and writes nothing.
    set_notes_shown: (args) => {
      fakeNotesShown = args["shown"] === true;
      return fakeShortcut();
    },
    export_notes_markdown: () => ({ count: 37, folder: "/Users/mara/Downloads/knobas-notes" }),
  };
}

/** What `?fake-ipc`'s query seeds before the bridge is installed. */
export function seed(params: URLSearchParams): void {
  seedCaptureShortcut(params);
}

const atLoad = seedOf({ fakeShortcutStored, fakeShortcutRefused, fakeNotesShown });
onFakeReset(() => {
  ({ fakeShortcutStored, fakeShortcutRefused, fakeNotesShown } = atLoad());
});

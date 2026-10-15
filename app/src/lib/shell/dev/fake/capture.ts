/**
 * `?fake-ipc`: The capture window (#503): its shortcut and the context it attaches.
 * `seed` reads the stored shortcut from the query.
 */
import type { Handler } from "../fake-tauri";

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
}

/**
 * The capture shortcut this session has stored, and why it is not holding.
 *
 * Seeded by {@link seedCaptureShortcut}.
 */
let fakeShortcutStored: string | null = null;
let fakeShortcutRefused = false;

function fakeShortcut() {
  return {
    accelerator: fakeShortcutStored,
    refusal:
      fakeShortcutStored !== null && fakeShortcutRefused
        ? `${fakeShortcutStored} is registered by another application`
        : null,
  };
}

function fakeSetShortcut(accelerator: unknown) {
  const typed = typeof accelerator === "string" ? accelerator.trim() : "";
  fakeShortcutStored = typed === "" ? null : typed;
  return fakeShortcut();
}

/** This feature's commands. */
export function handlers(): Record<string, Handler> {
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
  };
}

/** What `?fake-ipc`'s query seeds before the bridge is installed. */
export function seed(params: URLSearchParams): void {
  seedCaptureShortcut(params);
}

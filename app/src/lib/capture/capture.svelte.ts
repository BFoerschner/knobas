/**
 * What the capture window remembers, and what each of its three exits does
 * (issue #503, spec #491 stories 40–43).
 *
 * `CONTEXT.md`, **Capture**: a note *"created on the first keystroke and never
 * on an empty one"*, carrying two ordinary links — `captured-in` to the
 * context of the last **stored** room the reader stood in, and `captured-from`
 * to the foreground. Both ids come from the backend
 * ({@link captureContext}), which is where the main window put them; nothing
 * here works either of them out, because the in-app *New note* and this window
 * are the same command's two callers and a second opinion about what the reader
 * was looking at is the divergence the deputy's ruling of 2026-09-08 on #502
 * refused to create.
 *
 * # The three exits, and why they are two behaviours
 *
 * Escape and ⌘Enter both **close with the note saved**, and this module does
 * not distinguish them — the ticket asks for one behaviour under two keys, and
 * a capture window is not a dialog with a discard: what was typed was typed.
 * The third is *Open in knobas*, which saves, brings the note into the main
 * window and closes this one.
 *
 * An **empty** close is the exception, and it is the whole reason
 * {@link Capture.noteId} is nullable: no keystroke means no note was ever
 * created, so there is nothing to save and nothing left behind.
 *
 * # Saved on close, not on a timer
 *
 * The row is written on the first keystroke and updated once, on the way out.
 * That is not `NoteView`'s 700 ms autosave, and the difference is deliberate: a
 * capture window lives for seconds and is closed by its own two keys, so a
 * debounce would buy one direction only — a power cut mid-sentence — at the
 * price of a second writer racing the close. What the first keystroke buys is
 * the direction that matters and is story 2's: the row exists before the
 * sentence does, so a window that goes away does not take the thought with it.
 */
import { bornWith } from "../detail/relations";
import { ipcErrorMessage } from "../ipc";
import {
  captureContext as realCaptureContext,
  createNote as realCreateNote,
  revealNote as realRevealNote,
  saveNote as realSaveNote,
  type CaptureContext,
  type NoteDetail,
} from "../ipc/entity";

/**
 * Shut the window this code is running in.
 *
 * The real `close` port, and it is here rather than defaulted to a no-op in
 * {@link createCapture} because a no-op is the wrong failure: a capture whose
 * port was mis-wired would take the keystroke, write the note and then leave an
 * always-on-top window with no way out. A default that really closes is a
 * default that can only be wrong in a test, where the test supplies its own.
 *
 * `getCurrentWindow` needs the injected Tauri internals, so it is imported
 * where it is called: a module-level import would be evaluated by every vitest
 * file that reaches this one.
 */
async function closeThisWindow(): Promise<void> {
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  await getCurrentWindow().close();
}

/** Everything this window does that leaves it, injectable so a test needs no Tauri. */
export interface CapturePorts {
  captureContext: () => Promise<CaptureContext>;
  createNote: typeof realCreateNote;
  saveNote: typeof realSaveNote;
  revealNote: (noteId: string) => Promise<void>;
  /** Shut the window. The real one is {@link closeThisWindow}. */
  close: () => Promise<void>;
}

/**
 * The note's title: its **first non-blank line**, and the body is what follows
 * that line.
 *
 * A split rather than a copy. The title field in the main window would
 * otherwise repeat the body's opening words back at the reader, and a note
 * whose body began with its own heading is not what anybody typed. Nothing is
 * lost: a one-line capture is a note with a title and an empty body, which is
 * exactly what a one-line thought is.
 *
 * Leading blank lines are skipped rather than becoming an empty title —
 * somebody who pressed Return first still meant the sentence after it.
 */
export function split(text: string): { title: string; body: string } {
  const lines = text.split("\n");
  const first = lines.findIndex((line) => line.trim() !== "");
  if (first === -1) return { title: "", body: "" };
  return {
    title: lines[first]!.trim(),
    body: lines.slice(first + 1).join("\n"),
  };
}

export interface Capture {
  /** What has been typed, and what the textarea binds to. */
  text: string;
  /** The note's id once it exists, or `null` while nothing has been typed. */
  readonly noteId: string | null;
  /** The last failure, as a sentence, or `null`. */
  readonly failure: string | null;
  /** True while a write is in flight, so the exits cannot overlap. */
  readonly busy: boolean;
  /** Take what has been typed, creating the note the first time. */
  typed(text: string): Promise<void>;
  /** Escape and ⌘Enter: save what there is, and close. */
  finish(): Promise<void>;
  /** *Open in knobas*: save, put the note in the main window, and close. */
  openInMain(): Promise<void>;
}

export function createCapture(ports?: Partial<CapturePorts>): Capture {
  const io: CapturePorts = {
    captureContext: realCaptureContext,
    createNote: realCreateNote,
    saveNote: realSaveNote,
    revealNote: realRevealNote,
    close: closeThisWindow,
    ...ports,
  };

  const state = $state<{
    text: string;
    noteId: string | null;
    failure: string | null;
    busy: boolean;
  }>({ text: "", noteId: null, failure: null, busy: false });

  /**
   * The recorded pair, read once when the window opens.
   *
   * Started here rather than awaited on the first keystroke, because the
   * keystroke is what the reader is waiting on and the answer is already true:
   * the main window recorded it before the shortcut was pressed, and it cannot
   * change while this window is in front. A read that fails is an **empty**
   * pair rather than a refusal — a note with no links is still the thought, and
   * losing the attribution is honest where losing the observation is not
   * (`commands::time`'s heartbeat, the same rule).
   */
  const recorded: Promise<CaptureContext> = io
    .captureContext()
    .catch(() => ({ context: null, foreground: null }));

  /**
   * The one line every write goes through: queued behind whatever is already
   * in flight, and turning any rejection into the sentence the window shows.
   *
   * **A queue and not a `busy` guard that drops.** Escape can arrive while the
   * first keystroke's `create_note` is still going -- that is a fast typist and
   * a slow database, not an edge case -- and a guard that refused the second
   * act would be a window that ignores Escape. Chaining makes the close wait
   * for the create it has to save into, in the order the reader made them.
   */
  let chain: Promise<void> = Promise.resolve();
  function queue(work: () => Promise<void>): Promise<void> {
    state.busy = true;
    chain = chain
      .then(async () => {
        try {
          await work();
          state.failure = null;
        } catch (rejection) {
          state.failure = ipcErrorMessage(rejection);
        }
      })
      .finally(() => {
        state.busy = false;
      });
    return chain;
  }

  /** Save what is typed, if there is a note to save it into. */
  async function save(): Promise<NoteDetail | null> {
    if (state.noteId === null) return null;
    const { title, body } = split(state.text);
    return io.saveNote(state.noteId, title, body);
  }

  return {
    get text() {
      return state.text;
    },
    set text(next: string) {
      state.text = next;
    },
    get noteId() {
      return state.noteId;
    },
    get failure() {
      return state.failure;
    },
    get busy() {
      return state.busy;
    },
    async typed(text: string) {
      state.text = text;
      // **Never on an empty one.** A textarea that was typed into and then
      // emptied again has still never had a note, and a keystroke that is only
      // whitespace is not a thought — `split` would give it neither a title
      // nor a body.
      if (state.noteId !== null || text.trim() === "") return;
      await queue(async () => {
        // Re-read inside the queue: a second keystroke queued behind the first
        // must not write a second note, and the first one's id lands here.
        if (state.noteId !== null) return;
        const { title, body } = split(text);
        const born = await io.createNote(title, body, bornWith(await recorded));
        state.noteId = born.note.id;
      });
    },
    async finish() {
      await queue(async () => {
        await save();
        await io.close();
      });
    },
    async openInMain() {
      await queue(async () => {
        const saved = await save();
        if (saved !== null) await io.revealNote(saved.note.id);
        await io.close();
      });
    },
  };
}

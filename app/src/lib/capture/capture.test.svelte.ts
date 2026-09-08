/**
 * The capture window's state (issue #503, criterion 2).
 *
 * The seam is `createCapture`'s ports: a keystroke in, IPC calls out. What the
 * backend then stores is `crates/knobas-app/tests/capture_ipc.rs`'s, and which
 * links a note is *allowed* to be born with is #502's; what is asserted here is
 * the window's own five decisions — when a note is created, when it is not, what
 * its title is, what each exit does, and which two links the recorded pair
 * becomes.
 */
import { expect, test } from "vitest";

import { createCapture, split, type CapturePorts } from "./capture.svelte";
import type { CaptureContext, NoteDetail } from "../ipc/entity";

const NOTE_ID = "note:0f2c1a";

/** A `NoteDetail` with the id under test; nothing here reads the rest of it. */
function detail(id = NOTE_ID): NoteDetail {
  return {
    note: {
      id,
      title: "",
      body_md: "",
      created_at: "2026-09-08T09:00:00Z",
      updated_at: "2026-09-08T09:00:00Z",
    },
    refs: [],
    links: [],
  };
}

/**
 * A capture over a bridge that records every call.
 *
 * `close` is recorded rather than stubbed away, because *whether the window
 * shut* is half of what every exit test asserts — a `finish` that saved and
 * left the window up is the failure a test that only looked at the save would
 * pass.
 */
function harness(
  recorded: CaptureContext = { context: "ctx:sepa", foreground: "mock:PAY-231" },
  overrides: Partial<CapturePorts> = {},
) {
  const calls: string[] = [];
  const created: { title: string | undefined; body: string | undefined; links: unknown }[] = [];
  const saved: { noteId: string; title: string; body: string }[] = [];
  const revealed: string[] = [];

  const ports: CapturePorts = {
    captureContext: async () => {
      calls.push("captureContext");
      return recorded;
    },
    createNote: async (title, bodyMd, links) => {
      calls.push("createNote");
      created.push({ title, body: bodyMd, links });
      return detail();
    },
    saveNote: async (noteId, title, bodyMd) => {
      calls.push("saveNote");
      saved.push({ noteId, title, body: bodyMd });
      return detail(noteId);
    },
    revealNote: async (noteId) => {
      calls.push("revealNote");
      revealed.push(noteId);
    },
    close: async () => {
      calls.push("close");
    },
    ...overrides,
  };

  return { capture: createCapture(ports), calls, created, saved, revealed };
}

// --- the title, on its own ---------------------------------------------------

test("the title is the first non-blank line and the body is what follows it", () => {
  expect(split("Retry storm\nthe queue backs up at 09:00\nand again at 10:00")).toEqual({
    title: "Retry storm",
    body: "the queue backs up at 09:00\nand again at 10:00",
  });
});

test("a one-line capture is a title and an empty body", () => {
  expect(split("Ask Mara about the SEPA cutover")).toEqual({
    title: "Ask Mara about the SEPA cutover",
    body: "",
  });
});

test("a leading blank line is skipped rather than becoming an empty title", () => {
  expect(split("\n\n  Retry storm  \nthe queue backs up")).toEqual({
    title: "Retry storm",
    body: "the queue backs up",
  });
});

test("nothing but whitespace is neither a title nor a body", () => {
  expect(split("   \n\t\n")).toEqual({ title: "", body: "" });
});

// --- creation ----------------------------------------------------------------

test("the first keystroke creates the note, with what was typed and the two links", async () => {
  const { capture, created } = harness();
  await capture.typed("R");

  expect(capture.noteId).toBe(NOTE_ID);
  expect(created).toEqual([
    {
      title: "R",
      body: "",
      links: [
        { target_id: "ctx:sepa", relation: "captured-in" },
        { target_id: "mock:PAY-231", relation: "captured-from" },
      ],
    },
  ]);
});

test("the second keystroke creates nothing more", async () => {
  const { capture, created } = harness();
  await capture.typed("R");
  await capture.typed("Re");
  await capture.typed("Ret");

  expect(created).toHaveLength(1);
  expect(capture.noteId).toBe(NOTE_ID);
});

/**
 * A keystroke that is only whitespace is not a thought, and it must not leave a
 * row behind: `split` would give it neither a title nor a body, so what a
 * "created on space" would write is an empty note nobody asked for.
 */
test("whitespace alone creates nothing", async () => {
  const { capture, created } = harness();
  await capture.typed(" ");
  await capture.typed("\n\t ");

  expect(created).toEqual([]);
  expect(capture.noteId).toBeNull();
});

test("a capture in a derived room with nothing open is born with no links at all", async () => {
  const { capture, created } = harness({ context: null, foreground: null });
  await capture.typed("Idea");

  expect(created).toEqual([{ title: "Idea", body: "", links: [] }]);
});

/**
 * A pair the backend could not answer is an empty pair, not a refused capture.
 * Losing the attribution is honest; losing the observation is not — the rule
 * `commands::time`'s heartbeat already keeps for the same two facts.
 */
test("a capture whose context could not be read is still written, with no links", async () => {
  const { capture, created } = harness(
    { context: "ctx:sepa", foreground: "mock:PAY-231" },
    { captureContext: () => Promise.reject(new Error("not_ready")) },
  );
  await capture.typed("Idea");

  expect(created).toEqual([{ title: "Idea", body: "", links: [] }]);
  expect(capture.failure).toBeNull();
});

// --- the exits ---------------------------------------------------------------

test("an empty close creates nothing and saves nothing", async () => {
  const { capture, calls } = harness();
  await capture.finish();

  // `captureContext` is the read the window starts on the way up, before any
  // keystroke; the assertion is on the whole list rather than an absence, so a
  // fourth call appearing here would be a failure and not a silence.
  expect(calls).toEqual(["captureContext", "close"]);
  expect(capture.noteId).toBeNull();
});

test("closing after typing saves the note first, and the title is the first line", async () => {
  const { capture, calls, saved } = harness();
  await capture.typed("Retry storm\nthe queue backs up at 09:00");
  await capture.finish();

  expect(saved).toEqual([
    { noteId: NOTE_ID, title: "Retry storm", body: "the queue backs up at 09:00" },
  ]);
  // The order is the assertion, not just the pair: a close that ran first would
  // take the window away with the last sentence unsaved.
  expect(calls).toEqual(["captureContext", "createNote", "saveNote", "close"]);
});

/**
 * *Open in knobas* is the third exit and does one thing more: it saves, hands
 * the note to the main window, and then closes.
 *
 * The **order** is what is asserted, and the reveal being before the close is
 * the point: a window that closed first would have nothing to report a refused
 * reveal into.
 */
test("open-in-main saves, reveals the note, and then closes", async () => {
  const { capture, calls, revealed, saved } = harness();
  await capture.typed("Retry storm");
  await capture.openInMain();

  expect(saved).toEqual([{ noteId: NOTE_ID, title: "Retry storm", body: "" }]);
  expect(revealed).toEqual([NOTE_ID]);
  expect(calls).toEqual(["captureContext", "createNote", "saveNote", "revealNote", "close"]);
});

/**
 * A close that arrives while the first keystroke's `create_note` is still going.
 *
 * A fast typist and a slow database, not an edge case — and the failure it
 * guards is the one a `busy` flag that *dropped* would cause: Escape ignored,
 * with the window still up.
 */
test("Escape during the first write still saves into the note that write made", async () => {
  let release: (value: NoteDetail) => void = () => {};
  const slow = new Promise<NoteDetail>((resolve) => {
    release = resolve;
  });
  // Built by hand rather than through `harness`, so that the one slow port is
  // still a *recording* one: an override that replaced the recorder would make
  // the call order this test is about unobservable.
  const calls: string[] = [];
  const saved: { noteId: string; title: string; body: string }[] = [];
  const capture = createCapture({
    captureContext: async () => {
      calls.push("captureContext");
      return { context: null, foreground: null };
    },
    createNote: () => {
      calls.push("createNote");
      return slow;
    },
    saveNote: async (noteId, title, bodyMd) => {
      calls.push("saveNote");
      saved.push({ noteId, title, body: bodyMd });
      return detail(noteId);
    },
    revealNote: async () => {},
    close: async () => {
      calls.push("close");
    },
  });

  const typing = capture.typed("Retry storm");
  const closing = capture.finish();
  release(detail());
  await typing;
  await closing;

  expect(saved).toEqual([{ noteId: NOTE_ID, title: "Retry storm", body: "" }]);
  expect(calls).toEqual(["captureContext", "createNote", "saveNote", "close"]);
});

// --- failures ----------------------------------------------------------------

test("a note that could not be written says so and leaves the window up", async () => {
  const { capture, calls } = harness(
    { context: null, foreground: null },
    {
      createNote: () =>
        Promise.reject({ code: "not_ready", message: "the database is still starting" }),
    },
  );
  await capture.typed("Idea");

  expect(capture.failure).toBe("the database is still starting");
  expect(capture.noteId).toBeNull();
  expect(calls).not.toContain("close");
});

/**
 * A failed write is retried by the next keystroke rather than needing the reader
 * to do something. The window has one control, and it is the keyboard.
 */
test("the next keystroke after a failed write tries again", async () => {
  const created: string[] = [];
  let fail = true;
  const capture = createCapture({
    captureContext: async () => ({ context: null, foreground: null }),
    createNote: async (title) => {
      created.push(title ?? "");
      if (fail) throw { code: "not_ready", message: "the database is still starting" };
      return detail();
    },
    saveNote: async () => detail(),
    revealNote: async () => {},
    close: async () => {},
  });

  await capture.typed("I");
  expect(capture.noteId).toBeNull();
  fail = false;
  await capture.typed("Id");

  expect(created).toEqual(["I", "Id"]);
  expect(capture.noteId).toBe(NOTE_ID);
});

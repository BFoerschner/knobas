/**
 * The capture window as a reader meets it (issue #503, criterion 2).
 *
 * What each exit *does* is `capture.test.svelte.ts`'s; what is asserted here is
 * the half that only a rendered window has — that **Escape and ⌘Enter both
 * reach it**, that typing in the box is what creates the note, and that the
 * button is not offered before there is a note for it to open.
 *
 * The keys are the reason this file exists. They are the window's only chrome:
 * it has no title bar, so a keydown that stopped being listened for would leave
 * an always-on-top rectangle with no way out, and nothing in
 * `capture.svelte.ts` can see that.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import CaptureWindow from "./CaptureWindow.svelte";
import type { CapturePorts } from "./capture.svelte";
import type { NoteDetail } from "../ipc/entity";

const NOTE_ID = "note:0f2c1a";

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

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;

function render(overrides: Partial<CapturePorts> = {}) {
  const calls: string[] = [];
  const saved: { noteId: string; title: string; body: string }[] = [];
  const revealed: string[] = [];
  let closed = false;

  app = mount(CaptureWindow, {
    target,
    props: {
      ports: {
        close: async () => {
          closed = true;
          calls.push("close");
        },
        captureContext: async () => ({ context: "ctx:sepa", foreground: "mock:PAY-231" }),
        createNote: async () => {
          calls.push("createNote");
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
        ...overrides,
      },
    },
  });
  flushSync();
  return { calls, saved, revealed, wasClosed: () => closed };
}

async function settle() {
  for (let i = 0; i < 4; i += 1) await Promise.resolve();
  flushSync();
}

function text(): string {
  return (target.textContent ?? "").replace(/\s+/g, " ").trim();
}

function box(): HTMLTextAreaElement {
  const found = target.querySelector<HTMLTextAreaElement>("textarea");
  if (!found) throw new Error(`no capture box: ${text()}`);
  return found;
}

function button(): HTMLButtonElement {
  const found = target.querySelector<HTMLButtonElement>('button[aria-label="Open in knobas"]');
  if (!found) throw new Error(`no *Open in knobas* button: ${text()}`);
  return found;
}

/** Type the way a person does, so the `oninput` handler sees it. */
async function type(value: string) {
  const area = box();
  area.value = value;
  area.dispatchEvent(new Event("input", { bubbles: true }));
  await settle();
}

/** A real keydown on `window`, which is where the component listens. */
async function press(key: string, modifiers: { meta?: boolean; ctrl?: boolean } = {}) {
  window.dispatchEvent(
    new KeyboardEvent("keydown", {
      key,
      metaKey: modifiers.meta ?? false,
      ctrlKey: modifiers.ctrl ?? false,
      bubbles: true,
      cancelable: true,
    }),
  );
  await settle();
}

beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  if (app) unmount(app);
  app = undefined;
  target.remove();
});

test("the box has an accessible name and the caret starts in it", () => {
  render();
  expect(box().getAttribute("aria-label")).toBe("Capture");
  expect(document.activeElement).toBe(box());
});

test("the window says nothing is kept until something is typed", () => {
  render();
  expect(text()).toContain("Nothing is kept until you type");
});

test("typing creates the note, and the window says so", async () => {
  const { calls } = render();
  await type("Retry storm");

  expect(calls).toEqual(["createNote"]);
  expect(text()).toContain("Saved as a note");
});

test("Escape closes the window with what was typed saved", async () => {
  const { calls, saved, wasClosed } = render();
  await type("Retry storm\nthe queue backs up");
  await press("Escape");

  expect(saved).toEqual([
    { noteId: NOTE_ID, title: "Retry storm", body: "the queue backs up" },
  ]);
  expect(wasClosed()).toBe(true);
  expect(calls).toEqual(["createNote", "saveNote", "close"]);
});

test("⌘Enter closes the window with what was typed saved", async () => {
  const { saved, wasClosed } = render();
  await type("Retry storm");
  await press("Enter", { meta: true });

  expect(saved).toEqual([{ noteId: NOTE_ID, title: "Retry storm", body: "" }]);
  expect(wasClosed()).toBe(true);
});

/**
 * Ctrl+Enter beside ⌘Enter, because knobas builds for three platforms and has
 * been run on one. On the other two the modifier is Ctrl, and a window with no
 * title bar whose only exit was a Mac key would be a window nobody there can
 * close.
 */
test("Ctrl+Enter closes it too", async () => {
  const { wasClosed } = render();
  await type("Retry storm");
  await press("Enter", { ctrl: true });

  expect(wasClosed()).toBe(true);
});

/**
 * A bare Return is a **new line**, not an exit. Without this the window could
 * only ever hold one line, and the title-and-body split below it would have
 * nothing to split.
 */
test("a bare Return does not close the window", async () => {
  const { wasClosed, calls } = render();
  await type("Retry storm");
  await press("Enter");

  expect(wasClosed()).toBe(false);
  expect(calls).toEqual(["createNote"]);
});

test("Escape on an empty window closes it and writes nothing", async () => {
  const { calls, wasClosed } = render();
  await press("Escape");

  expect(calls).toEqual(["close"]);
  expect(wasClosed()).toBe(true);
});

test("the button is not offered until there is a note to open", async () => {
  render();
  expect(button().disabled).toBe(true);

  await type("Retry storm");
  expect(button().disabled).toBe(false);
});

test("the button saves, opens the note in the main window, and closes this one", async () => {
  const { calls, revealed, wasClosed } = render();
  await type("Retry storm");
  button().click();
  await settle();

  expect(revealed).toEqual([NOTE_ID]);
  expect(wasClosed()).toBe(true);
  expect(calls).toEqual(["createNote", "saveNote", "revealNote", "close"]);
});

test("a write that failed is shown in the window and the window stays up", async () => {
  const { wasClosed } = render({
    createNote: () =>
      Promise.reject({ code: "not_ready", message: "the database is still starting" }),
  });
  await type("Retry storm");

  expect(text()).toContain("the database is still starting");
  expect(wasClosed()).toBe(false);
});

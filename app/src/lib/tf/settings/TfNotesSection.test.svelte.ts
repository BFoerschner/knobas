/**
 * Settings › Notes and capture (#832), against stand-ins for its three
 * commands. Whether the shortcut is registered is
 * `crates/knobas-app/tests/it/capture_ipc.rs`'s; here it is what the section
 * draws and what each control sends.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { CaptureShortcut } from "../../ipc/entity";
import { toasts } from "../../shell/toasts.svelte";
import type { NotesPorts } from "./notes-ports";
import TfNotesSection from "./TfNotesSection.svelte";

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  toasts.items = [];
  target = document.createElement("div");
  document.body.appendChild(target);
});

afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

const NOW = new Date("2026-10-16T10:00:00Z");

function stub(initial: Partial<CaptureShortcut> = {}) {
  let stored: CaptureShortcut = {
    accelerator: "CmdOrCtrl+Alt+N",
    refusal: null,
    shown: false,
    note_count: 37,
    last_note_at: "2026-08-12T09:00:00Z",
    ...initial,
  };
  const sent: boolean[] = [];
  let exported = 0;
  const ports: NotesPorts = {
    captureShortcut: () => Promise.resolve(stored),
    setNotesShown: (shown) => {
      sent.push(shown);
      stored = { ...stored, shown };
      return Promise.resolve(stored);
    },
    exportNotes: () => {
      exported += 1;
      return Promise.resolve({ count: 37, folder: "/Users/mara/Downloads/knobas-notes" });
    },
  };
  return { ports, sent, exported: () => exported };
}

async function render(ports: NotesPorts) {
  view = mount(TfNotesSection, { target, props: { ports, now: () => NOW } });
  await Promise.resolve();
  await Promise.resolve();
  flushSync();
}

const text = () => target.textContent ?? "";
const button = (name: string) =>
  [...target.querySelectorAll("button")].find(
    (b) => (b.getAttribute("aria-label") ?? b.textContent?.trim()) === name,
  ) as HTMLButtonElement;

test("hidden by default: the switch is off and the shortcut says it is inactive", async () => {
  await render(stub().ports);

  expect(button("Show notes").getAttribute("role")).toBe("switch");
  expect(button("Show notes").getAttribute("aria-checked")).toBe("false");
  expect(text()).toContain("Inactive while notes are hidden.");
  expect([...target.querySelectorAll("kbd")].map((k) => k.textContent)).toEqual(["⌥", "⌘", "N"]);
  expect(text()).toContain("37 notes, the last from 12 August");
});

test("turning the switch on sends it and the shortcut works again", async () => {
  const fake = stub();
  await render(fake.ports);

  button("Show notes").click();
  await Promise.resolve();
  await Promise.resolve();
  flushSync();

  expect(fake.sent).toEqual([true]);
  expect(button("Show notes").getAttribute("aria-checked")).toBe("true");
  expect(text()).toContain("Opens a small window over whatever you are doing.");
});

test("a shortcut the system refused says so while notes are shown", async () => {
  await render(stub({ shown: true, refusal: "taken by another application" }).ports);
  expect(text()).toContain("Not registered: taken by another application");
});

test("Export notes as Markdown says where the files went", async () => {
  const fake = stub();
  await render(fake.ports);

  button("Export notes as Markdown").click();
  await Promise.resolve();
  await Promise.resolve();
  flushSync();

  expect(fake.exported()).toBe(1);
  expect(toasts.items.map((t) => t.text)).toContain("Saved 37 notes as Markdown to ~/Downloads/knobas-notes");
});

test("Move to Confluence… says it comes with the Docs view", async () => {
  await render(stub().ports);
  button("Move to Confluence…").click();
  expect(toasts.items.at(-1)?.text).toContain("comes with the Docs view");
});

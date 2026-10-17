/**
 * Settings › Notes and capture (#832), against stand-ins for its commands
 * and, for *Move to Confluence…* (#941), the page picker's reads. Whether
 * the shortcut is registered is `crates/knobas-app/tests/it/capture_ipc.rs`'s;
 * here it is what the section draws and what each control sends.
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
  const moved: string[] = [];
  let exported = 0;
  const ports: NotesPorts = {
    moveNotes: (parent) => {
      moved.push(parent);
      return Promise.resolve(Array.from({ length: 37 }, () => ({}) as never));
    },
    listSources: () =>
      Promise.resolve([{ id: "wiki", display_name: "Wiki", adapter_kind: "confluence", enabled: true }] as never),
    listAdapters: () => Promise.resolve([{ adapter_kind: "confluence", write_ops: ["create_page"] }] as never),
    search: () =>
      Promise.resolve({
        interpreted: { text: "", prefix: null, filters: {}, unknown_tokens: [] },
        groups: [
          {
            kind: "page",
            hits: [
              {
                entity_id: "wiki:98500",
                kind: "page",
                source_id: "wiki",
                updated_at: null,
                synced_at: "2026-10-16T07:00:00Z",
                title: "Notes archive",
                path: null,
                rank: 1,
                snippet: [],
              },
            ],
          },
        ],
        total: 1,
        took_ms: 1,
        coverage: [],
      } as never),
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
  return { ports, sent, moved, exported: () => exported };
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

async function settle() {
  for (let tick = 0; tick < 10; tick += 1) await Promise.resolve();
  flushSync();
}

/** In the open dialog: find *Notes archive*, pick it, and press *Move 37 notes*. */
async function pickAndMove() {
  const find = target.querySelector<HTMLInputElement>('input[aria-label="Parent page"]')!;
  find.value = "archive";
  find.dispatchEvent(new Event("input", { bubbles: true }));
  await settle();
  button("Notes archive").click();
  flushSync();
  button("Move 37 notes").click();
  await settle();
}

test("Move to Confluence… picks a page and queues the notes under it", async () => {
  const fake = stub();
  await render(fake.ports);

  button("Move to Confluence…").click();
  await settle();
  expect(target.querySelector('[role="dialog"] h2')?.textContent).toBe("Move notes to Confluence");
  expect(button("Move 37 notes").disabled).toBe(true);

  await pickAndMove();

  expect(fake.moved).toEqual(["wiki:98500"]);
  expect(target.querySelector('[role="dialog"]')).toBeNull();
  expect(toasts.items.at(-1)?.text).toBe("Queued 37 pages in Confluence. Your notes stay in knobas.");
});

test("a move the backend refuses keeps the dialog open and says why", async () => {
  const fake = stub();
  fake.ports.moveNotes = () => Promise.reject({ code: "not_found", message: "wiki:98500 is not in the mirror" });
  await render(fake.ports);

  button("Move to Confluence…").click();
  await settle();
  await pickAndMove();

  expect(target.querySelector('[role="dialog"]')).not.toBeNull();
  expect(toasts.items.at(-1)?.text).toBe("Could not move the notes: wiki:98500 is not in the mirror");
});

test("with no notes there is nothing to move", async () => {
  await render(stub({ note_count: 0 }).ports);
  expect(button("Move to Confluence…").disabled).toBe(true);
});

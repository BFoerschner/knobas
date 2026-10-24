/**
 * A note's read-only page (#1504): `#/note/<id>` while notes are shown. The
 * bridge is injected; what `get_note` and `delete_note` do with a note is
 * `crates/knobas-app/tests/it/entity.rs`'s.
 */
import { flushSync, mount, tick, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { LinkEnd, LinkEntry, NoteDetail } from "../../ipc/entity";
import { toasts } from "../../shell/toasts.svelte";
import { createLayers } from "../layers.svelte";
import TfNote, { type NotePorts } from "./TfNote.svelte";

const ID = "note:7a1";
const BODY = "Retries need a jittered backoff, see [[mock:PAY-231]].\n\n  Ask [[mock:NOPE-1]] and [[mock:PAY-219]] too.";

function end(entity_id: string, kind: string, title: string, deleted_at: string | null = null): LinkEnd {
  return { entity_id, kind, title, deleted_at };
}

function link(to: LinkEnd, relation: string, fromHere = true): LinkEntry {
  return {
    link: {
      id: `link:${to.entity_id}`,
      from_id: fromHere ? ID : to.entity_id,
      to_id: fromHere ? to.entity_id : ID,
      relation,
      origin: "manual",
      note: null,
      created_by: "user",
      created_at: "",
      confirmed_at: "",
      rule: null,
      rule_class: null,
      reason: null,
    },
    other: to,
  };
}

const PAY_231 = end("mock:PAY-231", "ticket", "Retry failed SEPA payouts");

function detail(over: Partial<NoteDetail["note"]> = {}): NoteDetail {
  return {
    note: { id: ID, title: "Retry backoff, thinking aloud", body_md: BODY, created_at: "", updated_at: "", ...over },
    refs: [
      { target_id: "mock:PAY-231", target: PAY_231 },
      { target_id: "mock:NOPE-1", target: null },
      { target_id: "mock:PAY-219", target: end("mock:PAY-219", "ticket", "Rotate PSP credentials", "2026-08-20T10:00:00Z") },
    ],
    links: [
      link(end("ctx:sepa", "ctx", "SEPA retries"), "captured-in"),
      link(PAY_231, "captured-from"),
      link(end("note:9b2", "note", "Ledger drift notes"), "related", false),
    ],
  };
}

let target: HTMLDivElement;
let view: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
  toasts.items = [];
});
afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
  toasts.items = [];
});

async function settle() {
  for (let round = 0; round < 6; round++) {
    await tick();
    await Promise.resolve();
  }
  flushSync();
}

function open(over: { shown?: boolean; note?: NoteDetail | "missing"; history?: boolean } = {}) {
  const calls = { deleted: [] as string[], replaced: [] as string[], back: 0 };
  const ports: NotePorts = {
    getNote: async () => {
      if (over.note === "missing") throw { code: "not_found", message: `there is no note ${ID}`, source_id: null };
      return over.note ?? detail();
    },
    deleteNote: async (id) => {
      calls.deleted.push(id);
      return true;
    },
    notesShown: async () => over.shown ?? true,
    replace: (hash) => calls.replaced.push(hash),
    canGoBack: () => over.history ?? true,
    back: () => calls.back++,
  };
  view = mount(TfNote, { target, props: { id: ID, ready: true, job: null, ports, layers: createLayers() } });
  return calls;
}

const text = () => target.textContent?.replace(/\s+/g, " ") ?? "";
const button = (name: string, within: ParentNode = document) =>
  [...within.querySelectorAll("button")].find((found) => found.textContent?.trim() === name) as HTMLButtonElement | undefined;

test("the page draws the title, the body as it was typed, and each resolved ref as a link to its entity", async () => {
  open();
  await settle();
  expect(target.querySelector("h1")?.textContent).toBe("Retry backoff, thinking aloud");
  const body = target.querySelector<HTMLElement>(".tf-note-body")!;
  // Pre-wrapped: the line breaks and the indent are the body's.
  expect(body.textContent).toBe(
    "Retries need a jittered backoff, see Retry failed SEPA payouts.\n\n  Ask [[mock:NOPE-1]] and Rotate PSP credentials too.",
  );
  const refs = [...body.querySelectorAll("a")].map((found) => [found.textContent, found.getAttribute("href")]);
  expect(refs).toEqual([["Retry failed SEPA payouts", "#/job/mock%3APAY-231"]]);
  // A withdrawn target is still named, not linked.
  expect(body.querySelector(".tf-note-gone")?.textContent).toBe("Rotate PSP credentials");
});

test("the note's links are rows that open their other end, a context's row only named", async () => {
  open();
  await settle();
  const rows = [...target.querySelectorAll("ul[aria-label='Links'] li")].map((row) => [
    row.textContent?.replace(/\s+/g, " ").trim(),
    row.querySelector("a")?.getAttribute("href") ?? null,
  ]);
  expect(rows).toEqual([
    ["Captured in SEPA retries", null],
    ["Captured from Retry failed SEPA payouts", "#/job/mock%3APAY-231"],
    ["Related to Ledger drift notes", "#/note/note%3A9b2"],
  ]);
});

test("a note that is not there reads the shell's Nothing lives at", async () => {
  open({ note: "missing" });
  await settle();
  expect(text()).toContain("Nothing lives at");
  expect(target.querySelector("h1")).toBeNull();
});

test("while notes are hidden the page sends the reader to Settings › Notes and capture", async () => {
  const calls = open({ shown: false });
  await settle();
  expect(calls.replaced).toEqual(["#/settings/notes"]);
  expect(target.querySelector("h1")).toBeNull();
});

test("a protocol note opens Time › Standup at its date instead", async () => {
  const calls = open({ note: detail({ title: "Standup 2026-09-03" }) });
  await settle();
  expect(calls.replaced).toEqual(["#/time/standup/2026-09-03"]);
});

test("Delete asks first, and Cancel deletes nothing", async () => {
  const calls = open();
  await settle();
  button("Delete", target)!.click();
  await settle();
  expect(document.querySelector(".tf-dialog h2")?.textContent).toBe("Delete Retry backoff, thinking aloud?");
  button("Cancel", document.querySelector(".tf-dialog")!)!.click();
  await settle();
  expect(calls.deleted).toEqual([]);
  expect(document.querySelector(".tf-dialog")).toBeNull();
});

test("a confirmed Delete deletes the note, goes back, and says so with no undo", async () => {
  const calls = open();
  await settle();
  button("Delete", target)!.click();
  await settle();
  button("Delete", document.querySelector(".tf-dialog")!)!.click();
  await settle();
  expect(calls.deleted).toEqual([ID]);
  expect(calls.back).toBe(1);
  expect(calls.replaced).toEqual([]);
  expect(toasts.items.map((toast) => toast.text)).toEqual(["Deleted Retry backoff, thinking aloud"]);
  expect(toasts.items[0]!.action).toBeUndefined();
});

test("deleted from the first address, the reader lands on Settings › Notes and capture", async () => {
  const calls = open({ history: false });
  await settle();
  button("Delete", target)!.click();
  await settle();
  button("Delete", document.querySelector(".tf-dialog")!)!.click();
  await settle();
  expect(calls.back).toBe(0);
  expect(calls.replaced).toEqual(["#/settings/notes"]);
});

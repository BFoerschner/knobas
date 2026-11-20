/** Settings › Search (the everything-search decisions, 2026-11-20, D13): a box for each text Everything matches, drawn at once and written behind. */
import { flushSync, mount, tick, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { EverythingMatches, SearchSettings } from "../../ipc";
import TfSearchSection from "./TfSearchSection.svelte";

const ALL: EverythingMatches = { ticket_text: true, pr_text: true, page_text: true, build_logs: true, file_names: true, file_text: true };
const READ: SearchSettings = { everything: ALL, tickets: 23_176, pull_requests: 1, pages: 5_032, pipelines: 91 };

let target: HTMLDivElement;
let view: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

async function draw(props: Record<string, unknown>): Promise<void> {
  view = mount(TfSearchSection, { target, props });
  for (let i = 0; i < 4; i++) await tick();
  flushSync();
}

const boxes = () => [...target.querySelectorAll<HTMLLabelElement>(".tf-stg-box")];
const lines = () => boxes().map((box) => `${box.querySelector("input")!.checked ? "[x]" : "[ ]"} ${box.textContent}`);

test("every box is drawn from the settings, each with what it covers", async () => {
  await draw({ read: async () => ({ ...READ, everything: { ...ALL, pr_text: false } }), write: vi.fn() });
  expect(lines()).toEqual([
    "[x] Ticket descriptions and comments23,176 tickets",
    "[ ] Pull request descriptions and comments1 pull request",
    "[x] Docs pages and their comments5,032 pages",
    "[x] Build logsThe end of each pipeline's last finished run.91 pipelines",
  ]);
  expect(target.querySelector("h1")!.textContent).toBe("Search");
});

test("a click is drawn at once and written, and the answer drawn", async () => {
  let release: (answer: SearchSettings) => void = () => {};
  const write = vi.fn((matches: EverythingMatches) => new Promise<SearchSettings>((done) => (release = done)).then(() => ({ ...READ, everything: matches })));
  await draw({ read: async () => READ, write });
  boxes()[0]!.querySelector("input")!.click();
  flushSync();
  expect(boxes()[0]!.querySelector("input")!.checked).toBe(false);
  expect(write).toHaveBeenCalledWith({ ...ALL, ticket_text: false });
  release(READ);
  for (let i = 0; i < 4; i++) await tick();
  flushSync();
  expect(boxes()[0]!.querySelector("input")!.checked).toBe(false);
});

test("a write that fails puts the box back and says so", async () => {
  await draw({ read: async () => READ, write: async () => Promise.reject({ code: "internal", message: "disk full" }) });
  boxes()[3]!.querySelector("input")!.click();
  for (let i = 0; i < 4; i++) await tick();
  flushSync();
  expect(boxes()[3]!.querySelector("input")!.checked).toBe(true);
  expect(target.textContent).toContain("Could not save the box: disk full");
});

test("a read that fails says so, and one not yet answered says it is reading", async () => {
  await draw({ read: () => new Promise(() => {}), write: vi.fn() });
  expect(target.textContent).toContain("Reading what Everything matches…");
  unmount(view!);
  view = null;
  await draw({ read: async () => Promise.reject({ code: "internal", message: "gone" }), write: vi.fn() });
  expect(target.textContent).toContain("Could not read what Everything matches: gone");
});

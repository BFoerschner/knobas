/** Settings › Search (the everything-search decisions, 2026-11-20, D13): a box for each text Everything matches, drawn at once and written behind. */
import { flushSync, mount, tick, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { EverythingMatches, SearchSettings } from "../../ipc";
import TfSearchSection from "./TfSearchSection.svelte";

const ALL: EverythingMatches = { ticket_text: true, pr_text: true, page_text: true, build_logs: true, file_names: true, file_text: true };
const READ: SearchSettings = { everything: ALL, tickets: 23_176, pull_requests: 1, pages: 5_032, pipelines: 91, files: 61_204, text_files: 48_930, kept_mb: 2048, text_bytes: 612 * 1024 * 1024, repositories_read: 61, repositories: 65, left_out: {} };

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
    "[x] File namesEvery file on each repository's default branch. A match shows the repository.61,204 files",
    "[x] File contentsText files on each repository's default branch. A match shows the repository and the line.48,930 files",
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

test("the text kept says how much, from how many repositories, against the bound; None and a bound passed say so", async () => {
  const kept = vi.fn(async (mb: number) => ({ ...READ, kept_mb: mb }));
  await draw({ read: async () => READ, write: vi.fn(), kept, listen: async () => () => {} });
  const status = () => target.querySelector(".tf-stg-status")!.textContent;
  expect(status()).toBe("612 MB from 61 of 65 repositories.4 are waiting for their first read. Read after each sync, from your clone where you have one, else from the forge.");
  const select = target.querySelector<HTMLSelectElement>("#tf-search-kept")!;
  expect([...select.options].map((o) => o.textContent)).toEqual(["None", "500 MB", "1 GB", "2 GB", "5 GB", "10 GB"]);
  expect(select.value).toBe("2048");
  select.value = "0";
  select.dispatchEvent(new Event("change", { bubbles: true }));
  for (let i = 0; i < 4; i++) await tick();
  flushSync();
  expect(kept).toHaveBeenCalledWith(0);
  expect(status()).toBe("No file text is kept.Files are still found by their names.");
  unmount(view!);
  view = null;
  await draw({ read: async () => ({ ...READ, kept_mb: 500 }), write: vi.fn(), listen: async () => () => {} });
  expect(status()).toContain("More than the bound");
});

test("Read every repository again asks for it and draws the answer", async () => {
  const reread = vi.fn(async () => ({ ...READ, repositories_read: 0, text_bytes: 0 }));
  await draw({ read: async () => READ, write: vi.fn(), reread, listen: async () => () => {} });
  const button = [...target.querySelectorAll("button")].find((b) => b.textContent === "Read every repository again")!;
  button.click();
  for (let i = 0; i < 4; i++) await tick();
  flushSync();
  expect(reread).toHaveBeenCalledTimes(1);
  expect(target.querySelector(".tf-stg-status")!.textContent).toContain("from 0 of 65 repositories");
});

test("the files pass's progress reads the page again", async () => {
  vi.useFakeTimers();
  let tell: () => void = () => {};
  const read = vi.fn(async () => READ);
  view = mount(TfSearchSection, { target, props: { read, write: vi.fn(), listen: async (handler: () => void) => ((tell = handler), () => {}) } });
  for (let i = 0; i < 4; i++) await Promise.resolve();
  flushSync();
  const before = read.mock.calls.length;
  tell();
  tell();
  vi.advanceTimersByTime(1000);
  vi.useRealTimers();
  expect(read.mock.calls.length).toBe(before + 1);
});

test("a read that fails says so, and one not yet answered says it is reading", async () => {
  await draw({ read: () => new Promise(() => {}), write: vi.fn() });
  expect(target.textContent).toContain("Reading what Everything matches…");
  unmount(view!);
  view = null;
  await draw({ read: async () => Promise.reject({ code: "internal", message: "gone" }), write: vi.fn() });
  expect(target.textContent).toContain("Could not read what Everything matches: gone");
});

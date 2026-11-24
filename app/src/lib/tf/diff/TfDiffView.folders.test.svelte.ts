/**
 * A folder renderer in a comparison (ux26, `docs/decisions/2026-11-20-a-comparison-is-drawn-rendered-and-may-end-at-the-
 * working-tree.md` rulings 7 and 8; the decision list's sections 8 to 10): with *Rendered* on, a changed folder a
 * renderer takes is listed as its group, the generated files that differ under its bar; only *Run* runs; the answer
 * lands in one flush with the bar's node kept; *Run N folder renders* takes at most 20 folders a press.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { FolderRenderer } from "../../ipc/app";
import type { DiffFile } from "../../ipc/entity";
import { laneOf } from "../../render/folder-compare.svelte";
import { fakeCompareBackend } from "../../render/folder-compare.test-support";
import { createDiffLayout } from "./diff-layout.svelte";
import { createDiffRendered, type DiffRenderedChoice } from "./diff-rendered.svelte";
import TfDiffView from "./TfDiffView.svelte";

const SHA = "4f2a91c0d2b9e8a7f6c5d4b3a2918f7e6d5c4b3a";
const WT = "~working-tree";
const SITE: FolderRenderer = { id: "site", name: "Low-code site", command: "lowcode {path} --out {outdir}", holds: [], folders: ["docs"], output: "folder", timeout_secs: 30 };
const file = (path: string): DiffFile => ({
  path,
  old_path: null,
  change: "modified",
  additions: 1,
  deletions: 1,
  hunks: [{ header: "@@ -1 +1 @@", lines: [{ kind: "del", old: 1, new: null, text: "a" }, { kind: "add", old: null, new: 1, text: "b" }] }],
  truncated: false,
  threads: [],
});
const DIFFERS = {
  files: [
    { path: "index.md", change: "modified" as const, before_size: 10, after_size: 12, binary: false },
    { path: "steps/04-alert.md", change: "added" as const, before_size: null, after_size: 30, binary: false },
  ],
  same: 9,
  eol_only: 0,
  before_files: 11,
  after_files: 12,
};

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;
let rendering: DiffRenderedChoice;
beforeEach(() => {
  target = document.createElement("div");
  document.body.appendChild(target);
  rendering = createDiffRendered(() => null);
});
afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
  vi.useRealTimers();
});

function draw(files: DiffFile[], fake: ReturnType<typeof fakeCompareBackend>, more: Record<string, unknown> = {}, host: HTMLElement = target) {
  const saved = (more["saved"] as FolderRenderer[] | undefined) ?? [SITE];
  view = mount(TfDiffView, {
    target: host,
    props: {
      files,
      context: { text: () => Promise.resolve(null), bytes: () => Promise.resolve(new TextEncoder().encode("x")) },
      rendering,
      layout: createDiffLayout(() => null),
      defaults: { list: [] },
      folders: { entityId: "repo", before: SHA, after: WT },
      folderPorts: fake.ports,
      folderSaved: { list: saved, ensure: () => Promise.resolve() },
      picks: { of: () => null, set: () => Promise.resolve(), ensure: () => Promise.resolve() },
      madeBytes: (token: string, path: string) => Promise.resolve(new TextEncoder().encode(`${token} ${path}\n`)),
      ...more,
    } as never,
  });
  flushSync();
}
const settle = async () => {
  for (let n = 0; n < 12; n += 1) await Promise.resolve();
  flushSync();
};
const rail = () =>
  [...target.querySelectorAll<HTMLElement>(".tf-flist > :not(.tf-wgap)")].map((row) =>
    row.classList.contains("tf-dgdir") ? `<${row.textContent!.trim()}>` : row.classList.contains("tf-dgband") ? `band: ${row.textContent}` : row.classList.contains("tf-fdir") ? `[${row.textContent!.trim()}]` : `${row.classList.contains("tf-dmade") ? "made " : ""}${row.querySelector(".tf-fname")!.textContent}`,
  );
const bar = () => target.querySelector<HTMLElement>(".tf-dfolder .tf-madebar")!;
const barButton = (name: string) => [...bar().querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === name);
const runAll = () => target.querySelector<HTMLButtonElement>(".tf-drun button")!;

test("with Rendered on and both sides kept, the rail lists the folder, its band and only the generated files that differ", async () => {
  const fake = fakeCompareBackend();
  fake.keep(laneOf(SHA), "a");
  fake.keep(laneOf(WT), "b");
  fake.answers.set("a\nb", DIFFERS);
  rendering.set(true);
  draw([file("lib/x.rs"), file("docs/index.md"), file("docs/steps/03-retry.md")], fake);
  await settle();
  expect(rail()).toEqual(["<docs>", "band: Made by Low-code siteReal files", "made index.md", "made steps/04-alert.md", "[lib]", "x.rs"]);
  expect(target.querySelector('.tf-diff[data-file="docs/index.md"]'), "no real file of the folder").toBeNull();
  expect(bar().querySelector("[role=status]")!.textContent).toBe("Low-code site made 11 files before and 12 after from 4f2a91c and your working tree: 2 differ.");
  expect(target.querySelector(".tf-dfend")!.textContent!.trim()).toBe("9 generated files are the same on both sides and are not listed.");
  expect(target.querySelectorAll(".tf-dunder .tf-diff").length).toBe(2);
  expect(fake.asked.every((a) => a.ask === "look"), "a look starts nothing").toBe(true);
});

test("without a checkout no folder is offered: no glyph, no group", async () => {
  const fake = fakeCompareBackend();
  rendering.set(true);
  draw([file("docs/index.md")], fake, { folders: null });
  await settle();
  expect(target.querySelector(".tf-dfmark, .tf-dgdir, .tf-dfolder, .tf-drun")).toBeNull();
  expect(fake.asked).toEqual([]);
});

test("the toggle starts no run; Run runs both sides, and their answer lands in one flush with the bar's node kept", async () => {
  const fake = fakeCompareBackend();
  fake.answers.set("t1\nt2", DIFFERS);
  draw([file("docs/index.md"), file("src/a.rs")], fake);
  await settle();
  expect(target.querySelector(".tf-dfolder")).toBeNull();
  rendering.set(true);
  flushSync();
  target.querySelector<HTMLButtonElement>(".tf-drendered")!.click();
  target.querySelector<HTMLButtonElement>(".tf-drendered")!.click();
  await settle();
  expect(fake.asked.filter((a) => a.ask !== "look"), "the pill is a way of looking").toEqual([]);
  expect(bar().querySelector("[role=status]")!.textContent).toBe("Not rendered yet for both sides.");
  expect(barButton("Run")!.classList.contains("tf-primary")).toBe(true);
  expect(target.querySelector(".tf-dfolder .tf-dfnote")!.textContent).toBe("Its generated files show here when both sides are made.");
  barButton("Run")!.click();
  await settle();
  expect(fake.asked.filter((a) => a.ask === "again").map((a) => a.lane)).toEqual([laneOf(SHA), laneOf(WT)]);
  expect(barButton("Stop")).toBeDefined();
  const node = target.querySelector(".tf-dfolder");
  fake.land(laneOf(SHA), "t1");
  await settle();
  expect(target.querySelector(".tf-dfnote"), "half an answer lists nothing").not.toBeNull();
  fake.land(laneOf(WT), "t2");
  await settle();
  expect(target.querySelector(".tf-dfolder")).toBe(node);
  expect(target.querySelector(".tf-dfnote")).toBeNull();
  expect(target.querySelectorAll(".tf-dunder .tf-diff").length).toBe(2);
});

test("a side that failed before turns the bar red, and What it said opens its stderr on the press", async () => {
  const fake = fakeCompareBackend();
  fake.keep(laneOf(SHA), "a");
  fake.slot(laneOf(WT)).failure = { why: "failed", status: 2, stderr: "lowcode: no project file\n" };
  rendering.set(true);
  draw([file("docs/index.md")], fake);
  await settle();
  expect(bar().classList.contains("tf-err")).toBe(true);
  expect(bar().querySelector("[role=status]")!.textContent).toBe("After: the command exited with status 2.");
  expect(target.querySelector(".tf-dffail"), "a look's answer moves nothing under the bar").toBeNull();
  barButton("What it said")!.click();
  flushSync();
  expect(target.querySelector(".tf-dffail pre")!.textContent).toBe("lowcode: no project file\n");
  expect(barButton("Run"), "a failed side wants a run").toBeDefined();
});

test("Run N folder renders presses at most 20 folders, and Stop all stops every lane asked", async () => {
  vi.useFakeTimers();
  const fake = fakeCompareBackend();
  const many: FolderRenderer = { ...SITE, folders: ["docs/*"] };
  const files = Array.from({ length: 21 }, (_, n) => file(`docs/f${String(n).padStart(2, "0")}/a.md`));
  rendering.set(true);
  draw(files, fake, { saved: [many] });
  await settle();
  expect(target.querySelector(".tf-drun-n")!.textContent).toBe("0 of 21 folders rendered");
  expect(runAll().textContent).toBe("Run 20 of 21 folder renders");
  runAll().click();
  await settle();
  const pressed = new Set(fake.asked.filter((a) => a.ask === "again").map((a) => a.folder));
  expect(pressed.size).toBe(20);
  expect(runAll().textContent).toBe("Stop all");
  vi.advanceTimersByTime(2100);
  runAll().click();
  await settle();
  expect(fake.stopped.length).toBe(40);
});

test("Real files puts the folder's files back, and the pill clears it", async () => {
  const fake = fakeCompareBackend();
  rendering.set(true);
  draw([file("docs/index.md")], fake);
  await settle();
  barButton("Real files")!.click();
  flushSync();
  expect(target.querySelector(".tf-dfolder")).toBeNull();
  expect(target.querySelector('.tf-diff[data-file="docs/index.md"]')).not.toBeNull();
  // The glyph groups it again.
  target.querySelector<HTMLButtonElement>(".tf-dfmark")!.click();
  await settle();
  expect(target.querySelector(".tf-dfolder")).not.toBeNull();
});

test("a renderer with no glob is never applied by the toggle, and its glyph groups the folder by hand", async () => {
  const fake = fakeCompareBackend();
  const any: FolderRenderer = { ...SITE, id: "any", name: "Any", folders: [] };
  rendering.set(true);
  draw([file("lib/x.rs")], fake, { saved: [any] });
  await settle();
  expect(target.querySelector(".tf-dfolder")).toBeNull();
  const glyph = target.querySelector<HTMLButtonElement>(".tf-dfmark")!;
  expect(glyph.title).toBe("Compare lib as Any makes it");
  glyph.click();
  await settle();
  expect(target.querySelector(".tf-dfolder")?.getAttribute("data-folder")).toBe("lib");
});

test("a generated JAR that differs is drawn rendered, each side listed from its generated folder by token (2026-11-22)", async () => {
  const fake = fakeCompareBackend();
  fake.keep(laneOf(SHA), "a");
  fake.keep(laneOf(WT), "b");
  fake.answers.set("a\nb", { ...DIFFERS, files: [{ path: "libs/app.jar", change: "modified" as const, before_size: 900, after_size: 950, binary: true }] });
  rendering.set(true);
  const listed: string[] = [];
  const index = (handle: string, crc: number) => ({
    handle, kind: "jar", size: 1, total: 0, packed: 0, names: "a.txt", sizes: [5], packed_sizes: [5], crcs: [crc], flags: [0],
    manifest: [], maven: [], release: null, releases: [], libs: [], prefix: 0, warnings: [],
  });
  draw([file("docs/index.md")], fake, {
    madeArchive: (token: string, path: string) => (listed.push(`${token}:${path}`), Promise.resolve(index(token, token === "a" ? 1 : 2))),
    archives: { nested: vi.fn(), entry: vi.fn(), cls: vi.fn(), decompile: vi.fn(), download: vi.fn(), java: () => Promise.resolve({ java: null, decompiler: null, needs: null, can_decompile: false, reason: null, set_path: null }) },
  });
  await settle();
  await settle();
  const jar = target.querySelector('.tf-dunder .tf-diff[data-file*="app.jar"]') ?? [...target.querySelectorAll(".tf-dunder .tf-diff")].find((c) => c.textContent!.includes("app.jar"))!;
  expect(listed.sort()).toEqual(["a:libs/app.jar", "b:libs/app.jar"]);
  expect([...jar.querySelectorAll(".tf-jdiff-row")].map((r) => r.textContent!.replace(/\s+/g, " ").trim())).toEqual(["M a.txt 5 B"]);
  expect(jar.textContent).not.toContain("A binary file");
});

test("a generated file picked lands with its head at the band's foot, and the rail marks it, a one-line file among others too (2026-11-24)", async () => {
  // Björn, 2026-11-24: "the top line where the current filename is shown hides single line files". A generated file's
  // key holds U+0000, which `CSS.escape` writes as U+FFFD, so the view never found its card: a pick scrolled it into view
  // at the scroller's top, under the band and the head stuck there, and the rail never marked a generated file.
  const page = document.createElement("div");
  page.className = "tf-main";
  target.appendChild(page);
  let top = 0;
  Object.defineProperty(page, "scrollTop", { configurable: true, get: () => top, set: (px: number) => (top = px) });
  Object.defineProperty(page, "scrollHeight", { configurable: true, get: () => 20_000 });
  Object.defineProperty(page, "clientHeight", { configurable: true, get: () => 800 });
  // Where each card starts down the page: the folder's two generated files (one line each), then a file of the change.
  const starts: Record<string, number> = { "docs\u0000index.md": 300, "docs\u0000steps/04-alert.md": 3_000, "lib/x.rs": 3_075 };
  const rect = HTMLElement.prototype.getBoundingClientRect;
  HTMLElement.prototype.getBoundingClientRect = function (this: HTMLElement) {
    const key = this.getAttribute("data-file");
    const [y, height] = this.classList.contains("tf-dband") ? [0, 44] : key !== null && key in starts ? [starts[key]! - top, 63] : [0, 0];
    return { top: y, bottom: y + height, left: 0, right: 0, width: 0, height, x: 0, y, toJSON: () => ({}) } as DOMRect;
  };
  try {
    const fake = fakeCompareBackend();
    fake.keep(laneOf(SHA), "a");
    fake.keep(laneOf(WT), "b");
    fake.answers.set("a\nb", DIFFERS);
    rendering.set(true);
    draw([file("docs/index.md"), file("docs/steps/03-retry.md"), file("lib/x.rs")], fake, {}, page);
    await settle();
    const made = (name: string) => [...target.querySelectorAll<HTMLButtonElement>(".tf-flist button.tf-dmade")].find((b) => b.querySelector(".tf-fname")!.textContent === name)!;
    const pill = () => target.querySelector(".tf-flist [aria-current=true] .tf-fname")?.textContent;
    made("steps/04-alert.md").click();
    flushSync();
    // Its head at the band's foot, not under the band at the scroller's top.
    expect(top).toBe(3_000 - 44);
    page.dispatchEvent(new Event("scroll"));
    flushSync();
    expect(pill()).toBe("steps/04-alert.md");
    // The reader scrolls a little: the file whose head is at the band's foot is still the one marked.
    top += 10;
    page.dispatchEvent(new Event("scroll"));
    flushSync();
    expect(pill()).toBe("steps/04-alert.md");
    // The first generated file, picked from further down, lands the same way.
    made("index.md").click();
    flushSync();
    expect(top).toBe(300 - 44);
  } finally {
    HTMLElement.prototype.getBoundingClientRect = rect;
  }
});

test("the window back in focus looks at each folder's sides again: a side the tool changed under turns stale (2026-11-24)", async () => {
  const fake = fakeCompareBackend();
  fake.keep(laneOf(SHA), "a");
  fake.keep(laneOf(WT), "b");
  fake.answers.set("a\nb", DIFFERS);
  rendering.set(true);
  draw([file("docs/index.md"), file("lib/x.rs")], fake);
  await settle();
  expect(barButton("Run again")).toBeDefined();
  const looks = fake.asked.length;
  // The tool changed: the backend's key for the commit's side moved, and what it holds is from an earlier state.
  fake.keep(laneOf(SHA), "a", false);
  window.dispatchEvent(new Event("focus"));
  await settle();
  expect(fake.asked.slice(looks).map((a) => [a.ask, a.lane])).toEqual([
    ["look", laneOf(SHA)],
    ["look", laneOf(WT)],
  ]);
  expect(bar().querySelector("[role=status]")!.textContent).toBe("Made before Low-code site changed.");
  expect(target.querySelectorAll(".tf-dunder .tf-diff").length, "what was compared stays where it is").toBe(2);
  expect(barButton("Run")).toBeDefined();
});

test("a generated file's last line break ends its last line: no empty line after it, and an added line counts one (2026-11-24)", async () => {
  const fake = fakeCompareBackend();
  fake.keep(laneOf(SHA), "a");
  fake.keep(laneOf(WT), "b");
  fake.answers.set("a\nb", DIFFERS);
  rendering.set(true);
  draw([file("docs/index.md"), file("lib/x.rs")], fake);
  await settle();
  const counts = (name: string) =>
    [...target.querySelectorAll<HTMLElement>(".tf-flist button.tf-dmade")].find((b) => b.querySelector(".tf-fname")!.textContent === name)!.querySelector(".tf-n")!.textContent!.trim();
  expect(counts("index.md")).toBe("+1 −1");
  expect(counts("steps/04-alert.md")).toBe("+1 −0");
});

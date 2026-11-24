/**
 * The XML view (its design, 2026-11-23, sections 5 to 9): the first frame before any worker answer, folding by caret,
 * key and *Levels*, the way down and the element picked, folds kept while the window lives, and a broken file.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

const opened = vi.hoisted(() => [] as { url: string; elsewhere: boolean }[]);
vi.mock("../../../shell/open-web", async (original) => ({
  ...(await original<typeof import("../../../shell/open-web")>()),
  openWeb: (url: string, how: { elsewhere?: boolean } = {}) => opened.push({ url, elsewhere: how.elsewhere === true }),
}));

import TfDocView from "./TfDocView.svelte";
import { forgetFolds } from "./doc.svelte";
import { localDocEngine, type DocEngine } from "../../../render/doc-port";
import type { RenderPort } from "../../../render/port";
import { BROKEN, fixture } from "../../../render/doc-fixture";
import { readXml } from "../../../render/xml-outline";
import { foldTable } from "../../../render/doc-model";
import { foldedTo, visibleLines } from "../../../render/doc-folds";

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;

const quiet: RenderPort = { highlight: () => ({ focus: () => {}, cancel: () => {} }) };
/** An engine that never answers: what the first frame draws alone. */
const never: DocEngine = () => new Promise(() => {});

beforeEach(() => {
  target = document.createElement("div");
  target.className = "tf-fileview";
  document.body.appendChild(target);
  Element.prototype.scrollIntoView = function () {};
  forgetFolds();
  opened.length = 0;
});

afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

const SMALL = ["<?xml version='1.0'?>", "<a xmlns='urn:a'>", "  <b>", "    <c>one</c>", "    <c>two</c>", "  </b>", "  <d>", "    <e/>", "  </d>", "</a>"].join("\n");

function draw(text: string, extra: Record<string, unknown> = {}) {
  view = mount(TfDocView, { target, props: { text, path: "orders/a.xml", repo: "r", render: quiet, engine: localDocEngine(), ...extra } });
  flushSync();
}

async function settle() {
  // The first document loads the outline's module: wait until the head says the outline landed.
  for (let round = 0; round < 60 && words(target.querySelector(".tf-xv-head")) === "Reading…"; round += 1) {
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
  }
  for (let round = 0; round < 6; round += 1) {
    for (let i = 0; i < 10; i += 1) await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
  }
}

const words = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, " ").trim() ?? "";
const rows = () => [...target.querySelectorAll<HTMLElement>(".tf-l[data-n]")].map((r) => Number(r.dataset["n"]));
const row = (n: number) => target.querySelector<HTMLElement>(`.tf-l[data-n="${n}"]`)!;
const head = () => words(target.querySelector(".tf-xv-head"));
function key(k: string, extra: KeyboardEventInit = {}) {
  row(rows()[0]!).dispatchEvent(new KeyboardEvent("keydown", { key: k, bubbles: true, cancelable: true, ...extra }));
  flushSync();
}
const stepper = () => [...target.querySelectorAll<HTMLButtonElement>(".tf-xv-step button")];

test("the first frame is the lines, the caret's column and the rail's head, before any worker answer", () => {
  draw(SMALL, { engine: never });
  expect(rows()).toEqual([1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
  // Every row holds the caret's column from the first frame, empty until the outline lands.
  expect(target.querySelectorAll(".tf-xv-cc")).toHaveLength(10);
  expect(target.querySelector(".tf-xv-caret")).toBeNull();
  expect(head()).toBe("Reading…");
  expect(target.querySelector(".tf-oas-modes .tf-seg")).not.toBeNull();
  expect(words(target.querySelector(".tf-xv-crumbs"))).toBe("Pick a line: the way down to it stands here.");
});

test("the outline lands as paint: carets where something folds, and the head says what the root names", async () => {
  draw(SMALL);
  await settle();
  expect(target.querySelectorAll(".tf-xv-caret")).toHaveLength(3);
  expect(row(2).querySelector(".tf-xv-caret")).not.toBeNull();
  expect(head()).toBe("Not checked Its namespace is urn:a. knobas does not check a document against a schema here.");
});

test("a caret folds; the fold is one line with its count, and a press on the count unfolds", async () => {
  draw(SMALL);
  await settle();
  row(3).querySelector<HTMLButtonElement>(".tf-xv-caret")!.dispatchEvent(new MouseEvent("click", { bubbles: true }));
  flushSync();
  expect(rows()).toEqual([1, 2, 3, 7, 8, 9, 10]);
  expect(words(row(3).querySelector(".tf-xv-fold"))).toBe("2 elements");
  expect(words(row(3).querySelector(".tf-xv-shut"))).toBe("</b>");
  row(3).querySelector<HTMLButtonElement>(".tf-xv-fold")!.dispatchEvent(new MouseEvent("click", { bubbles: true }));
  flushSync();
  expect(rows()).toHaveLength(10);
});

test("← on a leaf goes to its holder's line, and a second ← folds it; Shift → opens everything under", async () => {
  draw(SMALL);
  await settle();
  row(4).dispatchEvent(new MouseEvent("click", { bubbles: true }));
  flushSync();
  key("ArrowLeft");
  expect(row(3).classList.contains("tf-xv-cur")).toBe(true);
  key("ArrowLeft");
  expect(rows()).toEqual([1, 2, 3, 7, 8, 9, 10]);
  key("ArrowLeft");
  expect(row(2).classList.contains("tf-xv-cur")).toBe(true);
  // A shut fold is one line: its closing tag stands on it, dim.
  key("ArrowLeft", { shiftKey: true });
  expect(rows()).toEqual([1, 2]);
  key("ArrowRight", { shiftKey: true });
  expect(rows()).toHaveLength(10);
});

test("Levels: the number is the document's depth when it opens, + is off there, − and the digits fold by level", async () => {
  draw(fixture("order-4711.xml"));
  await settle();
  const [fewer, number, more] = stepper();
  expect(words(number)).toBe("9");
  expect(more!.disabled).toBe(true);
  fewer!.click();
  flushSync();
  expect(words(stepper()[1])).toBe("8");
  // The lines always window, so the rows drawn are the first of what `levels` levels leave (all of them is the model's).
  const table = foldTable(readXml(fixture("order-4711.xml")));
  const leftAt = (levels: number) => [...visibleLines(table, foldedTo(table, levels))];
  const drawnFrom = (levels: number) => {
    const drawn = rows();
    expect(drawn.length).toBeGreaterThan(0);
    expect(drawn).toEqual(leftAt(levels).slice(0, drawn.length));
  };
  expect(leftAt(3)).toHaveLength(96);
  expect(leftAt(2)).toHaveLength(16);
  key("3");
  drawnFrom(3);
  expect(words(stepper()[1])).toBe("3");
  key("+");
  expect(words(stepper()[1])).toBe("4");
  key("-");
  key("-");
  expect(words(stepper()[1])).toBe("2");
  drawnFrom(2);
  stepper()[1]!.click();
  flushSync();
  expect(words(stepper()[1])).toBe("9");
});

test("a picked line draws the way down to it, each name going to its line", async () => {
  draw(SMALL);
  await settle();
  row(5).dispatchEvent(new MouseEvent("click", { bubbles: true }));
  await settle();
  const crumbs = () => [...target.querySelectorAll(".tf-xv-crumbs button")].map((b) => words(b));
  expect(crumbs()).toEqual(["a", "b", "c[2]"]);
  key("ArrowUp");
  await settle();
  expect(crumbs()).toEqual(["a", "b", "c[1]"]);
});

test("folds stay for the same content: drawn folded in the first frame, with no outline", async () => {
  draw(SMALL);
  await settle();
  key("1");
  expect(rows()).toEqual([1, 2, 3, 7, 10]);
  unmount(view!);
  view = null;
  draw(SMALL, { engine: never });
  expect(rows()).toEqual([1, 2, 3, 7, 10]);
});

test("changed content opens unfolded and offers the folds as before", async () => {
  draw(SMALL);
  await settle();
  row(3).querySelector<HTMLButtonElement>(".tf-xv-caret")!.dispatchEvent(new MouseEvent("click", { bubbles: true }));
  flushSync();
  await new Promise((resolve) => setTimeout(resolve, 300));
  await settle();
  unmount(view!);
  view = null;
  draw(SMALL.replace("<a xmlns='urn:a'>", "<a xmlns='urn:a'>\n  <!-- new -->"));
  await settle();
  expect(rows()).toHaveLength(11);
  const offer = [...target.querySelectorAll("button")].find((b) => words(b) === "Fold as before (1)")!;
  offer.click();
  flushSync();
  expect(rows()).toEqual([1, 2, 3, 4, 8, 9, 10, 11]);
});

test("a file that is not well formed opens, folds as far as it reads and says where it breaks", async () => {
  draw(BROKEN);
  await settle();
  expect(rows()).toHaveLength(5);
  expect(head()).toBe("Not well formed At line 4, `</PRICES>` closes nothing open here: `<ITEM>` is open.".replace(/`/g, ""));
  expect(words(target.querySelector(".tf-oas-modes button[aria-pressed='true']"))).toBe("Findings 1");
  target.querySelector<HTMLButtonElement>(".tf-xv-head .tf-oas-ref")!.click();
  flushSync();
  expect(row(4).classList.contains("tf-xv-cur")).toBe(true);
});

const LINKED = ["<a>", "  <site>https://track.example/s/1</site>", "  <img>../media/b.png</img>", "  <doc>README.md</doc>", "</a>"].join("\n");
const files = (paths: string[]) => ({ has: (p: string) => (p === "" ? true : paths.includes(p)), ask: () => {}, open: vi.fn(), bytes: () => new Promise<Uint8Array>(() => {}) });

test("an address is a link that opens through openWeb, a press with ⌘ in the browser; W opens the picked line's", async () => {
  draw(LINKED, { files: files(["media/b.png", "README.md"]) });
  await settle();
  const link = row(2).querySelector<HTMLElement>(".tf-xv-link.tf-xv-web")!;
  expect(link.textContent).toBe("https://track.example/s/1");
  link.dispatchEvent(new MouseEvent("click", { bubbles: true }));
  link.dispatchEvent(new MouseEvent("click", { bubbles: true, metaKey: true }));
  expect(opened).toEqual([
    { url: "https://track.example/s/1", elsewhere: false },
    { url: "https://track.example/s/1", elsewhere: true },
  ]);
  row(2).dispatchEvent(new MouseEvent("click", { bubbles: true }));
  flushSync();
  key("w");
  key("W", { shiftKey: true });
  expect(opened.slice(2)).toEqual([
    { url: "https://track.example/s/1", elsewhere: false },
    { url: "https://track.example/s/1", elsewhere: true },
  ]);
});

test("a path is a link only where the repository holds a file there; an image opens in the file viewer", async () => {
  const repo = files(["media/b.png", "README.md"]);
  draw(LINKED.replace("orders/a.xml", "x"), { files: repo, path: "orders/a.xml" });
  await settle();
  row(4).querySelector<HTMLElement>(".tf-xv-link.tf-xv-path")!.dispatchEvent(new MouseEvent("click", { bubbles: true }));
  expect(repo.open).toHaveBeenCalledWith("README.md");
  row(3).querySelector<HTMLElement>(".tf-xv-link.tf-xv-path")!.dispatchEvent(new MouseEvent("click", { bubbles: true }));
  flushSync();
  expect(document.querySelector(".tf-viewer")).not.toBeNull();
  unmount(view!);
  view = null;
  draw(LINKED, { files: files([]) });
  await settle();
  expect(row(3).querySelector(".tf-xv-link")).toBeNull();
});

test("paths not read yet are asked for after the lines draw, as the repository page's state is written, and become links when they land", async () => {
  // The repository page reads its paths into state when asked: an ask made while the lines render would write state
  // inside the render (Svelte's state_unsafe_mutation, and no lines at all).
  const read = $state<{ paths: string[] | null }>({ paths: null });
  const asked = vi.fn(() => {
    read.paths = ["media/b.png", "README.md"];
  });
  const repo = { has: (p: string) => (read.paths === null ? null : p === "" ? true : read.paths.includes(p)), ask: asked, open: vi.fn(), bytes: () => new Promise<Uint8Array>(() => {}) };
  draw(LINKED, { files: repo });
  await settle();
  expect(asked).toHaveBeenCalledTimes(1);
  await new Promise((resolve) => setTimeout(resolve, 150));
  flushSync();
  expect(row(4).querySelector(".tf-xv-link.tf-xv-path")?.textContent).toBe("README.md");
});

test("a value knobas takes for something carries a chip at its line's end, in the first frame its line is drawn after the outline", async () => {
  const jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiI0NzExIn0.c2lnbmF0dXJl";
  draw(`<a>\n  <token>${jwt}</token>\n  <batch>TW2026A7781KQ40Z9XLMR3</batch>\n</a>`);
  await settle();
  expect(words(row(2).querySelector(".tf-xv-end .tf-xv-chip"))).toBe("JWT");
  expect(row(3).querySelector(".tf-xv-chip")).toBeNull();
});

test("a file written on one line is laid out, says so, and draws as written on a press", async () => {
  const long = `<r>${'<i n="1">one</i>'.repeat(200)}</r>`;
  draw(long);
  await settle();
  expect(words(target.querySelector(".tf-xv-laid"))).toBe("Laid out by knobas");
  // 202 lines laid out, the first of them drawn: an element a line, two spaces a level.
  expect(rows().length).toBeGreaterThan(2);
  expect(rows()).toEqual(rows().map((_, i) => i + 1));
  expect(row(2).querySelector(".tf-lt")?.textContent).toBe('  <i n="1">one</i>');
  [...target.querySelectorAll<HTMLButtonElement>("button")].find((b) => words(b) === "As written")!.click();
  await settle();
  expect(rows()).toEqual([1]);
});

// ---- The schema check (X2, sections 10 to 12) ----

import { localXsdEngine } from "../../../render/xsd-port";
import { familyFiles } from "../../../render/xsd-fixture";

/** The repository: the orders where the seed puts them, openTRANS's schemas where they name them. */
function schemaPort(extra: Partial<{ on: boolean; drop: string }> = {}) {
  const files = familyFiles("opentrans-2.1", "schemas/opentrans/");
  if (extra.drop !== undefined) files.delete(extra.drop);
  const read = vi.fn((p: string) => Promise.resolve(files.get(p) ?? null));
  return { read, paths: () => Promise.resolve([...files.keys(), "orders/order-4711.xml"]), pointed: () => Promise.resolve([]), on: () => Promise.resolve(extra.on ?? true), engine: localXsdEngine() };
}

async function checked() {
  // The check starts after the first frame: wait for its verdict, not the head the outline alone draws.
  const done = () => /departure|Valid|Schema not found here|No schema/.test(words(target.querySelector(".tf-xv-verdict"))) || words(target.querySelector(".tf-xv-head")).includes("Checking is off");
  for (let round = 0; round < 300 && !done(); round += 1) {
    await new Promise((resolve) => setTimeout(resolve, 10));
    flushSync();
  }
  await settle();
}

test("order-4711 opens checked: five departures at the ends of their lines, the head says against what", async () => {
  view = mount(TfDocView, { target, props: { text: fixture("order-4711.xml"), path: "orders/order-4711.xml", repo: "r", render: quiet, engine: localDocEngine(), schema: schemaPort() } });
  flushSync();
  // The first frame has its lines and no mark.
  expect(target.querySelectorAll(".tf-xv-bad").length).toBe(0);
  await checked();
  expect(words(target.querySelector(".tf-xv-verdict"))).toContain("5 departures");
  expect(words(target.querySelector(".tf-xv-head"))).toContain("Against opentrans_2_1.xsd, found beside the file its xsi:schemaLocation names.");
  // The rail opened on Findings, one row a departure, in the document's order.
  const rows = [...target.querySelectorAll(".tf-xv-list .tf-xv-frow")].map((r) => words(r.querySelector(".tf-xv-fline")));
  expect(rows).toEqual(["93", "708", "829", "830", "1,184"]);
});

test("a document whose schema is not in the repository says so, quietly where it names only a namespace", async () => {
  view = mount(TfDocView, { target, props: { text: fixture("order-4711.xml"), path: "orders/order-4711.xml", repo: "r", render: quiet, engine: localDocEngine(), schema: schemaPort({ drop: "schemas/opentrans/opentrans_2_1.xsd" }) } });
  flushSync();
  await checked();
  expect(words(target.querySelector(".tf-xv-verdict"))).toBe("Schema not found here");
  expect(words(target.querySelector(".tf-xv-head"))).toContain("It names ../schemas/opentrans/opentrans_2_1.xsd, and no file is there.");
});

test("checking off: the head says so and nothing is read", async () => {
  const port = schemaPort({ on: false });
  view = mount(TfDocView, { target, props: { text: fixture("order-4711.xml"), path: "orders/order-4711.xml", repo: "r", render: quiet, engine: localDocEngine(), schema: port } });
  flushSync();
  await checked();
  expect(words(target.querySelector(".tf-xv-head"))).toContain("Checking is off for this file.");
  expect(port.read).not.toHaveBeenCalled();
});

test("a document that says nothing of a schema reads nothing and says No schema", async () => {
  const port = schemaPort();
  draw(SMALL.replace(" xmlns='urn:a'", ""), { schema: port });
  await checked();
  expect(words(target.querySelector(".tf-xv-verdict"))).toBe("No schema");
  expect(port.read).not.toHaveBeenCalled();
});

test("N steps to each departure: its line wears the mark and its sentence; Stop checking clears them in one frame", async () => {
  view = mount(TfDocView, { target, props: { text: fixture("order-4711.xml"), path: "orders/order-4711.xml", repo: "r2", render: quiet, engine: localDocEngine(), schema: schemaPort() } });
  flushSync();
  await checked();
  const code = target.querySelector<HTMLElement>(".tf-xv-lines .tf-code")!;
  code.dispatchEvent(new KeyboardEvent("keydown", { key: "n", bubbles: true, cancelable: true }));
  await settle();
  const line = target.querySelector<HTMLElement>('.tf-l[data-n="93"]')!;
  expect(line.classList.contains("tf-xv-bad")).toBe(true);
  expect(words(line.querySelector(".tf-xv-say"))).toBe("DELIVERY_DATE stands where ORDER_DATE is expected.");
  expect(words(target.querySelector(".tf-xv-crumbs"))).toContain("DELIVERY_DATE");
  code.dispatchEvent(new KeyboardEvent("keydown", { key: "n", bubbles: true, cancelable: true }));
  await settle();
  expect(target.querySelector('.tf-xv-frow[aria-selected="true"] .tf-xv-fline')?.textContent).toBe("708");
  // Stop checking this file.
  [...target.querySelectorAll<HTMLButtonElement>("button")].find((b) => words(b) === "Schema ▾")!.click();
  flushSync();
  [...document.querySelectorAll<HTMLButtonElement>('[role="menuitem"]')].find((b) => words(b) === "Stop checking this file")!.click();
  flushSync();
  expect(target.querySelectorAll(".tf-xv-bad").length).toBe(0);
  expect(words(target.querySelector(".tf-xv-head"))).toContain("Checking is off for this file.");
});

/**
 * A PDF's rendered diff (#2282) in the one diff component, mounted as its
 * callers mount it: a changed PDF opens *Rendered*, page rows before and
 * after, the differing ones marked. pdf.js is a stand-in here (a "PDF" is
 * its page prints joined by `|`); `render/pdf-diff.test.ts` runs the
 * comparison on the real pdf.js and headless Chrome witnesses the drawing.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { DiffFile } from "../../ipc/entity";
import { plainPort, type PdfDocument, type RenderPort } from "../../render/port";
import { createDiffLayout } from "./diff-layout.svelte";
import TfDiffView from "./TfDiffView.svelte";

const PDF = (path: string): DiffFile => ({ path, old_path: null, change: "binary", additions: 0, deletions: 0, hunks: [], truncated: false, threads: [] });
const bytes = (...pages: string[]) => new TextEncoder().encode(pages.join("|"));

/** pdf.js stand-in: each page's print is its text; records the pages drawn and how many documents are open. */
function pdfPort() {
  const drawn: string[] = [];
  let open = 0;
  const port: RenderPort = {
    ...plainPort,
    async pdf(data) {
      const text = new TextDecoder().decode(data);
      if (text === "broken") return { error: "Invalid PDF structure." };
      const pages = text.split("|");
      open += 1;
      const pdf: PdfDocument = {
        pages: pages.length,
        page: async (n) => ({
          width: 816,
          height: 1056,
          draw: (_canvas, scale) => (drawn.push(`${pages[n - 1]}@${scale}`), { done: Promise.resolve(), cancel() {} }),
          text: async () => {},
          links: async () => [],
          lines: async () => [pages[n - 1]!],
          print: async () => ({ key: pages[n - 1]!, unsure: false }),
          pixels: async () => null,
        }),
        close: () => void (open -= 1),
      };
      return { pdf };
    },
  };
  return { port, drawn, open: () => open };
}

/** An `IntersectionObserver` that says every page box is near the view, a microtask after it is observed. */
class NearObserver {
  #boxes = new Set<Element>();
  constructor(private readonly callback: (entries: { isIntersecting: boolean; target: Element }[]) => void) {}
  observe(box: Element) {
    this.#boxes.add(box);
    queueMicrotask(() => this.#boxes.has(box) && this.callback([{ isIntersecting: true, target: box }]));
  }
  disconnect() {
    this.#boxes.clear();
  }
}
const strict = globalThis.IntersectionObserver;

let target: HTMLElement;
let views: ReturnType<typeof mount>[] = [];
beforeEach(() => {
  target = document.createElement("div");
  document.body.appendChild(target);
  globalThis.IntersectionObserver = NearObserver as unknown as typeof IntersectionObserver;
});
afterEach(() => {
  views.forEach((view) => unmount(view));
  views = [];
  target.remove();
  globalThis.IntersectionObserver = strict;
});

function draw(files: DiffFile[], sides: Record<string, [Uint8Array | null, Uint8Array | null]>, render: RenderPort) {
  const context = {
    text: () => Promise.resolve(null),
    bytes: (f: DiffFile, side: "old" | "new") => {
      const got = sides[f.path]?.[side === "old" ? 0 : 1];
      return got === undefined ? Promise.reject(new Error("refused")) : Promise.resolve(got);
    },
  };
  views.push(mount(TfDiffView, { target, props: { files, context, render, layout: createDiffLayout(() => null), defaults: { list: [] } } as never }));
  flushSync();
}

const settle = async () => {
  for (let n = 0; n < 30; n += 1) await Promise.resolve();
  flushSync();
};

const card = (path: string) => [...target.querySelectorAll(".tf-diff")].find((el) => el.querySelector(".tf-fh .tf-mono")?.textContent === path)!;
/** Each page row: its pages and its mark, as a reader sees them. */
const rows = (path: string) =>
  [...card(path).querySelectorAll(".tf-pdfd-row")].map((row) => {
    const pages = [...row.querySelectorAll(".tf-pdfd-cell")].map((cell) => cell.querySelector("canvas")?.getAttribute("aria-label")?.replace(`${path} `, "") ?? cell.textContent!.trim());
    return `${row.getAttribute("data-mark")}: ${pages.join(" | ")}${row.querySelector(".tf-pdfd-mark") ? ` [${row.querySelector(".tf-pdfd-mark")!.textContent}]` : ""}`;
  });

test("a changed PDF opens Rendered, page by page side by side, and marks the one changed page only", async () => {
  const pdf = pdfPort();
  draw([PDF("docs/spec.pdf")], { "docs/spec.pdf": [bytes("a", "b", "c"), bytes("a", "B", "c")] }, pdf.port);
  await settle();
  expect([...card("docs/spec.pdf").querySelectorAll(".tf-fh .tf-seg button")].map((b) => `${b.textContent}${b.getAttribute("aria-pressed") === "true" ? "*" : ""}`)).toEqual(["Rendered*", "Bytes"]);
  expect(rows("docs/spec.pdf")).toEqual([
    "same: page 1 before | page 1 after",
    "changed: page 2 before | page 2 after [changed]",
    "same: page 3 before | page 3 after",
  ]);
  expect(card("docs/spec.pdf").querySelector(".tf-pdfd-sum")?.textContent).toBe("3 → 3 pages · 1 changed");
  // Every page drawn on its side, at the diff's zoom.
  expect(pdf.drawn.sort()).toEqual(["B@0.5", "a@0.5", "a@0.5", "b@0.5", "c@0.5", "c@0.5"]);
});

test("a page inserted aligns the rest: only the new page is marked, added, beside no page before", async () => {
  const pdf = pdfPort();
  draw([PDF("docs/handbook.pdf")], { "docs/handbook.pdf": [bytes("a", "b", "c"), bytes("a", "x", "b", "c")] }, pdf.port);
  await settle();
  expect(rows("docs/handbook.pdf")).toEqual([
    "same: page 1 before | page 1 after",
    "added: no page before | page 2 after [added]",
    "same: page 2 before | page 3 after",
    "same: page 3 before | page 4 after",
  ]);
  expect(card("docs/handbook.pdf").querySelector(".tf-pdfd-sum")?.textContent).toBe("3 → 4 pages · 1 added");
});

test("a page taken out is marked removed; an added PDF (no side before) is every page added", async () => {
  const pdf = pdfPort();
  draw([PDF("a.pdf"), PDF("b.pdf")], { "a.pdf": [bytes("a", "b"), bytes("b")], "b.pdf": [null, bytes("a")] }, pdf.port);
  await settle();
  expect(rows("a.pdf")).toEqual(["removed: page 1 before | no page after [removed]", "same: page 2 before | page 1 after"]);
  expect(rows("b.pdf")).toEqual(["added: no page before | page 1 after [added]"]);
});

test("zoom redraws every page at the new scale; leaving Rendered closes both documents", async () => {
  const pdf = pdfPort();
  draw([PDF("docs/spec.pdf")], { "docs/spec.pdf": [bytes("a"), bytes("b")] }, pdf.port);
  await settle();
  expect(pdf.open()).toBe(2);
  pdf.drawn.length = 0;
  card("docs/spec.pdf").querySelector<HTMLButtonElement>("button[aria-label='Zoom in']")!.click();
  await settle();
  expect(pdf.drawn.sort()).toEqual(["a@0.67", "b@0.67"]);
  [...card("docs/spec.pdf").querySelectorAll<HTMLButtonElement>(".tf-fh .tf-seg button")].find((b) => b.textContent === "Bytes")!.click();
  flushSync();
  expect(pdf.open()).toBe(0);
});

test("a side that cannot be read, or is no PDF, says so", async () => {
  const pdf = pdfPort();
  draw([PDF("a.pdf"), PDF("b.pdf")], { "b.pdf": [bytes("a"), new TextEncoder().encode("broken")] }, pdf.port);
  await settle();
  expect(card("a.pdf").querySelector("[role=alert]")?.textContent).toContain("could not be read: refused");
  expect(card("b.pdf").querySelector("[role=alert]")?.textContent).toContain("The after side is not a PDF pdf.js can read: Invalid PDF structure.");
});

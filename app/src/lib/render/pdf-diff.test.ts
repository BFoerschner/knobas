/**
 * The PDF diff (#2282, spec #2234 step 6e) under vitest with the real pdf.js
 * (its legacy build, as `pdf-core.test.ts`): two PDFs compared page by page,
 * each page row marked same, changed, added or removed.
 */
import { resolve } from "node:path";

import { expect, test } from "vitest";

import { openPdf, type PdfLib } from "./pdf-core";
import { alignPages, comparePdfs, pageTextDiff, type PageRow } from "./pdf-diff";
import { fakePdf, type FakePage } from "./pdf-fixture";
import type { PdfDocument } from "./port";
import { pdfAssetOptions } from "./pdf-assets";

const lib = async () => (await import("pdfjs-dist/legacy/build/pdf.mjs")) as unknown as PdfLib;

async function open(bytes: Uint8Array, extra: Record<string, unknown> = {}): Promise<PdfDocument> {
  const answer = await openPdf(await lib(), bytes, extra);
  if (!("pdf" in answer)) throw new Error(`not opened: ${answer.error}`);
  return answer.pdf;
}

async function compare(before: FakePage[], after: FakePage[]): Promise<PageRow[]> {
  const [a, b] = await Promise.all([open(fakePdf({ pages: before })), open(fakePdf({ pages: after }))]);
  try {
    return await comparePdfs(a, b);
  } finally {
    a.close();
    b.close();
  }
}

const page = (title: string, ...more: string[]): FakePage => ({ lines: [title, ...more] });
const FOUR = [page("One", "intro"), page("Two", "retries: 3"), page("Three", "backoff"), page("Four", "end")];

test("a PDF with one page's text changed marks that page, and only that page", async () => {
  const after = FOUR.map((p, i) => (i === 1 ? page("Two", "retries: 5") : p));
  expect(await compare(FOUR, after)).toEqual([
    { old: 1, new: 1, mark: "same" },
    { old: 2, new: 2, mark: "changed" },
    { old: 3, new: 3, mark: "same" },
    { old: 4, new: 4, mark: "same" },
  ]);
});

test("a page inserted in the middle is marked added; the pages after it pair with their old selves, unmarked", async () => {
  const after = [FOUR[0]!, page("New", "inserted"), ...FOUR.slice(1)];
  expect(await compare(FOUR, after)).toEqual([
    { old: 1, new: 1, mark: "same" },
    { old: null, new: 2, mark: "added" },
    { old: 2, new: 3, mark: "same" },
    { old: 3, new: 4, mark: "same" },
    { old: 4, new: 5, mark: "same" },
  ]);
});

test("a page taken out is marked removed", async () => {
  const after = [FOUR[0]!, FOUR[2]!, FOUR[3]!];
  expect(await compare(FOUR, after)).toEqual([
    { old: 1, new: 1, mark: "same" },
    { old: 2, new: null, mark: "removed" },
    { old: 3, new: 2, mark: "same" },
    { old: 4, new: 3, mark: "same" },
  ]);
});

test("a page whose only change is an image is marked changed, though its text is the same", async () => {
  const before = [page("One"), { ...page("Logo"), image: "ff0000" }, page("Three")];
  const after = [page("One"), { ...page("Logo"), image: "0000ff" }, page("Three")];
  expect((await compare(before, after)).map((row) => row.mark)).toEqual(["same", "changed", "same"]);
});

test("a page whose text moved, same words, is changed", async () => {
  const before = [page("One", "a", "b")];
  const after = [page("One", "b", "a")];
  expect((await compare(before, after)).map((row) => row.mark)).toEqual(["changed"]);
});

test("alignment: pages between two matches pair up as changed, the rest are added or removed", () => {
  expect(alignPages(["a", "b", "c"], ["a", "x", "y", "c"])).toEqual([
    { old: 1, new: 1, mark: "same" },
    { old: 2, new: 2, mark: "changed" },
    { old: null, new: 3, mark: "added" },
    { old: 3, new: 4, mark: "same" },
  ]);
  expect(alignPages([], ["a"])).toEqual([{ old: null, new: 1, mark: "added" }]);
  expect(alignPages(["a", "b"], [])).toEqual([
    { old: 1, new: null, mark: "removed" },
    { old: 2, new: null, mark: "removed" },
  ]);
});

test("a page with a drawn annotation stays same when a page inserted before it renumbers the file's objects", async () => {
  const marked: FakePage = { ...page("Three", "reviewed"), box: true };
  const before = [page("One"), marked];
  const after = [page("One"), { ...page("New"), box: true as const }, marked];
  expect((await compare(before, after)).map((row) => row.mark)).toEqual(["same", "added", "same"]);
});

test("a page whose only change is a gradient's colour is marked changed; an unchanged gradient is same", async () => {
  const before = [{ ...page("One"), gradient: "ff0000" }, { ...page("Two"), gradient: "00ff00" }];
  const after = [{ ...page("One"), gradient: "0000ff" }, { ...page("Two"), gradient: "00ff00" }];
  expect((await compare(before, after)).map((row) => row.mark)).toEqual(["changed", "same"]);
});

/** A scan as Chrome's pdf.js sees it: every page's print the same and unsure (its image a bitmap), told apart only by its pixels. */
function scan(...inks: number[]): PdfDocument {
  const pageOf = (ink: number) =>
    ({
      print: async () => ({ key: "img:612x792", unsure: true }),
      pixels: async () => new Uint8ClampedArray([ink, ink, ink, 255]),
    }) as unknown as Awaited<ReturnType<PdfDocument["page"]>>;
  return { pages: inks.length, page: async (n: number) => pageOf(inks[n - 1]!), close() {} } as unknown as PdfDocument;
}

test("a page inserted into a scan, whose prints are all alike, is aligned by the pages' pixels", async () => {
  expect(await comparePdfs(scan(1, 2, 3), scan(1, 9, 2, 3))).toEqual([
    { old: 1, new: 1, mark: "same" },
    { old: null, new: 2, mark: "added" },
    { old: 2, new: 3, mark: "same" },
    { old: 3, new: 4, mark: "same" },
  ]);
  expect((await comparePdfs(scan(1, 2, 3), scan(1, 5, 3))).map((row) => row.mark)).toEqual(["same", "changed", "same"]);
});

/** The text diff of page `n` before and page `m` after (#2315), as `±text` lines, context lines as ` text`. */
async function textDiff(before: FakePage[], n: number | null, after: FakePage[], m: number | null, extra: Record<string, unknown> = {}): Promise<string[]> {
  const [a, b] = await Promise.all([open(fakePdf({ pages: before }), extra), open(fakePdf({ pages: after }), extra)]);
  try {
    const hunks = await pageTextDiff(n === null ? null : await a.page(n), m === null ? null : await b.page(m));
    return hunks.flatMap((hunk) => hunk.lines.map((line) => `${{ add: "+", del: "-", context: " " }[line.kind]}${line.text}`));
  } finally {
    a.close();
    b.close();
  }
}

test("a changed page's text diff says which line changed, beside the lines around it", async () => {
  const after = FOUR.map((p, i) => (i === 1 ? page("Two", "retries: 5") : p));
  expect(await textDiff(FOUR, 2, after, 2)).toEqual([" Two", "-retries: 3", "+retries: 5"]);
});

test("an added page's text diff is every line of it added; a removed page's, every line removed", async () => {
  expect(await textDiff([], null, FOUR, 3)).toEqual(["+Three", "+backoff"]);
  expect(await textDiff(FOUR, 4, [], null)).toEqual(["-Four", "-end"]);
});

test("a page whose only change is an image has no text diff", async () => {
  const before = [{ ...page("Logo", "the mark"), image: "ff0000" }];
  const after = [{ ...page("Logo", "the mark"), image: "0000ff" }];
  expect(await textDiff(before, 1, after, 1)).toEqual([]);
});

/** `node_modules/pdfjs-dist/`, laid out as the app serves it under `/pdfjs/` (#2313); in Node pdf.js reads the files itself. */
const SERVED = { ...pdfAssetOptions(`${resolve(process.cwd(), "node_modules/pdfjs-dist")}/`), useWorkerFetch: false };

test("a Japanese page set in a predefined CMap shows its text in the diff once the CMaps are served, and none without", async () => {
  const before = [{ ...page("Notice"), cjk: ["支払いは毎日です。"] }];
  const after = [{ ...page("Notice"), cjk: ["支払いは毎週です。"] }];
  expect(await textDiff(before, 1, after, 1, SERVED)).toEqual([" Notice", "-支払いは毎日です。", "+支払いは毎週です。"]);
  expect(await textDiff(before, 1, after, 1)).toEqual([]);
});

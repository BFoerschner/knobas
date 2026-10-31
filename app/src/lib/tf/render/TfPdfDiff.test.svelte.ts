/**
 * A PDF's *Rendered* diff with its text diff (#2315): under a changed page
 * row, the lines of its text that changed, read by the real pdf.js (its
 * legacy build, as `render/pdf-diff.test.ts`).
 */
import { mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import { openPdf, type PdfLib } from "../../render/pdf-core";
import { fakePdf, type FakePage } from "../../render/pdf-fixture";
import type { RenderPort } from "../../render/port";
import TfPdfDiff from "./TfPdfDiff.svelte";

const lib = async () => (await import("pdfjs-dist/legacy/build/pdf.mjs")) as unknown as PdfLib;
const port: RenderPort = { highlight: () => ({ focus() {}, cancel() {} }), pdf: async (bytes) => openPdf(await lib(), bytes) };

let target: HTMLElement;
let app: ReturnType<typeof mount> | null = null;
beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
});
afterEach(() => {
  if (app) unmount(app);
  app = null;
  target.remove();
});

const page = (...lines: string[]): FakePage => ({ lines });

async function settled(): Promise<void> {
  for (let i = 0; i < 200 && (target.querySelector(".tf-pdfd") === null || target.querySelector(".tf-pdfd-reading") !== null); i++)
    await new Promise((done) => setTimeout(done, 20));
}

/** Each row's text diff as `±text` lines, by the row's `p. a → b`; a row with none is absent. */
function textByRow(): Record<string, string[]> {
  const out: Record<string, string[]> = {};
  for (const row of target.querySelectorAll(".tf-pdfd-row")) {
    const text = row.querySelector(".tf-pdfd-text");
    if (text === null) continue;
    const lines = [...text.querySelectorAll(".tf-ln")].map((ln) => (ln.classList.contains("tf-add") ? "+" : ln.classList.contains("tf-del") ? "-" : " ") + ln.lastElementChild!.textContent!.slice(2));
    out[row.querySelector(".tf-pdfd-at .tf-mono")!.textContent!] = lines.length > 0 ? lines : [text.textContent!.trim()];
  }
  return out;
}

test("a changed page row shows which lines of its text changed; unchanged rows show no text diff", async () => {
  const before = [page("One", "intro"), page("Two", "retries: 3"), page("Three", "end")];
  const after = [page("One", "intro"), page("Two", "retries: 5"), page("Three", "end")];
  app = mount(TfPdfDiff, { target, props: { before: fakePdf({ pages: before }), after: fakePdf({ pages: after }), path: "spec.pdf", render: port } });
  await settled();
  expect(textByRow()).toEqual({ "p. 2 → 2": [" Two", "-retries: 3", "+retries: 5"] });
});

test("a changed page whose text is the same says so", async () => {
  const before = [{ ...page("Logo"), image: "ff0000" }];
  const after = [{ ...page("Logo"), image: "0000ff" }];
  app = mount(TfPdfDiff, { target, props: { before: fakePdf({ pages: before }), after: fakePdf({ pages: after }), path: "brand.pdf", render: port } });
  await settled();
  expect(textByRow()).toEqual({ "p. 1 → 1": ["Its text is the same: what changed is in the drawing."] });
});

test("a page whose text cannot be read keeps its row and pictures; its text diff says why", async () => {
  const broken: RenderPort = {
    ...port,
    pdf: async (bytes) => {
      const answer = await port.pdf!(bytes);
      if (!("pdf" in answer)) return answer;
      const { pdf } = answer;
      return { pdf: { ...pdf, page: async (n) => ({ ...(await pdf.page(n)), lines: () => Promise.reject(new Error("no text here")) }) } };
    },
  };
  app = mount(TfPdfDiff, { target, props: { before: fakePdf({ pages: [page("A", "1")] }), after: fakePdf({ pages: [page("A", "2")] }), path: "x.pdf", render: broken } });
  await settled();
  expect(textByRow()).toEqual({ "p. 1 → 1": ["Its text could not be read: no text here"] });
  expect(target.querySelectorAll(".tf-pdfd-row canvas")).toHaveLength(2);
});

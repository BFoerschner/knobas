/**
 * The PDF engine (#2260) under vitest with the real pdf.js: its legacy (Node)
 * build, the one the app's modern build shares its parser with. What is
 * witnessed is what the viewer gets back: the pages and their sizes, and
 * the links it may draw, nothing a PDF asks to run among them.
 */
import { expect, test } from "vitest";

import { fakePdf, HOSTILE_PDF, SPEC_PDF } from "../shell/dev/fake/views/pdf";
import { DOCUMENT_OPTIONS, openPdf, type PdfLib } from "./pdf-core";
import type { PdfDocument } from "./port";

const lib = async () => (await import("pdfjs-dist/legacy/build/pdf.mjs")) as unknown as PdfLib;

async function open(bytes: Uint8Array): Promise<PdfDocument> {
  const answer = await openPdf(await lib(), bytes);
  if (!("pdf" in answer)) throw new Error(`not opened: ${answer.error}`);
  return answer.pdf;
}

test("a three-page PDF opens with its page count and each page's size in CSS pixels", async () => {
  const pdf = await open(SPEC_PDF());
  try {
    expect(pdf.pages).toBe(3);
    const page = await pdf.page(2);
    // 612×792 points at 96/72.
    expect([page.width, page.height]).toEqual([816, 1056]);
  } finally {
    pdf.close();
  }
});

test("an external link is a web address and a link to a page is that page, each at its box", async () => {
  const pdf = await open(SPEC_PDF());
  try {
    expect(await (await pdf.page(1)).links()).toEqual([
      { rect: [96, 154.67, 437.33, 26.67], target: { url: "https://psp.example.com/status" } },
    ]);
    expect((await (await pdf.page(2)).links()).map((link) => link.target)).toEqual([{ page: 3 }]);
    expect(await (await pdf.page(3)).links()).toEqual([]);
  } finally {
    pdf.close();
  }
});

test("a hostile PDF opens, and of its links only the web page is drawn: no script, javascript: URI or launch", async () => {
  const pdf = await open(HOSTILE_PDF());
  try {
    expect(pdf.pages).toBe(2);
    expect((await (await pdf.page(1)).links()).map((link) => link.target)).toEqual([{ url: "https://psp.example.com/" }]);
  } finally {
    pdf.close();
  }
});

test("a link to a scheme a Markdown link may not take (mailto, ftp) is not drawn", async () => {
  const pdf = await open(
    fakePdf({
      pages: [
        {
          lines: ["x"],
          links: [
            { rect: [72, 700, 200, 720], to: { uri: "mailto:ops@example.com" } },
            { rect: [72, 650, 200, 670], to: { uri: "ftp://example.com/x" } },
            { rect: [72, 600, 200, 620], to: { uri: "http://example.com/plain" } },
          ],
        },
      ],
    }),
  );
  try {
    expect((await (await pdf.page(1)).links()).map((link) => link.target)).toEqual([{ url: "http://example.com/plain" }]);
  } finally {
    pdf.close();
  }
});

test("bytes that are not a PDF answer an error, not a document", async () => {
  const answer = await openPdf(await lib(), new TextEncoder().encode("not a pdf at all"));
  expect(answer).toEqual({ error: expect.stringMatching(/Invalid PDF/i) });
});

test("the document is opened with eval and XFA forms off", () => {
  expect(DOCUMENT_OPTIONS).toMatchObject({ isEvalSupported: false, enableXfa: false });
});

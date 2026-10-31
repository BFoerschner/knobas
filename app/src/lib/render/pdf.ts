/**
 * pdf.js in the app (#2260): its modern build and its worker, both loaded
 * with the first PDF opened (`port.ts`'s `enginePdf` imports this module).
 * One worker parses every PDF; pdf.js keeps it for the next document.
 *
 * The worker runs the parser over the PDF's bytes. Under Tauri 2.11.5 a
 * worker script gets no CSP (#2312, `notes/diagrams.md`), and it holds no
 * `__TAURI_INTERNALS__` and no invoke key, so it cannot reach IPC. What it
 * sends back is data: operator lists the display layer paints into a canvas
 * in the document, and text the text layer sets as `textContent` (never as
 * markup). See `pdf-core.ts` for what is off and why.
 *
 * The worker also fetches pdf.js's own files from the app's origin, each the
 * first time a PDF needs it (`pdf-assets.ts`, #2313). `useWorkerFetch` is
 * forced on: pdf.js would otherwise fetch them from the document whenever
 * the page is not served over http(s) (`tauri://localhost` on macOS), where
 * the CSP's `connect-src` refuses them.
 */
import { AnnotationMode, getDocument, PDFWorker, TextLayer } from "pdfjs-dist";
import PdfjsWorker from "pdfjs-dist/build/pdf.worker.min.mjs?worker";

import { PDFJS_ASSET_PREFIX, pdfAssetOptions } from "./pdf-assets";
import { openPdf } from "./pdf-core";
import type { PdfAnswer } from "./port";

let worker: PDFWorker | null = null;

export function renderPdf(bytes: Uint8Array): Promise<PdfAnswer> {
  // Handed to each document, so closing one does not stop it (a worker pdf.js made itself would go with its document).
  if (worker === null || worker.destroyed) worker = PDFWorker.create({ port: new PdfjsWorker({ name: "pdf" }) });
  // Absolute: the worker resolves a relative URL against its own script's.
  const base = new URL(`${import.meta.env.BASE_URL}${PDFJS_ASSET_PREFIX}`, globalThis.location.href).href;
  return openPdf({ getDocument, TextLayer, AnnotationMode }, bytes, { ...pdfAssetOptions(base), useWorkerFetch: true, worker });
}

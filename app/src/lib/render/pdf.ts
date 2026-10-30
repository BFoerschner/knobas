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
 */
import { AnnotationMode, getDocument, GlobalWorkerOptions, TextLayer } from "pdfjs-dist";
import PdfjsWorker from "pdfjs-dist/build/pdf.worker.min.mjs?worker";

import { openPdf } from "./pdf-core";
import type { PdfAnswer } from "./port";

export function renderPdf(bytes: Uint8Array): Promise<PdfAnswer> {
  // pdf.js makes one `PDFWorker` of this port and hands it to every document.
  GlobalWorkerOptions.workerPort ??= new PdfjsWorker({ name: "pdf" });
  return openPdf({ getDocument, TextLayer, AnnotationMode }, bytes);
}

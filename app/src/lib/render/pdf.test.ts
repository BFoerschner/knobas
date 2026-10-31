/**
 * What `renderPdf` hands pdf.js beside the bytes (#2313): the served files' options with an absolute base, and
 * `useWorkerFetch` forced on, because pdf.js would turn it off on `tauri://localhost` and fetch from the document,
 * where the CSP's `connect-src` refuses `'self'`.
 */
import { expect, test, vi } from "vitest";

const getDocument = vi.fn((_options: Record<string, unknown>) => ({ promise: Promise.reject(new Error("stub")), destroy: () => Promise.resolve() }));

vi.mock("pdfjs-dist", () => ({
  getDocument,
  TextLayer: class {},
  AnnotationMode: {},
  PDFWorker: { create: () => ({ destroyed: false }) },
}));
vi.mock("pdfjs-dist/build/pdf.worker.min.mjs?worker", () => ({ default: class {} }));

test("the worker fetches pdf.js's files itself, from an absolute URL below the app's base", async () => {
  const { renderPdf } = await import("./pdf");
  await renderPdf(new Uint8Array([1]));
  const options = getDocument.mock.calls[0]![0];
  expect(options.useWorkerFetch).toBe(true);
  expect(options.wasmUrl).toBe(new URL(`${import.meta.env.BASE_URL}pdfjs/wasm/`, globalThis.location.href).href);
  expect(new URL(options.wasmUrl as string).protocol).toMatch(/^https?:$/);
});

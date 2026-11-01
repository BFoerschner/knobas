/**
 * The files pdf.js fetches beside a PDF (#2313): its WASM image decoders and
 * its standard fonts, here read from `pdfjs-dist` on disk, the directories
 * the app serves under `/pdfjs/`. Witnessed with the real pdf.js (legacy build).
 */
import { resolve } from "node:path";

import { expect, test } from "vitest";

import { PDFJS_ASSET_PREFIX, pdfAssetOptions } from "./pdf-assets";
import { DOCUMENT_OPTIONS } from "./pdf-core";
import { SCAN_PDF } from "./pdf-fixture";

/** `node_modules/pdfjs-dist/`, laid out as the app serves it under `/pdfjs/`. */
const DIST = `${resolve(process.cwd(), "node_modules/pdfjs-dist")}/`;

async function firstPage(extra: Record<string, unknown>) {
  const lib = await import("pdfjs-dist/legacy/build/pdf.mjs");
  // In Node pdf.js reads the files itself (`NodeBinaryDataFactory`), so the worker does not fetch them.
  const task = lib.getDocument({ ...DOCUMENT_OPTIONS, ...extra, useWorkerFetch: false, data: SCAN_PDF() });
  const doc = await task.promise;
  const page = await doc.getPage(1);
  const list = await page.getOperatorList();
  const ids = (list.argsArray.flat() as unknown[]).filter((arg): arg is string => typeof arg === "string");
  /** An object the page names, once the worker sent it (`null` if it could not make it). */
  const object = (id: string) =>
    new Promise<Record<string, unknown> | null>((done, fail) => {
      setTimeout(() => fail(new Error(`${id} never arrived`)), 5000);
      (id.startsWith("g_") ? page.commonObjs : page.objs).get(id, done);
    });
  return { ids, object, close: () => task.destroy() };
}

// One test, in this order: pdf.js keeps its decoder settings in the worker's statics, and in Node every
// document shares one in-process worker, so a document opened with the files would let a later one decode too.
test("a JPEG 2000 image is not drawn without the served files, and with openjpeg.wasm its top-left square is green", async () => {
  const bare = await firstPage({});
  try {
    expect(await bare.object(bare.ids.find((id) => id.startsWith("img_"))!)).toBeNull();
  } finally {
    await bare.close();
  }
  const page = await firstPage(pdfAssetOptions(DIST));
  try {
    const image = await page.object(page.ids.find((id) => id.startsWith("img_"))!);
    expect(image).toMatchObject({ width: 16, height: 16 });
    const [r, g, b] = Array.from((image!.data as Uint8Array).slice(0, 3));
    // `1f7444`, within the codec's rounding.
    expect([r, g, b].map((c, i) => Math.abs(c! - [0x1f, 0x74, 0x44][i]!) <= 4)).toEqual([true, true, true]);
  } finally {
    await page.close();
  }
});

test("Helvetica, which the PDF does not embed, is drawn from pdf.js's own font file, not a system font", async () => {
  const page = await firstPage(pdfAssetOptions(DIST));
  try {
    const font = await page.object(page.ids.find((id) => id.startsWith("g_") && id.includes("_f"))!);
    expect([font!.missingFile, font!.systemFontInfo ?? null]).toEqual([false, null]);
  } finally {
    await page.close();
  }
});

test("the app serves every file the options point pdf.js at, the JavaScript decoders WebKit falls back to, and not the scripting sandbox", async () => {
  const { pdfjsAssetFiles } = await import("../../../vite-pdfjs-assets");
  const urls = pdfjsAssetFiles().map((file) => `/${file.url}`);
  const options = pdfAssetOptions(`/${PDFJS_ASSET_PREFIX}`) as Record<string, string>;
  // The names pdf.js 6.4 asks for (`pdf.worker.mjs`): the decoders and ICC profile by name, a font and a CMap by its PDF name.
  expect(urls).toEqual(
    expect.arrayContaining([
      `${options.wasmUrl}openjpeg.wasm`,
      `${options.wasmUrl}jbig2.wasm`,
      `${options.wasmUrl}qcms_bg.wasm`,
      `${options.wasmUrl}openjpeg_nowasm_fallback.js`,
      `${options.wasmUrl}jbig2_nowasm_fallback.js`,
      `${options.iccUrl}CGATS001Compat-v2-micro.icc`,
      `${options.standardFontDataUrl}LiberationSans-Regular.ttf`,
      `${options.standardFontDataUrl}FoxitSymbol.pfb`,
      `${options.cMapUrl}Adobe-Japan1-UCS2.bcmap`,
      `${options.cMapUrl}UniJIS-UTF16-H.bcmap`,
    ]),
  );
  expect(urls.filter((url) => url.endsWith(".bcmap"))).toHaveLength(168);
  // JavaScript only where WebAssembly may fail (#2388): no sandbox, no other script.
  expect(urls.filter((url) => /quickjs|\.js$/.test(url))).toEqual([
    `${options.wasmUrl}jbig2_nowasm_fallback.js`,
    `${options.wasmUrl}openjpeg_nowasm_fallback.js`,
  ]);
});

test("the dev server answers a JavaScript decoder as a module script may be: text/javascript", async () => {
  const { pdfjsAssets } = await import("../../../vite-pdfjs-assets");
  const plugin = pdfjsAssets() as unknown as {
    configResolved: (config: { base: string }) => void;
    configureServer: (server: { middlewares: { use: (fn: (...args: unknown[]) => void) => void } }) => void;
  };
  let middleware: ((...args: unknown[]) => void) | undefined;
  plugin.configResolved({ base: "/" });
  plugin.configureServer({ middlewares: { use: (fn) => (middleware = fn) } });
  const headers: Record<string, string> = {};
  let body: Buffer | undefined;
  const res = { setHeader: (name: string, value: string) => (headers[name] = value), end: (data: Buffer) => (body = data) };
  middleware!({ url: `/${PDFJS_ASSET_PREFIX}wasm/openjpeg_nowasm_fallback.js` }, res, () => {});
  expect([headers["Content-Type"], body!.length > 0]).toEqual(["text/javascript", true]);
});

/**
 * WebKit cannot compile pdf.js 6.4's `openjpeg.wasm` (relaxed SIMD), so in the
 * bundle pdf.js decodes a JPEG 2000 image with `openjpeg_nowasm_fallback.js`,
 * imported from the same `wasmUrl` (#2388). Here pdf.js runs with WebAssembly
 * off against a directory holding only the files the app serves.
 *
 * A file of its own: pdf.js keeps its decoder in the worker's statics, and
 * `pdf-assets.test.ts` opens documents with WebAssembly on.
 */
import { copyFileSync, mkdirSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";

import { expect, test } from "vitest";

import { PDFJS_ASSET_PREFIX, pdfAssetOptions } from "./pdf-assets";
import { DOCUMENT_OPTIONS } from "./pdf-core";
import { SCAN_PDF } from "./pdf-fixture";

test("without WebAssembly, the served JavaScript decoder draws the JPEG 2000 image: its top-left square is green", async () => {
  const { pdfjsAssetFiles } = await import("../../../vite-pdfjs-assets");
  const root = mkdtempSync(join(tmpdir(), "knobas-2388-"));
  for (const file of pdfjsAssetFiles()) {
    const to = join(root, file.url);
    mkdirSync(dirname(to), { recursive: true });
    copyFileSync(file.path, to);
  }
  const lib = await import("pdfjs-dist/legacy/build/pdf.mjs");
  const task = lib.getDocument({
    ...DOCUMENT_OPTIONS,
    ...pdfAssetOptions(`${root}/${PDFJS_ASSET_PREFIX}`),
    useWasm: false,
    useWorkerFetch: false,
    data: SCAN_PDF(),
  });
  try {
    const page = await (await task.promise).getPage(1);
    const list = await page.getOperatorList();
    const id = (list.argsArray.flat() as unknown[]).find((arg) => typeof arg === "string" && arg.startsWith("img_")) as string;
    const image = await new Promise<Record<string, unknown> | null>((done, fail) => {
      setTimeout(() => fail(new Error(`${id} never arrived`)), 10000);
      page.objs.get(id, done);
    });
    expect(image).toMatchObject({ width: 16, height: 16 });
    const [r, g, b] = Array.from((image!.data as Uint8Array).slice(0, 3));
    // `1f7444`, within the codec's rounding.
    expect([r, g, b].map((c, i) => Math.abs(c! - [0x1f, 0x74, 0x44][i]!) <= 4)).toEqual([true, true, true]);
  } finally {
    await task.destroy();
    rmSync(root, { recursive: true, force: true });
  }
});

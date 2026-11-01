/**
 * Serves pdf.js's WASM decoders, standard fonts, CMaps and ICC profile from
 * the app's own origin under `/pdfjs/` (#2313): from `node_modules` on the dev
 * server, and copied into `dist/pdfjs/` by a build, which Tauri embeds and
 * serves like any other asset. Unhashed, because pdf.js asks for each file by
 * its name below one base URL (`src/lib/render/pdf-assets.ts`).
 */
import { readdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import type { Plugin } from "vite";

import { PDFJS_ASSET_DIRS, PDFJS_ASSET_PREFIX } from "./src/lib/render/pdf-assets";

// Not `new URL(…, import.meta.url)`: Vite rewrites that form into an asset URL when vitest loads this file.
const DIST = `${join(dirname(fileURLToPath(import.meta.url)), "node_modules/pdfjs-dist")}/`;

// `text/javascript`: pdf.js imports a `*_nowasm_fallback.js` decoder as a module script.
const TYPES: Record<string, string> = { js: "text/javascript", wasm: "application/wasm", ttf: "font/ttf", pfb: "application/octet-stream" };

/** Each served file: its URL path below the base (`pdfjs/wasm/openjpeg.wasm`) and where it is on disk. */
export function pdfjsAssetFiles(dist = DIST): { url: string; path: string }[] {
  return Object.entries(PDFJS_ASSET_DIRS).flatMap(([dir, keep]) =>
    readdirSync(`${dist}${dir}`)
      .filter((name) => keep.test(name))
      .sort()
      .map((name) => ({ url: `${PDFJS_ASSET_PREFIX}${dir}/${name}`, path: `${dist}${dir}/${name}` })),
  );
}

export function pdfjsAssets(): Plugin {
  let base = "/";
  return {
    name: "knobas:pdfjs-assets",
    configResolved(config) {
      base = config.base;
    },
    configureServer(server) {
      const files = new Map(pdfjsAssetFiles().map((file) => [`${base}${file.url}`, file.path]));
      server.middlewares.use((req, res, next) => {
        const path = files.get((req.url ?? "").split("?")[0]!);
        if (path === undefined) return next();
        res.setHeader("Content-Type", TYPES[path.slice(path.lastIndexOf(".") + 1)] ?? "application/octet-stream");
        res.end(readFileSync(path));
      });
    },
    generateBundle() {
      for (const file of pdfjsAssetFiles()) this.emitFile({ type: "asset", fileName: file.url, source: readFileSync(file.path) });
    },
  };
}

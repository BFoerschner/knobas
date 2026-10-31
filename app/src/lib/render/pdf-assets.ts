/**
 * The files pdf.js's worker fetches beside a PDF (#2313), served from the
 * app's own origin under `/pdfjs/` (`vite-pdfjs-assets.ts` copies them out of
 * `pdfjs-dist`). pdf.js fetches each one only when a PDF needs it: a decoder
 * for the first JPEG 2000, JBIG2 or ICC-coloured image, a font file for the
 * first standard font a PDF does not embed, a CMap for the first text set in
 * a predefined CMap (CJK, mostly).
 */

/** What `vite-pdfjs-assets.ts` serves: each `pdfjs-dist` directory and the files of it the worker may ask for. */
export const PDFJS_ASSET_DIRS = {
  // The scripting sandbox (`quickjs-eval`) is never started (`pdf-core.ts`), and the
  // `*_nowasm_fallback.js` decoders run only when WebAssembly cannot: not served.
  wasm: /^(openjpeg|jbig2|qcms_bg)\.wasm$|^LICENSE_/,
  standard_fonts: /\.(pfb|ttf)$|^LICENSE_/,
  cmaps: /\.bcmap$|^LICENSE$/,
  iccs: /\.icc$|^LICENSE$/,
} as const;

/** Where the app serves `PDFJS_ASSET_DIRS`, below its base URL. */
export const PDFJS_ASSET_PREFIX = "pdfjs/";

/**
 * The `getDocument` options that point pdf.js at the directories below
 * `base` (a URL or path ending in `/`), laid out as `pdfjs-dist` lays them out.
 *
 * `useSystemFonts: false`: a standard font the PDF does not embed is drawn
 * from pdf.js's own file (Foxit, Liberation), the same on every machine,
 * rather than from whatever the system has under that name.
 */
export function pdfAssetOptions(base: string): Record<string, unknown> {
  return {
    wasmUrl: `${base}wasm/`,
    standardFontDataUrl: `${base}standard_fonts/`,
    cMapUrl: `${base}cmaps/`,
    cMapPacked: true,
    iccUrl: `${base}iccs/`,
    useSystemFonts: false,
  };
}

/**
 * The PDF engine (#2260, spec #2234 step 5): pdf.js, scripting and form
 * actions off. It takes the pdf.js module it is handed (the app's modern
 * build with its worker in `pdf.ts`, the legacy build in Node tests) and
 * answers the port's `PdfDocument`.
 *
 * What a PDF can ask to run, and why none of it does:
 *
 * - **JavaScript** (`/OpenAction`, the `/JavaScript` name tree, link and
 *   field actions) runs only in pdf.js's scripting sandbox, which only the
 *   pdf.js *viewer* starts (`PDFScriptingManager`, `pdf.sandbox.mjs`). This
 *   module uses the display layer alone and never loads it.
 * - **Eval**: pdf.js 6.4 compiles nothing with `eval`/`new Function` (the
 *   PostScript-function compiler is gone); `isEvalSupported: false` is passed
 *   anyway, so an upgrade that brings the option back finds it off.
 * - **Forms**: no `AnnotationLayer`, so no field is ever an HTML control.
 *   Pages render with `AnnotationMode.ENABLE`: a field's appearance is
 *   painted into the canvas as pixels. XFA is off (`enableXfa: false`).
 * - **Links** follow the Markdown rules (#2255): http and https go to the
 *   opener, a link to a page scrolls to it, and every other link (a
 *   `javascript:` or `file:` URI, a launch, a named or script action) is
 *   not drawn at all.
 * - **Fetches**: no `standardFontDataUrl`, `cMapUrl`, `iccUrl` or `wasmUrl`,
 *   so the worker fetches nothing; a font a PDF does not embed falls back to
 *   a system font, and a JPEG 2000 or JBIG2 image is not drawn.
 */
import type { PageViewport, PDFDocumentProxy, PDFPageProxy } from "pdfjs-dist";

import type { PdfAnswer, PdfDocument, PdfLink, PdfPage, PdfPrint, PdfTarget } from "./port";

/** The part of pdf.js this module uses. */
export interface PdfLib {
  getDocument: typeof import("pdfjs-dist").getDocument;
  TextLayer: typeof import("pdfjs-dist").TextLayer;
  AnnotationMode: typeof import("pdfjs-dist").AnnotationMode;
}

/** CSS pixels per PDF point. */
const CSS_PER_POINT = 96 / 72;

/**
 * What every document opens with. `isEvalSupported` is not in pdf.js 6.4's
 * types (it compiles nothing), hence the widened type.
 */
export const DOCUMENT_OPTIONS: Record<string, unknown> = {
  isEvalSupported: false,
  enableXfa: false,
  // A PDF's errors are its own; pdf.js says nothing on the console about them.
  verbosity: 0,
};

const round = (n: number) => Math.round(n * 100) / 100;

/** The link schemes a Markdown link may take to the opener (`render/markdown.ts`'s `resolveLink`). */
function webUrl(url: unknown): string | null {
  if (typeof url !== "string") return null;
  try {
    const { protocol } = new URL(url);
    return protocol === "http:" || protocol === "https:" ? url : null;
  } catch {
    return null;
  }
}

/** The 1-based page an explicit or named destination points at, or `null`. */
async function destPage(doc: PDFDocumentProxy, dest: unknown): Promise<number | null> {
  try {
    const explicit = typeof dest === "string" ? await doc.getDestination(dest) : dest;
    if (!Array.isArray(explicit) || explicit.length === 0) return null;
    const [ref] = explicit as [unknown];
    const index =
      typeof ref === "number" ? ref : ref !== null && typeof ref === "object" ? await doc.getPageIndex(ref as never) : null;
    return index === null || index < 0 || index >= doc.numPages ? null : index + 1;
  } catch {
    return null;
  }
}

interface LinkAnnotation {
  subtype?: string;
  rect?: number[];
  url?: unknown;
  dest?: unknown;
}

async function linksOf(doc: PDFDocumentProxy, page: PDFPageProxy, viewport: PageViewport): Promise<PdfLink[]> {
  const annotations = (await page.getAnnotations({ intent: "display" })) as LinkAnnotation[];
  const links: PdfLink[] = [];
  for (const annotation of annotations) {
    if (annotation.subtype !== "Link" || annotation.rect === undefined) continue;
    const url = webUrl(annotation.url);
    const at = url === null && annotation.dest !== undefined ? await destPage(doc, annotation.dest) : null;
    const target: PdfTarget | null = url !== null ? { url } : at !== null ? { page: at } : null;
    if (target === null) continue;
    const [ax, ay, bx, by] = annotation.rect as [number, number, number, number];
    const [x1, y1] = viewport.convertToViewportPoint(ax, ay) as [number, number];
    const [x2, y2] = viewport.convertToViewportPoint(bx, by) as [number, number];
    const left = Math.min(x1, x2);
    const top = Math.min(y1, y2);
    links.push({ rect: [round(left), round(top), round(Math.max(x1, x2) - left), round(Math.max(y1, y2) - top)], target });
  }
  return links;
}

/** A 53-bit hash of `text` (cyrb53), as hex. */
function digest(text: string): string {
  let h1 = 0xdeadbeef;
  let h2 = 0x41c6ce57;
  for (let i = 0; i < text.length; i++) {
    const c = text.charCodeAt(i);
    h1 = Math.imul(h1 ^ c, 2654435761);
    h2 = Math.imul(h2 ^ c, 1597334677);
  }
  h1 = Math.imul(h1 ^ (h1 >>> 16), 2246822507) ^ Math.imul(h2 ^ (h2 >>> 13), 3266489909);
  h2 = Math.imul(h2 ^ (h2 >>> 16), 2246822507) ^ Math.imul(h1 ^ (h1 >>> 13), 3266489909);
  return (4294967296 * (2097151 & h2) + (h1 >>> 0)).toString(16);
}

/** `digest` of a byte array, read 4096 bytes at a time into a string. */
const bytesDigest = (bytes: ArrayLike<number>) => {
  let text = "";
  for (let i = 0; i < bytes.length; i += 4096) text += String.fromCharCode(...Array.from({ length: Math.min(4096, bytes.length - i) }, (_, k) => bytes[i + k]!));
  return digest(text);
};

/** A pdf.js object id an operator names: an image (`img_p0_1`, `g_d0_img_p0_1`), a mask, a font (`g_d0_f1`, `g_d0_s1`), a shading (`pattern_p0_1`). */
const OBJECT_ID = /^(?:g_d\d+_|img_|mask_|pattern_|p\d+_)/;

/** An object reference as pdf.js writes it (`12R`, `12R1`): an annotation's or an optional content group's id, numbered per file. */
const REF = /^\d+R\d*$/;

/** A font as pdf.js hands it over (`FontFaceObject`); its file may already be dropped once the browser loaded it. */
interface FontObject {
  loadedName: string;
  fallbackName?: string;
  bbox?: unknown;
  fontMatrix?: unknown;
  vertical?: boolean;
  italic?: boolean;
  isType3Font?: boolean;
}

/**
 * The page's print (`PdfPrint`): its operator list with every value made
 * comparable across two documents. pdf.js names fonts and images per
 * document (`g_d0_f1` in one is `g_d1_f1` in the other) and annotations by
 * their object number (`12R`, which a re-export renumbers), so a font is
 * its metrics (not its file: a subset font's file changes with every new
 * character anywhere in the document), an image its size and a digest of
 * its pixels, a shading its parameters, an annotation id `ref`; a glyph is
 * its character and width; numbers are rounded to 1/1000 pt. A Type 3 font
 * draws its glyphs from operators the print does not see: unsure.
 *
 * The page's objects are let go after (`cleanup`), so a long document's
 * images are not all held at once; a page drawn later fetches them again.
 */
async function printOf(lib: PdfLib, page: PDFPageProxy): Promise<PdfPrint> {
  const list = await page.getOperatorList({ annotationMode: lib.AnnotationMode.ENABLE });
  let unsure = false;
  const resolved = (id: string): unknown => (page.objs.has(id) ? page.objs.get(id) : page.commonObjs.has(id) ? page.commonObjs.get(id) : undefined);
  const object = (id: string): unknown => {
    const obj = resolved(id);
    if (obj === undefined || obj === null) {
      unsure = true;
      return "?";
    }
    if (typeof obj !== "object") return plain(obj);
    if ("loadedName" in obj) {
      const font = obj as FontObject;
      if (font.isType3Font) unsure = true;
      return { font: plain([font.fallbackName ?? null, font.bbox ?? null, font.fontMatrix ?? null, !!font.vertical, !!font.italic]) };
    }
    const image = obj as { data?: ArrayLike<number> | null; width?: number; height?: number; bitmap?: unknown };
    if (image.data !== undefined && image.data !== null && typeof image.data === "object") return `img:${image.width}x${image.height}:${bytesDigest(image.data)}`;
    if (image.bitmap !== undefined && image.bitmap !== null) {
      unsure = true;
      return `img:${image.width}x${image.height}`;
    }
    // A shading pattern: its parameters (kind, colour stops, geometry), plain data.
    return plain(obj);
  };
  const plain = (value: unknown): unknown => {
    if (typeof value === "number") return Math.round(value * 1000) / 1000;
    if (typeof value === "string") return OBJECT_ID.test(value) ? object(value) : REF.test(value) ? "ref" : value;
    if (value === null || typeof value !== "object") return value ?? null;
    if (ArrayBuffer.isView(value)) return `bytes:${bytesDigest(value as unknown as ArrayLike<number>)}`;
    if (Array.isArray(value)) return value.map(plain);
    if ("unicode" in value) return `${(value as { unicode: string }).unicode}/${plain((value as { width?: number }).width)}`;
    if ("bitmap" in value) {
      unsure = true;
      return "bitmap";
    }
    return Object.fromEntries(Object.entries(value).sort(([a], [b]) => (a < b ? -1 : 1)).map(([k, v]) => [k, plain(v)]));
  };
  const [w, h] = [page.view[2]! - page.view[0]!, page.view[3]! - page.view[1]!];
  const key = digest(JSON.stringify([round(w), round(h), page.rotate, list.fnArray, plain(list.argsArray)]));
  page.cleanup();
  return { key, unsure };
}

function pageOf(lib: PdfLib, doc: PDFDocumentProxy, page: PDFPageProxy): PdfPage {
  const base = page.getViewport({ scale: CSS_PER_POINT });
  return {
    width: round(base.width),
    height: round(base.height),
    draw(canvas, scale) {
      const viewport = page.getViewport({ scale: scale * CSS_PER_POINT });
      const ratio = globalThis.devicePixelRatio || 1;
      canvas.width = Math.round(viewport.width * ratio);
      canvas.height = Math.round(viewport.height * ratio);
      // Through the CSSOM: the app's CSP refuses `style=""` in markup, not this.
      canvas.style.width = `${Math.round(viewport.width)}px`;
      canvas.style.height = `${Math.round(viewport.height)}px`;
      const task = page.render({
        canvas,
        viewport,
        transform: ratio === 1 ? undefined : [ratio, 0, 0, ratio, 0, 0],
        annotationMode: lib.AnnotationMode.ENABLE,
      });
      return { done: task.promise, cancel: () => task.cancel() };
    },
    async text(into, scale) {
      const viewport = page.getViewport({ scale: scale * CSS_PER_POINT });
      into.replaceChildren();
      into.style.setProperty("--total-scale-factor", String(viewport.scale));
      await new lib.TextLayer({ textContentSource: page.streamTextContent(), container: into, viewport }).render();
    },
    links: () => linksOf(doc, page, base),
    print: () => printOf(lib, page),
    async pixels(scale) {
      if (typeof document === "undefined") return null;
      const canvas = document.createElement("canvas");
      const viewport = page.getViewport({ scale: scale * CSS_PER_POINT });
      canvas.width = Math.round(viewport.width);
      canvas.height = Math.round(viewport.height);
      const context = canvas.getContext("2d", { willReadFrequently: true });
      if (context === null) return null;
      await page.render({ canvas, canvasContext: context, viewport, annotationMode: lib.AnnotationMode.ENABLE }).promise;
      const pixels = context.getImageData(0, 0, canvas.width, canvas.height).data;
      page.cleanup();
      return pixels;
    },
  };
}

/** `bytes` opened as a PDF, or the parser's error. The bytes are copied: pdf.js takes the buffer it is handed. */
export async function openPdf(lib: PdfLib, bytes: Uint8Array, extra: Record<string, unknown> = {}): Promise<PdfAnswer> {
  const task = lib.getDocument({ ...DOCUMENT_OPTIONS, ...extra, data: bytes.slice() });
  let doc: PDFDocumentProxy;
  try {
    doc = await task.promise;
  } catch (error) {
    void task.destroy();
    return { error: error instanceof Error ? error.message : String(error) };
  }
  const pdf: PdfDocument = {
    pages: doc.numPages,
    page: async (n) => pageOf(lib, doc, await doc.getPage(n)),
    close: () => void task.destroy(),
  };
  return { pdf };
}

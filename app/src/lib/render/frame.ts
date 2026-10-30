/**
 * The sandboxed frame (spec #2234 step 5, Björn's ruling of 2026-10-30 on
 * #2255): HTML or SVG that a third-party library (Mermaid, later D2 and
 * Graphviz) or a user's command renderer (#2262) hands back is sanitized with
 * DOMPurify, then drawn in an `<iframe sandbox="">`: no script runs in it and
 * its origin is opaque, so nothing in it reaches the app.
 *
 * SVG goes into the frame as an `<img>` of a data URL. An image runs nothing
 * and keeps the SVG's own `<style>`, which matters because a `srcdoc` frame
 * inherits the app's CSP (`style-src 'self'`): styles in the frame's own
 * document would be refused in the bundled app (#2304), styles inside an
 * image are not.
 *
 * {@link svgFrame} also sizes the frame to the drawing: the SVG's root gets
 * its viewBox's size as `width`/`height`, the `<img>` that size, and the
 * frame's body no margin (`marginwidth`/`marginheight`, presentational
 * attributes, which no CSP refuses).
 */
import DOMPurify from "dompurify";

import { base64Of } from "./bytes";
import { sanitizeSvg } from "./image";

/** A frame's document and the size it draws at, in CSS pixels. */
export interface SvgFrame {
  srcdoc: string;
  width: number;
  height: number;
}

/**
 * The frame that draws `markup` (SVG), sanitized and sized to it; `null`
 * when the markup holds no SVG. `dark` says which colour scheme the frame's
 * document takes, so the browser paints no opaque backdrop behind a drawing
 * whose scheme differs from the app's.
 */
export function svgFrame(markup: string, dark = false): SvgFrame | null {
  const svg = sanitizeSvg(markup);
  if (svg === null) return null;
  const [width, height] = sizeOf(svg);
  svg.setAttribute("width", String(width));
  svg.setAttribute("height", String(height));
  // Mermaid's root carries `style="max-width: …"` beside `width="100%"`; the size is now in the attributes.
  svg.removeAttribute("style");
  return { srcdoc: wrap(imageOf(svg, width, height), dark ? "dark" : "light"), width, height };
}

/**
 * The document a sandboxed frame draws for a command renderer's SVG or HTML
 * `body`, sanitized (#2262; `render/command.ts` re-exports it).
 */
export function frameDocument(output: "svg" | "html", body: string): string {
  if (output === "html") {
    // `<style>` is kept: in the app the frame is served under a policy of its own (`frameSource`), where styles
    // apply and nothing they name is fetched. `FORCE_BODY` keeps a `<style>` that opens the output.
    return wrap(
      DOMPurify.sanitize(body, { USE_PROFILES: { html: true, svg: true }, FORBID_TAGS: ["form", "iframe", "object", "embed"], FORCE_BODY: true }),
      null,
    );
  }
  const svg = sanitizeSvg(body);
  return wrap(imageOf(svg, null, null), null);
}

function sizeOf(svg: SVGSVGElement): [number, number] {
  const box = (svg.getAttribute("viewBox") ?? "").trim().split(/[\s,]+/).map(Number);
  if (box.length === 4 && box.every(Number.isFinite) && box[2]! > 0 && box[3]! > 0) return [Math.ceil(box[2]!), Math.ceil(box[3]!)];
  const width = parseFloat(svg.getAttribute("width") ?? "");
  const height = parseFloat(svg.getAttribute("height") ?? "");
  return [Number.isFinite(width) && width > 0 ? Math.ceil(width) : 300, Number.isFinite(height) && height > 0 ? Math.ceil(height) : 150];
}

function imageOf(svg: SVGSVGElement | null, width: number | null, height: number | null): string {
  // XMLSerializer writes the SVG namespace an `<img>` needs.
  const markup = svg === null ? "" : new XMLSerializer().serializeToString(svg);
  const size = width === null || height === null ? "" : ` width="${width}" height="${height}"`;
  return `<img alt=""${size} src="data:image/svg+xml;base64,${base64Of(new TextEncoder().encode(markup))}">`;
}

/** The frame's document; a sized drawing (`scheme` given) takes no body margin, so the frame is the drawing's size. */
function wrap(inner: string, scheme: "light" | "dark" | null): string {
  if (scheme === null) return `<!doctype html><html><head><meta charset="utf-8"></head><body>${inner}</body></html>`;
  return `<!doctype html><html><head><meta charset="utf-8"><meta name="color-scheme" content="${scheme}"></head><body marginwidth="0" marginheight="0">${inner}</body></html>`;
}

/** The part of Tauri's bridge a frame's source needs: `@tauri-apps/api/core`'s, a stand-in in tests. */
export interface FrameBridge {
  isTauri(): boolean;
  convertFileSrc(path: string, protocol: string): string;
}

/** The scheme the backend serves a frame's document from (`knobas_app::renderers::FRAME_SCHEME`). */
export const FRAME_SCHEME = "knobas-render";
/** Past this a document is drawn from `srcdoc` (unstyled in the app) rather than carried in a URL. */
const URL_DOCUMENT_CAP = 1_500_000;

/**
 * Where a frame draws a command renderer's output from (#2304). A `srcdoc` document inherits the app's CSP
 * (`style-src 'self'`), which refuses every `style` attribute and `<style>` in an HTML output. So under the app's
 * bridge the sanitized HTML is served from the backend's `knobas-render` scheme, its own origin, under a policy of
 * its own (inline styles and data images, no script, no request): the document travels in the URL, base64url, and
 * the frame keeps `sandbox=""`. SVG is drawn as an image, which its styles survive, so it stays a `srcdoc`; so does
 * everything in dev and tests, where there is no bridge (and no CSP), and a document too large for a URL.
 */
export function frameSource(output: "svg" | "html", body: string, bridge: FrameBridge): { src: string } | { srcdoc: string } {
  const document = frameDocument(output, body);
  if (output !== "html" || !bridge.isTauri()) return { srcdoc: document };
  const path = base64Of(new TextEncoder().encode(document)).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
  return path.length > URL_DOCUMENT_CAP ? { srcdoc: document } : { src: bridge.convertFileSrc(path, FRAME_SCHEME) };
}

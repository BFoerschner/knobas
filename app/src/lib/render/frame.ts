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
    return wrap(
      DOMPurify.sanitize(body, { USE_PROFILES: { html: true, svg: true }, FORBID_TAGS: ["style", "form", "iframe", "object", "embed"] }),
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

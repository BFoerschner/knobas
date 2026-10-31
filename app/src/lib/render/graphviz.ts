/**
 * Graphviz behind the renderer port (#2259, spec #2234 step 5): viz.js, the
 * WASM build of Graphviz. Only the diagram worker (`diagram.worker.ts`)
 * reaches this module, through a dynamic `import()`, so it and viz.js (WASM
 * inlined) are a chunk of their own that loads with the first Graphviz
 * diagram. The WASM runs in the worker, never in the app's document.
 *
 * - **Colours** are the knobas tokens of the theme drawn now, given as the
 *   graph's, nodes' and edges' default attributes: a colour the source sets
 *   itself wins.
 * - **Errors** are Graphviz's own messages, one per line.
 * - The SVG still goes through the sanitizer and the sandboxed frame
 *   (`frame.ts`) on the way into the page.
 */
import { instance, type Viz } from "@viz-js/viz";

import type { DiagramAnswer, Palette } from "./port";

/** A system font, as Mermaid's: an SVG drawn as an image loads no web font. */
const FONT = "Helvetica,Arial,sans-serif";

let viz: Promise<Viz> | null = null;

/** `code` (DOT) drawn by Graphviz in `palette`: its SVG, or Graphviz's errors. */
export async function renderGraphviz(code: string, palette: Palette): Promise<DiagramAnswer> {
  viz ??= instance();
  const p = palette;
  const result = (await viz).render(code, {
    format: "svg",
    graphAttributes: { bgcolor: p.sheet, color: p.rule, fontcolor: p.ink, fontname: FONT },
    nodeAttributes: { color: p.ink2, fillcolor: p.sheet2, style: "filled", fontcolor: p.ink, fontname: FONT },
    edgeAttributes: { color: p.ink2, fontcolor: p.ink, fontname: FONT },
  });
  if (result.status === "success") return { svg: result.output };
  const errors = result.errors.filter((e) => e.level !== "warning").map((e) => e.message.trim());
  return { error: errors.join("\n") || "Graphviz could not lay this graph out." };
}

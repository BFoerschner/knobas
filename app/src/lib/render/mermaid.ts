/**
 * Mermaid behind the renderer port (#2258, spec #2234 step 5, stories 32–36).
 * This module is only ever reached through a dynamic `import()` from
 * `port.ts`, so it and Mermaid are a chunk of their own that loads when the
 * first diagram is drawn, never with the app.
 *
 * - **Strict security level**: click directives bind nothing and a link's
 *   URL is sanitized; labels are SVG text, never HTML (`htmlLabels: false`).
 *   The SVG then still goes through the sanitizer and the sandboxed frame
 *   (`frame.ts`).
 * - **Colours** are the knobas tokens of the theme drawn now ({@link Palette},
 *   read by `diagram.ts`), on Mermaid's `base` theme.
 * - **One at a time**: Mermaid's configuration is global, so each render
 *   initializes it with its own palette and the renders queue.
 * - **Errors** come back as the parser's message, never as Mermaid's own
 *   error drawing (`suppressErrorRendering`).
 */
import mermaid, { type MermaidConfig } from "mermaid";

import type { DiagramAnswer, Palette } from "./port";

/** The font Mermaid lays text out in and the drawing shows it in. A system font: an SVG drawn as an image loads no web font. */
export const DIAGRAM_FONT = "system-ui, -apple-system, 'Segoe UI', sans-serif";

/** Mermaid's configuration for one render in `palette`. */
export function mermaidConfig(palette: Palette): MermaidConfig {
  const p = palette;
  return {
    startOnLoad: false,
    securityLevel: "strict",
    htmlLabels: false,
    flowchart: { htmlLabels: false },
    suppressErrorRendering: true,
    theme: "base",
    darkMode: p.dark,
    fontFamily: DIAGRAM_FONT,
    themeVariables: {
      darkMode: p.dark,
      fontFamily: DIAGRAM_FONT,
      fontSize: "14px",
      background: p.sheet,
      primaryColor: p.sheet2,
      primaryTextColor: p.ink,
      primaryBorderColor: p.ink2,
      secondaryColor: p.mutedBg,
      secondaryTextColor: p.ink,
      secondaryBorderColor: p.rule,
      tertiaryColor: p.sheet,
      tertiaryTextColor: p.ink,
      tertiaryBorderColor: p.rule,
      mainBkg: p.sheet2,
      nodeBorder: p.ink2,
      lineColor: p.ink2,
      textColor: p.ink,
      titleColor: p.ink,
      edgeLabelBackground: p.sheet,
      clusterBkg: p.sheet,
      clusterBorder: p.rule,
      noteBkgColor: p.mark,
      noteTextColor: p.ink,
      noteBorderColor: p.rule,
      actorBkg: p.sheet2,
      actorBorder: p.ink2,
      actorTextColor: p.ink,
      actorLineColor: p.ink2,
      signalColor: p.ink2,
      signalTextColor: p.ink,
      labelBoxBkgColor: p.sheet2,
      labelBoxBorderColor: p.ink2,
      labelTextColor: p.ink,
      loopTextColor: p.ink,
      activationBkgColor: p.mutedBg,
      activationBorderColor: p.ink2,
      sequenceNumberColor: p.sheet,
    },
  };
}

let queue: Promise<unknown> = Promise.resolve();
let next = 0;

/** `code` drawn by Mermaid in `palette`: its SVG, or the parser's error. */
export function renderMermaid(code: string, palette: Palette): Promise<DiagramAnswer> {
  const run = queue.then(() => drawOnce(code, palette));
  queue = run.catch(() => undefined);
  return run;
}

async function drawOnce(code: string, palette: Palette): Promise<DiagramAnswer> {
  mermaid.initialize(mermaidConfig(palette));
  const id = `tf-mermaid-${(next += 1)}`;
  // Mermaid lays the drawing out in the document to measure its text; this
  // holder is off screen and takes the diagram font from the app's own
  // stylesheet (`.tf-diagram-scratch`), since the bundled app refuses the
  // `<style>` Mermaid writes into it.
  const scratch = document.createElement("div");
  scratch.className = "tf-diagram-scratch";
  document.body.append(scratch);
  try {
    const { svg } = await mermaid.render(id, code, scratch);
    return { svg };
  } catch (error) {
    return { error: error instanceof Error ? error.message : String(error) };
  } finally {
    scratch.remove();
    document.getElementById(id)?.remove();
    document.getElementById(`d${id}`)?.remove();
  }
}

/**
 * D2 behind the renderer port (#2259, spec #2234 step 5): D2's official WASM
 * build (`@terrastruct/d2`). Only the diagram worker (`diagram.worker.ts`)
 * reaches this module, through a dynamic `import()`, so it and D2 are a
 * chunk of their own that loads with the first D2 diagram. The package runs
 * its WASM in a `blob:` worker of its own, nested in the diagram worker, so
 * it is bound by the diagram worker's CSP and not the document's.
 *
 * - **Colours** are the knobas tokens of the theme drawn now, as D2 theme
 *   overrides on its default theme. A theme override the source sets itself
 *   (`vars.d2-config.theme-overrides`) wins, and a source that picks a
 *   theme of its own (`theme-id`) keeps it.
 * - **One at a time**: the package answers one request at a time, so the
 *   renders queue.
 * - **Errors** are D2's messages, one per line, as `line 3:5: …`.
 * - The SVG still goes through the sanitizer and the sandboxed frame
 *   (`frame.ts`) on the way into the page.
 */
import { D2 } from "@terrastruct/d2";

import type { DiagramAnswer, Palette } from "./port";

/** The part of the package's `D2` this module uses. */
export interface D2Engine {
  compile(input: string, options?: Record<string, unknown>): Promise<{ diagram: unknown; renderOptions: Record<string, unknown> }>;
  render(diagram: never, options?: Record<string, unknown>): Promise<string>;
}

/** D2's theme colour slots, filled from the palette (D2's default theme names: N text to background, B shapes, AA/AB accents). */
function overrides(p: Palette): Record<string, string> {
  return {
    n1: p.ink, n2: p.ink2, n3: p.ink2, n4: p.rule, n5: p.rule, n6: p.mutedBg, n7: p.sheet,
    b1: p.ink2, b2: p.ink2, b3: p.rule, b4: p.mutedBg, b5: p.sheet2, b6: p.sheet2,
    aa2: p.ink2, aa4: p.mutedBg, aa5: p.sheet2, ab4: p.mutedBg, ab5: p.sheet2,
  };
}

/** A renderer over the engine `make` gives, made on the first render. */
export function d2Renderer(make: () => D2Engine): (code: string, palette: Palette) => Promise<DiagramAnswer> {
  let engine: D2Engine | null = null;
  let queue: Promise<unknown> = Promise.resolve();
  return (code, palette) => {
    const run = queue.then(() => drawOnce((engine ??= make()), code, palette));
    queue = run.catch(() => undefined);
    return run;
  };
}

async function drawOnce(engine: D2Engine, code: string, palette: Palette): Promise<DiagramAnswer> {
  try {
    const { diagram, renderOptions } = await engine.compile(code, { pad: 16 });
    const own = (renderOptions.themeOverrides ?? {}) as Record<string, string | null>;
    const themed = renderOptions.themeID === 0 || renderOptions.themeID == null;
    const themeOverrides = themed ? { ...overrides(palette), ...Object.fromEntries(Object.entries(own).filter(([, v]) => v != null)) } : own;
    const svg = await engine.render(diagram as never, { ...renderOptions, themeOverrides, noXMLTag: true });
    return { svg };
  } catch (error) {
    return { error: messageOf(error) };
  }
}

/** D2's compile error is a JSON list of `{ errmsg: "index:1:6: …" }`; anything else is its own message. */
function messageOf(error: unknown): string {
  const text = error instanceof Error ? error.message : String(error);
  try {
    const list = JSON.parse(text) as { errmsg?: string }[];
    if (Array.isArray(list) && list.every((e) => typeof e.errmsg === "string")) {
      return list.map((e) => e.errmsg!.replace(/^index:(\d+:\d+): /, "line $1: ")).join("\n");
    }
  } catch {
    // Not JSON: D2's message as it is.
  }
  return text;
}

/** The app's D2: the package's browser build. */
export const renderD2 = d2Renderer(() => new D2() as unknown as D2Engine);

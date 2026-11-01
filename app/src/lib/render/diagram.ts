/**
 * Drawing a diagram into the page (#2258, spec #2234 step 5, stories 32–36):
 * a Markdown fence (`mermaid`) and a diagram file in the file viewer
 * (`*.mmd`, `*.mermaid`) both come here, as do D2 (`d2`, `*.d2`) and
 * Graphviz (`dot`/`graphviz`, `*.dot`, `*.gv`; #2259), engines of their own
 * behind the port's `diagram`.
 *
 * - The source shows until the engine answers; where the port has no engine
 *   (jsdom's plain port) it stays.
 * - A drawing is sanitized and drawn in a sandboxed frame sized to it
 *   (`frame.ts`).
 * - A diagram that does not parse keeps its source and shows the parser's
 *   error above it, never a blank box (story 34).
 * - Colours are the knobas tokens of the theme drawn now; a theme switch
 *   (the `data-theme` the shell sets on `<html>`, or the OS's scheme) draws
 *   the diagram again in the other palette (story 35).
 */
import { svgFrame } from "./frame";
import type { DiagramRequest, Palette, RenderPort } from "./port";

export type DiagramEngine = DiagramRequest["engine"];

const NAMES: Record<DiagramEngine, string> = { mermaid: "Mermaid", d2: "D2", graphviz: "Graphviz" };

/** Whether a renderer id is a diagram engine's, drawn by `drawDiagram` (a rendered diff, #2279). */
export function isDiagramEngine(id: string): id is DiagramEngine {
  return Object.hasOwn(NAMES, id);
}

/** The light tokens (`tf/css/tokens.css`), for a document whose stylesheet holds none (jsdom). */
const LIGHT: Omit<Palette, "dark"> = {
  paper: "#E4E8EC",
  sheet: "#F7F8F9",
  sheet2: "#EEF1F4",
  ink: "#17212B",
  ink2: "#4B5663",
  rule: "#CBD2D9",
  mutedBg: "#E6E9EC",
  mark: "#F6E08F",
};

const TOKENS: Record<keyof typeof LIGHT, string> = {
  paper: "--paper",
  sheet: "--sheet",
  sheet2: "--sheet-2",
  ink: "--ink",
  ink2: "--ink-2",
  rule: "--rule",
  mutedBg: "--muted-bg",
  mark: "--mark",
};

/** The palette `root`'s tokens give now. Dark is a sheet darker than its ink. */
export function paletteOf(root: Element = document.documentElement): Palette {
  const style = getComputedStyle(root);
  const read = {} as Record<keyof typeof LIGHT, string>;
  for (const key of Object.keys(TOKENS) as (keyof typeof LIGHT)[]) read[key] = style.getPropertyValue(TOKENS[key]).trim() || LIGHT[key];
  return { dark: luminance(read.sheet) < luminance(read.ink), ...read };
}

function luminance(colour: string): number {
  const hex = /^#([0-9a-f]{6})$/i.exec(colour)?.[1];
  if (hex === undefined) return 1;
  const [r, g, b] = [0, 2, 4].map((at) => parseInt(hex.slice(at, at + 2), 16));
  return 0.2126 * r! + 0.7152 * g! + 0.0722 * b!;
}

/**
 * Calls `changed` whenever the theme drawn may have changed: the shell's
 * `data-theme` on `<html>`, or the OS's scheme. Returns how to stop.
 */
export function watchTheme(changed: () => void): () => void {
  const html = document.documentElement;
  const observer = new MutationObserver(changed);
  observer.observe(html, { attributes: true, attributeFilter: ["data-theme"] });
  const scheme = typeof window.matchMedia === "function" ? window.matchMedia("(prefers-color-scheme: dark)") : null;
  scheme?.addEventListener?.("change", changed);
  return () => {
    observer.disconnect();
    scheme?.removeEventListener?.("change", changed);
  };
}

/**
 * Draws `code` with `engine` into `into`, and again on every theme switch.
 * `into` keeps what it holds (a fence's `<pre>`) as the source; an empty one
 * gets the source as numbered lines. Returns how to stop.
 */
export function drawDiagram(engine: DiagramEngine, code: string, into: HTMLElement, port: RenderPort): () => void {
  const doc = into.ownerDocument;
  const source = into.childNodes.length > 0 ? [...into.childNodes] : [sourceLines(doc, code)];
  if (into.childNodes.length === 0) into.replaceChildren(...source);
  const diagram = port.diagram?.bind(port);
  if (diagram === undefined) return () => {};
  let live = true;
  let drawn = "";
  let ask = 0;

  const draw = () => {
    const palette = paletteOf();
    const key = JSON.stringify(palette);
    if (key === drawn) return;
    drawn = key;
    const asked = (ask += 1);
    void diagram({ engine, code, palette }).then(
      (answer) => {
        if (!live || asked !== ask) return;
        const frame = "svg" in answer ? svgFrame(answer.svg, palette.dark) : null;
        if (frame !== null) {
          const holder = doc.createElement("div");
          holder.className = "tf-diagram";
          const iframe = doc.createElement("iframe");
          iframe.className = "tf-dframe";
          iframe.setAttribute("sandbox", "");
          // The frame is the drawing's size and never scrolls itself (#2389): WebKit draws a frame's scrollbars in the
          // owner's `::-webkit-scrollbar` style, the app's unstyled track a white bar. A wider drawing scrolls its holder.
          iframe.setAttribute("scrolling", "no");
          iframe.title = `${NAMES[engine]} diagram`;
          iframe.width = String(frame.width);
          iframe.height = String(frame.height);
          iframe.srcdoc = frame.srcdoc;
          holder.append(iframe);
          into.replaceChildren(holder);
        } else {
          into.replaceChildren(failure(doc, engine, "error" in answer ? answer.error : "The drawing came back empty."), ...source);
        }
      },
      (error: unknown) => {
        if (live && asked === ask) into.replaceChildren(failure(doc, engine, error instanceof Error ? error.message : String(error)), ...source);
      },
    );
  };

  draw();
  const unwatch = watchTheme(draw);
  return () => {
    live = false;
    unwatch();
  };
}

function failure(doc: Document, engine: DiagramEngine, message: string): HTMLElement {
  const box = doc.createElement("div");
  box.className = "tf-stepfail";
  box.setAttribute("role", "alert");
  const title = doc.createElement("b");
  title.textContent = `${NAMES[engine]} could not draw this.`;
  const said = doc.createElement("pre");
  said.textContent = message;
  box.append(title, said);
  return box;
}

/** The source as the viewer's numbered lines (`.tf-code`, as the command renderer's fallback draws it). */
function sourceLines(doc: Document, code: string): HTMLElement {
  const pre = doc.createElement("pre");
  pre.className = "tf-code";
  code.split("\n").forEach((line, i) => {
    const row = doc.createElement("div");
    row.className = "tf-l";
    const number = doc.createElement("span");
    number.textContent = String(i + 1);
    const text = doc.createElement("span");
    text.textContent = line || " ";
    row.append(number, text);
    pre.append(row);
  });
  return pre;
}

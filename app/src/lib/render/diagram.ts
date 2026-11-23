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
 *
 * **A diagram keeps its room** (#2510, `docs/decisions/2026-11-20-a-diagram-keeps-its-room.md`): where a text goes on
 * under it (a Markdown fence, its {@link DiagramRoom}), nothing drawn under it moves when its drawing lands.
 *
 * - **Its room outlasts a redraw of its text**: a text drawn again (a read answered again) gives a fitted drawing the
 *   same room, so a return to a kept page moves nothing either.
 * - **Seen before**: the size it drew at is kept between sessions (`diagram-sizes.ts`), and it stands as an empty frame
 *   of that size from the frame its text draws in. A drawing made earlier in this session is kept whole (the latest
 *   64, about 12 MB at most), and drawn at once with no engine asked.
 * - **Never seen, its engine warm** (it has answered in this session; the app warms Mermaid while idle after start,
 *   {@link warmDiagrams}): what stands from it down is held unseen until its drawing is in, at most the text's hold
 *   (`TfMarkdown`, 300 ms). A cold engine cannot answer inside the hold, so nothing is held for it.
 * - **A drawing that lands after the page under it drew** keeps the room its source had: it is drawn whole inside it,
 *   scaled down where it is larger, and takes its own size when the reader presses *Show at full size* or the drawing
 *   (Fable's ruling of 2026-11-20). One that whole would be a speck (under 40%) shows its top at its own size, cut at
 *   the room's foot, with *Show all*. Where nothing seen stands under it (what is under it is held unseen, or below the
 *   window, or it is the last thing its page scrolls), it takes its own size at once. A parser's error that lands
 *   late scrolls inside the room.
 * - Its size is kept the moment it lands, so the next open draws it at full size in its own room.
 */
import { diagramKey, keepDiagramSize, keptDiagramSize } from "./diagram-sizes";
import { svgFrame, type SvgFrame } from "./frame";
import type { DiagramRequest, Palette, RenderPort } from "./port";
import { KNOBAS } from "../tf/theme/app-derive";

export type DiagramEngine = DiagramRequest["engine"];

const NAMES: Record<DiagramEngine, string> = { mermaid: "Mermaid", d2: "D2", graphviz: "Graphviz" };

/** Whether a renderer id is a diagram engine's, drawn by `drawDiagram` (a rendered diff, #2279). */
export function isDiagramEngine(id: string): id is DiagramEngine {
  return Object.hasOwn(NAMES, id);
}

/** knobas Light's tokens (`tf/theme/app-derive.ts`, which `tokens.css` holds equal), for a document whose stylesheet holds none (jsdom). */
const LIGHT: Omit<Palette, "dark"> = {
  paper: KNOBAS.light.paper,
  sheet: KNOBAS.light.sheet,
  sheet2: KNOBAS.light.sheet2,
  ink: KNOBAS.light.ink,
  ink2: KNOBAS.light.ink2,
  rule: KNOBAS.light.rule,
  mutedBg: KNOBAS.light.mutedBg,
  mark: KNOBAS.light.mark,
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
 * `data-theme` on `<html>`, a theme's colours (`data-paint`, which
 * `tf/themes.svelte.ts` moves when a slot changes or an edit is saved), or the
 * OS's scheme. Returns how to stop.
 */
export function watchTheme(changed: () => void): () => void {
  const html = document.documentElement;
  const observer = new MutationObserver(changed);
  observer.observe(html, { attributes: true, attributeFilter: ["data-theme", "data-paint"] });
  const scheme = typeof window.matchMedia === "function" ? window.matchMedia("(prefers-color-scheme: dark)") : null;
  scheme?.addEventListener?.("change", changed);
  return () => {
    observer.disconnect();
    scheme?.removeEventListener?.("change", changed);
  };
}

/** Where a diagram stands in a text that goes on under it (a Markdown fence): what keeping its room needs. */
export interface DiagramRoom {
  /**
   * Holds what stands from `block` down unseen until the diagram's answer is in, at most the text's own hold
   * (`TfMarkdown`); answers how to end it. Without it nothing is held.
   */
  hold?: ((block: HTMLElement) => () => void) | undefined;
  /** Where the block's actions stand (a fence's header, before its *Copy*), for *Show at full size*. */
  actions?: HTMLElement | null;
}

/** The engines that have answered through each port in this session: such an engine can answer inside a hold. */
const warmed = new WeakMap<RenderPort, Set<DiagramEngine>>();

function warmOf(port: RenderPort): Set<DiagramEngine> {
  let warm = warmed.get(port);
  if (warm === undefined) warmed.set(port, (warm = new Set()));
  return warm;
}

/**
 * How many drawings a port's session keeps whole, and how many characters of frame documents in all (about 12 MB of
 * memory): the oldest go first, and a drawing larger than the whole room is not kept.
 */
const DRAWN_KEPT = 64;
const DRAWN_ROOM = 6_000_000;

/** The drawings each port made in this session, by engine, palette, text scale and source. */
const drawings = new WeakMap<RenderPort, Map<string, SvgFrame>>();

function drawnOf(port: RenderPort): Map<string, SvgFrame> {
  let drawn = drawings.get(port);
  if (drawn === undefined) drawings.set(port, (drawn = new Map()));
  return drawn;
}

function keepDrawing(port: RenderPort, key: string, frame: SvgFrame): void {
  const drawn = drawnOf(port);
  drawn.set(key, frame);
  let held = 0;
  for (const kept of drawn.values()) held += kept.srcdoc.length;
  for (const [oldest, kept] of drawn) {
    if (drawn.size <= DRAWN_KEPT && held <= DRAWN_ROOM) break;
    drawn.delete(oldest);
    held -= kept.srcdoc.length;
  }
}

/** The text scale an engine lays its labels out at (`--fs` where the scratch box stands, `mermaid.ts`). */
function textScale(doc: Document): string {
  return doc.body === null ? "" : getComputedStyle(doc.body).getPropertyValue("--fs").trim();
}

/** Whether `el` is painted: not held unseen in its room (`tf-heldbelow`, a hold), itself or inside something that is. */
function seen(el: Element): boolean {
  // jsdom has no `checkVisibility`; there a hold's own class says it.
  return typeof el.checkVisibility === "function" ? el.checkVisibility({ visibilityProperty: true }) : el.closest(".tf-heldbelow") === null;
}

/**
 * Whether anything seen stands under `block` (its box `rect`) in the window (`view` high), so that it would move if
 * the block changed height: what comes after the block, or after anything it is inside of, up to the box that scrolls
 * them, laid out below the block's bottom in its column. What is held unseen there (the text's hold, and what a page
 * holds under the text) is not seen, and what stands outside the scroller does not move with the block.
 */
function seenUnder(block: HTMLElement, rect: DOMRect, view: number): boolean {
  for (let el: Element | null = block; el !== null && el !== block.ownerDocument.body; el = el.parentElement) {
    for (let next = el.nextElementSibling; next !== null; next = next.nextElementSibling) {
      // Unseen first, with no box read: under a hold every block after this one is.
      if (!seen(next)) continue;
      const box = next.getBoundingClientRect();
      if (box.height <= 0 || box.top >= view || box.top < rect.bottom - 1) continue;
      if (box.right <= rect.left || box.left >= rect.right) continue;
      return true;
    }
    const holder = el.parentElement;
    if (holder !== null && /^(auto|scroll)$/.test(getComputedStyle(holder).overflowY)) break;
  }
  return false;
}

/** The room a fitted drawing leaves around itself, top and bottom and at each side (`markdown.css`, `.tf-roomkept`). */
const FIT_PAD_Y = 6;
const FIT_PAD_X = 18;

/**
 * The least a drawing is scaled to stand whole in a kept room. Smaller than this it is a speck (a tall D2 diagram of a
 * four-line source came to 14%), so its top stands in the room at its own size instead, cut at the room's foot.
 */
const FIT_FLOOR = 0.4;

/**
 * Draws `code` with `engine` into `into`, and again on every theme switch.
 * `into` keeps what it holds (a fence's `<pre>`) as the source; an empty one
 * gets the source as numbered lines. With a `room` (a text goes on under it)
 * the diagram keeps its room; without one it takes its drawing's size whenever
 * that lands (a diagram file, nothing under it). Returns how to stop.
 */
export function drawDiagram(engine: DiagramEngine, code: string, into: HTMLElement, port: RenderPort, room: DiagramRoom | null = null): () => void {
  const doc = into.ownerDocument;
  const source = into.childNodes.length > 0 ? [...into.childNodes] : [sourceLines(doc, code)];
  if (into.childNodes.length === 0) into.replaceChildren(...source);
  const diagram = port.diagram?.bind(port);
  if (diagram === undefined) return () => {};
  let live = true;
  let drawn = "";
  let ask = 0;
  /**
   * The frame standing in `into`: a drawing, or the empty stand-in of a kept size. `fitted`: inside a kept room, the
   * drawing of `of` (its own size) drawn at `w` by `h`.
   */
  let box: { iframe: HTMLIFrameElement; w: number; h: number; fitted: boolean; of?: { w: number; h: number } } | null = null;
  /**
   * The height `into` is kept at, once a drawing or an error landed late. A text drawn again hands its fences the
   * rooms they were kept at (`keepFenceRooms`), so a fitted drawing comes back fitted and nothing under it moves.
   */
  let lockedAt: number | null = (into.classList.contains("tf-roomkept") && parseFloat(into.style.height)) || null;
  /** The last drawing, for *Show at full size*. */
  let latest: SvgFrame | null = null;
  let full: HTMLButtonElement | null = null;
  let release: (() => void) | null = null;
  /** The call's own task: the text was put on the page in it, so nothing of it is drawn yet. */
  let first = true;

  const endHold = () => {
    release?.();
    release = null;
  };

  /** A holder and its frame of `w` by `h`, nothing drawn in it yet. */
  const shell = (w: number, h: number): [HTMLElement, HTMLIFrameElement] => {
    const holder = doc.createElement("div");
    holder.className = "tf-diagram";
    const iframe = doc.createElement("iframe");
    iframe.className = "tf-dframe";
    iframe.setAttribute("sandbox", "");
    // The frame is the drawing's size and never scrolls itself (#2389): WebKit draws a frame's scrollbars in the
    // owner's `::-webkit-scrollbar` style, the app's unstyled track a white bar. A wider drawing scrolls its holder.
    iframe.setAttribute("scrolling", "no");
    iframe.title = `${NAMES[engine]} diagram`;
    iframe.width = String(w);
    iframe.height = String(h);
    holder.append(iframe);
    return [holder, iframe];
  };

  /**
   * Whether the block may take another height now with nothing seen moving: no text goes on under it, it is not laid
   * out, or nothing seen stands under it in the window ({@link seenUnder}).
   */
  const free = (): boolean => {
    if (room === null) return true;
    const rect = into.getBoundingClientRect();
    return rect.height <= 0 || !seenUnder(into, rect, doc.defaultView?.innerHeight ?? 0);
  };

  /** Keeps `into` at the height it has now. */
  const lock = (): number => {
    const height = into.getBoundingClientRect().height;
    into.style.height = `${height}px`;
    into.classList.add("tf-roomkept");
    return height;
  };

  const unlock = () => {
    if (lockedAt === null) return;
    lockedAt = null;
    into.style.height = "";
    into.classList.remove("tf-roomkept");
  };

  const unoffer = () => {
    full?.remove();
    full = null;
  };

  /** The drawing at its own size, in a room of its own. */
  const own = (frame: SvgFrame) => {
    unlock();
    unoffer();
    const [holder, iframe] = shell(frame.width, frame.height);
    iframe.srcdoc = frame.srcdoc;
    into.replaceChildren(holder);
    box = { iframe, w: frame.width, h: frame.height, fitted: false };
  };

  /** An empty frame of a kept size, until the drawing lands in it. */
  const standIn = (w: number, h: number) => {
    const [holder, iframe] = shell(w, h);
    into.replaceChildren(holder);
    box = { iframe, w, h, fitted: false };
  };

  /** The reader's press: the drawing at its own size, whatever that moves. */
  const grow = () => {
    if (latest === null) return;
    const after = full !== null && doc.activeElement === full ? full.nextElementSibling : null;
    own(latest);
    if (after instanceof HTMLElement) after.focus();
  };

  /**
   * The drawing inside the room `into` has now: whole, scaled down where it is larger than that room; or, where whole
   * would be under {@link FIT_FLOOR}, its top at its own size (no wider than the room), cut at the room's foot.
   */
  const fit = (frame: SvgFrame) => {
    lockedAt ??= lock();
    const roomH = Math.max(1, lockedAt - 2 * FIT_PAD_Y);
    const roomW = into.clientWidth - 2 * FIT_PAD_X;
    const wide = Math.min(1, roomW > 0 ? roomW / frame.width : 1);
    const whole = Math.min(wide, roomH / frame.height);
    const cut = whole < FIT_FLOOR;
    const scale = cut ? wide : whole;
    const w = Math.max(1, Math.floor(frame.width * scale));
    const h = Math.max(1, Math.floor(frame.height * scale));
    const [holder, iframe] = shell(w, h);
    iframe.srcdoc = scale < 1 ? frame.at(w, h) : frame.srcdoc;
    into.replaceChildren(holder);
    box = { iframe, w, h, fitted: true, of: { w: frame.width, h: frame.height } };
    if (scale >= 1 && !cut) return unoffer();
    holder.classList.add(cut ? "tf-dcut" : "tf-dfit");
    holder.addEventListener("click", grow);
    if (room?.actions == null) return;
    if (full === null) {
      full = doc.createElement("button");
      full.type = "button";
      full.className = "tf-btn tf-sm tf-ghost tf-dfull";
      full.addEventListener("click", grow);
      room.actions.insertBefore(full, room.actions.querySelector(".tf-fence-copy"));
    }
    full.textContent = cut ? "Show all" : "Show at full size";
  };

  /**
   * A drawing that is in: into the frame of its size already standing (a fitted one too, where it fitted a drawing of
   * this size: the same drawing in a theme's new colours swaps its picture and lays nothing out), else its own room
   * where that moves nothing, else fitted.
   */
  const place = (frame: SvgFrame) => {
    latest = frame;
    if (box !== null && !box.fitted && box.w === frame.width && box.h === frame.height) box.iframe.srcdoc = frame.srcdoc;
    else if (box?.of !== undefined && box.of.w === frame.width && box.of.h === frame.height)
      box.iframe.srcdoc = box.w < frame.width || box.h < frame.height ? frame.at(box.w, box.h) : frame.srcdoc;
    // In the call's own task nothing of the text is drawn yet, so the block takes its own size, unless its room came kept.
    else if (first ? lockedAt === null : free()) own(frame);
    else fit(frame);
  };

  const fail = (message: string) => {
    if (lockedAt === null && !free()) lockedAt = lock();
    unoffer();
    box = null;
    into.replaceChildren(failure(doc, engine, message), ...source);
  };

  const draw = () => {
    const palette = paletteOf();
    const scale = textScale(doc);
    const key = `${JSON.stringify(palette)}${scale}`;
    if (key === drawn) return;
    drawn = key;
    const asked = (ask += 1);
    const sizeKey = diagramKey(engine, code, scale);
    const drawnKey = `${engine}\n${key}\n${code}`;
    const ready = drawnOf(port).get(drawnKey);
    if (ready !== undefined) {
      place(ready);
      endHold();
      return;
    }
    if (first && lockedAt === null) {
      const known = keptDiagramSize(sizeKey);
      if (known !== undefined) standIn(known.w, known.h);
      else if (room?.hold !== undefined && warmOf(port).has(engine)) release ??= room.hold(into);
    }
    void diagram({ engine, code, palette }).then(
      (answer) => {
        warmOf(port).add(engine);
        const frame = "svg" in answer ? svgFrame(answer.svg, palette.dark) : null;
        if (frame !== null) {
          // Kept whoever still waits for it: the drawing is paid for, and its place may be opened again.
          keepDrawing(port, drawnKey, frame);
          keepDiagramSize(sizeKey, { w: frame.width, h: frame.height });
        }
        if (!live || asked !== ask) return;
        if (frame !== null) place(frame);
        else fail("error" in answer ? answer.error : "The drawing came back empty.");
        endHold();
      },
      (error: unknown) => {
        if (!live || asked !== ask) return;
        fail(error instanceof Error ? error.message : String(error));
        endHold();
      },
    );
  };

  draw();
  first = false;
  const unwatch = watchTheme(draw);
  return () => {
    live = false;
    unwatch();
    endHold();
  };
}

/** After the window has drawn and is idle; WebKit before 18.4 has no `requestIdleCallback`. Never under jsdom, which draws nothing. */
function whenIdle(go: () => void): void {
  if (typeof navigator !== "undefined" && /jsdom/i.test(navigator.userAgent)) return;
  if (typeof requestIdleCallback === "function") requestIdleCallback(go, { timeout: 4000 });
  else setTimeout(go, 1500);
}

/** One small diagram of each kind a README most often holds: each kind's code is a chunk of its own inside Mermaid. */
export const WARM_DIAGRAMS = ["flowchart LR\n  a --> b", "sequenceDiagram\n  a->>b: c"] as const;

const warming = new WeakSet<RenderPort>();

/**
 * Mermaid loaded and run once while the app is idle after start (#2510), one small diagram an idle moment, so the
 * first diagram a reader opens is drawn by warm code and can land inside the text's hold: cold, Mermaid's chunk and
 * its first drawing took about 400 ms at 1x. Drawn through the port and thrown away, nothing put on the page. D2 and
 * Graphviz stay cold until their first diagram: each is megabytes of WASM few READMEs ask for.
 */
export function warmDiagrams(port: RenderPort, later: (go: () => void) => void = whenIdle): void {
  const diagram = port.diagram?.bind(port);
  if (diagram === undefined || warming.has(port) || typeof document === "undefined") return;
  warming.add(port);
  const next = (at: number) => {
    const code = WARM_DIAGRAMS[at];
    if (code === undefined) return;
    later(() => {
      const done = () => {
        warmOf(port).add("mermaid");
        next(at + 1);
      };
      // A warm-up that fails costs the warmth, never the app.
      void diagram({ engine: "mermaid", code, palette: paletteOf() }).then(done, () => next(at + 1));
    });
  };
  next(0);
}

/** Whether `engine` has answered through `port` in this session: for tests and the probe. */
export function diagramWarm(port: RenderPort, engine: DiagramEngine): boolean {
  return warmOf(port).has(engine);
}

/** Forgets what each port drew and warmed: a new session, for tests. */
export function forgetDiagrams(port: RenderPort): void {
  warmed.delete(port);
  drawings.delete(port);
  warming.delete(port);
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

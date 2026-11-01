/**
 * The renderer port (spec #2234 step 5, #2254): every engine a renderer
 * needs, behind one interface, so jsdom tests use a fake and the app uses the
 * workers. Highlighting and diagrams (Mermaid, #2258; D2 and Graphviz,
 * #2259) so far; pdf.js joins them.
 *
 * Highlighting is a job: `highlight` starts one and returns at once, and the
 * worker answers in pieces through `onLines` (lines `from`… of the text,
 * tokenized), the lines named by `focus` first. `onPlain` says the text has
 * no language to highlight in. Nothing is ever answered synchronously: the
 * caller draws the plain text first.
 */
import { convertFileSrc, isTauri } from "@tauri-apps/api/core";

import { FRAME_SCHEME } from "./frame";

/** A run of a line's text and its kind (`kw`, `str`, `num`, `com`, `fn`, `ty`; `""` is plain ink). */
export type Token = readonly [text: string, kind: string];
export type LineTokens = readonly Token[];

export interface HighlightRequest {
  text: string;
  /** The file's path, whose name picks the language… */
  path?: string;
  /** …or a Markdown fence's language. */
  lang?: string;
  /** The lines in view, `[from, to)` from 0: highlighted first. */
  focus?: [from: number, to: number];
}

export interface HighlightHandlers {
  onLines(from: number, lines: readonly LineTokens[]): void;
  onPlain?(): void;
  /** Every line has its tokens. */
  onDone?(): void;
}

export interface HighlightJob {
  /** The lines in view moved: highlight these next. */
  focus(from: number, to: number): void;
  /** Stop: nothing more is answered. */
  cancel(): void;
}

/**
 * The knobas tokens a diagram is coloured from (`--sheet`, `--ink`, …, as
 * `tf/css/tokens.css` defines them for the theme drawn now), and whether
 * that theme is dark.
 */
export interface Palette {
  dark: boolean;
  paper: string;
  sheet: string;
  sheet2: string;
  ink: string;
  ink2: string;
  rule: string;
  mutedBg: string;
  mark: string;
}

/** A diagram to draw: its engine, its source, the palette to draw it in. */
export interface DiagramRequest {
  engine: "mermaid" | "d2" | "graphviz";
  code: string;
  palette: Palette;
}

/** A drawn diagram's SVG (not yet sanitized), or the parser's error. */
export type DiagramAnswer = { svg: string } | { error: string };

/**
 * Where a link in a PDF goes, once the Markdown link rules (#2255) have had
 * it: a web page (http or https, for the opener) or a page of the document
 * (1-based). Any other link, and every action a PDF asks to run, is not drawn.
 */
export type PdfTarget = { url: string } | { page: number };

/** A link on a page: its box `[left, top, width, height]` in CSS pixels at zoom 1, and where it goes. */
export interface PdfLink {
  rect: [number, number, number, number];
  target: PdfTarget;
}

/** A drawing of a page under way. */
export interface PdfDraw {
  done: Promise<void>;
  cancel(): void;
}

/** One page of an open PDF. Its size is in CSS pixels at zoom 1. */
export interface PdfPage {
  width: number;
  height: number;
  /** Paints the page onto `canvas` at `scale`, sizing the canvas for the screen's pixel ratio. */
  draw(canvas: HTMLCanvasElement, scale: number): PdfDraw;
  /** Fills `into` with the page's text as transparent spans over the drawing, so it can be selected. */
  text(into: HTMLElement, scale: number): Promise<void>;
  links(): Promise<PdfLink[]>;
  /** The page's text as pdf.js reads it (`getTextContent`), one entry per line, blank lines left out: what its text diff compares (#2315). */
  lines(): Promise<string[]>;
  /** What the page is made of, to tell it from another page without drawing it (#2282, `render/pdf-diff.ts`). */
  print(): Promise<PdfPrint>;
  /** The page drawn at `scale` on a canvas off screen, its RGBA pixels; `null` where nothing draws (jsdom). */
  pixels(scale: number): Promise<Uint8ClampedArray | null>;
}

/**
 * A page's print: a digest of its size and its operator list (the text it
 * shows and where, its paths and colours, its images' pixels), equal for two
 * pages that draw the same. `unsure` when the list paints something the
 * digest could not read (an image handed over as a bitmap): the page is
 * then drawn as well, and its pixels join its key (`render/pdf-diff.ts`).
 */
export interface PdfPrint {
  key: string;
  unsure: boolean;
}

export interface PdfDocument {
  pages: number;
  /** Page `n`, 1-based. */
  page(n: number): Promise<PdfPage>;
  /** Lets the document and its worker's copy go. */
  close(): void;
}

/** An open PDF, or why its bytes are not one. */
export type PdfAnswer = { pdf: PdfDocument } | { error: string };

export interface RenderPort {
  highlight(request: HighlightRequest, handlers: HighlightHandlers): HighlightJob;
  /**
   * Draws a diagram (#2258). Absent where no engine runs (jsdom's
   * {@link plainPort}): the diagram's source stays as it is.
   */
  diagram?(request: DiagramRequest): Promise<DiagramAnswer>;
  /**
   * Opens a PDF's bytes with pdf.js (#2260). Absent where no engine runs
   * (jsdom's {@link plainPort}): the viewer says the PDF is not drawn here.
   */
  pdf?(bytes: Uint8Array): Promise<PdfAnswer>;
}

/** What the port sends the diagram worker (D2, Graphviz). */
export type DiagramMessage = { id: number } & DiagramRequest;

/** What the diagram worker answers. */
export interface DiagramReply {
  id: number;
  answer: DiagramAnswer;
}

/**
 * Diagrams drawn in a worker that speaks `DiagramMessage`s, started on the
 * first one. A worker that fails (it could not load, or its WASM was
 * refused) rejects every diagram it holds and is stopped, and the next
 * diagram starts a new one. Stopping it matters: an uncaught error does not
 * end a worker, and one left running keeps D2's WASM heap alive.
 */
export function workerDiagram(
  start: () => Pick<Worker, "postMessage" | "addEventListener" | "terminate">,
): (request: DiagramRequest) => Promise<DiagramAnswer> {
  let worker: ReturnType<typeof start> | null = null;
  let next = 0;
  const live = new Map<number, { resolve: (answer: DiagramAnswer) => void; reject: (error: Error) => void }>();
  return (request) => {
    if (worker === null) {
      const started = (worker = start());
      started.addEventListener("message", (event) => {
        const reply = (event as MessageEvent<DiagramReply>).data;
        live.get(reply.id)?.resolve(reply.answer);
        live.delete(reply.id);
      });
      started.addEventListener("error", (event) => {
        if (worker === started) worker = null;
        started.terminate();
        const error = new Error((event as ErrorEvent).message || "The diagram worker stopped.");
        for (const held of live.values()) held.reject(error);
        live.clear();
      });
    }
    const id = (next += 1);
    const answer = new Promise<DiagramAnswer>((resolve, reject) => live.set(id, { resolve, reject }));
    worker.postMessage({ id, ...request } satisfies DiagramMessage);
    return answer;
  };
}

/** What D2's frame says once it listens (`d2-frame.ts`). */
export interface FrameHello {
  hello: "d2";
}

/**
 * D2's frame (#2381) as a worker {@link workerDiagram} can hold: a hidden
 * `<iframe sandbox="allow-scripts">` at `src` (the backend's `knobas-render`
 * scheme: an opaque origin under a policy of its own, no Tauri globals), on
 * a main thread of its own, since D2's WASM overflows a WebKit worker's
 * stack while it starts.
 *
 * What is sent before the frame says hello waits for it. Only a message whose
 * source is the frame is an answer. A frame that has not said hello within
 * `limit` ms (refused, not built) fails as a worker does: an `error`.
 */
export function frameWorker(src: string, doc: Document = document, limit = 30_000): Pick<Worker, "postMessage" | "addEventListener" | "terminate"> {
  const frame = doc.createElement("iframe");
  frame.setAttribute("sandbox", "allow-scripts");
  frame.hidden = true;
  frame.title = "D2";
  frame.src = src;
  const events = new EventTarget();
  const view = doc.defaultView!;
  let held: unknown[] | null = [];
  const timer = setTimeout(() => {
    events.dispatchEvent(Object.assign(new Event("error"), { message: `D2's frame did not load within ${limit / 1000} s.` }));
  }, limit);
  const heard = (event: MessageEvent) => {
    if (event.source === null || event.source !== frame.contentWindow) return;
    if (held === null) events.dispatchEvent(new MessageEvent("message", { data: event.data as unknown }));
    else if ((event.data as Partial<FrameHello> | null)?.hello === "d2") {
      clearTimeout(timer);
      const waiting = held;
      held = null;
      for (const message of waiting) frame.contentWindow.postMessage(message, "*");
    }
  };
  view.addEventListener("message", heard);
  doc.body.append(frame);
  return {
    postMessage(message: unknown) {
      if (held === null) frame.contentWindow?.postMessage(message, "*");
      else held.push(message);
    },
    addEventListener: events.addEventListener.bind(events) as Worker["addEventListener"],
    terminate() {
      clearTimeout(timer);
      view.removeEventListener("message", heard);
      frame.remove();
    },
  };
}

/** What the port sends the worker. */
export type HighlightMessage =
  | ({ type: "open"; id: number } & HighlightRequest)
  | { type: "focus"; id: number; from: number; to: number }
  | { type: "cancel"; id: number };

/** What the worker answers. */
export type HighlightReply =
  | { type: "lines"; id: number; from: number; lines: LineTokens[] }
  | { type: "plain"; id: number }
  | { type: "done"; id: number };

/** A port over a worker that speaks `HighlightMessage`s, started on the first job. */
export function workerPort(start: () => Pick<Worker, "postMessage" | "addEventListener">): RenderPort {
  let worker: ReturnType<typeof start> | null = null;
  let next = 0;
  const live = new Map<number, HighlightHandlers>();
  const send = (message: HighlightMessage) => worker!.postMessage(message);
  return {
    highlight(request, handlers) {
      if (worker === null) {
        worker = start();
        worker.addEventListener("message", (event) => {
          const reply = (event as MessageEvent<HighlightReply>).data;
          const to = live.get(reply.id);
          if (!to) return;
          if (reply.type === "lines") to.onLines(reply.from, reply.lines);
          else {
            live.delete(reply.id);
            if (reply.type === "plain") to.onPlain?.();
            else to.onDone?.();
          }
        });
      }
      const id = (next += 1);
      live.set(id, handlers);
      send({ type: "open", id, ...request });
      return {
        focus: (from, to) => live.has(id) && send({ type: "focus", id, from, to }),
        cancel: () => {
          if (live.delete(id)) send({ type: "cancel", id });
        },
      };
    },
  };
}

/** A port that highlights nothing: every text stays plain (where there are no workers, as under jsdom). */
export const plainPort: RenderPort = {
  highlight(_request, handlers) {
    let live = true;
    queueMicrotask(() => live && handlers.onPlain?.());
    return { focus() {}, cancel: () => (live = false) };
  },
};

let shared: RenderPort | null = null;

let wasm: ((request: DiagramRequest) => Promise<DiagramAnswer>) | null = null;
let d2: ((request: DiagramRequest) => Promise<DiagramAnswer>) | null = null;

/** D2's frame page on the backend's scheme (`knobas_app::renderers::D2_FRAME_PAGE`). */
const D2_FRAME = "d2-frame.html";

/**
 * Mermaid, loaded on the first diagram: `mermaid.ts` and Mermaid itself are
 * a chunk of their own. It runs on the main thread, since it lays text out
 * in the document.
 *
 * Graphviz is WASM, and runs in the diagram worker (`diagram.worker.ts`),
 * started on the first Graphviz diagram. Its WASM never runs in the
 * document, so the document's CSP needs no `'wasm-unsafe-eval'` (#2259).
 *
 * D2 is WASM too, but its Go runtime overflows a WebKit worker's stack while
 * it starts (#2381), so it needs a main thread: under the app's bridge D2's
 * frame ({@link frameWorker}), under a policy of its own, so the document's
 * stays as it is; in dev and tests, where there is no bridge and no CSP, the
 * page itself.
 */
export function engineDiagram(request: DiagramRequest): Promise<DiagramAnswer> {
  if (request.engine === "mermaid") return import("./mermaid").then(({ renderMermaid }) => renderMermaid(request.code, request.palette));
  if (request.engine === "d2") {
    d2 ??= isTauri()
      ? workerDiagram(() => frameWorker(convertFileSrc(D2_FRAME, FRAME_SCHEME)))
      : ({ code, palette }) => import("./d2").then(({ renderD2 }) => renderD2(code, palette));
    return d2(request);
  }
  wasm ??= workerDiagram(() => new Worker(new URL("./diagram.worker.ts", import.meta.url), { type: "module", name: "diagram" }));
  return wasm(request);
}

/**
 * pdf.js, loaded with the first PDF opened (#2260): `pdf.ts`, pdf.js's
 * display layer and its worker are chunks of their own.
 */
export function enginePdf(bytes: Uint8Array): Promise<PdfAnswer> {
  return import("./pdf").then(({ renderPdf }) => renderPdf(bytes));
}

/** The app's port: Shiki in a module worker, the diagram engines and pdf.js, or {@link plainPort} where there is no `Worker`. */
export function renderPort(): RenderPort {
  shared ??=
    typeof Worker === "undefined"
      ? plainPort
      : {
          ...workerPort(() => new Worker(new URL("./highlight.worker.ts", import.meta.url), { type: "module", name: "highlight" })),
          diagram: engineDiagram,
          pdf: enginePdf,
        };
  return shared;
}

/** One snippet's lines, highlighted, or `null` where its language is unknown (a Markdown fence, #2255). */
export function highlightSnippet(port: RenderPort, text: string, lang: string): Promise<LineTokens[] | null> {
  return new Promise((resolve) => {
    const lines: LineTokens[] = [];
    port.highlight(
      { text, lang },
      {
        onLines: (from, got) => got.forEach((line, i) => (lines[from + i] = line)),
        onPlain: () => resolve(null),
        onDone: () => resolve(lines),
      },
    );
  });
}

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

export interface RenderPort {
  highlight(request: HighlightRequest, handlers: HighlightHandlers): HighlightJob;
  /**
   * Draws a diagram (#2258). Absent where no engine runs (jsdom's
   * {@link plainPort}): the diagram's source stays as it is.
   */
  diagram?(request: DiagramRequest): Promise<DiagramAnswer>;
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
 * refused) rejects every diagram it holds, and the next diagram starts a
 * new one.
 */
export function workerDiagram(
  start: () => Pick<Worker, "postMessage" | "addEventListener">,
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

/**
 * Mermaid, loaded on the first diagram: `mermaid.ts` and Mermaid itself are
 * a chunk of their own. It runs on the main thread, since it lays text out
 * in the document.
 *
 * D2 and Graphviz are WASM, and run in the diagram worker
 * (`diagram.worker.ts`), started on the first of them; each engine is a
 * chunk of its own inside it. Their WASM never runs in the document, so the
 * document's CSP needs no `'wasm-unsafe-eval'` (#2259).
 */
export function engineDiagram(request: DiagramRequest): Promise<DiagramAnswer> {
  if (request.engine === "mermaid") return import("./mermaid").then(({ renderMermaid }) => renderMermaid(request.code, request.palette));
  wasm ??= workerDiagram(() => new Worker(new URL("./diagram.worker.ts", import.meta.url), { type: "module", name: "diagram" }));
  return wasm(request);
}

/** The app's port: Shiki in a module worker and the diagram engines, or {@link plainPort} where there is no `Worker`. */
export function renderPort(): RenderPort {
  shared ??=
    typeof Worker === "undefined"
      ? plainPort
      : {
          ...workerPort(() => new Worker(new URL("./highlight.worker.ts", import.meta.url), { type: "module", name: "highlight" })),
          diagram: engineDiagram,
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

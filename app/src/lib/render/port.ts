/**
 * The renderer port (spec #2234 step 5, #2254): every engine a renderer
 * needs, behind one interface, so jsdom tests use a fake and the app uses the
 * workers. Only highlighting so far; Mermaid, D2, Graphviz and pdf.js join it
 * as methods of their own.
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

export interface RenderPort {
  highlight(request: HighlightRequest, handlers: HighlightHandlers): HighlightJob;
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

/** The app's port: Shiki in a module worker, or {@link plainPort} where there is no `Worker`. */
export function renderPort(): RenderPort {
  shared ??=
    typeof Worker === "undefined"
      ? plainPort
      : workerPort(() => new Worker(new URL("./highlight.worker.ts", import.meta.url), { type: "module", name: "highlight" }));
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

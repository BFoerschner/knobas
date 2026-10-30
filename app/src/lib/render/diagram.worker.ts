/// <reference lib="webworker" />
/**
 * The diagram worker (#2259): D2 and Graphviz, whose engines are WASM, on a
 * thread of their own. Each engine is imported on its first diagram, so a
 * page with a DOT graph never loads D2, and the reverse.
 *
 * Why a worker and not the document: compiling WASM needs
 * `'wasm-unsafe-eval'`, and D2 also builds its layout engine with
 * `new Function` (`'unsafe-eval'`). The app's CSP (`tauri.conf.json`) is
 * sent as a header on `.html` assets only, so this worker, loaded from the
 * app's own origin, runs under no policy of its own, and the document's
 * policy stays as strict as it was. The SVG that comes back is sanitized and
 * framed by the page (`frame.ts`).
 */
import type { DiagramAnswer, DiagramMessage, DiagramReply } from "./port";

async function draw({ engine, code, palette }: DiagramMessage): Promise<DiagramAnswer> {
  try {
    if (engine === "d2") return await (await import("./d2")).renderD2(code, palette);
    if (engine === "graphviz") return await (await import("./graphviz")).renderGraphviz(code, palette);
    return { error: `No ${engine} engine in the diagram worker.` };
  } catch (error) {
    return { error: error instanceof Error ? error.message : String(error) };
  }
}

self.addEventListener("message", (event: MessageEvent<DiagramMessage>) => {
  void draw(event.data).then((answer) => self.postMessage({ id: event.data.id, answer } satisfies DiagramReply));
});

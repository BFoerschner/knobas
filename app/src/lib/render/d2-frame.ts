/**
 * D2's frame (#2381), the page `d2-frame.html` loads: D2 drawn on this
 * frame's main thread, since its Go WASM overflows a WebKit worker's stack
 * while it starts. The frame is sandboxed to scripts (an opaque origin) and
 * served by the backend under a policy of its own that allows WASM and
 * `new Function`; the app's document keeps its own (`render/port.ts`'s
 * `frameWorker`).
 *
 * It speaks the diagram worker's protocol with its parent: a
 * `DiagramMessage` in, a `DiagramReply` out, and a hello once it listens.
 * Only its parent is heard. D2 loads on the first diagram.
 */
import type { DiagramAnswer, DiagramMessage, DiagramReply, FrameHello } from "./port";

const parent = window.parent;

async function draw({ engine, code, palette }: DiagramMessage): Promise<DiagramAnswer> {
  if (engine !== "d2") return { error: `No ${engine} engine in D2's frame.` };
  try {
    return await (await import("./d2")).renderD2(code, palette);
  } catch (error) {
    return { error: error instanceof Error ? error.message : String(error) };
  }
}

window.addEventListener("message", (event: MessageEvent<DiagramMessage>) => {
  if (event.source !== parent) return;
  const { id } = event.data;
  // The parent's origin is the app's; this frame's is opaque, so it can name no target but `*`. Only the SVG goes back.
  void draw(event.data).then((answer) => parent.postMessage({ id, answer } satisfies DiagramReply, "*"));
});
parent.postMessage({ hello: "d2" } satisfies FrameHello, "*");

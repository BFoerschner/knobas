/**
 * The diagram worker's side of the port (#2259): Graphviz runs its WASM in a
 * worker, and D2 in a frame held as one (#2381), started on the first such
 * diagram, never before, and each answer reaches the request it belongs to.
 */
import { expect, test } from "vitest";

import { frameWorker, workerDiagram, type DiagramMessage, type DiagramReply, type Palette } from "./port";

const PALETTE: Palette = {
  dark: false, paper: "#E4E8EC", sheet: "#F7F8F9", sheet2: "#EEF1F4", ink: "#17212B", ink2: "#4B5663",
  rule: "#CBD2D9", mutedBg: "#E6E9EC", mark: "#F6E08F",
};

/** A worker held by hand: what it was sent, and a way to answer or to fail. */
function heldWorker() {
  const sent: DiagramMessage[] = [];
  const listeners: Record<string, ((event: unknown) => void)[]> = {};
  let starts = 0;
  let stopped = 0;
  const worker = {
    terminate: () => {
      stopped += 1;
    },
    postMessage: (message: DiagramMessage) => sent.push(message),
    addEventListener: (type: string, listener: (event: unknown) => void) => (listeners[type] ??= []).push(listener),
  };
  return {
    sent,
    get starts() {
      return starts;
    },
    get stopped() {
      return stopped;
    },
    start: () => {
      starts += 1;
      return worker;
    },
    answer: (reply: DiagramReply) => listeners.message?.forEach((l) => l({ data: reply })),
    fail: (message: string) => listeners.error?.forEach((l) => l({ message })),
  };
}

test("the worker starts on the first diagram, once, and each answer settles its own request", async () => {
  const held = heldWorker();
  const draw = workerDiagram(held.start);
  expect(held.starts).toBe(0);
  const d2 = draw({ engine: "d2", code: "a -> b", palette: PALETTE });
  const dot = draw({ engine: "graphviz", code: "digraph { a }", palette: PALETTE });
  expect(held.starts).toBe(1);
  expect(held.sent.map((m) => [m.engine, m.code])).toEqual([["d2", "a -> b"], ["graphviz", "digraph { a }"]]);
  const [first, second] = held.sent;
  held.answer({ id: second!.id, answer: { error: "syntax error in line 1" } });
  held.answer({ id: first!.id, answer: { svg: "<svg></svg>" } });
  expect(await d2).toEqual({ svg: "<svg></svg>" });
  expect(await dot).toEqual({ error: "syntax error in line 1" });
});

test("a worker that fails rejects what it holds, is stopped, and the next diagram starts a new one", async () => {
  const held = heldWorker();
  const draw = workerDiagram(held.start);
  const lost = draw({ engine: "d2", code: "a", palette: PALETTE });
  held.fail("WebAssembly.instantiate(): refused by CSP");
  await expect(lost).rejects.toThrow("refused by CSP");
  expect(held.stopped).toBe(1);
  void draw({ engine: "d2", code: "b", palette: PALETTE });
  expect(held.starts).toBe(2);
});

/**
 * D2's frame as the diagram worker (#2381): a hidden `sandbox="allow-scripts"`
 * frame from the backend's scheme, which says hello once it listens. What is
 * sent before that waits for it; only what the frame itself posts is an
 * answer; a frame that never says hello is a failed worker.
 */
function framed(limit = 1000) {
  const worker = frameWorker("knobas-render://localhost/d2-frame.html", document, limit);
  const frame = document.querySelector<HTMLIFrameElement>("iframe[src='knobas-render://localhost/d2-frame.html']")!;
  const into: unknown[] = [];
  frame.contentWindow!.postMessage = ((message: unknown) => into.push(message)) as Window["postMessage"];
  const heard: unknown[] = [];
  worker.addEventListener("message", (event) => heard.push((event as MessageEvent).data));
  const from = (source: Window | null, data: unknown) => window.dispatchEvent(new MessageEvent("message", { data, source }));
  return { worker, frame, into, heard, from };
}

test("D2's frame is hidden and sandboxed to scripts alone, and holds what is sent until it says hello", () => {
  const { worker, frame, into, heard, from } = framed();
  try {
    expect(frame.getAttribute("sandbox")).toBe("allow-scripts");
    expect(frame.hidden).toBe(true);
    const ask = { id: 1, engine: "d2", code: "a -> b", palette: PALETTE } satisfies DiagramMessage;
    worker.postMessage(ask);
    expect(into).toEqual([]);

    from(frame.contentWindow, { hello: "d2" });
    expect(into).toEqual([ask]);
    expect(heard).toEqual([]);

    // Only the frame's own messages are answers.
    from(window, { id: 1, answer: { svg: "<svg>forged</svg>" } });
    expect(heard).toEqual([]);
    from(frame.contentWindow, { id: 1, answer: { svg: "<svg/>" } });
    expect(heard).toEqual([{ id: 1, answer: { svg: "<svg/>" } }]);
  } finally {
    worker.terminate();
  }
  expect(frame.isConnected).toBe(false);
});

test("a D2 frame that never says hello fails as a worker does", async () => {
  const { worker, frame } = framed(20);
  const failed = new Promise<string>((resolve) => worker.addEventListener("error", (event) => resolve((event as ErrorEvent).message)));
  expect(await failed).toBe("D2's frame did not load within 0.02 s.");
  worker.terminate();
  expect(frame.isConnected).toBe(false);
});

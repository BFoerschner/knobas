/**
 * The diagram worker's side of the port (#2259): D2 and Graphviz run their
 * WASM in a worker started on the first such diagram, never before, and each
 * answer reaches the request it belongs to.
 */
import { expect, test } from "vitest";

import { workerDiagram, type DiagramMessage, type DiagramReply, type Palette } from "./port";

const PALETTE: Palette = {
  dark: false, paper: "#E4E8EC", sheet: "#F7F8F9", sheet2: "#EEF1F4", ink: "#17212B", ink2: "#4B5663",
  rule: "#CBD2D9", mutedBg: "#E6E9EC", mark: "#F6E08F",
};

/** A worker held by hand: what it was sent, and a way to answer or to fail. */
function heldWorker() {
  const sent: DiagramMessage[] = [];
  const listeners: Record<string, ((event: unknown) => void)[]> = {};
  let starts = 0;
  const worker = {
    postMessage: (message: DiagramMessage) => sent.push(message),
    addEventListener: (type: string, listener: (event: unknown) => void) => (listeners[type] ??= []).push(listener),
  };
  return {
    sent,
    get starts() {
      return starts;
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

test("a worker that fails rejects what it holds, and the next diagram starts a new one", async () => {
  const held = heldWorker();
  const draw = workerDiagram(held.start);
  const lost = draw({ engine: "d2", code: "a", palette: PALETTE });
  held.fail("WebAssembly.instantiate(): refused by CSP");
  await expect(lost).rejects.toThrow("refused by CSP");
  void draw({ engine: "d2", code: "b", palette: PALETTE });
  expect(held.starts).toBe(2);
});

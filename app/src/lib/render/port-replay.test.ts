/**
 * The port holds the jobs it was given and answers them again when the highlighter changes (the highlighter re-ruling,
 * 2026-11-21, test 9).
 */
import { expect, test } from "vitest";

import { workerPort, type EngineChoice, type HighlightMessage, type HighlightReply } from "./port";
import type { EngineId } from "./roles";

/** A worker that records what it is sent, and answers when told to. */
function fakeWorker() {
  const sent: HighlightMessage[] = [];
  let listen: (event: { data: HighlightReply }) => void = () => {};
  const worker = {
    postMessage: (message: HighlightMessage) => void sent.push(message),
    addEventListener: (_type: string, handler: (event: { data: HighlightReply }) => void) => (listen = handler),
  } as unknown as Pick<Worker, "postMessage" | "addEventListener">;
  return { worker, sent, answer: (reply: HighlightReply) => listen({ data: reply }) };
}

function port() {
  const w = fakeWorker();
  let engine: EngineId = "pierre";
  const failed: EngineId[] = [];
  const choice: EngineChoice = { engine: () => engine, failed: (e) => void failed.push(e) };
  const p = workerPort(() => w.worker, () => choice);
  return { ...w, port: p, failed, choose: (e: EngineId) => (engine = e) };
}

const opens = (sent: HighlightMessage[]) => sent.filter((m): m is Extract<HighlightMessage, { type: "open" }> => m.type === "open");

test("after rehighlight a held job is opened again with the new engine and its last focus, and answers reach the same handlers", () => {
  const p = port();
  const got: string[] = [];
  const job = p.port.highlight({ text: "a", path: "A.java" }, { onLines: (_from, lines) => void got.push(String(lines[0]![0]![1])) });
  job.focus(10, 40);
  p.choose("shiki");
  p.port.rehighlight();
  const [first, second] = opens(p.sent);
  expect(first).toMatchObject({ engine: "pierre", path: "A.java" });
  expect(second).toMatchObject({ engine: "shiki", path: "A.java", focus: [10, 40] });
  expect(p.sent).toContainEqual({ type: "cancel", id: first!.id });
  // The old run's answer goes nowhere; the new one's reaches the handlers.
  p.answer({ type: "lines", id: first!.id, from: 0, lines: [[["a", "kw"]]] });
  p.answer({ type: "lines", id: second!.id, from: 0, lines: [[["a", "str"]]] });
  expect(got).toEqual(["str"]);
});

test("a finished job is replayed; a cancelled one is not and is let go", () => {
  const p = port();
  const done = p.port.highlight({ text: "a", path: "A.java" }, { onLines: () => {} });
  const gone = p.port.highlight({ text: "b", path: "B.java" }, { onLines: () => {} });
  const [a] = opens(p.sent);
  p.answer({ type: "done", id: a!.id });
  gone.cancel();
  expect(p.port.held()).toBe(1);
  p.sent.length = 0;
  p.port.rehighlight();
  expect(opens(p.sent).map((m) => m.path)).toEqual(["A.java"]);
  // A finished job is not cancelled in the worker: it is not running there.
  expect(p.sent.filter((m) => m.type === "cancel")).toEqual([]);
  done.cancel();
  expect(p.port.held()).toBe(0);
});

test("a replay that ends plain says nothing, so what is drawn stays; the first plain is said", () => {
  const p = port();
  let plains = 0;
  p.port.highlight({ text: "a", path: "a.zzq" }, { onLines: () => {}, onPlain: () => void (plains += 1) });
  p.answer({ type: "plain", id: opens(p.sent)[0]!.id });
  p.port.rehighlight();
  p.answer({ type: "plain", id: opens(p.sent)[1]!.id });
  expect(plains).toBe(1);
});

test("a worker's failed reaches the choice, and warm asks the chosen engine, or one to try again", () => {
  const p = port();
  p.port.warm();
  p.answer({ type: "failed", engine: "pierre" });
  p.port.warm("pierre");
  expect(p.failed).toEqual(["pierre"]);
  expect(p.sent).toEqual([
    { type: "warm", engine: "pierre" },
    { type: "warm", engine: "pierre", retry: true },
  ]);
});

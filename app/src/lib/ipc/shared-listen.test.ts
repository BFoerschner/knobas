// One bridge listener per event for the window (`shared-listen.ts`, the ux30 end gate, Windows): a page that mounts
// where another heard the same event sends no listen, and one that goes sends no unlisten.
import { afterEach, expect, test, vi } from "vitest";

const bridge = vi.hoisted(() => {
  const heard = new Map<string, ((event: { event: string; id: number; payload: unknown }) => void)[]>();
  const listen = vi.fn(async (event: string, handler: (event: { event: string; id: number; payload: unknown }) => void) => {
    heard.set(event, [...(heard.get(event) ?? []), handler]);
    return unlisten;
  });
  const unlisten = vi.fn();
  const emit = (event: string, payload: unknown) => {
    for (const handler of heard.get(event) ?? []) handler({ event, id: 1, payload });
  };
  return { listen, unlisten, emit, heard };
});
vi.mock("@tauri-apps/api/event", () => ({ listen: bridge.listen }));

import { BRIDGE } from "../tf/builds/pipeline-ports";
import { SharedListeners } from "./shared-listen";

afterEach(() => {
  bridge.listen.mockClear();
  bridge.unlisten.mockClear();
});

test("the builds pages' ports hear an event through one bridge listener, however many pages hear it", async () => {
  const one: unknown[] = [];
  const two: unknown[] = [];
  const stopOne = await BRIDGE.listen("knobas://view-fresh-test", ({ payload }) => one.push(payload));
  const stopTwo = await BRIDGE.listen("knobas://view-fresh-test", ({ payload }) => two.push(payload));
  expect(bridge.listen).toHaveBeenCalledTimes(1);
  bridge.emit("knobas://view-fresh-test", "a");
  stopOne();
  bridge.emit("knobas://view-fresh-test", "b");
  // A page that goes sends no unlisten; the next to hear the event sends no listen.
  stopTwo();
  const three: unknown[] = [];
  await BRIDGE.listen("knobas://view-fresh-test", ({ payload }) => three.push(payload));
  bridge.emit("knobas://view-fresh-test", "c");
  expect([one, two, three]).toEqual([["a"], ["a", "b"], ["c"]]);
  expect(bridge.listen).toHaveBeenCalledTimes(1);
  expect(bridge.unlisten).not.toHaveBeenCalled();
});

test("a handler that joins twice leaves once for each, and one that throws keeps no other from hearing", async () => {
  const shared = new SharedListeners(bridge.listen);
  const got: string[] = [];
  const handler = ({ payload }: { payload: unknown }) => got.push(String(payload));
  const first = await shared.listen("twice", handler);
  await shared.listen("twice", handler);
  await shared.listen("twice", () => {
    throw new Error("a page's own fault");
  });
  const thrown = new Promise<unknown>((resolve) => {
    const queued = globalThis.queueMicrotask;
    vi.spyOn(globalThis, "queueMicrotask").mockImplementation((task) => queued(() => { try { task(); } catch (error) { resolve(error); } }));
  });
  bridge.emit("twice", "x");
  first();
  bridge.emit("twice", "y");
  expect(got).toEqual(["x", "x", "y"]);
  expect(String(await thrown)).toContain("a page's own fault");
  vi.restoreAllMocks();
});

test("a listener the bridge refused is asked for again by the next page", async () => {
  const listen = vi.fn().mockRejectedValueOnce(new Error("no bridge")).mockResolvedValue(() => {});
  const shared = new SharedListeners(listen);
  await expect(shared.listen("refused", () => {})).rejects.toThrow("no bridge");
  await shared.listen("refused", () => {});
  expect(listen).toHaveBeenCalledTimes(2);
});

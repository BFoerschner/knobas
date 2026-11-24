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
import { watchEchoes } from "../tf/queued.svelte";
import { SharedListeners, windowListen } from "./shared-listen";

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
  // The last page leaves and the next comes in the same update (a pipeline's page for a run's): no unlisten, no listen.
  stopTwo();
  const three: unknown[] = [];
  const stopThree = BRIDGE.listen("knobas://view-fresh-test", ({ payload }) => three.push(payload));
  await stopThree;
  bridge.emit("knobas://view-fresh-test", "c");
  expect([one, two, three]).toEqual([["a"], ["a", "b"], ["c"]]);
  expect(bridge.listen).toHaveBeenCalledTimes(1);
  expect(bridge.unlisten).not.toHaveBeenCalled();
  // Once no page hears it when an update's microtasks are done, the listener goes: nothing outlives the pages.
  (await stopThree)();
  await Promise.resolve();
  await Promise.resolve();
  expect(bridge.unlisten).toHaveBeenCalledTimes(1);
  await BRIDGE.listen("knobas://view-fresh-test", () => {});
  expect(bridge.listen).toHaveBeenCalledTimes(2);
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

// The ux30 end gate, Windows: a repository's first open sent nine listen messages as its page and its tab each heard the
// same events. Its parts hear them through the window's listeners now, as Builds' pages do.
test("the parts of a page that hear the same events share the window's one listener for each, with Builds' pages", async () => {
  const stopEchoes = watchEchoes("repo", () => {});
  const stopAgain = watchEchoes("repo-files", () => {});
  await vi.waitFor(() => expect(bridge.listen).toHaveBeenCalled());
  const events = new Set(bridge.listen.mock.calls.map(([event]) => event));
  // One bridge listener an event, however many parts hear it.
  expect(bridge.listen).toHaveBeenCalledTimes(events.size);
  const [event] = [...events];
  const heard: unknown[] = [];
  const stopBuilds = await BRIDGE.listen(event!, ({ payload }) => heard.push(payload));
  const stopOwn = await windowListen(event!, () => {});
  expect(bridge.listen).toHaveBeenCalledTimes(events.size);
  stopEchoes();
  stopAgain();
  stopBuilds();
  stopOwn();
  await Promise.resolve();
  await Promise.resolve();
  expect(bridge.unlisten).toHaveBeenCalledTimes(events.size);
});

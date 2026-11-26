/** A folder render's ask and listen loop (ADR-0044, ADR-0046): open while shown, look when not, again on a run's end. */
import { flushSync } from "svelte";
import { expect, test, vi } from "vitest";

import type { FolderAsk, FolderRenderView } from "../ipc/app";
import { FolderRun, type FolderRenderPorts, type FolderRunAt } from "./folder-run.svelte";

async function settle() {
  for (let i = 0; i < 10; i++) await Promise.resolve();
  flushSync();
}

function harness(start: boolean, lane?: string) {
  const asked: FolderAsk[] = [];
  let heard: ((event: { payload: unknown }) => void) | null = null;
  const ports: FolderRenderPorts = {
    render: (at) => {
      asked.push(at);
      return Promise.resolve({ renderer: at.renderer, folder: at.folder, drawn: null, current: false, running: null, failure: null, from: "clone", commit: null } satisfies FolderRenderView);
    },
    stop: () => Promise.resolve(),
    leave: () => Promise.resolve(),
    output: () => Promise.resolve(new Uint8Array()),
    frame: () => Promise.resolve("t"),
    listen: (_event, handler) => {
      heard = handler;
      return Promise.resolve(() => {});
    },
  };
  const at: FolderRunAt = $state({ entityId: "r", renderer: "site", command: "", folder: "flows", commit: null, paths: null, start, ...(lane === undefined ? {} : { lane }) });
  let run!: FolderRun;
  const stop = $effect.root(() => {
    run = new FolderRun(() => at, ports);
  });
  flushSync();
  const tell = (running: unknown, folder = "flows", lane?: string, more: Record<string, unknown> = {}) =>
    heard?.({ payload: { entity_id: "r", renderer: "site", folder, running, lane: lane ?? "", ...more } });
  return { asked, at, run: () => run, stop, tell };
}

test("open while shown, look when not, and open when a hidden page is shown again", async () => {
  const shown = harness(true);
  await settle();
  expect(shown.asked.map((a) => a.ask)).toEqual(["open"]);
  shown.stop();

  const hidden = harness(false);
  await settle();
  expect(hidden.asked.map((a) => a.ask)).toEqual(["look"]);
  hidden.at.start = true;
  await settle();
  expect(hidden.asked.map((a) => a.ask).at(-1)).toBe("open");
  hidden.stop();
});

test("progress is taken without an ask, and a run's end asks once", async () => {
  const h = harness(true);
  await settle();
  const before = h.asked.length;
  h.tell({ state: "keeping", done: 3, total: 9 });
  flushSync();
  expect(h.run().view?.running).toEqual({ state: "keeping", done: 3, total: 9 });
  expect(h.asked.length).toBe(before);
  h.tell(null, "elsewhere");
  await settle();
  expect(h.asked.length, "another folder's move asks nothing").toBe(before);
  h.tell(null);
  await settle();
  expect(h.asked.slice(before).map((a) => a.ask)).toEqual(["look"]);
  h.stop();
});

test("a comparison's side asks in its own lane, and another lane's move is not its own (ux26)", async () => {
  const h = harness(true, "cmp:~working-tree");
  await settle();
  expect(h.asked.at(-1)?.lane).toBe("cmp:~working-tree");
  const before = h.asked.length;
  h.tell(null, "flows", "");
  h.tell(null, "flows", "cmp:4f2a91c0d2b9e8a7f6c5d4b3a2918f7e6d5c4b3a");
  await settle();
  expect(h.asked.length, "the Files tab's and the other side's moves ask nothing").toBe(before);
  h.tell(null, "flows", "cmp:~working-tree");
  await settle();
  expect(h.asked.slice(before).map((a) => a.ask)).toEqual(["look"]);
  h.stop();
});

test("a new state of the same folder (the working tree's stamp, ux29) asks again, keeps the drawing and its listener", async () => {
  const asked: FolderAsk[] = [];
  let listens = 0;
  const left: string[] = [];
  const drawn = { token: "W", kind: { kind: "folder", files: 1, bytes: 8, left_out: 0 }, size: 8, commit: null, from: "working_tree", took_ms: 1, drawn_at: "2026-11-21T08:00:00Z" } as const;
  let hold: Promise<void> = Promise.resolve();
  const ports: FolderRenderPorts = {
    render: async (at) => {
      asked.push(at);
      await hold;
      return { renderer: at.renderer, folder: at.folder, drawn, current: true, running: null, failure: null, from: "working_tree", commit: null } satisfies FolderRenderView;
    },
    stop: () => Promise.resolve(),
    leave: (_e, _r, folder) => (left.push(folder), Promise.resolve()),
    output: () => Promise.resolve(new Uint8Array()),
    frame: () => Promise.resolve("t"),
    listen: () => ((listens += 1), Promise.resolve(() => {})),
  };
  const at: FolderRunAt = $state({ entityId: "r", renderer: "site", command: "", folder: "flows", commit: "~working-tree", paths: null, start: true, lane: "cmp:~working-tree", stamp: "s1" });
  let run!: FolderRun;
  const stop = $effect.root(() => {
    run = new FolderRun(() => at, ports);
  });
  flushSync();
  await settle();
  expect(run.view?.drawn?.token).toBe("W");
  let release!: () => void;
  hold = new Promise((resolve) => (release = resolve));
  at.stamp = "s2";
  await settle();
  expect(asked.map((a) => [a.ask, a.commit, a.lane])).toEqual([
    ["open", "~working-tree", "cmp:~working-tree"],
    ["open", "~working-tree", "cmp:~working-tree"],
  ]);
  expect(run.view?.drawn?.token, "drawn while the new answer is on its way").toBe("W");
  expect([listens, left.length], "one listener, never left").toEqual([1, 0]);
  release();
  // Another folder is a new place: it starts blank.
  hold = new Promise(() => {});
  at.folder = "docs";
  await settle();
  expect(run.view).toBeNull();
  stop();
});


test("a folder the backend dropped (a command changed it) is made again where it is shown, looked at where not (2026-11-25)", async () => {
  for (const [start, want] of [
    [true, "open"],
    [false, "look"],
  ] as const) {
    const h = harness(start);
    await settle();
    const before = h.asked.length;
    h.tell(null, "flows", undefined, { dropped: true });
    await settle();
    expect(h.asked.slice(before).map((a) => a.ask)).toEqual([want]);
    h.stop();
  }
});

test("elapsed timer resets to 0s on re-run after completion or stop, and never double-counts backend elapsed_ms updates (#2523)", async () => {
  vi.useFakeTimers();
  try {
    let nextView: FolderRenderView | null = null;
    const asked: FolderAsk[] = [];
    let heard: ((event: { payload: unknown }) => void) | null = null;
    const ports: FolderRenderPorts = {
      render: (at) => {
        asked.push(at);
        return Promise.resolve(
          nextView ?? {
            renderer: at.renderer,
            folder: at.folder,
            drawn: null,
            current: false,
            running: null,
            failure: null,
            from: "clone",
            commit: null,
          },
        );
      },
      stop: () => Promise.resolve(),
      leave: () => Promise.resolve(),
      output: () => Promise.resolve(new Uint8Array()),
      frame: () => Promise.resolve("t"),
      listen: (_event, handler) => {
        heard = handler;
        return Promise.resolve(() => {});
      },
    };
    const at: FolderRunAt = $state({ entityId: "r", renderer: "site", command: "", folder: "flows", commit: null, paths: null, start: true });
    let run!: FolderRun;
    const stop = $effect.root(() => {
      run = new FolderRun(() => at, ports);
    });
    flushSync();
    await settle();
    const tell = (running: unknown) => heard?.({ payload: { entity_id: "r", renderer: "site", folder: "flows", running, lane: "" } });

    // Run 1: starts running at 0ms and ticks for 5 seconds.
    tell({ state: "running", elapsed_ms: 0 });
    flushSync();
    expect(run.elapsed).toBe(0);
    vi.advanceTimersByTime(5000);
    flushSync();
    expect(run.elapsed).toBe(5000);

    // Mid-run backend elapsed_ms update must replace the baseline, not double-count against stale local ticks.
    tell({ state: "running", elapsed_ms: 5200 });
    flushSync();
    expect(run.elapsed).toBe(5200);
    vi.advanceTimersByTime(1000);
    flushSync();
    expect(run.elapsed).toBe(6200);

    // Run 1 finishes.
    tell(null);
    await settle();
    expect(run.elapsed).toBe(0);

    // Re-run via press(): Run 2 must start from 0s, not carry over Run 1's ticks.
    nextView = { renderer: "site", folder: "flows", drawn: null, current: false, running: { state: "running", elapsed_ms: 0 }, failure: null, from: "clone", commit: null };
    run.press();
    await settle();
    expect(run.elapsed).toBe(0);
    vi.advanceTimersByTime(2000);
    flushSync();
    expect(run.elapsed).toBe(2000);

    // Fresh backend answer during a running state resets local ticks too (no double-count).
    nextView = { renderer: "site", folder: "flows", drawn: null, current: false, running: { state: "running", elapsed_ms: 2300 }, failure: null, from: "clone", commit: null };
    run.look();
    await settle();
    expect(run.elapsed).toBe(2300);

    // Stop via press() mid-run, wait past PressHold, then re-run: starts from 0s again.
    vi.advanceTimersByTime(3000);
    flushSync();
    expect(run.elapsed).toBe(5300);
    run.press();
    flushSync();
    expect(run.elapsed).toBe(0);
    vi.advanceTimersByTime(1000);
    nextView = { renderer: "site", folder: "flows", drawn: null, current: false, running: { state: "running", elapsed_ms: 0 }, failure: null, from: "clone", commit: null };
    run.press();
    await settle();
    expect(run.elapsed).toBe(0);

    stop();
  } finally {
    vi.useRealTimers();
  }
});

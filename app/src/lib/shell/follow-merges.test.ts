/**
 * The reverse direction's trigger (#44, stories 17 and 19): a sync run ending
 * runs the follow-merges pass, and a moved ticket is announced.
 */
import { beforeEach, expect, test, vi } from "vitest";

const handlers: ((event: { payload: { running: boolean } }) => void)[] = [];
let unlistened = 0;

vi.mock("@tauri-apps/api/event", () => ({
  listen: (_event: string, handler: (event: { payload: { running: boolean } }) => void) => {
    handlers.push(handler);
    return Promise.resolve(() => {
      unlistened += 1;
    });
  },
}));

const followMerges = vi.fn<() => Promise<number>>();
vi.mock("../ipc/entity", () => ({
  followMerges: () => followMerges(),
}));

const pushed: { text: string }[] = [];
vi.mock("./toasts.svelte", () => ({
  push: (spec: { text: string }) => {
    pushed.push(spec);
    return 1;
  },
}));

import { startFollowingMerges } from "./follow-merges";

/** Let the pass's own microtasks run. */
async function settled(): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
  await Promise.resolve();
}

function syncEnded(): void {
  for (const handler of [...handlers]) handler({ payload: { running: false } });
}

beforeEach(() => {
  handlers.length = 0;
  pushed.length = 0;
  unlistened = 0;
  followMerges.mockReset();
});

test("a sync run ending runs the pass; a run starting does not", async () => {
  followMerges.mockResolvedValue(0);
  const stop = startFollowingMerges();
  await settled();

  for (const handler of [...handlers]) handler({ payload: { running: true } });
  await settled();
  expect(followMerges).not.toHaveBeenCalled();

  syncEnded();
  await settled();
  expect(followMerges).toHaveBeenCalledTimes(1);
  stop();
});

test("a moved ticket is announced, and a pass that moved nothing says nothing", async () => {
  followMerges.mockResolvedValueOnce(2).mockResolvedValue(0);
  const stop = startFollowingMerges();
  await settled();

  syncEnded();
  await settled();
  expect(pushed.map((toast) => toast.text)).toEqual([
    "Merged pull requests moved 2 tickets to In Review.",
  ]);

  syncEnded();
  await settled();
  expect(pushed.length).toBe(1);
  stop();
});

test("run endings during a pass coalesce into exactly one more pass", async () => {
  let release: (moved: number) => void = () => {};
  followMerges
    .mockImplementationOnce(() => new Promise<number>((resolve) => (release = resolve)))
    .mockResolvedValue(0);
  const stop = startFollowingMerges();
  await settled();

  syncEnded();
  await settled();
  expect(followMerges).toHaveBeenCalledTimes(1);

  // Three more endings while the first pass is still out.
  syncEnded();
  syncEnded();
  syncEnded();
  await settled();
  expect(followMerges).toHaveBeenCalledTimes(1);

  release(0);
  await settled();
  expect(followMerges).toHaveBeenCalledTimes(2);
  stop();
});

test("a failed pass is quiet and the next ending retries it", async () => {
  followMerges.mockRejectedValueOnce(new Error("not ready")).mockResolvedValue(1);
  const stop = startFollowingMerges();
  await settled();

  syncEnded();
  await settled();
  expect(pushed.length).toBe(0);

  syncEnded();
  await settled();
  expect(pushed.map((toast) => toast.text)).toEqual([
    "A merged pull request moved its ticket to In Review.",
  ]);
  stop();
});

test("stopping unlistens", async () => {
  followMerges.mockResolvedValue(0);
  const stop = startFollowingMerges();
  await settled();
  stop();
  await settled();
  expect(unlistened).toBe(1);
});

/**
 * The flow store's polling (issue #44, story 11). Since #779 the run answers
 * at once, the steps run behind it, and the store follows the flow while it
 * moves.
 *
 * `startWorkRun` answers only when the whole sequence stops, but the rows move
 * underneath it — a step is marked `running` while its dispatch is out — and
 * the store re-reading the flow on an interval is the only way the stepper can
 * draw that. The two properties worth having: the peek's answer is drawn while
 * the run is out, and the run's own answer wins the moment it lands.
 */
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { StartWorkStep } from "../ipc/entity";

let peeked: StartWorkStep[] = [];
let peeks = 0;
let releaseRun: (steps: StartWorkStep[]) => void = () => {};
let retried: () => Promise<StartWorkStep[]> = () => Promise.resolve([]);

vi.mock("../ipc/entity", () => ({
  startWorkFlow: () => {
    peeks += 1;
    return Promise.resolve(peeked);
  },
  startWorkRun: () =>
    new Promise<StartWorkStep[]>((resolve) => {
      releaseRun = resolve;
    }),
  startWorkRetry: () => retried(),
  startWorkSkip: () => Promise.resolve([]),
  startWorkAmend: () => Promise.resolve([]),
}));

const { createStartWork, editable, edited } = await import("./start-work.svelte");

function one(id: number, outcome: StartWorkStep["outcome"]): StartWorkStep {
  return {
    id,
    ticket_id: "jira:PAY-231",
    step: "create_branch",
    position: 0,
    outcome,
    payload: { CreateBranch: { entity: "gitea:r", name: "b", from_ref: "main" } },
    write_id: null,
    detail: null,
    updated_at: "2026-08-29T09:00:00Z",
  };
}

beforeEach(() => {
  vi.useFakeTimers();
  peeked = [];
  peeks = 0;
  retried = () => Promise.resolve([]);
});

afterEach(() => {
  vi.useRealTimers();
});

test("run answers at once, and the flow is re-read while it moves, each step drawn as it lands", async () => {
  const sw = createStartWork("jira:PAY-231");
  const steps = [one(1, "pending"), { ...one(2, "pending"), position: 1 }];
  const running = sw.run();
  // The command answers while the first send is still held: the store does not wait for it.
  releaseRun(steps);
  await running;
  expect(sw.busy).toBe(false);
  expect(sw.following).toBe(true);

  peeked = [one(1, "running"), { ...one(2, "pending"), position: 1 }];
  await vi.advanceTimersByTimeAsync(800);
  expect(sw.steps.map((step) => step.outcome)).toEqual(["running", "pending"]);

  // Between two steps: the first landed, the second is not started yet. Still moving.
  peeked = [one(1, "succeeded"), { ...one(2, "pending"), position: 1 }];
  await vi.advanceTimersByTimeAsync(800);
  expect(sw.following).toBe(true);

  peeked = [one(1, "succeeded"), { ...one(2, "succeeded"), position: 1 }];
  await vi.advanceTimersByTimeAsync(800);
  expect(sw.steps.map((step) => step.outcome)).toEqual(["succeeded", "succeeded"]);
  expect(sw.following).toBe(false);

  // Settled: nothing polls any more.
  const before = peeks;
  await vi.advanceTimersByTimeAsync(3000);
  expect(peeks).toBe(before);
});

test("a flow that stops on a failed or queued step is no longer followed", async () => {
  for (const stop of ["failed", "queued"] as const) {
    const sw = createStartWork("jira:PAY-231");
    const running = sw.run();
    releaseRun([one(1, "pending")]);
    await running;
    peeked = [one(1, stop)];
    await vi.advanceTimersByTimeAsync(800);
    expect(sw.steps.map((step) => step.outcome)).toEqual([stop]);
    expect(sw.following).toBe(false);
    const before = peeks;
    await vi.advanceTimersByTimeAsync(3000);
    expect(peeks).toBe(before);
  }
});

test("stop ends the following, for a stepper that goes away while the flow moves", async () => {
  const sw = createStartWork("jira:PAY-231");
  const running = sw.run();
  releaseRun([one(1, "running")]);
  await running;
  sw.stop();
  const before = peeks;
  await vi.advanceTimersByTimeAsync(3000);
  expect(peeks).toBe(before);
});

test("a stepper whose page is hidden reads nothing while it follows, and reads where the flow got to on its return (#2249)", async () => {
  let shown = true;
  const sw = createStartWork("jira:PAY-231", undefined, () => shown);
  const running = sw.run();
  releaseRun([one(1, "running")]);
  await running;
  shown = false;
  const before = peeks;
  peeked = [one(1, "succeeded")];
  await vi.advanceTimersByTimeAsync(3000);
  expect(peeks).toBe(before);
  expect(sw.following).toBe(true);

  shown = true;
  await vi.advanceTimersByTimeAsync(800);
  expect(peeks).toBe(before + 1);
  expect(sw.steps.map((step) => step.outcome)).toEqual(["succeeded"]);
  expect(sw.following).toBe(false);
});

test("edited replaces one key of the op, so the repository picker's amendment carries the entity", () => {
  const step = one(1, "pending");
  const amended = edited(step, "entity", "gitea:other") as Record<string, Record<string, unknown>>;
  const [tag] = Object.keys(amended);
  expect(amended[tag!]!.entity).toBe("gitea:other");
  expect(Object.keys(amended)).toHaveLength(1);
});

test("only a pending or a failed step is the reader's to edit, as the backend edits no other (#1707)", () => {
  const outcomes = ["pending", "running", "succeeded", "queued", "failed", "skipped"] as const;
  expect(outcomes.filter(editable)).toEqual(["pending", "failed"]);
});

test("a refusal of a step that moved on re-reads the flow, so the stale row is redrawn with the refusal (#1707)", async () => {
  const sw = createStartWork("jira:PAY-231");
  peeked = [one(1, "failed")];
  await sw.load(null);
  // The step landed underneath the stepper, which still draws it failed.
  peeked = [one(1, "succeeded")];
  retried = () =>
    Promise.reject({ code: "conflict", message: "the create_branch step is succeeded now, so it cannot be retried", source_id: null });
  await sw.retry(1);
  expect(sw.error).toBe("the create_branch step is succeeded now, so it cannot be retried");
  expect(sw.steps.map((step) => step.outcome)).toEqual(["succeeded"]);
  expect(sw.following).toBe(false);
});

/**
 * The flow store's polling (issue #44, story 11).
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

vi.mock("../ipc/entity", () => ({
  startWorkFlow: () => {
    peeks += 1;
    return Promise.resolve(peeked);
  },
  startWorkRun: () =>
    new Promise<StartWorkStep[]>((resolve) => {
      releaseRun = resolve;
    }),
  startWorkRetry: () => Promise.resolve([]),
  startWorkSkip: () => Promise.resolve([]),
  startWorkAmend: () => Promise.resolve([]),
}));

const { createStartWork } = await import("./start-work.svelte");

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
});

afterEach(() => {
  vi.useRealTimers();
});

test("while the run is out, the flow is re-read and what it answers is drawn", async () => {
  const sw = createStartWork("jira:PAY-231");
  const running = sw.run();

  peeked = [one(1, "running")];
  await vi.advanceTimersByTimeAsync(1500);
  expect(peeks).toBeGreaterThanOrEqual(2);
  expect(sw.steps.map((step) => step.outcome)).toEqual(["running"]);

  releaseRun([one(1, "succeeded")]);
  await running;
  expect(sw.steps.map((step) => step.outcome)).toEqual(["succeeded"]);
  expect(sw.busy).toBe(false);

  // The run has answered: nothing polls any more.
  const before = peeks;
  await vi.advanceTimersByTimeAsync(3000);
  expect(peeks).toBe(before);
});

test("a peek that resolves after the run's answer cannot overwrite it", async () => {
  const sw = createStartWork("jira:PAY-231");

  // The peek hangs until after the run answers.
  let releasePeek: (steps: StartWorkStep[]) => void = () => {};
  peeked = [];
  const slow = new Promise<StartWorkStep[]>((resolve) => {
    releasePeek = resolve;
  });
  const entity = await import("../ipc/entity");
  vi.spyOn(entity, "startWorkFlow").mockImplementation(() => slow);

  const running = sw.run();
  await vi.advanceTimersByTimeAsync(800);

  releaseRun([one(1, "succeeded")]);
  await running;
  expect(sw.steps.map((step) => step.outcome)).toEqual(["succeeded"]);

  releasePeek([one(1, "running")]);
  await Promise.resolve();
  await Promise.resolve();
  expect(sw.steps.map((step) => step.outcome)).toEqual(
    ["succeeded"],
    // A stale peek landing late must not un-succeed the flow on screen.
  );
});

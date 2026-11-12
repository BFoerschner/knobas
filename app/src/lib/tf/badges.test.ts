import { expect, test } from "vitest";

import { createBadges, downIn, failingIn, type BadgePorts } from "./badges.svelte";

test("failing configurations are the err lines of the Builds tile's tally", () => {
  expect(
    failingIn({
      tally: [
        { kind: "build", tone: "err", count: 2 },
        { kind: "build", tone: "ok", count: 5 },
        { kind: "build", tone: "info", count: 1 },
      ],
    }),
  ).toBe(2);
  expect(failingIn({ tally: null })).toBe(0);
});

test("a paused monitor is not down", () => {
  expect(
    downIn([
      { state: "down", tombstoned: false, hidden: false },
      { state: "down", tombstoned: true, hidden: false },
      { state: "up", tombstoned: false, hidden: false },
      { state: "warn", tombstoned: false, hidden: false },
    ]),
  ).toBe(1);
});

test("a hidden monitor is not down either: its alerts reach nobody", () => {
  expect(downIn([{ state: "down", tombstoned: false, hidden: true }])).toBe(0);
});

function ports(over: Partial<BadgePorts> = {}): BadgePorts & { fire: (event: string, payload: unknown) => void } {
  const handlers = new Map<string, ((event: { payload: unknown }) => void)[]>();
  return {
    failingBuilds: () => Promise.resolve(2),
    monitorsDown: () => Promise.resolve(1),
    listen: (event, handler) => {
      handlers.set(event, [...(handlers.get(event) ?? []), handler]);
      return Promise.resolve(() => {});
    },
    fire: (event, payload) => {
      for (const handler of handlers.get(event) ?? []) handler({ payload });
    },
    ...over,
  };
}

test("nothing is counted until the shell asks, then both are", async () => {
  const badges = createBadges(ports());
  expect([badges.builds, badges.monitors]).toEqual([null, null]);
  await badges.refresh();
  expect([badges.builds, badges.monitors]).toEqual([2, 1]);
});

test("a failed read keeps the last answer instead of clearing the badge", async () => {
  let fail = false;
  const badges = createBadges(
    ports({ monitorsDown: () => (fail ? Promise.reject(new Error("not_ready")) : Promise.resolve(3)) }),
  );
  await badges.refresh();
  fail = true;
  await badges.refresh();
  expect(badges.monitors).toBe(3);
});

test("a finished sync run re-reads the counts; a run still going does not", async () => {
  let builds = 1;
  const io = ports({ failingBuilds: () => Promise.resolve(builds) });
  const badges = createBadges(io);
  const stop = badges.start();
  await badges.refresh();
  builds = 3;
  io.fire("sync:state", { running: true });
  await Promise.resolve();
  await Promise.resolve();
  expect(badges.builds).toBe(1);
  io.fire("sync:state", { running: false });
  await new Promise((resolve) => setTimeout(resolve, 0));
  expect(badges.builds).toBe(3);
  stop();
});

test("a sync wave's burst of events reads each count twice at most, the last after the last event (the query audit, 2026-11-12)", async () => {
  let builds = 0;
  let monitors = 0;
  let land: () => void = () => {};
  const probe = ports({
    failingBuilds: () => ((builds += 1), new Promise<number>((resolve) => (land = () => resolve(builds)))),
    monitorsDown: () => ((monitors += 1), Promise.resolve(monitors)),
  });
  const badges = createBadges(probe);
  badges.start();
  await Promise.resolve();
  const ended = { source_id: "jira", running: false };
  for (let i = 0; i < 6; i += 1) probe.fire("sync:state", ended);
  probe.fire("activity:new", {});
  expect([builds, monitors]).toEqual([1, 1]);
  land();
  await new Promise((resolve) => setTimeout(resolve, 0));
  expect([builds, monitors]).toEqual([2, 2]);
  land();
  await new Promise((resolve) => setTimeout(resolve, 0));
  expect([badges.builds, badges.monitors, builds, monitors]).toEqual([2, 2, 2, 2]);
});

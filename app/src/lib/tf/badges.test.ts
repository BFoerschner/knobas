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
      { state: "down", tombstoned: false },
      { state: "down", tombstoned: true },
      { state: "up", tombstoned: false },
      { state: "warn", tombstoned: false },
    ]),
  ).toBe(1);
});

function ports(over: Partial<BadgePorts> = {}): BadgePorts & { fire: (event: string, payload: unknown) => void } {
  const handlers = new Map<string, ((event: { payload: unknown }) => void)[]>();
  return {
    failingBuilds: () => Promise.resolve(2),
    monitorsDown: () => Promise.resolve(1),
    proposals: () => Promise.resolve(4),
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

test("nothing is counted until the shell asks, then all three are", async () => {
  const badges = createBadges(ports());
  expect([badges.builds, badges.monitors, badges.proposals]).toEqual([null, null, null]);
  await badges.refresh();
  expect([badges.builds, badges.monitors, badges.proposals]).toEqual([2, 1, 4]);
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

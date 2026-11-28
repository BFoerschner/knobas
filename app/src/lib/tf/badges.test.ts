import { expect, test } from "vitest";

import type { BubbleCounts, RunCounts } from "../ipc/entity";
import { createBadges, downIn, type BadgePorts } from "./badges.svelte";
import { noCounts } from "./builds/builds";
import { RunsRead } from "./builds/runs-read.svelte";

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

/** Every pipeline's counts summed into one answer, as `run_counts` gives them. */
function answer(total: Partial<BubbleCounts> = {}, visiting = false): RunCounts {
  const counts = { ...noCounts(), ...total };
  return { pipelines: [{ pipeline: "tc:buildType:Payout_Tests", counts }], total: counts, visiting, fresh: [] };
}

/** A runs store whose counts come from `counts`, as the window's one reads them. */
function runsWith(counts: () => Promise<RunCounts>): RunsRead {
  const store = new RunsRead();
  store.settleMs = 0;
  store.ports = { ...store.ports, counts, inFront: () => Promise.resolve(true) };
  return store;
}

function ports(
  over: Partial<BadgePorts> = {},
  store: RunsRead = runsWith(() => Promise.resolve(answer({ red: 2 }))),
): BadgePorts & { fire: (event: string, payload: unknown) => void } {
  const handlers = new Map<string, ((event: { payload: unknown }) => void)[]>();
  return {
    readBuilds: () => store.readCounts(),
    builds: () => store.topBar,
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

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

test("nothing is counted until the shell asks, then both are", async () => {
  const badges = createBadges(ports());
  expect([badges.builds, badges.monitors]).toEqual([null, null]);
  await badges.refresh();
  expect([badges.builds?.red, badges.monitors]).toEqual([2, 1]);
});

test("a failed read keeps the last answer instead of clearing the badge", async () => {
  let fail = false;
  const store = runsWith(() => (fail ? Promise.reject(new Error("not_ready")) : Promise.resolve(answer({ red: 4 }))));
  const badges = createBadges(
    ports({ monitorsDown: () => (fail ? Promise.reject(new Error("not_ready")) : Promise.resolve(3)) }, store),
  );
  await badges.refresh();
  fail = true;
  await badges.refresh();
  expect([badges.monitors, badges.builds?.red]).toEqual([3, 4]);
});

test("a finished sync run re-reads the counts; a run still going does not", async () => {
  let red = 1;
  const io = ports({}, runsWith(() => Promise.resolve(answer({ red }))));
  const badges = createBadges(io);
  const stop = badges.start();
  await badges.refresh();
  red = 3;
  io.fire("sync:state", { running: true });
  await Promise.resolve();
  await Promise.resolve();
  expect(badges.builds?.red).toBe(1);
  io.fire("sync:state", { running: false });
  await settle();
  expect(badges.builds?.red).toBe(3);
  stop();
});

test("a sync wave's burst of events reads each count twice at most, the last after the last event (the query audit, 2026-11-12)", async () => {
  let builds = 0;
  let monitors = 0;
  let land: () => void = () => {};
  const store = runsWith(() => ((builds += 1), new Promise<RunCounts>((resolve) => (land = () => resolve(answer({ red: builds }))))));
  const probe = ports({ monitorsDown: () => ((monitors += 1), Promise.resolve(monitors)) }, store);
  const badges = createBadges(probe);
  badges.start();
  await Promise.resolve();
  const ended = { source_id: "jira", running: false };
  for (let i = 0; i < 6; i += 1) probe.fire("sync:state", ended);
  probe.fire("activity:new", {});
  expect([builds, monitors]).toEqual([1, 1]);
  land();
  await settle();
  expect([builds, monitors]).toEqual([2, 2]);
  land();
  await settle();
  expect([badges.builds?.red, badges.monitors, builds, monitors]).toEqual([2, 2, 2, 2]);
});

/**
 * A backend over three runs of one pipeline, as `run_counts` reads them (ADR-0063): a run's stamp is when it ended, the
 * instant is when Builds was last left, and while Builds is in front the visit's start bounds green.
 */
function backend() {
  let clock = 10;
  const runs = new Map<string, { bubble: "blue" | "green" | "red"; stamp: number }>();
  let at = 0;
  let visit: number | null = null;
  const counts = (): Promise<RunCounts> => {
    const total = noCounts();
    for (const run of runs.values()) {
      if (run.bubble === "green" && !(run.stamp > at && run.stamp <= (visit ?? Infinity))) continue;
      total[run.bubble] += 1;
    }
    return Promise.resolve(answer(total, visit !== null));
  };
  return {
    counts,
    inFront: (front: boolean) => {
      clock += 1;
      if (front) visit ??= clock;
      else if (visit !== null) ((visit = null), (at = clock));
      return Promise.resolve(true);
    },
    /** A sync wrote `id` in `bubble`. */
    run(id: string, bubble: "blue" | "green" | "red") {
      clock += 1;
      runs.set(id, { bubble, stamp: clock });
    },
  };
}

test("the counts follow a run that starts, passes and fails, with no reload; with Builds in front a pass adds no green", async () => {
  const server = backend();
  const store = runsWith(server.counts);
  store.ports = { ...store.ports, inFront: server.inFront };
  const io = ports({}, store);
  const badges = createBadges(io);
  badges.start();
  await badges.refresh();
  expect(badges.builds).toEqual(noCounts());

  const synced = async () => (io.fire("sync:state", { running: false }), await settle());
  server.run("a", "blue");
  server.run("b", "blue");
  await synced();
  expect(badges.builds).toMatchObject({ blue: 2, green: 0, red: 0 });
  server.run("a", "green");
  await synced();
  expect(badges.builds).toMatchObject({ blue: 1, green: 1, red: 0 });

  await store.inFront(true);
  expect(badges.builds).toMatchObject({ blue: 1, green: 0, red: 0 });
  server.run("b", "green");
  server.run("c", "blue");
  await synced();
  expect(badges.builds).toMatchObject({ blue: 1, green: 0, red: 0 });
  server.run("c", "red");
  await synced();
  expect(badges.builds).toMatchObject({ blue: 0, green: 0, red: 1 });

  await store.inFront(false);
  expect(badges.builds, "the visit's passes were seen").toMatchObject({ blue: 0, green: 0, red: 1 });
});

test("the shell's start reads the monitors alone: the builds' counts come with the runs store's first word (#2569b)", async () => {
  // At start the runs store tells the backend where the window is and reads the counts then (`started`, `inFront`);
  // a read here as well went out beside Recent's first page and every other start-up read.
  let builds = 0;
  const badges = createBadges(ports({ readBuilds: async () => void (builds += 1) }));
  await badges.readMonitors();
  expect([badges.monitors, builds]).toEqual([1, 0]);
});

/**
 * The open-alert store (#444): one read, one list, one count.
 *
 * What is worth pinning here is the pair of decisions the module argues for
 * and neither of which a type can hold: that the count is the list's length —
 * so the top strip's badge and the Assets view's list cannot come apart — and
 * that a **failed** read leaves the list standing, because an estate whose
 * badge blinks to zero during a hiccup is an outage the reader stops looking
 * for.
 */
import { expect, test, vi } from "vitest";

import { EVENTS } from "../ipc";
import type { OpenAlert } from "../ipc/assets";
import { createAlerts } from "./alerts.svelte";

function alert(id: number, monitor: string, state: "down" | "warn" = "down"): OpenAlert {
  return {
    id,
    monitor_id: `kuma:${id}`,
    monitor_name: monitor,
    web_url: null,
    state,
    opened_at: "2026-09-07T08:00:00Z",
    acked_at: null,
    assets: [],
  };
}

/** A `listen()` stand-in that hands the test the handlers it registered. */
function fakeListen() {
  const events: string[] = [];
  const handlers: (() => void)[] = [];
  let unlistened = 0;
  return {
    events,
    handlers,
    get unlistened() {
      return unlistened;
    },
    fire() {
      for (const handler of handlers) handler();
    },
    listen: (event: string, handler: () => void) => {
      events.push(event);
      handlers.push(handler);
      return Promise.resolve(() => {
        unlistened += 1;
      });
    },
  };
}

test("the count is the list's length, so the badge cannot disagree with the list", async () => {
  const alerts = createAlerts({
    openAlerts: () => Promise.resolve([alert(1, "gitea"), alert(2, "canary", "warn")]),
    listen: () => Promise.resolve(() => {}),
  });

  expect(alerts.count).toBe(0);
  await alerts.refresh();
  expect(alerts.open.map((row) => row.monitor_name)).toEqual(["gitea", "canary"]);
  expect(alerts.count).toBe(alerts.open.length);
  expect(alerts.count).toBe(2);
});

test("a failed read is reported and leaves the list where it was", async () => {
  let answer: () => Promise<OpenAlert[]> = () => Promise.resolve([alert(1, "gitea")]);
  const alerts = createAlerts({
    openAlerts: () => answer(),
    listen: () => Promise.resolve(() => {}),
  });

  await alerts.refresh();
  expect(alerts.count).toBe(1);

  answer = () => Promise.reject(new Error("the database went away"));
  await alerts.refresh();
  expect(alerts.error).toContain("the database went away");
  expect(alerts.count).toBe(1);
});

test("it re-reads on a sync run and on an activity line, and stops when told", async () => {
  const events = fakeListen();
  let reads = 0;
  const alerts = createAlerts({
    openAlerts: () => {
      reads += 1;
      return Promise.resolve([alert(1, "gitea")]);
    },
    listen: events.listen,
  });

  const stop = alerts.start();
  await vi.waitFor(() => expect(events.handlers.length).toBe(2));
  // The two signals an alert moves on: a run reconciles them and an ack
  // writes a history line. There is deliberately no channel of the alert's
  // own — `alerts.svelte.ts` records the argument.
  expect(events.events).toEqual([EVENTS.activityNew, EVENTS.syncState]);
  // Subscribing is not seeding: the shell seeds, which is the division
  // `inbox.start()`/`inbox.refresh()` already makes.
  expect(reads).toBe(0);

  events.fire();
  await vi.waitFor(() => expect(reads).toBe(2));

  stop();
  expect(events.unlistened).toBe(2);
  events.fire();
  await Promise.resolve();
  // A stopped store does not read.
  expect(reads).toBe(2);
});

test("starting twice does not subscribe twice, and the second teardown is inert", async () => {
  const events = fakeListen();
  const alerts = createAlerts({
    openAlerts: () => Promise.resolve([]),
    listen: events.listen,
  });

  const stop = alerts.start();
  await vi.waitFor(() => expect(events.handlers.length).toBe(2));
  const again = alerts.start();
  again();
  // The second teardown must not strand the first subscription.
  expect(events.unlistened).toBe(0);
  stop();
  expect(events.unlistened).toBe(2);
});

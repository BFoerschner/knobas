/**
 * What a stop opens in the ticket-first shell, and what each of the draft's
 * buttons does to the clock (#826, R-13.1 to R-13.3; #896).
 *
 * The claims that matter are about **time not getting lost**: the draft opens
 * over the running clock, so *Keep running* touches nothing; the clock stops
 * only on *Log*, *Stop, log later* or *Discard*; and a switch to another
 * ticket must not start the new timer before the reader has said what happens
 * to the old one (R-13.2).
 */
import { expect, test } from "vitest";

import type { AdHocBlock, Block, Draft, ReaderDay, RunningTimer, TimerTarget } from "../ipc/time";
import type { ToastSpec } from "../shell/toasts.svelte";
import { createDraftFlow, type DraftFlowPorts } from "./drafting.svelte";

const PAY: TimerTarget = { kind: "entity", entity_id: "jira:PAY-231" };
const OPS: TimerTarget = { kind: "entity", entity_id: "jira:OPS-12" };

function block(target: TimerTarget, id = 10): Block {
  return {
    id,
    started_at: "2026-09-03T09:00:00Z",
    ended_at: "2026-09-03T10:30:00Z",
    target,
    kind: "manual",
    ended_by_relaunch: false,
    worklog_id: null,
  };
}

const DRAFT: Draft = {
  entity_id: "jira:PAY-231",
  day: "2026-09-03",
  started_at: "2026-09-03T09:00:00Z",
  ended_at: "2026-09-03T10:30:00Z",
  seconds: 90 * 60,
  block_ids: [],
  running_since: "2026-09-03T09:00:00Z",
  candidates: [],
  comment: "",
};

interface Over {
  draft?: Draft | null;
  offer?: AdHocBlock | null;
  running?: TimerTarget | null;
  since?: string;
}

/** A clock and a backend that answer like the real ones, with a log of what was asked. */
function rig(over: Over = {}) {
  const log: string[] = [];
  const asked: ReaderDay[] = [];
  const adHocAsked: ReaderDay[] = [];
  const toasts: ToastSpec[] = [];
  const since = over.since ?? "2026-09-03T09:00:00Z";
  let running: RunningTimer | null =
    over.running === null ? null : { target: over.running ?? PAY, started_at: since, last_heartbeat: since };
  const ports: DraftFlowPorts = {
    timer: {
      get current() {
        return running;
      },
      stop: () => {
        log.push("stop");
        const closed = running ? { ...block(running.target), started_at: running.started_at } : null;
        running = null;
        return Promise.resolve(closed);
      },
      start: (target) => {
        log.push(`start ${target.kind === "entity" ? target.entity_id : target.label}`);
        running = { target, started_at: "2026-09-03T10:31:00Z", last_heartbeat: "2026-09-03T10:31:00Z" };
        return Promise.resolve();
      },
    },
    adHocBlock: (id, when) => {
      log.push(`ad-hoc ${id}`);
      adHocAsked.push(when);
      return Promise.resolve(over.offer ?? null);
    },
    worklogDraft: (entityId, when) => {
      log.push(`draft ${entityId}`);
      asked.push(when);
      return Promise.resolve(over.draft === undefined ? DRAFT : over.draft);
    },
    onadhoc: () => log.push("ad-hoc dialog"),
    push: (toast) => void toasts.push(toast),
    complain: (error) => log.push(`complain ${String(error)}`),
    onday: () => log.push("open the day"),
  };
  return { flow: createDraftFlow(ports), log, asked, adHocAsked, toasts, running: () => running };
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));
const starts = (log: string[]) => log.filter((line) => line.startsWith("start"));

test("a stop on a ticket opens its draft with the clock still running", async () => {
  const { flow, log, running } = rig();
  flow.stop();
  await settle();
  expect(flow.open?.draft).toEqual(DRAFT);
  expect(flow.open?.running).toEqual(PAY);
  expect(log).toEqual(["draft jira:PAY-231"]);
  expect(running()?.target, "the chip reads the timer behind the draft").toEqual(PAY);
});

test("the draft is asked for the day the running stretch started on", async () => {
  const { flow, asked } = rig({ since: "2026-09-02T21:30:00Z" });
  flow.stop();
  await settle();
  // The suite runs in Europe/Berlin: 21:30Z is 23:30 on the 2nd.
  expect(asked.map((when) => when.day)).toEqual(["2026-09-02"]);
});

/**
 * A day's offset is the one at its own local midnight, the offset `log_work`
 * is sent (#1978). The suite runs in Europe/Berlin: 2026-10-25 starts on +120
 * and is on +60 by 03:00; 2026-03-29 starts on +60 and is on +120 by 03:00. A
 * stretch started at 20:00Z on either day carries the midnight offset, not the
 * one in force when it started.
 */
const CLOCK_CHANGES = [
  { day: "2026-10-25", midnight: 120 },
  { day: "2026-03-29", midnight: 60 },
];

test.each(CLOCK_CHANGES)("the draft for a stretch started on $day is asked with that day's midnight offset", async ({ day, midnight }) => {
  const { flow, asked } = rig({ since: `${day}T20:00:00Z` });
  flow.stop();
  await settle();
  expect(asked).toEqual([{ day, offsetMinutes: midnight }]);
});

test.each(CLOCK_CHANGES)("the ad-hoc check after a stop on $day is asked with that day's midnight offset", async ({ day, midnight }) => {
  const { flow, adHocAsked } = rig({ draft: null, since: `${day}T20:00:00Z` });
  flow.stop();
  await settle();
  expect(adHocAsked).toEqual([{ day, offsetMinutes: midnight }]);
});

test("Keep running closes the draft and leaves the clock alone", async () => {
  const { flow, log, running } = rig();
  flow.stop();
  await settle();
  flow.settle("keep");
  await settle();
  expect(flow.open).toBeNull();
  expect(log).toEqual(["draft jira:PAY-231"]);
  expect(running()?.target).toEqual(PAY);
});

test("Stop, log later stops the clock and says where the time is", async () => {
  const { flow, log, toasts, running } = rig();
  flow.stop();
  await settle();
  flow.settle("later");
  await settle();
  expect(flow.open).toBeNull();
  expect(running()).toBeNull();
  expect(log).toEqual(["draft jira:PAY-231", "stop"]);
  expect(toasts.map((toast) => toast.text)).toEqual(["Timer stopped. The time is kept in today's blocks, unlogged."]);
  expect(toasts[0]?.action?.label).toBe("Open the day");
  toasts[0]?.action?.run();
  expect(log.at(-1)).toBe("open the day");
});

test("Log stops the clock before anything is logged, and once", async () => {
  const { flow, log, running } = rig();
  flow.stop();
  await settle();
  await flow.stopClock();
  expect(running(), "the block exists before log_work re-derives the day").toBeNull();
  flow.settle("logged");
  await settle();
  expect(log).toEqual(["draft jira:PAY-231", "stop"]);
});

test("Keep running after a Log that failed puts the clock back on the ticket", async () => {
  const { flow, log, running } = rig();
  flow.stop();
  await settle();
  await flow.stopClock();
  flow.settle("keep");
  await settle();
  expect(log.at(-1)).toBe("start jira:PAY-231");
  expect(running()?.target).toEqual(PAY);
});

test("Discard after a stop stops the clock and writes nothing", async () => {
  const { flow, log, toasts, running } = rig();
  flow.stop();
  await settle();
  flow.settle("discard");
  await settle();
  expect(running()).toBeNull();
  expect(log).toEqual(["draft jira:PAY-231", "stop"]);
  expect(toasts).toEqual([]);
});

test("a stop with nothing to draft stops the clock and offers the ad-hoc dialog's check", async () => {
  const { flow, log } = rig({ draft: null });
  flow.stop();
  await settle();
  expect(flow.open).toBeNull();
  expect(log).toEqual(["draft jira:PAY-231", "stop", "ad-hoc 10"]);
});

test("starting on another ticket while one runs opens the running one's draft first, and starts nothing yet", async () => {
  const { flow, log, running } = rig();
  await flow.switchTo(OPS, "OPS-12");
  await settle();
  expect(log).toEqual(["draft jira:PAY-231"]);
  expect(flow.open?.after?.target).toEqual(OPS);
  expect(running()?.target, "the new timer started before the reader answered the draft").toEqual(PAY);
});

test("logging the switch's draft starts the timer it was waiting on", async () => {
  const { flow, log, toasts } = rig();
  await flow.switchTo(OPS, "OPS-12");
  await settle();
  await flow.stopClock();
  flow.settle("logged");
  await settle();
  expect(log).toEqual(["draft jira:PAY-231", "stop", "start jira:OPS-12"]);
  expect(toasts.map((toast) => toast.text)).toContain("Timing OPS-12.");
});

test("Stop, log later on a switch stops the old timer, then starts the waiting one", async () => {
  const { flow, log } = rig();
  await flow.switchTo(OPS, "OPS-12");
  await settle();
  flow.settle("later");
  await settle();
  expect(log).toEqual(["draft jira:PAY-231", "stop", "start jira:OPS-12"]);
});

test("Keep running on a switch keeps the old ticket and drops the new one, saying so", async () => {
  const { flow, log, running, toasts } = rig();
  await flow.switchTo(OPS, "OPS-12");
  await settle();
  flow.settle("keep");
  await settle();
  expect(starts(log)).toEqual([]);
  expect(running()?.target).toEqual(PAY);
  expect(toasts.map((toast) => toast.text)).toEqual(["Not started on OPS-12: the timer kept running."]);
});

test("a switch away from something with no draft starts the new timer at once", async () => {
  const { flow, log } = rig({ draft: null });
  await flow.switchTo(OPS, "OPS-12");
  await settle();
  expect(flow.open).toBeNull();
  expect(log).toEqual(["draft jira:PAY-231", "stop", "ad-hoc 10", "start jira:OPS-12"]);
});

test("a switch away from a label opens the ad-hoc dialog and starts the new timer", async () => {
  const offer = { suggestion: null } as unknown as AdHocBlock;
  const { flow, log } = rig({ offer, running: { kind: "label", label: "DB config" } });
  await flow.switchTo(OPS, "OPS-12");
  await settle();
  expect(log).toEqual(["stop", "ad-hoc 10", "ad-hoc dialog", "start jira:OPS-12"]);
});

test("with nothing running, a start is a start", async () => {
  const { flow, log } = rig({ running: null });
  await flow.switchTo(OPS, "OPS-12");
  expect(log).toEqual(["start jira:OPS-12"]);
});

test("a draft shown without a stop is not marked as one, and Discard closes it with nothing stopped or started", async () => {
  const { flow, log } = rig();
  flow.show({ ...DRAFT, running_since: null });
  expect(flow.open?.running).toBeNull();
  await flow.stopClock();
  flow.settle("discard");
  await settle();
  expect(flow.open).toBeNull();
  expect(log).toEqual([]);
});

test("Log on a draft shown without a stop that counts the running stretch stops the clock first, then starts it again", async () => {
  // The Day tab's *Log…* on the ticket being timed: `log_work` covers only
  // closed blocks, so a stretch the draft counted but left running would be
  // logged now and offered again at the next stop.
  const { flow, log, running } = rig();
  flow.show(DRAFT);
  expect(flow.open?.running, "no stop opened it").toBeNull();
  await flow.stopClock();
  expect(log).toEqual(["stop"]);
  flow.settle("logged");
  await settle();
  expect(log).toEqual(["stop", "start jira:PAY-231"]);
  expect(running()?.target).toEqual(PAY);
});

test("a shown draft whose stretch is on another ticket's clock leaves that clock alone", async () => {
  const { flow, log } = rig({ running: OPS });
  flow.show(DRAFT);
  await flow.stopClock();
  flow.settle("logged");
  await settle();
  expect(log).toEqual([]);
});

test("Discard on a switch's draft still starts the timer it was waiting on", async () => {
  const { flow, log } = rig();
  await flow.switchTo(OPS, "OPS-12");
  await settle();
  flow.settle("discard");
  await settle();
  expect(log).toEqual(["draft jira:PAY-231", "stop", "start jira:OPS-12"]);
});

test("a start or stop while a draft is open or being read is refused, so it cannot run under the dialog", async () => {
  const { flow, log, toasts } = rig();
  await flow.switchTo(OPS, "OPS-12");
  expect(flow.busy, "the draft's read is in flight").toBe(true);
  await flow.switchTo(PAY, "PAY-231");
  await settle();
  expect(flow.open).not.toBeNull();
  flow.stop();
  expect(starts(log)).toEqual([]);
  expect(log).not.toContain("stop");
  expect(toasts.map((toast) => toast.text)).toEqual(["Answer the worklog draft first.", "Answer the worklog draft first."]);
  flow.settle("later");
  expect(flow.busy, "the stop behind the answer is still in flight").toBe(true);
  await settle();
  expect(flow.busy).toBe(false);
});

/**
 * What a stop opens in the ticket-first shell, and what each of the draft's
 * buttons does to the clock (#826, R-13.1 to R-13.3).
 *
 * The claims that matter are about **time not getting lost**: a switch to
 * another ticket must not start the new timer before the reader has said what
 * happens to the old one (R-13.2), and *Keep running* must put the clock back
 * on the ticket the stop took it off.
 */
import { expect, test } from "vitest";

import type { AdHocBlock, Block, Draft, RunningTimer, TimerTarget } from "../ipc/time";
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
  block_ids: [10],
  candidates: [],
  comment: "",
};

/** A clock and a backend that answer like the real ones, with a log of what was asked. */
function rig(over: { draft?: Draft | null; offer?: AdHocBlock | null; running?: TimerTarget | null } = {}) {
  const log: string[] = [];
  const toasts: ToastSpec[] = [];
  let running: RunningTimer | null =
    over.running === null
      ? null
      : { target: over.running ?? PAY, started_at: "2026-09-03T09:00:00Z", last_heartbeat: "2026-09-03T09:00:00Z" };
  const ports: DraftFlowPorts = {
    timer: {
      get current() {
        return running;
      },
      stop: () => {
        log.push("stop");
        const closed = running ? block(running.target) : null;
        running = null;
        return Promise.resolve(closed);
      },
      start: (target) => {
        log.push(`start ${target.kind === "entity" ? target.entity_id : target.label}`);
        running = { target, started_at: "2026-09-03T10:31:00Z", last_heartbeat: "2026-09-03T10:31:00Z" };
        return Promise.resolve();
      },
    },
    adHocBlock: (id) => {
      log.push(`ad-hoc ${id}`);
      return Promise.resolve(over.offer ?? null);
    },
    worklogDraft: (entityId) => {
      log.push(`draft ${entityId}`);
      return Promise.resolve(over.draft === undefined ? DRAFT : over.draft);
    },
    onadhoc: () => log.push("ad-hoc dialog"),
    push: (toast) => void toasts.push(toast),
    complain: (error) => log.push(`complain ${String(error)}`),
    onday: () => log.push("open the day"),
  };
  return { flow: createDraftFlow(ports), log, toasts, running: () => running };
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

test("a stop on a ticket opens its draft, marked as opened by a stop", async () => {
  const { flow } = rig();
  flow.stopped(block(PAY));
  await settle();
  expect(flow.open?.draft).toEqual(DRAFT);
  expect(flow.open?.stopped).toEqual(PAY);
});

test("Keep running puts the clock back on the ticket the stop took it off", async () => {
  const { flow, log, running } = rig({ running: null });
  flow.stopped(block(PAY));
  await settle();
  flow.settle("keep");
  await settle();
  expect(flow.open).toBeNull();
  expect(log.at(-1)).toBe("start jira:PAY-231");
  expect(running()?.target).toEqual(PAY);
});

test("Stop, log later leaves the clock stopped and says where the time is", async () => {
  const { flow, log, toasts, running } = rig({ running: null });
  flow.stopped(block(PAY));
  await settle();
  flow.settle("later");
  await settle();
  expect(flow.open).toBeNull();
  expect(running()).toBeNull();
  expect(toasts.map((toast) => toast.text)).toEqual(["Timer stopped. The time is kept in today's blocks, unlogged."]);
  expect(toasts[0]?.action?.label).toBe("Open the day");
  toasts[0]?.action?.run();
  expect(log.at(-1)).toBe("open the day");
});

test("starting on another ticket while one runs opens the running one's draft first, and starts nothing yet", async () => {
  const { flow, log, running } = rig();
  await flow.switchTo(OPS, "OPS-12");
  await settle();
  expect(log).toEqual(["stop", "ad-hoc 10", "draft jira:PAY-231"]);
  expect(flow.open?.after?.target).toEqual(OPS);
  expect(running(), "the new timer started before the reader answered the draft").toBeNull();
});

test("logging the switch's draft starts the timer it was waiting on", async () => {
  const { flow, log, toasts } = rig();
  await flow.switchTo(OPS, "OPS-12");
  await settle();
  flow.settle("logged");
  await settle();
  expect(log.at(-1)).toBe("start jira:OPS-12");
  expect(toasts.map((toast) => toast.text)).toContain("Timing OPS-12.");
});

test("Stop, log later on a switch starts the waiting timer too", async () => {
  const { flow, log } = rig();
  await flow.switchTo(OPS, "OPS-12");
  await settle();
  flow.settle("later");
  await settle();
  expect(log.at(-1)).toBe("start jira:OPS-12");
});

test("Keep running on a switch keeps the old ticket and drops the new one, saying so", async () => {
  const { flow, log, running, toasts } = rig();
  await flow.switchTo(OPS, "OPS-12");
  await settle();
  flow.settle("keep");
  await settle();
  expect(log.filter((line) => line.startsWith("start"))).toEqual(["start jira:PAY-231"]);
  expect(running()?.target).toEqual(PAY);
  expect(toasts.map((toast) => toast.text)).toEqual(["Not started on OPS-12: the timer kept running."]);
});

test("a switch away from something with no draft starts the new timer at once", async () => {
  const { flow, log } = rig({ draft: null });
  await flow.switchTo(OPS, "OPS-12");
  await settle();
  expect(flow.open).toBeNull();
  expect(log.at(-1)).toBe("start jira:OPS-12");
});

test("a switch away from a label opens the ad-hoc dialog and starts the new timer", async () => {
  const offer = { suggestion: null } as unknown as AdHocBlock;
  const { flow, log } = rig({ offer, running: { kind: "label", label: "DB config" } });
  await flow.switchTo(OPS, "OPS-12");
  await settle();
  expect(log).toContain("ad-hoc dialog");
  expect(log.at(-1)).toBe("start jira:OPS-12");
});

test("with nothing running, a start is a start", async () => {
  const { flow, log } = rig({ running: null });
  await flow.switchTo(OPS, "OPS-12");
  expect(log).toEqual(["start jira:OPS-12"]);
});

test("a draft shown without a stop is not marked as one, and Discard closes it with nothing started", async () => {
  const { flow, log } = rig({ running: null });
  flow.show(DRAFT);
  expect(flow.open?.stopped).toBeNull();
  flow.settle("discard");
  await settle();
  expect(flow.open).toBeNull();
  expect(log).toEqual([]);
});

test("Discard on a switch's draft still starts the timer it was waiting on", async () => {
  const { flow, log } = rig();
  await flow.switchTo(OPS, "OPS-12");
  await settle();
  flow.settle("discard");
  await settle();
  expect(log.at(-1)).toBe("start jira:OPS-12");
});

test("a start while a draft is open or being read is refused, so it cannot run under the dialog", async () => {
  const { flow, log, toasts } = rig();
  await flow.switchTo(OPS, "OPS-12");
  expect(flow.busy, "the draft's read is in flight").toBe(true);
  await flow.switchTo(PAY, "PAY-231");
  await settle();
  expect(flow.open).not.toBeNull();
  await flow.switchTo(PAY, "PAY-231");
  expect(log.filter((line) => line.startsWith("start"))).toEqual([]);
  expect(toasts.map((toast) => toast.text)).toEqual(["Answer the worklog draft first.", "Answer the worklog draft first."]);
  flow.settle("later");
  expect(flow.busy).toBe(false);
});

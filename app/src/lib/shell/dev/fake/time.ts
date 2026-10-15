/**
 * `?fake-ipc`: The timer, the day's blocks and the worklog draft (#826), and the rooms
 * UI's week timesheet.
 */
import type { Handler } from "../fake-tauri";
import { CORPUS } from "./corpus";

/**
 * `week_timesheet` (#816's *Time this week*): PAY-231 logged on Monday and
 * Tuesday, and tracked but unlogged on the fourth day asked for.
 */
function fakeWeek(args: Record<string, unknown>) {
  const days = ((args["days"] as { day: string }[] | undefined) ?? []).map((window) => window.day);
  const cell = (logged: number, unlogged: number) => ({
    tracked_seconds: logged + unlogged,
    offered_seconds: 0,
    logged_seconds: logged,
    held_seconds: 0,
    unlogged_seconds: unlogged,
  });
  const PLAN: [number, number][] = [[3 * 3600, 0], [4.5 * 3600, 0], [0, 0], [0, 4 * 3600 + 14 * 60]];
  return {
    days,
    rows: [
      {
        target: { kind: "entity", entity_id: "mock:PAY-231" },
        title: "Retry failed SEPA payouts",
        cells: days.map((_, index) => cell(...(PLAN[index] ?? [0, 0]))),
      },
    ],
    past_horizon: days.map(() => false),
  };
}

// ---------------------------------------------------------------- time (#826)

type FakeTarget = { kind: "entity"; entity_id: string } | { kind: "label"; label: string };
interface FakeBlock {
  id: number;
  started_at: string;
  ended_at: string;
  target: FakeTarget;
  kind: "manual";
  ended_by_relaunch: boolean;
  worklog_id: number | null;
}

/**
 * A clock that keeps what it is told in memory, and a draft built from its
 * blocks: the sum of the day's unlogged blocks on the ticket, and a fixed set
 * of candidates on PAY-231 so the dialog has boxes to draw.
 */
function fakeTime(params: URLSearchParams): Record<string, Handler> {
  const minutesAgo = (minutes: number) => new Date(Date.now() - minutes * 60_000).toISOString();
  let nextBlock = 501;
  let nextWorklog = 71;
  const blocks: FakeBlock[] = [];
  let running: { target: FakeTarget; started_at: string; last_heartbeat: string } | null = null;
  if (params.has("fake-timer")) {
    const pay: FakeTarget = { kind: "entity", entity_id: "mock:PAY-231" };
    blocks.push({
      id: nextBlock++,
      started_at: minutesAgo(298),
      ended_at: minutesAgo(168),
      target: pay,
      kind: "manual",
      ended_by_relaunch: false,
      worklog_id: null,
    });
    running = { target: pay, started_at: minutesAgo(34), last_heartbeat: minutesAgo(0) };
  }
  const titleOf = (target: FakeTarget) =>
    target.kind === "entity" ? (CORPUS.find((row) => row.entity_id === target.entity_id)?.title ?? null) : null;
  const unlogged = (entityId: string) =>
    blocks.filter(
      (block) => block.worklog_id === null && block.target.kind === "entity" && block.target.entity_id === entityId,
    );
  const candidates = (from: string) => {
    const at = (minutes: number) => new Date(new Date(from).getTime() + minutes * 60_000).toISOString();
    return [
      { id: "item:mock:9f2c1ab", source: "mirror", at: at(22), entity_id: "mock:9f2c1ab", bullet: "- commit 9f2c1ab payout: key the retry on the mandate id" },
      { id: "write:31", source: "write", at: at(40), entity_id: "mock:payout-service#142", bullet: "- replied to Jonas on PR #142" },
      { id: "activity:4211", source: "activity", at: at(125), entity_id: "mock:payout-service#318", bullet: "- ran payout-service #318" },
      { id: "item:mock:ENG-SEPA", source: "mirror", at: at(60), entity_id: "mock:ENG-SEPA", bullet: "- edited SEPA retry design" },
    ].sort((a, b) => a.at.localeCompare(b.at));
  };
  return {
    current_timer: () => running,
    timer_heartbeat: () => running,
    start_timer: (args) => {
      if (running) throw { code: "conflict", message: "A timer is already running.", source_id: null };
      const now = new Date().toISOString();
      running = { target: args["target"] as FakeTarget, started_at: now, last_heartbeat: now };
      return running;
    },
    stop_timer: () => {
      if (!running) return null;
      const block: FakeBlock = {
        id: nextBlock++,
        started_at: running.started_at,
        ended_at: new Date().toISOString(),
        target: running.target,
        kind: "manual",
        ended_by_relaunch: false,
        worklog_id: null,
      };
      blocks.push(block);
      running = null;
      return block;
    },
    // A block on a ticket: the draft is what opens.
    ad_hoc_block: () => null,
    day_blocks: (args) => {
      const from = String(args["from"]);
      const to = String(args["to"]);
      return {
        past_horizon: false,
        blocks: blocks
          .filter((block) => block.started_at < to && block.ended_at > from)
          .map((block) => ({ block, title: titleOf(block.target) })),
      };
    },
    worklog_draft: (args) => {
      const entityId = String(args["entityId"]);
      if (!entityId.startsWith("mock:")) return null;
      const spans = unlogged(entityId);
      if (spans.length === 0) return null;
      const started_at = spans[0]!.started_at;
      const ended_at = spans.at(-1)!.ended_at;
      const seconds = spans.reduce(
        (sum, block) => sum + Math.round((Date.parse(block.ended_at) - Date.parse(block.started_at)) / 1000),
        0,
      );
      const seen = entityId === "mock:PAY-231" ? candidates(started_at) : [];
      return {
        entity_id: entityId,
        day: String(args["day"]),
        started_at,
        ended_at,
        seconds,
        block_ids: spans.map((block) => block.id),
        candidates: seen,
        comment: seen.map((candidate) => candidate.bullet).join("\n"),
      };
    },
    log_work: (args) => {
      const entityId = String(args["entityId"]);
      const spans = unlogged(entityId);
      const id = nextWorklog++;
      for (const block of spans) block.worklog_id = id;
      return {
        id,
        entity_id: entityId,
        started_at: String(args["startedAt"]),
        seconds: Number(args["seconds"]),
        comment: String(args["comment"]),
        block_ids: spans.map((block) => block.id),
        write_queue_id: null,
        remote_id: null,
        created_at: new Date().toISOString(),
      };
    },
  };
}

/** This feature's commands. */
export function handlers(params: URLSearchParams): Record<string, Handler> {
  return {
    // The timer, the day's blocks and the worklog draft (#826), so a stop under
    // `?fake-ipc` opens the draft a screenshot is taken of. `fake-timer` starts
    // the window with a timer on PAY-231 and a morning block before it; without
    // it nothing runs. Fixture-only: the rules are witnessed over a real
    // database in `time_ipc.rs` and `worklog_ipc.rs`.
    ...fakeTime(params),
    week_timesheet: (args) => fakeWeek(args),
  };
}

/**
 * `?fake-ipc`: The timer, the day's blocks and their edits (#826, #827), the worklog draft, and the rooms
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
  // "No target, app open": 35 minutes today, for the Day tab's fifth figure (#827).
  const now = new Date().toISOString();
  const windows = (args["days"] as { from: string; to: string }[] | undefined) ?? [];
  const open = windows.map((window) => (window.from <= now && now < window.to ? 35 * 60 : 0));
  return {
    days,
    rows: [
      {
        target: { kind: "entity", entity_id: "mock:PAY-231" },
        title: "Retry failed SEPA payouts",
        cells: days.map((_, index) => cell(...(PLAN[index] ?? [0, 0]))),
      },
      ...(open.some((seconds) => seconds > 0)
        ? [{ target: null, title: null, cells: open.map((seconds) => ({ ...cell(0, 0), tracked_seconds: seconds })) }]
        : []),
    ],
    past_horizon: days.map(() => false),
  };
}

// ---------------------------------------------------------- time (#826, #827)

type FakeTarget = { kind: "entity"; entity_id: string } | { kind: "label"; label: string };
interface FakeBlock {
  id: number;
  started_at: string;
  ended_at: string;
  target: FakeTarget;
  kind: "manual" | "passive";
  ended_by_relaunch: boolean;
  worklog_id: number | null;
}

/** Today at `hour:minute`, by the browser's clock. */
function todayAt(hour: number, minute: number): string {
  const at = new Date();
  at.setHours(hour, minute, 0, 0);
  return at.toISOString();
}

/**
 * Today's blocks, prototype-6's Friday (#827): PAY-228 logged, a passive
 * stretch on PR #142, PAY-231 unlogged, a gap, and a label block. Every row
 * state the Day tab draws but the running timer, which `fake-timer` adds.
 */
function todaysBlocks(): FakeBlock[] {
  const entity = (entity_id: string): FakeTarget => ({ kind: "entity", entity_id });
  const manual = { kind: "manual" as const, ended_by_relaunch: false, worklog_id: null };
  return [
    { id: 1, started_at: todayAt(8, 50), ended_at: todayAt(9, 25), target: entity("mock:PAY-228"), ...manual, worklog_id: 41 },
    {
      id: 2,
      started_at: todayAt(9, 25),
      ended_at: todayAt(9, 40),
      target: entity("mock:payout-service#142"),
      ...manual,
      kind: "passive",
    },
    { id: 3, started_at: todayAt(9, 40), ended_at: todayAt(11, 50), target: entity("mock:PAY-231"), ...manual },
    {
      id: 5,
      started_at: todayAt(13, 10),
      ended_at: todayAt(13, 58),
      target: { kind: "label", label: "Standup and planning" },
      ...manual,
    },
  ];
}

/** Thirty days back: a day before this is past the observation horizon. */
function fakeHorizon(): string {
  const cut = new Date();
  cut.setDate(cut.getDate() - 30);
  return cut.toISOString();
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
  // The day's blocks are always there (#827); PAY-231's 09:40 to 11:50 is
  // also what a stop's draft adds up.
  let blocks: FakeBlock[] = todaysBlocks();
  let running: { target: FakeTarget; started_at: string; last_heartbeat: string } | null = null;
  if (params.has("fake-timer")) {
    const pay: FakeTarget = { kind: "entity", entity_id: "mock:PAY-231" };
    running = { target: pay, started_at: minutesAgo(34), last_heartbeat: minutesAgo(0) };
  }
  /** A block the reader may change: there, and not logged. */
  const editable = (id: unknown) => {
    const block = blocks.find((candidate) => candidate.id === Number(id));
    if (!block) throw { code: "not_found", message: `there is no block ${String(id)}`, source_id: null };
    if (block.worklog_id !== null) {
      throw { code: "invalid", message: "This block is logged, so it is read-only.", source_id: null };
    }
    return block;
  };
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
    // `null` for a block on a ticket, so the draft is what opens; otherwise
    // knobas' suggestion: the PR is linked to PAY-231, a label gets the ticket
    // logged last (#827).
    ad_hoc_block: (args) => {
      const block = blocks.find((candidate) => candidate.id === Number(args["blockId"]));
      if (!block) throw { code: "not_found", message: "there is no such block", source_id: null };
      if (block.target.kind === "entity" && /^mock:[A-Z]+-\d+$/.test(block.target.entity_id)) return null;
      const linked = block.target.kind === "entity";
      const entity_id = linked ? "mock:PAY-231" : "mock:PAY-228";
      return {
        suggestion: {
          entity_id,
          title: titleOf({ kind: "entity", entity_id }),
          rule: linked ? "linked_to_target" : "last_logged",
        },
      };
    },
    day_blocks: (args) => {
      const from = String(args["from"]);
      const to = String(args["to"]);
      return {
        past_horizon: from < fakeHorizon(),
        blocks: blocks
          .filter((block) => block.started_at < to && block.ended_at > from)
          .sort((a, b) => a.started_at.localeCompare(b.started_at))
          .map((block) => ({ block, title: titleOf(block.target) })),
      };
    },
    // The Day tab's edits (#827): in memory, so a click visibly does something.
    update_block: (args) => {
      const block = editable(args["id"]);
      Object.assign(block, {
        started_at: String(args["startedAt"]),
        ended_at: String(args["endedAt"]),
        target: args["target"] as FakeTarget,
        kind: "manual",
        ended_by_relaunch: false,
      });
      return { block, title: titleOf(block.target) };
    },
    create_block: (args) => {
      const block: FakeBlock = {
        id: nextBlock++,
        started_at: String(args["startedAt"]),
        ended_at: String(args["endedAt"]),
        target: args["target"] as FakeTarget,
        kind: "manual",
        ended_by_relaunch: false,
        worklog_id: null,
      };
      blocks.push(block);
      return { block, title: titleOf(block.target) };
    },
    delete_block: (args) => {
      const block = editable(args["id"]);
      blocks = blocks.filter((candidate) => candidate !== block);
      return null;
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

/**
 * `?fake-ipc`: The timer, the day's blocks and their edits (#826, #827), the worklog draft, and the rooms
 * UI's week timesheet.
 */
import type { Handler } from "../fake-tauri";
import { dayKey } from "../../../time/day";
import { CORPUS } from "./corpus";

/**
 * `week_timesheet` (#816's *Time this week*, #828's Week tab): prototype-6's
 * `tmWeekRows`, Monday first, as [tracked, logged, held] minutes per day.
 *
 * Five rows: PAY-231 (Friday unlogged), PAY-228 (an hour held on Thursday),
 * OPS-77, a label ("Standup and planning") and "No target, app open". A
 * window past the fifth day is empty, as a weekend is.
 *
 * Divergences from prototype-6: its label row's note ("Logged to PAY-200 each
 * day") has no field in `WeekRow`, so nothing here carries it; the row titles
 * are the corpus's, not the prototype's shorter ones; and the numbers are the
 * same whatever week is asked for, where the prototype answers "everything
 * logged" for the week before. Replaces #816's three-day plan, which nothing
 * else read.
 */
const WEEK_PLAN: {
  target: { kind: "entity"; entity_id: string } | { kind: "label"; label: string };
  title: string | null;
  days: [number, number, number][];
}[] = [
  {
    target: { kind: "entity", entity_id: "mock:PAY-231" },
    title: "Retry failed SEPA payouts",
    days: [[0, 0, 0], [90, 90, 0], [180, 180, 0], [270, 180, 0], [164, 0, 0]],
  },
  {
    target: { kind: "entity", entity_id: "mock:PAY-228" },
    title: "Fix ledger drift on partial refunds",
    days: [[120, 120, 0], [60, 60, 0], [90, 90, 0], [160, 100, 60], [35, 35, 0]],
  },
  {
    target: { kind: "entity", entity_id: "mock:OPS-77" },
    title: "Rotate the staging database credentials",
    days: [[60, 60, 0], [30, 30, 0], [0, 0, 0], [0, 0, 0], [0, 0, 0]],
  },
  {
    target: { kind: "label", label: "Standup and planning" },
    title: null,
    days: [[30, 30, 0], [30, 30, 0], [30, 30, 0], [30, 30, 0], [48, 0, 0]],
  },
];
/** Prototype-6's "No target, app open" row, in minutes per day. */
const WEEK_OPEN = [18, 12, 36, 6, 35];

function fakeWeek(args: Record<string, unknown>) {
  const windows = (args["days"] as { day: string }[] | undefined) ?? [];
  const days = windows.map((window) => window.day);
  const cell = (tracked: number, logged: number, held: number) => ({
    tracked_seconds: tracked * 60,
    offered_seconds: 0,
    logged_seconds: logged * 60,
    held_seconds: held * 60,
    unlogged_seconds: Math.max(0, tracked - logged - held) * 60,
  });
  const rows: unknown[] = WEEK_PLAN.map((row) => ({
    target: row.target,
    title: row.title,
    cells: windows.map((_, index) => cell(...(row.days[index] ?? [0, 0, 0]))),
  }));
  rows.push({ target: null, title: null, cells: windows.map((_, index) => cell(WEEK_OPEN[index] ?? 0, 0, 0)) });
  return { days, rows, past_horizon: days.map(() => false) };
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

/** Today's focused time with no target (#901): prototype-6's 12:35 to 13:10. */
function todaysOpen(): { started_at: string; ended_at: string }[] {
  return [{ started_at: todayAt(12, 35), ended_at: todayAt(13, 10) }];
}

/**
 * Prototype-6's Thu 21 PAY-231 row in *Log all*, which no block backs: one
 * place for its start hour, length and comment (#828).
 */
const YESTERDAY_ROW = {
  entity_id: "mock:PAY-231",
  hour: 10,
  seconds: 90 * 60,
  comment: "Retry classification for TEMP_UNAVAILABLE; review on PR #142.",
};

/** This time yesterday, by the browser's clock. */
function yesterdayAt(): Date {
  const at = new Date();
  at.setDate(at.getDate() - 1);
  return at;
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
  let lastWindows: { day: string; from: string; to: string }[] = [];
  let loggedYesterday = false;
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
      const claimed = [...blocks, ...(running ? [{ started_at: running.started_at, ended_at: new Date().toISOString() }] : [])];
      return {
        past_horizon: from < fakeHorizon(),
        blocks: blocks
          .filter((block) => block.started_at < to && block.ended_at > from)
          .sort((a, b) => a.started_at.localeCompare(b.started_at))
          .map((block) => ({ block, title: titleOf(block.target) })),
        // Prototype-6's open stretch (#901), 12:35 to 13:10, the 35 minutes the
        // week's Friday "No target, app open" cell holds. Gone, whole, once a
        // block or the running timer claims any of it.
        open: todaysOpen().filter(
          (stretch) =>
            stretch.started_at < to &&
            stretch.ended_at > from &&
            !claimed.some((claim) => claim.started_at < stretch.ended_at && claim.ended_at > stretch.started_at),
        ),
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
    // *Dismiss* (#900): only an offer, and it stays gone.
    dismiss_block: (args) => {
      const block = editable(args["id"]);
      if (block.kind !== "passive") {
        throw { code: "invalid", message: "only a block knobas offered can be dismissed", source_id: null };
      }
      blocks = blocks.filter((candidate) => candidate !== block);
      return null;
    },
    worklog_draft: (args) => {
      const entityId = String(args["entityId"]);
      if (!entityId.startsWith("mock:")) return null;
      const day = String(args["day"]);
      // A running stretch that started on this day and ticket is counted below (#896).
      const runningThatDay =
        running?.target.kind === "entity" &&
        running.target.entity_id === entityId &&
        dayKey(new Date(running.started_at)) === day;
      // Yesterday's PAY-231 (#828): *Log all*'s fixed row, with prototype-6's comment.
      // Not when the timer started yesterday (just before midnight): then the draft is its stretch (#1031).
      if (entityId === YESTERDAY_ROW.entity_id && day === dayKey(yesterdayAt()) && !loggedYesterday && !runningThatDay) {
        const at = yesterdayAt();
        at.setHours(YESTERDAY_ROW.hour, 0, 0, 0);
        const started_at = at.toISOString();
        const { seconds, comment } = YESTERDAY_ROW;
        return {
          entity_id: entityId,
          day,
          started_at,
          ended_at: started_at,
          seconds,
          block_ids: [],
          running_since: null,
          candidates: [],
          comment,
        };
      }
      const spans = unlogged(entityId);
      // The running timer's stretch up to now, on its ticket and the day it started (#896).
      const running_since = runningThatDay ? running!.started_at : null;
      const all = [...spans, ...(running_since ? [{ started_at: running_since, ended_at: new Date().toISOString() }] : [])];
      all.sort((a, b) => a.started_at.localeCompare(b.started_at));
      if (all.length === 0) return null;
      const started_at = all[0]!.started_at;
      const ended_at = all.reduce((last, span) => (span.ended_at > last ? span.ended_at : last), all[0]!.ended_at);
      const seconds = all.reduce(
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
        running_since,
        candidates: seen,
        comment: seen.map((candidate) => candidate.bullet).join("\n"),
      };
    },
    // The day's unlogged blocks on the ticket, by the browser's clock: *Log all*
    // sends one per day and ticket (#828). Yesterday's fixed PAY-231 row has no
    // block, so logging it is remembered instead.
    log_work: (args) => {
      const entityId = String(args["entityId"]);
      const day = args["day"] === undefined ? null : String(args["day"]);
      if (entityId === YESTERDAY_ROW.entity_id && day === dayKey(yesterdayAt())) loggedYesterday = true;
      const spans = unlogged(entityId).filter((block) => day === null || dayKey(new Date(block.started_at)) === day);
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
    // The week's *Log all* (#828): one worklog per day and ticket from that
    // day's unlogged **manual** blocks on a ticket; a label or passive block
    // is never in it, and the running timer is not until it stops. Prototype-6
    // also lists yesterday's PAY-231 (1h 30m), which no block here backs, so
    // it is a fixed row for the window whose day is yesterday.
    log_all_preview: (args) => planLogAll(args),
    log_all: (args) => {
      const plan = planLogAll(args);
      if (plan.some((row) => row.blocks === 0)) loggedYesterday = true;
      return plan.map((row) => {
        const spans = row.blocks > 0 ? unloggedOn(row) : [];
        const id = nextWorklog++;
        for (const block of spans) block.worklog_id = id;
        return {
          id,
          entity_id: row.entity_id,
          started_at: row.started_at,
          seconds: row.seconds,
          comment: "",
          block_ids: spans.map((block) => block.id),
          write_queue_id: null,
          remote_id: null,
          created_at: new Date().toISOString(),
        };
      });
    },
  };

  /** The unlogged manual ticket blocks that start inside one planned day. */
  function unloggedOn(row: { day: string; entity_id: string }, windows = lastWindows) {
    const window = windows.find((candidate) => candidate.day === row.day);
    return blocks.filter(
      (block) =>
        window !== undefined &&
        block.worklog_id === null &&
        block.kind === "manual" &&
        block.target.kind === "entity" &&
        block.target.entity_id === row.entity_id &&
        block.started_at >= window.from &&
        block.started_at < window.to,
    );
  }

  function planLogAll(args: Record<string, unknown>) {
    const windows = (args["days"] as { day: string; from: string; to: string }[] | undefined) ?? [];
    lastWindows = windows;
    const yesterday = dayKey(yesterdayAt());
    const plan: { day: string; entity_id: string; title: string | null; started_at: string; seconds: number; blocks: number }[] = [];
    for (const window of windows) {
      const ids = new Set<string>();
      for (const block of blocks) {
        if (block.worklog_id !== null || block.kind !== "manual" || block.target.kind !== "entity") continue;
        if (block.started_at < window.from || block.started_at >= window.to) continue;
        if (/^mock:[A-Z]+-\d+$/.test(block.target.entity_id)) ids.add(block.target.entity_id);
      }
      for (const entity_id of [...ids].sort()) {
        const spans = unloggedOn({ day: window.day, entity_id }, [window]);
        plan.push({
          day: window.day,
          entity_id,
          title: titleOf({ kind: "entity", entity_id }),
          started_at: spans[0]!.started_at,
          seconds: spans.reduce((sum, block) => sum + Math.round((Date.parse(block.ended_at) - Date.parse(block.started_at)) / 1000), 0),
          blocks: spans.length,
        });
      }
      if (window.day === yesterday && !loggedYesterday) {
        plan.push({
          day: window.day,
          entity_id: YESTERDAY_ROW.entity_id,
          title: titleOf({ kind: "entity", entity_id: YESTERDAY_ROW.entity_id }),
          started_at: new Date(new Date(window.from).getTime() + YESTERDAY_ROW.hour * 3600_000).toISOString(),
          seconds: YESTERDAY_ROW.seconds,
          blocks: 0,
        });
      }
    }
    return plan;
  }
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

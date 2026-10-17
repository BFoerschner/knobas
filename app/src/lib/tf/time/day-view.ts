/**
 * The Time view's Day tab, as arithmetic (#827, R-13.4): which row each block
 * is, the five figures, and where the strip draws each row.
 *
 * Pure, over the reads: `day_blocks` for the blocks and the open stretches,
 * `ad_hoc_block` for whether a block is on a ticket and what knobas would
 * suggest when it is not, and the timer for the running block. The component
 * asks; this decides.
 *
 * ## Six states, one per row
 *
 * - `logged`: a manual block a worklog covers. Read-only (spec #272, story 20).
 * - `unlogged`: a manual block on a ticket, waiting for a worklog.
 * - `label`: a manual block on something a worklog cannot go to, a label or a
 *   page. *Log to <ticket>* when `ad_hoc_block` has a suggestion.
 * - `running`: the timer, from its start to now. Not a block until it stops,
 *   and the only row the strip draws yellow.
 * - `offered`: a passive block, knobas' guess at what was in front of you.
 * - `open`: focused time nothing claims (#901), from `day_blocks`' `open`:
 *   knobas was in front, with no timer and nothing it could name.
 *
 * Time between rows that is neither a block nor open is no row: knobas was not
 * in front, and that may be lunch as easily as work (R-13.4 draws no such row).
 *
 * Whether a block is on a ticket is the backend's answer (`ad_hoc_block` is
 * `null` for a block on a ticket), never a list of kinds kept here.
 */
import type { AdHocBlock, DayBlock, OpenStretch, RunningTimer, Suggestion, TimerTarget } from "../../ipc/time";
import { dayBounds, segmentsOf } from "../../time/day";

export type RowState = "logged" | "unlogged" | "label" | "running" | "offered" | "open";

export interface DayRow {
  key: string;
  state: RowState;
  /** RFC 3339. */
  from: string;
  /** RFC 3339; for the running row, now. */
  to: string;
  /** Milliseconds, so the figures add before they floor. */
  ms: number;
  /** The block behind the row; `null` for the running timer and open time. */
  block: DayBlock | null;
  /** What the row is on; `null` for open time. */
  target: TimerTarget | null;
  /** The ticket *Add to* or *Log to* names, or `null`. */
  suggestion: Suggestion | null;
}

export interface DayInput {
  /** `YYYY-MM-DD`, the reader's own day. */
  day: string;
  blocks: DayBlock[];
  /**
   * `ad_hoc_block`'s answer per block id: `null` is "on a ticket", an object
   * is "not a ticket" with knobas' suggestion, a missing id is not answered yet.
   */
  offers: Record<number, AdHocBlock | null | undefined>;
  /** The day's open stretches, as `day_blocks` answers them. */
  open?: OpenStretch[];
  running: RunningTimer | null;
  now: Date;
}

/** The id the running timer's stand-in carries through `segmentsOf`. */
const RUNNING_ID = -1;

const instant = (iso: string) => new Date(iso).getTime();

function stateOf(entry: DayBlock, offer: AdHocBlock | null | undefined): RowState {
  const { block } = entry;
  if (block.kind === "passive") return "offered";
  if (block.worklog_id !== null) return "logged";
  if (offer === undefined) return block.target.kind === "entity" ? "unlogged" : "label";
  return offer === null ? "unlogged" : "label";
}

function suggestionOf(entry: DayBlock, offer: AdHocBlock | null | undefined): Suggestion | null {
  const { block } = entry;
  if (offer) return offer.suggestion;
  // On a ticket: a passive block offers its own ticket.
  if (offer === null && block.kind === "passive" && block.target.kind === "entity") {
    return { entity_id: block.target.entity_id, title: entry.title, rule: "linked_to_target" };
  }
  return null;
}

/** The day: a row per block, the running timer and each open stretch, in time order. */
export function dayRows(input: DayInput): DayRow[] {
  const open = (input.open ?? []).map(
    (stretch, index): DayRow => ({
      key: `open-${index}`,
      state: "open",
      from: stretch.started_at,
      to: stretch.ended_at,
      ms: instant(stretch.ended_at) - instant(stretch.started_at),
      block: null,
      target: null,
      suggestion: null,
    }),
  );
  return [...blockRows(input), ...open].sort((a, b) => instant(a.from) - instant(b.from));
}

function blockRows(input: DayInput): DayRow[] {
  const { from: start, to: end } = dayBounds(input.day);
  const blocks = [...input.blocks];

  const running = input.running;
  const nowMs = input.now.getTime();
  if (running !== null && instant(running.started_at) < end.getTime() && nowMs > start.getTime()) {
    const startedMs = Math.max(instant(running.started_at), start.getTime());
    blocks.push({
      block: {
        id: RUNNING_ID,
        started_at: new Date(startedMs).toISOString(),
        ended_at: new Date(Math.min(nowMs, end.getTime())).toISOString(),
        target: running.target,
        kind: "manual",
        ended_by_relaunch: false,
        worklog_id: null,
      },
      title: null,
    });
  }

  return segmentsOf(blocks).flatMap((segment): DayRow[] => {
    if (segment.kind === "gap") return [];
    const entry = segment.block;
    const { block } = entry;
    const ms = Math.max(0, instant(block.ended_at) - instant(block.started_at));
    if (block.id === RUNNING_ID) {
      return [
        {
          key: "running",
          state: "running",
          from: block.started_at,
          to: block.ended_at,
          ms,
          block: null,
          target: block.target,
          suggestion: null,
        },
      ];
    }
    const offer = input.offers[block.id];
    return [
      {
        key: segment.key,
        state: stateOf(entry, offer),
        from: block.started_at,
        to: block.ended_at,
        ms,
        block: entry,
        target: block.target,
        suggestion: suggestionOf(entry, offer),
      },
    ];
  });
}

/**
 * A run of adjacent offered rows that all go to one ticket (#1108): job, its
 * pull request, job again is one piece of work, so one *Add* takes it.
 */
export interface OfferRun {
  /** The ticket every row in the run goes to. */
  entityId: string;
  /** Two or more rows, in time order, each still with its own actions. */
  rows: DayRow[];
  /** RFC 3339: the first row's start and the last row's end. */
  from: string;
  to: string;
  /** Their summed length, in milliseconds; the time between rows is not in it. */
  ms: number;
}

/**
 * The ticket an offered row goes to for certain: its own ticket, or the one a
 * confirmed link names. Both read as `linked_to_target` (see `suggestionOf`).
 * A guess (`last_logged`, `context_anchor`) is no certainty and goes nowhere.
 */
function certainTicket(row: DayRow): string | null {
  if (row.state !== "offered") return null;
  return row.suggestion?.rule === "linked_to_target" ? row.suggestion.entity_id : null;
}

/**
 * The day's runs, in time order. Adjacent means next to each other in the list:
 * any other row between them (open time, a block you tracked, the timer, an
 * offered row going elsewhere or to a guess) breaks the run. Time that is no
 * row (lunch, as far as knobas knows) does not, because taking a run gives each
 * row to the ticket over its own stretch and claims nothing between.
 *
 * Only the offer is grouped. The record keeps a block per visit.
 */
export function offerRuns(rows: DayRow[]): OfferRun[] {
  const runs: OfferRun[] = [];
  let current: OfferRun | null = null;
  for (const row of rows) {
    const ticket = certainTicket(row);
    if (ticket !== null && current !== null && current.entityId === ticket) {
      current.rows.push(row);
      current.to = row.to;
      current.ms += row.ms;
      continue;
    }
    if (current !== null && current.rows.length > 1) runs.push(current);
    current = ticket === null ? null : { entityId: ticket, rows: [row], from: row.from, to: row.to, ms: row.ms };
  }
  if (current !== null && current.rows.length > 1) runs.push(current);
  return runs;
}

/** The five figures above the strip, in whole minutes. */
export interface DayFigures {
  /** Manual blocks and the running timer: what you said your day was. */
  tracked: number;
  logged: number;
  /** Manual blocks on a ticket with no worklog yet. */
  toLog: number;
  /** Passive blocks, beside tracked rather than in it. */
  offered: number;
  /** Focused time nothing claims: the sum of the open rows. */
  open: number;
}

const minutes = (ms: number) => Math.floor(ms / 60_000);

export function dayFigures(rows: DayRow[]): DayFigures {
  const sum = (keep: (row: DayRow) => boolean) =>
    minutes(rows.filter(keep).reduce((total, row) => total + row.ms, 0));
  return {
    tracked: sum((row) => ["logged", "unlogged", "label", "running"].includes(row.state)),
    logged: sum((row) => row.state === "logged"),
    toLog: sum((row) => row.state === "unlogged"),
    offered: sum((row) => row.state === "offered"),
    open: sum((row) => row.state === "open"),
  };
}

/** `"2h 10m"`: prototype-6's duration, minutes and never rounded. */
export function hm(total: number): string {
  const hours = Math.floor(total / 60);
  const rest = total % 60;
  if (hours === 0) return `${rest}m`;
  return rest === 0 ? `${hours}h` : `${hours}h ${rest}m`;
}

/** The strip's hours, `from` to `to`. */
export interface StripWindow {
  from: number;
  to: number;
}

/**
 * 08:00 to 18:00, as R-13.4 draws it, widened by whole hours when a row sits
 * outside: a strip that clipped a late evening would hide the time it is for.
 */
export function stripWindow(day: string, rows: DayRow[]): StripWindow {
  const midnight = dayBounds(day).from.getTime();
  let from = 8;
  let to = 18;
  for (const row of rows) {
    const a = (Math.max(instant(row.from), midnight) - midnight) / 3_600_000;
    const b = (instant(row.to) - midnight) / 3_600_000;
    from = Math.min(from, Math.floor(a));
    to = Math.max(to, Math.min(24, Math.ceil(b)));
  }
  return { from, to };
}

/** Where a stretch sits on the strip, in percent of its width, clipped to it. */
export function place(
  day: string,
  window: StripWindow,
  fromIso: string,
  toIso: string,
): { left: number; width: number } {
  const midnight = dayBounds(day).from.getTime();
  const span = (window.to - window.from) * 3_600_000;
  const pct = (iso: string) => {
    const share = (instant(iso) - midnight - window.from * 3_600_000) / span;
    return Math.min(100, Math.max(0, share * 100));
  };
  const left = pct(fromIso);
  return { left: round(left), width: round(pct(toIso) - left) };
}

/** Percentages to two places, so an attribute does not carry float noise. */
function round(value: number): number {
  return Math.round(value * 100) / 100;
}

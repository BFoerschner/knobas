/**
 * The timer and its blocks — `crates/knobas-app/src/commands/time.rs`.
 *
 * Hand-written, and pinned to the Rust by tests in that module which
 * `include_str!` this file: a field, a union member or a command name added on
 * one side only fails `cargo test`, not merely `svelte-check`.
 *
 * A §10.8-ratified module pair, on the backup module's precedent (issue #278).
 * **Every** time command belongs here — the block, worklog, draft, day and
 * week commands #279 and #280 add as well as these four — so the bridge grows
 * one module rather than one more section of `entity.ts` per ticket.
 *
 * There is no timer *event*. The shell learns that a timer started or stopped
 * from the activity signal it already watches (`EVENTS.activityNew`, read by
 * `shell/latest-change.svelte.ts`), and ticks the elapsed reading client-side
 * from {@link RunningTimer.started_at}. Adding a channel would be a second
 * thing to keep in step with the first.
 */
import { invoke } from "@tauri-apps/api/core";

/**
 * A timer on something knobas has an id for — a ticket, page, note, repo or
 * asset.
 *
 * A **stored context is never one of these** (`CONTEXT.md`, *timer target*): a
 * context is a set, and time on a set has nowhere to go. `start_timer` refuses
 * a `ctx:` id with `invalid`, and the picker never offers one.
 */
export interface EntityTarget {
  kind: "entity";
  /** `"<namespace>:<key>"` — `knobas_core::entity::EntityRef`. */
  entity_id: string;
}

/** A timer on work with no entity behind it — "DB config for the migration". */
export interface LabelTarget {
  kind: "label";
  /** Trimmed by the backend, and never empty. */
  label: string;
}

/**
 * What a timer, and therefore a block, is attributed to — `time::TimerTarget`.
 *
 * Exactly one of the two, which is why it is a discriminated union rather than
 * an object with two optional fields: the schema says exactly-one
 * (`timer_target_chk`), the Rust enum says it, and this says it too.
 */
export type TimerTarget = EntityTarget | LabelTarget;

/** How a block came to exist — `time::BlockKind`. */
export type BlockKind = "manual" | "passive";

/** The timer that is running — `time::RunningTimer`. */
export interface RunningTimer {
  target: TimerTarget;
  /** RFC 3339, UTC. What the strip's elapsed reading counts from. */
  started_at: string;
  /**
   * The last moment knobas was known to be alive, RFC 3339, UTC.
   *
   * Nothing draws this. It is the stamp a stranded timer's block is closed at
   * when knobas starts again.
   */
  last_heartbeat: string;
}

/** One stretch of time knobas owns — `time::Block`. */
export interface Block {
  id: number;
  /** RFC 3339, UTC. */
  started_at: string;
  /** RFC 3339, UTC. */
  ended_at: string;
  target: TimerTarget;
  kind: BlockKind;
  /**
   * *Ended when knobas closed* — the block was closed by the relaunch sweep at
   * the last heartbeat, not by anybody pressing stop. The day review offers
   * *Extend to now* on these (#279).
   */
  ended_by_relaunch: boolean;
  /** The worklog this block was logged into (#280); `null` until then. */
  worklog_id: number | null;
}

/** The timer that is running, or `null`. */
export function currentTimer(): Promise<RunningTimer | null> {
  return invoke<RunningTimer | null>("current_timer");
}

/**
 * Start the timer on `target`.
 *
 * Rejects with `conflict` when one is already running — every surface that
 * switches targets stops first, because a stop is what closes a block — and
 * with `invalid` for a stored context, a malformed entity id or a blank label.
 */
export function startTimer(target: TimerTarget): Promise<RunningTimer> {
  return invoke<RunningTimer>("start_timer", { target });
}

/**
 * Stop the timer and close its block. `null` when nothing was running, which
 * is a success: a second *Stop* is not a message to dismiss.
 */
export function stopTimer(): Promise<Block | null> {
  return invoke<Block | null>("stop_timer");
}

/**
 * Say the window is still alive, and get the timer back as it now stands.
 *
 * Sent every thirty seconds while the window is focused. `foreground` is what
 * the reader has in front of them — open detail, else the room's anchor, else
 * `null`. It is what passive attribution (#281) will be derived from; today
 * the backend vets it and stores nothing.
 *
 * **A foreground the backend dislikes never costs the beat.** The stamp is
 * what a stranded timer's block is closed at, so a refused beat would freeze
 * it and the next relaunch would close the block hours early; a bad foreground
 * is logged on the backend instead.
 */
export function timerHeartbeat(foreground: TimerTarget | null): Promise<RunningTimer | null> {
  return invoke<RunningTimer | null>("timer_heartbeat", { foreground });
}

/**
 * One block as the day review draws it — `time::day::DayBlock`.
 *
 * The title is beside the block rather than inside it, and the split is the
 * point: a block is knobas' own durable record, and the title is the mirror's
 * *current* opinion of a row that may since have been renamed or purged. A
 * block whose entity is gone still says how long it was and what it was on.
 */
export interface DayBlock {
  block: Block;
  /**
   * What the mirror calls the target, or `null`.
   *
   * `null` for a label block — there is no entity — and for an entity the
   * mirror has never held, has purged, or holds under a blank title. One
   * question for the view, with one answer: show the id instead.
   */
  title: string | null;
}

/**
 * The blocks overlapping `[from, to)`, earliest first.
 *
 * **The caller computes the interval**, and for the day review it is the
 * reader's own local midnight and the next one — `dayBounds` in
 * `lib/time/day.ts`. The backend is given instants rather than a date because
 * the machine's timezone is a fact only this side holds, and an offset would
 * be the wrong shape as well: a day containing a DST change is 23 or 25 hours
 * long and has two of them.
 *
 * Overlap, not containment: a block that ran through midnight is on both days
 * it touched, so the strip a person most wants to fix has something to edit.
 */
export function dayBlocks(from: string, to: string): Promise<DayBlock[]> {
  return invoke<DayBlock[]>("day_blocks", { from, to });
}

/**
 * Move a block's start, its end and its target — and, with `endedAt` set to
 * now, this is *Extend to now*.
 *
 * All three every time, because the reader is stating what the block *is*.
 * That is also why a successful update always clears
 * {@link Block.ended_by_relaunch}: the marker means *knobas guessed this end*,
 * and once a person has said what the end is, it is theirs.
 *
 * Rejects with `invalid` for a stored context, a malformed entity id, a blank
 * label, an end before its start, and a block already logged into a worklog —
 * which is read-only, so that what knobas shows never disagrees with what the
 * ticket holds — and with `not_found` for a block that is no longer there.
 */
export function updateBlock(
  id: number,
  startedAt: string,
  endedAt: string,
  target: TimerTarget,
): Promise<DayBlock> {
  return invoke<DayBlock>("update_block", { id, startedAt, endedAt, target });
}

/**
 * Delete a block. Nothing comes back — the day review re-reads the day, which
 * is the one answer that is true about the rest of the strip as well.
 *
 * Rejects the way {@link updateBlock} does: a logged block cannot be deleted
 * either.
 */
export function deleteBlock(id: number): Promise<void> {
  return invoke<void>("delete_block", { id });
}

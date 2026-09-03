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
 */
export function timerHeartbeat(foreground: TimerTarget | null): Promise<RunningTimer | null> {
  return invoke<RunningTimer | null>("timer_heartbeat", { foreground });
}

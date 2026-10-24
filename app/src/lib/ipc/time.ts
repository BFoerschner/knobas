/**
 * The timer and its blocks — `crates/knobas-app/src/commands/time.rs`.
 *
 * The types are generated from the Rust (`./generated/time.ts`, contract
 * §2.6 as amended by #1325, switched by #1330): a field is explained in its
 * Rust doc comment and nowhere else. The `invoke` functions stay hand-written,
 * pinned to the Rust by tests in that module which `include_str!` this file,
 * and so do {@link ReaderDay} and {@link LoggedWork}, which group arguments
 * this file spreads into its calls rather than anything that crosses the
 * bridge whole.
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
import type {
  AdHocBlock,
  Block,
  DayBlock,
  DayRecord,
  DayWindow,
  Draft,
  PlannedWorklog,
  RunningTimer,
  TimerTarget,
  Week,
  Worklog,
} from "./generated/time";

export type {
  AdHocBlock,
  Block,
  BlockKind,
  Candidate,
  CandidateSource,
  DayBlock,
  DayRecord,
  DayWindow,
  Draft,
  OpenStretch,
  PlannedWorklog,
  RunningTimer,
  Suggestion,
  SuggestionRule,
  TimerTarget,
  Week,
  WeekCell,
  WeekRow,
  Worklog,
} from "./generated/time";

// One name per arm of the target union, for a caller that holds one arm.
// Picked out of the generated union, so an arm has one declaration.

/** {@link TimerTarget}'s `entity` arm. */
export type EntityTarget = Extract<TimerTarget, { kind: "entity" }>;
/** {@link TimerTarget}'s `label` arm. */
export type LabelTarget = Extract<TimerTarget, { kind: "label" }>;

/** The timer that is running, or `null`. */
export function currentTimer(): Promise<RunningTimer | null> {
  return invoke<RunningTimer | null>("current_timer");
}

/**
 * Start the timer on `target`, in the room the reader is standing in.
 *
 * `inRoom` is the **stored context** they are in — `RoomContext.filter.context`
 * — and `null` for every derived room (*All work*, a source, a project), which
 * is not a stored context and has nothing to anchor a suggestion to. It is
 * recorded on the block and read later by {@link adHocBlock}'s second rule
 * (#281): where the reader stood when the clock started is a different fact
 * from where they are standing when they stop.
 *
 * Rejects with `conflict` when one is already running — every surface that
 * switches targets stops first, because a stop is what closes a block — and
 * with `invalid` for a stored context as the *target*, a malformed entity id, a
 * blank label, or a room that is not a `ctx:` id.
 */
export function startTimer(
  target: TimerTarget,
  inRoom: string | null = null,
): Promise<RunningTimer> {
  return invoke<RunningTimer>("start_timer", { target, inRoom });
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
 * `null`. It is what passive attribution (#282) derives its blocks from: while
 * that setting is on the backend stores one observation per beat, and while it
 * is off it stores nothing at all.
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
export function dayBlocks(from: string, to: string): Promise<DayRecord> {
  return invoke<DayRecord>("day_blocks", { from, to });
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
 * **This is also *Assign…* on a passive block** (#282): a successful update
 * writes `kind: "manual"`, because the moment a person states a block's target
 * knobas' guess has become their record and must stop being something the next
 * day read reconciles away under them.
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

/**
 * *Dismiss* an offered (passive) block (#900): it is gone, and the next day
 * read does not offer that stretch again. `deleteBlock` on an offer only lasts
 * until the next read, which re-derives the day's offers.
 *
 * Rejects `invalid` for a block that is not an offer.
 */
export function dismissBlock(id: number): Promise<void> {
  return invoke<void>("dismiss_block", { id });
}

/**
 * **The reader's own reckoning of a day** — which the backend cannot work out
 * for itself, and which both worklog calls take.
 *
 * One object rather than two positional arguments, because they travel
 * together through every signature on both sides of the bridge and one of them
 * is a string: `logWork(entityId, day, …, startedAt, …)` with three strings in
 * a row is a swap nothing catches.
 */
export interface ReaderDay {
  /** `"YYYY-MM-DD"`, in the reader's own reckoning. */
  day: string;
  /**
   * Minutes east of UTC at `day`'s own local midnight, `getTimezoneOffset`
   * **negated**. Build the pair with `readerDay(day)` from `lib/time/day`,
   * never by hand: on a clock change the offset at another hour differs by
   * one, and a draft read with one and logged with the other covers two
   * windows (#1978). This machine is the only thing that knows which day the
   * reader means.
   */
  offsetMinutes: number;
}

/**
 * The worklog draft for a ticket and one of the reader's days, or `null`.
 *
 * **Ask on every stop; `null` is the ordinary answer.** Whether a worklog can
 * go somewhere is the backend's decision, read off the source's declared write
 * ops — so the shell opens the draft when it gets one and does nothing when it
 * does not, rather than keeping a list of kinds that can be timed and logged.
 */
export function worklogDraft(entityId: string, when: ReaderDay): Promise<Draft | null> {
  return invoke<Draft | null>("worklog_draft", {
    entityId,
    day: when.day,
    offsetMinutes: when.offsetMinutes,
  });
}

/** What the reader settled on in the draft. */
export interface LoggedWork {
  /** RFC 3339 — when the work began. Editable in the draft. */
  startedAt: string;
  /** How long was **worked**. Editable in the draft. */
  seconds: number;
  /** May be empty: a worklog with no words is a worklog. */
  comment: string;
}

/**
 * Log the day's work on a ticket: the write is queued like any other, a local
 * copy exists at once, and the blocks it covers become read-only.
 *
 * Which blocks are covered is **not** passed: the backend re-derives them, so
 * a webview cannot log another ticket's time or the same block twice.
 *
 * Rejects with `conflict` when the day's time on that ticket has already been
 * logged, and with `invalid` for a source that takes no worklogs or a duration
 * that is not positive.
 */
export function logWork(
  entityId: string,
  when: ReaderDay,
  logged: LoggedWork,
): Promise<Worklog> {
  return invoke<Worklog>("log_work", {
    entityId,
    day: when.day,
    offsetMinutes: when.offsetMinutes,
    startedAt: logged.startedAt,
    seconds: logged.seconds,
    comment: logged.comment,
  });
}

/**
 * Write a block over a stretch nobody claimed — *Assign…* on a gap (#282).
 *
 * The counterpart of {@link updateBlock}, which is what *Assign…* on a
 * **passive** block calls. The reader's sentence is the same either way ("this
 * half-hour was this ticket"); the only difference is whether knobas already
 * had a row to put it on, and both end in a manual block.
 *
 * Rejects with `invalid` for a stored context, a malformed entity id, a blank
 * label and an end before its start. Overlapping an existing block is allowed:
 * blocks have been editable since #279 and may overlap, and a refusal here
 * would mean a reader who mistyped a minute could not say what they meant.
 */
export function createBlock(
  startedAt: string,
  endedAt: string,
  target: TimerTarget,
): Promise<DayBlock> {
  return invoke<DayBlock>("create_block", { startedAt, endedAt, target });
}

/**
 * Whether passive attribution is switched on — `false` until somebody says
 * otherwise (#282).
 *
 * **Off means knobas records nothing**, not that it records and declines to
 * look: with it off the heartbeat still stamps the timer alive and stores no
 * observation, and the day review derives nothing.
 */
export function passiveAttribution(): Promise<boolean> {
  return invoke<boolean>("passive_attribution");
}

/**
 * Switch passive attribution on or off, and get back what is now stored.
 *
 * The stored value rather than nothing, so the toggle draws what the database
 * holds instead of what the click asked for — the rule `setBackupSchedule`
 * follows on the same surface.
 *
 * Switching it off stops the recording; the passive blocks already offered
 * stay where they are, because nothing passive has ever reached a source and
 * there is therefore nothing to withdraw.
 */
export function setPassiveAttribution(enabled: boolean): Promise<boolean> {
  return invoke<boolean>("set_passive_attribution", { enabled });
}

/**
 * What *Log an ad-hoc block* should show for a block, or `null` because that
 * block is not an ad-hoc one.
 *
 * **Ask on every stop, before {@link worklogDraft}.** Whether a block's target
 * is somewhere a worklog can go is the backend's decision, read off the
 * source's declared write ops — so the shell asks once and opens whichever
 * dialog it is handed, rather than keeping a list of kinds that goes stale the
 * day an adapter starts taking worklogs.
 *
 * The day is the day the **block started on**, in the reader's own reckoning,
 * for the reason {@link worklogDraft} takes one: a stop at 00:10 closes an
 * afternoon that belongs to yesterday.
 *
 * Rejects with `not_found` for a block that is no longer there.
 */
export function adHocBlock(blockId: number, when: ReaderDay): Promise<AdHocBlock | null> {
  return invoke<AdHocBlock | null>("ad_hoc_block", {
    blockId,
    day: when.day,
    offsetMinutes: when.offsetMinutes,
  });
}

/**
 * The week timesheet: a row per target and a cell per day.
 *
 * **The caller computes the days**, the way {@link dayBlocks}'s caller
 * computes one — the machine's timezone is a fact only this side holds, and a
 * single UTC offset would move one edge of the week that contains a
 * daylight-saving change.
 *
 * Rejects with `invalid` for no days, more than seven, or windows that overlap
 * — an overlap would count one block into two columns.
 */
export function weekTimesheet(days: DayWindow[]): Promise<Week> {
  return invoke<Week>("week_timesheet", { days });
}

/**
 * What *Log all* would send, before it sends any of it.
 *
 * The confirmation's list. Ask for it, show it, and send each row the reader
 * ticks with {@link logWork} — a bulk write into a ticketing system other
 * people read is not something to discover afterwards.
 */
export function logAllPreview(days: DayWindow[]): Promise<PlannedWorklog[]> {
  return invoke<PlannedWorklog[]>("log_all_preview", { days });
}

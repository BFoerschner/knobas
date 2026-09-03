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

/**
 * How a block came to exist — `time::BlockKind`.
 *
 * `"passive"` is knobas' own guess at what was open (#282) and is never logged
 * anywhere on its own. Assigning one through {@link updateBlock} makes it
 * `"manual"`; nothing turns a manual block back.
 */
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
 * One day as the day review reads it — `time::day::DayRecord`.
 *
 * A record rather than the bare list this read used to answer with, because
 * one thing is true of the **day** and not of any block on it: whether knobas
 * still has the observations for it (#337).
 */
export interface DayRecord {
  /** The blocks overlapping the day, earliest first. */
  blocks: DayBlock[];
  /**
   * Whether the day reaches back past what retention has swept.
   *
   * knobas keeps a month of observations, and `#/time/<date>` takes any date,
   * so a day older than that draws exactly the strip a day nobody had the app
   * open on draws. This is what tells the two apart.
   *
   * Passive blocks the day was already offered are still drawn: retention took
   * the evidence, not the record made from it while the evidence was there. It
   * says the strip cannot be added to, never that what is on it is untrue.
   */
  past_horizon: boolean;
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

/** Where a worklog-draft candidate was seen — `worklog::CandidateSource`. */
export type CandidateSource = "mirror" | "activity";

/**
 * One thing knobas saw the reader do inside a draft's interval —
 * `worklog::Candidate`.
 *
 * A proposal, not a record: the draft draws these as checkboxes and the
 * comment is the {@link bullet}s of the ticked ones, joined with newlines.
 * **Do not compose a bullet here** — the wording is the backend's, so that
 * every worklog knobas has ever sent is spelled one way.
 */
export interface Candidate {
  /** Stable within a draft — `"item:jira:PAY-231"`, `"activity:4211"`. */
  id: string;
  source: CandidateSource;
  /** RFC 3339, UTC. */
  at: string;
  entity_id: string | null;
  /** The line this candidate contributes to the comment, verbatim. */
  bullet: string;
}

/** What stopping the timer on a ticket offers — `worklog::Draft`. */
export interface Draft {
  entity_id: string;
  /** `"2026-09-03"`, the reader's own day. */
  day: string;
  /** RFC 3339, UTC — the first block's start. */
  started_at: string;
  /**
   * RFC 3339, UTC — the last block's end.
   *
   * **Not `started_at` plus {@link seconds}.** The difference between the two
   * is the gaps between the day's blocks, which is exactly what is *not*
   * logged.
   */
  ended_at: string;
  /** The time about to be logged: the blocks' durations added up. */
  seconds: number;
  /** The blocks this draft is made of, oldest first. */
  block_ids: number[];
  candidates: Candidate[];
  /** The comment as generated from every candidate. Editable, down to empty. */
  comment: string;
}

/** knobas' copy of a worklog it has sent, or is still sending — `worklog::Worklog`. */
export interface Worklog {
  id: number;
  entity_id: string;
  /** RFC 3339, UTC. */
  started_at: string;
  seconds: number;
  comment: string;
  /** The blocks it was made of. They carry its id and are read-only now. */
  block_ids: number[];
  /** The write that carries it; read its state with `pendingWrites`. */
  write_queue_id: number | null;
  /** What Jira called it — `null` while the write is still owed. */
  remote_id: string | null;
  /** RFC 3339, UTC. */
  created_at: string;
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
   * Minutes east of UTC — `offsetMinutes()` from `lib/time/draft`, which is
   * `getTimezoneOffset` **negated**. This machine is the only thing that knows
   * which day the reader means.
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
 * Which rule produced an ad-hoc block's suggestion — `suggest::SuggestionRule`.
 *
 * The dialog draws a sentence per member, and that is the whole of what this
 * union is for: a suggestion whose reason is not on screen is a guess the
 * reader can only trust or ignore.
 */
export type SuggestionRule = "linked_to_target" | "context_anchor" | "last_logged";

/** The ticket *Log an ad-hoc block* offers, and why — `suggest::Suggestion`. */
export interface Suggestion {
  entity_id: string;
  /**
   * What the mirror calls it, or `null` — for an entity it has never held, has
   * purged, or holds under a blank title. One question, one answer: show the
   * id instead. The rule {@link DayBlock.title} states.
   */
  title: string | null;
  rule: SuggestionRule;
}

/**
 * What stopping the timer on a page, a note, a repo or a label offers —
 * `suggest::AdHocBlock`.
 *
 * **Two absences, and they mean different things.** {@link adHocBlock}
 * answering `null` is *this block is on a ticket*, and the worklog draft is
 * what opens. This shape with `suggestion: null` is *the dialog opens and
 * knobas has nothing to suggest* — no rule fired, and *Keep local* is the
 * default.
 *
 * One field: the caller passed the block id in and still holds it, so an echo
 * of it here would be a value with no reader.
 */
export interface AdHocBlock {
  suggestion: Suggestion | null;
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
 * One of the reader's days, as the week timesheet asks for it —
 * `week::DayWindow`.
 *
 * Both halves travel together because neither side can derive the other: the
 * backend queries on the instants and keys the cell on the date, and it must
 * not do the timezone arithmetic itself. `weekWindows` in `lib/time/week.ts`
 * is what builds these — one `Date` per local midnight, so a week containing a
 * daylight-saving change is still seven correct windows rather than seven
 * equal ones.
 */
export interface DayWindow {
  /** `"2026-08-24"`, in the reader's own reckoning. */
  day: string;
  /** RFC 3339, UTC — the reader's local midnight. */
  from: string;
  /** RFC 3339, UTC — the next local midnight. Half-open. */
  to: string;
}

/**
 * One target's numbers on one day, in **seconds** — `week::WeekCell`.
 *
 * Seconds on the wire and minutes on the screen. `CONTEXT.md`'s timesheet is
 * minute-granular with no rounding, and rounding once in the view on a number
 * that never lost precision is what keeps a week's total equal to the total of
 * its days.
 */
export interface WeekCell {
  /**
   * The day's **manual** blocks on this target, added up — and on the
   * no-target row, the focused time no block covered.
   *
   * Passive blocks are not in it: tracked time is what the reader said their
   * day was, which is the same rule the day review's heading states.
   */
  tracked_seconds: number;
  /**
   * The day's **passive** blocks on this target — what knobas is offering.
   *
   * Beside {@link tracked_seconds} rather than inside it, and present rather
   * than dropped: the day strip above draws these blocks and says "*N*
   * offered" beside its own tracked total, so a week that left them out would
   * disagree with it about the same afternoon. Nothing logs one until a person
   * assigns it, so it pays into no other number here.
   */
  offered_seconds: number;
  /**
   * Worklog seconds whose write is `pending` or `sent`.
   *
   * **A pending worklog counts as logged** (spec #272 story 39): the number is
   * about what the reader did, not about sync timing.
   */
  logged_seconds: number;
  /**
   * Worklog seconds whose write is waiting on a person — `held`, `refused`,
   * `discarded`, or a copy whose queue row is gone.
   *
   * Shown as *held* rather than folded into either neighbour: the blocks under
   * it already carry a worklog id, so *Log all* will not offer them again, and
   * drawing it as unlogged would invite logging one afternoon twice.
   */
  held_seconds: number;
  /** `tracked - logged - held`, floored at zero. */
  unlogged_seconds: number;
}

/** One row of the timesheet — `week::WeekRow`. */
export interface WeekRow {
  /**
   * The target, or `null` for the **"no target, app open"** row.
   *
   * That row is focused time no block covers, so that the week's total is
   * honest (spec story 41). It has no target and therefore nothing to log:
   * *Log all* never sees it. It is present only when there is something to
   * say — a week knobas was shut for has no such row at all, which is how
   * "the app was closed" reads differently from "the app was open and idle".
   */
  target: TimerTarget | null;
  /** What the mirror calls the target, or `null` — the `DayBlock` split. */
  title: string | null;
  /** One cell per day in {@link Week.days}, in that order. */
  cells: WeekCell[];
}

/** The week, as the timesheet draws it — `week::Week`. */
export interface Week {
  /** `"2026-08-24"` … — the column headings, in the order asked for. */
  days: string[];
  /** Targets first in title order, then the no-target row if it has one. */
  rows: WeekRow[];
  /**
   * Which of {@link Week.days} knobas has no observations for, in the same
   * order (#337).
   *
   * One entry per day rather than one flag for the week, because a week that
   * straddles the horizon is the ordinary case: `week::vet` bounds a
   * timesheet's column *count* and says nothing about where its windows sit.
   *
   * It is on the week and not on the "no target, app open" row because that
   * row is dropped when it has nothing to say — and a week entirely past the
   * horizon is precisely the week with no such row and the most to explain.
   */
  past_horizon: boolean[];
}

/** One worklog *Log all* would create — `week::PlannedWorklog`. */
export interface PlannedWorklog {
  /** `"2026-08-24"` — the day this worklog is for. */
  day: string;
  entity_id: string;
  /** The mirror's title for it, or `null`. For the reader to recognise. */
  title: string | null;
  /** RFC 3339, UTC — the first covered block's start. */
  started_at: string;
  /** The covered blocks' durations added up, gaps excluded. */
  seconds: number;
  /** How many blocks it concatenates. */
  blocks: number;
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
 * What {@link logAll} would send, before it sends any of it.
 *
 * The confirmation's list. Ask for it, show it, and call {@link logAll} only
 * if the reader says yes — a bulk write into a ticketing system other people
 * read is not something to discover afterwards.
 */
export function logAllPreview(days: DayWindow[]): Promise<PlannedWorklog[]> {
  return invoke<PlannedWorklog[]>("log_all_preview", { days });
}

/**
 * Log the week: one worklog per day and ticket from that day's unlogged
 * **manual** blocks.
 *
 * Passive blocks and ad-hoc label blocks are never touched — a passive block
 * is knobas' own guess, and a label is not somewhere a worklog can go. The
 * work is re-derived rather than taken from the plan, so a timer that stopped
 * while the confirmation was on screen is logged too.
 *
 * Rejects with whatever the queue said, **after** the rest of the week has
 * been logged: each worklog is its own write and one that failed does not undo
 * the copies of the ones that did not.
 */
export function logAll(days: DayWindow[]): Promise<Worklog[]> {
  return invoke<Worklog[]>("log_all", { days });
}

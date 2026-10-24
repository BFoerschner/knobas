/**
 * What a stop opens in the ticket-first shell, and what the worklog draft's
 * buttons do to the clock (#826, R-13.1 to R-13.3; #896).
 *
 * **The draft opens over the running clock**, as prototype-6 draws it:
 * `worklog_draft` counts the running timer's stretch up to now (#896), so a
 * stop on a ticket asks for the draft first and leaves the clock alone. The
 * buttons then say what happens to it:
 *
 * - *Log <time> to <key>* stops the clock first ({@link DraftFlow.stopClock}),
 *   so the stretch is a block before `log_work` re-derives the day's blocks;
 * - *Stop, log later* stops the clock and leaves the time in the day;
 * - *Keep running* closes the draft and touches nothing: no time is lost;
 * - *Discard* stops the clock and writes nothing: the stop the reader pressed
 *   still happens, and the time stays in the day, unlogged, as with *Stop,
 *   log later* but without its toast.
 *
 * A stop on something with no draft (a label, a page, a ticket whose source
 * takes no worklogs) stops the clock at once and asks `ad_hoc_block` what the
 * closed block calls for, as the rooms UI's `offerOn` does.
 *
 * **A switch waits for the answer** (R-13.2): starting on another ticket while
 * one runs opens the running one's draft, and the new timer starts only after
 * *Log*, *Stop, log later* or *Discard* have stopped the old one. *Keep
 * running* keeps the old ticket and drops the new start, saying so. A stop
 * with nothing to draft starts the new timer at once, so a switch never needs
 * a dialog to finish.
 *
 * **A draft no stop opened can count the running stretch too**: the Day tab's
 * *Log…* on the ticket being timed, that day. Its *Log* stops the clock so
 * `log_work` covers the stretch it counted, and starts it again on the same
 * ticket once the draft is answered.
 *
 * **One draft at a time.** While a draft is open, being read, or its answer is
 * stopping the clock, a second start or stop would collide with it; the flow
 * refuses it with a toast instead.
 */
import type { AdHocBlock, Block, Draft, ReaderDay, TimerTarget } from "../ipc/time";
import type { Timer } from "../shell/timer.svelte";
import type { ToastSpec } from "../shell/toasts.svelte";
import { dayOffsetMinutes } from "../time/day";
import { localDay } from "../time/draft";

/** How the reader left the draft. */
export type DraftOutcome = "logged" | "later" | "keep" | "discard";

/** A start that waits for the running timer's draft to be answered. */
export interface WaitingStart {
  target: TimerTarget;
  /** What the toast calls it: `Timing <title>.` */
  title: string;
}

/** The draft on screen, and what opened it. */
export interface Drafting {
  draft: Draft;
  /**
   * The target the clock runs on under the draft, when a stop opened it;
   * `null` when no stop did (the ad-hoc dialog's *Log to a ticket…*).
   */
  running: TimerTarget | null;
  after: WaitingStart | null;
}

export interface DraftFlowPorts {
  timer: Pick<Timer, "current" | "stop" | "start">;
  adHocBlock: (blockId: number, when: ReaderDay) => Promise<AdHocBlock | null>;
  worklogDraft: (entityId: string, when: ReaderDay) => Promise<Draft | null>;
  /** Open the ad-hoc block dialog: the stop was not on a ticket. */
  onadhoc: (block: Block, offer: AdHocBlock) => void;
  push: (toast: ToastSpec) => void;
  complain: (error: unknown) => void;
  /** *Open the day*: the time view. */
  onday: () => void;
}

export interface DraftFlow {
  readonly open: Drafting | null;
  /** A draft is open, being read, or its answer is stopping the clock. */
  readonly busy: boolean;
  /** Say why a start or stop was refused while {@link busy}. */
  refuse(): void;
  /** Stop pressed (`⌘T`, the chip) with a timer running. */
  stop(): void;
  /** Start on `target`, through the running timer's draft when there is one. */
  switchTo(target: TimerTarget, title: string): Promise<void>;
  /** Show a draft no stop opened: the ad-hoc dialog's *Log to a ticket…*. */
  show(draft: Draft): void;
  /** *Log*'s first step: stop the clock under the open draft, if it runs. */
  stopClock(): Promise<void>;
  /** The reader's answer; closes the draft. */
  settle(outcome: DraftOutcome): void;
}

function same(a: TimerTarget, b: TimerTarget): boolean {
  return a.kind === "entity"
    ? b.kind === "entity" && a.entity_id === b.entity_id
    : b.kind === "label" && a.label === b.label;
}

/** The day `at` falls on, with the offset `log_work` is sent for it: the one at that day's midnight. */
function dayOf(at: string): ReaderDay {
  const day = localDay(new Date(at));
  return { day, offsetMinutes: dayOffsetMinutes(day) };
}

export function createDraftFlow(ports: DraftFlowPorts): DraftFlow {
  let open = $state.raw<Drafting | null>(null);
  /** Reads and stops still in flight for a draft. */
  let inFlight = $state(0);
  /** Whether *Log* already stopped the clock under the open draft. */
  let halted = false;
  /**
   * What *Log* stopped under a draft no stop opened, to be started again once
   * the draft is answered: the reader never asked that clock to stop.
   */
  let resume: TimerTarget | null = null;

  const timing = (title: string) => ports.push({ text: `Timing ${title}.` });

  function startWaiting(after: WaitingStart | null) {
    if (after === null) return;
    void ports.timer
      .start(after.target)
      .then(() => timing(after.title))
      .catch(ports.complain);
  }

  /** What a closed block with no draft calls for: `ad_hoc_block`'s dialog, or nothing. */
  async function offerAdHoc(closed: Block | null, after: WaitingStart | null) {
    if (closed !== null) {
      try {
        const adHoc = await ports.adHocBlock(closed.id, dayOf(closed.started_at));
        if (adHoc) ports.onadhoc(closed, adHoc);
      } catch (error) {
        ports.complain(error);
      }
    }
    startWaiting(after);
  }

  /**
   * The running timer's draft, opened over the running clock; or, with none
   * to open, the stop and the ad-hoc check. Whatever happens, a waiting start
   * is not lost.
   */
  async function through(after: WaitingStart | null) {
    const running = ports.timer.current;
    if (running === null) return startWaiting(after);
    inFlight += 1;
    try {
      if (running.target.kind === "entity") {
        let draft: Draft | null = null;
        try {
          draft = await ports.worklogDraft(running.target.entity_id, dayOf(running.started_at));
        } catch (error) {
          // The stop still happens; the time stays in the day, unlogged.
          ports.complain(error);
        }
        if (draft !== null) {
          halted = false;
          open = { draft, running: running.target, after };
          return;
        }
      }
      await offerAdHoc(await ports.timer.stop(), after);
    } catch (error) {
      ports.complain(error);
    } finally {
      inFlight -= 1;
    }
  }

  return {
    get open() {
      return open;
    },
    get busy() {
      return open !== null || inFlight > 0;
    },
    refuse() {
      ports.push({ text: "Answer the worklog draft first." });
    },
    stop() {
      if (this.busy) return this.refuse();
      void through(null);
    },
    async switchTo(target, title) {
      if (this.busy) return this.refuse();
      const running = ports.timer.current;
      if (running === null) {
        try {
          await ports.timer.start(target);
          timing(title);
        } catch (error) {
          ports.complain(error);
        }
        return;
      }
      if (same(running.target, target)) return timing(title);
      await through({ target, title });
    },
    show(draft) {
      halted = false;
      resume = null;
      open = { draft, running: null, after: null };
    },
    async stopClock() {
      if (open === null || halted) return;
      if (open.running === null) {
        // A draft no stop opened (the Day tab's *Log…*) still counts the
        // running stretch when the clock is on its ticket and day. `log_work`
        // logs the draft's seconds but covers only closed blocks, so the
        // stretch must be a block first, or its time is logged now and offered
        // again at the next stop. The clock starts again after the answer.
        const current = ports.timer.current;
        const on = open.draft.entity_id;
        if (open.draft.running_since === null || current?.target.kind !== "entity" || current.target.entity_id !== on) {
          return;
        }
        await ports.timer.stop();
        resume = current.target;
        halted = true;
        return;
      }
      await ports.timer.stop();
      halted = true;
    },
    settle(outcome) {
      const was = open;
      const stopped = halted;
      const restart = resume;
      open = null;
      halted = false;
      resume = null;
      if (was === null) return;
      if (restart !== null) {
        // Stopped only so the log could cover the stretch: put it back.
        void ports.timer.start(restart).catch(ports.complain);
        return;
      }
      if (outcome === "keep") {
        // A *Log* that failed after stopping the clock: put it back.
        if (was.running !== null && stopped) void ports.timer.start(was.running).catch(ports.complain);
        if (was.after !== null) ports.push({ text: `Not started on ${was.after.title}: the timer kept running.` });
        return;
      }
      inFlight += 1;
      void (was.running === null || stopped ? Promise.resolve() : ports.timer.stop())
        .then(() => {
          if (outcome === "later") {
            ports.push({
              text: "Timer stopped. The time is kept in today's blocks, unlogged.",
              action: { label: "Open the day", run: ports.onday },
            });
          }
          startWaiting(was.after);
        })
        .catch(ports.complain)
        .finally(() => (inFlight -= 1));
    },
  };
}

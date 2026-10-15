/**
 * What a stop opens in the ticket-first shell, and what the worklog draft's
 * buttons do to the clock (#826, R-13.1 to R-13.3).
 *
 * The draft is made of **closed** blocks (`worklog_draft` reads the day's
 * unlogged blocks), so the clock is stopped before the draft opens, as in the
 * rooms UI. The buttons then say what happens next:
 *
 * - *Log <time> to <key>* logs it (through the write queue, in the dialog);
 * - *Stop, log later* leaves the clock stopped and the time in the day;
 * - *Keep running* starts the clock again on the ticket the stop took it off,
 *   so the next stop's draft covers both stretches (the draft concatenates the
 *   day's blocks);
 * - *Discard* closes a draft that no stop opened, with nothing written.
 *
 * **A switch waits for the answer** (R-13.2): starting on another ticket while
 * one runs stops the running one and opens its draft, and the new timer starts
 * only after *Log* or *Stop, log later*. *Keep running* keeps the old ticket
 * and drops the new start. A stop with nothing to draft starts the new timer at
 * once, so a switch never needs a dialog to finish.
 */
import type { AdHocBlock, Block, Draft, ReaderDay, TimerTarget } from "../ipc/time";
import type { Timer } from "../shell/timer.svelte";
import type { ToastSpec } from "../shell/toasts.svelte";
import { localDay, offsetMinutes } from "../time/draft";

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
  /** The target a stop took the clock off, or `null` when no stop opened it. */
  stopped: TimerTarget | null;
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
  /** A stop happened (`⌘T`, the chip): offer what its block calls for. */
  stopped(closed: Block | null): void;
  /** Start on `target`, through the running timer's draft when there is one. */
  switchTo(target: TimerTarget, title: string): Promise<void>;
  /** Show a draft no stop opened: the ad-hoc dialog's *Log to a ticket…*. */
  show(draft: Draft): void;
  /** The reader's answer; closes the draft. */
  settle(outcome: DraftOutcome): void;
}

function same(a: TimerTarget, b: TimerTarget): boolean {
  return a.kind === "entity"
    ? b.kind === "entity" && a.entity_id === b.entity_id
    : b.kind === "label" && a.label === b.label;
}

export function createDraftFlow(ports: DraftFlowPorts): DraftFlow {
  let open = $state.raw<Drafting | null>(null);

  function startWaiting(after: WaitingStart | null) {
    if (after === null) return;
    void ports.timer
      .start(after.target)
      .then(() => ports.push({ text: `Timing ${after.title}.` }))
      .catch(ports.complain);
  }

  /**
   * Ask the backend what the closed block calls for, as the rooms UI's
   * `offerOn` does: `ad_hoc_block` first, whose `null` means *on a ticket*,
   * then `worklog_draft`. Whatever happens, a waiting start is not lost.
   */
  function offer(closed: Block | null, after: WaitingStart | null) {
    if (closed === null) return startWaiting(after);
    const day = { day: localDay(new Date(closed.started_at)), offsetMinutes: offsetMinutes() };
    void ports
      .adHocBlock(closed.id, day)
      .then(async (adHoc) => {
        if (adHoc) {
          ports.onadhoc(closed, adHoc);
          return startWaiting(after);
        }
        if (closed.target.kind !== "entity") return startWaiting(after);
        const draft = await ports.worklogDraft(closed.target.entity_id, day);
        if (draft === null) return startWaiting(after);
        open = { draft, stopped: closed.target, after };
      })
      .catch((error: unknown) => {
        ports.complain(error);
        startWaiting(after);
      });
  }

  return {
    get open() {
      return open;
    },
    stopped(closed) {
      offer(closed, null);
    },
    async switchTo(target, title) {
      const running = ports.timer.current;
      if (running === null) {
        try {
          await ports.timer.start(target);
          ports.push({ text: `Timing ${title}.` });
        } catch (error) {
          ports.complain(error);
        }
        return;
      }
      if (same(running.target, target)) {
        ports.push({ text: `Timing ${title}.` });
        return;
      }
      try {
        offer(await ports.timer.stop(), { target, title });
      } catch (error) {
        ports.complain(error);
      }
    },
    show(draft) {
      open = { draft, stopped: null, after: null };
    },
    settle(outcome) {
      const was = open;
      open = null;
      if (was === null) return;
      if (outcome === "keep") {
        if (was.stopped !== null) void ports.timer.start(was.stopped).catch(ports.complain);
        return;
      }
      if (outcome === "later") {
        ports.push({
          text: "Timer stopped. The time is kept in today's blocks, unlogged.",
          action: { label: "Open the day", run: ports.onday },
        });
      }
      if (outcome !== "discard") startWaiting(was.after);
    },
  };
}

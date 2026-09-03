/**
 * The one global timer, live — the store the top strip and ⌘T both read
 * (issue #278).
 *
 * One home, for the reason `health.svelte.ts` records for credential health:
 * three surfaces act on the timer — the strip, the keyboard, the launcher's
 * *Start timer* row — and three independent reads is three chances for them to
 * disagree about what the clock is on.
 *
 * ## What ticks and what does not
 *
 * **The elapsed reading is computed in the webview**, from
 * {@link RunningTimer.started_at} and a clock that moves once a second. It is
 * never fetched: the backend already told this store when the timer started,
 * and polling for a number both sides can work out is a round trip per second
 * for nothing.
 *
 * **The timer itself is re-read on the activity signal**, not on a poll. Every
 * start and stop writes an activity line (`knobas_app::time`'s module docs say
 * why there is no event of the timer's own), so `activity:new` is already the
 * announcement that something happened — including a start or a stop this
 * window did not make.
 *
 * ## The heartbeat
 *
 * Every thirty seconds, **and only while the window is focused**. Two things
 * ride on it and neither tolerates a beat sent by a window nobody is looking
 * at: it is the last-alive stamp a stranded timer is closed at, so a beat from
 * a backgrounded window would place a crash later than it happened; and it is
 * the observation passive attribution (#282) will turn into passive blocks,
 * where an unfocused window must never count as work (story 27).
 *
 * The beat carries the **foreground** by the rule *open detail, else room
 * anchor, else none*. This store does not know the address, so the shell
 * supplies that rule as a function.
 */
import { listen as tauriListen } from "@tauri-apps/api/event";

import { EVENTS } from "../ipc";
import {
  currentTimer as realCurrentTimer,
  startTimer as realStartTimer,
  stopTimer as realStopTimer,
  timerHeartbeat as realHeartbeat,
  type Block,
  type RunningTimer,
  type TimerTarget,
} from "../ipc/time";
import { elapsedReading } from "./timer";

/** How often the window says it is alive. Spec #272: thirty seconds. */
export const HEARTBEAT_MS = 30_000;

/** How often the elapsed reading is recomputed. */
const TICK_MS = 1000;

/** The bridge this store needs, injectable so a test needs no Tauri. */
export interface TimerPorts {
  currentTimer: () => Promise<RunningTimer | null>;
  startTimer: (target: TimerTarget, inRoom: string | null) => Promise<RunningTimer>;
  stopTimer: () => Promise<Block | null>;
  timerHeartbeat: (foreground: TimerTarget | null) => Promise<RunningTimer | null>;
  listen: typeof tauriListen;
  /** Whether the window is focused. Injectable for the same reason. */
  focused: () => boolean;
  now: () => Date;
}

/**
 * What one press of ⌘T did — {@link Timer.press}'s answer.
 *
 * A shape rather than the bare word it used to be (#278 → #280): a stop
 * closes a block, and the block is what the worklog draft opens on. The word
 * alone would have the shell read the timer back to find out what it had just
 * closed, which is a different block whenever another surface started one in
 * between.
 */
export interface TimerPress {
  did: "started" | "stopped" | "pick";
  /** The block a stop closed. `null` for every other outcome. */
  closed: Block | null;
}

export interface Timer {
  /** The running timer, or `null`. */
  readonly current: RunningTimer | null;
  /** `"0:07"` / `"3:20:07"`, or `null` when nothing is running. */
  readonly elapsed: string | null;
  /**
   * What the next heartbeat will carry. The shell writes it; the store only
   * hands it on. `null` is *nothing in front of the reader*, which is a legal
   * observation and not a missing one.
   */
  foreground: TimerTarget | null;
  /**
   * **The stored context the reader is standing in**, which every start
   * records on the block it opens (#281).
   *
   * Ambient rather than an argument, for the reason {@link foreground} is: ⌘T
   * and the launcher's *Start timer* row both start a timer without knowing
   * about rooms, and a parameter would be one every future caller had to
   * remember to fill in — a start that forgot it would record the afternoon as
   * having run nowhere, and the ad-hoc dialog's second rule would quietly stop
   * firing.
   *
   * `null` for a derived room — *All work*, a source, a project — which is not
   * a stored context. The shell writes it from the resolved room's
   * `filter.context`.
   */
  roomContext: string | null;
  /** Re-read the timer from the backend. */
  refresh(): Promise<void>;
  /** Start on `target`. Rejects the way `start_timer` does. */
  start(target: TimerTarget): Promise<void>;
  /** Stop, and answer with the block that closed, or `null`. */
  stop(): Promise<Block | null>;
  /**
   * **Move the clock to another target: stop first, then start** (#278,
   * story 11) — what the launcher's *Start timer* row does.
   *
   * The stop comes first and unconditionally, because that is what closes the
   * block the reader was in; a start that let the backend refuse it as a
   * `conflict`, or one that replaced the row, would lose the block either
   * way. It answers with the block that closed, which is what #280's worklog
   * draft opens on.
   *
   * Here rather than in the shell's glue, for the reason `press()` is: the
   * order is a rule about the clock, and a rule the shell owned would be one
   * every future caller had to remember.
   */
  switchTo(target: TimerTarget): Promise<Block | null>;
  /**
   * **What ⌘T does**, decided here rather than in the keyboard.
   *
   * The three behaviours of issue #278, in the order the ticket states them:
   *
   * * a timer is running → **stop** it (and close its block);
   * * nothing running, something in front of the reader → **start** on it;
   * * nothing running and nothing in front → `"pick"`, and the shell opens
   *   the picker with its ad-hoc label field.
   *
   * The rule lives with the state it reads instead of in `keys.ts`, for the
   * reason the launcher's own `⌘K` binding lives in the launcher: a rule split
   * across two owners is one that can be spelled two ways. `keys.ts` decides
   * *that* the key was pressed; this decides what it means.
   *
   * `"pick"` rather than a callback: opening a dialog is the shell's, and a
   * store that could open one would be a store with a view in it — and so is
   * the worklog draft the returned {@link TimerPress.closed} block opens
   * (#280).
   */
  press(): Promise<TimerPress>;
  /** Subscribe, tick and beat. Returns its teardown. */
  begin(): () => void;
}

export function createTimer(ports?: Partial<TimerPorts>): Timer {
  /** Whether {@link Timer.begin} is already running. See its guard below. */
  let live = false;

  const io: TimerPorts = {
    currentTimer: realCurrentTimer,
    startTimer: realStartTimer,
    stopTimer: realStopTimer,
    timerHeartbeat: realHeartbeat,
    listen: tauriListen,
    focused: () => document.hasFocus(),
    now: () => new Date(),
    ...ports,
  };

  /**
   * A `$state` object with fields rather than three bare runes: an exported
   * closure cannot reassign a `let` its caller holds, and the getters below
   * read through something stable — the shape `latest-change.svelte.ts` uses.
   */
  const state = $state<{
    current: RunningTimer | null;
    now: Date;
    foreground: TimerTarget | null;
    roomContext: string | null;
  }>({
    current: null,
    now: io.now(),
    foreground: null,
    roomContext: null,
  });

  async function refresh(): Promise<void> {
    // A failed read leaves the last known timer alone rather than blanking the
    // strip: `current_timer` rejects with `not_ready` for the whole of
    // bring-up, and a strip that cleared itself on that would lose a running
    // timer every time the database restarted under it.
    try {
      state.current = await io.currentTimer();
    } catch {
      // Nothing to say here. The next activity line, or the next beat, tries
      // again, and the caller has no repair to offer.
    }
  }

  return {
    get current() {
      return state.current;
    },
    get elapsed() {
      const running = state.current;
      return running ? elapsedReading(running.started_at, state.now) : null;
    },
    get foreground() {
      return state.foreground;
    },
    set foreground(target: TimerTarget | null) {
      state.foreground = target;
    },
    get roomContext() {
      return state.roomContext;
    },
    set roomContext(context: string | null) {
      state.roomContext = context;
    },
    refresh,
    async start(target: TimerTarget) {
      // The answer is used directly rather than followed by a read: `start_timer`
      // returns the row it wrote, and a re-read would be a second round trip
      // that can only agree or race.
      state.current = await io.startTimer(target, state.roomContext);
    },
    async stop() {
      const closed = await io.stopTimer();
      state.current = null;
      return closed;
    },
    async switchTo(target: TimerTarget) {
      const closed = await this.stop();
      await this.start(target);
      return closed;
    },
    async press() {
      if (state.current) {
        return { did: "stopped", closed: await this.stop() };
      }
      const target = state.foreground;
      if (!target) return { did: "pick", closed: null };
      await this.start(target);
      return { did: "started", closed: null };
    },
    begin() {
      if (live) {
        // Already begun. Handing back a teardown that unwinds the *first*
        // set of intervals would strand them if the second caller stopped
        // first, so this one is a no-op and the first teardown remains the
        // real one -- the rule `health.svelte.ts`'s `start()` records.
        return () => {};
      }
      live = true;
      let stopped = false;
      let unlisten: (() => void) | undefined;

      // The clock, so `elapsed` moves. One interval for the whole session
      // rather than one per component: the value is one value.
      const ticking = setInterval(() => {
        state.now = io.now();
      }, TICK_MS);

      const beating = setInterval(() => {
        // The one condition. A beat from a window nobody is looking at is a
        // lie about when knobas was last in front of somebody, and the whole
        // of what the stamp is for.
        if (!io.focused()) return;
        void io
          .timerHeartbeat(state.foreground)
          .then((timer) => {
            state.current = timer;
          })
          .catch(() => {
            // Same as `refresh`: keep what is on screen. A beat is sent again
            // in thirty seconds.
          });
      }, HEARTBEAT_MS);

      // A start or a stop — this window's or another surface's — is an
      // activity line, and that is the signal (`knobas_app::time`: no event of
      // the timer's own). `listen` is itself an `invoke` and resolves a tick
      // or more later, so the "torn down before it resolved" race is handled
      // the way `health.svelte.ts` handles it.
      void io
        .listen(EVENTS.activityNew, () => void refresh())
        .then((off) => {
          if (stopped) {
            off();
            return;
          }
          unlisten = off;
        })
        .catch(() => {
          // A failed subscription is not a failed window, the rule
          // `health.svelte.ts` records: the clock still ticks, the beat still
          // lands, and this window's own starts and stops still write through
          // the store. What is lost is only news of a change made elsewhere.
        });

      return () => {
        live = false;
        stopped = true;
        clearInterval(ticking);
        clearInterval(beating);
        unlisten?.();
      };
    },
  };
}

/**
 * The session's timer. Mounted once by `App.svelte`, read by the strip, the
 * keyboard and the launcher.
 */
export const timer = createTimer();

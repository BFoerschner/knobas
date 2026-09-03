/**
 * The timer store: **⌘T's three behaviours**, the heartbeat's one condition,
 * and what the strip reads (issue #278).
 *
 * The seam is the store rather than `keys.ts`, because that is where the rule
 * lives: `keys.ts` says *the key was pressed* and `press()` says what it means
 * (its own documentation records why the line is drawn there). `keys.test`
 * covers the binding; this covers the meaning.
 *
 * Every port is a fake, so nothing here needs `window.__TAURI_INTERNALS__`,
 * and the clock and the focus are dictated rather than waited for.
 */
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { Block, RunningTimer, TimerTarget } from "../ipc/time";
import { createTimer, HEARTBEAT_MS, type TimerPorts } from "./timer.svelte";

const TICKET: TimerTarget = { kind: "entity", entity_id: "jira:PAY-231" };
const LABEL: TimerTarget = { kind: "label", label: "DB config for the migration" };
const STARTED = "2026-09-03T09:00:00Z";

function running(target: TimerTarget = TICKET, started = STARTED): RunningTimer {
  return { target, started_at: started, last_heartbeat: started };
}

function block(): Block {
  return {
    id: 1,
    started_at: STARTED,
    ended_at: "2026-09-03T09:45:00Z",
    target: TICKET,
    kind: "manual",
    ended_by_relaunch: false,
    worklog_id: null,
  };
}

/**
 * A bench with every port recorded, no bridge behind any of them, and a clock
 * and a focus the test dictates.
 */
function bench(overrides: Partial<TimerPorts> = {}) {
  const calls = {
    started: [] as TimerTarget[],
    stopped: 0,
    beats: [] as (TimerTarget | null)[],
    reads: 0,
  };
  let focused = true;
  let now = new Date(STARTED);
  let announce: (() => void) | undefined;

  const ports: Partial<TimerPorts> = {
    currentTimer: () => {
      calls.reads += 1;
      return Promise.resolve(null);
    },
    startTimer: (target) => {
      calls.started.push(target);
      return Promise.resolve(running(target));
    },
    stopTimer: () => {
      calls.stopped += 1;
      return Promise.resolve(block());
    },
    timerHeartbeat: (foreground) => {
      calls.beats.push(foreground);
      return Promise.resolve(null);
    },
    // The activity signal, modelled as a callback the test fires.
    listen: ((_name: string, handler: () => void) => {
      announce = handler;
      return Promise.resolve(() => {
        announce = undefined;
      });
    }) as unknown as TimerPorts["listen"],
    focused: () => focused,
    now: () => now,
    ...overrides,
  };

  return {
    timer: createTimer(ports),
    calls,
    blur: () => {
      focused = false;
    },
    focus: () => {
      focused = true;
    },
    tickTo: (iso: string) => {
      now = new Date(iso);
    },
    announce: () => announce?.(),
    listening: () => announce !== undefined,
  };
}

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

// -- ⌘T's three behaviours --------------------------------------------------

/** Behaviour 1: nothing running, something in front → start on it. */
test("⌘T with nothing running starts the timer on the foreground entity", async () => {
  const { timer, calls } = bench();
  timer.foreground = TICKET;

  await expect(timer.press()).resolves.toBe("started");
  expect(calls.started).toEqual([TICKET]);
  expect(timer.current?.target).toEqual(TICKET);
});

/** Behaviour 2: nothing running and nothing in front → the picker. */
test("⌘T with nothing running and nothing in front asks for a target", async () => {
  const { timer, calls } = bench();
  timer.foreground = null;

  await expect(timer.press()).resolves.toBe("pick");
  expect(calls.started, "a timer was started on nothing").toEqual([]);
  expect(timer.current).toBeNull();
});

/** Behaviour 3: running → stop, whatever is in front of the reader. */
test("⌘T with a timer running stops it, and does not start a second", async () => {
  const { timer, calls } = bench();
  timer.foreground = LABEL;
  await timer.start(TICKET);

  await expect(timer.press()).resolves.toBe("stopped");
  expect(calls.stopped).toBe(1);
  expect(calls.started, "the foreground was started over the stop").toEqual([TICKET]);
  expect(timer.current).toBeNull();
});

/**
 * A refused start leaves the store saying nothing is running, so the strip
 * does not draw a timer the backend refused to create.
 */
test("a refused start leaves the strip empty rather than optimistic", async () => {
  const { timer } = bench({
    startTimer: () => Promise.reject({ code: "conflict", message: "already running" }),
  });
  timer.foreground = TICKET;

  await expect(timer.press()).rejects.toMatchObject({ code: "conflict" });
  expect(timer.current).toBeNull();
});

/**
 * Story 11: *Start timer* on another entity **stops the running one first**,
 * so the block the reader was in is closed rather than lost.
 *
 * The assertion is the **order**, read off one call log rather than off two
 * counters: "stop happened" and "start happened" are both true whichever way
 * round they ran, and the wrong way round is exactly the bug — a start over a
 * running timer is refused by the backend with `conflict`, so the switch would
 * fail outright and the reader would be left on the old target.
 */
test("switching targets stops before it starts, and answers with the block that closed", async () => {
  const order: string[] = [];
  const belt = bench({
    stopTimer: () => {
      order.push("stop");
      return Promise.resolve(block());
    },
    startTimer: (target) => {
      order.push("start");
      return Promise.resolve(running(target));
    },
  });
  await belt.timer.start(TICKET);
  order.length = 0;

  const closed = await belt.timer.switchTo(LABEL);

  expect(order).toEqual(["stop", "start"]);
  expect(belt.timer.current?.target).toEqual(LABEL);
  // What #280's worklog draft opens on.
  expect(closed?.target).toEqual(TICKET);
});

/** ...and switching from nothing is a plain start, not a failure. */
test("switching with nothing running is just a start", async () => {
  const belt = bench({ stopTimer: () => Promise.resolve(null) });

  await expect(belt.timer.switchTo(TICKET)).resolves.toBeNull();
  expect(belt.calls.started).toEqual([TICKET]);
});

// -- what the strip reads ---------------------------------------------------

test("elapsed is null while nothing runs, and counts up once something does", async () => {
  const belt = bench();
  expect(belt.timer.elapsed).toBeNull();

  await belt.timer.start(TICKET);
  const stop = belt.timer.begin();
  expect(belt.timer.elapsed).toBe("0:00");

  belt.tickTo("2026-09-03T09:00:07Z");
  vi.advanceTimersByTime(1000);
  expect(belt.timer.elapsed).toBe("0:07");

  stop();
});

// -- the heartbeat ----------------------------------------------------------

test("the heartbeat is sent every thirty seconds and carries the foreground", () => {
  const belt = bench();
  belt.timer.foreground = TICKET;
  const stop = belt.timer.begin();

  vi.advanceTimersByTime(HEARTBEAT_MS - 1);
  expect(belt.calls.beats, "a beat went early").toEqual([]);

  vi.advanceTimersByTime(1);
  expect(belt.calls.beats).toEqual([TICKET]);

  belt.timer.foreground = null;
  vi.advanceTimersByTime(HEARTBEAT_MS);
  // `null` is an observation and not a missing one: nothing was in front of
  // the reader, which is what #282 will record as an unattributed gap.
  expect(belt.calls.beats).toEqual([TICKET, null]);

  stop();
});

/**
 * **The one condition**, and the reason it is load-bearing: the stamp is what
 * a stranded timer's block is closed at, so a beat from a window nobody is
 * looking at would place a crash later than it happened — and it is what
 * passive attribution will read, where an unfocused window must never count as
 * work (story 27).
 */
test("no heartbeat is sent while the window is unfocused, and they resume when it returns", () => {
  const belt = bench();
  belt.blur();
  const stop = belt.timer.begin();

  vi.advanceTimersByTime(HEARTBEAT_MS * 3);
  expect(belt.calls.beats, "an unfocused window said it was alive").toEqual([]);

  belt.focus();
  vi.advanceTimersByTime(HEARTBEAT_MS);
  expect(belt.calls.beats).toHaveLength(1);

  stop();
});

// -- the activity signal ----------------------------------------------------

/**
 * There is no timer event (`knobas_app::time`). A stop made anywhere else —
 * the launcher, another surface — reaches the strip because it wrote an
 * activity line, and that is the signal this store already watches.
 */
test("an activity line makes the store re-read the timer", async () => {
  const belt = bench();
  const stop = belt.timer.begin();
  await vi.advanceTimersByTimeAsync(0);
  expect(belt.listening(), "the activity signal was never subscribed to").toBe(true);

  const before = belt.calls.reads;
  belt.announce();
  await vi.advanceTimersByTimeAsync(0);
  expect(belt.calls.reads).toBe(before + 1);

  stop();
});

test("the subscription, the clock and the heartbeat are all torn down", async () => {
  const belt = bench();
  const stop = belt.timer.begin();
  await vi.advanceTimersByTimeAsync(0);

  stop();
  expect(belt.listening(), "the activity subscription outlived the store").toBe(false);
  vi.advanceTimersByTime(HEARTBEAT_MS * 2);
  expect(belt.calls.beats, "the heartbeat outlived the store").toEqual([]);
});

/**
 * **Beginning twice is one set of intervals, not two.**
 *
 * `App.svelte` calls `begin()` once, so this is latent -- and latent is
 * exactly what the rule `health.svelte.ts` records is about: the second
 * caller's teardown would unwind the first caller's subscription and strand
 * the second's, so a store that answered both with a real teardown would leak
 * a heartbeat that nobody can stop. The witness is the beat, because a second
 * interval is a second beat every thirty seconds.
 */
test("beginning twice does not double the heartbeat, and the first teardown is the real one", async () => {
  const belt = bench();
  const stop = belt.timer.begin();
  const stopAgain = belt.timer.begin();
  await vi.advanceTimersByTimeAsync(0);

  vi.advanceTimersByTime(HEARTBEAT_MS);
  expect(belt.calls.beats, "a second begin() started a second heartbeat").toHaveLength(1);

  // The second caller's teardown is the no-op, so it strands nothing...
  stopAgain();
  vi.advanceTimersByTime(HEARTBEAT_MS);
  expect(belt.calls.beats, "the no-op teardown stopped the real heartbeat").toHaveLength(2);

  // ...and the first caller's is still the one that stops everything.
  stop();
  vi.advanceTimersByTime(HEARTBEAT_MS * 2);
  expect(belt.calls.beats, "the first teardown did not stop the heartbeat").toHaveLength(2);
  expect(belt.listening()).toBe(false);
});

/**
 * **A subscription that never lands is not a window that stops working.**
 *
 * `listen` is itself an `invoke`, and it rejects when there is no bridge
 * behind it -- which is what browser QA under `?fake-ipc` looks like if the
 * fixture is not installed yet. The rule `health.svelte.ts` records: keep the
 * rest running. What is lost is only news of a change made elsewhere; the
 * clock still ticks and the beat still lands.
 */
test("a subscription that rejects leaves the clock and the heartbeat running", async () => {
  const belt = bench({ listen: (() => Promise.reject(new Error("no bridge"))) as never });
  const stop = belt.timer.begin();
  await vi.advanceTimersByTimeAsync(0);

  vi.advanceTimersByTime(HEARTBEAT_MS);
  expect(belt.calls.beats, "a failed subscription took the heartbeat with it").toHaveLength(1);
  stop();
});

/**
 * A failed read keeps what is on screen. `current_timer` rejects with
 * `not_ready` for the whole of bring-up, and a strip that blanked on that
 * would lose a running timer every time the database restarted under it.
 */
test("a failed read leaves the last known timer alone", async () => {
  const belt = bench({ currentTimer: () => Promise.reject(new Error("not_ready")) });
  await belt.timer.start(TICKET);

  await belt.timer.refresh();
  expect(belt.timer.current?.target).toEqual(TICKET);
});

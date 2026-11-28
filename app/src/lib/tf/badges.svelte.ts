/**
 * The top bar's count badges (#809, R-1.3), beside what the inbox store already
 * counts:
 *
 * - **Builds**: the unread runs by bubble (#2568, ADR-0063, `run_counts`):
 *   blue running, green passed, yellow passed with warnings, red failed,
 *   grey cancelled, every pipeline's summed, as the runs store holds them
 *   (`runs-read.svelte.ts`'s `topBar`), so a mark moves them at once. With
 *   Builds in front there is no green, yellow or grey. The room's Builds tile
 *   still counts every failing pipeline: there it is a fact about the work,
 *   here what still asks.
 * - **Monitors**: monitors down, `monitors_down`: the Monitors tab's own rows
 *   and state rule, counted by the backend. A paused (tombstoned) monitor is not down; it is not watching. A
 *   hidden one is left out too: its alerts reach nobody, so the count would be
 *   a state the reader cannot act on.
 *
 * Each is `null` until its read answers and stays at its last answer when a
 * later read fails: a badge that dropped to nothing on a failed read would
 * say the trouble was over.
 *
 * Re-read when a sync run finishes and when an activity line lands, the two
 * events the inbox store already moves on. Every read waits for the database,
 * which the shell signals by calling {@link Badges.refresh}.
 */
import { listen as tauriListen } from "@tauri-apps/api/event";

import { EVENTS } from "../ipc";
import { coalesce } from "../shell/coalesce";
import { monitorsDown, type MonitorRow } from "../ipc/assets";
import type { BubbleCounts } from "../ipc/entity";
import { runsRead } from "./builds/runs-read.svelte";
import { BUBBLE_WORDS, RAIL_COLUMNS } from "./builds/builds";
import type { SourceSyncStatus } from "../ipc/sources";

export interface BadgePorts {
  /** Read the runs' counts again; the badge draws {@link BadgePorts.builds}. */
  readBuilds: () => Promise<void>;
  /** The top bar's counts as last read, `null` before: a rune's read, so the badge follows a mark at once. */
  builds: () => BubbleCounts | null;
  monitorsDown: () => Promise<number>;
  listen: (event: string, handler: (event: { payload: unknown }) => void) => Promise<() => void>;
}

export interface Badges {
  readonly builds: BubbleCounts | null;
  readonly monitors: number | null;
  refresh(): Promise<void>;
  /**
   * The monitors' count alone: the shell's start (#2569b). The builds' counts are read by the runs store's first word
   * to the backend (`runsRead.started`, `inFront`), so a read here too went out beside Recent's first page.
   */
  readMonitors(): Promise<void>;
  /** Subscribe to the events that move the counts. Returns the teardown. */
  start(): () => void;
  /** Tests only: unsubscribe and forget the counts (#1743). */
  reset(): void;
}

/** One state's count in the top bar's Builds bubble. */
export interface BubblePart {
  bubble: keyof BubbleCounts;
  n: number;
  /** What the number counts, in words: the hidden text and the title say it, for whoever can't tell the colours. */
  word: string;
}

/** The top bar's Builds bubble: the unread total, the colour it is drawn in, and the counts it expands to. */
export interface BuildsBubble {
  total: number;
  /** The most critical state among them: red, else yellow, else green, else grey; blue only when nothing finished. */
  tone: keyof BubbleCounts;
  /** Each state above zero, most critical first: red, yellow, green, grey, then blue. */
  parts: BubblePart[];
}

/** The states most critical first, each with its word (Björn on #2568, 2026-11-27). */
const BY_CRITICAL: readonly (readonly [keyof BubbleCounts, string])[] = RAIL_COLUMNS.map((bubble) => [bubble, BUBBLE_WORDS[bubble]] as const);

/**
 * The top bar's one Builds bubble (Björn on #2568, 2026-11-27, replacing up to three numbers): the total of unread
 * runs, coloured by the most critical state present, expanding to the counts by state. `null` at zero or before the
 * counts are read: after *Mark all as read* there is no bubble.
 */
export function buildsBubble(counts: BubbleCounts | null): BuildsBubble | null {
  if (counts === null) return null;
  const parts = BY_CRITICAL.filter(([bubble]) => counts[bubble] > 0).map(([bubble, word]) => ({ bubble, n: counts[bubble], word }));
  const first = parts[0];
  if (first === undefined) return null;
  return { total: parts.reduce((sum, part) => sum + part.n, 0), tone: first.bubble, parts };
}

/** Monitors whose last reading is down, paused and hidden ones left out. */
export function downIn(rows: Pick<MonitorRow, "state" | "tombstoned" | "hidden">[]): number {
  return rows.filter((row) => row.state === "down" && !row.tombstoned && !row.hidden).length;
}

const REAL_PORTS: BadgePorts = {
  readBuilds: () => runsRead.readCounts(),
  builds: () => runsRead.topBar,
  // A count, not the roster: the roster carries each monitor's day of samples, and this is read on every sync's end,
  // on every page (PR #2479, the monitors-builds pass: 17.8 MB after a day at 218 monitors).
  monitorsDown: () => monitorsDown(),
  listen: (event, handler) => tauriListen(event, handler),
};

export function createBadges(ports: BadgePorts = REAL_PORTS): Badges {
  const state = $state<{ monitors: number | null }>({
    monitors: null,
  });
  let live = false;
  let stopLive: (() => void) | null = null;

  async function readMonitors() {
    try {
      state.monitors = await ports.monitorsDown();
    } catch {
      // Keep the last answer; see the module note.
    }
  }

  // Coalesced (the query audit, 2026-11-12): a sync wave's run ends and activity lines asked both reads six times in
  // 150 ms. While a read is out, any number of events make one more read after it, so the last word still follows the
  // last event (`coalesce`, as the inbox and the alerts read).
  const refresh = coalesce(async () => {
    // The builds' read keeps its last answer itself (`runsRead.readCounts`).
    await Promise.all([ports.readBuilds().catch(() => {}), readMonitors()]);
  });

  return {
    get builds() {
      return ports.builds();
    },
    get monitors() {
      return state.monitors;
    },
    refresh,
    readMonitors,
    start() {
      if (live) return () => {};
      live = true;
      // This subscription's own flag: a stop and a second start before the
      // first `listen` resolves must not hand the new one an old unlisten.
      let dead = false;
      const offs: (() => void)[] = [];
      const subscribe = (event: string, when: (payload: unknown) => boolean) => {
        void ports
          .listen(event, ({ payload }) => {
            if (!dead && when(payload)) void refresh();
          })
          .then((off) => {
            if (dead) off();
            else offs.push(off);
          })
          .catch(() => {});
      };
      subscribe(EVENTS.syncState, (payload) => !(payload as SourceSyncStatus).running);
      subscribe(EVENTS.activityNew, () => true);
      const stop = () => {
        live = false;
        dead = true;
        for (const off of offs.splice(0)) off();
      };
      stopLive = stop;
      return stop;
    },
    reset() {
      stopLive?.();
      stopLive = null;
      state.monitors = null;
    },
  };
}

/** The window's one. Tests build their own. */
export const badges = createBadges();

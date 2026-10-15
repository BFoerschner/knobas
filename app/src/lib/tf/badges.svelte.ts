/**
 * The top bar's count badges (#809, R-1.3), beside what the inbox store already
 * counts:
 *
 * - **Builds**: build configurations whose latest build failed. The room's
 *   Builds tile already asks this: `list_entities` under `latest_per_parent`
 *   keeps each configuration's newest build and tallies the set by tone, so
 *   the failing configurations are the `err` lines of that tally. No new
 *   command, and no second definition of *failing*.
 * - **Monitors**: monitors down, off `monitor_roster`, the Monitors tab's own
 *   read. A paused (tombstoned) monitor is not down; it is not watching.
 * - **Proposed links**: undecided proposals, `room_suggestions` over every
 *   source with no context, whose `total` is the count before its `limit`.
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
import { monitorRoster, type MonitorRow } from "../ipc/assets";
import { listEntities, roomSuggestions, type EntityPage } from "../ipc/entity";
import type { SourceSyncStatus } from "../ipc/sources";

export interface BadgePorts {
  failingBuilds: () => Promise<number>;
  monitorsDown: () => Promise<number>;
  proposals: () => Promise<number>;
  listen: (event: string, handler: (event: { payload: unknown }) => void) => Promise<() => void>;
}

export interface Badges {
  readonly builds: number | null;
  readonly monitors: number | null;
  readonly proposals: number | null;
  refresh(): Promise<void>;
  /** Subscribe to the events that move the counts. Returns the teardown. */
  start(): () => void;
}

/** Failing configurations, off the Builds tile's own tally. */
export function failingIn(page: Pick<EntityPage, "tally">): number {
  return (page.tally ?? [])
    .filter((line) => line.tone === "err")
    .reduce((all, line) => all + line.count, 0);
}

/** Monitors whose last reading is down, paused ones left out. */
export function downIn(rows: Pick<MonitorRow, "state" | "tombstoned">[]): number {
  return rows.filter((row) => row.state === "down" && !row.tombstoned).length;
}

const REAL_PORTS: BadgePorts = {
  failingBuilds: () =>
    listEntities(
      {
        sources: [],
        kinds: ["build"],
        updated_within_days: null,
        order: "latest_per_parent",
        context: null,
        project: null,
        include_deleted: false,
      },
      1,
      0,
    ).then(failingIn),
  monitorsDown: () => monitorRoster().then(downIn),
  proposals: () => roomSuggestions([], null, 1).then((page) => page.total),
  listen: (event, handler) => tauriListen(event, handler),
};

export function createBadges(ports: BadgePorts = REAL_PORTS): Badges {
  const state = $state<{ builds: number | null; monitors: number | null; proposals: number | null }>({
    builds: null,
    monitors: null,
    proposals: null,
  });
  let live = false;

  async function read(key: keyof typeof state, run: () => Promise<number>) {
    try {
      state[key] = await run();
    } catch {
      // Keep the last answer; see the module note.
    }
  }

  async function refresh() {
    await Promise.all([
      read("builds", ports.failingBuilds),
      read("monitors", ports.monitorsDown),
      read("proposals", ports.proposals),
    ]);
  }

  return {
    get builds() {
      return state.builds;
    },
    get monitors() {
      return state.monitors;
    },
    get proposals() {
      return state.proposals;
    },
    refresh,
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
      return () => {
        live = false;
        dead = true;
        for (const off of offs.splice(0)) off();
      };
    },
  };
}

/** The window's one. Tests build their own. */
export const badges = createBadges();

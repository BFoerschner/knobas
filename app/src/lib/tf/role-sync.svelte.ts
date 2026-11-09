/**
 * A role view's *Sync now* (#2106): one ask per role, held by the shell's
 * role frame (`TfPage.svelte`) so a view drawn again mid-run, or left and
 * come back to, still shows its runs going.
 *
 * `sync_role` names the role and the mirror answers which sources fill it:
 * one incremental run of each enabled filler, started or joined. Those runs
 * are in progress until `sync:state` says each ended (ADR-0005: a run id
 * always comes with an ending). Then {@link RoleSync.revision} moves, and a
 * view whose rows are not re-read on a run's end by themselves re-reads on it.
 *
 * Nothing here decides who fills a role: the answer's runs are the runs.
 *
 * A press ends in one toast, replaced by the next press's (F-448), and it names the source, never the role (F-455, F-068):
 * *Synced Tidewater Confluence*, *Tidewater TeamCity is up to date*, *Could not sync Tidewater TeamCity: the sign-in failed*.
 * Several runs read *Synced 2 sources*, and a run that failed is named whichever the others did. The ask refused keeps its
 * *Could not sync: <why>*, which the button raises.
 */
import { EVENTS, ipcErrorMessage } from "../ipc";
import { syncNow, syncRole, syncStatus, type RoleRun, type SourceSyncStatus } from "../ipc/sources";
import { listen as tauriListen } from "@tauri-apps/api/event";
import { push } from "../shell/toasts.svelte";
import { sourceRoles } from "./roles.svelte";

export interface RoleSyncPorts {
  syncRole: (role: string) => Promise<RoleRun[]>;
  /** One source's run, started or joined: a detail's *Sync now* (#2107). */
  syncNow: (sourceId: string) => Promise<number>;
  syncStatus: () => Promise<SourceSyncStatus[]>;
  listen: (event: string, handler: (event: { payload: unknown }) => void) => Promise<() => void>;
}

/** Why a run that ended didn't sync, in the words after *Could not sync <name>:*. */
const WHY = { unauthorized: "the sign-in failed", unreachable: "the source can't be reached", error: "the run failed" } as const;

/** A run an ask answered. *Sync all* is told no source: each run's ending says whose it was. */
type AskedRun = { source_id: string | null; run_id: number };

/** What {@link createRoleSync} holds a run under while it goes. */
const runKey = (runId: number | null) => `run:${runId}`;

/** One role's *Sync now*. */
export interface RoleSync {
  readonly role: string;
  /** Whether runs it asked for are still going, or the ask is out. */
  readonly running: boolean;
  /** How many runs are still going. */
  readonly going: number;
  /** Moves once each time every run of an ask has ended. */
  readonly revision: number;
  /**
   * Moves each time a run of a source that fills the role ends having changed something, whoever started it: a scheduled
   * sync, a write's follow-up run (the third smoothness pass of PR #2479). A shown role view reads again on it, so a
   * ticket made elsewhere comes onto the board without leaving it. Heard from the first time it is read.
   */
  readonly ran: number;
  /** Why the last ask was refused, or `null`. */
  readonly failed: string | null;
  /** Ask for the runs; nothing while they still go. `name` is what the answer calls a single run: else its source's name. */
  start(name?: string): Promise<void>;
}

export interface RoleSyncs {
  /** The role's *Sync now*, the same one each time it is asked for. */
  of(role: string): RoleSync;
  /** Back to no role asked for. */
  forget(): void;
}

/** The bridge's ports. */
export const ROLE_SYNC_BRIDGE: RoleSyncPorts = {
  syncRole: (role) => syncRole(role),
  syncNow: (sourceId) => syncNow(sourceId),
  syncStatus: () => syncStatus(),
  listen: (event, handler) => tauriListen(event, handler),
};

export function createRoleSyncs(ports: RoleSyncPorts): RoleSyncs {
  return createSyncs(ports, (role) => ports.syncRole(role), (role, sourceId) => sourceRoles.filledBy(role, sourceId));
}

/**
 * A detail's *Sync now* (#2107), for an item whose adapter does not read one
 * item: the item's source's run, held by source id the way a role's runs are
 * held by role, so every detail of one source shows the one run going.
 * `sync_now` joins a run already in flight rather than starting a second.
 * {@link RoleSync.role} is the source's id here.
 */
export function createSourceSyncs(ports: Omit<RoleSyncPorts, "syncRole">): RoleSyncs {
  return createSyncs(
    ports,
    async (sourceId) => [{ source_id: sourceId, run_id: await ports.syncNow(sourceId) }],
    (name, sourceId) => name === sourceId,
  );
}

/**
 * Settings' *Sync all* (F-652): the runs `sync_all` started or joined, which it answers by id alone. Its answer is on the
 * one subject `sync:all`. One for each press, so a press while some still go starts what has ended.
 */
export function createAllSync(ports: Pick<RoleSyncPorts, "syncStatus" | "listen">, syncAll: () => Promise<number[]>): RoleSync {
  return createRoleSync(ports, "all", async () => (await syncAll()).map((run_id) => ({ source_id: null, run_id })), () => true);
}

/** The runs asked for under one name, the same ones each time it is asked for. */
function createSyncs(
  ports: Pick<RoleSyncPorts, "syncStatus" | "listen">,
  ask: (name: string) => Promise<RoleRun[]>,
  counts: (name: string, sourceId: string) => boolean,
): RoleSyncs {
  const held = new Map<string, RoleSync>();
  return {
    of(name) {
      let sync = held.get(name);
      if (sync === undefined) {
        sync = createRoleSync(ports, name, () => ask(name), (sourceId) => counts(name, sourceId));
        held.set(name, sync);
      }
      return sync;
    },
    forget() {
      held.clear();
    },
  };
}

function createRoleSync(
  ports: Pick<RoleSyncPorts, "syncStatus" | "listen">,
  role: string,
  ask: () => Promise<AskedRun[]>,
  counts: (sourceId: string) => boolean,
): RoleSync {
  const state = $state<{ asking: boolean; pending: Record<string, number>; revision: number; failed: string | null; ran: number }>({
    asking: false,
    pending: {},
    revision: 0,
    failed: null,
    ran: 0,
  });
  /** Whether `ran` is heard yet: from its first read, for as long as the window lives. */
  let hearing = false;
  function hearRuns() {
    if (hearing) return;
    hearing = true;
    void ports
      .listen(EVENTS.syncState, ({ payload }) => {
        const row = payload as SourceSyncStatus;
        if (!row.running && row.run_id !== null && row.last_changed !== false && counts(row.source_id)) state.ran += 1;
      })
      .catch(() => {
        hearing = false;
      });
  }
  let stop: (() => void) | null = null;
  /** What this ask's runs have said so far, for its answer. */
  let said: { name: string | undefined; runs: number; last: string; failed: Map<string, string[]>; changed: boolean } = { name: undefined, runs: 0, last: role, failed: new Map(), changed: false };

  const going = () => Object.keys(state.pending).length;

  /** What the press ends in, once every run has. */
  function answer() {
    const { runs, last, failed, changed } = said;
    const subject = `sync:${role}`;
    if (failed.size > 0) {
      const lines = [...failed].map(([why, names]) => `Could not sync ${new Intl.ListFormat("en").format(names)}: ${why}`);
      push({ text: lines.join(". "), tone: "err", subject });
    } else if (runs === 1) push({ text: changed ? `Synced ${last}` : `${last} is up to date`, subject });
    else push({ text: changed ? `Synced ${runs} sources` : `${runs} sources are up to date`, subject });
  }

  function finish(ended: boolean) {
    stop?.();
    stop = null;
    state.asking = false;
    state.pending = {};
    if (ended) state.revision += 1;
  }

  /** One status heard: a run of ours that is no longer going has ended. */
  function heard(row: SourceSyncStatus) {
    const key = row.source_id in state.pending ? row.source_id : runKey(row.run_id);
    const ours = state.pending[key];
    if (ours === undefined || row.running || row.run_id === null || row.run_id < ours) return;
    const { [key]: _ended, ...rest } = state.pending;
    state.pending = rest;
    said.last = said.name ?? sourceRoles.nameOf(row.source_id);
    if (row.last_outcome !== null && row.last_outcome !== "ok") {
      const why = WHY[row.last_outcome];
      said.failed.set(why, [...(said.failed.get(why) ?? []), said.last]);
    }
    if (row.last_changed !== false) said.changed = true;
    if (!state.asking && going() === 0) {
      answer();
      finish(true);
    }
  }

  return {
    role,
    get running() {
      return state.asking || going() > 0;
    },
    get going() {
      return going();
    },
    get revision() {
      return state.revision;
    },
    get ran() {
      hearRuns();
      return state.ran;
    },
    get failed() {
      return state.failed;
    },
    async start(name) {
      if (state.asking || going() > 0) return;
      state.asking = true;
      state.failed = null;
      said = { name, runs: 0, last: role, failed: new Map(), changed: false };
      try {
        // Listening first, so an ending sent while the answer travels is heard.
        const off = await ports.listen(EVENTS.syncState, ({ payload }) => heard(payload as SourceSyncStatus));
        stop = off;
        const runs = await ask();
        state.pending = Object.fromEntries(runs.map((run) => [run.source_id ?? runKey(run.run_id), run.run_id]));
        said.runs = runs.length;
        state.asking = false;
        if (runs.length === 0) {
          push({ text: "Nothing to sync", subject: `sync:${role}` });
          finish(false);
          return;
        }
        // A run that ended before the listener caught it is over by the status.
        // A failed status read loses nothing: the listener still hears each ending.
        for (const row of await ports.syncStatus().catch(() => [])) heard(row);
      } catch (error) {
        state.failed = ipcErrorMessage(error);
        finish(false);
      }
    },
  };
}

/** The window's. */
export const roleSyncs = createRoleSyncs(ROLE_SYNC_BRIDGE);

/** The window's, by source: the details' *Sync now* (#2107). */
export const sourceSyncs = createSourceSyncs(ROLE_SYNC_BRIDGE);

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
 * A press ends in one toast, replaced by the next press's (F-448): *Synced wiki*, *TeamCity is up to date*, and a run that
 * failed says so. The ask refused keeps its *Could not sync: <why>*, which the button raises.
 */
import { EVENTS, ipcErrorMessage } from "../ipc";
import { syncNow, syncRole, syncStatus, type RoleRun, type SourceSyncStatus } from "../ipc/sources";
import { listen as tauriListen } from "@tauri-apps/api/event";
import { push } from "../shell/toasts.svelte";
import { roleWord } from "./settings/settings";

export interface RoleSyncPorts {
  syncRole: (role: string) => Promise<RoleRun[]>;
  /** One source's run, started or joined: a detail's *Sync now* (#2107). */
  syncNow: (sourceId: string) => Promise<number>;
  syncStatus: () => Promise<SourceSyncStatus[]>;
  listen: (event: string, handler: (event: { payload: unknown }) => void) => Promise<() => void>;
}

/** Why a run that ended didn't sync, in the words after *Could not sync <name>:*. */
const WHY = { unauthorized: "the sign-in failed", unreachable: "the source can't be reached", error: "the run failed" } as const;

/** One role's *Sync now*. */
export interface RoleSync {
  readonly role: string;
  /** Whether runs it asked for are still going, or the ask is out. */
  readonly running: boolean;
  /** How many runs are still going. */
  readonly going: number;
  /** Moves once each time every run of an ask has ended. */
  readonly revision: number;
  /** Why the last ask was refused, or `null`. */
  readonly failed: string | null;
  /** Ask for the runs; nothing while they still go. `name` is what the answer calls them: else the role's word. */
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
  return createSyncs(ports, (role) => ports.syncRole(role));
}

/**
 * A detail's *Sync now* (#2107), for an item whose adapter does not read one
 * item: the item's source's run, held by source id the way a role's runs are
 * held by role, so every detail of one source shows the one run going.
 * `sync_now` joins a run already in flight rather than starting a second.
 * {@link RoleSync.role} is the source's id here.
 */
export function createSourceSyncs(ports: RoleSyncPorts): RoleSyncs {
  return createSyncs(ports, async (sourceId) => [{ source_id: sourceId, run_id: await ports.syncNow(sourceId) }]);
}

/** The runs asked for under one name, the same ones each time it is asked for. */
function createSyncs(ports: RoleSyncPorts, ask: (name: string) => Promise<RoleRun[]>): RoleSyncs {
  const held = new Map<string, RoleSync>();
  return {
    of(name) {
      let sync = held.get(name);
      if (sync === undefined) {
        sync = createRoleSync(ports, name, () => ask(name));
        held.set(name, sync);
      }
      return sync;
    },
    forget() {
      held.clear();
    },
  };
}

function createRoleSync(ports: RoleSyncPorts, role: string, ask: () => Promise<RoleRun[]>): RoleSync {
  const state = $state<{ asking: boolean; pending: Record<string, number>; revision: number; failed: string | null }>({
    asking: false,
    pending: {},
    revision: 0,
    failed: null,
  });
  let stop: (() => void) | null = null;
  /** What this ask's runs have said so far, for its answer. */
  let said: { name: string; why: string | null; changed: boolean } = { name: role, why: null, changed: false };

  const going = () => Object.keys(state.pending).length;

  /** What the press ends in, once every run has. */
  function answer() {
    const { name, why, changed } = said;
    const subject = `sync:${role}`;
    if (why !== null) push({ text: `Could not sync ${name}: ${why}`, tone: "err", subject });
    else push({ text: changed ? `Synced ${name}` : `${name.charAt(0).toUpperCase()}${name.slice(1)} is up to date`, subject });
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
    const ours = state.pending[row.source_id];
    if (ours === undefined || row.running || row.run_id === null || row.run_id < ours) return;
    const { [row.source_id]: _ended, ...rest } = state.pending;
    state.pending = rest;
    if (row.last_outcome !== null && row.last_outcome !== "ok") said.why = WHY[row.last_outcome];
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
    get failed() {
      return state.failed;
    },
    async start(name = roleWord(role)) {
      if (state.asking || going() > 0) return;
      state.asking = true;
      state.failed = null;
      said = { name, why: null, changed: false };
      try {
        // Listening first, so an ending sent while the answer travels is heard.
        const off = await ports.listen(EVENTS.syncState, ({ payload }) => heard(payload as SourceSyncStatus));
        stop = off;
        const runs = await ask();
        state.pending = Object.fromEntries(runs.map((run) => [run.source_id, run.run_id]));
        state.asking = false;
        if (runs.length === 0) {
          push({ text: `Nothing to sync for ${name}`, subject: `sync:${role}` });
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

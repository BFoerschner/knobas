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
 */
import { EVENTS, ipcErrorMessage } from "../ipc";
import { syncRole, syncStatus, type RoleRun, type SourceSyncStatus } from "../ipc/sources";
import { listen as tauriListen } from "@tauri-apps/api/event";

export interface RoleSyncPorts {
  syncRole: (role: string) => Promise<RoleRun[]>;
  syncStatus: () => Promise<SourceSyncStatus[]>;
  listen: (event: string, handler: (event: { payload: unknown }) => void) => Promise<() => void>;
}

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
  /** Ask for the runs; nothing while they still go. */
  start(): Promise<void>;
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
  syncStatus: () => syncStatus(),
  listen: (event, handler) => tauriListen(event, handler),
};

export function createRoleSyncs(ports: RoleSyncPorts): RoleSyncs {
  const held = new Map<string, RoleSync>();
  return {
    of(role) {
      let sync = held.get(role);
      if (sync === undefined) {
        sync = createRoleSync(ports, role);
        held.set(role, sync);
      }
      return sync;
    },
    forget() {
      held.clear();
    },
  };
}

function createRoleSync(ports: RoleSyncPorts, role: string): RoleSync {
  const state = $state<{ asking: boolean; pending: Record<string, number>; revision: number; failed: string | null }>({
    asking: false,
    pending: {},
    revision: 0,
    failed: null,
  });
  let stop: (() => void) | null = null;

  const going = () => Object.keys(state.pending).length;

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
    if (!state.asking && going() === 0) finish(true);
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
    async start() {
      if (state.asking || going() > 0) return;
      state.asking = true;
      state.failed = null;
      try {
        // Listening first, so an ending sent while the answer travels is heard.
        const off = await ports.listen(EVENTS.syncState, ({ payload }) => heard(payload as SourceSyncStatus));
        stop = off;
        const runs = await ports.syncRole(role);
        state.pending = Object.fromEntries(runs.map((run) => [run.source_id, run.run_id]));
        state.asking = false;
        if (runs.length === 0) {
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

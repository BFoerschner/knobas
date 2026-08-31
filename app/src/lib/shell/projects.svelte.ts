/**
 * The projects the corpus shows, live — the switcher's third derived
 * population (#209, ADR-0010).
 *
 * The same shape as `contexts.svelte.ts` and `health.svelte.ts`, and for the
 * same reason: one fact with one home, so nothing in the shell can hold a
 * second opinion about which rooms exist. The seed is the shell's, once the
 * database can answer; what keeps it current afterwards is a **sync run
 * ending**, because that is the only event that says the mirror moved — a
 * project appears when its first item syncs and stops being offered when the
 * last one leaves, and until then nothing else would ever say so.
 */
import { listen as tauriListen } from "@tauri-apps/api/event";

import { EVENTS } from "../ipc";
import { listProjects as realListProjects, type Project } from "../ipc/entity";
import type { SourceSyncStatus } from "../ipc/sources";

/** The IPC this store needs, injectable so a test needs no Tauri bridge. */
export interface ProjectsPorts {
  listProjects: () => Promise<Project[]>;
  listen: (
    event: string,
    handler: (event: { payload: SourceSyncStatus }) => void,
  ) => Promise<() => void>;
}

export interface Projects {
  /** Every project the live corpus shows, ordered by source then key. */
  readonly all: Project[];
  /**
   * Read the census. The shell's to call once the database is ready, and any
   * caller's after it has just changed the corpus — a second reseed is only
   * ever a harmless double read.
   */
  reseed(): Promise<void>;
  /**
   * Subscribe to `sync:state`. Returns the teardown; a second `start` while
   * live is a no-op whose teardown unwinds nothing.
   */
  start(): () => void;
}

export function createProjects(ports?: ProjectsPorts): Projects {
  const io: ProjectsPorts = ports ?? {
    listProjects: realListProjects,
    listen: (event, handler) => tauriListen<SourceSyncStatus>(event, handler),
  };

  const state = $state<{ rows: Project[] }>({ rows: [] });
  let live = false;

  return {
    get all() {
      return state.rows;
    },
    async reseed() {
      try {
        const rows = await io.listProjects();
        if (!live) return;
        state.rows = rows;
      } catch {
        // `not_ready` during bring-up, or a read that failed: the switcher
        // keeps the rooms it has. The subscription and the shell's ready-seed
        // are the retries.
      }
    },
    start() {
      if (live) return () => {};
      live = true;
      let off: (() => void) | undefined;

      void io
        .listen(EVENTS.syncState, (event) => {
          // A run that just *finished* is new material; the payload is coarse
          // by rule, so this is the only signal there is that the mirror
          // moved. Re-listing mid-run would only read a half-written corpus.
          if (!live || event.payload.running) return;
          void this.reseed();
        })
        .then((unlisten) => {
          if (live) off = unlisten;
          else unlisten();
        })
        .catch(() => {
          // A failed subscription is not a failed window: the rooms seeded at
          // bring-up still stand, they simply stop moving.
        });

      return () => {
        live = false;
        off?.();
        off = undefined;
      };
    },
  };
}

/** The one the window uses. Tests build their own with {@link createProjects}. */
export const projects = createProjects();

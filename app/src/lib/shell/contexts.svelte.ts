/**
 * The stored contexts, live — one fact, one home (#47).
 *
 * The switcher, the room heading and the inbox's per-context filter all draw
 * the same list, and the pattern is `health.svelte.ts`'s for the same reason:
 * independent fetches are chances to disagree. The seed is the shell's, once
 * the database can answer; `contexts:changed` keeps it current afterwards —
 * fired on create and on the first promotion, and carrying the row, though
 * this store re-lists rather than splicing because `list_contexts` is one
 * cheap local read and the list is the authority on order.
 */
import { listen as tauriListen } from "@tauri-apps/api/event";

import { EVENTS } from "../ipc";
import { listContexts as realListContexts, type ContextRow } from "../ipc/entity";

/** The IPC this store needs, injectable so a test needs no Tauri bridge. */
export interface ContextsPorts {
  listContexts: () => Promise<ContextRow[]>;
  listen: (event: string, handler: () => void) => Promise<() => void>;
}

export interface Contexts {
  /** Every unarchived context, newest first — `list_contexts`'s own order. */
  readonly all: ContextRow[];
  /**
   * Read the whole list. The shell's to call once the database is ready, and
   * any caller's after it has just changed the set — the event also lands
   * here, so a second reseed is only ever a harmless double read.
   */
  reseed(): Promise<void>;
  /**
   * Subscribe to `contexts:changed`. Returns the teardown; a second `start`
   * while live is a no-op whose teardown unwinds nothing.
   */
  start(): () => void;
}

export function createContexts(ports?: ContextsPorts): Contexts {
  const io: ContextsPorts = ports ?? {
    listContexts: realListContexts,
    listen: (event, handler) => tauriListen(event, handler),
  };

  const state = $state<{ rows: ContextRow[] }>({ rows: [] });
  let live = false;

  return {
    get all() {
      return state.rows;
    },
    async reseed() {
      try {
        const rows = await io.listContexts();
        if (!live) return;
        state.rows = rows;
      } catch {
        // `not_ready` during bring-up, or a read that failed: the switcher
        // keeps what it has. The subscription and the shell's ready-seed are
        // the retries.
      }
    },
    start() {
      if (live) return () => {};
      live = true;
      let off: (() => void) | undefined;

      void io
        .listen(EVENTS.contextsChanged, () => {
          if (live) void this.reseed();
        })
        .then((unlisten) => {
          if (live) off = unlisten;
          else unlisten();
        })
        .catch(() => {
          // A failed subscription is not a failed window; creates in this
          // window still reseed through their own callers.
        });

      return () => {
        live = false;
        off?.();
        off = undefined;
      };
    },
  };
}

/** The one the window uses. Tests build their own with {@link createContexts}. */
export const contexts = createContexts();

/**
 * Land in a context that was just made (#47) — the one navigation both
 * makers share (the switcher's `+ new`, the detail's *Promote*).
 *
 * The store is reseeded directly as well as by `contexts:changed`, because
 * the navigation lands *now* and a room whose tab has not arrived yet would
 * flash the *All work* fallback.
 */
export async function openFreshContext(
  row: ContextRow,
  go: (hash: string) => void,
): Promise<void> {
  await contexts.reseed();
  go(`#/ctx/${row.id}`);
}

/**
 * Which **adapter** each configured source is an instance of (#285).
 *
 * One fact, one home, the shape `projects.svelte.ts` and `health.svelte.ts`
 * have. What needs it is the switcher's kind word: a project keeps its
 * source's own word (ADR-0010), so a Confluence source's project rooms are
 * *spaces* and a Jira source's are *projects*, and the only thing that can
 * separate them is which adapter the source runs. The census cannot carry the
 * answer — `Project` is part of the frozen IPC schema (contract §10.8) and a
 * field on it would need a ratified exception — and `health.all`, which is
 * where the switcher gets its source list, carries credentials and no adapter
 * kind.
 *
 * ## Why this is not live on `sync:state`
 *
 * A source's adapter kind is **immutable** (P10: `SourcePatch` cannot change
 * it, because the instance id is baked into every entity id). So the map moves
 * only when the *set* of sources does, and the two events that say so are the
 * ones the health store already answers to: bring-up, and a health reading
 * changing — which is what a newly added source's first check emits. A sync
 * run ending cannot add a source, so re-listing on it would be one
 * `list_sources` per run for an answer that never changed.
 *
 * ## The failure direction is the generic word
 *
 * A source this map has not heard of yet — read failed, `not_ready` during
 * bring-up, added seconds ago — resolves to `null`, and the caller says
 * *project*. A chip reading "project" over a space is a word that is merely
 * generic; a chip that guessed "space" over a Jira project would be wrong.
 */
import { listen as tauriListen } from "@tauri-apps/api/event";

import { EVENTS } from "../ipc";
import { listSources, type CredentialHealth, type SourceSummary } from "../ipc/sources";

/** The IPC this store needs, injectable so a test needs no Tauri bridge. */
export interface SourceKindsPorts {
  listSources: () => Promise<Pick<SourceSummary, "id" | "adapter_kind">[]>;
  listen: (
    event: string,
    handler: (event: { payload: CredentialHealth }) => void,
  ) => Promise<() => void>;
}

export interface SourceKinds {
  /** Which adapter this source runs, or `null` where nothing has said. */
  of(sourceId: string): string | null;
  /**
   * Read the map. The shell's to call once the database is ready, and any
   * caller's after it has just changed the set of sources.
   */
  reseed(): Promise<void>;
  /**
   * Subscribe to `source:health`. Returns the teardown; a second `start`
   * while live is a no-op whose teardown unwinds nothing.
   */
  start(): () => void;
}

export function createSourceKinds(ports?: SourceKindsPorts): SourceKinds {
  const io: SourceKindsPorts = ports ?? {
    listSources,
    listen: (event, handler) => tauriListen<CredentialHealth>(event, handler),
  };

  const state = $state<{ byId: Record<string, string> }>({ byId: {} });
  let live = false;

  return {
    of(sourceId: string) {
      return state.byId[sourceId] ?? null;
    },
    async reseed() {
      try {
        const rows = await io.listSources();
        if (!live) return;
        state.byId = Object.fromEntries(rows.map((row) => [row.id, row.adapter_kind]));
      } catch {
        // `not_ready` during bring-up, or a read that failed: the map keeps
        // what it has, and a source it has never heard of gets the generic
        // word. The subscription and the shell's ready-seed are the retries.
      }
    },
    start() {
      if (live) return () => {};
      live = true;
      let off: (() => void) | undefined;

      void io
        .listen(EVENTS.sourceHealth, () => {
          if (!live) return;
          // A health reading changed. Which sources exist may have changed
          // with it — that event is what a source added mid-session emits
          // once its credential is first checked.
          void this.reseed();
        })
        .then((unlisten) => {
          if (live) off = unlisten;
          else unlisten();
        })
        .catch(() => {
          // A failed subscription is not a failed window: whatever was seeded
          // at bring-up still stands, it simply stops moving.
        });

      return () => {
        live = false;
        off?.();
        off = undefined;
      };
    },
  };
}

/** The one the window uses. Tests build their own with {@link createSourceKinds}. */
export const sourceKinds = createSourceKinds();

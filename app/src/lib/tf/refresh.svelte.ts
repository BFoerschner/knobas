/**
 * An item's detail's *Refresh* (#2107, ADR-0031): the one item read from its
 * source (`refresh_entity`), then the detail read again over what it drew, so
 * it settles on what came back and never blanks.
 *
 * Which button a detail draws is its read's answer, not a second call: an
 * item whose adapter reads one of its kind (`EntityDetail.refreshes`, #2146)
 * gets *Refresh*, one that does not gets *Sync now*, which runs the item's source through
 * {@link sourceSyncs}, the run every detail of that source shares. A turned-off
 * source gets neither; its banner's remedy is re-enabling it. A tombstoned
 * item keeps *Refresh*: a read that finds it brings it back.
 *
 * An instance that answers *not offered* all the same (its config turned the
 * read off) gets the run in its place, and its detail says *Sync now* from then.
 */
import type { EntityDetail } from "../ipc/entity";
import { ipcErrorMessage } from "../ipc";
import { refreshEntity, type EntityRefresh } from "../ipc/sources";
import { sourceSyncs, type RoleSync, type RoleSyncs } from "./role-sync.svelte";
import { push } from "../shell/toasts.svelte";

/**
 * What a refresh reads of the item it is drawn for: an {@link EntityDetail},
 * or a surface's own row that says the same (a monitor's roster row, #2138).
 */
export type RefreshSubject = Pick<EntityDetail, "refreshes"> & {
  row: Pick<EntityDetail["row"], "entity_id">;
  source: Pick<EntityDetail["source"], "id" | "display_name" | "enabled">;
};

/** What a detail offers: *Refresh*, *Sync now*, or nothing. */
export type RefreshOffer = "refresh" | "sync" | null;

/** The offer `detail`'s read makes. */
function refreshOffer(detail: RefreshSubject): RefreshOffer {
  if (!detail.source.enabled) return null;
  return detail.refreshes ? "refresh" : "sync";
}

export interface DetailRefreshPorts {
  refreshEntity: (entityId: string) => Promise<EntityRefresh>;
  /** The runs a detail's *Sync now* starts or joins, by source id. */
  syncs: RoleSyncs;
}

export const DETAIL_REFRESH_BRIDGE: DetailRefreshPorts = {
  refreshEntity: (entityId) => refreshEntity(entityId),
  syncs: sourceSyncs,
};

/** One detail's *Refresh*. */
export interface DetailRefresh {
  /** While the read is out: the button says *Refreshing…*. */
  readonly reading: boolean;
  /** The last refresh's failure line, `Couldn't refresh: <reason>`, or `null`. */
  readonly failed: string | null;
  /** What `detail` offers, *Sync now* in place of *Refresh* once the source said it does not read one item. */
  offer(detail: RefreshSubject): RefreshOffer;
  /** The run `detail`'s *Sync now* starts or joins. */
  syncOf(detail: RefreshSubject): RoleSync;
  /**
   * Refresh `detail`'s item, then `reread` the detail. A failure keeps the
   * detail as it was and says why.
   */
  refresh(detail: RefreshSubject, reread: () => Promise<void> | void): Promise<void>;
  /** Back to nothing asked, for another item. */
  reset(): void;
}

export function createDetailRefresh(ports: DetailRefreshPorts = DETAIL_REFRESH_BRIDGE): DetailRefresh {
  const state = $state<{ reading: boolean; failed: string | null; unoffered: Record<string, true> }>({
    reading: false,
    failed: null,
    unoffered: {},
  });
  /** The newest ask: an answer for an item the reader left is dropped. */
  let asked = 0;

  return {
    get reading() {
      return state.reading;
    },
    get failed() {
      return state.failed;
    },
    offer(detail) {
      const offer = refreshOffer(detail);
      return offer === "refresh" && state.unoffered[detail.source.id] ? "sync" : offer;
    },
    syncOf(detail) {
      return ports.syncs.of(detail.source.id);
    },
    async refresh(detail, reread) {
      if (state.reading) return;
      const mine = ++asked;
      state.reading = true;
      state.failed = null;
      let answer: EntityRefresh;
      try {
        answer = await ports.refreshEntity(detail.row.entity_id);
      } catch (error) {
        if (mine === asked) {
          state.reading = false;
          state.failed = `Couldn't refresh: ${ipcErrorMessage(error)}`;
        }
        return;
      }
      if (mine !== asked) return;
      if (answer.outcome === "failed") {
        state.reading = false;
        state.failed = `Couldn't refresh: ${answer.reason}`;
        return;
      }
      if (answer.outcome === "not_offered") {
        state.unoffered[detail.source.id] = true;
        state.reading = false;
        const run = ports.syncs.of(detail.source.id);
        await run.start();
        if (run.failed !== null) push({ text: `Could not sync: ${run.failed}`, tone: "err" });
        return;
      }
      // Refreshed, gone, or turned off since the detail was read: the detail
      // read again says which, the old one drawn until it answers.
      try {
        await reread();
      } finally {
        if (mine === asked) state.reading = false;
      }
    },
    reset() {
      asked += 1;
      state.reading = false;
      state.failed = null;
    },
  };
}

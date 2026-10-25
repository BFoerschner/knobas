/**
 * The run and pipeline pages' *Refresh* in a test (#2137): the item's
 * `get_entity` read, and the refresh's ports with what they were asked.
 */
import type { EntityDetail, EntityRow } from "../../ipc/entity";
import type { EntityRefresh, SourceSyncStatus } from "../../ipc/sources";
import type { DetailRefreshPorts } from "../refresh.svelte";
import { createSourceSyncs } from "../role-sync.svelte";

/** `get_entity` of `entityId`: a TeamCity item that reads one item, unless `extra` says otherwise. */
export function detailOf(entityId: string, extra: Partial<EntityDetail> = {}): EntityDetail {
  return {
    row: { entity_id: entityId, source_id: "teamcity", kind: "build", title: entityId } as EntityRow,
    source: { id: "teamcity", display_name: "Tidewater CI", adapter_kind: "teamcity", enabled: true },
    refreshes: true,
    deleted_at: null,
    links: [],
    ...extra,
  } as unknown as EntityDetail;
}

export interface RefreshProbe {
  ports: DetailRefreshPorts;
  /** The ids `refresh_entity` was asked for. */
  refreshed: string[];
  /** The sources `sync_now` was asked to run. */
  synced: string[];
  /** What `refresh_entity` answers next. */
  answer: EntityRefresh;
  /** Send `sync:state` to the *Sync now* runs. */
  status: (row: SourceSyncStatus) => void;
}

export function refreshProbe(): RefreshProbe {
  const handlers = new Set<(event: { payload: unknown }) => void>();
  const probe: RefreshProbe = {
    refreshed: [],
    synced: [],
    answer: { outcome: "refreshed" },
    status: (row) => {
      for (const handler of [...handlers]) handler({ payload: row });
    },
    ports: {
      refreshEntity: (entityId) => (probe.refreshed.push(entityId), Promise.resolve(probe.answer)),
      syncs: createSourceSyncs({
        syncRole: () => Promise.reject(new Error("a detail never syncs a role")),
        syncNow: (sourceId) => (probe.synced.push(sourceId), Promise.resolve(77)),
        syncStatus: () => Promise.resolve([]),
        listen: (_event, handler) => {
          handlers.add(handler);
          return Promise.resolve(() => void handlers.delete(handler));
        },
      }),
    },
  };
  return probe;
}

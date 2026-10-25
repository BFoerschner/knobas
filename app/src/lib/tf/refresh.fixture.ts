/**
 * A detail's *Refresh* and *Sync now* in a test (#2144, over #2107's
 * `refresh.svelte.ts`): the ports with what they were asked, and the
 * `sync:state` a test sends to end a *Sync now* run; and `detailOf`, the
 * `get_entity` read of an item of a kind (a build unless said otherwise) the
 * run and pipeline pages draw them from.
 * The detail pages, the monitor roster and the residue harness build their
 * refresh from this one fake.
 */
import type { EntityDetail, EntityRow } from "../ipc/entity";
import type { EntityRefresh, SourceSyncStatus } from "../ipc/sources";
import type { DetailRefreshPorts } from "./refresh.svelte";
import { createSourceSyncs } from "./role-sync.svelte";

/** The run id `sync_now` answers unless a test says otherwise, and the one {@link RefreshProbe.end} ends. */
const RUN = 77;

/**
 * `get_entity` of `entityId`: a TeamCity item of `kind` that reads one item,
 * unless `extra` says otherwise. A test whose ids are of several kinds (a
 * pipeline, its runs, the ticket *This job* resolves) passes each its own.
 */
export function detailOf(entityId: string, extra: Partial<EntityDetail> = {}, kind = "build"): EntityDetail {
  return {
    row: { entity_id: entityId, source_id: "teamcity", kind, title: entityId } as EntityRow,
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
  /** What `refresh_entity` answers next; a test may swap it. */
  answer: () => Promise<EntityRefresh>;
  /** What `sync_now` answers next, the run's id; a test may swap it. */
  run: () => Promise<number>;
  /** Send `sync:state` saying `sourceId`'s run {@link RUN} ended well. */
  end: (sourceId: string) => void;
}

export interface RefreshProbeOptions {
  /** What `refresh_entity` answers; *refreshed* by default. */
  answer?: () => Promise<EntityRefresh>;
  /**
   * How the ports' default answers (refresh, run, status, unsubscribe) are
   * delivered, at once by default. The residue harness passes its `deferred`
   * so each lands after the page is gone. A role sync is never asked for.
   */
  hold?: <T>(value: T) => Promise<T>;
}

export function refreshProbe(options: RefreshProbeOptions = {}): RefreshProbe {
  const hold = options.hold ?? (<T>(value: T) => Promise.resolve(value));
  const handlers = new Set<(event: { payload: unknown }) => void>();
  const probe: RefreshProbe = {
    refreshed: [],
    synced: [],
    answer: options.answer ?? (() => hold<EntityRefresh>({ outcome: "refreshed" })),
    run: () => hold(RUN),
    end: (sourceId) => {
      const row: SourceSyncStatus = {
        source_id: sourceId,
        running: false,
        run_id: RUN,
        started_at: null,
        last_finished_at: null,
        last_outcome: "ok",
        next_run_at: null,
        backoff_until: null,
        last_changed: null,
      };
      for (const handler of [...handlers]) handler({ payload: row });
    },
    ports: {
      refreshEntity: (entityId) => (probe.refreshed.push(entityId), probe.answer()),
      syncs: createSourceSyncs({
        syncRole: () => Promise.reject(new Error("a detail never syncs a role")),
        syncNow: (sourceId) => (probe.synced.push(sourceId), probe.run()),
        syncStatus: () => hold<SourceSyncStatus[]>([]),
        listen: (_event, handler) => {
          handlers.add(handler);
          return hold(() => void handlers.delete(handler));
        },
      }),
    },
  };
  return probe;
}

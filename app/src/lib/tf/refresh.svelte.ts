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
 *
 * A press that read the item ends in one toast, replaced by the next press's (F-359): *Refreshed from TeamCity*, or
 * what the page knows better, which it says with {@link DetailRefresh.answer} while it reads again. While the source's
 * last sync failed, the toast says that too, since the band above still does (F-362): *Refreshed from TeamCity. Its
 * sign-in is still failing.*
 *
 * Named (ux25, ADR-0047): a page that names what it reads (`refresh(…, name)`, the header's button always does) says so in
 * that toast while the read is out, *Refreshing payout-service…*, and the same toast ends it in place, *Refreshed
 * payout-service from Tidewater Git*. The button keeps its width (`TfDetailRefresh`), so nothing on the page moves; a
 * failure drops the toast and keeps its one line under the header (`TfDetailBanners`).
 *
 * A press while the source's run holds it (Fable's ruling 4 of the ux25 pass) writes nothing to the mirror, as before,
 * but reads the page again at once, since its views are the source's own answers, and presses once more when that run
 * ends, as long as the page still shows the item: the run may have walked it before what the reader is looking for.
 */
import type { EntityDetail } from "../ipc/entity";
import { ipcErrorMessage } from "../ipc";
import { refreshEntity, syncStatus, type EntityRefresh, type SourceSyncStatus } from "../ipc/sources";
import { sourceSyncs, type RoleSync, type RoleSyncs } from "./role-sync.svelte";
import { dismiss, push } from "../shell/toasts.svelte";
import { EVENTS } from "../ipc";
import { listen as tauriListen } from "@tauri-apps/api/event";

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
  /** How each source's last run ended, for what a *Refresh* adds to its toast (F-362). */
  syncStatus: () => Promise<SourceSyncStatus[]>;
  /** Settles when a run of `sourceId` next ends (`sync:state`); never, where nothing can tell. */
  runEnds?: (sourceId: string) => Promise<void>;
}

/** The next end of a run of `sourceId`, as `sync:state` tells it. */
function nextRunEnd(sourceId: string): Promise<void> {
  return new Promise((resolve) => {
    let stop: (() => void) | null = null;
    let ended = false;
    void tauriListen(EVENTS.syncState, ({ payload }) => {
      const row = payload as SourceSyncStatus;
      if (ended || row.running || row.source_id !== sourceId) return;
      ended = true;
      stop?.();
      resolve();
    }).then(
      (unlisten) => {
        if (ended) unlisten();
        else stop = unlisten;
      },
      () => {},
    );
  });
}

export const DETAIL_REFRESH_BRIDGE: DetailRefreshPorts = {
  refreshEntity: (entityId) => refreshEntity(entityId),
  syncs: sourceSyncs,
  syncStatus: () => syncStatus(),
  runEnds: nextRunEnd,
};

/** How long the toast that says a read is out stands, at most: the read's end replaces it long before. */
const READING_MS = 60_000;

/** What a refreshed item's toast adds while its source's last run failed. Nothing here says the refresh read nothing: it did. */
const STILL = {
  unauthorized: "Its sign-in is still failing.",
  unreachable: "Its last sync couldn't reach it.",
  error: "Its last sync failed.",
} as const;

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
   * detail as it was and says why. `name` is what the press reads, said in its
   * toast while it reads and when it ends; without one, only the end is said.
   */
  refresh(detail: RefreshSubject, reread: () => Promise<void> | void, name?: string): Promise<void>;
  /** While `reread` runs: the words this press ends in, in place of *Refreshed from <source>*. `null` claims nothing. */
  answer(words: string | null): void;
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
  /** What the page had this ask say: `null` is nothing, and `undefined` is that it said nothing about it. */
  let words: string | null | undefined;

  /** One press: `retry` is whether a run in flight earns it one more press once that run ends (Fable's ruling 4). */
  async function press(detail: RefreshSubject, reread: () => Promise<void> | void, name: string | undefined, retry: boolean): Promise<void> {
    if (state.reading) return;
    const mine = ++asked;
    state.reading = true;
    state.failed = null;
    words = undefined;
    const subject = `refresh:${detail.row.entity_id}`;
    // What the press reads, said while it reads: the toast its end replaces in place.
    let reading: number | null = name === undefined ? null : push({ text: `Refreshing ${name}…`, subject, ms: READING_MS });
    const failed = (why: string) => {
      if (mine !== asked) return;
      state.reading = false;
      state.failed = `Couldn't refresh: ${why}`;
      // The line under the header says it; the toast that said the read was out goes.
      if (reading !== null) dismiss(reading);
      reading = null;
    };
    let answer: EntityRefresh;
    try {
      answer = await ports.refreshEntity(detail.row.entity_id);
    } catch (error) {
      failed(ipcErrorMessage(error));
      return;
    }
    if (mine !== asked) return;
    if (answer.outcome === "failed") {
      failed(answer.reason);
      return;
    }
    // A run of the source holds its lock for the whole run (the query audit, 2026-11-12: 74 to 242 s), so the press
    // says the sync is running instead of waiting behind it, reads the page's own views again now, and presses once
    // more when that run ends.
    if (answer.outcome === "run_in_flight") {
      state.reading = false;
      push({ text: `${detail.source.display_name} is syncing now. This page updates when that sync ends.`, subject });
      void reread();
      const ends = ports.runEnds;
      if (retry && ends !== undefined) {
        void ends(detail.source.id).then(() => {
          if (mine === asked) void press(detail, reread, name, false);
        });
      }
      return;
    }
    if (answer.outcome === "not_offered") {
      if (reading !== null) dismiss(reading);
      state.unoffered[detail.source.id] = true;
      state.reading = false;
      const run = ports.syncs.of(detail.source.id);
      await run.start(detail.source.display_name);
      if (run.failed !== null) push({ text: `Could not sync ${detail.source.display_name}: ${run.failed}`, tone: "err" });
      return;
    }
    // Refreshed, gone, or turned off since the detail was read: the detail
    // read again says which, the old one drawn until it answers.
    try {
      await reread();
    } finally {
      if (mine === asked) state.reading = false;
    }
    // Gone or turned off, the detail's banner says which.
    if (mine !== asked || answer.outcome !== "refreshed" || words === null) {
      if (reading !== null && mine === asked) dismiss(reading);
      return;
    }
    if (words !== undefined) {
      push({ text: words, subject });
      return;
    }
    const rows = await ports.syncStatus().catch(() => []);
    if (mine !== asked) return;
    const outcome = rows.find((row) => row.source_id === detail.source.id)?.last_outcome ?? "ok";
    const refreshed = name === undefined ? `Refreshed from ${detail.source.display_name}` : `Refreshed ${name} from ${detail.source.display_name}`;
    push({ text: outcome === "ok" ? refreshed : `${refreshed}. ${STILL[outcome]}`, subject });
  }

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
    async refresh(detail, reread, name) {
      await press(detail, reread, name, true);
    },
    answer(said) {
      words = said;
    },
    reset() {
      asked += 1;
      state.reading = false;
      state.failed = null;
      words = undefined;
    },
  };
}

<!--
  Monitors (#864, R-11.3 to R-11.5): `#/monitors` and `#/monitors/unmonitored`,
  one page with two tabs as prototype-6's `renderMonitors(tab)`.

  - **Monitors**: the filter bar (text over monitor, asset and type; *This
    job / My jobs / All*; *Down / Slow / Up / Paused / Pending / Maintenance*; *Type*), then the
    roster sorted down, slow, paused, up. Each row: the monitor and its
    type, the state chip, the 24-hour bar, the response, the 30-day uptime,
    what it **Watches** (the asset's name and type, linking to the asset),
    *Ack* on a down monitor, which *Acked* or the toast's *Undo* takes back, *Pause* or *Resume* where the source offers
    it, and *Hide*. A hidden monitor leaves the roster for the fold at its
    end (*N hidden monitors*, closed until the reader opens it, or open by
    itself when it holds every match), where it is drawn dimmed with a
    neutral chip and *Unhide* in place of *Hide*. Every filter applies to the
    fold as to the roster. Its alerts reach no inbox, so it raises no desktop
    notification and offers no *Ack*; knobas-local, Uptime Kuma goes on
    checking it. While a pause or resume is on its way its echo (*queued*, then *sent*,
    `tf/queued.svelte.ts`, #2105) stands in the State cell after the state chip, because it says the state is changing,
    and the button stays in its place, turned off, until the roster reads the new state.
  - **Assets nothing watches**: containers, services, routes, databases and VMs
    no monitor watches, each with *Create monitor* (`TfCreateMonitor.svelte`).
    A row whose create is on its way draws its echo instead (#1959, #2105),
    *Monitor queued* and then *Monitor sent*:
    the row stays listed until the poll mirrors the monitor and attaches it,
    and Uptime Kuma takes a second monitor of the same name. The read names
    the create (`monitor_on_its_way`), so a reload draws it too. One held as
    unconfirmed (sent, never answered, so the monitor may be in Kuma) is on
    its way too, until the reader sends it again or discards it (#1966). A
    pending or unconfirmed one is followed: refused or held on drift, it gives
    *Create monitor* back; sent or withdrawn, the list is read again. The
    echo goes once the roster holds the monitor Kuma made or the read names
    no such create; a sent create the read still names is drawn sent. The
    refresh after one of the page's writes reads both lists again, and while
    anything is on its way so does the end of every sync, which is when a
    sent one's monitor arrives.

  A source of the roster's that can't sync draws the pipeline's band over the
  tabs (`staleLine`) with its fix, and the header then says nothing of when it
  synced: a check's own time is not a sync.

  **Refresh (#2138, #2107's pieces).** Monitors has no page of one monitor,
  so a monitor's *Refresh*, or *Sync now* where its source does not read one
  item (`MonitorRow.refreshes`), ends its row's actions, after *Ack* and
  *Pause* or *Resume*; its failure is a full-width line under that row. A
  refresh reads the roster again over what it drew, and the pause and resume
  echoes settle on that read as on any other. The roster holds only enabled
  sources' monitors, so no row is turned off; a tombstoned monitor is drawn
  *Paused* (#2100), so no row is withdrawn either: the two banners have
  nothing to say here.

  Over `monitor_roster`, `open_alerts` (only for each down row's `acked_at`, so
  an ack outlives the visit; the tab draws no alert of its own, #1296), `ack_alert`, `submit_write` (`PauseMonitor`,
  `ResumeMonitor`, `CreateMonitor`), `set_monitor_hidden`, `unmonitored_assets`, `get_asset` and
  `edit_asset` for the create, `list_sources` for the header's source name,
  `refresh_entity` and the source's run for a row's *Refresh* or *Sync now*,
  and `job_reach` (`job/open-jobs.ts`) for the scopes. Knobas reads monitors
  here; monitoring stays Uptime Kuma's.
-->
<script lang="ts" module>
  import type {
    AssetDetail,
    AssetEdit,
    AssetRow,
    MonitorRow,
    OpenAlert,
    RouteDetail,
    RouteEdit,
    RouteRow,
    UnmonitoredAsset,
  } from "../../ipc/assets";
  import type { QueuedWrite, SourceSummary, SourceSyncStatus, WriteOpPayload } from "../../ipc/sources";
  import type { WriteFollower } from "../../shell/follow-write";
  import { counted } from "../counted";
  import { PressHold } from "../press-hold.svelte";
  import type { OpenJobPorts } from "../job/open-jobs";
  import type { SourceFacts } from "../job/job";
  import type { DetailRefreshPorts } from "../refresh.svelte";

  /** The reads and writes this page makes, injectable so a test needs no bridge. */
  type SourceRow = SourceFacts & Partial<Pick<SourceSummary, "health" | "auth_kind">>;

  export interface MonitorPorts extends OpenJobPorts {
    /** The sources' names, and whether each can sign in. */
    listSources: () => Promise<SourceRow[]>;
    monitorRoster: () => Promise<MonitorRow[]>;
    unmonitoredAssets: () => Promise<UnmonitoredAsset[]>;
    openAlerts: () => Promise<OpenAlert[]>;
    ackAlert: (monitorId: string) => Promise<OpenAlert>;
    /** *Acked* pressed, or *Undo* on the ack's toast. */
    unackAlert: (monitorId: string) => Promise<OpenAlert>;
    setMonitorHidden: (monitorId: string, hidden: boolean) => Promise<boolean>;
    submitWrite: (payload: WriteOpPayload) => Promise<QueuedWrite>;
    getAsset: (assetId: string) => Promise<AssetDetail>;
    editAsset: (assetId: string, edits: AssetEdit[]) => Promise<AssetRow>;
    getRoute: (routeId: string) => Promise<RouteDetail>;
    editRoute: (routeId: string, edits: RouteEdit[]) => Promise<RouteRow>;
    /** The bridge's events, for the end of a sync while a create is on its way. */
    listen: (event: string, handler: (event: { payload: unknown }) => void) => Promise<() => void>;
    /** What follows a pending create in the queue. */
    follower: () => WriteFollower;
    /** A row's *Refresh* and *Sync now* (#2138); the bridge's when left out. */
    refresh?: DetailRefreshPorts;
    /** The roster changed what the nav's badge counts: a monitor hidden or back. */
    badgesChanged?: () => void;
    /** How each source's last sync ended, for the band over the page; the bridge's when left out. */
    syncStatus?: () => Promise<SourceSyncStatus[]>;
  }
</script>

<script lang="ts">
  import { heldByPointer, heldHeight, holdUnderPointer } from "../row-hold";
  import TfStateChip from "../TfStateChip.svelte";
  import { badges } from "../badges.svelte";
  import { listen as tauriListen } from "@tauri-apps/api/event";
  import { tick, untrack } from "svelte";
  import { MONITOR_AT } from "../route";
  import TfNothingHere from "../TfNothingHere.svelte";
  import type { RoleFrameProps } from "../page-entry";

  import { ipcErrorMessage, isIpcError } from "../../ipc";
  import {
    ackAlert,
    unackAlert,
    editAsset,
    editRoute,
    getAsset,
    getRoute,
    monitorRoster,
    openAlerts,
    setMonitorHidden,
    unmonitoredAssets,
  } from "../../ipc/assets";
  import { getEntity, jobReach, listContexts, submitWrite } from "../../ipc/entity";
  import { listSources, syncStatus } from "../../ipc/sources";
  import { mayHaveLanded, writeFollower } from "../../shell/follow-write";
  import { echoOn, echoWrite, echoesOn, settleEchoes, watchEchoes, type Echo } from "../queued.svelte";
  import { leftTheInbox } from "../../inbox/inbox.svelte";
  import { toastQueued, type WriteSource } from "../../shell/queued-toast";
  import { ago } from "../../shell/time";
  import { dismiss, push } from "../../shell/toasts.svelte";
  import { Windowed } from "../../shell/windowed.svelte";
  import FilterBar from "../filter/FilterBar.svelte";
  import TfNoMatch from "../filter/TfNoMatch.svelte";
  import { sessionFilter, sessionFold } from "../filter/filter-state.svelte";
  import { MediaQuery } from "svelte/reactivity";
  import { barsOf } from "./monitors";
  import type { FilterConfig, FilterScope } from "../filter/filter";
  import Marked from "../filter/Marked.svelte";
  import { listenSynced } from "../builds/pipeline-ports";
  import { staleLine, syncWith } from "../builds/pipeline";
  import { jobIndex, readOpenJobs, type OpenJob } from "../job/open-jobs";
  import {
    stateChip,
    gaps,
    hiddenFoldOpen,
    isRoute,
    isRouteId,
    kindWord,
    prefill,
    routePrefill,
    type MonitorSubject,
    responseText,
    segments,
    sorted,
    STATE_CHIPS,
    uptimeText,
    visibleMonitors,
  } from "./monitors";
  import TfCreateMonitor from "./TfCreateMonitor.svelte";
  import TfDetailRefresh from "../TfDetailRefresh.svelte";
  import { createDetailRefresh, type DetailRefresh, type RefreshSubject } from "../refresh.svelte";

  let {
    ready,
    rest,
    job,
    ports = {
      monitorRoster: () => monitorRoster(),
      unmonitoredAssets: () => unmonitoredAssets(),
      openAlerts: () => openAlerts(),
      ackAlert: (monitorId) => ackAlert(monitorId),
      unackAlert: (monitorId) => unackAlert(monitorId),
      setMonitorHidden: (monitorId, hidden) => setMonitorHidden(monitorId, hidden),
      submitWrite: (payload) => submitWrite(payload),
      getAsset: (assetId) => getAsset(assetId),
      editAsset: (assetId, edits) => editAsset(assetId, edits),
      getRoute: (routeId) => getRoute(routeId),
      editRoute: (routeId, edits) => editRoute(routeId, edits),
      listSources: () => listSources(),
      listContexts: () => listContexts(),
      getEntity: (entityId) => getEntity(entityId),
      jobReach: (entityId) => jobReach(entityId),
      listen: (event, handler) => tauriListen(event, handler),
      follower: () => writeFollower(),
      badgesChanged: () => void badges.refresh(),
    },
    now: fixedNow,
    sync,
    syncRevision = 0,
  }: {
    /** Whether the database answers yet: the reads wait for it. */
    ready: boolean;
    /** What follows `monitors` in the address: `unmonitored`, or nothing. */
    rest: string[];
    /** The chosen job (spec #2227), as a ticket key. */
    job: string | null;
    ports?: MonitorPorts;
    /** Injectable clock, for the bars and the header's *synced*. */
    now?: Date;
  } & RoleFrameProps = $props();

  const now = $derived(fixedNow ?? new Date());
  const tab = $derived<"all" | "unmonitored">(rest[0] === "unmonitored" ? "unmonitored" : "all");
  /** The monitor the address opens the roster at, marked: an inbox alert's, a set member's. */
  const markedId = $derived(rest[0] === MONITOR_AT ? (rest[1] ?? null) : null);

  /**
   * The roster as read: the read narrows itself to the sources filling the
   * monitoring role, so nothing here keeps rows by source (#1297, ADR-0029
   * as amended on 2026-10-19).
   */
  let roster = $state<MonitorRow[] | null>(null);
  let unwatched = $state<UnmonitoredAsset[] | null>(null);
  let sourceNames = $state<Record<string, string>>({});
  /** The sources as last read, `null` until then: whether one can sign in decides the band and the header's *synced*. */
  let sources = $state<SourceRow[] | null>(null);
  let syncRows = $state<SourceSyncStatus[]>([]);
  let failed = $state<string | null>(null);
  let jobs = $state<OpenJob[]>([]);
  /** When each monitor's open alert was acked, by monitor id: the open alerts' `acked_at`, so it outlives the visit (#1461). */
  let acked = $state<Record<string, string>>({});
  /** Each ack's toast while it stands: taking the ack back takes its *Undo* with it. */
  const ackToasts = new Map<string, number>();
  /** Rows whose write has not answered: a double click is one press. */
  let pressing = $state<Record<string, true>>({});
  let creating = $state<MonitorSubject | null>(null);
  let opening = $state<string | null>(null);

  /** Bumped to read both lists again. */
  let revision = $state(0);
  // The frame moves this when *Sync now*'s runs end and each time the page is shown again, so a token entered in Settings takes the band away (#2106).
  let syncedAt = untrack(() => syncRevision);
  $effect(() => {
    if (syncRevision === syncedAt) return;
    syncedAt = syncRevision;
    untrack(() => (revision += 1));
  });
  /** The refreshes waiting on the roster read that answers next (#2138). */
  let rereads: (() => void)[] = [];

  $effect(() => {
    if (!ready) return;
    void revision;
    let live = true;
    failed = null;
    const fail = (error: unknown) => {
      if (live) failed = ipcErrorMessage(error);
    };
    ports
      .monitorRoster()
      .then((rows) => {
        if (!live) return;
        keepRefreshers(rows);
        roster = rows;
        settleRoster(rows);
      }, fail)
      .finally(() => {
        // Only a read no newer one replaced settles the refreshes, all of them:
        // each started before it (a `reread` starts a read), so it saw theirs.
        if (!live) return;
        const waiting = rereads;
        rereads = [];
        for (const done of waiting) done();
      });
    ports.openAlerts().then(
      (alerts) => {
        if (!live) return;
        const read: Record<string, string> = {};
        for (const alert of alerts) if (alert.acked_at !== null) read[alert.monitor_id] = alert.acked_at;
        acked = read;
      },
      () => {},
    );
    ports.unmonitoredAssets().then((rows) => {
      if (!live) return;
      unwatched = gaps(rows);
      settleUnwatched(rows);
    }, fail);
    ports.listSources().then(
      (rows) => {
        if (!live) return;
        sourceNames = Object.fromEntries(rows.map((row) => [row.id, row.display_name]));
        sources = rows;
      },
      () => {
        if (live) sources ??= [];
      },
    );
    (ports.syncStatus ?? syncStatus)().then(
      (rows) => {
        if (live) syncRows = rows;
      },
      () => {},
    );
    return () => {
      live = false;
    };
  });

  $effect(() => {
    if (!ready) return;
    const current = job;
    let live = true;
    void readOpenJobs(ports, current).then((open) => {
      if (live) jobs = open;
    });
    return () => {
      live = false;
    };
  });

  // -- the page's echoes (#2105) ----------------------------------------------

  /** The surface Monitors' echoes are tagged with, for `watchEchoes`. */
  const MONITORS = "monitors";
  /** The kind of a create's echo on its asset, its text the monitor's name; and of a pause's or resume's on its monitor. */
  const CREATE = "monitor";
  const TOGGLE = "state";

  /** Whether a roster row reads paused: gone from `/metrics`, or (with #2100's refresh) said so. */
  const isPaused = (row: MonitorRow) => row.tombstoned || row.state === "paused";

  /** The roster's check: a create goes once it holds the monitor Kuma made; a pause or a resume once the row reads the new state. */
  function settleRoster(rows: readonly MonitorRow[]) {
    const names = new Set(rows.map((row) => row.name));
    for (const row of unwatched ?? []) settleEchoes(row.id, (echo) => echo.kind === CREATE && names.has(echo.text));
    for (const row of rows) settleEchoes(row.entity_id, (echo) => echo.kind === TOGGLE && (echo.text === "pause") === isPaused(row));
  }

  /**
   * The unwatched list's check: a create goes once its asset is watched (no
   * longer listed), or once the read names no such create on it (withdrawn
   * in the queue, which leaves the open rows as a sent one does).
   */
  function settleUnwatched(rows: readonly UnmonitoredAsset[]) {
    const listed = new Map(rows.map((row) => [row.id, row]));
    for (const id of untrack(() => echoedAssets)) {
      const row = listed.get(id);
      settleEchoes(id, (echo) => echo.kind === CREATE && (row === undefined || row.monitor_on_its_way?.id !== echo.write));
    }
  }

  /** The assets this session echoed a create on. */
  let echoedAssets = $state<string[]>([]);

  /** A create on its way, echoed on its asset and followed: refused or held on drift, the row offers the create again; sent, the list is read again. */
  function echoCreate(asset: string, name: string, write: QueuedWrite) {
    if (echoesOn(asset, CREATE).some((echo) => echo.write === write.id)) return;
    if (!echoedAssets.includes(asset)) echoedAssets = [...echoedAssets, asset];
    echoWrite(asset, CREATE, name, {
      write,
      following: { name: `Monitor ${name}` },
      follower: ports.follower(),
      surface: MONITORS,
      source: write.source_id,
      onstop: () => {
        markOnItsWay((going) => going?.id === write.id, null);
        revision += 1;
      },
      onsent: () => (revision += 1),
    });
  }

  // A create the read names as on its way and the queue may still hold
  // (pending, or held as unconfirmed, #1966) is echoed and followed.
  $effect(() => {
    for (const row of unwatched ?? []) {
      const write = row.monitor_on_its_way;
      if (write === null || !(write.state === "pending" || mayHaveLanded(write))) continue;
      const payload = write.payload as { CreateMonitor?: { name?: string } } | undefined;
      untrack(() => echoCreate(row.id, payload?.CreateMonitor?.name ?? row.name, write));
    }
  });

  /** What a row's create draws: its echo's word, else the read's (a sent create still counts as on its way); `null` for none. */
  function createWord(row: UnmonitoredAsset): Echo["state"] | null {
    const echo = echoOn(row.id, CREATE);
    if (echo !== null) return echo.state;
    const write = row.monitor_on_its_way;
    return write === null ? null : write.state === "sent" ? "sent" : "queued";
  }

  /** Set the create on its way of every row `which` picks, as the page knows it before the next read. */
  function markOnItsWay(which: (write: QueuedWrite | null, row: UnmonitoredAsset) => boolean, write: QueuedWrite | null) {
    if (unwatched === null) return;
    unwatched = unwatched.map((row) => (which(row.monitor_on_its_way, row) ? { ...row, monitor_on_its_way: write } : row));
  }

  // The refresh after one of the page's writes reads both lists again; while
  // a create or a pause is on its way, so does the end of every sync, which
  // is when a sent one's monitor arrives or Kuma's /metrics moves.
  $effect(() => untrack(() => watchEchoes(MONITORS, () => (revision += 1), { listen: ports.listen, runEnds: false })));
  const waiting = $derived(
    (unwatched ?? []).some((row) => createWord(row) !== null) || (roster ?? []).some((row) => echoOn(row.entity_id, TOGGLE) !== null),
  );
  $effect(() => {
    if (!waiting) return;
    return listenSynced(ports, null, (row) => (syncRows = syncWith(syncRows, row)), () => (revision += 1));
  });

  /** What *Monitor queued* says on hover: where the create is. */
  function queuedTitle(write: QueuedWrite): string {
    const source = sourceNames[write.source_id] ?? "the monitoring source";
    if (mayHaveLanded(write)) {
      return `Sent to ${source}, which did not answer, so it may be there. Check ${source}, then send it again or discard it in the write queue.`;
    }
    return write.state === "sent"
      ? `Created in ${source}. The next poll attaches it here.`
      : `Queued for ${source}. Open the write queue to see it.`;
  }

  // -- the filter bar --------------------------------------------------------

  const filter = sessionFilter("monitors");
  /** The hidden fold as the reader last left it; `null` until touched, so it may open by itself on a search only it answers. */
  const hiddenFold = sessionFold("monitors:hidden");
  // Prototype-6 opens Monitors on *All*.
  if (filter.scope === null) filter.scope = "all";
  $effect(() => {
    if (job === null && filter.scope === "job") filter.scope = "all";
  });

  const index = $derived(jobIndex(jobs));
  const types = $derived([...new Set((roster ?? []).map((row) => kindWord(row.monitor_type)).filter(Boolean))].sort());
  const config = $derived<FilterConfig>({
    id: "monitors",
    placeholder: "Filter monitors",
    noun: "monitors",
    one: "monitor",
    scopes: [
      ...(job !== null ? [{ id: "job", label: "This job", count: roster === null ? null : visibleMonitors(roster, { scope: "job", job, states: [], type: "", text: "" }, index).shown.length, title: `The monitors ${job} reaches`, query: "" } satisfies FilterScope] : []),
      { id: "mine", label: "Mine", title: "The monitors the jobs you work on reach", query: "" },
      { id: "all", label: "All", query: "" },
    ],
    chips: [{ key: "st", label: "State", options: STATE_CHIPS.map((value) => ({ value, label: value })) }],
    dropdowns: [{ key: "type", label: "Type", options: types.map((value) => ({ value, label: value })) }],
  });
  const scope = $derived<"job" | "mine" | "all">(
    filter.scope === "job" && job !== null ? "job" : filter.scope === "mine" ? "mine" : "all",
  );
  const kept = $derived(
    roster === null
      ? { shown: [], hidden: [] }
      : visibleMonitors(
          sorted(roster),
          {
            scope,
            job,
            states: filter.chips["st"] ?? [],
            type: filter.dropdowns["type"] ?? "",
            text: filter.text,
          },
          index,
        ),
  );
  /** The marked monitor, whatever the filter and the fold say: the reader came for it. */
  const markedRow = $derived(markedId === null ? null : (roster?.find((row) => row.entity_id === markedId) ?? null));
  const rows = $derived(markedRow !== null && !kept.shown.includes(markedRow) ? [markedRow, ...kept.shown] : kept.shown);
  const hiddenRows = $derived(markedRow === null ? kept.hidden : kept.hidden.filter((row) => row !== markedRow));
  const foldOpen = $derived(hiddenFoldOpen(hiddenFold.open, kept));
  /** The roster's own size, for the count: a hidden monitor is counted apart, unless it is marked and so a row of the list. */
  const unhiddenTotal = $derived(roster === null ? null : roster.filter((row) => !row.hidden || row === markedRow).length);

  /** Only the reader's toggle is remembered, not the one the open attribute raises as the fold is drawn. */
  function toggleFold(event: Event) {
    const open = (event.currentTarget as HTMLDetailsElement).open;
    if (open !== foldOpen) hiddenFold.open = open;
  }

  /** The header's line: how many checks, from which source, synced when. */
  /** Assets nothing watches: their own band, over their name, type and owner. */
  const unwatchedFilter = sessionFilter("monitors-unwatched");
  const unwatchedConfig: FilterConfig = { id: "monitors-unwatched", placeholder: "Filter assets", noun: "assets", one: "asset" };
  const shownUnwatched = $derived.by(() => {
    const words = unwatchedFilter.text.trim().toLowerCase();
    if (unwatched === null || words === "") return unwatched ?? [];
    return unwatched.filter((asset) => [asset.name, asset.type_label, asset.owner ?? "", asset.environment ?? ""].join(" ").toLowerCase().includes(words));
  });

  /** Below 1240 px half-hour bars would be under a pixel wide: one bar an hour, the worse of its two half-hours. */
  const hourlyBars = new MediaQuery("(max-width: 1240px)");

  /** A source of the roster's that can't sync says so over the page, with its fix, as a pipeline's does. */
  const stale = $derived.by(() => {
    if (roster === null || sources === null) return [];
    const ids = [...new Set(roster.map((row) => row.source_id))];
    return ids.flatMap((id) => {
      const line = staleLine({ source_id: id, source_name: sourceNames[id] ?? id }, sources ?? [], syncRows, now, "Pauses and new monitors");
      return line === null ? [] : [{ id, line }];
    });
  });

  const header = $derived.by(() => {
    if (roster === null) return "Checks from your monitoring source, each watching an asset.";
    const names = [...new Set(roster.map((row) => sourceNames[row.source_id] ?? row.source_id))];
    const newest = roster.map((row) => row.checked_at).sort().at(-1) ?? null;
    const from = names.length > 0 ? ` from ${names.join(" and ")}` : "";
    // A check's own time is not a sync: where a source can't sync, the band says how old its data is.
    const synced = newest !== null && sources !== null && stale.length === 0 ? `, synced ${ago(newest, now)}` : "";
    return `${counted(roster.length, "check", "checks")}${from}${synced}. Each one watches an asset.`;
  });

  // -- a row's Refresh (#2138) -----------------------------------------------

  /** Each monitor's own *Refresh*, so one row's *Refreshing…* is not another's; made as the roster is read. */
  const refreshers = new Map<string, DetailRefresh>();
  function keepRefreshers(rows: readonly MonitorRow[]) {
    const ids = new Set(rows.map((row) => row.entity_id));
    for (const id of refreshers.keys()) if (!ids.has(id)) refreshers.delete(id);
    for (const id of ids) if (!refreshers.has(id)) refreshers.set(id, createDetailRefresh(untrack(() => ports.refresh)));
  }

  type Entry = { row: MonitorRow; failed: string | null; held?: never } | { row: null; failed: null; held?: boolean };
  const FOLD_KEY = "\u0000hidden-fold";
  const HELD_KEY = "\u0000held-row";
  /** The row a mouse press on *Hide* sent to the fold, still holding its place under the pointer (F-138). */
  let holding = $state.raw<{ id: string; row: MonitorRow; at: number; height: number } | null>(null);
  /** Where its empty row stands: once it has left the list, and until *Undo* brings it back. */
  const holdAt = $derived(holding !== null && !rows.some((row) => row.entity_id === holding?.id) ? holding.at : null);
  $effect(() => {
    if (holding === null) return;
    return holdUnderPointer(() => document.querySelector(".tf-mon-held"), () => (holding = null));
  });
  /** The listed rows, the held row's empty one among them. */
  const withHeld = (list: Entry[]): Entry[] => {
    if (holdAt === null) return list;
    const next = rows[holdAt];
    const at = next === undefined ? list.length : list.findIndex((entry) => entry.row === next);
    return [...list.slice(0, at), { row: null, failed: null, held: true }, ...list.slice(at)];
  };
  const withFailures = (list: readonly MonitorRow[]): Entry[] =>
    list.flatMap((row) => {
      const failed = refreshers.get(row.entity_id)?.failed ?? null;
      return failed === null ? [{ row, failed }] : [{ row, failed: null }, { row, failed }];
    });
  /**
   * The roster's table rows in order: each monitor, and under it the reason its
   * refresh failed; then, when the filter keeps any hidden monitor, the fold's
   * summary and, while it is open, those monitors the same way.
   */
  const entries = $derived<Entry[]>([
    ...withHeld(withFailures(rows)),
    ...(hiddenRows.length > 0 ? [{ row: null, failed: null }] : []),
    ...(hiddenRows.length > 0 && foldOpen ? withFailures(hiddenRows) : []),
  ]);
  /** Thousands of monitors draw only those near the viewport (#2210); a row is 42 px. */
  const win = new Windowed({ count: () => entries.length, key: (i) => entryKey(entries[i]!), estimate: 42 });

  /** A mark comes into view once, as the address names it; a later read of the roster leaves the reader where they are. */
  let broughtIn: string | null = null;
  $effect(() => {
    const id = markedId;
    if (id === null) {
      broughtIn = null;
      return;
    }
    if (broughtIn === id || markedRow === null) return;
    broughtIn = id;
    win.reveal(untrack(() => entries).findIndex((entry) => entry.row?.entity_id === id && entry.failed === null));
    void tick().then(() => document.querySelector(".tf-kept:not([hidden]) .tf-mon-at")?.scrollIntoView?.({ block: "center" }));
  });

  function entryKey(entry: Entry): string {
    if (entry.held) return HELD_KEY;
    if (entry.row === null) return FOLD_KEY;
    return entry.failed === null ? entry.row.entity_id : `${entry.row.entity_id}\u0000failed`;
  }

  /** What a row's *Refresh* reads: the roster holds only enabled sources' monitors. */
  function subjectOf(row: MonitorRow): RefreshSubject {
    return {
      refreshes: row.refreshes,
      row: { entity_id: row.entity_id },
      source: { id: row.source_id, display_name: sourceNames[row.source_id] ?? row.source_id, enabled: true },
    };
  }

  /** Read the roster again over what it drew; settles once that read answers. */
  function reread(): Promise<void> {
    return new Promise((resolve) => {
      rereads.push(resolve);
      revision += 1;
    });
  }

  // -- the row actions -------------------------------------------------------

  /** *Acked* holds about two seconds after it says so (F-351): a double press is one ack, and *Undo* is there at once. */
  const ackHold = new PressHold();

  async function ack(row: MonitorRow) {
    if (pressing[row.entity_id]) return;
    pressing[row.entity_id] = true;
    try {
      const alert = await ports.ackAlert(row.entity_id);
      acked[row.entity_id] = alert?.acked_at ?? new Date().toISOString();
      ackHold.hold(row.entity_id);
      ackToasts.set(row.entity_id, push({ text: "Acked. The alert stays open until the monitor recovers.", action: { label: "Undo", run: () => void unack(row, true) } }));
    } catch (error) {
      refusedAck(row, error);
    } finally {
      delete pressing[row.entity_id];
    }
  }

  /** *Acked* pressed, or *Undo* on the ack's toast (`undo`, which no hold stops): the alert asks again, in the inbox too. */
  async function unack(row: MonitorRow, undo = false) {
    if (pressing[row.entity_id] || (!undo && ackHold.held(row.entity_id))) return;
    pressing[row.entity_id] = true;
    try {
      await ports.unackAlert(row.entity_id);
      delete acked[row.entity_id];
      dropAckToast(row);
      push({ text: `${row.name} is asking again, in the Inbox too.` });
    } catch (error) {
      refusedAck(row, error);
    } finally {
      delete pressing[row.entity_id];
    }
  }

  function dropAckToast(row: MonitorRow) {
    const id = ackToasts.get(row.entity_id);
    if (id !== undefined) dismiss(id);
    ackToasts.delete(row.entity_id);
  }

  /** The alert closed while the row was drawn: not a failure, so it says what happened and the row is read again. */
  function refusedAck(row: MonitorRow, error: unknown) {
    if (isIpcError(error) && error.code === "not_found") {
      delete acked[row.entity_id];
      dropAckToast(row);
      push({ text: leftTheInbox(row.name) });
      revision += 1;
      return;
    }
    push({ text: ipcErrorMessage(error), tone: "err" });
  }

  /** *Hide* or *Unhide*: knobas-local, so the row moves at once and the roster is read again. */
  async function setHidden(row: MonitorRow, hidden: boolean, event?: MouseEvent) {
    if (pressing[row.entity_id]) return;
    pressing[row.entity_id] = true;
    const box = hidden && heldByPointer(event) ? (event?.currentTarget as Element).closest("tr")?.getBoundingClientRect() : undefined;
    if (box !== undefined) holding = { id: row.entity_id, row, at: rows.indexOf(row), height: box.height };
    try {
      await ports.setMonitorHidden(row.entity_id, hidden);
      if (roster !== null) roster = roster.map((each) => (each.entity_id === row.entity_id ? { ...each, hidden } : each));
      // A hidden monitor is not down as far as the nav says: there is nothing left to act on.
      ports.badgesChanged?.();
      push({
        text: hidden
          ? `Hid ${row.name}. Its alerts stay quiet.`
          : `${row.name} is back on the roster, and its alerts reach the inbox again.`,
        ...(hidden ? { action: { label: "Undo", run: () => void setHidden({ ...row, hidden: true }, false) } } : {}),
      });
      revision += 1;
    } catch (error) {
      push({ text: ipcErrorMessage(error), tone: "err" });
    } finally {
      delete pressing[row.entity_id];
    }
  }

  /** The pause and resume a row may offer, by the op its source declares. */
  const ROW_OPS: Record<string, { label: string; build: (entity: string) => WriteOpPayload }> = {
    pause_monitor: { label: "Pause", build: (entity) => ({ PauseMonitor: { entity } }) },
    resume_monitor: { label: "Resume", build: (entity) => ({ ResumeMonitor: { entity } }) },
  };

  async function toggle(row: MonitorRow, op: string) {
    const form = ROW_OPS[op];
    if (!form || pressing[row.entity_id] || echoOn(row.entity_id, TOGGLE) !== null) return;
    pressing[row.entity_id] = true;
    try {
      const write = await ports.submitWrite(form.build(row.entity_id));
      toastQueued(`${form.label.toLowerCase()} ${row.name}`, { id: row.source_id, display_name: sourceNames[row.source_id] ?? "" }, { write });
      echoWrite(row.entity_id, TOGGLE, form.label.toLowerCase(), {
        write,
        following: { name: `${form.label} ${row.name}` },
        follower: ports.follower(),
        surface: MONITORS,
        source: row.source_id,
      });
    } catch (error) {
      push({ text: ipcErrorMessage(error), tone: "err" });
    } finally {
      delete pressing[row.entity_id];
    }
  }

  async function openCreate(asset: UnmonitoredAsset) {
    if (opening !== null) return;
    opening = asset.id;
    try {
      if (isRoute(asset)) {
        const detail = await ports.getRoute(asset.id);
        creating = { id: asset.id, name: asset.name, targets: detail.monitor_targets, opening: routePrefill(asset) };
      } else {
        const detail = await ports.getAsset(asset.id);
        creating = { id: asset.id, name: detail.asset.name, targets: detail.monitor_targets, opening: prefill(detail) };
      }
    } catch (error) {
      push({ text: ipcErrorMessage(error), tone: "err" });
    } finally {
      opening = null;
    }
  }

  /** The name goes onto the asset or the route the dialog was opened for, so the next poll attaches it there. */
  function recordName(subject: MonitorSubject, name: string): Promise<unknown> {
    return isRouteId(subject.id)
      ? ports.editRoute(subject.id, [{ field: "monitors", added: [name] }])
      : ports.editAsset(subject.id, [{ field: "monitors", added: [name] }]);
  }

  function created(what: { name: string; write: QueuedWrite; source: WriteSource }) {
    const subject = creating;
    creating = null;
    toastQueued(`create monitor for ${subject?.name ?? what.name}`, what.source, { write: what.write });
    // Drawn at once, and followed by the effect above as the read will name it.
    // The re-read starts after the row was queued, and drops any read before it.
    markOnItsWay((_, row) => row.id === subject?.id, what.write);
    if (subject) echoCreate(subject.id, what.name, what.write);
    revision += 1;
  }

  /** A row opens its asset; a click on a link or button inside it already did its own thing. */
  function openAsset(event: MouseEvent, id: string) {
    if ((event.target as Element | null)?.closest("a, button")) return;
    location.hash = addressOf(id);
  }

  /** A route opens at `#/route/<id>` (the Tree at its exposing asset), an asset at `#/asset/<id>`. */
  function addressOf(id: string): string {
    return `#/${isRouteId(id) ? "route" : "asset"}/${encodeURIComponent(id)}`;
  }
</script>

<header class="tf-fhead">
  <div>
    <h1>Monitors</h1>
    <div class="tf-sub">{tab === "unmonitored" ? "Containers, services, routes, databases and VMs that no monitor watches." : header}</div>
  </div>
  {#if sync}<div class="tf-acts">{@render sync()}</div>{/if}
</header>
{#each stale as { id, line } (id)}
  <div class="tf-stale tf-bstale">
    {line.text}
    {#if line.fix !== null}<a href={line.fix.href}>{line.fix.words}</a>{/if}
  </div>
{/each}
<nav class="tf-tabs" aria-label="Monitors">
  <a href="#/monitors" aria-current={tab === "all" ? "page" : undefined}
    >Monitors{#if roster !== null && roster.length > 0}{" "}<span class="tf-count">{roster.length}</span>{/if}</a
  >
  <a href="#/monitors/unmonitored" aria-current={tab === "unmonitored" ? "page" : undefined}
    >Assets nothing watches{#if unwatched !== null && unwatched.length > 0}{" "}<span class="tf-count"
        >{unwatched.length}</span
      >{/if}</a
  >
</nav>
{#if tab === "all"}
  <FilterBar {config} {filter} shown={roster === null ? null : rows.length} total={unhiddenTotal}>
    {#snippet after()}<span class="tf-mon-hidn">{#if hiddenRows.length > 0}, {hiddenRows.length} hidden{/if}</span>{/snippet}
  </FilterBar>
{:else}
  <FilterBar config={unwatchedConfig} filter={unwatchedFilter} shown={unwatched === null ? null : shownUnwatched.length} total={unwatched?.length ?? null} />
{/if}
<div class="tf-cbody">
  {#if failed !== null}
    <p class="tf-ifail">Could not read the monitors: {failed}</p>
  {/if}
  {#if markedId !== null && roster !== null && markedRow === null}
    <TfNothingHere message="knobas does not hold this monitor. It may not have synced yet." href="#/monitors" action="All monitors" />
  {:else if tab === "unmonitored"}
    {#if unwatched !== null && unwatched.length > 0 && shownUnwatched.length === 0}
      <TfNoMatch noun="assets" filter={unwatchedFilter} />
    {:else if unwatched !== null && unwatched.length > 0}
      <table class="tf-tbl tf-mon-gaps">
        <thead><tr><th>Asset</th><th>Type</th><th>Env</th><th>Owner</th><th><span class="tf-sr">Actions</span></th></tr></thead>
        <tbody>
          {#each shownUnwatched as asset (asset.id)}
            {@const word = createWord(asset)}
            <tr class="tf-row" onclick={(event) => openAsset(event, asset.id)}>
              <td><a href={addressOf(asset.id)}>{asset.name}</a></td>
              <td>{asset.type_label}</td>
              <td>{asset.environment ?? ""}</td>
              <td><span class="tf-rowval">{asset.owner ?? ""}</span></td>
              <td class="tf-mon-acts">
                {#if word !== null}
                  <TfStateChip text="Monitor {word}" tone="info" title={asset.monitor_on_its_way === null ? undefined : queuedTitle(asset.monitor_on_its_way)} />
                {:else}
                  <span class="tf-rowend"><span class="tf-rowacts"><button type="button" class="tf-btn tf-sm" disabled={opening === asset.id} onclick={() => void openCreate(asset)}
                    >Create monitor…</button
                  ></span></span>
                {/if}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    {:else if unwatched !== null}
      <div class="tf-empty">Every container, service, route, database and VM has a monitor.</div>
    {/if}
  {:else if roster !== null}
    {#if entries.length > 0}
      <table class="tf-tbl tf-mon">
        <!-- Fixed columns: an action taken on one row, or a filter, moves nothing in the others. -->
        <colgroup><col class="tf-monc-name" /><col class="tf-monc-state" /><col class="tf-monc-watch" /><col class="tf-monc-bars" /><col class="tf-monc-num" /><col class="tf-monc-num" /><col class="tf-monc-acts" /></colgroup>
        <thead>
          <tr><th>Monitor</th><th>State</th><th>Watches</th><th>Last 24 h<span class="tf-mbar-axis" aria-hidden="true"><span>24 h ago</span><span>now</span></span></th><th class="tf-num">Response</th><th class="tf-num">30 days</th><th><span class="tf-sr">Actions</span></th></tr>
        </thead>
        <tbody {@attach win.list}>
          {#if win.before > 0}<tr class="tf-wgap" aria-hidden="true"><td colspan="7" {@attach win.room("before")}></td></tr>{/if}
          {#each win.slice(entries) as entry (entryKey(entry))}
            {@const row = entry.row}
            {#if row === null && entry.held}
              <!-- Its words stay, dimmed, for as long as the pointer rests on it (F-064). Nothing in them can be pressed. -->
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <tr class="tf-mon-held" aria-hidden="true" onmousedown={(event) => event.preventDefault()} {@attach win.row(HELD_KEY)} {@attach heldHeight(holding?.height ?? 0)}>
                {#if holding !== null}
                  {@const was = holding.row}
                  {@const chip = stateChip(was, now)}
                  <td>{was.name}{#if was.monitor_type}{" "}<span class="tf-mon-type">{kindWord(was.monitor_type)}</span>{/if}</td>
                  <td><div class="tf-mon-state"><TfStateChip text={chip.label} />{#if chip.since}<span class="tf-mon-since">{chip.since}</span>{/if}</div></td>
                  <td>{#each was.assets as asset (asset.id)}<div><span class="tf-mon-asset">{asset.name}</span>{" "}<span class="tf-mon-type">{asset.type_label}</span></div>{/each}</td>
                  <td><div class="tf-mon-24"><span class="tf-mon-figs">{responseText(was.response_time_ms)}, {uptimeText(was)}</span></div></td>
                  <td class="tf-num"><span>{responseText(was.response_time_ms)}</span></td>
                  <td class="tf-num"><span>{uptimeText(was)}</span></td>
                  <td></td>
                {:else}<td colspan="7"></td>{/if}
              </tr>
            {:else if row === null}
              <tr class="tf-mon-foldrow" {@attach win.row(FOLD_KEY)}>
                <td colspan="7">
                  <!-- The summary alone, as Builds' folds: the hidden rows follow it while it is open. -->
                  <details class="tf-mon-fold" open={foldOpen} ontoggle={toggleFold}>
                    <summary
                      ><span>{counted(hiddenRows.length, "hidden monitor", "hidden monitors")}</span>
                      <span class="tf-dim">{hiddenRows.length === 1 ? "Its alerts don’t" : "Their alerts don’t"} reach the inbox or notify you.</span></summary
                    >
                  </details>
                </td>
              </tr>
            {:else if entry.failed !== null}
              <tr class="tf-mon-failed" {@attach win.row(entryKey(entry))}>
                <td colspan="7"><div class="tf-banner tf-err tf-refresh-failed" role="alert">{entry.failed}</div></td>
              </tr>
            {:else}
              {@const chip = stateChip(row, now)}
              <!-- A hidden monitor draws its state in no tone and offers no Ack: its alerts reach no inbox. -->
              {@const tone = row.hidden ? null : chip.tone}
              {@const down = tone === "err" && !row.tombstoned}
              {@const toggled = echoOn(row.entity_id, TOGGLE)}
              {@const refresher = refreshers.get(row.entity_id)}
              <tr class:tf-mon-hid={row.hidden} class:tf-mon-at={row.entity_id === markedId} aria-current={row.entity_id === markedId ? "true" : undefined} {@attach win.row(entryKey(entry))}>
                <td><Marked text={row.name} typed={filter.text} />{#if row.monitor_type}{" "}<span class="tf-dim tf-mon-type">{kindWord(row.monitor_type)}</span>{/if}</td>
                <td><div class="tf-mon-state"><TfStateChip text={chip.label} {tone} />{#if toggled !== null}<!-- The pause's or resume's echo (#2105): *queued*, then *sent*, until the roster reads the new state. --><TfStateChip text={toggled.state} tone="info" class="tf-queued-mark" />{/if}{#if chip.since}<span class="tf-mon-since">{chip.since}</span>{/if}</div></td>
                <!-- Before the bars and figures, not after them: the row's actions stand in the last column's place on hover, and this column's links have to stay in reach. -->
                <td>
                  {#each row.assets as asset (asset.id)}
                    <div><a class="tf-mon-asset" href="#/asset/{encodeURIComponent(asset.id)}"><Marked text={asset.name} typed={filter.text} /></a>{" "}<span class="tf-dim tf-mon-type">{asset.type_label}</span></div>
                  {:else}
                    <span class="tf-dim">no asset</span>
                  {/each}
                </td>
                <td>
                  <div class="tf-mon-24">
                    <span class="tf-mbar" role="img" aria-label="Last 24 hours of {row.name}">
                      {#each barsOf(segments(row, now.getTime()), hourlyBars.current) as bar (bar.at)}<i class="tf-seg-{bar.state}" title={bar.title}></i>{/each}
                    </span>
                    <!-- Where the two figures' columns have gone (`monitors.css`), they stay under the bars. -->
                    <span class="tf-mon-figs tf-rowval" title="Response {responseText(row.response_time_ms)}, {uptimeText(row)} up over 30 days">{responseText(row.response_time_ms)}, {uptimeText(row)}</span>
                  </div>
                </td>
                <td class="tf-num"><span class="tf-rowval">{responseText(row.response_time_ms)}</span></td>
                <td class="tf-num"><span class="tf-rowval">{uptimeText(row)}</span></td>
                <td class="tf-mon-acts"><div class="tf-rowend">
                  {#if down}
                    <button type="button" class="tf-btn tf-sm tf-mon-ack" aria-pressed={row.entity_id in acked} title={row.entity_id in acked ? "Acked. Press to take it back, so the alert asks again." : undefined} onclick={() => void (row.entity_id in acked ? unack(row) : ack(row))}
                      >{row.entity_id in acked ? "Acked" : "Ack"}</button
                    >
                  {/if}
                  <span class="tf-rowacts">
                  <!-- Unkeyed: when the read turns *Pause* into *Resume* it is one button, so focus stays on it. -->
                  {#each row.actions.filter((op) => op in ROW_OPS) as op}
                    <button type="button" class="tf-btn tf-sm tf-ghost" aria-disabled={toggled !== null ? "true" : undefined} title={toggled !== null ? "On its way. It settles when the roster reads the new state." : undefined} onclick={() => void toggle(row, op)}>{ROW_OPS[op]!.label}</button>
                  {/each}
                  <button
                    type="button"
                    class="tf-btn tf-sm tf-ghost"
                    disabled={pressing[row.entity_id] === true}
                    title={row.hidden ? "Show it on the roster and let its alerts reach the inbox again" : "Take it off the roster; its alerts stop reaching the inbox and stop notifying"}
                    onclick={(event) => void setHidden(row, !row.hidden, event)}>{row.hidden ? "Unhide" : "Hide"}</button
                  >
                  {#if refresher}<TfDetailRefresh detail={subjectOf(row)} refresh={refresher} {reread} small ghost />{/if}
                  </span>
                </div></td>
              </tr>
            {/if}
          {/each}
          {#if win.after > 0}<tr class="tf-wgap" aria-hidden="true"><td colspan="7" {@attach win.room("after")}></td></tr>{/if}
        </tbody>
      </table>
    {:else if roster.length === 0}
      <div class="tf-empty">No monitors yet. Add an Uptime Kuma source under Settings › Sources and its checks show here.</div>
    {:else}
      <TfNoMatch noun="monitors" {filter}>
        {#if scope !== "all"}
          <button type="button" class="tf-btn tf-sm" onclick={() => (filter.scope = "all")}>Search all monitors</button>
        {/if}
      </TfNoMatch>
    {/if}
  {/if}
</div>
{#if creating !== null}
  <TfCreateMonitor
    subject={creating}
    record={(name) => recordName(creating!, name)}
    queue={ports.submitWrite}
    onclose={() => (creating = null)}
    oncreated={created}
  />
{/if}

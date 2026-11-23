<!--
  Monitors (#864, R-11.3 to R-11.5): `#/monitors` and `#/monitors/unmonitored`,
  one page with two tabs as prototype-6's `renderMonitors(tab)`.

  - **Monitors** (redrawn in the monitors-page pass, 2026-11-19; Björn: "it should be one of the smoothest pages of
    all"): the filter bar (text over monitor, asset and type; *This job / My jobs / All*; *Down / Slow / Up / Paused /
    Pending / Maintenance*; *Type*), a line of what is wrong (*16 down, 2 slow, 200 up*, in the width that shouts),
    then the roster sorted down, slow, paused, up, every row drawn and one line high. Each row: the monitor and its
    type, its state (a chip where something is wrong, a quiet word where it is up), when it went down or how slow it
    is, what it **Watches**, the 24-hour bar, the 30-day uptime, and *Ack* on a down monitor, which *Acked* takes
    back. A row is pressed to open the monitor beside the roster (`#/monitors/at/<id>`, which an inbox alert opens
    too; `Esc` closes it): its target, response, uptimes, certificate and assets, *Ack*, *Pause* or *Resume* where the
    source offers it, *Hide* or *Unhide*, *Refresh* or *Sync now* (#2138, its failure under them), and its page in
    Uptime Kuma. A hidden monitor (#2447) is drawn after the roster under one line that says so, dimmed, its bar
    grey, its state neutral and with no *Ack*; every filter applies to it as to the roster. Its alerts reach no
    inbox, so it raises no desktop notification; knobas-local, Uptime Kuma goes on checking it. While a pause or
    resume is on its way its echo (*queued*, then *sent*, `tf/queued.svelte.ts`, #2105) stands after the state chip,
    and the button stays in its place, turned off, until the roster reads the new state.

    A poll redraws only what it changed: each read keeps every row whose drawn parts are the same (`keepDrawn`), and
    a row's response time is drawn only while it is slow, so a minute's poll that moves every response by a
    millisecond draws nothing. The roster reads each monitor's bar as 48 letters (`0099`), not its day of samples.
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

  **Refresh (#2138, #2107's pieces).** Monitors has no page of one monitor, so the open monitor's *Refresh*, or
  *Sync now* where its source does not read one item (`MonitorRow.refreshes`), is among its actions, and its failure
  is a line under them. A refresh reads the roster again over what it drew, and the pause and resume echoes settle on
  that read as on any other. The roster holds only enabled sources' monitors, so no row is turned off; a tombstoned
  monitor is drawn *Paused* (#2100), so no row is withdrawn either.

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
    AssetType,
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
    /** The open monitor's *Link an asset…* and *New asset for it…* (2026-11-19): the estate's levels, its types, a new asset and the link. The bridge's when left out. */
    assetTree?: (parentId: string | null) => Promise<AssetRow[]>;
    assetTypes?: () => Promise<AssetType[]>;
    createAsset?: (typeId: string, name: string, parentId?: string | null) => Promise<AssetRow>;
    /** Draw the `monitored-by` link from the asset to the monitor. */
    linkAsset?: (assetId: string, monitorId: string) => Promise<unknown>;
  }
</script>

<script lang="ts">
  import TfStateChip from "../TfStateChip.svelte";
  import { stateWord } from "../state-word";
  import TfSideEdge from "../TfSideEdge.svelte";
  import { sideWidth } from "../side-width.svelte";
  import { badges } from "../badges.svelte";
  import { listen as tauriListen } from "@tauri-apps/api/event";
  import { tick, untrack } from "svelte";
  import { MONITOR_AT, monitorHash } from "../route";
  import TfNothingHere from "../TfNothingHere.svelte";
  import type { RoleFrameProps } from "../page-entry";

  import { ipcErrorMessage, isIpcError } from "../../ipc";
  import {
    ackAlert,
    unackAlert,
    assetTree,
    assetTypes,
    createAsset,
    editAsset,
    editRoute,
    getAsset,
    getRoute,
    monitorRoster,
    openAlerts,
    setMonitorHidden,
    unmonitoredAssets,
  } from "../../ipc/assets";
  import { createLink, getEntity, jobReach, listContexts, submitWrite } from "../../ipc/entity";
  import CreateDialog from "../../assets/CreateDialog.svelte";
  import LinkAssetDialog from "../../assets/LinkAssetDialog.svelte";
  import { listSources, syncStatus } from "../../ipc/sources";
  import { mayHaveLanded, writeFollower } from "../../shell/follow-write";
  import { echoOn, echoWrite, echoesOn, settleEchoes, watchEchoes, type Echo } from "../queued.svelte";
  import { toastQueued, type WriteSource } from "../../shell/queued-toast";
  import { ago } from "../../shell/time";
  import { dismiss, push } from "../../shell/toasts.svelte";
  import { HeldOrder } from "../held-order.svelte";
  import FilterBar from "../filter/FilterBar.svelte";
  import TfNoMatch from "../filter/TfNoMatch.svelte";
  import { sessionFilter } from "../filter/filter-state.svelte";
  import { MediaQuery } from "svelte/reactivity";
  import { barsOf, keepDrawn, stripPaint, type Bar } from "./monitors";
  import { chipCounts } from "../../assets/monitors";
  import { on } from "svelte/events";
  import type { FilterConfig, FilterScope } from "../filter/filter";
  import Marked from "../filter/Marked.svelte";
  import { listenSynced } from "../builds/pipeline-ports";
  import { staleLine, syncWith } from "../builds/pipeline";
  import { jobIndex, readOpenJobs, type OpenJob } from "../job/open-jobs";
  import {
    assetFor,
    stateChip,
    gaps,
    isRoute,
    isRouteId,
    kindWord,
    monitorMatches,
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
  import { menuThing } from "../context/thing";
  import { createDetailRefresh, type DetailRefresh, type RefreshSubject } from "../refresh.svelte";
  import { layers } from "../layers.svelte";
  import { pageVisible } from "../kept-pages.svelte";

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
    /** What follows `monitors` in the address: `unmonitored`, `at/<id>`, or nothing. */
    rest: string[];
    /** The chosen job (spec #2227), as a ticket key. */
    job: string | null;
    ports?: MonitorPorts;
    /** Injectable clock, for the chips' *since* and the header's *synced*. */
    now?: Date;
  } & RoleFrameProps = $props();

  /** The clock the header's *synced* and the open monitor read: the time of the last roster read. */
  let readAt = $state(new Date());
  const now = $derived(fixedNow ?? readAt);
  const tab = $derived<"all" | "unmonitored">(rest[0] === "unmonitored" ? "unmonitored" : "all");
  /** The monitor the address opens, beside the roster: an inbox alert's, a set member's. */
  const markedId = $derived(rest[0] === MONITOR_AT ? (rest[1] ?? null) : null);
  /**
   * The monitor open beside the roster: the address's, or a row's press. A press opens it on the page and changes no
   * address, so nothing above the roster comes or goes as it opens (a deeper address drew the breadcrumb's bar, and the
   * roster dropped under the pointer); `Esc` closes it as any panel on a page (`layers`). An address that names one
   * opens it, and the way back is the address's.
   */
  let openId = $state<string | null>(untrack(() => markedId));
  $effect(() => {
    const id = markedId;
    untrack(() => (openId = id));
  });
  const shown = pageVisible();
  $effect(() => {
    if (openId === null || openId === markedId || !shown()) return;
    return untrack(() => layers.add({ kind: "panel", close: () => (openId = null) }));
  });
  /** *Close*: a press's monitor closes on the page; the address's goes back to the roster's own address. */
  function closeMonitor() {
    if (openId !== null && openId === markedId) location.hash = "#/monitors";
    else openId = null;
  }

  /**
   * The roster as drawn: the read narrows itself to the sources filling the monitoring role (#1297, ADR-0029 as
   * amended on 2026-10-19). Raw, and each read keeps every row whose drawn parts are the same as the object it was
   * (`keepDrawn`), so a poll redraws the rows it changed and nothing else (the monitors-page pass, 2026-11-19).
   */
  let roster = $state.raw<MonitorRow[] | null>(null);
  /** The roster as last read, every row its newest object: what the open monitor draws (its response, when it was read). */
  let latest = $state.raw<ReadonlyMap<string, MonitorRow>>(new Map());
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
  /** The monitor *Link an asset…* is open for. */
  let linkingFor = $state<MonitorRow | null>(null);
  /** The monitor *New asset for it…* is open for, with the estate's types read for the dialog. */
  let newFor = $state<{ monitor: MonitorRow; types: AssetType[] } | null>(null);
  /** Whether the estate holds no asset at all: *Assets nothing watches* then says so rather than "every … has a monitor". */
  let estateEmpty = $state(false);

  /** Bumped to read everything again: a write of the page's, a create's echo moving. */
  let revision = $state(0);
  /**
   * Bumped when a run of the role's sources ends with something new (an Uptime Kuma poll, every minute), and when the
   * page is shown again after one: the roster and what it is drawn with are read again, and *Assets nothing watches*
   * only while it is the tab shown (#2106).
   */
  let synced = $state(0);
  let syncedAt = untrack(() => syncRevision);
  $effect(() => {
    if (syncRevision === syncedAt) return;
    syncedAt = syncRevision;
    untrack(() => (synced += 1));
  });
  /** The refreshes waiting on the roster read that answers next (#2138). */
  let rereads: (() => void)[] = [];

  $effect(() => {
    if (!ready) return;
    void revision;
    void synced;
    let live = true;
    const fail = (error: unknown) => {
      if (live) failed = ipcErrorMessage(error);
    };
    ports
      .monitorRoster()
      .then((rows) => {
        if (!live) return;
        failed = null;
        keepRefreshers(rows);
        readAt = new Date();
        latest = new Map(rows.map((row) => [row.entity_id, row]));
        roster = keepDrawn(untrack(() => roster), rows);
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
        // The same acks as before are the same object: nothing that reads them draws again.
        if (JSON.stringify(read) !== JSON.stringify(untrack(() => $state.snapshot(acked)))) acked = read;
      },
      () => {},
    );
    ports.listSources().then(
      (rows) => {
        if (!live) return;
        const names = Object.fromEntries(rows.map((row) => [row.id, row.display_name]));
        if (JSON.stringify(names) !== JSON.stringify(untrack(() => $state.snapshot(sourceNames)))) sourceNames = names;
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

  // *Assets nothing watches*: on the first read and every write, when its tab is shown, and on a sync's end only while
  // it is. Its own count, not the effect's cleanup, drops an answer a newer read replaced: a sync on the roster's tab
  // wakes the effect and reads nothing, and must not drop the read in flight.
  let unwatchedRevision = -1;
  let unwatchedReads = 0;
  $effect(() => {
    if (!ready) return;
    const at = revision;
    const shown = tab === "unmonitored";
    void synced;
    if (at === unwatchedRevision && !shown) return;
    unwatchedRevision = at;
    const mine = ++unwatchedReads;
    ports.unmonitoredAssets().then(
      (rows) => {
        if (mine !== unwatchedReads) return;
        unwatched = gaps(rows);
        settleUnwatched(rows);
        // Nothing unwatched is either every asset watched or no asset at all; the estate's top tells them apart.
        if (rows.length > 0) estateEmpty = false;
        else
          void treeOf(null).then(
            (roots) => {
              if (mine === unwatchedReads) estateEmpty = roots.length === 0;
            },
            () => {},
          );
      },
      (error: unknown) => {
        if (mine === unwatchedReads) failed = ipcErrorMessage(error);
      },
    );
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
  /**
   * Every monitor the scope, the state chips and the type keep. The typed words don't take a row out of the DOM: they
   * hide the rows they leave out (`shows`), so the next key brings a row back by an attribute and not by building it
   * (the ux24 pass, 2026-11-20: a Backspace that brought every monitor back built all 218 rows again, 104 and 205 ms at
   * CPU 4x and 6x at full screen in WebView2).
   */
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
            text: "",
          },
          index,
        ),
  );
  /** The monitors the typed words keep, or `null` while nothing is typed. */
  const matched = $derived.by(() => {
    const text = filter.text;
    if (roster === null || text.trim() === "") return null;
    return new Set(roster.filter((row) => monitorMatches(row, text)).map((row) => row.entity_id));
  });
  /** Whether a drawn row shows: the typed words keep it, or it is the open monitor, whatever the filter says. */
  function shows(row: MonitorRow): boolean {
    return matched === null || matched.has(row.entity_id) || row.entity_id === openId;
  }
  /** The open monitor, whatever the filter says: the reader came for it. */
  const markedRow = $derived(openId === null ? null : (roster?.find((row) => row.entity_id === openId) ?? null));
  /**
   * The roster stays where it first drew (the still-rows pass, 2026-11-11): a check that goes down sorts first, and a
   * sync bringing it jumped every row under the pointer. It waits behind *Sort again*; a monitor a sync adds above
   * waits behind *Show N new*. A row a press takes away (*Hide*) goes.
   */
  const rosterHold = new HeldOrder<(typeof kept.shown)[number]>((row) => row.entity_id, () => "", { keepLeft: false });
  const heldRoster = $derived(
    roster === null
      ? { rows: [] as typeof kept.shown, waiting: 0, stale: false }
      : rosterHold.place(kept.shown, JSON.stringify([scope, job, filter.chips, filter.dropdowns, filter.text])),
  );
  const heldBar = $derived({ waiting: heldRoster.waiting, stale: heldRoster.stale, show: () => rosterHold.release() });
  const rows = $derived(markedRow !== null && !markedRow.hidden && !heldRoster.rows.includes(markedRow) ? [markedRow, ...heldRoster.rows] : heldRoster.rows);
  const hiddenRows = $derived(markedRow === null || !markedRow.hidden || kept.hidden.includes(markedRow) ? kept.hidden : [markedRow, ...kept.hidden]);
  /** The rows the typed words keep, of the roster and of the hidden monitors: what the page counts and says. */
  const shownRows = $derived(matched === null ? rows : rows.filter(shows));
  const shownHidden = $derived(matched === null ? hiddenRows : hiddenRows.filter(shows));
  /** The roster's first shown row draws no rule over it: the header's edge is its rule. */
  const topRow = $derived(shownRows[0]?.entity_id ?? null);
  /** The roster's own size, for the count: a hidden monitor is counted apart. */
  const unhiddenTotal = $derived(roster === null ? null : roster.filter((row) => !row.hidden).length);
  /** What the summary says: how many of the monitors not hidden read each way. */
  /** Whether any monitor watches an asset: the roster's *Watches* header names a column only when one holds something. */
  const anyWatches = $derived((roster ?? []).some((row) => row.assets.length > 0));
  const counts = $derived.by(() => {
    const tally = Object.fromEntries(chipCounts((roster ?? []).filter((row) => !row.hidden)).map((chip) => [chip.state, chip.count]));
    return { down: tally["down"] ?? 0, slow: tally["warn"] ?? 0, up: tally["up"] ?? 0, paused: tally["paused"] ?? 0, other: (tally["pending"] ?? 0) + (tally["other"] ?? 0) };
  });

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

  /**
   * A monitor's 24-hour strip, one element (`stripPaint`): its bars painted through the CSSOM (CSP refuses an inline
   * style), and the bar under the pointer named in its title. Painted once a row is drawn, again only when its bar
   * changes (a keyed row whose drawn parts are the same is the same object, `keepDrawn`).
   */
  function paintStrip(bars: Bar[], hourly: boolean, quiet: boolean, big = false) {
    return (node: HTMLElement) => {
      const paint = stripPaint(bars, hourly, quiet, big);
      node.style.setProperty("background-image", paint.image);
      node.style.setProperty("width", `${paint.width}px`);
      const name = (event: PointerEvent) => {
        const bar = bars[Math.min(bars.length - 1, Math.max(0, Math.floor(event.offsetX / paint.pitch)))];
        if (bar && node.title !== bar.title) node.title = bar.title;
      };
      const off = on(node, "pointermove", name, { passive: true });
      return () => {
        off();
        node.style.removeProperty("background-image");
        node.style.removeProperty("width");
      };
    };
  }

  /**
   * The roster in blocks of `BLOCK` rows, each block a box the browser lays out and paints only near the viewport
   * (`content-visibility: auto`, `monitors.css`). Every row is in the DOM, and a block's room is its rows' exact height
   * (`blockRoom`), so a block drawn as it comes into view moves nothing. One box a block, not a row: each row its own
   * box made 218 clips that every frame of a scroll laid out into layers again (the monitors-page pass, 2026-11-19).
   */
  const BLOCK = 16;
  function blocksOf(list: readonly MonitorRow[]): MonitorRow[][] {
    const blocks: MonitorRow[][] = [];
    for (let at = 0; at < list.length; at += BLOCK) blocks.push(list.slice(at, at + BLOCK));
    return blocks;
  }

  /**
   * The blocks drawn with their rows (the ux30 Windows pass, 2026-11-23,
   * `docs/decisions/2026-11-23-monitors-draws-the-window-first.md`): on the page's first draw the first
   * {@link FIRST_BLOCKS}, more than a full-screen window holds, and {@link FILL_BLOCKS} more on each frame after, until
   * every row is drawn. A block waiting for its rows holds their exact room (`tf-mon-later`), so a block filling moves
   * nothing. Building all 218 rows in the press's task was 150 ms of script at CPU 6x in WebView2, and their style and
   * layout 180 more, when Monitors opened again after the kept pages had let it go. Once drawn, a block stays drawn.
   */
  const FIRST_BLOCKS = 4;
  const FILL_BLOCKS = 3;
  let drawnBlocks = $state(FIRST_BLOCKS);
  const rowBlocks = $derived(Math.ceil(rows.length / BLOCK));
  const allBlocks = $derived(rowBlocks + Math.ceil(hiddenRows.length / BLOCK));
  $effect(() => {
    if (drawnBlocks >= allBlocks) return;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const frame = requestAnimationFrame(() => {
      timer = setTimeout(() => (drawnBlocks += FILL_BLOCKS));
    });
    return () => {
      cancelAnimationFrame(frame);
      clearTimeout(timer);
    };
  });
  /**
   * A block's room while it is not drawn: its shown rows at their one height, through the CSSOM (CSP refuses an inline
   * style). The blocks stay the same 16 rows whatever is typed, so a key moves no row from one block to another.
   */
  function blockRoom(rows: number) {
    return (node: HTMLElement) => {
      node.style.setProperty("--tf-mon-rows", String(rows));
    };
  }

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
    if (roster === null) return "Checks from your monitoring source.";
    const names = [...new Set(roster.map((row) => sourceNames[row.source_id] ?? row.source_id))];
    const newest = [...latest.values()].map((row) => row.checked_at).sort().at(-1) ?? null;
    const from = names.length > 0 ? ` from ${names.join(" and ")}` : "";
    // A check's own time is not a sync: where a source can't sync, the band says how old its data is.
    const synced = newest !== null && sources !== null && stale.length === 0 ? `, synced ${ago(newest, now)}` : "";
    // What they watch, as it is: an estate with no asset linked says nothing of it (2026-11-19).
    const watching = roster.filter((row) => row.assets.length > 0).length;
    const watches = watching === 0 ? "" : watching === roster.length ? " Each one watches an asset." : ` ${watching} of them watch an asset.`;
    return `${counted(roster.length, "check", "checks")}${from}${synced}.${watches}`;
  });

  // -- a monitor's Refresh (#2138) -------------------------------------------

  /** Each monitor's own *Refresh*, so one row's *Refreshing…* is not another's; made as the roster is read. */
  const refreshers = new Map<string, DetailRefresh>();
  function keepRefreshers(rows: readonly MonitorRow[]) {
    const ids = new Set(rows.map((row) => row.entity_id));
    for (const id of refreshers.keys()) if (!ids.has(id)) refreshers.delete(id);
    for (const id of ids) if (!refreshers.has(id)) refreshers.set(id, createDetailRefresh(untrack(() => ports.refresh)));
  }

  /** The open monitor as last read (its response and when it was read move with every poll), and its refresh. */
  const panel = $derived(markedRow === null ? null : (latest.get(markedRow.entity_id) ?? markedRow));
  const panelRefresh = $derived(panel === null ? undefined : refreshers.get(panel.entity_id));

  /** An open monitor's row comes into view once, as the address names it; a later read leaves the reader where they are. */
  let broughtIn: string | null = null;
  $effect(() => {
    const id = markedId;
    if (id === null) {
      broughtIn = null;
      return;
    }
    if (broughtIn === id || markedRow === null) return;
    broughtIn = id;
    void tick().then(() => document.querySelector(".tf-kept:not([hidden]) .tf-mon-row[aria-current]")?.scrollIntoView?.({ block: "nearest" }));
  });

  /** What a monitor's *Refresh* reads: the roster holds only enabled sources' monitors. */
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

  // -- the actions -----------------------------------------------------------

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

  /**
   * The alert closed while the row was drawn: not a failure, so it says what happened and the row is read again. Here
   * that is the monitor recovering, which the page shows, not the Inbox, which a monitor with no asset never reached
   * (an *Undo* after a real recovery, the live run 2026-11-09).
   */
  function refusedAck(row: MonitorRow, error: unknown) {
    if (isIpcError(error) && error.code === "not_found") {
      delete acked[row.entity_id];
      dropAckToast(row);
      push({ text: `${row.name} has recovered, so its alert is closed.` });
      revision += 1;
      return;
    }
    push({ text: ipcErrorMessage(error), tone: "err" });
  }

  /**
   * *Hide* turns into *Unhide* under the press, the same button with the monitor still open, so it holds about two
   * seconds as *Acked* does (F-351): a double press is one hide. The toast's *Undo* is not held.
   */
  const hideHold = new PressHold();

  /** *Hide* or *Unhide*: knobas-local, so the row moves at once (to the hidden monitors, or back) and the roster is read again. */
  async function setHidden(row: MonitorRow, hidden: boolean, undo = false) {
    if (pressing[row.entity_id] || (!undo && hideHold.held(row.entity_id))) return;
    pressing[row.entity_id] = true;
    try {
      await ports.setMonitorHidden(row.entity_id, hidden);
      if (!undo) hideHold.hold(row.entity_id);
      if (roster !== null) roster = roster.map((each) => (each.entity_id === row.entity_id ? { ...each, hidden } : each));
      // A hidden monitor is not down as far as the nav says: there is nothing left to act on.
      ports.badgesChanged?.();
      push({
        text: hidden
          ? `Hid ${row.name}. Its alerts stay quiet.`
          : `${row.name} is back on the roster, and its alerts reach the inbox again.`,
        ...(hidden ? { action: { label: "Undo", run: () => void setHidden({ ...row, hidden: true }, false, true) } } : {}),
      });
      revision += 1;
    } catch (error) {
      push({ text: ipcErrorMessage(error), tone: "err" });
    } finally {
      delete pressing[row.entity_id];
    }
  }

  /** The pause and resume a monitor may offer, by the op its source declares. */
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

  // -- what a monitor watches (2026-11-19: Assets has its nav entry again, and the panel's *Watches* offered nothing) --

  const treeOf = (parentId?: string | null) => (ports.assetTree ?? assetTree)(parentId ?? null);
  /** The `monitored-by` link, from the asset to the monitor, as an import and *Create monitor for this asset* draw it through a name. */
  const linkTo = (assetId: string, monitorId: string) => (ports.linkAsset ?? ((from, to) => createLink(from, to, "monitored-by")))(assetId, monitorId);
  /** The estate's types, read once for *New asset for it…*. */
  let assetTypeList: AssetType[] | null = null;

  function linked(monitor: MonitorRow, asset: AssetRow) {
    linkingFor = null;
    push({ text: `${monitor.name} watches ${asset.name}` });
    revision += 1;
  }

  async function openNewAsset(monitor: MonitorRow) {
    linkingFor = null;
    try {
      assetTypeList ??= await (ports.assetTypes ?? assetTypes)();
      newFor = { monitor, types: assetTypeList };
    } catch (error) {
      push({ text: ipcErrorMessage(error), tone: "err" });
    }
  }

  /** The asset *New asset for it…* made, linked to its monitor; it is in Assets either way, so a failed link says so. */
  async function madeFor(monitor: MonitorRow, asset: AssetRow) {
    newFor = null;
    const open = { label: "Open", run: () => (location.hash = `#/asset/${encodeURIComponent(asset.id)}`) };
    try {
      await linkTo(asset.id, monitor.entity_id);
      push({ text: `Added ${asset.name} to Assets, watched by ${monitor.name}`, action: open });
    } catch (error) {
      push({ text: `Added ${asset.name} to Assets, but could not link it: ${ipcErrorMessage(error)}`, tone: "err", action: open });
    }
    revision += 1;
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

  /** An unwatched row opens its asset; a click on a link or button inside it already did its own thing. */
  function openAsset(event: MouseEvent, id: string) {
    if ((event.target as Element | null)?.closest("a, button")) return;
    location.hash = addressOf(id);
  }

  /** A monitor's row opens it beside the roster, and pressed again closes it; a press on a link or a button in it did its own thing. */
  function openMonitor(event: MouseEvent, id: string) {
    if ((event.target as Element | null)?.closest("a, button:not(.tf-mon-open)")) return;
    if (openId === id) closeMonitor();
    else openId = id;
  }

  /** A route opens at `#/route/<id>` (the Tree at its exposing asset), an asset at `#/asset/<id>`. */
  function addressOf(id: string): string {
    return `#/${isRouteId(id) ? "route" : "asset"}/${encodeURIComponent(id)}`;
  }

  /** The uptime windows Kuma publishes, in words. */
  const WINDOW_WORDS: Record<string, string> = { "1d": "1 day", "7d": "7 days", "30d": "30 days", "365d": "1 year" };
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
  <FilterBar {config} {filter} shown={roster === null ? null : shownRows.length} total={unhiddenTotal} held={heldBar}>
    {#snippet after()}<span class="tf-mon-hidn">{#if hiddenRows.length > 0}, {hiddenRows.length} hidden{/if}</span>{/snippet}
  </FilterBar>
{:else}
  <FilterBar config={unwatchedConfig} filter={unwatchedFilter} shown={unwatched === null ? null : shownUnwatched.length} total={unwatched?.length ?? null} />
{/if}

{#snippet monitorRow(row: MonitorRow)}
  <!-- A hidden monitor draws its state in no tone and offers no Ack: its alerts reach no inbox. -->
  {@const chip = stateChip(row, fixedNow)}
  {@const tone = row.hidden ? null : chip.tone}
  {@const down = tone === "err" && !row.tombstoned}
  {@const toggled = echoOn(row.entity_id, TOGGLE)}
  <!-- The row's name is its link, so a key reaches it there; the press anywhere else in the row is the pointer's way. -->
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
  <div
    class="tf-mon-row"
    class:tf-mon-err={down}
    class:tf-mon-warn={tone === "warn"}
    class:tf-mon-hid={row.hidden}
    class:tf-mon-top={row.entity_id === topRow}
    hidden={!shows(row)}
    role="listitem"
    aria-current={row.entity_id === openId ? "true" : undefined}
    onclick={(event) => openMonitor(event, row.entity_id)}
    {@attach menuThing(() => ({ kind: "monitor", id: row.entity_id, hash: monitorHash(row.entity_id), alerting: down && !(row.entity_id in acked), ack: () => void ack(row) }))}
  >
    <!-- The name is the row's button, for the keyboard; the pointer may press anywhere on the row. -->
    <span class="tf-mon-name"
      ><button type="button" class="tf-mon-open" aria-expanded={row.entity_id === openId}><Marked text={row.name} typed={shows(row) ? filter.text : ""} /></button
      >{#if row.monitor_type}{" "}<span class="tf-mon-type">{kindWord(row.monitor_type)}</span>{/if}</span
    >
    <span class="tf-mon-state">
      {#if tone === "err" || tone === "warn" || tone === "info"}<TfStateChip text={chip.label} {tone} />{:else}<span class="tf-mon-word">{stateWord(chip.label)}</span>{/if}
      {#if toggled !== null}<!-- The pause's or resume's echo (#2105): *queued*, then *sent*, until the roster reads the new state. --><TfStateChip text={toggled.state} tone="info" class="tf-queued-mark" />{/if}
      {#if chip.since}<span class="tf-mon-since" title={chip.since}>{chip.since}</span>{:else if tone === "warn"}<span class="tf-mon-since">{responseText(row.response_time_ms)}</span>{/if}
    </span>
    <span class="tf-mbar" class:tf-mbar-hourly={hourlyBars.current} role="img" aria-label="Last 24 hours of {row.name}" {@attach paintStrip(barsOf(segments(row), hourlyBars.current), hourlyBars.current, row.hidden)}></span>
    <span class="tf-mon-up">{uptimeText(row)}</span>
    <span class="tf-mon-ack"
      >{#if down}<button
          type="button"
          class="tf-btn tf-sm"
          aria-pressed={row.entity_id in acked}
          title={row.entity_id in acked ? "Acked. Press to take it back, so the alert asks again." : "Ack the alert: it stays open, and stops asking"}
          onclick={() => void (row.entity_id in acked ? unack(row) : ack(row))}>{row.entity_id in acked ? "Acked" : "Ack"}</button
        >{/if}</span
    >
    <span class="tf-mon-watch" title={row.assets.length === 0 ? undefined : row.assets.map((asset) => asset.name).join(", ")}
      >{#each row.assets as asset, at (asset.id)}{#if at > 0}{", "}{/if}<a href="#/asset/{encodeURIComponent(asset.id)}"><Marked text={asset.name} typed={shows(row) ? filter.text : ""} /></a>{/each}</span
    >
  </div>
{/snippet}

<!-- A block of rows (`blocksOf`): its room is its shown rows' (`blockRoom`), and with none shown it is hidden too. One
     the page has not drawn yet (`drawnBlocks`) holds that room empty for a frame or two. -->
{#snippet monitorBlock(block: MonitorRow[], at: number)}
  {@const open = matched === null ? block.length : block.filter(shows).length}
  {@const drawn = at < drawnBlocks}
  <div class="tf-mon-block" class:tf-mon-later={!drawn} hidden={open === 0} {@attach blockRoom(open)}>{#if drawn}{#each block as row (row.entity_id)}{@render monitorRow(row)}{/each}{/if}</div>
{/snippet}

<div class="tf-cbody tf-mon-body" {@attach sideWidth("monitor")}>
  {#if tab === "all" && panel !== null}<TfSideEdge side="monitor" />{/if}
  <!-- With a monitor open, what each row watches stops at the panel's edge, its names in its title (2026-11-19). -->
  <div class="tf-mon-main" class:tf-mon-paneled={tab === "all" && panel !== null}>
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
      {:else if unwatched !== null && estateEmpty}
        <div class="tf-empty">
          <p>No assets yet, so nothing is left unwatched. Add what you run under <a href="#/assets">Assets</a>, or open a monitor and press <em>New asset for it</em>.</p>
        </div>
      {:else if unwatched !== null}
        <div class="tf-empty">Every container, service, route, database and VM has a monitor.</div>
      {/if}
    {:else if roster !== null}
      {#if roster.length === 0}
        <div class="tf-empty">No monitors yet. Add an Uptime Kuma source under Settings › Sources and its checks show here.</div>
      {:else}
        <!-- What is wrong, in the width that shouts: down and slow in their tones, the rest quiet (the monitors-page pass, 2026-11-19). -->
        <p class="tf-mon-sum" aria-label="Monitors by state">
          {#if counts.down + counts.slow === 0}
            <span class="tf-mon-fig tf-mon-calm"><b>{counts.up}</b> up</span>
          {:else}
            <span class="tf-mon-fig" class:tf-mon-figerr={counts.down > 0}><b>{counts.down}</b> down</span>
            <span class="tf-mon-fig" class:tf-mon-figwarn={counts.slow > 0}><b>{counts.slow}</b> slow</span>
            <span class="tf-mon-fig tf-mon-calm"><b>{counts.up}</b> up</span>
          {/if}
          {#if counts.paused > 0}<span class="tf-mon-fig tf-mon-calm"><b>{counts.paused}</b> paused</span>{/if}
          {#if counts.other > 0}<span class="tf-mon-fig tf-mon-calm"><b>{counts.other}</b> other</span>{/if}
        </p>
        <!-- The rows the typed words leave out stay drawn, hidden, and so does the list when none is left (`shows`). -->
        {#if rows.length + hiddenRows.length > 0}
          <div class="tf-mon-list" role="list" aria-label="Monitors" hidden={shownRows.length + shownHidden.length === 0}>
            <div class="tf-mon-head" aria-hidden="true">
              <span>Monitor</span><span>State</span><span class="tf-mbar-axis"><span>24 h ago</span><span>now</span></span><span class="tf-mon-up">30 days</span><span></span><span class="tf-mon-watch">{#if anyWatches}Watches{/if}</span>
            </div>
            {#each blocksOf(rows) as block, at (at)}{@render monitorBlock(block, at)}{/each}
            {#if hiddenRows.length > 0}
              <!-- Hidden monitors are drawn after the roster, all of them, dimmed: nothing folds, so nothing redraws or moves (#2447, the monitors-page pass). -->
              <div class="tf-mon-hidhead" role="presentation" hidden={shownHidden.length === 0}>
                <span>{counted(shownHidden.length, "hidden monitor", "hidden monitors")}</span>
                <span class="tf-dim">{shownHidden.length === 1 ? "Its alerts don’t" : "Their alerts don’t"} reach the inbox or notify you.</span>
              </div>
              {#each blocksOf(hiddenRows) as block, at (at)}{@render monitorBlock(block, rowBlocks + at)}{/each}
            {/if}
          </div>
        {/if}
        {#if shownRows.length + shownHidden.length === 0}
          <TfNoMatch noun="monitors" {filter}>
            {#if scope !== "all"}
              <button type="button" class="tf-btn tf-sm" onclick={() => (filter.scope = "all")}>Search all monitors</button>
            {/if}
          </TfNoMatch>
        {/if}
      {/if}
    {/if}
  </div>
  {#if tab === "all" && panel !== null}
    {@const chip = stateChip(panel, now)}
    {@const tone = panel.hidden ? null : chip.tone}
    {@const down = tone === "err" && !panel.tombstoned}
    {@const toggled = echoOn(panel.entity_id, TOGGLE)}
    <!-- The open monitor: everything its row leaves out, and what can be done to it. `Esc` closes it. -->
    <aside class="tf-mon-panel" aria-label={panel.name}>
      <header>
        <h2>{panel.name}</h2>
        <button type="button" class="tf-btn tf-sm tf-ghost" onclick={closeMonitor}>Close</button>
      </header>
      <div class="tf-mon-pstate">
        <TfStateChip text={chip.label} {tone} />
        {#if toggled !== null}<TfStateChip text={toggled.state} tone="info" class="tf-queued-mark" />{/if}
        {#if chip.since}<span class="tf-mon-since">{chip.since}</span>{/if}
        {#if panel.hidden}<span class="tf-dim">Hidden: its alerts reach no one.</span>{/if}
      </div>
      <div class="tf-mon-pbar">
        <span class="tf-mbar tf-mbar-big" role="img" aria-label="Last 24 hours of {panel.name}" {@attach paintStrip(barsOf(segments(panel), false), false, panel.hidden, true)}></span>
        <span class="tf-mbar-axis"><span>24 h ago</span><span>12 h</span><span>now</span></span>
      </div>
      <dl class="tf-mon-facts">
        <dt>Checks</dt>
        <dd>{kindWord(panel.monitor_type) || "A check"}{#if panel.target}{" "}<span class="tf-mono">{panel.target}</span>{/if}</dd>
        <dt>Response</dt>
        <dd>{responseText(panel.response_time_ms)}</dd>
        <dt>Uptime</dt>
        <dd>{#if panel.uptime.length === 0}—{:else}{#each panel.uptime as u, at (u.window)}{#if at > 0}{", "}{/if}{Number((u.ratio * 100).toFixed(2))} % over {WINDOW_WORDS[u.window] ?? u.window}{/each}{/if}</dd>
        {#if panel.cert_days_remaining !== null}
          <dt>Certificate</dt>
          <dd>Expires in {counted(Math.floor(panel.cert_days_remaining), "day", "days")}</dd>
        {/if}
        <dt>Watches</dt>
        <dd class="tf-mon-pwatch">
          <span>{#each panel.assets as asset, at (asset.id)}{#if at > 0}{", "}{/if}<a href="#/asset/{encodeURIComponent(asset.id)}">{asset.name}</a> <span class="tf-dim">{asset.type_label}</span>{:else}<span class="tf-dim">Nothing yet.</span>{/each}</span>
          <!-- Both always drawn, so a link that lands moves neither: a monitor may watch more than one asset. -->
          <span class="tf-mon-pwacts"
            ><button type="button" class="tf-btn tf-sm" onclick={() => (linkingFor = panel)}>Link an asset…</button
            ><button type="button" class="tf-btn tf-sm" onclick={() => void openNewAsset(panel)}>New asset for it…</button></span
          >
        </dd>
        <dt>Read</dt>
        <dd>{ago(panel.checked_at, now)} from {sourceNames[panel.source_id] ?? panel.source_id}</dd>
      </dl>
      <div class="tf-mon-pacts">
        {#if down}
          <button type="button" class="tf-btn tf-sm" aria-pressed={panel.entity_id in acked} onclick={() => void (panel.entity_id in acked ? unack(panel) : ack(panel))}>{panel.entity_id in acked ? "Acked" : "Ack"}</button>
        {/if}
        <!-- Unkeyed: when the read turns *Pause* into *Resume* it is one button, so focus stays on it. -->
        {#each panel.actions.filter((op) => op in ROW_OPS) as op}
          <button type="button" class="tf-btn tf-sm" aria-disabled={toggled !== null ? "true" : undefined} title={toggled !== null ? "On its way. It settles when the roster reads the new state." : undefined} onclick={() => void toggle(panel, op)}>{ROW_OPS[op]!.label}</button>
        {/each}
        <button
          type="button"
          class="tf-btn tf-sm"
          disabled={pressing[panel.entity_id] === true}
          title={panel.hidden ? "Show it on the roster and let its alerts reach the inbox again" : "Move it to the hidden monitors; its alerts stop reaching the inbox and stop notifying"}
          onclick={() => void setHidden(panel, !panel.hidden)}>{panel.hidden ? "Unhide" : "Hide"}</button
        >
        {#if panelRefresh}<TfDetailRefresh detail={subjectOf(panel)} refresh={panelRefresh} {reread} small />{/if}
        <!-- Named after the monitor's own source, as Settings › Sources names it, never a product (the build-header pass,
             2026-11-20: two monitoring sources, or one an installed adapter fills, are not "Uptime Kuma"). -->
        {#if panel.web_url}<a class="tf-btn tf-sm tf-ghost" href={panel.web_url} target="_blank" rel="noreferrer" title="Open in {sourceNames[panel.source_id] || panel.source_id}"><span class="tf-cut">Open in {sourceNames[panel.source_id] || panel.source_id}</span></a>{/if}
      </div>
      {#if panelRefresh?.failed}<div class="tf-banner tf-err tf-refresh-failed" role="alert">{panelRefresh.failed}</div>{/if}
    </aside>
  {/if}
</div>
{#if linkingFor !== null}
  {@const monitor = linkingFor}
  <LinkAssetDialog
    {monitor}
    watched={new Set(monitor.assets.map((asset) => asset.id))}
    tree={treeOf}
    link={(assetId) => linkTo(assetId, monitor.entity_id)}
    onclose={() => (linkingFor = null)}
    onlinked={(asset) => linked(monitor, asset)}
    oncreate={() => void openNewAsset(monitor)}
  />
{/if}
{#if newFor !== null}
  {@const monitor = newFor.monitor}
  <CreateDialog
    at={{ id: null, parent: null }}
    types={newFor.types}
    create={(typeId, name, parentId) => (ports.createAsset ?? createAsset)(typeId, name, parentId)}
    start={{ ...assetFor(monitor), note: `watched by ${monitor.name}` }}
    onclose={() => (newFor = null)}
    oncreated={(asset) => void madeFor(monitor, asset)}
  />
{/if}
{#if creating !== null}
  <TfCreateMonitor
    subject={creating}
    record={(name) => recordName(creating!, name)}
    queue={ports.submitWrite}
    onclose={() => (creating = null)}
    oncreated={created}
  />
{/if}

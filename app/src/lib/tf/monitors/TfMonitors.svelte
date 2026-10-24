<!--
  Monitors (#864, R-11.3 to R-11.5): `#/monitors` and `#/monitors/unmonitored`,
  one page with two tabs as prototype-6's `renderMonitors(tab)`.

  - **Monitors**: the filter bar (text over monitor, asset and type; *This
    job / My jobs / All*; *Down / Warning / Up / Paused*; *Type*), then the
    roster sorted down, warning, paused, up. Each row: the monitor and its
    type, the state chip, the 24-hour bar, the response, the 30-day uptime,
    what it **Watches** (the asset's name and type, linking to the asset),
    *Ack* on a down monitor, and *Pause* or *Resume* where the source offers
    it.
  - **Assets nothing watches**: containers, services, routes, databases and VMs
    no monitor watches, each with *Create monitor* (`TfCreateMonitor.svelte`).
    A row whose create is on its way draws *Monitor queued* instead (#1959):
    the row stays listed until the poll mirrors the monitor and attaches it,
    and Uptime Kuma takes a second monitor of the same name. The read names
    the create (`monitor_on_its_way`), so a reload draws it too. A pending one
    is followed: held or refused, it gives *Create monitor* back; sent or
    withdrawn, the list is read again. While any row is on its way, the end of
    every sync reads it again, which is when a sent one's monitor arrives.

  Over `monitor_roster`, `open_alerts` (only for each down row's `acked_at`, so
  an ack outlives the visit; the tab draws no alert of its own, #1296), `ack_alert`, `submit_write` (`PauseMonitor`,
  `ResumeMonitor`, `CreateMonitor`), `unmonitored_assets`, `get_asset` and
  `edit_asset` for the create, `list_sources` for the header's source name,
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
  import type { QueuedWrite, WriteOpPayload } from "../../ipc/sources";
  import type { WriteFollower } from "../../shell/follow-write";
  import { counted } from "../counted";
  import type { OpenJobPorts } from "../job/open-jobs";

  /** The reads and writes this page makes, injectable so a test needs no bridge. */
  export interface MonitorPorts extends OpenJobPorts {
    monitorRoster: () => Promise<MonitorRow[]>;
    unmonitoredAssets: () => Promise<UnmonitoredAsset[]>;
    openAlerts: () => Promise<OpenAlert[]>;
    ackAlert: (monitorId: string) => Promise<OpenAlert>;
    submitWrite: (payload: WriteOpPayload) => Promise<QueuedWrite>;
    getAsset: (assetId: string) => Promise<AssetDetail>;
    editAsset: (assetId: string, edits: AssetEdit[]) => Promise<AssetRow>;
    getRoute: (routeId: string) => Promise<RouteDetail>;
    editRoute: (routeId: string, edits: RouteEdit[]) => Promise<RouteRow>;
    /** The bridge's events, for the end of a sync while a create is on its way. */
    listen: (event: string, handler: (event: { payload: unknown }) => void) => Promise<() => void>;
    /** What follows a pending create in the queue. */
    follower: () => WriteFollower;
  }
</script>

<script lang="ts">
  import { listen as tauriListen } from "@tauri-apps/api/event";
  import { onDestroy } from "svelte";

  import { ipcErrorMessage } from "../../ipc";
  import { ackAlert, editAsset, editRoute, getAsset, getRoute, monitorRoster, openAlerts, unmonitoredAssets } from "../../ipc/assets";
  import { getEntity, jobReach, listContexts, submitWrite } from "../../ipc/entity";
  import { listSources } from "../../ipc/sources";
  import { followWrite, writeFollower } from "../../shell/follow-write";
  import { toastQueued, type WriteSource } from "../../shell/queued-toast";
  import { ago } from "../../shell/time";
  import { push } from "../../shell/toasts.svelte";
  import FilterBar from "../filter/FilterBar.svelte";
  import { sessionFilter } from "../filter/filter-state.svelte";
  import type { FilterConfig, FilterScope } from "../filter/filter";
  import Marked from "../filter/Marked.svelte";
  import { listenSynced } from "../builds/pipeline-ports";
  import { jobIndex, readOpenJobs, type OpenJob } from "../job/open-jobs";
  import {
    stateChip,
    gaps,
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

  let {
    ready,
    rest,
    job,
    ports = {
      monitorRoster: () => monitorRoster(),
      unmonitoredAssets: () => unmonitoredAssets(),
      openAlerts: () => openAlerts(),
      ackAlert: (monitorId) => ackAlert(monitorId),
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
    },
    now: fixedNow,
  }: {
    /** Whether the database answers yet: the reads wait for it. */
    ready: boolean;
    /** What follows `monitors` in the address: `unmonitored`, or nothing. */
    rest: string[];
    /** The job the reader was last in, as a ticket key. */
    job: string | null;
    ports?: MonitorPorts;
    /** Injectable clock, for the bars and the header's *synced*. */
    now?: Date;
  } = $props();

  const now = $derived(fixedNow ?? new Date());
  const tab = $derived<"all" | "unmonitored">(rest[0] === "unmonitored" ? "unmonitored" : "all");

  /**
   * The roster as read: the read narrows itself to the sources filling the
   * monitoring role, so nothing here keeps rows by source (#1297, ADR-0029
   * as amended on 2026-10-19).
   */
  let roster = $state<MonitorRow[] | null>(null);
  let unwatched = $state<UnmonitoredAsset[] | null>(null);
  let sourceNames = $state<Record<string, string>>({});
  let failed = $state<string | null>(null);
  let jobs = $state<OpenJob[]>([]);
  /** When each monitor's open alert was acked, by monitor id: the open alerts' `acked_at`, so it outlives the visit (#1461). */
  let acked = $state<Record<string, string>>({});
  /** Rows whose write has not answered: a double click is one press. */
  let pressing = $state<Record<string, true>>({});
  let creating = $state<MonitorSubject | null>(null);
  let opening = $state<string | null>(null);

  /** Bumped to read both lists again. */
  let revision = $state(0);

  $effect(() => {
    if (!ready) return;
    void revision;
    let live = true;
    failed = null;
    const fail = (error: unknown) => {
      if (live) failed = ipcErrorMessage(error);
    };
    ports.monitorRoster().then((rows) => {
      if (live) roster = rows;
    }, fail);
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
      if (live) unwatched = gaps(rows);
    }, fail);
    ports.listSources().then(
      (rows) => {
        if (live) sourceNames = Object.fromEntries(rows.map((row) => [row.id, row.display_name]));
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

  const following: (() => void)[] = [];
  /** The pending creates followed, by write id: one follow each, however often the list is read. */
  const creates = new Map<number, () => void>();
  onDestroy(() => {
    for (const stop of following.splice(0)) stop();
    for (const stop of creates.values()) stop();
    creates.clear();
  });

  // -- a create on its way (#1959) -------------------------------------------

  $effect(() => {
    const pending = new Map<number, UnmonitoredAsset>();
    for (const row of unwatched ?? []) {
      const write = row.monitor_on_its_way;
      if (write?.state === "pending" && !pending.has(write.id)) pending.set(write.id, row);
    }
    for (const [id, stop] of creates) {
      if (pending.has(id)) continue;
      stop();
      creates.delete(id);
    }
    for (const [id, row] of pending) if (!creates.has(id)) creates.set(id, followCreate(id, row));
  });

  /** Follow one pending create: held or refused, the row offers the create again; sent or withdrawn, the list says which. */
  function followCreate(id: number, row: UnmonitoredAsset): () => void {
    const payload = row.monitor_on_its_way?.payload as { CreateMonitor?: { name?: string } } | undefined;
    return followWrite(
      { id },
      {
        name: `Monitor ${payload?.CreateMonitor?.name ?? row.name}`,
        onstop: () => {
          creates.delete(id);
          onItsWay((write) => write?.id === id, null);
          revision += 1;
        },
        onleave: () => {
          creates.delete(id);
          revision += 1;
        },
      },
      ports.follower(),
    );
  }

  /** Set the create on its way of every row `which` picks, as the page knows it before the next read. */
  function onItsWay(which: (write: QueuedWrite | null, row: UnmonitoredAsset) => boolean, write: QueuedWrite | null) {
    if (unwatched === null) return;
    unwatched = unwatched.map((row) => (which(row.monitor_on_its_way, row) ? { ...row, monitor_on_its_way: write } : row));
  }

  const waiting = $derived((unwatched ?? []).some((row) => row.monitor_on_its_way !== null));
  $effect(() => {
    if (!waiting) return;
    return listenSynced(ports, null, () => {}, () => (revision += 1));
  });

  /** What *Monitor queued* says on hover: where the create is. */
  function queuedTitle(write: QueuedWrite): string {
    const source = sourceNames[write.source_id] ?? "the monitoring source";
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
    placeholder: "Filter by monitor, asset or type",
    noun: "monitors",
    one: "monitor",
    scopes: [
      ...(job !== null ? [{ id: "job", label: "This job", tag: job, query: "" } satisfies FilterScope] : []),
      { id: "mine", label: "My jobs", query: "" },
      { id: "all", label: "All", query: "" },
    ],
    chips: [{ key: "st", label: "State", options: STATE_CHIPS.map((value) => ({ value, label: value })) }],
    dropdowns: [{ key: "type", label: "Type", options: types.map((value) => ({ value, label: value })) }],
  });
  const scope = $derived<"job" | "mine" | "all">(
    filter.scope === "job" && job !== null ? "job" : filter.scope === "mine" ? "mine" : "all",
  );
  const rows = $derived(
    roster === null
      ? []
      : visibleMonitors(
          sorted(roster),
          { scope, job, states: filter.chips["st"] ?? [], type: filter.dropdowns["type"] ?? "", text: filter.text },
          index,
        ),
  );

  /** The header's line: how many checks, from which source, synced when. */
  const header = $derived.by(() => {
    if (roster === null) return "Checks from your monitoring source, each watching an asset.";
    const sources = [...new Set(roster.map((row) => sourceNames[row.source_id] ?? row.source_id))];
    const newest = roster.map((row) => row.checked_at).sort().at(-1) ?? null;
    const from = sources.length > 0 ? ` from ${sources.join(" and ")}` : "";
    const synced = newest !== null ? `, synced ${ago(newest, now)}` : "";
    return `${counted(roster.length, "check", "checks")}${from}${synced}. Each one watches an asset. Bars cover the last 24 hours in half-hours.`;
  });

  // -- the row actions -------------------------------------------------------

  async function ack(row: MonitorRow) {
    if (pressing[row.entity_id]) return;
    pressing[row.entity_id] = true;
    try {
      const alert = await ports.ackAlert(row.entity_id);
      acked[row.entity_id] = alert?.acked_at ?? new Date().toISOString();
      push({ text: "Acked in knobas. The alert stays open until the monitor recovers." });
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
    if (!form || pressing[row.entity_id]) return;
    pressing[row.entity_id] = true;
    try {
      const write = await ports.submitWrite(form.build(row.entity_id));
      toastQueued(`${form.label.toLowerCase()} ${row.name}`, { id: row.source_id, display_name: sourceNames[row.source_id] ?? "" });
      following.push(followWrite(write, { name: `${form.label} ${row.name}` }));
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
    toastQueued(`create monitor for ${subject?.name ?? what.name}`, what.source);
    // Drawn at once, and followed by the effect above as the read will name it.
    // The re-read starts after the row was queued, and drops any read before it.
    onItsWay((_, row) => row.id === subject?.id, what.write);
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
    <div class="tf-sub">{header}</div>
  </div>
</header>
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
  <FilterBar {config} {filter} shown={roster === null ? null : rows.length} total={roster?.length ?? null} />
{/if}
<div class="tf-cbody">
  {#if failed !== null}
    <p class="tf-ifail">Could not read the monitors: {failed}</p>
  {/if}
  {#if tab === "unmonitored"}
    <p class="tf-dim tf-mon-lead">Containers, services, routes, databases and VMs that no monitor watches.</p>
    {#if unwatched !== null && unwatched.length > 0}
      <table class="tf-tbl">
        <thead><tr><th>Asset</th><th>Type</th><th>Env</th><th>Owner</th><th><span class="tf-sr">Actions</span></th></tr></thead>
        <tbody>
          {#each unwatched as asset (asset.id)}
            <tr class="tf-row" onclick={(event) => openAsset(event, asset.id)}>
              <td class="tf-mono"><a href={addressOf(asset.id)}>{asset.name}</a></td>
              <td>{asset.type_label}</td>
              <td>{asset.environment ?? ""}</td>
              <td>{asset.owner ?? ""}</td>
              <td class="tf-mon-acts">
                {#if asset.monitor_on_its_way !== null}
                  <span class="tf-chip tf-info" title={queuedTitle(asset.monitor_on_its_way)}>Monitor queued</span>
                {:else}
                  <button type="button" class="tf-btn tf-sm" disabled={opening === asset.id} onclick={() => void openCreate(asset)}
                    >Create monitor</button
                  >
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
    {#if rows.length > 0}
      <table class="tf-tbl tf-mon">
        <thead>
          <tr><th>Monitor</th><th>State</th><th>Last 24 h</th><th>Response</th><th>30 days</th><th>Watches</th><th><span class="tf-sr">Actions</span></th></tr>
        </thead>
        <tbody>
          {#each rows as row (row.entity_id)}
            {@const chip = stateChip(row)}
            {@const down = chip.tone === "err" && !row.tombstoned}
            <tr>
              <td><Marked text={row.name} typed={filter.text} />{#if row.monitor_type}{" "}<span class="tf-dim tf-mon-type">{kindWord(row.monitor_type)}</span>{/if}</td>
              <td><span class="tf-chip" class:tf-ok={chip.tone === "ok"} class:tf-warn={chip.tone === "warn"} class:tf-err={chip.tone === "err"} class:tf-info={chip.tone === "info"}>{chip.label}</span></td>
              <td>
                <span class="tf-mbar" role="img" aria-label="Last 24 hours of {row.name}">
                  {#each segments(row, now.getTime()) as segment, at (at)}<i class="tf-seg-{segment}"></i>{/each}
                </span>
              </td>
              <td class="tf-num">{responseText(row.response_time_ms)}</td>
              <td class="tf-num">{uptimeText(row)}</td>
              <td>
                {#each row.assets as asset (asset.id)}
                  <div><a class="tf-mono tf-mon-asset" href="#/asset/{encodeURIComponent(asset.id)}"><Marked text={asset.name} typed={filter.text} /></a>{" "}<span class="tf-dim tf-mon-type">{asset.type_label}</span></div>
                {:else}
                  <span class="tf-dim">no asset</span>
                {/each}
              </td>
              <td class="tf-mon-acts">
                {#if down}
                  <button type="button" class="tf-btn tf-sm" disabled={row.entity_id in acked} title={row.entity_id in acked ? `Acked ${acked[row.entity_id]}` : undefined} onclick={() => void ack(row)}
                    >{row.entity_id in acked ? "Acked" : "Ack"}</button
                  >
                {/if}
                {#each row.actions.filter((op) => op in ROW_OPS) as op (op)}
                  <button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => void toggle(row, op)}>{ROW_OPS[op]!.label}</button>
                {/each}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    {:else if roster.length === 0}
      <div class="tf-empty">No monitors yet. Add an Uptime Kuma source under Settings › Sources and its checks show here.</div>
    {:else}
      <div class="tf-empty">
        No monitor matches.
        {#if scope === "job"}
          <button type="button" class="tf-btn tf-sm" onclick={() => (filter.scope = "all")}>Search all monitors</button>
        {/if}
      </div>
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

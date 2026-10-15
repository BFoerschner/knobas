<!--
  Settings in the ticket-first shell (#830, R-13a.1, R-13a.2): prototype-6's
  `renderSettings`. The breadcrumb, the sections on the left, and the section
  on the right.

  Sources and a source's page are built here. Every other section names
  itself and offers the rooms UI, which still has it, until #831 and #844
  build them.

  This component owns the reads both Sources pages draw from: the source
  list, each source's sync status and the adapters' descriptors. A finished
  run re-lists, because a row's last sync and item count arrive only with
  `list_sources` (the rooms view's #83); a running one only patches its
  status. Only the newest list read may land (`latestRead`, #107).
-->
<script lang="ts">
  import { ipcErrorMessage, EVENTS } from "../../ipc";
  import type { SourceDescriptor, SourceSummary, SourceSyncStatus } from "../../ipc/sources";
  import { health as sharedHealth, type Health } from "../../shell/health.svelte";
  import { latestRead } from "../../shell/latest-read";
  import { layers as sharedLayers, type Layers } from "../layers.svelte";
  import { jobHash } from "../route";
  import { addSourcePanel } from "./add-source.svelte";
  import { realSettingsPorts, type SettingsPorts } from "./ports";
  import { SETTINGS_SECTIONS, settingsPlace, sourceState } from "./settings";
  import TfSourcePage from "./TfSourcePage.svelte";
  import TfSources from "./TfSources.svelte";

  let {
    rest,
    job,
    ready,
    onrooms,
    ports = realSettingsPorts,
    health = sharedHealth,
    layers = sharedLayers,
    now = () => new Date(),
  }: {
    /** What follows `#/settings` in the address. */
    rest: string[];
    /** The job the reader was last in, for the breadcrumb. */
    job: string | null;
    /** Whether the database answers yet. */
    ready: boolean;
    /** Switch back to the rooms UI, for the sections not built here yet. */
    onrooms: () => void;
    ports?: SettingsPorts;
    health?: Pick<Health, "get" | "patch" | "replace">;
    layers?: Layers;
    now?: () => Date;
  } = $props();

  const place = $derived(settingsPlace(rest));
  const sourceId = $derived("source" in place ? place.source : null);

  let sources = $state<SourceSummary[] | null>(null);
  let error = $state<string | null>(null);
  let adapters = $state<SourceDescriptor[]>([]);
  let statuses = $state<Record<string, SourceSyncStatus>>({});
  /** Moves on every finished run, so a source's page re-reads its runs. */
  let finished = $state(0);
  let clock = $state(new Date());
  $effect(() => {
    clock = now();
  });

  const read = latestRead<SourceSummary[]>();
  function load() {
    return read(ports.listSources, {
      ok: (rows) => {
        sources = rows;
        error = null;
        health.replace(rows.map((row) => row.health));
      },
      fail: (cause) => (error = ipcErrorMessage(cause)),
    });
  }

  $effect(() => {
    if (!ready) return;
    let dead = false;
    const offs: (() => void)[] = [];
    void load();
    void ports
      .listAdapters()
      .then((rows) => {
        if (!dead) adapters = rows;
      })
      .catch(() => {});
    void ports
      .syncStatus()
      .then((rows) => {
        if (dead) return;
        const seeded = Object.fromEntries(rows.map((row) => [row.source_id, row]));
        statuses = { ...seeded, ...statuses };
      })
      .catch(() => {});
    void ports
      .listen(EVENTS.syncState, ({ payload }) => {
        const row = payload as SourceSyncStatus;
        statuses = { ...statuses, [row.source_id]: row };
        clock = now();
        if (!row.running) {
          finished += 1;
          void load();
        }
      })
      .then((off) => (dead ? off() : offs.push(off)))
      .catch(() => {});
    const tick = setInterval(() => (clock = now()), 30_000);
    return () => {
      dead = true;
      clearInterval(tick);
      for (const off of offs.splice(0)) off();
    };
  });

  // A source the Add-source dialog just added is a new row.
  let seenAdded = addSourcePanel.added;
  $effect(() => {
    if (addSourcePanel.added === seenAdded) return;
    seenAdded = addSourcePanel.added;
    void load();
  });

  /** A locally started run, before its first `sync:state` arrives. */
  function markStarted(id: string) {
    const had = statuses[id];
    statuses = {
      ...statuses,
      [id]: {
        source_id: id,
        running: true,
        run_id: null,
        started_at: now().toISOString(),
        last_finished_at: had?.last_finished_at ?? null,
        last_outcome: had?.last_outcome ?? null,
        next_run_at: null,
        backoff_until: null,
        last_changed: had?.last_changed ?? null,
      },
    };
  }

  function stopped(id: string) {
    const had = statuses[id];
    if (had) statuses = { ...statuses, [id]: { ...had, running: false } };
  }

  function replaceRow(next: SourceSummary) {
    if (sources) sources = sources.map((row) => (row.id === next.id ? next : row));
    health.patch(next.health);
  }

  const anyFailing = $derived(
    (sources ?? []).some((source) => sourceState(source, health.get(source.id), null).failed),
  );
  const current = $derived(sourceId === null ? null : (sources?.find((row) => row.id === sourceId) ?? null));
  const sectionName = $derived(SETTINGS_SECTIONS.find(([key]) => key === place.section)?.[1] ?? "Sources");
  const crumbName = $derived(sourceId === null ? sectionName : (current?.display_name ?? sourceId));
</script>

<div class="tf-full">
  <nav class="tf-crumbs" aria-label="Breadcrumb">
    {#if job !== null}
      <a class="tf-jobchip tf-card" href={jobHash(job)}>{job}</a><span class="tf-sep">›</span>
    {/if}
    <a href="#/settings">Settings</a><span class="tf-sep">›</span>
    <strong>{crumbName}</strong>
    <span class="tf-esc">{job !== null ? `Back to ${job}` : "Back"} <kbd>Esc</kbd></span>
  </nav>
  <div class="tf-stg">
    <nav class="tf-stg-nav" aria-label="Settings">
      {#each SETTINGS_SECTIONS as [key, label] (key)}
        <a href="#/settings/{key}" aria-current={key === place.section ? "page" : undefined}>
          {label}
          {#if key === "sources" && anyFailing}
            <span class="tf-dot tf-err" title="A source needs you"></span>
          {/if}
        </a>
      {/each}
    </nav>
    <div class="tf-stg-body">
      {#if place.section !== "sources"}
        <header class="tf-stg-h"><h1>{sectionName}</h1></header>
        <p class="tf-stg-p">{sectionName} is not built in the new shell yet. The rooms UI has every setting today.</p>
        <div class="tf-stg-acts">
          <button class="tf-btn tf-primary" onclick={onrooms}>Switch back to the rooms UI</button>
        </div>
      {:else if error !== null && sources === null}
        <header class="tf-stg-h"><h1>Sources</h1></header>
        <p class="tf-stg-p tf-stg-errt">Could not read the sources: {error}</p>
        <div class="tf-stg-acts"><button class="tf-btn" onclick={() => void load()}>Try again</button></div>
      {:else if sources === null}
        <header class="tf-stg-h"><h1>Sources</h1></header>
      {:else if sourceId === null}
        <TfSources
          {sources}
          {statuses}
          {health}
          now={clock}
          {ports}
          onstarted={(ids) => ids.forEach(markStarted)}
          onstopped={(ids) => ids.forEach(stopped)}
        />
      {:else if current === null}
        <header class="tf-stg-h"><h1>{sourceId}</h1></header>
        <p class="tf-stg-p">There is no source called {sourceId}. It may have been removed.</p>
        <div class="tf-stg-acts"><a class="tf-btn" href="#/settings">All sources</a></div>
      {:else}
        {#key current.id}
          <TfSourcePage
            source={current}
            adapter={adapters.find((row) => row.adapter_kind === current.adapter_kind) ?? null}
            health={health.get(current.id)}
            status={statuses[current.id] ?? null}
            {finished}
            now={clock}
            {ports}
            {layers}
            onhealth={(next) => health.patch(next)}
            onchanged={replaceRow}
            onstarted={() => markStarted(current.id)}
            onstopped={() => stopped(current.id)}
            onremoved={() => {
              location.hash = "#/settings";
              void load();
            }}
          />
        {/key}
      {/if}
    </div>
  </div>
</div>

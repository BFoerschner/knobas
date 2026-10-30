<!--
  Settings in the ticket-first shell (#830, R-13a.1, R-13a.2): prototype-6's
  `renderSettings`. The breadcrumb, the sections on the left, and the section
  on the right.

  Sources and a source's page are built here (#830); Time, Notifications,
  Standup, Monitoring, Backup and export, Appearance, Keyboard, and Logs and
  database each draw from their own component (#831), Checkouts and Open in…
  too (#844).

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
  import { push } from "../../shell/toasts.svelte";
  import TfAddSource from "./TfAddSource.svelte";
  import { notifications as sharedNotifications, type Notifications } from "../../inbox/notify.svelte";
  import { theme as sharedTheme, type Theme } from "../theme.svelte";
  import { realSectionPorts, realSettingsPorts, type SectionPorts, type SettingsPorts } from "./ports";
  import { SETTINGS_SECTIONS, settingsHash, settingsPlace, sourceState } from "./settings";
    import TfSourcePage from "./TfSourcePage.svelte";
  import TfSources from "./TfSources.svelte";
  import TfInstalledAdapters from "./TfInstalledAdapters.svelte";
  import TfAppearanceSection from "./TfAppearanceSection.svelte";
  import TfBackupSection from "./TfBackupSection.svelte";
  import TfCheckoutsSection from "./TfCheckoutsSection.svelte";
  import TfKeyboardSection from "./TfKeyboardSection.svelte";
  import TfLogsSection from "./TfLogsSection.svelte";
  import TfMonitoringSection from "./TfMonitoringSection.svelte";
  import TfNotesSection from "./TfNotesSection.svelte";
  import TfNotificationsSection from "./TfNotificationsSection.svelte";
  import TfStandupSection from "./TfStandupSection.svelte";
  import TfTimeSection from "./TfTimeSection.svelte";
  import TfPerformanceSection from "./TfPerformanceSection.svelte";

  let {
    rest,
    job,
    jobAt = null,
    ready,
    ports = realSettingsPorts,
    sectionPorts = realSectionPorts,
    theme = sharedTheme,
    notifications = sharedNotifications,
    health = sharedHealth,
    layers = sharedLayers,
    now = () => new Date(),
  }: {
    /** What follows `#/settings` in the address. */
    rest: string[];
    /** The job the reader was last in, for the breadcrumb. */
    job: string | null;
    /** The address the job was opened at (#1525), which its chip links; its key when omitted. */
    jobAt?: string | null;
    /** Whether the database answers yet. */
    ready: boolean;
    ports?: SettingsPorts;
    /** The other sections' commands (#831). */
    sectionPorts?: SectionPorts;
    /** The colour theme, the top bar's toggle's own (#831). */
    theme?: Theme;
    /** Notifications' store: the inbox notifier's own, so the switch is the one that fires. */
    notifications?: Pick<Notifications, "kinds" | "loaded" | "permission" | "error" | "busy" | "reseed" | "choose">;
    health?: Pick<Health, "get" | "patch" | "replace">;
    layers?: Layers;
    now?: () => Date;
  } = $props();

  /** The sections #831 draws whose first paint waits on a read. */
  const SECTIONS_THAT_READ: ReadonlySet<string> = new Set(["time", "notifications", "standup", "monitoring", "notes", "backup", "logs", "checkouts", "performance"]);

  const place = $derived(settingsPlace(rest));
  const sourceId = $derived("source" in place ? place.source : null);

  let sources = $state<SourceSummary[] | null>(null);
  let error = $state<string | null>(null);
  let adapters = $state<SourceDescriptor[]>([]);
  let statuses = $state<Record<string, SourceSyncStatus>>({});
  /** Moves when *Add a source* closes: an install there adds an adapter row. */
  let installs = $state(0);
  /** Moves on every finished run, so a source's page re-reads its runs. */
  let finished = $state(0);
  // svelte-ignore state_referenced_locally
  let clock = $state(now());

  const readAdapters = latestRead<SourceDescriptor[]>();
  /**
   * The descriptors the source pages are handed, read again whenever an
   * adapter is installed, updated or uninstalled (#1406): a page handed `null`
   * draws no config form and the bare kind for its adapter.
   */
  function loadAdapters() {
    return readAdapters(ports.listAdapters, { ok: (rows) => (adapters = rows), fail: () => {} });
  }

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
    void loadAdapters();
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

  /** Whether *Add a source* is open (#903); a source it adds is a new row. */
  let adding = $state(false);

  /** *Add a source* closed: an install there adds an Installed adapters row and a descriptor. */
  function closeAdd() {
    adding = false;
    installs += 1;
    void loadAdapters();
  }

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
      <a class="tf-jobchip tf-card" href={jobHash(jobAt ?? job)}>{job}</a><span class="tf-sep">›</span>
    {/if}
    <a class="tf-crumb" href="#/settings">Settings</a><span class="tf-sep">›</span>
    <strong>{crumbName}</strong>
    <span class="tf-esc">{job !== null ? `Back to ${job}` : "Back"} <kbd>Esc</kbd></span>
  </nav>
  <div class="tf-stg">
    <nav class="tf-stg-nav" aria-label="Settings">
      {#each SETTINGS_SECTIONS as [key, label] (key)}
        <a href={settingsHash(key)} aria-current={key === place.section ? "page" : undefined}>
          {label}
          {#if key === "sources" && anyFailing}
            <span class="tf-dot tf-err" role="img" aria-label="A source needs you" title="A source needs you"></span>
          {/if}
        </a>
      {/each}
    </nav>
    <div class="tf-stg-body">
      {#if place.section === "sources" && error !== null && sources !== null}
        <!-- A re-read that failed under a list already drawn: the list stays, and says it may be old. -->
        <p class="tf-stg-p tf-stg-errt" role="status">Could not read the sources again: {error}</p>
      {/if}
      {#if !ready && place.section !== "sources" && SECTIONS_THAT_READ.has(place.section)}
        <!-- Their reads wait for the database, as the source list does. -->
        <header class="tf-stg-h"><h1>{sectionName}</h1></header>
      {:else if place.section === "time"}
        <TfTimeSection ports={sectionPorts} />
      {:else if place.section === "notifications"}
        <TfNotificationsSection ports={sectionPorts} store={notifications} />
      {:else if place.section === "standup"}
        <TfStandupSection ports={sectionPorts} {layers} />
      {:else if place.section === "monitoring"}
        <TfMonitoringSection ports={sectionPorts} />
      {:else if place.section === "backup"}
        <TfBackupSection ports={sectionPorts} {layers} now={clock} onrestored={() => void load()} />
      {:else if place.section === "appearance"}
        <TfAppearanceSection {theme} />
      {:else if place.section === "performance"}
        <TfPerformanceSection ports={sectionPorts} />
      {:else if place.section === "keyboard"}
        <TfKeyboardSection />
      {:else if place.section === "logs"}
        <TfLogsSection ports={sectionPorts} />
      {:else if place.section === "checkouts"}
        <TfCheckoutsSection {layers} />
      {:else if place.section === "notes"}
        <TfNotesSection {layers} />
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
          onadd={() => (adding = true)}
        />
        <TfInstalledAdapters
          {ports}
          {layers}
          refresh={installs}
          onremoved={() => void load()}
          onchanged={() => void loadAdapters()}
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
            add={"add" in place ? place.add : undefined}
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
{#if adding}
  <TfAddSource
    {ports}
    {layers}
    onclose={closeAdd}
    onsaved={() => {
      closeAdd();
      push({ text: "Added. The first sync is running." });
      void load();
    }}
  />
{/if}

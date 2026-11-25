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
  import TfSideEdge from "../TfSideEdge.svelte";
  import { sideWidth } from "../side-width.svelte";
  import { tick } from "svelte";

  import { pageVisible } from "../kept-pages.svelte";
  import TfRailBar from "../TfRailBar.svelte";
  import TfScroller from "../TfScroller.svelte";
  import { everyWhileShown } from "../while-shown.svelte";
  import { ipcErrorMessage, EVENTS } from "../../ipc";
  import type { SourceDescriptor, SourceSummary, SourceSyncStatus } from "../../ipc/sources";
  import { health as sharedHealth, type Health } from "../../shell/health.svelte";
  import { latestRead } from "../../shell/latest-read";
  import { layers as sharedLayers, type Layers } from "../layers.svelte";
  import type { EscTarget } from "../escape";
  import TfCrumbs from "../TfCrumbs.svelte";
  import { push } from "../../shell/toasts.svelte";
  import TfAddSource from "./TfAddSource.svelte";
  import { notifications as sharedNotifications, type Notifications } from "../../inbox/notify.svelte";
  import { theme as sharedTheme, type Theme } from "../theme.svelte";
  import { realSectionPorts, realSettingsPorts, type SectionPorts, type SettingsPorts } from "./ports";
  import { sourceDrafts } from "./source-drafts.svelte";
  import { addsSource, settingsHash, settingsKnows, settingsName, settingsParent, settingsPlace, sourceHash, sourceState } from "./settings";
  import { lifecycle } from "../../shell/lifecycle.svelte";
  import { replaceAddress } from "../opener";
  import TfNothingHere from "../TfNothingHere.svelte";
    import TfSourcePage from "./TfSourcePage.svelte";
  import TfSources from "./TfSources.svelte";
  import TfInstalledAdapters from "./TfInstalledAdapters.svelte";
  import TfAppearanceSection from "./TfAppearanceSection.svelte";
  import TfFontsSection from "./TfFontsSection.svelte";
  import TfThemeSection from "./TfThemeSection.svelte";
  import TfThemesSection from "./TfThemesSection.svelte";
  import TfSettingsNav from "./TfSettingsNav.svelte";
  import TfBackupSection from "./TfBackupSection.svelte";
  import TfCheckoutsSection from "./TfCheckoutsSection.svelte";
  import { appPagePlace } from "../openin/app-page";
  import { apps as sharedApps } from "../openin/apps.svelte";
  import TfAppPage from "../openin/TfAppPage.svelte";
  import TfFolderRendererPage from "./TfFolderRendererPage.svelte";
  import TfCommandsSection from "./TfCommandsSection.svelte";
  import TfCommandPage from "./TfCommandPage.svelte";
  import { commandPagePlace } from "./command-page";
  import { folderPagePlace } from "./folder-page";
  import TfApiServersSection from "./TfApiServersSection.svelte";
  import TfKeyboardSection from "./TfKeyboardSection.svelte";
  import TfLogsSection from "./TfLogsSection.svelte";
  import TfMonitoringSection from "./TfMonitoringSection.svelte";
  import TfDocsSection from "./TfDocsSection.svelte";
  import TfSearchSection from "./TfSearchSection.svelte";
  import TfNotesSection from "./TfNotesSection.svelte";
  import TfNotificationsSection from "./TfNotificationsSection.svelte";
  import TfStandupSection from "./TfStandupSection.svelte";
  import TfTimeSection from "./TfTimeSection.svelte";
  import TfPerformanceSection from "./TfPerformanceSection.svelte";
  import TfRenderersSection from "./TfRenderersSection.svelte";
  import TfUpdatesSection from "./TfUpdatesSection.svelte";
  import { updates } from "../update.svelte";
  import { marked } from "./updates";

  let {
    rest,
    back = null,
    ready,
    ports = realSettingsPorts,
    sectionPorts = realSectionPorts,
    theme = sharedTheme,
    notifications = sharedNotifications,
    health = sharedHealth,
    layers = sharedLayers,
    now = () => new Date(),
    demo = lifecycle.status?.demo ?? false,
  }: {
    /** What follows `#/settings` in the address. */
    rest: string[];
    /** Where `Esc` goes, which the breadcrumb's hint names; `null` where it does nothing (#2228). */
    back?: EscTarget | null;
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
    /** The `--demo` profile (P13): Sources offers the Tidewater dataset while it has no source (ux31). */
    demo?: boolean;
  } = $props();

  /** The sections #831 draws whose first paint waits on a read. */
  const SECTIONS_THAT_READ: ReadonlySet<string> = new Set(["time", "notifications", "standup", "monitoring", "docs", "search", "notes", "backup", "logs", "checkouts", "performance", "renderers"]);

  const place = $derived(settingsPlace(rest));
  const sourceId = $derived("source" in place ? place.source : null);
  /** Checkouts' application page (PR #2479, 2026-11-13), `null` anywhere else. */
  const appAt = $derived(appPagePlace(rest));
  /** A command's page (ADR-0060), `null` anywhere else. */
  const commandAt = $derived(commandPagePlace(rest));
  /** Renderers' folder renderer page (ADR-0044), `null` anywhere else. */
  const folderAt = $derived(folderPagePlace(rest));
  const appName = $derived(
    appAt === null ? null : appAt.app === null ? "Add an application" : (sharedApps.settings?.applications.find((app) => app.id === appAt.app)?.name ?? "An application"),
  );
  /** An address that names neither a section nor a source: the rail stays, so the way back is one click. */
  const known = $derived(settingsKnows(rest));

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
  // Moved every thirty seconds while the page is shown, at once on its return (#2249).
  everyWhileShown(pageVisible(), 30_000, () => (clock = now()));

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
    return () => {
      dead = true;
      for (const off of offs.splice(0)) off();
    };
  });

  /** Whether *Add a source* is open (#903) by its button; a source it adds opens its page (ux31). */
  let adding = $state(false);
  /** `#/settings/sources/add` (ux31): Sources with the dialog already open, where Tickets' empty state sends a fresh profile. */
  const addressAdds = $derived(addsSource(rest));

  /** *Add a source* closed: an install there adds an Installed adapters row and a descriptor. */
  function closeAdd() {
    adding = false;
    // The dialog's address goes with it, in place, so `Esc` or a back step does not open it again.
    if (addsSource(rest)) replaceAddress(settingsHash("sources"));
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

  /** What was typed on a source's page and left there (F-635): the rail says so from any section. */
  const anyDraft = $derived(sources?.some((source) => sourceDrafts.has(source.id)) ?? false);
  const anyFailing = $derived(
    (sources ?? []).some((source) => sourceState(source, health.get(source.id), null).failed),
  );
  const current = $derived(sourceId === null ? null : (sources?.find((row) => row.id === sourceId) ?? null));
  const sectionName = $derived(settingsName(place.section));
  /** The section a child's page sits under: its crumb links to the parent's page. */
  const parentSection = $derived(known && sourceId === null ? settingsParent(place.section) : null);
  /** `#/settings` itself: the *Settings* crumb would lead back to this page. */
  const bareSettings = $derived(rest.length === 0);
  const crumbName = $derived(!known ? "Nothing here" : sourceId === null ? sectionName : (current?.display_name ?? sourceId));
  /** Below 900 px, whether the sections rail is unfolded under its bar. */
  let railOpen = $state(false);
  /** Which section or source is drawn: only a move between them starts the column at its top, not a dialog's own address. */
  const shownPlace = $derived(`${place.section}/${sourceId ?? ""}/${appAt === null ? "" : (appAt.app ?? "add")}`);
  /**
   * The section's column is the page's scroller, so moving to another section starts it at its top (F-650). After the
   * flush: `keepScroll` on the kept page sets back what it noted before the move, and its effect runs after this one, which
   * a section drawn at once showed as a page opened part way down.
   */
  function atTopOnMove(scroller: HTMLElement) {
    void shownPlace;
    void tick().then(() => (scroller.scrollTop = 0));
  }
</script>

<div class="tf-full tf-stgpage">
  <TfCrumbs
    parts={[
      bareSettings ? { name: "Settings" } : { name: "Settings", href: "#/settings" },
      ...(appName !== null && known
        ? [{ name: sectionName, href: settingsHash("checkouts") }, { name: appName }]
        : sourceId === null
          ? [...(parentSection === null ? [] : [{ name: settingsName(parentSection), href: settingsHash(parentSection) }]), { name: crumbName }]
          : [{ name: "Sources", href: "#/settings/sources" }, { name: crumbName }]),
    ]}
    {back}
  />
  <TfRailBar label="Section" value={known ? sectionName : "None"} open={railOpen} controls="tf-settings-rail" onpress={() => (railOpen = !railOpen)} onclose={() => (railOpen = false)} {layers} />
  <div class="tf-stg" class:tf-rail-shut={!railOpen} {@attach sideWidth("settings")}>
    <TfSideEdge side="settings" />
    <nav id="tf-settings-rail" class="tf-stg-nav tf-rail" aria-label="Settings">
      <TfSettingsNav current={known ? place.section : null} onpage={() => (railOpen = false)}>
        {#snippet badge(key)}
          {#if key === "sources" && anyFailing}
            <span class="tf-dot tf-err" role="img" aria-label="A source needs you" title="A source needs you"></span>
          {:else if key === "sources" && anyDraft}
            <span class="tf-dot tf-warn" role="img" aria-label="A source has changes not saved" title="A source has changes not saved"></span>
          {:else if key === "updates" && marked(updates.state)}
            <span class="tf-dot tf-info" role="img" aria-label="An update is waiting" title="An update is waiting"></span>
          {/if}
        {/snippet}
      </TfSettingsNav>
    </nav>
    <TfScroller class="tf-stg-scroll" {@attach atTopOnMove}>
    <div class="tf-stg-body">
      {#if !known}
        <header class="tf-stg-h"><h1>Settings</h1></header>
        <TfNothingHere message="Settings has no such section." href={settingsHash("sources")} action="Sources" />
      {:else if place.section === "sources" && error !== null && sources !== null}
        <!-- A re-read that failed under a list already drawn: the list stays, and says it may be old. -->
        <p class="tf-stg-p tf-stg-errt" role="status">Could not read the sources again: {error}</p>
      {/if}
      {#if !known}
        <!-- Said above. -->
      {:else if !ready && place.section !== "sources" && SECTIONS_THAT_READ.has(place.section)}
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
      {:else if place.section === "docs"}
        <TfDocsSection {...sectionPorts.docsOutline ? { outline: sectionPorts.docsOutline } : {}} />
      {:else if place.section === "search"}
        <TfSearchSection {...sectionPorts.searchSettings ? { read: sectionPorts.searchSettings } : {}} />
      {:else if place.section === "backup"}
        <TfBackupSection ports={sectionPorts} {layers} now={clock} onrestored={() => void load()} />
      {:else if place.section === "appearance"}
        <TfAppearanceSection {theme} {ready} ports={sectionPorts} />
      {:else if place.section === "theme"}
        <TfThemeSection mode={theme} />
      {:else if place.section === "code-colours"}
        <TfThemesSection mode={theme} />
      {:else if place.section === "fonts"}
        <TfFontsSection {...sectionPorts.installedFonts ? { installedFonts: sectionPorts.installedFonts } : {}} />
      {:else if place.section === "performance"}
        <TfPerformanceSection ports={sectionPorts} />
      {:else if place.section === "renderers" && folderAt !== null}
        {#key rest.join("/")}
          <TfFolderRendererPage at={folderAt} {back} />
        {/key}
      {:else if place.section === "renderers"}
        <TfRenderersSection {...sectionPorts.renderers ? { ports: sectionPorts.renderers } : {}} />
      {:else if place.section === "api-servers"}
        <TfApiServersSection />
      {:else if place.section === "keyboard"}
        <TfKeyboardSection />
      {:else if place.section === "logs"}
        <TfLogsSection ports={sectionPorts} />
      {:else if place.section === "updates"}
        <TfUpdatesSection {layers} />
      {:else if place.section === "commands" && commandAt !== null}
        {#key rest.join("/")}
          <TfCommandPage at={commandAt} {back} />
        {/key}
      {:else if place.section === "commands"}
        <TfCommandsSection />
      {:else if place.section === "checkouts" && appAt !== null}
        {#key rest.join("/")}
          <TfAppPage at={appAt} {back} />
        {/key}
      {:else if place.section === "checkouts"}
        <TfCheckoutsSection {layers} />
      {:else if place.section === "notes"}
        <TfNotesSection {layers} />
      {:else if error !== null && sources === null}
        <header class="tf-stg-h tf-stg-hline">
          <h1>Sources</h1>
          <div class="tf-acts"><button class="tf-btn" onclick={() => void load()}>Try again</button></div>
        </header>
        <p class="tf-stg-p tf-stg-errt">Could not read the sources: {error}</p>
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
          {demo}
          onloaded={() => void load()}
        />
        <TfInstalledAdapters
          {ports}
          {layers}
          refresh={installs}
          onremoved={() => void load()}
          onchanged={() => void loadAdapters()}
        />
      {:else if current === null}
        <header class="tf-stg-h"><h1>Settings</h1></header>
        <TfNothingHere message="Settings has no such source. It may have been removed." href={settingsHash("sources")} action="Sources" />
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
            reenter={"reenter" in place && place.reenter === true}
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
    </TfScroller>
  </div>
</div>
{#if adding || (addressAdds && sources !== null)}
  <TfAddSource
    {ports}
    {layers}
    taken={(sources ?? []).map((source) => source.id)}
    onclose={closeAdd}
    onsaved={(added) => {
      adding = false;
      installs += 1;
      void loadAdapters();
      push({ text: added.sync_by_hand ? `Added ${added.display_name}. It syncs only when you press Sync now.` : "Added. The first sync is running." });
      // The new source's page (ux31): its run's walk draws there as the first sync goes, and a by-hand source's
      // Sync now is there. In place of the add address, so a back step does not open the dialog again.
      if (addsSource(rest)) replaceAddress(sourceHash(added.id));
      else location.hash = sourceHash(added.id);
      void load();
    }}
  />
{/if}

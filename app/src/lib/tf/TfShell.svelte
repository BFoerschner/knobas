<!--
  The ticket-first shell (#809): the frame every view of the redesign is drawn
  in, and since the rooms UI went (#881) the only one.

  It owns the top bar, the status bar, the toasts, the theme, the keyboard and
  the address. Everything that is the whole app's rather than one shell's
  stays in `App.svelte` and reaches it as a callback: the launcher behind
  *Search* and `⌘K`, and the timer verb behind the chip and `⌘T`, which stops
  into the worklog draft (#280).

  **The tokens are on `:root`** since the rooms UI went (#881): `.tf` paints
  its own paper and fills the window. The theme follows the OS through
  `prefers-color-scheme`; the toggle overrides it for the session by setting
  `data-theme` on `<html>`, from the one theme setting Settings › Appearance
  also writes (`theme.svelte.ts`, #831), so the dialogs mounted outside `.tf`
  follow it too.

  **Dialogs ported to this shell arrive through `dialogs`** and are drawn
  inside it: the worklog draft (#826), the ⌘T picker and the ad-hoc dialog
  (#1432), and the ⌘K launcher (#1422).
-->
<script lang="ts" module>
  import type { SourceSummary as Summary, SourceSyncStatus as SyncRow } from "../ipc/sources";

  /** The reads the status bar needs, injectable so a test needs no bridge. */
  export interface TfShellPorts {
    /** The source list: the status bar's names, and each source's role claims (#1196) and misses (#1199). */
    listSources: () => Promise<(Pick<Summary, "id" | "display_name" | "roles" | "role_misses"> & Partial<Pick<Summary, "enabled">>)[]>;
    syncStatus: () => Promise<SyncRow[]>;
    listen: (event: string, handler: (event: { payload: unknown }) => void) => Promise<() => void>;
    /** *Pages kept open* as stored (#2247), read once the database answers; the default stands without it. */
    pagesKeptOpen?: () => Promise<number>;
    /** The command renderers saved in Settings › Renderers (#2262), installed in the registry once the database answers. */
    userRenderers?: () => Promise<import("../ipc/app").UserRenderer[]>;
    /** The default renderer per file pattern (#2263), held once the database answers. */
    rendererDefaults?: () => Promise<import("../ipc/app").RendererDefault[]>;
    /** The named pipelines (#2265), held once the database answers. */
    namedPipelines?: () => Promise<import("../ipc/app").NamedPipeline[]>;
    /** The command steps (#2265), held once the database answers. */
    commandSteps?: () => Promise<import("../ipc/app").CommandStep[]>;
    /** *Run pipelines on diffs* (#2270), held once the database answers; on without it. */
    diffPipelines?: () => Promise<boolean>;
  }
</script>

<script lang="ts">
  import { listen as tauriListen } from "@tauri-apps/api/event";
  import { onMount, type Snippet } from "svelte";

  import { EVENTS } from "../ipc";
  import { commandSteps, diffPipelines, namedPipelines, pagesKeptOpen, rendererDefaults, userRenderers } from "../ipc/app";
  import { diffPipelines as heldDiffPipelines } from "./diff/diff-pipelines.svelte";
  import { rendererDefaults as heldDefaults } from "../render/defaults.svelte";
  import { commandSteps as heldCommandSteps, namedPipelines as heldPipelines } from "../render/pipelines.svelte";
  import { defaultPipeline } from "../render/defaults";
  import { installPipelines } from "./diff/piped";
  import { installCommands } from "../render/command";
  import { listSources, syncStatus, type SourceSummary, type SourceSyncStatus } from "../ipc/sources";
  import { inbox as sharedInbox, type Inbox } from "../inbox/inbox.svelte";
  import { health as sharedHealth, type Health } from "../shell/health.svelte";
  import { historyStep, inTextField } from "../shell/keys";
  import type { Lifecycle } from "../shell/lifecycle.svelte";
  import { assetTargetId, targetReading } from "../shell/timer";
  import { timer as sharedTimer, type Timer } from "../shell/timer.svelte";
  import WriteQueue from "../shell/WriteQueue.svelte";
  import {
    createWriteQueue,
    decisionsIn,
    openWriteQueue,
    writeQueuePanel,
    type WriteQueue as Queue,
  } from "../shell/write-queue.svelte";
  import { badges as sharedBadges, type Badges } from "./badges.svelte";
  import { escStep, escTarget } from "./escape";
  import { keptPages as sharedKeptPages, type KeptPagesSetting } from "./kept-pages.svelte";
  import { layers as sharedLayers, type Layers } from "./layers.svelte";
  import { onReady } from "./on-ready.svelte";
  import { onRefreshKey } from "./page-refresh.svelte";
  import { parseTfHash, sectionOf } from "./route";
  import { theme as sharedTheme, type Theme } from "./theme.svelte";
  import { sourceReadings } from "./status";
  import { sourceRoles } from "./roles.svelte";
  import { entityAddress, legacyAddress } from "./address";
  import { keyOf } from "./job/job";
  import { bridgePorts, type JobPorts } from "./job/ports";
  import { breadcrumbJob, currentJob, linkRequest } from "./job/current.svelte";
  import { chosenJob } from "./job/chosen.svelte";
  import TfPage from "./TfPage.svelte";
  import { roleSyncs as sharedRoleSyncs, type RoleSyncs } from "./role-sync.svelte";
  import TfStatusBar from "./TfStatusBar.svelte";
  import TfToasts from "./TfToasts.svelte";
  import TfTopBar from "./TfTopBar.svelte";
  import "./tf.css";

  let {
    lifecycle,
    onsearch,
    ontimer,
    timer = sharedTimer,
    inbox = sharedInbox,
    health = sharedHealth,
    badges = sharedBadges,
    layers = sharedLayers,
    theme = sharedTheme,
    queue = createWriteQueue(),
    ports = {
      listSources,
      syncStatus,
      listen: (event, handler) => tauriListen(event, handler),
      pagesKeptOpen,
      userRenderers,
      rendererDefaults,
      namedPipelines,
      commandSteps,
      diffPipelines,
    },
    prefersDark = () => window.matchMedia("(prefers-color-scheme: dark)").matches,
    onstarttimer = () => {},
    jobPorts = bridgePorts,
    dialogs,
    onlog,
    timeRevision = 0,
    roleSyncs = sharedRoleSyncs,
    ontimechanged,
    nav = { back: () => history.back(), forward: () => history.forward() },
    kept = sharedKeptPages,
  }: {
    lifecycle: Pick<Lifecycle, "ready">;
    /** Open the launcher. */
    onsearch: () => void;
    /** The timer verb: stop into the worklog draft, or start. Required, as in `TopStrip`. */
    ontimer: (() => void) | undefined;
    timer?: Pick<Timer, "current" | "elapsed">;
    inbox?: Pick<Inbox, "count">;
    health?: Pick<Health, "all">;
    badges?: Badges;
    layers?: Layers;
    /** The colour theme, which Settings › Appearance writes too (#831). */
    theme?: Theme;
    queue?: Queue;
    ports?: TfShellPorts;
    prefersDark?: () => boolean;
    /** Start the timer on a ticket, stopping another into its draft first (#816). */
    onstarttimer?: (entityId: string, title: string) => void;
    /** The job page's reads (#816); the bridge's when omitted. */
    jobPorts?: JobPorts;
    /**
     * Dialogs drawn in this shell's tokens, inside `.tf`: the worklog draft
     * (#826). Each registers itself on `layers` so `Esc` closes it. It is given
     * the sources' run statuses the shell already holds (#1581), so the launcher
     * reads no second copy.
     */
    dialogs?: Snippet<[SyncRow[]]>;
    /** Open the worklog draft for a ticket and day (the Time view's *Log…*). */
    onlog?: (entityId: string, day: string) => void;
    /** Bumped by `App.svelte` when a draft or a dialog changed a day's blocks. */
    timeRevision?: number;
    ontimechanged?: () => void;
    /** Each role view's *Sync now* (#2106); the window's when omitted. */
    roleSyncs?: RoleSyncs;
    /** History's back and forward (#693, #1086); the window's when omitted. */
    nav?: { back: () => void; forward: () => void };
    /** *Pages kept open* (#2247), which the page host reads; the window's when omitted. */
    kept?: KeptPagesSetting;
  } = $props();

  let hash = $state(location.hash);
  const route = $derived(parseTfHash(hash));

  /** Put `next` in place of the address the reader is on, without a step back to it. */
  function replaceHash(next: string) {
    history.replaceState(history.state, "", next);
    // The state itself, not a `hashchange`: on the first pass the listener (an `onMount`) is not there yet.
    hash = next;
  }

  /**
   * An address the rooms UI handed out (#1085) — a notification delivered
   * before it, a bookmark — lands where the new shell keeps that thing.
   * `#/entity/<id>` names no kind, so the entity is read first, once the
   * database answers; an id the mirror does not know stays *Nothing lives at …*.
   */
  $effect(() => {
    if (route.view !== "unknown") return;
    const legacy = legacyAddress(route.hash);
    if (legacy === null) return;
    if ("go" in legacy) {
      replaceHash(legacy.go);
      return;
    }
    if (!lifecycle.ready) return;
    const from = route.hash;
    void jobPorts
      .getEntity(legacy.resolve)
      .then((detail) => {
        if (location.hash === from) replaceHash(entityAddress(detail.row.kind, detail.row.entity_id));
      })
      // Not found, or not readable: the address stays, and the page says so.
      .catch(() => {});
  });

  /**
   * The write queue panel belongs to the page it was opened on (#1474): a new
   * address closes it, so it never sits over the next page or its dialogs.
   * Plain, not state: the effect compares against it and must not re-run on it;
   * seeded as `hash` is, from the window.
   */
  let panelHash = location.hash;
  $effect(() => {
    if (hash === panelHash) return;
    panelHash = hash;
    writeQueuePanel.open = false;
  });

  /**
   * The chosen job (spec #2227, `job/chosen.svelte.ts`), read from the
   * browser storage when the shell mounts, as a restart does. It replaces the
   * "job you were last in" (#2230): the Job tab, the pages' *This job* and
   * the launcher's job elsewhere than on a job page read it. `key` is what
   * the tab names; `entityId` is where every way back to it goes, so a key
   * two sources hold returns to the one that was chosen (#1525).
   */
  chosenJob.load();
  const chosen = $derived(chosenJob.id === null ? null : { key: keyOf(chosenJob.id), entityId: chosenJob.id });

  /**
   * Where `Esc` goes once nothing is open, for the breadcrumb's hint (#2228):
   * the ladder's own rungs, so the hint names what the key does.
   */
  const back = $derived(escTarget(route));

  /** What the timer runs on, as a ticket key; an asset or a label is no job. */
  const timedTicket = $derived.by(() => {
    const target = timer.current?.target ?? null;
    if (target === null || target.kind !== "entity" || assetTargetId(target) !== null) return null;
    return targetReading(target);
  });
  /**
   * The Job tab's job (R-1.2, spec #2227): the chosen job, else the ticket a
   * timer runs on that was not started here (a start chooses its ticket),
   * with its entity id, which `create_block` needs (#1027). Both are ids
   * already, so nothing is read for it.
   */
  const navJobId = $derived.by(() => {
    if (chosen !== null) return chosen;
    const target = timer.current?.target ?? null;
    return timedTicket === null || target === null || target.kind !== "entity" ? null : { key: timedTicket, entityId: target.entity_id };
  });

  /**
   * The job the launcher's *Link to <key>* targets, and ⌘T falls back to
   * (`breadcrumbJob`, spec #2227): the job on screen on the Job page (`null`
   * while it is still being read), otherwise the chosen job. Never a guessed
   * job: with none chosen there is none.
   */
  $effect(() => {
    const onScreen = currentJob.entityId;
    const job = route.view === "job" ? (onScreen === null ? null : { key: keyOf(onScreen), entityId: onScreen }) : chosen;
    breadcrumbJob.set(job);
    return () => breadcrumbJob.set(null);
  });
  // A link asked for a job is the job page's to take; leaving for any other
  // page before it did drops it, so a later visit does not open the dialog.
  $effect(() => {
    if (route.view !== "job") linkRequest.drop();
  });

  // ---- the status bar's reads, once the database can answer ----
  let sources = $state<Pick<SourceSummary, "id" | "display_name">[] | null>(null);
  let sync = $state<SourceSyncStatus[]>([]);
  let now = $state(new Date());
  /** Bumped by an event that asks for the source list again. */
  let sourcesAsked = $state(0);
  const askSources = () => (sourcesAsked += 1);
  /**
   * Which sources the health store holds, and when it changes the bar's list
   * is stale (#2059). Removing a source sends no event: Settings re-lists the
   * sources and hands the store the sources there are now (`Health.replace`) on
   * Remove, Uninstall, Add source and restore alike. A `source:health` for a
   * source the store did not hold changes it too.
   */
  const heldSources = $derived(health.all.map((row) => row.source_id).join("\n"));

  const ready = () => lifecycle.ready;
  // *Pages kept open* as stored (#2247): until it answers, the default holds.
  onReady(ready, () => ports.pagesKeptOpen, (count) => kept.set(count));
  // The command renderers (#2262): until they answer, the built-ins draw everything.
  onReady(ready, () => ports.userRenderers, (saved) => installCommands(saved));
  // The default renderer per file pattern (#2263): until it answers, the registry's order decides.
  onReady(ready, () => ports.rendererDefaults, (list) => heldDefaults.hold(list));

  // Every diff's default pipeline per file (#2270) is the viewer's (#2265): read from the held stores at each ask, so a
  // pipeline edited in Settings, or a default saved from the viewer, reaches the diffs too. Installed once.
  installPipelines({
    of: (path) => {
      const found = defaultPipeline(path, heldDefaults.list, heldPipelines.list);
      return found === null ? null : { name: found.pipeline.name, pattern: found.pattern, steps: found.pipeline.steps };
    },
  });

  // The named pipelines and command steps (#2265): until they answer, no file opens through a default pipeline.
  onReady(ready, () => ports.namedPipelines, (list) => heldPipelines.hold(list));
  onReady(ready, () => ports.commandSteps, (list) => heldCommandSteps.hold(list));
  // *Run pipelines on diffs* (#2270): until it answers, it is on.
  onReady(ready, () => ports.diffPipelines, (on) => heldDiffPipelines.hold(on));

  $effect(() => {
    if (!lifecycle.ready) return;
    void heldSources;
    void sourcesAsked;
    // A read this one overtakes lands nothing: the newer answer stands.
    let dead = false;
    void ports
      .listSources()
      .then((rows) => {
        if (dead) return;
        sources = rows;
        // Which sources answer an optional read of a role (#1208); the mirror narrows the rows (#1300).
        sourceRoles.take(rows);
      })
      .catch(() => {});
    return () => {
      dead = true;
    };
  });

  $effect(() => {
    if (!lifecycle.ready) return;
    let dead = false;
    const offs: (() => void)[] = [];
    const hold = (pending: Promise<() => void>) =>
      void pending.then((off) => (dead ? off() : offs.push(off))).catch(() => {});

    void ports
      .syncStatus()
      .then((rows) => {
        // A row an event already brought is newer than the seed's.
        if (!dead) sync = [...sync, ...rows.filter((row) => !sync.some((had) => had.source_id === row.source_id))];
      })
      .catch(() => {});
    void badges.refresh();
    void queue.refresh();

    hold(
      ports.listen(EVENTS.syncState, ({ payload }) => {
        const row = payload as SourceSyncStatus;
        const was = sync.find((other) => other.source_id === row.source_id);
        sync = [...sync.filter((other) => other.source_id !== row.source_id), row];
        // A finished run may have judged the source's roles (#1199): a role its
        // records missed, or filled again, changes what the surfaces draw.
        if (was?.running === true && !row.running) askSources();
      }),
    );
    // Every queue transition writes an activity line.
    hold(ports.listen(EVENTS.activityNew, () => void queue.refresh()));
    const stopBadges = badges.start();
    const tick = setInterval(() => (now = new Date()), 30_000);

    return () => {
      dead = true;
      clearInterval(tick);
      stopBadges();
      for (const off of offs.splice(0)) off();
    };
  });

  const readings = $derived(sourceReadings(sources, health.all, sync, now));

  // ---- the keyboard ----
  function go(next: string) {
    location.hash = next;
  }

  /** The focused field that types, which `Esc` may clear or leave, or `null`. */
  function focusedField(): HTMLElement | null {
    const el = document.activeElement;
    return el instanceof HTMLElement && inTextField(el) ? el : null;
  }

  /**
   * Whether a dialog is up. History stays put while one is: the dialog asks
   * something about this address, and stepping away would leave it open over
   * another view or close it unanswered (#1086).
   */
  function dialogOpen(): boolean {
    return layers.open.some((layer) => layer.kind === "dialog");
  }

  function onkeydown(event: KeyboardEvent) {
    // ⌘R presses the page's *Refresh* or *Sync now* (#2156), and the webview
    // never reloads; an overlay owns the keyboard, as it does history's keys.
    if (onRefreshKey(event, dialogOpen())) return;
    const travel = historyStep(event);
    if (travel !== null) {
      event.preventDefault();
      if (dialogOpen()) return;
      if (travel === "back") nav.back();
      else nav.forward();
      return;
    }
    const mod = event.metaKey || event.ctrlKey;
    if (mod && event.key.toLowerCase() === "k") {
      event.preventDefault();
      onsearch();
      return;
    }
    if (mod && event.key.toLowerCase() === "t") {
      event.preventDefault();
      ontimer?.();
      return;
    }
    if (mod && event.key === ",") {
      event.preventDefault();
      go("#/settings");
      return;
    }
    if (event.key !== "Escape" || event.defaultPrevented) return;

    const field = focusedField();
    const step = escStep({
      layers: layers.open,
      field:
        field === null
          ? null
          : {
              filter: field.hasAttribute("data-filter"),
              text: field instanceof HTMLInputElement ? field.value : "",
            },
      route,
    });
    if (step.do === "nothing") return;
    event.preventDefault();
    if (step.do === "close") layers.open[step.index]?.close();
    else if (step.do === "clear-filter" && field instanceof HTMLInputElement) {
      field.value = "";
      field.dispatchEvent(new Event("input", { bubbles: true }));
    } else if (step.do === "blur") field?.blur();
    else if (step.do === "go") go(step.hash);
  }

  // A forced theme is on `<html>`, where the tokens are, so everything in the
  // window follows it; none is the OS deciding. Taken off when the shell goes.
  $effect(() => {
    const html = document.documentElement;
    if (theme.choice === "system") html.removeAttribute("data-theme");
    else html.setAttribute("data-theme", theme.choice);
    return () => html.removeAttribute("data-theme");
  });

  /**
   * The mouse's back (3) and forward (4) buttons (#693). Navigated on
   * `mouseup`, the moment a browser does; every event of the press is
   * `preventDefault`ed so the webview adds no navigation or selection of its own.
   */
  function onmouse(event: MouseEvent) {
    if (event.button !== 3 && event.button !== 4) return;
    event.preventDefault();
    if (event.type !== "mouseup" || dialogOpen()) return;
    if (event.button === 3) nav.back();
    else nav.forward();
  }
  const MOUSE = ["mousedown", "mouseup", "auxclick"] as const;

  onMount(() => {
    const onhash = () => (hash = location.hash);
    window.addEventListener("hashchange", onhash);
    window.addEventListener("keydown", onkeydown);
    for (const type of MOUSE) window.addEventListener(type, onmouse);
    return () => {
      window.removeEventListener("hashchange", onhash);
      window.removeEventListener("keydown", onkeydown);
      for (const type of MOUSE) window.removeEventListener(type, onmouse);
    };
  });
</script>

<div class="tf">
  <TfTopBar
    current={sectionOf(route)}
    job={navJobId?.key ?? null}
    jobAt={navJobId?.entityId ?? null}
    {inbox}
    {badges}
    {timer}
    {onsearch}
    {ontimer}
    ontheme={() => theme.toggle(prefersDark())}
  />
  <main class="tf-main" tabindex="-1">
    <TfPage
      {kept}
      ctx={{
        route,
        ready: lifecycle.ready,
        job: chosen?.key ?? null,
        jobAt: chosen?.entityId ?? null,
        back,
        timedTicket,
        navJob: navJobId,
        timer,
        layers,
        theme,
        ontimer,
        onstarttimer,
        jobPorts,
        onlog,
        timeRevision,
        ontimechanged,
        roleSyncs,
      }}
    />
  </main>
  <TfStatusBar {readings} pending={queue.counts.pending} decisions={decisionsIn(queue.counts)} onqueue={openWriteQueue} />
  {@render dialogs?.(sync)}
  {#if writeQueuePanel.open}
    <WriteQueue {queue} {now} {layers} onclose={() => (writeQueuePanel.open = false)} />
  {/if}
  <TfToasts />
</div>

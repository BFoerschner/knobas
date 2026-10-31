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
  import { onMount, untrack, type Snippet } from "svelte";

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
  import { escStep, escTarget, openerName } from "./escape";
  import { keptPages as sharedKeptPages, type KeptPagesSetting } from "./kept-pages.svelte";
  import { layers as sharedLayers, type Layers } from "./layers.svelte";
  import { onReady } from "./on-ready.svelte";
  import { goWithoutOpener, samePage, stamped, stampOf, takeTold, type Opened } from "./opener";
  import { onRefreshKey } from "./page-refresh.svelte";
  import { parseTfHash, sectionOf } from "./route";
  import { theme as sharedTheme, type Theme } from "./theme.svelte";
  import { sourceReadings } from "./status";
  import { sourceRoles } from "./roles.svelte";
  import { entityAddress, legacyAddress } from "./address";
  import { isEntityId, keyOf, resolveTicket } from "./job/job";
  import { bridgePorts, type JobPorts } from "./job/ports";
  import { breadcrumbJob, linkRequest } from "./job/current.svelte";
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

  /**
   * How the entry the reader is on was opened (`opener.ts`, #2229): the page
   * it was opened from, which `Esc` returns to, and the entry before it. The
   * window's first address was opened by nothing.
   */
  let here = $state<Opened>({ opener: null, prev: null });
  history.replaceState(stamped(history.state, { opener: null, prev: null }), "");

  /** Put `next` in place of the address the reader is on, without a step back to it; a rooms address opened nothing. */
  function replaceHash(next: string) {
    here = { opener: null, prev: here.prev };
    history.replaceState(stamped(history.state, $state.snapshot(here)), "", next);
    // The state itself, not a `hashchange`: on the first pass the listener (an `onMount`) is not there yet.
    hash = next;
  }

  let main: HTMLElement | undefined = $state();

  /** The name the shown page's breadcrumb gives the page itself, read as it is left, or `null`. */
  function ownName(): string | null {
    for (const crumbs of main?.querySelectorAll(".tf-crumbs") ?? []) {
      if (crumbs.closest("[hidden]") !== null) continue;
      const own = [...crumbs.querySelectorAll("strong")].at(-1);
      return own?.textContent?.trim() || null;
    }
    return null;
  }

  /**
   * The address moved (#2229). An entry already stamped is one history
   * stepped to, or one replaced in place: it keeps its stamp. A new entry is
   * stamped here, before the page it left is gone: opened by nothing when it
   * was told so (`goWithoutOpener`), the page's own opener on a tab switch,
   * otherwise the page it left.
   */
  function arrive() {
    const now = location.hash;
    const how = takeTold(now);
    const had = stampOf(history.state);
    if (how !== "replace" && had !== null) {
      here = had;
    } else {
      if (how === "unopened") here = { opener: null, prev: hash };
      else if (how === null && samePage(hash, now)) here = { opener: here.opener, prev: hash };
      else if (how === null) here = { opener: { hash, name: openerName(route, ownName()) }, prev: hash };
      history.replaceState(stamped(history.state, $state.snapshot(here)), "");
    }
    hash = now;
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
   * The job the reader was last in: the Job tab's and the launcher's job
   * (R-1.2). Only a job actually opened, as prototype-6's `S.job`. Neither
   * the breadcrumb nor `Esc` reads it (#2228). `key` is what the tab names;
   * `segment` is the address
   * it was opened at, a bare key or an entity id (#1476), which every way
   * back to it uses, so a key two sources hold returns to the one the
   * reader was on (#1525). Kept, not replaced, while the segment stays: a tab
   * switch is a new route, and a new object would re-run every read keyed on
   * it, the nav job's among them (#1987).
   */
  let lastJob = $state<{ key: string; segment: string } | null>(null);
  $effect(() => {
    if (route.view !== "job" || route.key === null) return;
    const segment = route.key;
    if (untrack(() => lastJob?.segment) === segment) return;
    lastJob = { key: isEntityId(segment) ? keyOf(segment) : segment, segment };
  });

  /**
   * Where `Esc` goes once nothing is open, for the breadcrumb's hint (#2228):
   * the ladder's own rungs, so the hint names what the key does.
   */
  const back = $derived(escTarget(route, here.opener));

  /** What the timer runs on, as a ticket key; an asset or a label is no job. */
  const timedTicket = $derived.by(() => {
    const target = timer.current?.target ?? null;
    if (target === null || target.kind !== "entity" || assetTargetId(target) !== null) return null;
    return targetReading(target);
  });
  /** The Job tab's job: the last one opened, else the timer's ticket (R-1.2). */
  const navJob = $derived(lastJob?.key ?? timedTicket);
  /** Where the Job tab goes: the address the last job was opened at, else the timer's ticket by its id. */
  const navJobAt = $derived.by(() => {
    if (lastJob !== null) return lastJob.segment;
    const target = timer.current?.target ?? null;
    return timedTicket === null || target === null || target.kind !== "entity" ? null : target.entity_id;
  });

  /**
   * The current job as an entity id, which `create_block` needs (#1027). The
   * timer's ticket is already one; the last job opened is read at the address
   * it was opened at, as the job page reads it (#1525): an id as itself, a
   * bare key under every source. `null` while it is not known.
   */
  let navJobId = $state<{ key: string; entityId: string } | null>(null);
  /** The address `navJobId` was read for; plain, so the effect does not re-run on it. */
  let navJobRead: string | null = null;
  $effect(() => {
    const key = navJob;
    const segment = lastJob?.segment ?? null;
    // The timer is read only with no job opened: a start or stop must not re-run a read for a segment already held (#1687).
    const timed = segment === null ? (timer.current?.target ?? null) : null;
    if (key === null || !lifecycle.ready) {
      navJobId = null;
      navJobRead = null;
      return;
    }
    if (segment === null && timed !== null && timed.kind === "entity") {
      navJobId = { key, entityId: timed.entity_id };
      navJobRead = null;
      return;
    }
    // Another source's ticket under the same key is another job: never answer with the last one's id.
    if (segment !== navJobRead) navJobId = null;
    navJobRead = segment;
    let stale = false;
    void jobPorts
      .listSources()
      .then((sources) => resolveTicket(segment ?? key, sources, jobPorts.getEntity))
      .then((found) => {
        if (!stale) navJobId = found === null ? null : { key, entityId: found.row.entity_id };
      })
      .catch(() => {
        if (!stale) navJobId = null;
      });
    return () => {
      stale = true;
    };
  });

  /**
   * The job a page stands for (#1500), for the launcher's *Link to <key>*:
   * the job page's, or the last job on a view opened from it. Tickets, Inbox
   * and Settings are no job's.
   */
  const crumbKey = $derived.by(() => {
    if (route.view === "job") return route.key === null ? null : (lastJob?.key ?? null);
    if (route.view === "item") return lastJob?.key ?? null;
    if (route.view === "full" && route.section !== "inbox" && route.section !== "settings") return lastJob?.key ?? null;
    return null;
  });
  $effect(() => {
    const id = navJobId;
    breadcrumbJob.set(crumbKey !== null && id !== null && id.key === crumbKey ? id : null);
    return () => breadcrumbJob.set(null);
  });
  // A link asked for a job is the job page's to take; leaving for any other
  // page before it did drops it, so a later visit does not open the dialog.
  $effect(() => {
    if (route.view !== "job" || route.key === null) linkRequest.drop();
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
      opener: here.opener,
    });
    if (step.do === "nothing") return;
    event.preventDefault();
    if (step.do === "close") layers.open[step.index]?.close();
    else if (step.do === "clear-filter" && field instanceof HTMLInputElement) {
      field.value = "";
      field.dispatchEvent(new Event("input", { bubbles: true }));
    } else if (step.do === "blur") field?.blur();
    // The opener is the entry before: step back to it, so `⌘]` comes forward to this page again.
    else if (step.do === "go" && step.opened && step.hash === here.prev) nav.back();
    else if (step.do === "go") goWithoutOpener(step.hash);
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
    const onhash = () => arrive();
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
    job={navJob}
    jobAt={navJobAt}
    {inbox}
    {badges}
    {timer}
    {onsearch}
    {ontimer}
    ontheme={() => theme.toggle(prefersDark())}
  />
  <main class="tf-main" tabindex="-1" bind:this={main}>
    <TfPage
      {kept}
      ctx={{
        route,
        ready: lifecycle.ready,
        job: lastJob?.key ?? null,
        jobAt: lastJob?.segment ?? null,
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

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

  **Dialogs not yet ported to this shell are mounted outside `.tf`**, the
  write queue and the Add-source dialog (#830) here and the launcher, the
  picker and the ad-hoc dialog in `App.svelte`; they draw with
  `src/unported.css`. Dialogs ported to this shell arrive through `dialogs`
  and are drawn inside it: the worklog draft (#826).
-->
<script lang="ts" module>
  import type { SourceSummary as Summary, SourceSyncStatus as SyncRow } from "../ipc/sources";

  /** The reads the status bar needs, injectable so a test needs no bridge. */
  export interface TfShellPorts {
    listSources: () => Promise<Pick<Summary, "id" | "display_name">[]>;
    syncStatus: () => Promise<SyncRow[]>;
    listen: (event: string, handler: (event: { payload: unknown }) => void) => Promise<() => void>;
  }
</script>

<script lang="ts">
  import { listen as tauriListen } from "@tauri-apps/api/event";
  import { onMount, type Snippet } from "svelte";

  import { EVENTS } from "../ipc";
  import { listSources, syncStatus, type SourceSummary, type SourceSyncStatus } from "../ipc/sources";
  import { inbox as sharedInbox, type Inbox } from "../inbox/inbox.svelte";
  import { health as sharedHealth, type Health } from "../shell/health.svelte";
  import { inTextField } from "../shell/keys";
  import type { Lifecycle } from "../shell/lifecycle.svelte";
  import { assetTargetId, targetReading } from "../shell/timer";
  import { timer as sharedTimer, type Timer } from "../shell/timer.svelte";
  import WriteQueue from "../shell/WriteQueue.svelte";
  import {
    createWriteQueue,
    openWriteQueue,
    writeQueuePanel,
    type WriteQueue as Queue,
  } from "../shell/write-queue.svelte";
  import { badges as sharedBadges, type Badges } from "./badges.svelte";
  import { escStep } from "./escape";
  import { layers as sharedLayers, type Layers } from "./layers.svelte";
  import { parseTfHash, sectionOf } from "./route";
  import { theme as sharedTheme, type Theme } from "./theme.svelte";
  import { sourceReadings } from "./status";
  import { entityAddress, legacyAddress } from "./address";
  import { isEntityId, keyOf, resolveTicket } from "./job/job";
  import { bridgePorts, type JobPorts } from "./job/ports";
  import TfPage from "./TfPage.svelte";
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
    },
    prefersDark = () => window.matchMedia("(prefers-color-scheme: dark)").matches,
    onstarttimer = () => {},
    jobPorts = bridgePorts,
    dialogs,
    onlog,
    timeRevision = 0,
    ontimechanged,
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
     * (#826). Each registers itself on `layers` so `Esc` closes it.
     */
    dialogs?: Snippet;
    /** Open the worklog draft for a ticket and day (the Time view's *Log…*). */
    onlog?: (entityId: string, day: string) => void;
    /** Bumped by `App.svelte` when a draft or a dialog changed a day's blocks. */
    timeRevision?: number;
    ontimechanged?: () => void;
  } = $props();

  let hash = $state(location.hash);
  const route = $derived(parseTfHash(hash));

  /** Put `next` in place of the address the reader is on, without a step back to it. */
  function replaceHash(next: string) {
    history.replaceState(history.state, "", next);
    window.dispatchEvent(new HashChangeEvent("hashchange"));
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
   * The job the reader was last in: the breadcrumb's chip and where `Esc`
   * returns (R-1.6). Only a job actually opened, as prototype-6's `S.job`.
   */
  let lastJob = $state<string | null>(null);
  $effect(() => {
    // The key, also when the address held an id: the tab names a key (R-1.2).
    if (route.view === "job" && route.key !== null) lastJob = isEntityId(route.key) ? keyOf(route.key) : route.key;
  });

  /** What the timer runs on, as a ticket key; an asset or a label is no job. */
  const timedTicket = $derived.by(() => {
    const target = timer.current?.target ?? null;
    if (target === null || target.kind !== "entity" || assetTargetId(target) !== null) return null;
    return targetReading(target);
  });
  /** The Job tab's job: the last one opened, else the timer's ticket (R-1.2). */
  const navJob = $derived(lastJob ?? timedTicket);

  /**
   * The current job as an entity id, which `create_block` needs (#1027). The
   * timer's ticket is already one; the last job opened is a key, looked for
   * under every source as the job page does. `null` while it is not known.
   */
  let navJobId = $state<{ key: string; entityId: string } | null>(null);
  $effect(() => {
    const key = navJob;
    const timed = timer.current?.target ?? null;
    if (key === null || !lifecycle.ready) {
      navJobId = null;
      return;
    }
    if (lastJob === null && timed !== null && timed.kind === "entity") {
      navJobId = { key, entityId: timed.entity_id };
      return;
    }
    let stale = false;
    void jobPorts
      .listSources()
      .then((sources) => resolveTicket(key, sources, jobPorts.getEntity))
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


  // ---- the status bar's reads, once the database can answer ----
  let sources = $state<Pick<SourceSummary, "id" | "display_name">[] | null>(null);
  let sync = $state<SourceSyncStatus[]>([]);
  let now = $state(new Date());

  $effect(() => {
    if (!lifecycle.ready) return;
    let dead = false;
    const offs: (() => void)[] = [];
    const hold = (pending: Promise<() => void>) =>
      void pending.then((off) => (dead ? off() : offs.push(off))).catch(() => {});

    const readSources = () =>
      void ports
        .listSources()
        .then((rows) => {
          if (!dead) sources = rows;
        })
        .catch(() => {});
    readSources();
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
        sync = [...sync.filter((other) => other.source_id !== row.source_id), row];
      }),
    );
    // A source added or removed changes its health reading first.
    hold(ports.listen(EVENTS.sourceHealth, readSources));
    // Every queue transition writes an activity line (`StatusBar.svelte`).
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

  function onkeydown(event: KeyboardEvent) {
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
      job: lastJob,
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

  onMount(() => {
    const onhash = () => (hash = location.hash);
    window.addEventListener("hashchange", onhash);
    window.addEventListener("keydown", onkeydown);
    return () => {
      window.removeEventListener("hashchange", onhash);
      window.removeEventListener("keydown", onkeydown);
    };
  });
</script>

<div class="tf">
  <TfTopBar
    current={sectionOf(route)}
    job={navJob}
    {inbox}
    {badges}
    {timer}
    {onsearch}
    {ontimer}
    ontheme={() => theme.toggle(prefersDark())}
  />
  <main class="tf-main" tabindex="-1">
    <TfPage
      ctx={{
        route,
        ready: lifecycle.ready,
        job: lastJob,
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
      }}
    />
  </main>
  <TfStatusBar {readings} pending={queue.counts.pending} onqueue={openWriteQueue} />
  {@render dialogs?.()}
  <TfToasts />
</div>
{#if writeQueuePanel.open}
  <WriteQueue {queue} {now} onclose={() => (writeQueuePanel.open = false)} />
{/if}

<!--
  The ticket-first shell (#809): the frame every view of the redesign is drawn
  in, mounted instead of the rooms UI when the setting says so
  (`shell-choice.ts`).

  It owns the top bar, the status bar, the toasts, the theme, the keyboard and
  the address. Everything that is the whole app's rather than one shell's
  stays in `App.svelte` and reaches it as a callback: the launcher behind
  *Search* and `⌘K`, and the timer verb behind the chip and `⌘T`, which stops
  into the worklog draft (#280) exactly as it does in the rooms UI.

  **The tokens are on this element, not on `:root`** (design system, *Getting
  it into the app*): `.tf` sets them, paints its own paper and fills the
  window, and the rooms UI's `app.css` is untouched. The theme follows the OS
  through `prefers-color-scheme`; the toggle overrides it for the session by
  setting `data-theme` here.

  **Dialogs styled by the rooms UI are mounted outside `.tf`**, the write
  queue here and the launcher and worklog draft in `App.svelte`: inside it
  they would read this shell's `--ok`, `--muted`, `--sans` and `--mono`
  instead of their own.
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
  import { onMount } from "svelte";

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
  import { switchShell } from "./shell-choice";
  import { sourceReadings } from "./status";
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
    queue = createWriteQueue(),
    ports = {
      listSources,
      syncStatus,
      listen: (event, handler) => tauriListen(event, handler),
    },
    prefersDark = () => window.matchMedia("(prefers-color-scheme: dark)").matches,
    onrooms = () => switchShell("rooms"),
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
    queue?: Queue;
    ports?: TfShellPorts;
    prefersDark?: () => boolean;
    onrooms?: () => void;
  } = $props();

  let hash = $state(location.hash);
  const route = $derived(parseTfHash(hash));

  /**
   * The job the reader was last in: the breadcrumb's chip and where `Esc`
   * returns (R-1.6). Only a job actually opened, as prototype-6's `S.job`.
   */
  let lastJob = $state<string | null>(null);
  $effect(() => {
    if (route.view === "job" && route.key !== null) lastJob = route.key;
  });

  /** What the timer runs on, as a ticket key; an asset or a label is no job. */
  const timedTicket = $derived.by(() => {
    const target = timer.current?.target ?? null;
    if (target === null || target.kind !== "entity" || assetTargetId(target) !== null) return null;
    return targetReading(target);
  });
  /** The Job tab's job: the last one opened, else the timer's ticket (R-1.2). */
  const navJob = $derived(lastJob ?? timedTicket);

  /** The session's override of the OS theme, or `null` to follow it (R-1.7). */
  let theme = $state<"light" | "dark" | null>(null);
  function toggleTheme() {
    const dark = theme !== null ? theme === "dark" : prefersDark();
    theme = dark ? "light" : "dark";
  }

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

<div class="tf" data-theme={theme ?? undefined}>
  <TfTopBar
    current={sectionOf(route)}
    job={navJob}
    {inbox}
    {badges}
    {timer}
    {onsearch}
    {ontimer}
    ontheme={toggleTheme}
  />
  <main class="tf-main" tabindex="-1">
    <TfPage {route} ready={lifecycle.ready} job={lastJob} {timedTicket} {onrooms} />
  </main>
  <TfStatusBar {readings} pending={queue.counts.pending} onqueue={openWriteQueue} />
  <TfToasts />
</div>
{#if writeQueuePanel.open}
  <WriteQueue {queue} {now} onclose={() => (writeQueuePanel.open = false)} />
{/if}

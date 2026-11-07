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
  import { onMount, tick, untrack, type Snippet } from "svelte";

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
  import { escHolder, escStep, escTarget, escThrough, openerName, windowTitle } from "./escape";
  import { keptPages as sharedKeptPages, type KeptPagesSetting } from "./kept-pages.svelte";
  import { catchReenterPress, secretPrompt } from "./settings/secret-prompt.svelte";
  import TfSecretPromptHost from "./settings/TfSecretPromptHost.svelte";
  import type { SettingsPorts } from "./settings/ports";
  import { recolours } from "./settings/state-colours";
  import { layers as sharedLayers, type Layers } from "./layers.svelte";
  import { onReady } from "./on-ready.svelte";
  import { goBackTo, goWithoutOpener, leftName, NOT_OPENED, openerOf, samePage, stamped, stampOf, takeTold, type Opened, type Told, type Trip } from "./opener";
  import { provideBackStep, provideShownPageName } from "./page-name.svelte";
  import { onRefreshKey } from "./page-refresh.svelte";
  import { PAGES } from "./pages";
  import { capturePlace, resetScrolls, restorePlace, type Place } from "./return-place";
  import { installRowRoom } from "./row-room";
  import { installFocusKeep } from "./focus-keep";
  import { installFindKey } from "./filter-keys";
  import { installPressGuard } from "./press-guard";
  import { isTopPage, parentOf, parseTfHash, sectionHash, sectionOf, sectionTabOf, type Section } from "./route";
  import { sectionFilter } from "../launcher/section";
  import { chordOf, refusalOf } from "../launcher/keys";
  import { launcherKeys, type KeyStore } from "../launcher/keys.svelte";
  import { theme as sharedTheme, type Theme } from "./theme.svelte";
  import { sourceReadings } from "./status";
  import { sourceRoles } from "./roles.svelte";
  import { entityAddress, legacyAddress } from "./address";
  import { ticketRef } from "./job/job";
  import { bridgePorts, type JobPorts } from "./job/ports";
  import { breadcrumbJob, currentJob } from "./job/current.svelte";
  import { chosenJob } from "./job/chosen.svelte";
  import { scopedTicket } from "./job/scoped.svelte";
  import { cardWidth } from "./job/card-width.svelte";
  import { selectTicket } from "./tickets/TicketsPage.svelte";
  import { pipelineHash } from "./builds/builds";
  import { sessionValue } from "./filter/filter-state.svelte";
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
    keys = launcherKeys,
    secretPorts,
  }: {
    lifecycle: Pick<Lifecycle, "ready">;
    /**
     * Open the launcher with `query` in its box (#2425): `""` for ⌘K, the
     * section's filter (`sectionFilter`) for `/`, ⌘/ and the top bar's button.
     */
    onsearch: (query: string) => void;
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
    /** The keys bound (#2429), whose saved lists' chords open search on the list; the window's when omitted. */
    keys?: KeyStore;
    /** What the credential prompt over a page reads and writes; the bridge's when omitted. */
    secretPorts?: Pick<SettingsPorts, "listSources" | "listAdapters" | "appStatus" | "setSourceSecret" | "syncNow">;
  } = $props();

  let hash = $state(location.hash);
  const route = $derived(parseTfHash(hash));

  /**
   * How the entry the reader is on was opened (`opener.ts`, #2229): the page
   * it was opened from, which `Esc` returns to, and the entry before it. The
   * window's first address was opened by nothing.
   */
  let here = $state.raw<Opened>({ ...NOT_OPENED, prev: null });
  history.replaceState(stamped(history.state, { ...NOT_OPENED, prev: null }), "");

  /** Put `next` in place of the address the reader is on, without a step back to it; a rooms address opened nothing. */
  function replaceHash(next: string) {
    here = { ...NOT_OPENED, prev: here.prev };
    history.replaceState(stamped(history.state, here), "", next);
    // The state itself, not a `hashchange`: on the first pass the listener (an `onMount`) is not there yet.
    hash = next;
  }

  // ---- the place the reader left (`return-place.ts`) ----
  let mainEl = $state<HTMLElement>();
  /** What was last pressed on a page: on the way out, the row that opened the next one. */
  let pressed: Element | null = null;
  /** Where each address was left, for the way back to it. */
  const places = new Map<string, Place>();
  const PLACES = 60;
  /** The next address change is the way back: `Esc` or *Back to*. A step through history says so itself. */
  let stepping = false;
  /** The address a press on a nav entry is on its way to: a return to the section as it was left, which opens nothing. */
  let navTo: string | null = null;
  /** The kept pages drawn so far: one shown for the first time starts at the top, not at the last page's scroll. */
  const drawnPages = new WeakSet<Element>();
  /** Ends the return under way when another address comes. */
  let endReturn: (() => void) | null = null;

  const shownKept = (): Element | null => mainEl?.querySelector(":scope > .tf-kept:not([hidden])") ?? null;

  /** Note where `from` was left, before its page hides. */
  function notePlace(from: string, to: string) {
    const page = shownKept();
    if (!mainEl || page === null) return;
    places.delete(from);
    const place = capturePlace(mainEl, page, pressed, to);
    places.set(from, place);
    // A run pressed on bare `#/builds`: the entry comes back by the pipeline's own address, and that is this page.
    const pipeline = from === sectionHash("builds") && place.pressed?.closest("a")?.getAttribute("href") === to ? parentOf(parseTfHash(to)) : null;
    if (pipeline?.startsWith("#/builds/")) places.set(pipeline, place);
    if (places.size > PLACES) places.delete(places.keys().next().value!);
    pressed = null;
  }

  /** A step to another address of the page `from` was opened from (`#/settings` opened a source, `Esc` goes to `#/settings/sources`): where it was opened. */
  function openingPlace(from: string, to: string): Place | undefined {
    return [...places].reverse().find(([left, place]) => place.to === from && samePage(left, to))?.[1];
  }

  /**
   * Once `to` is drawn: back to its place on a return, or on a nav press, which leaves focus where it is; a new place in a
   * kept page, or a page never shown, at the top.
   */
  function settlePlace(to: string, from: string, returning: boolean, opening: boolean, left: Element | null, nav: boolean) {
    endReturn?.();
    endReturn = null;
    void tick().then(() => {
      const page = shownKept();
      if (!mainEl || page === null || location.hash !== to) return;
      const first = !drawnPages.has(page);
      drawnPages.add(page);
      const place = returning ? (places.get(to) ?? openingPlace(from, to)) : nav ? places.get(to) : undefined;
      if (place !== undefined) endReturn = restorePlace(place, page, !nav);
      else if (opening && page === left) resetScrolls(mainEl, page);
      else if (opening && first) mainEl.scrollTop = 0;
    });
  }

  /** The name the shown page gives itself (`page-name.svelte.ts`), read as it is left. */
  const shownPage = provideShownPageName();
  // The window's name follows the page, and the page's name as it is read (F-015).
  $effect(() => {
    document.title = windowTitle(route, shownPage.name);
  });
  // A press on *Back to <name>* is the key's own step.
  provideBackStep((to, opened) => leave(to, opened));

  /**
   * The address moved (#2229). An entry already stamped is one history
   * stepped to, or one replaced in place: it keeps its stamp. A new entry is
   * stamped here, before the page it left is gone ({@link stampFor}). Either way
   * the card row's narrowing is what the entry says.
   */
  function arrive() {
    const now = location.hash;
    // Two writes before the events are delivered are two events that read one address. The second moved nothing, and
    // its stamped entry would read as a return, whose hold would then undo the reader's next scroll.
    if (now === hash) return;
    const how = takeTold(now);
    const had = stampOf(history.state);
    const left = shownKept();
    // The page the app first drew was never shown by an address change, and it has been drawn all the same.
    if (left !== null) drawnPages.add(left);
    const nav = navTo === now;
    navTo = null;
    if (how !== "replace") notePlace(hash, now);
    // A stamped entry is one history stepped to, which is a return as `Esc` is. A tab switch is neither that nor a new place.
    const returning = how !== "replace" && (stepping || had !== null);
    const opening = how !== "replace" && !returning && !samePage(hash, now);
    stepping = false;
    if (how !== "replace" && had !== null) {
      here = had;
    } else {
      if (how !== "replace") here = stampFor(how, now);
      history.replaceState(stamped(history.state, here), "");
    }
    if (how !== "replace") scopedTicket.hold(here.trip);
    const was = hash;
    hash = now;
    if (how !== "replace") settlePlace(now, was, returning, opening, left, nav);
  }

  /**
   * How a new entry at `now` came to be: opened by nothing when it was told so; the stamp its opener had on an `Esc`
   * step that couldn't step back; the page's own opener on a tab switch; otherwise the page it left. A press keeps
   * what a card row narrowed, or takes what the row just narrowed, since a row says so before its address moves.
   */
  function stampFor(how: Exclude<Told, "replace"> | null, now: string): Opened {
    if (how === "unopened") return { ...NOT_OPENED, prev: hash };
    if (how === "back") return { ...(here.opener?.from ?? NOT_OPENED), prev: hash };
    if (how === "launched") return { opener: openerOf(hash, openerName(route, shownPage.name), here), trip: null, visit: isTopPage(parseTfHash(now)), prev: hash };
    const trip = tripAt(now);
    if (how === null && samePage(hash, now)) return { opener: here.opener, trip, visit: here.visit, prev: hash };
    return { opener: openerOf(hash, leftName() ?? openerName(route, shownPage.name), here), trip, visit: how === "visit", prev: hash };
  }

  /** What a press to `now` keeps narrowed: Sub-tasks is the bare list's, so a saved view ends it, as *All tickets* does. */
  function tripAt(now: string): Trip | null {
    const to = parseTfHash(now);
    const trip = scopedTicket.trip;
    return trip?.section === "tickets" && to.view === "tickets" && to.rest.length > 0 ? null : trip;
  }

  // ---- section memory: a nav entry returns to the section as you left it (design system, the return contract) ----
  /** The address of each section's tab or saved view it was last on, outside a card row's narrowing. */
  const sectionTabs = $state<Partial<Record<Section, string>>>({});
  const lastPipeline = sessionValue("builds-last-pipeline");
  $effect(() => {
    const onRun = route.view === "full" && route.head === "build";
    const section = onRun ? "builds" : sectionTabOf(route);
    if (section === null || untrack(() => scopedTicket.section) === section) return;
    // Builds remembers only a pipeline it holds, so an address it doesn't is nothing to come back to, and a run leads back to its own.
    if (section === "builds" && route.view === "full" && route.rest[0] !== undefined) {
      if (route.rest[0] === lastPipeline.value) sectionTabs.builds = onRun ? pipelineHash(route.rest[0]) : hash;
      return;
    }
    // A tab the page does not have is said so, and is nothing to come back to.
    if (PAGES.find((candidate) => candidate.matches(route))?.knows?.(route) === false) return;
    sectionTabs[section] = hash;
  });
  const navHref = (section: Exclude<Section, "job">) => sectionTabs[section] ?? sectionHash(section);

  /**
   * A press on a nav entry: the section as it was left, each scroller included, opened by nothing, so `Esc` never jumps from
   * one section to another by way of a tab. It ends a card row's narrowing, wherever that was: a nav entry starts a new trip.
   */
  function onnav(_section: Exclude<Section, "job">, target: string) {
    if (location.hash === target) {
      here = { ...NOT_OPENED, prev: here.prev };
      history.replaceState(stamped(history.state, here), "");
      scopedTicket.clear();
    } else {
      navTo = target;
      goWithoutOpener(target);
    }
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
   * "job you were last in" (#2230): `#/job`, the pages' *This job* and the
   * launcher's job elsewhere than on a ticket page read it. `key` is what
   * the pages name; `entityId` is where every way back to it goes, so a key
   * two sources hold returns to the one that was chosen (#1525).
   */
  chosenJob.load();
  // The ticket page card's width (#2402), kept in the same storage.
  cardWidth.load();
  const chosen = $derived(chosenJob.id === null ? null : ticketRef(chosenJob.id));
  /**
   * A ticket a card's sections row opened its section on (#2401,
   * `job/scoped.svelte.ts`) is that section's pages' *This job* while it is
   * on screen (`TfKeptPage`). Each history entry says whether it is part of that
   * trip (`arrive`). A page can end it in place (*All tickets*), and then the
   * entry stops saying so.
   */
  $effect(() => {
    if (scopedTicket.section !== null || untrack(() => here.trip) === null) return;
    here = { ...here, trip: null };
    history.replaceState(stamped(history.state, here), "");
  });

  /**
   * Where `Esc` goes once nothing is open, for the breadcrumb's hint (#2228):
   * the ladder's own rungs, so the hint names what the key does.
   */
  const visiting = $derived(here.visit || (scopedTicket.section !== null && scopedTicket.section === sectionOf(route) && route.view !== "ticket" && route.view !== "job"));
  const back = $derived(escTarget(route, here.opener, visiting, shownPage.parent));

  /** What the timer runs on, as a ticket key; an asset or a label is no job. */
  const timedTicket = $derived.by(() => {
    const target = timer.current?.target ?? null;
    if (target === null || target.kind !== "entity" || assetTargetId(target) !== null) return null;
    return targetReading(target);
  });
  /**
   * The job the shell's pages default to (R-1.2, spec #2227): the chosen job, else the ticket a
   * timer runs on that was not started here (a start chooses its ticket),
   * with its entity id, which `create_block` needs (#1027). Both are ids
   * already, so nothing is read for it.
   */
  const defaultJob = $derived.by(() => {
    if (chosen !== null) return chosen;
    const target = timer.current?.target ?? null;
    return timedTicket === null || target === null || target.kind !== "entity" ? null : { key: timedTicket, entityId: target.entity_id };
  });

  /**
   * The job the launcher's *Link to <key>* targets, and ⌘T falls back to
   * (`breadcrumbJob`, spec #2227): the ticket on screen on the Job page
   * (`null` while it is still being read) or on a ticket page (its address
   * names it), otherwise the chosen job. Never a guessed job: with none
   * chosen there is none.
   */
  $effect(() => {
    const onScreen = route.view === "ticket" ? route.id : currentJob.entityId;
    const job = route.view === "job" || route.view === "ticket" ? (onScreen === null ? null : ticketRef(onScreen)) : chosen;
    breadcrumbJob.set(job);
    return () => breadcrumbJob.set(null);
  });

  // ---- the status bar's reads, once the database can answer ----
  let sources = $state<Pick<SourceSummary, "id" | "display_name">[] | null>(null);
  let sync = $state<SourceSyncStatus[]>([]);
  /** Moves on each set or clear of a state colour (spec #2463): the page host drops its hidden pages then. */
  let recoloured = $state(0);
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
  // The saved lists' chords (#2429), read once the database answers; the launcher reads them again on each opening.
  onReady(ready, () => keys.load, () => {});
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
    // Every queue transition writes an activity line. So does a set or a clear
    // of a state colour, after which every hidden page drew stale states.
    hold(
      ports.listen(EVENTS.activityNew, ({ payload }) => {
        void queue.refresh();
        if (recolours(payload)) recoloured += 1;
      }),
    );
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
      onsearch("");
      return;
    }
    // `/` outside a field, and ⌘/ anywhere, are the section's search (#2425):
    // the launcher with the section's filter, the pages having no field of
    // their own. Read by the key, not the code: on a German board `/` is ⇧7.
    // A dialog owns the keyboard, the launcher's own included.
    if (event.key === "/" && !event.altKey && !event.defaultPrevented && !dialogOpen()) {
      if (mod || focusedField() === null) {
        event.preventDefault();
        onsearch(sectionFilter(route));
        return;
      }
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
    // A saved list's chord (#2429): search on the list, from any page and in
    // a field too, as ⌘K is. After the shell's own keys, which win; never a
    // chord the panel refuses; a dialog, the open launcher's included, owns
    // the keyboard (the launcher answers it there itself).
    const chord = chordOf(event);
    if (chord !== null && !event.defaultPrevented && refusalOf(event) === null && !dialogOpen()) {
      const list = keys.listOf(chord);
      if (list) {
        event.preventDefault();
        onsearch(`${list.text} `);
        return;
      }
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
              through: escThrough(field),
            },
      route,
      opener: here.opener,
      visiting,
      parent: shownPage.parent,
    });
    if (step.do === "nothing") return;
    event.preventDefault();
    if (step.do === "close") layers.open[step.index]?.close();
    else if (step.do === "clear-filter" && field instanceof HTMLInputElement) {
      field.value = "";
      field.dispatchEvent(new Event("input", { bubbles: true }));
    } else if (step.do === "blur") {
      // Out to what holds the field, never the body: the next `Tab` is back in it, and the next `Esc` is the page's.
      const holder = field === null ? null : escHolder(field);
      field?.blur();
      holder?.focus({ preventScroll: true });
    }
    else if (step.do === "go") leave(step.hash, step.opened);
  }

  /** `Esc`'s step to `hash` (rungs 5 to 8), `opened` when it is the entry's opener. */
  function leave(hash: string, opened: boolean) {
    // Back on the Tickets that opened this ticket page, that ticket is selected again (spec #2227).
    if (opened && route.view === "ticket" && parseTfHash(hash).view === "tickets") selectTicket(route.id);
    stepping = true;
    // The opener is the entry before: step back to it, so `⌘]` comes forward to this page again.
    if (opened && hash === here.prev) nav.back();
    else if (opened) goBackTo(hash);
    else goWithoutOpener(hash);
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

  // First, so that a press it swallows reaches nothing else.
  onMount(() => installPressGuard());
  onMount(() => installFindKey(() => onsearch(sectionFilter(route))));
  onMount(() => installRowRoom());
  onMount(() => installFocusKeep());
  onMount(() => {
    const onhash = () => arrive();
    window.addEventListener("hashchange", onhash);
    window.addEventListener("keydown", onkeydown);
    window.addEventListener("click", catchReenterPress, true);
    for (const type of MOUSE) window.addEventListener(type, onmouse);
    return () => {
      window.removeEventListener("hashchange", onhash);
      window.removeEventListener("keydown", onkeydown);
      window.removeEventListener("click", catchReenterPress, true);
      for (const type of MOUSE) window.removeEventListener(type, onmouse);
    };
  });
</script>

<div class="tf">
  <TfTopBar
    current={sectionOf(route)}
    {inbox}
    {badges}
    {timer}
    job={chosen?.key ?? null}
    onsearch={() => onsearch(sectionFilter(route))}
    {ontimer}
    ontheme={() => theme.toggle(prefersDark())}
    {navHref}
    {onnav}
  />
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions: not a control, it notes what a press on a page was on -->
  <main class="tf-main" tabindex="-1" bind:this={mainEl} onclickcapture={(event) => (pressed = event.target instanceof Element ? event.target : null)}>
    <TfPage
      {kept}
      stale={recoloured}
      ctx={{
        route,
        ready: lifecycle.ready,
        job: chosen?.key ?? null,
        jobAt: chosen?.entityId ?? null,
        back,
        timedTicket,
        defaultJob,
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
  {#if secretPrompt.source !== null}
    <TfSecretPromptHost sourceId={secretPrompt.source} {layers} ports={secretPorts} />
  {/if}
  <TfToasts />
  <div class="tf-floats"></div>
</div>

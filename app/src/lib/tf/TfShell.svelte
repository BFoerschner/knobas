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
  import type { ClipState } from "../ipc/passwords";
  import { clipped } from "./passwords/clip.svelte";


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
    /** Where Docs opens and the spaces it hides (PR #2479, 2026-11-17), held once the database answers; Docs' own page and none hidden without it. */
    docsChoices?: () => Promise<import("../ipc/entity").DocsChoices>;
  }
</script>

<script lang="ts">
  import { listen as tauriListen } from "@tauri-apps/api/event";
  import { flushSync, onMount, tick, untrack, type Snippet } from "svelte";

  import { EVENTS } from "../ipc";
  import { commandSteps, diffPipelines, namedPipelines, pagesKeptOpen, rendererDefaults, userRenderers, type LinkHandoffTarget } from "../ipc/app";
  import { diffPipelines as heldDiffPipelines } from "./diff/diff-pipelines.svelte";
  import { docsChoices as readDocsChoices } from "../ipc/entity";
  import { docsChoices as heldDocsChoices } from "./docs/docs-choices.svelte";
  import { rendererDefaults as heldDefaults } from "../render/defaults.svelte";
  import { commandSteps as heldCommandSteps, namedPipelines as heldPipelines } from "../render/pipelines.svelte";
  import { defaultPipeline } from "../render/defaults";
  import { installPipelines, type DiffPipeline } from "./diff/piped";
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
  import { runsRead } from "./builds/runs-read.svelte";
  import { readDocsPageAhead } from "./docs/press-reads";
  import { goBackTo, goWithoutOpener, landFromSite, leftName, noteArrival, NOT_OPENED, openerOf, samePage, stamped, stampOf, takeTold, type Opened, type Told, type Trip } from "./opener";
  import { provideBackStep, provideShownPageName } from "./page-name.svelte";
  import { onRefreshKey } from "./page-refresh.svelte";
  import { onOpenKey } from "./page-open.svelte";
  import { PAGES } from "./pages";
  import { capturePlace, pressOpens, resetScrolls, restorePlace, type Place } from "./return-place";
  import { installRowRoom } from "./row-room";
  import { installFocusKeep, installKeysHere } from "./focus-keep";
  import { installInputModality } from "./input-modality";
  import { loadEveryFace } from "../shell/faces";
  import { warmDiffView } from "./diff/warm";
  import { warmDiagrams } from "../render/diagram";
  import { renderPort } from "../render/port";
  import { warmDocs } from "./docs/warm";
  import { aheadBuilt, openAhead, quietAndIdle } from "./ahead";
  import { installFindKey } from "./filter-keys";
  import { installPlainTyping } from "./plain-typing";
  import { installExternalLinks } from "../shell/external-links";
  import { installPressGuard } from "./press-guard";
  import { PressHold } from "./press-hold.svelte";
  import { itemOf, parentOf, parseTfHash, reopens, sectionHash, sectionOf, sectionTabOf, type Section } from "./route";
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
  import { homeInstead } from "./home";
  import { scopedTicket } from "./job/scoped.svelte";
  import { sideWidths } from "./side-width.svelte";
  import { selectTicket } from "./tickets/TicketsPage.svelte";
  import { pipelineHash } from "./builds/builds";
  import { isRunScopeHash, runScopeOf } from "./builds/recent";
  import { sessionValue } from "./filter/filter-state.svelte";
  import TfPage from "./TfPage.svelte";
  import { flipZen, isZenKey, zen } from "./zen.svelte";
  import { roleSyncs as sharedRoleSyncs, type RoleSyncs } from "./role-sync.svelte";
  import TfStatusBar from "./TfStatusBar.svelte";
  import TfRunHost from "./oneoff/TfRunHost.svelte";
  import { oneoffs } from "../render/oneoff.svelte";
  import TfAck from "./TfAck.svelte";
  import TfToasts from "./TfToasts.svelte";
  import TfThemeKeep from "./TfThemeKeep.svelte";
  import { push as pushToast } from "../shell/toasts.svelte";
  import { unreadableAtStart } from "./themes.svelte";
  import TfContextMenu from "./context/TfContextMenu.svelte";
  import TfTopBar from "./TfTopBar.svelte";
  import TfZoomLevel from "./TfZoomLevel.svelte";
  import { installZoomInput, zoom as sharedZoom, zoomKey, type Zoom } from "./zoom.svelte";
  import { noteScrolls } from "./scrolled";
  import { readLandingAtAddress } from "./repo/hover-reads";
  import { siteAllow, siteDo, siteRevealDownload, siteSettings, sitesKept, siteZoom, type SiteDownload, type SiteKey, type SiteRouted, type SiteStatus } from "../ipc/site";
  import { SITE_SETTLE_MS, siteStore } from "./site/site.svelte";
  import { closeSite, reloadSite, siteBack, siteForward } from "./site/actions";
  import TfSiteQuit from "./site/TfSiteQuit.svelte";
  import { ackBand } from "../shell/ack.svelte";
  import { openInBrowser } from "../shell/external-links"; // browser on purpose: a site's link to an address it may not show
  import { openSiteAt, openWeb } from "../shell/open-web";
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
      // Named only when called, as `docs-choices.svelte.ts`' own reads are.
      docsChoices: async () => readDocsChoices(),
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
    zoom = sharedZoom,
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
    /** The window's zoom (`zoom.svelte.ts`): its keys, its wheel, its plate and the status bar's level. */
    zoom?: Zoom;
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
  /**
   * The run, pull request or repository each section was left on by a nav press, tab included: a press on its row in that
   * section brings it back. Used once, and any other arrival at an item of the section ends it.
   */
  const leftItems: Partial<Record<Section, string>> = {};
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
   * Once `to` is drawn: back to its place on a return, or on a nav press or a row's reopening, which leave focus where it is;
   * a new place in a kept page, or a page never shown, at the top.
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
      // Not where the page it came from was left at the top: setting the scroll laid out the new page in the task that
      // builds it, 17 ms of a quarter-speed main thread on a repository's first open (the ux21 overs pass). `notePlace`
      // read it as that page was left, laid out then.
      else if (opening && first && leftAt(from) !== 0) mainEl.scrollTop = 0;
    });
  }

  /** Where the shell's scroller stood as `from` was left (`notePlace`), or `null` where that is not known. */
  function leftAt(from: string): number | null {
    return places.get(from)?.scrolls.find((one) => one.el === mainEl)?.top ?? null;
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
    let now = location.hash;
    // Two writes before the events are delivered are two events that read one address. The second moved nothing, and
    // its stamped entry would read as a return, whose hold would then undo the reader's next scroll.
    if (now === hash) return;
    const how = takeTold(now);
    const had = stampOf(history.state);
    const left = shownKept();
    // The page the app first drew was never shown by an address change, and it has been drawn all the same.
    if (left !== null) drawnPages.add(left);
    // A plain press on the row of the item this section was left on, and launcher jumps and links from elsewhere never are.
    const reopen = how === null && had === null ? reopening(now) : null;
    const nav = navTo === now || reopen !== null;
    navTo = null;
    // A site's landing (`landFromSite`) takes the site's entry in place, as a replaced address does, and keeps its stamp;
    // unlike one it is an opening, so the page it lands on settles as a new place.
    const inPlace = how === "replace" || how === "land";
    if (!inPlace) notePlace(hash, now);
    // In place of the row's address, on the entry the press made, so one press is one step back.
    if (reopen !== null) {
      history.replaceState(history.state, "", reopen);
      now = reopen;
    }
    const arrived = parseTfHash(now);
    if (itemOf(arrived) !== null) delete leftItems[sectionOf(arrived)!];
    // A stamped entry is one history stepped to, which is a return as `Esc` is. A tab switch is neither that nor a new place.
    const returning = !inPlace && (stepping || had !== null);
    const opening = how === "land" || (how !== "replace" && !returning && !samePage(hash, now));
    noteArrival({ hash: now, how, returning });
    stepping = false;
    if (!inPlace && had !== null) {
      here = had;
    } else {
      if (!inPlace) here = stampFor(how as Exclude<Told, "replace" | "land"> | null, now);
      history.replaceState(stamped(history.state, here), "");
    }
    if (how !== "replace") scopedTicket.hold(here.trip);
    const was = hash;
    hash = now;
    if (how !== "replace") settlePlace(now, was, returning, opening, left, nav);
  }

  /** The address a press going to `to` reopens, if `to` is the bare address of the item this section was left on. */
  function reopening(to: string): string | null {
    const section = sectionOf(route);
    const left = section === null ? undefined : leftItems[section];
    return left !== undefined && reopens(left, to) && pressOpens(pressed, to) ? left : null;
  }

  /**
   * How a new entry at `now` came to be: opened by nothing when it was told so; the stamp its opener had on an `Esc`
   * step that couldn't step back; the page's own opener on a tab switch; otherwise the page it left. A press keeps
   * what a card row narrowed, or takes what the row just narrowed, since a row says so before its address moves.
   */
  function stampFor(how: Exclude<Told, "replace" | "land"> | null, now: string): Opened {
    if (how === "unopened") return { ...NOT_OPENED, prev: hash };
    if (how === "back") return { ...(here.opener?.from ?? NOT_OPENED), prev: hash };
    if (how === "launched") return { opener: openerOf(hash, openerName(route, shownPage.name), here), trip: null, prev: hash };
    const trip = tripAt(now);
    if (how === null && samePage(hash, now)) return { opener: here.opener, trip, prev: hash };
    return { opener: openerOf(hash, leftName() ?? openerName(route, shownPage.name), here), trip, prev: hash };
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
  /** Where Builds was last stood (#2570, `TfBuilds`): *All pipelines*, a folder or a pipeline, as its address. */
  const lastBuildsPlace = sessionValue("builds-place");
  $effect(() => {
    const onRun = route.view === "full" && route.head === "build";
    // A source's page and *Add a source*'s address are not sections of Settings: the cog goes to Sources from there.
    if (route.view === "full" && route.head === "settings" && (route.rest[0] === "source" || (route.rest[0] === "sources" && route.rest[1] === "add"))) sectionTabs.settings = "#/settings/sources";
    const section = onRun ? "builds" : sectionTabOf(route);
    if (section === null || untrack(() => scopedTicket.section) === section) return;
    // Builds remembers only a pipeline it holds, so an address it doesn't is nothing to come back to, and a run leads back to its own.
    if (section === "builds" && route.view === "full" && route.rest[0] !== undefined) {
      // *All pipelines* and a folder (#2570) are places of Builds as a pipeline is.
      if (!onRun && runScopeOf(route.rest) !== null) sectionTabs.builds = hash;
      // A run's section page is the scope it was opened from, else the run's pipeline.
      else if (onRun && lastBuildsPlace.value !== null && isRunScopeHash(lastBuildsPlace.value)) sectionTabs.builds = lastBuildsPlace.value;
      else if (route.rest[0] === lastPipeline.value) sectionTabs.builds = onRun ? pipelineHash(route.rest[0]) : hash;
      return;
    }
    // A tab the page does not have is said so, and is nothing to come back to.
    if (PAGES.find((candidate) => candidate.matches(route))?.knows?.(route) === false) return;
    sectionTabs[section] = hash;
  });
  /**
   * The address each section was last stood on, at any depth: a Docs page or space, a ticket, a pull request or a
   * repository at its tab, a run at its tab, a marked monitor, a link set, a source's page (2026-11-13, Björn: "when i
   * select a page in docs, then go somewhere else and then return to docs my selected page isn't shown"). Not a card
   * row's narrowing, not home (`#/job` lights Tickets and is no place in it), not a tab the page does not have, and on
   * Builds not a pipeline it does not hold.
   */
  const sectionPlaces = $state<Partial<Record<Section, string>>>({});
  $effect(() => {
    const section = sectionOf(route);
    if (section === null || route.view === "job" || untrack(() => scopedTicket.section) === section) return;
    if (PAGES.find((candidate) => candidate.matches(route))?.knows?.(route) === false) return;
    if (route.view === "full" && route.head === "builds" && route.rest[0] !== undefined && route.rest[0] !== lastPipeline.value && runScopeOf(route.rest) === null) return;
    sectionPlaces[section] = hash;
  });
  /**
   * Docs' own page as the entry knows it: a search it was left on, else where Docs opens, its home when one is chosen
   * (PR #2479, 2026-11-17, `docs-choices.svelte.ts`). With none chosen, that is `#/docs`, as it was.
   */
  function docsOwn(): string {
    const tab = sectionTabs.docs;
    const at = tab === undefined ? null : parseTfHash(tab);
    return tab !== undefined && at?.view === "full" && at.rest[0] === "search" ? tab : heldDocsChoices.start;
  }
  /**
   * Where a nav entry goes: from another section, back to where you were in it; inside it, to the section's own page as
   * it was left (its tab, view, pipeline or search), which a press there moves nothing on.
   */
  const navHref = (section: Exclude<Section, "job">) => {
    const own = section === "docs" ? docsOwn() : (sectionTabs[section] ?? sectionHash(section));
    return sectionOf(route) === section ? own : (sectionPlaces[section] ?? own);
  };

  /**
   * The press rule's hold on an entry that just moved the address (`press-hold.svelte.ts`): a second press of it by a
   * person, a double click or a second `Enter`, lands on the page it came back to and does nothing, rather than going on
   * to the section's own page. Only while that page is the one shown: an address moved since makes the next press a new
   * one. A script's press is no person's and goes through, as the press guard lets it.
   */
  const navHold = new PressHold();
  /** The entry the hold is on, and the address its press went to. */
  let navHeld: { section: Section; on: string } | null = null;

  /**
   * A press on a nav entry: the section as it was left, each scroller included, opened by nothing, so `Esc` never jumps from
   * one section to another by way of a tab. It ends a card row's narrowing, wherever that was: a nav entry starts a new trip.
   * The cog and `⌘,` return the same way and keep their opener, since `Esc` leaves Settings for the page they were pressed on.
   * `person` says a person pressed it, whose second press within the hold is the same press.
   */
  function onnav(section: Exclude<Section, "job">, target: string, person = false) {
    const gear = section === "settings";
    if (person && navHold.held() && navHeld?.section === section && location.hash === navHeld.on) return;
    if (location.hash === target) {
      if (gear) return;
      here = { ...NOT_OPENED, prev: here.prev };
      history.replaceState(stamped(history.state, here), "");
      scopedTicket.clear();
      return;
    }
    const from = sectionOf(route);
    if (from !== null && itemOf(route) !== null) leftItems[from] = location.hash;
    navTo = target;
    if (person) {
      navHold.hold();
      navHeld = { section, on: target };
    }
    if (gear) go(target);
    else goWithoutOpener(target);
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
  /**
   * Home with no chosen job is *All tickets* (`home.ts`, 2026-11-20): a bare
   * `#/job` then (typed, kept in history, or home whose chosen job the
   * ticket page found gone) is replaced by `#/tickets` before anything
   * under it draws, so the page that only pointed at Tickets is never shown.
   */
  $effect.pre(() => {
    const instead = homeInstead(route, chosenJob.id);
    if (instead !== null) untrack(() => replaceHash(instead));
  });
  // Every sidebar's width (the ticket page card's among them, #2402), kept in the same storage.
  sideWidths.load();
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
   * On a site's page, the page knobas holds for where the site stands, when it is not the one it was opened at
   * (`TfSite.svelte`, Fable's ruling, 2026-11-25): `Esc` twice and *Back to* land there.
   */
  const siteLanding = $derived(route.view === "site" ? (siteStore.landing.get(route.source) ?? null) : null);
  /**
   * Where `Esc` goes once nothing is open, for the breadcrumb's hint (#2228): the ladder's own rungs, so the hint names
   * what the key does. The page's parent in its own section, never the opener's (#2559).
   */
  const back = $derived(escTarget(route, here.opener, shownPage.parent, siteLanding));

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
  // Each path's answer is kept until either list is replaced (each change replaces it whole): a compare of 10,000 files
  // asked it of every file each time its list landed (the stored-compare pass, 2026-11-14).
  let pipelineAnswers: { defaults: unknown; pipelines: unknown; by: Map<string, DiffPipeline | null> } = { defaults: null, pipelines: null, by: new Map() };
  installPipelines({
    of: (path) => {
      const [defaults, pipelines] = [heldDefaults.list, heldPipelines.list];
      if (pipelineAnswers.defaults !== defaults || pipelineAnswers.pipelines !== pipelines) pipelineAnswers = { defaults, pipelines, by: new Map() };
      const kept = pipelineAnswers.by.get(path);
      if (kept !== undefined) return kept;
      const found = defaultPipeline(path, defaults, pipelines);
      const answer = found === null ? null : { name: found.pipeline.name, pattern: found.pattern, steps: found.pipeline.steps };
      if (pipelineAnswers.by.size >= 50_000) pipelineAnswers.by.clear();
      pipelineAnswers.by.set(path, answer);
      return answer;
    },
  });

  // The named pipelines and command steps (#2265): until they answer, no file opens through a default pipeline.
  onReady(ready, () => ports.namedPipelines, (list) => heldPipelines.hold(list));
  onReady(ready, () => ports.commandSteps, (list) => heldCommandSteps.hold(list));
  // *Run pipelines on diffs* (#2270): until it answers, it is on.
  onReady(ready, () => ports.diffPipelines, (on) => heldDiffPipelines.hold(on));
  // Where Docs opens and the spaces it hides (PR #2479, 2026-11-17): until they answer, Docs' own page and every space.
  onReady(ready, () => ports.docsChoices, (choices) => heldDocsChoices.hold(choices));
  // The runs marked read (ux18): until they answer, every failed run asks, as before there were marks.
  onReady(ready, () => () => runsRead.load(), () => {});
  // Opening a run reads it and writes no activity line (ADR-0063): the counts that follow marks are read again here.
  runsRead.afterOpen = () => {
    void badges.refresh();
    void sharedInbox.refresh();
  };
  // Builds in front, or not (#2568, ADR-0063): leaving it moves the instant green counts from. The first answer, off
  // Builds, ends a visit a crash left open.
  let buildsAsked = false;
  $effect(() => {
    if (!lifecycle.ready) return;
    const front = sectionOf(route) === "builds";
    untrack(() => {
      if (!buildsAsked && !front) void runsRead.started();
      else void runsRead.inFront(front);
      buildsAsked = true;
    });
  });
  $effect(() => () => runsRead.untell());
  // Which sources' pages open inside knobas (ADR-0050): until they answer, every address opens the browser, as before.
  onReady(ready, () => siteSettings, (list) => siteStore.hold(list));
  // *Sites kept open*, which the native side holds once read.
  onReady(ready, () => sitesKept, () => {});

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
        // The sites' nameplates (ADR-0050).
        siteStore.nameAll(rows);
        // A source added since the sites' settings were read has none: its Settings page would have no Site section,
        // and its pages would open the browser, until the next start (the final suite's real window, 2026-11-22). A
        // source removed since still has one: an address on its server opened its site (the ux30 gate, Windows).
        if (siteStore.differs(rows.map((row) => row.id))) {
          void siteSettings()
            .then((list) => {
              if (!dead) siteStore.hold(list);
            })
            .catch(() => {});
        }
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
    // The monitors alone: the builds' counts come with the runs store's first word to the backend, 600 ms on (the
    // effect below `afterOpen`). Read here too, they went out beside Recent's first page (#2569b).
    void badges.readMonitors();
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
    // A source's site (ADR-0050): its state, the keys it gave knobas, where its links went, its downloads, its close,
    // and the quit that asks about it.
    hold(ports.listen(EVENTS.siteStatus, ({ payload }) => siteStore.update(payload as SiteStatus)));
    hold(ports.listen(EVENTS.siteClosed, ({ payload }) => siteStore.closed((payload as { source: string }).source)));
    hold(ports.listen(EVENTS.siteKey, ({ payload }) => siteKey(payload as SiteKey)));
    hold(ports.listen(EVENTS.siteRouted, ({ payload }) => siteRouted(payload as SiteRouted)));
    hold(ports.listen(EVENTS.siteDownload, ({ payload }) => siteDownload(payload as SiteDownload)));
    hold(ports.listen(EVENTS.siteQuit, ({ payload }) => (quitting = payload as SiteStatus[])));
    // What the clipboard holds for knobas (ADR-0056): the status bar's line and the entry's rows read it.
    hold(ports.listen(EVENTS.clipboardState, ({ payload }) => clipped((payload as ClipState | null) ?? { what: null, entity_id: null, until: null })));
    // External URL handed off via `knobas://open?url=...` or Firefox Native Messaging (ADR-0061).
    hold(ports.listen(EVENTS.linkHandoffOpen, ({ payload }) => openHandoff(payload as LinkHandoffTarget)));

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

  // ---- Zen mode, everywhere in Code (Z1, `zen.svelte.ts`): the shell owns its key, its button and its `Esc` rung ----
  // It holds while the reader moves between Code's pages, and ends on leaving Code.
  $effect(() => {
    const inCode = sectionOf(route) === "code";
    zen.here = inCode;
    if (!inCode) zen.on = false;
  });
  /** Turns Zen mode on or off with the topmost row the reader sees kept where it stands. */
  const toggleZen = () => flipZen(mainEl ?? null, flushSync);
  // While it shows, `Esc` leaves it before it leaves the page: an inline panel's rung.
  $effect(() => {
    if (!zen.shown) return;
    return untrack(() => layers.add({ kind: "panel", close: toggleZen }));
  });

  /** Back or forward: on a site's page in the site first, then in knobas (decisions §8; the mouse's buttons too, #2528). */
  function travelTo(travel: "back" | "forward") {
    if (route.view === "site" && (travel === "back" ? siteBack(route.source) : siteForward(route.source))) return;
    if (travel === "back") nav.back();
    else nav.forward();
  }

  function onkeydown(event: KeyboardEvent) {
    // On a site's page a key between two `Esc`s is a new start (decisions §9).
    if (route.view === "site" && event.key !== "Escape") siteStore.disarm(route.source);
    // ⌘R presses the page's *Refresh* or *Sync now* (#2156), and the webview
    // never reloads; an overlay owns the keyboard, as it does history's keys.
    if (onRefreshKey(event, dialogOpen())) return;
    // `O` presses the page's *Open in ⟨source⟩*, `⇧O` opens the browser (ADR-0050).
    if (onOpenKey(event, dialogOpen())) return;
    // ⌘+ / ⌘− / ⌘0 zoom the window from anywhere, a field and a dialog too, as a browser's do: the window's, not a page's.
    const zooming = zoomKey(event);
    if (zooming !== null) {
      event.preventDefault();
      if (zooming === 0) zoom.reset();
      else zoom.step(zooming);
      return;
    }
    const travel = historyStep(event);
    if (travel !== null) {
      event.preventDefault();
      if (dialogOpen()) return;
      travelTo(travel);
      return;
    }
    const mod = event.metaKey || event.ctrlKey;
    // ⌘W closes the site, through its question; ⌘L gives the site the keyboard back (decisions §8).
    if (route.view === "site" && mod && !event.shiftKey && !event.altKey && !dialogOpen()) {
      const key = event.key.toLowerCase();
      if (key === "w") {
        event.preventDefault();
        void closeSite(route.source);
        return;
      }
      if (key === "l") {
        event.preventDefault();
        void siteDo(route.source, "focus", false).catch(() => {});
        return;
      }
    }
    if (mod && event.key.toLowerCase() === "k") {
      event.preventDefault();
      onsearch("");
      return;
    }
    // `Z` on any Code page, outside a field, with no dialog, menu or launcher over it.
    if (zen.here && isZenKey(event) && focusedField() === null && !inTextField(event.target) && !layers.open.some((layer) => layer.kind !== "panel")) {
      event.preventDefault();
      toggleZen();
      return;
    }
    // `X` and `Shift X` on any Code page, outside a field, with no dialog, menu or launcher over it (ADR-0060): the
    // most specific *Run…* menu standing, and the last command run again.
    if (zen.here && event.key.toLowerCase() === "x" && !mod && !event.altKey && !event.defaultPrevented && focusedField() === null && !inTextField(event.target) && !layers.open.some((layer) => layer.kind !== "panel")) {
      if (event.shiftKey ? oneoffs.runLast() : oneoffs.openMenu()) event.preventDefault();
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
      if (!dialogOpen()) onnav("settings", navHref("settings"), event.isTrusted);
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
      parent: shownPage.parent,
      armed: route.view === "site" && siteStore.armed.has(route.source),
      landing: siteLanding,
    });
    if (step.do === "nothing") return;
    if (step.do === "arm") {
      event.preventDefault();
      if (route.view === "site") armSite(route.source);
      return;
    }
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
    else if (step.do === "go") {
      if (route.view === "site") leaveSite(route.source);
      else leave(step.hash, step.opened);
    }
  }

  // ---- a source's site (ADR-0050) ----
  /** The sites that hold changes, while knobas asks before it quits. */
  let quitting = $state<SiteStatus[] | null>(null);
  /** A site's native view covers the page: the floats stand out of its rectangle (decisions §7). */
  const siteShown = $derived(route.view === "site" && siteStore.status.get(route.source)?.shown === true);
  $effect(() => {
    ackBand.site = siteShown;
  });
  // A site is drawn at the window's zoom (decisions §14).
  $effect(() => {
    const level = zoom.level;
    if (lifecycle.ready) void siteZoom(level).catch(() => {});
  });

  /** A key knobas took from a site (decisions §8): the same function its own key runs with the keyboard in knobas. */
  function siteKey({ source, key }: SiteKey) {
    switch (key) {
      case "search":
        return onsearch("");
      case "settings":
        return onnav("settings", navHref("settings"));
      case "timer":
        return ontimer?.();
      case "reload":
        return void reloadSite(source);
      case "back":
        if (!siteBack(source)) nav.back();
        return;
      case "forward":
        if (!siteForward(source)) nav.forward();
        return;
      case "zoom-in":
        return zoom.step(1);
      case "zoom-out":
        return zoom.step(-1);
      case "zoom-reset":
        return zoom.reset();
      case "close":
        return void closeSite(source);
      case "bar":
        void siteDo(source, "focus-shell", false).catch(() => {});
        document.querySelector<HTMLElement>(".tf-kept:not([hidden]) .tf-site-at")?.focus({ preventScroll: true });
        return;
      case "esc-armed":
        return armSite(source);
      case "esc-leave":
        return leaveSite(source);
    }
  }

  /** A site's first `Esc`: its cap down, and the reads of the page the second lands on started (Fable's ruling, 2026-11-25). */
  function armSite(source: string) {
    siteStore.arm(source, siteStore.status.get(source)?.esc_again_ms ?? 500);
    const landing = siteStore.landing.get(source);
    if (landing?.kind === "page") readDocsPageAhead(landing.entityId);
  }

  /**
   * A site's second `Esc`: where `Esc` goes now. While the mirror is still asked what the site's address names (a read
   * of the local mirror, a few ms), it waits for that answer, at most {@link SITE_SETTLE_MS}, the first cap held down.
   */
  function leaveSite(source: string) {
    const go = () => {
      siteStore.disarm(source);
      const to = back;
      if (route.view === "site" && route.source === source && to !== null) leave(to.hash, to.opened);
    };
    const settling = siteStore.pending.get(source);
    if (settling === undefined) return go();
    void Promise.race([settling.catch(() => {}), new Promise((resolve) => setTimeout(resolve, SITE_SETTLE_MS))]).then(go);
  }

  /** Navigate to an external link handed off via `knobas://` or Firefox Native Messaging (ADR-0061). */
  function openHandoff(target: LinkHandoffTarget) {
    if (target.kind === "entity") {
      go(entityAddress(target.entity_kind, target.entity_id));
      return;
    }
    if (target.kind === "site") {
      void openSiteAt(target.source, target.url);
      return;
    }
    pushToast({
      text: "knobas holds nothing at that address yet.",
      action: { label: "Open in browser", run: () => openInBrowser(target.url) },
    });
  }

  /** Where a link a site may not show went (decisions §6): the bar says so for six seconds. */
  function siteRouted(routed: SiteRouted) {
    const name = siteStore.name(routed.source);
    if (routed.to === "site" && routed.other !== null) {
      openWeb(routed.url, { source: routed.other });
      return;
    }
    if (routed.to === "refused") {
      siteStore.say(routed.source, { text: `${routed.host} links do not open from a site.` });
      return;
    }
    openInBrowser(routed.url);
    let origin: string | null = null;
    try {
      origin = new URL(routed.url).origin;
    } catch {
      origin = null;
    }
    const text = `Opened ${routed.host} in your browser. It is not one of ${name}'s addresses.`;
    if (origin === null) {
      siteStore.say(routed.source, { text });
      return;
    }
    const allowed = origin;
    siteStore.say(routed.source, {
      text,
      action: {
        label: "Allow it here",
        run: () =>
          void siteAllow(routed.source, allowed)
            .then((setting) => {
              siteStore.holdOne(setting);
              siteStore.say(routed.source, null);
            })
            .catch(() => {}),
      },
    });
  }

  /** A site's download (decisions §11): said in its bar, never a toast over the site. */
  function siteDownload(download: SiteDownload) {
    if (download.state === "saved") {
      siteStore.say(download.source, {
        text: `Saved ${download.name} to Downloads.`,
        action: { label: "Show in folder", run: () => void siteRevealDownload(download.id).catch(() => {}) },
      });
    } else if (download.state === "failed") {
      siteStore.say(download.source, { text: `Could not save ${download.name}.` });
    }
  }

  /** `Esc`'s step to `hash` (rungs 5 to 8), `opened` when it is the entry's opener. */
  function leave(hash: string, opened: boolean) {
    // A site's landing takes the site's entry in place (Fable's ruling, 2026-11-25).
    if (route.view === "site" && back?.lands === true && hash === back.hash) {
      landFromSite(hash);
      return;
    }
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
   * Over a site's bar they are the site's back and forward first, as ⌘[ and ⌘] are
   * (#2528); over the site's page the native hook decides the same in Rust.
   */
  function onmouse(event: MouseEvent) {
    if (event.button !== 3 && event.button !== 4) return;
    event.preventDefault();
    if (event.type !== "mouseup" || dialogOpen()) return;
    travelTo(event.button === 3 ? "back" : "forward");
  }
  const MOUSE = ["mousedown", "mouseup", "auxclick"] as const;

  // First, so that a press it swallows reaches nothing else.
  onMount(() => installPressGuard());
  // A theme that could not be read at start was not drawn: said once, with the way to it (ADR-0055).
  onMount(() => {
    for (const { theme: t, kind } of unreadableAtStart.splice(0))
      pushToast({ text: `${t.name} is hard to read, so ${kind === "dark" ? "knobas Dark" : "knobas Light"} is drawn.`, action: { label: "Open Theme", run: () => go("#/settings/appearance/theme") } });
  });
  onMount(() => installFindKey(() => onsearch(sectionFilter(route))));
  onMount(() => installRowRoom());
  onMount(() => installPlainTyping());
  // A link to a web page opens in the browser, never in this window (Björn, 2026-11-13).
  onMount(() => installExternalLinks());
  onMount(() => installFocusKeep());
  onMount(() => installKeysHere());
  // A select rings after a key, not after a press (design system, Motion and focus).
  onMount(() => installInputModality());
  onMount(() => installZoomInput(zoom));
  // So no page's first draw waits on a face and lays itself out again when it lands (the render-cost pass, 2026-11-14).
  onMount(() => loadEveryFace());
  // So a pull request's first *Files changed* runs warm code (the forge-rounds pass, 2026-11-15).
  onMount(() => warmDiffView());
  // So the first XML or JSON file opened after a start is drawn by warm code, its check by workers already up (the XML
  // view's first open, 2026-11-25). Its module loads once the sections built ahead are and the reader is still, never
  // in the start's chunks.
  onMount(() => void aheadBuilt.then(quietAndIdle).then(() => import("./render/doc/doc-warm")).then((m) => m.warmXmlView(), () => {}));
  // So a README's first diagram is drawn by warm code, inside the text's hold (#2510).
  onMount(() => warmDiagrams(renderPort()));
  // So a Docs page kept hidden comes back laid out at Reading's new size (the ux18 Windows pass, Fable's ruling).
  onMount(() => warmDocs());
  // So a section's first press shows a page already built (the ux13-overs pass, 2026-11-15).
  onMount(() => openAhead(() => lifecycle.ready, () => sectionOf(route), (section) => untrack(() => navHref(section))));
  onMount(() => {
    // Each box's place first, while the last frame's layout is clean: a page the move hides takes it from there.
    const onhash = () => {
      noteScrolls();
      // A repository's landing reads before its page is built (Fable's ruling, the compare-first-open pass).
      readLandingAtAddress(location.hash);
      arrive();
    };
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

<div class="tf" class:tf-site-shown={siteShown}>
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
    zen={zen.here ? { on: zen.on, toggle: toggleZen } : null}
  />
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions: not a control, it notes what a press on a page was on -->
  <main class="tf-main" class:tf-zen={zen.shown} tabindex="-1" bind:this={mainEl} onclickcapture={(event) => (pressed = event.target instanceof Element ? event.target : null)}>
    <TfPage
      {kept}
      stale={recoloured}
      ctx={{
        route,
        ready: lifecycle.ready,
        job: chosen?.key ?? null,
        jobAt: chosen?.entityId ?? null,
        back,
        came: here.opener === null ? null : { hash: here.opener.hash, name: here.opener.name, opened: true },
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
  <TfStatusBar {readings} pending={queue.counts.pending} decisions={decisionsIn(queue.counts)} onqueue={openWriteQueue} {zoom} />
  {@render dialogs?.(sync)}
  {#if writeQueuePanel.open}
    <WriteQueue {queue} {now} {layers} onclose={() => (writeQueuePanel.open = false)} />
  {/if}
  {#if quitting !== null && quitting.length > 0}
    <TfSiteQuit unsaved={quitting} onclose={() => (quitting = null)} {layers} />
  {/if}
  {#if secretPrompt.source !== null}
    <TfSecretPromptHost sourceId={secretPrompt.source} {layers} ports={secretPorts} />
  {/if}
  <TfRunHost />
  <TfToasts />
  <TfThemeKeep />
  <TfAck />
  <TfZoomLevel {zoom} />
  <div class="tf-floats"></div>
  <!-- The right-click menu (2026-11-16): one for the window, in the float host. -->
  <TfContextMenu
    {onsearch}
    {onstarttimer}
    canTime={ontimer !== undefined}
    job={chosen === null ? null : { entityId: chosen.entityId, label: chosen.key }}
    {layers}
  />
</div>

<!--
  Docs (#859 to #862, requirements §10): one screen for `#/docs`,
  `#/docs/space/[<source>:]<key>` and `#/page/<id>[/<section>]`, as prototype-6's
  `renderDocs` → `renderPage`. `#/docs` is the Docs home (`TfDocsHome.svelte`):
  the tree on the left and no page open, and it is where the crumb and *Back to
  Docs* lead.

  **Left**, the space picker (*All spaces (N)*, then every space with its page
  count; it follows the open page's space until you pick one or a search
  starts), and under it *For you* and the page tree, or, while a search is
  on, its words with *Clear* and the hits (R-10.1 to R-10.4, R-10.8). Docs
  draws no search field (#2425): `/` opens the launcher on `#d`, whose
  *Search Docs for …* row opens `#/docs/search/<words>`, a search over every
  space; a label chip searches for itself. A search's address opens no page: the
  home stays in the main column until a hit is opened, and says there how many
  pages match, or that none does; arriving, the first hit takes focus. *New page* is at the
  bottom. Below 900px (`READER_DRAWER`, through `MediaQuery`) it is a drawer
  over the page (#2084, R-10.9): *Pages* at the start of the breadcrumb row
  and a label chip open it; `Esc` (it is a `drawer` layer), the backdrop
  and a page picked in it close it, and focus goes back to what opened it.

  **Right**, the page in the reading face, nothing in it capped in width
  (#2081, R-10.9): the breadcrumb, the title, who edited it when, who created it (#2080), the
  version, the reading time and the labels (each searches for itself). The
  title line is the canonical header (#2286): on its right the actions with
  *More ▾* (#2082), "Open in <source>" as a ghost, the page's *Refresh* or
  *Sync now* (#2136: `TfDetailRefresh` over `get_entity` of the page), and last
  the view's *Sync now* (the role frame's `sync`, #2106), left out while the
  page's own button is a *Sync now* (one per header, #2287), and in *More* where
  the row has no room for it (`readerFit`). The withdrawn, turned-off and
  failed-refresh banners sit under the meta line, then the blocks by section (*Edit section*),
  the page foot (*Work it belongs to*, *Linked from*, `TfDocFoot`) and the
  comments with the composer (R-10.9 to R-10.15). A page opened from a hit
  keeps the query: every match is marked and the find bar steps through them
  (R-10.5, R-10.12). The outline (`TfDocOutline`, R-10.15) is the page's own
  right column, sticking while the page scrolls; where the header's actions would
  stop fitting one row beside it (`readerFit`, by the sheet's own width) it folds
  between the meta line and the first section instead. Every page opened is a visit
  (`visit_entity`), which is what the home's *Continue reading* reads.

  Drawn over the declared Docs frame (ADR-0022): `docs_tree`, `docs_page`,
  `docs_search`, and `get_entity` of the open page for its *Refresh* (#2136);
  the writes go through the write queue.

  **This job** (#863, R-2.5, R-2.8): with a job current, *This job <key> N /
  All pages* sits above the space picker. On, the left column lists the pages
  the ticket reaches (`job_reach`, read once per job), grouped by reason, the
  space picker hides, and the search is kept to those pages with *Search all
  pages*. Off by default; the job's Pages section opens Docs with it on, and
  the choice is kept for the session (`sessionFilter("docs")`).

  **What Docs queued is drawn at once** (#1424, `tf/queued.svelte.ts`): a saved
  section's text in place of the old until the page's version moves, a comment
  at the end of the list until the page holds its words, a new page as a muted
  row with no link under its parent until the tree holds its title. Each write
  is an echo with the one lifecycle (#2102): *queued*, then *sent* once its
  write is sent, and gone on the read that shows it, which the end of every
  run and a refresh naming the write or its page bring. Held or refused, the echo goes and a toast offers *Edit and
  resend*, which opens the section editor, the composer or *New page* again
  with what was typed.
-->
<script lang="ts">
  import TfStateChip from "../TfStateChip.svelte";
  import { clockLabel } from "../../time/format";
  import { tick, untrack } from "svelte";
  import type { RoleFrameProps } from "../page-entry";
  import { MediaQuery } from "svelte/reactivity";
  import { sourceRoles, WIKI } from "../roles.svelte";

  import { ipcErrorMessage, isIpcError } from "../../ipc";
  import {
    docsCreatePage as realDocsCreatePage,
    docsEditSection as realDocsEditSection,
    docsExportMarkdown as realDocsExportMarkdown,
    docsPage as realDocsPage,
    docsSearch as realDocsSearch,
    docsTree as realDocsTree,
    getEntity as realGetEntity,
    jobReach as realJobReach,
    submitWrite as realSubmitWrite,
    visitEntity as realVisitEntity,
    type DocPage,
    type DocsHits,
    type DocsTree,
    type EntityDetail,
    type LinkEntry,
    type NewPage,
    type Reached,
  } from "../../ipc/entity";
  import { drawn } from "../../detail/relations";
  import type { PipelinePorts } from "../builds/pipeline-ports";
  import { writeFollower, type WriteFollower } from "../../shell/follow-write";
  import { openExternal as realOpenExternal } from "../../shell/open-external";
  import { askInto, refusalOf, refusedAttrs, type RefusedWrite, type RefusedWritesPort, type WriteAsk } from "../../shell/refused-writes";
  import { toastQueued, type WriteSource } from "../../shell/queued-toast";
  import { push, undoable } from "../../shell/toasts.svelte";
  import { sessionFilter } from "../filter/filter-state.svelte";
  import { layers as sharedLayers, type Layers } from "../layers.svelte";
  import { resolveTicket, username } from "../job/job";
  import TfLinkTo from "../job/TfLinkTo.svelte";
  import type { JobPorts } from "../job/ports";
  import { askAgain, echoWrite, echoesOn, sameWords, settleEchoes, takeAgain, watchEchoes, type Echo } from "../queued.svelte";
  import { createDetailRefresh, DETAIL_REFRESH_BRIDGE, type DetailRefreshPorts } from "../refresh.svelte";
  import TfDetailBanners from "../TfDetailBanners.svelte";
  import TfBareHeader from "../TfBareHeader.svelte";
  import TfScope from "../filter/TfScope.svelte";
  import TfBackHint from "../TfBackHint.svelte";
  import TfCrumbs from "../TfCrumbs.svelte";
  import TfNothingHere from "../TfNothingHere.svelte";
  import TfDocsSpace from "./TfDocsSpace.svelte";
  import TfRailBar from "../TfRailBar.svelte";
  import TfScroller from "../TfScroller.svelte";
  import TfDetailRefresh from "../TfDetailRefresh.svelte";
  import TfHeaderRefresh from "../TfHeaderRefresh.svelte";
  import TfKey from "../TfKey.svelte";
  import { initials } from "../tickets/tickets";
  import TfDocBlocks from "./TfDocBlocks.svelte";
  import TfDocBody from "./TfDocBody.svelte";
  import TfDocFoot from "./TfDocFoot.svelte";
  import TfDocHits from "./TfDocHits.svelte";
  import TfDocMore from "./TfDocMore.svelte";
  import TfDocOutline from "./TfDocOutline.svelte";
  import TfDocsHome from "./TfDocsHome.svelte";
  import TfDocTree from "./TfDocTree.svelte";
  import TfKeyed from "../TfKeyed.svelte";
  import { counted } from "../counted";
  import TfMarked from "./TfMarked.svelte";
  import TfNewPage from "./TfNewPage.svelte";
  import {
    copyLinkText,
    createdBy,
    LINK_RELATION,
    blockWords,
    firstName,
    forYouHash,
    forYouIn,
    forYouWhy,
    inlineWords,
    jobPageGroups,
    mentionsMe,
    noteReading,
    pageHash,
    pageToOpen,
    spaceAt,
    READER_DRAWER,
    readerFit,
    person,
    placeOf,
    reachedPages,
    readingAt,
    readingMinutes,
    revealMatch,
    scopeName,
    SEARCH_PAUSE_MS,
    sectionKind,
    sectionsOf,
    spaceHash,
    spaceId,
    spaceLabel,
    spaceOfPage,
    stemsOf,
    provideEmbeds,
  } from "./docs";
  import { replaceAddress } from "../opener";
  import { namePage } from "../page-name.svelte";
  import { pageVisible } from "../kept-pages.svelte";
  import { linkedOn } from "../job/connections";
  import type { EscTarget } from "../escape";
  import { filterKeys } from "../filter-keys";
  import { drafts } from "../drafts.svelte";
  import { sendKey } from "../inline-editor";

  /** The page's own reads and writes; *Link to…* uses the job's ports. */
  export interface DocsPorts {
    docsTree: typeof realDocsTree;
    docsPage: typeof realDocsPage;
    docsSearch: typeof realDocsSearch;
    docsExportMarkdown: typeof realDocsExportMarkdown;
    docsEditSection: typeof realDocsEditSection;
    docsCreatePage: typeof realDocsCreatePage;
    submitWrite: typeof realSubmitWrite;
    visitEntity: typeof realVisitEntity;
    /** What a ticket reaches, for *This job* (#863). */
    jobReach: typeof realJobReach;
    /**
     * The open page's row (#2136): whether its source reads one page
     * (`refreshes`), is turned on, and still holds it, for *Refresh* and the
     * two banners. A row that cannot be read draws neither.
     */
    getEntity: typeof realGetEntity;
    /** The header's *Refresh* (#2136); the bridge's when omitted. */
    refresh?: DetailRefreshPorts;
    openExternal: (url: string) => Promise<void>;
    copy: (text: string) => Promise<void>;
    /** What the page's source refuses its credential (#2053); the IPC read when omitted. */
    refusedWrites?: RefusedWritesPort;
    /** Hears `sync:state`, whose run ends read Docs again (#2072), and `item:refreshed` (#2102); the bridge's when omitted. */
    listen?: PipelinePorts["listen"];
    /** The pause in typing the search waits for, in ms (#2200); `SEARCH_PAUSE_MS` when omitted, 0 asks on every keystroke. */
    searchPause?: number;
  }

  type LinkPorts = Pick<JobPorts, "listSources" | "getEntity" | "createLink" | "unlink" | "search" | "launcherHome">;

  let {
    ready,
    rest,
    job,
    jobAt = null,
    ports,
    linkPorts,
    layers = sharedLayers,
    follower = writeFollower(),
    back = null,
    sync,
  }: {
    ready: boolean;
    /** What follows the address's first segment. */
    rest: string[];
    /** The chosen job (spec #2227), for *This job* and *Link to <job>*. */
    job: string | null;
    /** The address the job was opened at (#1525), which its ticket is read by; its key when omitted. */
    jobAt?: string | null;
    ports?: DocsPorts;
    linkPorts?: LinkPorts;
    layers?: Layers;
    /** Follows each queued edit, comment and page, so one held or refused drops its echo (#1424). */
    follower?: WriteFollower;
    /** Where `Esc` goes (`escTarget`), which the doc page's breadcrumb names; `null` where it does nothing. */
    back?: Pick<EscTarget, "name"> | null;
    // The frame's `syncRevision` (#2106) is left unread: this view reads
    // again on the end of every run of its role's sources already.
  } & RoleFrameProps = $props();

  const io: DocsPorts = $derived(
    ports ?? {
      docsTree: realDocsTree,
      docsPage: realDocsPage,
      docsSearch: realDocsSearch,
      docsExportMarkdown: realDocsExportMarkdown,
      docsEditSection: realDocsEditSection,
      docsCreatePage: realDocsCreatePage,
      submitWrite: realSubmitWrite,
      visitEntity: realVisitEntity,
      jobReach: realJobReach,
      getEntity: realGetEntity,
      openExternal: realOpenExternal,
      copy: (text) => navigator.clipboard.writeText(text),
    },
  );
  /**
   * The tree as read: the read narrows itself to the sources filling the wiki
   * role, so nothing here keeps spaces by source (#1296, ADR-0029 as amended
   * on 2026-10-19). The search is narrowed the same way.
   */
  let tree = $state.raw<DocsTree | null>(null);
  let treeError = $state<string | null>(null);
  let page = $state<DocPage | null>(null);
  // Whether the open page's source runs its embeds, for every embed drawn below (#2440).
  provideEmbeds(() => page?.run_embeds ?? true);
  // What *Back to <name>* calls Docs once it is left: the page's title, the space's name or the search, as the crumb
  // says (spec #2227). One name, here, because a read answering after the crumb drew would otherwise clear the crumb's.
  namePage(() => page?.title ?? spaceHere?.name ?? spaceHere?.key ?? (place.kind === "search" ? "Search" : null));
  let pageError = $state<string | null>(null);
  /** The reader's own accounts, as the last page read names them; until a page is read, as the sources' settings do, so a hit's line says *you* from the start (F-428). */
  let me = $state<string[]>([]);
  /** The page could not be opened because no such page is held. */
  let pageMissing = $state(false);
  /** The picked space (`spaceId`), `*` for all, or `null` to follow the page. */
  let picked = $state<string | null>(null);

  /** The search's words (#860): the address's (#2425) or a label chip's. */
  let query = $state("");
  let found = $state<DocsHits | null>(null);
  /** The words `found` answers, which an earlier search's list may not be until the new one is in. */
  let foundFor = $state<string | null>(null);
  let searchError = $state<string | null>(null);
  /** What the find bar marks: the needles a hit was opened with. */
  let find = $state("");
  let treeNav = $state<HTMLElement | null>(null);

  // ---- *This job* (#863) ----
  const filter = sessionFilter("docs");
  /** The read answered, and with no space: there is nothing to narrow, so *This job* is not offered and does not hold. */
  const nothingHeld = $derived(tree !== null && tree.spaces.length === 0);
  const jobOn = $derived(job !== null && filter.scope === "job" && !nothingHeld);
  /** What the job's ticket reaches, `null` until the read answers. */
  let reach = $state<Reached[] | null>(null);
  /** The titles of the first steps a page is reached through, for *Via …*. */
  let stepTitles = $state<Map<string, string>>(new Map());

  async function readReach(key: string): Promise<{ reach: Reached[]; titles: Map<string, string> }> {
    const ticket = await jobTicket(key);
    const rows = ticket === null ? [] : await io.jobReach(ticket);
    const steps = [...new Set(rows.filter((r) => r.step !== r.entity_id).map((r) => r.step))];
    const titles = new Map<string, string>();
    if (linkPorts && steps.length > 0) {
      const read = await Promise.allSettled(steps.map((id) => linkPorts!.getEntity(id)));
      read.forEach((answer, i) => {
        if (answer.status === "fulfilled") titles.set(steps[i]!, answer.value.row.title);
      });
    }
    return { reach: rows, titles };
  }

  /** What the job's ticket is read by: the address it was opened at (#1537), else its key. */
  const jobRef = $derived(job === null ? null : (jobAt ?? job));
  $effect(() => {
    const key = jobRef;
    reach = null;
    if (!ready || key === null) return;
    let live = true;
    untrack(() =>
      readReach(key).then(
        (answer) => {
          if (!live) return;
          reach = answer.reach;
          stepTitles = answer.titles;
        },
        // A reach that cannot be read lists nothing; *All pages* still reads.
        () => {
          if (live) reach = [];
        },
      ),
    );
    return () => {
      live = false;
    };
  });

  const jobIds = $derived(tree === null || reach === null ? null : reachedPages(reach, tree));
  const jobGroups = $derived(
    tree === null || reach === null || job === null ? [] : jobPageGroups(reach, tree, job, stepTitles),
  );

  const place = $derived(placeOf(rest));
  /** The last visit on its way: a read of the tree that overtook it would name the page before it. */
  let visited: Promise<unknown> = Promise.resolve();
  // *Continue reading* is the tree's: back on the home, it is read again once the visit is in, so it names the page read last.
  let wasHome = false;
  $effect(() => {
    const now = place.kind === "home";
    if (now && !wasHome && ready && untrack(() => tree) !== null) untrack(() => void visited.then(() => readTree(true)));
    wasHome = now;
  });
  const target = $derived(tree === null ? null : pageToOpen(place));
  /** `#/docs/space/<key>`: the space's own page, which lists its pages. */
  const spaceHere = $derived(tree === null ? undefined : spaceAt(place, tree));
  /** `#/docs`, and a search until one of its hits is opened: the Docs home, which opens no page. */
  const home = $derived(place.kind === "home" || place.kind === "search");
  /** Whether some source that is turned on fills the wiki role: what an empty tree is waiting for. */
  const hasSource = $derived(sourceRoles.syncs(WIKI));

  /** The reads asked so far; an answer to an older one than the last is dropped. */
  let treeAsked = 0;
  let pageAsked = 0;

  /**
   * Read the tree; a `quiet` read (a sync run's, #2072) that fails leaves the
   * tree as drawn. With none drawn yet it stands in for the first read it
   * dropped, so its error shows.
   */
  async function readTree(quiet: boolean) {
    const mine = ++treeAsked;
    try {
      const read = await io.docsTree();
      if (mine !== treeAsked) return;
      // A row of the home's that holds focus can move: the page read last goes up into *Continue reading* (F-438).
      const held = document.activeElement?.closest("a.tf-dh-row")?.getAttribute("href") ?? null;
      tree = read;
      treeError = null;
      settlePages(read);
      if (held !== null) void tick().then(() => keepRow(held));
    } catch (cause) {
      if (mine === treeAsked && (!quiet || tree === null)) treeError = ipcErrorMessage(cause);
    }
  }

  /** The home's row for `href` takes focus back, when the read that moved it left focus on nothing. */
  function keepRow(href: string): void {
    const at = document.activeElement;
    if (at !== null && at !== document.body && at.isConnected) return;
    [...(main?.querySelectorAll<HTMLElement>("a.tf-dh-row") ?? [])].find((row) => row.getAttribute("href") === href)?.focus({ preventScroll: true });
  }

  /**
   * Read page `id`. Opening it records the visit; a `quiet` read (a sync
   * run's, #2072) records none, and one that fails leaves the page as drawn.
   * A quiet read that dropped the opening read before `id` was drawn opens it
   * in that read's place: visit and error as an opening read. Answers whether
   * what it read is what is drawn now.
   */
  function readPage(id: string, quiet: boolean): Promise<boolean> {
    const mine = ++pageAsked;
    return io.docsPage(id).then(
      (answer) => {
        if (mine !== pageAsked) return false;
        const opens = !quiet || page?.entity_id !== id;
        page = answer;
        me = answer.me;
        pageError = null;
        if (opens) visited = io.visitEntity(id).catch(() => {});
        return true;
      },
      (cause: unknown) => {
        if (mine === pageAsked && (!quiet || page?.entity_id !== id)) {
          pageError = ipcErrorMessage(cause);
          pageMissing = isIpcError(cause) && cause.code === "not_found";
        }
        return false;
      },
    );
  }

  /**
   * Read the open page `id` and its row again, after a link, a sync run or a
   * refresh: no visit, and nothing once the reader has opened another page
   * (#2072, #2136).
   */
  function rereadPage(id: string | null): Promise<boolean> {
    if (id === null || id !== target) return Promise.resolve(false);
    return Promise.all([readPage(id, true), readDetail(id, true)]).then(([read]) => read);
  }

  // ---- *Refresh* (#2136, #2107) ----
  /** The open page's row (`get_entity`), `null` until read or when it cannot be. */
  let detail = $state<EntityDetail | null>(null);
  let detailAsked = 0;
  /** The header's *Refresh* or *Sync now*. */
  const refresh = createDetailRefresh(untrack(() => io.refresh ?? DETAIL_REFRESH_BRIDGE));

  /** Read page `id`'s row; a `quiet` read that fails keeps the row drawn. */
  function readDetail(id: string, quiet: boolean): Promise<void> {
    const mine = ++detailAsked;
    return io.getEntity(id).then(
      (answer) => {
        if (mine === detailAsked) detail = answer;
      },
      () => {
        if (mine === detailAsked && !quiet) detail = null;
      },
    );
  }

  /** *Try again* in the failed line: that line goes while it asks, so focus lands on what stands in its place after. */
  /**
   * A refresh's re-read: the tree, the open page and its row, each over what
   * Docs drew (#2072's quiet reads), so the picked space, the search, an open
   * section editor and the composer keep what they hold. A press of *Refresh*
   * gets its answer here, because nothing on the page need change (F-445); a
   * run's end reads the same way and has its own word in the button.
   */
  function reread(): Promise<void> {
    const pressed = refresh.reading;
    const was = page;
    return Promise.all([readTree(true), rereadPage(target)]).then(([, read]) => {
      if (pressed && read && was !== null && page?.entity_id === was.entity_id) sayRefreshed(was.version, page);
      else if (pressed) refresh.answer(null);
    });
  }

  /** What a press of *Refresh* found, which the press ends in. */
  function sayRefreshed(before: DocPage["version"], now: DocPage): void {
    const version = now.version === null ? "" : `version ${now.version}`;
    const moved = now.version !== before;
    const text = moved ? `Refreshed${version === "" ? "" : `, now ${version}`}` : `Page is up to date${version === "" ? "" : `, ${version}`}`;
    refresh.answer(text);
  }

  $effect(() => {
    if (!ready) return;
    untrack(() => void readTree(false));
  });

  $effect(() => {
    const id = target;
    if (!ready) return;
    pageError = null;
    detail = null;
    // No page to open: what was open goes, or the home would draw under its header.
    if (id === null) {
      page = null;
      return;
    }
    untrack(() => {
      refresh.reset();
      void readPage(id, false);
      void readDetail(id, false);
    });
    // Another page, or none: the reads asked for this one are dropped.
    return () => {
      ++pageAsked;
      ++detailAsked;
    };
  });

  // A sync run's end (every run's, #2072) and a refresh naming one of Docs'
  // echoes (#2102) read the tree and the open page again, so what they brought
  // in settles what Docs queued without Docs being opened again.
  $effect(() => {
    if (!ready) return;
    return untrack(() =>
      watchEchoes(
        DOCS,
        () => {
          void readTree(true);
          void rereadPage(target);
        },
        { listen: io.listen, everyRun: true },
      ),
    );
  });

  const pageSpace = $derived(tree && page ? spaceOfPage(tree, page.entity_id) : undefined);
  /**
   * The breadcrumb's space link: the page's own space, named with its source
   * where another source holds its key too (#1455).
   */
  const crumbSpace = $derived(
    page?.space ? spaceHash(tree?.spaces ?? [], { source_id: page.source_id, key: page.space.key }) : null,
  );
  const shownSpace = $derived.by(() => {
    if (tree === null || picked === "*") return undefined;
    if (picked !== null) return tree.spaces.find((s) => spaceId(s) === picked);
    // The home shows every space, a space's page that space, and a page its own: one the tree does not hold shows them all.
    return place.kind === "page" ? spaceOfPage(tree, place.id) : spaceHere;
  });
  const pickerValue = $derived(shownSpace ? spaceId(shownSpace) : "*");
  const total = $derived(tree?.spaces.reduce((n, s) => n + s.pages.length, 0) ?? 0);
  const parent = $derived(page?.ancestors.at(-1));
  const current = $derived(page?.entity_id ?? target);
  const searching = $derived(query.trim() !== "");
  const forYou = $derived(forYouIn(tree?.for_you ?? [], shownSpace));

  // The search asks once typing pauses (#2200), for what was typed last; an
  // answer to an older keystroke is dropped.
  let asked = 0;
  /** Bumped by *Try again* on a search that could not ask (F-439). */
  let searchAttempt = $state(0);
  $effect(() => {
    void searchAttempt;
    const text = query;
    const scope = !jobOn && shownSpace ? { source_id: shownSpace.source_id, key: shownSpace.key } : null;
    if (!ready || text.trim() === "") {
      found = null;
      searchError = null;
      return;
    }
    const mine = ++asked;
    const search = io.docsSearch;
    const ask = () =>
      void search(text, scope).then(
        (answer) => {
          if (mine !== asked) return;
          found = answer;
          foundFor = text;
          searchError = null;
        },
        (cause: unknown) => {
          if (mine === asked) searchError = ipcErrorMessage(cause);
        },
      );
    const pause = io.searchPause ?? SEARCH_PAUSE_MS;
    if (pause <= 0) {
      ask();
      return;
    }
    const timer = setTimeout(ask, pause);
    return () => clearTimeout(timer);
  });

  // A search before any page is read: nothing has named the reader yet, and a hit would change from a name to *you* once opened.
  let meAsked = false;
  $effect(() => {
    const sources = linkPorts?.listSources;
    if (!searching || meAsked || sources === undefined || me.length > 0) return;
    meAsked = true;
    untrack(() =>
      void sources().then(
        (rows) => {
          const names = rows.flatMap((row) => username(row) ?? []);
          if (me.length === 0 && names.length > 0) me = names;
        },
        () => {},
      ),
    );
  });

  /** The hits as read, kept to the job's pages while *This job* is on. */
  const hits = $derived.by((): DocsHits | null => {
    if (found === null || !jobOn) return found;
    const kept = found.hits.filter((h) => jobIds?.has(h.entity_id) ?? false);
    return { ...found, total: kept.length, hits: kept };
  });
  const searchScope = $derived(jobOn ? `${job}'s pages` : scopeName(shownSpace));

  /** What the search can widen to: the whole of Docs from a space, every page from the job's. */
  const searchKept = $derived(jobOn ? ("job" as const) : shownSpace !== undefined ? ("space" as const) : null);

  /**
   * The Space picker. While a search is on it narrows the search. Otherwise a space goes to that space's page, which
   * opens no page (design system, Layout), and the address then says what the rail shows.
   * *All spaces* widens the rail on a page, and from a space's page goes back to the home.
   */
  function pick(event: Event): void {
    const value = (event.currentTarget as HTMLSelectElement).value;
    if (searching || tree === null) {
      picked = value;
      return;
    }
    if (value === "*") {
      picked = "*";
      if (place.kind === "space") location.hash = "#/docs";
      return;
    }
    const space = tree.spaces.find((s) => spaceId(s) === value);
    if (space === undefined) return;
    picked = null;
    location.hash = spaceHash(tree.spaces, space);
  }

  // `#/docs/search/<words>` (#2425): the address's words are the search, over
  // every space, as the launcher's `#d` was. Once per address: opening a hit
  // keeps the search, and *Clear* ends it.
  $effect(() => {
    if (place.kind !== "search") return;
    const words = place.words;
    untrack(() => {
      // Coming back from a hit is no arrival: the way back puts focus on the hit that was opened.
      arriving = query !== words;
      query = words;
      setFind("");
      picked = "*";
    });
  });

  // The home and a space's page say what the rail shows: nothing picked outlives the trip, and the home ends a search,
  // or `Esc` from a search's address would change nothing on screen.
  $effect(() => {
    const kind = place.kind;
    if (kind !== "home" && kind !== "space") return;
    untrack(() => {
      picked = null;
      if (kind === "home") endSearch();
    });
  });

  /** The search is over: the tree is back, and the highlighting and the search's *All spaces* gone. */
  function endSearch(): void {
    query = "";
    setFind("");
    picked = null;
    fromChip = null;
  }

  /** *Try again* on a search that could not ask: the words stay, and what it finds takes focus, as on arriving. */
  function retrySearch(): void {
    arriving = place.kind === "search";
    searchAttempt += 1;
  }

  // Leaving a search's address by anything but one of its hits leaves the search, as `Esc` does a row's narrowing.
  let onSearch = false;
  let openingHit = false;
  $effect(() => {
    const now = place.kind === "search";
    if (onSearch && !now && !openingHit) untrack(endSearch);
    onSearch = now;
    openingHit = false;
  });

  /** End the search, and a search's address becomes the page's. */
  function clearSearch(): void {
    endSearch();
    if (place.kind === "search") replaceAddress(current === null ? "#/docs" : pageHash(current));
  }

  /** Open a hit with its query carried over. */
  function openHit(entityId: string): void {
    setFind(found?.needles.join(" ") ?? "");
    fromChip = null;
    openingHit = true;
    location.hash = pageHash(entityId);
  }

  /** Widen a search kept to the job's pages or to one space. */
  function widenSearch(): void {
    if (jobOn) filter.scope = "all";
    else picked = "*";
  }

  /** A search's address that found nothing, or could not ask: the main column says so, where a page would be. */
  const answerHere = $derived(place.kind === "search" && (searchError !== null || hits?.hits.length === 0));
  /** A search's address that found pages: they are in the rail, and the main column says how many, so it does not read as nothing happened. */
  const hitsHere = $derived(place.kind === "search" && foundFor === place.words && (hits?.hits.length ?? 0) > 0);

  function searchLabel(label: string, chip: HTMLElement): void {
    query = label;
    setFind("");
    fromChip = chip;
    openTree(chip);
  }

  /** The label chip a search on a page came from: `Esc` ends that search and goes back to it, and leaves the page only after (F-437). */
  let fromChip = $state.raw<HTMLElement | null>(null);
  const shown = pageVisible();
  $effect(() => {
    if (fromChip === null || !shown()) return;
    return untrack(() =>
      layers.add({
        kind: "panel",
        close: () => {
          const chip = fromChip;
          clearSearch();
          chip?.focus();
        },
      }),
    );
  });

  // ---- below 900px the tree is a drawer over the page (#2084) ----
  /** A window narrower than 900px, where the tree leaves the grid and the Pages button opens it. */
  const drawerBand = new MediaQuery(READER_DRAWER);
  let drawer = $state(false);
  /** What opened the drawer, which focus goes back to when it closes. */
  let opener: HTMLElement | null = null;

  function openDrawer(from: Element | null): void {
    if (!drawer) opener = from instanceof HTMLElement ? from : null;
    drawer = true;
  }

  function closeDrawer(): void {
    if (!drawer) return;
    drawer = false;
    const back = opener;
    opener = null;
    if (back !== null && document.contains(back)) back.focus();
  }

  /** Below 900px, the drawer opened from `from`, with focus in it once it is drawn; wider, the tree is in its column already. */
  function openTree(from: Element | null): void {
    if (!drawerBand.current) return;
    openDrawer(from);
    void tick().then(() => treeNav?.focus());
  }

  // Widening the window past 900px resets the drawer: the tree is back in its column.
  $effect(() => {
    if (drawerBand.current) return;
    drawer = false;
    opener = null;
  });

  /** A search's address just arrived at, whose first hit takes focus once the list is in: after the launcher, focus is on nothing. */
  let arriving = false;
  /** Whether the reader went somewhere of their own while the search was asked: a field or a dialog keeps its focus. */
  const focusTaken = () => document.activeElement?.closest("input, textarea, select, [role='dialog']") != null;

  // Below 900 px a search's hits are in the closed drawer: a search that found some opens it, once per address.
  let drewHitsFor: string | null = null;
  $effect(() => {
    const words = place.kind === "search" ? place.words : null;
    if (words === null) drewHitsFor = null;
    if (words === null || !hitsHere || drewHitsFor === words) return;
    drewHitsFor = words;
    untrack(() => {
      if (drawerBand.current) openDrawer(main?.querySelector(".tf-railbar button") ?? null);
      if (!arriving) return;
      arriving = false;
      void tick().then(() => {
        if (!focusTaken()) treeNav?.querySelector<HTMLElement>("a.tf-dl-hit")?.focus();
      });
    });
  });

  // Open, the drawer is a layer of the shell's, so `Esc` closes it wherever focus is (rung 2).
  $effect(() => {
    if (!drawer) return;
    const off = untrack(() => layers.add({ kind: "drawer", close: closeDrawer }));
    return () => untrack(off);
  });

  /** `Esc` inside the drawer closes it here, and a page picked in it closes it too. */
  function ondrawerkey(event: KeyboardEvent): void {
    if (!drawer || event.key !== "Escape") return;
    event.preventDefault();
    closeDrawer();
  }

  function ondrawerclick(event: MouseEvent): void {
    if (drawer && event.target instanceof Element && event.target.closest("a[href]")) closeDrawer();
  }

  // ---- the find bar (R-10.12) ----
  const stems = $derived(stemsOf(find));
  let article = $state<HTMLElement | null>(null);
  let main = $state<HTMLElement | null>(null);
  let marks = $state(0);
  let at = $state(-1);

  /** The page's matches as drawn, counted for the find bar. */
  function matches(): HTMLElement[] {
    const all = article ? [...article.querySelectorAll("mark")] : [];
    marks = all.length;
    return all;
  }

  /** Mark match `at` as the current one. */
  function markCurrent(all: HTMLElement[]): void {
    all.forEach((m, i) => m.classList.toggle("tf-cur", i === at));
  }

  function jump(step: number): void {
    const all = matches();
    if (all.length === 0) return;
    at = (at + step + all.length) % all.length;
    markCurrent(all);
    const current = all[at];
    if (current) revealMatch(current);
    current?.scrollIntoView?.({ block: "center" });
  }

  /** The page and the query the find bar last started on. */
  let started: { id: string; stems: string[] } | null = null;
  /** *Continue reading* pressed: the page it names, drawn, goes back to where it was left (F-431). */
  let resuming: { id: string; top: number } | null = null;

  /** A page opened at its mention: the newest comment that names the reader, else the first mention in its body. */
  function toMention(): void {
    const comments = article?.querySelectorAll<HTMLElement>(".tf-msg.tf-mention");
    const at = comments?.[comments.length - 1] ?? article?.querySelector<HTMLElement>(".tf-ment.tf-me");
    at?.scrollIntoView?.({ block: "center" });
  }

  /** *For you*'s row on the page it names changes no address once it is there, so it jumps from here too. */
  const mentionHere = $derived(place.kind === "page" && place.mention === true && page?.entity_id === place.id);
  $effect(() => {
    if (mentionHere) void tick().then(toMention);
  });

  // A page drawn with a query marks its first match, unless the address names
  // a section, which is scrolled to instead. The same page read again (a sync
  // run's, #2072) stays where the reader is, on the match they stepped to.
  $effect(() => {
    const p = page;
    const query = stems;
    if (p === null) return;
    const again = started?.id === p.entity_id && started.stems === query;
    started = { id: p.entity_id, stems: query };
    void tick().then(() => {
      if (resuming?.id === p.entity_id) {
        const { top } = resuming;
        resuming = null;
        // A frame on too: opening an address sets the page's scroller back to the top as it shows the page.
        const back = () => {
          if (main !== null) main.scrollTop = top;
        };
        back();
        requestAnimationFrame(back);
      }
      if (again) {
        const all = matches();
        at = Math.min(at, all.length - 1);
        markCurrent(all);
        follow();
        return;
      }
      at = -1;
      if (place.kind === "page" && place.section !== null) {
        document.getElementById(`s-${place.section}`)?.scrollIntoView?.();
        matches();
      } else if (stems.length > 0) jump(1);
      else marks = 0;
      follow();
    });
  });

  function onfindkey(event: KeyboardEvent): void {
    if (event.key !== "Enter") return;
    event.preventDefault();
    jump(event.shiftKey ? -1 : 1);
  }

  /** Set by typing in the box: emptied by hand, the bar stays, because the next word is on its way. */
  let findTyped = $state(false);
  const findOpen = $derived((find !== "" || findTyped) && page !== null);

  /** What the find bar marks, from anything but the bar's own box. */
  function setFind(words: string): void {
    find = words;
    findTyped = false;
  }

  /** The bar closes, and focus left in it, or on nothing, goes to the page's title rather than the body (F-436). */
  function closeFind(): void {
    const at = document.activeElement;
    const free = at === null || at === document.body || at.closest(".tf-findbar") !== null;
    setFind("");
    if (free) article?.querySelector<HTMLElement>(".tf-rd-title")?.focus({ preventScroll: true });
  }

  // On the layer stack, so `Esc` closes the bar before it leaves the page, wherever focus is.
  $effect(() => {
    if (!findOpen || !shown()) return;
    return untrack(() => layers.add({ kind: "panel", close: closeFind }));
  });


  // ---- what the reader's width makes of its header (`readerFit`) ----
  let sheet = $state<HTMLElement | null>(null);
  let headActs = $state<HTMLElement | null>(null);
  /** The header's actions would not fit one row beside the outline, which folds above the page. */
  let outlineFolds = $state(false);
  /** The row does not fit even so: the view's *Sync now* is in *More*. */
  let syncInMore = $state(false);
  /** What *Sync now* takes of the row, measured while it is in it. */
  let syncRoom = 0;

  function fitHeader(): void {
    if (sheet === null || headActs === null) return;
    const inner = getComputedStyle(sheet);
    const room = sheet.clientWidth - (parseFloat(inner.paddingLeft) || 0) - (parseFloat(inner.paddingRight) || 0);
    // Not laid out: a page keep-alive holds hidden says nothing about its width.
    if (room <= 0) return;
    const gap = parseFloat(getComputedStyle(headActs).columnGap) || 0;
    const items = [...headActs.children] as HTMLElement[];
    const drawn = items.find((item) => item.classList.contains("tf-syncnow"));
    if (drawn) syncRoom = drawn.offsetWidth + gap;
    else if (!syncInMore) syncRoom = 0;
    const row = items.reduce((sum, item) => sum + item.offsetWidth, 0) + gap * Math.max(0, items.length - 1) + (drawn ? 0 : syncRoom);
    const fit = readerFit(room, row, syncRoom);
    outlineFolds = fit.outlineFolds;
    syncInMore = fit.syncInMore;
  }

  // The width and what the row holds both decide. The observer answers before the page paints.
  $effect(() => {
    if (sheet === null || headActs === null) return;
    void page;
    void detail;
    untrack(fitHeader);
    if (typeof ResizeObserver === "undefined") return;
    const seen = new ResizeObserver(() => fitHeader());
    seen.observe(sheet);
    seen.observe(headActs);
    return () => seen.disconnect();
  });

  /** The view's *Sync now* for *More*: never beside a page whose own button is one (#2287). */
  const syncForMore = $derived(syncInMore && (detail === null || refresh.offer(detail) !== "sync") ? sync : undefined);

  // ---- the outline follows the scrolling ----
  let active = $state<number | null>(null);
  function follow(): void {
    if (!main || !article) return;
    // Where the reader is on this page, for *Continue reading*. Only with its own page drawn: another's scroll is not this one's.
    if (place.kind === "page" && page?.entity_id === place.id) noteReading(place.id, main.scrollTop);
    // A section is the one you are in once its heading is in the column's top third, with its body in view under it.
    const top = main.getBoundingClientRect().top + main.clientHeight / 3;
    let now: number | null = null;
    let last: number | null = null;
    for (const h of article.querySelectorAll<HTMLElement>("h2[id^='s-']")) {
      if (h.getBoundingClientRect().top < top) now = Number(h.id.slice(2));
      last = Number(h.id.slice(2));
    }
    // At the end of the page the last section is the one you are in, however short it is.
    if (last !== null && main.scrollTop > 0 && main.scrollTop + main.clientHeight >= main.scrollHeight - 4) now = last;
    active = now;
  }

  /** The outline's entry the reader clicked: it stays marked until they scroll by hand. */
  let pickedSection = $state<number | null>(null);
  $effect(() => {
    if (place.kind === "page" && place.section !== null) pickedSection = place.section;
  });
  const marked = $derived(pickedSection ?? active);

  /** Scroll to section `number`, where the page is drawn. */
  function toSection(number: number): void {
    void tick().then(() => article?.querySelector(`#s-${number}`)?.scrollIntoView?.());
  }

  // An outline entry changes the address on the page already open: nothing is read again, so the scroll is here.
  // By the section's number, so another route's draw of the same address does not undo where the reader scrolled to.
  const sectionHere = $derived(place.kind === "page" ? place.section : null);
  $effect(() => {
    if (sectionHere !== null) toSection(sectionHere);
  });

  /** An entry pressed: the one the address already names changes no address, so it scrolls from here too. */
  function pickSection(number: number): void {
    pickedSection = number;
    toSection(number);
  }

  const SCROLL_KEYS = new Set(["ArrowUp", "ArrowDown", "PageUp", "PageDown", "Home", "End", " "]);
  /** The reader moves the page by hand: the outline follows the scrolling again. */
  function releaseSection(event: KeyboardEvent): void {
    if (SCROLL_KEYS.has(event.key)) pickedSection = null;
  }

  // ---- actions ----
  let linking = $state(false);
  let newPage = $state<{ space: string | null; parent: string | null; title?: string; text?: string } | null>(null);
  /** Each page's composer holds its own words, so a draft never lands on the next page (F-429). */
  const commentKey = (entityId: string) => `comment:${entityId}`;
  const comment = $derived(page === null ? "" : drafts.get(commentKey(page.entity_id)));
  let commenting = $state(false);

  /** The page's writes its source's credential cannot make, asked once the page is drawn (#2053). */
  let refused = $state<RefusedWrite[]>([]);
  /** As a string, so a read of the same page asks nothing again. */
  const writeAsks = $derived(JSON.stringify(page === null ? [] : page.writes.map((op) => ({ entity: page!.entity_id, op }))));
  $effect(() => askInto(JSON.parse(writeAsks) as WriteAsk[], io.refusedWrites, (answer) => (refused = answer)));
  /** Why the open page's `op` cannot be made, or `null`. */
  const refusal = (op: string) => refusalOf(refused, page?.entity_id ?? null, op);
  /**
   * The tree's *New page* opens on the space the tree shows. The open page's
   * answer speaks for it only where that is the page's own space; elsewhere
   * the dialog asks about its own *Under*.
   */
  const treeNewPageRefusal = $derived(
    shownSpace === undefined || shownSpace === pageSpace ? refusal("create_page") : null,
  );
  let composer = $state<HTMLTextAreaElement | null>(null);

  async function open(url: string): Promise<void> {
    try {
      await io.openExternal(url);
    } catch (cause) {
      push({ text: `Could not open it: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }

  async function copy(code: string): Promise<void> {
    try {
      await io.copy(code);
      push({ text: "Copied" });
    } catch (cause) {
      push({ text: `Could not copy: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }

  async function copyLink(p: DocPage): Promise<void> {
    try {
      await io.copy(copyLinkText(p));
      push({ text: p.web_url ? `Copied the knobas link and the ${p.source_name} link` : "Copied the knobas link" });
    } catch (cause) {
      push({ text: `Could not copy: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }

  async function exportMarkdown(p: DocPage): Promise<void> {
    try {
      const done = await io.docsExportMarkdown(p.entity_id);
      push({ text: `Saved ${done.file} to ~/Downloads` });
    } catch (cause) {
      push({ text: `Could not export it: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }

  /** The job's ticket, as `create_link` names it; `null` without the job's ports. */
  async function jobTicket(key: string): Promise<string | null> {
    if (!linkPorts) return null;
    const sources = await linkPorts.listSources().catch(() => []);
    const found = await resolveTicket(key, sources, linkPorts.getEntity);
    return found?.row.entity_id ?? null;
  }

  /** A press of *Link to <job>* still on its way, so a second press links nothing twice. */
  let linkingJob = $state(false);

  async function linkToJob(p: DocPage, ref: string, key: string): Promise<void> {
    if (linkingJob) return;
    linkingJob = true;
    try {
      const ticket = await jobTicket(ref);
      if (ticket === null) {
        push({ text: `${key} is not in the mirror`, tone: "err" });
        return;
      }
      const row = drawn(LINK_RELATION, p.entity_id, ticket);
      const made = await linkPorts!.createLink(row.fromId, row.toId, row.relation);
      toastLinked(`Linked to ${key}`, p.entity_id, made.id);
      await rereadPage(p.entity_id);
    } catch (cause) {
      push({ text: `Could not link it: ${ipcErrorMessage(cause)}`, tone: "err" });
    } finally {
      linkingJob = false;
    }
  }

  /** A link just made says so with a way back, from where it was made (F-433). */
  function toastLinked(text: string, pageId: string, linkId: string): void {
    undoable(text, () =>
      void linkPorts!
        .unlink(linkId)
        .then(() => rereadPage(pageId))
        .catch((cause: unknown) => push({ text: `Could not unlink: ${ipcErrorMessage(cause)}`, tone: "err" })),
    );
  }

  /** A press of *Unlink* still on its way, so a second press withdraws and toasts nothing twice. */
  let unlinking = false;

  /**
   * The foot's *Unlink*: a link drawn by hand goes at once, with *Undo*, as the ticket's drawer has it. The page read
   * again says what the toast says: a page whose text still names the ticket keeps it listed, and says why (F-447).
   */
  async function unlinkWork(pageId: string, link: LinkEntry["link"]): Promise<void> {
    if (!linkPorts || unlinking) return;
    unlinking = true;
    const ports = linkPorts;
    try {
      await ports.unlink(link.id);
      await rereadPage(pageId);
      const kept = page?.entity_id === pageId ? page.work.find((work) => !work.linked && (work.entity_id === link.to_id || work.entity_id === link.from_id)) : undefined;
      undoable(kept === undefined ? "Unlinked in knobas only." : `Link removed. The page still names ${kept.key}, so it stays listed.`, () =>
        void ports
          .createLink(link.from_id, link.to_id, link.relation, link.note ?? undefined)
          .then(() => rereadPage(pageId))
          .catch((cause: unknown) => push({ text: `Could not link it again: ${ipcErrorMessage(cause)}`, tone: "err" })),
      );
    } catch (cause) {
      push({ text: `Could not unlink: ${ipcErrorMessage(cause)}`, tone: "err" });
    } finally {
      unlinking = false;
    }
  }

  // ---- what this page queued (#1424), its echoes (#2102) ----

  /** The surface Docs' echoes are tagged with, for `watchEchoes`. */
  const DOCS = "docs";

  /** A tree read drops each queued page it holds: a page of that title under that parent. */
  function settlePages(read: DocsTree): void {
    const under = new Map<string, Set<string>>();
    for (const node of read.spaces.flatMap((s) => s.pages)) {
      if (node.parent === null) continue;
      under.set(node.parent, (under.get(node.parent) ?? new Set()).add(node.title.trim()));
    }
    for (const [parent, titles] of under) settleEchoes(parent, (echo) => echo.kind === "page" && titles.has(echo.text.trim()));
  }

  // A page read drops a section edit once the version moved, and a comment it holds:
  // by the id the source gave the comment where its write was stamped with one, so an
  // edit in the source before the read does not keep it queued (#2074); by its words
  // where the write has no id yet, or its source names none.
  $effect(() => {
    const p = page;
    if (p === null) return;
    const version = String(p.version);
    const said = p.comments.map((c) => c.blocks.map(blockWords).join(" "));
    const held = new Set(p.comments.flatMap((c) => (c.id === null ? [] : [c.id])));
    const commentOfWrite = new Map(p.sent_comments.map((s) => [s.write_id, s.comment_id]));
    const holds = (echo: Echo) => {
      const id = echo.write === null ? undefined : commentOfWrite.get(echo.write);
      return id === undefined ? said.some((words) => sameWords(words, echo.text)) : held.has(id);
    };
    untrack(() =>
      settleEchoes(
        p.entity_id,
        (echo) => (echo.kind.startsWith("section ") && echo.base !== version) || (echo.kind === "comment" && holds(echo)),
      ),
    );
  });

  /** The comments queued on the open page that its read does not hold yet. */
  const queuedComments = $derived(page === null ? [] : echoesOn(page.entity_id, "comment"));

  // *Edit and resend* on a comment: the composer with its words.
  $effect(() => {
    if (page === null) return;
    const words = takeAgain(page.entity_id, "comment");
    if (words === null) return;
    drafts.put(commentKey(page.entity_id), words);
    void tick().then(() => composer?.focus());
  });

  // *Edit and resend* on a new page: *New page* with what it asked for.
  $effect(() => {
    if (page === null || tree === null) return;
    const asked = takeAgain<NewPage>(page.entity_id, "page");
    if (asked === null) return;
    const space = spaceOfPage(tree, asked.parent);
    newPage = { space: space ? spaceId(space) : null, parent: asked.parent, title: asked.title, text: asked.text };
  });

  /** A page's source: where its writes go, named in their toasts when its sign-in fails. */
  const sourceOf = (p: Pick<DocPage, "source_id" | "source_name">): WriteSource => ({ id: p.source_id, display_name: p.source_name });

  async function saveSection(p: DocPage, section: number, text: string): Promise<boolean> {
    try {
      const write = await io.docsEditSection(p.entity_id, section, p.version, text);
      const heading = sectionsOf(p.blocks)[section]?.heading;
      const name = heading ? inlineWords(heading.text) : "";
      const id = p.entity_id;
      const kind = sectionKind(section);
      toastQueued(`edit section “${name}” of ${p.title}`, sourceOf(p), { write });
      echoWrite(id, kind, text, {
        write,
        base: String(p.version),
        surface: DOCS,
        source: p.source_id,
        following: {
          name: `Edit of “${name}” on ${p.title}`,
          resend: () => {
            askAgain(id, kind, text);
            location.hash = pageHash(id, section);
          },
        },
        follower,
      });
      return true;
    } catch (cause) {
      push({ text: `Could not queue the edit: ${ipcErrorMessage(cause)}`, tone: "err" });
      return false;
    }
  }

  async function sendComment(p: DocPage): Promise<void> {
    const body = drafts.get(commentKey(p.entity_id));
    if (body.trim() === "" || commenting || refusal("comment") !== null) return;
    commenting = true;
    try {
      const id = p.entity_id;
      const write = await io.submitWrite({ Comment: { entity: id, body } });
      toastQueued(`comment on ${p.title}`, sourceOf(p), { write });
      echoWrite(id, "comment", body, {
        write,
        many: true,
        surface: DOCS,
        source: p.source_id,
        following: {
          name: `Comment on ${p.title}`,
          resend: () => {
            askAgain(id, "comment", body);
            location.hash = pageHash(id);
          },
        },
        follower,
      });
      drafts.forget(commentKey(id));
    } catch (cause) {
      push({ text: `Could not queue the comment: ${ipcErrorMessage(cause)}`, tone: "err" });
    } finally {
      commenting = false;
    }
  }

  /** ⌘Enter in the composer: read when pressed, because the attachment outlives one read of the page. */
  function sendOpenComment(): void {
    if (page !== null) void sendComment(page);
  }

  async function createPage(next: NewPage, linkToJob: boolean): Promise<boolean> {
    // The new page goes under the open one, to its source.
    const under = page;
    try {
      const ticket = linkToJob && jobRef !== null ? await jobTicket(jobRef) : null;
      if (linkToJob && job !== null && ticket === null) {
        // Asked for a link we cannot make: refuse, the dialog stays open (as *Link to <job>* does).
        push({ text: `${job} is not in the mirror`, tone: "err" });
        return false;
      }
      const write = await io.docsCreatePage(next, ticket);
      const title = next.title.trim();
      toastQueued(`create page ${title}`, under ? sourceOf(under) : null, { write });
      echoWrite(next.parent, "page", title, {
        write,
        many: true,
        surface: DOCS,
        ...(under ? { source: under.source_id } : {}),
        following: {
          name: `Page ${title}`,
          resend: () => {
            askAgain(next.parent, "page", next);
            location.hash = pageHash(next.parent);
          },
        },
        follower,
      });
      return true;
    } catch (cause) {
      push({ text: `Could not queue the page: ${ipcErrorMessage(cause)}`, tone: "err" });
      return false;
    }
  }

</script>

<div class="tf-rd" class:tf-rd-drawer={drawer}>
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <nav
    id="tf-docs-tree"
    class="tf-rd-tree"
    aria-label="Docs"
    tabindex="-1"
    bind:this={treeNav}
    onkeydown={ondrawerkey}
    onclick={ondrawerclick}
  >
    <div class="tf-dl-head">
      {#if job !== null && tree !== null && !nothingHeld}
        <TfScope
          scopes={[{ id: "job", label: "This job", count: jobIds?.size ?? null, title: `The pages ${job} reaches` }, { id: "all", label: "All" }]}
          scope={jobOn ? "job" : "all"}
          onpick={(id) => (filter.scope = id)}
        />
      {/if}
      {#if !jobOn && tree !== null && tree.spaces.length > 0}
        <select aria-label="Space" value={pickerValue} title={shownSpace?.name ?? undefined} onchange={pick}>
          <option value="*">All spaces ({total})</option>
          {#each tree.spaces as space (spaceId(space))}
            <option value={spaceId(space)}>{spaceLabel(space)}</option>
          {/each}
        </select>
      {/if}
      {#if searching}
        <p class="tf-dl-q">Search <b>{query.trim()}</b> <button type="button" class="tf-linkish" onclick={clearSearch}>Clear</button></p>
      {/if}
    </div>
    <div class="tf-dl-body">
      {#if treeError !== null}
        <p class="tf-dl-note">Could not read the page tree: {treeError}</p>
      {:else if searching}
        {#if searchError !== null}
          <p class="tf-dl-note">Could not search: {searchError} <button type="button" class="tf-linkish" onclick={retrySearch}>Try again</button></p>
        {:else if hits !== null}
          <TfDocHits
            found={hits}
            {current}
            scope={searchScope}
            kept={searchKept}
            {me}
            answered={answerHere}
            onall={widenSearch}
            onpick={openHit}
          />
        {/if}
      {:else if jobOn && tree !== null}
        {#each jobGroups as group (group.reason)}
          <div class="tf-dl-for">
            <h4><TfKeyed text={group.reason} /></h4>
            {#each group.pages as item (item.entity_id)}
              <a href={pageHash(item.entity_id)} aria-current={item.entity_id === current ? "true" : undefined}
                >{item.title}<span class="tf-why">{item.where}</span></a
              >
            {/each}
          </div>
        {:else}
          {#if jobIds !== null}
            <p class="tf-dl-note">
              Nothing in Docs is on <TfKey value={job ?? ""} /> yet. Use Link to <TfKey value={job ?? ""} /> on a page.
              <button type="button" class="tf-linkish" onclick={() => (filter.scope = "all")}>Search all pages</button>
            </p>
          {/if}
        {/each}
      {:else}
        <!-- The home lists *For you* itself. -->
        {#if forYou.length > 0 && !home}
          <div class="tf-dl-for">
            <h4>For you</h4>
            {#each forYou as item (item.entity_id)}
              <!-- On the page it names, the row marks that page and still jumps to the mention. -->
              <a
                href={forYouHash(item.entity_id)}
                aria-current={item.entity_id === current ? "true" : undefined}
                onclick={() => {
                  if (mentionHere && item.entity_id === current) toMention();
                }}>{item.title}<span class="tf-why">{forYouWhy(item)}</span></a
              >
            {/each}
          </div>
        {/if}
        {#if tree === null}
          {#if treeError === null}
            <div class="tf-skel tf-skel-lines" aria-hidden="true">
              {#each [0, 1, 2, 3, 4, 5] as line (line)}<span class="tf-skel-bar"></span>{/each}
            </div>
          {/if}
        {:else if tree.spaces.length === 0}
          <p class="tf-dl-note">{hasSource ? "No pages have synced yet." : "No wiki source is set up yet."}</p>
        {:else if shownSpace === undefined}
          {#each tree.spaces as space (spaceId(space))}
            <details open={current !== null && space.pages.some((p) => p.entity_id === current)}>
              <summary class="tf-spc"><span class="tf-spc-n" title={space.name ?? space.key}>{space.name ?? space.key}</span> <span class="tf-count">{space.pages.length}</span></summary>
              <div class="tf-fold-in"><TfDocTree {space} {current} /></div>
            </details>
          {/each}
        {:else if shownSpace}
          <TfDocTree space={shownSpace} {current} />
        {/if}
      {/if}
    </div>
    {#if page?.writes.includes("create_page")}
      {@const p = page}
      <div class="tf-dl-foot">
        <button
          type="button"
          class="tf-btn tf-sm"
          {...refusedAttrs(treeNewPageRefusal)}
          onclick={() => {
            if (treeNewPageRefusal !== null) return;
            const space = shownSpace ?? pageSpace;
            newPage = { space: space ? spaceId(space) : null, parent: p.entity_id };
          }}
          >New page…</button>
      </div>
    {/if}
  </nav>
  {#if drawer}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="tf-rd-backdrop" onclick={closeDrawer}></div>
  {/if}

  <TfScroller class="tf-rd-main" bind:ref={main} onscroll={follow} onwheel={() => (pickedSection = null)} onpointerdown={(event) => { if (event.target === event.currentTarget) pickedSection = null; }} onkeydown={releaseSection}>
    <!-- The one fold bar below 900 px. The tree it opens is a drawer, because the page does not reflow under it. -->
    {#if drawerBand.current}
      <TfRailBar
        label={place.kind === "search" ? "Search" : spaceHere !== undefined ? "Space" : "Page"}
        value={place.kind === "search" ? place.words : (page?.title ?? spaceHere?.name ?? spaceHere?.key ?? "Docs home")}
        open={drawer} controls="tf-docs-tree" onpress={(event) => openTree(event.currentTarget as Element)} />
    {/if}
    {#if findOpen}
      <div class="tf-findbar" role="search">
        <span>Matches for</span>
        <input
          value={find}
          oninput={(e) => {
            findTyped = true;
            find = e.currentTarget.value;
          }}
          onkeydown={onfindkey}
          aria-label="Find in page"
          {@attach filterKeys({ close: closeFind })}
        />
        <!-- A held width: the arrows beside it do not move as the words change. -->
        <span class="tf-findn">{find.trim() === "" ? "" : marks === 0 ? "none on this page" : `${Math.max(at, 0) + 1} of ${marks}`}</span>
        <button type="button" disabled={marks === 0} onclick={() => jump(-1)} aria-label="Previous">↑</button>
        <button type="button" disabled={marks === 0} onclick={() => jump(1)} aria-label="Next">↓</button>
        <button type="button" class="tf-findx" onclick={closeFind} aria-label="Stop highlighting">✕</button>
      </div>
    {/if}
    {#if home}
      <!-- A search's address, or a ticket card's row that opened the home narrowed to that ticket: the same bar its other rows land under. -->
      {#if place.kind === "search"}
        <div class="tf-dh-crumb"><TfCrumbs parts={[{ name: "Docs", href: "#/docs" }, { name: "Search" }]} {back} /></div>
      {:else if back !== null}
        <div class="tf-dh-crumb"><TfCrumbs parts={[{ name: "Docs" }]} {back} /></div>
      {/if}
      <TfDocsHome
        {tree}
        failed={treeError}
        {job}
        groups={jobGroups}
        jobFirst={jobOn}
        {sync}
        notice={answerHere || hitsHere ? searchAnswer : undefined}
        onretry={() => void readTree(false)}
        onresume={(id) => (resuming = { id, top: readingAt(id) })}
      />
    {:else if spaceHere !== undefined}
      <div class="tf-dh-crumb"><TfCrumbs parts={[{ name: "Docs", href: "#/docs" }, { name: spaceHere.name ?? spaceHere.key }]} {back} /></div>
      <TfDocsSpace space={spaceHere} {sync} />
    {:else if tree === null}
      {#if treeError !== null}
        <!-- No tree to place the address in: the view's own header, so *Sync now* is still where every view puts it (#2286). -->
        <TfBareHeader title="Docs" {sync} />
        <TfNothingHere message="Could not read Docs: {treeError}" onretry={() => void readTree(false)} />
      {:else}
        <div class="tf-skel tf-skel-lines tf-dh-wait" aria-label="Reading Docs" role="status">
          {#each [0, 1, 2, 3, 4, 5] as line (line)}<span class="tf-skel-bar"></span>{/each}
        </div>
      {/if}
    {:else if place.kind === "space"}
      <TfCrumbs parts={[{ name: "Docs", href: "#/docs" }, { name: "Space" }]} {back} />
      <TfNothingHere message="knobas does not hold this space. It may not have synced yet." href="#/docs" action="Docs home" />
    {:else if pageError !== null}
      <TfCrumbs parts={[{ name: "Docs", href: "#/docs" }, { name: "Page" }]} {back} />
      {#if pageMissing}
        <TfNothingHere message="knobas does not hold this page. It may not have synced yet." href="#/docs" action="Docs home" />
      {:else}
        <TfNothingHere message="This page could not be opened: {pageError}" onretry={target === null ? undefined : () => void readPage(target, false)} />
      {/if}
    {:else if page === null}
      <div class="tf-skel tf-skel-lines tf-dh-wait" aria-label="Reading the page" role="status">
        {#each [0, 1, 2, 3, 4, 5] as line (line)}<span class="tf-skel-bar"></span>{/each}
      </div>
    {:else}
      {@const p = page}
      {@const created = createdBy(p)}
    <nav class="tf-crumbs tf-rd-crumbrow" aria-label="Breadcrumb">
      <div class="tf-rd-crumb">
        <a href="#/docs">Docs</a>
        {#if p.space}
          › <a href={crumbSpace ?? undefined}>{p.space.name ?? p.space.key}</a>
        {/if}
        {#if parent}
          › {#if parent.entity_id}<a href={pageHash(parent.entity_id)}>{parent.title}</a>{:else}{parent.title}{/if}
        {/if}
        › <strong><TfMarked text={p.title} {stems} /></strong>
      </div>
      <!-- Where `Esc` goes, as every breadcrumb names it (spec #2227, story 35). -->
      {#if back !== null}<TfBackHint {back} />{/if}
    </nav>
      <div class="tf-rd-sheet" class:tf-rd-solo={outlineFolds} bind:this={sheet}>
        <article class="tf-rd-page" bind:this={article}>
          <!-- The canonical header (#2238, #2286): the title, and on its line the actions in the
               Controls order, "Open in" a ghost, the page's Refresh and the view's Sync now last. -->
          <div class="tf-fhead tf-rd-head">
            <h1 class="tf-rd-title" tabindex="-1">
              <TfMarked text={p.title} {stems} />
            </h1>
            <div class="tf-acts" bind:this={headActs}>
              {#if job !== null && linkPorts}
                {@const key = job}
                {@const ref = jobAt ?? job}
                {#if p.work.some((work) => work.key === key && work.linked)}
                  <!-- Already on the job: nothing to press, and the page says so where the button was. -->
                  <span class="tf-rd-on" title="This page is linked to {key}">On <TfKey value={key} /></span>
                {:else}
                  <button type="button" class="tf-btn" disabled={linkingJob} onclick={() => linkToJob(p, ref, key)}>Link to <TfKey value={key} /></button>
                {/if}
              {/if}
              {#if p.writes.includes("create_page")}
                <button
                  type="button"
                  class="tf-btn"
                  {...refusedAttrs(refusal("create_page"))}
                  onclick={() => {
                    if (refusal("create_page") === null) newPage = { space: pageSpace ? spaceId(pageSpace) : null, parent: p.entity_id };
                  }}
                  >New page here…</button>
              {/if}
              <TfDocMore
                onlink={linkPorts ? () => (linking = true) : undefined}
                oncopy={() => copyLink(p)}
                onexport={p.readable ? () => exportMarkdown(p) : undefined}
                sync={syncForMore}
              />
              {#if p.web_url}
                {@const url = p.web_url}
                <button type="button" class="tf-btn tf-ghost" onclick={() => open(url)}>Open in {p.source_name}</button>
              {/if}
              {#if !syncInMore}
                <TfHeaderRefresh {detail} {refresh} {reread} {sync} noun="page" />
              {:else if detail !== null}
                <TfDetailRefresh {detail} {refresh} {reread} page noun="page" />
              {/if}
            </div>
          </div>
          <div class="tf-rd-meta">
            {#if p.editor !== null || p.updated_at !== null}
              <span>{person(p.editor, p.editor_name, p.me)} edited {clockLabel(p.updated_at)}</span>
            {/if}
            {#if created}
              <span>Created by {created.who}{#if created.at !== null}, {clockLabel(created.at)}{/if}</span>
            {/if}
            {#if p.version !== null}<span>version {p.version}</span>{/if}
            {#if p.readable}<span>{readingMinutes(p)} min read</span>{/if}
            {#if p.labels.length > 0}
              <!-- The labels wrap as one group, so none is left alone on a line. -->
              <span class="tf-rd-labels">
                {#each p.labels as label (label)}
                  <button type="button" class="tf-lab" title="Search for {label}" onclick={(event) => searchLabel(label, event.currentTarget)}
                    >{label}</button>
                {/each}
              </span>
            {/if}
          </div>
          {#if detail !== null}<TfDetailBanners {detail} {refresh} {reread} />{/if}
          {#if outlineFolds}
            {#key p.entity_id}
              <TfDocOutline page={p} {stems} active={marked} folded onpick={pickSection} />
            {/key}
          {/if}
          <div class="tf-doc">
            {#if p.readable}
              <TfDocBody
                blocks={p.blocks}
                entityId={p.entity_id}
                sourceName={p.source_name}
                webUrl={p.web_url}
                sections={p.sections}
                editable={p.writes.includes("update_page") && p.version !== null}
                refusal={refusal("update_page")}
                {stems}
                onopen={open}
                oncopy={copy}
                onsave={(section, text) => saveSection(p, section, text)}
              />
            {:else}
              <p class="tf-dl-note">knobas cannot read this page's body. Open it in {p.source_name}.</p>
            {/if}
            {#key p.entity_id}
              <TfDocFoot page={p} spaces={tree.spaces} links={detail === null ? [] : linkedOn(detail)} onunlink={linkPorts ? (link) => unlinkWork(p.entity_id, link) : undefined} />
            {/key}
            <div class="tf-cmts">
              <h2>Comments {#if p.comments.length + queuedComments.length > 0}<span class="tf-count">{p.comments.length + queuedComments.length}</span>{/if}</h2>
              {#each p.comments as c, i (i)}
                {@const mine = c.author !== null && p.me.includes(c.author)}
                <div class="tf-msg" class:tf-mention={c.blocks.some(mentionsMe)}>
                  <span class="tf-av" class:tf-me={mine}>{initials(c.author)}</span>
                  <div class="tf-cmt-body">
                    <div class="tf-who">
                      <b>{mine ? "You" : firstName(person(c.author, c.author_name, p.me))}</b>
                      {c.at === null ? "" : clockLabel(c.at)}
                    </div>
                    <TfDocBlocks
                      blocks={c.blocks}
                      entityId={p.entity_id}
                      sourceName={p.source_name}
                      webUrl={p.web_url}
                      onopen={open}
                      oncopy={copy}
                      {stems}
                    />
                  </div>
                </div>
              {/each}
              {#each queuedComments as echo, n (n)}
                <div class="tf-msg tf-queued">
                  <span class="tf-av tf-me">{initials(p.me[0] ?? null)}</span>
                  <div class="tf-cmt-body">
                    <div class="tf-who"><b>You</b> <TfStateChip text={echo.state} tone="info" /></div>
                    <p class="tf-cmt-queued">{echo.text}</p>
                  </div>
                </div>
              {/each}
              {#if p.comments.length === 0 && queuedComments.length === 0}
                <p class="tf-dl-note">No comments yet.</p>
              {/if}
              {#if p.writes.includes("comment")}
                <div class="tf-compose" tabindex="-1">
                  <textarea
                    bind:this={composer}
                    value={comment}
                    oninput={(event) => drafts.put(commentKey(p.entity_id), event.currentTarget.value)}
                    placeholder="Comment on this page"
                    aria-label="Comment on this page"
                    {@attach sendKey(sendOpenComment)}
                  ></textarea>
                  <div class="tf-acts">
                    <button
                      type="button"
                      class="tf-btn tf-sm tf-primary"
                      disabled={commenting || comment.trim() === ""}
                      {...refusedAttrs(refusal("comment"))}
                      onclick={() => sendComment(p)}>Comment</button>
                  </div>
                </div>
              {/if}
            </div>
          </div>
        </article>
        {#if !outlineFolds}
          <TfDocOutline page={p} {stems} active={marked} folded={false} onpick={pickSection} />
        {/if}
      </div>
    {/if}
  </TfScroller>
</div>

{#snippet searchAnswer()}
  {#if hitsHere && !answerHere}
    {@const total = hits?.total ?? 0}
    <p class="tf-dh-found" role="status">
      {counted(total, "page", "pages")} {total === 1 ? "matches" : "match"} <b>{query.trim()}</b>{#if drawerBand.current}. <button type="button" class="tf-linkish" onclick={(event) => openTree(event.currentTarget)}>Show them</button>{:else}, listed on the left.{/if}
    </p>
  {:else}
  <div class="tf-empty tf-dh-answer">
    {#if searchError !== null}
      <p>Could not search: {searchError}</p>
      <button type="button" class="tf-btn tf-sm" onclick={retrySearch}>Try again</button>
    {:else}
      <p>No page matches <b>{query.trim()}</b>. Every word has to appear in the page or its comments.</p>
    {/if}
    {#if searchKept !== null}
      <button type="button" class="tf-btn tf-sm" onclick={widenSearch}>{searchKept === "job" ? "Search all pages" : "Search all spaces"}</button>
    {:else}
      <button type="button" class="tf-btn tf-sm" onclick={clearSearch}>Clear search</button>
    {/if}
  </div>
  {/if}
{/snippet}

{#if linking && page !== null && linkPorts}
  {@const p = page}
  <TfLinkTo
    jobKey={p.title}
    entityId={p.entity_id}
    ports={linkPorts}
    {layers}
    linked={[...p.work.filter((work) => work.linked).map((work) => work.entity_id), ...(detail === null ? [] : linkedOn(detail).map((entry) => entry.other.entity_id))]}
    onclose={() => (linking = false)}
    onlinked={(target, made) => {
      linking = false;
      toastLinked(`Linked to ${target.title} in knobas`, p.entity_id, made.id);
      void rereadPage(p.entity_id);
    }}
  />
{/if}

{#if newPage !== null && tree !== null}
  <TfNewPage
    {tree}
    space={newPage.space}
    parent={newPage.parent}
    title={newPage.title ?? ""}
    text={newPage.text ?? ""}
    {job}
    sourceName={page?.source_name ?? "the source"}
    {layers}
    onclose={() => (newPage = null)}
    oncreate={createPage}
    refusedWrites={io.refusedWrites}
  />
{/if}

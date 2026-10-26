<!--
  Docs (#859 to #862, requirements §10): one screen for `#/docs`,
  `#/docs/space/[<source>:]<key>` and `#/page/<id>[/<section>]`, as prototype-6's
  `renderDocs` → `renderPage`.

  **Left**, the space picker (*All spaces (N)*, then every space with its page
  count; it follows the open page's space until you pick one or start
  searching), the search field that names its scope (`/` focuses it), its
  one line of syntax, and under it *For you* and the page tree, or, with text
  in the field, the hits (R-10.1 to R-10.4, R-10.8). *New page* is at the
  bottom. Below 900px (`READER_DRAWER`, through `MediaQuery`) it is a drawer
  over the page (#2084, R-10.9): *Pages* at the start of the breadcrumb row,
  `/` and a label chip open it; `Esc` (it is a `drawer` layer), the backdrop
  and a page picked in it close it, and focus goes back to what opened it.

  **Right**, the page in the reading face, nothing in it capped in width
  (#2081, R-10.9): the breadcrumb, the title, who edited it when, who created it (#2080), the
  version, the reading time and the labels (each searches for itself), the
  actions row with *More ▾* (#2082) and, at its end, *Refresh* or *Sync now*
  (#2136: `TfDetailRefresh` over `get_entity` of the page) with the withdrawn,
  turned-off and failed-refresh banners under it, the blocks by section (*Edit section*),
  the page foot (*Work it belongs to*, *Linked from*, `TfDocFoot`) and the
  comments with the composer (R-10.9 to R-10.15). A page opened from a hit
  keeps the query: every match is marked and the find bar steps through them
  (R-10.5, R-10.12). The outline (`TfDocOutline`, R-10.15) is the page's own
  right column, sticking while the page scrolls; below 1240px
  (`READER_NARROW`, read through Svelte's `MediaQuery`) it folds between the actions
  row and the first section instead. `#/docs` opens the page read last, and every
  page opened is a visit (`visit_entity`), which is what "read last" reads.

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
  import { clockLabel } from "../../time/format";
  import { tick, untrack } from "svelte";
  import type { RoleFrameProps } from "../page-entry";
  import { MediaQuery } from "svelte/reactivity";

  import { ipcErrorMessage } from "../../ipc";
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
    type NewPage,
    type Reached,
  } from "../../ipc/entity";
  import { drawn } from "../../detail/relations";
  import type { PipelinePorts } from "../builds/pipeline-ports";
  import { writeFollower, type WriteFollower } from "../../shell/follow-write";
  import { openExternal as realOpenExternal } from "../../shell/open-external";
  import { askInto, refusalOf, refusedAttrs, type RefusedWrite, type RefusedWritesPort, type WriteAsk } from "../../shell/refused-writes";
  import { toastQueued, type WriteSource } from "../../shell/queued-toast";
  import { push } from "../../shell/toasts.svelte";
  import { sessionFilter } from "../filter/filter-state.svelte";
  import { layers as sharedLayers, type Layers } from "../layers.svelte";
  import { resolveTicket } from "../job/job";
  import TfLinkTo from "../job/TfLinkTo.svelte";
  import type { JobPorts } from "../job/ports";
  import { askAgain, echoWrite, echoesOn, sameWords, settleEchoes, takeAgain, watchEchoes, type Echo } from "../queued.svelte";
  import { createDetailRefresh, DETAIL_REFRESH_BRIDGE, type DetailRefreshPorts } from "../refresh.svelte";
  import { jobHash } from "../route";
  import TfDetailBanners from "../TfDetailBanners.svelte";
  import TfDetailRefresh from "../TfDetailRefresh.svelte";
  import { initials } from "../tickets/tickets";
  import TfDocBlocks from "./TfDocBlocks.svelte";
  import TfDocBody from "./TfDocBody.svelte";
  import TfDocFoot from "./TfDocFoot.svelte";
  import TfDocHits from "./TfDocHits.svelte";
  import TfDocMore from "./TfDocMore.svelte";
  import TfDocOutline from "./TfDocOutline.svelte";
  import TfDocTree from "./TfDocTree.svelte";
  import TfMarked from "./TfMarked.svelte";
  import TfNewPage from "./TfNewPage.svelte";
  import {
    copyLinkText,
    createdBy,
    LINK_RELATION,
    blockWords,
    firstPage,
    firstName,
    forYouIn,
    forYouWhy,
    inlineWords,
    jobPageGroups,
    pageHash,
    pageToOpen,
    READER_DRAWER,
    READER_NARROW,
    person,
    placeOf,
    reachedPages,
    readingMinutes,
    scopeName,
    sectionKind,
    sectionsOf,
    spaceHash,
    spaceId,
    spaceLabel,
    spaceOfPage,
    stemsOf,
  } from "./docs";

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
    /** The actions row's *Refresh* (#2136); the bridge's when omitted. */
    refresh?: DetailRefreshPorts;
    openExternal: (url: string) => Promise<void>;
    copy: (text: string) => Promise<void>;
    /** What the page's source refuses its credential (#2053); the IPC read when omitted. */
    refusedWrites?: RefusedWritesPort;
    /** Hears `sync:state`, whose run ends read Docs again (#2072), and `item:refreshed` (#2102); the bridge's when omitted. */
    listen?: PipelinePorts["listen"];
  }

  type LinkPorts = Pick<JobPorts, "listSources" | "getEntity" | "createLink" | "search" | "launcherHome">;

  let {
    ready,
    rest,
    job,
    jobAt = null,
    ports,
    linkPorts,
    layers = sharedLayers,
    follower = writeFollower(),
    sync,
  }: {
    ready: boolean;
    /** What follows the address's first segment. */
    rest: string[];
    /** The job the reader was last in, for the breadcrumb's chip and *Link to <job>*. */
    job: string | null;
    /** The address the job was opened at (#1525), which its chip links; its key when omitted. */
    jobAt?: string | null;
    ports?: DocsPorts;
    linkPorts?: LinkPorts;
    layers?: Layers;
    /** Follows each queued edit, comment and page, so one held or refused drops its echo (#1424). */
    follower?: WriteFollower;
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
  let tree = $state<DocsTree | null>(null);
  let treeError = $state<string | null>(null);
  let page = $state<DocPage | null>(null);
  let pageError = $state<string | null>(null);
  /** The picked space (`spaceId`), `*` for all, or `null` to follow the page. */
  let picked = $state<string | null>(null);

  /** The search field's text (#860). */
  let query = $state("");
  let found = $state<DocsHits | null>(null);
  let searchError = $state<string | null>(null);
  /** What the find bar marks: the needles a hit was opened with. */
  let find = $state("");
  let field = $state<HTMLInputElement | null>(null);

  // ---- *This job* (#863) ----
  const filter = sessionFilter("docs");
  const jobOn = $derived(job !== null && filter.scope === "job");
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
  const target = $derived(tree === null ? null : pageToOpen(place, tree));

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
      tree = read;
      treeError = null;
      settlePages(read);
    } catch (cause) {
      if (mine === treeAsked && (!quiet || tree === null)) treeError = ipcErrorMessage(cause);
    }
  }

  /**
   * Read page `id`. Opening it records the visit; a `quiet` read (a sync
   * run's, #2072) records none, and one that fails leaves the page as drawn.
   * A quiet read that dropped the opening read before `id` was drawn opens it
   * in that read's place: visit and error as an opening read.
   */
  function readPage(id: string, quiet: boolean): Promise<void> {
    const mine = ++pageAsked;
    return io.docsPage(id).then(
      (answer) => {
        if (mine !== pageAsked) return;
        const opens = !quiet || page?.entity_id !== id;
        page = answer;
        pageError = null;
        if (opens) void io.visitEntity(id).catch(() => {});
      },
      (cause: unknown) => {
        if (mine === pageAsked && (!quiet || page?.entity_id !== id)) pageError = ipcErrorMessage(cause);
      },
    );
  }

  /**
   * Read the open page `id` and its row again, after a link, a sync run or a
   * refresh: no visit, and nothing once the reader has opened another page
   * (#2072, #2136).
   */
  function rereadPage(id: string | null): Promise<void> {
    if (id === null || id !== target) return Promise.resolve();
    return Promise.all([readPage(id, true), readDetail(id, true)]).then(() => {});
  }

  // ---- *Refresh* (#2136, #2107) ----
  /** The open page's row (`get_entity`), `null` until read or when it cannot be. */
  let detail = $state<EntityDetail | null>(null);
  let detailAsked = 0;
  /** The actions row's *Refresh* or *Sync now*. */
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

  /**
   * A refresh's re-read: the tree, the open page and its row, each over what
   * Docs drew (#2072's quiet reads), so the picked space, the search, an open
   * section editor and the composer keep what they hold.
   */
  function reread(): Promise<void> {
    return Promise.all([readTree(true), rereadPage(target)]).then(() => {});
  }

  $effect(() => {
    if (!ready) return;
    untrack(() => void readTree(false));
  });

  $effect(() => {
    const id = target;
    if (!ready || id === null) return;
    pageError = null;
    detail = null;
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
    return pageSpace ?? tree.spaces[0];
  });
  const pickerValue = $derived(picked === "*" ? "*" : shownSpace ? spaceId(shownSpace) : "*");
  const total = $derived(tree?.spaces.reduce((n, s) => n + s.pages.length, 0) ?? 0);
  const parent = $derived(page?.ancestors.at(-1));
  const current = $derived(page?.entity_id ?? target);
  const searching = $derived(query.trim() !== "");
  const forYou = $derived(forYouIn(tree?.for_you ?? [], shownSpace));

  // Every keystroke searches; an answer to an older keystroke is dropped.
  let asked = 0;
  $effect(() => {
    const text = query;
    const scope = !jobOn && shownSpace ? { source_id: shownSpace.source_id, key: shownSpace.key } : null;
    if (!ready || text.trim() === "") {
      found = null;
      searchError = null;
      return;
    }
    const mine = ++asked;
    void io.docsSearch(text, scope).then(
      (answer) => {
        if (mine !== asked) return;
        found = answer;
        searchError = null;
      },
      (cause: unknown) => {
        if (mine === asked) searchError = ipcErrorMessage(cause);
      },
    );
  });

  /** The hits as read, kept to the job's pages while *This job* is on. */
  const hits = $derived.by((): DocsHits | null => {
    if (found === null || !jobOn) return found;
    const kept = found.hits.filter((h) => jobIds?.has(h.entity_id) ?? false);
    return { ...found, total: kept.length, hits: kept };
  });
  const searchScope = $derived(jobOn ? `${job}'s pages` : scopeName(shownSpace));

  function pick(event: Event): void {
    const value = (event.currentTarget as HTMLSelectElement).value;
    picked = value;
    if (value === "*" || tree === null || searching) return;
    const space = tree.spaces.find((s) => spaceId(s) === value);
    if (space && space !== pageSpace) {
      const first = firstPage(space);
      if (first) location.hash = pageHash(first.entity_id);
    }
  }

  function typed(event: Event): void {
    const value = (event.currentTarget as HTMLInputElement).value;
    // Typing keeps the search in the space the picker shows, so opening a hit
    // in another space does not narrow the hits under the reader.
    if (!jobOn && picked === null && value.trim() !== "" && shownSpace) picked = spaceId(shownSpace);
    query = value;
    // Clearing the field brings the tree back and drops the highlighting.
    if (value.trim() === "") find = "";
  }

  /** Open a hit with its query carried over. */
  function openHit(entityId: string): void {
    find = found?.needles.join(" ") ?? "";
    location.hash = pageHash(entityId);
  }

  function onsearchkey(event: KeyboardEvent): void {
    const list = hits?.hits ?? [];
    if (!searching || list.length === 0) return;
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp" && event.key !== "Enter") return;
    event.preventDefault();
    const at = list.findIndex((h) => h.entity_id === current);
    const to =
      event.key === "Enter"
        ? Math.max(at, 0)
        : Math.max(0, Math.min(list.length - 1, at + (event.key === "ArrowDown" ? 1 : -1)));
    openHit(list[to]!.entity_id);
  }

  function searchLabel(label: string, chip: HTMLElement): void {
    query = label;
    find = "";
    openSearch(chip);
  }

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

  /**
   * The search field takes focus, below 900px in the drawer, opened from
   * `from`, once it is drawn: a hidden field takes none.
   */
  function openSearch(from: Element | null): void {
    if (drawerBand.current) openDrawer(from);
    if (drawer) void tick().then(() => field?.focus());
    else field?.focus();
  }

  // Widening the window past 900px resets the drawer: the tree is back in its column.
  $effect(() => {
    if (drawerBand.current) return;
    drawer = false;
    opener = null;
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

  // `/` focuses the search from anywhere in Docs, opening the drawer first below 900px.
  $effect(() => {
    const onkey = (event: KeyboardEvent) => {
      if (event.key !== "/" || event.metaKey || event.ctrlKey || event.altKey) return;
      const at = document.activeElement;
      if (at instanceof HTMLInputElement || at instanceof HTMLTextAreaElement || at instanceof HTMLSelectElement) return;
      if (at instanceof HTMLElement && at.isContentEditable) return;
      event.preventDefault();
      openSearch(at);
    };
    window.addEventListener("keydown", onkey);
    return () => window.removeEventListener("keydown", onkey);
  });

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
    all[at]?.scrollIntoView?.({ block: "center" });
  }

  /** The page and the query the find bar last started on. */
  let started: { id: string; stems: string[] } | null = null;

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

  // ---- the outline follows the scrolling ----
  /** A window narrower than 1240px, where the outline folds above the page. */
  const narrow = new MediaQuery(READER_NARROW);
  let active = $state<number | null>(null);
  function follow(): void {
    if (!main || !article) return;
    const top = main.getBoundingClientRect().top + 90;
    let now: number | null = null;
    for (const h of article.querySelectorAll<HTMLElement>("h2[id^='s-']")) {
      if (h.getBoundingClientRect().top < top) now = Number(h.id.slice(2));
    }
    active = now;
  }

  // ---- actions ----
  let linking = $state(false);
  let newPage = $state<{ space: string | null; parent: string | null; title?: string; text?: string } | null>(null);
  let comment = $state("");
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

  async function linkToJob(p: DocPage, ref: string, key: string): Promise<void> {
    try {
      const ticket = await jobTicket(ref);
      if (ticket === null) {
        push({ text: `${key} is not in the mirror`, tone: "err" });
        return;
      }
      const row = drawn(LINK_RELATION, p.entity_id, ticket);
      await linkPorts!.createLink(row.fromId, row.toId, row.relation);
      push({ text: `Linked to ${key}` });
      void rereadPage(p.entity_id);
    } catch (cause) {
      push({ text: `Could not link it: ${ipcErrorMessage(cause)}`, tone: "err" });
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
    comment = words;
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
      toastQueued(`edit section “${name}” of ${p.title}`, sourceOf(p));
      return true;
    } catch (cause) {
      push({ text: `Could not queue the edit: ${ipcErrorMessage(cause)}`, tone: "err" });
      return false;
    }
  }

  async function sendComment(p: DocPage): Promise<void> {
    if (comment.trim() === "" || commenting || refusal("comment") !== null) return;
    commenting = true;
    try {
      const body = comment;
      const id = p.entity_id;
      const write = await io.submitWrite({ Comment: { entity: id, body } });
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
      toastQueued(`comment on ${p.title}`, sourceOf(p));
      comment = "";
    } catch (cause) {
      push({ text: `Could not queue the comment: ${ipcErrorMessage(cause)}`, tone: "err" });
    } finally {
      commenting = false;
    }
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
      toastQueued(`create page ${title}`, under ? sourceOf(under) : null);
      return true;
    } catch (cause) {
      push({ text: `Could not queue the page: ${ipcErrorMessage(cause)}`, tone: "err" });
      return false;
    }
  }

</script>

<div class="tf-rd" class:tf-rd-drawer={drawer}>
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <nav id="tf-docs-tree" class="tf-rd-tree" aria-label="Docs" onkeydown={ondrawerkey} onclick={ondrawerclick}>
    <div class="tf-dl-head">
      {#if job !== null}
        <div class="tf-seg tf-dl-scope" role="group" aria-label="Scope">
          <button type="button" aria-pressed={jobOn} onclick={() => (filter.scope = "job")}
            >This job <span class="tf-jobk">{job}</span>{#if jobIds !== null && jobIds.size > 0}{" "}{jobIds.size}{/if}</button
          >
          <button type="button" aria-pressed={!jobOn} onclick={() => (filter.scope = "all")}>All pages</button>
        </div>
      {/if}
      {#if !jobOn}
        <select aria-label="Space" value={pickerValue} onchange={pick}>
          <option value="*">All spaces ({total})</option>
          {#each tree?.spaces ?? [] as space (spaceId(space))}
            <option value={spaceId(space)}>{spaceLabel(space)}</option>
          {/each}
        </select>
      {/if}
      <label class="tf-fsearch">
        <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true"
          ><circle cx="7" cy="7" r="5" /><path d="M11 11l3.5 3.5" /></svg
        >
        <input
          bind:this={field}
          value={query}
          oninput={typed}
          onkeydown={onsearchkey}
          data-filter
          placeholder="Search {searchScope}"
          aria-label="Search pages"
          autocomplete="off"
        />
        <kbd>/</kbd>
      </label>
      <div class="tf-dl-hint"><code>"…"</code> phrase, <code>-word</code> leave out, <code>by:</code> <code>updated:7d</code></div>
      {@render sync?.()}
    </div>
    <div class="tf-dl-body">
      {#if treeError !== null}
        <p class="tf-dl-note">Could not read the page tree: {treeError}</p>
      {:else if searching}
        {#if searchError !== null}
          <p class="tf-dl-note">Could not search: {searchError}</p>
        {:else if hits !== null}
          <TfDocHits
            found={hits}
            {current}
            scope={searchScope}
            kept={jobOn ? "job" : shownSpace !== undefined ? "space" : null}
            me={page?.me ?? []}
            onall={() => {
              if (jobOn) filter.scope = "all";
              else picked = "*";
            }}
            onpick={openHit}
          />
        {/if}
      {:else if jobOn}
        {#each jobGroups as group (group.reason)}
          <div class="tf-dl-for">
            <h4>{group.reason}</h4>
            {#each group.pages as item (item.entity_id)}
              <a href={pageHash(item.entity_id)} aria-current={item.entity_id === current ? "true" : undefined}
                >{item.title}<span class="tf-why">{item.where}</span></a
              >
            {/each}
          </div>
        {:else}
          {#if jobIds !== null}
            <p class="tf-dl-note">
              Nothing in Docs is on {job} yet. Use Link to {job} on a page.
              <button type="button" class="tf-linkish" onclick={() => (filter.scope = "all")}>Search all pages</button>
            </p>
          {/if}
        {/each}
      {:else}
        {#if forYou.length > 0}
          <div class="tf-dl-for">
            <h4>For you</h4>
            {#each forYou as item (item.entity_id)}
              <a href={pageHash(item.entity_id)} aria-current={item.entity_id === current ? "true" : undefined}
                >{item.title}<span class="tf-why">{forYouWhy(item)}</span></a
              >
            {/each}
          </div>
        {/if}
        {#if tree !== null && tree.spaces.length === 0}
          <p class="tf-dl-note">No pages are synced yet. A source with pages, such as Confluence, fills this tree.</p>
        {:else if tree !== null && picked === "*"}
          {#each tree.spaces as space (spaceId(space))}
            <details open={current !== null && space.pages.some((p) => p.entity_id === current)}>
              <summary class="tf-spc">{space.key} {space.name ?? ""} <span class="tf-count">{space.pages.length}</span></summary>
              <TfDocTree {space} {current} />
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
          >New page</button>
      </div>
    {/if}
  </nav>
  {#if drawer}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="tf-rd-backdrop" onclick={closeDrawer}></div>
  {/if}

  <section class="tf-rd-main" bind:this={main} onscroll={follow}>
    {#if find !== "" && page !== null}
      <div class="tf-findbar" role="search">
        <span>Matches for</span>
        <input value={find} oninput={(e) => (find = e.currentTarget.value)} onkeydown={onfindkey} aria-label="Find in page" />
        <span>{marks === 0 ? "none on this page" : `${Math.max(at, 0) + 1} of ${marks}`}</span>
        <button type="button" onclick={() => jump(-1)} aria-label="Previous">↑</button>
        <button type="button" onclick={() => jump(1)} aria-label="Next">↓</button>
        <button type="button" class="tf-findx" onclick={() => (find = "")} aria-label="Stop highlighting">✕</button>
      </div>
    {/if}
    {#if pageError !== null}
      <div class="tf-empty"><p>Could not open this page: {pageError}</p></div>
    {:else if page !== null}
      {@const p = page}
      {@const created = createdBy(p)}
      <div class="tf-rd-sheet">
        <article class="tf-rd-page" bind:this={article}>
          <div class="tf-rd-crumbrow">
            {#if drawerBand.current}
              <button
                type="button"
                class="tf-btn tf-sm tf-rd-pages"
                aria-controls="tf-docs-tree"
                aria-expanded={drawer}
                onclick={(event) => openSearch(event.currentTarget)}
                ><svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true"
                  ><path d="M2.5 3.5h11M5 8h8.5M5 12.5h8.5M2.5 8h.5M2.5 12.5h.5" /></svg
                >Pages</button
              >
            {/if}
            <div class="tf-rd-crumb">
              {#if job !== null}
                <a class="tf-jobchip tf-card" href={jobHash(jobAt ?? job)}>{job}</a> ›
              {/if}
              <a href="#/docs">Docs</a>
              {#if p.space}
                › <a href={crumbSpace ?? undefined}>{p.space.name ?? p.space.key}</a>
              {/if}
              {#if parent}
                › {#if parent.entity_id}<a href={pageHash(parent.entity_id)}>{parent.title}</a>{:else}{parent.title}{/if}
              {/if}
            </div>
          </div>
          <h1 class="tf-rd-title">
            <TfMarked text={p.title} {stems} />
          </h1>
          <div class="tf-rd-meta">
            {#if p.editor !== null || p.updated_at !== null}
              <span>{person(p.editor, p.editor_name, p.me)} edited {clockLabel(p.updated_at)}</span>
            {/if}
            {#if created}
              <span>Created by {created.who}{#if created.at !== null}, {clockLabel(created.at)}{/if}</span>
            {/if}
            {#if p.version !== null}<span>version {p.version}</span>{/if}
            {#if p.readable}<span>{readingMinutes(p)} min read</span>{/if}
            {#each p.labels as label (label)}
              <button type="button" class="tf-chip" title="Search for {label}" onclick={(event) => searchLabel(label, event.currentTarget)}
                >{label}</button>
            {/each}
          </div>
          <div class="tf-rd-acts">
            {#if job !== null && linkPorts}
              {@const key = job}
              {@const ref = jobAt ?? job}
              <button type="button" class="tf-btn tf-sm" onclick={() => linkToJob(p, ref, key)}>Link to {key}</button>
            {/if}
            {#if linkPorts}
              <button type="button" class="tf-btn tf-sm" onclick={() => (linking = true)}>Link to…</button>
            {/if}
            {#if p.writes.includes("create_page")}
              <button
                type="button"
                class="tf-btn tf-sm"
                {...refusedAttrs(refusal("create_page"))}
                onclick={() => {
                  if (refusal("create_page") === null) newPage = { space: pageSpace ? spaceId(pageSpace) : null, parent: p.entity_id };
                }}
                >New page here</button>
            {/if}
            <button type="button" class="tf-btn tf-sm" onclick={() => copyLink(p)}>Copy link</button>
            {#if p.web_url}
              {@const url = p.web_url}
              <button type="button" class="tf-btn tf-sm" onclick={() => open(url)}>Open in {p.source_name}</button>
            {/if}
            {#if p.readable}
              <TfDocMore onexport={() => exportMarkdown(p)} />
            {/if}
            {#if detail !== null}<TfDetailRefresh {detail} {refresh} {reread} page />{/if}
          </div>
          {#if detail !== null}<TfDetailBanners {detail} failed={refresh.failed} />{/if}
          {#if narrow.current}
            {#key p.entity_id}
              <TfDocOutline page={p} {stems} {active} folded />
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
              <TfDocFoot page={p} spaces={tree?.spaces ?? []} />
            {/key}
            <div class="tf-cmts">
              <h2>Comments {#if p.comments.length > 0}<span class="tf-count">{p.comments.length}</span>{/if}</h2>
              {#each p.comments as c, i (i)}
                {@const mine = c.author !== null && p.me.includes(c.author)}
                <div class="tf-msg">
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
                    <div class="tf-who"><b>You</b> <span class="tf-chip tf-info">{echo.state}</span></div>
                    <p class="tf-cmt-queued">{echo.text}</p>
                  </div>
                </div>
              {/each}
              {#if p.comments.length === 0 && queuedComments.length === 0}
                <p class="tf-dl-note">No comments yet.</p>
              {/if}
              {#if p.writes.includes("comment")}
                <div class="tf-compose">
                  <textarea bind:this={composer} bind:value={comment} placeholder="Comment on this page" aria-label="Comment on this page"
                  ></textarea>
                  <div class="tf-acts">
                    <button
                      type="button"
                      class="tf-btn tf-primary"
                      disabled={commenting || comment.trim() === ""}
                      {...refusedAttrs(refusal("comment"))}
                      onclick={() => sendComment(p)}>Comment</button>
                  </div>
                </div>
              {/if}
            </div>
          </div>
        </article>
        {#if !narrow.current}
          <TfDocOutline page={p} {stems} {active} folded={false} />
        {/if}
      </div>
    {:else if tree !== null && target === null}
      <div class="tf-empty"><p>Nothing to read yet. Pages appear here once a docs source has synced.</p></div>
    {/if}
  </section>
</div>

{#if linking && page !== null && linkPorts}
  {@const p = page}
  <TfLinkTo
    jobKey={p.title}
    entityId={p.entity_id}
    ports={linkPorts}
    {layers}
    onclose={() => (linking = false)}
    onlinked={() => {
      linking = false;
      push({ text: "Linked in knobas" });
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

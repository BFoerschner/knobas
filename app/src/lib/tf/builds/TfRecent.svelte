<!--
  Builds' run table at *All pipelines* and a folder (one page since #2570, `mockups/builds-recent/`; *Recent* from
  2026-11-25, `docs/decisions/2026-11-25-recent-runs.md`, Björn: "I want a view in builds that puts the most recent
  builds at the top, hiding all the folders").

  The newest runs of every pipeline, or of a folder's pipelines (`pipelines`, by id, asked of the backend), newest first
  by when each started or, not started, was queued, read from the mirror a page at a time (`recent_runs`, 60 first, then
  200 a page, at most 2,000 held), with no folds: each row is a pipeline's run row (`TfRunRow`) with a cell naming its
  pipeline, the run's number after it and the folders it sits in as quiet text. Day groups with clock times, as a
  pipeline's runs. Its header names the scope (`title`) beside *Sync now*; `TfBuilds` owns the rail and the scope.

  - **Filter**: the bar's scope (*This job* with a job, *Mine*, *All*), *Result* and, with two or more CI sources
    listed, *Source* narrow the read itself; the typed words narrow the runs held.
  - **Fresh**: the head is read again when a sync ends and when a refresh moved a run it holds or landed a write; the
    runs in flight are read at their source when it draws, when shown again and when the window comes back (at most
    once in 5 s, `runs-in-flight.ts`); while it is shown and the window has focus, each CI source is asked for its run
    once its last ended a minute ago. New runs wait behind *Show N new runs* while the pointer rests on the list
    (`HeldOrder`), and are lit once as they draw.
  - **Back**: what it held, its scroll and the run you opened are kept for the session, so `Esc` from a run lands on the
    same rows with that run's number focused, at the scope it was opened from.
  - **Keys**: Up and Down step the runs, Enter opens one, `m` marks a failure read or unread.
-->
<script lang="ts" module>
  import type { RecentCursor, RecentFilter, RecentPipeline, RecentRun, RecentRuns, RunRow } from "../../ipc/entity";
  import type { SourceSummary, SourceSyncStatus } from "../../ipc/sources";
  import type { EchoListen } from "../queued.svelte";

  /** The reads *Recent* makes, injectable so a test needs no bridge. */
  export interface RecentPorts {
    recent: (filter: RecentFilter, before: RecentCursor | null, limit: number) => Promise<RecentRuns>;
    /** A run read at its source by id, in the background lane (`refresh_entity`). */
    follow?: (runId: string) => Promise<unknown>;
    listen?: EchoListen;
    /** The job's branches, for *This job*. */
    jobBranches?: (job: string) => Promise<string[]>;
    /** The configured sources and what they are doing, for the minute's ask. */
    sources?: () => Promise<SourceSummary[]>;
    status?: () => Promise<SourceSyncStatus[]>;
    syncNow?: (sourceId: string) => Promise<number>;
    /** Whether the window has focus: the minute's ask waits for it. */
    focused?: () => boolean;
  }

  /** What *Recent* held when it was left: drawn again at once on the way back (Fable's ruling 3). */
  interface Kept {
    basis: string;
    runs: RecentRun[];
    pipelines: ReadonlyMap<string, RecentPipeline>;
    next: RecentCursor | null;
    scroll: number;
    opened: string | null;
  }
  let kept: Kept | null = null;

  /** Forget what *Recent* held: a new session, which is what each test is. */
  export function forgetRecent(): void {
    kept = null;
  }

  /** How long after a source's last run ended *Recent* asks it for another while shown (Fable's ruling 7). */
  export const ASK_AFTER_MS = 60_000;
  /** How often *Recent* looks whether a source is due: a source is asked within a quarter minute of being due. */
  export const ASK_TICK_MS = 15_000;
</script>

<script lang="ts">
  import { onDestroy, tick, untrack, type Snippet } from "svelte";
  import { ipcErrorMessage } from "../../ipc";
  import { recentRuns } from "../../ipc/entity";
  import { listSources, refreshEntity, syncNow, syncStatus } from "../../ipc/sources";
  import { windowListen } from "../../ipc/shared-listen";
  import { EVENTS } from "../../ipc";
  import type { RefreshEnded } from "../../ipc/sources";
  import { cacheEvents } from "../../ipc/answer-cache";
  import { Windowed } from "../../shell/windowed.svelte";
  import FilterBar from "../filter/FilterBar.svelte";
  import TfNoMatch from "../filter/TfNoMatch.svelte";
  import { sessionFilter } from "../filter/filter-state.svelte";
  import type { FilterConfig } from "../filter/filter";
  import TfWindowedRows from "../TfWindowedRows.svelte";
  import { HeldOrder } from "../held-order.svelte";
  import { counted } from "../counted";
  import { pageVisible } from "../kept-pages.svelte";
  import { onFocusWhileShown, everyWhileShown } from "../while-shown.svelte";
  import TfRunRow from "./TfRunRow.svelte";
  import { heldOrder, inOrder } from "../filter/ranked.svelte";
  import { failedRun, runHash, unreadRun } from "./builds";
  import { RESULTS, byDay, usualSeconds } from "./pipeline";
  import { readJobBranches } from "./pipeline-ports";
  import { runsRead } from "./runs-read.svelte";
  import { InFlightReads, IN_FLIGHT_AFTER_MS, IN_FLIGHT_AGAIN_MS, runsInFlight } from "./runs-in-flight";
  import { askRunAgain } from "./run-again";
  import { RECENT_FIRST, RECENT_HOLD, RECENT_PAGE, recentFilterOf, recentMarks, recentText, rowPipeline, withHead, withOlder, withPipelines } from "./recent";

  const BRIDGE: RecentPorts = {
    recent: (filter, before, limit) => recentRuns(filter, before, limit),
    follow: (runId) => refreshEntity(runId, true),
    jobBranches: (job) => readJobBranches(job),
    sources: () => listSources(),
    status: () => syncStatus(),
    syncNow: (sourceId) => syncNow(sourceId),
    focused: () => typeof document !== "undefined" && document.hasFocus(),
  };

  let {
    ready,
    job,
    ports = BRIDGE,
    crumbs,
    sync,
    title,
    pipelines: scoped = [],
    place = null,
    paths = "long",
    syncRevision = 0,
    ondrawn,
  }: {
    ready: boolean;
    job: string | null;
    ports?: RecentPorts | undefined;
    /** The breadcrumb, the Builds page's. */
    crumbs: Snippet;
    /** The view's *Sync now* (#2106). */
    sync?: Snippet | undefined;
    /** What the header names: *All pipelines*, or the folder's name (#2570). */
    title: string;
    /**
     * Whose runs it lists (#2570): `[]` every pipeline's, else a folder's pipelines by id; `null` while the folder's
     * pipelines are still being read, when it reads nothing.
     */
    pipelines?: readonly string[] | null;
    /** The folder's place at a folder (#2572, `RunScope`): each row says its folders from it down. `null` at *All*. */
    place?: readonly string[] | null;
    /** How wide the folder column stands (`folderColumn`): the names start at the same line on every row. */
    paths?: "long" | "short" | "none";
    /** Moves when a sync ended: the head is read again. */
    syncRevision?: number;
    /** Its first page has drawn: the Builds list is read behind it, so a run opens at once. */
    ondrawn?: () => void;
  } = $props();

  const shown = pageVisible();
  const filter = sessionFilter("builds-recent");
  if (filter.scope === null) filter.scope = "all";

  // -- the job's branches, for *This job* --------------------------------------
  let jobBranches = $state<string[] | null>(null);
  $effect(() => {
    const wanted = job;
    jobBranches = null;
    if (wanted === null || !ready) return;
    (ports.jobBranches ?? BRIDGE.jobBranches!)(wanted).then(
      (names) => {
        if (job === wanted) jobBranches = names;
      },
      () => {
        if (job === wanted) jobBranches = [];
      },
    );
  });
  const scope = $derived(job !== null || filter.scope !== "job" ? filter.scope : "all");
  const asked = $derived<RecentFilter>(
    recentFilterOf({ scope, results: filter.chips["result"] ?? [], source: filter.dropdowns["source"] ?? null }, scope === "job" ? jobBranches : null, scoped ?? []),
  );
  /** Waiting for the job's branches, or a folder's pipelines: nothing is read until they are known. */
  const waiting = $derived((scope === "job" && jobBranches === null) || scoped === null);
  const basis = $derived(JSON.stringify(asked));

  // -- the reads ---------------------------------------------------------------
  const back = kept;
  let held = $state.raw<RecentRun[] | null>(back?.runs ?? null);
  let pipelines = $state.raw<ReadonlyMap<string, RecentPipeline>>(back?.pipelines ?? new Map());
  let next = $state.raw<RecentCursor | null>(back?.next ?? null);
  let heldFor = back?.basis ?? "";
  let failed = $state<string | null>(null);
  let older = $state<"idle" | "loading" | "failed">("idle");
  /** The head read the older page out belongs to (`reads` when it was asked). */
  let olderFor = -1;
  /** Every source a read has named: *Source* is offered once there are two. */
  let seenSources = $state.raw<Map<string, string>>(new Map([...(back?.pipelines.values() ?? [])].map((p) => [p.source_id, p.source_name])));
  let reads = 0;
  let askedOnce = false;

  function lay(page: RecentRuns) {
    pipelines = withPipelines(pipelines, page.pipelines);
    const sources = new Map(seenSources);
    for (const p of page.pipelines) sources.set(p.source_id, p.source_name);
    if (sources.size !== seenSources.size) seenSources = sources;
  }

  /** The head read again, laid over what is held; a new filter starts from it alone. */
  async function readHead() {
    if (!ready || waiting) return;
    const at = ++reads;
    const wanted = basis;
    const filterNow = asked;
    // Nothing held for this filter: the rows that fill the window first, drawn, then the rest of the page below them.
    const first = heldFor !== wanted || held === null;
    try {
      const page = await ports.recent(filterNow, null, first ? RECENT_FIRST : RECENT_PAGE);
      if (at !== reads) return;
      lay(page);
      const fresh = heldFor !== wanted || held === null;
      const whole = page.next === null;
      const laid = fresh ? page.runs.slice(0, RECENT_HOLD) : withHead(held!, page.runs, whole);
      // What is held past the head keeps its cursor; a head that is all of it has none.
      const keptOlder = !fresh && !whole && laid.length > page.runs.length;
      next = whole ? null : keptOlder ? next : page.next;
      held = laid;
      heldFor = wanted;
      failed = null;
      inFlightSoon();
      // Builds reads its pipeline list once the first rows have painted (#2570): heard at once, the list's answer (571 KB
      // at Björn's scale) and the rail it now draws at every scope could land inside the first rows' frame.
      if (first) afterPaint(() => untrack(() => ondrawn)?.());
      else untrack(() => ondrawn)?.();
      // The rest once the first rows have painted: asked at once, its answer came back inside the first rows' render on
      // the scale database at 6x, and the frame then drew all 200 in two passes (merger's review of #2569).
      if (first && next !== null) afterPaint(() => at === reads && void more(RECENT_PAGE - RECENT_FIRST));
      // Shown: the sources asked once, then each minute (Fable's ruling 7). The first ask once the first rows have
      // painted too: its two reads went out beside their render (#2569b).
      if (!askedOnce) {
        askedOnce = true;
        afterPaint(() => void askSources());
      }
    } catch (cause) {
      if (at === reads) failed = ipcErrorMessage(cause);
      untrack(() => ondrawn)?.();
    }
  }

  /** The frame and the task `afterPaint` waits on, cancelled when the page goes. */
  let paintFrame: number | undefined;
  let paintTask: ReturnType<typeof setTimeout> | undefined;
  const painted: (() => void)[] = [];
  /** `run` once the page has painted (as `TfDocs`' `afterPaint`); at once where there are no frames. */
  function afterPaint(run: () => void): void {
    if (typeof requestAnimationFrame !== "function") return run();
    painted.push(run);
    if (paintFrame !== undefined || paintTask !== undefined) return;
    paintFrame = requestAnimationFrame(() => {
      paintFrame = undefined;
      paintTask = setTimeout(() => {
        paintTask = undefined;
        for (const one of painted.splice(0)) one();
      }, 0);
    });
  }

  /** An older page of `limit` runs, after what is held. */
  async function more(limit = RECENT_PAGE) {
    // An older page out for a head read since replaced is no reason to wait: its answer is dropped.
    if (held === null || next === null || (older === "loading" && olderFor === reads) || held.length >= RECENT_HOLD) return;
    const at = (olderFor = reads);
    older = "loading";
    try {
      const page = await ports.recent(asked, next, limit);
      if (at !== reads) {
        // A head read came between (a sync's news, a new filter): the footer is not left saying *Reading*.
        if (olderFor === at) older = "idle";
        return;
      }
      lay(page);
      held = withOlder(held, page.runs);
      next = held.length >= RECENT_HOLD ? null : page.next;
      older = "idle";
      inFlightSoon();
    } catch {
      if (at === reads) older = "failed";
      else if (olderFor === at) older = "idle";
    }
  }

  // A new filter, or the first draw: the head. What came back from a run is drawn first and read again behind it.
  $effect(() => {
    void basis;
    void ready;
    void waiting;
    untrack(() => {
      if (heldFor !== basis) held = null;
      void readHead();
    });
  });
  let revisionAt = untrack(() => syncRevision);
  $effect(() => {
    const at = syncRevision;
    if (at === revisionAt) return;
    revisionAt = at;
    untrack(() => void readHead());
  });

  /** The head read again at most once in 5 s, the last always read: a refresh that moved a run, a landed write. */
  let headSoon: ReturnType<typeof setTimeout> | undefined;
  let headAt = 0;
  function readHeadSoon() {
    if (headSoon !== undefined) return;
    headSoon = setTimeout(() => {
      headSoon = undefined;
      headAt = Date.now();
      void readHead();
    }, Math.max(0, headAt + IN_FLIGHT_AGAIN_MS - Date.now()));
  }
  $effect(() => {
    if (!ready) return;
    const listen = untrack(() => ports.listen) ?? ((event, handler) => windowListen(event, handler));
    let dead = false;
    let off: (() => void) | null = null;
    let offSync: (() => void) | null = null;
    // A CI source's run that changed something ended (a scheduled one, the minute's ask, *Sync now*): the head again.
    void listen(EVENTS.syncState, ({ payload }) => {
      const row = payload as SourceSyncStatus;
      if (dead || row.running || row.last_changed !== true) return;
      if (seenSources.size > 0 && !seenSources.has(row.source_id)) return;
      cacheEvents.syncState(row);
      readHeadSoon();
    })
      .then((stop) => (dead ? stop() : (offSync = stop)))
      .catch(() => {});
    void listen(EVENTS.itemRefreshed, ({ payload }) => {
      const ended = payload as RefreshEnded;
      if (dead || ended.ended === "failed") return;
      // A run it holds moved, or a write landed (a run started from a run's page): the head again.
      if (ended.write_id === null && !(held?.some((one) => one.run.entity_id === ended.entity_id) ?? false)) return;
      cacheEvents.refreshed(ended);
      readHeadSoon();
    })
      .then((stop) => (dead ? stop() : (off = stop)))
      .catch(() => {});
    return () => {
      dead = true;
      off?.();
      offSync?.();
    };
  });

  // -- runs in flight, and the sources' own runs -------------------------------
  const inFlight = new InFlightReads(untrack(() => ports.follow));
  let inFlightTimer: ReturnType<typeof setTimeout> | undefined;
  function inFlightSoon() {
    clearTimeout(inFlightTimer);
    inFlightTimer = setTimeout(() => inFlight.read(runsInFlight(held?.map((one) => one.run) ?? null)), IN_FLIGHT_AFTER_MS);
  }
  onFocusWhileShown(shown, () => inFlight.read(runsInFlight(held?.map((one) => one.run) ?? null)));

  /**
   * Each CI source asked for its own run once its last ended a minute ago, while shown and focused (Fable's ruling 7):
   * as wide as the source, so a run started at TeamCity shows within a minute. A source that syncs only by hand, is off,
   * is running or backing off is never asked.
   */
  async function askSources() {
    const { sources, status, syncNow: start } = ports;
    if (!ready || sources === undefined || status === undefined || start === undefined) return;
    if (!(ports.focused ?? BRIDGE.focused!)()) return;
    const ci = new Set(seenSources.keys());
    if (ci.size === 0) return;
    try {
      const [configured, states] = await Promise.all([sources(), status()]);
      const now = Date.now();
      for (const source of configured) {
        if (!ci.has(source.id) || !source.enabled || source.sync_by_hand) continue;
        const state = states.find((one) => one.source_id === source.id);
        if (state === undefined || state.running) continue;
        if (state.backoff_until !== null && Date.parse(state.backoff_until) > now) continue;
        const ended = state.last_finished_at === null ? 0 : Date.parse(state.last_finished_at);
        if (now - ended < ASK_AFTER_MS) continue;
        void start(source.id).catch(() => {});
      }
    } catch {
      // Nothing asked: the next minute asks again.
    }
  }
  everyWhileShown(() => ready && shown(), ASK_TICK_MS, () => void askSources());
  onFocusWhileShown(shown, () => void askSources());

  // -- what is drawn -----------------------------------------------------------
  /**
   * The typed words rank the runs held through the app's ranking (#2535), each by its own words and its pipeline's
   * name and place, and decide only what keeps: the runs stay newest first, in their days. A new list whenever the
   * pipelines or how the place reads change, since both are in each run's text.
   */
  const runs = $derived.by(() => {
    void pipelines;
    void manySources;
    void place;
    return [...(held ?? [])];
  });
  const runText = (one: RecentRun) => recentText(one, pipelines, manySources, place);
  const runOrder = heldOrder<RecentRun>("recent-runs");
  $effect(() => runOrder.ask(filter.text, runs, runText));
  const typed = $derived(inOrder(runs, runOrder.rows(filter.text, runs, runText)));
  const runMarking = $derived(runOrder.typed(filter.text, runs));
  const byId = $derived(new Map((held ?? []).map((one) => [one.run.entity_id, one])));
  const runHold = new HeldOrder<RecentRun>((one) => one.run.entity_id);
  const holdBasis = $derived(JSON.stringify([basis, filter.text, runOrder.settled(filter.text, runs)]));
  // Only rows read for the filter now: a new filter's basis is here a turn before the effect that drops the last rows, and
  // placed then they were held under it, kept as "left" once its page landed, and drawn with no run behind them.
  const placed = $derived(held === null || heldFor !== basis ? { rows: [] as RecentRun[], waiting: 0, stale: false } : runHold.place(typed, holdBasis));
  // A run's place moves only when it starts (queued by when it was queued, then by when it started): that is no reason to
  // offer *Sort again*, since the list holds its order (#2574). A run that left is.
  const heldBar = $derived({ waiting: placed.waiting, stale: (placed.left ?? 0) > 0, show: () => runHold.release() });

  /**
   * The pointer off the runs (#2574, the mockup's *States*): a new run takes the top, lit once, and a run that left
   * goes; a queued run that started keeps its row and its place, and only its chip, number and clock change.
   */
  let pointerOn = $state(false);
  $effect(() => {
    if (!shown()) pointerOn = false;
  });
  $effect(() => {
    if (!pointerOn && (placed.waiting > 0 || (placed.left ?? 0) > 0)) untrack(() => runHold.settle());
  });
  let runsBox = $state<HTMLElement | null>(null);
  function showNew() {
    runHold.release();
    void tick().then(() => runsBox?.querySelector(".tf-runtbl tr")?.scrollIntoView?.({ block: "nearest" }));
  }

  /** The runs a read brought to the top after the list drew, each lit once. */
  let arrived = $state.raw<ReadonlySet<string>>(new Set());
  let drawnRuns: Set<string> | null = null;
  let drawnFor: string | null = null;
  $effect(() => {
    const rows = placed.rows;
    const at = holdBasis;
    if (drawnFor !== at) {
      drawnFor = at;
      drawnRuns = null;
    }
    if (rows.length === 0) return;
    if (drawnRuns === null) {
      drawnRuns = new Set(rows.map((one) => one.run.entity_id));
      return;
    }
    const seen = drawnRuns;
    const top = rows.findIndex((one) => seen.has(one.run.entity_id));
    const fresh = rows.slice(0, Math.max(0, top));
    for (const one of rows) seen.add(one.run.entity_id);
    if (fresh.length > 0) untrack(() => (arrived = new Set([...arrived, ...fresh.map((one) => one.run.entity_id)])));
  });

  const days = $derived(byDay(placed.rows.map((one) => one.run)));
  const dayWin = new Windowed({ count: () => days.length, key: (i) => days[i]!.key, estimate: 260, from: 30 });
  const RUN_ROW_PX = 34;
  const manySources = $derived(seenSources.size > 1);
  /** Each pipeline's longest run held: what its runs' bars are measured against, as on its own page. */
  const longest = $derived.by(() => {
    const most = new Map<string, number>();
    for (const one of held ?? []) {
      const took = one.run.duration_seconds ?? 0;
      if (took > (most.get(one.pipeline_id) ?? 0)) most.set(one.pipeline_id, took);
    }
    return most;
  });
  /** Each pipeline's usual time over the runs held (`usualSeconds`): the track its running runs' bars fill along (#2574). */
  const usual = $derived.by(() => {
    const byPipeline = new Map<string, RunRow[]>();
    for (const one of held ?? []) {
      const runs = byPipeline.get(one.pipeline_id);
      if (runs === undefined) byPipeline.set(one.pipeline_id, [one.run]);
      else runs.push(one.run);
    }
    return new Map([...byPipeline].map(([id, runs]) => [id, usualSeconds(runs)]));
  });

  const config = $derived<FilterConfig>({
    id: "builds-recent",
    placeholder: "Filter runs",
    noun: "runs",
    one: "run",
    scopes: [
      ...(job !== null ? [{ id: "job", label: "This job", title: `Runs on ${job}'s branches`, query: "" }] : []),
      { id: "mine", label: "Mine", title: "Runs you started", query: "" },
      { id: "all", label: "All", query: "" },
    ],
    chips: [{ key: "result", label: "Result", options: RESULTS.map((value) => ({ value, label: value })) }],
    dropdowns: manySources
      ? [{ key: "source", label: "Source", options: [...seenSources.entries()].map(([value, label]) => ({ value, label })) }]
      : [],
  });

  // -- marks -------------------------------------------------------------------
  $effect(() => runsRead.hold("recent", (held ?? []).filter((one) => failedRun(one.run)).map((one) => one.run.entity_id)));
  const narrowed = $derived(filter.text.trim() !== "" || asked.mine || asked.branches !== null || asked.results.length > 0 || asked.sources.length > 0);
  const unreadListed = $derived(placed.rows.filter((one) => unreadRun(one.run, runsRead.read)).length);
  /** A folder's scope (#2570): its pipelines, or none at *All pipelines*. */
  const folder = $derived(scoped !== null && scoped.length > 0 ? scoped : null);
  const markSaid = $derived(
    narrowed
      ? `Mark all as read: the ${counted(unreadListed, "unread failed run", "unread failed runs")} listed`
      : folder !== null
        ? `Mark all as read: every failed run of the pipelines in ${title}`
        : "Mark all as read: every failed run of every pipeline",
  );
  function markAllRead() {
    const listed = placed.rows.filter((one) => failedRun(one.run)).map((one) => one.run.entity_id);
    if (narrowed) void runsRead.markAll({ runs: listed, pipelines: [] });
    else if (folder !== null) void runsRead.markAll({ runs: listed, pipelines: [...folder] });
    else void runsRead.markAll({ runs: [], pipelines: [], everything: true });
  }

  // -- open, keys, the way back --------------------------------------------------
  /** The run last opened or stepped to: focused on the way back. Never drawn, so no state. */
  let opened: string | null = back?.opened ?? null;
  function scrollerOf(): HTMLElement | null {
    return runsBox?.closest<HTMLElement>(".tf-scroller") ?? null;
  }
  function openRow(event: MouseEvent, one: RecentRun) {
    opened = one.run.entity_id;
    if ((event.target as Element | null)?.closest("a, button, input, select, label")) return;
    if ((window.getSelection()?.toString() ?? "") !== "") return;
    location.hash = runHash(one.pipeline_id, one.run.entity_id);
  }
  function runAgain(one: RecentRun) {
    opened = one.run.entity_id;
    askRunAgain(one.run.entity_id);
    location.hash = runHash(one.pipeline_id, one.run.entity_id);
  }
  function runKeys(event: KeyboardEvent) {
    if (event.altKey || event.ctrlKey || event.metaKey || event.shiftKey || event.defaultPrevented) return;
    const row = event.target instanceof Element ? event.target.closest<HTMLElement>("tr[data-run]") : null;
    if (row === null) return;
    if (event.key === "m") {
      const one = byId.get(row.dataset["run"] ?? "");
      if (one === undefined || !failedRun(one.run)) return;
      event.preventDefault();
      void runsRead.toggle(one.run);
      return;
    }
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
    const links = [...(runsBox?.querySelectorAll<HTMLElement>("tr[data-run] a.tf-run-pno") ?? [])];
    const at = links.findIndex((link) => row.contains(link));
    const to = links[event.key === "ArrowDown" ? at + 1 : at - 1];
    if (to === undefined) return;
    event.preventDefault();
    to.focus();
    to.scrollIntoView?.({ block: "nearest" });
    const run = to.closest<HTMLElement>("tr[data-run]")?.dataset["run"];
    if (run !== undefined) opened = run;
  }
  // Back from a run: its rows, its scroll, and the run you opened focused, once the windowed rows have drawn it.
  let restored = back === null;
  $effect(() => {
    if (restored || runsBox === null || placed.rows.length === 0) return;
    restored = true;
    const at = back?.scroll ?? 0;
    const was = back?.opened ?? null;
    const scroller = scrollerOf();
    if (scroller !== null) scroller.scrollTop = at;
    let tries = 0;
    const focus = () => {
      const row = [...(runsBox?.querySelectorAll<HTMLElement>("tr[data-run]") ?? [])].find((one) => one.dataset["run"] === was);
      const link = row?.querySelector<HTMLElement>("a.tf-run-pno");
      if (link) link.focus({ preventScroll: true });
      else if ((tries += 1) < 10) requestAnimationFrame(focus);
    };
    if (was !== null) void tick().then(focus);
  });

  // An older page when the list's end comes near.
  let sentinel = $state<HTMLElement | null>(null);
  $effect(() => {
    const end = sentinel;
    if (end === null || typeof IntersectionObserver === "undefined") return;
    const seen = new IntersectionObserver((entries) => {
      if (entries.some((entry) => entry.isIntersecting)) void untrack(() => more());
    }, { rootMargin: "600px" });
    seen.observe(end);
    return () => seen.disconnect();
  });

  onDestroy(() => {
    painted.length = 0;
    if (paintFrame !== undefined && typeof cancelAnimationFrame === "function") cancelAnimationFrame(paintFrame);
    clearTimeout(paintTask);
    clearTimeout(headSoon);
    clearTimeout(inFlightTimer);
    reads += 1;
    kept = held === null ? null : { basis: heldFor, runs: held, pipelines, next, scroll: scrollerOf()?.scrollTop ?? 0, opened };
  });
</script>

<!-- ux18's mark, at the bar's end after the count: every failed run of every pipeline unfiltered, the runs listed filtered. -->
{#snippet after()}
  <button type="button" class="tf-iconbtn tf-sm tf-readall" aria-label="Mark all as read" title={markSaid} onclick={markAllRead}
    ><svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"
      ><path d="M1.5 8.5l3 3 6-7M7.5 11.5l1 0.5 6-7.5" /></svg
    ></button
  >
{/snippet}

{@render crumbs()}
<!-- The scope's name (#2570), no count: the header stands as a pipeline's does, so nothing above the bar moves between scopes. -->
<header class="tf-fhead tf-bhead">
  <div><h1>{title}</h1></div>
  {#if sync}<div class="tf-acts">{@render sync()}</div>{/if}
</header>

<div class="tf-runbar tf-recentbar">
  <FilterBar {config} {filter} shown={held === null ? null : placed.rows.length} total={held?.length ?? null} room={RECENT_PAGE} held={heldBar} {after} />
</div>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="tf-runs tf-recent" data-paths={paths} bind:this={runsBox} onpointerenter={() => (pointerOn = true)} onpointerleave={() => (pointerOn = false)} onkeydown={runKeys}>
  {#if placed.waiting > 0}
    <div class="tf-newruns"><button type="button" title="Started since the list drew. They wait here while the pointer is on the runs, so nothing moves under it." onclick={showNew}>Show {counted(placed.waiting, "new run", "new runs")}</button></div>
  {/if}
  {#if failed !== null}
    <p class="tf-dim">Runs could not be read: {failed}</p>
  {:else if held === null}
    <p class="tf-dim">Reading runs…</p>
  {:else if held.length === 0 && !narrowed}
    <div class="tf-empty"><p>No runs yet. A CI source's runs show here once it has synced.</p></div>
  {:else if placed.rows.length === 0}
    <TfNoMatch noun="runs" {filter}>
      {#if filter.scope !== "all"}
        <button type="button" class="tf-btn tf-sm" onclick={() => (filter.scope = "all")}>Show all runs</button>
      {/if}
    </TfNoMatch>
  {/if}
  <div class="tf-days" {@attach dayWin.list}>
    {#each dayWin.slice(days) as day, at (day.key)}
      <div class="tf-day" class:tf-day-first={dayWin.start + at === 0} {@attach dayWin.row(day.key)}>
        <h3 class="tf-dayh">{day.label}</h3>
        <table class="tf-tbl tf-runtbl" aria-label={`Runs, ${day.label}`}>
          <colgroup
            ><col class="tf-run-no" /><col class="tf-run-pipe" /><col class="tf-run-br" /><col class="tf-run-trig" /><col class="tf-run-tests" /><col
              class="tf-run-dur"
            /><col class="tf-run-at" /><col class="tf-run-acts" /></colgroup
          >
          <TfWindowedRows rows={day.runs} key={(run) => run.entity_id} estimate={RUN_ROW_PX} colspan={8} overscan={150}>
            {#snippet row(run, attach)}
              {@const one = byId.get(run.entity_id)!}
              {@const pipe = pipelines.get(one.pipeline_id)}
              <TfRunRow
                {run}
                {attach}
                href={runHash(one.pipeline_id, run.entity_id)}
                typed={runMarking}
                marks={filter.text.trim() === "" ? null : recentMarks(one, pipelines, manySources, runOrder.marks(one), place)}
                {job}
                jobBranches={jobBranches ?? []}
                max={longest.get(one.pipeline_id) ?? 0}
                usual={usual.get(one.pipeline_id) ?? null}
                arrived={arrived.has(run.entity_id)}
                pipeline={rowPipeline(pipe, manySources, place)}
                onopen={(event) => openRow(event, one)}
                onagain={() => runAgain(one)}
              />
            {/snippet}
          </TfWindowedRows>
        </table>
      </div>
    {/each}
  </div>
  {#if held !== null && held.length > 0}
    <div class="tf-older" hidden={placed.rows.length === 0} bind:this={sentinel}>
      {#if older === "failed"}
        <p class="tf-dim">Older runs could not be read.</p>
        <button type="button" class="tf-btn tf-sm" onclick={() => ((older = "idle"), void more())}>Try again</button>
      {:else if older === "loading"}
        <p class="tf-dim">Reading older runs…</p>
      {:else if held.length >= RECENT_HOLD}
        <p class="tf-dim">The newest {RECENT_HOLD.toLocaleString("en")} runs. A pipeline's own page reads its older history.</p>
      {:else if next === null}
        <p class="tf-dim">Every run knobas holds.</p>
      {:else}
        <button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => void more()}>Older runs</button>
      {/if}
    </div>
  {/if}
</div>

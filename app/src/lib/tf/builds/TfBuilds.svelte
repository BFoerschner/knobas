<!--
  Builds (#848, R-9.1, R-9.4a, ADR-0022): the tracer of the Builds frame.
  The shell draws it over the declared pipeline and run kinds, so nothing
  here names an adapter: the source's word is its configured name, drawn
  once, as the header line over its pipelines.

  - **Left**, the pipeline list: with a job current, *This job <key> / All*
    (#850, *This job* by default) and the *Failing* and *Running* chips; the
    page's search is the launcher's `#b`, which `/` opens (#2425). *All* draws *Pinned* first
    (knobas-local, #849), then each source's project tree as deep as it goes
    (folds with their count and *N failing*, own pipelines before
    subgroups, a chain of single children as one row, a fold open on a
    failure, the selection or anything narrowing, else as the reader left
    it). *This job* draws the same tree pruned to the job's pipelines, each
    with its reasons, under "N of M pipelines are relevant to KEY". The
    list is a tree of one-line rows (the rail-tree pass, 2026-11-13): each
    pipeline a glyph of its latest run (`TfRunGlyph`), its name in err tone
    when that run failed with the prefix its fold's pipelines share drawn
    dim, and a time only when it failed, runs or waits (`railTime`); a
    hover gives its groups, name and latest run. A fold's summary counts
    what fails and runs under it; *Collapse all* / *Expand all* sets every
    fold as the reader's own. Up and Down step along the rows, Left and
    Right shut and open a fold. The selected one is drawn inverted and kept
    in view. The list is windowed (#2211) over its flattened rows
    (`listRows`): headings, pipelines and the summaries of folds, a fold's
    rows following its summary while it is open.
  - **Right**, whose runs the rail has selected (one page, #2570, `mockups/builds-recent/`): *All pipelines* (the
    rail's first row, above its tools) and a folder list their runs in one table (`TfRecent.svelte`), its header
    naming the scope with *Sync now*; a pipeline keeps its page (#851, `TfPipeline.svelte`): header with *Pin* and,
    last, the view's *Sync now* (#2287), history chart, and the Runs, Parameters and Triggers tabs. The crumbs and the
    header stand where they stand at every scope. The rail's tools narrow the rail, never the table.

  **Read and unread** (ux18, `docs/decisions/2026-11-17-mark-builds-as-read.md`): a pipeline's name and time are err
  only while its newest failure is unread (`asksAttention`, the window's `runsRead`); read, its glyph stays red. A
  fold counts every failing pipeline and tones the count err, and opens by itself, only for one that asks. *Mark all as
  read* in the tools marks every failed run of the pipelines listed, with *Undo*.

  **A run started here** (the TeamCity run fix, 2026-11-25): its pipeline's row says *queued* from the queue's answer
  (`withQueued`), and a row whose run moved at the source (the write queue reads a run it started to its end) reads
  that pipeline's runs again, at most once in 5 s.

  `#/builds` draws the last place stood on this session, *All pipelines* the first time (Björn's answer 4 on #2554);
  `#/builds/recent` is *All pipelines*; `#/builds/folder/<source id>/<group>/…` a folder by project, and
  `#/builds/folder/~repo/<repository id>` one by repository (`~repo` alone: *No repository*), which turns the rail to
  *By repository*; `#/builds/<entity id>` selects that pipeline. `#/build/<pipeline>/<run>` is the
  run page (#853, `TfRun.svelte`) with its pipeline selected on the left;
  `#/build/<run>` finds the run's pipeline and moves to that address.
-->
<script lang="ts" module>
  import { SvelteMap } from "svelte/reactivity";
  import TfKey from "../TfKey.svelte";
  import TfKeyed from "../TfKeyed.svelte";
  import type { RecentPorts } from "./TfRecent.svelte";
  import { replaceAddress } from "../opener";

  import type { CommitRepository, JobPipeline, PipelineRow, RailPipeline, RunRow, SourceView } from "../../ipc/entity";

  /** The reads and the write this page makes, injectable so a test needs no bridge. */
  export interface BuildsPorts {
    /** The rail's read (#2571): each pipeline's latest run, its runs in flight and its commits, never the strip. */
    pipelines: () => Promise<RailPipeline[]>;
    runs: (pipelineId: string) => Promise<RunRow[]>;
    /** Pin or unpin (#849); the bridge's when omitted. */
    setPinned?: (pipelineId: string, pinned: boolean) => Promise<boolean>;
    /** The job's pipelines and their reasons, by the job's key (#850); the bridge's when omitted. */
    jobPipelines?: (job: string) => Promise<JobPipeline[]>;
    /** A run's whole view, for the pipeline a bare run address names (#853); the bridge's when omitted. */
    view?: (runId: string) => Promise<SourceView | null>;
    /** The repositories of the list's commits, for *By repository* (the repo-builds pass); the bridge's when omitted. */
    commitRepositories?: (commitIds: string[]) => Promise<CommitRepository[]>;
    /** The shell's events: `item:refreshed` reads the list again when a run it draws moved; the window's when omitted. */
    listen?: EchoListen;
    /** *Recent*'s reads (2026-11-25); the bridge's when omitted. */
    recent?: RecentPorts;
  }

  /** The folds the reader opened or closed this session, by key (R-9.3). */
  const remembered = new SvelteMap<string, boolean>();

  /** Forget the folds: a new session, which is what each test is. */
  export function forgetFolds(): void {
    remembered.clear();
  }
</script>

<script lang="ts">
  import TfSideEdge from "../TfSideEdge.svelte";
  import { sideWidth } from "../side-width.svelte";
  import TfScope from "../filter/TfScope.svelte";
  import TfNothingHere from "../TfNothingHere.svelte";
  import { menuThing } from "../context/thing";
  import TfRailBar from "../TfRailBar.svelte";
  import TfScroller from "../TfScroller.svelte";
  import { afterLayout } from "../../shell/after-layout";
  import TfBareHeader from "../TfBareHeader.svelte";
  import { flushSync, tick, untrack } from "svelte";
  import type { RoleFrameProps } from "../page-entry";

  import {
    commitRepositories,
    getEntity,
    jobPipelines as readJobPipelines,
    pipelineRuns,
    railPipelines as readPipelines,
    setPipelinePinned,
    type PipelineReason,
  } from "../../ipc/entity";
  import { listSources } from "../../ipc/sources";
  import { resolveTicket } from "../job/job";
  import { Windowed } from "../../shell/windowed.svelte";
  import { sessionFilter, sessionValue } from "../filter/filter-state.svelte";
  import type { EscTarget } from "../escape";
  import TfCrumbs, { type Crumb } from "../TfCrumbs.svelte";
  import TfPipeline from "./TfPipeline.svelte";
  import TfRun from "./TfRun.svelte";
  import TfRunGlyph from "./TfRunGlyph.svelte";
  import TfRecent from "./TfRecent.svelte";
  import { RECENT_HASH, REPO_FOLDER, folderPipelines, runScopeHash, runScopeOf, type RunScope } from "./recent";
  import { parseTfHash } from "../route";
  import { pipelineOfRun } from "./run";
  import { runsRead } from "./runs-read.svelte";
  import { RUN_BRIDGE } from "./pipeline-ports";
  import { QUEUED_RUN, withQueued } from "./pipeline";
  import { echoesOn, type EchoListen } from "../queued.svelte";
  import { collectDescendants, isAltClick, TREE_EXPAND_CAP } from "../tree-alt";
  import { windowListen } from "../../ipc/shared-listen";
  import { EVENTS } from "../../ipc";
  import type { RefreshEnded } from "../../ipc/sources";
  import { cacheEvents } from "../../ipc/answer-cache";
  import {
    CHIPS,
    foldHolds,
    foldOpen,
    groupsOf,
    recentNewestFirst,
    relaid,
    asksAttention,
    unreadRunsOf,
    namesEvery,
    foldPipelines,
    keptPipelines,
    latestTime,
    listRows,
    pipelineHash,
    pinnedOf,
    railGlyph,
    railTime,
    reasonsById,
    reasonsLine,
    repoTreeOf,
    runHash,
    treeOf,
    type Fold,
    type Scope,
  } from "./builds";

  let {
    ready,
    head,
    rest,
    job,
    jobAt = null,
    back = null,
    ports = { pipelines: readPipelines, runs: (id) => pipelineRuns(id) },
    sync,
    syncRevision = 0,
  }: {
    /** Whether the database answers yet: the reads wait for it. */
    ready: boolean;
    /** `builds` for the list, `build` for a run's address. */
    head: string;
    /** What follows the head in the address. */
    rest: string[];
    /** The chosen job (spec #2227), for *This job*. */
    job: string | null;
    /** The chosen job's entity id (#1525), which its ticket is read by; its key when omitted. */
    jobAt?: string | null;
    /** Where `Esc` goes, which the breadcrumb's hint names; `null` where it does nothing (#2228). */
    back?: EscTarget | null;
    ports?: BuildsPorts;
  } & RoleFrameProps = $props();

  const filter = sessionFilter("builds-pipelines");
  /** The runs marked read (ux18), `null` until read: a failure asks for attention until its run is in it. */
  const read = $derived(runsRead.read);
  $effect(() => {
    if (ready) void runsRead.load();
  });
  const lastPipeline = sessionValue("builds-last-pipeline");
  const chips = $derived(filter.chips["state"] ?? []);

  /**
   * The pipelines as `pipelines` answers them: the read is CI's, so the mirror
   * narrows it to the CI role's fillers and the page drops nothing (#1295,
   * ADR-0029 as amended on 2026-10-19).
   */
  let all = $state.raw<RailPipeline[] | null>(null);
  let failed = $state<string | null>(null);

  /**
   * Where Builds was last stood this session (#2570): *All pipelines*, a folder or a pipeline, as its address. A bare
   * `#/builds` draws that place, and *All pipelines* the first time (Björn's answer 4 on #2554), at its own address: it
   * stays the section's top page, which `Esc` leaves alone, as it drew the last pipeline before.
   */
  const lastPlace = sessionValue("builds-place");
  /** The address under `#/builds` the page draws: a bare one's is the last place's. */
  const here = $derived(head === "builds" && rest.length === 0 ? (parseTfHash(lastPlace.value ?? RECENT_HASH) as { rest?: string[] }).rest ?? [] : rest);
  /**
   * Whose runs the one table lists (#2570, the mockup's rule 1): *All pipelines* (`#/builds/recent`) or a folder
   * (`#/builds/folder/<place>`); `null` on a pipeline's page or a run's, which keep their own.
   */
  const runScope = $derived<RunScope | null>(head === "builds" ? runScopeOf(here) : null);
  /** *All pipelines*' first page has drawn: the list is read behind it, not beside it. */
  let recentDrawn = $state(false);

  $effect(() => {
    if (!ready || all !== null || (runScope?.kind === "all" && !recentDrawn)) return;
    ports.pipelines().then(
      (rows) => (all = rows.map(recentNewestFirst)),
      (error: unknown) => (failed = message(error)),
    );
  });

  /** Bumped on every read again of the list, so an older one answering late is dropped. */
  let listReads = 0;
  /**
   * The list read again, kept drawn meanwhile: when *Sync now*'s runs end
   * (#2106), and on the pipeline page's *Refresh* (#2137), whose row of the
   * pipeline is among it. A failed read keeps the list and says so, until a
   * read again answers.
   */
  async function rereadPipelines() {
    const at = ++listReads;
    try {
      const rows = await ports.pipelines();
      if (at !== listReads) return;
      all = rows.map(recentNewestFirst);
      failed = null;
    } catch (error) {
      if (at === listReads) failed = message(error);
    }
  }

  /**
   * A failure marked unread that the list names nowhere (#2543): it was read when the list was read, so no pipeline's
   * `unread` holds it, and the rail's count would miss it. The list is read again once the mark's write answered, so the
   * read sees it. A mark the list names (*Undo* of a press, a run the rail reads) moves the count through `read`
   * alone.
   */
  let readBefore = untrack(() => read);
  $effect(() => {
    const now = read;
    const before = readBefore;
    readBefore = now;
    if (before === null || now === null || before === now) return;
    const back = [...before].filter((id) => !now.has(id));
    if (back.length > 0 && untrack(() => all !== null && !namesEvery(all, back))) void runsRead.settled().then(rereadPipelines);
  });

  /**
   * A pipeline's row read again when a run it draws moved, or a run queued on it here was read back (the TeamCity run
   * fix, 2026-11-25): its runs alone, laid into its row as the rail's read lays them (`relaid`), not the whole list. At
   * once on the first, then at most once in `RAIL_AGAIN_MS`, the last always read.
   */
  const RAIL_AGAIN_MS = 5_000;
  let railReadAt = 0;
  let railSoon: ReturnType<typeof setTimeout> | undefined;
  const railOwed = new Set<string>();
  function rereadSoon(pipelineId: string): void {
    railOwed.add(pipelineId);
    if (railSoon !== undefined) return;
    const wait = Math.max(0, railReadAt + RAIL_AGAIN_MS - Date.now());
    railSoon = setTimeout(() => {
      railSoon = undefined;
      railReadAt = Date.now();
      const owed = [...railOwed];
      railOwed.clear();
      for (const id of owed) void rereadRuns(id);
    }, wait);
  }
  /** One pipeline's runs read again and laid into its row; the list's next read replaces it whole as before. */
  async function rereadRuns(pipelineId: string): Promise<void> {
    const at = listReads;
    let rows: RunRow[];
    try {
      rows = await ports.runs(pipelineId);
    } catch {
      return;
    }
    // A read of the whole list since answers for this row too.
    if (at !== listReads || all === null) return;
    all = all.map((p) => (p.entity_id === pipelineId ? relaid(p, rows) : p));
  }
  /** The listed pipeline whose run the refresh that ended moved, or on which it read back a run queued here; `null` for none. */
  function movedPipeline(ended: RefreshEnded): string | null {
    if (ended.ended === "failed" || all === null) return null;
    const moved =
      ended.write_id !== null
        ? all.find((p) => echoesOn(p.entity_id, QUEUED_RUN).some((echo) => echo.write === ended.write_id))
        : all.find((p) => p.recent.some((run) => run.entity_id === ended.entity_id));
    return moved?.entity_id ?? null;
  }
  $effect(() => {
    if (!ready) return;
    const listen = untrack(() => ports.listen) ?? ((event, handler) => windowListen(event, handler));
    let dead = false;
    let off: (() => void) | null = null;
    void listen(EVENTS.itemRefreshed, ({ payload }) => {
      const moved = dead ? null : movedPipeline(payload as RefreshEnded);
      if (moved === null) return;
      // What it moved goes from the window's answers before this row's read (`hearRuns`' reason).
      cacheEvents.refreshed(payload as RefreshEnded);
      rereadSoon(moved);
    })
      .then((stop) => (dead ? stop() : (off = stop)))
      .catch(() => {});
    return () => {
      dead = true;
      off?.();
      clearTimeout(railSoon);
      railSoon = undefined;
    };
  });

  let pipelinesAt = untrack(() => syncRevision);
  $effect(() => {
    const at = syncRevision;
    if (!ready || at === pipelinesAt) return;
    pipelinesAt = at;
    void untrack(() => rereadPipelines());
  });

  // One default on every page (design system, Layout): *All*. A ticket card's row turns *This job* on.
  const scope = $derived<Scope>(job !== null && filter.scope === "job" ? "job" : "all");
  let reasons = $state<Map<string, PipelineReason[]> | null>(null);
  /** What the job's ticket is read by: the address it was opened at (#1537), else its key. */
  const jobRef = $derived(job === null ? null : (jobAt ?? job));
  $effect(() => {
    const key = jobRef;
    reasons = null;
    if (!ready || key === null) return;
    const read = ports.jobPipelines ?? jobPipelinesByKey;
    read(key).then(
      (rows) => {
        if (jobRef === key) reasons = reasonsById(rows);
      },
      // A job whose reasons cannot be read keeps nothing: *This job* says so
      // through its empty state, and *All* still lists everything.
      () => {
        if (jobRef === key) reasons = new Map();
      },
    );
  });
  /** The job's pipelines, whatever the scope: a bare `#/builds` with nothing remembered opens the job's first. */
  const jobsOwn = $derived(all === null || reasons === null ? [] : all.filter((p) => reasons!.has(p.entity_id)));
  /** The pipelines *This job* narrows the list to, before the words and chips narrow them. */
  const onJob = $derived(scope === "job" ? jobsOwn : []);
  const kept = $derived(
    all === null ? [] : keptPipelines(scope === "job" ? onJob : all, chips),
  );
  /** The pipeline on the right: the one the list picks, or on a run's address the run's own. */
  function selectedOf(list: RailPipeline[]): RailPipeline | null {
    return list.find((p) => p.entity_id === (head === "builds" ? here : rest)[0]) ?? null;
  }
  const selected = $derived(all === null || runScope !== null ? null : selectedOf(all));
  $effect(() => {
    if (selected !== null && (rest.length > 0 || head !== "builds")) lastPipeline.value = selected.entity_id;
  });
  // The place a bare `#/builds` comes back to: a scope or a pipeline stood on, never a run (its scope stays the place).
  $effect(() => {
    if (head !== "builds" || rest.length === 0) return;
    if (runScope !== null) lastPlace.value = runScopeHash(runScope);
    else if (selected !== null) lastPlace.value = pipelineHash(selected.entity_id);
  });
  /** The page you are on where the rail's filter leaves it out; `null` while the job's reasons are still being read. */
  const outside = $derived(
    selected === null || (scope === "job" && reasons === null) || kept.some((p) => p.entity_id === selected.entity_id) ? null : selected,
  );

  // `#/build/<run id>` (a job's or the inbox's row) names the run alone: its
  // pipeline is the one whose recent runs hold it, else the one its view names.
  /** A bare run address no pipeline was found for. */
  let lost = $state<string | null>(null);
  $effect(() => {
    if (all === null || head !== "build" || rest.length !== 1) return;
    // A run's address that names a pipeline and no run is the pipeline's page.
    if (selected !== null) return replaceAddress(pipelineHash(selected.entity_id));
    const id = rest[0]!;
    const owner = all.find((p) => p.recent.some((r) => r.entity_id === id));
    if (owner !== undefined) {
      replaceAddress(runHash(owner.entity_id, id));
      return;
    }
    const ids = all.map((p) => p.entity_id);
    void (ports.view ?? RUN_BRIDGE.view)(id).then(
      (view) => {
        const found = pipelineOfRun(view, ids);
        if (found !== null && rest[0] === id) replaceAddress(runHash(found, id));
        else lost = id;
      },
      () => (lost = id),
    );
  });
  const pinned = $derived(scope === "job" ? [] : pinnedOf(kept));

  /**
   * *By project* or *By repository* (the repo-builds pass): the source's own tree, or a fold for each repository the
   * pipeline's runs built. A run names its commit (`commit_id`) already, so the repositories are one keyed read of
   * those commits, made only while *By repository* is on and kept by the answer cache.
   */
  const grouping = sessionValue("builds-group");
  const byRepository = $derived(grouping.value === "repository");
  /** Each commit the list's runs name, by its repositories; `null` until the read answered. */
  let commitRepos = $state.raw<Map<string, { id: string; title: string }[]> | null>(null);
  /** Why the read failed: said over the list, with *Try again*, since every pipeline then sits under *No repository*. */
  let commitReposFailed = $state<string | null>(null);
  /** Bumped by *Try again*: the same commits are asked again. */
  let commitsRetry = $state(0);
  let commitsAsked = "";
  $effect(() => {
    void commitsRetry;
    if (!byRepository || all === null) return;
    const ids = [...new Set(all.flatMap((p) => p.commits))].sort();
    const asked = JSON.stringify(ids);
    if (asked === commitsAsked) return;
    commitsAsked = asked;
    untrack(() => ports.commitRepositories ?? commitRepositories)(ids).then(
      (rows) => {
        if (asked !== commitsAsked) return;
        const map = new Map<string, { id: string; title: string }[]>();
        for (const row of rows) map.set(row.commit_id, [...(map.get(row.commit_id) ?? []), { id: row.repo_id, title: row.repo_title }]);
        commitRepos = map;
        commitReposFailed = null;
      },
      // A read that fails groups nothing by repository: every pipeline is under *No repository*, and the list says why.
      (error: unknown) => {
        if (asked !== commitsAsked) return;
        commitRepos = new Map();
        commitReposFailed = message(error);
        commitsAsked = "";
      },
    );
  });
  /** Each listed pipeline's commits (#2571): what its last twelve runs built, as the rail's read names them. */
  const commitsOf = $derived(new Map((all ?? []).map((p) => [p.entity_id, p.commits])));
  /** The repositories a pipeline's last twelve runs built, each once. */
  function reposOf(pipeline: PipelineRow): { id: string; title: string }[] {
    const seen = new Map<string, { id: string; title: string }>();
    for (const commit of commitsOf.get(pipeline.entity_id) ?? []) for (const repo of commitRepos?.get(commit) ?? []) seen.set(repo.id, repo);
    return [...seen.values()];
  }
  const trees = $derived(byRepository ? (commitRepos === null ? [] : repoTreeOf(kept, reposOf, read)) : treeOf(kept, read));

  // ---- a folder's scope (#2570) ----
  /** A folder's place: a source's groups (*By project*) or a repository (*By repository*); `null` at no folder. */
  const folderPath = $derived(runScope?.kind === "folder" ? runScope.path : null);
  const repoFolder = $derived(folderPath?.[0] === REPO_FOLDER);
  // A repository's folder is drawn by *By repository*: its address turns the rail to it, so the rail shows its pill.
  $effect(() => {
    if (repoFolder && !untrack(() => byRepository)) grouping.value = "repository";
  });
  /**
   * The folder's pipelines by id, read against every pipeline (the rail's tools narrow the rail, never the table, the
   * mockup's rule 2); `null` while the pipelines, or a repository's commits, are still being read.
   */
  const folderIds = $derived(
    folderPath === null || all === null || (repoFolder && commitRepos === null) ? null : folderPipelines(all, folderPath, reposOf),
  );
  /** What the header and the last crumb call the folder: its last group, or its repository's name. */
  const folderName = $derived.by(() => {
    if (folderPath === null) return "";
    if (!repoFolder) return folderPath.at(-1) ?? "";
    const id = folderPath[1];
    if (id === undefined) return "No repository";
    for (const repos of commitRepos?.values() ?? []) for (const repo of repos) if (repo.id === id) return repo.title;
    return "";
  });
  /** The group above the folder, as a pipeline's crumbs name its group. */
  const folderAbove = $derived(folderPath !== null && !repoFolder && folderPath.length > 2 ? folderPath.at(-2)! : null);
  /** Whether `f` is the folder the table lists: drawn as the rail's pill. */
  function isScope(f: Fold): boolean {
    return folderPath !== null && f.path.length === folderPath.length && f.path.every((part, at) => part === folderPath[at]);
  }
  /** Select a folder: its table, and the fold opened as the reader's own toggle (the mockup's *Interactions*). */
  function selectFold(f: Fold): void {
    remembered.set(f.key, true);
    location.hash = runScopeHash({ kind: "folder", path: f.path });
  }

  /**
   * *Mark all as read* (ux18): every failed run of the pipelines the rail lists now, its filter applied, so unfiltered
   * every pipeline. The unread runs drawn turn at once; the backend marks the rest of each pipeline's history.
   */
  function markAllRead() {
    void runsRead.markAll({
      runs: unreadRunsOf(kept, read),
      pipelines: kept.map((p) => p.entity_id),
    });
  }
  /** Whether anything narrows the list, which opens every fold. */
  const narrowed = $derived(chips.length > 0 || scope === "job");
  /**
   * A fold the reader has not opened or shut keeps the state it first drew in (the still-rows pass, 2026-11-11): one
   * opens by itself while a pipeline in it fails, and a sync bringing a pass shut it, or a failure opened it, and the
   * rail's rows moved under the pointer. A changed filter decides again; the open pipeline's fold still opens.
   */
  let foldsDrew = new Map<string, boolean>();
  let foldsBasis = "";
  /** What the list draws, flattened: the windowed list's rows. */
  const rows = $derived.by(() => {
    const basis = JSON.stringify([narrowed, scope, chips]);
    if (basis !== foldsBasis) {
      foldsDrew = new Map();
      foldsBasis = basis;
    }
    const at = selected?.entity_id ?? null;
    return listRows(pinned, trees, scope, (f) => {
      const own = remembered.get(f.key);
      if (own !== undefined) return own;
      if (!foldsDrew.has(f.key)) foldsDrew.set(f.key, foldOpen(undefined, f, at, narrowed));
      return foldsDrew.get(f.key)! || foldHolds(f, at);
    });
  });
  // A pipeline or a fold is one 26 px line; headings are measured once drawn. From 40 rows, not the default 300: the
  // scale estate's 75 rows drawn whole were a third of Builds' first open, a 194 ms task at 4x with the runs beside them
  // (the Windows pass, 2026-11-14), as the Inbox windows from 40.
  const win = new Windowed({ count: () => rows.length, key: (i) => rows[i]!.key, estimate: 26, from: 40, overscan: 300 });
  /** Whether any drawn fold is open: the tools' button then shuts them all, else opens them all. */
  const anyOpen = $derived(rows.some((row) => row.kind === "fold" && row.open));

  /** The job's key resolved to its ticket, then its pipelines (#850). */
  async function jobPipelinesByKey(key: string) {
    const sources = await listSources();
    const ticket = await resolveTicket(key, sources, getEntity);
    return ticket === null ? [] : readJobPipelines(ticket.row.entity_id);
  }

  function setScope(next: Scope) {
    filter.scope = next;
  }

  async function togglePin(pipeline: PipelineRow) {
    const write = ports.setPinned ?? setPipelinePinned;
    const now = await write(pipeline.entity_id, !pipeline.pinned);
    if (all !== null) all = all.map((p) => (p.entity_id === pipeline.entity_id ? { ...p, pinned: now } : p));
  }

  /** A row's indent class: 14px a level, as prototype-6's, up to {@link DEEPEST}. */
  const DEEPEST = 8;
  function indent(depth: number): string {
    return `tf-d${Math.min(depth, DEEPEST)}`;
  }

  function rememberFold(f: Fold, event: Event) {
    const open = (event.currentTarget as HTMLDetailsElement).open;
    // Only the reader's own toggle is remembered, not the one the open
    // attribute raises when the fold is drawn.
    if (open !== foldOpen(remembered.get(f.key), f, selected?.entity_id ?? null, narrowed)) remembered.set(f.key, open);
  }

  /** Alt+click on a Builds folder toggles it and every nested folder under it. */
  function altFold(f: Fold, isOpen: boolean, event: MouseEvent) {
    if (!isAltClick(event)) return;
    event.preventDefault();
    const want = !isOpen;
    for (const node of collectDescendants([f], (x) => x.folds, TREE_EXPAND_CAP)) remembered.set(node.key, want);
  }

  /**
   * *Collapse all* / *Expand all*: every fold, as if the reader had toggled each. The unfiltered tree's too, so a fold
   * a chip or *This job* hides is set when it comes back (a filter can join single children another way).
   */
  function setAllFolds(open: boolean) {
    const walk = (folds: readonly Fold[]) => {
      for (const f of folds) {
        remembered.set(f.key, open);
        walk(f.folds);
      }
    };
    for (const tree of [...trees, ...treeOf(all ?? [])]) walk(tree.folds);
  }

  /** The drawn tree's folds by key. */
  function foldsByKey(): Map<string, Fold> {
    const map = new Map<string, Fold>();
    for (const row of rows) if (row.kind === "fold") map.set(row.fold.key, row.fold);
    return map;
  }

  /** A drawn row's depth, read off its indent class. */
  function depthOf(row: HTMLElement): number {
    return Number(/\btf-d(\d)\b/.exec(row.className)?.[1] ?? 0);
  }

  /**
   * The rail's keys, the docs tree's table (`docs/tree-keys.ts`, F-444) over the rows drawn: Up and Down step, Home and
   * End go to the ends, Right opens a fold and then steps into it, Left shuts it, and from a pipeline goes to its fold.
   * A fold opened or shut so is the reader's own toggle.
   */
  function railKeys(event: KeyboardEvent) {
    if (event.altKey || event.ctrlKey || event.metaKey || event.shiftKey || list === undefined) return;
    const row = event.target instanceof HTMLElement ? event.target : null;
    if (row === null || !row.matches(".tf-pl-rows .tf-tr")) return;
    const drawn = [...list.querySelectorAll<HTMLElement>(".tf-pl-rows .tf-tr")];
    const at = drawn.indexOf(row);
    const fold = row.dataset["fold"];
    const open = fold !== undefined && (row.parentElement as HTMLDetailsElement | null)?.open === true;
    let next: HTMLElement | undefined;
    switch (event.key) {
      case "ArrowDown":
        next = drawn[at + 1];
        break;
      case "ArrowUp":
        next = drawn[at - 1];
        break;
      case "Home":
        next = drawn[0];
        break;
      case "End":
        next = drawn.at(-1);
        break;
      case "ArrowRight":
        if (fold !== undefined && !open) remembered.set(fold, true);
        else if (open && drawn[at + 1] !== undefined && depthOf(drawn[at + 1]!) > depthOf(row)) next = drawn[at + 1];
        break;
      case "Enter": {
        // A fold's Enter selects it (the mockup's *Interactions*); Right and Left open and shut it. A pipeline's link goes.
        const f = fold === undefined ? undefined : foldsByKey().get(fold);
        if (f === undefined) return;
        selectFold(f);
        break;
      }
      case "ArrowLeft":
        if (fold !== undefined && open) remembered.set(fold, false);
        else next = drawn.slice(0, at).reverse().find((above) => above.dataset["fold"] !== undefined && depthOf(above) < depthOf(row));
        break;
      default:
        return;
    }
    event.preventDefault();
    next?.focus();
    next?.scrollIntoView?.({ block: "nearest" });
  }

  function message(error: unknown): string {
    if (error && typeof error === "object" && "message" in error) return String(error.message);
    return String(error);
  }

  let list: HTMLElement | undefined = $state();

  function toggle(chip: string) {
    filter.chips["state"] = chips.includes(chip) ? chips.filter((c) => c !== chip) : [...chips, chip];
  }

  // The selected pipeline stays in view as the list scrolls on its own (R-9.4a). Once the frame has laid the rail out
  // (`afterLayout`), not in the task that mounts the page: there its reads laid out the whole new page, a run's with it,
  // and the run's own reads waited behind them (the ux30 end gate, Windows). The frame still paints the rail scrolled.
  /**
   * A run opened from a scope's table (#2570; *Recent* before, 2026-11-25) holds the rail's rows unseen in their room
   * until the scroll below has put its pipeline in view: drawn, they stood where the scope left them for a frame, then
   * moved (`just moves`, `recent-run` and `run`). Only that step: a pipeline pressed in the rail is in view already.
   */
  let railUnseen = $state(false);
  let wasScope = untrack(() => runScope !== null);
  $effect.pre(() => {
    const now = runScope !== null;
    if (wasScope && !now && head === "build") railUnseen = true;
    wasScope = now;
  });
  $effect(() => {
    if (selected === null || list === undefined) {
      if (list !== undefined && all !== null) railUnseen = false;
      return;
    }
    const id = selected.entity_id;
    const rail = list;
    let gone = false;
    let cancel = () => {};
    void tick().then(() => {
      if (gone) return;
      cancel = afterLayout(rail, () => {
        // Drawn first if the window left it out.
        win.reveal(rows.findIndex((row) => row.kind === "pipeline" && row.pipeline.entity_id === id));
        flushSync();
        rail.querySelector('[aria-current="true"]')?.scrollIntoView?.({ block: "nearest" });
        railUnseen = false;
      });
    });
    return () => {
      gone = true;
      cancel();
    };
  });


  /** Below 900 px, whether the pipelines rail is unfolded under its bar. */
  let railOpen = $state(false);
</script>

<!-- The header while no pipeline or run is drawn: the view's name and its *Sync now* (#2287). -->
{#snippet bare()}
  <TfBareHeader title="Builds" {sync} />
{/snippet}

<!-- A run scope's crumbs (#2570): *All pipelines*, or a folder under the group above it, as a pipeline's name its group. -->
{#snippet scopeCrumbs()}
  <TfCrumbs parts={[{ name: "Builds" }, ...(runScope?.kind === "folder" ? [...(folderAbove !== null ? [{ name: folderAbove }] : []), { name: folderName }] : [{ name: "All pipelines" }])]} {back} />
{/snippet}

{#snippet crumbs(parts: Crumb[])}
  <TfCrumbs {parts} {back} />
{/snippet}

<!-- A pipeline's name, the prefix its fold's pipelines share drawn dim (`sharedPrefix`). -->
{#snippet name(pipeline: PipelineRow, pre: string)}
  <span class="tf-pi-nm" class:tf-bad={asksAttention(pipeline, read)}
    >{#if pre !== ""}<span class="tf-pi-pre">{pre}</span>{/if}{pipeline.name.slice(pre.length)}</span
  >
{/snippet}

{#snippet item(pipeline: PipelineRow, depth: number, key: string, pre: string, tag: string | null)}
  {@const drawn = withQueued(pipeline, echoesOn(pipeline.entity_id, QUEUED_RUN))}
  {@const glyph = railGlyph(drawn)}
  {@const time = railTime(drawn, undefined, read)}
  {@const latest = latestTime(pipeline)}
  {@const why = scope === "job" ? reasonsLine(reasons?.get(pipeline.entity_id) ?? [], job ?? "") : ""}
  <a
    class={`tf-tr tf-pi ${indent(depth)}`}
    href={pipelineHash(pipeline.entity_id)}
    aria-current={pipeline.entity_id === selected?.entity_id}
    {@attach menuThing(() => ({ kind: "pipeline", id: pipeline.entity_id, hash: pipelineHash(pipeline.entity_id), unread: unreadRunsOf([pipeline], read) }))}
    title={`${[...groupsOf(pipeline), pipeline.name].join(" › ")}\n${glyph.word}${latest !== "" ? ` ${latest}` : ""}`}
    {@attach win.row(key)}
  >
    <TfRunGlyph {...glyph} />
    {@render name(pipeline, pre)}
    {#if tag !== null}<span class="tf-pi-src">{tag}</span>{/if}
    {#if why !== ""}<span class="tf-pi-why"><TfKeyed text={why} /></span>{/if}
    {#if time !== null}<span class={`tf-pi-when${time.tone !== null ? ` tf-${time.tone}` : ""}`}>{time.text}</span>{/if}
  </a>
{/snippet}

{#snippet foldView(f: Fold, depth: number, open: boolean, key: string)}
  <!-- The summary alone: a fold's rows follow it while it is open, and a closed one has none (#2200's class, #2211). -->
  <details class="tf-pl-fold" {open} ontoggle={(event) => rememberFold(f, event)} {@attach win.row(key)}>
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
    <!-- Its name selects it and opens it; a press beside the name folds it (#2570, the mockup's *Interactions*). -->
    <summary class={`tf-tr tf-tfold ${indent(depth)}`} data-fold={f.key} aria-current={isScope(f)} title={`${f.path[0] === REPO_FOLDER ? f.name : f.path.slice(1).join(" › ")}\n${f.count === 1 ? "1 pipeline" : `${f.count} pipelines`}`}
      onclick={(event) => {
        if (!isAltClick(event) && (event.target as Element).closest(".tf-fold-nm")) {
          event.preventDefault();
          selectFold(f);
          return;
        }
        altFold(f, open, event);
      }}
      {@attach menuThing(() => {
        const under = foldPipelines(f);
        return { kind: "pipeline-folder", name: f.name, unread: unreadRunsOf(under, read), pipelines: under.map((p) => p.entity_id) };
      })}
      ><span class="tf-fold-nm">{f.name}</span><span class="tf-fold-at"
        >{#if f.failing > 0}<span class="tf-count" class:tf-err={f.asking > 0}>{f.failing}<span class="tf-sr">{" failing"}</span></span>{/if}{#if f.running > 0}<span
            class="tf-count tf-info">{f.running}<span class="tf-sr">{" running"}</span></span
          >{/if}<span class="tf-fold-n">{f.count}<span class="tf-sr">{f.count === 1 ? " pipeline" : " pipelines"}</span></span></span
      ></summary
    >
  </details>
{/snippet}

<div class="tf-bv" class:tf-rail-shut={!railOpen} {@attach sideWidth("builds")}>
  <TfSideEdge side="builds" />
  <TfRailBar label="Pipeline" value={selected?.name ?? (runScope?.kind === "folder" ? folderName : "All pipelines")} open={railOpen} controls="tf-pipelines-rail" onpress={() => (railOpen = !railOpen)} onclose={() => (railOpen = false)} />
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
  <nav
    id="tf-pipelines-rail"
    class="tf-pl tf-rail"
    aria-label="Pipelines"
    onclick={(event) => {
      if ((event.target as Element).closest("a[href]")) railOpen = false;
    }}
    onkeydown={railKeys}
  >
    <!-- *All pipelines* (#2570, Björn's answer 9): the rail's first row, above its tools, which never narrow it. -->
    <div class="tf-pl-home">
      <a class="tf-tr tf-pi tf-d0 tf-pl-root" href={RECENT_HASH} aria-current={runScope?.kind === "all"} title="Every pipeline's runs, newest first"
        ><svg class="tf-pl-rooticon" viewBox="0 0 16 16" aria-hidden="true"
          ><path d="M2 4h12M2 8h12M2 12h8" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" /></svg
        ><span class="tf-pi-nm">All pipelines</span><span class="tf-fold-n" class:tf-unseen={all === null}
          >{all?.length ?? 0}<span class="tf-sr">{all?.length === 1 ? " pipeline" : " pipelines"}</span></span
        ></a
      >
    </div>
    <div class="tf-pl-top">
      <div class="tf-pl-group">
        {#if job !== null}
          <TfScope
            scopes={[{ id: "job", label: "This job", count: reasons === null ? null : jobsOwn.length, title: `The pipelines ${job} reaches` }, { id: "all", label: "All" }]}
            {scope}
            onpick={(id) => setScope(id as Scope)}
          />
        {/if}
        <div class="tf-seg" role="group" aria-label="Group by">
          <button type="button" aria-pressed={!byRepository} onclick={() => (grouping.value = null)}>By project</button>
          <button type="button" aria-pressed={byRepository} onclick={() => (grouping.value = "repository")}>By repository</button>
        </div>
      </div>
      <div class="tf-pl-tools">
        <div class="tf-fchips" role="group" aria-label="Show only">
          {#each CHIPS as chip (chip)}
            <button type="button" aria-pressed={chips.includes(chip)} onclick={() => toggle(chip)}>{chip}</button>
          {/each}
        </div>
        <div class="tf-trail">
          {#if rows.some((row) => row.kind === "fold")}
            <button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => setAllFolds(!anyOpen)}>{anyOpen ? "Collapse all" : "Expand all"}</button>
          {/if}
          {#if kept.length > 0}
            <!-- ux18: what the rail lists now, its filter applied. An icon at the rail's width, so the tools stay one line
                 and no row moves; its words are its name and its tip. -->
            {@const said = narrowed ? "Mark all as read: every failed run of the pipelines listed" : "Mark all as read: every failed run of every pipeline"}
            <button type="button" class="tf-iconbtn tf-sm tf-readall" aria-label="Mark all as read" title={said} onclick={markAllRead}
              ><svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"
                ><path d="M1.5 8.5l3 3 6-7M7.5 11.5l1 0.5 6-7.5" /></svg
              ></button
            >
          {/if}
        </div>
      </div>
    </div>
    <div class="tf-pl-list" bind:this={list}>
      {#if failed !== null}
        <p class="tf-pl-note">Pipelines could not be read: {failed}</p>
      {:else if all !== null && all.length === 0}
        <p class="tf-pl-note">No pipelines yet. A CI source's pipelines show here once it has synced.</p>
      {:else if all !== null && scope === "job" && reasons !== null}
        <p class="tf-scope-note">
          {onJob.length} of {all.length} pipelines are relevant to <span class="tf-mono">{job}</span>{#if kept.length !== onJob.length}, {kept.length} shown{/if}.
          <button type="button" class="tf-linkish" onclick={() => setScope("all")}>Show all</button>
        </p>
      {/if}
      {#if all !== null && all.length > 0 && kept.length === 0 && (scope === "all" || reasons !== null)}
        <p class="tf-pl-note">
          No pipeline matches.
          {#if scope === "job"}<button type="button" class="tf-linkish" onclick={() => setScope("all")}>Search all pipelines</button>{/if}
        </p>
      {/if}
      {#if outside !== null}
        <!-- The filter hides the page you are on, so the rail would show no place at all: it stays, until you leave it. -->
        <div class="tf-pl-pin">
          <a class="tf-tr tf-pi tf-d0" href={pipelineHash(outside.entity_id)} aria-current="true" title={outside.name}>
            <TfRunGlyph {...railGlyph(outside)} />
            {@render name(outside, "")}
            <span class="tf-pi-why">{#if scope === "job" && !jobsOwn.some((p) => p.entity_id === outside.entity_id)}Not in <TfKey value={job ?? ""} />{:else}Hidden by the filter{/if}</span>
          </a>
        </div>
      {/if}
      {#if byRepository && commitRepos === null && all !== null && all.length > 0}
        <p class="tf-pl-note">Reading which repositories each pipeline builds…</p>
      {:else if byRepository && commitReposFailed !== null}
        <p class="tf-pl-note">
          Which repositories the pipelines build could not be read: {commitReposFailed}.
          <button type="button" class="tf-linkish" onclick={() => (commitsRetry += 1)}>Try again</button>
        </p>
      {/if}
      <div class="tf-pl-rows" class:tf-unseen={railUnseen} {@attach win.list}>
        {#each win.slice(rows) as row (row.key)}
          {#if row.kind === "head"}
            <h3 {@attach win.row(row.key)}>{row.text}</h3>
          {:else if row.kind === "pipeline"}
            {@render item(row.pipeline, row.depth, row.key, row.pre, row.tag)}
          {:else}
            {@render foldView(row.fold, row.depth, row.open, row.key)}
          {/if}
        {/each}
      </div>
    </div>
  </nav>
  <TfScroller class="tf-bmain">
    {#if runScope !== null && folderIds !== null && folderIds.length === 0}
      {@render scopeCrumbs()}
      {@render bare()}
      <TfNothingHere message="knobas does not hold this folder. Its pipelines may not have synced yet." href={RECENT_HASH} action="All pipelines" />
    {:else if runScope !== null}
      <!-- One table at every run scope (#2570): its rows, header and bar stand where they stand at the next scope. -->
      <TfRecent
        {ready}
        ports={ports.recent}
        {job}
        crumbs={scopeCrumbs}
        {sync}
        title={runScope.kind === "all" ? "All pipelines" : folderName}
        pipelines={runScope.kind === "all" ? [] : folderIds}
        {syncRevision}
        ondrawn={() => (recentDrawn = true)}
      />
    {:else if head === "build" && selected !== null && rest.length >= 2}
      <TfRun pipeline={selected} {rest} {job} runs={ports.runs} {crumbs} {sync} />
    {:else if head === "build"}
      {@render crumbs([{ name: "Builds", href: "#/builds" }, { name: "Run" }])}
      {@render bare()}
      {#if all !== null && (rest.length !== 1 || lost === rest[0])}
        <TfNothingHere message="knobas does not hold this run. It may not have synced yet." href="#/builds" action="All pipelines" />
      {/if}
    {:else if selected !== null}
      <!-- One crumb at both of a pipeline's addresses: `#/builds` draws this page already, so nothing above it links. -->
      {@render crumbs([{ name: "Builds" }, ...(selected.group !== null ? [{ name: selected.group }] : []), { name: selected.name }])}
      <TfPipeline
        pipeline={selected}
        {rest}
        {job}
        runs={ports.runs}
        onpin={() => togglePin(selected!)}
        rereadRow={rereadPipelines}
        {sync}
      />
    {:else if all !== null && all.length > 0}
      {@render crumbs([{ name: "Builds" }, { name: "Pipeline" }])}
      {@render bare()}
      <TfNothingHere message="knobas does not hold this pipeline. It may not have synced yet." href="#/builds" action="All pipelines" />
    {:else}
      {@render crumbs([{ name: "Builds" }])}
      {@render bare()}
    {/if}
  </TfScroller>
</div>

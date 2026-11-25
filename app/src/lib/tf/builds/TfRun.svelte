<!--
  The run page (#853, R-9.9 to R-9.11, ADR-0022): the right-hand side of
  Builds at `#/build/<pipeline>/<run>[/<tab>]`, drawn over the declared run
  (`pipeline_runs`) and today's build view (`entity_view`).

  - **Header**: the pipeline and number with the result chip; the branch,
    who or what started it, when, and how long it took. *Re-run* (the same
    branch and values, queued at once), *Run again with changes…* (the Run…
    dialog from this run, #852) and *Open in <source>*. A run in flight has
    *Stop*, disabled: it needs a new write. A run the source's credential
    cannot make (`refused_writes`, #2040) draws *Re-run* and both *Run
    again…* disabled with the reason, and one that may not override
    parameters (#2055) draws *Re-run* of a run that carried overrides so.
  - **Overview**: a failed run leads with its first failed test (the
    details as the source wrote them, *Show it in the log*, *Open the test
    file* where the details name a path, *Create ticket* or the ticket
    filed from it (#1451, `TfRunTicket`), and the line saying how the runs
    before it went); a running run says what it is doing; then the *Run*
    panel and the first four *Artifacts*.
  - **Changes**: the commits in the run, each opening its commit.
  - **Parameters**: each value in this run beside the default, the branch,
    and *Run again with these…*.
  - **Log** (#854, `TfRunLog`), **Tests** (#855, `TfRunTests`) and
    **Artifacts** (#856, `TfRunArtifacts`) read the run live (`run_log`,
    `run_tests`, `run_artifacts`). A download, here and on the Overview, goes
    through knobas into the downloads folder (`download_run_file`, D8).

  - **Steps bar** (#1003, R-9.10), under the header on every tab: one
    segment per step of the run's log (`run_log`'s steps, #854), as wide as
    its time, toned by result, the duration as its tip. A click opens the
    Log tab at that step. The page reads the log once and hands that read to
    the Log tab; a run in flight is read again when the Log tab opens. No
    log, or a log without steps, draws no bar.

  - **In flight** (the TeamCity run fix, 2026-11-25, `runs-in-flight.ts`): a refresh with no write that moved the run
    reads its row and item again, and the whole page when it went from queued to running or to finished; the run is
    refreshed at the source once it drew, when shown again and when the window comes back, at most once in 5 s.

  - **Refresh** (#2137, after #2107): the run as `get_entity` reads it draws
    *Refresh* or *Sync now* at the end of the header's actions
    (`TfDetailRefresh`), and the withdrawn and turned-off banners under the
    header (`TfDetailBanners`). A refresh reads the run again as a finished
    sync does, over what the page drew. The Builds view's *Sync now*
    (`sync`, #2287) comes after it, last, unless the detail's
    button is itself a *Sync now* (its source reads no one item).
-->
<script lang="ts">
  import TfStateChip from "../TfStateChip.svelte";
  import { clockLabel, secondsLabel } from "../../time/format";
  import { onDestroy, onMount, untrack, type Snippet } from "svelte";

  import { ipcErrorMessage } from "../../ipc";
  import type { EntityDetail, PipelineRow, PipelineSettings, RunLog, RunRow, ShownView, SourceView } from "../../ipc/entity";
  import TfHeldNote from "../TfHeldNote.svelte";
  import { onViewFresh } from "../repo/view-fresh.svelte";
  import type { SourceSummary, SourceSyncStatus } from "../../ipc/sources";
  import { toastQueued } from "../../shell/queued-toast";
  import { askInto, refusalOf, refusedAttrs, type RefusedWrite } from "../../shell/refused-writes";
  import { push } from "../../shell/toasts.svelte";
  import { acknowledged, openingWords } from "../../shell/ack.svelte";
  import { webPlaceName } from "../../shell/open-web";
  import { settleEchoes } from "../queued.svelte";
  import { createDetailRefresh } from "../refresh.svelte";
  import { heldHead } from "../held-head.svelte";
  import TfDetailBanners from "../TfDetailBanners.svelte";
  import TfNothingHere from "../TfNothingHere.svelte";
  import { jumpAcross } from "../opener";
  import { parentPage } from "../page-name.svelte";
  import TfHeaderRefresh from "../TfHeaderRefresh.svelte";
  import { commitHash, fileHash } from "../repo/repo";
  import { fitLabel } from "../time/day-view";
  import { RunHold } from "./run-hold.svelte";
  import { failedRun, pipelineHash, runTone, stateWord } from "./builds";
  import { runsRead } from "./runs-read.svelte";
  import { pageVisible } from "../kept-pages.svelte";
  import { onFocusWhileShown } from "../while-shown.svelte";
  import { IN_FLIGHT_AFTER_MS, InFlightReads, hearRuns, inFlight as runInFlight, phase } from "./runs-in-flight";
  import { inTextField } from "../../shell/keys";
  import {
    chipTone,
    defaultBranch,
    jobBranchFor,
    echoRun,
    pipelineSource,
    queuedRunWhat,
    resultWord,
    runBranch,
    runLanded,
    staleLine,
    syncWith,
    triggerLabel,
    RUN_OP,
    runFieldAsks,
    runFieldRefusals,
  } from "./pipeline";
  import { RUN_BRIDGE, listenSynced, type RunPorts } from "./pipeline-ports";
  import type { RunRequest } from "./run-dialog";
  import TfRunChain from "./TfRunChain.svelte";
  import TfRunArtifacts from "./TfRunArtifacts.svelte";
  import TfRunDialog from "./TfRunDialog.svelte";
  import TfRunLog from "./TfRunLog.svelte";
  import TfRunTests from "./TfRunTests.svelte";
  import TfRunTicket from "./TfRunTicket.svelte";
  import { downloadAndSay, fitSteps, isCommand, stepSegments } from "./run-reads";
  import { fonts } from "../fonts.svelte";
  import {
    RUN_TABS,
    noSuchRunTab,
    artifactsOf,
    builtCommit,
    repositoryOfCommit,
    changesOf,
    contextLine,
    failureHeading,
    firstFailure,
    parameterRows,
    previousOnBranch,
    rerunRequest,
    runTabHash,
    runTabOf,
    sectionOf,
    ticketDraftOf,
    tookLine,
    type FirstFailure,
  } from "./run";

  let {
    pipeline,
    rest,
    job,
    runs: readRuns,
    crumbs,
    sync: syncNow,
    ports = RUN_BRIDGE,
  }: {
    pipeline: PipelineRow;
    /** What follows `build` in the address: the pipeline, the run, then the tab. */
    rest: string[];
    job: string | null;
    /** The mirrored runs, newest first (`pipeline_runs`). */
    runs: (pipelineId: string) => Promise<RunRow[]>;
    /** The breadcrumb, drawn by the Builds page. */
    crumbs: Snippet<[{ name: string; href?: string }[]]>;
    /** The Builds view's *Sync now* (#2106), last in the header (#2287). */
    sync?: Snippet | undefined;
    ports?: RunPorts;
  } = $props();

  const pipelineId = $derived(pipeline.entity_id);
  const runId = $derived(rest[1] ?? "");
  const tab = $derived(runTabOf(rest));
  const noTab = $derived(noSuchRunTab(rest));

  /** How many older pages a run not among the latest is looked for in. */
  const OLDER_PAGES = 5;

  let held = $state<RunRow[] | null>(null);
  let runsFailed = $state<string | null>(null);
  let view = $state<ShownView | SourceView | null>(null);
  /** Why the source did not answer the read behind a view drawn from what knobas held (ADR-0042). */
  let heldFailed = $state<string | null>(null);
  let viewRead = $state(false);
  let viewFailed = $state<string | null>(null);
  let failure = $state<FirstFailure | null>(null);
  let settings = $state<PipelineSettings | null>(null);
  let sources = $state<SourceSummary[]>([]);
  let sync = $state<SourceSyncStatus[]>([]);
  let jobBranches = $state<string[]>([]);
  let log = $state<RunLog | null>(null);
  /** The run's one log read: the steps bar draws over it, and the Log tab is handed it. */
  let logRead: Promise<RunLog | null> | null = null;
  let logSettled = false;
  /** Whether the run's log read has answered: the steps bar's room says what it found, once it has. Apart from `logSettled`, which the Log tab's port reads inside its effect. */
  let logAnswered = $state(false);
  /** The step a click on the bar opens the Log tab at; `seq` tells two clicks on one step apart. */
  let openStep = $state<{ step: number; seq: number } | null>(null);
  let openSeq = 0;
  let token = 0;
  /** How many times the page has read the run again after a sync (#1450): the open tab reads again on each. */
  let rereads = $state(0);
  /** The run as `get_entity` reads it (#2137): what its *Refresh* and banners draw over; `null` until read. */
  let detail = $state<EntityDetail | null>(null);
  /** Whether `get_entity` has answered for the run, found or not: the header's *Refresh* or *Sync now* waits on it. */
  let detailRead = $state(false);
  /** The header's *Refresh* or *Sync now* (#2107). */
  const refresh = createDetailRefresh(untrack(() => ports.refresh));

  $effect(() => {
    const wanted = { pipelineId, runId };
    untrack(() => open(wanted.pipelineId, wanted.runId));
  });
  /**
   * The header draws once, whole (`held-head.svelte.ts`): *Re-run* is the view's and *Refresh* or *Sync now* the item's, which
   * landed after the header and pushed *Re-run* and *Open in TeamCity* 105 px left. The tabs under it wait with it: their counts are the view's.
   * So does the rest of the page (`tf-heldbelow` on each part under it): *Refresh run* wraps the actions under the title at 1440 px, and the header
   * growing a line while it was held pushed the panels 29 px down.
   */
  const drawnHead = heldHead(
    () => runId,
    () => detailRead && viewRead,
  );

  /**
   * What the source's credential cannot write on the run's pipeline (#2040), and the run's fields
   * that need more than the run (#2055), asked once per pipeline.
   */
  let refused = $state<RefusedWrite[]>([]);
  $effect(() => {
    refused = [];
    const asks = [{ entity: pipelineId, op: RUN_OP }, ...runFieldAsks(pipelineId)];
    return askInto(asks, ports.refusedWrites, (answer) => (refused = answer));
  });
  const cannotRun = $derived(refusalOf(refused, pipelineId, RUN_OP));
  const fieldRefusals = $derived(runFieldRefusals(refused, pipelineId));
  function runAgain() {
    if (cannotRun === null) dialog = true;
  }

  function open(pipe: string, id: string) {
    const mine = ++token;
    held = null;
    runsFailed = null;
    view = null;
    viewRead = false;
    viewFailed = null;
    failure = null;
    settings = null;
    log = null;
    logAnswered = false;
    detail = null;
    detailRead = false;
    refresh.reset();
    clearTimeout(inFlightSoon);
    inFlightReads.reset();
    void findRun(pipe, id, mine).then(() => {
      // Once the run has drawn, one in flight is read at the source: started or finished since the last run of the
      // source, it says so now (the TeamCity run fix, 2026-11-25). A page left meanwhile sets nothing.
      if (mine !== token) return;
      inFlightSoon = setTimeout(() => {
        if (mine === token) readInFlight();
      }, IN_FLIGHT_AFTER_MS);
    });
    void readLog(id, mine);
    void readView(id, mine);
    void readDetail(id, mine);
    readSettings(pipe, mine);
  }

  /** The run's item (#2137); a failed read keeps the last one, and none draws no *Refresh*. */
  function readDetail(id: string, mine: number): Promise<void> {
    return ports.getEntity(id).then(
      (answer) => {
        if (mine !== token) return;
        detail = answer;
        detailRead = true;
      },
      () => {
        if (mine === token) detailRead = true;
      },
    );
  }

  /** The run's view, and its first failed test where the view counts failures; a failed read keeps the last view. */
  function readView(id: string, mine: number): Promise<void> {
    return ports.view(id).then(
      (answer) => {
        if (mine !== token) return;
        view = answer;
        viewRead = true;
        viewFailed = null;
        heldFailed = null;
        if ((sectionOf(answer, "tests")?.count ?? 0) > 0) readFailure(id, mine);
        else failure = null;
      },
      (cause: unknown) => {
        if (mine !== token) return;
        viewFailed = ipcErrorMessage(cause);
        viewRead = true;
      },
    );
  }

  // The run's view knobas held, read again behind it (ADR-0042).
  onViewFresh(
    () => runId,
    null,
    () => {
      const id = runId;
      if (view !== null) void readView(id, token);
    },
    untrack(() => ports.listen),
    (why) => (heldFailed = why),
  );

  // The pipeline's settings knobas held, read again behind them (ADR-0042).
  onViewFresh(
    () => pipelineId,
    "settings",
    () => readSettings(pipelineId, token),
    untrack(() => ports.listen),
  );

  function readSettings(pipe: string, mine: number) {
    ports.settings(pipe).then(
      (answer) => {
        if (mine === token) settings = answer;
      },
      () => {},
    );
  }

  /** The run among the pipeline's runs, older pages asked for while it is not there. */
  async function findRun(pipe: string, id: string, mine: number) {
    try {
      let rows = await readRuns(pipe);
      for (let page = 0; page < OLDER_PAGES && rows.length > 0 && !rows.some((r) => r.entity_id === id); page += 1) {
        const older = await ports.older(pipe, rows.at(-1)!.entity_id);
        if (older.length === 0) break;
        rows = [...rows, ...older];
      }
      if (mine !== token) return;
      settleEchoes(pipe, runLanded(rows));
      held = rows;
      runsFailed = null;
    } catch (cause) {
      if (mine === token) runsFailed = ipcErrorMessage(cause);
    }
  }

  /**
   * A sync of the run's source finished (#1450), or a *Refresh* read the run
   * (#2137): the run, its item, its view, its log and the open tab again,
   * over what is drawn. The tab, its filter and an open dialog stay; a failed
   * read keeps the last answer and says so. Settles once the page's own
   * reads have answered.
   */
  function reread(pipe: string, id: string): Promise<unknown> {
    // A new token: a read again answering after a later one is dropped.
    const mine = ++token;
    const reads = [findRun(pipe, id, mine), readLog(id, mine), readView(id, mine), readDetail(id, mine)];
    // The first settings read, if still out, answers under the old token.
    if (settings === null) readSettings(pipe, mine);
    rereads += 1;
    return Promise.allSettled(reads);
  }

  /** The *Refresh*'s read again, of the run the page is on. */
  const rereadRun = async () => void (await reread(pipelineId, runId));

  $effect(() => {
    const wanted = { pipelineId, runId };
    return listenSynced(
      ports,
      pipeline.source_id,
      (row) => (sync = syncWith(sync, row)),
      () => reread(wanted.pipelineId, wanted.runId),
    );
  });

  function readLog(id: string, mine: number): Promise<RunLog | null> {
    const read = ports.log(id);
    logRead = read;
    logSettled = false;
    read.then(
      (answer) => {
        if (mine !== token) return;
        log = answer;
        logSettled = true;
        logAnswered = true;
      },
      () => {
        if (mine !== token) return;
        logSettled = true;
        logAnswered = true;
      },
    );
    return read;
  }

  /**
   * The Log tab's reads: the page's own log read, or a new one for a run in
   * flight whose read has come back (its log has grown since).
   */
  const logPorts: Pick<RunPorts, "log" | "download"> = {
    log: (id) => (logRead !== null && id === runId && !(inFlight && logSettled) ? logRead : readLog(id, token)),
    download: (id, target) => ports.download(id, target),
  };

  // A step asked for is the Log tab's only while the reader stays on it.
  $effect(() => {
    if (tab !== "log") openStep = null;
  });

  function readFailure(id: string, mine: number) {
    ports.view(id, "tests").then(
      (answer) => {
        if (mine === token) failure = firstFailure(sectionOf(answer, "tests"));
      },
      () => {},
    );
  }

  onMount(() => {
    void ports.sources().then((rows) => (sources = rows), () => {});
    void ports.syncStatus().then((rows) => (sync = rows), () => {});
  });

  $effect(() => {
    const wanted = job;
    jobBranches = [];
    if (wanted === null) return;
    ports.jobBranches(wanted).then(
      (names) => {
        if (job === wanted) jobBranches = names;
      },
      () => {},
    );
  });

  onDestroy(() => {
    token += 1;
    clearTimeout(inFlightSoon);
  });

  const run = $derived(held?.find((r) => r.entity_id === runId) ?? null);

  /** The run, refreshed by id when the page is shown while it is in flight (the TeamCity run fix). */
  const inFlightReads = new InFlightReads(untrack(() => ports.follow));
  let inFlightSoon: ReturnType<typeof setTimeout> | undefined;
  function readInFlight() {
    const now = held?.find((r) => r.entity_id === runId) ?? null;
    if (now !== null && runInFlight(now)) inFlightReads.read([now.entity_id]);
  }

  // The run moved at the source (a run started in knobas is read to its end, or this page read it): its row and item
  // again from the mirror, and, where it went from queued to running or to finished, the whole page as a sync's end
  // reads it (its view, log and open tab).
  $effect(() => {
    const wanted = { pipelineId, runId };
    return untrack(() =>
      hearRuns(
        ports.listen,
        (entity) => entity === wanted.runId,
        () => {
          const mine = token;
          const was = phase(held?.find((r) => r.entity_id === wanted.runId) ?? null);
          void Promise.all([findRun(wanted.pipelineId, wanted.runId, mine), readDetail(wanted.runId, mine)]).then(() => {
            if (mine !== token) return;
            const is = phase(held?.find((r) => r.entity_id === wanted.runId) ?? null);
            if (is !== was) void reread(wanted.pipelineId, wanted.runId);
          });
        },
      ),
    );
  });

  /**
   * `m` on the run page (ux18): *Mark as read* or *Mark as unread* on this run, when it finished failed. Opening it
   * reads nothing: only a mark does. Not in a field, a dialog, or a page a keep-alive holds hidden.
   */
  const runShown = pageVisible();
  // Shown again or the window back: a run in flight read at the source, at most once in 5 s.
  onFocusWhileShown(runShown, readInFlight);
  onMount(() => {
    const onkey = (event: KeyboardEvent) => {
      if (event.key !== "m" || event.metaKey || event.ctrlKey || event.altKey || event.shiftKey || event.defaultPrevented) return;
      if (!runShown() || run === null || !failedRun(run)) return;
      const at = document.activeElement;
      if (at instanceof HTMLElement && (inTextField(at) || at.closest(".tf-dialog, select, [role='menu']") !== null)) return;
      event.preventDefault();
      void runsRead.toggle(run);
    };
    window.addEventListener("keydown", onkey);
    return () => window.removeEventListener("keydown", onkey);
  });
  const previous = $derived(run === null || held === null ? null : previousOnBranch(held, run));
  const mainBranch = $derived(defaultBranch(settings));
  const startedBy = $derived(run === null ? null : triggerLabel(run) || null);
  const agent = $derived(run?.agent ?? null);
  const now = $derived(run?.current_step ?? null);
  const commit = $derived(run === null ? null : builtCommit(run));
  const firstRepoId = $derived(settings?.repositories.find((repo) => repo.entity_id !== null)?.entity_id ?? null);
  /** The repository the run's commit sits in; the pipeline's first mirrored one where the commit names none (#1154). */
  const commitRepoId = $derived(repositoryOfCommit(commit?.entityId ?? null, settings?.repositories));
  const repoId = $derived(commitRepoId ?? firstRepoId);
  /** A change's own repository where its id names one, else the run's. */
  const repoOfChange = (changeId: string | null): string | null =>
    repositoryOfCommit(changeId, settings?.repositories) ?? repoId;
  const artifacts = $derived(artifactsOf(view));
  const changes = $derived(changesOf(view));
  const parameters = $derived(parameterRows(view, settings));
  const testsCount = $derived(sectionOf(view, "tests")?.count ?? 0);
  const context = $derived(run === null || held === null ? "" : contextLine(held, run, mainBranch));
  const failed = $derived(run !== null && runTone(run) === "err");
  const inFlight = $derived(run !== null && run.finished === null);
  const stale = $derived(staleLine(pipeline, sources, sync));
  const jobBranch = $derived(held === null ? null : jobBranchFor(held, jobBranches));
  const read = $derived(stepSegments(log, inFlight, settings?.steps ?? []));

  /** The header's chip: the run's own word, *running* or *queued* while in flight. */
  /** A step's name at what its segment holds: whole, or ended with an ellipsis, never cut mid-letter. */
  let barPx = $state(0);
  let pen: CanvasRenderingContext2D | null = null;
  function measureBar(bar: HTMLElement) {
    // Read now, before the bar first paints: the observer's first answer came a frame late, and the steps fitted to no width
    // drew once and then moved by a few pixels (the nothing-moves pass).
    barPx = bar.clientWidth;
    if (typeof ResizeObserver === "undefined") return;
    const seen = new ResizeObserver(() => (barPx = bar.clientWidth));
    seen.observe(bar);
    return () => seen.disconnect();
  }
  /**
   * A name's width as `.tf-stepsbar text` draws it: 12 px, or 11 px of code, times the role's size (Settings › Fonts,
   * read here so the bar is fitted again when a size or a face changes); 0 with no canvas to measure on. A step's name
   * is the interface's text, so its weight and letter spacing are the interface's too (2026-11-21): the tracking comes
   * after every character, as the browser draws it. A command keeps its face's own spacing.
   */
  function nameWidth(name: string, text = name): number {
    const command = isCommand(name);
    const role = command ? "code" : "ui";
    const px = ((command ? 11 : 12) * fonts.size(role)) / 100;
    void fonts.family(role);
    const weight = command ? 400 : fonts.knob("ui", "wt");
    const track = command ? 0 : (fonts.knob("ui", "ls") / 100) * px;
    pen ??= document.createElement("canvas").getContext("2d");
    if (pen === null) return 0;
    const face = command ? "--mono" : "--sans";
    pen.font = `${weight} ${px}px ${getComputedStyle(document.documentElement).getPropertyValue(face)}`;
    return pen.measureText(text).width + [...text].length * track;
  }
  /** What a label's inset takes from its segment: 10 px before it, 8 px after. */
  const STEP_INSET = 18;
  /** A px over its measure, so percentages rounding on the way back cannot cost a name its end. */
  const segments = $derived(fitSteps(read, barPx, (name) => nameWidth(name) + STEP_INSET + 1));
  function stepLabel(name: string, widthPct: number): string | null {
    if (barPx === 0) return name;
    return fitLabel(name, (widthPct / 100) * barPx - STEP_INSET, (text) => nameWidth(name, text));
  }

  function chipWord(r: RunRow): string {
    return stateWord(r.finished === null ? resultWord(r) : (r.state?.text ?? ""));
  }

  // -- the writes ---------------------------------------------------------------

  /**
   * *Re-run* and *Run again…*: queued at once, and the toast says so (R-9.8).
   * The page draws no queued row of its own (#853); the pipeline page draws
   * it at the top of its runs until a read holds it (#1449).
   */
  async function queue(request: RunRequest): Promise<number | null> {
    let id: number;
    try {
      const write = await ports.submitWrite(request.payload);
      id = write.id;
      echoRun(pipeline, write, runBranch(request.payload), held, ports.follower());
      // Once sent, the way to the run it started: the page's own runs, where it is drawn (F-338, F-350).
      toastQueued(queuedRunWhat(pipeline, request.summary), pipelineSource(pipeline), { write, where: { label: "Show in runs", run: () => (location.hash = pipelineHash(pipelineId)) } });
    } catch (cause) {
      push({ text: `Could not queue the run: ${ipcErrorMessage(cause)}`, tone: "err" });
      return null;
    }
    if (request.preset === null) return id;
    try {
      await ports.savePreset(request.preset);
    } catch (cause) {
      push({ text: `Could not save the preset: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
    return id;
  }

  /**
   * What *Re-run* sends: this run's settings, its overrides among them. A credential that may not
   * send overrides may not send those (#2055), so it is drawn disabled saying why, unless the run
   * ran on the defaults alone.
   */
  const rerunning = $derived(run !== null && view !== null ? rerunRequest(pipelineId, run, settings, view) : null);
  const cannotRerun = $derived(
    cannotRun ??
      (rerunning !== null && "RunBuild" in rerunning.payload && rerunning.payload.RunBuild.parameters.length > 0
        ? fieldRefusals.cannotOverride
        : null),
  );
  /** *Re-run* holds as what its write is doing for about two seconds, so a second press can't queue a second run (F-338). */
  const rerunHold = new RunHold(() => pipelineId);
  function rerun() {
    if (rerunning === null || cannotRerun !== null) return;
    const request = rerunning;
    void rerunHold.press(() => queue(request));
  }

  let dialog = $state(false);

  function onrun(request: RunRequest) {
    dialog = false;
    void queue(request);
  }

  // A run sits under its pipeline, by the name its crumb gives it.
  parentPage(() => ({ hash: pipelineHash(pipelineId), name: pipeline.name }));

  /** A top-level artifact file into the downloads folder (D8, #856). */
  const downloadFile = (name: string) => downloadAndSay(ports.download, runId, { kind: "file", path: name }, name);

  function openExternal(url: string | null) {
    if (url === null) return;
    void acknowledged(openingWords(webPlaceName(url)), () => ports.openExternal(url)).catch((cause: unknown) => {
      push({ text: `Could not open ${pipeline.source_name}: ${ipcErrorMessage(cause)}`, tone: "err" });
    });
  }

</script>

{@render crumbs([
  { name: "Builds" },
  ...(pipeline.group !== null ? [{ name: pipeline.group }] : []),
  { name: pipeline.name, href: pipelineHash(pipelineId) },
  { name: run?.number != null ? `#${run.number}` : "Run" },
])}

{#if runsFailed !== null && held === null}
  <TfNothingHere message="This pipeline's runs could not be read: {runsFailed}" />
{:else if held === null}
  <p class="tf-dim tf-runs">Reading the run…</p>
{:else if run === null}
  <TfNothingHere message="knobas does not hold this run. It may not have synced yet." href={pipelineHash(pipelineId)} action="All runs of {pipeline.name}" />
{:else}
  {@const at = run.started ?? run.updated_at}
  <header class="tf-fhead tf-bhead tf-holds" class:tf-unread={!drawnHead.shown}>
    <div>
      <h1 data-failed={failedRun(run) ? "" : undefined}>
        {pipeline.name} #{run.number ?? ""}
        {#if run.state !== null}<TfStateChip text={chipWord(run)} class={`tf-run-chip ${chipTone(run)}`} />{/if}
      </h1>
      <div class="tf-sub">
        {#if run.branch !== null}<span class="tf-mono">{run.branch}</span>{/if}{startedBy !== null ? `, started by ${startedBy}` : ""}{at !== null
          ? `, ${clockLabel(at)}`
          : ""}{run.duration_seconds !== null ? `, took ${secondsLabel(run.duration_seconds)}` : ""}
      </div>
    </div>
    <div class="tf-acts">
      {#if !inFlight}
        <button
          type="button"
          class="tf-btn tf-primary"
          disabled={view === null}
          aria-disabled={cannotRerun !== null || rerunHold.word !== null ? "true" : undefined}
          title={cannotRerun ?? (view === null && viewRead ? "The run's settings could not be read" : undefined)}
          onclick={rerun}><span class="tf-keepw" data-keep="Queued"><span>{rerunHold.word ?? "Re-run"}</span></span></button
        >
        <button
          type="button"
          class="tf-btn"
          {...refusedAttrs(cannotRun)}
          onclick={runAgain}><span>Run again<span class="tf-wide">{" "}with changes</span>…</span></button
        >
      {:else}
        <button type="button" class="tf-btn" aria-disabled="true" title="Needs a new write">Stop</button>
      {/if}
      {#if failedRun(run)}
        {@const unread = runsRead.unread(run)}
        <button type="button" class="tf-btn tf-ghost" title={`${unread ? "Mark as read" : "Mark as unread"} (M)`} onclick={() => void runsRead.toggle(run)}
          >{unread ? "Mark as read" : "Mark as unread"}</button
        >
      {/if}
      {#if run.web_url !== null}
        <button type="button" class="tf-btn tf-ghost" aria-label="Open in {pipeline.source_name}" title="Open in {pipeline.source_name}" onclick={() => openExternal(run.web_url)}><span class="tf-wide tf-cut">Open in {pipeline.source_name}</span><svg class="tf-narrow" width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M6.5 3.5H3.5v9h9v-3M9 3h4v4M13 3 7.5 8.5" /></svg></button>
      {/if}
      <TfHeaderRefresh {detail} {refresh} reread={rereadRun} sync={syncNow} noun="run" />
    </div>
  </header>
  {#if detail !== null}<TfDetailBanners {detail} {refresh} reread={rereadRun} held={!drawnHead.shown} />{/if}

  {#if segments.length > 0}
    <div class="tf-stepsbar" class:tf-heldbelow={!drawnHead.shown} {@attach measureBar}>
      <svg role="group" aria-label="Steps" width="100%" height="30">
        {#each segments as segment (segment.index)}
          <a
            href={runTabHash(pipelineId, runId, "log")}
            aria-label={segment.tip}
            onclick={() => (openStep = segment.state === "wait" ? null : { step: segment.index, seq: ++openSeq })}
          >
            <title>{segment.tip}</title>
            <svg x={`${segment.x}%`} width={`${segment.width}%`} height="100%" aria-hidden="true">
              <rect class={`tf-sbseg tf-${segment.state}`} width="100%" height="100%" />
              {#if segment.index > 0}<rect class="tf-sbgap" width="2" height="100%" />{/if}
              {#if stepLabel(segment.name, segment.width) !== null}<text class:tf-sbcmd={isCommand(segment.name)} x="10" y="19">{stepLabel(segment.name, segment.width)}</text>{/if}
            </svg>
          </a>
        {/each}
      </svg>
    </div>
  {:else}
    <!-- The bar's room from the first frame (the nothing-moves pass, 2026-11-17): the log is read live from the source, and the bar
         landing with it pushed the run's panels 29 px down. An empty track while it reads, then what it found. -->
    <div class="tf-stepsbar tf-sbnone" class:tf-heldbelow={!drawnHead.shown}>{#if logAnswered}<span>{log === null ? "No log" : "No steps in the log"}</span>{/if}</div>
  {/if}

  {#if stale !== null}
    <div class="tf-stale tf-bstale" class:tf-heldbelow={!drawnHead.shown}>
      {stale.text}
      {#if stale.fix !== null}<a href={stale.fix.href}>{stale.fix.words}</a>{/if}
    </div>
  {/if}
  {#if runsFailed !== null}
    <p class="tf-dim tf-runs" class:tf-heldbelow={!drawnHead.shown}>Runs could not be read: {runsFailed}</p>
  {/if}

  <nav class="tf-tabs" class:tf-unread={!drawnHead.shown} aria-label="Run">
    {#each RUN_TABS as entry (entry.id)}
      <a href={runTabHash(pipelineId, runId, entry.id)} aria-current={entry.id === tab && !noTab ? "page" : undefined}
        >{entry.name}{#if entry.id === "tests" && testsCount > 0}{" "}<span class="tf-count tf-err">{testsCount} failed</span
          >{:else if entry.id === "artifacts" && artifacts.total > 0}{" "}<span class="tf-count">{artifacts.total}</span>{/if}</a
      >
    {/each}
  </nav>

  <div class="tf-runbody" class:tf-heldbelow={!drawnHead.shown}>
    {#if view !== null && !noTab && (tab === "overview" || tab === "changes" || tab === "params")}<TfHeldNote heldAt={"held_at" in view ? view.held_at : null} source={pipeline.source_name} failed={heldFailed} />{/if}
    {#if viewFailed !== null && !noTab && (tab === "overview" || tab === "changes" || tab === "params")}
      <p class="tf-dim">The run could not be read from {pipeline.source_name}: {viewFailed}</p>
    {/if}
    {#if noTab}
      <TfNothingHere message="A run has no such tab." href={runTabHash(pipelineId, runId, "overview")} action="Overview" />
    {:else if tab === "overview"}
      <div class="tf-runov">
        {#if failed && failure !== null}
          <div class="tf-problem">
            <h3>{failureHeading(failure)}</h3>
            {#if failure.details !== null}<pre>{failure.details}</pre>{/if}
            <div class="tf-acts">
              <a class="tf-btn tf-sm" href={runTabHash(pipelineId, runId, "log")} onclick={(event) => jumpAcross(event, "Overview")}>Show in log</a>
              {#if failure.location !== null && repoId !== null}
                <a class="tf-btn tf-sm tf-ghost" href={fileHash(repoId, failure.location.path)}>Open the test file</a>
              {/if}
              <TfRunTicket
                {runId}
                href={runTabHash(pipelineId, runId, "overview")}
                draft={ticketDraftOf(failure, pipeline.name, run)}
                {sources}
                {ports}
                {rereads}
              />
            </div>
            {#if context !== ""}<span class="tf-dim">{context}</span>{/if}
          </div>
        {/if}
        {#if inFlight && now !== null}
          <div class="tf-callout tf-info">{now}</div>
        {/if}
        <div class="tf-grid">
          <section class="tf-panel tf-headed" aria-label="Run">
            <header><h3>Run</h3></header>
            <dl class="tf-facts tf-runfacts">
              <dt>Branch</dt>
              <dd class="tf-mono">{run.branch ?? ""}</dd>
              <dt>Commit</dt>
              <dd class="tf-mono">
                {#if commit !== null && commit.entityId !== null && repoId !== null}<a href={commitHash(repoId, commit.entityId)}
                    >{commit.sha}</a
                  >{:else if commit !== null}{commit.sha}{:else}<span class="tf-dim">none</span>{/if}
              </dd>
              <dt>Started by</dt>
              <dd>{startedBy ?? ""}</dd>
              <dt>Agent</dt>
              <dd class="tf-mono">{agent ?? ""}</dd>
              <dt>Started</dt>
              <dd>{run.started !== null ? clockLabel(run.started) : "not yet"}</dd>
              <dt>Took</dt>
              <dd>{tookLine(run, previous)}</dd>
            </dl>
          </section>
          <TfRunChain {runId} branch={run.branch} {rereads} />
          <section class="tf-panel tf-headed" aria-label="Artifacts">
            <header>
              <h3>Artifacts</h3>
              {#if artifacts.total > 0}<a class="tf-open" href={runTabHash(pipelineId, runId, "artifacts")}>All artifacts</a>{/if}
            </header>
            <div class="tf-rows">
              {#each artifacts.first as artifact, i (i)}
                <div class="tf-r">
                  <span class="tf-t tf-mono">{artifact.title}</span>
                  <span class="tf-rend">
                    {#if artifact.meta !== null}<span class="tf-dim tf-rowval">{artifact.meta}</span>{/if}
                    {#if artifact.url !== null}
                      <span class="tf-rowend"><span class="tf-rowacts"><button type="button" class="tf-btn tf-sm" onclick={() => downloadFile(artifact.title)}>Download</button></span></span>
                    {/if}
                  </span>
                </div>
              {:else}
                <div class="tf-r tf-dim">{viewRead ? "None yet." : "…"}</div>
              {/each}
            </div>
          </section>
        </div>
      </div>
    {:else if tab === "changes"}
      {#if viewRead && viewFailed === null && changes.length === 0}
        <div class="tf-empty"><p>No changes in this run.</p></div>
      {:else if changes.length > 0}
        <table class="tf-tbl" aria-label="Changes">
          <tbody>
            {#each changes as change, i (i)}
              <tr>
                <td class="tf-mono tf-nowrap"
                  >{#if change.entity_id !== null && repoOfChange(change.entity_id) !== null}<a href={commitHash(repoOfChange(change.entity_id)!, change.entity_id)}>{change.chip ?? ""}</a
                    >{:else}{change.chip ?? ""}{/if}</td
                >
                <td>{change.title}</td>
                <td class="tf-dim">{change.meta ?? ""}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
    {:else if tab === "params"}
      {#if viewRead && viewFailed === null}
        <table class="tf-tbl" aria-label="Parameters">
          <thead><tr><th>Parameter</th><th>Value in this run</th><th>Default</th></tr></thead>
          <tbody>
            {#each parameters as parameter (parameter.name)}
              <tr>
                <td class="tf-mono">{parameter.name}</td>
                <td class="tf-mono">{#if parameter.secret}<span class="tf-dim">(hidden)</span>{:else}{parameter.value || "(empty)"}{/if}</td>
                <td class="tf-mono tf-dim">{parameter.default === null ? "" : parameter.default || "(empty)"}</td>
              </tr>
            {/each}
            <tr>
              <td class="tf-mono">branch</td>
              <td class="tf-mono">{run.branch ?? ""}</td>
              <td class="tf-mono tf-dim">{mainBranch ?? ""}</td>
            </tr>
          </tbody>
        </table>
        {#if !inFlight}
          <button
            type="button"
            class="tf-btn tf-sm tf-runagain"
            {...refusedAttrs(cannotRun)}
            onclick={runAgain}>Run again with these…</button
          >
        {/if}
      {/if}
    {:else if tab === "log"}
      <TfRunLog {runId} {inFlight} {failed} ports={logPorts} {openStep} {rereads} />
    {:else if tab === "tests"}
      <TfRunTests {runId} branch={run.branch} logHref={runTabHash(pipelineId, runId, "log")} {ports} {rereads} />
    {:else if tab === "artifacts"}
      <TfRunArtifacts {runId} sourceName={pipeline.source_name} {ports} {rereads} />
    {/if}
  </div>

  {#if dialog}
    <TfRunDialog
      {pipeline}
      {settings}
      runs={held}
      from={run}
      {jobBranch}
      secretNoun={stale?.fix?.secret ?? null}
      {...fieldRefusals}
      {ports}
      onclose={() => (dialog = false)}
      {onrun}
    />
  {/if}
{/if}

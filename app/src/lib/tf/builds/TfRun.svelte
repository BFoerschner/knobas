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
-->
<script lang="ts">
  import { clockLabel, secondsLabel } from "../../time/format";
  import { onDestroy, onMount, untrack, type Snippet } from "svelte";

  import { ipcErrorMessage } from "../../ipc";
  import type { PipelineRow, PipelineSettings, RunLog, RunRow, SourceView } from "../../ipc/entity";
  import type { SourceSummary, SourceSyncStatus } from "../../ipc/sources";
  import { toastQueued } from "../../shell/queued-toast";
  import { askInto, refusalOf, refusedAttrs, type RefusedWrite } from "../../shell/refused-writes";
  import { push } from "../../shell/toasts.svelte";
  import { settleQueued } from "../queued.svelte";
  import { commitHash, fileHash } from "../repo/repo";
  import { pipelineHash, runTone } from "./builds";
  import {
    chipTone,
    defaultBranch,
    jobBranchFor,
    markQueuedRun,
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
  import { downloadAndSay, stepSegments } from "./run-reads";
  import {
    RUN_TABS,
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
    ports?: RunPorts;
  } = $props();

  const pipelineId = $derived(pipeline.entity_id);
  const runId = $derived(rest[1] ?? "");
  const tab = $derived(runTabOf(rest));

  /** How many older pages a run not among the latest is looked for in. */
  const OLDER_PAGES = 5;

  let held = $state<RunRow[] | null>(null);
  let runsFailed = $state<string | null>(null);
  let view = $state<SourceView | null>(null);
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
  /** The step a click on the bar opens the Log tab at; `seq` tells two clicks on one step apart. */
  let openStep = $state<{ step: number; seq: number } | null>(null);
  let openSeq = 0;
  let token = 0;
  /** How many times the page has read the run again after a sync (#1450): the open tab reads again on each. */
  let rereads = $state(0);

  $effect(() => {
    const wanted = { pipelineId, runId };
    untrack(() => open(wanted.pipelineId, wanted.runId));
  });

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
    void findRun(pipe, id, mine);
    void readLog(id, mine);
    readView(id, mine);
    readSettings(pipe, mine);
  }

  /** The run's view, and its first failed test where the view counts failures; a failed read keeps the last view. */
  function readView(id: string, mine: number) {
    ports.view(id).then(
      (answer) => {
        if (mine !== token) return;
        view = answer;
        viewRead = true;
        viewFailed = null;
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
      settleQueued(pipe, runLanded(rows));
      held = rows;
      runsFailed = null;
    } catch (cause) {
      if (mine === token) runsFailed = ipcErrorMessage(cause);
    }
  }

  /**
   * A sync of the run's source finished (#1450): the run, its view, its log
   * and the open tab again, over what is drawn. The tab, its filter and an
   * open dialog stay; a failed read keeps the last answer and says so.
   */
  function reread(pipe: string, id: string) {
    // A new token: a read again answering after a later one is dropped.
    const mine = ++token;
    void findRun(pipe, id, mine);
    void readLog(id, mine);
    readView(id, mine);
    // The first settings read, if still out, answers under the old token.
    if (settings === null) readSettings(pipe, mine);
    rereads += 1;
  }

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
      },
      () => {
        if (mine === token) logSettled = true;
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
  });

  const run = $derived(held?.find((r) => r.entity_id === runId) ?? null);
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
  const segments = $derived(stepSegments(log, inFlight, settings?.steps ?? []));

  /** The header's chip: the run's own word, *running* or *queued* while in flight. */
  function chipWord(r: RunRow): string {
    return r.finished === null ? resultWord(r) : (r.state?.text ?? "");
  }

  // -- the writes ---------------------------------------------------------------

  /**
   * *Re-run* and *Run again…*: queued at once, and the toast says so (R-9.8).
   * The page draws no queued row of its own (#853); the pipeline page draws
   * it at the top of its runs until a read holds it (#1449).
   */
  async function queue(request: RunRequest) {
    try {
      const write = await ports.submitWrite(request.payload);
      markQueuedRun(pipeline, write, runBranch(request.payload), held, ports.follower());
      toastQueued(queuedRunWhat(pipeline, request.summary), pipelineSource(pipeline));
    } catch (cause) {
      push({ text: `Could not queue the run: ${ipcErrorMessage(cause)}`, tone: "err" });
      return;
    }
    if (request.preset === null) return;
    try {
      await ports.savePreset(request.preset);
    } catch (cause) {
      push({ text: `Could not save the preset: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
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
  function rerun() {
    if (rerunning === null || cannotRerun !== null) return;
    void queue(rerunning);
  }

  let dialog = $state(false);

  function onrun(request: RunRequest) {
    dialog = false;
    void queue(request);
  }

  /** A top-level artifact file into the downloads folder (D8, #856). */
  const downloadFile = (name: string) => downloadAndSay(ports.download, runId, { kind: "file", path: name }, name);

  function openExternal(url: string | null) {
    if (url === null) return;
    void ports.openExternal(url).catch((cause: unknown) => {
      push({ text: `Could not open ${pipeline.source_name}: ${ipcErrorMessage(cause)}`, tone: "err" });
    });
  }

</script>

{@render crumbs([
  { name: "Builds", href: "#/builds" },
  { name: pipeline.name, href: pipelineHash(pipelineId) },
  { name: run?.number != null ? `#${run.number}` : "Run" },
])}

{#if runsFailed !== null && held === null}
  <div class="tf-empty"><p>Runs could not be read: {runsFailed}</p></div>
{:else if held === null}
  <p class="tf-dim tf-runs">Reading the run…</p>
{:else if run === null}
  <div class="tf-empty">
    <p>No run of {pipeline.name} at this address.</p>
    <a class="tf-btn" href={pipelineHash(pipelineId)}>Back to the pipeline</a>
  </div>
{:else}
  {@const at = run.started ?? run.updated_at}
  <header class="tf-fhead tf-bhead">
    <div>
      <h1>
        {pipeline.name} #{run.number ?? ""}
        {#if run.state !== null}<span class={`tf-chip tf-run-chip ${chipTone(run)}`}>{chipWord(run)}</span>{/if}
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
          aria-disabled={cannotRerun !== null ? "true" : undefined}
          title={cannotRerun ?? (view === null && viewRead ? "The run's settings could not be read" : undefined)}
          onclick={rerun}>Re-run</button
        >
        <button
          type="button"
          class="tf-btn"
          {...refusedAttrs(cannotRun)}
          onclick={runAgain}>Run again with changes…</button
        >
      {:else}
        <button type="button" class="tf-btn" aria-disabled="true" title="Needs a new write">Stop</button>
      {/if}
      {#if run.web_url !== null}
        <button type="button" class="tf-btn tf-ghost" onclick={() => openExternal(run.web_url)}>Open in {pipeline.source_name}</button>
      {/if}
    </div>
  </header>

  {#if segments.length > 0}
    <div class="tf-stepsbar">
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
              <text x="10" y="19">{segment.name}</text>
            </svg>
          </a>
        {/each}
      </svg>
    </div>
  {/if}

  {#if stale !== null}
    <div class="tf-stale tf-bstale">
      {stale.text}
      {#if stale.fix !== null}<a href={stale.fix.href}>{stale.fix.words}</a>{/if}
    </div>
  {/if}
  {#if runsFailed !== null}
    <p class="tf-dim tf-runs">Runs could not be read: {runsFailed}</p>
  {/if}

  <nav class="tf-tabs" aria-label="Run">
    {#each RUN_TABS as entry (entry.id)}
      <a href={runTabHash(pipelineId, runId, entry.id)} aria-current={entry.id === tab ? "page" : undefined}
        >{entry.name}{#if entry.id === "tests" && testsCount > 0}{" "}<span class="tf-count tf-err">{testsCount} failed</span
          >{:else if entry.id === "artifacts" && artifacts.total > 0}{" "}<span class="tf-count">{artifacts.total}</span>{/if}</a
      >
    {/each}
  </nav>

  <div class="tf-runbody">
    {#if viewFailed !== null && (tab === "overview" || tab === "changes" || tab === "params")}
      <p class="tf-dim">The run could not be read from {pipeline.source_name}: {viewFailed}</p>
    {/if}
    {#if tab === "overview"}
      <div class="tf-runov">
        {#if failed && failure !== null}
          <div class="tf-problem">
            <h3>{failureHeading(failure)}</h3>
            {#if failure.details !== null}<pre>{failure.details}</pre>{/if}
            <div class="tf-acts">
              <a class="tf-btn tf-sm" href={runTabHash(pipelineId, runId, "log")}>Show it in the log</a>
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
          <section class="tf-panel" aria-label="Run">
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
          <section class="tf-panel" aria-label="Artifacts">
            <header>
              <h3>Artifacts</h3>
              {#if artifacts.total > 0}<a class="tf-open" href={runTabHash(pipelineId, runId, "artifacts")}>All {artifacts.total}</a>{/if}
            </header>
            <div class="tf-rows">
              {#each artifacts.first as artifact, i (i)}
                <div class="tf-r">
                  <span class="tf-t tf-mono">{artifact.title}</span>
                  <span class="tf-rend">
                    {#if artifact.meta !== null}<span class="tf-dim">{artifact.meta}</span>{/if}
                    {#if artifact.url !== null}
                      <button type="button" class="tf-btn tf-sm" onclick={() => downloadFile(artifact.title)}>Download</button>
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
            class="tf-btn tf-runagain"
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

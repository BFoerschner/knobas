<!--
  The run page (#853, R-9.9 to R-9.11, ADR-0022): the right-hand side of
  Builds at `#/build/<pipeline>/<run>[/<tab>]`, drawn over the declared run
  (`pipeline_runs`) and today's build view (`entity_view`).

  - **Header**: the pipeline and number with the result chip; the branch,
    who or what started it, when, and how long it took. *Re-run* (the same
    branch and values, queued at once), *Run again with changes…* (the Run…
    dialog from this run, #852) and *Open in <source>*. A run in flight has
    *Stop*, disabled: it needs a new write.
  - **Overview**: a failed run leads with its first failed test (the
    details as the source wrote them, *Show it in the log*, *Open the test
    file* where the details name a path, and the line saying how the runs
    before it went); a running run says what it is doing; then the *Run*
    panel and the first four *Artifacts*.
  - **Changes**: the commits in the run, each opening its commit.
  - **Parameters**: each value in this run beside the default, the branch,
    and *Run again with these…*.
  - *Log*, *Tests* and *Artifacts* are later tickets' (#854 to #856) and
    say so.

  The steps bar (R-9.10) waits for a read of a run's steps, which nothing
  answers yet (#854's step boundaries).
-->
<script lang="ts">
  import { onDestroy, onMount, untrack, type Snippet } from "svelte";

  import { ipcErrorMessage } from "../../ipc";
  import type { PipelineRow, PipelineSettings, RunRow, SourceView } from "../../ipc/entity";
  import type { SourceSummary, SourceSyncStatus } from "../../ipc/sources";
  import { push } from "../../shell/toasts.svelte";
  import { openWriteQueue } from "../../shell/write-queue.svelte";
  import { commitHash, fileHash } from "../repo/repo";
  import { when } from "../tickets/tickets";
  import { duration, pipelineHash, runTone } from "./builds";
  import { defaultBranch, jobBranchFor, resultWord, staleLine } from "./pipeline";
  import { RUN_BRIDGE, type RunPorts } from "./pipeline-ports";
  import type { RunRequest } from "./run-dialog";
  import TfRunDialog from "./TfRunDialog.svelte";
  import {
    RUN_TABS,
    artifactsOf,
    changesOf,
    commitOf,
    contextLine,
    factOf,
    failureHeading,
    firstFailure,
    parameterRows,
    previousOnBranch,
    rerunRequest,
    runTabHash,
    runTabOf,
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
  let token = 0;

  $effect(() => {
    const wanted = { pipelineId, runId };
    untrack(() => open(wanted.pipelineId, wanted.runId));
  });

  function open(pipe: string, id: string) {
    const mine = ++token;
    held = null;
    runsFailed = null;
    view = null;
    viewRead = false;
    viewFailed = null;
    failure = null;
    settings = null;
    void findRun(pipe, id, mine);
    ports.view(id).then(
      (answer) => {
        if (mine !== token) return;
        view = answer;
        viewRead = true;
        const tests = answer?.sections.find((s) => s.id === "tests");
        if ((tests?.count ?? 0) > 0) readFailure(id, mine);
      },
      (cause: unknown) => {
        if (mine !== token) return;
        viewFailed = ipcErrorMessage(cause);
        viewRead = true;
      },
    );
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
      if (mine === token) held = rows;
    } catch (cause) {
      if (mine === token) runsFailed = ipcErrorMessage(cause);
    }
  }

  function readFailure(id: string, mine: number) {
    ports.view(id, "tests").then(
      (answer) => {
        if (mine === token) failure = firstFailure(answer?.sections.find((s) => s.id === "tests") ?? null);
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
  const startedBy = $derived(factOf(view, "Triggered by"));
  const agent = $derived(factOf(view, "Agent"));
  const now = $derived(factOf(view, "Now"));
  const commit = $derived(commitOf(view));
  const repoId = $derived(settings?.repositories.find((repo) => repo.entity_id !== null)?.entity_id ?? null);
  const artifacts = $derived(artifactsOf(view));
  const changes = $derived(changesOf(view));
  const parameters = $derived(parameterRows(view, settings));
  const testsCount = $derived(view?.sections.find((s) => s.id === "tests")?.count ?? 0);
  const failed = $derived(run !== null && runTone(run) === "err");
  const inFlight = $derived(run !== null && run.finished === null);
  const stale = $derived(staleLine(pipeline, sources.map((s) => s.health), sync));
  const jobBranch = $derived(held === null ? null : jobBranchFor(held, jobBranches));

  function chipTone(r: RunRow): string {
    const tone = runTone(r);
    if (tone === "run" || tone === "q") return "tf-info";
    return tone === "ok" || tone === "err" || tone === "warn" ? `tf-${tone}` : "";
  }

  /** The header's chip: the run's own word, *running* or *queued* while in flight. */
  function chipWord(r: RunRow): string {
    const tone = runTone(r);
    return tone === "run" || tone === "q" ? resultWord(r) : (r.state?.text ?? "");
  }

  // -- the writes ---------------------------------------------------------------

  /** *Re-run* and *Run again…*: queued at once, and the toast says so (R-9.8). */
  async function queue(request: RunRequest) {
    try {
      await ports.submitWrite(request.payload);
      push({
        text: `Queued: run ${pipeline.name}. ${request.summary}. ${stale?.fix ? `${pipeline.source_name} sign-in is failing, so it waits there.` : "Sends with the next sync."}`,
        action: { label: "Show queue", run: openWriteQueue },
      });
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

  function rerun() {
    if (run === null || view === null) return;
    void queue(rerunRequest(pipelineId, run, settings, view));
  }

  let dialog = $state(false);

  function onrun(request: RunRequest) {
    dialog = false;
    void queue(request);
  }

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

{#if runsFailed !== null}
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
          ? `, ${when(at)}`
          : ""}{run.duration_seconds !== null ? `, took ${duration(run.duration_seconds)}` : ""}
      </div>
    </div>
    <div class="tf-acts">
      {#if !inFlight}
        <button
          type="button"
          class="tf-btn tf-primary"
          disabled={view === null}
          title={view === null && viewRead ? "The run's settings could not be read" : undefined}
          onclick={rerun}>Re-run</button
        >
        <button type="button" class="tf-btn" onclick={() => (dialog = true)}>Run again with changes…</button>
      {:else}
        <button type="button" class="tf-btn" aria-disabled="true" title="Needs a new write">Stop</button>
      {/if}
      {#if run.web_url !== null}
        <button type="button" class="tf-btn tf-ghost" onclick={() => openExternal(run.web_url)}>Open in {pipeline.source_name}</button>
      {/if}
    </div>
  </header>

  {#if stale !== null}
    <div class="tf-stale tf-bstale">
      {stale.text}
      {#if stale.fix !== null}<a href={stale.fix}>Re-enter password</a>{/if}
    </div>
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
            </div>
            {#if held !== null && contextLine(held, run, mainBranch) !== ""}
              <span class="tf-dim">{contextLine(held, run, mainBranch)}</span>
            {/if}
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
                  >{:else if commit !== null}{commit.sha}{:else}<span class="tf-dim">{viewRead ? "none" : "…"}</span>{/if}
              </dd>
              <dt>Started by</dt>
              <dd>{startedBy ?? ""}</dd>
              <dt>Agent</dt>
              <dd class="tf-mono">{agent ?? ""}</dd>
              <dt>Started</dt>
              <dd>{run.started !== null ? when(run.started) : "not yet"}</dd>
              <dt>Took</dt>
              <dd>{tookLine(run, previous)}</dd>
            </dl>
          </section>
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
                      <button type="button" class="tf-btn tf-sm" onclick={() => openExternal(artifact.url)}>Download</button>
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
                  >{#if change.entity_id !== null && repoId !== null}<a href={commitHash(repoId, change.entity_id)}>{change.chip ?? ""}</a
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
          <button type="button" class="tf-btn tf-runagain" onclick={() => (dialog = true)}>Run again with these…</button>
        {/if}
      {/if}
    {:else}
      <div class="tf-empty"><p>{RUN_TABS.find((t) => t.id === tab)?.name} is not built in the new shell yet.</p></div>
    {/if}
  </div>

  {#if dialog}
    <TfRunDialog
      {pipeline}
      {settings}
      runs={held}
      from={run}
      {jobBranch}
      signInFailing={stale?.fix != null}
      {ports}
      onclose={() => (dialog = false)}
      {onrun}
    />
  {/if}
{/if}

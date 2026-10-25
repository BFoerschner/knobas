<!--
  The pipeline page (#851, R-9.4 to R-9.6, ADR-0022): the right-hand side of
  Builds at `#/builds/<pipeline>[/<tab>]`, drawn over the declared pipeline
  and run and the two live reads of `knobas_source::pipeline`.

  - **Header**: the name; one line saying what it is in the source's word
    (the label its descriptor gives the pipeline kind), its group, the
    repository it builds (a link where the mirror holds it) and what triggers
    it; *Run…*, *Run on <job branch>* (or the default branch), *Open in
    <source>*. A warn line when the source's sign-in or last sync failed.
  - **History**: the last thirty runs as bars, height duration and colour
    result, each a link to its run with its facts as the tip; *Passed*,
    *Usual duration*, *Last passing*.
  - **Tabs**: *Runs* (the filter bar, runs by day, each with result, branch
    and the job's chip, how it started, its test summary (*1 failed, 41
    passed*, #1448), duration bar, start, *Log* and *Run again…*; older
    runs asked of the source when the end scrolls into view), *Parameters*
    (each default with the kind its spec gives and *Change…* through the
    write queue, a select for a fixed list), *Triggers*.

  *Run…* and *Run again…* open the Run… dialog (#852, `TfRunDialog`). *Pin* writes through the list's pins (#849, `onpin`). A run started here shows as
  *queued* at once, never as started (R-9.8).

  A run or a parameter change the source's credential cannot make
  (`refused_writes`, #2040) is drawn disabled with the reason, and a press on
  it opens nothing.
-->
<script lang="ts">
  import { secondsLabel } from "../../time/format";
  import { onDestroy, onMount, untrack } from "svelte";

  import { ipcErrorMessage } from "../../ipc";
  import type { PipelineRow, PipelineSettings, RunRow, Setting } from "../../ipc/entity";
  import type { SourceDescriptor, SourceSummary, SourceSyncStatus, WriteOpPayload } from "../../ipc/sources";
  import { toastQueued } from "../../shell/queued-toast";
  import { askInto, refusalOf, refusedAttrs, type RefusedWrite } from "../../shell/refused-writes";
  import { push } from "../../shell/toasts.svelte";
  import { settingPayload } from "../../detail/view/settings";
  import FilterBar from "../filter/FilterBar.svelte";
  import { sessionFilter } from "../filter/filter-state.svelte";
  import Marked from "../filter/Marked.svelte";
  import type { FilterConfig } from "../filter/filter";
  import { queuedMarks, settleQueued } from "../queued.svelte";
  import { runHash, runTone, shortTime } from "./builds";
  import { chainSentence } from "./chain";
  import { BRIDGE, listenSynced, type PipelinePorts } from "./pipeline-ports";
  import { fieldsOf, type RunRequest } from "./run-dialog";
  import TfRunDialog from "./TfRunDialog.svelte";
  import {
    BAR_STEP,
    BAR_WIDTH,
    CHART_HEIGHT,
    DURBAR_WIDTH,
    RESULTS,
    TABS,
    appendOlder,
    barHeight,
    barTip,
    branchesOf,
    byDay,
    chipTone,
    defaultBranch,
    durbarWidth,
    historyFacts,
    historyOf,
    isStartedHere,
    jobBranchFor,
    keptRuns,
    longest,
    markQueuedRun,
    pipelineWord,
    pipelineSource,
    QUEUED_RUN,
    RUN_OP,
    runFieldAsks,
    runFieldRefusals,
    queuedRunRows,
    queuedRunWhat,
    resultWord,
    runBranch,
    runLanded,
    staleLine,
    syncWith,
    tabHash,
    tabOf,
    triggerDetail,
    triggerLabel,
    triggersSentence,
  } from "./pipeline";

  let {
    pipeline,
    rest,
    job,
    runs: readRuns,
    onpin,
    ports = BRIDGE,
  }: {
    pipeline: PipelineRow;
    /** What follows `builds` in the address: the pipeline, then the tab. */
    rest: string[];
    /** The job the reader was last in. */
    job: string | null;
    /** The mirrored runs, newest first (`pipeline_runs`). */
    runs: (pipelineId: string) => Promise<RunRow[]>;
    /** *Pin* / *Unpin*: the list's pins (#849), which the list owns. */
    onpin?: () => void;
    ports?: PipelinePorts;
  } = $props();

  const tab = $derived(tabOf(rest));
  const id = $derived(pipeline.entity_id);

  // -- reads ----------------------------------------------------------------

  let held = $state<RunRow[] | null>(null);
  let failed = $state<string | null>(null);
  let settings = $state<PipelineSettings | null>(null);
  let settingsFailed = $state<string | null>(null);
  let settingsRead = $state(false);
  let jobBranches = $state<string[]>([]);
  let adapters = $state<SourceDescriptor[]>([]);
  let sources = $state<SourceSummary[]>([]);
  let sync = $state<SourceSyncStatus[]>([]);
  /** Older history: `more` while the source may hold more, `end` once it said none. */
  let older = $state<"more" | "loading" | "end">("more");
  let olderFailed = $state<string | null>(null);
  /** Bumped on every new pipeline, so an answer for the last one is dropped. */
  let token = 0;

  $effect(() => {
    const wanted = id;
    untrack(() => open(wanted));
  });

  /**
   * What the source's credential cannot write on this pipeline (#2040): a run, the op its
   * settings name for a parameter, and the run's fields that need more than the run (#2055).
   * Asked once the settings are read, so once per pipeline.
   */
  let refused = $state<RefusedWrite[]>([]);
  const parameterOp = $derived(settings?.parameter_write_op ?? null);
  $effect(() => {
    refused = [];
    if (!settingsRead) return;
    const asks = [
      { entity: id, op: RUN_OP },
      ...(parameterOp !== null ? [{ entity: id, op: parameterOp }] : []),
      ...runFieldAsks(id),
    ];
    return askInto(asks, ports.refusedWrites, (answer) => (refused = answer));
  });
  const cannotRun = $derived(refusalOf(refused, id, RUN_OP));
  const cannotChange = $derived(parameterOp === null ? null : refusalOf(refused, id, parameterOp));
  const fieldRefusals = $derived(runFieldRefusals(refused, id));

  function open(wanted: string) {
    const mine = ++token;
    held = null;
    failed = null;
    settings = null;
    settingsFailed = null;
    settingsRead = false;
    older = "more";
    olderFailed = null;
    readRuns(wanted).then(
      (rows) => {
        if (mine !== token) return;
        settleQueued(wanted, runLanded(rows));
        held = rows;
      },
      (cause: unknown) => {
        if (mine === token) failed = ipcErrorMessage(cause);
      },
    );
    ports.settings(wanted).then(
      (answer) => {
        if (mine !== token) return;
        settings = answer;
        settingsRead = true;
      },
      (cause: unknown) => {
        if (mine !== token) return;
        settingsFailed = ipcErrorMessage(cause);
        settingsRead = true;
      },
    );
  }

  /** Bumped on every read again, so an older one answering late is dropped. */
  let rereads = 0;

  /**
   * A sync of the pipeline's source finished (#1450): the runs again, older
   * pages kept, the reader's filter, tab and dialog untouched. A failed read
   * keeps the runs it had and says so.
   */
  function reread(wanted: string) {
    const mine = token;
    const seq = ++rereads;
    const current = () => mine === token && seq === rereads;
    readRuns(wanted).then(
      (rows) => {
        if (!current()) return;
        settleQueued(wanted, runLanded(rows));
        failed = null;
        held = appendOlder(rows, held ?? []);
      },
      (cause: unknown) => {
        if (current()) failed = ipcErrorMessage(cause);
      },
    );
  }

  $effect(() => {
    const wanted = id;
    return listenSynced(
      ports,
      pipeline.source_id,
      (row) => (sync = syncWith(sync, row)),
      () => reread(wanted),
    );
  });

  // Facts about the sources, read once: the word, the warn line.
  onMount(() => {
    void ports.adapters().then((rows) => (adapters = rows), () => {});
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

  /**
   * Runs queued this session, here or on a run page, drawn *queued* until a
   * read holds them (R-9.8, #1449); the queue holding or refusing one drops it.
   */
  const started = $derived(queuedRunRows(queuedMarks(id, QUEUED_RUN), defaultBranch(settings)));
  const all = $derived(held === null ? null : [...started, ...held]);

  async function more() {
    if (held === null || older !== "more" || held.length === 0) return;
    const mine = token;
    const before = held.at(-1)!.entity_id;
    older = "loading";
    olderFailed = null;
    try {
      const rows = await ports.older(id, before);
      if (mine !== token) return;
      held = appendOlder(held, rows);
      older = rows.length === 0 ? "end" : "more";
    } catch (cause) {
      if (mine !== token) return;
      olderFailed = ipcErrorMessage(cause);
      older = "more";
    }
  }

  // The end of the list in view asks for the next page (R-9.5). Svelte 5 sets a
  // `bind:this` to `null`, not `undefined`, when the element goes (another tab),
  // and `observe(null)` threw and stopped every later effect in the window (#1446).
  let sentinel: HTMLElement | null | undefined = $state();
  $effect(() => {
    const at = sentinel;
    if (at == null || typeof IntersectionObserver === "undefined") return;
    const watcher = new IntersectionObserver((entries) => {
      if (entries.some((entry) => entry.isIntersecting) && olderFailed === null) void more();
    });
    watcher.observe(at);
    return () => watcher.disconnect();
  });

  // -- header ----------------------------------------------------------------

  const adapterKind = $derived(sources.find((s) => s.id === pipeline.source_id)?.adapter_kind ?? null);
  const word = $derived(pipelineWord(adapters, adapterKind));
  /** The project path, as the source nests it (#849), else the group. */
  const project = $derived(pipeline.path.length > 0 ? pipeline.path.join(" › ") : (pipeline.group ?? ""));
  const repositories = $derived(settings?.repositories ?? []);
  const triggers = $derived(triggersSentence(settings));
  const chain = $derived(chainSentence(settings?.chain ?? [], pipeline.entity_id));
  const stale = $derived(staleLine(pipeline, sources, sync));
  const jobBranch = $derived(all === null ? null : jobBranchFor(all, jobBranches));
  const fallback = $derived(defaultBranch(settings));

  /** *Run on <branch>*: one run on the queue, drawn queued at once (R-9.8). */
  function runOn(branch: string | null) {
    if (cannotRun !== null) return;
    const words = branch ?? fallback ?? "the default branch";
    void queue({ RunBuild: { entity: id, branch, parameters: [] } }, `On ${words}, defaults`);
  }

  /**
   * Queue a run and draw it *queued* at the top at once (R-9.8). The toast
   * names it by the dialog's line, as prototype-6's `startRun` does.
   */
  async function queue(payload: WriteOpPayload, summary: string): Promise<boolean> {
    try {
      const write = await ports.submitWrite(payload);
      markQueuedRun(pipeline, write, runBranch(payload), held, ports.follower());
      toastQueued(queuedRunWhat(pipeline, summary), pipelineSource(pipeline));
      return true;
    } catch (cause) {
      push({ text: `Could not queue the run: ${ipcErrorMessage(cause)}`, tone: "err" });
      return false;
    }
  }

  /** *Run…* and *Run again…*: the Run… dialog (#852), from a run's settings or none. */
  let dialog = $state<{ from: RunRow | null } | null>(null);

  function runDialog(from: RunRow | null) {
    if (cannotRun !== null) return;
    dialog = { from };
  }

  async function onrun(request: RunRequest) {
    dialog = null;
    if (!(await queue(request.payload, request.summary)) || request.preset === null) return;
    try {
      await ports.savePreset(request.preset);
    } catch (cause) {
      push({ text: `Could not save the preset: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }

  function openInSource() {
    if (pipeline.web_url === null) return;
    void ports.openExternal(pipeline.web_url).catch((cause: unknown) => {
      push({ text: `Could not open ${pipeline.source_name}: ${ipcErrorMessage(cause)}`, tone: "err" });
    });
  }

  // -- history ---------------------------------------------------------------

  const history = $derived(all === null ? [] : historyOf(all));
  const max = $derived(longest(all ?? []));
  const facts = $derived(historyFacts(all ?? []));

  // -- runs tab --------------------------------------------------------------

  const filter = sessionFilter("builds-runs");
  if (filter.scope === null) filter.scope = "all";
  const config = $derived<FilterConfig>({
    id: "builds-runs",
    placeholder: "Filter runs by number, branch, commit or person",
    noun: "runs",
    one: "run",
    hint: false,
    scopes: [
      ...(job !== null ? [{ id: "job", label: "My job’s branches", query: "" }] : []),
      { id: "mine", label: "Started by me", query: "" },
      { id: "all", label: "All", query: "" },
    ],
    chips: [{ key: "result", label: "Result", options: RESULTS.map((value) => ({ value, label: value })) }],
    dropdowns: [{ key: "branch", label: "Branch", options: branchesOf(all ?? []).map((value) => ({ value, label: value })) }],
  });
  const kept = $derived(
    all === null
      ? []
      : keptRuns(
          all,
          {
            text: filter.text,
            // *My job's branches* is only offered with a job; a remembered one without reads as *All*.
            scope: job !== null || filter.scope !== "job" ? filter.scope : "all",
            results: filter.chips["result"] ?? [],
            branch: filter.dropdowns["branch"] ?? null,
          },
          jobBranches,
        ),
  );
  const days = $derived(byDay(kept));

  /**
   * A click anywhere on a run row opens the run (R-9.6, prototype-6's
   * `tr.row`, #1452); the number stays the keyboard's link. A link or button
   * in the row does its own, a selection of the row's words is not a click,
   * and a queued row has no run yet.
   */
  function openRow(event: MouseEvent, run: RunRow) {
    if (isStartedHere(run)) return;
    if ((event.target as Element | null)?.closest("a, button, input, select, label")) return;
    if ((window.getSelection()?.toString() ?? "") !== "") return;
    location.hash = runHash(id, run.entity_id);
  }

  // -- parameters tab --------------------------------------------------------

  let editing = $state<string | null>(null);
  let draft = $state("");
  /** Each parameter's field as the Run… dialog asks it (#1447): its kind and, for a fixed list, the options. */
  const fields = $derived(new Map(fieldsOf(settings).map((field) => [field.name, field])));
  let queuedValues = $state<Record<string, string>>({});

  /** Escape leaves the edit, and goes no further (the page's own Escape goes back). */
  function cancelOnEscape(event: KeyboardEvent) {
    if (event.key !== "Escape") return;
    event.stopPropagation();
    editing = null;
  }

  function change(setting: Setting) {
    if (cannotChange !== null) return;
    editing = setting.name;
    draft = setting.secret ? "" : setting.value;
  }

  async function save(setting: Setting) {
    const op = settings?.parameter_write_op ?? null;
    const payload = op === null ? null : settingPayload(op, id, setting.name, draft, setting.secret ? null : setting.value);
    if (payload === null) return;
    try {
      await ports.submitWrite(payload);
      queuedValues = { ...queuedValues, [setting.name]: setting.secret ? "" : draft };
      editing = null;
      toastQueued(`set ${setting.name} on ${pipeline.name}`, pipelineSource(pipeline));
    } catch (cause) {
      push({ text: `Could not queue the change: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }

  onDestroy(() => {
    token += 1;
  });
</script>

<header class="tf-fhead tf-bhead">
  <div>
    <h1>{pipeline.name}</h1>
    <div class="tf-sub">
      {pipeline.source_name}{word !== null ? ` ${word}` : ""}{project !== "" ? ` in project ${project}` : ""}{#if repositories.length > 0}, builds
        {#each repositories as repository, at (at)}{#if at > 0},{" "}{/if}{#if repository.entity_id !== null}<a
              class="tf-mono"
              href={`#/repo/${encodeURIComponent(repository.entity_id)}`}>{repository.name}</a
            >{:else}<span class="tf-mono">{repository.name}</span>{/if}{/each}{/if}.{triggers !== "" ? ` ${triggers}` : ""}{chain !== "" ? ` ${chain}` : ""}
    </div>
  </div>
  <div class="tf-acts">
    <button
      type="button"
      class="tf-btn tf-primary"
      {...refusedAttrs(cannotRun)}
      onclick={() => runDialog(null)}>Run…</button
    >
    {#if jobBranch !== null}
      <button
        type="button"
        class="tf-btn"
        {...refusedAttrs(cannotRun)}
        onclick={() => runOn(jobBranch)}>Run on <span class="tf-mono">{jobBranch}</span></button
      >
    {:else}
      <button
        type="button"
        class="tf-btn"
        {...refusedAttrs(cannotRun)}
        onclick={() => runOn(null)}
        >Run on {#if fallback !== null}<span class="tf-mono">{fallback}</span>{:else}the default branch{/if}</button
      >
    {/if}
    {#if onpin !== undefined}
      <button type="button" class="tf-btn tf-ghost" onclick={onpin}>{pipeline.pinned ? "Unpin" : "Pin"}</button>
    {/if}
    {#if pipeline.web_url !== null}
      <button type="button" class="tf-btn tf-ghost" onclick={openInSource}>Open in {pipeline.source_name}</button>
    {/if}
  </div>
</header>

{#if stale !== null}
  <div class="tf-stale tf-bstale">
    {stale.text}
    {#if stale.fix !== null}<a href={stale.fix.href}>{stale.fix.words}</a>{/if}
  </div>
{/if}

{#if failed !== null}
  <p class="tf-dim tf-runs">Runs could not be read: {failed}</p>
{/if}
{#if all !== null && all.length > 0}
  <div class="tf-hist">
    <div class="tf-chart">
      <svg
        class="tf-bars"
        width={history.length * (BAR_STEP)}
        height={CHART_HEIGHT}
        role="group"
        aria-label={`Duration of the last ${history.length} runs`}
      >
        {#each history as run, at (run.entity_id)}
          {@const h = barHeight(run, max)}
          <a href={runHash(id, run.entity_id)} aria-label={`Run ${run.number ?? "queued"}, ${resultWord(run)}`}>
            <title>{barTip(run)}</title>
            <rect class={`tf-bar tf-run-${runTone(run)}`} x={at * (BAR_STEP)} y={CHART_HEIGHT - h} width={BAR_WIDTH} height={h} rx="2" />
          </a>
        {/each}
      </svg>
      <div class="tf-axis">
        <span>{history[0]?.started ? shortTime(history[0].started) : ""}</span><span>now</span>
      </div>
      <div class="tf-legend">
        <span><i class="tf-run-ok"></i>passed</span><span><i class="tf-run-err"></i>failed</span><span
          ><i class="tf-run-run"></i>running</span
        ><span>bar height is duration</span>
      </div>
    </div>
    <div class="tf-facts2">
      <div><b>Passed</b>{facts.passed} of the last {facts.finished}</div>
      <div><b>Usual duration</b>{facts.usual !== null ? secondsLabel(facts.usual) : "none"}</div>
      <div>
        <b>Last passing</b>{#if facts.lastPassing !== null}<a href={runHash(id, facts.lastPassing.entity_id)}
            >#{facts.lastPassing.number}</a
          >{facts.lastPassing.started ? `, ${shortTime(facts.lastPassing.started)}` : ""}{:else}none{/if}
      </div>
    </div>
  </div>
{/if}

<nav class="tf-tabs" aria-label="Pipeline">
  {#each TABS as entry (entry.id)}
    <a href={tabHash(id, entry.id)} aria-current={entry.id === tab ? "page" : undefined}
      >{entry.name}{#if entry.id === "runs" && all !== null && all.length > 0}{" "}<span class="tf-count">{all.length}</span>{/if}</a
    >
  {/each}
</nav>

{#if tab === "runs"}
  <div class="tf-runbar">
    <FilterBar {config} {filter} shown={all === null ? null : kept.length} total={all?.length ?? null} />
  </div>
  <div class="tf-runs">
    {#if all === null && failed === null}
      <p class="tf-dim">Reading runs…</p>
    {:else if all !== null && all.length === 0}
      <div class="tf-empty"><p>No runs yet.</p></div>
    {:else if all !== null && kept.length === 0}
      <div class="tf-empty"><p>No run matches.</p></div>
    {/if}
    {#each days as day (day.label)}
      <h3 class="tf-dayh">{day.label}</h3>
      <table class="tf-tbl tf-runtbl" aria-label={`Runs, ${day.label}`}>
        <!-- One width per column on every day's table, so they line up down the page (#1452). -->
        <colgroup
          ><col class="tf-run-no" /><col class="tf-run-br" /><col class="tf-run-trig" /><col class="tf-run-tests" /><col
            class="tf-run-dur"
          /><col class="tf-run-at" /><col class="tf-run-acts" /></colgroup
        >
        <tbody>
          {#each day.runs as run (run.entity_id)}
            {@const tone = runTone(run)}
            <tr class:tf-row={!isStartedHere(run)} onclick={(event) => openRow(event, run)}>
              <td class="tf-nowrap tf-run-no">
                {#if isStartedHere(run)}
                  <b>queued</b>
                {:else}
                  <a href={runHash(id, run.entity_id)}><b><Marked text={run.number !== null ? `#${run.number}` : "queued"} typed={filter.text} /></b></a>
                {/if}
                {#if run.state !== null}<span class={`tf-chip ${chipTone(run)}`}>{tone === "q" ? "queued" : run.state.text}</span>{/if}
              </td>
              <td class="tf-mono tf-run-br"
                >{#if run.branch !== null}<Marked text={run.branch} typed={filter.text} />{/if}{#if run.branch !== null && jobBranches.includes(run.branch) && job !== null}{" "}<span
                    class="tf-chip tf-jobchip-sm">{job}</span
                  >{/if}</td
              >
              <td class="tf-dim tf-run-trig"><Marked text={triggerLabel(run)} typed={filter.text} /></td>
              <td class="tf-nowrap tf-run-tests"
                >{#if run.tests !== null}{#if run.tests.failed > 0}<span class="tf-tests-failed">{run.tests.failed} failed</span>,{" "}{/if}{run.tests
                    .passed} passed{#if run.tests.ignored > 0}, {run.tests.ignored} ignored{/if}{/if}</td
              >
              <td class="tf-nowrap tf-num tf-run-dur">
                {#if run.duration_seconds !== null}
                  <svg class="tf-durbar" width={DURBAR_WIDTH} height="4" aria-hidden="true"
                    ><rect width={durbarWidth(run.duration_seconds, max)} height="4" rx="2" /></svg
                  >{secondsLabel(run.duration_seconds)}
                {:else if tone === "run"}
                  <progress class="tf-progress" aria-label="Running"></progress> running
                {:else}
                  waiting
                {/if}
              </td>
              <td class="tf-nowrap tf-dim tf-num tf-run-at">{run.started !== null ? shortTime(run.started) : ""}</td>
              <td class="tf-nowrap tf-run-acts">
                {#if !isStartedHere(run)}
                  <a class="tf-btn tf-sm tf-ghost" href={`${runHash(id, run.entity_id)}/log`}>Log</a>
                  {#if run.finished !== null}
                    <button
                      type="button"
                      class="tf-btn tf-sm tf-ghost"
                      {...refusedAttrs(cannotRun)}
                      onclick={() => runDialog(run)}>Run again…</button
                    >
                  {/if}
                {/if}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/each}
    {#if all !== null && all.length > 0}
      <div class="tf-older" bind:this={sentinel}>
        {#if olderFailed !== null}
          <p class="tf-dim">Older runs could not be read: {olderFailed}</p>
          <button type="button" class="tf-btn tf-sm" onclick={() => ((olderFailed = null), void more())}>Try again</button>
        {:else if older === "loading"}
          <p class="tf-dim">Reading older runs…</p>
        {:else if older === "end"}
          <p class="tf-dim">The start of this pipeline’s history.</p>
        {:else}
          <button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => void more()}>Older runs</button>
        {/if}
      </div>
    {/if}
  </div>
{:else if tab === "params"}
  <div class="tf-runs tf-bsettings">
    {#if settingsFailed !== null}
      <p class="tf-dim">Parameters could not be read from {pipeline.source_name}: {settingsFailed}</p>
    {:else if !settingsRead}
      <p class="tf-dim">Reading parameters…</p>
    {:else if settings === null || settings.parameters.length === 0}
      <div class="tf-empty"><p>No parameters. Parameters added in {pipeline.source_name} show here.</p></div>
    {:else}
      <p class="tf-dim">
        The defaults every run starts from. A run can override them in Run…; changing a default here writes to {pipeline.source_name}.
      </p>
      <table class="tf-tbl" aria-label="Parameters">
        <thead><tr><th>Parameter</th><th>Default</th><th>Kind</th><th></th></tr></thead>
        <tbody>
          {#each settings.parameters as setting (setting.name)}
            {@const field = fields.get(setting.name)}
            <tr>
              <td class="tf-mono">{setting.name}</td>
              <td class="tf-mono">
                {#if editing === setting.name && field?.kind.type === "select"}
                  <select
                    aria-label={`New default for ${setting.name}`}
                    bind:value={draft}
                    onkeydown={cancelOnEscape}
                  >
                    {#each field.kind.options as option (option.value)}
                      <option value={option.value}>{option.label ?? option.value}</option>
                    {/each}
                  </select>
                {:else if editing === setting.name}
                  <input
                    type={setting.secret ? "password" : "text"}
                    aria-label={`New default for ${setting.name}`}
                    bind:value={draft}
                    onkeydown={(event) => {
                      if (event.key === "Enter") void save(setting);
                      cancelOnEscape(event);
                    }}
                  />
                {:else if setting.secret}
                  <span class="tf-dim">(hidden)</span>
                {:else}
                  {(queuedValues[setting.name] ?? setting.value) || "(empty)"}
                {/if}
                {#if setting.name in queuedValues}<span class="tf-chip tf-info">queued</span>{/if}
              </td>
              <td class="tf-dim">
                {field?.kindWord ?? "text"}
                {#if !setting.secret && setting.inherited}<span class="tf-chip">inherited</span>{/if}
              </td>
              <td class="tf-nowrap">
                {#if editing === setting.name}
                  <button type="button" class="tf-btn tf-sm tf-primary" onclick={() => void save(setting)}>Save</button>
                  <button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => (editing = null)}>Cancel</button>
                {:else if settings.parameter_write_op !== null && setting.editable}
                  <button
                    type="button"
                    class="tf-btn tf-sm"
                    {...refusedAttrs(cannotChange)}
                    onclick={() => change(setting)}>Change…</button
                  >
                {/if}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
  </div>
{:else}
  <div class="tf-runs tf-bsettings">
    {#if settingsFailed !== null}
      <p class="tf-dim">Triggers could not be read from {pipeline.source_name}: {settingsFailed}</p>
    {:else if !settingsRead}
      <p class="tf-dim">Reading triggers…</p>
    {:else if settings === null || settings.triggers.length === 0}
      <div class="tf-empty"><p>No triggers. Runs start only when someone starts one.</p></div>
    {:else}
      <table class="tf-tbl" aria-label="Triggers">
        <tbody>
          {#each settings.triggers as trigger, at (at)}
            <tr>
              <td
                ><b>{trigger.name} trigger</b>{#if trigger.disabled}{" "}<span class="tf-chip">disabled</span>{/if}{#if trigger.facts.length > 0}<span
                    class="tf-sub">{triggerDetail(trigger)}</span
                  >{/if}</td
              >
            </tr>
          {/each}
        </tbody>
      </table>
      <p class="tf-dim">Read only. Edit triggers in {pipeline.source_name}.</p>
    {/if}
  </div>
{/if}

{#if dialog !== null}
  <TfRunDialog
    {pipeline}
    {settings}
    runs={all ?? []}
    from={dialog.from}
    {jobBranch}
    secretNoun={stale?.fix?.secret ?? null}
    {...fieldRefusals}
    {ports}
    onclose={() => (dialog = null)}
    {onrun}
  />
{/if}

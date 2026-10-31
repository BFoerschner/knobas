<!--
  Builds (#848, R-9.1, R-9.4a, ADR-0022): the tracer of the Builds frame.
  The shell draws it over the declared pipeline and run kinds, so nothing
  here names an adapter: the source's word is its configured name, drawn
  once, as the header line over its pipelines.

  - **Left**, the pipeline list: with a job current, *This job <key> / All*
    (#850, *This job* by default); the filter field (`/` focuses it, `Esc`
    clears it through the shell's ladder, which finds it by `data-filter`)
    and the *Failing* and *Running* chips. *All* draws *Pinned* first
    (knobas-local, #849), then each source's project tree as deep as it goes
    (folds with their count and *N failing*, own pipelines before
    subgroups, a chain of single children as one row, a fold open on a
    failure, the selection or anything narrowing, else as the reader left
    it). *This job* draws the same tree pruned to the job's pipelines, each
    with its reasons, under "N of M pipelines are relevant to KEY". Each
    pipeline by name, in err tone when its latest run failed, the time of
    that run, and a strip of the last twelve runs newest right. A hover
    gives its groups and name. The selected one is drawn inverted and kept
    in view. The list is windowed (#2211) over its flattened rows
    (`listRows`): headings, pipelines and the summaries of folds, a fold's
    rows following its summary while it is open.
  - **Right**, the selected pipeline's page (#851, `TfPipeline.svelte`):
    header with *Pin*, history chart, and the Runs, Parameters and Triggers
    tabs.

  `#/builds` selects the first failing pipeline, else the first;
  `#/builds/<entity id>` selects that one. `#/build/<pipeline>/<run>` is the
  run page (#853, `TfRun.svelte`) with its pipeline selected on the left;
  `#/build/<run>` finds the run's pipeline and moves to that address.
-->
<script lang="ts" module>
  import { SvelteMap } from "svelte/reactivity";

  import type { JobPipeline, PipelineRow, RunRow, SourceView } from "../../ipc/entity";

  /** The reads and the write this page makes, injectable so a test needs no bridge. */
  export interface BuildsPorts {
    pipelines: () => Promise<PipelineRow[]>;
    runs: (pipelineId: string) => Promise<RunRow[]>;
    /** Pin or unpin (#849); the bridge's when omitted. */
    setPinned?: (pipelineId: string, pinned: boolean) => Promise<boolean>;
    /** The job's pipelines and their reasons, by the job's key (#850); the bridge's when omitted. */
    jobPipelines?: (job: string) => Promise<JobPipeline[]>;
    /** A run's whole view, for the pipeline a bare run address names (#853); the bridge's when omitted. */
    view?: (runId: string) => Promise<SourceView | null>;
  }

  /** The folds the reader opened or closed this session, by key (R-9.3). */
  const remembered = new SvelteMap<string, boolean>();

  /** Forget the folds: a new session, which is what each test is. */
  export function forgetFolds(): void {
    remembered.clear();
  }
</script>

<script lang="ts">
  import { flushSync, onMount, tick, untrack } from "svelte";
  import type { RoleFrameProps } from "../page-entry";

  import {
    getEntity,
    jobPipelines as readJobPipelines,
    pipelineRuns,
    pipelines as readPipelines,
    setPipelinePinned,
    type PipelineReason,
  } from "../../ipc/entity";
  import { listSources } from "../../ipc/sources";
  import { resolveTicket } from "../job/job";
  import { inTextField } from "../../shell/keys";
  import { Windowed } from "../../shell/windowed.svelte";
  import { sessionFilter } from "../filter/filter-state.svelte";
  import Marked from "../filter/Marked.svelte";
  import type { EscTarget } from "../escape";
  import TfPipeline from "./TfPipeline.svelte";
  import TfRun from "./TfRun.svelte";
  import { pipelineOfRun } from "./run";
  import { RUN_BRIDGE } from "./pipeline-ports";
  import {
    CHIPS,
    foldOpen,
    groupsOf,
    isFailing,
    keptPipelines,
    latestTime,
    listRows,
    pipelineHash,
    pipelineOf,
    pinnedOf,
    reasonsById,
    reasonsLine,
    runHash,
    runTone,
    stripOf,
    treeOf,
    type Fold,
    type Scope,
  } from "./builds";
  import { pageVisible } from "../kept-pages.svelte";

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
    /** The job the reader was last in, for *This job*. */
    job: string | null;
    /** The address the job was opened at (#1525), which its ticket is read by; its key when omitted. */
    jobAt?: string | null;
    /** Where `Esc` goes, which the breadcrumb's hint names; `null` where it does nothing (#2228). */
    back?: EscTarget | null;
    ports?: BuildsPorts;
  } & RoleFrameProps = $props();

  const filter = sessionFilter("builds-pipelines");
  const chips = $derived(filter.chips["state"] ?? []);

  /**
   * The pipelines as `pipelines` answers them: the read is CI's, so the mirror
   * narrows it to the CI role's fillers and the page drops nothing (#1295,
   * ADR-0029 as amended on 2026-10-19).
   */
  let all = $state.raw<PipelineRow[] | null>(null);
  let failed = $state<string | null>(null);

  $effect(() => {
    if (!ready || all !== null) return;
    ports.pipelines().then(
      (rows) => (all = rows),
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
      all = rows;
      failed = null;
    } catch (error) {
      if (at === listReads) failed = message(error);
    }
  }

  let pipelinesAt = untrack(() => syncRevision);
  $effect(() => {
    const at = syncRevision;
    if (!ready || at === pipelinesAt) return;
    pipelinesAt = at;
    void untrack(() => rereadPipelines());
  });

  // *This job* (#850): on by default while a job is current.
  const scope = $derived<Scope>(job !== null && (filter.scope ?? "job") === "job" ? "job" : "all");
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
  /** The job's pipelines, before the words and chips narrow them. */
  const onJob = $derived(
    all === null || scope !== "job" || reasons === null ? [] : all.filter((p) => reasons!.has(p.entity_id)),
  );
  const kept = $derived(
    all === null ? [] : keptPipelines(scope === "job" ? onJob : all, filter.text, chips),
  );
  /** The pipeline on the right: the one the list picks, or on a run's address the run's own. */
  function selectedOf(list: PipelineRow[]): PipelineRow | null {
    if (head === "builds") return pipelineOf(list, rest, onJob);
    return list.find((p) => p.entity_id === rest[0]) ?? null;
  }
  const selected = $derived(all === null ? null : selectedOf(all));

  // `#/build/<run id>` (a job's or the inbox's row) names the run alone: its
  // pipeline is the one whose recent runs hold it, else the one its view names.
  /** A bare run address no pipeline was found for. */
  let lost = $state<string | null>(null);
  $effect(() => {
    if (all === null || head !== "build" || rest.length !== 1 || selected !== null) return;
    const id = rest[0]!;
    const owner = all.find((p) => p.recent.some((r) => r.entity_id === id));
    if (owner !== undefined) {
      location.replace(runHash(owner.entity_id, id));
      return;
    }
    const ids = all.map((p) => p.entity_id);
    void (ports.view ?? RUN_BRIDGE.view)(id).then(
      (view) => {
        const found = pipelineOfRun(view, ids);
        if (found !== null && rest[0] === id) location.replace(runHash(found, id));
        else lost = id;
      },
      () => (lost = id),
    );
  });
  const pinned = $derived(scope === "job" ? [] : pinnedOf(kept));
  const trees = $derived(treeOf(kept));
  /** Whether anything narrows the list, which opens every fold. */
  const narrowed = $derived(filter.text.trim() !== "" || chips.length > 0 || scope === "job");
  /** What the list draws, flattened: the windowed list's rows. */
  const rows = $derived(
    listRows(
      pinned,
      trees,
      scope,
      (f) => foldOpen(remembered.get(f.key), f, selected?.entity_id ?? null, narrowed),
    ),
  );
  // A pipeline with its strip is about 36 px; headings and summaries are measured once drawn.
  const win = new Windowed({ count: () => rows.length, key: (i) => rows[i]!.key, estimate: 36 });

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

  function message(error: unknown): string {
    if (error && typeof error === "object" && "message" in error) return String(error.message);
    return String(error);
  }

  let field: HTMLInputElement | undefined = $state();
  let list: HTMLElement | undefined = $state();

  function toggle(chip: string) {
    filter.chips["state"] = chips.includes(chip) ? chips.filter((c) => c !== chip) : [...chips, chip];
  }

  // The selected pipeline stays in view as the list scrolls on its own (R-9.4a).
  $effect(() => {
    if (selected === null || list === undefined) return;
    const id = selected.entity_id;
    void tick().then(() => {
      // Drawn first if the window left it out.
      win.reveal(rows.findIndex((row) => row.kind === "pipeline" && row.pipeline.entity_id === id));
      flushSync();
      list?.querySelector('[aria-current="true"]')?.scrollIntoView?.({ block: "nearest" });
    });
  });

  const pageShown = pageVisible();
  onMount(() => {
    const onkey = (event: KeyboardEvent) => {
      // A page keep-alive holds hidden (#2247) takes no keys.
      if (!pageShown()) return;
      if (event.key !== "/" || event.metaKey || event.ctrlKey || event.altKey || event.defaultPrevented) return;
      const at = document.activeElement;
      if (at instanceof HTMLElement && inTextField(at)) return;
      event.preventDefault();
      field?.focus();
    };
    window.addEventListener("keydown", onkey);
    return () => window.removeEventListener("keydown", onkey);
  });

  function stateText(run: RunRow): string {
    return run.state?.text ?? "";
  }
</script>

{#snippet crumbs(parts: { name: string; href?: string }[])}
  <nav class="tf-crumbs" aria-label="Breadcrumb">
    {#each parts as part, at (at)}
      {#if part.href !== undefined}
        <a class="tf-crumb" href={part.href}>{part.name}</a><span class="tf-sep">›</span>
      {:else}
        <strong>{part.name}</strong>
      {/if}
    {/each}
    {#if back !== null}<span class="tf-esc">Back to {back.name} <kbd>Esc</kbd></span>{/if}
  </nav>
{/snippet}

{#snippet item(pipeline: PipelineRow, depth: number, key: string)}
  {@const strip = stripOf(pipeline)}
  {@const why = scope === "job" ? reasonsLine(reasons?.get(pipeline.entity_id) ?? [], job ?? "") : ""}
  <a
    class={`tf-pi ${indent(depth)}`}
    href={pipelineHash(pipeline.entity_id)}
    aria-current={pipeline.entity_id === selected?.entity_id}
    title={[...groupsOf(pipeline), pipeline.name].join(" › ")}
    {@attach win.row(key)}
  >
    <span class="tf-pi-nm" class:tf-bad={isFailing(pipeline)}><Marked text={pipeline.name} typed={filter.text} /></span>
    <span class="tf-pi-when">{latestTime(pipeline)}</span>
    <span
      class="tf-strip"
      role="img"
      aria-label={strip.length > 0
        ? `Last ${strip.length} runs, newest ${stateText(strip[strip.length - 1]!) || "without a state"}`
        : "No runs"}
    >
      {#each strip as run (run.entity_id)}<i class={`tf-run-${runTone(run)}`}></i>{/each}
    </span>
    {#if why !== ""}<span class="tf-pi-why">{why}</span>{/if}
  </a>
{/snippet}

{#snippet foldView(f: Fold, depth: number, open: boolean, key: string)}
  <!-- The summary alone: a fold's rows follow it while it is open, and a closed one has none (#2200's class, #2211). -->
  <details class="tf-pl-fold" {open} ontoggle={(event) => rememberFold(f, event)} {@attach win.row(key)}>
    <summary class={indent(depth)}
      >{f.name} <span class="tf-count">{f.count}</span>{#if f.failing > 0}<span class="tf-chip tf-err">{f.failing} failing</span
        >{/if}</summary
    >
  </details>
{/snippet}

<div class="tf-bv">
  <nav class="tf-pl" aria-label="Pipelines">
    <div class="tf-pl-top">
      {#if job !== null}
        <div class="tf-seg" role="group" aria-label="Scope">
          <button type="button" aria-pressed={scope === "job"} onclick={() => setScope("job")}
            >This job <span class="tf-jobk">{job}</span></button
          >
          <button type="button" aria-pressed={scope === "all"} onclick={() => setScope("all")}>All</button>
        </div>
      {/if}
      <label class="tf-fsearch">
        <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true"
          ><circle cx="7" cy="7" r="5" /><path d="M11 11l3.5 3.5" /></svg
        >
        <input
          bind:this={field}
          bind:value={filter.text}
          data-filter
          placeholder={all !== null ? `Filter ${all.length} pipelines` : "Filter pipelines"}
          aria-label="Filter pipelines"
        />
        <kbd>/</kbd>
      </label>
      <div class="tf-fchips" role="group" aria-label="Show only">
        {#each CHIPS as chip (chip)}
          <button type="button" aria-pressed={chips.includes(chip)} onclick={() => toggle(chip)}>{chip}</button>
        {/each}
      </div>
      {@render sync?.()}
    </div>
    <div class="tf-pl-list" bind:this={list}>
      {#if failed !== null}
        <p class="tf-pl-note">Pipelines could not be read: {failed}</p>
      {:else if all !== null && all.length === 0}
        <p class="tf-pl-note">No pipelines yet. A CI source's pipelines show here once it has synced.</p>
      {:else if all !== null && scope === "job" && reasons !== null}
        <p class="tf-scope-note">
          {kept.length} of {all.length} pipelines are relevant to <span class="tf-mono">{job}</span>.
          <button type="button" class="tf-linkish" onclick={() => setScope("all")}>Show all</button>
        </p>
      {/if}
      {#if all !== null && all.length > 0 && kept.length === 0 && (scope === "all" || reasons !== null)}
        <p class="tf-pl-note">
          No pipeline matches.
          {#if scope === "job"}<button type="button" class="tf-linkish" onclick={() => setScope("all")}>Search all pipelines</button>{/if}
        </p>
      {/if}
      <div class="tf-pl-rows" {@attach win.list}>
        {#each win.slice(rows) as row (row.key)}
          {#if row.kind === "head"}
            <h3 {@attach win.row(row.key)}>{row.text}</h3>
          {:else if row.kind === "pipeline"}
            {@render item(row.pipeline, row.depth, row.key)}
          {:else}
            {@render foldView(row.fold, row.depth, row.open, row.key)}
          {/if}
        {/each}
      </div>
    </div>
  </nav>
  <section class="tf-bmain">
    {#if head === "build" && selected !== null && rest.length >= 2}
      <TfRun pipeline={selected} {rest} {job} runs={ports.runs} {crumbs} />
    {:else if head === "build"}
      {@render crumbs([{ name: "Builds", href: "#/builds" }, { name: "Run" }])}
      {#if all !== null && (rest.length !== 1 || lost === rest[0])}
        <div class="tf-empty"><p>No run at this address. Pick a pipeline on the left.</p></div>
      {/if}
    {:else if selected !== null}
      {@render crumbs([
        { name: "Builds", href: "#/builds" },
        ...(selected.group !== null ? [{ name: selected.group, href: "#/builds" }] : []),
        { name: selected.name },
      ])}
      <TfPipeline pipeline={selected} {rest} {job} runs={ports.runs} onpin={() => togglePin(selected!)} rereadRow={rereadPipelines} />
    {:else if all !== null && all.length > 0}
      {@render crumbs([{ name: "Builds" }])}
      <div class="tf-empty"><p>No pipeline at this address. Pick one on the left.</p></div>
    {:else}
      {@render crumbs([{ name: "Builds" }])}
    {/if}
  </section>
</div>

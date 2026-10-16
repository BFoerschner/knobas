<!--
  Builds (#848, R-9.1, R-9.4a, ADR-0022): the tracer of the Builds frame.
  The shell draws it over the declared pipeline and run kinds, so nothing
  here names an adapter: the source's word is its configured name, drawn
  once, as the header line over its pipelines.

  - **Left**, the pipeline list, flat (the project tree is #849, *This job*
    #850): the filter field (`/` focuses it, `Esc` clears it through the
    shell's ladder, which finds it by `data-filter`) and the *Failing* and
    *Running* chips; then each pipeline by name, in err tone when its latest
    run failed, the time of that run, and a strip of the last twelve runs
    newest right. A hover gives its group and name. The selected one is
    drawn inverted and kept in view.
  - **Right**, the selected pipeline's runs as a plain list, newest first.
    The pipeline page (header, chart, day groups, filter bar) is #851.

  `#/builds` selects the first failing pipeline, else the first;
  `#/builds/<entity id>` selects that one. `#/build/<pipeline>/<run>` is the
  run page (#853), not built yet, and says so.
-->
<script lang="ts" module>
  import type { PipelineRow, RunRow } from "../../ipc/entity";

  /** The reads this page makes, injectable so a test needs no bridge. */
  export interface BuildsPorts {
    pipelines: () => Promise<PipelineRow[]>;
    runs: (pipelineId: string) => Promise<RunRow[]>;
  }
</script>

<script lang="ts">
  import { onMount, tick } from "svelte";

  import { pipelineRuns, pipelines as readPipelines } from "../../ipc/entity";
  import { inTextField } from "../../shell/keys";
  import { sessionFilter } from "../filter/filter-state.svelte";
  import Marked from "../filter/Marked.svelte";
  import { jobHash } from "../route";
  import {
    CHIPS,
    duration,
    isFailing,
    keptPipelines,
    latestTime,
    pipelineHash,
    pipelineOf,
    runHash,
    runTone,
    shortTime,
    stripOf,
  } from "./builds";

  let {
    ready,
    head,
    rest,
    job,
    ports = { pipelines: readPipelines, runs: (id) => pipelineRuns(id) },
  }: {
    /** Whether the database answers yet: the reads wait for it. */
    ready: boolean;
    /** `builds` for the list, `build` for a run's address. */
    head: string;
    /** What follows the head in the address. */
    rest: string[];
    /** The job the reader was last in, for the breadcrumb. */
    job: string | null;
    ports?: BuildsPorts;
  } = $props();

  const filter = sessionFilter("builds-pipelines");
  const chips = $derived(filter.chips["state"] ?? []);

  let all = $state<PipelineRow[] | null>(null);
  let failed = $state<string | null>(null);

  $effect(() => {
    if (!ready || all !== null) return;
    ports.pipelines().then(
      (rows) => (all = rows),
      (error: unknown) => (failed = message(error)),
    );
  });

  const kept = $derived(all === null ? [] : keptPipelines(all, filter.text, chips));
  const selected = $derived(all === null || head !== "builds" ? null : pipelineOf(all, rest));
  /** The pipelines by source, in the list's order: one header line each. */
  const bySource = $derived(
    kept.reduce<{ source: string; rows: PipelineRow[] }[]>((groups, row) => {
      const last = groups.at(-1);
      if (last && last.source === row.source_name) last.rows.push(row);
      else groups.push({ source: row.source_name, rows: [row] });
      return groups;
    }, []),
  );

  let runs = $state<RunRow[] | null>(null);
  let runsFailed = $state<string | null>(null);
  $effect(() => {
    const id = selected?.entity_id ?? null;
    runs = null;
    runsFailed = null;
    if (id === null) return;
    ports.runs(id).then(
      (rows) => {
        if (selected?.entity_id === id) runs = rows;
      },
      (error: unknown) => {
        if (selected?.entity_id === id) runsFailed = message(error);
      },
    );
  });

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
    void tick().then(() => list?.querySelector('[aria-current="true"]')?.scrollIntoView?.({ block: "nearest" }));
  });

  onMount(() => {
    const onkey = (event: KeyboardEvent) => {
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

  function chipTone(run: RunRow): string {
    const tone = run.state?.tone;
    return tone === "ok" || tone === "err" || tone === "warn" || tone === "info" ? `tf-${tone}` : "";
  }

  function took(run: RunRow): string {
    if (run.duration_seconds !== null) return duration(run.duration_seconds);
    const tone = runTone(run);
    return tone === "run" ? "running" : tone === "q" ? "waiting" : "";
  }
</script>

{#snippet crumbs(parts: { name: string; href?: string }[])}
  <nav class="tf-crumbs" aria-label="Breadcrumb">
    {#if job !== null}
      <a class="tf-jobchip tf-card" href={jobHash(job)}>{job}</a><span class="tf-sep">›</span>
    {/if}
    {#each parts as part, at (at)}
      {#if part.href !== undefined}
        <a class="tf-crumb" href={part.href}>{part.name}</a><span class="tf-sep">›</span>
      {:else}
        <strong>{part.name}</strong>
      {/if}
    {/each}
    <span class="tf-esc">{job !== null ? `Back to ${job}` : "Back"} <kbd>Esc</kbd></span>
  </nav>
{/snippet}

<div class="tf-bv">
  <nav class="tf-pl" aria-label="Pipelines">
    <div class="tf-pl-top">
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
    </div>
    <div class="tf-pl-list" bind:this={list}>
      {#if failed !== null}
        <p class="tf-pl-note">Pipelines could not be read: {failed}</p>
      {:else if all !== null && all.length === 0}
        <p class="tf-pl-note">No pipelines yet. A CI source's pipelines show here once it has synced.</p>
      {:else if all !== null && kept.length === 0}
        <p class="tf-pl-note">No pipeline matches.</p>
      {/if}
      {#each bySource as group (group.source)}
        <h3>{group.source}</h3>
        {#each group.rows as pipeline (pipeline.entity_id)}
          {@const strip = stripOf(pipeline)}
          <a
            class="tf-pi"
            href={pipelineHash(pipeline.entity_id)}
            aria-current={pipeline.entity_id === selected?.entity_id}
            title={pipeline.group !== null ? `${pipeline.group} › ${pipeline.name}` : pipeline.name}
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
          </a>
        {/each}
      {/each}
    </div>
  </nav>
  <section class="tf-bmain">
    {#if head === "build"}
      {@render crumbs([{ name: "Builds", href: "#/builds" }, { name: "Run" }])}
      <div class="tf-empty">
        <p>The run page is not built in the new shell yet.</p>
        {#if rest[0] !== undefined}
          <a class="tf-btn" href={pipelineHash(rest[0])}>Back to the pipeline</a>
        {/if}
      </div>
    {:else if selected !== null}
      {@render crumbs([
        { name: "Builds", href: "#/builds" },
        ...(selected.group !== null ? [{ name: selected.group, href: "#/builds" }] : []),
        { name: selected.name },
      ])}
      <header class="tf-fhead">
        <div>
          <h1>{selected.name}</h1>
          {#if selected.group !== null}<div class="tf-sub">In {selected.group}.</div>{/if}
        </div>
      </header>
      <div class="tf-runs">
        {#if runsFailed !== null}
          <p class="tf-dim">Runs could not be read: {runsFailed}</p>
        {:else if runs !== null && runs.length === 0}
          <div class="tf-empty"><p>No runs yet.</p></div>
        {:else if runs !== null}
          <table class="tf-tbl" aria-label="Runs">
            <tbody>
              {#each runs as run (run.entity_id)}
                <tr>
                  <td class="tf-nowrap tf-run-no">
                    <a href={runHash(selected.entity_id, run.entity_id)}><b>{run.number !== null ? `#${run.number}` : "queued"}</b></a>
                    {#if stateText(run) !== ""}<span class={`tf-chip ${chipTone(run)}`}>{stateText(run)}</span>{/if}
                  </td>
                  <td class="tf-mono tf-run-br">{run.branch ?? ""}</td>
                  <td class="tf-nowrap tf-num">{took(run)}</td>
                  <td class="tf-nowrap tf-dim tf-num">{run.started !== null ? shortTime(run.started) : ""}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        {/if}
      </div>
    {:else if all !== null && all.length > 0}
      {@render crumbs([{ name: "Builds" }])}
      <div class="tf-empty"><p>No pipeline at this address. Pick one on the left.</p></div>
    {:else}
      {@render crumbs([{ name: "Builds" }])}
    {/if}
  </section>
</div>

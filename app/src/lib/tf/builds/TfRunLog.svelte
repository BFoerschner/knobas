<!--
  The run page's Log tab (#854, R-9.11 Log): the whole log up to the cap
  (`run_log`), split into its steps.

  - Each step is a fold with its duration and line count; a failed or
    running step opens by itself, and so does one with a find hit or with
    *Errors and warnings only* on. Lines before, between and after the steps
    sit outside every fold.
  - Line numbers, timestamps (toggled), err and warn lines toned.
  - The sticky toolbar: *Find in log* with "N of M" (`Enter` next,
    `Shift+Enter` previous), *Errors and warnings only*, *Wrap lines*,
    *Timestamps*, *Jump to the first error* on a failed run, *Download log*.
  - A step the run page's steps bar asked for (`openStep`, #1003) opens and
    scrolls into view once the log is read.
  - A fold holds its lines only while it is open, so 20,000 lines in
    closed steps cost their summaries.
  - The log is a windowed list (#2211) of its flattened rows (`logRows`):
    each step's summary, the shown lines of an open one, the loose lines.
    Only the rows near the viewport are drawn; find and *Jump to the first
    error* reveal a line before scrolling to it.
-->
<script lang="ts">
  import { flushSync, tick, untrack } from "svelte";

  import { ipcErrorMessage } from "../../ipc";
  import type { LogLine, RunLog } from "../../ipc/entity";
  import { Windowed } from "../../shell/windowed.svelte";
  import { marks } from "../filter/filter";
  import type { RunPorts } from "./pipeline-ports";
  import {
    findCount,
    hitsIn,
    lineShown,
    logBlocks,
    downloadAndSay,
    logLine,
    logRows,
    stepMatch,
    stepOpens,
    stepState,
    stepSummary,
    type StepBlock,
  } from "./run-reads";

  let {
    runId,
    inFlight,
    failed,
    ports,
    openStep = null,
    rereads = 0,
  }: {
    runId: string;
    /** The run has not finished: its last step is running. */
    inFlight: boolean;
    /** The run failed: *Jump to the first error* is offered. */
    failed: boolean;
    ports: Pick<RunPorts, "log" | "download">;
    /** The step to open at, from a click on the steps bar; `seq` tells two clicks on one step apart. */
    openStep?: { step: number; seq: number } | null;
    /** Bumped when the page reads the run again (#1450): the log is read again, the reader's place kept. */
    rereads?: number;
  } = $props();

  let log = $state<RunLog | null>(null);
  let readFailed = $state<string | null>(null);
  let typed = $state("");
  let errorsOnly = $state(false);
  let wrap = $state(false);
  let stamps = $state(true);
  let current = $state(0);
  /** The folds the reader opened or closed, by step index; the rest follow `stepOpens`. */
  let toggled = $state<Record<number, boolean>>({});
  let viewer: HTMLDivElement | undefined = $state();
  let token = 0;

  $effect(() => {
    const id = runId;
    untrack(() => {
      log = null;
      readFailed = null;
      toggled = {};
    });
    read(id);
  });

  /** A read again keeps what is drawn, and a failed one keeps the last log. */
  let first = true;
  $effect(() => {
    void rereads;
    if (first) return void (first = false);
    untrack(() => read(runId));
  });

  function read(id: string) {
    const mine = ++token;
    ports.log(id).then(
      (answer) => {
        if (mine !== token) return;
        log = answer;
        readFailed = null;
      },
      (cause: unknown) => {
        if (mine === token && log === null) readFailed = ipcErrorMessage(cause);
      },
    );
  }

  /** The `seq` of the last step opened from the bar, so a click opens it once. */
  let lastSeq = 0;
  $effect(() => {
    const wanted = openStep;
    if (wanted === null || log === null || wanted.seq === lastSeq) return;
    lastSeq = wanted.seq;
    untrack(() => (toggled[wanted.step] = true));
    void tick().then(() => {
      win.reveal(rows.findIndex((row) => row.kind === "step" && row.block.index === wanted.step));
      flushSync();
      viewer?.querySelector(`details[data-s="${wanted.step}"]`)?.scrollIntoView({ block: "start" });
    });
  });

  const blocks = $derived(log === null ? [] : logBlocks(log));
  const states = $derived(log === null ? [] : log.steps.map((_, i) => stepState(log!, i, inFlight)));
  /** Each shown line's first hit's place among all hits, by line number. */
  const hits = $derived.by(() => {
    const first = new Map<number, number>();
    let total = 0;
    if (log === null || typed.trim() === "") return { first, total };
    for (const line of log.lines) {
      if (!lineShown(line, typed, errorsOnly)) continue;
      const n = hitsIn(line.text, typed);
      if (n > 0) {
        first.set(line.number, total);
        total += n;
      }
    }
    return { first, total };
  });
  const firstError = $derived(log?.lines.find((l) => l.tone === "err") ?? null);

  function isOpen(block: StepBlock): boolean {
    return toggled[block.index] ?? stepOpens(block, states[block.index] ?? "ok", typed, errorsOnly);
  }

  /** What is drawn, flattened: the windowed list's rows. */
  const rows = $derived(logRows(blocks, isOpen, (line) => lineShown(line, typed, errorsOnly)));
  // A line is 12 px at 1.55; a summary and a loose run's edges are measured once drawn.
  const win = new Windowed<number>({ count: () => rows.length, key: (i) => rows[i]!.key, estimate: 18.6 });

  function ontoggle(index: number, event: Event) {
    const open = (event.currentTarget as HTMLDetailsElement).open;
    if (open !== (toggled[index] ?? null)) toggled[index] = open;
  }

  function find(event: Event) {
    typed = (event.currentTarget as HTMLInputElement).value;
    current = 0;
    // A new find opens every step with a match: the reader's folds give way.
    toggled = {};
  }

  /** Opens `line`'s step, draws the line if the window left it out, and scrolls `selector` to the middle. */
  function show(selector: string, line: LogLine) {
    if (line.step !== null) toggled[line.step] = true;
    flushSync();
    win.reveal(rows.findIndex((row) => row.kind === "line" && row.line.number === line.number));
    flushSync();
    viewer?.querySelector(selector)?.scrollIntoView({ block: "center" });
  }

  function jump(step: 1 | -1) {
    if (hits.total === 0 || log === null) return;
    current = stepMatch(current, step, hits.total);
    let line: number | undefined;
    for (const [number, at] of hits.first) if (at <= current) line = number;
    const found = log.lines.find((l) => l.number === line);
    if (found) show("mark.tf-curm", found);
  }

  function keys(event: KeyboardEvent) {
    if (event.key !== "Enter") return;
    event.preventDefault();
    jump(event.shiftKey ? -1 : 1);
  }

  const download = () => downloadAndSay(ports.download, runId, { kind: "log" }, "the log");
</script>

{#snippet logRow(line: LogLine)}
  {@const start = hits.first.get(line.number)}
  {@const runs = start === undefined ? null : marks(line.text, typed)}
  <div
    class="tf-ll"
    class:tf-e={line.tone === "err"}
    class:tf-w={line.tone === "warn"}
    data-n={line.number}
    {@attach win.row(line.number)}
  >
    <span class="tf-no">{line.number}</span><span class="tf-ts">{line.time ?? ""}</span><span
      >{#if runs === null || start === undefined}{line.text || " "}{:else}{#each runs as run, at (at)}{#if run.hit}{@const n =
              start + runs.slice(0, at).filter((r) => r.hit).length}<mark class:tf-curm={n === current}>{run.text}</mark
            >{:else}{run.text}{/if}{/each}{/if}</span
    >
  </div>
{/snippet}

<div class="tf-logbar">
  <label class="tf-fsearch">
    <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true"
      ><circle cx="7" cy="7" r="5" /><path d="M11 11l3.5 3.5" /></svg
    >
    <input placeholder="Find in log" aria-label="Find in log" value={typed} oninput={find} onkeydown={keys} />
    <span class="tf-dim tf-lqn" aria-live="polite">{findCount(typed, hits.total, current)}</span>
  </label>
  <span class="tf-nav2">
    <button type="button" aria-label="Previous match" onclick={() => jump(-1)}>↑</button><button
      type="button"
      aria-label="Next match"
      onclick={() => jump(1)}>↓</button
    >
  </span>
  <div class="tf-fchips">
    <button type="button" aria-pressed={errorsOnly} onclick={() => (errorsOnly = !errorsOnly)}>Errors and warnings only</button>
    <button type="button" aria-pressed={wrap} onclick={() => (wrap = !wrap)}>Wrap lines</button>
    <button type="button" aria-pressed={stamps} onclick={() => (stamps = !stamps)}>Timestamps</button>
  </div>
  <span class="tf-logacts">
    {#if failed && firstError !== null}
      <button type="button" class="tf-btn tf-sm" onclick={() => firstError && show(`.tf-ll[data-n="${firstError.number}"]`, firstError)}
        >Jump to the first error</button
      >
    {/if}
    <button type="button" class="tf-btn tf-sm" onclick={download}>Download log</button>
  </span>
</div>

{#if readFailed !== null}
  <p class="tf-dim">The log could not be read: {readFailed}</p>
{:else if log === null}
  <p class="tf-dim">Reading the log…</p>
{:else if log.total === 0}
  <div class="tf-empty"><p>{inFlight ? "The run has written no log yet." : "This run has no log."}</p></div>
{:else}
  <div class="tf-logv" class:tf-wrap={wrap} class:tf-nots={!stamps} bind:this={viewer} {@attach win.list}>
    <!-- Drawn anew when *Errors and warnings only* turns: dropping 15,000 lines one at a time took 3 s (#2200's class). -->
    {#key errorsOnly}
    {#each win.slice(rows) as row (row.key)}
      {#if row.kind === "line"}
        {@render logRow(row.line)}
      {:else if row.kind === "step"}
        {@const block = row.block}
        {@const st = states[block.index] ?? "ok"}
        <!-- The summary alone: the step's lines are the rows after it, so a long step windows like the rest. -->
        <details
          class:tf-l0={row === rows[0]}
          data-s={block.index}
          open={isOpen(block)}
          ontoggle={(event) => ontoggle(block.index, event)}
          {@attach win.row(row.key)}
        >
          <summary
            ><span class={`tf-dot tf-${st}`}></span>{block.step.name}<span class="tf-d">{stepSummary(block.step, st)}</span></summary
          >
        </details>
      {:else}
        <div class={row.kind === "loose-top" ? "tf-logloose" : "tf-logloose-end"} class:tf-l0={row === rows[0]} {@attach win.row(row.key)}></div>
      {/if}
    {/each}
    {/key}
  </div>
  <p class="tf-dim tf-logfoot">{logLine(log)}</p>
{/if}

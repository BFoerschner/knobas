<!--
  The run page's Tests tab (#855, R-9.11 Tests): the run's tests
  (`run_tests`), failed first, under the filter bar (text, *Result*:
  Failed / Passed, *Module*).

  A failed test opens to its assertion as the source wrote it, *Show in
  log*, and how often it failed in the last 10 runs (`test_history`, read
  when it opens). The first five failed tests start open.
-->
<script lang="ts">
  import { untrack } from "svelte";

  import { ipcErrorMessage } from "../../ipc";
  import type { RunTest, RunTests, TestOutcome } from "../../ipc/entity";
  import FilterBar from "../filter/FilterBar.svelte";
  import Marked from "../filter/Marked.svelte";
  import type { FilterConfig } from "../filter/filter";
  import { sessionFilter } from "../filter/filter-state.svelte";
  import type { RunPorts } from "./pipeline-ports";
  import { RESULT_CHIPS, historyLine, keptTests, modulesOf, testTime } from "./run-reads";

  let {
    runId,
    branch,
    logHref,
    ports,
  }: {
    runId: string;
    /** The run's branch: a history whose failures were all on it says so. */
    branch: string | null;
    /** The run's Log tab. */
    logHref: string;
    ports: Pick<RunPorts, "tests" | "history">;
  } = $props();

  /** How many failed tests start open. */
  const OPEN_FAILED = 5;

  let read = $state<RunTests | null>(null);
  let readFailed = $state<string | null>(null);
  /** Opened or closed by the reader, by test name. */
  let opened = $state<Record<string, boolean>>({});
  /** Per test name: its outcomes, `"reading"`, or the failure. */
  let histories = $state<Record<string, TestOutcome[] | "reading" | { failed: string }>>({});
  let token = 0;

  $effect(() => {
    const id = runId;
    const mine = ++token;
    untrack(() => {
      read = null;
      readFailed = null;
      opened = {};
      histories = {};
    });
    ports.tests(id).then(
      (answer) => {
        if (mine === token) read = answer;
      },
      (cause: unknown) => {
        if (mine === token) readFailed = ipcErrorMessage(cause);
      },
    );
  });

  const tests = $derived(read?.tests ?? []);
  const filter = $derived(sessionFilter(`run-tests:${runId}`));
  const config = $derived<FilterConfig>({
    id: `run-tests:${runId}`,
    placeholder: "Filter tests",
    noun: "tests",
    chips: [{ key: "st", label: "Result", options: RESULT_CHIPS.map((c) => ({ ...c })) }],
    dropdowns: [{ key: "mod", label: "Module", options: modulesOf(tests).map((m) => ({ value: m, label: m })) }],
  });
  const shown = $derived(keptTests(tests, filter.text, filter.chips["st"] ?? [], filter.dropdowns["mod"] ?? null));
  const firstFailed = $derived(new Set(tests.filter((t) => t.status === "failed").slice(0, OPEN_FAILED).map((t) => t.name)));

  function isOpen(test: RunTest): boolean {
    return test.status === "failed" && (opened[test.name] ?? firstFailed.has(test.name));
  }

  // Each open failed test's history, read once.
  $effect(() => {
    for (const test of shown) {
      if (!isOpen(test) || test.id === null) continue;
      const key = test.name;
      if (untrack(() => histories[key]) !== undefined) continue;
      histories[key] = "reading";
      const mine = token;
      ports.history(runId, test.id).then(
        (answer) => {
          if (mine === token) histories[key] = answer ?? [];
        },
        (cause: unknown) => {
          if (mine === token) histories[key] = { failed: ipcErrorMessage(cause) };
        },
      );
    }
  });

  function historyText(test: RunTest): string | null {
    if (test.id === null) return null;
    const history = histories[test.name];
    if (history === undefined) return null;
    if (history === "reading") return "…";
    if ("failed" in history) return `Its history could not be read: ${history.failed}`;
    return historyLine(history, branch);
  }
</script>

<FilterBar {config} {filter} shown={read === null ? null : shown.length} total={read === null ? null : tests.length} />

{#if readFailed !== null}
  <p class="tf-dim">The tests could not be read: {readFailed}</p>
{:else if read === null}
  <p class="tf-dim">Reading the tests…</p>
{:else}
  <div class="tf-atree tf-tres">
    {#each shown as test (test.name)}
      {@const open = isOpen(test)}
      {@const time = testTime(test.duration_ms)}
      <div class="tf-trow">
        <span class={`tf-dot ${test.status === "failed" ? "tf-err" : test.status === "passed" ? "tf-ok" : ""}`}></span>
        <span class="tf-nm"
          >{#if test.status === "failed"}<button type="button" aria-label={test.name} aria-expanded={open} onclick={() => (opened[test.name] = !open)}
              ><Marked text={test.name} typed={filter.text} /></button
            >{:else}<Marked text={test.name} typed={filter.text} />{/if}</span
        >
        <span class="tf-dim">{time ?? ""}</span>
        <span
          >{#if test.status === "failed"}<span class="tf-chip tf-err">{test.muted ? "failed, muted" : "failed"}</span
            >{:else}<span class="tf-dim">{test.status}</span>{/if}</span
        >
      </div>
      {#if open}
        {@const line = historyText(test)}
        <div class="tf-tdetail">
          {#if test.details !== null}<pre>{test.details.trimEnd()}</pre>{/if}
          <div class="tf-tacts">
            <a class="tf-btn tf-sm" href={logHref}>Show in log</a>
            {#if line !== null}<span class="tf-dim">{line}</span>{/if}
          </div>
        </div>
      {/if}
    {:else}
      <div class="tf-empty"><p>{tests.length === 0 ? "This run reported no tests." : "No test matches."}</p></div>
    {/each}
  </div>
  {#if read.truncated}
    <p class="tf-dim tf-logfoot">The first {tests.length.toLocaleString("en-US")} tests, failed ones first. The run has more.</p>
  {/if}
{/if}

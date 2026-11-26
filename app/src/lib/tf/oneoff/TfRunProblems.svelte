<!--
  *Problems* (ADR-0060, section 15): what a run's lines said about places in the checkout, a file a group, a row its
  line, its severity as a tone dot and a word, what was said and the rule. A row is a link to its line.
-->
<script lang="ts">
  import { many, problems, type Problem } from "../../render/oneoff";
  import type { Run } from "../../render/oneoff.svelte";

  let { run, onproblem }: { run: Run; onproblem: (problem: Problem) => void } = $props();

  const groups = $derived.by(() => {
    void run.version;
    return problems([...run.found.values()].sort((a, b) => a.n - b.n));
  });
  /** The most files and findings a file drawn: a run with more says how many, and Output holds them all. */
  const FILES = 200;
  const EACH = 500;
  const drawnCount = $derived(groups.slice(0, FILES).reduce((n, g) => n + Math.min(g.list.length, EACH), 0));
  const allCount = $derived(groups.reduce((n, g) => n + g.list.length, 0));
  const tone = (severity: string) => (severity === "error" ? "err" : severity === "warning" ? "warn" : "muted");
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex: a scroller takes focus so the arrows scroll it (arrival focus, design system Dialogs) -->
<div class="tf-oo-probs" tabindex="0" role="region" aria-label="What {run.command.name} found">
  {#each groups.slice(0, FILES) as group (group.path)}
    {@const cut = group.path.lastIndexOf("/") + 1}
    <div class="tf-oo-pf">
      <b><span class="tf-pdir">{group.path.slice(0, cut)}</span>{group.path.slice(cut)}</b>
      <span>{many(group.list.length, "problem", "problems")}</span>
    </div>
    {#each group.list.slice(0, EACH) as problem (problem.n)}
      <button type="button" class="tf-oo-p" aria-current={run.at === problem.n} title="Open {problem.path} at line {problem.line}" onclick={() => onproblem(problem)}>
        <span>{problem.line}{problem.col !== null ? `:${problem.col}` : ""}</span>
        <span class="tf-oo-sev tf-{tone(problem.severity)}"><span class="tf-dot {tone(problem.severity) === 'muted' ? '' : `tf-${tone(problem.severity)}`}"></span>{problem.severity || "said"}</span>
        <span class="tf-oo-said">{problem.said}</span>
        <span class="tf-oo-rule" title={problem.rule}>{problem.rule}</span>
      </button>
    {/each}
  {/each}
  {#if allCount > drawnCount}
    <p class="tf-oo-pf tf-dim">{many(allCount - drawnCount, "more problem", "more problems")}: Output holds every line.</p>
  {/if}
</div>

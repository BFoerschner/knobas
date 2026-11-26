<!--
  The run Overview's *Chain on <branch>* panel (#857, R-9.13; prototype-6's
  `chainPanel`): one box per step of the run's chain with that step's run on
  the branch, the run on screen outlined. A box opens its run, or its pipeline
  where the step has not run on the branch. Read live (`run_chain`); drawn
  only for a run in a chain, and nothing while the read is out or failed.

  A chain of more than seven steps folds its middle (the build-header pass, 2026-11-20: seventy boxes over ten rows):
  the first step, the run's own with two either side, and the last, each gap a *… N more* a pointer presses to show
  every step. The keyboard's way is the header's *Show all 70 steps*, which turns into *Show fewer* and keeps focus, so a
  second Enter folds again rather than opening a step. A step's long name ends in an ellipsis, whole in its title.
-->
<script lang="ts">
  import { runChain, type ChainStepRow } from "../../ipc/entity";
  import { CHAIN_FOLD_OVER, chainBox, chainWindow } from "./chain";

  let {
    runId,
    branch,
    read = runChain,
    rereads = 0,
  }: {
    runId: string;
    /** Bumped when the page reads the run again (#1450): the chain is read again over what it drew. */
    rereads?: number;
    /** The run's branch, the panel's heading. */
    branch: string | null;
    read?: (runId: string) => Promise<ChainStepRow[]>;
  } = $props();

  let steps = $state<ChainStepRow[]>([]);
  let drawn: string | null = null;

  $effect(() => {
    const wanted = runId;
    void rereads;
    // A new run starts empty; the same run read again keeps its boxes until the answer.
    if (drawn !== wanted) steps = [];
    drawn = wanted;
    read(wanted).then(
      (answer) => {
        if (wanted === runId) steps = answer;
      },
      () => {},
    );
  });

  const boxes = $derived(steps.map((step) => chainBox(step, runId)));
  /** Every step shown: a press on a fold or the header's toggle. A new run folds again. */
  let unfolded = $state<string | null>(null);
  const folds = $derived(boxes.length > CHAIN_FOLD_OVER);
  const open = $derived(unfolded === runId);
  const shown = $derived(open ? boxes.map((_, i) => i) : chainWindow(boxes.length, boxes.findIndex((box) => box.here)));
  /** How many steps a fold at `at` in `shown` stands for. */
  function folded(at: number): number {
    return (shown[at + 1] ?? boxes.length) - (shown[at - 1] ?? -1) - 1;
  }
</script>

{#if boxes.length > 1}
  <section class="tf-panel tf-headed tf-chain" aria-label="Chain">
    <header>
      <h3>Chain on {#if branch !== null}<span class="tf-mono">{branch}</span>{:else}the default branch{/if}</h3>
      {#if folds}<button type="button" class="tf-open" aria-expanded={open} onclick={() => (unfolded = open ? null : runId)}
          ><span class="tf-keepw" data-keep="Show all {boxes.length} steps"><span>{open ? "Show fewer" : `Show all ${boxes.length} steps`}</span></span></button
        >{/if}
    </header>
    <div class="tf-chainrow">
      {#each shown as index, i (index ?? `fold-${i}`)}
        {#if i > 0}<span class="tf-arrow" aria-hidden="true">›</span>{/if}
        {#if index === null}
          <!-- A pointer's shortcut: the keyboard's is the header's toggle, so the row's Tab stops are its steps. -->
          <button type="button" class="tf-chainfold" tabindex="-1" title="Show every step of the chain" onclick={() => (unfolded = runId)}
            >… {folded(i)} more</button
          >
        {:else}
          {@const box = boxes[index]!}
          <a class="tf-link tf-{box.tone}" href={box.href} title={box.name} aria-current={box.here ? "page" : undefined}
            ><b>{box.name}</b><span>{box.line}</span></a
          >
        {/if}
      {/each}
    </div>
    <p class="tf-dim">Each step starts when the one before it passes. Run… on any step runs the steps before it too.</p>
  </section>
{/if}

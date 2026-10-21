<!--
  The run Overview's *Chain on <branch>* panel (#857, R-9.13; prototype-6's
  `chainPanel`): one box per step of the run's chain with that step's run on
  the branch, the run on screen outlined. A box opens its run, or its pipeline
  where the step has not run on the branch. Read live (`run_chain`); drawn
  only for a run in a chain, and nothing while the read is out or failed.
-->
<script lang="ts">
  import { runChain, type ChainStepRow } from "../../ipc/entity";
  import { chainBox } from "./chain";

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
</script>

{#if boxes.length > 1}
  <section class="tf-panel tf-chain" aria-label="Chain">
    <header><h3>Chain on {#if branch !== null}<span class="tf-mono">{branch}</span>{:else}the default branch{/if}</h3></header>
    <div class="tf-chainrow">
      {#each boxes as box, i (i)}
        {#if i > 0}<span class="tf-arrow" aria-hidden="true">›</span>{/if}
        <a class="tf-link tf-{box.tone}" href={box.href} aria-current={box.here ? "page" : undefined}
          ><b>{box.name}</b><span>{box.line}</span></a
        >
      {/each}
    </div>
    <p class="tf-dim">Each step starts when the one before it passes. Run… on any step runs the steps before it too.</p>
  </section>
{/if}

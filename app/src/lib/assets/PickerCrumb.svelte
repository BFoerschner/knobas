<!--
  Where a picker is standing, in *Move…* and *Add a route…* (F-526, F-543, F-546).

  One line whatever the depth. *Estate* and the last two levels stay whole, and with
  more above them the middle folds into one plain `…`, the whole path in its title:
  `←` walks up, so the fold is no menu. Three levels draw all three, because a `…` for
  one name says less than the name, and that one gives way first if room runs out.
-->
<script lang="ts">
  import type { AssetRow } from "../ipc/assets";

  let {
    crumb,
    onstand,
  }: {
    /** The levels walked into, outermost first. Empty at the top of the estate. */
    crumb: AssetRow[];
    /** Stand at the foot of this path: the top of the estate when it is empty. */
    onstand: (path: AssetRow[]) => void;
  } = $props();

  const folded = $derived(crumb.length > 3 ? crumb.slice(0, -2) : []);
</script>

<p class="crumb">
  <button class="link top" title="Estate" disabled={crumb.length === 0} onclick={() => onstand([])}>
    Estate
  </button>
  {#if folded.length > 0}
    <span class="sep" aria-hidden="true">/</span>
    <span class="fold" title={folded.map((step) => step.name).join(" / ")}>…</span>
  {/if}
  {#each crumb.slice(folded.length) as step, index (step.id)}
    {@const at = folded.length + index}
    <span class="sep" aria-hidden="true">/</span>
    <button
      class="link"
      class:gives={at < crumb.length - 2}
      title={step.name}
      disabled={at === crumb.length - 1}
      onclick={() => onstand(crumb.slice(0, at + 1))}
    >
      {step.name}
    </button>
  {/each}
</p>

<style>
  .crumb {
    display: flex;
    align-items: baseline;
    gap: 4px;
    margin: 0;
    font-size: var(--t-xs);
    color: var(--muted);
  }

  /* Whole unless two very long names can't both fit: then they share what is left and keep their titles. */
  .crumb button {
    flex: 0 1 auto;
    min-width: 2ch;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .crumb .gives {
    flex-shrink: 100;
  }

  .crumb .top,
  .crumb .sep,
  .crumb .fold {
    flex: none;
  }

  .crumb button:disabled {
    color: var(--text);
    cursor: default;
  }
</style>

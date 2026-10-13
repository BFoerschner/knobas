<!--
  Where a tree or a file is (#711): the steps from the tree's top, each one a
  section the source named that opens in the same tab, the last one the place
  the reader is. `onback` adds *Back*, to wherever this tab was before; the
  tab passes it only when it has somewhere to go back to.

  Mono and quiet, like a path in a terminal prompt: slashes between, the
  current step in the text colour, the rest muted until hovered.
-->
<script lang="ts">
  import type { Crumb } from "../../ipc/entity";

  let {
    trail,
    onopen,
    onback,
  }: {
    trail: Crumb[];
    /** Open a step's section in this tab. */
    onopen: (section: string) => void;
    /** Go back to where this tab was before. Omitted: no *Back*. */
    onback?: (() => void) | undefined;
  } = $props();
</script>

<div class="tr">
  {#if onback}
    <button class="btn ghost sm tr-back" aria-label="Back" title="Back in this tab" onclick={onback}>
      <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M10 4l-4 4 4 4" /></svg>
    </button>
  {/if}
  <nav aria-label="Path">
    <ol>
      {#each trail as crumb, index (index)}
        <li>
          {#if index > 0}<span class="tr-sep" aria-hidden="true">/</span>{/if}
          {#if index === trail.length - 1}
            <span class="tr-here" aria-current="location">{crumb.name}</span>
          {:else}
            <button class="tr-step" onclick={() => onopen(crumb.section)}>{crumb.name}</button>
          {/if}
        </li>
      {/each}
    </ol>
  </nav>
</div>

<style>
  .tr {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }

  .tr-back svg {
    width: 12px;
    height: 12px;
    stroke: currentColor;
    fill: none;
    stroke-width: 1.5;
  }

  nav {
    min-width: 0;
  }

  ol {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    list-style: none;
    font: 400 12px var(--mono);
  }

  li {
    display: flex;
    align-items: center;
    min-width: 0;
  }

  .tr-sep {
    padding: 0 4px;
    color: var(--faint);
  }

  .tr-step {
    color: var(--muted);
    overflow-wrap: anywhere;
    text-align: left;
  }

  .tr-step:hover {
    color: var(--text);
    text-decoration: underline;
    text-underline-offset: 2px;
  }

  .tr-here {
    color: var(--text);
    font-weight: 500;
    overflow-wrap: anywhere;
  }
</style>

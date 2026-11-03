<!--
  The `?` card (`.tf-ln-help` in `tf/css/launcher.css`).

  Generated from `syntax.json`, which mirrors the token table in
  `crates/knobas-search/src/query.rs` — and the mirroring is *enforced*, not
  promised: `crates/knobas-search/tests/it/help_card.rs` runs every row's probe
  through the real parser and fails if the card advertises a filter the grammar
  greys out, greys out one the grammar honours, or leaves a prefix of the
  grammar undocumented.

  Each row is a button that inserts itself, so the card is also how the syntax
  is learned by using it. The card ends with the colour legend (#2421): what
  a monogram's colour says, per kind, from the merged state colours the
  launcher reads (#2467).
-->
<script lang="ts">
  import { kindRegistry } from "../shell/kind-registry.svelte";
  import { kindLabel, kindMonogram } from "../shell/kinds";
  import type { LegendLine } from "./legend";
  import type { LauncherRow } from "./rows";

  let {
    rows,
    selected,
    heading = "Prefix syntax",
    keys = [],
    legend = [],
    optionId,
    onopen,
    onhover,
  }: {
    rows: LauncherRow[];
    selected: number;
    /** The card's heading: the typed `?` card's, or the ⌘? overlay's. */
    heading?: string;
    /** The keys the box answers, listed at the bottom of the card (#2424). */
    keys?: [string, string][];
    /** The colour legend (#2421, #2467): the merged state colours, per kind, as the launcher read them. */
    legend?: LegendLine[];
    /** The element id of the option at a flat-list index (#1411). */
    optionId: (index: number) => string;
    onopen: (row: LauncherRow) => void;
    onhover: (index: number) => void;
  } = $props();
</script>

<div class="tf-ln-h"><span class="tf-ln-lab">{heading}</span></div>
<div class="tf-ln-help">
  {#each rows as row, i (row.id)}
    {#if row.kind === "syntax"}
    {@const entry = row.entry}
    <button
      id={optionId(i)}
      class="tf-ln-opt tf-ln-syn"
      class:tf-ln-on={i === selected}
      class:tf-ln-greyed={entry.expect === "reported"}
      role="option"
      aria-selected={i === selected}
      onclick={() => onopen(row)}
      onmouseenter={() => onhover(i)}
    >
      <span class="tf-ln-tok">{entry.token}</span>
      <span class="tf-ln-s">{entry.summary}</span>
    </button>
    {/if}
  {/each}
</div>
{#if keys.length > 0}
  <div class="tf-ln-keys" aria-label="Keys">
    {#each keys as [key, what] (key)}
      <span><kbd>{key}</kbd> {what}</span>
    {/each}
  </div>
{/if}

{#if legend.length > 0}
  <div class="tf-ln-h"><span class="tf-ln-lab">Colours</span></div>
  <div class="tf-ln-legend" role="list" aria-label="Colours">
    {#each legend as line (line.kind)}
      {@const info = kindRegistry.info(line.kind)}
      <div class="tf-ln-lg" role="listitem">
        <span class="tf-ln-lgk">{kindLabel(line.kind, info)}</span>
        {#each line.tones as entry (entry.tone)}
          <span class="tf-ln-lgt"
            ><span class="tf-ln-mg" data-tone={entry.tone} aria-hidden="true">{kindMonogram(line.kind, info)}</span><span
              >{entry.words.join(", ")}</span
            ></span
          >
        {/each}
        {#if line.rest !== null}<span class="tf-ln-lgt">{line.rest}</span>{/if}
      </div>
    {/each}
  </div>
{/if}

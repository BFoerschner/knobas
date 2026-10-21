<!--
  The `?` card (`.tf-ln-help` in `tf/css/launcher.css`).

  Generated from `syntax.json`, which mirrors the token table in
  `crates/knobas-search/src/query.rs` — and the mirroring is *enforced*, not
  promised: `crates/knobas-search/tests/it/help_card.rs` runs every row's probe
  through the real parser and fails if the card advertises a filter the grammar
  greys out, greys out one the grammar honours, or leaves a prefix of the
  grammar undocumented.

  Each row is a button that inserts itself, so the card is also how the syntax
  is learned by using it.
-->
<script lang="ts">
  import type { LauncherRow } from "./rows";

  let {
    rows,
    selected,
    optionId,
    onopen,
    onhover,
  }: {
    rows: LauncherRow[];
    selected: number;
    /** The element id of the option at a flat-list index (#1411). */
    optionId: (index: number) => string;
    onopen: (row: LauncherRow) => void;
    onhover: (index: number) => void;
  } = $props();
</script>

<div class="tf-ln-h"><span class="tf-ln-lab">Prefix syntax</span><span class="tf-ln-n">{rows.length}</span></div>
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

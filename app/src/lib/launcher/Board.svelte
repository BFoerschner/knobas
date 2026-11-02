<!--
  What an empty box shows (#2421, spec #2419): the recent items, nothing else.
  No smart lists, no inbox block, no time block, no source strip. Favourites
  join above the recents under their own heading (#2428).

  The recent items are drawn by the same `Row` the results use, so a row reads
  the same on the board as in an answer.
-->
<script lang="ts">
  import type { LauncherHome } from "../ipc";
  import { kindMonogram, kindSingular } from "../shell/kinds";
  import { kindRegistry } from "../shell/kind-registry.svelte";
  import { cellsOf } from "./cells";
  import Row from "./Row.svelte";
  import type { LauncherRow } from "./rows";

  let {
    home,
    rows,
    selected,
    optionId,
    onopen,
    onhover,
  }: {
    home: LauncherHome;
    rows: LauncherRow[];
    selected: number;
    /** The element id of the option at a flat-list index (#1411). */
    optionId: (index: number) => string;
    onopen: (row: LauncherRow) => void;
    onhover: (index: number) => void;
  } = $props();
</script>

{#if home.recent.length > 0}
  <div class="tf-ln-h"><span class="tf-ln-lab">Recent</span></div>
{/if}
{#each rows as row, index (row.id)}
  {#if row.kind === "recent"}
    {@const item = row.row}
    {@const info = kindRegistry.info(item.kind)}
    {@const word = kindSingular(item.kind, info)}
    {@const cells = cellsOf({ kind: item.kind, entityId: item.entity_id, title: item.title, path: item.path, cell: item.cell, kindWord: word })}
    <Row
      monogram={kindMonogram(item.kind, info)}
      tone={item.state?.tone ?? null}
      kindName={item.state ? `${word}: ${item.state.text}` : word}
      cell={cells.cell}
      mono={cells.mono}
      title={cells.title}
      id={optionId(index)}
      selected={index === selected}
      onopen={() => onopen(row)}
      onhover={() => onhover(index)}
    />
  {/if}
{/each}

{#if home.recent.length === 0}
  <p class="tf-ln-none">Nothing has been synced yet. Add a source from the gear, or start knobas with <strong>--demo</strong>.</p>
{/if}

<!--
  What an empty box shows (#2421, spec #2419): the favourites under one
  heading, lists and entities in the order they were favourited (#2428), then
  the recent items. No smart lists, no inbox block, no time block, no source
  strip.

  Both blocks are drawn by the same `Row` the results use, so a row reads
  the same on the board as in an answer.
-->
<script lang="ts">
  import type { LauncherHome } from "../ipc";
  import { kindMonogram, kindSingular } from "../shell/kinds";
  import { kindRegistry } from "../shell/kind-registry.svelte";
  import { stateWord } from "../tf/state-word";
  import { cellsOf } from "./cells";
  import Row from "./Row.svelte";
  import type { LauncherRow } from "./rows";

  let {
    home,
    rows,
    selected,
    optionId,
    chordOf = () => null,
    sourceOf = () => null,
    onopen,
    onhover,
  }: {
    home: LauncherHome;
    rows: LauncherRow[];
    selected: number;
    /** The element id of the option at a flat-list index (#1411). */
    optionId: (index: number) => string;
    /** The chord bound to a row (#2429), drawn at its end; none when omitted. */
    chordOf?: (row: LauncherRow) => string | null;
    /** The source a row names, where its key repeats in the list (#1476); none when omitted. */
    sourceOf?: (row: LauncherRow) => string | null;
    onopen: (row: LauncherRow) => void;
    onhover: (index: number) => void;
  } = $props();
</script>

{#each rows as row, index (row.id)}
  {#if row.kind === "favourite" || row.kind === "recent"}
    {@const item = row.row}
    {@const info = kindRegistry.info(item.kind)}
    {@const word = kindSingular(item.kind, info)}
    {@const cells = cellsOf(item, word)}
    {#if index === 0 && row.kind === "favourite"}
      <div class="tf-ln-h"><span class="tf-ln-lab">Favourites</span></div>
    {/if}
    {#if row.kind === "recent" && rows[index - 1]?.kind !== "recent"}
      <div class="tf-ln-h"><span class="tf-ln-lab">Recent</span></div>
    {/if}
    <Row
      monogram={kindMonogram(item.kind, info)}
      tone={item.state?.tone ?? null}
      kindName={item.state ? `${word}: ${stateWord(item.state.text)}` : word}
      cell={cells.cell}
      mono={cells.mono}
      title={cells.title}
      chord={chordOf(row)}
      source={sourceOf(row)}
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

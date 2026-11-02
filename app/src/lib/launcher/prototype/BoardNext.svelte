<!--
  PROTOTYPE: the board as rounds 4 and 6 want it (Björn, 2026-11-02): the
  favourite lists, the favourites, the recent items, and nothing else. `launcher/Board.svelte` draws the smart lists, the
  inbox, today's time and the source strip too; every one of them is gone here.
-->
<script lang="ts">
  import type { LauncherHome } from "../../ipc";
  import type { CredentialHealth } from "../../ipc/sources";
  import { kindMonogram, kindSingular } from "../../shell/kinds";
  import type { LauncherRow } from "../rows";
  import { cellOf } from "./cells";
  import RowNext from "./RowNext.svelte";

  let {
    home,
    favourites = 0,
    sources,
    rows,
    selected,
    optionId,
    now,
    onopen,
    onhover,
  }: {
    home: LauncherHome;
    /** How many of `home.recent` are favourites, drawn under their own heading first. */
    favourites?: number;
    sources: CredentialHealth[];
    rows: LauncherRow[];
    selected: number;
    optionId: (index: number) => string;
    now?: Date | undefined;
    onopen: (row: LauncherRow) => void;
    onhover: (index: number) => void;
  } = $props();

  const recentFrom = $derived(Math.max(0, rows.findIndex((row) => row.kind === "recent")));
  // svelte-ignore state_referenced_locally
  void sources;
  // svelte-ignore state_referenced_locally
  void now;
</script>

{#if home.smart_lists.length + favourites > 0}
  <div class="tf-ln-h"><span class="tf-ln-lab">Favourites</span></div>
{/if}
{#each home.smart_lists as list, i (list.id)}
  {@const row = rows[i]}
  <RowNext
    monogram="LI"
    tone={list.needs_attention ? "err" : null}
    kindName={list.needs_attention ? "Saved list: needs attention" : "Saved list"}
    cell="list"
    title={list.label}
    id={optionId(i)}
    selected={i === selected}
    onopen={() => row && onopen(row)}
    onhover={() => onhover(i)}
  />
{/each}
{#each home.recent as item, i (item.entity_id)}
  {@const index = recentFrom + i}
  {@const row = rows[index]}
  {#if i === favourites}
    <div class="tf-ln-h"><span class="tf-ln-lab">Recent</span></div>
  {/if}
  {#if row}
    {@const cell = cellOf(item.kind, item.entity_id, kindSingular(item.kind))}
    <RowNext
      monogram={kindMonogram(item.kind)}
      tone={item.state?.tone ?? null}
      kindName={item.state ? `${kindSingular(item.kind)}: ${item.state.text}` : kindSingular(item.kind)}
      cell={cell.cell}
      mono={cell.mono}
      title={item.title}
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

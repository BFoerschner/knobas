<!--
  PROTOTYPE: the board as rounds 4 and 6 want it (Björn, 2026-11-02): the
  favourite lists, the favourites, the recent items, and nothing else. `launcher/Board.svelte` draws the smart lists, the
  inbox, today's time and the source strip too; every one of them is gone here.
-->
<script lang="ts">
  import type { LauncherHome } from "../../ipc";
  import type { CredentialHealth } from "../../ipc/sources";
  import { sharesKey } from "../format";
  import Row from "../Row.svelte";
  import type { LauncherRow } from "../rows";

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
  const shares = $derived(sharesKey(home.recent));
</script>

{#if home.smart_lists.length + favourites > 0}
  <div class="tf-ln-h"><span class="tf-ln-lab">Favourites</span><span class="tf-ln-n">{home.smart_lists.length + favourites}</span></div>
{/if}
{#each home.smart_lists as list, i (list.id)}
  {@const row = rows[i]}
  <div class="tf-ln-slrow" class:tf-ln-on={i === selected} role="presentation" onmouseenter={() => onhover(i)}>
    <button id={optionId(i)} class="tf-ln-sl" role="option" aria-selected={i === selected} disabled={list.needs_attention} onclick={() => row && onopen(row)}>
      <span class="tf-ln-nm">
        {list.label}
        <small class:tf-ln-att={list.needs_attention}>{list.description}</small>
      </span>
      <span class="tf-ln-c" class:tf-ln-chg={list.changed}>
        {#if list.needs_attention}<span class="tf-ln-att">Needs attention</span>{:else}{list.count}{/if}
      </span>
    </button>
  </div>
{/each}
{#each home.recent as item, i (item.entity_id)}
  {@const index = recentFrom + i}
  {@const row = rows[index]}
  {#if i === favourites}
    <div class="tf-ln-h"><span class="tf-ln-lab">Recent</span><span class="tf-ln-n">{home.recent.length - favourites}</span></div>
  {/if}
  {#if row}
    <Row
      entityId={item.entity_id}
      kind={item.kind}
      sourceId={item.source_id}
      title={item.title}
      syncedAt={item.synced_at}
      path={item.path}
      state={item.state}
      named={shares(item)}
      {sources}
      {now}
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

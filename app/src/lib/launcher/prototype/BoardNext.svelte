<!--
  PROTOTYPE: the board as round 4 wants it (Björn, 2026-11-02): the recent
  items and nothing else. `launcher/Board.svelte` draws the smart lists, the
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
    sources,
    rows,
    selected,
    optionId,
    now,
    onopen,
    onhover,
  }: {
    home: LauncherHome;
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

<div class="tf-ln-h"><span class="tf-ln-lab">Recent</span><span class="tf-ln-n">{home.recent.length}</span></div>
{#each home.recent as item, i (item.entity_id)}
  {@const index = recentFrom + i}
  {@const row = rows[index]}
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

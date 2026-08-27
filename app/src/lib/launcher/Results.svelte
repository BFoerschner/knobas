<!--
  The grouped answer (spec §4: "results grouped by type with source monogram
  and sync age").

  The group order is the **backend's** — `knobas_search::group` fixes it
  (tickets, PRs, builds, …, unknown kinds last), so a launcher that sorted its
  own groups would be a second opinion about the same question. Same for
  `label`, `plural` and `monogram`: they come off the group, which the engine
  filled from the adapter's declared `KindInfo` (§3a), never from a table here.
-->
<script lang="ts">
  import type { SearchResponse } from "../ipc";
  import type { CredentialHealth } from "../ipc/sources";
  import Row from "./Row.svelte";
  import type { LauncherRow } from "./rows";

  let {
    response,
    rows,
    selected,
    sources,
    now,
    onopen,
    onhover,
  }: {
    response: SearchResponse;
    /** The flat selectable list, so an index means the same thing everywhere. */
    rows: LauncherRow[];
    selected: number;
    sources: CredentialHealth[];
    now?: Date | undefined;
    onopen: (row: LauncherRow) => void;
    onhover: (index: number) => void;
  } = $props();

  /** Where each group's hits start in the flat list. */
  const offsets = $derived.by(() => {
    const out: number[] = [];
    let at = 0;
    for (const group of response.groups) {
      out.push(at);
      at += group.hits.length;
    }
    return out;
  });
</script>

{#each response.groups as group, g (group.kind)}
  <div class="secl">
    <span class="lab">{group.plural}</span>
    <span class="n">
      {group.total}
      {#if group.total > group.hits.length}
        <span class="more">· {group.total - group.hits.length} more</span>
      {/if}
    </span>
  </div>
  {#each group.hits as hit, h (hit.entity_id)}
    {@const index = (offsets[g] ?? 0) + h}
    {@const row = rows[index]}
    {#if row}
      <Row
        entityId={hit.entity_id}
        kind={hit.kind}
        sourceId={hit.source_id}
        title={hit.title}
        syncedAt={hit.synced_at}
        snippet={hit.snippet}
        {sources}
        {now}
        selected={index === selected}
        onopen={() => onopen(row)}
        onhover={() => onhover(index)}
      />
    {/if}
  {/each}
{/each}

{#if response.groups.length === 0}
  <p class="none">
    Nothing in the local index matches
    <span class="mono">{response.interpreted.text || "these filters"}</span>.
  </p>
{/if}

<style>
  /* `.secl` — the sticky group heading (round 3, line 411). */
  .secl {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 24px;
    padding: 0 12px 0 8px;
    border-bottom: 1px solid var(--hair);
    background: var(--panel);
    position: sticky;
    top: 0;
    z-index: 1;
  }
  .n {
    margin-left: auto;
    font: 400 11px var(--mono);
    color: var(--faint);
  }
  /* §4 asks for the count *and* what the page left behind: a group of 400
     showing 10 has to say so, or the reader thinks there are ten. */
  .more {
    color: var(--muted);
  }
  .none {
    padding: 14px 12px;
    color: var(--muted);
    font-size: 12px;
    line-height: 1.5;
  }
</style>

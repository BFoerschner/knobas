<!--
  PROTOTYPE: the grouped answer with `RowNext` rows and no counts (round 9).
  `launcher/Results.svelte` is the shipped one; the group order and words
  are still the backend's, the cells are decided here per kind.
-->
<script lang="ts">
  import type { SearchResponse } from "../../ipc";
  import type { LauncherRow } from "../rows";
  import { cellOf } from "./cells";
  import RowNext from "./RowNext.svelte";

  let {
    response,
    rows,
    selected,
    optionId,
    onopen,
    onhover,
  }: {
    response: SearchResponse;
    rows: LauncherRow[];
    selected: number;
    optionId: (index: number) => string;
    onopen: (row: LauncherRow) => void;
    onhover: (index: number) => void;
  } = $props();

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
  <div class="tf-ln-h"><span class="tf-ln-lab">{group.plural}</span></div>
  {#each group.hits as hit, h (hit.entity_id)}
    {@const index = (offsets[g] ?? 0) + h}
    {@const row = rows[index]}
    {@const cell = cellOf(hit.kind, hit.entity_id, hit.title, hit.path, group.label)}
    {#if row}
      <RowNext
        monogram={group.monogram}
        tone={hit.state?.tone ?? null}
        kindName={hit.state ? `${group.label}: ${hit.state.text}` : group.label}
        cell={cell.cell}
        mono={cell.mono}
        title={cell.title}
        id={optionId(index)}
        selected={index === selected}
        onopen={() => onopen(row)}
        onhover={() => onhover(index)}
      />
    {/if}
  {/each}
{/each}
{#if response.groups.length === 0}
  <p class="tf-ln-none">Nothing in the mirror matches <strong>{response.interpreted.text || "these filters"}</strong>.</p>
{/if}

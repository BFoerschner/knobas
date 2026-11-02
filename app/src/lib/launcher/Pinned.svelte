<!--
  The row the Tab sheet acts on (#2426), lifted under the box: drawn by the
  one row component the results and the board use, with the same cells, so
  it reads as the row it was. Not selectable: the arrows walk the actions.
-->
<script lang="ts">
  import { kindRegistry } from "../shell/kind-registry.svelte";
  import { kindMonogram, kindSingular } from "../shell/kinds";
  import { clockLabel } from "../time/format";
  import Row from "./Row.svelte";
  import { cellsOf } from "./cells";
  import type { LauncherRow } from "./rows";

  let { row, id, now }: { row: LauncherRow; id: string; now?: Date | undefined } = $props();

  const face = $derived.by(() => {
    if (row.kind === "inbox") {
      return { monogram: "IN", tone: null, kindName: "Inbox line", cell: clockLabel(row.item.occurred_at, now ?? new Date(), true), mono: true, title: row.item.title };
    }
    if (row.kind !== "hit" && row.kind !== "recent") return null;
    const item = row.kind === "hit" ? row.hit : row.row;
    const info = row.kind === "recent" ? kindRegistry.info(item.kind) : null;
    const word = row.kind === "hit" ? row.group.label : kindSingular(item.kind, info);
    const monogram = row.kind === "hit" ? row.group.monogram : kindMonogram(item.kind, info);
    const cells = cellsOf({ kind: item.kind, entityId: item.entity_id, title: item.title, path: item.path, cell: item.cell, kindWord: word });
    return {
      monogram,
      tone: item.state?.tone ?? null,
      kindName: item.state ? `${word}: ${item.state.text}` : word,
      cell: cells.cell,
      mono: cells.mono,
      title: cells.title,
    };
  });
</script>

{#if face}
  <div class="tf-ln-pinned" role="listbox" aria-label="The row the actions act on">
    <Row {...face} {id} selected={false} onopen={() => {}} onhover={() => {}} />
  </div>
{/if}

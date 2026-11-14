<!--
  A table body that draws only the rows near the viewport once it holds `from`
  of them (`shell/windowed.svelte.ts`), with spacer rows for the room of the
  rest: for a table drawn once per group, where each group's rows need a window
  of their own (a pipeline's runs, one table a day). The caller draws each row
  through `row`, putting the attachment it is handed on the row's `<tr>`, so
  the window measures it and holds it still.

  ```svelte
  <TfWindowedRows rows={day.runs} key={(run) => run.entity_id} estimate={37} colspan={7}>
    {#snippet row(run, attach)}<tr {@attach attach}>…</tr>{/snippet}
  </TfWindowedRows>
  ```
-->
<script lang="ts" generics="T">
  import type { Snippet } from "svelte";
  import type { Attachment } from "svelte/attachments";
  import { Windowed } from "../shell/windowed.svelte";

  let {
    rows,
    key,
    estimate,
    colspan,
    from = 40,
    overscan = 600,
    row,
  }: {
    rows: readonly T[];
    /** Row's key: unique, stable while it is the same row. */
    key: (row: T) => string;
    /** A row's height before it is drawn, in pixels. */
    estimate: number;
    /** How many columns a spacer row spans. */
    colspan: number;
    /** A body of fewer rows draws every one. */
    from?: number;
    /** How many pixels beyond the viewport to draw each side. */
    overscan?: number;
    row: Snippet<[T, Attachment<HTMLElement>]>;
  } = $props();

  // svelte-ignore state_referenced_locally
  const win = new Windowed({ count: () => rows.length, key: (i) => key(rows[i]!), estimate, from, overscan });
</script>

<tbody {@attach win.list}>
  {#if win.before > 0}
    <tr class="tf-wgap" aria-hidden="true"><td {colspan} {@attach win.room("before")}></td></tr>
  {/if}
  {#each win.slice(rows) as item (key(item))}
    {@render row(item, win.row(key(item)))}
  {/each}
  {#if win.after > 0}
    <tr class="tf-wgap" aria-hidden="true"><td {colspan} {@attach win.room("after")}></td></tr>
  {/if}
</tbody>

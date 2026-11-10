<!--
  A space's own page, `#/docs/space/<key>`: every page it holds, indented under its parents, with a page that holds
  others heavier and more room above it, so a long list reads as groups. It opens no page and records no visit, so the
  Space picker and a tile's head are filters you can read, not a jump into the first page.
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  import type { DocSpace } from "../../ipc/entity";
  import { counted } from "../counted";
  import { childrenByParent, pageHash, spaceName, spaceSource } from "./docs";
  import { Windowed } from "../../shell/windowed.svelte";

  /** `spaces` is the tree's, so a space whose name another source's space has says its source. */
  let { space, spaces = [], sync }: { space: DocSpace; spaces?: readonly DocSpace[]; sync?: Snippet | undefined } = $props();
  const from = $derived(spaceSource(space, spaces));

  /** The pages in reading order, each with its depth. */
  const rows = $derived.by(() => {
    const under = childrenByParent(space);
    const held = new Set(space.pages.map((page) => page.entity_id));
    const out: { id: string; title: string; depth: number; holds: boolean }[] = [];
    const walk = (parent: string | null, depth: number) => {
      for (const page of under.get(parent) ?? []) {
        out.push({ id: page.entity_id, title: page.title, depth, holds: under.has(page.entity_id) });
        walk(page.entity_id, depth + 1);
      }
    };
    walk(null, 0);
    // A page whose parent the space does not hold still shows, at the top.
    for (const [parent, pages] of under) {
      if (parent === null || held.has(parent)) continue;
      for (const page of pages) {
        out.push({ id: page.entity_id, title: page.title, depth: 0, holds: under.has(page.entity_id) });
        walk(page.entity_id, 1);
      }
    }
    return out;
  });
  /** A space of thousands of pages draws the rows on screen and near it (2026-11-10: 2,250 rows, 4,575 nodes). */
  const win = new Windowed({ count: () => rows.length, key: (i) => rows[i]!.id, estimate: 36 });
</script>

<div class="tf-dh">
  <header class="tf-fhead">
    <div>
      <h1>{spaceName(space)}{#if from !== null}{" "}<span class="tf-dim">on {from}</span>{/if}</h1>
      <div class="tf-sub">{counted(space.pages.length, "page", "pages")}</div>
    </div>
    {#if sync}<div class="tf-acts">{@render sync()}</div>{/if}
  </header>
  <div class="tf-dh-body">
    <section class="tf-panel" aria-label="Pages in {spaceName(space)}{from === null ? '' : ` on ${from}`}">
      <div class="tf-dh-rows" {@attach win.list}>
        {#each win.slice(rows) as row, at (row.id)}
          <a
            class="tf-dh-row tf-dh-rowt tf-dh-d{Math.min(row.depth, 4)}"
            class:tf-dh-holds={row.holds}
            class:tf-dh-later={win.start + at > 0}
            href={pageHash(row.id)}
            {@attach win.row(row.id)}><span class="tf-dh-t">{row.title}</span></a
          >
        {:else}
          <p class="tf-dh-none">This space has no synced pages yet.</p>
        {/each}
      </div>
    </section>
  </div>
</div>

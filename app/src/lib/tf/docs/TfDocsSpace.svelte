<!--
  A space's own page, `#/docs/space/<key>`: every page it holds, indented under its parents. It opens no page and
  records no visit, so the Space picker and a tile's title are filters you can read, not a jump into the first page.
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  import type { DocSpace } from "../../ipc/entity";
  import { counted } from "../counted";
  import { childrenByParent, pageHash } from "./docs";

  let { space, sync }: { space: DocSpace; sync?: Snippet | undefined } = $props();

  /** The pages in reading order, each with its depth. */
  const rows = $derived.by(() => {
    const under = childrenByParent(space);
    const held = new Set(space.pages.map((page) => page.entity_id));
    const out: { id: string; title: string; depth: number }[] = [];
    const walk = (parent: string | null, depth: number) => {
      for (const page of under.get(parent) ?? []) {
        out.push({ id: page.entity_id, title: page.title, depth });
        walk(page.entity_id, depth + 1);
      }
    };
    walk(null, 0);
    // A page whose parent the space does not hold still shows, at the top.
    for (const [parent, pages] of under) {
      if (parent === null || held.has(parent)) continue;
      for (const page of pages) {
        out.push({ id: page.entity_id, title: page.title, depth: 0 });
        walk(page.entity_id, 1);
      }
    }
    return out;
  });
</script>

<div class="tf-dh">
  <header class="tf-fhead">
    <div>
      <h1>{space.name ?? space.key}</h1>
      <div class="tf-sub">{counted(space.pages.length, "page", "pages")}</div>
    </div>
    {#if sync}<div class="tf-acts">{@render sync()}</div>{/if}
  </header>
  <div class="tf-dh-body">
    <section class="tf-panel" aria-label="Pages in {space.name ?? space.key}">
      <div class="tf-dh-rows">
        {#each rows as row (row.id)}
          <a class="tf-dh-row tf-dh-rowt tf-dh-d{Math.min(row.depth, 4)}" href={pageHash(row.id)}><span class="tf-dh-t">{row.title}</span></a>
        {:else}
          <p class="tf-dh-none">This space has no synced pages yet.</p>
        {/each}
      </div>
    </section>
  </div>
</div>

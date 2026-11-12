<!--
  A space's own page, `#/docs/space/<key>`: every page it holds, indented under its parents, with a page that holds
  others heavier and more room above it, so a long list reads as groups. It opens no page and records no visit, so the
  Space picker and a tile's head are filters you can read, not a jump into the first page.

  Drawn before the space's every page is read (the instant-open pass, 2026-11-12, `tree-store.ts`), it shows its header
  at once, from the count the outline holds, and its list once whole, so no row moves; the list's skeleton shows only
  if that read takes longer than {@link SKELETON_AFTER_MS}, where a list read from memory would flash it for a frame.
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  import type { DocSpace } from "../../ipc/entity";
  import { counted } from "../counted";
  import { childrenByParent, pageCount, pageHash, spaceName, spaceSource } from "./docs";
  import { Windowed } from "../../shell/windowed.svelte";

  /** `spaces` is the tree's, so a space whose name another source's space has says its source. */
  let {
    space,
    spaces = [],
    sync,
    whole = true,
  }: {
    space: DocSpace;
    spaces?: readonly DocSpace[];
    sync?: Snippet | undefined;
    /** Whether `space` holds every page of it; until then the header is drawn and the list waits. */
    whole?: boolean;
  } = $props();

  /** How long the list waits for its pages before it draws its skeleton. */
  const SKELETON_AFTER_MS = 150;
  let late = $state(false);
  $effect(() => {
    if (whole) {
      late = false;
      return;
    }
    const timer = setTimeout(() => (late = true), SKELETON_AFTER_MS);
    return () => clearTimeout(timer);
  });
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
      <div class="tf-sub">{counted(pageCount(space), "page", "pages")}</div>
    </div>
    {#if sync}<div class="tf-acts">{@render sync()}</div>{/if}
  </header>
  <div class="tf-dh-body">
    <section class="tf-panel" aria-label="Pages in {spaceName(space)}{from === null ? '' : ` on ${from}`}">
      {#if !whole}
        <div class="tf-dh-rows" aria-busy="true" aria-label="Reading the pages of {spaceName(space)}">
          {#if late}
            <div class="tf-skel tf-skel-lines">{#each [0, 1, 2, 3, 4, 5] as line (line)}<span class="tf-skel-bar"></span>{/each}</div>
          {/if}
        </div>
      {:else}
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
      {/if}
    </section>
  </div>
</div>

<!--
  The page's outline, *On this page* (#861, R-10.15 as amended by #2078): it
  follows the scrolling and counts the find bar's matches per section (#860).

  On a wide window it is the page's right column, sticking while the page
  scrolls (the margin). Below 1240px (`folded`) it is a fold between the
  meta line and the first section, closed, its summary counting the
  sections and, while the find bar is open, the matches in them, a badge
  drawn only above zero (#2081).
-->
<script lang="ts">
  import type { DocPage } from "../../ipc/entity";
  import { inlineWords, pageHash, sectionMatches, sectionsOf } from "./docs";

  let {
    page,
    stems,
    active,
    folded,
  }: {
    page: DocPage;
    stems: string[];
    /** The section the reader is in, by its number. */
    active: number | null;
    /** Drawn as the fold above the page rather than as the margin. */
    folded: boolean;
  } = $props();

  const outline = $derived(sectionsOf(page.blocks).filter((s) => s.heading !== null));
  const counts = $derived(sectionMatches(page.blocks, stems));
  const total = $derived([...counts.values()].reduce((sum, n) => sum + n, 0));
</script>

{#snippet toc()}
  <nav class="tf-toc">
    {#each outline as section (section.number)}
      {@const n = counts.get(section.number)}
      <a href={pageHash(page.entity_id, section.number)} class:tf-on={active === section.number}
        >{inlineWords(section.heading?.text ?? [])}{#if n}<span class="tf-hits">{n}</span>{/if}</a
      >
    {/each}
  </nav>
{/snippet}

{#if folded}
  {#if outline.length > 0}
    <details class="tf-rd-fold">
      <summary
        >On this page<span class="tf-count">{outline.length} section{outline.length === 1 ? "" : "s"}</span
        >{#if total > 0}<span class="tf-hits">{total} match{total === 1 ? "" : "es"}</span>{/if}</summary
      >
      {@render toc()}
    </details>
  {/if}
{:else}
  <aside class="tf-rd-outline" aria-label="On this page">
    {#if outline.length > 0}
      <h3>On this page</h3>
      {@render toc()}
    {/if}
  </aside>
{/if}

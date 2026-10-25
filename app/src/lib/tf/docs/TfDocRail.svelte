<!--
  The reader's margin (#861, R-10.15 as amended by #2080): only *On this
  page*, the outline that follows the scrolling and counts the find bar's
  matches per section (#860). The work, the backlinks and the history went
  into the page: its foot and its meta line.
-->
<script lang="ts">
  import type { DocPage } from "../../ipc/entity";
  import { inlineWords, pageHash, sectionMatches, sectionsOf } from "./docs";

  let {
    page,
    stems,
    active,
  }: {
    page: DocPage;
    stems: string[];
    /** The section the reader is in, by its number. */
    active: number | null;
  } = $props();

  const outline = $derived(sectionsOf(page.blocks).filter((s) => s.heading !== null));
  const counts = $derived(sectionMatches(page.blocks, stems));
</script>

<aside class="tf-rd-rail" aria-label="On this page">
  {#if outline.length > 0}
    <h3>On this page</h3>
    <nav class="tf-toc">
      {#each outline as section (section.number)}
        {@const n = counts.get(section.number)}
        <a href={pageHash(page.entity_id, section.number)} class:tf-on={active === section.number}
          >{inlineWords(section.heading?.text ?? [])}{#if n}<span class="tf-hits">{n}</span>{/if}</a
        >
      {/each}
    </nav>
  {/if}
</aside>

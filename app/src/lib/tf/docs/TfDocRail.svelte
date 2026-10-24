<!--
  The reader's right rail (#861, R-10.15): *On this page*, the outline that
  follows the scrolling (and counts the find bar's matches per section,
  #860), *Work it belongs to* with each ticket's status now, *Pages that link
  here* and *History*. A part with nothing in it is left out. A version
  with no message reads as prototype-6's *Created* or *Edited*.
-->
<script lang="ts">
  import type { DocPage } from "../../ipc/entity";
  import { clockLabel } from "../../time/format";
  import { jobHash } from "../route";
  import { inlineWords, pageHash, sectionMatches, sectionsOf, shortWho } from "./docs";

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

<aside class="tf-rd-rail" aria-label="About this page">
  {#if outline.length > 0}
    <div>
      <h3>On this page</h3>
      <nav class="tf-toc" aria-label="On this page">
        {#each outline as section (section.number)}
          {@const n = counts.get(section.number)}
          <a href={pageHash(page.entity_id, section.number)} class:tf-on={active === section.number}
            >{inlineWords(section.heading?.text ?? [])}{#if n}<span class="tf-hits">{n}</span>{/if}</a
          >
        {/each}
      </nav>
    </div>
  {/if}
  {#if page.work.length > 0}
    <div>
      <h3>Work it belongs to</h3>
      <div class="tf-rail-list">
        {#each page.work as work (work.entity_id)}
          <a href={jobHash(work.entity_id)}
            ><span class="tf-mono">{work.key}</span> {work.title}{#if work.status}<span class="tf-s">{work.status}</span
              >{/if}</a
          >
        {/each}
      </div>
    </div>
  {/if}
  {#if page.backlinks.length > 0}
    <div>
      <h3>Pages that link here</h3>
      <div class="tf-rail-list">
        {#each page.backlinks as link (link.entity_id)}
          <a href={pageHash(link.entity_id)}
            >{link.title}{#if link.space_key}<span class="tf-s">{link.space_key}</span>{/if}</a
          >
        {/each}
      </div>
    </div>
  {/if}
  {#if page.history.length > 0}
  <div>
    <h3>History</h3>
    <div class="tf-rail-list">
      {#each page.history as version, i (i)}
        <div>
          {version.message ?? (version.number === 1 ? "Created" : "Edited")}<span class="tf-s"
            >{shortWho(version.by, version.by_name, page.me)}, {clockLabel(version.at)}</span
          >
        </div>
      {/each}
    </div>
  </div>
  {/if}
</aside>

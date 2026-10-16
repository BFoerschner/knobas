<!--
  The Docs search's hits in the left pane (#860, R-10.3, R-10.8): the header
  line, then each page with its title in the reading face, its space (when
  searching all) and path, the snippet with its section or commenter, and
  the match count, editor and time. The open page's hit has the bar. With no
  hit, the AND rule, and *Search all spaces* in one space.

  Every string is source text; the matches are the backend's segments.
-->
<script lang="ts">
  import type { DocsHits } from "../../ipc/entity";
  import { when } from "../tickets/tickets";
  import { hitLine, hitsLine, markText, pageHash, snippetLabel, stem } from "./docs";

  let {
    found,
    current,
    scope,
    inSpace,
    me,
    onall,
    onpick,
  }: {
    found: DocsHits;
    current: string | null;
    /** The scope's words: *all spaces* or the space's name. */
    scope: string;
    /** Whether the search is kept to one space, which offers *Search all spaces*. */
    inSpace: boolean;
    me: string[];
    onall: () => void;
    /** A hit is opened: the reader keeps the query. */
    onpick: (entityId: string) => void;
  } = $props();

  const stems = $derived(found.needles.map(stem));
</script>

<div class="tf-dl-meta">
  {hitsLine(found.total, scope, found.took_ms)}{#if inSpace}. <button type="button" class="tf-linkish" onclick={onall}
      >Search all spaces</button>{/if}
</div>
{#each found.hits as hit (hit.entity_id)}
  {@const label = snippetLabel(hit.snippet)}
  <a
    class="tf-dl-hit"
    href={pageHash(hit.entity_id)}
    aria-current={hit.entity_id === current ? "true" : undefined}
    onclick={() => onpick(hit.entity_id)}
  >
    <span class="tf-t"
      >{#each markText(hit.title, stems) as part, i (i)}{#if part.hit}<mark>{part.text}</mark>{:else}{part.text}{/if}{/each}</span
    >
    <span class="tf-pth">{!inSpace && hit.space ? `${hit.space.key} › ` : ""}{hit.path ?? "top level"}</span>
    <span class="tf-sn"
      >{#if label}<b>{label}</b>{" "}{/if}{#each hit.snippet.segments as seg, i (i)}{#if seg.hit}<mark>{seg.text}</mark
          >{:else}{seg.text}{/if}{/each}</span
    >
    <span class="tf-n">{hitLine(hit, me, when)}</span>
  </a>
{:else}
  <p class="tf-dl-note">
    No page matches. Every word has to appear in the page or its comments.{#if inSpace}
      <button type="button" class="tf-linkish" onclick={onall}>Search all spaces</button>{/if}
  </p>
{/each}

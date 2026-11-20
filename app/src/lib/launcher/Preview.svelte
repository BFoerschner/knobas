<!--
  @component
  The preview pane of *Everything* (#2431, spec #2419): where the words appear
  in the selected row, from the hit's own `excerpts`, the words marked. One
  read: the excerpts came with the answer, so the pane asks no port for
  anything. Each excerpt is a button that opens the row at that excerpt (D12).

  **A repository its files found** (the everything-search decisions,
  2026-11-20, D11): a count line under its name, then its files, each with
  its path, the file's name in ink, and up to three lines around the match.
  The answer's file draws at once; the others the launcher asks once the
  selection rests (`search_files`, `more`) draw under it when they land, so
  nothing drawn moves.

  A hit with no excerpt may be one by its title, its key, a stem the excerpts'
  rule misses or a property, and the pane cannot tell them apart, so its
  sentence names no place (#2481).

  The text is the source's and is printed as text, never as markup; a mark is
  a `hit` segment (roadmap §4 gotcha 7, `knobas_search::snippet`).
-->
<script lang="ts" module>
  /** How long the selection rests before the pane follows it, in ms. */
  export const PREVIEW_DEBOUNCE_MS = 120;
</script>

<script lang="ts">
  import type { Excerpt, FilesFound, FilesInRepo } from "../ipc";
  import { countLine, markedWords, pathPieces, quotedLines } from "./files";

  let {
    title,
    excerpts,
    files = null,
    more = null,
    onopen,
  }: {
    /** The selected row's title, or `null` when no row is selected or it is no hit. */
    title: string | null;
    excerpts: Excerpt[];
    /** How many of a repository's files hold the words, where its files found it (D11). */
    files?: FilesFound | null;
    /** The repository's other files, as `search_files` answered them; `null` until it lands. */
    more?: FilesInRepo | null;
    /** Open the row at the excerpt pressed. */
    onopen: (excerpt: Excerpt) => void;
  } = $props();

  /** The files the pane lists: the answer's first, then the read's others by their path, eight at most. */
  const fileExcerpts = $derived.by(() => {
    const first = excerpts.filter((excerpt) => excerpt.file);
    const seen = new Set(first.map((excerpt) => excerpt.file!.path));
    const rest = (more?.files ?? []).filter((excerpt) => excerpt.file && !seen.has(excerpt.file.path));
    return [...first, ...rest].slice(0, 8);
  });
  const otherExcerpts = $derived(excerpts.filter((excerpt) => !excerpt.file));
</script>

<aside class="tf-ln-preview" aria-label="Where it matches">
  {#if title === null}
    <p class="tf-ln-none">Select a row to see where the words appear.</p>
  {:else}
    <p class="tf-ln-pv-h">{title}</p>
    {#if files}<p class="tf-ln-pv-n">{countLine(files, more, fileExcerpts.length)}</p>{/if}
    {#if excerpts.length === 0}
      <p class="tf-ln-none">No passage to quote.</p>
    {:else}
      {#each fileExcerpts as excerpt (excerpt.file!.path)}
        {@const file = excerpt.file!}
        {@const words = markedWords(excerpt)}
        <button type="button" class="tf-ln-ex tf-ln-file" onclick={() => onopen(excerpt)}>
          <span class="tf-ln-file-at"
            ><span class="tf-ln-file-path"
              >{#each pathPieces(file.path, words) as piece, j (j)}{#if piece.hit}<mark class:tf-ln-file-name={piece.name}>{piece.text}</mark
                  >{:else}<span class:tf-ln-file-name={piece.name}>{piece.text}</span>{/if}{/each}</span
            >{#if file.line != null}<span class="tf-ln-file-line">line {file.line}</span>{/if}</span
          >
          {#if file.line != null}
            {@const lines = quotedLines(excerpt)}
            <span class="tf-ln-file-code"
              >{#if lines.before !== null}<span class="tf-ln-file-near">{lines.before}</span>{/if}<span class="tf-ln-file-hit"
                >{#each lines.line as seg, j (j)}{#if seg.hit}<mark>{seg.text}</mark>{:else}{seg.text}{/if}{/each}</span
              >{#if lines.after !== null}<span class="tf-ln-file-near">{lines.after}</span>{/if}</span
            >
          {/if}
        </button>
      {/each}
      {#each otherExcerpts as excerpt, i (i)}
        <button type="button" class="tf-ln-ex" onclick={() => onopen(excerpt)}>
          {#if excerpt.place}<span class="tf-ln-ex-at">{excerpt.place}</span>{/if}
          <span class="tf-ln-ex-t"
            >{#each excerpt.segments as seg, j (j)}{#if seg.hit}<mark>{seg.text}</mark>{:else}{seg.text}{/if}{/each}</span
          >
        </button>
      {/each}
    {/if}
  {/if}
</aside>

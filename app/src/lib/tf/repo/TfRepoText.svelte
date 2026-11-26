<!--
  *Text search*'s answer in the Files tree's place (ADR-0062): a line saying what was searched and where, the file
  names that hold the words when *Search in repository* asked for them, then each file with its matching lines, a
  line's number and its text with the hits marked. A file opens at its top, a line at that line. The text is the file's:
  drawn as text, never as markup.

  Where the lines were read is always said: the clone's commit, the working tree, or the files index's copy of the
  default branch, which is the only one that is not the branch shown when the tab shows another.
-->
<script lang="ts">
  import type { TextFound } from "../../ipc/entity";
  import { thousands } from "../counted";
  import type { TextSearch } from "./text-search.svelte";

  let {
    search,
    hrefOf,
    lineHrefOf,
    current = null,
    onopen,
  }: {
    search: TextSearch;
    /** A file's address. */
    hrefOf: (path: string) => string;
    /** A line's address, from 1. */
    lineHrefOf: (path: string, line: number) => string;
    /** The file the tab shows, marked among the files. */
    current?: string | null;
    /** A file or a line was opened from the list. */
    onopen?: () => void;
  } = $props();

  const found = $derived(search.found);

  function plural(n: number, one: string, many: string): string {
    return `${thousands(n)} ${n === 1 ? one : many}`;
  }

  /** What was searched and where, in one line. */
  function said(found: TextFound): string {
    const lines = found.files.reduce((sum, file) => sum + file.hits.length + file.more, 0);
    const what = found.files.length === 0 ? "No line matches" : `${plural(lines, "line", "lines")} in ${plural(found.files.length, "file", "files")}`;
    const where =
      found.from === "working_tree"
        ? "in the working tree"
        : found.from === "index"
          ? `in knobas's copy of ${found.branch ?? "the default branch"}, as of the last sync`
          : `in your clone at ${found.commit?.slice(0, 7) ?? "the branch's commit"}`;
    return `${what} ${where}.`;
  }

  /** The directory part of a path, and its name. */
  function split(path: string): [string, string] {
    const cut = path.lastIndexOf("/") + 1;
    return [path.slice(0, cut), path.slice(cut)];
  }
</script>

{#snippet fileIcon()}
  <svg class="tf-ficon" viewBox="0 0 16 16" aria-hidden="true"><path d="M4 2h5l3 3v9H4z M9 2v3h3" /></svg>
{/snippet}

<div class="tf-txres" class:tf-txreading={search.reading && found !== null} aria-busy={search.reading}>
  {#if found?.invalid}
    <p class="tf-gonote tf-txbad" role="status">{found.invalid}</p>
  {:else if search.failed !== null}
    <p class="tf-cfail">Could not search: {search.failed}</p>
  {:else if found === null}
    <p class="tf-gonote" role="status">Searching…</p>
  {:else if found.from === "none"}
    <p class="tf-gonote" role="status">{found.why}</p>
  {:else}
    <p class="tf-gonote" role="status">{said(found)}</p>
    {#if found.names.length > 0}
      <p class="tf-txhead">Files named “{search.typed.trim()}”</p>
      {#each found.names as name (name)}
        {@const [dir, base] = split(name)}
        <a class="tf-gotof" href={hrefOf(name)} aria-current={name === current ? "true" : undefined} onclick={() => onopen?.()}
          >{@render fileIcon()}<span>{#if dir}<span class="tf-pdir">{dir}</span>{/if}{base}</span></a
        >
      {/each}
      {#if found.files.length > 0}<p class="tf-txhead">Lines</p>{/if}
    {/if}
    {#each found.files as file (file.path)}
      {@const [dir, base] = split(file.path)}
      <a class="tf-gotof tf-txfile" href={hrefOf(file.path)} aria-current={file.path === current ? "true" : undefined} onclick={() => onopen?.()}
        >{@render fileIcon()}<span>{#if dir}<span class="tf-pdir">{dir}</span>{/if}{base}</span><span class="tf-txcount">{thousands(file.hits.length + file.more)}</span></a
      >
      {#each file.hits as hit (hit.line)}
        <a class="tf-txhit" href={lineHrefOf(file.path, hit.line)} onclick={() => onopen?.()}
          ><span class="tf-txn">{hit.line}</span><span class="tf-txl"
            >{#each hit.parts as part, at (at)}{#if part.hit}<mark>{part.text}</mark>{:else}{part.text}{/if}{/each}</span
          ></a
        >
      {/each}
      {#if file.more > 0}
        <p class="tf-txmore">{plural(file.more, "more line", "more lines")} in this file</p>
      {/if}
    {/each}
    {#if found.cut}
      <p class="tf-gonote">The search stopped at its first lines: narrow the words to see the rest.</p>
    {/if}
    {#if found.skipped > 0}
      <p class="tf-gonote">{plural(found.skipped, "binary or large file was", "binary or large files were")} not searched.</p>
    {/if}
    {#if found.from === "index"}
      <p class="tf-gonote">Not cloned here, so the default branch's text is searched. Clone the repository to search any branch.</p>
    {/if}
  {/if}
</div>

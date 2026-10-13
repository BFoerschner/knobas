<!--
  One file of a tree (#711): its trail, a quiet line of what it is (lines,
  size), then its text monospace with a number for each line. No syntax
  highlighting (spec #701). A Markdown file opens drawn by `MarkdownText`, as
  a README is, with its raw text one toggle away.

  The numbers and the text are two `pre`s side by side, one text node each,
  rather than an element per line: a file may be 10,000 lines, and the two
  columns line up because they share the font and the line height and
  neither wraps. Every string is source text, interpolated (rule 5).

  A file a cap cut says so in the amber the diff's cut uses, and a binary one
  says what it is; both offer the file's own page at its source.
-->
<script lang="ts">
  import type { Crumb } from "../../ipc/entity";
  import { sizeOf } from "./bytes";
  import MarkdownText from "./MarkdownText.svelte";
  import Trail from "./Trail.svelte";

  let {
    trail,
    path,
    text,
    language,
    size,
    truncated,
    binary,
    url,
    sourceName,
    onopen,
    onback,
    onopenurl,
  }: {
    trail: Crumb[];
    path: string;
    text: string;
    language: string | null;
    size: number;
    truncated: boolean;
    binary: boolean;
    url: string | null;
    sourceName: string;
    onopen: (section: string) => void;
    onback?: (() => void) | undefined;
    onopenurl: (url: string) => void;
  } = $props();

  /** The lines as drawn: a final newline ends the last line, it opens none. */
  const body = $derived(text.endsWith("\n") ? text.slice(0, -1) : text);
  const count = $derived(binary ? 0 : body.split("\n").length);
  const numbers = $derived(Array.from({ length: count }, (_, i) => i + 1).join("\n"));
  const markdown = $derived(language === "markdown" && !binary);

  /** The reader asked for the raw text of a Markdown file. */
  let raw = $state(false);
</script>

<div class="fb">
  <div class="fb-head">
    <Trail {trail} {onopen} {onback} />
    <span class="fb-meta k">
      {#if !binary}{count.toLocaleString("en")} {count === 1 ? "line" : "lines"} · {/if}{sizeOf(size)}
    </span>
    {#if markdown}
      <button class="btn ghost sm fb-raw" aria-pressed={raw} onclick={() => (raw = !raw)}>Raw</button>
    {/if}
  </div>

  {#if truncated}
    <p class="fb-cut">
      knobas shows the first {count.toLocaleString("en")} lines of this file.
      {#if url}
        {@const page = url}
        <button class="btn sm" onclick={() => onopenurl(page)}>Open in {sourceName}</button>
      {/if}
    </p>
  {/if}

  {#if binary}
    <p class="fb-note">
      Binary file, {sizeOf(size)}: nothing to read here.
      {#if url}
        {@const page = url}
        <button class="btn sm" onclick={() => onopenurl(page)}>Open in {sourceName}</button>
      {/if}
    </p>
  {:else if markdown && !raw}
    <div class="fb-md"><MarkdownText text={body} {onopenurl} /></div>
  {:else if count > 0 && body !== ""}
    <div class="fb-box" style:--gw="{String(count).length}ch" aria-label={path} role="region">
      <pre class="fb-nums" aria-hidden="true">{numbers}</pre>
      <pre class="fb-code">{body}</pre>
    </div>
  {:else}
    <p class="fb-note">This file is empty.</p>
  {/if}
</div>

<style>
  .fb {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .fb-head {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }

  .fb-meta {
    margin-left: auto;
    flex: none;
    font-size: 11px;
    color: var(--faint);
  }

  .fb-raw[aria-pressed="true"] {
    color: var(--text);
    background: var(--raised);
  }

  .fb-cut,
  .fb-note {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    color: var(--muted);
    font-size: 12px;
  }

  .fb-cut {
    color: var(--amber);
  }

  .fb-md {
    padding: 2px 2px 0;
  }

  /* One hairline box that scrolls both ways; the gutter stays put. */
  .fb-box {
    display: grid;
    grid-template-columns: calc(var(--gw) + 16px) 1fr;
    max-height: 70vh;
    overflow: auto;
    border: 1px solid var(--hair);
    border-radius: 2px;
    background: var(--bg);
    font: 400 12px/18px var(--mono);
  }

  .fb-box pre {
    margin: 0;
    padding: 4px 0;
    font: inherit;
    white-space: pre;
  }

  .fb-box .fb-nums {
    position: sticky;
    left: 0;
    padding: 4px 8px 4px 0;
    text-align: right;
    color: var(--faint);
    background: var(--bg);
    border-right: 1px solid var(--hair);
    user-select: none;
    font-variant-numeric: tabular-nums;
  }

  .fb-box .fb-code {
    padding: 4px 12px 4px 10px;
    color: var(--text);
  }
</style>

<!--
  A diff block (ADR-0020; `CONTEXT.md` **Diff**), drawn the way a code review
  reads one: the shape of the change first (files, +/− counts), then one card
  per file, each a run of hunks with the old and new line numbers in two
  gutters.

  Colour only where something changed: added lines on the theme's `--ok`,
  removed ones on `--fail`, both as a faint wash so the text stays the
  loudest thing on the line. No syntax highlighting (spec #660, out of scope).

  Every string is source text and is interpolated (rule 5, gotcha 7): a line
  holding `<b>` is a line holding `<b>`.
-->
<script lang="ts">
  import type { DiffChange, DiffFile } from "../../ipc/entity";

  let {
    files,
    truncated,
    webUrl,
    sourceName,
    onopenurl,
  }: {
    files: DiffFile[];
    /** The per-view cap cut it: files after the cut carry counts only. */
    truncated: boolean;
    /** The entity's own page at its source, offered where a file was cut. */
    webUrl: string | null;
    /** The source's display name, for *Open in <source>*. */
    sourceName: string;
    onopenurl: (url: string) => void;
  } = $props();

  /** Which cards the reader closed, by index. Everything opens expanded. */
  let closed = $state<Record<number, boolean>>({});
  let cards = $state<(HTMLElement | null)[]>([]);

  const additions = $derived(files.reduce((sum, file) => sum + file.additions, 0));
  const deletions = $derived(files.reduce((sum, file) => sum + file.deletions, 0));
  const allClosed = $derived(files.length > 0 && files.every((_, index) => closed[index]));

  const LETTER: Record<DiffChange, string> = {
    added: "A",
    modified: "M",
    deleted: "D",
    renamed: "R",
    binary: "B",
  };

  function toggleAll() {
    const close = !allClosed;
    closed = Object.fromEntries(files.map((_, index) => [index, close]));
  }

  /** Open the card and bring it into view. */
  function jump(index: number) {
    closed[index] = false;
    cards[index]?.scrollIntoView({ block: "start" });
  }

  function linesOf(file: DiffFile): number {
    return file.hunks.reduce((sum, hunk) => sum + hunk.lines.length, 0);
  }

  /** Wide enough for the largest line number in the file, so the gutters line up. */
  function gutter(file: DiffFile): number {
    let max = 0;
    for (const hunk of file.hunks) {
      for (const line of hunk.lines) max = Math.max(max, line.old ?? 0, line.new ?? 0);
    }
    return Math.max(2, String(max).length);
  }
</script>

<div class="df">
  {#if truncated}
    <p class="df-cut">
      This diff stops at the line cap: files after the cut show their counts only.
      {#if webUrl}
        <button class="btn sm" onclick={() => onopenurl(webUrl)}>Open in {sourceName}</button>
      {/if}
    </p>
  {/if}

  <div class="df-sum">
    <span class="k">{files.length} {files.length === 1 ? "file" : "files"}</span>
    <span class="k add">+{additions}</span>
    <span class="k del">−{deletions}</span>
    {#if files.length > 1}
      <button class="btn ghost sm" onclick={toggleAll}>
        {allClosed ? "Expand all" : "Collapse all"}
      </button>
    {/if}
  </div>

  {#if files.length > 1}
    <ol class="df-index">
      {#each files as file, index (index)}
        <li>
          <button title="Go to {file.path}" onclick={() => jump(index)}>
            <span class="df-letter {file.change}">{LETTER[file.change]}</span>
            <span class="df-ipath">{file.path}</span>
            <span class="k add">+{file.additions}</span>
            <span class="k del">−{file.deletions}</span>
          </button>
        </li>
      {/each}
    </ol>
  {/if}

  {#each files as file, index (index)}
    {@const open = !closed[index]}
    <section class="df-file" bind:this={cards[index]}>
      <button
        class="df-head"
        aria-expanded={open}
        onclick={() => (closed[index] = open)}
      >
        <svg class="chev" class:open viewBox="0 0 16 16" aria-hidden="true">
          <path d="M6 4l4 4-4 4" />
        </svg>
        <span class="df-change {file.change}">{file.change}</span>
        <span class="df-path">
          {#if file.old_path}
            <span class="old">{file.old_path}</span>
            <span class="arrow">→</span>
          {/if}
          {file.path}
        </span>
        <span class="k add">+{file.additions}</span>
        <span class="k del">−{file.deletions}</span>
      </button>

      {#if open}
        {#if file.change === "binary"}
          <p class="df-note">Binary file: no lines to show.</p>
        {:else if file.hunks.length === 0 && !file.truncated}
          <p class="df-note">
            {file.change === "renamed" ? "Renamed without changes." : "No line changes."}
          </p>
        {:else if file.hunks.length > 0}
          <div class="df-body" style:--gw="{gutter(file)}ch">
            <div class="df-lines">
              {#each file.hunks as hunk, h (h)}
                <div class="df-hunk">{hunk.header}</div>
                {#each hunk.lines as line, l (l)}
                  <div class="df-line {line.kind}" data-kind={line.kind}>
                    <span class="df-old">{line.old ?? ""}</span>
                    <span class="df-new">{line.new ?? ""}</span>
                    <span class="df-mark" aria-hidden="true">
                      {line.kind === "add" ? "+" : line.kind === "del" ? "−" : ""}
                    </span>
                    <span class="df-text">{line.text}</span>
                  </div>
                {/each}
              {/each}
            </div>
          </div>
        {/if}
        {#if file.truncated}
          <p class="df-note">
            {#if linesOf(file) > 0}
              knobas shows the first {linesOf(file).toLocaleString("en")} lines of this file.
            {:else}
              This file is past the diff's cut; knobas shows its counts only.
            {/if}
            {#if webUrl}
              <button class="btn sm" onclick={() => onopenurl(webUrl)}>Open in {sourceName}</button>
            {/if}
          </p>
        {/if}
      {/if}
    </section>
  {/each}
</div>

<style>
  .df {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .add {
    color: var(--ok);
  }

  .del {
    color: var(--fail);
  }

  .df-sum {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 12px;
  }

  .df-sum .btn {
    margin-left: auto;
  }

  .df-cut,
  .df-note {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    margin: 0;
    color: var(--muted);
    font-size: 12px;
  }

  .df-cut {
    color: var(--amber);
  }

  .df-note {
    padding: 6px 10px;
  }

  /* The file list: one quiet line per file, a jump target each. */
  .df-index {
    list-style: none;
    border: 1px solid var(--hair);
    border-radius: 2px;
    max-height: 168px;
    overflow: auto;
  }

  .df-index li + li {
    border-top: 1px solid var(--hair);
  }

  .df-index button {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    height: 24px;
    padding: 0 8px;
    text-align: left;
    font: 400 12px var(--mono);
  }

  .df-index button:hover {
    background: var(--raised);
  }

  .df-ipath {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .df-letter {
    width: 12px;
    flex: none;
    font: 500 11px var(--mono);
    color: var(--faint);
  }

  .df-letter.added {
    color: var(--ok);
  }

  .df-letter.deleted {
    color: var(--fail);
  }

  /* One file. A hairline box, not a card with a shadow. */
  .df-file {
    border: 1px solid var(--hair);
    border-radius: 2px;
    background: var(--bg);
    scroll-margin-top: 8px;
  }

  .df-head {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    min-height: 28px;
    padding: 4px 8px;
    text-align: left;
    background: var(--raised);
    font-size: 12px;
  }

  .df-head:hover {
    background: var(--hair);
  }

  .chev {
    width: 12px;
    height: 12px;
    flex: none;
    stroke: var(--muted);
    fill: none;
    stroke-width: 1.5;
    transition: transform 0.12s;
  }

  .chev.open {
    transform: rotate(90deg);
  }

  .df-change {
    flex: none;
    font: 500 10px var(--mono);
    letter-spacing: 0.04em;
    color: var(--faint);
    border: 1px solid var(--hair2);
    border-radius: 2px;
    padding: 0 4px;
  }

  .df-change.added {
    color: var(--ok);
  }

  .df-change.deleted {
    color: var(--fail);
  }

  .df-path {
    flex: 1;
    min-width: 0;
    font: 500 12px var(--mono);
    overflow-wrap: anywhere;
  }

  .df-path .old {
    color: var(--muted);
  }

  .df-path .arrow {
    color: var(--faint);
    margin: 0 4px;
  }

  .df-body {
    overflow-x: auto;
    border-top: 1px solid var(--hair);
  }

  .df-lines {
    min-width: 100%;
    width: max-content;
    font: 400 12px/18px var(--mono);
  }

  .df-hunk {
    padding: 2px 8px 2px calc(var(--gw) * 2 + 34px);
    color: var(--faint);
    background: var(--panel);
    white-space: pre;
  }

  .df-line {
    display: grid;
    grid-template-columns: calc(var(--gw) + 10px) calc(var(--gw) + 10px) 14px auto;
  }

  .df-old,
  .df-new {
    padding-right: 6px;
    text-align: right;
    color: var(--faint);
    user-select: none;
    font-variant-numeric: tabular-nums;
  }

  .df-new {
    border-right: 1px solid var(--hair);
  }

  .df-mark {
    text-align: center;
    color: var(--faint);
    user-select: none;
  }

  .df-text {
    padding-right: 12px;
    white-space: pre;
  }

  .df-line.add {
    background: color-mix(in srgb, var(--ok) 11%, transparent);
  }

  .df-line.add .df-mark {
    color: var(--ok);
  }

  .df-line.del {
    background: color-mix(in srgb, var(--fail) 12%, transparent);
  }

  .df-line.del .df-mark {
    color: var(--fail);
  }

  @media (prefers-reduced-motion: reduce) {
    .chev {
      transition: none;
    }
  }
</style>

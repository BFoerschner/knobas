<!--
  A page body's blocks (#859, R-10.11), drawn from the fixed set and nothing
  else. A part the source has and knobas does not draw says so, with *Open
  this part in <source>* (story 134). An embed runs in a sandboxed frame
  (`TfDocEmbed`, #2440). The page's sections and their anchors
  are `TfDocBody`'s; a heading here is one inside a panel or a comment.

  An include draws the page it includes in a light frame reading "from
  <title>", the title a link to that page; one knobas could not fill is a
  placeholder saying why, naming the page and linking it where the mirror
  holds it, else offering the source.

  Recurses into lists, panels, collapsibles, table cells and includes by importing itself. A task item (spec
  #2432) draws its state as the Markdown view's box: a picture, never a
  control, so nothing here ticks a task.
-->
<script lang="ts">
  import Self from "./TfDocBlocks.svelte";
  import { whileNear } from "../../shell/near-view";
  import TfDocEmbed from "./TfDocEmbed.svelte";
  import TfDocInlines from "./TfDocInlines.svelte";
  import TfMarked from "./TfMarked.svelte";
  import type { DocBlock, TableCell } from "../../ipc/entity";
  import { DOC_BLOCKS_FIRST, TABLE_ROWS_FIRST, headed, headerColumn, includeMiss, pageHash, panelLabel, panelTone } from "./docs";

  let {
    blocks,
    entityId,
    sourceName,
    webUrl,
    onopen,
    oncopy,
    stems = [],
    whole = true,
  }: {
    blocks: DocBlock[];
    entityId: string;
    sourceName: string;
    webUrl: string | null;
    onopen: (url: string) => void;
    oncopy: (code: string) => void;
    /** What the find bar marks (#860). */
    stems?: string[];
    /**
     * Draw everything now. Where it is not (a page body drawing in pieces, `TfDocBody`), a long table or list draws
     * its first rows and the rest a piece a task: a 2,000-row table was one task of 243 ms, 921 ms read again (the
     * extremes pass, 2026-11-13). Whatever needs the whole body on the page (a find, a section's address) asks for it
     * whole.
     */
    whole?: boolean;
  } = $props();

  /** A table's rows drawn at first, and how many more each task after. */
  const ROWS_FIRST = TABLE_ROWS_FIRST;
  const ROWS_MORE = 150;
  /** Each long table's rows (or list's items) drawn so far, by its place among `blocks`, while these are the blocks. */
  let grown = $state<{ of: DocBlock[]; rows: Record<number, number> }>({ of: [], rows: {} });
  const rowsDrawn = (index: number) => (grown.of === blocks ? (grown.rows[index] ?? ROWS_FIRST) : ROWS_FIRST);
  /** A long table's rows, or a long list's items, as far as they are drawn yet. */
  function bodyRows<T>(index: number, rows: T[]): T[] {
    return whole || rows.length <= rowsDrawn(index) ? rows : rows.slice(0, rowsDrawn(index));
  }
  /** How many rows (a table's) or items (a list's) a block has. */
  const lengthOf = (block: DocBlock) => (block.type === "table" ? block.rows.length : block.type === "list" ? block.items.length : 0);
  // The rest of each long table, a piece a task, so the page paints and answers between them.
  $effect(() => {
    if (whole) return;
    const now = blocks;
    const short = now.flatMap((block, index) => (lengthOf(block) > rowsDrawn(index) ? [index] : []));
    if (short.length === 0) return;
    const later = setTimeout(() => {
      const rows = grown.of === now ? { ...grown.rows } : {};
      for (const index of short) rows[index] = (rows[index] ?? ROWS_FIRST) + ROWS_MORE;
      grown = { of: now, rows };
    }, 0);
    return () => clearTimeout(later);
  });
  /**
   * A run of thousands of blocks under one heading (a pasted log, an export) draws its first blocks and the rest a
   * piece a task, as a long table does: a body's pieces are its sections (`TfDocBody`), and one section can be the
   * whole page.
   */
  const BLOCKS_FIRST = DOC_BLOCKS_FIRST;
  const BLOCKS_MORE = 200;
  let blocksGrown = $state<{ of: DocBlock[]; n: number }>({ of: [], n: BLOCKS_FIRST });
  const blocksDrawn = $derived(blocksGrown.of === blocks ? blocksGrown.n : BLOCKS_FIRST);
  const shownBlocks = $derived(whole || blocks.length <= blocksDrawn ? blocks : blocks.slice(0, blocksDrawn));
  /** The next blocks, while the end of those drawn nears the view (`whileNear`); `false` once all are drawn. */
  function moreBlocks(): boolean {
    if (whole || blocks.length <= blocksDrawn) return false;
    blocksGrown = { of: blocks, n: blocksDrawn + BLOCKS_MORE };
    return true;
  }

  /** A span as the attribute draws it: none for one. */
  function span(n: number): number | undefined {
    return n > 1 ? n : undefined;
  }
</script>

{#snippet th(cell: TableCell, scope: "col" | "row" | undefined)}
  <th {scope} colspan={span(cell.colspan)} rowspan={span(cell.rowspan)}
    ><Self blocks={cell.blocks} {entityId} {sourceName} {webUrl} {onopen} {oncopy} {stems} {whole} /></th
  >
{/snippet}

{#each shownBlocks as block, index (index)}
  {#if block.type === "heading"}
    <h2><TfDocInlines text={block.text} {onopen} {stems} /></h2>
  {:else if block.type === "paragraph"}
    <p><TfDocInlines text={block.text} {onopen} {stems} /></p>
  {:else if block.type === "list"}
    <svelte:element this={block.ordered ? "ol" : "ul"}>
      {#each bodyRows(index, block.items) as item, i (i)}
        {#if item.checked === null}
          <li><Self blocks={item.blocks} {entityId} {sourceName} {webUrl} {onopen} {oncopy} {stems} {whole} /></li>
        {:else}
          <li class="tf-task">
            <span class={item.checked ? "tf-box tf-on" : "tf-box"} role="img" aria-label={item.checked ? "Done" : "Not done"}></span>
            <div><Self blocks={item.blocks} {entityId} {sourceName} {webUrl} {onopen} {oncopy} {stems} {whole} /></div>
          </li>
        {/if}
      {/each}
    </svelte:element>
  {:else if block.type === "code"}
    {@const code = block.code}
    <!-- The language and Copy sit in a strip above the code, so neither covers a long line (#2083). -->
    <div class="tf-pre">
      <div class="tf-pre-bar">
        {#if block.language}<span>{block.language}</span>{/if}
        <button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => oncopy(code)}>Copy</button>
      </div>
      <pre><code><TfMarked text={code} {stems} /></code></pre>
    </div>
  {:else if block.type === "table"}
    <!-- A cell holds blocks and spans what it spans; a header row heads columns and a header column rows (#2436).
         A table wider than the reader scrolls inside its frame, never widening the page. -->
    {@const head = headed(block.rows)}
    {@const rowHeads = headerColumn(block.rows, head ? 1 : 0)}
    <div class="tf-doc-tbl">
      <table>
        {#if head}
          <thead>
            <tr>
              {#each block.rows[0] ?? [] as cell, c (c)}{@render th(cell, "col")}{/each}
            </tr>
          </thead>
        {/if}
        <tbody>
          {#each bodyRows(index, head ? block.rows.slice(1) : block.rows) as row, r (r)}
            <tr>
              {#each row as cell, c (c)}
                {#if cell.header}{@render th(cell, rowHeads && c === 0 ? "row" : undefined)}{:else}<td
                    colspan={span(cell.colspan)}
                    rowspan={span(cell.rowspan)}
                    ><Self blocks={cell.blocks} {entityId} {sourceName} {webUrl} {onopen} {oncopy} {stems} {whole} /></td
                  >{/if}
              {/each}
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {:else if block.type === "panel"}
    <div class="tf-callout tf-{panelTone(block.kind)}">
      <span class="tf-lbl">{panelLabel(block)}</span>
      <div><Self blocks={block.blocks} {entityId} {sourceName} {webUrl} {onopen} {oncopy} {stems} {whole} /></div>
    </div>
  {:else if block.type === "collapsible"}
    <!-- Closed, as the source draws it (spec #2432); the find bar opens it when it steps to a match inside (`revealMatch`).
         Keyed by the page, so one opened here is not still open where the next page draws one. -->
    {#key entityId}
      <details class="tf-collapse">
        <summary><TfMarked text={block.title} {stems} /></summary>
        <div><Self blocks={block.blocks} {entityId} {sourceName} {webUrl} {onopen} {oncopy} {stems} {whole} /></div>
      </details>
    {/key}
  {:else if block.type === "include"}
    <!-- Another page drawn in place (spec #2432), as knobas filled it from the mirror when the page was read. -->
    {#if block.unfilled === null}
      <div class="tf-incl">
        <div class="tf-incl-from">
          from {#if block.entity_id}<a class="tf-pgl" href={pageHash(block.entity_id)}><em>{block.title}</em></a
            >{:else}<em>{block.title}</em>{/if}
        </div>
        <Self blocks={block.blocks} {entityId} {sourceName} {webUrl} {onopen} {oncopy} {stems} {whole} />
      </div>
    {:else}
      <div class="tf-macro">
        <span
          >{includeMiss(block.unfilled).before}{#if block.entity_id}<a class="tf-pgl" href={pageHash(block.entity_id)}
              ><b>{block.title}</b></a
            >{:else}<b>{block.title}</b>{/if}{includeMiss(block.unfilled).after}</span
        >
        {#if !block.entity_id && webUrl}
          {@const url = webUrl}
          <button type="button" class="tf-btn tf-sm" onclick={() => onopen(url)}>Open this part in {sourceName}</button>
        {/if}
      </div>
    {/if}
  {:else if block.type === "placeholder"}
    <div class="tf-macro">
      <span>This part is a {sourceName} <b>{block.name}</b> macro, which knobas does not draw.</span>
      {#if webUrl}
        {@const url = webUrl}
        <button type="button" class="tf-btn tf-sm" onclick={() => onopen(url)}>Open this part in {sourceName}</button>
      {/if}
    </div>
  {:else if block.type === "embed"}
    <TfDocEmbed content={block.content} {sourceName} {webUrl} {onopen} />
  {/if}
{/each}{#if shownBlocks.length < blocks.length}<div class="tf-doc-more" aria-hidden="true" {@attach whileNear(moreBlocks)}></div>{/if}

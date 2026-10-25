<!--
  A page body's blocks (#859, R-10.11), drawn from the fixed set and nothing
  else. A part the source has and knobas does not draw says so, with *Open
  this part in <source>* (story 134). The page's sections and their anchors
  are `TfDocBody`'s; a heading here is one inside a panel or a comment.

  Recurses into lists and panels by importing itself.
-->
<script lang="ts">
  import Self from "./TfDocBlocks.svelte";
  import TfDocInlines from "./TfDocInlines.svelte";
  import TfMarked from "./TfMarked.svelte";
  import type { DocBlock } from "../../ipc/entity";
  import { headed, panelLabel, panelTone } from "./docs";

  let {
    blocks,
    entityId,
    sourceName,
    webUrl,
    onopen,
    oncopy,
    stems = [],
  }: {
    blocks: DocBlock[];
    entityId: string;
    sourceName: string;
    webUrl: string | null;
    onopen: (url: string) => void;
    oncopy: (code: string) => void;
    /** What the find bar marks (#860). */
    stems?: string[];
  } = $props();
</script>

{#each blocks as block, index (index)}
  {#if block.type === "heading"}
    <h2><TfDocInlines text={block.text} {onopen} {stems} /></h2>
  {:else if block.type === "paragraph"}
    <p><TfDocInlines text={block.text} {onopen} {stems} /></p>
  {:else if block.type === "list"}
    <svelte:element this={block.ordered ? "ol" : "ul"}>
      {#each block.items as item, i (i)}
        <li><Self blocks={item} {entityId} {sourceName} {webUrl} {onopen} {oncopy} {stems} /></li>
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
    {@const head = headed(block.rows)}
    <table>
      {#if head}
        <thead>
          <tr>
            {#each block.rows[0] ?? [] as cell, c (c)}<th><TfDocInlines text={cell.text} {onopen} {stems} /></th>{/each}
          </tr>
        </thead>
      {/if}
      <tbody>
        {#each head ? block.rows.slice(1) : block.rows as row, r (r)}
          <tr>
            {#each row as cell, c (c)}
              {#if cell.header}<th><TfDocInlines text={cell.text} {onopen} {stems} /></th>{:else}<td
                  ><TfDocInlines text={cell.text} {onopen} {stems} /></td
                >{/if}
            {/each}
          </tr>
        {/each}
      </tbody>
    </table>
  {:else if block.type === "panel"}
    <div class="tf-callout tf-{panelTone(block.kind)}">
      <span class="tf-lbl">{panelLabel(block)}</span>
      <div><Self blocks={block.blocks} {entityId} {sourceName} {webUrl} {onopen} {oncopy} {stems} /></div>
    </div>
  {:else if block.type === "placeholder"}
    <div class="tf-macro">
      <span>This part is a {sourceName} <b>{block.name}</b> macro, which knobas does not draw.</span>
      {#if webUrl}
        {@const url = webUrl}
        <button type="button" class="tf-btn tf-sm" onclick={() => onopen(url)}>Open this part in {sourceName}</button>
      {/if}
    </div>
  {/if}
{/each}

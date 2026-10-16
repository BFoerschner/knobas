<!--
  A page body's blocks (#859, R-10.11), drawn from the fixed set and nothing
  else. A heading carries its section anchor (`#/page/<id>/<n>`, prototype-6's
  numbering); a part the source has and knobas does not draw says so, with
  *Open this part in <source>* (story 134).

  Recurses into lists and panels by importing itself.
-->
<script lang="ts">
  import Self from "./TfDocBlocks.svelte";
  import TfDocInlines from "./TfDocInlines.svelte";
  import type { DocBlock } from "../../ipc/entity";
  import { headed, pageHash, panelLabel, panelTone, sectionNumbers } from "./docs";

  let {
    blocks,
    entityId,
    sourceName,
    webUrl,
    onopen,
    oncopy,
    anchors = false,
  }: {
    blocks: DocBlock[];
    entityId: string;
    sourceName: string;
    webUrl: string | null;
    onopen: (url: string) => void;
    oncopy: (code: string) => void;
    /** Whether headings are the page's sections (the body, not a panel or a comment). */
    anchors?: boolean;
  } = $props();

  const sections = $derived(anchors ? sectionNumbers(blocks) : new Map<number, number>());
</script>

{#each blocks as block, index (index)}
  {#if block.type === "heading"}
    {@const n = sections.get(index)}
    <h2 id={n === undefined ? undefined : `s-${n}`}>
      <TfDocInlines text={block.text} {onopen} />
      {#if n !== undefined}
        <span class="tf-tools"><a href={pageHash(entityId, n)} title="Link to this section">#</a></span>
      {/if}
    </h2>
  {:else if block.type === "paragraph"}
    <p><TfDocInlines text={block.text} {onopen} /></p>
  {:else if block.type === "list"}
    <svelte:element this={block.ordered ? "ol" : "ul"}>
      {#each block.items as item, i (i)}
        <li><Self blocks={item} {entityId} {sourceName} {webUrl} {onopen} {oncopy} /></li>
      {/each}
    </svelte:element>
  {:else if block.type === "code"}
    {@const code = block.code}
    <pre><button type="button" class="tf-btn tf-sm tf-ghost tf-copy" onclick={() => oncopy(code)}>Copy</button><code
        >{code}</code
      >{#if block.language}<span class="tf-lang">{block.language}</span>{/if}</pre>
  {:else if block.type === "table"}
    {@const head = headed(block.rows)}
    <table>
      {#if head}
        <thead>
          <tr>
            {#each block.rows[0] ?? [] as cell, c (c)}<th><TfDocInlines text={cell.text} {onopen} /></th>{/each}
          </tr>
        </thead>
      {/if}
      <tbody>
        {#each head ? block.rows.slice(1) : block.rows as row, r (r)}
          <tr>
            {#each row as cell, c (c)}
              {#if cell.header}<th><TfDocInlines text={cell.text} {onopen} /></th>{:else}<td
                  ><TfDocInlines text={cell.text} {onopen} /></td
                >{/if}
            {/each}
          </tr>
        {/each}
      </tbody>
    </table>
  {:else if block.type === "panel"}
    <div class="tf-callout tf-{panelTone(block.kind)}">
      <span class="tf-lbl">{panelLabel(block)}</span>
      <div><Self blocks={block.blocks} {entityId} {sourceName} {webUrl} {onopen} {oncopy} /></div>
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

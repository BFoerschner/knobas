<!--
  Markdown a source wrote -- a repository's README (#663) -- drawn from the
  blocks `markdown.ts` read, with elements this file names and nothing else.

  **No `{@html}`** (gotcha 7, ADR-0020 rule 5): every string is interpolated,
  so HTML a README embeds shows as the characters it is made of. A link is a
  button, as in `StorageBody`: an anchor would navigate the webview itself, so
  a web link goes to the OS browser through the shell, and a link of any
  other scheme is its words.

  Dressed like a Confluence page body (`Detail.svelte`'s `.d-body.storage`),
  so a README and a wiki page read alike.
-->
<script lang="ts">
  import { parseMarkdown, type Inline } from "./markdown";

  let {
    text,
    onopenurl,
  }: {
    text: string;
    /** Open a web link. Called only with an `http:` or `https:` URL. */
    onopenurl?: ((url: string) => void) | undefined;
  } = $props();

  const blocks = $derived(parseMarkdown(text));

  /** A web link this panel may hand to the OS; anything else is words. */
  function webLink(href: string): string | null {
    try {
      const url = new URL(href);
      return url.protocol === "http:" || url.protocol === "https:" ? url.href : null;
    } catch {
      return null;
    }
  }
</script>

{#snippet runs(inline: Inline[])}{#each inline as run, index (index)}{#if run.kind === "text"}{run.text}{:else if run.kind === "code"}<code>{run.text}</code>{:else if run.kind === "strong"}<strong>{run.text}</strong>{:else if run.kind === "em"}<em>{run.text}</em>{:else if run.kind === "image"}<span class="md-img">{run.text || "image"}</span>{:else}{@const url = onopenurl ? webLink(run.href) : null}{#if url}<button class="md-link" type="button" title={url} onclick={() => onopenurl?.(url)}>{run.text}</button>{:else}{run.text}{/if}{/if}{/each}{/snippet}

<div class="md">
  {#each blocks as block, index (index)}
    {#if block.kind === "heading"}
      <svelte:element this={`h${block.level}`}>{@render runs(block.inline)}</svelte:element>
    {:else if block.kind === "paragraph"}
      <p>{@render runs(block.inline)}</p>
    {:else if block.kind === "list"}
      <svelte:element this={block.ordered ? "ol" : "ul"} start={block.ordered ? block.start : undefined}>
        {#each block.items as item, i (i)}
          <li class="d{item.depth}">{@render runs(item.inline)}</li>
        {/each}
      </svelte:element>
    {:else if block.kind === "code"}
      <pre><code>{block.text}</code></pre>
    {:else if block.kind === "quote"}
      <blockquote>{@render runs(block.inline)}</blockquote>
    {:else if block.kind === "table"}
      <pre class="md-table">{block.text}</pre>
    {:else}
      <hr />
    {/if}
  {/each}
</div>

<style>
  .md {
    font-size: 12.5px;
    line-height: 1.55;
    overflow-wrap: anywhere;
  }

  .md > :first-child {
    margin-top: 0;
  }

  .md :global(h1),
  .md :global(h2),
  .md :global(h3),
  .md :global(h4),
  .md :global(h5),
  .md :global(h6) {
    margin: 12px 0 4px;
    font-family: var(--disp);
    font-size: 14px;
    font-weight: 600;
    letter-spacing: 0.02em;
    text-transform: uppercase;
  }

  /* The README's title: one step up, the only one. */
  .md :global(h1) {
    font-size: 16px;
  }

  .md :global(h4),
  .md :global(h5),
  .md :global(h6) {
    color: var(--muted);
  }

  .md p,
  .md :global(ul),
  .md :global(ol),
  .md blockquote {
    margin: 6px 0;
  }

  .md :global(ul),
  .md :global(ol) {
    padding-left: 18px;
  }

  /* A nested item, by how deep; `markdown.ts` caps the depth at four. */
  .md .d1 {
    margin-left: 16px;
  }

  .md .d2 {
    margin-left: 32px;
  }

  .md .d3 {
    margin-left: 48px;
  }

  .md .d4 {
    margin-left: 64px;
  }

  .md blockquote {
    padding-left: 10px;
    border-left: 2px solid var(--hair2);
    color: var(--muted);
  }

  .md code {
    font-family: var(--mono);
    font-size: 11.5px;
  }

  .md pre {
    margin: 6px 0;
    padding: 8px;
    overflow-x: auto;
    background: var(--raised);
    border-radius: 3px;
    font-family: var(--mono);
    font-size: 11.5px;
    line-height: 1.45;
    white-space: pre;
    overflow-wrap: normal;
  }

  .md hr {
    margin: 10px 0;
    border: 0;
    border-top: 1px solid var(--hair);
  }

  .md-img {
    color: var(--faint);
  }

  .md-img::before {
    content: "[";
  }

  .md-img::after {
    content: "]";
  }

  .md-link {
    padding: 0;
    border: 0;
    background: none;
    color: var(--link);
    font: inherit;
    text-align: left;
    text-decoration: underline;
    cursor: pointer;
  }
</style>

<!--
  A Confluence page body, rendered from the nodes `storage-format.ts` parsed
  (#285).

  **Real elements, never a string.** `{@html}` is forbidden here — the rule is
  enforced by `shell/house-rules.test.ts`, and roadmap §4 gotcha 7 is why — so
  a page's markup is turned into a tree first and this walks it. Every string
  that arrives is interpolated as text, which is to say Svelte escapes it, and
  an element exists on screen only because the parser's allow-list named it.
  There is no code path that carries an attribute other than a link's `href`.

  It recurses by importing itself, which is Svelte 5's replacement for the
  deprecated `<svelte:self>`.

  A **link** is a `<button>`, not an `<a>`. An anchor with an absolute `href`,
  clicked in a Tauri webview, navigates the webview: the app becomes a browser
  showing a wiki, with no way back. So a body link does what *Open in browser*
  does — hands the URL to the OS — and the callback is the caller's, which
  keeps this component free of the IPC and of the toast a refusal needs.
-->
<script lang="ts">
  import Self from "./StorageBody.svelte";
  import type { StorageNode } from "./storage-format";

  let {
    nodes,
    onopenlink,
  }: {
    nodes: StorageNode[];
    /**
     * Open a link's target. Called only with an `http:`/`https:` URL — the
     * parser dropped every other scheme before this component saw it.
     */
    onopenlink: (href: string) => void;
  } = $props();
</script>

<!--
  Keyed by index, which is the honest key: a node has no identity of its own,
  the whole list is rebuilt whenever the body changes, and nothing here holds
  state that a re-order would have to follow.
-->
{#each nodes as node, index (index)}{#if node.kind === "text"}{node.text}{:else if node.kind ===
    "macro"}<span class="ac">{node.label}</span>{:else if node.tag === "a"}{#if node.href}{@const href =
      node.href}<button class="lnk" type="button" onclick={() => onopenlink(href)}><Self nodes={node.children} {onopenlink} /></button>{:else}<Self
      nodes={node.children}
      {onopenlink}
    />{/if}{:else if node.children.length === 0}<svelte:element this={node.tag} />{:else}<svelte:element this={node.tag}><Self nodes={node.children} {onopenlink} /></svelte:element>{/if}{/each}

<style>
  /*
    A macro, named. Deliberately loud: the reason it is here is that the
    section it sits in will refuse a knobas-side edit, and a reader who cannot
    see the boundary cannot see why.
  */
  .ac {
    display: inline-block;
    padding: 1px 6px;
    border: 1px dashed var(--hair2);
    border-radius: 3px;
    color: var(--faint);
    font-family: var(--mono);
    font-size: 11px;
    text-transform: lowercase;
    white-space: nowrap;
  }

  /*
    A link. A button by necessity (see the header), so it is dressed as the
    link it stands for rather than as a control.
  */
  .lnk {
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

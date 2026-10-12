<!--
  A text block (ADR-0020): a source's prose -- a commit message, a pull
  request's description, a repository's README. Plain text is drawn as it is.
  Markdown (`markdown: true`) is drawn legibly -- headings, lists, code -- by
  `MarkdownText`, which reads its structure and never renders it as markup
  (rule 5, gotcha 7).
-->
<script lang="ts">
  import MarkdownText from "./MarkdownText.svelte";

  let {
    text,
    markdown,
    onopenurl,
  }: {
    text: string;
    markdown: boolean;
    /** Open a web link a Markdown text carries. */
    onopenurl?: (url: string) => void;
  } = $props();
</script>

{#if markdown}
  <MarkdownText {text} {onopenurl} />
{:else}
  <p class="tb">{text}</p>
{/if}

<style>
  .tb {
    font-size: 12.5px;
    line-height: 1.55;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
</style>

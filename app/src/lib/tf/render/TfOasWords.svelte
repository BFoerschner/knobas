<!--
  A description in the API explorer (ux31): Markdown through the file view's own renderer (sanitized as a README
  is), plain text where it holds nothing Markdown would draw, held to `max` characters with *Show all*.
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  import { clampWords, needsMarkdown } from "../../render/openapi-view";
  import type { OasState } from "./oas.svelte";

  let {
    text,
    st,
    k,
    max = 0,
    prose = null,
    block = false,
  }: {
    text: string;
    st: OasState;
    /** What *Show all* is kept by. */
    k: string;
    /** About how many characters are drawn before *Show all*; 0 for all. */
    max?: number;
    prose?: Snippet<[string]> | null;
    /** A paragraph of its own, not a field's line. */
    block?: boolean;
  } = $props();

  const all = $derived(st.expand.has(`${k}#all`));
  const held = $derived(clampWords(text, all ? 0 : max));
  const md = $derived(prose !== null && needsMarkdown(held.text));
</script>

{#if held.text !== ""}
  {#if md}<div class="tf-oas-prose">{@render prose!(held.text)}</div>{:else if block}<p>{held.text}</p>{:else}{held.text}{/if}
  {#if held.cut}<button type="button" class="tf-oas-more" onclick={() => st.expand.add(`${k}#all`)}>Show all</button>{/if}
{/if}

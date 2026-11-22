<!--
  A description in the API explorer (ux31). Inline Markdown (`code`, *emphasis*, **strong**, [links](…)) is read by
  `inlineMarkdown` and drawn as text runs, never as markup; anything more (a list, a heading, a fence, a table) goes
  through the file view's own renderer, sanitized as a README is. Held to `max` characters with *Show all*. A link's
  address opens through the app's way out, as a link in Markdown does.
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  import { clampWords, inlineMarkdown, needsBlocks } from "../../render/openapi-view";
  import { routeWeb } from "../../shell/open-web";
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
  const blocks = $derived(prose !== null && needsBlocks(held.text));
  const paras = $derived(blocks ? [] : inlineMarkdown(held.text));

  function open(event: MouseEvent, href: string) {
    if (!/^https?:\/\//i.test(href)) return;
    event.preventDefault();
    void routeWeb(href).catch(() => {});
  }
</script>

{#if held.text !== ""}
  {#if blocks}<div class="tf-oas-prose">{@render prose!(held.text)}</div>
  {:else}
    {#each paras as runs, p (p)}
      {#if block || paras.length > 1}<p class="tf-oas-para">{@render line(runs)}</p>{:else}{@render line(runs)}{/if}
    {/each}
  {/if}
  {#if held.cut}<button type="button" class="tf-oas-more" onclick={() => st.expand.add(`${k}#all`)}>Show all</button>{/if}
{/if}

{#snippet line(runs: ReturnType<typeof inlineMarkdown>[number])}{#each runs as r, i (i)}{#if r.k === "code"}<code>{r.t}</code>{:else if r.k === "em"}<em>{r.t}</em>{:else if r.k === "strong"}<b>{r.t}</b>{:else if r.k === "link"}<a class="tf-oas-link" href={r.href} title={r.href} onclick={(e) => open(e, r.href!)}>{r.t}</a>{:else}{r.t}{/if}{/each}{/snippet}

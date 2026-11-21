<!--
  A Docs page's block of code (ADR-0051): plain first, then coloured by the highlighter where the block names a
  language it knows, the find bar's marks kept over the colours. The block holds its highlighting job while it is
  drawn, so a change of highlighter colours it again in place, and cancels it when it goes. A token is a span with a
  colour, so the block's size never changes when the colours land.
-->
<script lang="ts">
  import { onDestroy, untrack } from "svelte";

  import { renderPort, type LineTokens, type RenderPort } from "../../render/port";
  import { codeRuns } from "./code-runs";

  let { code, language, stems, render = renderPort() }: { code: string; language: string | null; stems: string[]; render?: RenderPort } = $props();

  const lines = $derived(code.split("\n"));
  let tokens = $state<(LineTokens | undefined)[]>([]);

  // One job for the text the block was drawn with: a block's code does not change while it is drawn.
  const job = untrack(() =>
    language && language.trim() !== ""
      ? render.highlight(
          { text: code, lang: language },
          {
            onLines: (from, got) => {
              const next = [...tokens];
              got.forEach((line, i) => (next[from + i] = line));
              tokens = next;
            },
          },
        )
      : null,
  );
  onDestroy(() => job?.cancel());
</script>

<pre><code
    >{#each lines as line, n (n)}{#if n > 0}{"\n"}{/if}{#each codeRuns(line, tokens[n], stems) as run, i (i)}{#if run.hit}<mark
          >{#if run.kind === ""}{run.text}{:else}<span class="tf-hl-{run.kind}">{run.text}</span>{/if}</mark
        >{:else if run.kind === ""}{run.text}{:else}<span class="tf-hl-{run.kind}">{run.text}</span>{/if}{/each}{/each}</code
  ></pre>

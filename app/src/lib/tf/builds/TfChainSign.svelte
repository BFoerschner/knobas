<!--
  A pipeline's place in its chain, in the Builds header's line (the build-header pass, 2026-11-20; design system,
  *Controls*, the sign): *Step 64 of 70*, a sign of a few words, so a chain of seventy long names can't wrap the line
  or push anything. Björn: "Teamcity configuration, builds <huge repo link> step 64 of the chain x > y > z > foo > bar >
  etc. Make it that it doesn't destroy the ui."

  The chain is in its bubble (`TfSign`: hover, focus, a press pins it), first step first, numbered, this pipeline's in
  ink and scrolled to when it opens; each step opens its pipeline. Closed, the chain's sentence (*Step 64 of the chain
  A › B › …*) is the sign's description.
-->
<script lang="ts">
  import type { ChainStep } from "../../ipc/entity";
  import type { Layers } from "../layers.svelte";
  import TfSign from "../TfSign.svelte";
  import { pipelineHash } from "./builds";
  import { chainPlace, chainSentence } from "./chain";

  let { chain, pipelineId, layers }: { chain: readonly ChainStep[]; pipelineId: string; layers?: Layers | undefined } = $props();

  const place = $derived(chainPlace(chain, pipelineId));
  const sentence = $derived(chainSentence(chain, pipelineId));

  /** Opened, the list scrolls to this pipeline's step, inside itself only: never the page. */
  function toHere(list: HTMLElement) {
    const here = list.querySelector<HTMLElement>("[aria-current]");
    if (here === null || list.clientHeight === 0) return;
    list.scrollTop = Math.max(0, here.offsetTop - list.offsetTop - (list.clientHeight - here.offsetHeight) / 2);
  }
</script>

{#if place !== null}
  <TfSign class="tf-chainsign" {layers}>
    {#snippet face()}<svg viewBox="0 0 16 16" aria-hidden="true"
        ><path d="M6.5 9.5 9.5 6.5M7 4.5l1.25-1.25a2.47 2.47 0 0 1 3.5 3.5L10.5 8M9 11.5l-1.25 1.25a2.47 2.47 0 0 1-3.5-3.5L5.5 8" /></svg
      ><span class="tf-sign-word">Step {place.step}</span><span class="tf-sign-state"><span class="tf-sr">{" "}</span>of {place.of}</span>{/snippet}
    {#snippet bubble({ open, uid })}
      <span class="tf-signbubble-what">The chain, first step first</span>
      <span class="tf-sr" id="{uid}-desc">{sentence}</span>
      {#if open}
        <ol class="tf-chainlist" {@attach toHere}>
          {#each chain as step, i (i)}
            <li>
              <a class="tf-wrapall" href={pipelineHash(step.pipeline)} aria-current={step.pipeline === pipelineId ? "page" : undefined}
                >{step.name}</a
              >
            </li>
          {/each}
        </ol>
      {/if}
    {/snippet}
  </TfSign>
{/if}

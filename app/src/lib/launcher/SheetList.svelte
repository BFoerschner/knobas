<!--
  What the Tab sheet draws under its pinned row: its actions, or a flow's step
  and choices (#2426, #2427). `SheetMode` holds all of it; this draws it and
  hands the clicks back.

  Keys from a clicked chip or row land here, so Esc goes back to the results
  and never reaches the dialog; a click gives the box its focus back.
-->
<script lang="ts">
  import { JOB_LINK_RELATIONS } from "../tf/job/link-relations";
  import { aimed } from "./aim";
  import type { SheetMode } from "./sheet-mode.svelte";

  let {
    sheet,
    id,
    optionId,
    onkeydown,
    onclick,
  }: {
    sheet: SheetMode;
    id: string;
    optionId: (index: number) => string;
    onkeydown: (event: KeyboardEvent) => void;
    onclick: () => void;
  } = $props();
</script>

{#if sheet.current}
  {@const current = sheet.current}
  <div {id} class="tf-ln-list tf-ln-sheet" role="listbox" tabindex="-1" aria-label="Actions on the selected result" {onkeydown} {onclick}>
    {#if current.flow}
      {@const flow = current.flow}
      <!-- A flow's step: its title, the picks so far, and its choices as the box narrows them. -->
      <div class="tf-ln-h">
        <span>{sheet.step?.title ?? ""}</span>
        {#if flow.picks.length > 0}<span class="tf-ln-s">{flow.picks.join(" → ")}</span>{/if}
      </div>
      {#each sheet.choices as choice, index (choice.id)}
        <button
          id={optionId(index)}
          class="tf-ln-opt"
          class:tf-ln-on={index === current.selected}
          role="option"
          aria-selected={index === current.selected}
          onclick={() => sheet.pick(choice)}
          onpointermove={aimed(() => sheet.hover(index))}
        >
          <span class="tf-ln-nm">{choice.label}</span>
          <span class="tf-ln-s">{choice.detail}</span>
        </button>
      {/each}
      {#if flow.choices === null}
        <p class="tf-ln-none">Reading…</p>
      {:else if sheet.choices.length === 0}
        <p class="tf-ln-none">{current.filter === "" ? "Nothing to pick here." : `Nothing matches “${current.filter}”.`}</p>
      {/if}
    {/if}
    {#each sheet.shown as action, index (action.id)}
      {@const bound = sheet.boundTo(action)}
      {#if action.does.kind === "link"}
        <!-- Not a button: the relation chips inside it are buttons of their own. -->
        <div
          id={optionId(index)}
          class="tf-ln-opt tf-ln-link"
          class:tf-ln-on={index === current.selected}
          role="option"
          tabindex="-1"
          aria-selected={index === current.selected}
          onpointermove={aimed(() => sheet.hover(index))}
        >
          <button type="button" class="tf-ln-nm" onclick={() => sheet.perform(action)}>{action.label}</button>
          <span class="tf-fchips" role="group" aria-label="Relation">
            {#each JOB_LINK_RELATIONS as relation, r (relation.id)}
              <button type="button" tabindex="-1" aria-pressed={r === current.relation} onclick={() => sheet.relate(r)}>{relation.label}</button>
            {/each}
          </span>
          {#if bound}<kbd class="tf-ln-chord" title="Its key">{bound}</kbd>{/if}
        </div>
      {:else}
        <button
          id={optionId(index)}
          class="tf-ln-opt"
          class:tf-ln-on={index === current.selected}
          role="option"
          aria-selected={index === current.selected}
          onclick={() => sheet.perform(action)}
          onpointermove={aimed(() => sheet.hover(index))}
        >
          <span class="tf-ln-nm">{action.label}</span>
          <span class="tf-ln-s">{action.detail}</span>
          {#if bound}<kbd class="tf-ln-chord" title="Its key">{bound}</kbd>{/if}
        </button>
      {/if}
    {/each}
    {#if !current.flow && sheet.shown.length === 0}
      <p class="tf-ln-none">No action matches “{current.filter}”.</p>
    {/if}
  </div>
{/if}

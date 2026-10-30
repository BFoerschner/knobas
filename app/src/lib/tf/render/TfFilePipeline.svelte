<!--
  The pipeline bar (#2264, spec #2234 step 5; `mockups/code-area/pipeline.html`, state *editing*): the steps
  between the open file's bytes and its renderer, edited in place.

  `bytes → step → step → renderer`. Each step has a grip (drag it, or focus it and press ← or →), its name, its
  arguments inline (a jq filter, a decompressor, a format) and a remove button. *Add step* lists the built-in
  steps (`render/pipeline.ts`). A changed jq filter is sent a moment after typing stops, or at once on `Enter`;
  every other change at once. *Reset* drops the pipeline. Saving one as a pattern's default, and the person's own
  command steps, are #2265's.
-->
<script lang="ts">
  import { untrack } from "svelte";

  import type { PipeFormat, PipeStep } from "../../ipc/entity";
  import { FORMATS, STEP_CHOICES, moveStep, nameAfter, stepName, withStep } from "../../render/pipeline";
  import { layers as sharedLayers, type Layers } from "../layers.svelte";

  let {
    steps,
    path,
    shownAs,
    failedAt = null,
    running = false,
    onchange,
    onreset,
    layers = sharedLayers,
  }: {
    steps: PipeStep[];
    /** The open file's path: the menu's defaults read its extension. */
    path: string;
    /** The renderer the output is drawn by, as the bar's last box says it. */
    shownAs: string;
    /** The step that failed, from 0. */
    failedAt?: number | null;
    running?: boolean;
    onchange: (steps: PipeStep[]) => void;
    onreset: () => void;
    layers?: Layers | undefined;
  } = $props();

  /** How long typing in a jq filter rests before it runs. */
  const SETTLE_MS = 350;

  let adding = $state(false);
  let face = $state<HTMLButtonElement | null>(null);
  let menu = $state<HTMLDivElement | null>(null);
  let bar = $state<HTMLDivElement | null>(null);
  let typing: ReturnType<typeof setTimeout> | null = null;
  let dragged: number | null = null;

  $effect(() => {
    if (!adding) return;
    const off = untrack(() => layers.add({ kind: "dialog", close: () => closeMenu() }));
    queueMicrotask(() => menu?.querySelector<HTMLElement>("button")?.focus());
    const outside = (event: MouseEvent) => {
      const target = event.target as Node | null;
      if (target && !menu?.contains(target) && !face?.contains(target)) adding = false;
    };
    window.addEventListener("click", outside, true);
    return () => {
      off();
      window.removeEventListener("click", outside, true);
    };
  });
  $effect(() => () => {
    if (typing !== null) clearTimeout(typing);
  });

  function closeMenu() {
    adding = false;
    face?.focus();
  }

  function menuKeys(event: KeyboardEvent) {
    const items = [...(menu?.querySelectorAll<HTMLElement>("button") ?? [])];
    const at = items.indexOf(document.activeElement as HTMLElement);
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      closeMenu();
    } else if (event.key === "ArrowDown" && items.length > 0) {
      event.preventDefault();
      items[(at + 1) % items.length]?.focus();
    } else if (event.key === "ArrowUp" && items.length > 0) {
      event.preventDefault();
      items[(at - 1 + items.length) % items.length]?.focus();
    }
  }

  function add(make: (name: string) => PipeStep) {
    adding = false;
    onchange([...steps, make(nameAfter(path, steps))]);
  }

  function remove(index: number) {
    onchange(steps.filter((_, i) => i !== index));
  }

  function move(from: number, to: number) {
    if (to < 0 || to >= steps.length) return;
    onchange(moveStep(steps, from, to));
    queueMicrotask(() => bar?.querySelectorAll<HTMLElement>(".tf-pgrip")[to]?.focus());
  }

  function gripKeys(event: KeyboardEvent, index: number) {
    if (event.key === "ArrowLeft") {
      event.preventDefault();
      move(index, index - 1);
    } else if (event.key === "ArrowRight") {
      event.preventDefault();
      move(index, index + 1);
    }
  }

  function filterTyped(index: number, filter: string, now: boolean) {
    if (typing !== null) clearTimeout(typing);
    typing = null;
    const send = () => onchange(withStep(steps, index, { step: "jq", filter }));
    if (now) send();
    else typing = setTimeout(send, SETTLE_MS);
  }

  function filterKeys(event: KeyboardEvent, index: number) {
    if (event.key === "Enter") {
      event.preventDefault();
      filterTyped(index, (event.currentTarget as HTMLInputElement).value, true);
    }
  }

  function drop(event: DragEvent, index: number) {
    event.preventDefault();
    if (dragged !== null) move(dragged, index);
    dragged = null;
  }
</script>

<div class="tf-pipe" bind:this={bar}>
  <div class="tf-psteps">
    <span class="tf-plab">Pipeline</span><span class="tf-psrc">bytes</span><span class="tf-parrow" aria-hidden="true">→</span>
    {#each steps as step, i (i)}
      {@const name = stepName(step)}
      <span
        class="tf-pstep"
        class:tf-pbad={failedAt === i}
        role="group"
        aria-label="Step {i + 1}, {name}"
        draggable="true"
        ondragstart={() => (dragged = i)}
        ondragover={(event) => event.preventDefault()}
        ondrop={(event) => drop(event, i)}
      >
        <button type="button" class="tf-pgrip" aria-label="Move {name}" title="Drag, or press ← or →" onkeydown={(event) => gripKeys(event, i)}
          ><svg width="10" height="12" viewBox="0 0 10 12" fill="currentColor" aria-hidden="true"
            ><circle cx="3" cy="2" r="1" /><circle cx="7" cy="2" r="1" /><circle cx="3" cy="6" r="1" /><circle cx="7" cy="6" r="1" /><circle
              cx="3"
              cy="10"
              r="1"
            /><circle cx="7" cy="10" r="1" /></svg
          ></button
        >
        {#if step.step === "jq"}
          <span class="tf-pname">jq</span>
          <input
            class="tf-parg tf-pfilter"
            aria-label="jq filter"
            spellcheck="false"
            value={step.filter}
            oninput={(event) => filterTyped(i, (event.currentTarget as HTMLInputElement).value, false)}
            onkeydown={(event) => filterKeys(event, i)}
          />
        {:else if step.step === "pretty"}
          <span class="tf-pname">pretty-print</span>
          <select
            class="tf-parg"
            aria-label="Format"
            value={step.format}
            onchange={(event) => onchange(withStep(steps, i, { step: "pretty", format: (event.currentTarget as HTMLSelectElement).value as PipeFormat }))}
          >
            {#each FORMATS as f (f.format)}<option value={f.format}>{f.label}</option>{/each}
          </select>
        {:else if step.step === "convert"}
          {@const from = step.from}
          {@const to = step.to}
          <span class="tf-pname">convert</span>
          <select
            class="tf-parg"
            aria-label="From"
            value={from}
            onchange={(event) => onchange(withStep(steps, i, { step: "convert", from: (event.currentTarget as HTMLSelectElement).value as PipeFormat, to }))}
          >
            {#each FORMATS as f (f.format)}<option value={f.format}>{f.label}</option>{/each}
          </select>
          <select
            class="tf-parg"
            aria-label="To"
            value={to}
            onchange={(event) => onchange(withStep(steps, i, { step: "convert", from, to: (event.currentTarget as HTMLSelectElement).value as PipeFormat }))}
          >
            {#each FORMATS as f (f.format)}<option value={f.format}>{f.label}</option>{/each}
          </select>
        {:else}
          <span class="tf-pname">{name}</span>
        {/if}
        <button type="button" class="tf-px" aria-label="Remove {name}" title="Remove" onclick={() => remove(i)}
          ><svg width="10" height="10" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8" /></svg
          ></button
        >
      </span><span class="tf-parrow" aria-hidden="true">→</span>
    {/each}
    <span class="tf-pstep tf-prenderer">{shownAs}</span>
    <span class="tf-pmenu">
      <button bind:this={face} type="button" class="tf-btn tf-sm tf-ghost" aria-haspopup="menu" aria-expanded={adding} onclick={() => (adding = !adding)}
        >+ Add step</button
      >
      {#if adding}
        <!-- svelte-ignore a11y_interactive_supports_focus -->
        <div class="tf-menu" role="menu" tabindex="-1" aria-label="Add step" bind:this={menu} onkeydown={menuKeys}>
          <div class="tf-rsw-mh">Built in</div>
          {#each STEP_CHOICES as choice (choice.id)}
            <button type="button" role="menuitem" onclick={() => add(choice.make)}
              ><span>{choice.label}</span><span class="tf-rsw-kb">{choice.io}</span></button
            >
          {/each}
        </div>
      {/if}
    </span>
  </div>
  <div class="tf-acts">
    <span class="tf-dim tf-small" aria-live="polite">{running ? "Running…" : "Not saved"}</span>
    <button type="button" class="tf-btn tf-sm tf-ghost" onclick={onreset}>Reset</button>
  </div>
</div>

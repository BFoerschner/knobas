<!--
  The keep plate (ADR-0055, the decisions' 3.9): while an unreadable theme is drawn on trial, *Keep ⟨name⟩? Going back
  to ⟨name⟩ in 15 s.* with *Go back now* and *Keep it*, or after an edit under the floor *Undoing that change in 15 s.*
  with *Undo now*. Focus starts on going back, so `Enter`, `Esc` and the time running out all go back. The plate stands
  in knobas Light's own colours (`scopeValues` of the built-in), so it reads whatever the window has become. Above
  the toasts, mounted once by the shell.
-->
<script lang="ts">
  import { tick, untrack } from "svelte";

  import { layers } from "./layers.svelte";
  import { cssVars } from "./theme/paint";
  import { BUILTINS } from "./theme/builtins";
  import { scopeValues, themes as appThemes, type Themes } from "./themes.svelte";

  let { themes = appThemes, now = () => Date.now() }: { themes?: Themes; now?: () => number } = $props();

  const trial = $derived(themes.trial);
  const KNOBAS_LIGHT = BUILTINS.find((t) => t.id === "knobas-light")!;
  let left = $state(15);
  let back = $state<HTMLButtonElement | null>(null);

  $effect(() => {
    const t = trial;
    if (!t) return;
    const count = () => (left = Math.max(0, Math.ceil((t.endsAt - now()) / 1000)));
    count();
    const every = setInterval(count, 250);
    const off = untrack(() => layers.add({ kind: "panel", close: () => themes.endTrial() }));
    void tick().then(() => back?.focus());
    return () => {
      clearInterval(every);
      off();
    };
  });
</script>

{#if trial}
  <div class="tf-apt-keep" role="alertdialog" aria-label={trial.kind === "use" ? `Keep ${trial.theme.name}?` : "Undo that change?"} {@attach cssVars(() => scopeValues(KNOBAS_LIGHT, null))}>
    {#if trial.kind === "use"}
      <span>Keep {trial.theme.name}? Going back to {trial.backTo} in <b>{left}</b> s.</span>
      <button type="button" class="tf-btn tf-sm" bind:this={back} onclick={() => themes.endTrial()}>Go back now</button>
    {:else}
      <span>{trial.theme.name} is hard to read now. Undoing that change in <b>{left}</b> s.</span>
      <button type="button" class="tf-btn tf-sm" bind:this={back} onclick={() => themes.endTrial()}>Undo now</button>
    {/if}
    <button type="button" class="tf-btn tf-sm tf-primary" onclick={() => themes.keepTrial()}>Keep it</button>
  </div>
{/if}

<!--
  Settings › Appearance (#831, R-13a.8): prototype-6's `STGV.appearance`. The
  same setting as the top bar's toggle (`theme.svelte.ts`). Below it, *State
  colours* (#2466), which reads the database and so waits for it: Theme
  draws before the database is up.
-->
<script lang="ts">
  import type { Theme, ThemeChoice } from "../theme.svelte";
  import TfStateColours, { type StateColourPorts } from "./TfStateColours.svelte";

  let { theme, ready = true, ports }: { theme: Theme; ready?: boolean; ports?: StateColourPorts } = $props();

  const CHOICES: readonly (readonly [ThemeChoice, string])[] = [
    ["system", "Like the system"],
    ["light", "Light"],
    ["dark", "Dark"],
  ];
</script>

<header class="tf-stg-h"><h1>Appearance</h1></header>
<div class="tf-stg-row">
  <div class="tf-stg-l">Theme</div>
  <div class="tf-stg-c">
    <div class="tf-seg" role="group" aria-label="Theme">
      {#each CHOICES as [key, label] (key)}
        <button aria-pressed={theme.choice === key} onclick={() => theme.set(key)}>{label}</button>
      {/each}
    </div>
  </div>
</div>
{#if ready}
  <TfStateColours {...ports ? { ports } : {}} />
{/if}

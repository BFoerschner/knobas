<!--
  Settings › Appearance (#831, R-13a.8): prototype-6's `STGV.appearance`. The
  same setting as the top bar's toggle (`theme.svelte.ts`): *Mode*, light or dark (its colours are Settings ›
  Appearance › Theme since ADR-0055). Below it, *State
  colours* (#2466), which reads the database and so waits for it: Theme
  draws before the database is up. Between them, *Zoom* (`TfZoomSetting.svelte`), which needs no database either.
  The faces and their sizes, *Code font* and *Ligatures in code* among them, are Settings › Fonts since 2026-11-18
  (`TfFontsSection.svelte`).
-->
<script lang="ts">
  import type { Theme, ThemeChoice } from "../theme.svelte";
  import TfStateColours, { type StateColourPorts } from "./TfStateColours.svelte";
  import TfZoomSetting from "./TfZoomSetting.svelte";

  let {
    theme,
    ready = true,
    ports,
  }: { theme: Theme; ready?: boolean; ports: StateColourPorts } = $props();

  const CHOICES: readonly (readonly [ThemeChoice, string])[] = [
    ["system", "Like the system"],
    ["light", "Light"],
    ["dark", "Dark"],
  ];
</script>

<header class="tf-stg-h"><h1>Appearance</h1></header>
<div class="tf-stg-row">
  <div class="tf-stg-l">Mode</div>
  <div class="tf-stg-c">
    <div class="tf-seg" role="group" aria-label="Mode">
      {#each CHOICES as [key, label] (key)}
        <button aria-pressed={theme.choice === key} onclick={() => theme.set(key)}>{label}</button>
      {/each}
    </div>
  </div>
</div>
<TfZoomSetting />
{#if ready}
  <TfStateColours {ports} />
{/if}

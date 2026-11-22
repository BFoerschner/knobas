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
  import { colourBlind as sharedColourBlind, confusions, type ColourBlind } from "../colour-blind.svelte";
  import { themes as windowThemes, type Themes } from "../themes.svelte";
  import type { Theme, ThemeChoice } from "../theme.svelte";
  import TfStateColours, { type StateColourPorts } from "./TfStateColours.svelte";
  import TfZoomSetting from "./TfZoomSetting.svelte";

  let {
    theme,
    ready = true,
    ports,
    themes = windowThemes,
    colourBlind = sharedColourBlind,
  }: { theme: Theme; ready?: boolean; ports: StateColourPorts; themes?: Themes; colourBlind?: ColourBlind } = $props();

  /** Each side's theme and the states in it that read alike to a colour-blind reader. */
  const SIDES = [
    ["light", "While light", "knobas-light-cb"],
    ["dark", "While dark", "knobas-dark-cb"],
  ] as const;

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
<div class="tf-stg-row tf-cbm">
  <label class="tf-stg-l" for="tf-cbm-on"
    >Colour blindness<small
      >Warns when a theme's state colours read alike to people with protanopia, deuteranopia or tritanopia (Machado, 2009).</small
    ></label
  >
  <div class="tf-stg-c tf-cbm-c">
    <button id="tf-cbm-on" type="button" class="tf-stg-sw" role="switch" aria-checked={colourBlind.on} aria-label="Warn about colours that read alike" onclick={() => colourBlind.set(!colourBlind.on)}
      ><i></i></button
    >
    {#if colourBlind.on}
      {#each SIDES as [kind, label, safe] (kind)}
        {@const drawn = themes.appDrawn(kind)}
        {@const pairs = confusions(themes.colours(kind))}
        <div class="tf-cbm-side" data-side={kind}>
          <b>{label}, {drawn.name}:</b>
          {#if pairs.length === 0}
            <span>every state reads apart.</span>
          {:else}
            <ul>
              {#each pairs as pair (`${pair.a}-${pair.b}-${pair.vision}`)}
                <li>{pair.sentence}</li>
              {/each}
            </ul>
            {#if drawn.id !== safe}
              <button type="button" class="tf-btn tf-sm" onclick={() => themes.chooseApp(safe)}>Use {themes.find(safe)?.name}</button>
            {/if}
          {/if}
        </div>
      {/each}
    {/if}
  </div>
</div>
{#if ready}
  <TfStateColours {ports} />
{/if}

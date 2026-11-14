<!--
  The key panel (#2429): the keys large as they are pressed. The box's keys go
  to `panel.press`, which the launcher asks first; this draws the answer.
-->
<script lang="ts">
  import type { KeyPanelMode } from "./key-panel.svelte";
  import { kindInSentence } from "./keys";
  import { chordParts, keyHint } from "../shell/platform";

  let { panel }: { panel: KeyPanelMode } = $props();
</script>

<div class="tf-ln-bind" role="group" aria-label="Bind keys">
  <p class="tf-ln-bind-what">
    {#if panel.target.kind === "list"}
      The keys that open <strong>{panel.target.label}</strong> from anywhere
    {:else}
      The keys that run <strong>{panel.target.label}</strong> on any {kindInSentence(panel.target.on)} you have selected
    {/if}
  </p>
  <div class="tf-ln-bind-keys" aria-live="polite">
    {#if panel.chord}
      {#each chordParts(panel.chord) as glyph, i (i)}<kbd class="tf-ln-bigkey">{glyph}</kbd>{/each}
    {:else if panel.held.length > 0}
      {#each chordParts(panel.held.join("")) as glyph, i (i)}<kbd class="tf-ln-bigkey tf-ln-held">{glyph}</kbd>{/each}
      <span class="tf-ln-bind-hint">… and a letter or digit</span>
    {:else}
      <span class="tf-ln-bind-hint">{keyHint("Press a modifier and a letter or digit, such as ⌘⇧O")}</span>
    {/if}
  </div>
  {#if panel.refused}<p class="tf-ln-bind-refused" role="alert">{panel.refused}</p>{/if}
  <!-- The Controls order: the extra action (Clear) first, then a ghost Cancel, then the primary. -->
  <div class="tf-ln-bind-btns">
    <button type="button" class="tf-btn tf-sm" onclick={() => panel.clear()}>Clear</button>
    <button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => panel.cancel()}>Cancel</button>
    <button type="button" class="tf-btn tf-sm tf-primary" onclick={() => void panel.keep()}>OK</button>
  </div>
  <p class="tf-ln-bind-hint">Enter keeps the keys, Backspace clears them, Esc leaves things as they were.</p>
</div>

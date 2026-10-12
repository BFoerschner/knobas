<!--
  A log block (ADR-0020): monospace text with meaningful lines, a build log's
  tail. `truncated` says this is the end of something longer, and the note says
  so above it, where the reader starts.

  **It opens scrolled to its end** (#665): a tail is read from the bottom,
  where a build failed, and a block taller than its box would otherwise open on
  the least interesting line. A step's script, the other thing drawn as a log,
  usually fits its box, and then there is nothing to scroll.
-->
<script lang="ts">
  let { text, truncated }: { text: string; truncated: boolean } = $props();

  let box = $state<HTMLPreElement | null>(null);

  // On mount and whenever the text changes (a deferred tab's read landing).
  $effect(() => {
    void text;
    if (box) box.scrollTop = box.scrollHeight;
  });
</script>

{#if truncated}
  <p class="lb-note">The end of a longer log.</p>
{/if}
<pre class="log lb" bind:this={box}>{text}</pre>

<style>
  .lb-note {
    margin: 0 0 4px;
    color: var(--muted);
    font-size: 12px;
  }

  .lb {
    max-height: 60vh;
    color: var(--text);
  }
</style>

<!--
  A log block (ADR-0020): monospace text with meaningful lines, a build log's
  tail. `truncated` says this is the end of something longer, and the note says
  so above it, where the reader starts.
-->
<script lang="ts">
  let { text, truncated }: { text: string; truncated: boolean } = $props();
</script>

{#if truncated}
  <p class="lb-note">The end of a longer log.</p>
{/if}
<pre class="log lb">{text}</pre>

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

<!--
  A pipeline's latest run as one 12 px glyph, for the Builds rail (the rail-tree pass, 2026-11-13,
  `mockups/builds/rail-tree.md`). A pass is a quiet tick and a failure a filled disc, so a scan down the rail's left
  edge finds the red; a running run's dot pulses, the one motion nobody started (design system, Motion). The tones are
  the rail's own (`--ok`, `--err`…), which a selected row re-declares for the ink (`builds.css`). Spread `railGlyph`.
-->
<script lang="ts">
  import type { GlyphTone } from "./builds";

  let { tone, word }: { tone: GlyphTone; word: string } = $props();
</script>

<svg class="tf-rg tf-rg-{tone}" viewBox="0 0 16 16" width="12" height="12" role="img" aria-label={word}>
  {#if tone === "ok"}
    <path class="tf-rg-line" d="M3.5 8.5 6.5 11.5 12.5 4.5" stroke-width="2" />
  {:else if tone === "err"}
    <circle cx="8" cy="8" r="7" fill="currentColor" />
    <path class="tf-rg-x" d="M5.4 5.4l5.2 5.2M10.6 5.4l-5.2 5.2" stroke-width="1.8" />
  {:else if tone === "warn"}
    <path d="M8 1.5 15 14H1z" fill="currentColor" />
    <path class="tf-rg-x" d="M8 6v3.6" stroke-width="1.7" />
    <circle class="tf-rg-dot" cx="8" cy="11.8" r=".95" />
  {:else if tone === "run"}
    <circle class="tf-rg-pulse" cx="8" cy="8" r="5" fill="currentColor" />
  {:else if tone === "q"}
    <circle class="tf-rg-dash" cx="8" cy="8" r="5.6" stroke-width="1.6" stroke-dasharray="2.4 2.2" />
  {:else if tone === "muted"}
    <circle class="tf-rg-line" cx="8" cy="8" r="5.6" stroke-width="1.6" />
    <path class="tf-rg-line" d="M4.2 11.8 11.8 4.2" stroke-width="1.6" />
  {:else}
    <path class="tf-rg-line" d="M4.5 8h7" stroke-width="1.6" />
  {/if}
</svg>

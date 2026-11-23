<!--
  The ✕ of a dismiss, a clear or a failed check, drawn (2026-11-23, Fable's ruling in the ux30 card-drag gate). Inter has
  no U+2715, so the first ✕ a window drew made Blink search the system's fonts for one: 49 ms at CPU 4x inside the frame
  of the first drop on the ticket board, whose toast was the first ✕ on the page. Drawn, it costs no search, and it is
  the same on the Mac and on Windows.

  The glyph's own box, so no button around it changes size: as wide as the ✕ advanced (.762 em) and as tall as its ink
  (.70 em), standing on the baseline as the glyph did. Two strokes in the text's colour, round at the ends, a tenth of an
  em thick as the glyph's were; heavier where the text around it is bold (`chrome.css`).
-->
<script lang="ts">
  // No props: the ✕ takes its size and colour from the text around it.
</script>

<svg class="tf-xicon" viewBox="0 0 76 70" aria-hidden="true" focusable="false"><path d="M8 5 68 65M68 5 8 65" /></svg>

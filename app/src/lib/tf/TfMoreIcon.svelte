<!--
  The ⋯ of a *More* menu, drawn (the ux30 Windows pass, 2026-11-23), as `TfXIcon` draws the ✕. Inter has no U+22EF,
  so the first ⋯ a page drew made Blink search the system's fonts for one: in WebView2 on Windows it went through
  Segoe UI and Times New Roman to Cambria Math, about 50 ms at CPU 4x in the first open of the list of every ticket,
  whose views' menus were the page's only ⋯. Drawn, it costs no search, and it is the same on the Mac and on Windows.

  Three dots on the text's middle, an em wide as the glyph was, in the text's colour.
-->
<script lang="ts">
  // No props: the ⋯ takes its size and colour from the text around it.
</script>

<svg class="tf-moreicon" viewBox="0 0 16 16" aria-hidden="true" focusable="false"><circle cx="3" cy="8" r="1.3" /><circle cx="8" cy="8" r="1.3" /><circle cx="13" cy="8" r="1.3" /></svg>

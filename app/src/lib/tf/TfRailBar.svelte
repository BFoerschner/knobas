<!--
  The one fold bar (design system, Layout): below 900 px a stacked side rail folds behind this bar, which names what is
  open. Tickets' views, Builds' pipelines, Settings' sections and Docs' tree all draw it. A link picked in the rail folds
  it again. Above 900 px the bar is not drawn (`.tf-railbar`, chrome.css).
-->
<script lang="ts">
  let {
    label,
    value,
    open,
    controls,
    onpress,
  }: {
    /** What the rail lists: *View*, *Pipeline*, *Section*, *Page*. */
    label: string;
    /** What is open. */
    value: string;
    open: boolean;
    /** The rail's id. */
    controls: string;
    onpress: (event: MouseEvent) => void;
  } = $props();
</script>

<div class="tf-railbar">
  <button type="button" class="tf-btn tf-sm" aria-expanded={open} aria-controls={controls} onclick={onpress}>
    {label}: <b>{value}</b><i class="tf-railmark" aria-hidden="true"></i>
  </button>
</div>

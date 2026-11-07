<!--
  **The inner scroller of a viewport-high page** (design system, Layout): Builds' right side, Docs' page, Tickets'
  board, Settings' section column, a ticket's reading column.

  No scroller reserves room for its bar. The host is a size container, the scroller fills it, and the one body is as
  wide as the host (`100cqw`) whether or not a bar is showing, so nothing inside moves when one appears. The bar takes
  the last 8 px of the page's right padding, and the clip around the body holds what passes under it, so that is no
  overflow to scroll to. A band ends at the bar only while there is something to scroll.

  `class` names the scroller, not the host, because `TfDiffView` and the tests look a scroller up by its name. A
  container name goes on the host from the stylesheet (`.tf-shost:has(> .tf-bmain)`). Attributes and attachments reach
  the scroller. `ref` is the scroller's element.

  It takes focus without being a stop on the way through the page, so a press in its text leaves PageDown, Space and the
  arrows here (*The box you pressed in holds the keys*, `focus-keep.ts`). It draws no ring (`chrome.css`).
-->
<script lang="ts">
  import type { Snippet } from "svelte";
  import type { HTMLAttributes } from "svelte/elements";

  let {
    class: scroller,
    ref = $bindable(null),
    children,
    ...rest
  }: Omit<HTMLAttributes<HTMLElement>, "class"> & {
    /** The scroller's own class: `tf-bmain`, `tf-rd-main`, `tf-board-wrap`, `tf-stg-scroll` or `tf-work`. */
    class: string;
    ref?: HTMLElement | null;
    children: Snippet;
  } = $props();
</script>

<div class="tf-shost">
  <section class="tf-scroller {scroller}" tabindex="-1" bind:this={ref} {...rest}>
    <div class="tf-sclip"><div class="tf-sbody">{@render children()}</div></div>
  </section>
</div>

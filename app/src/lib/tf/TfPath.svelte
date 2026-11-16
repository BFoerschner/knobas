<!--
  A filesystem path on one line that never runs past its box (the clone-path pass, 2026-11-16; design system, *Paths*):
  where it is too long, it gives up its middle and keeps its start and the folder or file at its end, which is the part
  that says which one it is (`C:\Users\someone\sou…\payout-service`). Its title is the whole path. The end itself is cut
  only when it alone is wider than the box.
-->
<script lang="ts">
  import { pathParts } from "./path";

  let { path, class: className = "" }: { path: string; class?: string } = $props();

  const parts = $derived(pathParts(path));
</script>

<span class="tf-path tf-mono {className}" title={path}
  >{#if parts.head !== ""}<span class="tf-path-head">{parts.head}</span>{/if}<span class="tf-path-tail">{parts.tail}</span></span
>

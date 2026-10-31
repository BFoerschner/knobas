<!--
  A view's header while it has no item drawn (#2286, #2287): the view's name
  and the role frame's *Sync now*, so the button is where every header puts it
  (Controls, *Header*). Builds draws it with no pipeline or run open, Docs with
  no page to head.
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  let { title, sync }: { title: string; sync?: Snippet | undefined } = $props();
</script>

<header class="tf-fhead">
  <div><h1>{title}</h1></div>
  {#if sync}<div class="tf-acts">{@render sync()}</div>{/if}
</header>

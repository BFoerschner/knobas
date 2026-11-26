<!-- Beside `TfSideEdge.test.svelte.ts`: a sidebar's grid as a page draws one, its edge first, the two columns after it. -->
<script lang="ts">
  import TfSideEdge from "./TfSideEdge.svelte";
  import { sideWidth, type SideId } from "./side-width.svelte";

  let { side, onfit }: { side: SideId; onfit?: ((grid: HTMLElement) => number | null) | undefined } = $props();
</script>

<div class="grid" {@attach sideWidth(side)}>
  <TfSideEdge {side} {onfit} />
  <nav class="rail">The rail</nav>
  <section class="main">The main column</section>
</div>

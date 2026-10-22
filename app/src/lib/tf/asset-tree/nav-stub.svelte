<script lang="ts" module>
  import type { AssetsNav } from "../../assets/tree";

  /** The nav the page last handed its view; read by `TfAssetTree.test.svelte.ts`. */
  export const seen: { nav: AssetsNav | null } = { nav: null };
</script>

<script lang="ts">
  let { nav }: { nav: AssetsNav } = $props();
  $effect.pre(() => {
    seen.nav = nav;
  });
</script>

<script lang="ts" module>
  import type { AssetRow } from "../../ipc/assets";
  import type { AssetsNav } from "../../assets/tree";

  /** The nav the page last handed its view, and the way the view says what the estate's top holds; read by `TfAssetTree.test.svelte.ts`. */
  export const seen: { nav: AssetsNav | null; onroots: ((roots: AssetRow[]) => void) | null } = { nav: null, onroots: null };
</script>

<script lang="ts">
  let { nav, onroots }: { nav: AssetsNav; onroots?: (roots: AssetRow[]) => void } = $props();
  $effect.pre(() => {
    seen.nav = nav;
    seen.onroots = onroots ?? null;
  });
</script>

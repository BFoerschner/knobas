<!-- The Files tab under a page that counts its refreshes (`refreshed.svelte.ts`), as `TfRepo` holds it, for a test. -->
<script lang="ts">
  import type { ComponentProps } from "svelte";
  import TfRepoFiles from "./TfRepoFiles.svelte";
  import { provideTabRereads } from "./refreshed.svelte";

  let { props }: { props: ComponentProps<typeof TfRepoFiles> } = $props();
  provideTabRereads();
</script>

<TfRepoFiles {...props} />

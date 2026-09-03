<!--
  Test fixture for `Shell.test.svelte.ts` — nothing else imports it, so it is
  tree-shaken out of the app bundle.

  It supplies the `main` snippet (which cannot be handed to `mount()` from a
  plain TypeScript test) and a stub lifecycle, so the test can drive the frame
  without a backend.
-->
<script lang="ts">
  import Shell from "./Shell.svelte";
  import type { Router } from "./router.svelte";
  import type { Lifecycle } from "./lifecycle.svelte";

  let { router, onsearch }: { router: Router; onsearch: () => void } = $props();

  const lifecycle: Lifecycle = {
    db: { state: "ready" },
    status: {
      db: { state: "ready" },
      first_run: false,
      demo: true,
      source_count: 1,
      app_version: "0.1.0",
    },
    ready: true,
    error: null,
    start: async () => {},
    stop: () => {},
    retry: async () => {},
  };
</script>

<Shell {router} {lifecycle} {onsearch} ontimer={undefined}>
  {#snippet main()}
    <div class="empty">room</div>
  {/snippet}
</Shell>

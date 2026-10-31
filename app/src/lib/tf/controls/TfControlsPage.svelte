<!--
  The door to the controls specimen (#2280). The specimen is loaded only in a
  dev build: `import.meta.env.DEV` folds to `false` in a release bundle, and
  Rollup drops the branch and `TfControls.svelte` with it.
-->
<script lang="ts">
  import type { Component } from "svelte";

  let Specimen = $state<Component | null>(null);

  if (import.meta.env.DEV) {
    void import("./TfControls.svelte").then((module) => (Specimen = module.default));
  }
</script>

{#if Specimen}<Specimen />{/if}

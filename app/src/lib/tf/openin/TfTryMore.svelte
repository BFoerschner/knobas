<!--
  The words beside *Try it on*'s repository picker, on the application page and
  a folder renderer's page: *Finding more…* while later batches of checkouts are
  read, and once every batch is in with no checkout, the way to one. Both are
  drawn from the first frame in one cell, each unseen while it doesn't apply, so
  the picker's row is as wide and as tall whichever shows (2026-11-17: on the
  saved scale database no repository is cloned, and the section gave way to one
  line once the checkouts answered).
-->
<script lang="ts">
  import type { TryRepos } from "./try-repos.svelte";

  let { tryOn }: { tryOn: TryRepos } = $props();
</script>

<span class="tf-stg-more">
  <small class="tf-dim" role="status" class:tf-stg-done={!tryOn.more}>Finding more…</small>
  <small class:tf-stg-done={!tryOn.none || tryOn.failed !== null}><a href="#/code">Clone one in Code</a></small>
</span>

<!--
  The shell's breadcrumb for a full view (R-1.6): the job you were last in on
  the card yellow, then the view's name, and *Back to <key>* beside `Esc` on
  the right; with no job, the view's own name and *Back*. `Esc` itself is the
  escape chain's (`escape.ts`); this only says so.

  `TfPage.svelte` draws it above a page that sets `crumbs` and above a full
  view not built yet; the item page (#1408) draws it itself, because its name
  is the item's kind, which only its own read knows.
-->
<script lang="ts">
  import { jobHash } from "./route";

  let {
    job,
    jobAt = null,
    name,
  }: {
    /** The job's key, which the chip names. */
    job: string | null;
    /** The address the job was opened at (#1525), which the chip links; its key when omitted. */
    jobAt?: string | null;
    name: string;
  } = $props();
</script>

<nav class="tf-crumbs" aria-label="Breadcrumb">
  {#if job !== null}
    <a class="tf-jobchip tf-card" href={jobHash(jobAt ?? job)}>{job}</a><span class="tf-sep">›</span>
  {/if}
  <strong>{name}</strong>
  <span class="tf-esc">{job !== null ? `Back to ${job}` : "Back"} <kbd>Esc</kbd></span>
</nav>

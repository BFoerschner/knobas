<!--
  *Keep for this field*: the chain kept as a named pipeline for a field in files of a pattern (ADR-0059, ruling 11).
  Its own component, as the pipeline's *Save as* is, so the missing line stands beside *Keep* (design system, the enabled
  rule) inside the value viewer's dialog, which has no footer for `TfDialog`'s `why`.
-->
<script lang="ts">
  let {
    keeping = $bindable(),
    saving,
    onkeep,
    oncancel,
  }: {
    keeping: { name: string; field: string; files: string };
    /** A keep in flight. */
    saving: boolean;
    onkeep: () => void;
    oncancel: () => void;
  } = $props();

  /** What *Keep* waits for, in words beside it. */
  const needs = $derived(
    keeping.name.trim() === ""
      ? "Name it first."
      : keeping.field.trim() === ""
        ? "Say which field it is for."
        : keeping.files.trim() === ""
          ? "Say which files it is for."
          : null,
  );
</script>

<form
  class="tf-psave tf-xv-keep"
  onsubmit={(event) => {
    event.preventDefault();
    if (needs === null) onkeep();
  }}
>
  <label>Name <input aria-label="Name" bind:value={keeping.name} spellcheck="false" /></label>
  <label>For the field <input class="tf-pmono" aria-label="For the field" bind:value={keeping.field} spellcheck="false" /></label>
  <label>in <input class="tf-pmono tf-xv-short" aria-label="In the files" bind:value={keeping.files} spellcheck="false" /></label>
  {#if needs !== null}<p class="tf-dwhy" role="status">{needs}</p>{/if}
  <button type="submit" class="tf-btn tf-sm tf-primary" disabled={saving || needs !== null}>Keep</button>
  <button type="button" class="tf-btn tf-sm tf-ghost" onclick={oncancel}>Cancel</button>
</form>

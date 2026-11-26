<!--
  The ticket's summary in the reading column, and its edit (ADR-0054, section 25). The title is text a person selects
  and copies, not a button; a pencil after it (*Edit the summary (E)*) turns it into a one-line field with *Save* and
  *Cancel*: `⌘Enter` or `Enter` saves, `Esc` cancels (`inlineEditor`). The pencil's room is kept where the summary
  does not open, unseen, so the heading does not rewrap when the source's answer lands.
-->
<script lang="ts">
  import { inlineEditor } from "../inline-editor";
  import type { Layers } from "../layers.svelte";
  import type { FieldEdits } from "./field-edits.svelte";

  let {
    title,
    edits,
    editing = $bindable(false),
    layers,
  }: {
    /** The summary as the read holds it. */
    title: string;
    edits: FieldEdits | null;
    editing?: boolean;
    layers?: Layers | undefined;
  } = $props();

  const control = $derived(edits?.control("title") ?? null);
  const shown = $derived(control?.echo?.text ?? title);
  let draft = $state("");

  $effect.pre(() => {
    if (editing) draft = shown;
  });

  function save() {
    const next = draft.trim();
    editing = false;
    if (next === "" || next === shown) return;
    edits?.edit("title", { title: next }, next, "Retitle {key}");
  }
</script>

{#if editing && control?.can}
  <form
    class="tf-tktitleedit"
    onsubmit={(event) => {
      event.preventDefault();
      save();
    }}
    {@attach inlineEditor(() => (editing = false), { send: save, back: () => document.querySelector<HTMLElement>(".tf-tktitlepen"), ...(layers ? { layers } : {}) })}
  >
    <input type="text" aria-label="Summary" bind:value={draft} spellcheck="true" />
    <button type="submit" class="tf-btn tf-sm tf-primary">Save</button>
    <button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => (editing = false)}>Cancel</button>
  </form>
{:else}
  <h1>{shown}{#if control?.chip}{" "}<span class="tf-chip tf-tktitlechip" class:tf-info={control.echo?.state === "queued"}>{control.chip}</span>{/if}</h1>
  <button
    type="button"
    class="tf-iconbtn tf-sm tf-tktitlepen"
    class:tf-unseen={!control?.can}
    title={control?.can ? "Edit the summary (E)" : (control?.why ?? undefined)}
    aria-label="Edit the summary"
    aria-hidden={!control?.can}
    tabindex={control?.can ? 0 : -1}
    disabled={!control?.can}
    onclick={() => (editing = true)}
    ><svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true"><path d="M11 2.5l2.5 2.5L6 12.5H3.5V10z" /><path d="M9.5 4l2.5 2.5" /></svg></button
  >
{/if}

<!--
  A page body by section (#859, #862; R-10.11, R-10.13). Section 0 is what
  comes before the first heading; each heading opens the next, numbered as
  prototype-6 numbers them, with its anchor (`#/page/<id>/<n>`) and, on hover,
  *Edit section*.

  *Edit section* turns a text-only section into a text box with *Save
  section* and *Cancel*; the backend says which sections are text-only
  (`DocSection.refusal`), and a refused one draws the button disabled with the
  reason. Saving goes through the write queue (`onsave`). A page its
  source's credential may not edit (`refusal`, #2053) draws every *Edit
  section*, and the *Save section* of an editor already open, disabled with
  that reason.

  A saved section draws its queued text in place of the old, muted with its echo,
  *queued*, then *sent* (#1424, #2102), until a read of the page has a newer version; *Edit
  section* then starts from the queued text, and *Edit and resend* on a held
  or refused save opens the editor with it.
-->
<script lang="ts">
  import type { DocBlock, DocSection } from "../../ipc/entity";
  import TfDocBlocks from "./TfDocBlocks.svelte";
  import TfDocInlines from "./TfDocInlines.svelte";
  import { refusedAttrs } from "../../shell/refused-writes";
  import { echoOn, takeAgain } from "../queued.svelte";
  import { pageHash, refusalReason, sectionKind, sectionsOf } from "./docs";

  let {
    blocks,
    entityId,
    sourceName,
    webUrl,
    sections,
    editable,
    refusal = null,
    stems = [],
    onopen,
    oncopy,
    onsave,
  }: {
    blocks: DocBlock[];
    entityId: string;
    sourceName: string;
    webUrl: string | null;
    /** What the backend says of each numbered section. */
    sections: DocSection[];
    /** Whether the page's source takes a section edit at all. */
    editable: boolean;
    /** Why the source's credential may not edit this page (#2053), or `null`. */
    refusal?: string | null;
    stems?: string[];
    onopen: (url: string) => void;
    oncopy: (code: string) => void;
    /** Queue the edit; `true` once it is queued. */
    onsave: (section: number, text: string) => Promise<boolean>;
  } = $props();

  let editing = $state<number | null>(null);
  let draft = $state("");
  let saving = $state(false);

  // Another page closes an open editor; the same page read again keeps it
  // (#2072). The `$derived` is what keeps it: the effect hears the id only when
  // it changes, not every new page object the prop is read through.
  const pageId = $derived(entityId);
  $effect(() => {
    void pageId;
    editing = null;
  });

  // *Edit and resend* on a held or refused save: the editor, with what was typed.
  $effect(() => {
    for (const section of sections) {
      const again = takeAgain(entityId, sectionKind(section.number));
      if (again === null) continue;
      editing = section.number;
      draft = again;
    }
  });

  /** The text a save of `number` queued, drawn until the page's version moves. */
  const queuedText = (number: number) => echoOn(entityId, sectionKind(number))?.text ?? null;
  /** Where that save's write is: `queued` or `sent` (#2102). */
  const queuedState = (number: number) => echoOn(entityId, sectionKind(number))?.state ?? null;

  function open(number: number) {
    const section = sections.find((s) => s.number === number);
    if (!section || section.text === null || refusal !== null) return;
    editing = number;
    draft = queuedText(number) ?? section.text;
  }

  async function save(number: number) {
    // An editor can be open when the refusal lands: opened before the answer,
    // or reopened by *Edit and resend*.
    if (saving || refusal !== null) return;
    saving = true;
    try {
      if (await onsave(number, draft)) editing = null;
    } finally {
      saving = false;
    }
  }

  function focus(node: HTMLTextAreaElement) {
    node.focus();
  }
</script>

{#each sectionsOf(blocks) as section (section.number)}
  {#if section.heading === null}
    <TfDocBlocks blocks={section.blocks} {entityId} {sourceName} {webUrl} {onopen} {oncopy} {stems} />
  {:else}
    {@const said = sections.find((s) => s.number === section.number)}
    {@const queued = queuedText(section.number)}
    <section data-sec={section.number}>
      <h2 id={`s-${section.number}`}>
        <TfDocInlines text={section.heading.text} {onopen} {stems} />
        {#if queued !== null}<span class="tf-chip tf-info tf-queued-mark">{queuedState(section.number)}</span>{/if}
        <span class="tf-tools">
          <a href={pageHash(entityId, section.number)} title="Link to this section">#</a>
          {#if editable && said}
            {#if said.refusal === null}
              <button type="button" {...refusedAttrs(refusal)} onclick={() => open(section.number)}>Edit section</button>
            {:else}
              <button type="button" disabled title={refusalReason(said.refusal, sourceName)}>Edit section</button>
            {/if}
          {/if}
        </span>
      </h2>
      {#if editing === section.number}
        <div class="tf-sec-edit">
          <textarea bind:value={draft} aria-label="Section text" use:focus></textarea>
          <div class="tf-acts">
            <span class="tf-dim">Text, lists and panels. Saved to {sourceName} through the write queue.</span>
            <button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => (editing = null)}>Cancel</button>
            <button
              type="button"
              class="tf-btn tf-sm tf-primary"
              disabled={saving}
              {...refusedAttrs(refusal)}
              onclick={() => save(section.number)}>Save section</button>
          </div>
        </div>
      {:else if queued !== null}
        <p class="tf-sec-queued">{queued}</p>
      {:else}
        <TfDocBlocks blocks={section.blocks} {entityId} {sourceName} {webUrl} {onopen} {oncopy} {stems} />
      {/if}
    </section>
  {/if}
{/each}

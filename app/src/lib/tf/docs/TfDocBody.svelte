<!--
  A page body by section (#859, #862; R-10.11, R-10.13). Section 0 is what
  comes before the first heading; each heading opens the next, numbered as
  prototype-6 numbers them, with its anchor (`#/page/<id>/<n>`) and, on hover,
  *Edit section*.

  *Edit section* turns a text-only section into a text box with *Save
  section* and *Cancel*; the backend says which sections are text-only
  (`DocSection.refusal`), and a refused one draws the button disabled with the
  reason. Saving goes through the write queue (`onsave`).
-->
<script lang="ts">
  import type { DocBlock, DocSection } from "../../ipc/entity";
  import TfDocBlocks from "./TfDocBlocks.svelte";
  import TfDocInlines from "./TfDocInlines.svelte";
  import { pageHash, refusalReason, sectionsOf } from "./docs";

  let {
    blocks,
    entityId,
    sourceName,
    webUrl,
    sections,
    editable,
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
    stems?: string[];
    onopen: (url: string) => void;
    oncopy: (code: string) => void;
    /** Queue the edit; `true` once it is queued. */
    onsave: (section: number, text: string) => Promise<boolean>;
  } = $props();

  let editing = $state<number | null>(null);
  let draft = $state("");
  let saving = $state(false);

  // Another page closes an open editor.
  $effect(() => {
    void entityId;
    editing = null;
  });

  function open(number: number) {
    const section = sections.find((s) => s.number === number);
    if (!section || section.text === null) return;
    editing = number;
    draft = section.text;
  }

  async function save(number: number) {
    if (saving) return;
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
    <section data-sec={section.number}>
      <h2 id={`s-${section.number}`}>
        <TfDocInlines text={section.heading.text} {onopen} {stems} />
        <span class="tf-tools">
          <a href={pageHash(entityId, section.number)} title="Link to this section">#</a>
          {#if editable && said}
            {#if said.refusal === null}
              <button type="button" onclick={() => open(section.number)}>Edit section</button>
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
            <button type="button" class="tf-btn tf-sm tf-primary" disabled={saving} onclick={() => save(section.number)}
              >Save section</button>
          </div>
        </div>
      {:else}
        <TfDocBlocks blocks={section.blocks} {entityId} {sourceName} {webUrl} {onopen} {oncopy} {stems} />
      {/if}
    </section>
  {/if}
{/each}

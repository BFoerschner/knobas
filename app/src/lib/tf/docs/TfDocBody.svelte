<!--
  A page body by section (#859, #862; R-10.11, R-10.13). Section 0 is what
  comes before the first heading; each heading opens the next, numbered as
  prototype-6 numbers them, with its anchor (`#/page/<id>/<n>`) and, on hover,
  `#`, which copies the section's link (F-450, because the app shows no address), and *Edit section*.

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

  An open editor's words are a draft (`drafts`, `section:<page>:<section>`, F-430): `Esc` or
  leaving the page closes the editor and keeps them, the section's heading says *Draft*, and
  *Edit section…* opens on them. *Cancel* throws them away, and a save ends them.
-->
<script lang="ts">
  import TfStateChip from "../TfStateChip.svelte";
  import { drafts } from "../drafts.svelte";
  import { inlineEditor } from "../inline-editor";
  import type { DocBlock, DocSection } from "../../ipc/entity";
  import TfDocBlocks from "./TfDocBlocks.svelte";
  import TfDocInlines from "./TfDocInlines.svelte";
  import { refusedAttrs } from "../../shell/refused-writes";
  import { echoOn, takeAgain } from "../queued.svelte";
  import { inlineWords, refusalReason, sectionKind, sectionsOf } from "./docs";
  import { flushSync } from "svelte";

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
    oncopylink,
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
    /** The heading's `#`: copy section `number`'s link, and say which section it was. */
    oncopylink?: ((number: number, heading: string) => void) | undefined;
    /** Queue the edit; `true` once it is queued. */
    onsave: (section: number, text: string) => Promise<boolean>;
  } = $props();

  // A long page draws in pieces (PR #2479: the largest Confluence page's 216 KB body was one 94 ms task, 264 ms on a
  // main thread a quarter as fast): the sections that fit `FIRST` characters of body, then `MORE` each task after,
  // each its own task, so the page paints and answers between them. `drawAll` draws the rest at once, for whatever
  // needs the whole body on the page: a section's address, a find, the place a reader left.
  const FIRST = 40_000;
  const MORE = 24_000;
  const all = $derived(sectionsOf(blocks));
  const weights = $derived(all.map((section) => JSON.stringify(section.blocks).length + 64));
  /** How much of this page's body to draw now; a page drawn whole stays whole when it is read again. */
  let grown = $state<{ id: string; budget: number }>({ id: "", budget: FIRST });
  const budget = $derived(grown.id === entityId ? grown.budget : FIRST);
  const shown = $derived.by(() => {
    let used = 0;
    let n = 0;
    while (n < all.length && (n === 0 || used + weights[n]! <= budget)) used += weights[n++]!;
    return n === all.length ? all : all.slice(0, n);
  });
  $effect(() => {
    const id = entityId;
    const more = budget + MORE;
    if (shown.length >= all.length) return;
    const later = setTimeout(() => (grown = { id, budget: more }), 0);
    return () => clearTimeout(later);
  });

  /** Draws the rest of the page's body now, before what needs all of it on the page. */
  export function drawAll(): void {
    if (shown.length >= all.length) return;
    grown = { id: entityId, budget: Infinity };
    flushSync();
  }

  let editing = $state<number | null>(null);
  let draft = $state("");
  let saving = $state(false);
  /** The section the editor opened on, which focus goes back to once it has closed. */
  let openedOn: number | null = null;
  const draftKey = (number: number) => `section:${entityId}:${number}`;

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
      openedOn = section.number;
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
    openedOn = number;
    draft = drafts.has(draftKey(number)) ? drafts.get(draftKey(number)) : (queuedText(number) ?? section.text);
  }

  /** What was typed is the draft; words back at the section's own text are none. */
  function typed(number: number, words: string) {
    draft = words;
    const start = queuedText(number) ?? sections.find((s) => s.number === number)?.text ?? null;
    if (words === start) drafts.forget(draftKey(number));
    else drafts.put(draftKey(number), words);
  }

  /** `Esc`, and anything else that only closes: the draft stays for *Edit section…*. */
  function close() {
    editing = null;
  }

  /** *Cancel*: the explicit discard. */
  function cancel(number: number) {
    drafts.forget(draftKey(number));
    editing = null;
  }

  /** ⌘Enter: read when pressed, because the attachment outlives one read of the page. */
  function saveOpen() {
    if (editing !== null) void save(editing);
  }

  /** The heading's *Edit section…*, drawn again once the editor has closed. */
  const backToEdit = () => (openedOn === null ? null : (document.getElementById(`s-${openedOn}`)?.querySelector<HTMLElement>(".tf-tools .tf-sec-open") ?? null));

  async function save(number: number) {
    // An editor can be open when the refusal lands: opened before the answer,
    // or reopened by *Edit and resend*.
    if (saving || refusal !== null) return;
    saving = true;
    try {
      if (await onsave(number, draft)) {
        drafts.forget(draftKey(number));
        editing = null;
      }
    } finally {
      saving = false;
    }
  }
</script>

{#each shown as section (section.number)}
  {#if section.heading === null}
    <TfDocBlocks blocks={section.blocks} {entityId} {sourceName} {webUrl} {onopen} {oncopy} {stems} />
  {:else}
    {@const said = sections.find((s) => s.number === section.number)}
    {@const queued = queuedText(section.number)}
    <section data-sec={section.number}>
      <h2 id={`s-${section.number}`}>
        <TfDocInlines text={section.heading.text} {onopen} {stems} />
        {#if queued !== null}<TfStateChip text={queuedState(section.number)} tone="info" class="tf-queued-mark" />{/if}
        {#if editing !== section.number && drafts.has(draftKey(section.number))}<TfStateChip text="draft" tone="info" class="tf-queued-mark" />{/if}
        <span class="tf-tools">
          {#if oncopylink}
            <button type="button" title="Copy link to this section" aria-label="Copy link to this section" onclick={() => oncopylink(section.number, inlineWords(section.heading?.text ?? []))}>#</button>
          {/if}
          {#if editable && said}
            {#if said.refusal === null}
              <button type="button" class="tf-sec-open" {...refusedAttrs(refusal)} onclick={() => open(section.number)}>Edit section…</button>
            {:else}
              <button type="button" class="tf-sec-open" disabled title={refusalReason(said.refusal, sourceName)}>Edit section…</button>
            {/if}
          {/if}
        </span>
      </h2>
      {#if editing === section.number}
        <div class="tf-sec-edit" {@attach inlineEditor(close, { back: backToEdit, send: saveOpen, escCloses: true })}>
          <textarea value={draft} aria-label="Section text" oninput={(event) => typed(section.number, event.currentTarget.value)}></textarea>
          <div class="tf-acts">
            <span class="tf-dim">Text, lists and panels. Saved to {sourceName} through the write queue.</span>
            <button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => cancel(section.number)}>Cancel</button>
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

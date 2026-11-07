<!--
  *New page* (#862, R-10.16), prototype-6's dialog: the space, *Under* (every
  page of the space, with its path), the title, *Link it to <job>* when a job
  is open, and a plain-text body where `##` starts a section and `-` a list.
  It goes through the write queue (`docs_create_page`).

  *Under* has no *Top of the space*: `CreatePage` writes under a mirrored
  page, so the space's first page is the default when no page is given. Its
  options are in order of their path (F-452).

  *Under* is asked `refused_writes` as it changes (#2053): a parent whose
  space the source's credential may not add pages to draws *Create page*
  disabled with the reason. With no title *Create page* is disabled, and the line beside it says so (design system,
  the enabled rule).

  **The words outlive the dialog** (the key table, F-442): `Esc` and × close it and keep what was typed, under the
  space and page it was opened on, with the *Space* and *Under* chosen since, so the words never come back under another
  parent than they were written for. *Cancel* and a queued page are what end a draft.
-->
<script lang="ts">
  import type { DocsTree, NewPage } from "../../ipc/entity";
  import { askInto, refusalOf, refusedAttrs, type RefusedWrite, type RefusedWritesPort } from "../../shell/refused-writes";
  import type { Layers } from "../layers.svelte";
  import TfKey from "../TfKey.svelte";
  import TfDialog from "../settings/TfDialog.svelte";
  import { drafts } from "../drafts.svelte";
  import { firstPage, pagesByPath, spaceId } from "./docs";
  import { untrack } from "svelte";

  let {
    tree,
    space: initialSpace,
    parent: initialParent,
    title: initialTitle = "",
    text: initialText = "",
    job,
    sourceName,
    layers,
    onclose,
    oncreate,
    refusedWrites,
  }: {
    tree: DocsTree;
    /** The space it opens on (`spaceId`). */
    space: string | null;
    /** The page it opens under. */
    parent: string | null;
    /** What *Edit and resend* opens it with (#1424): the title and text a refused create carried. */
    title?: string;
    text?: string;
    job: string | null;
    sourceName: string;
    layers: Layers;
    onclose: () => void;
    /** Queue it; `true` once queued, which closes the dialog. */
    oncreate: (page: NewPage, linkToJob: boolean) => Promise<boolean>;
    /** What the source refuses its credential; the IPC read when omitted. */
    refusedWrites?: RefusedWritesPort | undefined;
  } = $props();

  interface Held {
    space: string;
    under: string;
    title: string;
    text: string;
  }

  // svelte-ignore state_referenced_locally
  const draftKey = `new-page:${initialSpace ?? ""}:${initialParent ?? ""}`;
  /** What `Esc` or × left here. *Edit and resend* brings its own words, which win. A space that has gone since gives its place back. */
  function heldDraft(): Held | null {
    if (initialTitle !== "" || initialText !== "" || !drafts.has(draftKey)) return null;
    const held = JSON.parse(drafts.get(draftKey)) as Held;
    return tree.spaces.some((s) => spaceId(s) === held.space) ? held : { ...held, space: initialSpace ?? "", under: initialParent ?? "" };
  }
  const held = untrack(heldDraft);

  // svelte-ignore state_referenced_locally
  let picked = $state(held?.space || initialSpace || (tree.spaces[0] ? spaceId(tree.spaces[0]) : ""));
  const space = $derived(tree.spaces.find((s) => spaceId(s) === picked));
  // svelte-ignore state_referenced_locally
  let under = $state(held?.under ?? initialParent ?? "");
  // svelte-ignore state_referenced_locally
  let title = $state(held?.title ?? initialTitle);
  // svelte-ignore state_referenced_locally
  let text = $state(held?.text ?? initialText);
  let link = $state(true);
  let sending = $state(false);

  // A space whose pages do not hold the chosen parent starts at its first page.
  $effect(() => {
    if (space && !space.pages.some((p) => p.entity_id === under)) under = firstPage(space)?.entity_id ?? "";
  });

  // Blank words are no draft, so a dialog opened and left holds nothing back.
  $effect(() => {
    const words = title.trim() === "" && text.trim() === "" ? "" : JSON.stringify({ space: picked, under, title, text } satisfies Held);
    untrack(() => drafts.put(draftKey, words));
  });

  /** *Cancel*: the explicit discard. */
  function discard(): void {
    drafts.forget(draftKey);
    onclose();
  }

  /** The space's pages by their place in it, prototype-6's `path › title`, so a folder's pages stand together. */
  const places = $derived(space === undefined ? [] : pagesByPath(space));

  let refused = $state<RefusedWrite[]>([]);
  $effect(() => askInto(under === "" ? [] : [{ entity: under, op: "create_page" }], refusedWrites, (answer) => (refused = answer)));
  const refusal = $derived(refusalOf(refused, under === "" ? null : under, "create_page"));

  async function create() {
    if (sending || under === "" || refusal !== null) return;
    sending = true;
    try {
      if (await oncreate({ parent: under, title, text }, job !== null && link)) discard();
    } finally {
      sending = false;
    }
  }
</script>

<TfDialog why={title.trim() === "" ? "Give the page a title." : under === "" ? "Choose where it goes." : null} title="New page" {onclose} {layers} className="tf-np-dialog">
  {#snippet body()}
    <div class="tf-formgrid">
      <span>Space</span>
      <select aria-label="Space" bind:value={picked}>
        {#each tree.spaces as s (spaceId(s))}
          <option value={spaceId(s)}>{s.name ?? s.key}</option>
        {/each}
      </select>
      <span>Under</span>
      <select aria-label="Under" bind:value={under}>
        {#each places as p (p.entity_id)}
          <option value={p.entity_id}>{p.label}</option>
        {/each}
      </select>
      <span>Title <small class="tf-req">required</small></span>
      <input type="text" aria-label="Title" data-arrive bind:value={title} />
      {#if job !== null}
        <label class="tf-ck"><input type="checkbox" bind:checked={link} /> <span>Link it to <TfKey value={job} /></span></label>
      {/if}
      <span>Text</span>
      <textarea
        class="tf-np-text"
        rows="8"
        aria-label="Page text"
        placeholder="Write in plain text. ## starts a section, - starts a list."
        bind:value={text}
      ></textarea>
    </div>
    <p class="tf-dlg-note">Created in {sourceName} through the write queue. Tables and macros need {sourceName} for now.</p>
  {/snippet}
  {#snippet footer()}
    <button type="button" class="tf-btn tf-ghost" onclick={discard}>Cancel</button>
    <button
      type="button"
      class="tf-btn tf-primary"
      disabled={sending || title.trim() === "" || under === ""}
      {...refusedAttrs(refusal)}
      onclick={create}>Create page</button>
  {/snippet}
</TfDialog>

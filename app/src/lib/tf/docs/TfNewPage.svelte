<!--
  *New page* (#862, R-10.16), prototype-6's dialog: the space, *Under* (every
  page of the space, with its path), the title, *Link it to <job>* when a job
  is open, and a plain-text body where `##` starts a section and `-` a list.
  It goes through the write queue (`docs_create_page`).

  *Under* has no *Top of the space*: `CreatePage` writes under a mirrored
  page, so the space's first page is the default when no page is given.

  *Under* is asked `refused_writes` as it changes (#2053): a parent whose
  space the source's credential may not add pages to draws *Create page*
  disabled with the reason.
-->
<script lang="ts">
  import type { DocsTree, NewPage } from "../../ipc/entity";
  import { askInto, refusalOf, refusedAttrs, type RefusedWrite, type RefusedWritesPort } from "../../shell/refused-writes";
  import type { Layers } from "../layers.svelte";
  import TfDialog from "../settings/TfDialog.svelte";
  import { firstPage, spaceId } from "./docs";

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
    refusedWrites?: RefusedWritesPort;
  } = $props();

  // svelte-ignore state_referenced_locally
  let picked = $state(initialSpace ?? (tree.spaces[0] ? spaceId(tree.spaces[0]) : ""));
  const space = $derived(tree.spaces.find((s) => spaceId(s) === picked));
  // svelte-ignore state_referenced_locally
  let under = $state(initialParent ?? "");
  // svelte-ignore state_referenced_locally
  let title = $state(initialTitle);
  // svelte-ignore state_referenced_locally
  let text = $state(initialText);
  let link = $state(true);
  let sending = $state(false);

  // A space whose pages do not hold the chosen parent starts at its first page.
  $effect(() => {
    if (space && !space.pages.some((p) => p.entity_id === under)) under = firstPage(space)?.entity_id ?? "";
  });

  /** A page's place in its space, prototype-6's `path › title`. */
  function label(id: string): string {
    const pages = space?.pages ?? [];
    const names: string[] = [];
    const seen = new Set<string>();
    let at = pages.find((p) => p.entity_id === id);
    while (at && !seen.has(at.entity_id)) {
      seen.add(at.entity_id);
      names.unshift(at.title);
      const up = at.parent;
      at = up === null ? undefined : pages.find((p) => p.entity_id === up);
    }
    return names.join(" › ");
  }

  let refused = $state<RefusedWrite[]>([]);
  $effect(() => askInto(under === "" ? [] : [{ entity: under, op: "create_page" }], refusedWrites, (answer) => (refused = answer)));
  const refusal = $derived(refusalOf(refused, under === "" ? null : under, "create_page"));

  async function create() {
    if (sending || under === "" || refusal !== null) return;
    sending = true;
    try {
      if (await oncreate({ parent: under, title, text }, job !== null && link)) onclose();
    } finally {
      sending = false;
    }
  }
</script>

<TfDialog title="New page" {onclose} {layers} className="tf-np-dialog">
  {#snippet body()}
    <div class="tf-formgrid">
      <span>Space</span>
      <select aria-label="Space" bind:value={picked}>
        {#each tree.spaces as s (spaceId(s))}
          <option value={spaceId(s)}>{s.key} {s.name ?? ""}</option>
        {/each}
      </select>
      <span>Under</span>
      <select aria-label="Under" bind:value={under}>
        {#each space?.pages ?? [] as p (p.entity_id)}
          <option value={p.entity_id}>{label(p.entity_id)}</option>
        {/each}
      </select>
      <span>Title</span>
      <input type="text" aria-label="Title" placeholder="Title" bind:value={title} />
      {#if job !== null}
        <span></span>
        <label class="tf-ck"><input type="checkbox" bind:checked={link} /> Link it to {job}</label>
      {/if}
    </div>
    <textarea
      class="tf-np-text"
      rows="8"
      aria-label="Page text"
      placeholder="Write in plain text. ## starts a section, - starts a list."
      bind:value={text}
    ></textarea>
    <p class="tf-dlg-note">Created in {sourceName} through the write queue. Tables and macros need {sourceName} for now.</p>
  {/snippet}
  {#snippet footer()}
    <button type="button" class="tf-btn tf-ghost" onclick={onclose}>Cancel</button>
    <button
      type="button"
      class="tf-btn tf-primary"
      disabled={sending || title.trim() === "" || under === ""}
      {...refusedAttrs(refusal)}
      onclick={create}>Create page</button>
  {/snippet}
</TfDialog>

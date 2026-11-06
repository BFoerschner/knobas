<!--
  The new shell's one create-ticket dialog (#1451): the project the ticket
  goes in, its type and, where the surface lets the reader word it
  (`words`), its title and body. Standup files an action item's own words
  (the words drawn as a note); a failed run's problem block files a title
  and body prefilled from the failing test, which the reader may change.
  The projects are `list_projects`', read by the surface.

  A project the source's credential cannot file in (`refused_writes`, #2052,
  asked about every project drawn) stays pickable, and picked draws *Create
  ticket* disabled with the reason above it: another project may take it.

  It arrives on *Title* (F-453), so typing doesn't move the project, and shift-`Tab` reaches *Type* and *Project*.

  A surface that names a `draftKey` keeps what was changed through `Esc` and ×, as *New page* does (F-647). *Cancel* is the
  explicit discard, and filing ends the draft too.
-->
<script lang="ts">
  import { untrack } from "svelte";

  import type { Project } from "../ipc/entity";
  import { askInto, refusalOf, refusedAttrs, type RefusedWrite, type RefusedWritesPort, type WriteAsk } from "../shell/refused-writes";
  import { canFile, projectEntity, type TicketDraft } from "./create-ticket";
  import { drafts } from "./drafts.svelte";
  import { layers as sharedLayers, type Layers } from "./layers.svelte";
  import TfDialog from "./settings/TfDialog.svelte";

  let {
    draft,
    projects,
    words = false,
    layers = sharedLayers,
    refusedWrites,
    draftKey,
    onclose,
    onfile,
  }: {
    /** Where what was typed outlives the dialog; with none, it goes with the dialog. */
    draftKey?: string | undefined;
    /** What the dialog opens with. */
    draft: TicketDraft;
    projects: readonly Project[];
    /** Draw the title and body as fields; otherwise the title is a note and the body is the surface's. */
    words?: boolean;
    layers?: Layers;
    /** What the projects' sources' credentials cannot write (#2052); the bridge's when left out. */
    refusedWrites?: RefusedWritesPort | undefined;
    onclose: () => void;
    onfile: (draft: TicketDraft & { project: string }) => void;
  } = $props();

  const uid = $props.id();

  let editing = $state<TicketDraft>(
    untrack(() => ({ ...draft, ...(draftKey !== undefined && drafts.has(draftKey) ? (JSON.parse(drafts.get(draftKey)) as TicketDraft) : {}) })),
  );

  // Only what differs from what the dialog opened with is a draft, so one opened and left holds nothing back.
  $effect(() => {
    if (draftKey === undefined) return;
    const same = editing.project === draft.project && editing.type === draft.type && editing.title === draft.title && editing.body === draft.body;
    const words = same ? "" : JSON.stringify(editing);
    untrack(() => drafts.put(draftKey, words));
  });

  /** *Cancel*: the explicit discard. */
  function discard(): void {
    if (draftKey !== undefined) drafts.forget(draftKey);
    onclose();
  }

  let refused = $state<RefusedWrite[]>([]);
  /** As a string, so the same projects drawn again ask nothing again. */
  const asks = $derived(JSON.stringify(projects.map((project) => ({ entity: projectEntity(project), op: "create_ticket" }))));
  $effect(() => askInto(JSON.parse(asks) as WriteAsk[], refusedWrites, (answer) => (refused = answer)));
  const cannot = $derived(refusalOf(refused, editing.project, "create_ticket"));
</script>

<TfDialog why={editing.project === null ? "Choose a project." : editing.type.trim() === "" ? "Say what type it is." : editing.title.trim() === "" ? "Give it a title." : null} title="Create a ticket" {layers} {onclose}>
  {#snippet body()}
    {#if !words}<p class="tf-dlg-note">{editing.title}</p>{/if}
    <div class="tf-formgrid">
      <label for="tf-ct-project-{uid}">Project <small class="tf-req">required</small></label>
      <select
        id="tf-ct-project-{uid}"
        aria-label="Project the ticket goes in"
        value={editing.project ?? ""}
        onchange={(event) => (editing.project = event.currentTarget.value || null)}
      >
        <option value="">Choose…</option>
        {#each projects as project (projectEntity(project))}
          <option value={projectEntity(project)}>{project.name ?? project.key}</option>
        {/each}
      </select>
      <label for="tf-ct-type-{uid}">Type <small class="tf-req">required</small></label>
      <input
        id="tf-ct-type-{uid}"
        type="text"
        aria-label="Ticket type"
        value={editing.type}
        oninput={(event) => (editing.type = event.currentTarget.value)}
      />
      {#if words}
        <label for="tf-ct-title-{uid}">Title <small class="tf-req">required</small></label>
        <input
          id="tf-ct-title-{uid}"
          type="text"
          aria-label="Ticket title"
          data-arrive
          value={editing.title}
          oninput={(event) => (editing.title = event.currentTarget.value)}
        />
      {/if}
      {#if words}
        <label for="tf-ct-body-{uid}">Description</label>
        <textarea
          id="tf-ct-body-{uid}"
          aria-label="Ticket description"
          rows="8"
          value={editing.body}
          oninput={(event) => (editing.body = event.currentTarget.value)}
        ></textarea>
      {/if}
    </div>
    {#if cannot !== null}<p class="tf-dlg-note tf-fail">{cannot}</p>{/if}
  {/snippet}
  {#snippet footer()}
    <button class="tf-btn tf-ghost" onclick={discard}>Cancel</button>
    <button
      class="tf-btn tf-primary"
      {...refusedAttrs(cannot)}
      disabled={!canFile(editing)}
      onclick={() => {
        const filed = { ...editing };
        if (canFile(filed) && cannot === null) {
          if (draftKey !== undefined) drafts.forget(draftKey);
          onfile(filed);
        }
      }}
    >
      Create ticket
    </button>
  {/snippet}
</TfDialog>

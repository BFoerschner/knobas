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
-->
<script lang="ts">
  import { untrack } from "svelte";

  import type { Project } from "../ipc/entity";
  import { askInto, refusalOf, refusedAttrs, type RefusedWrite, type RefusedWritesPort, type WriteAsk } from "../shell/refused-writes";
  import { canFile, projectEntity, type TicketDraft } from "./create-ticket";
  import { layers as sharedLayers, type Layers } from "./layers.svelte";
  import TfDialog from "./settings/TfDialog.svelte";

  let {
    draft,
    projects,
    words = false,
    layers = sharedLayers,
    refusedWrites,
    onclose,
    onfile,
  }: {
    /** What the dialog opens with. */
    draft: TicketDraft;
    projects: readonly Project[];
    /** Draw the title and body as fields; otherwise the title is a note and the body is the surface's. */
    words?: boolean;
    layers?: Layers;
    /** What the projects' sources' credentials cannot write (#2052); the bridge's when left out. */
    refusedWrites?: RefusedWritesPort;
    onclose: () => void;
    onfile: (draft: TicketDraft & { project: string }) => void;
  } = $props();

  let editing = $state<TicketDraft>(untrack(() => ({ ...draft })));

  let refused = $state<RefusedWrite[]>([]);
  /** As a string, so the same projects drawn again ask nothing again. */
  const asks = $derived(JSON.stringify(projects.map((project) => ({ entity: projectEntity(project), op: "create_ticket" }))));
  $effect(() => askInto(JSON.parse(asks) as WriteAsk[], refusedWrites, (answer) => (refused = answer)));
  const cannot = $derived(refusalOf(refused, editing.project, "create_ticket"));
</script>

<TfDialog title="Create a ticket" {layers} {onclose}>
  {#snippet body()}
    {#if !words}<p class="tf-dlg-note">{editing.title}</p>{/if}
    <div class="tf-formgrid">
      <span>Project</span>
      <select
        aria-label="Project the ticket goes in"
        value={editing.project ?? ""}
        onchange={(event) => (editing.project = event.currentTarget.value || null)}
      >
        <option value="">Choose…</option>
        {#each projects as project (projectEntity(project))}
          <option value={projectEntity(project)}>{project.name ?? project.key}</option>
        {/each}
      </select>
      <span>Type</span>
      <input
        type="text"
        aria-label="Ticket type"
        value={editing.type}
        oninput={(event) => (editing.type = event.currentTarget.value)}
      />
      {#if words}
        <span>Title</span>
        <input
          type="text"
          aria-label="Ticket title"
          value={editing.title}
          oninput={(event) => (editing.title = event.currentTarget.value)}
        />
      {/if}
    </div>
    {#if cannot !== null}<p class="tf-dlg-note tf-fail">{cannot}</p>{/if}
    {#if words}
      <textarea
        aria-label="Ticket description"
        rows="8"
        value={editing.body}
        oninput={(event) => (editing.body = event.currentTarget.value)}
      ></textarea>
    {/if}
  {/snippet}
  {#snippet footer()}
    <button class="tf-btn tf-ghost" onclick={onclose}>Cancel</button>
    <button
      class="tf-btn tf-primary"
      {...refusedAttrs(cannot)}
      disabled={!canFile(editing)}
      onclick={() => {
        const filed = { ...editing };
        if (canFile(filed) && cannot === null) onfile(filed);
      }}
    >
      Create ticket
    </button>
  {/snippet}
</TfDialog>

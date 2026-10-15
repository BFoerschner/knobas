<!--
  *Link to…* from the Connections drawer (#820, R-6.1), prototype-6's dialog in
  the new shell: what to link to, the relation, and a note.

  The picker is the launcher's `Session`, as in the rooms UI's `LinkDialog`:
  the same debounce, sequencing and selection, over the job's ports. The link
  is written by `create_link` (BUILT) and stays in knobas, which the dialog
  says in one line (R-15.2).
-->
<script lang="ts">
  import type { LinkRow } from "../../ipc/entity";
  import type { SearchHit } from "../../ipc/search";
  import { Session, type SessionPorts } from "../../launcher/session.svelte";
  import { linkFailureMessage } from "../../detail/links.svelte";
  import { drawn } from "../../detail/relations";
  import { kindSingular } from "../../shell/kinds";
  import type { Layers } from "../layers.svelte";
  import TfDialog from "../settings/TfDialog.svelte";

  let {
    jobKey,
    entityId,
    ports,
    layers,
    onclose,
    onlinked,
  }: {
    jobKey: string;
    entityId: string;
    ports: Pick<SessionPorts, "search" | "launcherHome"> & {
      createLink: (fromId: string, toId: string, relation?: string, note?: string) => Promise<LinkRow>;
    };
    layers: Layers;
    onclose: () => void;
    onlinked: (target: SearchHit) => void;
  } = $props();

  /** The relations prototype-6 offers, as `create_link` stores them. */
  const OFFERED = [
    { id: "related", label: "related" },
    { id: "implements", label: "implements" },
    { id: "documents", label: "documents" },
    { id: "runs-on", label: "runs on" },
    { id: "blocks", label: "blocks" },
    { id: "depends-on", label: "depends on" },
  ];

  // svelte-ignore state_referenced_locally
  // Read once: a `Session` owns a debounce timer and a request counter.
  const session = new Session({ search: ports.search, launcherHome: ports.launcherHome });

  let target = $state<SearchHit | null>(null);
  let relation = $state("related");
  let note = $state("");
  let failure = $state<string | null>(null);
  let writing = $state(false);

  const hits = $derived(
    session.rows.flatMap((row, index) => (row.kind === "hit" && row.hit.entity_id !== entityId ? [{ hit: row.hit, index }] : [])),
  );

  function pick(hit: SearchHit) {
    target = hit;
    failure = null;
  }

  function onsearchkey(event: KeyboardEvent) {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      session.move(event.key === "ArrowDown" ? 1 : -1);
    } else if (event.key === "Enter") {
      event.preventDefault();
      const row = session.current;
      if (row?.kind === "hit") pick(row.hit);
    }
  }

  async function link() {
    if (!target || writing) return;
    writing = true;
    failure = null;
    try {
      const row = drawn(relation, entityId, target.entity_id);
      await ports.createLink(row.fromId, row.toId, row.relation, note.trim() === "" ? undefined : note.trim());
      onlinked(target);
    } catch (rejection) {
      failure = linkFailureMessage(rejection);
    } finally {
      writing = false;
    }
  }
</script>

<TfDialog title="Link {jobKey} to…" {onclose} {layers}>
  {#snippet body()}
    {#if target}
      <div class="tf-picked">
        <span>{target.title}</span>
        <span class="tf-dim">{kindSingular(target.kind, null)}</span>
        <button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => (target = null)}>Change</button>
      </div>
    {:else}
      <input
        type="text"
        autocomplete="off"
        placeholder="Search for a ticket, PR, build, page, asset"
        aria-label="Search for a ticket, PR, build, page, asset"
        value={session.raw}
        oninput={(event) => session.type(event.currentTarget.value)}
        onkeydown={onsearchkey}
      />
      {#if session.error}
        <p class="tf-dim">{session.error}</p>
      {/if}
      {#if hits.length > 0}
        <div class="tf-picklist" role="listbox" aria-label="Link targets">
          {#each hits as { hit, index } (hit.entity_id)}
            <button type="button" role="option" aria-selected={index === session.selected} onclick={() => pick(hit)}>
              <span>{hit.title}</span><span class="tf-dim">{kindSingular(hit.kind, null)}</span>
            </button>
          {/each}
        </div>
      {/if}
    {/if}
    <label class="tf-field"
      >Relation
      <select aria-label="Relation" bind:value={relation}>
        {#each OFFERED as option (option.id)}
          <option value={option.id}>{option.label}</option>
        {/each}
      </select>
    </label>
    <input type="text" placeholder="Note (optional)" aria-label="Note (optional)" bind:value={note} />
    {#if failure}<p class="tf-fail" role="alert">{failure}</p>{/if}
    <p class="tf-dim tf-small">Links stay in knobas and are never written to a source.</p>
  {/snippet}
  {#snippet footer()}
    <button type="button" class="tf-btn tf-ghost" onclick={onclose}>Cancel</button>
    <button type="button" class="tf-btn tf-primary" disabled={!target || writing} onclick={() => void link()}>Link</button>
  {/snippet}
</TfDialog>

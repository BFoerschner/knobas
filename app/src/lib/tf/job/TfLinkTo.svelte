<!--
  *Link to…* from the Connections drawer (#820, R-6.1), prototype-6's dialog in
  the new shell: what to link to, the relation, and a note.

  The picker is the launcher's `Session`, as in the rooms UI's `LinkDialog`:
  the same debounce, sequencing and selection, over the job's ports. The link
  is written by `create_link` (BUILT) and stays in knobas, which the dialog
  says in one line (R-15.2).
-->
<script lang="ts">
  import { onMount, tick } from "svelte";

  import type { LinkRow } from "../../ipc/entity";
  import type { SearchHit } from "../../ipc/search";
  import { Session, type SessionPorts } from "../../launcher/session.svelte";
  import { linkFailureMessage } from "../../detail/links";
  import { drawn } from "../../detail/relations";
  import { JOB_LINK_RELATIONS } from "./link-relations";
  import { kindSingular } from "../../shell/kinds";
  import type { Layers } from "../layers.svelte";
  import type { LinkTarget } from "./current.svelte";
  import TfDialog from "../settings/TfDialog.svelte";
  import TfTick from "../TfTick.svelte";

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
    onlinked: (target: LinkTarget) => void;
  } = $props();

  /** The relations a job's link offers, one list with the launcher's *Link to ⟨job⟩* (#2426). */
  const OFFERED = JOB_LINK_RELATIONS;

  // svelte-ignore state_referenced_locally
  // Read once: a `Session` owns a debounce timer and a request counter.
  const session = new Session({ search: ports.search, launcherHome: ports.launcherHome });
  // Disposed on unmount, as the launcher's is (#1668): a close inside the
  // debounce would otherwise fire a search for a dialog that is gone.
  onMount(() => () => session.dispose());

  let target = $state<LinkTarget | null>(null);
  let relation = $state("related");
  let note = $state("");
  let failure = $state<string | null>(null);
  let writing = $state(false);
  /** The highlighted hit, for the arrow keys and `Enter`. Its own index, because the list drops this ticket's own row. */
  let cursor = $state(0);
  /** Whether the arrow keys have moved: until then no row is marked, though `Enter` takes the first (design system, Forms). */
  let keyed = $state(false);
  const uid = $props.id();
  let field = $state<HTMLInputElement | null>(null);
  let primary = $state<HTMLButtonElement | null>(null);

  const hits = $derived(session.rows.flatMap((row) => (row.kind === "hit" && row.hit.entity_id !== entityId ? [row.hit] : [])));

  function pick(hit: SearchHit) {
    target = hit;
    failure = null;
    // The box that held focus is gone, and the pick made the primary ready.
    void tick().then(() => primary?.focus());
  }

  function change() {
    target = null;
    cursor = 0;
    keyed = false;
    // The box is drawn again on the next tick; focus it there.
    void tick().then(() => field?.focus());
  }

  function onsearch(event: Event & { currentTarget: HTMLInputElement }) {
    cursor = 0;
    keyed = false;
    session.type(event.currentTarget.value);
  }

  function onsearchkey(event: KeyboardEvent) {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      keyed = true;
      const last = hits.length - 1;
      cursor = Math.min(Math.max(0, last), Math.max(0, cursor + (event.key === "ArrowDown" ? 1 : -1)));
    } else if (event.key === "Enter") {
      event.preventDefault();
      const hit = hits[cursor];
      if (hit !== undefined) pick(hit);
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

<TfDialog why={target ? null : "Pick what to link it to."} title="Link {jobKey}" {onclose} {layers}>
  {#snippet body()}
    <div class="tf-formgrid">
      {#if target}
        <span id="tf-lk-to-{uid}">Link to</span>
        <div class="tf-onrow" role="group" aria-labelledby="tf-lk-to-{uid}">
          <span class="tf-chosen"
            ><TfTick /><span class="tf-chosent" title={target.title}>{target.title}</span><span class="tf-dim">{kindSingular(target.kind, null)}</span></span
          >
          <button type="button" class="tf-btn tf-sm tf-ghost" onclick={change}>Change</button>
        </div>
      {:else}
        <label for="tf-lk-to-{uid}">Link to <small class="tf-req">required</small></label>
        <input
          id="tf-lk-to-{uid}"
          type="text"
          autocomplete="off"
          placeholder="Search for a ticket, PR, build, page, asset"
          aria-label="Search for a ticket, PR, build, page, asset"
          bind:this={field}
          value={session.raw}
          oninput={onsearch}
          onkeydown={onsearchkey}
        />
        {#if session.error}
          <p class="tf-dim">{session.error}</p>
        {/if}
        {#if hits.length > 0}
          <div class="tf-fwide tf-picklist" role="listbox" aria-label="Link targets">
            {#each hits as hit, index (hit.entity_id)}
              <button type="button" role="option" aria-selected={keyed && index === cursor} onclick={() => pick(hit)}>
                <span>{hit.title}</span><span class="tf-dim">{kindSingular(hit.kind, null)}</span>
              </button>
            {/each}
          </div>
        {/if}
      {/if}
      <label for="tf-lk-rel-{uid}">Relation</label>
      <select id="tf-lk-rel-{uid}" bind:value={relation}>
        {#each OFFERED as option (option.id)}
          <option value={option.id}>{option.label.charAt(0).toUpperCase() + option.label.slice(1)}</option>
        {/each}
      </select>
      <label for="tf-lk-note-{uid}">Note</label>
      <input id="tf-lk-note-{uid}" type="text" placeholder="Optional" bind:value={note} />
    </div>
    {#if failure}<p class="tf-fail" role="alert">{failure}</p>{/if}
    <p class="tf-dim tf-small">Links stay in knobas and are never written to a source.</p>
  {/snippet}
  {#snippet footer()}
    <button type="button" class="tf-btn tf-ghost" onclick={onclose}>Cancel</button>
    <button type="button" class="tf-btn tf-primary" bind:this={primary} disabled={!target || writing} onclick={() => void link()}>Link</button>
  {/snippet}
</TfDialog>

<!--
  *New view* for *Save as a view* and *Duplicate* (#812, R-3.12): a name, a
  group, and the text the view will keep, shown as it will be stored. The
  editor (#814) replaces this with the whole form; until then how the tickets
  show comes from where the view started.

  *Create view* stays off until there is a name. A save that fails keeps the
  dialog open and says why.
-->
<script lang="ts">
  import type { TicketViewDraft } from "../../ipc/search";
  import TfDialog from "../settings/TfDialog.svelte";

  let {
    draft,
    groups,
    onsave,
    onclose,
  }: {
    draft: TicketViewDraft;
    /** The groups there are, offered under *Group*. */
    groups: string[];
    /** Save it; a rejection's message is shown in the dialog. */
    onsave: (draft: TicketViewDraft) => Promise<void>;
    onclose: () => void;
  } = $props();

  // The dialog edits its own copy: the draft it was opened with stays as it was.
  // svelte-ignore state_referenced_locally
  let name = $state(draft.name);
  // svelte-ignore state_referenced_locally
  let group = $state(draft.group);
  let saving = $state(false);
  const named = $derived(name.trim() !== "");
  let failed = $state<string | null>(null);
  const uid = $props.id();

  async function save() {
    if (name.trim() === "" || saving) return;
    saving = true;
    failed = null;
    try {
      await onsave({ ...draft, name: name.trim(), group: group.trim() || "Mine" });
    } catch (error) {
      failed = error && typeof error === "object" && "message" in error ? String(error.message) : String(error);
    } finally {
      saving = false;
    }
  }
</script>

<TfDialog title="New view" {onclose}>
  {#snippet body()}
    <form
      class="tf-vw-form"
      id="tf-vw-form-{uid}"
      onsubmit={(event) => {
        event.preventDefault();
        void save();
      }}
    >
      <label>
        <span>Name</span>
        <input type="text" aria-label="Name" bind:value={name} placeholder="For example, Payments bugs this sprint" />
      </label>
      <label>
        <span>Group</span>
        <input type="text" aria-label="Group" bind:value={group} list="tf-vw-groups-{uid}" />
        <datalist id="tf-vw-groups-{uid}">
          {#each groups as option (option)}<option value={option}></option>{/each}
        </datalist>
        <small>Pick one, or type a new group</small>
      </label>
    </form>
    <div class="tf-vw-which">
      <span class="tf-pane-h">Which tickets</span>
      {#if draft.query.trim() === ""}
        <span class="tf-dim">every synced ticket</span>
      {:else}
        <code class="tf-mono">{draft.query}</code>
      {/if}
    </div>
    {#if failed !== null}<p class="tf-fail" role="alert">The view was not saved: {failed}</p>{/if}
  {/snippet}
  {#snippet footer()}
    <span class="tf-first tf-dim tf-vw-store">Kept in knobas. Jira never sees it.</span>
    <button type="button" class="tf-btn tf-ghost" onclick={onclose}>Cancel</button>
    <button type="submit" class="tf-btn tf-primary" form="tf-vw-form-{uid}" disabled={!named || saving}>Create view</button>
  {/snippet}
</TfDialog>

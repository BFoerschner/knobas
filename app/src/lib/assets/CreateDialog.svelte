<!--
  *New asset* — the dialog behind the plus on every column header (issue #429,
  spec #427 stories 17, 18 and 34).

  Two fields and no more. Story 17 is *"a new host takes seconds"*, and every
  property an asset could carry is editable in the pane a moment later — so
  asking for them here would put the slow half of the work in front of the
  fast half.

  ## The type list is ordered, never shortened

  The parent's type conventions put what usually goes here at the top, under a
  *usual here* line; **every** type stays in the list under *any type*. A
  container held under a compose project that runs on a VM elsewhere (story 16)
  is the shape a filter here would have refused, and the estate is somebody's
  real infrastructure rather than a diagram.

  ## The id is not asked for

  Story 18: *creating is not naming*. `create_asset` mints `asset:<uuid>` and
  answers with the row, and the Tree goes to its address — so the new asset is
  selected in its own column with its type's monogram without this dialog
  knowing what it is called.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../ipc";
  import type { AssetRow, AssetType, createAsset } from "../ipc/assets";
  import TfDialog from "../tf/settings/TfDialog.svelte";
  import { defaultTypeId, typeChoices, usualHere } from "./editing";

  let {
    at,
    types,
    create,
    onclose,
    oncreated,
  }: {
    /**
     * The level being created inside — `AssetsView`'s `CreateAt`: the
     * `parentId` to send, and the asset it names for the subtitle and the
     * conventions. Both halves are `null` at the top of the estate.
     */
    at: { id: string | null; parent: AssetRow | null };
    /** The built-in table, read once by the view off `asset_types`. */
    types: AssetType[];
    create: typeof createAsset;
    onclose: () => void;
    /** An asset was written. The Tree goes to its address; this dialog is done. */
    oncreated: (row: AssetRow) => void;
  } = $props();

  const choices = $derived(typeChoices(types, at.parent?.type_id ?? null));
  const usual = $derived(usualHere(choices));

  // svelte-ignore state_referenced_locally
  // Read once, at init, and deliberately: this is what the dialog *opens* on.
  // Re-deriving it would overwrite the reader's own choice the next time the
  // table or the parent changed underneath.
  let typeId = $state(defaultTypeId(choices) ?? "");
  let name = $state("");
  /** What the last attempt refused with, in the dialog rather than as a toast. */
  let failure = $state<string | null>(null);
  let writing = $state(false);

  /** Unique per instance, so two stacked dialogs cannot share a field id. */
  const fieldId = `new-asset-${Math.random().toString(36).slice(2, 9)}`;

  /** `Enter` in either field is the same as pressing *Create*. */
  function onfieldkeydown(event: KeyboardEvent) {
    if (event.key !== "Enter") return;
    event.preventDefault();
    void submit();
  }

  async function submit() {
    if (writing || name.trim() === "" || typeId === "") return;
    writing = true;
    failure = null;
    try {
      oncreated(await create(typeId, name, at.id));
    } catch (rejection) {
      // In place, and in the backend's own words: a blank name and a type this
      // build does not carry are refused by `assets::create` by name, and an
      // answer to a dialog belongs in the dialog.
      failure = ipcErrorMessage(rejection);
    } finally {
      writing = false;
    }
  }
</script>

<TfDialog
  closeButton
  title="New asset"
  subtitle={at.parent === null ? "At the top of the estate" : `In ${at.parent.name}`}
  {onclose}
>
  {#snippet body()}
    <div class="tf-formgrid">
        <label for="{fieldId}-name">Name (needed)</label>
        <!-- `TfDialog` focuses the first field, which is this one. -->
        <input
          class="inp"
          id="{fieldId}-name"
          type="text"
          autocomplete="off"
          placeholder="vm-db-01"
          bind:value={name}
          onkeydown={onfieldkeydown}
        />
        <label for="{fieldId}-type">Type</label>
        <select
          class="inp"
          id="{fieldId}-type"
          bind:value={typeId}
          onkeydown={onfieldkeydown}
        >
          <!--
            Two groups rather than two lists: the conventions are an ordering,
            and a reader who wants something else finds it in the same control
            without going anywhere.
          -->
          {#if choices.usual.length > 0}
            <optgroup label="Usual here">
              {#each choices.usual as type (type.id)}
                <option value={type.id}>{type.label}</option>
              {/each}
            </optgroup>
            <optgroup label="Any type">
              {#each choices.rest as type (type.id)}
                <option value={type.id}>{type.label}</option>
              {/each}
            </optgroup>
          {:else}
            {#each choices.rest as type (type.id)}
              <option value={type.id}>{type.label}</option>
            {/each}
          {/if}
        </select>
        {#if usual}
          <span class="hint">{usual}</span>
        {/if}

      {#if failure}
        <p class="fail" role="alert">{failure}</p>
      {/if}
    </div>
  {/snippet}

  {#snippet footer()}
    {#if name.trim() === ""}<span class="tf-dim tf-picked">Name it first.</span>{/if}
    <button class="tf-btn tf-ghost" onclick={onclose}>Cancel</button>
    <button
      class="tf-btn tf-primary"
      disabled={writing || name.trim() === "" || typeId === ""}
      onclick={() => void submit()}
    >
      {writing ? "Creating…" : "Create"}
    </button>
  {/snippet}
</TfDialog>

<style>
  /* A note on the field above it, in the field's column. */
  .hint {
    grid-column: 2;
    margin-top: -2px;
    font-size: var(--t-xs);
    color: var(--ink-2);
  }
</style>

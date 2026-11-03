<!--
  *New asset*: the dialog behind *New asset* on the tree and an asset's page,
  and the plus on every column header (issue #429, spec #427 stories 17, 18
  and 34).

  A name and a type, then **the type's own properties**, there to fill in
  while the reader has them to hand: a VM is born with its hostname and IP
  rather than as a name with four empty fields to chase one at a time. They
  are optional. Story 17 is *"a new host takes seconds"*, and an empty field
  costs nothing: the asset's page fills it in later.

  *Create and add another* keeps the dialog open at the same level with the
  same type, empty, with the name focused again, so a Docker engine's twelve
  containers are twelve names and twelve `Enter`s.

  ## The type list is ordered, never shortened

  The parent's type conventions put what usually goes here at the top, under a
  *usual here* line; **every** type stays in the list under *any type*. A
  container held under a compose project that runs on a VM elsewhere (story 16)
  is the shape a filter here would have refused.

  ## The id is not asked for

  Story 18: *creating is not naming*. `create_asset` mints `asset:<uuid>` and
  answers with the row; the properties are a second write on that row, so when
  they fail the asset still stands and the dialog says what did not save.
-->
<script lang="ts">
  import { tick } from "svelte";
  import { ipcErrorMessage } from "../ipc";
  import type { AssetRow, AssetType, createAsset, editAsset, PropertyValue } from "../ipc/assets";
  import TfDialog from "../tf/settings/TfDialog.svelte";
  import { defaultTypeId, inputTypeFor, parseProperty, propertyEdit, typeChoices, usualHere } from "./editing";

  let {
    at,
    types,
    create,
    edit,
    onclose,
    oncreated,
  }: {
    /**
     * The level being created inside: the `parentId` to send, and the asset it
     * names for the subtitle and the conventions. Both halves are `null` at
     * the top of the estate.
     */
    at: { id: string | null; parent: AssetRow | null };
    /** The built-in table, read once by the view off `asset_types`. */
    types: AssetType[];
    create: typeof createAsset;
    /** Writes the properties filled in here onto the new asset. */
    edit: typeof editAsset;
    onclose: () => void;
    /**
     * An asset was written. `more` is *Create and add another*: the dialog
     * stays open for the next one and the caller only reads again.
     */
    oncreated: (row: AssetRow, more: boolean) => void;
  } = $props();

  const choices = $derived(typeChoices(types, at.parent?.type_id ?? null));
  const usual = $derived(usualHere(choices));

  // svelte-ignore state_referenced_locally
  // Read once, at init: this is what the dialog *opens* on. Re-deriving it
  // would overwrite the reader's own choice when the table changed underneath.
  let typeId = $state(defaultTypeId(choices) ?? "");
  let name = $state("");
  /** What is typed per property key; kept across a change of type, so a hostname typed for a VM survives a switch to a hypervisor. */
  let values = $state<Record<string, string>>({});
  /** What the last attempt refused with, in the dialog rather than as a toast. */
  let failure = $state<string | null>(null);
  /** What *Create and add another* made last, said in the dialog that stays open. */
  let made = $state<string | null>(null);
  let writing = $state(false);
  let nameField = $state<HTMLInputElement | null>(null);

  const uid = $props.id();
  const schema = $derived(types.find((type) => type.id === typeId));

  /** `Enter` in any field creates; with ⌘ or Ctrl it creates and stays for the next one. */
  function onfieldkeydown(event: KeyboardEvent) {
    if (event.key !== "Enter") return;
    event.preventDefault();
    void submit(event.metaKey || event.ctrlKey);
  }

  /** The filled-in properties as edits, or the first one that is not a value of its kind. */
  function propertyEdits(): { edits: ReturnType<typeof propertyEdit>[] } | { refused: string } {
    const edits: ReturnType<typeof propertyEdit>[] = [];
    for (const property of schema?.properties ?? []) {
      const parsed = parseProperty(property.kind, values[property.key] ?? "");
      if ("refused" in parsed) return { refused: `${property.label}: ${parsed.refused}` };
      if (parsed.value !== null) edits.push(propertyEdit(property.key, parsed.value as PropertyValue));
    }
    return { edits };
  }

  async function submit(more: boolean) {
    if (writing || name.trim() === "" || typeId === "") return;
    const props = propertyEdits();
    if ("refused" in props) {
      failure = props.refused;
      return;
    }
    writing = true;
    failure = null;
    let row: AssetRow;
    try {
      row = await create(typeId, name, at.id);
    } catch (rejection) {
      // In place, and in the backend's own words: a blank name and a type this
      // build does not carry are refused by `assets::create` by name.
      failure = ipcErrorMessage(rejection);
      writing = false;
      return;
    }
    if (props.edits.length > 0) {
      try {
        await edit(row.id, props.edits);
      } catch (rejection) {
        // The asset stands. Saying so here, and staying open, is the one way
        // the reader learns which half of the gesture landed.
        failure = `Created ${row.name}, but its properties did not save: ${ipcErrorMessage(rejection)}`;
        writing = false;
        oncreated(row, true);
        return;
      }
    }
    writing = false;
    oncreated(row, more);
    if (!more) return;
    made = row.name;
    name = "";
    values = {};
    await tick();
    nameField?.focus();
  }
</script>

<TfDialog
  closeButton
  title="New asset"
  subtitle={at.parent === null ? "At the top of the estate" : `In ${at.parent.name}`}
  className="tf-as-new"
  {onclose}
>
  {#snippet body()}
    <div class="tf-formgrid">
      <label for="{uid}-name">Name</label>
      <!-- `TfDialog` focuses the first field, which is this one. -->
      <input
        id="{uid}-name"
        type="text"
        class="tf-mono"
        autocomplete="off"
        spellcheck="false"
        bind:this={nameField}
        bind:value={name}
        onkeydown={onfieldkeydown}
      />
      <label for="{uid}-type">Type</label>
      <select id="{uid}-type" bind:value={typeId} onkeydown={onfieldkeydown}>
        <!--
          Two groups rather than two lists: the conventions are an ordering,
          and a reader who wants something else finds it in the same control.
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
    </div>
    {#if usual}
      <p class="tf-dim tf-note tf-as-hint">{usual}</p>
    {/if}

    {#if schema && schema.properties.length > 0}
      <h3 class="tf-as-newh">{schema.label} properties <span class="tf-dim">optional</span></h3>
      <div class="tf-formgrid">
        {#each schema.properties as property (property.key)}
          <label for="{uid}-p-{property.key}">{property.label}</label>
          <input
            id="{uid}-p-{property.key}"
            type={inputTypeFor(property.kind)}
            class:tf-mono={property.kind === "url"}
            autocomplete="off"
            spellcheck="false"
            value={values[property.key] ?? ""}
            oninput={(event) => (values[property.key] = event.currentTarget.value)}
            onkeydown={onfieldkeydown}
          />
        {/each}
      </div>
    {/if}
    <p class="tf-dim tf-note">Environment and owner come from what it sits inside. Anything else goes on its page.</p>

    {#if made !== null && failure === null}
      <p class="tf-as-made" role="status">Created {made}. Name the next one.</p>
    {/if}
    {#if failure}
      <p class="tf-ifail" role="alert">{failure}</p>
    {/if}
  {/snippet}

  {#snippet footer()}
    <button class="tf-btn tf-ghost" onclick={onclose}>Cancel</button>
    <button
      class="tf-btn"
      disabled={writing || name.trim() === "" || typeId === ""}
      title="Create this one and start the next at the same place (⌘Enter)"
      onclick={() => void submit(true)}
    >
      Create and add another
    </button>
    <button
      class="tf-btn tf-primary"
      disabled={writing || name.trim() === "" || typeId === ""}
      onclick={() => void submit(false)}
    >
      {writing ? "Creating…" : "Create"}
    </button>
  {/snippet}
</TfDialog>

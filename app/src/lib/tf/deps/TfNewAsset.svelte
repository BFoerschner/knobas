<!--
  *New asset* (#876, R-11.20, R-11.19a): a name, a type and where it sits, as
  the asset tree's create does. The layer's types come first; the line under
  *Inside* names what usually goes inside the place picked. From a ticket,
  *Link it to <key>* is ticked.

  `create_asset` leaves environment and owner unset, so they inherit. The
  ticket link is a second call once the asset exists; when it fails the asset
  still stands, so the dialog closes and the page says the link did not save.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import type { AssetType, DepLayer, Dependencies } from "../../ipc/assets";
  import TfDialog from "../settings/TfDialog.svelte";
  import { typeChoices, usualLine } from "./add";
  import { assetPaths } from "./estate-paths";
  import type { AddPorts } from "./ports";

  let {
    read,
    layer = null,
    parent = null,
    ticket = null,
    ports,
    onclose,
    oncreated,
  }: {
    /** The whole estate: where *Inside* picks from. */
    read: Dependencies;
    /** The layer whose *+ Add* opened it, or `null` from *Add ▾*. */
    layer?: DepLayer | null;
    /** The place *Inside* starts on. */
    parent?: string | null;
    /** The ticket the map opens from: its key and entity id. */
    ticket?: { key: string; id: string } | null;
    ports: AddPorts;
    onclose: () => void;
    /** Created: its id and name, and why the ticket link did not save, if it did not. */
    oncreated: (made: { id: string; name: string; unlinked: string | null }) => void;
  } = $props();

  const uid = $props.id();
  let types = $state.raw<AssetType[]>([]);
  let name = $state("");
  let typeId = $state("");
  // svelte-ignore state_referenced_locally
  let inside = $state<string | null>(parent);
  let link = $state(true);
  let failure = $state<string | null>(null);
  let writing = $state(false);

  const choices = $derived(typeChoices(layer?.kinds ?? [], types));
  const places = $derived(assetPaths(read).places);
  const usual = $derived(usualLine(inside, read, types));

  async function load() {
    try {
      types = await ports.assetTypes();
      typeId = (choices.here[0] ?? choices.rest[0])?.id ?? "";
    } catch (error) {
      failure = `The asset types could not be read: ${ipcErrorMessage(error)}`;
    }
  }
  void load();

  async function create() {
    if (writing) return;
    const named = name.trim();
    if (named === "") {
      failure = "A name is needed.";
      return;
    }
    writing = true;
    failure = null;
    let id: string;
    try {
      id = (await ports.createAsset(typeId, named, inside)).id;
    } catch (error) {
      failure = ipcErrorMessage(error);
      writing = false;
      return;
    }
    let unlinked: string | null = null;
    if (ticket !== null && link) {
      try {
        await ports.createLink(ticket.id, id);
      } catch (error) {
        unlinked = ipcErrorMessage(error);
      }
    }
    writing = false;
    oncreated({ id, name: named, unlinked });
  }
</script>

<TfDialog title={layer ? `New asset in ${layer.name}` : "New asset"} {onclose}>
  {#snippet body()}
    <div class="tf-formgrid">
      <label for="{uid}-name">Name</label>
      <input
        id="{uid}-name"
        type="text"
        class="tf-mono"
        placeholder="payments-api"
        autocomplete="off"
        bind:value={name}
        onkeydown={(event) => {
          if (event.key === "Enter") {
            event.preventDefault();
            void create();
          }
        }}
      />
      <label for="{uid}-type">Type</label>
      <select id="{uid}-type" bind:value={typeId}>
        {#if choices.here.length > 0}
          <optgroup label="In this layer">
            {#each choices.here as type (type.id)}<option value={type.id}>{type.label}</option>{/each}
          </optgroup>
          <optgroup label="Any type">
            {#each choices.rest as type (type.id)}<option value={type.id}>{type.label}</option>{/each}
          </optgroup>
        {:else}
          {#each choices.rest as type (type.id)}<option value={type.id}>{type.label}</option>{/each}
        {/if}
      </select>
      <label for="{uid}-in">Inside</label>
      <select id="{uid}-in" value={inside ?? ""} onchange={(e) => (inside = e.currentTarget.value || null)}>
        <option value="">Top of the estate</option>
        {#each places as place (place.id)}<option value={place.id}>{place.path}</option>{/each}
      </select>
    </div>
    <p class="tf-dim tf-note">{usual}</p>
    {#if ticket !== null}
      <label class="tf-ck"><input type="checkbox" bind:checked={link} /> Link it to {ticket.key}, so it is on this ticket's map</label>
    {/if}
    <p class="tf-dim tf-note">Environment and owner come from what it sits inside. Its properties are in its editor.</p>
    {#if failure !== null}<p class="tf-ifail" role="alert">{failure}</p>{/if}
  {/snippet}
  {#snippet footer()}
    <button type="button" class="tf-btn tf-ghost" onclick={onclose}>Cancel</button>
    <button type="button" class="tf-btn tf-primary" disabled={writing || typeId === ""} onclick={() => void create()}>Create</button>
  {/snippet}
</TfDialog>

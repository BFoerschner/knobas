<!--
  Where generated folders are kept (ADR-0046), one row under Settings ›
  Renderers' folder renderers: the store's path, how many folders and bytes
  of its cap, and where that place comes from. *Change…* asks for a folder
  (the default its placeholder; the backend's refusal under the field, never
  inside a checkout); *Clear* removes every generated folder, disabled at
  zero with its room kept.

  Counting the folders walks the store on disk the first time (#2519), so the
  section does not wait for it: until the store answers (`store` is `null`)
  the row keeps its room unseen, its count two lines high and its path cut
  rather than wrapped under, so it shows where it stands and nothing moves.
-->
<script lang="ts">
  import { clearFolderRenderStore, setFolderRenderStore, type MadeStore } from "../../ipc/app";
  import { ipcErrorMessage } from "../../ipc";
  import { sizeLabel } from "../../render/folder";
  import { push } from "../../shell/toasts.svelte";
  import { thousands } from "../counted";
  import { layers as sharedLayers, type Layers } from "../layers.svelte";
  import TfPath from "../TfPath.svelte";
  import TfDialog from "./TfDialog.svelte";

  let {
    store,
    set = (path) => setFolderRenderStore(path),
    clear = () => clearFolderRenderStore(),
    onstore,
    layers = sharedLayers,
  }: {
    /** `null` while the store is read: the row is held unseen in its room. */
    store: MadeStore | null;
    set?: (path: string | null) => Promise<MadeStore>;
    clear?: () => Promise<MadeStore>;
    /** The store as the backend answered after a change or a clear. */
    onstore: (next: MadeStore) => void;
    layers?: Layers;
  } = $props();

  const PLACE = { set: "In the folder you chose.", clones: "Beside your clones.", profile: "In knobas's own data, until you set a clones root." } as const;
  const counted = (n: number) => `${thousands(n)} ${n === 1 ? "folder" : "folders"}`;
  /** The cap in whole GB where it is whole (4 GB), else as any size. */
  const capLabel = (bytes: number) => (bytes % 1024 ** 3 === 0 ? `${bytes / 1024 ** 3} GB` : sizeLabel(bytes));

  let editing = $state(false);
  let typed = $state("");
  let refused = $state<string | null>(null);
  let saving = $state(false);
  let clearing = $state(false);

  function change() {
    if (store === null) return;
    typed = store.set ?? "";
    refused = null;
    editing = true;
  }

  async function save(path: string | null) {
    saving = true;
    try {
      onstore(await set(path));
      editing = false;
    } catch (cause) {
      refused = ipcErrorMessage(cause);
    } finally {
      saving = false;
    }
  }

  async function clearAll() {
    if (store === null) return;
    clearing = true;
    const before = store;
    try {
      onstore(await clear());
      push({ text: `Cleared ${thousands(before.folders)} generated ${before.folders === 1 ? "folder" : "folders"}, ${sizeLabel(before.bytes)}` });
    } catch (cause) {
      push({ text: `Could not clear the generated folders: ${ipcErrorMessage(cause)}`, tone: "err" });
    } finally {
      clearing = false;
    }
  }
</script>

<div class="tf-stg-row tf-made-row" style:visibility={store === null ? "hidden" : undefined}>
  <div class="tf-stg-l tf-made-l">
    Generated folders<small>{#if store !== null}{counted(store.folders)}, {sizeLabel(store.bytes)} of {capLabel(store.cap_bytes)}. {PLACE[store.place]}{/if}</small>
  </div>
  <div class="tf-stg-c tf-made-c">
    {#if store !== null}<TfPath path={store.dir} />{/if}
    <button type="button" class="tf-btn tf-sm" disabled={store === null} onclick={change}>Change…</button>
    <button type="button" class="tf-btn tf-sm tf-ghost" disabled={store === null || store.folders === 0 || clearing} onclick={() => void clearAll()}>Clear</button>
  </div>
</div>

{#if editing && store !== null}
  <TfDialog title="Where generated folders are kept" {layers} onclose={() => (editing = false)}>
    {#snippet body()}
      <form
        class="tf-formgrid"
        id="tf-stg-made"
        onsubmit={(event) => {
          event.preventDefault();
          void save(typed.trim() === "" ? null : typed.trim());
        }}
      >
        <label for="tf-stg-made-in">Folder</label>
        <input id="tf-stg-made-in" aria-label="Folder" class="tf-mono" type="text" autocomplete="off" spellcheck="false" placeholder={store.default_dir} bind:value={typed} />
      </form>
      <p class="tf-dsub">knobas keeps them in a folder named knobas-rendered inside it. Changing it removes what is kept in the old place. Folders are made again when you open them.</p>
      {#if refused !== null}<div class="tf-banner tf-err tf-dbanner" role="alert">{refused}</div>{/if}
    {/snippet}
    {#snippet footer()}
      <button type="button" class="tf-btn tf-ghost" disabled={saving || store.set === null} onclick={() => void save(null)}>Use the default</button>
      <button type="submit" class="tf-btn tf-primary" form="tf-stg-made" disabled={saving}>Save</button>
    {/snippet}
  </TfDialog>
{/if}

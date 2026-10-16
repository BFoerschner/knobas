<!--
  *Where should clones go?* (#846, R-8.25), prototype-6's `openCloneRoot`:
  asked by *Clone* when no clones root is set, prefilled `~/code`. *Set and
  clone* saves it (`set_clones_root`, the same setting Settings › Checkouts
  edits) and then clones.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import { layers as sharedLayers, type Layers } from "../layers.svelte";
  import TfDialog from "../settings/TfDialog.svelte";

  let {
    save,
    layers = sharedLayers,
    onclose,
    ondone,
  }: {
    /** `set_clones_root`. */
    save: (path: string) => Promise<void>;
    layers?: Layers;
    onclose: () => void;
    /** After the root is saved: clone. */
    ondone: () => void;
  } = $props();

  let root = $state("~/code");
  let error = $state<string | null>(null);
  let busy = $state(false);

  async function go() {
    busy = true;
    try {
      await save(root.trim() || "~/code");
      onclose();
      ondone();
    } catch (cause) {
      error = ipcErrorMessage(cause);
      busy = false;
    }
  }
</script>

<TfDialog title="Where should clones go?" {onclose} {layers}>
  {#snippet body()}
    <form
      class="tf-formgrid"
      id="tf-clone-root"
      onsubmit={(event) => {
        event.preventDefault();
        void go();
      }}
    >
      <label for="tf-crt">Clones root</label>
      <input id="tf-crt" class="tf-mono" type="text" autocomplete="off" spellcheck="false" bind:value={root} />
    </form>
    <p class="tf-dsub">
      knobas clones every repository into its own folder here, and looks here for clones you made yourself. You can change
      it in Settings › Checkouts.
    </p>
    {#if error !== null}<div class="tf-banner tf-err tf-dbanner" role="alert">{error}</div>{/if}
  {/snippet}
  {#snippet footer()}
    <button type="button" class="tf-btn tf-ghost" onclick={onclose}>Cancel</button>
    <button type="submit" class="tf-btn tf-primary" form="tf-clone-root" disabled={busy}>Set and clone</button>
  {/snippet}
</TfDialog>

<!--
  Quitting with sites that hold unsaved changes (decisions §10, §16). Rust asks every live site's page at once, each for
  at most 300 ms, and quits when none objects; otherwise it sends `site:quit` with the ones that do, and this dialog
  names them. *Show* goes to that site; *Keep knobas open* is first; *Quit anyway* quits. Arrival focus is on the ×, by
  the dialog's own rule, so `Enter` after it opens quits nothing. A second quit within 3 seconds always quits (Rust).
-->
<script lang="ts">
  import { siteQuit, type SiteStatus } from "../../ipc/site";
  import { layers as sharedLayers, type Layers } from "../layers.svelte";
  import TfDialog from "../settings/TfDialog.svelte";
  import { siteHash, siteStore, type SiteStore } from "./site.svelte";

  let {
    unsaved,
    onclose,
    layers = sharedLayers,
    quit = () => void siteQuit(),
    store = siteStore,
  }: {
    unsaved: readonly SiteStatus[];
    onclose: () => void;
    layers?: Layers;
    quit?: () => void;
    store?: SiteStore;
  } = $props();

  const sentence = $derived(
    unsaved.length === 1
      ? `${store.name(unsaved[0]!.source)} holds changes it has not saved. Quitting closes it.`
      : `${unsaved.length === 2 ? "Two" : unsaved.length} sites hold changes they have not saved. Quitting closes them.`,
  );

  function show(source: string) {
    onclose();
    location.hash = siteHash(source);
  }
</script>

<TfDialog title="Quit with unsaved changes?" {onclose} {layers} className="tf-site-quit">
  {#snippet body()}
    <p>{sentence}</p>
    <ul class="tf-site-quit-list">
      {#each unsaved as site (site.source)}
        <li>
          <span class="tf-site-name">{store.name(site.source)}</span>
          <span class="tf-site-quit-title">{site.title}</span>
          <button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => show(site.source)}>Show</button>
        </li>
      {/each}
    </ul>
  {/snippet}
  {#snippet footer()}
    <button type="button" class="tf-btn tf-ghost" onclick={onclose}>Keep knobas open</button>
    <button type="button" class="tf-btn tf-primary tf-danger" onclick={quit}>Quit anyway</button>
  {/snippet}
</TfDialog>

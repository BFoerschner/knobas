<!--
  Settings › Renderers › *Folder renderers* (ADR-0044): the saved list, each
  row its page (`TfFolderRendererPage`, `#/settings/renderers/folder/<id>`),
  and *Add a folder renderer*. The list is the app's copy
  (`render/folders.svelte.ts`), so the Code page draws what is saved here.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import { folderRenderers, type FolderRenderers } from "../../render/folders.svelte";
  import { secondsLabel } from "../../time/format";
  import { folderPageHash, OUTPUTS, takesLine } from "./folder-page";
  import { land, LAND_ADD, landKey, takeLanding } from "./landing";

  let {
    folders = folderRenderers,
    answered = false,
  }: {
    folders?: FolderRenderers;
    /** Whether the page has read the list already: an empty one says so at once, not a frame after its own read (the nothing-moves pass). */
    answered?: boolean;
  } = $props();

  let failed = $state<string | null>(null);
  let read = $state(false);
  $effect(() => {
    folders.load().then(
      () => (read = true),
      (cause: unknown) => (failed = ipcErrorMessage(cause)),
    );
  });
  // Back from a folder renderer's page: focus the entry it saved, the one that took a removed one's place, or *Add a folder renderer*.
  $effect(() => {
    if (!read) return;
    const key = takeLanding("folder-renderers");
    if (key !== null) land("folder-renderers", key);
  });
  const outputOf = (value: string) => OUTPUTS.find((o) => o.value === value)?.label.replace(/ \(.*\)$/, "") ?? value;
</script>

<div class="tf-stg-sec" data-land-list="folder-renderers">
  <h2 tabindex="-1" data-land={landKey("folder-renderers", LAND_ADD)}>Folder renderers</h2>
  <p class="tf-stg-p">
    A program of yours that takes a folder and makes a picture, text, Markdown or a page out of it. The Code page draws a folder it takes in the
    file’s place. It runs on a copy of the folder at the commit shown, never in your checkout.
  </p>
  {#if failed !== null}
    <p class="tf-stg-errt" role="status">Could not read the folder renderers: {failed}</p>
  {:else if (read || answered) && folders.list.length === 0}
    <p class="tf-dim">No folder renderers yet.</p>
  {:else if folders.list.length > 0}
    <table class="tf-tbl tf-rnd-own">
      <thead><tr><th>Renderer</th><th>Command</th><th>Takes</th><th>Makes</th><th><span class="tf-sr">Actions</span></th></tr></thead>
      <tbody>
        {#each folders.list as r (r.id)}
          <tr class="tf-land" tabindex="-1" data-land={landKey("folder-renderers", r.id)}>
            <td><b>{r.name}</b></td>
            <td class="tf-small tf-wrapall"><span class="tf-mono">{r.command}</span></td>
            <td class="tf-small">{takesLine(r)}</td>
            <td class="tf-small">{outputOf(r.output)}, {secondsLabel(r.timeout_secs)}</td>
            <td class="tf-right"><a class="tf-btn tf-sm tf-ghost" href={folderPageHash({ renderer: r.id })} aria-label="Edit {r.name}">Edit</a></td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
  <div class="tf-stg-acts tf-stg-gap"><a class="tf-btn tf-sm" href={folderPageHash()}>Add a folder renderer</a></div>
</div>

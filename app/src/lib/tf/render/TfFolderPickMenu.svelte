<!--
  The menu a folder's glyph opens while several folder renderers take the folder and none was picked (ux26, ruling 10;
  the mock's tag 12): each renderer as its name over its program, the first ticked, then *Folder renderers…* to Settings.
  Choosing one is the folder's pick. The Files tree and a comparison's file list open the same one.
-->
<script lang="ts">
  import type { FolderRenderer } from "../../ipc/app";
  import { settingsHash } from "../settings/settings";
  import { programOf } from "../path";
  import TfMenu from "../TfMenu.svelte";
  import TfPath from "../TfPath.svelte";

  let {
    open = $bindable(false),
    opener,
    folder,
    taking,
    onpick,
  }: {
    open?: boolean;
    opener: HTMLElement;
    /** The folder's path, `""` the top. */
    folder: string;
    /** The renderers that take it, in the person's order. */
    taking: readonly FolderRenderer[];
    onpick: (renderer: FolderRenderer) => void;
  } = $props();
</script>

<TfMenu bind:open {opener} label="Render {folder === '' ? 'the repository' : folder} with" className="tf-rsw-m tf-frpick-m" focusOn="button">
  <div class="tf-rsw-mh">Render with</div>
  {#each taking as r, n (r.id)}
    <button type="button" role="menuitemradio" aria-checked={n === 0} class:tf-on={n === 0} title="{r.name}
{r.command}" onclick={() => onpick(r)}>
      <span class="tf-rsw-tick" aria-hidden="true">{n === 0 ? "✓" : ""}</span>
      <span class="tf-rsw-name"><span class="tf-rsw-nm">{r.name}</span>{" "}<TfPath class="tf-rsw-sub" path={programOf(r.command)} title={r.command} /></span>
    </button>
  {/each}
  <hr />
  <a role="menuitem" href={settingsHash("renderers")} onclick={() => (open = false)}><span class="tf-rsw-tick" aria-hidden="true"></span>Folder renderers…</a>
</TfMenu>

<!--
  The made bar's *Open in… ▾* (ADR-0046): the person's applications on a
  generated folder, or the generated file shown, as `TfOpenIn` offers them
  on a checkout. Each with its command filled, the default first and focused,
  so `Enter` runs it; a file uses an application's file command, the folder
  command for one that has none. An application whose command needs `{url}`
  is drawn disabled: a generated folder has no page on the source. Then
  *Copy path*: the file's absolute path, or the folder's. Disabled, its room
  held, until a folder is kept.
-->
<script lang="ts">
  import { folderRenderOpen } from "../../ipc/app";
  import { ipcErrorMessage } from "../../ipc";
  import { push } from "../../shell/toasts.svelte";
  import { layers as sharedLayers, type Layers } from "../layers.svelte";
  import TfMenu from "../TfMenu.svelte";
  import TfPath from "../TfPath.svelte";
  import { apps as sharedApps, type Apps } from "./apps.svelte";
  import { commandFor, fill, joinInside, menuOrder } from "./openin";

  let {
    token,
    dir,
    file = null,
    line = null,
    apps = sharedApps,
    run = (application, token, file, line) => folderRenderOpen(application, token, file, line),
    copy = (text: string) => navigator.clipboard.writeText(text),
    layers = sharedLayers,
  }: {
    /** The generated folder's token; `null` while none is kept. */
    token: string | null;
    /** Where it is kept, absolute (`MadeTree.dir`). */
    dir: string | null;
    /** A generated file shown, from the folder's top. */
    file?: string | null;
    line?: number | null;
    apps?: Apps;
    run?: (application: string | null, token: string, file: string | null, line: number | null) => Promise<string[]>;
    copy?: (text: string) => Promise<void>;
    layers?: Layers;
  } = $props();

  let open = $state(false);
  let face = $state<HTMLButtonElement | null>(null);
  const entries = $derived(apps.settings === null ? [] : menuOrder(apps.settings));
  const fileAt = $derived(dir !== null && file !== null ? joinInside(dir, file) : null);
  const ready = $derived(token !== null && dir !== null);

  $effect(() => {
    void apps.ensure();
  });

  function start(id: string, name: string) {
    open = false;
    if (token === null) return;
    run(id, token, file, line).catch((cause: unknown) => push({ text: `Could not open ${name}: ${ipcErrorMessage(cause)}`, tone: "err" }));
  }

  function copyPath() {
    open = false;
    const path = fileAt ?? dir;
    if (path === null) return;
    copy(path).then(
      () => push({ text: `Copied ${path}`, subject: "copy-made-path" }),
      (cause: unknown) => push({ text: `Could not copy: ${ipcErrorMessage(cause)}`, tone: "err" }),
    );
  }
</script>

<span class="tf-openin">
  <button
    bind:this={face}
    type="button"
    class="tf-btn tf-sm tf-ghost"
    aria-haspopup="menu"
    aria-expanded={open}
    disabled={!ready}
    title={dir === null ? "Open in an application once a folder is made" : `Open ${fileAt ?? dir} in an application`}
    onclick={(event) => {
      event.stopPropagation();
      open = !open;
    }}>Open in…<svg class="tf-caret" width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true"><path d="M4 6l4 4 4-4" /></svg></button
  >
  <TfMenu bind:open opener={face} label="Open in" className="tf-appmenu" {layers}>
    {#each entries as entry (entry.app.id)}
      {@const command = commandFor(entry.app, file)}
      {@const needsPage = command.includes("{url}")}
      <button
        role="menuitem"
        disabled={needsPage}
        title={needsPage ? `${entry.app.name}'s command needs a page on the source, and a generated folder has none` : undefined}
        onclick={() => start(entry.app.id, entry.app.name)}
        ><b>{entry.app.name}{#if entry.isDefault}{" "}<span class="tf-dim tf-plain">(default)</span>{/if}</b>{#if dir !== null}<TfPath
            path={fill(command, { path: dir, file: fileAt, line })}
          />{/if}</button
      >
    {/each}
    {#if entries.length > 0}<hr />{/if}
    <button role="menuitem" onclick={copyPath}><b>Copy path</b>{#if dir !== null}<TfPath path={fileAt ?? dir} />{/if}</button>
  </TfMenu>
</span>

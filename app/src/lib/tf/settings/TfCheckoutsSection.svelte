<!--
  Settings › Checkouts and Open in… (#844, R-13a.3), prototype-6's
  `STGV.checkouts` over `checkout_settings`: the clones root (*Change…*
  edits it, `set_clones_root`), the git knobas found, and the *Open in…*
  applications with *Make default* and *Edit applications…*.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import { setClonesRoot } from "../../ipc/entity";
  import { push } from "../../shell/toasts.svelte";
  import { layers as sharedLayers, type Layers } from "../layers.svelte";
  import { apps as sharedApps, type Apps } from "../openin/apps.svelte";
  import TfAppsManage from "../openin/TfAppsManage.svelte";

  let {
    apps = sharedApps,
    saveRoot = (path: string | null) => setClonesRoot(path),
    layers = sharedLayers,
  }: {
    apps?: Apps;
    saveRoot?: (path: string | null) => Promise<void>;
    layers?: Layers;
  } = $props();

  let editingRoot = $state(false);
  let typed = $state("");
  let managing = $state(false);

  const settings = $derived(apps.settings);

  $effect(() => {
    void apps.reload();
  });

  async function saveTyped() {
    try {
      await saveRoot(typed.trim() === "" ? null : typed.trim());
      editingRoot = false;
      await apps.reload();
    } catch (cause) {
      push({ text: `Could not save the clones root: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }

  async function makeDefault(id: string) {
    if (settings === null) return;
    try {
      await apps.save(settings.applications, id);
    } catch (cause) {
      push({ text: `Could not save the default: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }
</script>

<header class="tf-stg-h">
  <h1>Checkouts and Open in…</h1>
  <p>Where your clones live, and the applications that open them.</p>
</header>
{#if settings !== null}
  <div class="tf-stg-row">
    <div class="tf-stg-l">Clones root<small>knobas finds clones here and clones new ones into it.</small></div>
    <div class="tf-stg-c tf-stg-inl">
      {#if editingRoot}
        <input class="tf-stg-in tf-mono" type="text" aria-label="Clones root" bind:value={typed} />
        <button class="tf-btn tf-sm tf-primary" onclick={() => void saveTyped()}>Save</button>
        <button class="tf-btn tf-sm tf-ghost" onclick={() => (editingRoot = false)}>Cancel</button>
      {:else}
        <span class="tf-mono">{settings.clones_root ?? "Not set"}</span>
        <button class="tf-btn tf-sm" onclick={() => ((typed = settings.clones_root ?? "~/code"), (editingRoot = true))}>Change…</button>
      {/if}
    </div>
  </div>
  <div class="tf-stg-row">
    <div class="tf-stg-l">
      git<small>knobas runs git to read a checkout, show its uncommitted diff and clone. Never to commit, fetch or pull.</small>
    </div>
    <div class="tf-stg-c">
      {#if settings.git.status === "found"}
        <span class="tf-stg-ok"><span class="tf-dot tf-ok"></span> git at <span class="tf-mono">{settings.git.path}</span></span>
      {:else if settings.git.status === "no_git"}
        <span><span class="tf-dot tf-warn"></span> git was not found on this machine</span>
      {:else}
        <span><span class="tf-dot tf-warn"></span> git could not be checked: {settings.git.reason}</span>
      {/if}
    </div>
  </div>
  <div class="tf-stg-sec">
    <h2>Open in…</h2>
    <p class="tf-stg-p">
      The first one is the default for <b>Open</b>. <span class="tf-mono">{"{path}"}</span> is the checkout,
      <span class="tf-mono">{"{file}"}</span> and <span class="tf-mono">{"{line}"}</span> the file you have open.
    </p>
    <table class="tf-tbl">
      <thead><tr><th>Application</th><th>Opens a checkout</th><th>Opens a file</th><th><span class="tf-sr">Default</span></th></tr></thead>
      <tbody>
        {#each settings.applications as app (app.id)}
          <tr>
            <td><b>{app.name}</b>{#if app.id === settings.default_application}{" "}<span class="tf-chip">default</span>{/if}</td>
            <td class="tf-mono tf-small">{app.command}</td>
            <td class="tf-mono tf-small">{#if app.file_command}{app.file_command}{:else}<span class="tf-dim">opens the checkout</span>{/if}</td>
            <td class="tf-right">
              {#if app.id !== settings.default_application}
                <button class="tf-btn tf-sm tf-ghost" onclick={() => void makeDefault(app.id)}>Make default</button>
              {/if}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
    <div class="tf-stg-acts tf-stg-gap"><button class="tf-btn" onclick={() => (managing = true)}>Edit applications…</button></div>
  </div>
{/if}

{#if managing}
  <TfAppsManage {apps} sample={null} {layers} onclose={() => (managing = false)} />
{/if}

<!--
  Settings › Checkouts and Open in… (#844, R-13a.3), prototype-6's
  `STGV.checkouts` over `checkout_settings`: the clones root (*Change…*
  edits it, `set_clones_root`), the git knobas found, and the *Open in…*
  applications with *Make default* and *Edit applications…*. Then the pull
  request worktrees knobas made under the clones root (#2285, ADR-0035), each
  with *Remove*: a closed one also goes on the next pull request opened.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import {
    pullRequestWorktrees,
    removePullRequestWorktree,
    setClonesRoot,
    type PullRequestWorktree,
  } from "../../ipc/entity";
  import { push } from "../../shell/toasts.svelte";
  import { layers as sharedLayers, type Layers } from "../layers.svelte";
  import { apps as sharedApps, type Apps } from "../openin/apps.svelte";
  import TfAppsManage from "../openin/TfAppsManage.svelte";

  let {
    apps = sharedApps,
    saveRoot = (path: string | null) => setClonesRoot(path),
    worktreePorts = { list: () => pullRequestWorktrees(), remove: (path: string) => removePullRequestWorktree(path) },
    layers = sharedLayers,
  }: {
    apps?: Apps;
    saveRoot?: (path: string | null) => Promise<void>;
    /** The pull request worktrees (#2285): the list, and removing one. */
    worktreePorts?: { list: () => Promise<PullRequestWorktree[]>; remove: (path: string) => Promise<PullRequestWorktree[]> };
    layers?: Layers;
  } = $props();

  let worktrees = $state<PullRequestWorktree[] | null>(null);

  let editingRoot = $state(false);
  let typed = $state("");
  let managing = $state(false);

  const settings = $derived(apps.settings);

  $effect(() => {
    void apps.reload();
    worktreePorts.list().then(
      (list) => (worktrees = list),
      () => (worktrees = []),
    );
  });

  async function removeWorktree(path: string) {
    try {
      worktrees = await worktreePorts.remove(path);
    } catch (cause) {
      push({ text: `Could not remove the worktree: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }

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
        {#if settings.clones_root !== null}<span class="tf-mono">{settings.clones_root}</span>{:else}<span class="tf-dim">Not set</span>{/if}
        <button class="tf-btn tf-sm" onclick={() => ((typed = settings.clones_root ?? "~/code"), (editingRoot = true))}>Change…</button>
      {/if}
    </div>
  </div>
  <div class="tf-stg-row">
    <div class="tf-stg-l">
      git<small
        >knobas runs git to read a checkout, show its uncommitted diff, clone, and fetch a pull request into its own worktree. Never to
        commit, pull or push.</small
      >
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
      The default is listed first in Open in… and runs on <b>Enter</b>. <span class="tf-mono">{"{path}"}</span> is the checkout,
      <span class="tf-mono">{"{file}"}</span> and <span class="tf-mono">{"{line}"}</span> the file you have open.
    </p>
    <table class="tf-tbl">
      <thead><tr><th>Application</th><th>Opens a checkout</th><th>Opens a file</th><th><span class="tf-sr">Default</span></th></tr></thead>
      <tbody>
        {#each settings.applications as app (app.id)}
          <tr>
            <td><b>{app.name}</b>{#if app.id === settings.default_application}{" "}<span class="tf-chip">default</span>{/if}</td>
            <td class="tf-small"><span class="tf-mono">{app.command}</span></td>
            <td class="tf-small">{#if app.file_command}<span class="tf-mono">{app.file_command}</span>{:else}<span class="tf-dim">opens the checkout</span>{/if}</td>
            <td class="tf-right">
              {#if app.id !== settings.default_application}
                <button class="tf-btn tf-sm tf-ghost" onclick={() => void makeDefault(app.id)}>Make default</button>
              {/if}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
    <div class="tf-stg-acts tf-stg-gap"><button class="tf-btn tf-sm" onclick={() => (managing = true)}>Edit applications…</button></div>
  </div>
  <div class="tf-stg-sec">
    <h2>Pull request worktrees</h2>
    <p class="tf-stg-p">
      A pull request opens in a worktree of its own under the clones root, so your checkout is never touched. A closed one is removed
      the next time you open a pull request; one with changes in it stays until you remove it.
    </p>
    {#if worktrees !== null && worktrees.length > 0}
      <table class="tf-tbl">
        <thead><tr><th>Pull request</th><th>Worktree</th><th><span class="tf-sr">Remove</span></th></tr></thead>
        <tbody>
          {#each worktrees as tree (tree.path)}
            <tr>
              <td>#{tree.number}{#if tree.closed}{" "}<span class="tf-chip">closed</span>{/if}</td>
              <td class="tf-small"><span class="tf-mono">{tree.path}</span></td>
              <td class="tf-right"
                ><button class="tf-btn tf-sm tf-ghost" title="Remove {tree.path}" onclick={() => void removeWorktree(tree.path)}
                  >Remove</button
                ></td
              >
            </tr>
          {/each}
        </tbody>
      </table>
    {:else if worktrees !== null}
      <p class="tf-dim">None yet.</p>
    {/if}
  </div>
{/if}

{#if managing}
  <TfAppsManage {apps} sample={null} {layers} onclose={() => (managing = false)} />
{/if}

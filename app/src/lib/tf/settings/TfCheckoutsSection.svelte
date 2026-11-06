<!--
  Settings › Checkouts and Open in… (#844, R-13a.3), prototype-6's
  `STGV.checkouts` over `checkout_settings`: the clones root (*Change…* opens
  its dialog, `set_clones_root`), the git knobas found, and the *Open in…*
  applications with *Make default* and *Edit applications…*. Then the pull
  request worktrees knobas made under the clones root (#2285, ADR-0035), each
  with *Remove…*, which confirms because it deletes a folder and what is in it:
  a closed one also goes on the next pull request opened.
-->
<script lang="ts">
  import TfStateChip from "../TfStateChip.svelte";
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
  import TfDialog from "./TfDialog.svelte";

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
  let rootError = $state<string | null>(null);
  let savingRoot = $state(false);
  let managing = $state(false);
  /** The worktree *Remove…* is asking about. */
  let removing = $state<PullRequestWorktree | null>(null);

  const settings = $derived(apps.settings);
  /** The typed root is the one stored: the dialog's primary has nothing to do. */
  const rootUnchanged = $derived((typed.trim() === "" ? null : typed.trim()) === (settings?.clones_root ?? null));

  function changeRoot() {
    typed = settings?.clones_root ?? "~/code";
    rootError = null;
    editingRoot = true;
  }

  $effect(() => {
    void apps.reload();
    worktreePorts.list().then(
      (list) => (worktrees = list),
      () => (worktrees = []),
    );
  });

  async function removeWorktree(path: string) {
    removing = null;
    try {
      worktrees = await worktreePorts.remove(path);
    } catch (cause) {
      push({ text: `Could not remove the worktree: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }

  async function saveTyped() {
    if (rootUnchanged || savingRoot) return;
    savingRoot = true;
    rootError = null;
    try {
      await saveRoot(typed.trim() === "" ? null : typed.trim());
      editingRoot = false;
      await apps.reload();
    } catch (cause) {
      // The dialog stays open with what was typed, and says why under it.
      rootError = ipcErrorMessage(cause);
    } finally {
      savingRoot = false;
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
  <h1>Checkouts and Open in</h1>
  <p>Where your clones live, and the applications that open them.</p>
</header>
{#if settings !== null}
  <div class="tf-stg-row">
    <div class="tf-stg-l">Clones root<small>knobas finds clones here and clones new ones into it.</small></div>
    <div class="tf-stg-c tf-stg-inl">
      {#if settings.clones_root !== null}<span class="tf-mono">{settings.clones_root}</span>{:else}<span class="tf-dim">Not set</span>{/if}
      <button class="tf-btn tf-sm" onclick={changeRoot}>Change…</button>
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
    <table class="tf-tbl tf-stg-apps">
      <thead><tr><th>Application</th><th>Opens a checkout</th><th>Opens a file</th><th><span class="tf-sr">Default</span></th></tr></thead>
      <tbody>
        {#each settings.applications as app (app.id)}
          <tr>
            <td><b>{app.name}</b>{#if app.id === settings.default_application}{" "}<TfStateChip text="default" />{/if}</td>
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
      <table class="tf-tbl tf-stg-trees">
        <thead><tr><th>Pull request</th><th>Worktree</th><th><span class="tf-sr">Remove</span></th></tr></thead>
        <tbody>
          {#each worktrees as tree (tree.path)}
            <tr>
              <td>#{tree.number}{#if tree.closed}{" "}<TfStateChip text="closed" />{/if}</td>
              <td class="tf-small"><span class="tf-mono">{tree.path}</span></td>
              <td class="tf-right"
                ><button class="tf-btn tf-sm tf-ghost tf-danger" title="Remove {tree.path}" onclick={() => (removing = tree)}
                  >Remove…</button
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

{#if editingRoot}
  <TfDialog title="Change the clones root" {layers} onclose={() => (editingRoot = false)}>
    {#snippet body()}
      <form
        class="tf-formgrid"
        id="tf-stg-root"
        onsubmit={(event) => {
          event.preventDefault();
          void saveTyped();
        }}
      >
        <label for="tf-stg-root-in">Clones root</label>
        <input id="tf-stg-root-in" aria-label="Clones root" class="tf-mono" type="text" autocomplete="off" spellcheck="false" bind:value={typed} />
      </form>
      <p class="tf-dsub">knobas finds clones here and clones new ones into it. Clones already made stay where they are. Empty means none is set.</p>
      {#if rootError !== null}<div class="tf-banner tf-err tf-dbanner" role="alert">{rootError}</div>{/if}
    {/snippet}
    {#snippet footer()}
      {#if rootUnchanged}<p class="tf-dwhy" role="status">This is the root already.</p>{/if}
      <button type="button" class="tf-btn tf-ghost" onclick={() => (editingRoot = false)}>Cancel</button>
      <button type="submit" class="tf-btn tf-primary" form="tf-stg-root" disabled={rootUnchanged || savingRoot}>Save</button>
    {/snippet}
  </TfDialog>
{/if}

{#if removing !== null}
  {@const tree = removing}
  <TfDialog title="Remove the worktree of #{tree.number}?" {layers} onclose={() => (removing = null)}>
    {#snippet body()}
      <p class="tf-dlg-note">Deletes <span class="tf-mono">{tree.path}</span> from this machine, with any changes in it. The pull request itself is not touched.</p>
    {/snippet}
    {#snippet footer()}
      <button type="button" class="tf-btn tf-ghost" onclick={() => (removing = null)}>Cancel</button>
      <button type="button" class="tf-btn tf-primary tf-danger" onclick={() => void removeWorktree(tree.path)}>Remove worktree</button>
    {/snippet}
  </TfDialog>
{/if}

{#if managing}
  <TfAppsManage {apps} sample={null} {layers} onclose={() => (managing = false)} />
{/if}

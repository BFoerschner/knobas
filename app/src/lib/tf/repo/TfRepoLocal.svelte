<!--
  The repository page's *Local changes* tab (#847, R-8.9), prototype-6's
  `tab==='local'`:

  - **Cloned here**: the checkout line (path, branch, ahead and behind,
    *Open in…*); the line that knobas only reads here; then the uncommitted
    diff (`entity_checkout_diff`, ADR-0026), each file marked "modified, not
    staged" or "modified, staged", drawn by the pull request's diff block
    (`TfDiffView`); untracked files listed by name. Read when the tab opens and
    when the window gets focus.
  - **Not on this machine**: *Clone*, and *It is already cloned somewhere
    else…*, which sets the path by hand (`set_checkout_override`).
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import type { CheckoutView, DiffRead } from "../../ipc/entity";
  import TfDiffView from "../diff/TfDiffView.svelte";
  import { treeStanding } from "../code/code";
  import { repoName } from "../openin/openin";
  import TfOpenIn from "../openin/TfOpenIn.svelte";
  import TfDialog from "../settings/TfDialog.svelte";
  import { localFiles } from "./repo";
  import TfWaiting from "../TfWaiting.svelte";
  import { waitPhase } from "../waiting";

  let {
    repoId,
    title,
    checkout,
    readDiff,
    setPath,
  }: {
    repoId: string;
    /** `owner/name`. */
    title: string;
    /** `undefined` while `entity_checkout` has not answered. */
    checkout: CheckoutView | undefined;
    /** `entity_checkout_diff`. */
    readDiff: (entityId: string) => Promise<DiffRead>;
    /** `set_checkout_override`; answers the fresh view. */
    setPath: (path: string) => Promise<void>;
  } = $props();

  let read = $state<DiffRead | null>(null);
  let failed = $state<string | null>(null);
  let asking = $state(false);
  let typed = $state("");
  let pathError = $state<string | null>(null);

  const path = $derived(checkout?.path ?? null);
  const tree = $derived(checkout?.tree?.status === "read" ? checkout.tree : null);
  /** "3 ahead, 1 changed" without the branch, which the line names on its own. */
  const where = $derived(tree !== null ? treeStanding(tree) : null);
  const files = $derived(localFiles(read));
  const untracked = $derived(read?.status === "read" ? read.untracked : []);
  const name = $derived(repoName(title));

  async function load() {
    if (path === null) return;
    try {
      read = await readDiff(repoId);
      failed = null;
    } catch (cause) {
      failed = ipcErrorMessage(cause);
    }
  }

  $effect(() => {
    if (path === null) return;
    void load();
    const again = () => void load();
    window.addEventListener("focus", again);
    return () => window.removeEventListener("focus", again);
  });

  async function save() {
    const value = typed.trim();
    if (value === "") return;
    try {
      await setPath(value);
      asking = false;
    } catch (cause) {
      pathError = ipcErrorMessage(cause);
    }
  }
</script>

{#if checkout === undefined}
  <!-- Not read yet. -->
{:else if path !== null}
  <div class="tf-checkout tf-local-line">
    <span class="tf-mono">{path}</span>
    {#if tree !== null}<span>on <span class="tf-mono">{tree.branch ?? "a detached head"}</span></span>{/if}
    {#if where !== null}<span>{where}</span>{/if}
    <span class="tf-acts"><TfOpenIn entityId={repoId} {title} {checkout} /></span>
  </div>
  <p class="tf-dim tf-local-note">
    Read with read-only git, when you open this tab and when the window gets focus. knobas never commits, stashes or pushes
    here: do that in your editor or terminal.
  </p>
  <!-- The focus re-read answers in a blink from local git: it swaps the diff in place, undimmed; only the first read waits here (#2246). -->
  <TfWaiting
    phase={waitPhase({ drawn: read !== null, reading: false, failed: failed !== null })}
    error={failed === null ? null : `Could not read the local changes: ${failed}`}
    shape="lines"
  >
  {#if read === null}
    <!-- Not drawn: the skeleton stands in. -->
  {:else if read.status === "no_git"}
    <div class="tf-empty"><p>git was not found on this machine, so knobas cannot read the checkout.</p></div>
  {:else if read.status === "not_a_repo"}
    <div class="tf-empty"><p>{path} is not a git working tree any more.</p></div>
  {:else if read.status === "failed"}
    <p class="tf-cfail">git could not read the checkout: {read.reason}</p>
  {:else if files.length === 0 && untracked.length === 0}
    <div class="tf-empty">Working tree clean.</div>
  {:else}
    <TfDiffView {files} />
    {#if read.unstaged.truncated || read.staged.truncated}
      <p class="tf-dim tf-local-note">The diff is cut here: it is longer than knobas draws. Your editor has all of it.</p>
    {/if}
    {#if untracked.length > 0}
      <div class="tf-panel tf-untracked">
        <header><h3>Untracked</h3></header>
        <ul>
          {#each untracked as file (file)}<li class="tf-mono">{file}</li>{/each}
        </ul>
        {#if read.untracked_truncated}<p class="tf-dim">And more than knobas lists.</p>{/if}
      </div>
    {/if}
  {/if}
  </TfWaiting>
{:else}
  <div class="tf-empty">
    Not on this machine yet.
    <span class="tf-local-acts">
      <TfOpenIn entityId={repoId} {title} {checkout} variant="primary" />
      <button type="button" class="tf-btn tf-sm tf-ghost" onclick={() => ((asking = true), (typed = ""), (pathError = null))}
        >It is already cloned somewhere else…</button
      >
    </span>
  </div>
{/if}

{#if asking}
  <TfDialog title="Where is {name} cloned?" onclose={() => (asking = false)}>
    {#snippet body()}
      <form
        class="tf-formgrid"
        id="tf-set-path"
        onsubmit={(event) => {
          event.preventDefault();
          void save();
        }}
      >
        <label for="tf-spp">Path</label>
        <input id="tf-spp" class="tf-mono" type="text" autocomplete="off" spellcheck="false" placeholder="/Users/you/work/{name}" bind:value={typed} />
      </form>
      <p class="tf-dsub">A path you set wins over the clones-root scan.</p>
      {#if pathError !== null}<div class="tf-banner tf-err tf-dbanner" role="alert">{pathError}</div>{/if}
    {/snippet}
    {#snippet footer()}
      <button type="button" class="tf-btn tf-ghost" onclick={() => (asking = false)}>Cancel</button>
      <button type="submit" class="tf-btn tf-primary" form="tf-set-path" disabled={typed.trim() === ""}>Set path</button>
    {/snippet}
  </TfDialog>
{/if}

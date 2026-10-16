<!--
  *New branch…* (#839, R-8.8), prototype-6's `openNewBranch`: a name
  prefilled `feature/<job key>-`, the branch or commit it starts from, and
  *Create branch*, which queues `CreateBranch` in the repository (#707's op,
  its §10.8 entry). The header opens it from the branch list; a commit opens
  it from that commit (*New branch from here…*), its whole sha the value and
  its short sha the label, as the source's own commit view offers it.

  The branch's id is the source's to mint, so the dialog makes no link and
  opens nothing (#707's ruling): the Branches tab draws it queued.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import type { QueuedWrite, WriteOpPayload } from "../../ipc/sources";
  import { push } from "../../shell/toasts.svelte";
  import { openWriteQueue } from "../../shell/write-queue.svelte";
  import TfDialog from "../settings/TfDialog.svelte";
  import { markMade } from "./queued.svelte";
  import { newBranchName } from "./repo";

  let {
    repoId,
    repoName,
    branches,
    from = null,
    job,
    submitWrite,
    onclose,
  }: {
    repoId: string;
    /** `owner/name`, as the header has it. */
    repoName: string;
    /** Every branch to start from, the default first. */
    branches: readonly string[];
    /** A commit to start from instead, offered first. */
    from?: { value: string; label: string } | null;
    job: string | null;
    submitWrite: (payload: WriteOpPayload) => Promise<QueuedWrite>;
    onclose: () => void;
  } = $props();

  // Prefilled once, when the dialog opens.
  let name = $state(newBranchName((() => job)()));
  let start = $state((() => from?.value ?? branches[0] ?? "")());
  let busy = $state(false);

  const ready = $derived(name.trim() !== "" && !name.trim().endsWith("/") && start !== "" && !busy);

  async function create() {
    if (!ready) return;
    const branch = name.trim();
    busy = true;
    try {
      await submitWrite({ CreateBranch: { entity: repoId, name: branch, from_ref: start } });
      markMade(repoId, branch);
      push({
        text: `Queued: create branch ${branch} in ${repoName}. Sends with the next sync.`,
        action: { label: "Show queue", run: openWriteQueue },
      });
      onclose();
    } catch (cause) {
      push({ text: `Could not queue the branch: ${ipcErrorMessage(cause)}`, tone: "err" });
      busy = false;
    }
  }
</script>

<TfDialog title="New branch in {repoName}" {onclose}>
  {#snippet body()}
    <form
      class="tf-formgrid"
      id="tf-newbranch"
      onsubmit={(event) => {
        event.preventDefault();
        void create();
      }}
    >
      <label for="tf-nb-name">Name</label>
      <input id="tf-nb-name" class="tf-mono" type="text" autocomplete="off" spellcheck="false" bind:value={name} />
      <label for="tf-nb-from">From</label>
      <select id="tf-nb-from" bind:value={start}>
        {#if from !== null}<option value={from.value}>{from.label}</option>{/if}
        {#each branches as branch (branch)}<option value={branch}>{branch}</option>{/each}
      </select>
    </form>
  {/snippet}
  {#snippet footer()}
    <button type="button" class="tf-btn tf-ghost" onclick={onclose}>Cancel</button>
    <button type="submit" class="tf-btn tf-primary" form="tf-newbranch" disabled={!ready}>Create branch</button>
  {/snippet}
</TfDialog>

<!--
  *New branch…* (#839, R-8.8), prototype-6's `openNewBranch`: a name
  prefilled `feature/<job key>-`, the branch or commit it starts from, and
  *Create branch*, which queues `CreateBranch` in the repository (#707's op,
  its §10.8 entry). The header opens it from the branch list; a commit opens
  it from that commit (*New branch from here…*), its whole sha the value and
  its short sha the label, as the source's own commit view offers it.

  The branch's id is the source's to mint (#707's ruling), so the dialog opens
  nothing: the Branches tab draws it queued. With a job, *Link to <job>*
  (#984), ticked, asks `submit_write` to link the branch to the job's ticket
  once the write has gone and a sync has brought the branch in.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import type { ChoiceOption } from "../../ipc/entity";
  import { toastQueued, type WriteSource } from "../../shell/queued-toast";
  import { push } from "../../shell/toasts.svelte";
  import { resolveTicket } from "../job/job";
  import TfDialog from "../settings/TfDialog.svelte";
  import { echoWrite } from "../queued.svelte";
  import type { RepoPorts } from "./TfRepo.svelte";
  import { MADE, REPO_BRANCHES, newBranchName } from "./repo";

  let {
    repoId,
    repoName,
    branches,
    from = null,
    job,
    ports,
    source = null,
    onclose,
  }: {
    repoId: string;
    /** `owner/name`, as the header has it. */
    repoName: string;
    /** Every branch to start from, the default first. */
    branches: readonly string[];
    /** A commit to start from instead, offered first. */
    from?: ChoiceOption | null;
    job: string | null;
    ports: Pick<RepoPorts, "submitWrite" | "listSources" | "getEntity">;
    /** The repository's source: where the write goes, named in its toast when its sign-in fails. */
    source?: WriteSource | null;
    onclose: () => void;
  } = $props();

  // Prefilled once, when the dialog opens.
  let name = $state(newBranchName((() => job)()));
  let start = $state((() => from?.value ?? branches[0] ?? "")());
  let link = $state(true);
  let busy = $state(false);

  const ready = $derived(name.trim() !== "" && !name.trim().endsWith("/") && start !== "" && !busy);

  /** The ticket the job's key names in the mirror, the link's other end. */
  async function jobTicket(key: string): Promise<string | null> {
    const found = await resolveTicket(key, await ports.listSources(), ports.getEntity);
    return found?.row.entity_id ?? null;
  }

  async function create() {
    if (!ready) return;
    const branch = name.trim();
    busy = true;
    try {
      const linking = job !== null && link ? job : null;
      const ticket = linking === null ? null : await jobTicket(linking);
      if (linking !== null && ticket === null) {
        push({ text: `${job} is not in the mirror, so the branch cannot be linked to it`, tone: "err" });
        busy = false;
        return;
      }
      const write = await ports.submitWrite({ CreateBranch: { entity: repoId, name: branch, from_ref: start } }, ticket ?? undefined);
      echoWrite(repoId, MADE, branch, { write, many: true, surface: REPO_BRANCHES, ...(source ? { source: source.id } : {}) });
      toastQueued(`create branch ${branch} in ${repoName}`, source);
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
      {#if job !== null}
        <span>Link to</span>
        <label class="tf-ck"><input type="checkbox" bind:checked={link} /> {job} <span class="tf-dim">(in knobas)</span></label>
      {/if}
    </form>
  {/snippet}
  {#snippet footer()}
    <button type="button" class="tf-btn tf-ghost" onclick={onclose}>Cancel</button>
    <button type="submit" class="tf-btn tf-primary" form="tf-newbranch" disabled={!ready}>Create branch</button>
  {/snippet}
</TfDialog>

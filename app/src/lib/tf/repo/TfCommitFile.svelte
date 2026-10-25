<!--
  *Commit change…* (#845, R-8.6), prototype-6's `openCommitEdit`: the edit's
  diff, a message prefilled with the job's key, and where it goes: *Commit to
  <branch>* or *Commit to a new branch and open a pull request* with the new
  branch's name. It warns when the local checkout is on the branch the commit
  goes to, which this commit leaves one behind. Where the source declares the
  file's commit disabled (#985: a protected branch), *Commit to <branch>* is
  disabled with the source's reason under it, and the new branch is picked.

  *Commit change* queues `CommitFile` (#822) in the repository against the
  blob the file was read at (#883), so a file that moved since is held with
  both versions in the write queue (*Send mine anyway*, *Edit…*, *Discard*).
  The checkout is never written (ADR-0016, ADR-0021). Onto the branch, the
  page goes back to the file, which says the commit is queued; onto a new
  branch, it goes on to that branch's compare page.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import { toastQueued, type WriteSource } from "../../shell/queued-toast";
  import { push } from "../../shell/toasts.svelte";
  import { lineDiff } from "../code/line-diff";
  import TfDiff from "../code/TfDiff.svelte";
  import TfDialog from "../settings/TfDialog.svelte";
  import { echoWrite } from "../queued.svelte";
  import type { RepoPorts } from "./TfRepo.svelte";
  import { MADE, REPO_BRANCHES, REPO_FILES, commitKind, commitPrefill, compareHash, fileHash } from "./repo";

  let {
    repoId,
    branch,
    path,
    before,
    after,
    baseSha,
    refusal = null,
    job,
    checkoutBranch,
    ports,
    source = null,
    onclose,
  }: {
    repoId: string;
    /** The branch the file was read on. */
    branch: string;
    path: string;
    /** The file as read, and as edited. */
    before: string;
    after: string;
    /** The blob the file was read at: the version the commit is made against. */
    baseSha: string;
    /** Why the source refuses a commit straight onto `branch` (#985), `null` where it takes one. */
    refusal?: string | null;
    job: string | null;
    /** The branch the local checkout is on, `null` without one. */
    checkoutBranch: string | null;
    ports: Pick<RepoPorts, "submitWrite">;
    /** The repository's source: where the write goes, named in its toast when its sign-in fails. */
    source?: WriteSource | null;
    onclose: () => void;
  } = $props();

  // Prefilled once, when the dialog opens.
  const prefill = commitPrefill((() => job)(), (() => path)());
  let message = $state(prefill.message);
  // A branch the source refuses the commit on opens on the new branch, as
  // prototype-6's `openCommitEdit` does for `main`.
  let target = $state<"same" | "new">((() => (refusal === null ? "same" : "new"))());
  let newBranch = $state(prefill.branch);
  let busy = $state(false);

  const diff = $derived(lineDiff(path, before, after));
  const ready = $derived(
    message.trim() !== "" &&
      (target === "same" ? refusal === null : newBranch.trim() !== "" && newBranch.trim() !== branch) &&
      !busy,
  );

  async function commit() {
    if (!ready) return;
    busy = true;
    const onto = target === "new" ? newBranch.trim() : null;
    try {
      const write = await ports.submitWrite({
        CommitFile: { repo: repoId, branch, new_branch: onto, path, content: after, message: message.trim(), base_sha: baseSha },
      });
      toastQueued(onto === null ? `commit ${path} to ${branch}` : `commit ${path} to ${onto}, then open a PR`, source);
      onclose();
      if (onto === null) {
        echoWrite(repoId, commitKind(branch, path), message.trim(), { write, base: baseSha, surface: REPO_FILES, ...(source ? { source: source.id } : {}) });
        location.hash = fileHash(repoId, path);
      } else {
        echoWrite(repoId, MADE, onto, { write, many: true, surface: REPO_BRANCHES, ...(source ? { source: source.id } : {}) });
        location.hash = compareHash(repoId, onto);
      }
    } catch (cause) {
      push({ text: `Could not queue the commit: ${ipcErrorMessage(cause)}`, tone: "err" });
      busy = false;
    }
  }
</script>

<TfDialog title="Commit {path}" {onclose} className="tf-cf-dialog">
  {#snippet body()}
    <TfDiff files={[diff]} />
    <div class="tf-formgrid">
      <label for="tf-cf-msg">Message</label>
      <input id="tf-cf-msg" type="text" autocomplete="off" bind:value={message} />
    </div>
    <div class="tf-radio" role="radiogroup" aria-label="Where the commit goes">
      <label>
        <input
          type="radio"
          name="tf-cf-target"
          value="same"
          bind:group={target}
          disabled={refusal !== null}
          aria-describedby={refusal === null ? undefined : "tf-cf-refusal"}
        />
        <span
          >Commit to <span class="tf-mono">{branch}</span>{#if refusal !== null}<small id="tf-cf-refusal"
              >{refusal}</small
            >{/if}</span
        >
      </label>
      <label>
        <input type="radio" name="tf-cf-target" value="new" bind:group={target} />
        <span
          >Commit to a new branch and open a pull request<small
            ><input
              id="tf-cf-branch"
              class="tf-mono"
              type="text"
              aria-label="New branch"
              autocomplete="off"
              spellcheck="false"
              bind:value={newBranch}
              onfocus={() => (target = "new")}
            /></small
          ></span
        >
      </label>
    </div>
    {#if target === "same" && checkoutBranch === branch}
      <div class="tf-banner tf-warn tf-dbanner">
        Your checkout is on this branch. After this commit it is 1 behind; pull in your terminal before you push.
      </div>
    {/if}
    <p class="tf-dim tf-note">It waits in the queue like every other write.</p>
  {/snippet}
  {#snippet footer()}
    <button type="button" class="tf-btn tf-ghost" onclick={onclose}>Keep editing</button>
    <button type="button" class="tf-btn tf-primary" disabled={!ready} onclick={() => void commit()}>Commit change</button>
  {/snippet}
</TfDialog>

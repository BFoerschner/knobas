<!--
  *Commit change…* (#845, R-8.6), prototype-6's `openCommitEdit`: the edit's
  diff, a message prefilled with the job's key, and where it goes: *Commit to
  <branch>* or *Commit to a new branch and open a pull request* with the new
  branch's name. It warns when the local checkout is on the branch the commit
  goes to, which this commit leaves one behind.

  *Commit change* queues `CommitFile` (#822) in the repository against the
  blob the file was read at (#883), so a file that moved since is held with
  both versions in the write queue (*Send mine anyway*, *Edit…*, *Discard*).
  The checkout is never written (ADR-0016, ADR-0021). Onto the branch, the
  page goes back to the file, which says the commit is queued; onto a new
  branch, it goes on to that branch's compare page.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import type { QueuedWrite, WriteOpPayload } from "../../ipc/sources";
  import { push } from "../../shell/toasts.svelte";
  import { openWriteQueue } from "../../shell/write-queue.svelte";
  import { lineDiff } from "../code/line-diff";
  import TfDiff from "../code/TfDiff.svelte";
  import TfDialog from "../settings/TfDialog.svelte";
  import { markCommit, markMade } from "./queued.svelte";
  import { commitPrefill, compareHash, fileHash } from "./repo";

  let {
    repoId,
    branch,
    path,
    before,
    after,
    baseSha,
    job,
    checkoutBranch,
    submitWrite,
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
    job: string | null;
    /** The branch the local checkout is on, `null` without one. */
    checkoutBranch: string | null;
    submitWrite: (payload: WriteOpPayload) => Promise<QueuedWrite>;
    onclose: () => void;
  } = $props();

  // Prefilled once, when the dialog opens.
  const prefill = commitPrefill((() => job)(), (() => path)());
  let message = $state(prefill.message);
  let target = $state<"same" | "new">("same");
  let newBranch = $state(prefill.branch);
  let busy = $state(false);

  const diff = $derived(lineDiff(path, before, after));
  const ready = $derived(
    message.trim() !== "" && (target === "same" || (newBranch.trim() !== "" && newBranch.trim() !== branch)) && !busy,
  );

  async function commit() {
    if (!ready) return;
    busy = true;
    const onto = target === "new" ? newBranch.trim() : null;
    try {
      await submitWrite({
        CommitFile: { repo: repoId, branch, new_branch: onto, path, content: after, message: message.trim(), base_sha: baseSha },
      });
      push({
        text:
          onto === null
            ? `Queued: commit ${path} to ${branch}. Sends with the next sync.`
            : `Queued: commit ${path} to ${onto}, then open a PR. Sends with the next sync.`,
        action: { label: "Show queue", run: openWriteQueue },
      });
      onclose();
      if (onto === null) {
        markCommit(repoId, branch, path, { message: message.trim(), base: baseSha });
        location.hash = fileHash(repoId, path);
      } else {
        markMade(repoId, onto);
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
        <input type="radio" name="tf-cf-target" value="same" bind:group={target} />
        <span>Commit to <span class="tf-mono">{branch}</span></span>
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

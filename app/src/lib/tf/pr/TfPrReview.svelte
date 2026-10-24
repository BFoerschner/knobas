<!--
  *Submit review* (#842, R-8.18), prototype-6's `openReview`: a summary, the
  verdict (*Comment*, *Approve*, *Request changes*, each with its one-line
  meaning) and the pending line comments that go with it, queued as one
  `SubmitReview` through the source's own action for the verdict
  (`detail/view/actions.ts` builds it).

  Only the verdicts the source offers are drawn (`reviewChoices`): one's own
  pull request takes no approval, and the comment-only review is the
  source's only with line comments to send. A verdict whose words the source
  requires (*Request changes*) is not sent without them. Once queued, the
  comments it took are settled: one edited meanwhile stays pending.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import type { ViewAction } from "../../ipc/entity";
  import type { QueuedWrite, WriteOpPayload } from "../../ipc/sources";
  import { actionPayload, missingFields } from "../../detail/view/actions";
  import { asReview, lineName, type PendingCommentStore } from "../../detail/view/pending-comments.svelte";
  import { toastQueued, type WriteSource } from "../../shell/queued-toast";
  import { push } from "../../shell/toasts.svelte";
  import TfDialog from "../settings/TfDialog.svelte";
  import { reviewChoices, type Verdict } from "./pr";

  let {
    actions,
    prId,
    name,
    base,
    pending,
    source = null,
    submitWrite,
    onqueued,
    onclose,
  }: {
    actions: readonly ViewAction[];
    prId: string;
    /** `PR #142`. */
    name: string;
    base: string | null;
    pending: PendingCommentStore;
    /** The pull request's source: where the review goes, named in its toast when its sign-in fails. */
    source?: WriteSource | null;
    submitWrite: (payload: WriteOpPayload) => Promise<QueuedWrite>;
    onqueued: (write: QueuedWrite) => void;
    onclose: () => void;
  } = $props();

  const uid = $props.id();
  const drafts = $derived(pending.of(prId));
  const choices = $derived(reviewChoices(actions, drafts.length, base));
  // The first sendable verdict, once, when the dialog opens: prototype-6 checks *Comment*.
  let verdict = $state<Verdict | null>(
    (() => reviewChoices(actions, pending.of(prId).length, base).find((choice) => choice.action !== null)?.verdict ?? null)(),
  );
  let summary = $state("");
  let busy = $state(false);

  const chosen = $derived(choices.find((choice) => choice.verdict === verdict) ?? null);
  const missing = $derived(chosen?.action ? missingFields(chosen.action, { body: summary }) : ["verdict"]);

  async function send() {
    const action = chosen?.action;
    if (busy || !action || missing.length > 0) return;
    const sent = [...drafts];
    const payload = actionPayload(action, prId, { body: summary.trim() }, [], asReview(sent));
    if (payload === null) return;
    busy = true;
    try {
      const write = await submitWrite(payload);
      pending.settle(prId, sent);
      toastQueued(`submit review on ${name}`, source);
      onqueued(write);
      onclose();
    } catch (cause) {
      push({ text: `Could not queue the review: ${ipcErrorMessage(cause)}`, tone: "err" });
      busy = false;
    }
  }
</script>

<TfDialog title="Submit your review of {name}" {onclose} className="tf-pr-dialog">
  {#snippet body()}
    <form
      id="tf-prr-{uid}"
      class="tf-pra"
      onsubmit={(event) => {
        event.preventDefault();
        void send();
      }}
    >
      <textarea
        rows="3"
        bind:value={summary}
        aria-label="Summary"
        placeholder={chosen?.action?.fields.some((field) => field.name === "body" && field.required) ? "Summary" : "Summary (optional)"}
      ></textarea>
      <fieldset class="tf-radio">
        <legend class="tf-sr">Verdict</legend>
        {#each choices as choice (choice.verdict)}
          <label title={choice.why ?? undefined}
            ><input type="radio" name="tf-prr-{uid}-verdict" value={choice.verdict} disabled={choice.action === null} bind:group={verdict} />
            <span>{choice.label}<small>{choice.meaning}</small></span></label
          >
        {/each}
      </fieldset>
      {#if drafts.length > 0}
        <div class="tf-pane-h">{drafts.length} line {drafts.length === 1 ? "comment goes" : "comments go"} with it</div>
        {#each drafts as d (d.id)}
          <div class="tf-dim tf-prr-line"><span class="tf-mono">{d.path}</span>, {lineName(d.side, d.line)}: {d.body}</div>
        {/each}
      {/if}
    </form>
  {/snippet}
  {#snippet footer()}
    <button type="button" class="tf-btn tf-ghost" onclick={onclose}>Cancel</button>
    <button type="submit" class="tf-btn tf-primary" form="tf-prr-{uid}" disabled={busy || missing.length > 0}>Submit review</button>
  {/snippet}
</TfDialog>

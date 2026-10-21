<!--
  *Merge…* (#843, R-8.19), prototype-6's `openMerge`, over the source's own
  *Merge…* action (Gitea's `land.rs`, the one place mergeability is
  decided):

  - while nothing stands in the way, *Ready to merge?*'s lines with their
    dots; else a warn box naming each reason the source gives, and *Merge*
    disabled with the source's sentence as its tooltip;
  - the style (merge commit, rebase, squash), from the action's `style`
    choice; *Squash* opens its commit message, prefilled by the source;
  - *Delete <branch> after merging*, ticked where the source ticks it;
  - where knobas holds a link to a ticket, the line that the merge follower
    (`follow_merges`) moves it to In Review once the merge lands.

  Queued as one `MergePullRequest` (`detail/view/actions.ts`).
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import type { ViewAction } from "../../ipc/entity";
  import type { QueuedWrite, WriteOpPayload } from "../../ipc/sources";
  import { actionPayload, initialValues } from "../../detail/view/actions";
  import { toastQueued, type WriteSource } from "../../shell/queued-toast";
  import { push } from "../../shell/toasts.svelte";
  import TfDialog from "../settings/TfDialog.svelte";
  import { mergeReasons, type ReadyLine } from "./pr";

  let {
    action,
    prId,
    name,
    head,
    base,
    source,
    ready,
    ticket,
    submitWrite,
    onqueued,
    onclose,
  }: {
    action: ViewAction;
    prId: string;
    /** `PR #142`. */
    name: string;
    head: string | null;
    base: string | null;
    /**
     * The pull request's source: its name is what the warn box says blocks the
     * merge, and the merge's toast names it when its sign-in fails.
     */
    source: WriteSource | null;
    /** *Ready to merge?*'s lines. */
    ready: readonly ReadyLine[];
    /** The key of the ticket knobas links the pull request to, or `null`. */
    ticket: string | null;
    submitWrite: (payload: WriteOpPayload) => Promise<QueuedWrite>;
    onqueued: () => void;
    onclose: () => void;
  } = $props();

  const uid = $props.id();
  // The answers start as the source declared them, once, when the dialog opens.
  let values = $state(initialValues((() => action)()));
  let busy = $state(false);

  const reasons = $derived(mergeReasons(action));
  const style = $derived(action.fields.find((field) => field.name === "style"));
  const options = $derived(style?.kind.type === "choice" ? style.kind.options : []);
  const hasDelete = $derived(action.fields.some((field) => field.name === "delete_branch"));
  const hasMessage = $derived(action.fields.some((field) => field.name === "message"));
  const into = $derived(base ?? "its base");

  /** Prototype-6's words for the SPI's three styles; another is the source's own label. */
  function styleWords(value: string, label: string): string {
    if (value === "merge") return "Create a merge commit";
    if (value === "rebase") return `Rebase onto ${into}`;
    if (value === "squash") return "Squash into one commit";
    return label;
  }

  async function send() {
    if (busy || action.disabled !== null) return;
    const payload = actionPayload(action, prId, values);
    if (payload === null) return;
    busy = true;
    try {
      await submitWrite(payload);
      toastQueued(`merge ${name}`, source);
      onqueued();
      onclose();
    } catch (cause) {
      push({ text: `Could not queue the merge: ${ipcErrorMessage(cause)}`, tone: "err" });
      busy = false;
    }
  }
</script>

<TfDialog title="Merge {name} into {into}" {onclose} className="tf-pr-dialog">
  {#snippet body()}
    <form
      id="tf-prm-{uid}"
      class="tf-pra"
      onsubmit={(event) => {
        event.preventDefault();
        void send();
      }}
    >
      {#if action.disabled === null}
        <div class="tf-prready tf-prm-ready">
          {#each ready as line, n (n)}
            <span><span class="tf-dot tf-{line.tone}"></span> {line.text}</span>
          {/each}
        </div>
      {:else}
        <div class="tf-banner tf-warn tf-dbanner tf-prm-blocked" role="note">
          {source?.display_name ?? "The source"} blocks this merge:
          {#each reasons as reason, n (n)}<span>{reason}</span>{/each}
        </div>
      {/if}
      <fieldset class="tf-radio">
        <legend class="tf-sr">Style</legend>
        {#each options as option (option.value)}
          <label
            ><input type="radio" name="tf-prm-{uid}-style" value={option.value} bind:group={values["style"]} />
            <span
              >{styleWords(option.value, option.label)}{#if option.value === "squash" && hasMessage && values["style"] === "squash"}<small
                  ><input type="text" aria-label="Squash commit message" bind:value={values["message"]} /></small
                >{/if}</span
            ></label
          >
        {/each}
      </fieldset>
      {#if hasDelete}
        <label class="tf-ck"
          ><input
            type="checkbox"
            checked={values["delete_branch"] === "true"}
            onchange={(event) => (values["delete_branch"] = event.currentTarget.checked ? "true" : "false")}
          />
          Delete <span class="tf-mono">{head ?? "the branch"}</span> after merging</label
        >
      {/if}
      {#if ticket !== null}
        <p class="tf-dim tf-prm-note">After the merge, {ticket} moves to In Review (the merge follower already does this).</p>
      {/if}
    </form>
  {/snippet}
  {#snippet footer()}
    <button type="button" class="tf-btn tf-ghost" onclick={onclose}>Cancel</button>
    <button
      type="submit"
      class="tf-btn tf-primary"
      form="tf-prm-{uid}"
      aria-disabled={action.disabled !== null ? "true" : undefined}
      title={action.disabled ?? undefined}
      disabled={busy}>Merge</button
    >
  {/snippet}
</TfDialog>

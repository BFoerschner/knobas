<!--
  One of the pull request's toolbar writes as its source declares it (#841,
  R-8.15): *Update branch…*, *Edit…*, *Close* or *Reopen*. The source names
  the fields, the sentence and the tone (ADR-0020 rule 4); this draws them in
  the new shell's dialog and queues the write `detail/view/actions.ts` builds
  from the answers, against the pull request.

  A text is an input, a long text a text area, a choice radio buttons, a flag
  a checkbox. *Edit…* sends only what changed (#706): sent back as drawn, it
  closes without a write.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../../ipc";
  import type { ViewAction } from "../../ipc/entity";
  import type { QueuedWrite, WriteOpPayload } from "../../ipc/sources";
  import { push } from "../../shell/toasts.svelte";
  import { actionName, actionPayload, answeredAsDrawn, initialValues, missingFields } from "../../detail/view/actions";
  import { toastQueued } from "../repo/queued.svelte";
  import TfDialog from "../settings/TfDialog.svelte";

  let {
    action,
    prId,
    title,
    what,
    submitWrite,
    onqueued,
    onclose,
  }: {
    action: ViewAction;
    /** The pull request the view is of: the write's target unless the action names its own. */
    prId: string;
    /** The dialog's heading. */
    title: string;
    /** What the toast says was queued (`close PR #142`). */
    what: string;
    submitWrite: (payload: WriteOpPayload) => Promise<QueuedWrite>;
    /** Called once the write is queued. */
    onqueued: () => void;
    onclose: () => void;
  } = $props();

  const uid = $props.id();
  // The answers start as the source declared them, once, when the dialog opens.
  let values = $state(initialValues((() => action)()));
  let busy = $state(false);

  const missing = $derived(missingFields(action, values));

  async function send() {
    if (busy || missing.length > 0) return;
    if (answeredAsDrawn(action, values)) {
      onclose();
      return;
    }
    const payload = actionPayload(action, prId, values);
    if (payload === null) {
      push({ text: `Could not queue ${actionName(action.label).toLowerCase()}: the answers do not make a write.`, tone: "err" });
      return;
    }
    busy = true;
    try {
      await submitWrite(payload);
      toastQueued(what);
      onqueued();
      onclose();
    } catch (cause) {
      push({ text: `Could not queue the write: ${ipcErrorMessage(cause)}`, tone: "err" });
      busy = false;
    }
  }
</script>

<TfDialog {title} {onclose} className="tf-pr-dialog">
  {#snippet body()}
    <form
      id="tf-pra-{uid}"
      class="tf-pra"
      onsubmit={(event) => {
        event.preventDefault();
        void send();
      }}
    >
      {#if action.confirm !== null}<p>{action.confirm}</p>{/if}
      {#each action.fields as field (field.name)}
        {@const id = `tf-pra-${uid}-${field.name}`}
        {#if field.kind.type === "text"}
          <label for={id}>{field.label}</label>
          <input {id} type="text" aria-label={field.label} autocomplete="off" placeholder={field.placeholder ?? undefined} bind:value={values[field.name]} />
        {:else if field.kind.type === "long_text"}
          <label for={id}>{field.label}</label>
          <textarea {id} rows="6" aria-label={field.label} placeholder={field.placeholder ?? undefined} bind:value={values[field.name]}></textarea>
        {:else if field.kind.type === "choice"}
          <fieldset class="tf-radio">
            <legend class="tf-sr">{field.label}</legend>
            {#each field.kind.options as option (option.value)}
              <label><input type="radio" name={id} value={option.value} bind:group={values[field.name]} /> <span>{option.label}</span></label>
            {/each}
          </fieldset>
        {:else}
          <label class="tf-ck"
            ><input
              type="checkbox"
              checked={values[field.name] === "true"}
              onchange={(event) => (values[field.name] = (event.currentTarget as HTMLInputElement).checked ? "true" : "false")}
            />
            {field.label}</label
          >
        {/if}
      {/each}
    </form>
  {/snippet}
  {#snippet footer()}
    <button type="button" class="tf-btn tf-ghost" onclick={onclose}>Cancel</button>
    <button
      type="submit"
      class="tf-btn {action.danger ? 'tf-danger' : 'tf-primary'}"
      form="tf-pra-{uid}"
      aria-label={actionName(action.label)}
      disabled={busy || missing.length > 0}>{actionName(action.label)}</button
    >
  {/snippet}
</TfDialog>

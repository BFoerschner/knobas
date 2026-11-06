<!--
  One write a source declares on an entity's view (ADR-0020 rule 4), in the
  new shell's dialog: the pull request's toolbar writes (#841, R-8.15:
  *Update branch…*, *Edit…*, *Close*, *Reopen*, a source action) and an
  item's source actions (#1407, a custom role's writes, ADR-0029). The source
  names the fields, the sentence and the tone; this draws them and queues the
  write `detail/view/actions.ts` builds from the answers, against the entity.

  A text is an input, a long text a text area, a choice radio buttons, a flag
  a checkbox. *Edit…* sends only what changed (#706): sent back as drawn, it
  closes without a write. Lifted from the pull request page (#1407), its
  class (`tf-pr-dialog`) kept with it.

  Typed words outlive the dialog (the key table, `drafts`, F-238): `Esc` or × closes it and
  keeps them for the next time it opens on this entity. *Cancel* throws them away. A choice
  is not kept, because a pick stays a choice made fresh each time.
-->
<script lang="ts">
  import { ipcErrorMessage } from "../ipc";
  import type { ViewAction } from "../ipc/entity";
  import type { QueuedWrite, WriteOpPayload } from "../ipc/sources";
  import { toastQueued, type WriteSource } from "../shell/queued-toast";
  import { push } from "../shell/toasts.svelte";
  import { actionName, actionPayload, answeredAsDrawn, initialValues, missingFields } from "../detail/view/actions";
  import { drafts } from "./drafts.svelte";
  import TfDialog from "./settings/TfDialog.svelte";

  let {
    action,
    entityId,
    title,
    what,
    primary = null,
    source = null,
    submitWrite,
    onqueued,
    onclose,
  }: {
    action: ViewAction;
    /** The entity the view is of: the write's target unless the action names its own. */
    entityId: string;
    /** The dialog's heading. */
    title: string;
    /** What the toast says was queued (`close PR #142`, `book repair on Quay crane 4 inspection`), or how to say it from what was chosen. */
    what: string | ((chose: string) => string);
    /** The primary's words where the action's own name is not what happens (*Save changes*); the name when omitted. */
    primary?: string | null;
    /** The entity's source: where the write goes, named in its toast when its sign-in fails. */
    source?: WriteSource | null;
    submitWrite: (payload: WriteOpPayload) => Promise<QueuedWrite>;
    /** Called once the write is queued, with the words of what was chosen, for a page that marks it (the pull request's chip). */
    onqueued?: (write: QueuedWrite, chose: string) => void;
    onclose: () => void;
  } = $props();

  const uid = $props.id();
  /** The fields typed into: what a draft holds. */
  const TYPED = ["text", "long_text"];
  // svelte-ignore state_referenced_locally
  const draftKey = `action:${entityId}:${action.write_op}:${action.label}`;
  // svelte-ignore state_referenced_locally
  const asDrawn = initialValues(action);
  // The answers start as the source declared them, once, when the dialog opens, with any words kept from the last time.
  let values = $state<Record<string, string>>({ ...asDrawn, ...(drafts.has(draftKey) ? (JSON.parse(drafts.get(draftKey)) as Record<string, string>) : {}) });
  let busy = $state(false);

  /** What is typed differently from what the source drew is the draft. */
  function keepWords() {
    const typed = Object.fromEntries(action.fields.filter((field) => TYPED.includes(field.kind.type) && values[field.name] !== asDrawn[field.name]).map((field) => [field.name, values[field.name] ?? ""]));
    if (Object.keys(typed).length === 0) drafts.forget(draftKey);
    else drafts.put(draftKey, JSON.stringify(typed));
  }

  /** *Cancel*: the explicit discard. */
  function cancel() {
    drafts.forget(draftKey);
    onclose();
  }

  /** The words of what was chosen in the choices, for a toast and a chip to name it. */
  const chose = $derived(
    action.fields
      .flatMap((field) => (field.kind.type === "choice" ? field.kind.options.filter((option) => option.value === values[field.name]).map((option) => option.label) : []))
      .join(", "),
  );

  const missing = $derived(missingFields(action, values));
  /** What the primary waits for, by the labels the reader sees. */
  const stillNeeded = $derived.by(() => {
    if (missing.length === 0) return null;
    const fields = action.fields.filter((field) => missing.includes(field.name));
    // A list to pick from is in front of the reader, so the line needs no name for it.
    if (fields.length === 1 && fields[0]!.kind.type === "choice") return "Pick one first.";
    return `Still needs ${fields.map((field) => field.label).join(" and ")}.`;
  });

  async function send() {
    if (busy || missing.length > 0) return;
    if (answeredAsDrawn(action, values)) {
      cancel();
      return;
    }
    const payload = actionPayload(action, entityId, values);
    if (payload === null) {
      push({ text: `Could not queue ${actionName(action.label).toLowerCase()}: the answers do not make a write.`, tone: "err" });
      return;
    }
    busy = true;
    try {
      const write = await submitWrite(payload);
      toastQueued(typeof what === "function" ? what(chose) : what, source, { write });
      onqueued?.(write, chose);
      cancel();
    } catch (cause) {
      push({ text: `Could not queue the write: ${ipcErrorMessage(cause)}`, tone: "err" });
      busy = false;
    }
  }
</script>

<TfDialog why={busy ? null : stillNeeded} {title} {onclose} className="tf-pr-dialog">
  {#snippet body()}
    <form
      id="tf-pra-{uid}"
      class="tf-formgrid"
      oninput={keepWords}
      onsubmit={(event) => {
        event.preventDefault();
        void send();
      }}
    >
      {#if action.confirm !== null}<p>{action.confirm}</p>{/if}
      {#each action.fields as field (field.name)}
        {@const id = `tf-pra-${uid}-${field.name}`}
        {#if field.kind.type === "text"}
          <label for={id}>{field.label}{#if field.required}{" "}<small class="tf-req">required</small>{/if}</label>
          <input {id} type="text" aria-label={field.label} autocomplete="off" placeholder={field.placeholder ?? undefined} bind:value={values[field.name]} />
        {:else if field.kind.type === "long_text"}
          <label for={id}>{field.label}{#if field.required}{" "}<small class="tf-req">required</small>{/if}</label>
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
    <button type="button" class="tf-btn tf-ghost" onclick={cancel}>Cancel</button>
    <button
      type="submit"
      class="tf-btn tf-primary" class:tf-danger={action.danger}
      form="tf-pra-{uid}"
      disabled={busy || missing.length > 0}>{primary ?? actionName(action.label)}</button
    >
  {/snippet}
</TfDialog>

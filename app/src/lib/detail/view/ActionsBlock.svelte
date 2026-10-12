<!--
  An actions block (#667, ADR-0020 rule 4): the writes a reader can start from
  this view, one button each.

  Generic on purpose. It knows a label, a list of choices and a sentence, and
  never which op it draws: `SourceView` has already dropped any action whose op
  the source does not declare, and `onaction` (the detail) turns the choice into
  a write and queues it. A view never writes.

  An action with choices or a sentence opens a dialog; the first choice is
  picked. One with neither queues on the button. The dialog stays open when
  the write could not be queued, so the reader can try again or cancel.
-->
<script lang="ts">
  import type { ViewAction } from "../../ipc/entity";
  import Modal from "../../shell/Modal.svelte";
  import { actionName } from "./actions";

  let {
    actions,
    onaction,
  }: {
    actions: ViewAction[];
    /** Queue `action` with `option` picked; answers whether it was queued. */
    onaction: (action: ViewAction, option: string | null) => Promise<boolean>;
  } = $props();

  /** The action whose dialog is up, if any. */
  let open = $state<ViewAction | null>(null);
  /** The choice picked in that dialog. */
  let picked = $state<string | null>(null);
  /** True while a write is being queued, so nothing double-fires. */
  let sending = $state(false);

  function start(action: ViewAction) {
    if (action.options.length === 0 && action.confirm === null) {
      void send(action, null);
      return;
    }
    open = action;
    picked = action.options[0] ?? null;
  }

  async function send(action: ViewAction, option: string | null) {
    if (sending) return;
    sending = true;
    try {
      if (await onaction(action, option)) open = null;
    } finally {
      sending = false;
    }
  }

  /** Left and right move the pick, as a radio group does. */
  function key(event: KeyboardEvent, options: string[], index: number) {
    const to =
      event.key === "ArrowRight" || event.key === "ArrowDown"
        ? (index + 1) % options.length
        : event.key === "ArrowLeft" || event.key === "ArrowUp"
          ? (index - 1 + options.length) % options.length
          : null;
    if (to === null) return;
    event.preventDefault();
    picked = options[to] ?? picked;
    const group = (event.currentTarget as HTMLElement).parentElement;
    group?.querySelectorAll<HTMLButtonElement>("[role=radio]")[to]?.focus();
  }
</script>

<div class="ab">
  {#each actions as action, index (index)}
    <button class="btn sm" disabled={sending} onclick={() => start(action)}>{action.label}</button>
  {/each}
</div>

{#if open}
  {@const action = open}
  {@const name = actionName(action.label)}
  <Modal title={name} center onclose={() => (open = null)}>
    {#snippet body()}
      {#if action.confirm}
        <p class="ab-q">{action.confirm}</p>
      {/if}
      {#if action.options.length > 0}
        <div class="seg ab-opts" role="radiogroup" aria-label="{name}: choose one">
          {#each action.options as option, index (option)}
            <button
              class="seg-b"
              class:on={option === picked}
              role="radio"
              aria-checked={option === picked}
              tabindex={option === picked ? 0 : -1}
              onclick={() => (picked = option)}
              onkeydown={(event) => key(event, action.options, index)}
            >
              {option}
            </button>
          {/each}
        </div>
      {/if}
    {/snippet}
    {#snippet footer()}
      <span class="spacer"></span>
      <button class="btn ghost" type="button" onclick={() => (open = null)}>Cancel</button>
      <button class="btn pri" type="button" disabled={sending} onclick={() => void send(action, picked)}>
        {name}
      </button>
    {/snippet}
  </Modal>
{/if}

<style>
  .ab {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .ab-q {
    margin: 0 0 12px;
    font-size: 13px;
    overflow-wrap: anywhere;
  }

  /* The choices read as values, in the dialog's own scale. */
  .ab-opts {
    height: 24px;
  }

  .ab-opts .seg-b {
    padding: 0 10px;
    font-size: 11px;
    text-transform: none;
    letter-spacing: 0;
  }
</style>

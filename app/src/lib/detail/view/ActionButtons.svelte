<!--
  A row of a view's actions, one button each (#667, #702): the view's toolbar,
  or an actions block inside one section.

  Generic on purpose. It knows a label and a tone and never which op it
  draws: `SourceView` has already dropped any action whose op the source does
  not declare or the shell cannot queue, and `onaction` (the detail) turns the
  dialog's values into a write and queues it. A view never writes.

  A `primary` action is filled, a `danger` one in the error tone. One the
  source says cannot be taken now stays in its place, disabled, with the
  source's reason as its tooltip and its accessible description; it stays
  focusable (`aria-disabled`) so the reason can be reached by keyboard too.
  Pressing an action opens its declared dialog (`ActionDialog`); one with
  nothing to ask, no sentence and no danger queues on the button.
-->
<script lang="ts">
  import type { Setting, ViewAction } from "../../ipc/entity";
  import type { BuildParameter } from "../../ipc/sources";
  import ActionDialog from "./ActionDialog.svelte";
  import { opensDialog, type ActionValues } from "./actions";

  let {
    actions,
    settings = [],
    size = "sm",
    onaction,
  }: {
    actions: ViewAction[];
    /** The entity's own settings, for an action `with_settings`. */
    settings?: Setting[];
    /** `md` in the toolbar, `sm` inside a section. */
    size?: "sm" | "md";
    /** Queue `action` with the dialog's values and changed settings; answers whether it was queued. */
    onaction: (action: ViewAction, values: ActionValues, changed: BuildParameter[]) => Promise<boolean>;
  } = $props();

  /** The action whose dialog is up, if any. */
  let open = $state<ViewAction | null>(null);
  /** True while a button's own write is being queued, so nothing double-fires. */
  let sending = $state(false);

  /** Unique per instance, so description ids cannot collide. */
  const uid = Math.random().toString(36).slice(2, 9);

  async function press(action: ViewAction) {
    if (action.disabled !== null || sending) return;
    if (opensDialog(action)) {
      open = action;
      return;
    }
    sending = true;
    try {
      await onaction(action, {}, []);
    } finally {
      sending = false;
    }
  }
</script>

<div class="ab">
  {#each actions as action, index (index)}
    <button
      class="btn"
      class:sm={size === "sm"}
      class:pri={action.primary && !action.danger && action.disabled === null}
      class:danger={action.danger}
      class:off={action.disabled !== null}
      type="button"
      aria-disabled={action.disabled !== null || sending || undefined}
      aria-describedby={action.disabled !== null ? `ab-${uid}-${index}` : undefined}
      title={action.disabled ?? undefined}
      onclick={() => void press(action)}
    >
      {action.label}
    </button>
    {#if action.disabled !== null}
      <span class="vh" id="ab-{uid}-{index}">{action.disabled}</span>
    {/if}
  {/each}
</div>

{#if open}
  {@const action = open}
  <ActionDialog
    {action}
    {settings}
    onsubmit={(values, changed) => onaction(action, values, changed)}
    onclose={() => (open = null)}
  />
{/if}

<style>
  .ab {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  /* `.btn:disabled`'s look, kept focusable so the reason stays reachable. */
  .btn.off {
    opacity: 0.4;
    cursor: default;
  }

  .btn.off:hover {
    background: transparent;
  }
</style>

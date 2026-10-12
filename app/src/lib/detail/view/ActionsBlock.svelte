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

  **How the dialog asks is the op's** (`actionForm` in `actions.ts`, #669),
  still without this file naming one: a choice is picked from the options or
  typed with them as suggestions, and a form with `parameters` lists the
  entity's own settings (`parameters`, the view's settings blocks of this
  entity, already on screen) to change for this one write. A value starts as
  drawn, a secret starts empty in a password field, a value the source does
  not let change is shown and not offered, and only what the reader changed
  is handed to `onaction`. Every dialog opens afresh.
-->
<script lang="ts">
  import type { Setting, ViewAction } from "../../ipc/entity";
  import type { BuildParameter } from "../../ipc/sources";
  import Modal from "../../shell/Modal.svelte";
  import { actionForm, actionName, changedParameters } from "./actions";

  let {
    actions,
    parameters = [],
    onaction,
  }: {
    actions: ViewAction[];
    /** The entity's own settings, for a dialog whose form changes them for one write. */
    parameters?: Setting[];
    /**
     * Queue `action` with `option` picked or typed and `changed` values;
     * answers whether it was queued.
     */
    onaction: (action: ViewAction, option: string | null, changed: BuildParameter[]) => Promise<boolean>;
  } = $props();

  /** The action whose dialog is up, if any. */
  let open = $state<ViewAction | null>(null);
  /** The choice picked in that dialog. */
  let picked = $state<string | null>(null);
  /** True while a write is being queued, so nothing double-fires. */
  let sending = $state(false);
  /** What is typed in each of the open dialog's values, by name. */
  let drafts = $state<Record<string, string>>({});

  /** Unique per instance, so field and suggestion-list ids cannot collide. */
  const uid = Math.random().toString(36).slice(2, 9);

  const form = $derived(open ? actionForm(open) : null);
  /** The values the open dialog offers, as drawn when it opened. */
  let offered = $state<Setting[]>([]);
  const changed = $derived(changedParameters(offered, drafts));

  function start(action: ViewAction) {
    const shape = actionForm(action);
    if (action.options.length === 0 && action.confirm === null && !shape.parameters) {
      void send(action, null);
      return;
    }
    open = action;
    picked = shape.choice === "pick" ? (action.options[0] ?? null) : "";
    offered = shape.parameters ? parameters : [];
    drafts = Object.fromEntries(offered.map((s) => [s.name, s.secret ? "" : s.value]));
  }

  async function send(action: ViewAction, option: string | null) {
    if (sending) return;
    sending = true;
    try {
      if (await onaction(action, option, changed)) open = null;
    } finally {
      sending = false;
    }
  }

  /** Enter in a field runs the dialog's write, as its button would. */
  function enter(event: KeyboardEvent, action: ViewAction) {
    if (event.key !== "Enter") return;
    event.preventDefault();
    void send(action, picked);
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
      {#if form?.choice === "suggest"}
        <div class="ab-f">
          <label class="lab" for="ab-branch-{uid}">Branch</label>
          <input
            id="ab-branch-{uid}"
            class="inp k"
            list="ab-sug-{uid}"
            aria-label="Branch"
            placeholder="default branch"
            autocomplete="off"
            spellcheck="false"
            bind:value={picked}
            onkeydown={(event) => enter(event, action)}
          />
          <datalist id="ab-sug-{uid}">
            {#each action.options as option (option)}
              <option value={option}></option>
            {/each}
          </datalist>
        </div>
      {:else if action.options.length > 0}
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
      {#if offered.length > 0}
        <div class="sec-h ab-ph"><span class="lab">Parameters</span><span class="k muted">{offered.length}</span></div>
        <div class="ab-ps" role="list">
          {#each offered as setting (setting.name)}
            {@const touched = changed.some((c) => c.name === setting.name)}
            <div class="ab-p" role="listitem" class:on={touched}>
              <span class="ab-pn" class:inh={setting.inherited}>{setting.name}</span>
              {#if setting.editable}
                <input
                  class="ab-pi"
                  type={setting.secret ? "password" : "text"}
                  aria-label={setting.name}
                  placeholder={setting.secret ? "hidden" : undefined}
                  autocomplete="off"
                  spellcheck="false"
                  bind:value={drafts[setting.name]}
                  onkeydown={(event) => enter(event, action)}
                />
              {:else}
                <span class="ab-pv">{setting.value}</span>
              {/if}
              <span class="ab-pm">{touched ? "changed" : setting.inherited ? "inherited" : ""}</span>
            </div>
          {/each}
        </div>
      {/if}
    {/snippet}
    {#snippet footer()}
      {#if offered.length > 0}
        <span class="l">{changed.length === 0 ? "as configured" : `${changed.length} changed`}</span>
      {/if}
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

  /* The branch: one labelled field, the dialog's first input. */
  .ab-f {
    display: grid;
    gap: 6px;
    margin-bottom: 14px;
  }

  .ab-ph {
    margin-bottom: 4px;
  }

  /*
    The values read as the Parameters tab does -- name, value, a quiet mark --
    with the value an input of the row's own height. A changed row's name
    steps up to the text colour; nothing else is coloured.
  */
  .ab-p {
    display: grid;
    grid-template-columns: minmax(120px, 38%) 1fr 64px;
    align-items: center;
    gap: 0 10px;
    min-height: 28px;
    border-bottom: 1px solid var(--hair);
    font: 400 12px/18px var(--mono);
  }

  .ab-p:last-child {
    border-bottom: 0;
  }

  .ab-pn {
    color: var(--muted);
    overflow-wrap: anywhere;
  }

  .ab-pn.inh {
    color: var(--faint);
  }

  .ab-p.on .ab-pn {
    color: var(--text);
  }

  .ab-pi {
    width: 100%;
    height: 22px;
    padding: 0 6px;
    font: inherit;
    color: var(--text);
    background: var(--bg);
    border: 1px solid var(--hair2);
    border-radius: 2px;
  }

  .ab-pi:focus {
    outline: none;
    border-color: var(--link);
  }

  .ab-pv {
    padding: 0 7px;
    color: var(--muted);
    overflow-wrap: anywhere;
  }

  .ab-pm {
    font: 400 10px/18px var(--mono);
    color: var(--faint);
    text-align: right;
  }

  .ab-opts .seg-b {
    padding: 0 10px;
    font-size: 11px;
    text-transform: none;
    letter-spacing: 0;
  }
</style>

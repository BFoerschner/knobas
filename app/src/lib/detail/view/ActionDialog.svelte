<!--
  One action's dialog, as the source declared it (#702, spec #701): the
  sentence, then each field in order, then -- for an action `with_settings` --
  the entity's own settings to change for this one write (#669).

  Generic on purpose: it knows field kinds and never which op it draws. A text
  is one line (its suggestions a `datalist`), a long text a plain text area
  (Markdown the source will render; never rendered here, ADR-0020 rule 5), a
  choice a segmented pick (a `select` past four options), a flag a checkbox.
  A required field left blank stops the write and says so beside the field.
  What the reader answered goes up as text by field name; only the settings
  the reader changed go with it.

  It mounts afresh for every opening, so a cancelled dialog leaves nothing
  behind.
-->
<script lang="ts">
  import { tick, untrack } from "svelte";
  import type { ActionField, Setting, ViewAction } from "../../ipc/entity";
  import type { BuildParameter } from "../../ipc/sources";
  import Modal from "../../shell/Modal.svelte";
  import { actionName, changedParameters, initialValues, missingFields, type ActionValues } from "./actions";
  import { lineName, type PendingComment } from "./pending-comments.svelte";

  let {
    action,
    settings = [],
    pending = [],
    onsubmit,
    onclose,
  }: {
    action: ViewAction;
    /** The entity's own settings, offered when the action is `with_settings`. */
    settings?: Setting[];
    /** The pending line comments that go with this write (#709), listed for the reader. */
    pending?: PendingComment[];
    /** Queue the write; answers whether it was queued, and the dialog closes only then. */
    onsubmit: (values: ActionValues, changed: BuildParameter[]) => Promise<boolean>;
    onclose: () => void;
  } = $props();

  // Read once, on purpose: the dialog mounts afresh for each opening, so the
  // values start from the action as it was when pressed and never reset
  // under the reader's typing. `untrack` says so to the rune compiler.
  const initial = untrack(() => ({
    values: initialValues(action),
    offered: action.with_settings ? settings : [],
  }));

  let values = $state<ActionValues>(initial.values);
  /** The settings this dialog offers, as drawn when it opened. */
  const offered: Setting[] = initial.offered;
  let drafts = $state<Record<string, string>>(
    Object.fromEntries(offered.map((s) => [s.name, s.secret ? "" : s.value])),
  );
  const changed = $derived(changedParameters(offered, drafts));

  /** Set by the first press of the write: from then on a blank required field says so. */
  let tried = $state(false);
  let sending = $state(false);
  const missing = $derived(tried ? missingFields(action, values) : []);

  const name = $derived(actionName(action.label));
  /** Unique per instance, so field ids cannot collide. */
  const uid = Math.random().toString(36).slice(2, 9);
  const id = (field: ActionField) => `ad-${uid}-${field.name}`;

  let form = $state<HTMLElement | null>(null);

  async function send() {
    if (sending) return;
    tried = true;
    const blank = missingFields(action, values);
    if (blank.length > 0) {
      form?.querySelector<HTMLElement>(`[data-field="${CSS.escape(blank[0]!)}"]`)?.focus();
      return;
    }
    sending = true;
    try {
      if (await onsubmit({ ...values }, changed)) onclose();
    } finally {
      sending = false;
    }
  }

  /**
   * Where the reader starts: the first field (a choice on its picked option).
   * With nothing to fill in, the write itself -- or Cancel for a danger, so an
   * Enter on arrival never destroys anything. After the Modal has moved focus
   * in and noted the opener to give it back to, hence the tick.
   */
  $effect(() => {
    void tick().then(() => {
      const dlg = form?.closest<HTMLElement>("[role=dialog]");
      const first = action.fields[0];
      const target = first
        ? (form?.querySelector<HTMLElement>(`[data-field="${CSS.escape(first.name)}"] [aria-checked=true]`) ??
          form?.querySelector<HTMLElement>(`[data-field="${CSS.escape(first.name)}"]`))
        : dlg?.querySelector<HTMLElement>(action.danger ? "[data-cancel]" : "[data-send]");
      target?.focus();
    });
  });

  /**
   * Enter sends, as the button would, from a one-line field, a choice's
   * option or a checkbox (as a native form does); in a text area it takes
   * Cmd/Ctrl.
   */
  function sendOnEnter(event: KeyboardEvent, multiline = false) {
    if (event.key !== "Enter" || (multiline && !(event.metaKey || event.ctrlKey))) return;
    event.preventDefault();
    void send();
  }

  /** Left and right move the pick, as a radio group does. */
  function movePick(event: KeyboardEvent, field: ActionField, index: number) {
    if (field.kind.type !== "choice") return;
    const options = field.kind.options;
    const to =
      event.key === "ArrowRight" || event.key === "ArrowDown"
        ? (index + 1) % options.length
        : event.key === "ArrowLeft" || event.key === "ArrowUp"
          ? (index - 1 + options.length) % options.length
          : null;
    if (to === null) {
      sendOnEnter(event);
      return;
    }
    event.preventDefault();
    values[field.name] = options[to]?.value ?? values[field.name]!;
    const group = (event.currentTarget as HTMLElement).parentElement;
    group?.querySelectorAll<HTMLButtonElement>("[role=radio]")[to]?.focus();
  }
</script>

<Modal title={name} center {onclose}>
  {#snippet body()}
    <div class="ad" bind:this={form}>
      {#if action.confirm}
        <p class="ad-q" class:danger={action.danger}>{action.confirm}</p>
      {:else if action.danger}
        <!-- A danger always asks (spec #701); a source that sent no sentence gets the shell's. -->
        <p class="ad-q danger">This cannot be undone from knobas.</p>
      {/if}
      {#if pending.length > 0}
        <div class="ad-lc">
          <p class="ad-lch">
            <span class="ad-dot" aria-hidden="true"></span>
            {pending.length}
            {pending.length === 1 ? "line comment goes" : "line comments go"} with this review
          </p>
          <ul class="ad-lcs">
            {#each pending as comment (comment.id)}
              <li>
                <span class="ad-lcw k">{comment.path}<span class="faint">, {lineName(comment.side, comment.line)}</span></span>
                <span class="ad-lcb">{comment.body}</span>
              </li>
            {/each}
          </ul>
        </div>
      {/if}
      {#each action.fields as field (field.name)}
        {@const bad = missing.includes(field.name)}
        <div class="ad-f" class:bad>
          {#if field.kind.type === "flag"}
            <label class="ad-chk">
              <input
                type="checkbox"
                data-field={field.name}
                aria-invalid={bad || undefined}
                checked={values[field.name] === "true"}
                onchange={(event) => (values[field.name] = String(event.currentTarget.checked))}
                onkeydown={(event) => sendOnEnter(event)}
              />
              <span>{field.label}</span>
            </label>
          {:else}
            <label class="lab" for={id(field)}>{field.label}</label>
            {#if field.kind.type === "text"}
              <input
                id={id(field)}
                class="inp k"
                data-field={field.name}
                list={field.kind.suggestions.length > 0 ? `${id(field)}-sug` : undefined}
                placeholder={field.placeholder ?? undefined}
                aria-invalid={bad || undefined}
                autocomplete="off"
                spellcheck="false"
                bind:value={values[field.name]}
                onkeydown={(event) => sendOnEnter(event)}
              />
              {#if field.kind.suggestions.length > 0}
                <datalist id="{id(field)}-sug">
                  {#each field.kind.suggestions as suggestion (suggestion)}
                    <option value={suggestion}></option>
                  {/each}
                </datalist>
              {/if}
            {:else if field.kind.type === "long_text"}
              <textarea
                id={id(field)}
                class="inp ad-ta"
                data-field={field.name}
                rows="6"
                placeholder={field.placeholder ?? undefined}
                aria-invalid={bad || undefined}
                bind:value={values[field.name]}
                onkeydown={(event) => sendOnEnter(event, true)}
              ></textarea>
            {:else if field.kind.type === "choice" && field.kind.options.length > 4}
              <select
                id={id(field)}
                class="inp k"
                data-field={field.name}
                aria-invalid={bad || undefined}
                bind:value={values[field.name]}
                onkeydown={(event) => sendOnEnter(event)}
              >
                {#each field.kind.options as option (option.value)}
                  <option value={option.value}>{option.label}</option>
                {/each}
              </select>
            {:else if field.kind.type === "choice"}
              {@const kind = field.kind}
              <div
                id={id(field)}
                class="seg ad-opts"
                role="radiogroup"
                aria-label={field.label}
                aria-invalid={bad || undefined}
                data-field={field.name}
                tabindex="-1"
              >
                {#each kind.options as option, index (option.value)}
                  {@const on = option.value === values[field.name]}
                  <button
                    type="button"
                    class="seg-b"
                    class:on
                    role="radio"
                    aria-checked={on}
                    tabindex={on ? 0 : -1}
                    onclick={() => (values[field.name] = option.value)}
                    onkeydown={(event) => movePick(event, field, index)}
                  >
                    {option.label}
                  </button>
                {/each}
              </div>
            {/if}
          {/if}
          {#if bad}
            <span class="ad-err">Required</span>
          {/if}
        </div>
      {/each}
      {#if offered.length > 0}
        <div class="sec-h ad-ph"><span class="lab">Parameters</span><span class="k muted">{offered.length}</span></div>
        <div class="ad-ps" role="list">
          {#each offered as setting (setting.name)}
            {@const touched = changed.some((c) => c.name === setting.name)}
            <div class="ad-p" role="listitem" class:on={touched}>
              <span class="ad-pn" class:inh={setting.inherited}>{setting.name}</span>
              {#if setting.editable}
                <input
                  class="inp k ad-pi"
                  type={setting.secret ? "password" : "text"}
                  aria-label={setting.name}
                  placeholder={setting.secret ? "hidden" : undefined}
                  autocomplete="off"
                  spellcheck="false"
                  bind:value={drafts[setting.name]}
                  onkeydown={(event) => sendOnEnter(event)}
                />
              {:else}
                <span class="ad-pv">{setting.value}</span>
              {/if}
              <span class="ad-pm">{touched ? "changed" : setting.inherited ? "inherited" : ""}</span>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  {/snippet}
  {#snippet footer()}
    {#if offered.length > 0}
      <span class="l">{changed.length === 0 ? "as configured" : `${changed.length} changed`}</span>
    {/if}
    <span class="spacer"></span>
    <button class="btn ghost" type="button" data-cancel onclick={onclose}>Cancel</button>
    <button
      class="btn"
      class:pri={!action.danger}
      class:danger={action.danger}
      type="button"
      data-send
      disabled={sending}
      onclick={() => void send()}
    >
      {name}
    </button>
  {/snippet}
</Modal>

<style>
  .ad {
    display: grid;
    gap: 14px;
  }

  /* The pending line comments going with this write (#709): a quiet list,
     amber only in its dot -- they are not sent yet. */
  .ad-lc {
    display: grid;
    gap: 6px;
  }

  .ad-lch {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    font-size: 12px;
  }

  .ad-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--amber);
  }

  .ad-lcs {
    list-style: none;
    max-height: 132px;
    overflow: auto;
    border: 1px solid var(--hair);
    border-radius: 2px;
  }

  .ad-lcs li {
    display: grid;
    gap: 1px;
    padding: 4px 8px;
    border-left: 2px solid var(--amber);
  }

  .ad-lcs li + li {
    border-top: 1px solid var(--hair);
  }

  .ad-lcw {
    font-size: 11px;
    overflow-wrap: anywhere;
  }

  .ad-lcb {
    font-size: 12px;
    color: var(--muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .ad-q {
    margin: 0;
    font-size: 13px;
    overflow-wrap: anywhere;
  }

  .ad-q.danger {
    color: var(--fail);
  }

  /* One labelled field: label over input, the dialog's own scale. */
  .ad-f {
    display: grid;
    gap: 6px;
  }

  .ad-f.bad .inp,
  .ad-f.bad .ad-opts {
    border-color: var(--fail);
  }

  .ad-err {
    font: 400 11px var(--mono);
    color: var(--fail);
  }

  .ad-ta {
    font: 400 12px/1.5 var(--mono);
    min-height: 96px;
  }

  .ad-chk {
    display: flex;
    gap: 8px;
    align-items: center;
    font-size: 12px;
  }

  .ad-chk input {
    accent-color: var(--text);
  }

  /* The choices read as values, in the dialog's own scale. */
  .ad-opts {
    height: 24px;
    justify-self: start;
  }

  .ad-opts .seg-b {
    padding: 0 10px;
    font-size: 11px;
    text-transform: none;
    letter-spacing: 0;
  }

  .ad-ph {
    margin-bottom: -10px;
  }

  /*
    The values read as the Parameters tab does -- name, value, a quiet mark --
    with the value an input of the row's own height. A changed row's name
    steps up to the text colour; nothing else is coloured.
  */
  .ad-p {
    display: grid;
    grid-template-columns: minmax(120px, 38%) 1fr 64px;
    align-items: center;
    gap: 0 10px;
    min-height: 28px;
    border-bottom: 1px solid var(--hair);
    font: 400 12px/18px var(--mono);
  }

  .ad-p:last-child {
    border-bottom: 0;
  }

  .ad-pn {
    color: var(--muted);
    overflow-wrap: anywhere;
  }

  .ad-pn.inh {
    color: var(--faint);
  }

  .ad-p.on .ad-pn {
    color: var(--text);
  }

  /* `.inp k` at the row's height. */
  .ad-pi {
    height: 22px;
    padding: 0 6px;
  }

  .ad-pv {
    padding: 0 7px;
    color: var(--muted);
    overflow-wrap: anywhere;
  }

  .ad-pm {
    font: 400 10px/18px var(--mono);
    color: var(--faint);
    text-align: right;
  }
</style>

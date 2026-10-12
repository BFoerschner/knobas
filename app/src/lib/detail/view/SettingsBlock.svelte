<!--
  A settings block (ADR-0020): named values, one row each -- a build
  configuration's parameters.

  **Editable in place** (#668, rule 4: a view never writes). Where the adapter
  marks a value `editable` and the shell can queue the block's op
  (`writable`), the value cell is a button; pressing it swaps the cell for an
  input of the same height, so nothing else on the row moves. Enter saves,
  Esc cancels. Saving hands the name, the value and the value the edit was
  made against (`null` for a secret, never drawn) to `onsave`, which queues the
  write -- the source refuses it if the value changed since; the row then carries a mark until the queue settles (`marks`, kept
  by the detail, which re-reads the view when the write lands).

  A secret value is never drawn -- the adapter does not send one -- and says
  `hidden`; its editor starts empty and types into a password field. An
  inherited one is marked, quietly, and its editor says before saving that
  the edit sets a value here that overrides the inherited one.
-->
<script lang="ts">
  import { tick } from "svelte";
  import type { Setting } from "../../ipc/entity";
  import type { SettingMark } from "./settings";

  let {
    title,
    values,
    writable = false,
    marks = {},
    onsave = () => Promise.resolve(false),
  }: {
    title: string | null;
    values: Setting[];
    /** Whether the shell can queue this block's op; without it no row is editable. */
    writable?: boolean;
    /** The rows whose edit is on its way through the queue, by name. */
    marks?: Record<string, SettingMark>;
    /**
     * Queue `name = value`, edited against `base` (the value drawn when the
     * edit opened; `null` for a secret); answers whether it was queued.
     */
    onsave?: (name: string, value: string, base: string | null) => Promise<boolean>;
  } = $props();

  /** The row being edited, by name, and what is typed in it. */
  let editing = $state<string | null>(null);
  let draft = $state("");
  /**
   * The value the open edit started from. Kept here, not read off the row at
   * save: a view re-read while the reader types must not move what the edit
   * was made against.
   */
  let base: string | null = null;
  let saving = $state(false);
  let input = $state<HTMLInputElement | null>(null);

  const MARK_CLASS: Record<SettingMark["state"], string> = {
    saving: "link",
    pending: "link",
    held: "amber",
    refused: "fail",
  };

  function canEdit(setting: Setting): boolean {
    return writable && setting.editable;
  }

  async function open(setting: Setting) {
    if (saving) return;
    editing = setting.name;
    base = setting.secret ? null : setting.value;
    draft = base ?? "";
    await tick();
    input?.focus();
    input?.select();
  }

  function close() {
    editing = null;
    draft = "";
    base = null;
  }

  async function save(setting: Setting) {
    if (saving) return;
    // Nothing changed: nothing to queue. A secret's old value is unknown, so
    // any value typed into it is a change.
    if (base !== null && draft === base) {
      close();
      return;
    }
    saving = true;
    try {
      if (await onsave(setting.name, draft, base)) close();
    } finally {
      saving = false;
    }
  }

  function key(event: KeyboardEvent, setting: Setting) {
    if (event.key === "Enter") {
      event.preventDefault();
      void save(setting);
    } else if (event.key === "Escape") {
      // The detail's own Esc closes the panel; this one only leaves the edit.
      event.preventDefault();
      event.stopPropagation();
      close();
    }
  }

  function hint(setting: Setting): string {
    const lead = [
      setting.secret ? "The current value is not shown." : null,
      setting.inherited ? "Overrides the inherited value with one set here." : null,
    ].filter((line) => line !== null);
    return [...lead, "Enter to save, Esc to cancel."].join(" ");
  }
</script>

<div class="sb">
  {#if title}
    <div class="sec-h"><span class="lab">{title}</span><span class="k muted">{values.length}</span></div>
  {/if}
  <div class="sb-rows" role="list">
    {#each values as setting, index (index)}
      {@const mark = marks[setting.name]}
      <div class="sb-row" role="listitem" class:inh={setting.inherited} class:open={editing === setting.name}>
        <span class="sb-name" title={setting.description ?? undefined}>{setting.name}</span>
        {#if editing === setting.name}
          <input
            bind:this={input}
            class="sb-input"
            type={setting.secret ? "password" : "text"}
            aria-label="New value of {setting.name}"
            placeholder={setting.secret ? "new value" : undefined}
            autocomplete="off"
            spellcheck="false"
            disabled={saving}
            bind:value={draft}
            onkeydown={(event) => key(event, setting)}
          />
          <span class="sb-mark">{#if saving}saving…{/if}</span>
          <span class="sb-hint">{hint(setting)}</span>
        {:else}
          {@const shown = mark && !setting.secret ? mark.value : setting.value}
          {#if canEdit(setting)}
            <button
              class="sb-val sb-edit"
              class:secret={setting.secret}
              class:queued={mark !== undefined}
              aria-label="Edit {setting.name}"
              disabled={mark !== undefined && mark.state === "saving"}
              onclick={() => void open(setting)}
            >
              {#if setting.secret}hidden{:else}{shown}{/if}
            </button>
          {:else}
            <span class="sb-val" class:secret={setting.secret}>
              {#if setting.secret}hidden{:else}{setting.value}{/if}
            </span>
          {/if}
          <span class="sb-mark {mark ? MARK_CLASS[mark.state] : ''}" title={mark?.detail ?? undefined}>
            {#if mark}{mark.state === "saving" ? "saving…" : mark.state}{:else if setting.inherited}inherited{/if}
          </span>
        {/if}
      </div>
    {/each}
  </div>
</div>

<style>
  .sb-row {
    display: grid;
    grid-template-columns: minmax(120px, 34%) 1fr auto;
    align-items: baseline;
    gap: 0 12px;
    min-height: 26px;
    padding: 4px 0;
    border-bottom: 1px solid var(--hair);
    font: 400 12px/18px var(--mono);
  }

  .sb-row:last-child {
    border-bottom: 0;
  }

  .sb-name {
    color: var(--muted);
    overflow-wrap: anywhere;
  }

  .sb-val {
    color: var(--text);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .sb-val.secret {
    color: var(--faint);
    font-style: italic;
  }

  /*
    An editable value is the value itself, pressable: no box, no icon. Its
    hover is a dotted hairline under the text -- the reading stays the reading
    until the reader asks to change it.
  */
  .sb-edit {
    justify-self: start;
    max-width: 100%;
    padding: 0;
    font: inherit;
    text-align: left;
    background: none;
    border: 0;
    border-bottom: 1px dotted transparent;
    cursor: text;
  }

  .sb-edit:hover,
  .sb-edit:focus-visible {
    border-bottom-color: var(--muted);
  }

  .sb-edit:focus-visible {
    outline-offset: 2px;
  }

  /* The value a queued write carries: not yet the source's. */
  .sb-edit.queued {
    color: var(--muted);
  }

  /* Same line height as the value it replaces, so the row does not move. */
  .sb-input {
    width: 100%;
    height: 18px;
    padding: 0 4px;
    margin: 0 -5px;
    font: inherit;
    color: var(--text);
    background: var(--bg);
    border: 1px solid var(--hair2);
    border-radius: 2px;
  }

  .sb-input:focus {
    outline: none;
    border-color: var(--muted);
  }

  .sb-hint {
    grid-column: 2 / -1;
    margin-top: 3px;
    font: 400 11px/16px var(--sans);
    color: var(--faint);
  }

  /* Set further up: the value is still the one in force, so it keeps the
     text colour, and only the name and the mark step back. */
  .sb-row.inh .sb-name {
    color: var(--faint);
  }

  .sb-mark {
    font: 400 10px/18px var(--mono);
    color: var(--faint);
  }

  .sb-mark.link {
    color: var(--link);
  }

  .sb-mark.amber {
    color: var(--amber);
  }

  .sb-mark.fail {
    color: var(--fail);
  }
</style>

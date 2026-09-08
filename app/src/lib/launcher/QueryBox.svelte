<!--
  The box itself (`.search .q`, round 3, lines 364-371).

  The input never loses focus while the overlay is open: everything else in the
  launcher is a list the keyboard walks, so a click that moved focus to a row
  would make the next keystroke go nowhere. Rows are `<button>`s for the
  accessibility tree and the pointer; focus stays here.
-->
<script lang="ts">
  let {
    value,
    placeholder,
    pending,
    footnote,
    oninput,
    onkeydown,
    onclose,
  }: {
    value: string;
    placeholder: string;
    /** A query is in flight — the caret dims rather than the box jumping. */
    pending: boolean;
    /** The right-hand note: "mirror · 0 pending writes". */
    footnote: string;
    oninput: (value: string) => void;
    onkeydown: (event: KeyboardEvent) => void;
    onclose: () => void;
  } = $props();

  let input = $state<HTMLInputElement | null>(null);

  /**
   * Take focus on mount and keep it.
   *
   * `$effect` rather than `autofocus`: the attribute is honoured once per
   * document in some engines and not at all after a re-render, and the overlay
   * is mounted and unmounted repeatedly.
   */
  $effect(() => {
    input?.focus();
  });

  /** The one export: the shell re-focuses after a click landed on a row. */
  export function focus(): void {
    input?.focus();
  }
</script>

<div class="q">
  <span class="caret" class:pending></span>
  <input
    bind:this={input}
    {value}
    {placeholder}
    autocomplete="off"
    spellcheck="false"
    aria-label="Search or act"
    oninput={(event) => oninput(event.currentTarget.value)}
    {onkeydown}
  />
  <span class="faint k">{footnote}</span>
  <kbd>⌘K</kbd>
  <button class="x" aria-label="Close" onclick={onclose}>
    <svg viewBox="0 0 16 16" aria-hidden="true">
      <path d="M4 4l8 8M12 4l-8 8" />
    </svg>
  </button>
</div>

<style>
  .q {
    display: flex;
    align-items: center;
    gap: 9px;
    height: 48px;
    padding: 0 12px 0 0;
    border-bottom: 1px solid var(--hair);
    flex: none;
  }
  .caret {
    width: 3px;
    height: 26px;
    background: var(--amber);
    flex: none;
  }
  /* Dimming the caret is the whole "loading" indicator. A spinner over a list
     that is about to be replaced draws the eye to the wrong place, and the
     debounce means it would flash on every word. */
  .caret.pending {
    background: var(--hair2);
  }
  input {
    flex: 1;
    min-width: 120px;
    height: 100%;
    font: 400 15px var(--sans);
    color: var(--text);
    background: transparent;
  }
  input::placeholder {
    color: var(--faint);
  }
  .k {
    white-space: nowrap;
  }
  .x {
    width: 26px;
    height: 26px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: 2px;
    color: var(--muted);
    flex: none;
  }
  .x:hover {
    background: var(--raised);
    color: var(--text);
  }
  .x svg {
    width: 14px;
    height: 14px;
    stroke: currentColor;
    fill: none;
    stroke-width: 1.6;
    stroke-linecap: round;
  }
</style>

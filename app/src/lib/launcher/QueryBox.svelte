<!--
  The box itself: prototype-6's `.launch input`, borderless 17 px over a
  bottom rule (#1422; the rules are `tf/css/launcher.css`).

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

  /**
   * The one export. On opening the launcher passes `select`, so a query kept
   * from the last opening is replaced by the next keystroke or paste rather
   * than appended to (#1480). After a click landed on a row it does not: the
   * caret stays where the reader left it.
   */
  export function focus(options: { select?: boolean } = {}): void {
    input?.focus();
    if (options.select) input?.select();
  }
</script>

<div class="tf-ln-q">
  <span class="tf-ln-caret" class:tf-ln-pending={pending}></span>
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
  <span class="tf-ln-foot">{footnote}</span>
  <kbd>⌘K</kbd>
  <button class="tf-ln-x" aria-label="Close" onclick={onclose}>
    <svg viewBox="0 0 16 16" aria-hidden="true">
      <path d="M4 4l8 8M12 4l-8 8" />
    </svg>
  </button>
</div>

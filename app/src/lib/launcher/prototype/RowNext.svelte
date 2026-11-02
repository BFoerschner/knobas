<!--
  PROTOTYPE: the result row, round 9 (Björn, 2026-11-02: "less is more").

  Three cells like a table: the kind's monogram in the state's colour, a key
  column (the ticket's key, the pull request's number, otherwise the kind's
  word), and the title. No second line, no provenance, no state chip: the
  state is the monogram's colour, and what a second line said is either in
  the title already or not worth the row's height.

  `launcher/Row.svelte` is the shipped row; this one would replace it for the
  launcher and the board, and `Results.svelte` would draw it.
-->
<script lang="ts">
  import type { Tone } from "../../ipc/entity";

  let {
    monogram,
    tone = null,
    kindName,
    cell,
    mono = false,
    title,
    id,
    selected,
    onopen,
    onhover,
  }: {
    monogram: string;
    /** The state's tone, which colours the monogram; `null` for a kind that declares none. */
    tone?: Tone | null;
    /** What the monogram stands for, for the title attribute and a screen reader. */
    kindName: string;
    /** The key column: a key, a number, a time, or the kind's word. */
    cell: string;
    /** Whether the cell is an identifier, drawn in the mono face. */
    mono?: boolean;
    title: string;
    id: string;
    selected: boolean;
    onopen: () => void;
    onhover: () => void;
  } = $props();
</script>

<button {id} class="row" class:tf-ln-on={selected} role="option" aria-selected={selected} onclick={onopen} onmouseenter={onhover}>
  <span class="mg" class:toned={tone !== null} data-tone={tone} title={kindName} aria-label={kindName}>{monogram}</span>
  <span class="cell" class:mono>{cell}</span>
  <span class="title">{title}</span>
</button>

<style>
  .row {
    display: grid; grid-template-columns: 28px 118px minmax(0, 1fr); gap: 12px; align-items: center; width: 100%;
    text-align: left; min-height: 38px; padding: 6px 18px; font-size: var(--t-md); color: var(--ink);
  }
  .row > * { min-width: 0; overflow: hidden; white-space: nowrap; text-overflow: ellipsis; }
  .row:hover { background: var(--sheet-2); }
  .row.tf-ln-on { background: var(--sheet-2); box-shadow: inset 3px 0 0 var(--ink); }
  .mg {
    display: inline-flex; align-items: center; justify-content: center; width: 28px; height: 20px; border: 1px solid var(--rule);
    border-radius: 3px; font-size: var(--t-2xs); font-weight: 600; letter-spacing: .04em; color: var(--ink-2);
  }
  /* The state's colour, the chip's own tones (`tf/css/chrome.css`), on the monogram instead of a chip. */
  .mg.toned[data-tone="ok"] { color: var(--ok); background: var(--ok-bg); border-color: transparent; }
  .mg.toned[data-tone="warn"] { color: var(--warn); background: var(--warn-bg); border-color: transparent; }
  .mg.toned[data-tone="err"] { color: var(--err); background: var(--err-bg); border-color: transparent; }
  .mg.toned[data-tone="info"] { color: var(--info); background: var(--info-bg); border-color: transparent; }
  .mg.toned[data-tone="muted"] { color: var(--muted); background: var(--muted-bg); border-color: transparent; }
  .cell { font-size: var(--t-sm); color: var(--ink-2); }
  .cell.mono { font-family: var(--mono); font-variant-ligatures: none; font-weight: 600; color: var(--ink); }
  .title { color: var(--ink); }
</style>

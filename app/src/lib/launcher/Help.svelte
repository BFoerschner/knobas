<!--
  The `?` card (`.prefixes`/`.pfx`, round 3, lines 386-389).

  Generated from `syntax.json`, which mirrors the token table in
  `crates/knobas-search/src/query.rs` — and the mirroring is *enforced*, not
  promised: `crates/knobas-search/tests/it/help_card.rs` runs every row's probe
  through the real parser and fails if the card advertises a filter the grammar
  greys out, greys out one the grammar honours, or leaves a prefix of the
  grammar undocumented.

  Each row is a button that inserts itself, so the card is also how the syntax
  is learned by using it.
-->
<script lang="ts">
  import type { LauncherRow } from "./rows";
  import { SYNTAX } from "./syntax";

  let {
    rows,
    selected,
    onopen,
    onhover,
  }: {
    rows: LauncherRow[];
    selected: number;
    onopen: (row: LauncherRow) => void;
    onhover: (index: number) => void;
  } = $props();
</script>

<div class="secl"><span class="lab">Prefix syntax</span><span class="n">{SYNTAX.length}</span></div>
<div class="prefixes">
  {#each SYNTAX as entry, i (entry.token)}
    {@const row = rows[i]}
    <button
      class="pfx"
      class:on={i === selected}
      class:greyed={entry.expect === "reported"}
      role="option"
      aria-selected={i === selected}
      onclick={() => row && onopen(row)}
      onmouseenter={() => onhover(i)}
    >
      <span class="k">{entry.token}</span>
      <span class="s">{entry.summary}</span>
    </button>
  {/each}
</div>

<style>
  .secl {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 24px;
    padding: 0 12px 0 8px;
    border-bottom: 1px solid var(--hair);
    background: var(--panel);
  }
  .n {
    margin-left: auto;
    font: 400 11px var(--mono);
    color: var(--faint);
  }
  .prefixes {
    display: grid;
    gap: 1px;
    padding: 6px 8px;
  }
  .pfx {
    display: grid;
    grid-template-columns: 74px 1fr;
    gap: 8px;
    align-items: baseline;
    width: 100%;
    text-align: left;
    padding: 4px;
    border-radius: 2px;
    font-size: 11.5px;
    color: var(--muted);
  }
  .pfx:hover,
  .pfx.on {
    background: var(--raised);
    color: var(--text);
  }
  .pfx .k {
    font: 500 11px var(--mono);
    color: var(--link);
    text-align: center;
    border: 1px solid var(--hair);
    border-radius: 2px;
    padding: 1px 0;
  }
  /* A token the parser reports rather than honours is drawn the way the chip
     bar draws it, so the two agree about what "does nothing yet" looks like. */
  .pfx.greyed .k {
    color: var(--faint);
    border-style: dashed;
  }
</style>

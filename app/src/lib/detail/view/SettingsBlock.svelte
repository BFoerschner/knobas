<!--
  A settings block (ADR-0020): named values, one row each -- a build
  configuration's parameters.

  Read-only until the ticket that wires edits through the write queue (rule 4:
  a view never writes; #668). The row is built for that ticket: the name, the
  value cell, and a narrow mark column, so an editable value swaps its cell for
  an input of the same height without moving anything else.

  A secret value is never drawn -- the adapter does not send one -- and says
  `hidden`. An inherited one is marked, quietly: it is set further up, and an
  edit here would override it rather than change it.
-->
<script lang="ts">
  import type { Setting } from "../../ipc/entity";

  let { title, values }: { title: string | null; values: Setting[] } = $props();
</script>

<div class="sb">
  {#if title}
    <div class="sec-h"><span class="lab">{title}</span><span class="k muted">{values.length}</span></div>
  {/if}
  <div class="sb-rows" role="list">
    {#each values as setting, index (index)}
      <div class="sb-row" role="listitem" class:inh={setting.inherited}>
        <span class="sb-name" title={setting.description ?? undefined}>{setting.name}</span>
        <span class="sb-val" class:secret={setting.secret}>
          {#if setting.secret}hidden{:else}{setting.value}{/if}
        </span>
        <span class="sb-mark">{#if setting.inherited}inherited{/if}</span>
      </div>
    {/each}
  </div>
</div>

<style>
  .sb-row {
    display: grid;
    grid-template-columns: minmax(120px, 34%) 1fr auto;
    align-items: baseline;
    gap: 12px;
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

  /* Set further up: the value is still the one in force, so it keeps the
     text colour, and only the name and the mark step back. */
  .sb-row.inh .sb-name {
    color: var(--faint);
  }

  .sb-mark {
    font: 400 10px/18px var(--mono);
    color: var(--faint);
  }
</style>

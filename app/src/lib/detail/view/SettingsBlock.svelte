<!--
  A settings block (ADR-0020), read-only until the ticket that wires its edits
  through the write queue (rule 4: a view never writes). A secret value is
  never drawn -- the adapter does not send one -- and an inherited one says
  where it comes from.
-->
<script lang="ts">
  import type { Setting } from "../../ipc/entity";

  let { title, values }: { title: string | null; values: Setting[] } = $props();
</script>

<div class="sb">
  {#if title}
    <div class="sec-h"><span class="lab">{title}</span><span class="k muted">{values.length}</span></div>
  {/if}
  <div class="kv sb-kv">
    {#each values as setting, index (index)}
      <span class="l" title={setting.description ?? undefined}>{setting.name}</span>
      <span class="v">
        {#if setting.secret}
          <span class="muted">hidden</span>
        {:else}
          {setting.value}
        {/if}
        {#if setting.inherited}
          <span class="sb-inh">inherited</span>
        {/if}
      </span>
    {/each}
  </div>
</div>

<style>
  .sb-kv .l {
    overflow-wrap: anywhere;
  }

  .sb-kv .v {
    overflow-wrap: anywhere;
  }

  .sb-inh {
    margin-left: 6px;
    font: 400 10px var(--mono);
    color: var(--faint);
  }
</style>

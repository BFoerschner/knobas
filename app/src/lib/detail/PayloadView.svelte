<!--
  The §3a generic projection, drawn.

  `.kv` rows for scalars, a `.sec p` block for anything long enough to be
  prose, and a `<details>` for an array or object — whose body is the value's
  own JSON, in a `.log`, **as text**.

  Every string here came out of a source system. All of them are interpolated;
  `{@html}` appears nowhere in `app/src/` and `house-rules.test.ts` refuses it
  (gotcha 7).
-->
<script lang="ts">
  import type { Projected } from "./payload";

  let { fields }: { fields: Projected[] } = $props();

  const scalars = $derived(fields.filter((field) => field.kind === "scalar"));
  const blocks = $derived(fields.filter((field) => field.kind !== "scalar"));
</script>

{#if fields.length === 0}
  <div class="empty">
    <p>This item carries no payload — its source mirrored a title and nothing else.</p>
  </div>
{:else}
  {#if scalars.length > 0}
    <div class="kv">
      {#each scalars as field (field.key)}
        <span class="l">{field.label}</span>
        <span class="v">{field.kind === "scalar" ? field.text : ""}</span>
      {/each}
    </div>
  {/if}

  {#each blocks as field (field.key)}
    {#if field.kind === "text"}
      <div class="pv-block">
        <span class="lab">{field.label}</span>
        <p>{field.text}</p>
      </div>
    {:else if field.kind === "nested"}
      <details class="pv-block">
        <summary><span class="lab">{field.label}</span> <span class="k">{field.summary}</span></summary>
        <pre class="log">{field.json}</pre>
      </details>
    {/if}
  {/each}
{/if}

<style>
  .pv-block {
    margin-top: 10px;
  }

  .pv-block p {
    margin-top: 4px;
    /*
      A payload string can be one 4 kB paragraph with no whitespace in it — a
      base64 blob, a stack trace, a URL. Without this it runs off the panel and
      takes the horizontal scrollbar with it.
    */
    overflow-wrap: anywhere;
    white-space: pre-wrap;
  }

  summary {
    cursor: pointer;
    display: flex;
    gap: 8px;
    align-items: baseline;
  }

  .pv-block pre {
    margin-top: 6px;
    max-height: 320px;
  }
</style>

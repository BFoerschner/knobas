<!--
  The element picked, beside the document (the XML view's design, 2026-11-23, section 9), from 1,480 px: its name,
  what it is and its line; its path with *Copy*; its namespace, its attributes, what it holds counted by name, or its
  value cut at 240 characters. With no schema it still says the path and what the element holds, which is what a
  reader copies into a query. What the schema says of it comes with the check (X2).
-->
<script lang="ts">
  import type { Picked } from "../../../render/doc-model";

  let {
    picked,
    oncopy,
  }: {
    picked: Picked | null;
    /** Copies a text, and says so. */
    oncopy: (text: string, what: string) => void;
  } = $props();
</script>

<div class="tf-oas-col tf-oas-page tf-xv-side">
  <div class="tf-oas-measure">
    {#if picked === null}
      <p class="tf-oas-fnote">Pick a line: what it holds stands here.</p>
    {:else}
      <h2 class="tf-xv-h"><span class="tf-xv-hname">{picked.name || picked.what}</span><span class="tf-xv-what">{picked.what}, line {picked.line.toLocaleString("en")}</span></h2>
      <dl class="tf-oas-kv tf-xv-kv">
        <dt>XPath</dt>
        <dd><code class="tf-xv-code">{picked.path}</code> <button type="button" class="tf-oas-more" onclick={() => oncopy(picked.path, "the XPath")}>Copy</button></dd>
        {#if picked.namespace !== null}
          <dt>Namespace</dt>
          <dd><span class="tf-xv-val">{picked.namespace}</span></dd>
        {/if}
        {#if picked.attributes.length > 0}
          <dt>Attributes</dt>
          <dd>{#each picked.attributes as a, i (i)}{#if i > 0}<br />{/if}<span class="tf-xv-pre">@</span>{a.name} <span class="tf-dim">=</span> <span class="tf-xv-val">{a.value}</span>{/each}</dd>
        {/if}
        {#if picked.holds.length > 0}
          <dt>Holds</dt>
          <dd>{#each picked.holds as h, i (h.name)}{#if i > 0}, {/if}{#if h.count > 1}<b class="tf-xv-occ">{h.count}</b>{" "}{/if}<code class="tf-xv-code">{h.name}</code>{/each}{#if picked.holdsMore > 0}{" "}and {picked.holdsMore} more names{/if}</dd>
        {:else if picked.value !== null && picked.value !== ""}
          <dt>Value</dt>
          <dd><span class="tf-xv-val">{picked.value}</span>{#if picked.valueLength > 240}{" "}<span class="tf-dim">{picked.valueLength.toLocaleString("en")} characters</span>{/if}</dd>
        {/if}
      </dl>
    {/if}
  </div>
</div>

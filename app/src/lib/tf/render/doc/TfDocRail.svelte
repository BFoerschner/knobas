<!--
  The XML view's rail (its design, 2026-11-23, sections 5, 11 and 22): the head, *Findings* and *Results*, their list,
  and the keys.

  **The head holds its room from the first frame**: a verdict line and two lines under it, so the list under it never
  moves when the outline lands, and the schema's check (X2, with the schema explorer) takes the same room. Until that
  check is built, the head says only what knobas read (Fable's ruling, 2026-11-23): *Not checked* and the schema the
  document names, *No schema*, or *Not well formed* and where. A tone is for what happened, so only the last has one.
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  import { coded } from "./coded";
  import type { DocState } from "./doc.svelte";

  let {
    st,
    mode = $bindable("results"),
    ongo,
    results = null,
    copy = null,
  }: {
    st: DocState;
    /** What the list shows. */
    mode?: "findings" | "results";
    /** Goes to a line and picks it. */
    ongo: (line: number) => void;
    /** The query's results, drawn by the query's own part (X4). */
    results?: Snippet | null;
    /** The results' *Copy ▾*, where a query answered. */
    copy?: Snippet | null;
  } = $props();

  const opened = $derived(st.opened);
  const broken = $derived(opened?.error ?? null);
  const root = $derived(opened?.root ?? null);
  /** The rail opens on *Findings* for a document that is not well formed: its break is the one row there. */
  let shownFor: unknown = null;
  $effect(() => {
    const o = opened;
    if (o === null || o === shownFor) return;
    shownFor = o;
    mode = o.error !== null ? "findings" : "results";
  });
</script>

{#snippet said(text: string)}{#each coded(text) as part, i (i)}{#if part.code}<code class="tf-xv-code">{part.text}</code>{:else}{part.text}{/if}{/each}{/snippet}

<div class="tf-oas-rail tf-xv-rail">
  <div class="tf-xv-head" aria-live="polite">
    {#if opened === null}
      <div class="tf-xv-verdict"><span class="tf-dim">Reading…</span></div>
      <p>&nbsp;</p>
      <p>&nbsp;</p>
    {:else if broken !== null}
      <div class="tf-xv-verdict tf-xv-bad"><b>Not well formed</b></div>
      <p>
        <button type="button" class="tf-oas-ref" onclick={() => ongo(broken.line)}>At line {broken.line.toLocaleString("en")}</button>, {@render said(broken.message.charAt(0).toLowerCase() + broken.message.slice(1))}
      </p>
      <p>&nbsp;</p>
    {:else if root !== null && root.location !== null}
      <div class="tf-xv-verdict"><b class="tf-xv-quiet">Not checked</b></div>
      <p>It names <code class="tf-xv-code">{root.location}</code>.</p>
      <p>knobas does not check a document against a schema.</p>
    {:else if root !== null && root.namespace !== ""}
      <div class="tf-xv-verdict"><b class="tf-xv-quiet">Not checked</b></div>
      <p>Its namespace is <code class="tf-xv-code">{root.namespace}</code>.</p>
      <p>knobas does not check a document against a schema.</p>
    {:else}
      <div class="tf-xv-verdict"><b class="tf-xv-quiet">No schema</b></div>
      <p>&nbsp;</p>
      <p>&nbsp;</p>
    {/if}
  </div>
  <div class="tf-oas-modes">
    <span class="tf-seg" role="group" aria-label="The list shows">
      <button type="button" aria-pressed={mode === "findings"} onclick={() => (mode = "findings")}>Findings{#if broken !== null}{" "}1{/if}</button>
      <button type="button" aria-pressed={mode === "results"} onclick={() => (mode = "results")}>Results</button>
    </span>
    {#if mode === "results" && copy !== null}{@render copy()}{/if}
  </div>
  <div class="tf-oas-list tf-xv-list" role="listbox" aria-label={mode === "findings" ? "Findings" : "Results"}>
    {#if mode === "findings"}
      {#if broken !== null}
        <button type="button" class="tf-oas-row tf-xv-frow" role="option" aria-selected={st.cursor === broken.line} onclick={() => ongo(broken.line)}>
          <span class="tf-xv-fline">{broken.line.toLocaleString("en")}</span>
          <span class="tf-xv-ftext"><span class="tf-xv-fname">Not well formed</span><span class="tf-xv-fsay">{@render said(broken.message)}</span></span>
        </button>
      {:else if opened !== null}
        <p class="tf-oas-none">Nothing was checked.</p>
      {/if}
    {:else if results !== null}
      {@render results()}
    {/if}
  </div>
  <div class="tf-oas-keys">
    <span><kbd>Q</kbd> query</span><span><kbd>N</kbd><kbd>P</kbd> step</span><span><kbd>←</kbd><kbd>→</kbd> fold</span><span><kbd>Space</kbd> look</span><span><kbd>⌘F</kbd> find</span>
  </div>
</div>

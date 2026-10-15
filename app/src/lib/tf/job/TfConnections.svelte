<!--
  The Connections drawer (#820, R-6.1, R-6.2): everything connected to the
  job, from the right, over a scrim, without moving the page.

  Top row: a filter field, then *Link to…* (*Apply a link set…* and *Save as a
  link set…* arrive with link sets, #869). Groups: **Linked on KEY**, the links
  drawn on the ticket, each with *Unlink* on hover or focus; **Hidden on KEY**
  with *Show again*, which holds the per-ticket hides link sets bring (#867)
  and so draws nothing until then; and the proposals box with *Judge them one
  by one*.

  It registers with the shell's layer stack while open, so `Esc` closes it
  (rung 2), as does a click on the scrim; focus goes to its filter and returns
  to what opened it, the *Connections* button.
-->
<script lang="ts">
  import { untrack } from "svelte";

  import type { LinkEntry, SuggestionEntry } from "../../ipc/entity";
  import Marked from "../filter/Marked.svelte";
  import type { Layers } from "../layers.svelte";
  import { filterLinks, sectionOf } from "./connections";
  import { itemHash } from "./job";

  let {
    jobKey,
    links,
    proposals,
    hidden = [],
    layers,
    onclose,
    onunlink,
    onlinkto,
    onshow = () => {},
  }: {
    jobKey: string;
    /** The links drawn on the ticket. */
    links: LinkEntry[];
    /** This ticket's proposals, `null` while the read is out. */
    proposals: SuggestionEntry[] | null;
    /** Rows hidden on this ticket (link sets, #867). */
    hidden?: { id: string; title: string }[];
    layers: Layers;
    onclose: () => void;
    onunlink: (entry: LinkEntry) => void;
    onlinkto: () => void;
    onshow?: (id: string) => void;
  } = $props();

  let field = $state<HTMLInputElement | null>(null);
  let text = $state("");

  const shown = $derived(filterLinks(links, text));

  $effect(() => {
    const restoreTo = document.activeElement;
    const off = untrack(() => layers.add({ kind: "drawer", close: () => onclose() }));
    field?.focus();
    return () => {
      untrack(off);
      if (restoreTo instanceof HTMLElement && document.contains(restoreTo)) restoreTo.focus();
    };
  });
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="tf-scrim" onclick={onclose}></div>
<aside class="tf-drawer" aria-label="Connections">
  <header>
    <h2>Connections of <span class="tf-mono">{jobKey}</span></h2>
    <button type="button" class="tf-btn tf-sm tf-ghost" onclick={onclose}>Esc</button>
  </header>
  <div class="tf-dfilter">
    <label class="tf-fsearch">
      <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true"
        ><circle cx="7" cy="7" r="5" /><path d="M11 11l3.5 3.5" /></svg
      >
      <input bind:this={field} bind:value={text} data-filter placeholder="Filter connections" aria-label="Filter connections" />
    </label>
    <div class="tf-dacts">
      <button type="button" class="tf-btn tf-sm" onclick={onlinkto}>Link to…</button>
    </div>
  </div>
  <div class="tf-dbody">
    <h3>Linked on {jobKey} <span class="tf-count">{shown.length}</span></h3>
    {#each shown as entry (entry.link.id)}
      <div class="tf-lk">
        <div>
          <a href={itemHash(entry.other)} onclick={onclose}><Marked text={entry.other.title} typed={text} /></a>
          <div class="tf-how">{sectionOf(entry.other.kind)}</div>
        </div>
        <button type="button" class="tf-x" onclick={() => onunlink(entry)} aria-label="Unlink {entry.other.title}">Unlink</button>
      </div>
    {:else}
      <p class="tf-dnone">Nothing linked on the ticket itself.</p>
    {/each}
    {#if hidden.length > 0}
      <h3>Hidden on {jobKey} <span class="tf-count">{hidden.length}</span></h3>
      {#each hidden as row (row.id)}
        <div class="tf-lk">
          <div class="tf-dim">{row.title}</div>
          <button type="button" class="tf-x tf-shown" onclick={() => onshow(row.id)}>Show again</button>
        </div>
      {/each}
    {/if}
    {#if proposals !== null && proposals.length > 0}
      <div class="tf-proposed">
        <strong>{proposals.length} proposed for {jobKey}</strong>
        <a class="tf-btn tf-sm" href="#/review/{encodeURIComponent(proposals[0]!.link.id)}" onclick={onclose}>Judge them one by one</a>
      </div>
    {/if}
    <p class="tf-dnote">Links and link sets stay in knobas and are never written to a source.</p>
  </div>
</aside>

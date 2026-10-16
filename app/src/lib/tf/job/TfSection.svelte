<!--
  One job section (R-4.6), whatever it holds: a title and its *Open … view*
  link, a one-line rollup, then the rows sorted by what asks for you and
  newest first (the caller sorts), **at most four**. Past four, a footer with
  an in-section filter ("Filter 8 builds"), *Show all N* and *All in … view*.
  A row from the parent carries a small label naming it ("Parent PAY-231").
  Every row opens the item's full view (R-4.10).

  Code, Builds, Assets, Monitors and Pages are each this, with their own
  rollup and, for Code and Builds, a line before the rows (the checkout, the
  stale-data warning).
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  import { rowMatches, type Rollup, type SectionRow } from "./job";

  let {
    id,
    title,
    openLabel,
    openHref,
    footHref,
    rows,
    rollup = null,
    roll,
    before,
    view = title.split(" ")[0]!,
    onopen,
  }: {
    /** The section's id: `tf-sec-<id>` names it for assistive tech. */
    id: string;
    title: string;
    /** The header link's words: *Open Builds view*. */
    openLabel: string;
    openHref: string;
    /** Where *All in … view* goes. */
    footHref: string;
    /** Sorted, or `null` while the read is out. */
    rows: SectionRow[] | null;
    /** The rollup, when the section draws the plain kind. */
    rollup?: Rollup | null;
    /** A rollup of the section's own shape, in place of `rollup`. */
    roll?: Snippet;
    /** A line between the rollup and the rows. */
    before?: Snippet;
    /** The full view *All in … view* names: the title's first word unless said. */
    view?: string;
    /** Called as the header link is followed: the view opens set to *This job* (R-2.5). */
    onopen?: () => void;
  } = $props();

  const CAP = 4;
  let filter = $state("");
  let expanded = $state(false);

  const all = $derived(rows ?? []);
  const matching = $derived(all.filter((row) => rowMatches(row, filter)));
  const shown = $derived(expanded || filter.trim() !== "" ? matching : matching.slice(0, CAP));
  const noun = $derived(title.toLowerCase());
</script>

<article class="tf-panel" aria-labelledby="tf-sec-{id}">
  <header>
    <h3 id="tf-sec-{id}">{title}</h3>
    <a class="tf-open" href={openHref} onclick={() => onopen?.()}>{openLabel} <svg viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true"><path d="M7 1.5h3.5V5M10.5 1.5 6 6M5 2H1.5v8.5H10V7" /></svg></a>
  </header>
  <div class="tf-roll">
    {#if rows === null}
      <span>…</span>
    {:else if all.length === 0}
      <span>Nothing connected.</span>
    {:else if roll}
      {@render roll()}
    {:else if rollup}
      <span>{rollup.lead}</span>
      {#each rollup.signals as signal (signal.text)}
        <span class="tf-signal"><span class="tf-dot tf-{signal.tone}"></span>{signal.text}</span>
      {/each}
    {/if}
  </div>
  {#if before}{@render before()}{/if}
  <div class="tf-rows">
    {#each shown as row (row.id)}
      <a class="tf-r" href={row.href}>
        <span class="tf-dot {row.tone ? `tf-${row.tone}` : ''}"></span>
        <span class="tf-t">
          {#if row.mono}<span class="tf-mono">{row.title}</span>{:else}{row.title}{/if}
          <span class="tf-rsub">{#if row.from}<span class="tf-via" title="From {row.from}">{row.from}</span>{/if}{#if row.repo}<span class="tf-repo">{row.repo}</span>{" "}{/if}{row.sub}</span>
        </span>
        {#if row.chip}<span class="tf-rend"><span class="tf-chip tf-{row.chip.tone}">{row.chip.text}</span></span>{/if}
      </a>
    {:else}
      {#if rows !== null && all.length === 0}
        <div class="tf-r tf-dim tf-small">Nothing connected yet. Use the connections drawer to link something.</div>
      {:else if rows !== null}
        <div class="tf-r tf-dim">Nothing matches.</div>
      {/if}
    {/each}
  </div>
  {#if all.length > CAP}
    <div class="tf-secfoot">
      <label class="tf-minif">
        <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" aria-hidden="true"><circle cx="7" cy="7" r="4.5" /><path d="m10.5 10.5 3 3" /></svg>
        <input data-filter bind:value={filter} placeholder="Filter {all.length} {noun}" aria-label="Filter {title}" />
      </label>
      {#if filter.trim() !== ""}
        <span class="tf-dim tf-small">{matching.length} match</span>
      {:else}
        <button class="tf-linkish" onclick={() => (expanded = !expanded)}>{expanded ? "Show fewer" : `Show all ${all.length}`}</button>
      {/if}
      <a class="tf-open tf-push" href={footHref}>All in {view} view</a>
    </div>
  {/if}
</article>

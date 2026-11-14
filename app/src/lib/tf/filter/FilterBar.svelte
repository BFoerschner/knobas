<!--
  The filter bar (#810, requirements §2): one component for every list view
  of the ticket-first shell. Left to right: the text field, the scope toggle,
  chips for fields with few values, dropdowns for fields with many, whatever
  the view adds, and the result count on the right (R-2.1).

  **The parameter shape is `FilterConfig`** (`filter.ts`), and later views pass
  one rather than fork this file:

  - `config.id`        the view; its state is kept for the session (R-2.6)
  - `config.placeholder`, `config.noun`
  - `config.scopes`    *This job / Mine / All*, each with the query it adds
  - `config.chips`     few values, several on at once, meaning any of them
  - `config.dropdowns` many values, *Any* first
  - `config.base`      the view's base query, for a view filtered on the
                       server through the search grammar (R-2.7); absent
                       for one that filters what it holds

  The bar writes into `filter`, the view reads it back (`toQuery`,
  `matchesAll`, `marks`) and calls nothing here. `onchange` fires after every
  change so a server-side view can read again.

  **Typing never moves focus or the caret** (R-2.2): the field is bound to
  `filter.text`, so the bar re-renders only what changed and the view below
  re-renders only its results.

  **Keys**: `Esc` is `filterKeys`'s: a press clears the words and focus stays, and a press
  on the empty box falls through to the shell's ladder (`escape.ts`). `/`
  is not the bar's (#2425): outside a field the shell opens the launcher with
  the section's filter, which is the section's search.
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  import { filterKeys } from "../filter-keys";
  import { countLine, type FilterConfig, type FilterState } from "./filter";
  import TfScope from "./TfScope.svelte";

  let {
    config,
    filter,
    shown,
    total,
    onchange = () => {},
    extra,
    after,
    held,
  }: {
    config: FilterConfig;
    /** The view's filter state, usually `sessionFilter(config.id)`. */
    filter: FilterState;
    /** How many rows the filter keeps, or `null` before the view has read. */
    shown: number | null;
    /** How many rows the view holds unfiltered, or `null` before it is known. */
    total: number | null;
    onchange?: () => void;
    /** What the view adds after the dropdowns: Tickets' *Lanes*. */
    extra?: Snippet;
    /** What follows the count at the end of the bar (Monitors' hidden count). */
    after?: Snippet | undefined;
    /**
     * A list that holds its rows where they first drew (`held-order.svelte.ts`): what its later reads brought that
     * would move them. The bar keeps the room for *Show N new* whether or not it is drawn, so it never wraps the bar.
     */
    held?: { waiting: number; stale: boolean; show: () => void } | undefined;
  } = $props();

  let field: HTMLInputElement | undefined = $state();

  const scopes = $derived(config.scopes ?? []);
  const scope = $derived(filter.scope ?? scopes[0]?.id ?? null);

  function typed(event: Event) {
    filter.text = (event.currentTarget as HTMLInputElement).value;
    onchange();
  }

  function clear() {
    filter.text = "";
    field?.focus();
    onchange();
  }

  function toggle(key: string, value: string) {
    const on = filter.chips[key] ?? [];
    filter.chips[key] = on.includes(value) ? on.filter((v) => v !== value) : [...on, value];
    onchange();
  }

  function choose(key: string, value: string) {
    if (value === "") delete filter.dropdowns[key];
    else filter.dropdowns[key] = value;
    onchange();
  }

  function pick(id: string) {
    filter.scope = id;
    onchange();
  }

  const shownChips = $derived((config.chips ?? []).filter((group) => group.options.length > 0));
  const hasSelects = $derived(extra !== undefined || (config.dropdowns ?? []).some((dropdown) => dropdown.options.length > 0));
  /** With no select to go with it, the last pills keep the count company when the row wraps. */
  const tailChips = $derived(hasSelects ? null : (shownChips.at(-1) ?? null));
  const leadChips = $derived(tailChips === null ? shownChips : shownChips.slice(0, -1));
  const heldWords = $derived(held === undefined ? "" : held.waiting > 0 ? `Show ${held.waiting} new` : held.stale ? "Sort again" : "");
</script>

<div class="tf-fbar" data-fid={config.id}>
  {#if config.search !== false}
  <label class="tf-fsearch" class:tf-has={filter.text !== ""}>
    <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true"
      ><circle cx="7" cy="7" r="5" /><path d="M11 11l3.5 3.5" /></svg
    >
    <input
      bind:this={field}
      {@attach filterKeys()}
      value={filter.text}
      oninput={typed}
      placeholder={config.placeholder}
      aria-label={config.placeholder}
    />
    {#if filter.text !== ""}
      <button type="button" class="tf-fclear" aria-label="Clear filter" onclick={clear}>✕</button>
    {/if}
  </label>
  {/if}
  {#if scopes.length > 0}
    <TfScope {scopes} {scope} onpick={pick} />
  {/if}
  {#snippet pills(group: (typeof shownChips)[number])}
    <div class="tf-fchips" role="group" aria-label={group.label}>
      {#each group.options as option (option.value)}
        {@const on = (filter.chips[group.key] ?? []).includes(option.value)}
        <button type="button" aria-pressed={on} onclick={() => toggle(group.key, option.value)}>
          {option.label}
        </button>
      {/each}
    </div>
  {/snippet}
  {#each leadChips as group (group.key)}{@render pills(group)}{/each}
  <!-- One flex item, so the bar's last things wrap together, the count at the end of their row: it never wraps alone. -->
  <div class="tf-ftail">
    {#if tailChips !== null}{@render pills(tailChips)}{/if}
    {#each config.dropdowns ?? [] as dropdown (dropdown.key)}
      {#if dropdown.options.length > 0}
        <label class="tf-fsel">
          {dropdown.label}
          <select
            aria-label={dropdown.label}
            class:tf-set={!!filter.dropdowns[dropdown.key]}
            value={filter.dropdowns[dropdown.key] ?? ""}
            onchange={(event) => choose(dropdown.key, event.currentTarget.value)}
          >
            <option value="">Any</option>
            {#each dropdown.options as option (option.value)}
              <option value={option.value}>{option.label}</option>
            {/each}
          </select>
        </label>
      {/if}
    {/each}
    {@render extra?.()}
    <span class="tf-fend">
      <!-- *Show N new* stands in the count's own room while rows wait (the still-rows pass), so it appearing moves nothing. -->
      <span
        class="tf-fcount"
        class:tf-fcount-held={heldWords !== ""}
        aria-live="polite"
        data-room={total === null ? "" : countLine(Math.max(0, total - 1), total, config.noun, config.one)}
        data-held={held === undefined ? undefined : "Show 99 new"}
        ><span class="tf-fcount-n">{shown !== null && total !== null ? countLine(shown, total, config.noun, config.one) : ""}</span
        >{#if held !== undefined && heldWords !== ""}<button
            type="button"
            class="tf-fheld"
            title={held.waiting > 0
              ? `${held.waiting === 1 ? "A row" : `${held.waiting} rows`} came in since the list drew. They wait here so nothing moves under the pointer.`
              : "The list reads in another order now. It keeps its rows where they drew until you ask."}
            onclick={held.show}>{heldWords}</button
          >{/if}</span
      >
      {@render after?.()}
    </span>
  </div>
</div>

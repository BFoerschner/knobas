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

  **Keys** (R-2.4): `/` anywhere outside a text field focuses the field. `Esc`
  is the shell's ladder (`escape.ts`), which finds this field by its
  `data-filter`: the first press clears it, the second leaves it.
-->
<script lang="ts">
  import { onMount, type Snippet } from "svelte";

  import { inTextField } from "../../shell/keys";
  import { countLine, type FilterConfig, type FilterState } from "./filter";
  import { pageVisible } from "../kept-pages.svelte";

  let {
    config,
    filter,
    shown,
    total,
    onchange = () => {},
    extra,
    after,
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
    /** What follows the count at the end of the bar (Tickets' *Save as a view*, #812). */
    after?: Snippet | undefined;
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

  const pageShown = pageVisible();
  onMount(() => {
    if (config.hint === false) return;
    const onkey = (event: KeyboardEvent) => {
      // A page keep-alive holds hidden (#2247) takes no keys.
      if (!pageShown()) return;
      if (event.key !== "/" || event.metaKey || event.ctrlKey || event.altKey || event.defaultPrevented) return;
      const at = document.activeElement;
      if (at instanceof HTMLElement && inTextField(at)) return;
      event.preventDefault();
      field?.focus();
    };
    window.addEventListener("keydown", onkey);
    return () => window.removeEventListener("keydown", onkey);
  });
</script>

<div class="tf-fbar" data-fid={config.id}>
  <label class="tf-fsearch" class:tf-has={filter.text !== ""}>
    <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true"
      ><circle cx="7" cy="7" r="5" /><path d="M11 11l3.5 3.5" /></svg
    >
    <input
      bind:this={field}
      data-filter
      value={filter.text}
      oninput={typed}
      placeholder={config.placeholder}
      aria-label={config.placeholder}
    />
    {#if filter.text !== ""}
      <button type="button" class="tf-fclear" aria-label="Clear filter" onclick={clear}>✕</button>
    {:else if config.hint !== false}
      <kbd>/</kbd>
    {/if}
  </label>
  {#if scopes.length > 0}
    <div class="tf-seg" role="group" aria-label="Scope">
      {#each scopes as choice (choice.id)}
        <button type="button" aria-pressed={scope === choice.id} onclick={() => pick(choice.id)}
          >{choice.label}{#if choice.tag}{" "}<span class="tf-jobk">{choice.tag}</span>{/if}</button
        >
      {/each}
    </div>
  {/if}
  {#each config.chips ?? [] as group (group.key)}
    {#if group.options.length > 0}
      <div class="tf-fchips" role="group" aria-label={group.label}>
        {#each group.options as option (option.value)}
          {@const on = (filter.chips[group.key] ?? []).includes(option.value)}
          <button type="button" aria-pressed={on} onclick={() => toggle(group.key, option.value)}>
            {option.label}
          </button>
        {/each}
      </div>
    {/if}
  {/each}
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
  <span class="tf-fcount" aria-live="polite"
    >{shown !== null && total !== null ? countLine(shown, total, config.noun, config.one) : ""}</span
  >
  {@render after?.()}
</div>

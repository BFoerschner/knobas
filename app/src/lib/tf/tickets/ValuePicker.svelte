<!--
  The values one condition of the view editor offers (#814, R-3.14), any of
  which it can mean.

  A few values are a row of toggles. From `SEARCH_FROM` on (a source's
  projects, its people, its epics) a row of toggles wraps into a wall nobody
  reads, so the values are a list in a box that scrolls, under a field that
  searches it: typed letters keep a value when they appear in it in order
  (`fuzzyRank`), the closest first, the matched letters marked. The picked
  values stay above the field as toggles, so a search that hides one from the
  list does not hide it from the condition.

  The field sits in the editor's form and its dialog, and takes two keys from
  them while it has a use for them: Enter picks the current value where the
  form would save the view, and Esc clears a search where the dialog would
  close. ↑ and ↓ move through the list; focus stays in the field.
-->
<script lang="ts">
  import { tick } from "svelte";

  import { fuzzyRank } from "../filter/fuzzy";

  /** The fewest values that are searched, not toggled. */
  const SEARCH_FROM = 9;
  /** The most rows drawn at once; a search narrows the rest into reach. */
  const DRAWN_MOST = 50;

  let {
    options,
    label,
    many,
    isPicked,
    ontoggle,
  }: {
    /** Each value as it is stored, and as it is shown. */
    options: [string, string][];
    /** The condition's field: *Project*. */
    label: string;
    /** What several of its values are called: *projects*. */
    many: string;
    isPicked: (value: string) => boolean;
    ontoggle: (value: string) => void;
  } = $props();

  const uid = $props.id();
  let typed = $state("");
  /** The row ↑, ↓ and Enter act on. */
  let current = $state(0);

  const searched = $derived(options.length >= SEARCH_FROM);
  const hits = $derived(fuzzyRank(options, typed, ([, shown]) => shown));
  const drawn = $derived(hits.slice(0, DRAWN_MOST));
  const picked = $derived(options.filter(([value]) => isPicked(value)));
  const words = $derived(`Search ${options.length} ${many}`);
  const optionId = (index: number) => `${uid}-opt-${index}`;

  function search(text: string) {
    typed = text;
    current = 0;
  }

  async function move(by: 1 | -1) {
    current = Math.min(Math.max(current + by, 0), Math.max(drawn.length - 1, 0));
    await tick();
    document.getElementById(optionId(current))?.scrollIntoView?.({ block: "nearest" });
  }

  function onkeydown(event: KeyboardEvent) {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      void move(event.key === "ArrowDown" ? 1 : -1);
    } else if (event.key === "Enter") {
      // Always the field's: Enter here must never save the view.
      event.preventDefault();
      const hit = drawn[current];
      if (hit === undefined) return;
      ontoggle(hit.option[0]);
      search("");
    } else if (event.key === "Escape" && typed !== "") {
      // The dialog leaves a key a field has taken alone (`TfDialog`).
      event.preventDefault();
      search("");
    }
  }
</script>

{#if !searched}
  <div class="tf-fchips tf-vw-vals">
    {#each options as [value, shown] (value)}
      <button type="button" class="tf-vw-val" aria-pressed={isPicked(value)} onclick={() => ontoggle(value)}>{shown}</button>
    {/each}
  </div>
{:else}
  <div class="tf-vw-pick">
    {#if picked.length > 0}
      <div class="tf-fchips tf-vw-vals" role="group" aria-label="Picked {many}">
        {#each picked as [value, shown] (value)}
          <button type="button" class="tf-vw-val" aria-pressed="true" onclick={() => ontoggle(value)}>{shown}</button>
        {/each}
      </div>
    {/if}
    <input
      type="text"
      role="combobox"
      autocomplete="off"
      aria-autocomplete="list"
      aria-expanded="true"
      aria-controls="{uid}-opts"
      aria-activedescendant={drawn.length > 0 ? optionId(current) : undefined}
      aria-label={words}
      placeholder={words}
      value={typed}
      oninput={(event) => search(event.currentTarget.value)}
      {onkeydown}
    />
    <div class="tf-vw-opts" id="{uid}-opts" role="listbox" aria-multiselectable="true" aria-label="{label} values" hidden={drawn.length === 0}>
      {#each drawn as { option: [value], runs }, index (value)}
        <!-- Not a tab stop, and a click leaves focus in the field: the list is worked from there. -->
        <button
          type="button"
          role="option"
          tabindex="-1"
          id={optionId(index)}
          class="tf-vw-opt"
          class:tf-vw-at={index === current}
          aria-selected={isPicked(value)}
          onmousedown={(event) => event.preventDefault()}
          onclick={() => ontoggle(value)}
        >
          <span class="tf-vw-tick" aria-hidden="true">{isPicked(value) ? "✓" : ""}</span>
          <span>{#each runs as run, at (at)}{#if run.hit}<mark>{run.text}</mark>{:else}{run.text}{/if}{/each}</span>
        </button>
      {/each}
    </div>
    {#if hits.length === 0}
      <p class="tf-dim">No match for {typed.trim()}.</p>
    {:else if hits.length > drawn.length}
      <p class="tf-dim">{hits.length - drawn.length} more. Type to narrow them down.</p>
    {/if}
  </div>
{/if}

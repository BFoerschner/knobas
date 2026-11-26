<!--
  What a filtered list says when the filter leaves nothing (design system, Words: "An empty view says what
  would fill it and offers the action"): one sentence and the way out, on every list that has a filter bar.
  `children` is what the view adds, such as *Search all monitors* from a job's scope. *Clear the filter* shows
  only when the words, a chip or a dropdown narrow the list; the scope is the view's own.
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  import { isFiltering, type FilterState } from "./filter";

  let {
    noun,
    filter,
    onclear = () => {},
    children,
  }: {
    /** The plural: *No monitors match.* */
    noun: string;
    filter: FilterState;
    /** After the filter is cleared, for a view that reads again on a change. */
    onclear?: () => void;
    children?: Snippet;
  } = $props();

  function clear() {
    filter.text = "";
    filter.chips = {};
    filter.dropdowns = {};
    onclear();
  }
</script>

<div class="tf-empty">
  <p>No {noun} match.</p>
  {@render children?.()}
  {#if isFiltering(filter)}<button type="button" class="tf-btn tf-sm" onclick={clear}>Clear the filter</button>{/if}
</div>

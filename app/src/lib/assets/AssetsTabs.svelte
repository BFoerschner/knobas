<!--
  The Assets view's tab strip — *Tree* and *Monitors* (#428, #448).

  One component and not a copy per surface, because the two tabs are drawn by
  two different views: the Tree is `AssetsView` with its columns, its search
  box and its pane, and the roster is `MonitorsView` with none of them. What
  makes them one *view* to a reader is this strip, so the strip has one
  spelling. A copy each would let one drift a label, an order or an
  `aria-current` from the other, and the drift would be invisible until
  somebody switched tabs.

  **The tab rides in the address** (`#/assets/tree`, `#/assets/monitors`), so
  switching is navigation and a filtered roster is a link somebody can hand
  over. `hashFor` and not a literal, the rule the strip has followed since
  #428: a literal would keep type-checking while pointing at the wrong tab.

  Never "board" — ADR-0009, which is why the first tab is called *Tree*.
-->
<script lang="ts">
  import { hashFor, type AssetsTab, type Router } from "../shell/router.svelte";

  let { router, tab }: { router: Router; tab: AssetsTab } = $props();

  /**
   * Both tabs, in the order they are drawn.
   *
   * Typed as `AssetsTab`, so a third member of that union has to be given a
   * label here or fail `svelte-check` — which is the whole reason the union is
   * not a bare string.
   */
  const TABS: { id: AssetsTab; label: string }[] = [
    { id: "tree", label: "Tree" },
    { id: "monitors", label: "Monitors" },
  ];
</script>

<nav class="tabs" aria-label="Assets views">
  {#each TABS as entry (entry.id)}
    <button
      class="tab {entry.id === tab ? 'on' : ''}"
      aria-current={entry.id === tab ? "page" : undefined}
      onclick={() => router.go(hashFor({ view: "assets", tab: entry.id, assetId: null }))}
    >
      {entry.label}
    </button>
  {/each}
</nav>

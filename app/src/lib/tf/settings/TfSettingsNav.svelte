<!--
  Settings' navigation as a tree (the subtrees ruling, 2026-11-21, `docs/decisions/2026-11-21-settings-has-subtrees.md`):
  `SETTINGS_TREE`'s rows, a leaf a link and a parent a fold whose title is its page's link, in the Docs tree's shape so
  its keys apply (`treeKeysOf`): one Tab stop, ↑ ↓ walk the rows shown, → opens a fold then steps in, ← closes it then
  steps to the parent, Home and End.

  Clicks follow the Docs tree's rule through its shared helper (`tf/fold-press.ts`,
  `docs/decisions/2026-11-21-a-click-on-a-docs-row-opens-its-fold.md`): a click on
  a parent's row opens its page and its fold and never closes it; a click on the open parent's row toggles its fold;
  the chevron folds alone. `Enter`, a modified click and the middle button keep the link's own way. A fold is kept
  (`settings-folds.svelte.ts`), and a move to a child's page opens its parent. A folded parent holding the open page
  takes Docs' mark for it.
-->
<script lang="ts">
  import { untrack } from "svelte";

  import { treeKeysOf } from "../docs/tree-keys";
  import { foldKeyed, foldPress } from "../fold-press";
  import { settingsFolds, type SettingsFolds } from "./settings-folds.svelte";
  import { SETTINGS_TREE, settingsHash, settingsParent, type SettingsSection } from "./settings";

  let {
    current,
    folds = settingsFolds,
    onpage,
    badge,
  }: {
    /** The section whose page is open, `null` where the address names none. */
    current: SettingsSection | null;
    folds?: SettingsFolds;
    /** A press opened a page (the narrow rail folds); a press that only folds does not call it. */
    onpage?: () => void;
    /** What a row carries after its name (Sources' state dot). */
    badge?: import("svelte").Snippet<[SettingsSection]>;
  } = $props();

  const treeKeys = treeKeysOf("a.tf-stg-leaf, summary > a.tf-stg-pl");

  // A move to a child's page opens its parent, and that is kept: leaving never closes a fold. On the move only.
  $effect.pre(() => {
    const parent = current === null ? null : settingsParent(current);
    if (parent !== null) untrack(() => folds.set(parent, true));
  });

  /** A fold changed by the browser itself (find in page, say): what it did is kept. */
  function toggled(event: Event & { currentTarget: HTMLDetailsElement }, key: SettingsSection): void {
    if (event.currentTarget.open !== folds.isOpen(key)) folds.set(key, event.currentTarget.open);
  }

  /** A leaf's press opens its page. */
  function leaf(event: MouseEvent): void {
    if (event.button === 0 && !event.ctrlKey && !event.metaKey && !event.shiftKey && !event.altKey) onpage?.();
  }
</script>

<div class="tf-stg-tree" {@attach treeKeys}>
  {#each SETTINGS_TREE as node (node.key)}
    {#if node.children}
      {@const open = folds.isOpen(node.key)}
      <details class="tf-stg-fold" {open} ontoggle={(event) => toggled(event, node.key)}>
        <!-- svelte-ignore a11y_no_redundant_roles -->
        <summary
          onclick={(event) => foldPress(event, current === node.key, (now) => folds.set(node.key, now))}
          onkeydown={(event) => foldKeyed(event, (now) => folds.set(node.key, now))}
          class:tf-holds={!open && current !== null && node.children.some((child) => child.key === current)}
          ><a class="tf-stg-pl" href={settingsHash(node.key)} aria-current={current === node.key ? "page" : undefined} onclick={(event) => current !== node.key && leaf(event)}
            >{node.name}{@render badge?.(node.key)}</a
          ></summary
        >
        {#each node.children as child (child.key)}
          <a class="tf-stg-leaf tf-stg-child" href={settingsHash(child.key)} aria-current={current === child.key ? "page" : undefined} onclick={leaf}
            >{child.name}{@render badge?.(child.key)}</a
          >
        {/each}
      </details>
    {:else}
      <a class="tf-stg-leaf" href={settingsHash(node.key)} aria-current={current === node.key ? "page" : undefined} onclick={leaf}
        >{node.name}{@render badge?.(node.key)}</a
      >
    {/if}
  {/each}
</div>

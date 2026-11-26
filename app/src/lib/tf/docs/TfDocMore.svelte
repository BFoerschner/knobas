<!--
  The reader's *More ▾* (#2082, R-10.10): in the header's actions before "Open in" (#2286),
  holding what a reader reaches for less: *Link to…*, *Copy link* and *Export as Markdown*. Without them the
  header's actions run to a second line in the reading column. The view's *Sync now* joins them beside a page's own
  *Refresh page*, and where the header has no room for it (`readerFit`): the same button, which is why it comes as the frame's snippet.

  `TfMenu` draws it.
-->
<script lang="ts">
  import { type Snippet } from "svelte";

  import TfMenu from "../TfMenu.svelte";

  let {
    onlink,
    oncopy,
    onexport,
    sync,
  }: {
    /** *Link to…*, where this reader can link. */
    onlink?: (() => void) | undefined;
    oncopy: () => void;
    /** *Export as Markdown*, for a page that can be read here. */
    onexport?: (() => void) | undefined;
    /** The view's *Sync now*, beside *Refresh page* or where the header has no room for it. */
    sync?: Snippet | undefined;
  } = $props();

  let open = $state(false);
  let button = $state<HTMLButtonElement | null>(null);

  function pick(run: () => void) {
    open = false;
    run();
  }

  /** The frame's button is an item here: the arrow keys reach it, as they do the others. */
  function asItem(slot: HTMLElement) {
    slot.querySelector("button")?.setAttribute("role", "menuitem");
  }
</script>

<span class="tf-rd-more">
  <button type="button" class="tf-btn" aria-haspopup="menu" aria-expanded={open} bind:this={button} onclick={() => (open = !open)}
    >More<svg class="tf-caret" width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true"><path d="M4 6l4 4 4-4" /></svg></button
  >
  <TfMenu bind:open opener={button} label="More" className="tf-rd-m">
      {#if onlink}<button type="button" role="menuitem" onclick={() => pick(onlink)}>Link to…</button>{/if}
      <button type="button" role="menuitem" onclick={() => pick(oncopy)}>Copy link</button>
      {#if onexport}<button type="button" role="menuitem" onclick={() => pick(onexport)}>Export as Markdown</button>{/if}
      {#if sync}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <span class="tf-rd-moresync" role="none" {@attach asItem} onclick={() => (open = false)}>{@render sync()}</span>
      {/if}
  </TfMenu>
</span>
